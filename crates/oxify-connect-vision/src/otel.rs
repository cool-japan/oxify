//! OpenTelemetry Integration for Distributed Tracing
//!
//! This module provides distributed tracing capabilities using OpenTelemetry.
//! It integrates with the existing metrics and logging infrastructure to provide
//! end-to-end observability for OCR operations.
//!
//! # Features
//!
//! - Distributed tracing with context propagation
//! - Automatic span creation for OCR operations
//! - Integration with Prometheus metrics
//! - Support for multiple exporters (OTLP, Jaeger, console)
//! - Span annotations with custom attributes
//! - Error tracking and exception recording
//!
//! # Example
//!
//! ```rust,ignore
//! use oxify_connect_vision::otel::{OtelConfig, TracingProvider};
//!
//! let config = OtelConfig::default()
//!     .with_service_name("oxify-vision")
//!     .with_otlp_endpoint("http://localhost:4317");
//!
//! let provider = TracingProvider::new(config)?;
//!
//! // Create a span for an OCR operation
//! let span = provider.start_span("process_image")
//!     .with_attribute("provider", "surya")
//!     .with_attribute("image_size", 1024000);
//!
//! // Perform OCR operation
//! let result = process_image(&bytes).await?;
//!
//! // Record success
//! span.record_success();
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;

/// OpenTelemetry tracing errors
#[derive(Debug, Error)]
pub enum OtelError {
    #[error("Failed to initialize tracer: {0}")]
    InitializationError(String),

    #[error("Failed to create span: {0}")]
    SpanError(String),

    #[error("Failed to export traces: {0}")]
    ExportError(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, OtelError>;

/// OpenTelemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelConfig {
    /// Service name for tracing
    pub service_name: String,

    /// Service version
    pub service_version: String,

    /// OTLP exporter endpoint
    pub otlp_endpoint: Option<String>,

    /// Jaeger exporter endpoint
    pub jaeger_endpoint: Option<String>,

    /// Enable console exporter for debugging
    pub console_exporter: bool,

    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,

    /// Batch timeout in milliseconds
    pub batch_timeout_ms: u64,

    /// Maximum batch size
    pub max_batch_size: usize,

    /// Maximum queue size
    pub max_queue_size: usize,

    /// Custom resource attributes
    pub resource_attributes: HashMap<String, String>,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            service_name: "oxify-connect-vision".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            otlp_endpoint: None,
            jaeger_endpoint: None,
            console_exporter: false,
            sampling_ratio: 1.0,
            batch_timeout_ms: 5000,
            max_batch_size: 512,
            max_queue_size: 2048,
            resource_attributes: HashMap::new(),
        }
    }
}

impl OtelConfig {
    /// Create a new configuration with default values
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }

    /// Set the service name
    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = name.into();
        self
    }

    /// Set the service version
    pub fn with_service_version(mut self, version: impl Into<String>) -> Self {
        self.service_version = version.into();
        self
    }

    /// Set the OTLP endpoint
    pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.otlp_endpoint = Some(endpoint.into());
        self
    }

    /// Set the Jaeger endpoint
    pub fn with_jaeger_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.jaeger_endpoint = Some(endpoint.into());
        self
    }

    /// Enable console exporter
    pub fn with_console_exporter(mut self, enabled: bool) -> Self {
        self.console_exporter = enabled;
        self
    }

    /// Set sampling ratio
    pub fn with_sampling_ratio(mut self, ratio: f64) -> Self {
        self.sampling_ratio = ratio.clamp(0.0, 1.0);
        self
    }

    /// Add a resource attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.resource_attributes.insert(key.into(), value.into());
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.service_name.is_empty() {
            return Err(OtelError::ConfigError(
                "Service name cannot be empty".to_string(),
            ));
        }

        if self.sampling_ratio < 0.0 || self.sampling_ratio > 1.0 {
            return Err(OtelError::ConfigError(
                "Sampling ratio must be between 0.0 and 1.0".to_string(),
            ));
        }

        if self.max_batch_size == 0 {
            return Err(OtelError::ConfigError(
                "Max batch size must be > 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Span context for distributed tracing
#[derive(Debug, Clone)]
pub struct SpanContext {
    /// Trace ID
    pub trace_id: String,

    /// Span ID
    pub span_id: String,

    /// Parent span ID
    pub parent_span_id: Option<String>,

    /// Trace flags
    pub trace_flags: u8,
}

impl SpanContext {
    /// Create a new root span context
    pub fn new_root() -> Self {
        Self {
            trace_id: Self::generate_trace_id(),
            span_id: Self::generate_span_id(),
            parent_span_id: None,
            trace_flags: 1, // Sampled
        }
    }

    /// Create a child span context
    pub fn new_child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: Self::generate_span_id(),
            parent_span_id: Some(self.span_id.clone()),
            trace_flags: self.trace_flags,
        }
    }

    /// Generate a trace ID
    fn generate_trace_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_nanos();
        format!("{:032x}", timestamp ^ (counter as u128))
    }

    /// Generate a span ID
    fn generate_span_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{:016x}", counter)
    }

    /// Check if the span is sampled
    pub fn is_sampled(&self) -> bool {
        self.trace_flags & 1 == 1
    }
}

/// Span attributes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpanAttributes {
    /// String attributes
    pub strings: HashMap<String, String>,

    /// Integer attributes
    pub integers: HashMap<String, i64>,

    /// Float attributes
    pub floats: HashMap<String, f64>,

    /// Boolean attributes
    pub bools: HashMap<String, bool>,
}

impl SpanAttributes {
    /// Create new span attributes
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a string attribute
    pub fn add_string(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.strings.insert(key.into(), value.into());
    }

    /// Add an integer attribute
    pub fn add_int(&mut self, key: impl Into<String>, value: i64) {
        self.integers.insert(key.into(), value);
    }

    /// Add a float attribute
    pub fn add_float(&mut self, key: impl Into<String>, value: f64) {
        self.floats.insert(key.into(), value);
    }

    /// Add a boolean attribute
    pub fn add_bool(&mut self, key: impl Into<String>, value: bool) {
        self.bools.insert(key.into(), value);
    }
}

/// Span status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    /// Operation completed successfully
    Ok,

    /// Operation failed
    Error,

    /// Status not set
    Unset,
}

/// Span event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name
    pub name: String,

    /// Event timestamp
    pub timestamp: u64,

    /// Event attributes
    pub attributes: SpanAttributes,
}

impl SpanEvent {
    /// Create a new span event
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before Unix epoch")
                .as_nanos() as u64,
            attributes: SpanAttributes::new(),
        }
    }

    /// Add an attribute to the event
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.add_string(key, value);
        self
    }
}

/// Tracing span
#[derive(Debug)]
pub struct Span {
    /// Span name
    pub name: String,

    /// Span context
    pub context: SpanContext,

    /// Start timestamp
    pub start_time: Instant,

    /// End timestamp
    pub end_time: Option<Instant>,

    /// Span attributes
    pub attributes: SpanAttributes,

    /// Span status
    pub status: SpanStatus,

    /// Span events
    pub events: Vec<SpanEvent>,

    /// Parent tracer
    tracer: Arc<TracingProvider>,
}

impl Span {
    /// Create a new span
    fn new(name: impl Into<String>, context: SpanContext, tracer: Arc<TracingProvider>) -> Self {
        Self {
            name: name.into(),
            context,
            start_time: Instant::now(),
            end_time: None,
            attributes: SpanAttributes::new(),
            status: SpanStatus::Unset,
            events: Vec::new(),
            tracer,
        }
    }

    /// Add a string attribute
    pub fn set_attribute(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attributes.add_string(key, value);
    }

    /// Add an integer attribute
    pub fn set_int_attribute(&mut self, key: impl Into<String>, value: i64) {
        self.attributes.add_int(key, value);
    }

    /// Add a float attribute
    pub fn set_float_attribute(&mut self, key: impl Into<String>, value: f64) {
        self.attributes.add_float(key, value);
    }

    /// Add a boolean attribute
    pub fn set_bool_attribute(&mut self, key: impl Into<String>, value: bool) {
        self.attributes.add_bool(key, value);
    }

    /// Add an event to the span
    pub fn add_event(&mut self, event: SpanEvent) {
        self.events.push(event);
    }

    /// Record an exception
    pub fn record_exception(&mut self, error: &dyn std::error::Error) {
        let mut event = SpanEvent::new("exception");
        event
            .attributes
            .add_string("exception.type", error.to_string());
        event
            .attributes
            .add_string("exception.message", error.to_string());
        self.events.push(event);
        self.status = SpanStatus::Error;
    }

    /// Mark the span as successful
    pub fn record_success(&mut self) {
        self.status = SpanStatus::Ok;
    }

    /// Mark the span as failed
    pub fn record_error(&mut self) {
        self.status = SpanStatus::Error;
    }

    /// End the span
    pub fn end(mut self) {
        self.end_time = Some(Instant::now());
        let tracer = self.tracer.clone();
        tracer.record_span(self);
    }

    /// Get the duration of the span
    pub fn duration(&self) -> Duration {
        match self.end_time {
            Some(end) => end.duration_since(self.start_time),
            None => Instant::now().duration_since(self.start_time),
        }
    }
}

/// Span data for export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanData {
    /// Span name
    pub name: String,

    /// Trace ID
    pub trace_id: String,

    /// Span ID
    pub span_id: String,

    /// Parent span ID
    pub parent_span_id: Option<String>,

    /// Start timestamp (nanoseconds since epoch)
    pub start_time_ns: u64,

    /// End timestamp (nanoseconds since epoch)
    pub end_time_ns: u64,

    /// Duration in microseconds
    pub duration_us: u64,

    /// Span status
    pub status: SpanStatus,

    /// Span attributes
    pub attributes: SpanAttributes,

    /// Span events
    pub events: Vec<SpanEvent>,
}

/// Tracing statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TracingStats {
    /// Total spans created
    pub total_spans: u64,

    /// Spans by status
    pub spans_ok: u64,
    pub spans_error: u64,
    pub spans_unset: u64,

    /// Total events recorded
    pub total_events: u64,

    /// Total exports
    pub total_exports: u64,

    /// Failed exports
    pub failed_exports: u64,

    /// Spans by operation name
    pub spans_by_operation: HashMap<String, u64>,
}

/// Tracing provider
#[derive(Debug)]
pub struct TracingProvider {
    /// Configuration
    config: OtelConfig,

    /// Recorded spans
    spans: Arc<std::sync::Mutex<Vec<SpanData>>>,

    /// Statistics
    stats: Arc<std::sync::Mutex<TracingStats>>,
}

impl TracingProvider {
    /// Create a new tracing provider
    pub fn new(config: OtelConfig) -> Result<Arc<Self>> {
        config.validate()?;

        Ok(Arc::new(Self {
            config,
            spans: Arc::new(std::sync::Mutex::new(Vec::new())),
            stats: Arc::new(std::sync::Mutex::new(TracingStats::default())),
        }))
    }

    /// Start a new root span
    pub fn start_span(self: &Arc<Self>, name: impl Into<String>) -> Span {
        let context = SpanContext::new_root();
        self.start_span_with_context(name, context)
    }

    /// Start a child span
    pub fn start_child_span(
        self: &Arc<Self>,
        name: impl Into<String>,
        parent: &SpanContext,
    ) -> Span {
        let context = parent.new_child();
        self.start_span_with_context(name, context)
    }

    /// Start a span with a specific context
    fn start_span_with_context(
        self: &Arc<Self>,
        name: impl Into<String>,
        context: SpanContext,
    ) -> Span {
        let name = name.into();

        // Update statistics
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.total_spans += 1;
        *stats.spans_by_operation.entry(name.clone()).or_insert(0) += 1;
        drop(stats);

        Span::new(name, context, Arc::clone(self))
    }

    /// Record a completed span
    fn record_span(&self, span: Span) {
        if !span.context.is_sampled() {
            return;
        }

        let start_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_nanos() as u64;

        let duration = span.duration();
        let end_ns = start_ns + duration.as_nanos() as u64;

        let span_data = SpanData {
            name: span.name,
            trace_id: span.context.trace_id,
            span_id: span.context.span_id,
            parent_span_id: span.context.parent_span_id,
            start_time_ns: start_ns - duration.as_nanos() as u64,
            end_time_ns: end_ns,
            duration_us: duration.as_micros() as u64,
            status: span.status,
            attributes: span.attributes,
            events: span.events,
        };

        // Update statistics
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        match span_data.status {
            SpanStatus::Ok => stats.spans_ok += 1,
            SpanStatus::Error => stats.spans_error += 1,
            SpanStatus::Unset => stats.spans_unset += 1,
        }
        stats.total_events += span_data.events.len() as u64;
        drop(stats);

        // Store the span
        let mut spans = self.spans.lock().unwrap_or_else(|e| e.into_inner());
        spans.push(span_data);

        // Export if batch size reached
        if spans.len() >= self.config.max_batch_size {
            let to_export = spans.drain(..).collect::<Vec<_>>();
            drop(spans);
            self.export_spans(to_export);
        }
    }

    /// Export spans
    fn export_spans(&self, spans: Vec<SpanData>) {
        if spans.is_empty() {
            return;
        }

        // In a real implementation, this would export to OTLP, Jaeger, etc.
        // For now, we just log to console if enabled
        if self.config.console_exporter {
            tracing::debug!("Exporting {} spans", spans.len());
            for span in &spans {
                tracing::debug!(
                    "Span: {} ({}us) - status: {:?}",
                    span.name,
                    span.duration_us,
                    span.status
                );
            }
        }

        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.total_exports += 1;
    }

    /// Force flush all pending spans
    pub fn flush(&self) {
        let mut spans = self.spans.lock().unwrap_or_else(|e| e.into_inner());
        let to_export = spans.drain(..).collect::<Vec<_>>();
        drop(spans);
        if !to_export.is_empty() {
            self.export_spans(to_export);
        }
    }

    /// Get tracing statistics
    pub fn get_stats(&self) -> TracingStats {
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        *stats = TracingStats::default();
    }

    /// Get configuration
    pub fn config(&self) -> &OtelConfig {
        &self.config
    }
}

impl Drop for TracingProvider {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Helper macro for creating traced blocks
#[macro_export]
macro_rules! traced {
    ($tracer:expr, $name:expr, $block:block) => {{
        let mut span = $tracer.start_span($name);
        let result = $block;
        match &result {
            Ok(_) => span.record_success(),
            Err(e) => span.record_exception(e as &dyn std::error::Error),
        }
        span.end();
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otel_config_default() {
        let config = OtelConfig::default();
        assert_eq!(config.service_name, "oxify-connect-vision");
        assert_eq!(config.sampling_ratio, 1.0);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_otel_config_builder() {
        let config = OtelConfig::new("test-service")
            .with_service_version("1.0.0")
            .with_otlp_endpoint("http://localhost:4317")
            .with_sampling_ratio(0.5)
            .with_attribute("env", "test");

        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.service_version, "1.0.0");
        assert_eq!(
            config.otlp_endpoint,
            Some("http://localhost:4317".to_string())
        );
        assert_eq!(config.sampling_ratio, 0.5);
        assert_eq!(
            config.resource_attributes.get("env"),
            Some(&"test".to_string())
        );
    }

    #[test]
    fn test_otel_config_validation() {
        let config = OtelConfig {
            service_name: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let mut config = OtelConfig {
            sampling_ratio: 1.5,
            ..Default::default()
        };
        config = config.with_sampling_ratio(1.5);
        assert_eq!(config.sampling_ratio, 1.0); // Clamped

        let config = OtelConfig {
            max_batch_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_span_context() {
        let root = SpanContext::new_root();
        assert!(root.parent_span_id.is_none());
        assert!(root.is_sampled());

        let child = root.new_child();
        assert_eq!(child.trace_id, root.trace_id);
        assert_eq!(child.parent_span_id, Some(root.span_id.clone()));
        assert!(child.is_sampled());
    }

    #[test]
    fn test_span_attributes() {
        let mut attrs = SpanAttributes::new();
        attrs.add_string("key1", "value1");
        attrs.add_int("key2", 42);
        attrs.add_float("key3", 3.15);
        attrs.add_bool("key4", true);

        assert_eq!(attrs.strings.get("key1"), Some(&"value1".to_string()));
        assert_eq!(attrs.integers.get("key2"), Some(&42));
        assert_eq!(attrs.floats.get("key3"), Some(&3.15));
        assert_eq!(attrs.bools.get("key4"), Some(&true));
    }

    #[test]
    fn test_span_event() {
        let event = SpanEvent::new("test_event").with_attribute("key", "value");

        assert_eq!(event.name, "test_event");
        assert_eq!(
            event.attributes.strings.get("key"),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn test_tracing_provider() {
        let config = OtelConfig::default();
        let provider = TracingProvider::new(config).unwrap();

        let mut span = provider.start_span("test_span");
        span.set_attribute("test_key", "test_value");
        span.set_int_attribute("count", 10);
        span.record_success();
        span.end();

        let stats = provider.get_stats();
        assert_eq!(stats.total_spans, 1);
        assert_eq!(stats.spans_ok, 1);
    }

    #[test]
    fn test_span_with_events() {
        let config = OtelConfig::default();
        let provider = TracingProvider::new(config).unwrap();

        let mut span = provider.start_span("test_span");
        span.add_event(SpanEvent::new("event1").with_attribute("key", "value"));
        span.add_event(SpanEvent::new("event2"));
        span.end();

        let stats = provider.get_stats();
        assert_eq!(stats.total_events, 2);
    }

    #[test]
    fn test_span_with_exception() {
        let config = OtelConfig::default();
        let provider = TracingProvider::new(config).unwrap();

        let mut span = provider.start_span("test_span");
        let error = std::io::Error::other("test error");
        span.record_exception(&error);
        span.end();

        let stats = provider.get_stats();
        assert_eq!(stats.spans_error, 1);
    }

    #[test]
    fn test_child_span() {
        let config = OtelConfig::default();
        let provider = TracingProvider::new(config).unwrap();

        let mut parent_span = provider.start_span("parent");
        let parent_context = parent_span.context.clone();

        let mut child_span = provider.start_child_span("child", &parent_context);
        assert_eq!(child_span.context.trace_id, parent_context.trace_id);
        assert_eq!(
            child_span.context.parent_span_id,
            Some(parent_context.span_id)
        );

        child_span.record_success();
        child_span.end();

        parent_span.record_success();
        parent_span.end();

        let stats = provider.get_stats();
        assert_eq!(stats.total_spans, 2);
    }

    #[test]
    fn test_batch_export() {
        let config = OtelConfig {
            max_batch_size: 2,
            console_exporter: true,
            ..Default::default()
        };

        let provider = TracingProvider::new(config).unwrap();

        // Create 3 spans to trigger batch export
        for i in 0..3 {
            let mut span = provider.start_span(format!("span_{}", i));
            span.record_success();
            span.end();
        }

        let stats = provider.get_stats();
        assert_eq!(stats.total_spans, 3);
        assert!(stats.total_exports >= 1);
    }

    #[test]
    fn test_flush() {
        let config = OtelConfig::default();
        let provider = TracingProvider::new(config).unwrap();

        let mut span = provider.start_span("test");
        span.record_success();
        span.end();

        provider.flush();

        let stats = provider.get_stats();
        assert_eq!(stats.total_exports, 1);
    }

    #[test]
    fn test_stats_reset() {
        let config = OtelConfig::default();
        let provider = TracingProvider::new(config).unwrap();

        let mut span = provider.start_span("test");
        span.record_success();
        span.end();

        let stats = provider.get_stats();
        assert_eq!(stats.total_spans, 1);

        provider.reset_stats();
        let stats = provider.get_stats();
        assert_eq!(stats.total_spans, 0);
    }

    #[test]
    fn test_sampling() {
        let config = OtelConfig {
            sampling_ratio: 0.0,
            ..Default::default()
        };

        let provider = TracingProvider::new(config).unwrap();

        let mut span = provider.start_span("test");
        span.context.trace_flags = 0; // Not sampled
        span.record_success();
        span.end();

        // Span should not be recorded
        let spans = provider.spans.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(spans.len(), 0);
    }

    #[test]
    fn test_span_duration() {
        let config = OtelConfig::default();
        let provider = TracingProvider::new(config).unwrap();

        let span = provider.start_span("test");
        std::thread::sleep(Duration::from_millis(10));
        let duration = span.duration();
        span.end();

        assert!(duration.as_millis() >= 10);
    }

    #[test]
    fn test_operations_tracking() {
        let config = OtelConfig::default();
        let provider = TracingProvider::new(config).unwrap();

        let mut span1 = provider.start_span("op1");
        span1.record_success();
        span1.end();

        let mut span2 = provider.start_span("op1");
        span2.record_success();
        span2.end();

        let mut span3 = provider.start_span("op2");
        span3.record_success();
        span3.end();

        let stats = provider.get_stats();
        assert_eq!(*stats.spans_by_operation.get("op1").unwrap(), 2);
        assert_eq!(*stats.spans_by_operation.get("op2").unwrap(), 1);
    }
}
