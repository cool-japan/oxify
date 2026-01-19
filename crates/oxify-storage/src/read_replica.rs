//! Read Replica Support
//!
//! This module provides read replica management for scaling read-heavy workloads.
//!
//! ## Features
//!
//! - Multiple read replica connections
//! - Round-robin load balancing
//! - Automatic failover to primary on replica failure
//! - Health monitoring for replicas
//! - Configurable replica routing
//! - Replication lag detection
//!
//! ## Usage
//!
//! ```ignore
//! use oxify_storage::read_replica::{ReplicaPool, ReplicaConfig};
//!
//! let config = ReplicaConfig {
//!     primary_url: "postgres://localhost/oxify".to_string(),
//!     replica_urls: vec![
//!         "postgres://replica1/oxify".to_string(),
//!         "postgres://replica2/oxify".to_string(),
//!     ],
//!     max_connections_per_replica: 10,
//!     prefer_replica_for_reads: true,
//!     max_replication_lag_seconds: 10,
//! };
//!
//! let pool = ReplicaPool::new(config).await?;
//!
//! // Read operations use replicas
//! let workflows = pool.read_query("SELECT * FROM workflows WHERE user_id = $1")
//!     .bind(user_id)
//!     .fetch_all()
//!     .await?;
//!
//! // Write operations use primary
//! pool.write_query("INSERT INTO workflows (...) VALUES (...)")
//!     .execute()
//!     .await?;
//! ```

use crate::{DatabasePool, PoolHealth, PoolMetrics, Result, StorageError};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Read replica configuration
#[derive(Debug, Clone)]
pub struct ReplicaConfig {
    /// Primary database URL (for writes and reads)
    pub primary_url: String,
    /// Read replica URLs
    pub replica_urls: Vec<String>,
    /// Maximum connections per replica
    pub max_connections_per_replica: u32,
    /// Minimum connections per replica
    pub min_connections_per_replica: u32,
    /// Prefer replicas for read queries
    pub prefer_replica_for_reads: bool,
    /// Maximum acceptable replication lag in seconds
    pub max_replication_lag_seconds: i64,
    /// Enable automatic failover to primary
    pub enable_failover: bool,
}

impl Default for ReplicaConfig {
    fn default() -> Self {
        Self {
            primary_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost/oxify".to_string()),
            replica_urls: vec![],
            max_connections_per_replica: 10,
            min_connections_per_replica: 2,
            prefer_replica_for_reads: true,
            max_replication_lag_seconds: 10,
            enable_failover: true,
        }
    }
}

/// Replica connection information
#[derive(Debug, Clone)]
struct ReplicaInfo {
    pool: PgPool,
    url: String,
    healthy: bool,
    last_check: std::time::Instant,
}

/// Read replica pool manager
pub struct ReplicaPool {
    config: ReplicaConfig,
    primary: DatabasePool,
    replicas: Arc<RwLock<Vec<ReplicaInfo>>>,
    next_replica: AtomicUsize,
}

impl ReplicaPool {
    /// Create a new replica pool
    pub async fn new(config: ReplicaConfig) -> Result<Self> {
        // Create primary connection pool
        let primary_pool = PgPoolOptions::new()
            .max_connections(config.max_connections_per_replica)
            .min_connections(config.min_connections_per_replica)
            .connect(&config.primary_url)
            .await
            .map_err(StorageError::Database)?;

        let primary = DatabasePool::from_pool(primary_pool);

        // Create replica connection pools
        let mut replicas = Vec::new();
        for replica_url in &config.replica_urls {
            match PgPoolOptions::new()
                .max_connections(config.max_connections_per_replica)
                .min_connections(config.min_connections_per_replica)
                .connect(replica_url)
                .await
            {
                Ok(pool) => {
                    info!("Connected to read replica: {}", replica_url);
                    replicas.push(ReplicaInfo {
                        pool,
                        url: replica_url.clone(),
                        healthy: true,
                        last_check: std::time::Instant::now(),
                    });
                }
                Err(e) => {
                    warn!("Failed to connect to replica {}: {}", replica_url, e);
                }
            }
        }

        Ok(Self {
            config,
            primary,
            replicas: Arc::new(RwLock::new(replicas)),
            next_replica: AtomicUsize::new(0),
        })
    }

    /// Get the primary pool (for writes and consistent reads)
    pub fn primary(&self) -> &DatabasePool {
        &self.primary
    }

    /// Get a replica pool for read operations
    ///
    /// Uses round-robin load balancing across healthy replicas.
    /// Falls back to primary if no healthy replicas are available.
    ///
    /// Returns a cloned PgPool which shares the same underlying connection pool.
    pub async fn replica(&self) -> PgPool {
        if !self.config.prefer_replica_for_reads {
            return self.primary.pool().clone();
        }

        let replicas = self.replicas.read().await;

        if replicas.is_empty() {
            debug!("No replicas available, using primary");
            return self.primary.pool().clone();
        }

        // Find healthy replicas
        let healthy_replicas: Vec<&ReplicaInfo> = replicas.iter().filter(|r| r.healthy).collect();

        if healthy_replicas.is_empty() {
            warn!("No healthy replicas available, using primary");
            return self.primary.pool().clone();
        }

        // Round-robin selection
        let index = self.next_replica.fetch_add(1, Ordering::Relaxed) % healthy_replicas.len();
        healthy_replicas[index].pool.clone()
    }

    /// Check replication lag for a replica
    ///
    /// Returns the lag in seconds, or None if unable to determine.
    pub async fn check_replication_lag(&self, replica_pool: &PgPool) -> Result<Option<i64>> {
        #[derive(sqlx::FromRow)]
        struct LagResult {
            lag_seconds: Option<i64>,
        }

        let result = sqlx::query_as::<_, LagResult>(
            r"
            SELECT EXTRACT(EPOCH FROM (NOW() - pg_last_xact_replay_timestamp()))::bigint as lag_seconds
            ",
        )
        .fetch_optional(replica_pool)
        .await?;

        Ok(result.and_then(|r| r.lag_seconds))
    }

    /// Health check for all replicas
    ///
    /// Updates the health status of each replica based on connectivity
    /// and replication lag.
    pub async fn health_check_replicas(&self) -> Result<Vec<ReplicaHealth>> {
        let mut replicas = self.replicas.write().await;
        let mut health_results = Vec::new();

        for replica in replicas.iter_mut() {
            // Skip if recently checked (< 30 seconds ago)
            if replica.last_check.elapsed().as_secs() < 30 {
                health_results.push(ReplicaHealth {
                    url: replica.url.clone(),
                    healthy: replica.healthy,
                    lag_seconds: None,
                    last_check_ago_seconds: replica.last_check.elapsed().as_secs(),
                });
                continue;
            }

            // Check connectivity
            let connectivity_ok = sqlx::query("SELECT 1").execute(&replica.pool).await.is_ok();

            if !connectivity_ok {
                replica.healthy = false;
                replica.last_check = std::time::Instant::now();
                health_results.push(ReplicaHealth {
                    url: replica.url.clone(),
                    healthy: false,
                    lag_seconds: None,
                    last_check_ago_seconds: 0,
                });
                continue;
            }

            // Check replication lag
            let lag = self
                .check_replication_lag(&replica.pool)
                .await
                .ok()
                .flatten();

            let lag_ok = lag.is_none_or(|l| l <= self.config.max_replication_lag_seconds);

            replica.healthy = connectivity_ok && lag_ok;
            replica.last_check = std::time::Instant::now();

            health_results.push(ReplicaHealth {
                url: replica.url.clone(),
                healthy: replica.healthy,
                lag_seconds: lag,
                last_check_ago_seconds: 0,
            });
        }

        Ok(health_results)
    }

    /// Get pool metrics for all pools (primary + replicas)
    pub async fn get_all_metrics(&self) -> AllPoolMetrics {
        let primary_metrics = self.primary.metrics();
        let replicas = self.replicas.read().await;

        let replica_metrics: Vec<PoolMetrics> = replicas
            .iter()
            .map(|r| {
                let max_connections = r.pool.options().get_max_connections();
                let min_connections = r.pool.options().get_min_connections();
                let size = r.pool.size();
                let num_idle = r.pool.num_idle();

                PoolMetrics {
                    stats: crate::pool::PoolStats {
                        size,
                        num_idle,
                        max_connections,
                        min_connections,
                    },
                    health: if r.healthy {
                        PoolHealth::Healthy
                    } else {
                        PoolHealth::Degraded
                    },
                    acquire_timeout_ms: 30_000,
                }
            })
            .collect();

        AllPoolMetrics {
            primary: primary_metrics,
            replicas: replica_metrics,
        }
    }

    /// Get count of healthy replicas
    pub async fn healthy_replica_count(&self) -> usize {
        let replicas = self.replicas.read().await;
        replicas.iter().filter(|r| r.healthy).count()
    }

    /// Get total replica count
    pub async fn total_replica_count(&self) -> usize {
        let replicas = self.replicas.read().await;
        replicas.len()
    }
}

/// Replica health information
#[derive(Debug, Clone)]
pub struct ReplicaHealth {
    /// Replica connection URL
    pub url: String,
    /// Whether the replica is healthy
    pub healthy: bool,
    /// Replication lag in seconds (if available)
    pub lag_seconds: Option<i64>,
    /// Seconds since last health check
    pub last_check_ago_seconds: u64,
}

/// Metrics for all pools
#[derive(Debug, Clone)]
pub struct AllPoolMetrics {
    /// Primary pool metrics
    pub primary: PoolMetrics,
    /// Replica pool metrics
    pub replicas: Vec<PoolMetrics>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ReplicaConfig::default();
        assert!(config.prefer_replica_for_reads);
        assert_eq!(config.max_replication_lag_seconds, 10);
        assert_eq!(config.max_connections_per_replica, 10);
    }

    #[test]
    fn test_replica_config_creation() {
        let config = ReplicaConfig {
            primary_url: "postgres://primary/db".to_string(),
            replica_urls: vec![
                "postgres://replica1/db".to_string(),
                "postgres://replica2/db".to_string(),
            ],
            max_connections_per_replica: 20,
            min_connections_per_replica: 5,
            prefer_replica_for_reads: true,
            max_replication_lag_seconds: 5,
            enable_failover: true,
        };

        assert_eq!(config.replica_urls.len(), 2);
        assert_eq!(config.max_connections_per_replica, 20);
    }
}
