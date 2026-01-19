//! Timeout handling for LLM providers

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmError, LlmProvider, LlmRequest,
    LlmResponse, LlmStream, Result, StreamingLlmProvider,
};
use async_trait::async_trait;
use std::time::Duration;

/// Configuration for timeout behavior
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Timeout for completion requests (default: 60s)
    pub request_timeout: Duration,

    /// Timeout for streaming requests (default: 120s)
    pub stream_timeout: Duration,

    /// Timeout for embedding requests (default: 30s)
    pub embedding_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(60),
            stream_timeout: Duration::from_secs(120),
            embedding_timeout: Duration::from_secs(30),
        }
    }
}

impl TimeoutConfig {
    /// Create a new config with a uniform timeout for all request types
    pub fn uniform(timeout: Duration) -> Self {
        Self {
            request_timeout: timeout,
            stream_timeout: timeout,
            embedding_timeout: timeout,
        }
    }

    /// Set the request timeout
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set the stream timeout
    pub fn with_stream_timeout(mut self, timeout: Duration) -> Self {
        self.stream_timeout = timeout;
        self
    }

    /// Set the embedding timeout
    pub fn with_embedding_timeout(mut self, timeout: Duration) -> Self {
        self.embedding_timeout = timeout;
        self
    }
}

/// A wrapper that adds timeout functionality to any LLM provider
pub struct TimeoutProvider<P> {
    inner: P,
    config: TimeoutConfig,
}

impl<P> TimeoutProvider<P> {
    /// Create a new TimeoutProvider with default timeout settings
    pub fn new(provider: P) -> Self {
        Self {
            inner: provider,
            config: TimeoutConfig::default(),
        }
    }

    /// Create a new TimeoutProvider with custom timeout configuration
    pub fn with_config(provider: P, config: TimeoutConfig) -> Self {
        Self {
            inner: provider,
            config,
        }
    }

    /// Create a new TimeoutProvider with a uniform timeout
    pub fn with_timeout(provider: P, timeout: Duration) -> Self {
        Self {
            inner: provider,
            config: TimeoutConfig::uniform(timeout),
        }
    }

    /// Get a reference to the inner provider
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Get a mutable reference to the inner provider
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }

    /// Get the timeout configuration
    pub fn config(&self) -> &TimeoutConfig {
        &self.config
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for TimeoutProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let timeout = self.config.request_timeout;

        match tokio::time::timeout(timeout, self.inner.complete(request)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(timeout_ms = timeout.as_millis(), "LLM request timed out");
                Err(LlmError::Timeout(timeout))
            }
        }
    }
}

#[async_trait]
impl<P: StreamingLlmProvider> StreamingLlmProvider for TimeoutProvider<P> {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let timeout = self.config.stream_timeout;

        match tokio::time::timeout(timeout, self.inner.complete_stream(request)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(
                    timeout_ms = timeout.as_millis(),
                    "LLM stream request timed out"
                );
                Err(LlmError::Timeout(timeout))
            }
        }
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for TimeoutProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let timeout = self.config.embedding_timeout;

        match tokio::time::timeout(timeout, self.inner.embed(request)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(
                    timeout_ms = timeout.as_millis(),
                    "Embedding request timed out"
                );
                Err(LlmError::Timeout(timeout))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_config_default() {
        let config = TimeoutConfig::default();
        assert_eq!(config.request_timeout, Duration::from_secs(60));
        assert_eq!(config.stream_timeout, Duration::from_secs(120));
        assert_eq!(config.embedding_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_timeout_config_uniform() {
        let config = TimeoutConfig::uniform(Duration::from_secs(45));
        assert_eq!(config.request_timeout, Duration::from_secs(45));
        assert_eq!(config.stream_timeout, Duration::from_secs(45));
        assert_eq!(config.embedding_timeout, Duration::from_secs(45));
    }

    #[test]
    fn test_timeout_config_builder() {
        let config = TimeoutConfig::default()
            .with_request_timeout(Duration::from_secs(90))
            .with_stream_timeout(Duration::from_secs(180))
            .with_embedding_timeout(Duration::from_secs(15));

        assert_eq!(config.request_timeout, Duration::from_secs(90));
        assert_eq!(config.stream_timeout, Duration::from_secs(180));
        assert_eq!(config.embedding_timeout, Duration::from_secs(15));
    }
}
