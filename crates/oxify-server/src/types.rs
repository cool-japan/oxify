//! Server types and configuration
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY (simplified for LLM workflow focus)
//! License: MIT OR Apache-2.0 (compatible with OxiRS)

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;

use crate::metrics::MetricsRegistry;
use crate::rate_limit::RateLimitConfig;
use crate::security::SecurityHeadersConfig;
use crate::tracing_config::TracingConfig;
use crate::validation::ValidationConfig;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server bind address
    pub address: SocketAddr,

    /// Graceful shutdown timeout in seconds
    pub graceful_shutdown_timeout: u64,

    /// Enable request logging
    pub request_logging: bool,

    /// Enable authentication
    pub auth_required: bool,

    /// Enable CORS
    #[cfg(feature = "cors")]
    pub cors: Option<CorsConfig>,

    /// Enable compression
    #[cfg(feature = "compression")]
    pub compression: bool,

    /// Rate limiting configuration
    #[serde(skip)]
    pub rate_limit: Option<RateLimitConfig>,

    /// Request validation configuration
    #[serde(skip)]
    pub validation: Option<ValidationConfig>,

    /// Security headers configuration
    #[serde(skip)]
    pub security_headers: Option<SecurityHeadersConfig>,

    /// Enable Prometheus metrics
    pub enable_metrics: bool,

    /// Metrics registry
    #[serde(skip)]
    pub metrics_registry: Option<MetricsRegistry>,

    /// Tracing configuration
    #[serde(skip)]
    pub tracing: Option<TracingConfig>,
}

impl ServerConfig {
    /// Create a new server configuration
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            graceful_shutdown_timeout: 30,
            request_logging: true,
            auth_required: false,
            #[cfg(feature = "cors")]
            cors: None,
            #[cfg(feature = "compression")]
            compression: true,
            rate_limit: None,
            validation: None,
            security_headers: None,
            enable_metrics: false,
            metrics_registry: None,
            tracing: None,
        }
    }

    /// Development configuration
    pub fn development() -> Self {
        let mut config = Self::new("127.0.0.1:3000".parse().unwrap());
        // Use relaxed security for development
        config.security_headers = Some(crate::security::relaxed_security_config());
        config.validation = Some(ValidationConfig::default());
        // Enable metrics for development
        config.enable_metrics = true;
        config.metrics_registry = Some(MetricsRegistry::default());
        config.tracing = Some(TracingConfig::new("oxify-server-dev").with_log_level("debug"));
        config
    }

    /// Production configuration
    pub fn production(address: SocketAddr) -> Self {
        let mut config = Self::new(address);
        config.auth_required = true;
        config.graceful_shutdown_timeout = 60;
        // Enable production hardening features
        config.rate_limit = Some(RateLimitConfig::default());
        config.validation = Some(ValidationConfig::default());
        config.security_headers = Some(crate::security::strict_security_config());
        // Enable observability for production
        config.enable_metrics = true;
        config.metrics_registry = Some(MetricsRegistry::default());
        config.tracing = Some(
            TracingConfig::new("oxify-server")
                .with_log_level("info")
                .with_json_format(true),
        );
        config
    }

    /// Get graceful shutdown timeout as Duration
    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.graceful_shutdown_timeout)
    }
}

/// CORS configuration
#[cfg(feature = "cors")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Allowed origins
    pub allowed_origins: Vec<String>,

    /// Allowed methods
    pub allowed_methods: Vec<String>,

    /// Allowed headers
    pub allowed_headers: Vec<String>,

    /// Max age for preflight cache
    pub max_age_secs: u64,
}

#[cfg(feature = "cors")]
impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
            ],
            allowed_headers: vec!["*".to_string()],
            max_age_secs: 3600,
        }
    }
}

/// Server error types
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Server startup failed: {0}")]
    StartupFailed(String),

    #[error("Bind error: {0}")]
    BindError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type Result<T> = std::result::Result<T, ServerError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_development() {
        let config = ServerConfig::development();
        assert_eq!(config.address.to_string(), "127.0.0.1:3000");
        assert_eq!(config.graceful_shutdown_timeout, 30);
        assert!(!config.auth_required);
    }

    #[test]
    fn test_server_config_production() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let config = ServerConfig::production(addr);
        assert_eq!(config.address.to_string(), "0.0.0.0:8080");
        assert_eq!(config.graceful_shutdown_timeout, 60);
        assert!(config.auth_required);
    }

    #[test]
    fn test_shutdown_timeout() {
        let config = ServerConfig::development();
        assert_eq!(config.shutdown_timeout(), Duration::from_secs(30));
    }
}
