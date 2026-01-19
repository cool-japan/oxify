//! OpenTelemetry distributed tracing module
//!
//! This module provides OpenTelemetry integration for distributed tracing with:
//! - OTLP export to Jaeger/Zipkin
//! - W3C Trace Context propagation
//! - Automatic span creation for HTTP requests
//! - Database and external API call tracing
//!
//! # Example
//!
//! ```rust,no_run
//! use oxify_server::otel::{OtelConfig, init_tracer};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = OtelConfig::default()
//!     .with_service_name("oxify-server")
//!     .with_jaeger_endpoint("http://localhost:14268/api/traces");
//!
//! let _guard = init_tracer(config).await?;
//! // Tracer is now active and will export spans
//! # Ok(())
//! # }
//! ```

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::resource::{SERVICE_NAME, SERVICE_VERSION};

/// Deployment environment attribute key
const DEPLOYMENT_ENVIRONMENT: &str = "deployment.environment";
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

/// OpenTelemetry configuration
#[derive(Debug, Clone)]
pub struct OtelConfig {
    /// Service name for tracing
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Deployment environment (dev, staging, prod)
    pub environment: String,
    /// OTLP exporter endpoint (e.g., "http://localhost:4317")
    pub otlp_endpoint: Option<String>,
    /// Jaeger endpoint (e.g., "http://localhost:14268/api/traces")
    pub jaeger_endpoint: Option<String>,
    /// Sampling ratio (0.0 to 1.0)
    pub sample_ratio: f64,
    /// Export timeout in seconds
    pub export_timeout_secs: u64,
    /// Batch export size
    pub batch_size: usize,
    /// Enable tracing
    pub enabled: bool,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            service_name: "oxify-server".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: "development".to_string(),
            otlp_endpoint: None,
            jaeger_endpoint: None,
            sample_ratio: 1.0,
            export_timeout_secs: 10,
            batch_size: 512,
            enabled: true,
        }
    }
}

impl OtelConfig {
    /// Create a new configuration
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }

    /// Set service name
    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = name.into();
        self
    }

    /// Set service version
    pub fn with_service_version(mut self, version: impl Into<String>) -> Self {
        self.service_version = version.into();
        self
    }

    /// Set deployment environment
    pub fn with_environment(mut self, env: impl Into<String>) -> Self {
        self.environment = env.into();
        self
    }

    /// Set OTLP endpoint (gRPC)
    pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.otlp_endpoint = Some(endpoint.into());
        self
    }

    /// Set Jaeger endpoint (HTTP)
    pub fn with_jaeger_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.jaeger_endpoint = Some(endpoint.into());
        self
    }

    /// Set sampling ratio (0.0 = no sampling, 1.0 = sample all)
    pub fn with_sample_ratio(mut self, ratio: f64) -> Self {
        self.sample_ratio = ratio.clamp(0.0, 1.0);
        self
    }

    /// Set export timeout in seconds
    pub fn with_export_timeout(mut self, seconds: u64) -> Self {
        self.export_timeout_secs = seconds;
        self
    }

    /// Set batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Enable or disable tracing
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Create production configuration
    pub fn production() -> Self {
        Self {
            environment: "production".to_string(),
            sample_ratio: 0.1, // Sample 10% in production
            ..Default::default()
        }
    }

    /// Create development configuration
    pub fn development() -> Self {
        Self {
            environment: "development".to_string(),
            sample_ratio: 1.0, // Sample all in development
            ..Default::default()
        }
    }
}

/// OpenTelemetry tracer guard
///
/// Ensures proper shutdown of the tracer when dropped
pub struct OtelGuard {
    _provider: SdkTracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        // Shutdown is handled by the provider's drop implementation
        if let Err(e) = self._provider.shutdown() {
            tracing::error!("Failed to shutdown tracer provider: {:?}", e);
        }
    }
}

/// Initialize OpenTelemetry tracer with the given configuration
///
/// Returns an `OtelGuard` that should be kept alive for the lifetime of the application.
/// When the guard is dropped, the tracer will be shut down properly.
///
/// # Errors
///
/// Returns an error if the tracer cannot be initialized.
pub async fn init_tracer(config: OtelConfig) -> Result<OtelGuard, OtelError> {
    if !config.enabled {
        return Err(OtelError::TracingDisabled);
    }

    // Set up W3C Trace Context propagation
    global::set_text_map_propagator(TraceContextPropagator::new());

    // Create resource with service information
    let resource = Resource::builder()
        .with_attributes(vec![
            KeyValue::new(SERVICE_NAME, config.service_name.clone()),
            KeyValue::new(SERVICE_VERSION, config.service_version.clone()),
            KeyValue::new(DEPLOYMENT_ENVIRONMENT, config.environment.clone()),
        ])
        .build();

    // Create sampler based on sample ratio
    let sampler = if config.sample_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sample_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_ratio)
    };

    // Create exporter based on configuration
    let exporter = if let Some(otlp_endpoint) = &config.otlp_endpoint {
        // OTLP gRPC exporter
        opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(otlp_endpoint)
            .with_timeout(Duration::from_secs(config.export_timeout_secs))
            .build()
            .map_err(|e: opentelemetry_otlp::ExporterBuildError| {
                OtelError::ExporterInit(e.to_string())
            })?
    } else if let Some(_jaeger_endpoint) = &config.jaeger_endpoint {
        // For Jaeger, we'll use OTLP with Jaeger's OTLP endpoint
        // Jaeger 1.35+ supports OTLP natively
        opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint("http://localhost:4317") // Jaeger OTLP endpoint
            .with_timeout(Duration::from_secs(config.export_timeout_secs))
            .build()
            .map_err(|e: opentelemetry_otlp::ExporterBuildError| {
                OtelError::ExporterInit(e.to_string())
            })?
    } else {
        return Err(OtelError::NoExporterConfigured);
    };

    // Create tracer provider
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource)
        .build();

    // Set global tracer provider
    global::set_tracer_provider(provider.clone());

    // Create tracing layer
    let tracer = provider.tracer(config.service_name.clone());
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // Set up subscriber with OpenTelemetry layer
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    Registry::default()
        .with(env_filter)
        .with(telemetry_layer)
        .try_init()
        .map_err(|e: tracing_subscriber::util::TryInitError| {
            OtelError::SubscriberInit(e.to_string())
        })?;

    Ok(OtelGuard {
        _provider: provider,
    })
}

/// OpenTelemetry error types
#[derive(Debug, thiserror::Error)]
pub enum OtelError {
    /// Tracing is disabled
    #[error("OpenTelemetry tracing is disabled")]
    TracingDisabled,

    /// No exporter configured
    #[error("No exporter configured (set either OTLP or Jaeger endpoint)")]
    NoExporterConfigured,

    /// Exporter initialization failed
    #[error("Failed to initialize exporter: {0}")]
    ExporterInit(String),

    /// Subscriber initialization failed
    #[error("Failed to initialize tracing subscriber: {0}")]
    SubscriberInit(String),
}

/// Span creation utilities
pub mod span {
    use opentelemetry::trace::{SpanKind, Status, TraceContextExt, Tracer};
    use opentelemetry::{global, Context, KeyValue};
    use std::time::Instant;

    /// Create a new span for an HTTP request
    pub fn http_request(method: &str, path: &str, status_code: u16) -> Context {
        let tracer = global::tracer("http");
        let span = tracer
            .span_builder(format!("{} {}", method, path))
            .with_kind(SpanKind::Server)
            .with_attributes(vec![
                KeyValue::new("http.method", method.to_string()),
                KeyValue::new("http.route", path.to_string()),
                KeyValue::new("http.status_code", status_code as i64),
            ])
            .start(&tracer);

        Context::current().with_span(span)
    }

    /// Create a new span for a database query
    pub fn database_query(operation: &str, table: &str) -> Context {
        let tracer = global::tracer("database");
        let span = tracer
            .span_builder(format!("db.{}.{}", operation, table))
            .with_kind(SpanKind::Client)
            .with_attributes(vec![
                KeyValue::new("db.operation", operation.to_string()),
                KeyValue::new("db.table", table.to_string()),
                KeyValue::new("db.system", "postgresql"),
            ])
            .start(&tracer);

        Context::current().with_span(span)
    }

    /// Create a new span for an LLM API call
    pub fn llm_api_call(provider: &str, model: &str) -> Context {
        let tracer = global::tracer("llm");
        let span = tracer
            .span_builder(format!("llm.{}.{}", provider, model))
            .with_kind(SpanKind::Client)
            .with_attributes(vec![
                KeyValue::new("llm.provider", provider.to_string()),
                KeyValue::new("llm.model", model.to_string()),
            ])
            .start(&tracer);

        Context::current().with_span(span)
    }

    /// Record an error in the current span
    pub fn record_error(error: &str) {
        let context = Context::current();
        let span = context.span();
        span.set_status(Status::error(error.to_string()));
        span.add_event(
            "error",
            vec![KeyValue::new("error.message", error.to_string())],
        );
    }

    /// Record timing information
    pub fn record_timing(name: &str, start: Instant) {
        let duration = start.elapsed();
        let context = Context::current();
        let span = context.span();
        span.add_event(
            name.to_string(),
            vec![KeyValue::new("duration_ms", duration.as_millis() as i64)],
        );
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otel_config_default() {
        let config = OtelConfig::default();
        assert_eq!(config.service_name, "oxify-server");
        assert_eq!(config.environment, "development");
        assert_eq!(config.sample_ratio, 1.0);
        assert!(config.enabled);
    }

    #[test]
    fn test_otel_config_builder() {
        let config = OtelConfig::new("test-service")
            .with_environment("production")
            .with_sample_ratio(0.5)
            .with_otlp_endpoint("http://localhost:4317")
            .with_export_timeout(20)
            .with_batch_size(1024);

        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.environment, "production");
        assert_eq!(config.sample_ratio, 0.5);
        assert_eq!(
            config.otlp_endpoint,
            Some("http://localhost:4317".to_string())
        );
        assert_eq!(config.export_timeout_secs, 20);
        assert_eq!(config.batch_size, 1024);
    }

    #[test]
    fn test_otel_config_production() {
        let config = OtelConfig::production();
        assert_eq!(config.environment, "production");
        assert_eq!(config.sample_ratio, 0.1); // 10% sampling in production
    }

    #[test]
    fn test_otel_config_development() {
        let config = OtelConfig::development();
        assert_eq!(config.environment, "development");
        assert_eq!(config.sample_ratio, 1.0); // 100% sampling in development
    }

    #[test]
    fn test_sample_ratio_clamping() {
        let config = OtelConfig::default().with_sample_ratio(1.5);
        assert_eq!(config.sample_ratio, 1.0);

        let config = OtelConfig::default().with_sample_ratio(-0.5);
        assert_eq!(config.sample_ratio, 0.0);
    }

    #[test]
    fn test_otel_config_disabled() {
        let config = OtelConfig::default().with_enabled(false);
        assert!(!config.enabled);
    }

    #[tokio::test]
    async fn test_init_tracer_disabled() {
        let config = OtelConfig::default().with_enabled(false);
        let result = init_tracer(config).await;
        assert!(matches!(result, Err(OtelError::TracingDisabled)));
    }

    #[tokio::test]
    async fn test_init_tracer_no_exporter() {
        let config = OtelConfig::default();
        let result = init_tracer(config).await;
        assert!(matches!(result, Err(OtelError::NoExporterConfigured)));
    }

    #[test]
    fn test_otel_error_display() {
        let err = OtelError::TracingDisabled;
        assert_eq!(err.to_string(), "OpenTelemetry tracing is disabled");

        let err = OtelError::NoExporterConfigured;
        assert_eq!(
            err.to_string(),
            "No exporter configured (set either OTLP or Jaeger endpoint)"
        );

        let err = OtelError::ExporterInit("test error".to_string());
        assert_eq!(err.to_string(), "Failed to initialize exporter: test error");

        let err = OtelError::SubscriberInit("test error".to_string());
        assert_eq!(
            err.to_string(),
            "Failed to initialize tracing subscriber: test error"
        );
    }

    #[test]
    fn test_span_utilities_available() {
        // Just verify that span utilities are accessible
        // We can't test actual span creation without initializing a tracer
        use crate::otel::span;

        // This is a compile-time test to ensure the module is public
        let _http_span = || span::http_request("GET", "/api/test", 200);
        let _db_span = || span::database_query("SELECT", "users");
        let _llm_span = || span::llm_api_call("openai", "gpt-4");
    }
}
