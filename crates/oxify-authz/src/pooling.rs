//! Connection Pooling Configuration
//!
//! Optimized PostgreSQL connection pooling for high-throughput authorization:
//! - **PgBouncer Integration**: External pooler for connection multiplexing
//! - **SQLx Pool Tuning**: Optimal pool sizes and timeouts
//! - **Connection Lifecycle**: Idle timeout, max lifetime, health checks
//! - **Performance Metrics**: Track pool utilization and wait times
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │         Application Servers (N)          │
//! │  ┌────────┐  ┌────────┐  ┌────────┐    │
//! │  │SQLx    │  │SQLx    │  │SQLx    │    │
//! │  │Pool(10)│  │Pool(10)│  │Pool(10)│    │
//! └──┴────┬───┴──┴────┬───┴──┴────┬───┴────┘
//!         │           │           │
//!         └───────────┼───────────┘
//!                     │
//!         ┌───────────▼───────────┐
//!         │     PgBouncer         │ ◄── Connection Multiplexing
//!         │   (Transaction Mode)  │     (Thousands of clients → ~100 DB conns)
//!         └───────────┬───────────┘
//!                     │
//!         ┌───────────▼───────────┐
//!         │    PostgreSQL         │
//!         │  (max_connections=200)│
//!         └───────────────────────┘
//! ```
//!
//! ## PgBouncer Configuration
//!
//! ```ini
//! [databases]
//! authz_db = host=localhost port=5432 dbname=authz
//!
//! [pgbouncer]
//! pool_mode = transaction
//! max_client_conn = 10000
//! default_pool_size = 100
//! reserve_pool_size = 10
//! server_idle_timeout = 60
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use oxify_authz::pooling::*;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PoolConfig {
//!     min_connections: 5,
//!     max_connections: 20,
//!     acquire_timeout_sec: 5,
//!     idle_timeout_sec: 300,
//!     max_lifetime_sec: 1800,
//!     ..Default::default()
//! };
//!
//! let pool = create_pool("postgres://localhost/db", config).await?;
//! let stats = pool_stats(&pool).await?;
//! # Ok(())
//! # }
//! ```

use crate::{AuthzError, Result};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool, Row};
use std::str::FromStr;
use std::time::Duration;

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of connections to maintain
    pub min_connections: u32,
    /// Maximum number of connections in pool
    pub max_connections: u32,
    /// Timeout for acquiring a connection (seconds)
    pub acquire_timeout_sec: u64,
    /// Idle timeout before closing connection (seconds)
    pub idle_timeout_sec: u64,
    /// Maximum lifetime of a connection (seconds)
    pub max_lifetime_sec: u64,
    /// Test connections before use
    pub test_before_acquire: bool,
    /// Enable statement logging
    pub log_statements: bool,
    /// Slow query threshold for logging (milliseconds)
    pub slow_query_threshold_ms: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 5,
            max_connections: 20,
            acquire_timeout_sec: 5,
            idle_timeout_sec: 300,  // 5 minutes
            max_lifetime_sec: 1800, // 30 minutes
            test_before_acquire: true,
            log_statements: false, // Disable in production for performance
            slow_query_threshold_ms: 100,
        }
    }
}

impl PoolConfig {
    /// High-throughput configuration (more connections, shorter lifetimes)
    pub fn high_throughput() -> Self {
        Self {
            min_connections: 10,
            max_connections: 50,
            acquire_timeout_sec: 3,
            idle_timeout_sec: 120,
            max_lifetime_sec: 600,
            test_before_acquire: false, // Skip for performance
            log_statements: false,
            slow_query_threshold_ms: 50,
        }
    }

    /// Low-latency configuration (balanced settings)
    pub fn low_latency() -> Self {
        Self {
            min_connections: 5,
            max_connections: 20,
            acquire_timeout_sec: 2,
            idle_timeout_sec: 180,
            max_lifetime_sec: 900,
            test_before_acquire: true,
            log_statements: false,
            slow_query_threshold_ms: 20,
        }
    }

    /// Development configuration (verbose logging)
    pub fn development() -> Self {
        Self {
            min_connections: 2,
            max_connections: 10,
            acquire_timeout_sec: 10,
            idle_timeout_sec: 600,
            max_lifetime_sec: 3600,
            test_before_acquire: true,
            log_statements: true,
            slow_query_threshold_ms: 500,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    /// Number of connections in pool
    pub size: u32,
    /// Number of idle connections
    pub idle: u32,
    /// Number of active connections
    pub active: u32,
    /// Maximum connections allowed
    pub max: u32,
    /// Wait time for connection acquisition (milliseconds)
    pub avg_wait_time_ms: f64,
}

/// Create a PostgreSQL connection pool with configuration
pub async fn create_pool(database_url: &str, config: PoolConfig) -> Result<PgPool> {
    // Parse connection options
    let mut connect_opts = PgConnectOptions::from_str(database_url)
        .map_err(|e| AuthzError::DatabaseError(format!("Invalid database URL: {}", e)))?;

    // Configure logging
    if config.log_statements {
        connect_opts = connect_opts.log_statements(tracing::log::LevelFilter::Debug);
        connect_opts = connect_opts.log_slow_statements(
            tracing::log::LevelFilter::Warn,
            Duration::from_millis(config.slow_query_threshold_ms),
        );
    } else {
        connect_opts = connect_opts.log_statements(tracing::log::LevelFilter::Off);
    }

    // Build pool
    let pool = PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_sec))
        .idle_timeout(Duration::from_secs(config.idle_timeout_sec))
        .max_lifetime(Duration::from_secs(config.max_lifetime_sec))
        .test_before_acquire(config.test_before_acquire)
        .connect_with(connect_opts)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to create pool: {}", e)))?;

    tracing::info!(
        "Connection pool created: min={}, max={}, acquire_timeout={}s",
        config.min_connections,
        config.max_connections,
        config.acquire_timeout_sec
    );

    Ok(pool)
}

/// Get pool statistics
pub async fn pool_stats(pool: &PgPool) -> Result<PoolStats> {
    let size = pool.size();
    let idle = pool.num_idle() as u32;
    let active = size.saturating_sub(idle);

    Ok(PoolStats {
        size,
        idle,
        active,
        max: pool.options().get_max_connections(),
        avg_wait_time_ms: 0.0, // Would need instrumentation to track
    })
}

/// Check pool health
pub async fn pool_health_check(pool: &PgPool) -> Result<bool> {
    match sqlx::query("SELECT 1").fetch_one(pool).await {
        Ok(_) => Ok(true),
        Err(e) => {
            tracing::error!("Pool health check failed: {}", e);
            Ok(false)
        }
    }
}

/// Get PostgreSQL server statistics
pub async fn server_stats(pool: &PgPool) -> Result<ServerStats> {
    let row = sqlx::query(
        r#"
        SELECT
            numbackends AS active_connections,
            xact_commit AS commits,
            xact_rollback AS rollbacks,
            blks_read AS blocks_read,
            blks_hit AS blocks_hit,
            tup_returned AS tuples_returned,
            tup_fetched AS tuples_fetched
        FROM pg_stat_database
        WHERE datname = current_database()
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| AuthzError::DatabaseError(format!("Failed to fetch server stats: {}", e)))?;

    Ok(ServerStats {
        active_connections: row.try_get("active_connections").unwrap_or(0),
        commits: row.try_get("commits").unwrap_or(0),
        rollbacks: row.try_get("rollbacks").unwrap_or(0),
        blocks_read: row.try_get("blocks_read").unwrap_or(0),
        blocks_hit: row.try_get("blocks_hit").unwrap_or(0),
        tuples_returned: row.try_get("tuples_returned").unwrap_or(0),
        tuples_fetched: row.try_get("tuples_fetched").unwrap_or(0),
    })
}

/// PostgreSQL server statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStats {
    /// Active connections to database
    pub active_connections: i32,
    /// Transaction commits
    pub commits: i64,
    /// Transaction rollbacks
    pub rollbacks: i64,
    /// Disk blocks read
    pub blocks_read: i64,
    /// Buffer cache hits
    pub blocks_hit: i64,
    /// Tuples returned
    pub tuples_returned: i64,
    /// Tuples fetched
    pub tuples_fetched: i64,
}

impl ServerStats {
    /// Calculate cache hit ratio
    pub fn cache_hit_ratio(&self) -> f64 {
        let total = self.blocks_read + self.blocks_hit;
        if total == 0 {
            0.0
        } else {
            self.blocks_hit as f64 / total as f64
        }
    }

    /// Calculate transaction success rate
    pub fn transaction_success_rate(&self) -> f64 {
        let total = self.commits + self.rollbacks;
        if total == 0 {
            1.0
        } else {
            self.commits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.max_connections, 20);
        assert_eq!(config.acquire_timeout_sec, 5);
        assert!(config.test_before_acquire);
    }

    #[test]
    fn test_pool_config_high_throughput() {
        let config = PoolConfig::high_throughput();
        assert_eq!(config.min_connections, 10);
        assert_eq!(config.max_connections, 50);
        assert_eq!(config.acquire_timeout_sec, 3);
        assert!(!config.test_before_acquire);
    }

    #[test]
    fn test_pool_config_low_latency() {
        let config = PoolConfig::low_latency();
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.max_connections, 20);
        assert_eq!(config.acquire_timeout_sec, 2);
        assert_eq!(config.slow_query_threshold_ms, 20);
    }

    #[test]
    fn test_pool_config_development() {
        let config = PoolConfig::development();
        assert_eq!(config.min_connections, 2);
        assert_eq!(config.max_connections, 10);
        assert!(config.log_statements);
    }

    #[test]
    fn test_server_stats_cache_hit_ratio() {
        let stats = ServerStats {
            active_connections: 10,
            commits: 1000,
            rollbacks: 10,
            blocks_read: 100,
            blocks_hit: 900,
            tuples_returned: 5000,
            tuples_fetched: 4500,
        };

        assert_eq!(stats.cache_hit_ratio(), 0.9);
        assert!((stats.transaction_success_rate() - 0.990).abs() < 0.001);
    }
}
