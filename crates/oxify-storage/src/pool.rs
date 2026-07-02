//! Database connection pool management
//!
//! Provides connection pooling for SQLite via the Pure-Rust OxiSQL stack
//! (`oxisql-pool` + `oxisql-sqlite-compat`, Limbo backend). Transactions are
//! obtained per checked-out connection through [`oxisql_core::Connection::transaction`]
//! rather than from the pool directly (see the module note on transactions below).
//!
//! # Transactions
//!
//! Under sqlx a `Transaction<'static, Sqlite>` was an owned value that could be
//! passed around and stored. OxiSQL transactions (`Box<dyn oxisql_core::Transaction>`)
//! are instead **borrowed from a checked-out connection**. Callers therefore
//! acquire a connection with [`DatabasePool::acquire`] and open a transaction on
//! it at the call site:
//!
//! ```ignore
//! use oxisql_core::Connection;
//! let conn = pool.acquire().await?;
//! let mut tx = conn.transaction().await?;
//! tx.execute("UPDATE ...", &[]).await?;
//! tx.commit().await?;
//! ```

use crate::{DatabaseConfig, Result, StorageError};
use oxisql_core::Connection;
use oxisql_pool::sqlite::{
    new_sqlite_compat_pool_with_config, SqliteCompatManager, SqliteCompatPool,
};
use oxisql_pool::PoolConfig;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A connection checked out from the pool.
///
/// Dereferences to an [`oxisql_sqlite_compat::SqliteConnection`], which implements
/// [`oxisql_core::Connection`]; bring `Connection` into scope to call `query` /
/// `execute` / `transaction` on it. The connection is returned to the pool when
/// this value is dropped.
pub type PooledConnection = deadpool::managed::Object<SqliteCompatManager>;

/// Default connection-acquisition timeout, in milliseconds.
const DEFAULT_ACQUIRE_TIMEOUT_MS: u64 = 30_000;

/// Default idle-connection expiry, in milliseconds.
const DEFAULT_IDLE_TIMEOUT_MS: u64 = 600_000;

/// Database connection pool
#[derive(Clone)]
pub struct DatabasePool {
    /// Shared handle to the underlying OxiSQL SQLite pool. Wrapped in `Arc`
    /// because `SqliteCompatPool` is not itself `Clone`, yet `DatabasePool` must
    /// be cloneable so stores can hold their own handle.
    inner: Arc<SqliteCompatPool>,
    /// Tracks whether [`close`](DatabasePool::close) has been called. The
    /// underlying `SqliteCompatPool` does not expose an `is_closed()` query, so
    /// we record the closed state ourselves (shared across clones via `Arc`).
    closed: Arc<AtomicBool>,
    /// Minimum idle connections requested at construction, retained for
    /// [`warmup`](DatabasePool::warmup) defaults and pool statistics.
    min_connections: u32,
    /// Configured connection-acquisition timeout, surfaced in [`PoolMetrics`].
    acquire_timeout_ms: u64,
}

impl DatabasePool {
    /// Create a new database pool.
    ///
    /// The `database_url` in `config` is normalised to a filesystem path (or
    /// `":memory:"`) via [`database_url_to_path`]; OxiSQL's SQLite backend opens
    /// a plain path rather than a `sqlite:` URL.
    pub async fn new(config: DatabaseConfig) -> Result<Self> {
        let path = database_url_to_path(&config.database_url);
        let acquire_timeout_ms = DEFAULT_ACQUIRE_TIMEOUT_MS;

        let pool_config = PoolConfig {
            max_size: config.max_connections as usize,
            min_idle: Some(config.min_connections as usize),
            connect_timeout_ms: Some(acquire_timeout_ms),
            idle_timeout_ms: Some(DEFAULT_IDLE_TIMEOUT_MS),
        };

        let inner = new_sqlite_compat_pool_with_config(path.as_str(), pool_config)
            .await
            .map_err(|e| {
                StorageError::Database(oxisql_core::OxiSqlError::ConnectionPool(e.to_string()))
            })?;

        Ok(Self {
            inner: Arc::new(inner),
            closed: Arc::new(AtomicBool::new(false)),
            min_connections: config.min_connections,
            acquire_timeout_ms,
        })
    }

    /// Warm up the connection pool.
    ///
    /// Pre-populates the pool with connections to avoid cold start latency.
    /// This is useful during application startup to ensure connections are
    /// ready before handling requests.
    ///
    /// # Arguments
    ///
    /// * `target_connections` - Number of connections to pre-create (defaults to
    ///   the pool's minimum idle count)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let pool = DatabasePool::new(config).await?;
    /// pool.warmup(Some(5)).await?; // Pre-create 5 connections
    /// ```
    pub async fn warmup(&self, target_connections: Option<u32>) -> Result<u32> {
        let target = target_connections.unwrap_or(self.min_connections);

        // Hold every acquired connection simultaneously so that the pool is
        // forced to create distinct connection objects (a sequential
        // acquire/drop cycle would keep reusing a single slot), then release
        // them all back to the idle pool at once.
        let mut acquired = Vec::new();
        let mut count = 0u32;

        for _ in 0..target {
            match self.inner.get().await {
                Ok(conn) => {
                    acquired.push(conn);
                    count += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to acquire connection during warmup: {e}");
                    break;
                }
            }
        }

        drop(acquired);

        tracing::info!("Warmed up connection pool with {count} connections");
        Ok(count)
    }

    /// Run database migrations.
    ///
    /// Applies every pending `.sql` file in the `migrations/` directory through
    /// [`oxisql_migrate`], recording applied versions in the `_oxisql_migrations`
    /// tracker table. Migrations are discovered at runtime (not embedded at
    /// compile time as with `sqlx::migrate!`), so `migrations/` is resolved
    /// relative to the process working directory.
    pub async fn migrate(&self) -> Result<()> {
        let conn = self.acquire().await?;
        let mut runner = oxisql_migrate::runner::MigrationRunner::new("migrations/");
        runner
            .run_with_conn(&*conn)
            .await
            .map_err(|e| StorageError::Migration(e.to_string()))?;
        Ok(())
    }

    /// Get a reference to the underlying OxiSQL SQLite pool.
    ///
    /// Useful for monitoring (`max_size`, `available`, `metrics`) and lifecycle
    /// (`close`) access. For running queries, prefer [`acquire`](DatabasePool::acquire).
    pub fn pool(&self) -> &SqliteCompatPool {
        self.inner.as_ref()
    }

    /// Check if database is healthy.
    pub async fn health_check(&self) -> Result<()> {
        let conn = self.acquire().await?;
        conn.query("SELECT 1", &[]).await?;
        Ok(())
    }

    /// Acquire a connection from the pool.
    ///
    /// Bring [`oxisql_core::Connection`] into scope to call `query` / `execute` /
    /// `transaction` on the returned handle.
    pub async fn acquire(&self) -> Result<PooledConnection> {
        let conn = self.inner.get().await?;
        Ok(conn)
    }

    /// Get pool statistics.
    pub fn stats(&self) -> PoolStats {
        let metrics = self.inner.metrics();
        // `size` is the number of connections currently instantiated, recovered
        // as active + idle from the deadpool status snapshot (deadpool exposes
        // active/idle counts, so this retains the original sqlx semantics of
        // "connections currently in the pool" rather than collapsing to max_size).
        let size = u32::try_from(metrics.active + metrics.idle).unwrap_or(u32::MAX);
        let max_connections = u32::try_from(metrics.max_size).unwrap_or(u32::MAX);
        PoolStats {
            size,
            num_idle: metrics.idle,
            max_connections,
            min_connections: self.min_connections,
        }
    }

    /// Get comprehensive pool metrics including health status.
    pub fn metrics(&self) -> PoolMetrics {
        PoolMetrics {
            stats: self.stats(),
            health: self.health_status(),
            acquire_timeout_ms: self.acquire_timeout_ms,
        }
    }

    /// Get pool health status based on utilization and state.
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

    /// Close the pool gracefully.
    ///
    /// This is synchronous (the OxiSQL pool's `close` is synchronous), unlike the
    /// previous sqlx-based implementation whose `close` was `async`. After this
    /// call no new connections are handed out and [`is_closed`](DatabasePool::is_closed)
    /// reports `true`.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.inner.close();
    }

    /// Check if the pool is closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
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

/// Normalise a database URL into a path accepted by the OxiSQL SQLite backend.
///
/// sqlx accepted `sqlite:`-scheme URLs with query parameters (e.g.
/// `sqlite:oxify.db?mode=rwc`), but `oxisql-sqlite-compat` opens a plain
/// filesystem path (or `":memory:"`). This strips the `sqlite:` / `sqlite://`
/// scheme and any `?query` suffix, and maps empty / `:memory:` forms to
/// `":memory:"`.
fn database_url_to_path(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .unwrap_or(url);
    let path = without_scheme.split('?').next().unwrap_or(without_scheme);
    if path.is_empty() || path == ":memory:" || path == "memory:" {
        ":memory:".to_string()
    } else {
        path.to_string()
    }
}

/// Pool statistics
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    /// Current number of connections instantiated in the pool (active + idle).
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

    #[test]
    fn test_database_url_to_path() {
        assert_eq!(database_url_to_path("sqlite:oxify.db?mode=rwc"), "oxify.db");
        assert_eq!(
            database_url_to_path("sqlite://data/oxify.db"),
            "data/oxify.db"
        );
        assert_eq!(database_url_to_path("sqlite::memory:"), ":memory:");
        assert_eq!(database_url_to_path("oxify.db"), "oxify.db");
        assert_eq!(database_url_to_path("sqlite:"), ":memory:");
    }

    /// MANDATORY MIGRATION-COUNT GATE.
    ///
    /// Opens a fresh in-memory oxisql pool and runs the *entire* on-disk
    /// `migrations/` directory through the runner, asserting that the number of
    /// migrations actually recognized + applied equals the number of migration
    /// `.sql` files present on disk.
    ///
    /// This guards against the oxisql-migrate filename landmine: files whose
    /// names do not match the exact `<14-digit-timestamp>__<name>.sql` pattern
    /// are *silently skipped* by the scanner (no error, no panic), which would
    /// leave the applied count below the file count. If this assertion ever
    /// fails with `applied < files`, a migration filename has drifted from the
    /// required pattern and must be corrected -- do NOT lower the expectation to
    /// match the wrong count.
    ///
    /// It doubles as an end-to-end check that every migration's SQL actually
    /// executes against the SQLite (Limbo) backend: an unresolved
    /// column/table reference aborts the run and fails this test loudly.
    #[tokio::test]
    async fn migration_count_gate_applies_every_file() -> Result<()> {
        // Pin to a single connection: oxisql opens an independent `:memory:`
        // database per pool slot, so one connection guarantees the runner and
        // every subsequent acquire observe the same underlying database.
        let config = DatabaseConfig {
            database_url: ":memory:".to_string(),
            max_connections: 1,
            min_connections: 1,
        };
        let pool = DatabasePool::new(config).await?;

        // Count migration files actually on disk. The directory is resolved
        // absolutely (via `CARGO_MANIFEST_DIR`) so the test does not depend on
        // the process working directory. Down-migration companions
        // (`*.down.sql`) are not separate migrations and are excluded.
        let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut file_count = 0usize;
        for entry in std::fs::read_dir(&migrations_dir)
            .map_err(|e| StorageError::Migration(format!("read_dir {migrations_dir:?}: {e}")))?
        {
            let entry = entry.map_err(|e| StorageError::Migration(format!("dir entry: {e}")))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".sql") && !name.ends_with(".down.sql") {
                file_count += 1;
            }
        }
        assert_eq!(
            file_count, 12,
            "expected 12 migration files on disk; update this guard only when \
             migrations are deliberately added or removed"
        );

        let conn = pool.acquire().await?;
        let mut runner = oxisql_migrate::runner::MigrationRunner::new(&migrations_dir);
        let applied = runner
            .run_with_conn(&*conn)
            .await
            .map_err(|e| StorageError::Migration(e.to_string()))?;
        assert_eq!(
            applied, file_count,
            "migration runner applied {applied} migrations but {file_count} \
             migration files exist on disk -- a filename likely drifted from the \
             required `<14-digit-timestamp>__<name>.sql` pattern and was silently \
             skipped by the scanner"
        );

        // Re-running against the same database must be a no-op: every version is
        // now recorded in the `_oxisql_migrations` tracker, so zero are applied.
        let reapplied = runner
            .run_with_conn(&*conn)
            .await
            .map_err(|e| StorageError::Migration(e.to_string()))?;
        assert_eq!(
            reapplied, 0,
            "re-running the migration set must be idempotent (tracker should \
             report every migration already applied)"
        );

        Ok(())
    }
}
