//! Structured logging configuration
//!
//! Provides enhanced logging with environment-based filtering and JSON formatting.

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

/// Tracing configuration
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Service name for logging
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Log level filter
    pub log_level: String,
    /// Enable JSON formatting
    pub json_format: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "oxify-server".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            log_level: "info".to_string(),
            json_format: false,
        }
    }
}

impl TracingConfig {
    /// Create a new tracing configuration
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }

    /// Set the log level
    pub fn with_log_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = level.into();
        self
    }

    /// Enable JSON formatting
    pub fn with_json_format(mut self, enabled: bool) -> Self {
        self.json_format = enabled;
        self
    }

    /// Initialize tracing with this configuration
    pub fn init(self) -> TracingGuard {
        // Create environment filter
        let env_filter = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(&self.log_level))
            .unwrap_or_else(|_| EnvFilter::new("info"));

        // Create registry with filter
        let registry = Registry::default().with(env_filter);

        // Add appropriate formatting layer
        if self.json_format {
            registry
                .with(
                    fmt::layer()
                        .json()
                        .with_current_span(true)
                        .with_span_list(true),
                )
                .init();
        } else {
            registry
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_line_number(true),
                )
                .init();
        }

        tracing::info!(
            service = %self.service_name,
            version = %self.service_version,
            level = %self.log_level,
            json_format = %self.json_format,
            "Tracing initialized"
        );

        TracingGuard
    }
}

/// Guard that ensures proper shutdown of tracing
#[derive(Debug)]
pub struct TracingGuard;

impl TracingGuard {
    /// Explicitly shutdown tracing (currently a no-op for tracing-subscriber)
    pub fn shutdown(self) {
        tracing::info!("Tracing shutdown");
    }
}

/// Initialize tracing with default configuration
pub fn init_tracing() -> TracingGuard {
    TracingConfig::default().init()
}

/// Initialize tracing for development
pub fn init_dev_tracing() -> TracingGuard {
    TracingConfig::new("oxify-server-dev")
        .with_log_level("debug")
        .with_json_format(false)
        .init()
}

/// Initialize tracing for production
pub fn init_prod_tracing() -> TracingGuard {
    TracingConfig::new("oxify-server")
        .with_log_level("info")
        .with_json_format(true)
        .init()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.service_name, "oxify-server");
        assert_eq!(config.log_level, "info");
        assert!(!config.json_format);
    }

    #[test]
    fn test_tracing_config_builder() {
        let config = TracingConfig::new("test-service")
            .with_log_level("debug")
            .with_json_format(true);

        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.log_level, "debug");
        assert!(config.json_format);
    }
}
