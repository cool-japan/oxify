//! # OxiFY Server
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY (simplified for LLM workflow orchestration)
//! License: MIT OR Apache-2.0 (compatible with OxiRS)
//!
//! Production-ready HTTP server implementation with:
//! - Axum web framework
//! - Graceful shutdown
//! - Request logging and tracing
//! - Authentication middleware
//! - CORS support
//! - Compression
//!
//! ## Example
//!
//! ```no_run
//! use oxify_server::{ServerRuntime, ServerConfig};
//!
//! # #[tokio::main]
//! # async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
//! // Create server with development configuration
//! let server = ServerRuntime::development();
//!
//! // Run the server
//! server.run().await?;
//! # Ok(())
//! # }
//! ```

pub mod acme;
pub mod async_optimization;
pub mod cache;
pub mod chaos;
pub mod connection_pool;
pub mod ddos_protection;
pub mod error;
pub mod graphql;
pub mod metrics;
pub mod middleware;
pub mod openapi;
pub mod otel;
pub mod rate_limit;
pub mod readiness;
pub mod security;
pub mod server;
pub mod shutdown;
pub mod sse;
pub mod tls;
pub mod tracing_config;
pub mod types;
pub mod validation;
pub mod websocket;

// Re-export commonly used types
pub use acme::{AcmeConfig, AcmeError, AcmeManager, ChallengeType};
pub use async_optimization::{AsyncConfig, AsyncStats, AsyncStatsTracker};
pub use cache::{CacheConfig, ResponseCache};
pub use connection_pool::{
    ConnectionMetadata, DbPoolConfig, Http2PoolConfig, PoolStats, PoolStatsTracker, RedisPoolConfig,
};
pub use ddos_protection::{
    ConnectionLimitConfig, ConnectionTracker, DdosConfig, DdosStats, DdosStatsTracker,
    RequestTimingTracker, SlowlorisConfig,
};
pub use error::{AppError, ProblemDetails};
pub use graphql::{
    create_schema as create_graphql_schema, graphql_handler, graphql_playground, ExecutionStatus,
    OxifySchema, UserRole, WorkflowStatus,
};
pub use metrics::MetricsRegistry;
pub use otel::{init_tracer, span, OtelConfig, OtelError, OtelGuard};
pub use rate_limit::{RateLimitConfig, RateLimiter};
pub use readiness::{
    AggregateReadiness, CheckerResult, ReadinessChecker, ReadinessRegistry, ReadinessStatus,
};
pub use security::{HstsConfig, SecurityHeadersConfig};
pub use server::ServerRuntime;
pub use sse::{SseConnectionManager, SseEventBroadcaster, SseEventType};
pub use tls::{CertificateInfo, CertificateMonitor, TlsConfig, TlsError, TlsVersion};
pub use tracing_config::{TracingConfig, TracingGuard};
pub use types::{Result, ServerConfig, ServerError};
pub use validation::ValidationConfig;
pub use websocket::{WsConnection, WsConnectionManager, WsMessage};

#[cfg(feature = "cors")]
pub use types::CorsConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config() {
        let config = ServerConfig::development();
        assert_eq!(config.address.to_string(), "127.0.0.1:3000");
    }

    #[test]
    fn test_server_runtime() {
        let _runtime = ServerRuntime::development();
        // Should not panic
    }
}
