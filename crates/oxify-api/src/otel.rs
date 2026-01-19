//! OpenTelemetry Distributed Tracing
//!
//! Provides distributed tracing capabilities using OpenTelemetry.
//! Exports traces to OTLP-compatible backends (Jaeger, Zipkin, etc.)

#[cfg(feature = "otel")]
use opentelemetry::trace::TracerProvider as _;
#[cfg(feature = "otel")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "otel")]
use opentelemetry_sdk::{
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
    Resource,
};
#[cfg(feature = "otel")]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "otel")]
use std::sync::OnceLock;

#[cfg(feature = "otel")]
static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

/// OpenTelemetry configuration
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OtelConfig {
    /// Service name for tracing
    pub service_name: String,
    /// OTLP endpoint (e.g., "http://localhost:4317" for Jaeger)
    pub otlp_endpoint: String,
    /// Sampling ratio (0.0 to 1.0, 1.0 = sample all traces)
    pub sample_ratio: f64,
    /// Enable tracing
    pub enabled: bool,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            service_name: "oxify-api".to_string(),
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
            sample_ratio: std::env::var("OTEL_SAMPLE_RATIO")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0),
            enabled: std::env::var("OTEL_ENABLED")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(false),
        }
    }
}

/// Initialize OpenTelemetry tracing
///
/// This sets up the tracing subscriber with OpenTelemetry export.
/// Requires the `otel` feature to be enabled.
#[cfg(feature = "otel")]
pub fn init_tracing(config: OtelConfig) -> anyhow::Result<()> {
    if !config.enabled {
        tracing::info!("OpenTelemetry tracing is disabled");
        return Ok(());
    }

    tracing::info!(
        "Initializing OpenTelemetry tracing (endpoint: {}, sample_ratio: {})",
        config.otlp_endpoint,
        config.sample_ratio
    );

    // Create OTLP exporter (OpenTelemetry 0.31 API)
    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.otlp_endpoint)
        .build()?;

    // Create resource (OpenTelemetry 0.31 API)
    let resource = Resource::builder()
        .with_attributes([opentelemetry::KeyValue::new(
            "service.name",
            config.service_name.clone(),
        )])
        .build();

    // Create tracer provider (OpenTelemetry 0.31 API)
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(otlp_exporter)
        .with_resource(resource)
        .with_sampler(Sampler::TraceIdRatioBased(config.sample_ratio))
        .with_id_generator(RandomIdGenerator::default())
        .build();

    // Get tracer
    let tracer = tracer_provider.tracer("oxify-api");

    // Store provider for shutdown
    let _ = TRACER_PROVIDER.set(tracer_provider);

    // Create tracing layer
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // Initialize subscriber with OpenTelemetry layer
    tracing_subscriber::registry()
        .with(telemetry_layer)
        .with(tracing_subscriber::fmt::layer().compact())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()?;

    tracing::info!("OpenTelemetry tracing initialized successfully");

    Ok(())
}

/// Initialize tracing without OpenTelemetry (when feature is disabled)
#[cfg(not(feature = "otel"))]
pub fn init_tracing(_config: OtelConfig) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    tracing::info!("Standard tracing initialized (OpenTelemetry feature disabled)");
    Ok(())
}

/// Shutdown OpenTelemetry gracefully
///
/// This flushes any pending traces before shutting down.
#[cfg(feature = "otel")]
pub fn shutdown_tracing() {
    if let Some(provider) = TRACER_PROVIDER.get() {
        if let Err(e) = provider.shutdown() {
            tracing::warn!("Error shutting down tracer provider: {:?}", e);
        }
    }
    tracing::info!("OpenTelemetry tracer provider shut down");
}

/// Shutdown tracing (no-op when feature is disabled)
#[cfg(not(feature = "otel"))]
pub fn shutdown_tracing() {
    // No-op when OpenTelemetry is disabled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otel_config_default() {
        let config = OtelConfig::default();
        assert_eq!(config.service_name, "oxify-api");
        assert!(!config.enabled); // Disabled by default unless env var is set
    }

    #[test]
    fn test_otel_config_custom() {
        let config = OtelConfig {
            service_name: "test-service".to_string(),
            otlp_endpoint: "http://jaeger:4317".to_string(),
            sample_ratio: 0.5,
            enabled: true,
        };

        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.otlp_endpoint, "http://jaeger:4317");
        assert_eq!(config.sample_ratio, 0.5);
        assert!(config.enabled);
    }
}
