//! Database connection pool management
//!
//! Provides connection pooling and transaction support for SQLite.

use crate::{DatabaseConfig, Result, StorageError};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{Sqlite, Transaction};
use std::future::Future;
use std::time::Duration;

/// Database connection pool
#[derive(Clone)]
pub struct DatabasePool {
    pool: SqlitePool,
}

impl DatabasePool {
    /// Create a new database pool
    pub async fn new(config: DatabaseConfig) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&config.database_url)
            .await?;

        Ok(Self { pool })
    }

    /// Create a DatabasePool from an existing SqlitePool
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Warm up the connection pool
    ///
    /// Pre-populates the pool with connections to avoid cold start latency.
    /// This is useful during application startup to ensure connections are
    /// ready before handling requests.
    ///
    /// # Arguments
    ///
    /// * `target_connections` - Number of connections to pre-create (defaults to min_connections)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let pool = DatabasePool::new(config).await?;
    /// pool.warmup(Some(5)).await?; // Pre-create 5 connections
    /// ```
    pub async fn warmup(&self, target_connections: Option<u32>) -> Result<u32> {
        let target =
            target_connections.unwrap_or_else(|| self.pool.options().get_min_connections());

        let mut acquired = Vec::new();
        let mut count = 0;

        // Acquire connections up to target
        for _ in 0..target {
            match self.pool.acquire().await {
                Ok(conn) => {
                    acquired.push(conn);
                    count += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to acquire connection during warmup: {}", e);
                    break;
                }
            }
        }

        // Release all acquired connections back to pool
        drop(acquired);

        tracing::info!("Warmed up connection pool with {} connections", count);
        Ok(count)
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| StorageError::Migration(e.to_string()))?;
        Ok(())
    }

    /// Get a reference to the underlying pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Check if database is healthy
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    // ==================== Transaction Support ====================

    /// Begin a new transaction
    pub async fn begin(&self) -> Result<Transaction<'static, Sqlite>> {
        let tx = self.pool.begin().await?;
        Ok(tx)
    }

    /// Execute a closure within a transaction
    /// The transaction is committed if the closure returns Ok, rolled back otherwise
    pub async fn transaction<F, T, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Transaction<'static, Sqlite>) -> Fut,
        Fut: Future<Output = Result<(Transaction<'static, Sqlite>, T)>>,
    {
        let tx = self.begin().await?;
        match f(tx).await {
            Ok((tx, result)) => {
                tx.commit().await?;
                Ok(result)
            }
            Err(e) => Err(e),
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            size: self.pool.size(),
            num_idle: self.pool.num_idle(),
            max_connections: self.pool.options().get_max_connections(),
            min_connections: self.pool.options().get_min_connections(),
        }
    }

    /// Get comprehensive pool metrics including health status
    pub fn metrics(&self) -> PoolMetrics {
        let stats = self.stats();
        let health = self.health_status();

        PoolMetrics {
            stats,
            health,
            acquire_timeout_ms: 30_000, // 30 seconds (configured in new())
        }
    }

    /// Get pool health status based on utilization and state
    pub fn health_status(&self) -> PoolHealth {
        if self.is_closed() {
            return PoolHealth::Critical;
        }

        let stats = self.stats();

        if stats.is_at_capacity() || !stats.has_available() {
            PoolHealth::Critical
        } else if stats.is_overutilized() {
            PoolHealth::Degraded
        } else {
            PoolHealth::Healthy
        }
    }

    /// Close the pool gracefully
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Check if the pool is closed
    pub fn is_closed(&self) -> bool {
        self.pool.is_closed()
    }

    /// Acquire a connection from the pool
    pub async fn acquire(&self) -> Result<sqlx::pool::PoolConnection<Sqlite>> {
        let conn = self.pool.acquire().await?;
        Ok(conn)
    }

    /// Export metrics in a format suitable for monitoring systems
    ///
    /// Returns a map of metric names to values for integration with
    /// monitoring systems like Prometheus, DataDog, etc.
    pub fn export_metrics(&self) -> std::collections::HashMap<String, f64> {
        let stats = self.stats();
        let mut metrics = std::collections::HashMap::new();

        metrics.insert("pool_size".to_string(), f64::from(stats.size));
        metrics.insert("pool_idle_connections".to_string(), stats.num_idle as f64);
        metrics.insert(
            "pool_active_connections".to_string(),
            stats.active_connections() as f64,
        );
        metrics.insert(
            "pool_max_connections".to_string(),
            f64::from(stats.max_connections),
        );
        metrics.insert(
            "pool_min_connections".to_string(),
            f64::from(stats.min_connections),
        );
        metrics.insert("pool_utilization".to_string(), stats.utilization());
        metrics.insert(
            "pool_at_capacity".to_string(),
            if stats.is_at_capacity() { 1.0 } else { 0.0 },
        );
        metrics.insert(
            "pool_has_available".to_string(),
            if stats.has_available() { 1.0 } else { 0.0 },
        );

        let health = self.health_status();
        metrics.insert(
            "pool_is_closed".to_string(),
            if self.is_closed() { 1.0 } else { 0.0 },
        );
        metrics.insert(
            "pool_is_healthy".to_string(),
            if health == PoolHealth::Healthy {
                1.0
            } else {
                0.0
            },
        );
        metrics.insert(
            "pool_is_degraded".to_string(),
            if health == PoolHealth::Degraded {
                1.0
            } else {
                0.0
            },
        );
        metrics.insert(
            "pool_is_critical".to_string(),
            if health == PoolHealth::Critical {
                1.0
            } else {
                0.0
            },
        );

        metrics
    }
}

/// Pool statistics
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    /// Current number of connections in the pool
    pub size: u32,
    /// Number of idle connections
    pub num_idle: usize,
    /// Maximum number of connections
    pub max_connections: u32,
    /// Minimum number of connections
    pub min_connections: u32,
}

impl PoolStats {
    /// Number of active (non-idle) connections
    pub fn active_connections(&self) -> usize {
        self.size as usize - self.num_idle
    }

    /// Check if the pool has available connections
    pub fn has_available(&self) -> bool {
        self.num_idle > 0 || (self.size as usize) < self.max_connections as usize
    }

    /// Calculate pool utilization as a percentage (0.0 to 1.0)
    pub fn utilization(&self) -> f64 {
        if self.max_connections == 0 {
            return 0.0;
        }
        f64::from(self.size) / f64::from(self.max_connections)
    }

    /// Check if pool is at capacity
    pub fn is_at_capacity(&self) -> bool {
        self.size >= self.max_connections
    }

    /// Check if pool is under-utilized (less than 50% of max)
    pub fn is_underutilized(&self) -> bool {
        self.utilization() < 0.5
    }

    /// Check if pool is over-utilized (more than 80% of max)
    pub fn is_overutilized(&self) -> bool {
        self.utilization() > 0.8
    }
}

/// Extended pool metrics for monitoring and observability
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    /// Basic pool statistics
    pub stats: PoolStats,
    /// Pool health status
    pub health: PoolHealth,
    /// Acquire timeout configuration (in milliseconds)
    pub acquire_timeout_ms: u64,
}

/// Pool health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolHealth {
    /// Pool is healthy and operating normally
    Healthy,
    /// Pool is degraded but functional (high utilization)
    Degraded,
    /// Pool is at capacity or closed
    Critical,
}

impl PoolHealth {
    pub fn as_str(&self) -> &'static str {
        match self {
            PoolHealth::Healthy => "healthy",
            PoolHealth::Degraded => "degraded",
            PoolHealth::Critical => "critical",
        }
    }
}

impl std::fmt::Display for PoolHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Transaction helper for manual transaction management
#[allow(dead_code)]
pub struct TransactionHelper {
    tx: Option<Transaction<'static, Sqlite>>,
    committed: bool,
}

#[allow(dead_code)]
impl TransactionHelper {
    /// Create a new transaction helper
    pub async fn new(pool: &DatabasePool) -> Result<Self> {
        let tx = pool.begin().await?;
        Ok(Self {
            tx: Some(tx),
            committed: false,
        })
    }

    /// Get a reference to the transaction
    pub fn tx(&mut self) -> &mut Transaction<'static, Sqlite> {
        self.tx.as_mut().expect("Transaction already consumed")
    }

    /// Commit the transaction
    pub async fn commit(mut self) -> Result<()> {
        if let Some(tx) = self.tx.take() {
            tx.commit().await?;
            self.committed = true;
        }
        Ok(())
    }

    /// Rollback the transaction
    pub async fn rollback(mut self) -> Result<()> {
        if let Some(tx) = self.tx.take() {
            tx.rollback().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_stats() {
        let stats = PoolStats {
            size: 10,
            num_idle: 3,
            max_connections: 20,
            min_connections: 2,
        };

        assert_eq!(stats.active_connections(), 7);
        assert!(stats.has_available());
    }

    #[test]
    fn test_pool_stats_at_capacity() {
        let stats = PoolStats {
            size: 20,
            num_idle: 0,
            max_connections: 20,
            min_connections: 2,
        };

        assert_eq!(stats.active_connections(), 20);
        assert!(!stats.has_available());
    }
}
