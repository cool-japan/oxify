//! Provider fallback mechanism for automatic failover

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmError, LlmProvider, LlmRequest,
    LlmResponse, LlmStream, Result, StreamingLlmProvider,
};
use async_trait::async_trait;

/// A provider that automatically falls back to alternative providers on failure
pub struct FallbackProvider<P> {
    providers: Vec<P>,
    /// Whether to retry on all errors or just retryable errors
    retry_all_errors: bool,
}

impl<P> FallbackProvider<P> {
    /// Create a new fallback provider with a list of providers
    ///
    /// # Panics
    /// Panics if the providers list is empty
    pub fn new(providers: Vec<P>) -> Self {
        assert!(!providers.is_empty(), "Must provide at least one provider");
        Self {
            providers,
            retry_all_errors: false,
        }
    }

    /// Configure whether to fallback on all errors (default: only retryable errors)
    pub fn with_retry_all_errors(mut self, retry_all: bool) -> Self {
        self.retry_all_errors = retry_all;
        self
    }

    /// Get the number of providers
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Check if an error should trigger fallback
    fn should_fallback(&self, error: &LlmError) -> bool {
        if self.retry_all_errors {
            true
        } else {
            // Only fallback on retryable errors
            matches!(
                error,
                LlmError::RateLimited(_)
                    | LlmError::NetworkError(_)
                    | LlmError::ApiError(_)
                    | LlmError::Timeout(_)
            )
        }
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for FallbackProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let mut last_error = None;

        for (idx, provider) in self.providers.iter().enumerate() {
            match provider.complete(request.clone()).await {
                Ok(response) => {
                    if idx > 0 {
                        tracing::info!(
                            provider_index = idx,
                            "Successfully failed over to alternative provider"
                        );
                    }
                    return Ok(response);
                }
                Err(e) => {
                    if self.should_fallback(&e) && idx < self.providers.len() - 1 {
                        tracing::warn!(
                            provider_index = idx,
                            error = %e,
                            "Provider failed, trying next provider"
                        );
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::ApiError("All providers failed".to_string())))
    }
}

#[async_trait]
impl<P: StreamingLlmProvider> StreamingLlmProvider for FallbackProvider<P> {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let mut last_error = None;

        for (idx, provider) in self.providers.iter().enumerate() {
            match provider.complete_stream(request.clone()).await {
                Ok(stream) => {
                    if idx > 0 {
                        tracing::info!(
                            provider_index = idx,
                            "Successfully failed over to alternative provider for streaming"
                        );
                    }
                    return Ok(stream);
                }
                Err(e) => {
                    if self.should_fallback(&e) && idx < self.providers.len() - 1 {
                        tracing::warn!(
                            provider_index = idx,
                            error = %e,
                            "Provider failed for streaming, trying next provider"
                        );
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::ApiError("All providers failed".to_string())))
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for FallbackProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let mut last_error = None;

        for (idx, provider) in self.providers.iter().enumerate() {
            match provider.embed(request.clone()).await {
                Ok(response) => {
                    if idx > 0 {
                        tracing::info!(
                            provider_index = idx,
                            "Successfully failed over to alternative embedding provider"
                        );
                    }
                    return Ok(response);
                }
                Err(e) => {
                    if self.should_fallback(&e) && idx < self.providers.len() - 1 {
                        tracing::warn!(
                            provider_index = idx,
                            error = %e,
                            "Embedding provider failed, trying next provider"
                        );
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::ApiError("All providers failed".to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[derive(Clone)]
    #[allow(dead_code)]
    enum MockErrorType {
        RateLimited,
        RateLimitedWithDelay(Duration),
        ApiError,
        InvalidRequest,
        Timeout,
    }

    struct MockProvider {
        should_fail: bool,
        fail_with: MockErrorType,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
            if self.should_fail {
                let err = match &self.fail_with {
                    MockErrorType::RateLimited => LlmError::RateLimited(None),
                    MockErrorType::RateLimitedWithDelay(d) => LlmError::RateLimited(Some(*d)),
                    MockErrorType::ApiError => LlmError::ApiError("API error".to_string()),
                    MockErrorType::InvalidRequest => {
                        LlmError::InvalidRequest("bad request".to_string())
                    }
                    MockErrorType::Timeout => LlmError::Timeout(Duration::from_secs(30)),
                };
                Err(err)
            } else {
                Ok(LlmResponse {
                    content: format!("Response to: {}", request.prompt),
                    model: "mock".to_string(),
                    usage: None,
                    tool_calls: Vec::new(),
                })
            }
        }
    }

    #[tokio::test]
    async fn test_fallback_first_provider_success() {
        let provider1 = MockProvider {
            should_fail: false,
            fail_with: MockErrorType::RateLimited,
        };
        let provider2 = MockProvider {
            should_fail: false,
            fail_with: MockErrorType::RateLimited,
        };

        let fallback = FallbackProvider::new(vec![provider1, provider2]);

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let result = fallback.complete(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "Response to: test");
    }

    #[tokio::test]
    async fn test_fallback_to_second_provider() {
        let provider1 = MockProvider {
            should_fail: true,
            fail_with: MockErrorType::RateLimitedWithDelay(Duration::from_secs(5)),
        };
        let provider2 = MockProvider {
            should_fail: false,
            fail_with: MockErrorType::RateLimited,
        };

        let fallback = FallbackProvider::new(vec![provider1, provider2]);

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let result = fallback.complete(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "Response to: test");
    }

    #[tokio::test]
    async fn test_fallback_all_providers_fail() {
        let provider1 = MockProvider {
            should_fail: true,
            fail_with: MockErrorType::RateLimited,
        };
        let provider2 = MockProvider {
            should_fail: true,
            fail_with: MockErrorType::ApiError,
        };

        let fallback = FallbackProvider::new(vec![provider1, provider2]);

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let result = fallback.complete(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fallback_non_retryable_error() {
        let provider1 = MockProvider {
            should_fail: true,
            fail_with: MockErrorType::InvalidRequest,
        };
        let provider2 = MockProvider {
            should_fail: false,
            fail_with: MockErrorType::RateLimited,
        };

        let fallback = FallbackProvider::new(vec![provider1, provider2]);

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        // Should not fallback on non-retryable errors
        let result = fallback.complete(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn test_fallback_retry_all_errors() {
        let provider1 = MockProvider {
            should_fail: true,
            fail_with: MockErrorType::InvalidRequest,
        };
        let provider2 = MockProvider {
            should_fail: false,
            fail_with: MockErrorType::RateLimited,
        };

        let fallback =
            FallbackProvider::new(vec![provider1, provider2]).with_retry_all_errors(true);

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        // Should fallback even on non-retryable errors when retry_all_errors is true
        let result = fallback.complete(request).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_provider_count() {
        let provider1 = MockProvider {
            should_fail: false,
            fail_with: MockErrorType::RateLimited,
        };
        let provider2 = MockProvider {
            should_fail: false,
            fail_with: MockErrorType::RateLimited,
        };

        let fallback = FallbackProvider::new(vec![provider1, provider2]);
        assert_eq!(fallback.provider_count(), 2);
    }
}
