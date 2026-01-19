//! OpenTelemetry Tracing Integration for Workflow Engine
//!
//! Provides distributed tracing for workflow and node execution using OpenTelemetry.
//!
//! ## Features
//!
//! - **Workflow Tracing**: Automatic span creation for entire workflow execution
//! - **Node Tracing**: Individual spans for each node execution
//! - **Metadata Annotations**: Enrich spans with execution parameters, node types, and results
//! - **Performance Metrics**: Capture execution latency and success rates
//! - **Distributed Context**: Propagate trace context across async tasks
//! - **Error Recording**: Capture errors and failures with full context
//!
//! ## Example
//!
//! ```rust,ignore
//! use oxify_engine::otel::{TracingConfig, init_tracing};
//!
//! // Initialize tracing (requires "otel" feature)
//! let config = TracingConfig::default();
//! init_tracing(config)?;
//!
//! // Execute workflow with automatic tracing
//! let engine = Engine::new();
//! let result = engine.execute(&workflow, context).await?;
//! ```

use anyhow::Result;

#[cfg(feature = "otel")]
use opentelemetry::{
    global,
    trace::{Span, SpanKind, Status, Tracer},
    KeyValue,
};

#[cfg(feature = "otel")]
use opentelemetry_sdk::{
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
    Resource,
};

/// Configuration for OpenTelemetry tracing
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Service name for this application
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,
    /// Enable detailed node execution tracing
    pub trace_nodes: bool,
    /// Enable detailed template resolution tracing
    pub trace_templates: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "oxify-engine".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            sampling_ratio: 1.0,
            trace_nodes: true,
            trace_templates: false,
        }
    }
}

/// Initialize OpenTelemetry tracing
///
/// This sets up the global tracer provider with the specified configuration.
/// Call this once at application startup.
#[cfg(feature = "otel")]
pub fn init_tracing(config: TracingConfig) -> Result<()> {
    // Create resource with service information using builder
    let resource = Resource::builder()
        .with_attributes([
            KeyValue::new("service.name", config.service_name),
            KeyValue::new("service.version", config.service_version),
        ])
        .build();

    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_id_generator(RandomIdGenerator::default())
        .with_sampler(Sampler::TraceIdRatioBased(config.sampling_ratio))
        .build();

    global::set_tracer_provider(provider);

    Ok(())
}

/// Shutdown tracing (call before application exit)
#[cfg(feature = "otel")]
pub fn shutdown_tracing() {
    // Note: In OpenTelemetry 0.31.0, shutdown is handled automatically when the provider is dropped
    // This function is kept for backward compatibility but does nothing
}

/// Trace a workflow execution
///
/// Creates a span for the entire workflow execution and records key metrics.
///
/// # Arguments
///
/// * `workflow_name` - Name of the workflow being executed
/// * `node_count` - Total number of nodes in the workflow
/// * `f` - Function that performs the workflow execution
#[cfg(feature = "otel")]
#[allow(dead_code)]
pub fn trace_workflow<F, T>(workflow_name: &str, node_count: usize, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let tracer = global::tracer("oxify-engine");
    let mut span = tracer
        .span_builder(format!("workflow.execute.{}", workflow_name))
        .with_kind(SpanKind::Internal)
        .start(&tracer);

    // Add attributes
    span.set_attribute(KeyValue::new("workflow.name", workflow_name.to_string()));
    span.set_attribute(KeyValue::new("workflow.node_count", node_count as i64));

    // Execute workflow
    let result = f();

    // Mark span based on result
    match &result {
        Ok(_) => {
            span.set_status(Status::Ok);
            span.set_attribute(KeyValue::new("workflow.status", "success"));
        }
        Err(e) => {
            span.set_status(Status::error(e.to_string()));
            span.set_attribute(KeyValue::new("workflow.status", "failed"));
            span.set_attribute(KeyValue::new("error.message", e.to_string()));
        }
    }

    span.end();

    result
}

/// Trace a node execution
///
/// Creates a span for a single node execution and records node-specific metrics.
///
/// # Arguments
///
/// * `node_id` - Unique identifier for the node
/// * `node_type` - Type of node (LLM, Retriever, Code, etc.)
/// * `f` - Function that performs the node execution
#[cfg(feature = "otel")]
#[allow(dead_code)]
pub fn trace_node<F, T>(node_id: &str, node_type: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let tracer = global::tracer("oxify-engine");
    let mut span = tracer
        .span_builder(format!("node.execute.{}", node_type))
        .with_kind(SpanKind::Internal)
        .start(&tracer);

    // Add attributes
    span.set_attribute(KeyValue::new("node.id", node_id.to_string()));
    span.set_attribute(KeyValue::new("node.type", node_type.to_string()));

    // Execute node
    let result = f();

    // Mark span based on result
    match &result {
        Ok(_) => {
            span.set_status(Status::Ok);
            span.set_attribute(KeyValue::new("node.status", "success"));
        }
        Err(e) => {
            span.set_status(Status::error(e.to_string()));
            span.set_attribute(KeyValue::new("node.status", "failed"));
            span.set_attribute(KeyValue::new("error.message", e.to_string()));
        }
    }

    span.end();

    result
}

/// Trace a parallel execution level
///
/// Creates a span for parallel execution of multiple nodes.
///
/// # Arguments
///
/// * `level` - Execution level number
/// * `node_count` - Number of nodes in this level
/// * `f` - Function that performs the parallel execution
#[cfg(feature = "otel")]
#[allow(dead_code)]
pub fn trace_parallel_level<F, T>(level: usize, node_count: usize, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let tracer = global::tracer("oxify-engine");
    let mut span = tracer
        .span_builder(format!("execution.level.{}", level))
        .with_kind(SpanKind::Internal)
        .start(&tracer);

    // Add attributes
    span.set_attribute(KeyValue::new("level.number", level as i64));
    span.set_attribute(KeyValue::new("level.node_count", node_count as i64));
    span.set_attribute(KeyValue::new("execution.parallel", true));

    // Execute level
    let result = f();

    // Mark span based on result
    match &result {
        Ok(_) => {
            span.set_status(Status::Ok);
        }
        Err(e) => {
            span.set_status(Status::error(e.to_string()));
            span.set_attribute(KeyValue::new("error.message", e.to_string()));
        }
    }

    span.end();

    result
}

/// Trace a retry attempt
///
/// Creates a span for a retry attempt with retry count information.
#[cfg(feature = "otel")]
#[allow(dead_code)]
pub fn trace_retry(node_id: &str, attempt: usize, max_retries: usize) {
    let tracer = global::tracer("oxify-engine");
    let mut span = tracer
        .span_builder(format!("node.retry.{}", node_id))
        .with_kind(SpanKind::Internal)
        .start(&tracer);

    span.set_attribute(KeyValue::new("node.id", node_id.to_string()));
    span.set_attribute(KeyValue::new("retry.attempt", attempt as i64));
    span.set_attribute(KeyValue::new("retry.max", max_retries as i64));

    span.end();
}

/// Trace a checkpoint operation
///
/// Creates a span for checkpoint save/load operations.
#[cfg(feature = "otel")]
#[allow(dead_code)]
pub fn trace_checkpoint<F, T>(operation: &str, checkpoint_id: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let tracer = global::tracer("oxify-engine");
    let mut span = tracer
        .span_builder(format!("checkpoint.{}", operation))
        .with_kind(SpanKind::Internal)
        .start(&tracer);

    span.set_attribute(KeyValue::new("checkpoint.id", checkpoint_id.to_string()));
    span.set_attribute(KeyValue::new("checkpoint.operation", operation.to_string()));

    let result = f();

    match &result {
        Ok(_) => {
            span.set_status(Status::Ok);
        }
        Err(e) => {
            span.set_status(Status::error(e.to_string()));
            span.set_attribute(KeyValue::new("error.message", e.to_string()));
        }
    }

    span.end();

    result
}

/// Helper to record an error event
#[cfg(feature = "otel")]
#[allow(dead_code)]
pub fn record_error(error_msg: &str, node_id: Option<&str>) {
    let tracer = global::tracer("oxify-engine");
    let mut span = tracer
        .span_builder("error.event")
        .with_kind(SpanKind::Internal)
        .start(&tracer);

    span.set_status(Status::error(error_msg.to_string()));
    span.set_attribute(KeyValue::new("error.message", error_msg.to_string()));

    if let Some(id) = node_id {
        span.set_attribute(KeyValue::new("node.id", id.to_string()));
    }

    span.end();
}

/// Helper to record a custom event
#[cfg(feature = "otel")]
#[allow(dead_code)]
pub fn record_event(event_name: &str, attributes: Vec<KeyValue>) {
    let tracer = global::tracer("oxify-engine");
    let mut span = tracer
        .span_builder(event_name.to_string())
        .with_kind(SpanKind::Internal)
        .start(&tracer);

    for attr in attributes {
        span.set_attribute(attr);
    }

    span.set_status(Status::Ok);
    span.end();
}

// Stub implementations when otel feature is disabled
#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn init_tracing(_config: TracingConfig) -> Result<()> {
    Ok(())
}

#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn shutdown_tracing() {}

#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn trace_workflow<F, T>(_workflow_name: &str, _node_count: usize, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    f()
}

#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn trace_node<F, T>(_node_id: &str, _node_type: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    f()
}

#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn trace_parallel_level<F, T>(_level: usize, _node_count: usize, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    f()
}

#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn trace_retry(_node_id: &str, _attempt: usize, _max_retries: usize) {}

#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn trace_checkpoint<F, T>(_operation: &str, _checkpoint_id: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    f()
}

#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn record_error(_error_msg: &str, _node_id: Option<&str>) {}

#[cfg(not(feature = "otel"))]
#[allow(dead_code)]
pub fn record_event(_event_name: &str, _attributes: Vec<()>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.service_name, "oxify-engine");
        assert_eq!(config.sampling_ratio, 1.0);
        assert!(config.trace_nodes);
        assert!(!config.trace_templates);
    }

    #[test]
    fn test_tracing_config_custom() {
        let config = TracingConfig {
            service_name: "my-engine".to_string(),
            service_version: "2.0.0".to_string(),
            sampling_ratio: 0.5,
            trace_nodes: false,
            trace_templates: true,
        };
        assert_eq!(config.service_name, "my-engine");
        assert_eq!(config.service_version, "2.0.0");
        assert_eq!(config.sampling_ratio, 0.5);
        assert!(!config.trace_nodes);
        assert!(config.trace_templates);
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
    fn test_trace_workflow() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        let result = trace_workflow("test_workflow", 5, || Ok("success"));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");

        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_trace_workflow_failure() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        let result: Result<String> =
            trace_workflow("test_workflow", 5, || Err(anyhow::anyhow!("test error")));

        assert!(result.is_err());

        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_trace_node() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        let result = trace_node("node_123", "LLM", || Ok("node result"));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "node result");

        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_trace_parallel_level() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        let result = trace_parallel_level(1, 3, || Ok(vec!["result1", "result2", "result3"]));

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);

        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_trace_retry() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        trace_retry("node_456", 2, 3);

        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_trace_checkpoint() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        let result = trace_checkpoint("save", "checkpoint_789", || Ok("checkpoint saved"));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "checkpoint saved");

        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_record_error() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        record_error("test error", Some("node_999"));
        record_error("test error without node", None);

        shutdown_tracing();
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_record_event() {
        let config = TracingConfig::default();
        init_tracing(config).unwrap();

        record_event(
            "custom.event",
            vec![KeyValue::new("key1", "value1"), KeyValue::new("key2", 42)],
        );

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

        let result = trace_workflow("test", 5, || Ok("success"));
        assert!(result.is_ok());

        let result = trace_node("node1", "LLM", || Ok("success"));
        assert!(result.is_ok());

        let result = trace_parallel_level(1, 3, || Ok("success"));
        assert!(result.is_ok());

        trace_retry("node1", 1, 3);

        let result = trace_checkpoint("save", "cp1", || Ok("success"));
        assert!(result.is_ok());

        record_error("error", Some("node1"));
        record_event("event", vec![]);
    }
}
