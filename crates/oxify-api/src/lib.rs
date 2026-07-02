//! OxiFY API - REST API for workflow orchestration

pub mod auth;
pub mod auth_handlers;
pub mod authz_middleware;
pub mod batch_handlers;
pub mod checkpoint_handlers;
pub mod checkpoint_types;
pub mod feature_flags;
pub mod handlers;
pub mod mcp_handlers;
pub mod mcp_types;
pub mod middleware;
pub mod otel;
pub mod rollback_handlers;
// Disabled for SQLite migration (requires SecretStore)
// pub mod secret_handlers;
// pub mod secret_types;
pub mod sse;
pub mod storage;
pub mod types;
pub mod user_types;
pub mod vector_handlers;
pub mod version_handlers;
pub mod version_types;
pub mod websocket;

#[cfg(feature = "graphql")]
pub mod graphql;
