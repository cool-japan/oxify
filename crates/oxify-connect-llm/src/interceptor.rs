//! Request/Response Interceptor System
//!
//! This module provides a flexible interceptor/middleware system that allows users to
//! intercept and modify requests before they are sent and responses after they are received.
//!
//! # Examples
//!
//! ```
//! use oxify_connect_llm::{
//!     LlmProvider, LlmRequest, LlmResponse, Result,
//!     InterceptorProvider, RequestInterceptor, ResponseInterceptor,
//!     OpenAIProvider,
//! };
//! use async_trait::async_trait;
//!
//! // Create a request interceptor that adds a prefix to all prompts
//! struct PrefixInterceptor {
//!     prefix: String,
//! }
//!
//! #[async_trait]
//! impl RequestInterceptor for PrefixInterceptor {
//!     async fn intercept_request(&self, mut request: LlmRequest) -> Result<LlmRequest> {
//!         request.prompt = format!("{}{}", self.prefix, request.prompt);
//!         Ok(request)
//!     }
//! }
//!
//! // Create a response interceptor that converts to uppercase
//! struct UppercaseInterceptor;
//!
//! #[async_trait]
//! impl ResponseInterceptor for UppercaseInterceptor {
//!     async fn intercept_response(&self, mut response: LlmResponse) -> Result<LlmResponse> {
//!         response.content = response.content.to_uppercase();
//!         Ok(response)
//!     }
//! }
//!
//! # async fn example() -> Result<()> {
//! // Wrap a provider with interceptors
//! let provider = OpenAIProvider::new("key".to_string(), "gpt-4".to_string());
//! let provider = InterceptorProvider::new(provider)
//!     .with_request_interceptor(Box::new(PrefixInterceptor {
//!         prefix: "Context: ".to_string(),
//!     }))
//!     .with_response_interceptor(Box::new(UppercaseInterceptor));
//!
//! // Use the provider normally - interceptors will be applied automatically
//! # Ok(())
//! # }
//! ```

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmProvider, LlmRequest, LlmResponse,
    LlmStream, Result, StreamingLlmProvider,
};
use async_trait::async_trait;
use std::sync::Arc;

/// Trait for intercepting and modifying requests before they are sent
#[async_trait]
pub trait RequestInterceptor: Send + Sync {
    /// Intercept and potentially modify a request before it's sent to the provider
    async fn intercept_request(&self, request: LlmRequest) -> Result<LlmRequest>;
}

/// Trait for intercepting and modifying responses after they are received
#[async_trait]
pub trait ResponseInterceptor: Send + Sync {
    /// Intercept and potentially modify a response after it's received from the provider
    async fn intercept_response(&self, response: LlmResponse) -> Result<LlmResponse>;
}

/// Trait for intercepting embedding requests
#[async_trait]
pub trait EmbeddingRequestInterceptor: Send + Sync {
    /// Intercept and potentially modify an embedding request before it's sent
    async fn intercept_embedding_request(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingRequest>;
}

/// Trait for intercepting embedding responses
#[async_trait]
pub trait EmbeddingResponseInterceptor: Send + Sync {
    /// Intercept and potentially modify an embedding response after it's received
    async fn intercept_embedding_response(
        &self,
        response: EmbeddingResponse,
    ) -> Result<EmbeddingResponse>;
}

/// Provider wrapper that applies interceptors to requests and responses
pub struct InterceptorProvider<P> {
    provider: Arc<P>,
    request_interceptors: Vec<Box<dyn RequestInterceptor>>,
    response_interceptors: Vec<Box<dyn ResponseInterceptor>>,
}

impl<P> InterceptorProvider<P> {
    /// Create a new interceptor provider wrapping the given provider
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
        }
    }

    /// Add a request interceptor (interceptors are applied in the order they are added)
    pub fn with_request_interceptor(mut self, interceptor: Box<dyn RequestInterceptor>) -> Self {
        self.request_interceptors.push(interceptor);
        self
    }

    /// Add a response interceptor (interceptors are applied in the order they are added)
    pub fn with_response_interceptor(mut self, interceptor: Box<dyn ResponseInterceptor>) -> Self {
        self.response_interceptors.push(interceptor);
        self
    }

    /// Apply all request interceptors to a request
    async fn apply_request_interceptors(&self, mut request: LlmRequest) -> Result<LlmRequest> {
        for interceptor in &self.request_interceptors {
            request = interceptor.intercept_request(request).await?;
        }
        Ok(request)
    }

    /// Apply all response interceptors to a response
    async fn apply_response_interceptors(&self, mut response: LlmResponse) -> Result<LlmResponse> {
        for interceptor in &self.response_interceptors {
            response = interceptor.intercept_response(response).await?;
        }
        Ok(response)
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for InterceptorProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let request = self.apply_request_interceptors(request).await?;
        let response = self.provider.complete(request).await?;
        self.apply_response_interceptors(response).await
    }
}

#[async_trait]
impl<P: StreamingLlmProvider> StreamingLlmProvider for InterceptorProvider<P> {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let request = self.apply_request_interceptors(request).await?;
        self.provider.complete_stream(request).await
    }
}

/// Provider wrapper for embedding interceptors
pub struct EmbeddingInterceptorProvider<P> {
    provider: Arc<P>,
    request_interceptors: Vec<Box<dyn EmbeddingRequestInterceptor>>,
    response_interceptors: Vec<Box<dyn EmbeddingResponseInterceptor>>,
}

impl<P> EmbeddingInterceptorProvider<P> {
    /// Create a new embedding interceptor provider wrapping the given provider
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
        }
    }

    /// Add an embedding request interceptor
    pub fn with_request_interceptor(
        mut self,
        interceptor: Box<dyn EmbeddingRequestInterceptor>,
    ) -> Self {
        self.request_interceptors.push(interceptor);
        self
    }

    /// Add an embedding response interceptor
    pub fn with_response_interceptor(
        mut self,
        interceptor: Box<dyn EmbeddingResponseInterceptor>,
    ) -> Self {
        self.response_interceptors.push(interceptor);
        self
    }

    /// Apply all request interceptors to an embedding request
    async fn apply_request_interceptors(
        &self,
        mut request: EmbeddingRequest,
    ) -> Result<EmbeddingRequest> {
        for interceptor in &self.request_interceptors {
            request = interceptor.intercept_embedding_request(request).await?;
        }
        Ok(request)
    }

    /// Apply all response interceptors to an embedding response
    async fn apply_response_interceptors(
        &self,
        mut response: EmbeddingResponse,
    ) -> Result<EmbeddingResponse> {
        for interceptor in &self.response_interceptors {
            response = interceptor.intercept_embedding_response(response).await?;
        }
        Ok(response)
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for EmbeddingInterceptorProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let request = self.apply_request_interceptors(request).await?;
        let response = self.provider.embed(request).await?;
        self.apply_response_interceptors(response).await
    }
}

// ===== Built-in Interceptors =====

/// Interceptor that logs requests
pub struct LoggingInterceptor {
    prefix: String,
}

impl LoggingInterceptor {
    /// Create a new logging interceptor with the given prefix
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }
}

#[async_trait]
impl RequestInterceptor for LoggingInterceptor {
    async fn intercept_request(&self, request: LlmRequest) -> Result<LlmRequest> {
        tracing::info!(
            "{} Request: prompt_len={}, temp={:?}, max_tokens={:?}",
            self.prefix,
            request.prompt.len(),
            request.temperature,
            request.max_tokens
        );
        Ok(request)
    }
}

#[async_trait]
impl ResponseInterceptor for LoggingInterceptor {
    async fn intercept_response(&self, response: LlmResponse) -> Result<LlmResponse> {
        tracing::info!(
            "{} Response: content_len={}, model={}, tokens={:?}",
            self.prefix,
            response.content.len(),
            response.model,
            response.usage.as_ref().map(|u| u.total_tokens)
        );
        Ok(response)
    }
}

/// Interceptor that sanitizes prompts by removing sensitive patterns
pub struct SanitizationInterceptor {
    patterns: Vec<String>,
}

impl SanitizationInterceptor {
    /// Create a new sanitization interceptor with patterns to remove
    pub fn new(patterns: Vec<String>) -> Self {
        Self { patterns }
    }
}

#[async_trait]
impl RequestInterceptor for SanitizationInterceptor {
    async fn intercept_request(&self, mut request: LlmRequest) -> Result<LlmRequest> {
        for pattern in &self.patterns {
            request.prompt = request.prompt.replace(pattern, "[REDACTED]");
            if let Some(ref mut system) = request.system_prompt {
                *system = system.replace(pattern, "[REDACTED]");
            }
        }
        Ok(request)
    }
}

/// Interceptor that enforces maximum content length
pub struct ContentLengthInterceptor {
    max_prompt_length: usize,
    max_response_length: usize,
}

impl ContentLengthInterceptor {
    /// Create a new content length interceptor
    pub fn new(max_prompt_length: usize, max_response_length: usize) -> Self {
        Self {
            max_prompt_length,
            max_response_length,
        }
    }
}

#[async_trait]
impl RequestInterceptor for ContentLengthInterceptor {
    async fn intercept_request(&self, mut request: LlmRequest) -> Result<LlmRequest> {
        if request.prompt.len() > self.max_prompt_length {
            request.prompt.truncate(self.max_prompt_length);
            request.prompt.push_str("...[truncated]");
        }
        Ok(request)
    }
}

#[async_trait]
impl ResponseInterceptor for ContentLengthInterceptor {
    async fn intercept_response(&self, mut response: LlmResponse) -> Result<LlmResponse> {
        if response.content.len() > self.max_response_length {
            response.content.truncate(self.max_response_length);
            response.content.push_str("...[truncated]");
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmError, Usage};

    // Mock provider for testing
    struct MockProvider;

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: format!("Response to: {}", request.prompt),
                model: "mock-model".to_string(),
                usage: Some(Usage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                }),
                tool_calls: Vec::new(),
            })
        }
    }

    // Test interceptor that adds a prefix
    struct PrefixInterceptor {
        prefix: String,
    }

    #[async_trait]
    impl RequestInterceptor for PrefixInterceptor {
        async fn intercept_request(&self, mut request: LlmRequest) -> Result<LlmRequest> {
            request.prompt = format!("{}{}", self.prefix, request.prompt);
            Ok(request)
        }
    }

    // Test interceptor that adds a suffix
    struct SuffixInterceptor {
        suffix: String,
    }

    #[async_trait]
    impl ResponseInterceptor for SuffixInterceptor {
        async fn intercept_response(&self, mut response: LlmResponse) -> Result<LlmResponse> {
            response.content.push_str(&self.suffix);
            Ok(response)
        }
    }

    // Test interceptor that fails
    struct FailingInterceptor;

    #[async_trait]
    impl RequestInterceptor for FailingInterceptor {
        async fn intercept_request(&self, _request: LlmRequest) -> Result<LlmRequest> {
            Err(LlmError::InvalidRequest("Test error".to_string()))
        }
    }

    #[tokio::test]
    async fn test_request_interceptor() {
        let provider = MockProvider;
        let provider = InterceptorProvider::new(provider).with_request_interceptor(Box::new(
            PrefixInterceptor {
                prefix: "PREFIX: ".to_string(),
            },
        ));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let response = provider.complete(request).await.unwrap();
        assert_eq!(response.content, "Response to: PREFIX: test");
    }

    #[tokio::test]
    async fn test_response_interceptor() {
        let provider = MockProvider;
        let provider = InterceptorProvider::new(provider).with_response_interceptor(Box::new(
            SuffixInterceptor {
                suffix: " SUFFIX".to_string(),
            },
        ));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let response = provider.complete(request).await.unwrap();
        assert!(response.content.ends_with(" SUFFIX"));
    }

    #[tokio::test]
    async fn test_multiple_interceptors() {
        let provider = MockProvider;
        let provider = InterceptorProvider::new(provider)
            .with_request_interceptor(Box::new(PrefixInterceptor {
                prefix: "A: ".to_string(),
            }))
            .with_request_interceptor(Box::new(PrefixInterceptor {
                prefix: "B: ".to_string(),
            }))
            .with_response_interceptor(Box::new(SuffixInterceptor {
                suffix: " X".to_string(),
            }))
            .with_response_interceptor(Box::new(SuffixInterceptor {
                suffix: " Y".to_string(),
            }));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let response = provider.complete(request).await.unwrap();
        assert_eq!(response.content, "Response to: B: A: test X Y");
    }

    #[tokio::test]
    async fn test_failing_interceptor() {
        let provider = MockProvider;
        let provider = InterceptorProvider::new(provider)
            .with_request_interceptor(Box::new(FailingInterceptor));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let result = provider.complete(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sanitization_interceptor() {
        let provider = MockProvider;
        let provider = InterceptorProvider::new(provider).with_request_interceptor(Box::new(
            SanitizationInterceptor::new(vec!["secret".to_string(), "password".to_string()]),
        ));

        let request = LlmRequest {
            prompt: "My secret is password123".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let response = provider.complete(request).await.unwrap();
        assert_eq!(
            response.content,
            "Response to: My [REDACTED] is [REDACTED]123"
        );
    }

    #[tokio::test]
    async fn test_content_length_interceptor() {
        let provider = MockProvider;
        let provider = InterceptorProvider::new(provider)
            .with_request_interceptor(Box::new(ContentLengthInterceptor::new(10, 100)))
            .with_response_interceptor(Box::new(ContentLengthInterceptor::new(100, 20)));

        let request = LlmRequest {
            prompt: "This is a very long prompt that should be truncated".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let response = provider.complete(request).await.unwrap();
        // Response should be truncated
        assert!(response.content.ends_with("...[truncated]"));
        assert!(response.content.len() <= 20 + "...[truncated]".len());
    }

    #[tokio::test]
    async fn test_logging_interceptor() {
        let provider = MockProvider;
        let provider = InterceptorProvider::new(provider)
            .with_request_interceptor(Box::new(LoggingInterceptor::new("TEST".to_string())))
            .with_response_interceptor(Box::new(LoggingInterceptor::new("TEST".to_string())));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: Vec::new(),
            images: Vec::new(),
        };

        // Should not fail even with logging
        let response = provider.complete(request).await.unwrap();
        assert!(response.content.contains("Response to: test"));
    }
}
