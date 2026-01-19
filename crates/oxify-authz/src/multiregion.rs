//! Multi-Region Support
//!
//! Geo-distributed authorization for low-latency global access:
//! - **Regional Deployments**: Deploy authorization engines in multiple regions
//! - **Async Replication**: Eventually consistent tuple propagation
//! - **Read-Local, Write-Primary**: Optimize for read latency
//! - **Conflict Resolution**: Handle concurrent writes across regions
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                  Global Load Balancer                   │
//! └──────┬─────────────────────┬─────────────────────┬──────┘
//!        │                     │                     │
//!   ┌────▼─────┐         ┌────▼─────┐         ┌────▼─────┐
//!   │ US-West  │         │ EU-West  │         │ AP-East  │
//!   │ (Read)   │         │ (Read)   │         │ (Read)   │
//!   └────┬─────┘         └────┬─────┘         └────┬─────┘
//!        │                     │                     │
//!        └─────────────────────┼─────────────────────┘
//!                              │
//!                        ┌─────▼──────┐
//!                        │   Primary  │ ◄── All Writes
//!                        │  (US-East) │
//!                        └────────────┘
//!                              │
//!                    ┌─────────┼──────────┐
//!                    │  Async Replication │
//!                    └────────────────────┘
//! ```
//!
//! ## Replication Strategies
//!
//! 1. **Leader-Follower**: Single write region, multiple read regions
//! 2. **Multi-Primary**: All regions accept writes (requires conflict resolution)
//! 3. **Hybrid**: Critical tenants use single-primary, others use multi-primary
//!
//! ## Usage
//!
//! ```no_run
//! use oxify_authz::multiregion::*;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = RegionConfig {
//!     region_id: "us-west-1".to_string(),
//!     is_primary: false,
//!     primary_url: "postgres://primary.example.com/db".to_string(),
//!     local_url: "postgres://localhost/db".to_string(),
//!     replication_lag_threshold_sec: 5,
//!     ..Default::default()
//! };
//!
//! let engine = MultiRegionEngine::new(config).await?;
//!
//! // Reads served locally
//! let subject = oxify_authz::Subject::User("alice".to_string());
//! let allowed = engine.check_local("doc", "123", "view", subject.clone()).await?;
//!
//! // Writes propagate to primary
//! let tuple = oxify_authz::RelationTuple::new("doc", "view", "123", subject);
//! engine.write_global(tuple).await?;
//! # Ok(())
//! # }
//! ```

use crate::{AuthzError, RelationTuple, Result, Subject};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Region identifier
pub type RegionId = String;

/// Multi-region configuration
#[derive(Debug, Clone)]
pub struct RegionConfig {
    /// This region's ID (e.g., "us-west-1")
    pub region_id: RegionId,
    /// Is this the primary region? (accepts writes)
    pub is_primary: bool,
    /// Primary region database URL
    pub primary_url: String,
    /// Local region database URL
    pub local_url: String,
    /// Maximum acceptable replication lag (seconds)
    pub replication_lag_threshold_sec: u32,
    /// Fallback to primary on replication lag
    pub fallback_on_lag: bool,
    /// Enable conflict resolution for multi-primary
    pub enable_conflict_resolution: bool,
}

impl Default for RegionConfig {
    fn default() -> Self {
        Self {
            region_id: "default".to_string(),
            is_primary: true,
            primary_url: String::new(),
            local_url: String::new(),
            replication_lag_threshold_sec: 5,
            fallback_on_lag: true,
            enable_conflict_resolution: false,
        }
    }
}

/// Replication status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    /// Region ID
    pub region_id: RegionId,
    /// Replication lag (seconds)
    pub lag_sec: f64,
    /// Is region healthy?
    pub healthy: bool,
    /// Last sync timestamp
    #[serde(skip)]
    pub last_sync: Option<Instant>,
    /// Pending writes
    pub pending_writes: usize,
}

/// Multi-region authorization engine
pub struct MultiRegionEngine {
    /// Configuration
    config: RegionConfig,
    /// Local database pool
    local_pool: Arc<PgPool>,
    /// Primary database pool
    primary_pool: Option<Arc<PgPool>>,
    /// Replication status
    status: Arc<RwLock<ReplicationStatus>>,
}

impl MultiRegionEngine {
    /// Create a new multi-region engine
    pub async fn new(config: RegionConfig) -> Result<Self> {
        // Connect to local database
        let local_pool = PgPool::connect(&config.local_url).await.map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to connect to local database: {}", e))
        })?;

        // Connect to primary database (if not primary region)
        let primary_pool = if !config.is_primary {
            let pool = PgPool::connect(&config.primary_url).await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to connect to primary database: {}", e))
            })?;
            Some(Arc::new(pool))
        } else {
            None
        };

        let engine = Self {
            config: config.clone(),
            local_pool: Arc::new(local_pool),
            primary_pool,
            status: Arc::new(RwLock::new(ReplicationStatus {
                region_id: config.region_id.clone(),
                lag_sec: 0.0,
                healthy: true,
                last_sync: None,
                pending_writes: 0,
            })),
        };

        // Start replication monitoring
        if !config.is_primary {
            engine.start_replication_monitor();
        }

        Ok(engine)
    }

    /// Check permission (read from local replica)
    pub async fn check_local(
        &self,
        namespace: &str,
        object_id: &str,
        relation: &str,
        subject: Subject,
    ) -> Result<bool> {
        // Check replication health
        let status = self.status.read().await;
        if !status.healthy && self.config.fallback_on_lag {
            drop(status);
            return self
                .check_primary(namespace, object_id, relation, subject)
                .await;
        }
        drop(status);

        // Query local database
        let query = r#"
            SELECT EXISTS(
                SELECT 1 FROM relation_tuples
                WHERE namespace = $1
                AND object_id = $2
                AND relation = $3
                AND subject_type = $4
                AND subject_id = $5
            ) AS allowed
        "#;

        let (subject_type, subject_id) = match subject {
            Subject::User(id) => ("user", id),
            Subject::UserSet {
                namespace,
                object_id,
                relation,
            } => (
                "userset",
                format!("{}:{}#{}", namespace, object_id, relation),
            ),
        };

        let row = sqlx::query(query)
            .bind(namespace)
            .bind(object_id)
            .bind(relation)
            .bind(subject_type)
            .bind(&subject_id)
            .fetch_one(&*self.local_pool)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Check query failed: {}", e)))?;

        let allowed: bool = row
            .try_get("allowed")
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to parse result: {}", e)))?;

        Ok(allowed)
    }

    /// Check permission (read from primary)
    async fn check_primary(
        &self,
        namespace: &str,
        object_id: &str,
        relation: &str,
        subject: Subject,
    ) -> Result<bool> {
        let primary = self
            .primary_pool
            .as_ref()
            .ok_or_else(|| AuthzError::DatabaseError("Primary pool not available".to_string()))?;

        let query = r#"
            SELECT EXISTS(
                SELECT 1 FROM relation_tuples
                WHERE namespace = $1
                AND object_id = $2
                AND relation = $3
                AND subject_type = $4
                AND subject_id = $5
            ) AS allowed
        "#;

        let (subject_type, subject_id) = match subject {
            Subject::User(id) => ("user", id),
            Subject::UserSet {
                namespace,
                object_id,
                relation,
            } => (
                "userset",
                format!("{}:{}#{}", namespace, object_id, relation),
            ),
        };

        let row = sqlx::query(query)
            .bind(namespace)
            .bind(object_id)
            .bind(relation)
            .bind(subject_type)
            .bind(&subject_id)
            .fetch_one(&**primary)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Check query failed: {}", e)))?;

        let allowed: bool = row
            .try_get("allowed")
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to parse result: {}", e)))?;

        Ok(allowed)
    }

    /// Write tuple (propagate to primary)
    pub async fn write_global(&self, tuple: RelationTuple) -> Result<()> {
        if self.config.is_primary {
            // Write locally if we're the primary
            self.write_local(tuple).await
        } else {
            // Forward write to primary
            self.write_primary(tuple).await
        }
    }

    /// Write to local database
    async fn write_local(&self, tuple: RelationTuple) -> Result<()> {
        let (subject_type, subject_id) = match tuple.subject {
            Subject::User(id) => ("user", id),
            Subject::UserSet {
                namespace,
                object_id,
                relation,
            } => (
                "userset",
                format!("{}:{}#{}", namespace, object_id, relation),
            ),
        };

        let query = r#"
            INSERT INTO relation_tuples (namespace, object_id, relation, subject_type, subject_id)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT DO NOTHING
        "#;

        sqlx::query(query)
            .bind(&tuple.namespace)
            .bind(&tuple.object_id)
            .bind(&tuple.relation)
            .bind(subject_type)
            .bind(&subject_id)
            .execute(&*self.local_pool)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Write failed: {}", e)))?;

        Ok(())
    }

    /// Write to primary database
    async fn write_primary(&self, tuple: RelationTuple) -> Result<()> {
        let primary = self
            .primary_pool
            .as_ref()
            .ok_or_else(|| AuthzError::DatabaseError("Primary pool not available".to_string()))?;

        let (subject_type, subject_id) = match tuple.subject {
            Subject::User(id) => ("user", id),
            Subject::UserSet {
                namespace,
                object_id,
                relation,
            } => (
                "userset",
                format!("{}:{}#{}", namespace, object_id, relation),
            ),
        };

        let query = r#"
            INSERT INTO relation_tuples (namespace, object_id, relation, subject_type, subject_id)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT DO NOTHING
        "#;

        sqlx::query(query)
            .bind(&tuple.namespace)
            .bind(&tuple.object_id)
            .bind(&tuple.relation)
            .bind(subject_type)
            .bind(&subject_id)
            .execute(&**primary)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Write failed: {}", e)))?;

        Ok(())
    }

    /// Check replication lag
    #[allow(dead_code)]
    async fn check_replication_lag(&self) -> Result<f64> {
        let query =
            "SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp())) AS lag_sec";

        let row = sqlx::query(query)
            .fetch_one(&*self.local_pool)
            .await
            .map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to check replication lag: {}", e))
            })?;

        let lag: Option<f64> = row
            .try_get("lag_sec")
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to parse lag: {}", e)))?;

        Ok(lag.unwrap_or(0.0))
    }

    /// Start replication monitoring background task
    fn start_replication_monitor(&self) {
        let local_pool = self.local_pool.clone();
        let status = self.status.clone();
        let threshold = self.config.replication_lag_threshold_sec;

        tokio::spawn(async move {
            let interval = Duration::from_secs(5);
            loop {
                tokio::time::sleep(interval).await;

                // Check replication lag
                let lag_result = sqlx::query(
                    "SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp())) AS lag_sec",
                )
                .fetch_one(&*local_pool)
                .await;

                let mut status_guard = status.write().await;
                match lag_result {
                    Ok(row) => {
                        let lag: Option<f64> = row.try_get("lag_sec").unwrap_or(None);
                        let lag_sec = lag.unwrap_or(0.0);

                        status_guard.lag_sec = lag_sec;
                        status_guard.healthy = lag_sec <= threshold as f64;
                        status_guard.last_sync = Some(Instant::now());

                        if !status_guard.healthy {
                            tracing::warn!(
                                "Region {} unhealthy: replication lag {}s",
                                status_guard.region_id,
                                lag_sec
                            );
                        }
                    }
                    Err(e) => {
                        status_guard.healthy = false;
                        tracing::error!("Replication health check failed: {}", e);
                    }
                }
            }
        });
    }

    /// Get replication status
    pub async fn replication_status(&self) -> ReplicationStatus {
        self.status.read().await.clone()
    }

    /// Get region ID
    pub fn region_id(&self) -> &str {
        &self.config.region_id
    }

    /// Is primary region?
    pub fn is_primary(&self) -> bool {
        self.config.is_primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_config_default() {
        let config = RegionConfig::default();
        assert_eq!(config.region_id, "default");
        assert!(config.is_primary);
        assert_eq!(config.replication_lag_threshold_sec, 5);
        assert!(config.fallback_on_lag);
    }

    #[test]
    fn test_replication_status() {
        let status = ReplicationStatus {
            region_id: "us-west-1".to_string(),
            lag_sec: 2.5,
            healthy: true,
            last_sync: Some(Instant::now()),
            pending_writes: 10,
        };

        assert_eq!(status.region_id, "us-west-1");
        assert_eq!(status.lag_sec, 2.5);
        assert!(status.healthy);
        assert_eq!(status.pending_writes, 10);
    }
}
