//! OxiFY Storage - SQLite persistence layer
//!
//! This crate provides database storage for workflows and executions using SQLite.
//!
//! # Architecture Overview
//!
//! ## Connection Pooling Strategy
//!
//! The storage layer uses the Pure-Rust OxiSQL SQLite pool
//! (`oxisql_pool::sqlite::SqliteCompatPool`, Limbo backend) for connection
//! management with the following strategy:
//!
//! ### Configuration
//! - **Max Connections**: Configurable (default: 10) - Upper limit of connections in pool
//! - **Min Connections**: Configurable (default: 2) - Minimum idle connections to maintain
//! - **Acquire Timeout**: 30 seconds - Maximum time to wait for an available connection
//!
//! ### Best Practices
//! 1. Use WAL mode for better concurrency
//! 2. Keep transactions short
//! 3. Use pool methods, never create direct connections
//!
//! Example:
//! ```ignore
//! let config = DatabaseConfig {
//!     database_url: "sqlite:oxify.db?mode=rwc".to_string(),
//!     max_connections: 20,
//!     min_connections: 5,
//! };
//! let pool = DatabasePool::new(config).await?;
//! ```

// Core modules (SQLite compatible) - minimal set
// mod api_key_store;     // Disabled - needs Vec<String> conversion
// pub mod archival;      // Disabled - needs Vec<Uuid> conversion
// mod audit_log_store;   // Disabled - complex
// pub mod backup;        // Disabled - complex
// pub mod batch_ops;     // Disabled - complex
pub mod cache;
// pub mod cache_warmer;  // Disabled - uses WorkflowRow
// mod checkpoint_store;  // Disabled - complex
// mod cleanup;           // Disabled - complex
pub mod connection_leak_detector;
pub mod db_utils;
mod encryption;
mod error;
mod execution_store;
pub mod health;
// pub mod jwks_cache;    // Disabled - complex
pub mod maintenance;
// pub mod maintenance_scheduler; // Disabled - complex
pub mod metrics_exporter;
// mod metrics_store;     // Disabled - needs major conversion
// migration_runner (hand-rolled schema_migrations tracker + Postgres-only
// sql/002_schema_constraints.sql) was retired during the sqlx -> oxisql
// migration; migrations now run through oxisql-migrate against the
// `migrations/` directory. See pool::DatabasePool::migrate.
pub mod migrations;
mod models;
pub mod pagination;
mod pool;
/// Internal row-mapping helpers (`RowExt` + `row_to!`) shared by store modules.
mod row_ext;
// pub mod pool_tuner;    // Disabled - complex
pub mod query_builder;
// pub mod query_profiler; // Disabled - complex
// mod quota_store;       // Disabled - complex
// #[cfg(feature = "redis-cache")]
// pub mod redis_cache;   // Disabled - depends on quota_store
pub mod retry;
// mod schedule_store;    // Disabled - complex
pub mod session_store;
// pub mod schema_validator; // Disabled - complex
// mod secret_store;      // Disabled - needs Vec<String> conversion
// pub mod seeding;       // Disabled - uses PgPool
pub mod soft_delete;
// pub mod transaction;   // Disabled - uses Postgres Transaction
mod user_store;
pub mod validation;
pub mod vector_cache;
// pub mod webhook_delivery; // Disabled - complex
// pub mod webhook_retry; // Disabled - complex
// mod webhook_store;     // Disabled - needs major conversion
mod workflow_store;
mod workflow_version_store;

// PostgreSQL-specific modules (disabled for SQLite)
// pub mod advisory_lock;
// pub mod bulk_copy;
// pub mod comment_manager;
// pub mod constraint_manager;
// pub mod enum_manager;
// pub mod explain_analyzer;
// pub mod extension_manager;
// pub mod fts;
// pub mod index_analyzer;
// pub mod jsonb_helpers;
// pub mod materialized_view;
// pub mod notify;
// pub mod optimistic_lock;
// pub mod partitioning;
// pub mod read_replica;
// pub mod rls_helper;
// pub mod sequence_manager;
// pub mod trigger_manager;
// pub mod view_manager;

// Minimal exports for SQLite migration
pub use cache::{Cache, CacheConfig, CacheMetrics, CacheStats};
pub use connection_leak_detector::{
    ConnectionToken, LeakDetector, LeakDetectorConfig, LeakReport, LeakStats, SuspectedLeak,
};
pub use encryption::EncryptionService;
pub use error::{ResourceId, ResourceType, Result, StorageError};
pub use execution_store::ExecutionStore;
pub use health::{ComponentHealth, HealthCheck, HealthCheckConfig, HealthReport, HealthStatus};
pub use maintenance::{
    IndexBloatInfo, MaintenanceConfig, MaintenanceResults, MaintenanceService, TableStats,
};
pub use metrics_exporter::{Metric, MetricType, MetricsExporter, MetricsFormat};
pub use models::{ExecutionRow, UserPermissionRow, UserRoleRow, UserRow, WorkflowRow};
pub use pagination::{
    CursorDirection, PageInfo, PaginationBuilder, PaginationRequest, PaginationResponse,
    PaginationStrategy,
};
pub use pool::{DatabasePool, PoolHealth, PoolMetrics, PoolStats, PooledConnection};
// #[cfg(feature = "redis-cache")]
// pub use redis_cache::{RedisCache, RedisCacheConfig, TwoLevelCache};
#[cfg(feature = "redis-cache")]
pub use session_store::RedisSessionStore;
pub use session_store::{
    InMemorySessionStore, SessionData, SessionError, SessionResult, SessionStore,
};
pub use soft_delete::{
    SoftDeleteBuilder, SoftDeleteFilter, SoftDeleteMetadata, SoftDeleteRestorer,
};
pub use user_store::UserStore;
pub use workflow_store::{
    BulkOperationResult, ConflictStrategy, ImportError, ImportOptions, ImportResult,
    WorkflowExport, WorkflowStore,
};
pub use workflow_version_store::{VersionComparison, WorkflowVersion, WorkflowVersionStore};

/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// SQLite connection URL
    pub database_url: String,
    /// Maximum number of connections in pool
    pub max_connections: u32,
    /// Minimum number of idle connections
    pub min_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:oxify.db?mode=rwc".to_string()),
            max_connections: 10,
            min_connections: 2,
        }
    }
}
