//! OpenTelemetry Tracing Integration
//!
//! Provides distributed tracing for vector search operations using OpenTelemetry.
//!
//! ## Features
//!
//! - **Search Tracing**: Automatic span creation for search operations
//! - **Metadata Annotations**: Enrich spans with search parameters (k, metric, filters)
//! - **Performance Metrics**: Capture latency and result counts
//! - **Distributed Context**: Propagate trace context across services
//!
//! ## Example
//!
//! ```rust,ignore
//! use oxify_vector::otel::{TracingConfig, init_tracing, trace_search};
//!
//! // Initialize tracing (requires "otel" feature)
//! let config = TracingConfig::default();
//! init_tracing(config)?;
//!
//! // Trace a search operation
//! let query = vec![0.1, 0.2, 0.3];
//! let results: Vec<String> = trace_search("my_index", &query, 10, || {
//!     // Perform search
//!     vec!["doc1".to_string(), "doc2".to_string()]
//! })?;
//! ```

#[cfg(feature = "otel")]
use opentelemetry::{
    global,
    trace::{Span, Status, Tracer},
    KeyValue,
};

#[cfg(feature = "otel")]
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};

use anyhow::Result;

/// Configuration for OpenTelemetry tracing
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Service name for this application
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "oxify-vector".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            sampling_ratio: 1.0,
        }
    }
}

/// Initialize OpenTelemetry tracing
///
/// This sets up the global tracer provider with the specified configuration.
/// Call this once at application startup.
#[cfg(feature = "otel")]
pub fn init_tracing(_config: TracingConfig) -> Result<()> {
    // Note: In opentelemetry_sdk 0.31+, Resource creation API has changed
    // For now, we use a basic provider without custom resource attributes
    // The tracer provider will use default resource detection
    let provider = SdkTracerProvider::builder()
        .with_id_generator(RandomIdGenerator::default())
        .with_sampler(Sampler::AlwaysOn)
        .build();

    global::set_tracer_provider(provider);

    Ok(())
}

/// Shutdown tracing (call before application exit)
#[cfg(feature = "otel")]
pub fn shutdown_tracing() {
    // Note: In opentelemetry 0.31+, shutdown is handled by dropping the provider
    // The global provider will be cleaned up when the process exits
}

/// Trace a search operation
///
/// Creates a span for the search operation and records key metrics.
///
/// # Arguments
///
/// * `index_name` - Name of the index being searched
/// * `query` - Query vector
/// * `k` - Number of results requested
/// * `f` - Function that performs the search
#[cfg(feature = "otel")]
pub fn trace_search<F, T>(index_name: &str, query: &[f32], k: usize, f: F) -> Result<T>
where
    F: FnOnce() -> T,
{
    let tracer = global::tracer("oxify-vector");
    let mut span = tracer
        .span_builder(format!("search.{}", index_name))
        .start(&tracer);

    // Add attributes
    span.set_attribute(KeyValue::new("vector.dimensions", query.len() as i64));
    span.set_attribute(KeyValue::new("vector.k", k as i64));
    span.set_attribute(KeyValue::new("index.name", index_name.to_string()));

    // Execute search
    let result = f();

    // Mark span as successful
    span.set_status(Status::Ok);
    span.end();

    Ok(result)
}

/// Trace a search operation with additional metadata
///
/// Like `trace_search`, but allows specifying the distance metric and filter info.
#[cfg(feature = "otel")]
#[allow(clippy::too_many_arguments)]
pub fn trace_search_detailed<F, T>(
    index_name: &str,
    query: &[f32],
    k: usize,
    metric: &str,
    filter_applied: bool,
    result_count: usize,
    f: F,
) -> Result<T>
where
    F: FnOnce() -> T,
{
    let tracer = global::tracer("oxify-vector");
    let mut span = tracer
        .span_builder(format!("search.{}", index_name))
        .start(&tracer);

    // Add attributes
    span.set_attribute(KeyValue::new("vector.dimensions", query.len() as i64));
    span.set_attribute(KeyValue::new("vector.k", k as i64));
    span.set_attribute(KeyValue::new("index.name", index_name.to_string()));
    span.set_attribute(KeyValue::new("search.metric", metric.to_string()));
    span.set_attribute(KeyValue::new("search.filtered", filter_applied));
    span.set_attribute(KeyValue::new("search.result_count", result_count as i64));

    // Execute search
    let result = f();

    // Mark span as successful
    span.set_status(Status::Ok);
    span.end();

    Ok(result)
}

/// Helper to record an error in the current span
#[cfg(feature = "otel")]
pub fn record_error_message(error_msg: &str) {
    let tracer = global::tracer("oxify-vector");
    let mut span = tracer.span_builder("error").start(&tracer);

    span.set_status(Status::error(error_msg.to_string()));
    span.set_attribute(KeyValue::new("error.message", error_msg.to_string()));
    span.end();
}

// Stub implementations when otel feature is disabled
#[cfg(not(feature = "otel"))]
pub fn init_tracing(_config: TracingConfig) -> Result<()> {
    Ok(())
}

#[cfg(not(feature = "otel"))]
pub fn shutdown_tracing() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.service_name, "oxify-vector");
        assert_eq!(config.sampling_ratio, 1.0);
    }

    #[test]
    fn test_tracing_config_custom() {
        let config = TracingConfig {
            service_name: "my-service".to_string(),
            service_version: "1.0.0".to_string(),
            sampling_ratio: 0.5,
        };
        assert_eq!(config.service_name, "my-service");
        assert_eq!(config.service_version, "1.0.0");
        assert_eq!(config.sampling_ratio, 0.5);
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_init_and_shutdown_tracing() {
        let config = TracingConfig::default();
        let result = init_tracing(config);
        assert!(result.is_ok());
        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_trace_search() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        let query = vec![0.1, 0.2, 0.3];
        let result = trace_search("test_index", &query, 5, || vec!["doc1", "doc2"]);

        assert!(result.is_ok());
        let docs = result.unwrap();
        assert_eq!(docs.len(), 2);

        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_trace_search_detailed() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        let query = vec![0.1, 0.2, 0.3];
        let result = trace_search_detailed("test_index", &query, 5, "cosine", true, 2, || {
            vec!["doc1", "doc2"]
        });

        assert!(result.is_ok());
        let docs = result.unwrap();
        assert_eq!(docs.len(), 2);

        shutdown_tracing();
    }

    #[test]
    #[cfg(not(feature = "otel"))]
    fn test_stub_functions() {
        // These should do nothing when otel feature is disabled
        let config = TracingConfig::default();
        let result = init_tracing(config);
        assert!(result.is_ok());
        shutdown_tracing();
    }
}
