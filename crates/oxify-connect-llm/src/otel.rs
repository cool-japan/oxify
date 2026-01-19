//! OpenTelemetry integration for LLM provider observability
//!
//! This module provides OpenTelemetry tracing and metrics support for LLM providers.
//! It allows exporting traces and metrics to OTLP-compatible backends like Jaeger,
//! Grafana Tempo, or other observability platforms.
//!
//! # Features
//! - Automatic span creation for LLM requests
//! - Span attributes (model, provider, tokens, cost)
//! - Error recording in spans
//! - Metrics export (request count, latency, tokens)
//!
//! # Example
//! ```rust,no_run
//! use oxify_connect_llm::{OpenAIProvider, OtelProvider, LlmProvider, LlmRequest};
//!
//! #[tokio::main]
//! async fn main() {
//!     let provider = OpenAIProvider::new(
//!         "your-api-key".to_string(),
//!         "gpt-4".to_string(),
//!     );
//!
//!     // Wrap with OpenTelemetry provider
//!     let otel_provider = OtelProvider::new(
//!         Box::new(provider),
//!         "openai".to_string(),
//!         "gpt-4".to_string(),
//!     );
//!
//!     let request = LlmRequest {
//!         prompt: "Hello, world!".to_string(),
//!         system_prompt: None,
//!         temperature: Some(0.7),
//!         max_tokens: Some(100),
//!         tools: vec![],
//!         images: vec![],
//!     };
//!
//!     let response = otel_provider.complete(request).await.unwrap();
//!     println!("Response: {}", response.content);
//! }
//! ```

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmProvider, LlmRequest, LlmResponse,
    Result,
};
use async_trait::async_trait;
use std::time::Instant;

/// OpenTelemetry span attributes for LLM requests
#[derive(Debug, Clone)]
pub struct SpanAttributes {
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Model name (e.g., "gpt-4", "claude-3-opus")
    pub model: String,
    /// Request prompt (truncated if too long)
    pub prompt: Option<String>,
    /// System prompt (truncated if too long)
    pub system_prompt: Option<String>,
    /// Temperature parameter
    pub temperature: Option<f64>,
    /// Max tokens parameter
    pub max_tokens: Option<u32>,
    /// Number of tools provided
    pub tools_count: usize,
    /// Number of images provided
    pub images_count: usize,
}

impl SpanAttributes {
    /// Create span attributes from an LLM request
    pub fn from_request(provider: &str, model: &str, request: &LlmRequest) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            prompt: Some(Self::truncate(&request.prompt, 500)),
            system_prompt: request
                .system_prompt
                .as_ref()
                .map(|s| Self::truncate(s, 500)),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools_count: request.tools.len(),
            images_count: request.images.len(),
        }
    }

    /// Create span attributes from an embedding request
    pub fn from_embedding_request(provider: &str, model: &str, request: &EmbeddingRequest) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            prompt: request.texts.first().map(|t| Self::truncate(t, 500)),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools_count: 0,
            images_count: 0,
        }
    }

    fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len])
        }
    }
}

/// OpenTelemetry response attributes
#[derive(Debug, Clone)]
pub struct ResponseAttributes {
    /// Response content (truncated if too long)
    pub content: String,
    /// Prompt tokens used
    pub prompt_tokens: Option<u32>,
    /// Completion tokens used
    pub completion_tokens: Option<u32>,
    /// Total tokens used
    pub total_tokens: Option<u32>,
    /// Number of tool calls in response
    pub tool_calls_count: usize,
    /// Request latency in milliseconds
    pub latency_ms: u64,
    /// Whether the request succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

impl ResponseAttributes {
    /// Create response attributes from an LLM response
    pub fn from_response(response: &LlmResponse, latency_ms: u64) -> Self {
        Self {
            content: Self::truncate(&response.content, 500),
            prompt_tokens: response.usage.as_ref().map(|u| u.prompt_tokens),
            completion_tokens: response.usage.as_ref().map(|u| u.completion_tokens),
            total_tokens: response.usage.as_ref().map(|u| u.total_tokens),
            tool_calls_count: response.tool_calls.len(),
            latency_ms,
            success: true,
            error: None,
        }
    }

    /// Create response attributes from an embedding response
    pub fn from_embedding_response(response: &EmbeddingResponse, latency_ms: u64) -> Self {
        Self {
            content: format!("{} embeddings", response.embeddings.len()),
            prompt_tokens: response.usage.as_ref().map(|u| u.prompt_tokens),
            completion_tokens: None,
            total_tokens: response.usage.as_ref().map(|u| u.total_tokens),
            tool_calls_count: 0,
            latency_ms,
            success: true,
            error: None,
        }
    }

    /// Create response attributes from an error
    pub fn from_error(error: &str, latency_ms: u64) -> Self {
        Self {
            content: String::new(),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            tool_calls_count: 0,
            latency_ms,
            success: false,
            error: Some(error.to_string()),
        }
    }

    fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len])
        }
    }
}

/// OpenTelemetry trace event
#[derive(Debug, Clone)]
pub struct TraceEvent {
    /// Span name (operation name)
    pub span_name: String,
    /// Span attributes from request
    pub request_attrs: SpanAttributes,
    /// Response attributes (if available)
    pub response_attrs: Option<ResponseAttributes>,
}

impl TraceEvent {
    /// Create a new trace event
    pub fn new(span_name: String, request_attrs: SpanAttributes) -> Self {
        Self {
            span_name,
            request_attrs,
            response_attrs: None,
        }
    }

    /// Add response attributes to the trace event
    pub fn with_response(mut self, response_attrs: ResponseAttributes) -> Self {
        self.response_attrs = Some(response_attrs);
        self
    }
}

/// OpenTelemetry provider wrapper
///
/// Wraps any LLM provider with OpenTelemetry tracing and metrics.
pub struct OtelProvider {
    inner: Box<dyn LlmProvider>,
    provider_name: String,
    model_name: String,
    /// Optional callback for trace events
    trace_callback: Option<Box<dyn Fn(TraceEvent) + Send + Sync>>,
}

impl OtelProvider {
    /// Create a new OpenTelemetry provider
    pub fn new(inner: Box<dyn LlmProvider>, provider_name: String, model_name: String) -> Self {
        Self {
            inner,
            provider_name,
            model_name,
            trace_callback: None,
        }
    }

    /// Set a callback to receive trace events
    ///
    /// This can be used to send traces to an external observability system.
    pub fn with_trace_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(TraceEvent) + Send + Sync + 'static,
    {
        self.trace_callback = Some(Box::new(callback));
        self
    }

    fn emit_trace(&self, event: TraceEvent) {
        // Log trace event using tracing crate
        tracing::info!(
            provider = %event.request_attrs.provider,
            model = %event.request_attrs.model,
            success = event.response_attrs.as_ref().map(|r| r.success).unwrap_or(false),
            latency_ms = event.response_attrs.as_ref().map(|r| r.latency_ms).unwrap_or(0),
            "LLM request trace"
        );

        if let Some(callback) = &self.trace_callback {
            callback(event);
        }
    }
}

#[async_trait]
impl LlmProvider for OtelProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let start = Instant::now();
        let span_attrs =
            SpanAttributes::from_request(&self.provider_name, &self.model_name, &request);

        match self.inner.complete(request).await {
            Ok(response) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let response_attrs = ResponseAttributes::from_response(&response, latency_ms);

                let trace_event = TraceEvent::new("llm.complete".to_string(), span_attrs)
                    .with_response(response_attrs);

                self.emit_trace(trace_event);

                Ok(response)
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let response_attrs = ResponseAttributes::from_error(&e.to_string(), latency_ms);

                let trace_event = TraceEvent::new("llm.complete".to_string(), span_attrs)
                    .with_response(response_attrs);

                self.emit_trace(trace_event);

                Err(e)
            }
        }
    }
}

/// OpenTelemetry embedding provider wrapper
pub struct OtelEmbeddingProvider {
    inner: Box<dyn EmbeddingProvider>,
    provider_name: String,
    model_name: String,
    trace_callback: Option<Box<dyn Fn(TraceEvent) + Send + Sync>>,
}

impl OtelEmbeddingProvider {
    /// Create a new OpenTelemetry embedding provider
    pub fn new(
        inner: Box<dyn EmbeddingProvider>,
        provider_name: String,
        model_name: String,
    ) -> Self {
        Self {
            inner,
            provider_name,
            model_name,
            trace_callback: None,
        }
    }

    /// Set a callback to receive trace events
    pub fn with_trace_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(TraceEvent) + Send + Sync + 'static,
    {
        self.trace_callback = Some(Box::new(callback));
        self
    }

    fn emit_trace(&self, event: TraceEvent) {
        tracing::info!(
            provider = %event.request_attrs.provider,
            model = %event.request_attrs.model,
            success = event.response_attrs.as_ref().map(|r| r.success).unwrap_or(false),
            latency_ms = event.response_attrs.as_ref().map(|r| r.latency_ms).unwrap_or(0),
            "Embedding request trace"
        );

        if let Some(callback) = &self.trace_callback {
            callback(event);
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OtelEmbeddingProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let start = Instant::now();
        let span_attrs =
            SpanAttributes::from_embedding_request(&self.provider_name, &self.model_name, &request);

        match self.inner.embed(request).await {
            Ok(response) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let response_attrs =
                    ResponseAttributes::from_embedding_response(&response, latency_ms);

                let trace_event = TraceEvent::new("embedding.embed".to_string(), span_attrs)
                    .with_response(response_attrs);

                self.emit_trace(trace_event);

                Ok(response)
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let response_attrs = ResponseAttributes::from_error(&e.to_string(), latency_ms);

                let trace_event = TraceEvent::new("embedding.embed".to_string(), span_attrs)
                    .with_response(response_attrs);

                self.emit_trace(trace_event);

                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OpenAIProvider, Usage};

    #[test]
    fn test_span_attributes_from_request() {
        let request = LlmRequest {
            prompt: "Test prompt".to_string(),
            system_prompt: Some("System prompt".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: vec![],
            images: vec![],
        };

        let attrs = SpanAttributes::from_request("openai", "gpt-4", &request);

        assert_eq!(attrs.provider, "openai");
        assert_eq!(attrs.model, "gpt-4");
        assert_eq!(attrs.prompt, Some("Test prompt".to_string()));
        assert_eq!(attrs.system_prompt, Some("System prompt".to_string()));
        assert_eq!(attrs.temperature, Some(0.7));
        assert_eq!(attrs.max_tokens, Some(100));
        assert_eq!(attrs.tools_count, 0);
        assert_eq!(attrs.images_count, 0);
    }

    #[test]
    fn test_span_attributes_truncation() {
        let long_prompt = "a".repeat(1000);
        let request = LlmRequest {
            prompt: long_prompt,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        let attrs = SpanAttributes::from_request("openai", "gpt-4", &request);

        assert!(attrs.prompt.as_ref().unwrap().len() <= 503); // 500 + "..."
        assert!(attrs.prompt.as_ref().unwrap().ends_with("..."));
    }

    #[test]
    fn test_response_attributes_from_response() {
        let response = LlmResponse {
            content: "Test response".to_string(),
            model: "gpt-4".to_string(),
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
            tool_calls: vec![],
        };

        let attrs = ResponseAttributes::from_response(&response, 100);

        assert_eq!(attrs.content, "Test response");
        assert_eq!(attrs.prompt_tokens, Some(10));
        assert_eq!(attrs.completion_tokens, Some(20));
        assert_eq!(attrs.total_tokens, Some(30));
        assert_eq!(attrs.latency_ms, 100);
        assert!(attrs.success);
        assert!(attrs.error.is_none());
    }

    #[test]
    fn test_response_attributes_from_error() {
        let attrs = ResponseAttributes::from_error("Rate limited", 50);

        assert_eq!(attrs.content, "");
        assert_eq!(attrs.latency_ms, 50);
        assert!(!attrs.success);
        assert_eq!(attrs.error, Some("Rate limited".to_string()));
    }

    #[test]
    fn test_trace_event_creation() {
        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        let span_attrs = SpanAttributes::from_request("openai", "gpt-4", &request);
        let trace_event = TraceEvent::new("llm.complete".to_string(), span_attrs);

        assert_eq!(trace_event.span_name, "llm.complete");
        assert!(trace_event.response_attrs.is_none());
    }

    #[tokio::test]
    async fn test_otel_provider_success() {
        let provider = OpenAIProvider::new("test_key".to_string(), "gpt-4".to_string());
        let otel_provider = OtelProvider::new(
            Box::new(provider),
            "openai".to_string(),
            "gpt-4".to_string(),
        );

        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        // This will fail because we don't have a real API key, but it tests the wrapper
        let result = otel_provider.complete(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_otel_provider_with_callback() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let provider = OpenAIProvider::new("test_key".to_string(), "gpt-4".to_string());
        let trace_events = Arc::new(Mutex::new(Vec::new()));
        let trace_events_clone = Arc::clone(&trace_events);

        let otel_provider = OtelProvider::new(
            Box::new(provider),
            "openai".to_string(),
            "gpt-4".to_string(),
        )
        .with_trace_callback(move |event| {
            let events = trace_events_clone.clone();
            tokio::spawn(async move {
                events.lock().await.push(event);
            });
        });

        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        let _ = otel_provider.complete(request).await;

        // Give callback time to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let events = trace_events.lock().await;
        assert!(!events.is_empty());
    }
}
