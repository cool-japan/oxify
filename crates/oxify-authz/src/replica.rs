//! Read Replica Support for Load Distribution
//!
//! Implements read-write splitting for PostgreSQL to scale authorization checks:
//! - **Primary**: All writes go to the primary database
//! - **Replicas**: Read operations distributed across replicas
//! - **Load Balancing**: Round-robin or random selection
//! - **Failover**: Automatic fallback to primary on replica failure
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │   Application   │
//! └────────┬────────┘
//!          │
//!     ┌────┴────┐
//!     │  Reads  │  Writes
//!     │         │
//! ┌───▼───┐  ┌─▼──────┐
//! │Replica│  │Primary │ ◄── Replication
//! │Pool   │  │(Write) │
//! │(Read) │  └────┬───┘
//! └───────┘       │
//!     │           │
//!  ┌──▼───┐   ┌──▼───┐
//!  │Rep-1 │   │Rep-2 │ (Replicas also serve reads)
//!  └──────┘   └──────┘
//! ```
//!
//! ## Benefits
//!
//! - **Scalability**: Horizontally scale read capacity
//! - **Performance**: Reduced load on primary database
//! - **High Availability**: Continue serving reads if primary fails
//! - **Geo-Distribution**: Place replicas near users
//!
//! ## Usage
//!
//! ```no_run
//! use oxify_authz::replica::*;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let pool = ReplicaPool::new(
//!     "postgres://primary/db",
//!     vec![
//!         "postgres://replica1/db",
//!         "postgres://replica2/db",
//!     ],
//!     ReplicaConfig::default(),
//! ).await?;
//!
//! // Writes go to primary
//! pool.execute_write("INSERT INTO ...").await?;
//!
//! // Reads load-balanced across replicas
//! let result = pool.execute_read("SELECT ...").await?;
//! # Ok(())
//! # }
//! ```

use crate::{AuthzError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Load balancing strategy for replicas
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoadBalanceStrategy {
    /// Round-robin selection
    RoundRobin,
    /// Random selection
    Random,
    /// Least connections
    LeastConnections,
    /// Weighted round-robin (requires replica weights)
    Weighted,
}

/// Replica configuration
#[derive(Debug, Clone)]
pub struct ReplicaConfig {
    /// Load balancing strategy
    pub strategy: LoadBalanceStrategy,
    /// Maximum replication lag (seconds) before marking replica unhealthy
    pub max_replication_lag_sec: u32,
    /// Health check interval (seconds)
    pub health_check_interval_sec: u64,
    /// Fallback to primary on replica failure
    pub fallback_to_primary: bool,
    /// Enable read from primary if all replicas are down
    pub read_from_primary: bool,
    /// Connection pool size per replica
    pub pool_size: u32,
}

impl Default for ReplicaConfig {
    fn default() -> Self {
        Self {
            strategy: LoadBalanceStrategy::RoundRobin,
            max_replication_lag_sec: 5,
            health_check_interval_sec: 10,
            fallback_to_primary: true,
            read_from_primary: false, // Prefer replicas for reads
            pool_size: 10,
        }
    }
}

/// Replica health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaHealth {
    /// Replica URL (sanitized)
    pub url: String,
    /// Is replica healthy?
    pub healthy: bool,
    /// Replication lag (seconds)
    pub replication_lag_sec: Option<f64>,
    /// Last health check
    #[serde(skip)]
    pub last_check: Option<Instant>,
    /// Active connections
    pub active_connections: u32,
}

/// Replica pool with read-write splitting
pub struct ReplicaPool {
    /// Primary database (writes)
    primary: Arc<PgPool>,
    /// Replica databases (reads)
    replicas: Arc<Vec<Arc<PgPool>>>,
    /// Configuration
    config: ReplicaConfig,
    /// Round-robin counter
    rr_counter: AtomicUsize,
    /// Health status
    health: Arc<RwLock<Vec<ReplicaHealth>>>,
}

impl ReplicaPool {
    /// Create a new replica pool
    pub async fn new(
        primary_url: &str,
        replica_urls: Vec<&str>,
        config: ReplicaConfig,
    ) -> Result<Self> {
        // Connect to primary
        let primary = PgPool::connect(primary_url).await.map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to connect to primary: {}", e))
        })?;

        // Connect to replicas
        let mut replicas = Vec::new();
        let mut health = Vec::new();

        for url in &replica_urls {
            match PgPool::connect(url).await {
                Ok(pool) => {
                    replicas.push(Arc::new(pool));
                    health.push(ReplicaHealth {
                        url: sanitize_url(url),
                        healthy: true,
                        replication_lag_sec: None,
                        last_check: None,
                        active_connections: 0,
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to connect to replica {}: {}", url, e);
                    if !config.fallback_to_primary {
                        return Err(AuthzError::DatabaseError(format!(
                            "Failed to connect to replica: {}",
                            e
                        )));
                    }
                }
            }
        }

        if replicas.is_empty() && !config.read_from_primary {
            return Err(AuthzError::DatabaseError(
                "No replicas available and read_from_primary is disabled".to_string(),
            ));
        }

        let pool = Self {
            primary: Arc::new(primary),
            replicas: Arc::new(replicas),
            config,
            rr_counter: AtomicUsize::new(0),
            health: Arc::new(RwLock::new(health)),
        };

        // Start health check background task
        pool.start_health_checks();

        Ok(pool)
    }

    /// Get primary pool (for writes)
    pub fn primary(&self) -> &PgPool {
        &self.primary
    }

    /// Select a replica for read operations
    pub async fn select_replica(&self) -> Option<Arc<PgPool>> {
        if self.replicas.is_empty() {
            return if self.config.read_from_primary {
                Some(self.primary.clone())
            } else {
                None
            };
        }

        match self.config.strategy {
            LoadBalanceStrategy::RoundRobin => self.select_round_robin().await,
            LoadBalanceStrategy::Random => self.select_random().await,
            LoadBalanceStrategy::LeastConnections => self.select_least_connections().await,
            LoadBalanceStrategy::Weighted => self.select_weighted().await,
        }
    }

    /// Round-robin replica selection
    async fn select_round_robin(&self) -> Option<Arc<PgPool>> {
        let health = self.health.read().await;
        let healthy_replicas: Vec<usize> = health
            .iter()
            .enumerate()
            .filter(|(_, h)| h.healthy)
            .map(|(i, _)| i)
            .collect();

        if healthy_replicas.is_empty() {
            return if self.config.fallback_to_primary {
                Some(self.primary.clone())
            } else {
                None
            };
        }

        let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) % healthy_replicas.len();
        Some(self.replicas[healthy_replicas[idx]].clone())
    }

    /// Random replica selection
    async fn select_random(&self) -> Option<Arc<PgPool>> {
        let health = self.health.read().await;
        let healthy_replicas: Vec<usize> = health
            .iter()
            .enumerate()
            .filter(|(_, h)| h.healthy)
            .map(|(i, _)| i)
            .collect();

        if healthy_replicas.is_empty() {
            return if self.config.fallback_to_primary {
                Some(self.primary.clone())
            } else {
                None
            };
        }

        // Use rng for random selection to avoid version conflicts
        use rand::Rng;
        let mut rng = rand::rng();
        let idx = rng.random_range(0..healthy_replicas.len());
        Some(self.replicas[healthy_replicas[idx]].clone())
    }

    /// Least connections replica selection
    async fn select_least_connections(&self) -> Option<Arc<PgPool>> {
        let health = self.health.read().await;
        let min_conn = health
            .iter()
            .filter(|h| h.healthy)
            .min_by_key(|h| h.active_connections)?;

        let idx = health.iter().position(|h| h.url == min_conn.url)?;
        Some(self.replicas[idx].clone())
    }

    /// Weighted round-robin (placeholder - requires weight config)
    async fn select_weighted(&self) -> Option<Arc<PgPool>> {
        // Fallback to round-robin for now
        self.select_round_robin().await
    }

    /// Execute a read query on a replica
    pub async fn execute_read(&self, query: &str) -> Result<Vec<sqlx::postgres::PgRow>> {
        let replica = self.select_replica().await.ok_or_else(|| {
            AuthzError::DatabaseError("No healthy replicas available".to_string())
        })?;

        sqlx::query(query)
            .fetch_all(&*replica)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Read query failed: {}", e)))
    }

    /// Execute a write query on the primary
    pub async fn execute_write(&self, query: &str) -> Result<sqlx::postgres::PgQueryResult> {
        sqlx::query(query)
            .execute(&*self.primary)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Write query failed: {}", e)))
    }

    /// Check replication lag for a replica
    #[allow(dead_code)]
    async fn check_replication_lag(&self, pool: &PgPool) -> Result<f64> {
        let row = sqlx::query(
            "SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp())) AS lag_sec",
        )
        .fetch_one(pool)
        .await
        .map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to check replication lag: {}", e))
        })?;

        let lag: Option<f64> = row.try_get("lag_sec").map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to parse replication lag: {}", e))
        })?;

        Ok(lag.unwrap_or(0.0))
    }

    /// Start background health check task
    fn start_health_checks(&self) {
        let replicas = self.replicas.clone();
        let health = self.health.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let interval = Duration::from_secs(config.health_check_interval_sec);
            loop {
                tokio::time::sleep(interval).await;

                let mut health_guard = health.write().await;
                for (idx, replica) in replicas.iter().enumerate() {
                    // Check replication lag
                    match sqlx::query("SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp())) AS lag_sec")
                        .fetch_one(&**replica)
                        .await
                    {
                        Ok(row) => {
                            let lag: Option<f64> = row.try_get("lag_sec").unwrap_or(None);
                            let lag_sec = lag.unwrap_or(0.0);

                            health_guard[idx].healthy = lag_sec <= config.max_replication_lag_sec as f64;
                            health_guard[idx].replication_lag_sec = Some(lag_sec);
                            health_guard[idx].last_check = Some(Instant::now());

                            if !health_guard[idx].healthy {
                                tracing::warn!(
                                    "Replica {} unhealthy: replication lag {}s",
                                    health_guard[idx].url,
                                    lag_sec
                                );
                            }
                        }
                        Err(e) => {
                            health_guard[idx].healthy = false;
                            tracing::error!("Health check failed for replica {}: {}", health_guard[idx].url, e);
                        }
                    }
                }
            }
        });
    }

    /// Get health status of all replicas
    pub async fn health_status(&self) -> Vec<ReplicaHealth> {
        self.health.read().await.clone()
    }

    /// Get number of healthy replicas
    pub async fn healthy_replica_count(&self) -> usize {
        self.health
            .read()
            .await
            .iter()
            .filter(|h| h.healthy)
            .count()
    }
}

/// Sanitize database URL (remove password)
fn sanitize_url(url: &str) -> String {
    // Remove password from URL for logging
    if let Some(idx) = url.find("://") {
        let scheme = &url[..idx + 3];
        let rest = &url[idx + 3..];

        if let Some(at_idx) = rest.find('@') {
            // There's user info before @
            let user_info = &rest[..at_idx];
            let host_and_path = &rest[at_idx + 1..];

            if let Some(colon_idx) = user_info.find(':') {
                // There's a password to sanitize
                let username = &user_info[..colon_idx];
                return format!("{}{}:****@{}", scheme, username, host_and_path);
            }
            // No password, just username
            return format!("{}{}@{}", scheme, user_info, host_and_path);
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_url() {
        let url = "postgres://user:password@localhost:5432/db";
        let sanitized = sanitize_url(url);
        assert_eq!(sanitized, "postgres://user:****@localhost:5432/db");

        let url_no_pass = "postgres://user@localhost:5432/db";
        let sanitized = sanitize_url(url_no_pass);
        assert_eq!(sanitized, url_no_pass);
    }

    #[test]
    fn test_replica_config_default() {
        let config = ReplicaConfig::default();
        assert_eq!(config.strategy, LoadBalanceStrategy::RoundRobin);
        assert_eq!(config.max_replication_lag_sec, 5);
        assert_eq!(config.health_check_interval_sec, 10);
        assert!(config.fallback_to_primary);
        assert!(!config.read_from_primary);
        assert_eq!(config.pool_size, 10);
    }
}
