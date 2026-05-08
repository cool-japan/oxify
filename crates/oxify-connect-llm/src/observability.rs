//! Observability middleware for LLM providers
//!
//! This module provides tracing, logging, and monitoring capabilities for LLM requests.

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmProvider, LlmRequest, LlmResponse,
    LlmStream, Result, StreamingLlmProvider,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

/// Provider wrapper with tracing and logging
pub struct ObservableProvider<P> {
    inner: P,
    provider_name: String,
}

impl<P> ObservableProvider<P> {
    /// Create a new observable provider wrapper
    pub fn new(inner: P, provider_name: String) -> Self {
        Self {
            inner,
            provider_name,
        }
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for ObservableProvider<P> {
    #[tracing::instrument(
        name = "llm_completion",
        skip(self, request),
        fields(
            provider = %self.provider_name,
            prompt_length = request.prompt.len(),
            has_system_prompt = request.system_prompt.is_some(),
            temperature = ?request.temperature,
            max_tokens = ?request.max_tokens,
            num_tools = request.tools.len(),
            num_images = request.images.len(),
        )
    )]
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let start = Instant::now();

        tracing::debug!(
            provider = %self.provider_name,
            prompt = %request.prompt.chars().take(100).collect::<String>(),
            "Starting LLM completion request"
        );

        let result = self.inner.complete(request).await;
        let duration = start.elapsed();

        match &result {
            Ok(response) => {
                tracing::info!(
                    provider = %self.provider_name,
                    duration_ms = duration.as_millis(),
                    model = %response.model,
                    content_length = response.content.len(),
                    prompt_tokens = response.usage.as_ref().map(|u| u.prompt_tokens),
                    completion_tokens = response.usage.as_ref().map(|u| u.completion_tokens),
                    total_tokens = response.usage.as_ref().map(|u| u.total_tokens),
                    num_tool_calls = response.tool_calls.len(),
                    "LLM completion succeeded"
                );
            }
            Err(e) => {
                tracing::error!(
                    provider = %self.provider_name,
                    duration_ms = duration.as_millis(),
                    error = %e,
                    "LLM completion failed"
                );
            }
        }

        result
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for ObservableProvider<P> {
    #[tracing::instrument(
        name = "embedding_generation",
        skip(self, request),
        fields(
            provider = %self.provider_name,
            num_texts = request.texts.len(),
            model = ?request.model,
        )
    )]
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let start = Instant::now();

        tracing::debug!(
            provider = %self.provider_name,
            num_texts = request.texts.len(),
            "Starting embedding generation"
        );

        let result = self.inner.embed(request).await;
        let duration = start.elapsed();

        match &result {
            Ok(response) => {
                tracing::info!(
                    provider = %self.provider_name,
                    duration_ms = duration.as_millis(),
                    model = %response.model,
                    num_embeddings = response.embeddings.len(),
                    embedding_dim = response.embeddings.first().map(|e| e.len()),
                    prompt_tokens = response.usage.as_ref().map(|u| u.prompt_tokens),
                    "Embedding generation succeeded"
                );
            }
            Err(e) => {
                tracing::error!(
                    provider = %self.provider_name,
                    duration_ms = duration.as_millis(),
                    error = %e,
                    "Embedding generation failed"
                );
            }
        }

        result
    }
}

#[async_trait]
impl<P: StreamingLlmProvider> StreamingLlmProvider for ObservableProvider<P> {
    #[tracing::instrument(
        name = "llm_streaming",
        skip(self, request),
        fields(
            provider = %self.provider_name,
            prompt_length = request.prompt.len(),
        )
    )]
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let start = Instant::now();

        tracing::debug!(
            provider = %self.provider_name,
            "Starting streaming LLM completion"
        );

        let result = self.inner.complete_stream(request).await;

        match &result {
            Ok(_) => {
                let duration = start.elapsed();
                tracing::info!(
                    provider = %self.provider_name,
                    duration_ms = duration.as_millis(),
                    "Streaming LLM completion started"
                );
            }
            Err(e) => {
                let duration = start.elapsed();
                tracing::error!(
                    provider = %self.provider_name,
                    duration_ms = duration.as_millis(),
                    error = %e,
                    "Streaming LLM completion failed to start"
                );
            }
        }

        result
    }
}

/// Metrics collector for LLM operations
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    /// Total number of requests
    pub total_requests: u64,
    /// Total number of successful requests
    pub successful_requests: u64,
    /// Total number of failed requests
    pub failed_requests: u64,
    /// Total tokens used (prompt + completion)
    pub total_tokens: u64,
    /// Total cost in USD
    pub total_cost_usd: f64,
    /// Total latency in milliseconds
    pub total_latency_ms: u64,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self::default()
    }

    /// Get average latency in milliseconds
    pub fn avg_latency_ms(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_latency_ms as f64 / self.total_requests as f64
        }
    }

    /// Get success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.successful_requests as f64 / self.total_requests as f64
        }
    }

    /// Get average cost per request in USD
    pub fn avg_cost_per_request(&self) -> f64 {
        if self.successful_requests == 0 {
            0.0
        } else {
            self.total_cost_usd / self.successful_requests as f64
        }
    }

    /// Export metrics in Prometheus text format
    ///
    /// Returns a string containing all metrics in Prometheus exposition format,
    /// ready to be scraped by a Prometheus server.
    ///
    /// # Example
    /// ```
    /// use oxify_connect_llm::Metrics;
    ///
    /// let metrics = Metrics {
    ///     total_requests: 100,
    ///     successful_requests: 95,
    ///     failed_requests: 5,
    ///     total_tokens: 50000,
    ///     total_cost_usd: 2.5,
    ///     total_latency_ms: 15000,
    /// };
    ///
    /// let prometheus_output = metrics.to_prometheus();
    /// assert!(prometheus_output.contains("llm_requests_total"));
    /// ```
    pub fn to_prometheus(&self) -> String {
        format!(
            "# HELP llm_requests_total Total number of LLM requests\n\
             # TYPE llm_requests_total counter\n\
             llm_requests_total {}\n\
             \n\
             # HELP llm_requests_successful_total Total number of successful LLM requests\n\
             # TYPE llm_requests_successful_total counter\n\
             llm_requests_successful_total {}\n\
             \n\
             # HELP llm_requests_failed_total Total number of failed LLM requests\n\
             # TYPE llm_requests_failed_total counter\n\
             llm_requests_failed_total {}\n\
             \n\
             # HELP llm_tokens_total Total number of tokens processed\n\
             # TYPE llm_tokens_total counter\n\
             llm_tokens_total {}\n\
             \n\
             # HELP llm_cost_usd_total Total cost in USD\n\
             # TYPE llm_cost_usd_total counter\n\
             llm_cost_usd_total {}\n\
             \n\
             # HELP llm_latency_ms_total Total latency in milliseconds\n\
             # TYPE llm_latency_ms_total counter\n\
             llm_latency_ms_total {}\n\
             \n\
             # HELP llm_latency_avg_ms Average latency in milliseconds\n\
             # TYPE llm_latency_avg_ms gauge\n\
             llm_latency_avg_ms {}\n\
             \n\
             # HELP llm_success_rate Success rate (0.0 to 1.0)\n\
             # TYPE llm_success_rate gauge\n\
             llm_success_rate {}\n\
             \n\
             # HELP llm_cost_avg_per_request_usd Average cost per request in USD\n\
             # TYPE llm_cost_avg_per_request_usd gauge\n\
             llm_cost_avg_per_request_usd {}\n",
            self.total_requests,
            self.successful_requests,
            self.failed_requests,
            self.total_tokens,
            self.total_cost_usd,
            self.total_latency_ms,
            self.avg_latency_ms(),
            self.success_rate(),
            self.avg_cost_per_request(),
        )
    }

    /// Export metrics with labels in Prometheus text format
    ///
    /// # Arguments
    /// * `provider_name` - Name of the LLM provider (e.g., "openai", "anthropic")
    /// * `model` - Model name (e.g., "gpt-4", "claude-3-opus")
    pub fn to_prometheus_with_labels(&self, provider_name: &str, model: &str) -> String {
        format!(
            "# HELP llm_requests_total Total number of LLM requests\n\
             # TYPE llm_requests_total counter\n\
             llm_requests_total{{provider=\"{}\",model=\"{}\"}} {}\n\
             \n\
             # HELP llm_requests_successful_total Total number of successful LLM requests\n\
             # TYPE llm_requests_successful_total counter\n\
             llm_requests_successful_total{{provider=\"{}\",model=\"{}\"}} {}\n\
             \n\
             # HELP llm_requests_failed_total Total number of failed LLM requests\n\
             # TYPE llm_requests_failed_total counter\n\
             llm_requests_failed_total{{provider=\"{}\",model=\"{}\"}} {}\n\
             \n\
             # HELP llm_tokens_total Total number of tokens processed\n\
             # TYPE llm_tokens_total counter\n\
             llm_tokens_total{{provider=\"{}\",model=\"{}\"}} {}\n\
             \n\
             # HELP llm_cost_usd_total Total cost in USD\n\
             # TYPE llm_cost_usd_total counter\n\
             llm_cost_usd_total{{provider=\"{}\",model=\"{}\"}} {}\n\
             \n\
             # HELP llm_latency_ms_total Total latency in milliseconds\n\
             # TYPE llm_latency_ms_total counter\n\
             llm_latency_ms_total{{provider=\"{}\",model=\"{}\"}} {}\n\
             \n\
             # HELP llm_latency_avg_ms Average latency in milliseconds\n\
             # TYPE llm_latency_avg_ms gauge\n\
             llm_latency_avg_ms{{provider=\"{}\",model=\"{}\"}} {}\n\
             \n\
             # HELP llm_success_rate Success rate (0.0 to 1.0)\n\
             # TYPE llm_success_rate gauge\n\
             llm_success_rate{{provider=\"{}\",model=\"{}\"}} {}\n\
             \n\
             # HELP llm_cost_avg_per_request_usd Average cost per request in USD\n\
             # TYPE llm_cost_avg_per_request_usd gauge\n\
             llm_cost_avg_per_request_usd{{provider=\"{}\",model=\"{}\"}} {}\n",
            provider_name,
            model,
            self.total_requests,
            provider_name,
            model,
            self.successful_requests,
            provider_name,
            model,
            self.failed_requests,
            provider_name,
            model,
            self.total_tokens,
            provider_name,
            model,
            self.total_cost_usd,
            provider_name,
            model,
            self.total_latency_ms,
            provider_name,
            model,
            self.avg_latency_ms(),
            provider_name,
            model,
            self.success_rate(),
            provider_name,
            model,
            self.avg_cost_per_request(),
        )
    }
}

/// Provider wrapper with metrics collection
pub struct MetricsProvider<P> {
    inner: Arc<P>,
    metrics: Arc<std::sync::Mutex<Metrics>>,
}

impl<P> MetricsProvider<P> {
    /// Create a new metrics provider wrapper
    pub fn new(inner: P) -> Self {
        Self {
            inner: Arc::new(inner),
            metrics: Arc::new(std::sync::Mutex::new(Metrics::new())),
        }
    }

    /// Get current metrics snapshot
    pub fn get_metrics(&self) -> Metrics {
        self.metrics.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reset metrics
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());
        *metrics = Metrics::new();
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for MetricsProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let start = Instant::now();
        let result = self.inner.complete(request).await;
        let duration = start.elapsed();

        let mut metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());
        metrics.total_requests += 1;
        metrics.total_latency_ms += duration.as_millis() as u64;

        match &result {
            Ok(response) => {
                metrics.successful_requests += 1;
                if let Some(usage) = &response.usage {
                    metrics.total_tokens += usage.total_tokens as u64;
                }
            }
            Err(_) => {
                metrics.failed_requests += 1;
            }
        }

        result
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for MetricsProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let start = Instant::now();
        let result = self.inner.embed(request).await;
        let duration = start.elapsed();

        let mut metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());
        metrics.total_requests += 1;
        metrics.total_latency_ms += duration.as_millis() as u64;

        match &result {
            Ok(response) => {
                metrics.successful_requests += 1;
                if let Some(usage) = &response.usage {
                    metrics.total_tokens += usage.total_tokens as u64;
                }
            }
            Err(_) => {
                metrics.failed_requests += 1;
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = Metrics::new();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.successful_requests, 0);
        assert_eq!(metrics.failed_requests, 0);
    }

    #[test]
    fn test_metrics_avg_latency() {
        let mut metrics = Metrics::new();
        metrics.total_requests = 5;
        metrics.total_latency_ms = 1000;
        assert_eq!(metrics.avg_latency_ms(), 200.0);
    }

    #[test]
    fn test_metrics_success_rate() {
        let mut metrics = Metrics::new();
        metrics.total_requests = 10;
        metrics.successful_requests = 8;
        assert_eq!(metrics.success_rate(), 0.8);
    }

    #[test]
    fn test_metrics_avg_cost() {
        let mut metrics = Metrics::new();
        metrics.successful_requests = 4;
        metrics.total_cost_usd = 2.0;
        assert_eq!(metrics.avg_cost_per_request(), 0.5);
    }

    #[test]
    fn test_metrics_zero_division() {
        let metrics = Metrics::new();
        assert_eq!(metrics.avg_latency_ms(), 0.0);
        assert_eq!(metrics.success_rate(), 0.0);
        assert_eq!(metrics.avg_cost_per_request(), 0.0);
    }

    #[test]
    fn test_prometheus_export() {
        let metrics = Metrics {
            total_requests: 100,
            successful_requests: 95,
            failed_requests: 5,
            total_tokens: 50000,
            total_cost_usd: 2.5,
            total_latency_ms: 15000,
        };

        let prometheus = metrics.to_prometheus();

        // Check that all expected metrics are present
        assert!(prometheus.contains("llm_requests_total 100"));
        assert!(prometheus.contains("llm_requests_successful_total 95"));
        assert!(prometheus.contains("llm_requests_failed_total 5"));
        assert!(prometheus.contains("llm_tokens_total 50000"));
        assert!(prometheus.contains("llm_cost_usd_total 2.5"));
        assert!(prometheus.contains("llm_latency_ms_total 15000"));

        // Check that calculated metrics are present
        assert!(prometheus.contains("llm_latency_avg_ms 150"));
        assert!(prometheus.contains("llm_success_rate 0.95"));

        // Check that HELP and TYPE annotations are present
        assert!(prometheus.contains("# HELP llm_requests_total"));
        assert!(prometheus.contains("# TYPE llm_requests_total counter"));
    }

    #[test]
    fn test_prometheus_export_with_labels() {
        let metrics = Metrics {
            total_requests: 50,
            successful_requests: 48,
            failed_requests: 2,
            total_tokens: 25000,
            total_cost_usd: 1.25,
            total_latency_ms: 7500,
        };

        let prometheus = metrics.to_prometheus_with_labels("openai", "gpt-4");

        // Check that labels are present
        assert!(prometheus.contains("llm_requests_total{provider=\"openai\",model=\"gpt-4\"} 50"));
        assert!(prometheus
            .contains("llm_requests_successful_total{provider=\"openai\",model=\"gpt-4\"} 48"));
        assert!(
            prometheus.contains("llm_requests_failed_total{provider=\"openai\",model=\"gpt-4\"} 2")
        );
        assert!(prometheus.contains("llm_tokens_total{provider=\"openai\",model=\"gpt-4\"} 25000"));

        // Check that HELP and TYPE annotations are still present
        assert!(prometheus.contains("# HELP llm_requests_total"));
        assert!(prometheus.contains("# TYPE llm_requests_total counter"));
    }

    #[test]
    fn test_prometheus_export_empty_metrics() {
        let metrics = Metrics::new();
        let prometheus = metrics.to_prometheus();

        // Should contain all metric names with zero values
        assert!(prometheus.contains("llm_requests_total 0"));
        assert!(prometheus.contains("llm_latency_avg_ms 0"));
        assert!(prometheus.contains("llm_success_rate 0"));
    }
}
