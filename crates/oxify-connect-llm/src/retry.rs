//! Retry logic with exponential backoff for LLM providers

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmError, LlmProvider, LlmRequest,
    LlmResponse, LlmStream, Result, StreamingLlmProvider,
};
use async_trait::async_trait;
use std::time::Duration;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    pub max_retries: u32,

    /// Initial delay before first retry (default: 1s)
    pub initial_delay: Duration,

    /// Maximum delay between retries (default: 30s)
    pub max_delay: Duration,

    /// Multiplier for exponential backoff (default: 2.0)
    pub backoff_multiplier: f64,

    /// Whether to add jitter to delays (default: true)
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create a new config with custom max retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Create a new config with custom initial delay
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Create a new config with custom max delay
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Create a new config with jitter disabled
    pub fn without_jitter(mut self) -> Self {
        self.jitter = false;
        self
    }

    /// Calculate delay for a given attempt number (0-indexed)
    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay =
            self.initial_delay.as_millis() as f64 * self.backoff_multiplier.powi(attempt as i32);
        let delay_ms = base_delay.min(self.max_delay.as_millis() as f64);

        let delay_ms = if self.jitter {
            // Add jitter: random value between 0 and delay_ms
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use std::time::SystemTime;

            let mut hasher = DefaultHasher::new();
            SystemTime::now().hash(&mut hasher);
            attempt.hash(&mut hasher);
            let hash = hasher.finish();

            let jitter_factor = (hash % 1000) as f64 / 1000.0; // 0.0 to 1.0
            delay_ms * (0.5 + jitter_factor * 0.5) // Between 50% and 100% of delay
        } else {
            delay_ms
        };

        Duration::from_millis(delay_ms as u64)
    }
}

/// Check if an error is retryable
fn is_retryable_error(error: &LlmError) -> bool {
    matches!(
        error,
        LlmError::RateLimited(_)
            | LlmError::NetworkError(_)
            | LlmError::ApiError(_)
            | LlmError::Timeout(_)
    )
}

/// A wrapper that adds retry functionality to any LLM provider
pub struct RetryProvider<P> {
    inner: P,
    config: RetryConfig,
}

impl<P> RetryProvider<P> {
    /// Create a new RetryProvider with default configuration
    pub fn new(provider: P) -> Self {
        Self {
            inner: provider,
            config: RetryConfig::default(),
        }
    }

    /// Create a new RetryProvider with custom configuration
    pub fn with_config(provider: P, config: RetryConfig) -> Self {
        Self {
            inner: provider,
            config,
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

    /// Get the retry configuration
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for RetryProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match self.inner.complete(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if attempt < self.config.max_retries && is_retryable_error(&e) {
                        // Use Retry-After header if available, otherwise use exponential backoff
                        let delay = e
                            .retry_after()
                            .unwrap_or_else(|| self.config.calculate_delay(attempt));
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_retries = self.config.max_retries,
                            delay_ms = delay.as_millis(),
                            error = %e,
                            "LLM request failed, retrying"
                        );
                        tokio::time::sleep(delay).await;
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::ApiError("Unknown retry error".to_string())))
    }
}

#[async_trait]
impl<P: StreamingLlmProvider> StreamingLlmProvider for RetryProvider<P> {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match self.inner.complete_stream(request.clone()).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    if attempt < self.config.max_retries && is_retryable_error(&e) {
                        // Use Retry-After header if available, otherwise use exponential backoff
                        let delay = e
                            .retry_after()
                            .unwrap_or_else(|| self.config.calculate_delay(attempt));
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_retries = self.config.max_retries,
                            delay_ms = delay.as_millis(),
                            error = %e,
                            "LLM stream request failed, retrying"
                        );
                        tokio::time::sleep(delay).await;
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::ApiError("Unknown retry error".to_string())))
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for RetryProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match self.inner.embed(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if attempt < self.config.max_retries && is_retryable_error(&e) {
                        // Use Retry-After header if available, otherwise use exponential backoff
                        let delay = e
                            .retry_after()
                            .unwrap_or_else(|| self.config.calculate_delay(attempt));
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_retries = self.config.max_retries,
                            delay_ms = delay.as_millis(),
                            error = %e,
                            "Embedding request failed, retrying"
                        );
                        tokio::time::sleep(delay).await;
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::ApiError("Unknown retry error".to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert!(config.jitter);
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::default()
            .with_max_retries(5)
            .with_initial_delay(Duration::from_millis(500))
            .with_max_delay(Duration::from_secs(60))
            .without_jitter();

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(500));
        assert_eq!(config.max_delay, Duration::from_secs(60));
        assert!(!config.jitter);
    }

    #[test]
    fn test_calculate_delay_no_jitter() {
        let config = RetryConfig::default().without_jitter();

        // Initial delay: 1s
        assert_eq!(config.calculate_delay(0), Duration::from_secs(1));
        // Second attempt: 1s * 2^1 = 2s
        assert_eq!(config.calculate_delay(1), Duration::from_secs(2));
        // Third attempt: 1s * 2^2 = 4s
        assert_eq!(config.calculate_delay(2), Duration::from_secs(4));
        // Fourth attempt: 1s * 2^3 = 8s
        assert_eq!(config.calculate_delay(3), Duration::from_secs(8));
    }

    #[test]
    fn test_calculate_delay_with_max() {
        let config = RetryConfig::default()
            .with_max_delay(Duration::from_secs(5))
            .without_jitter();

        // First attempts should be normal
        assert_eq!(config.calculate_delay(0), Duration::from_secs(1));
        assert_eq!(config.calculate_delay(1), Duration::from_secs(2));
        assert_eq!(config.calculate_delay(2), Duration::from_secs(4));

        // Higher attempts should be capped at max_delay
        assert_eq!(config.calculate_delay(3), Duration::from_secs(5));
        assert_eq!(config.calculate_delay(10), Duration::from_secs(5));
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error(&LlmError::RateLimited(None)));
        assert!(is_retryable_error(&LlmError::RateLimited(Some(
            Duration::from_secs(5)
        ))));
        assert!(is_retryable_error(&LlmError::ApiError("error".to_string())));
        assert!(is_retryable_error(&LlmError::Timeout(Duration::from_secs(
            30
        ))));
        assert!(!is_retryable_error(&LlmError::ConfigError(
            "invalid".to_string()
        )));
        assert!(!is_retryable_error(&LlmError::InvalidRequest(
            "bad".to_string()
        )));
    }
}
