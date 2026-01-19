//! Enhanced error types with rich context
//!
//! This module provides error utilities with detailed context to help with debugging.

use crate::LlmError;
use std::fmt;

/// Error context for better debugging
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// The operation that failed
    pub operation: String,
    /// The provider that was being used
    pub provider: Option<String>,
    /// The model that was being used
    pub model: Option<String>,
    /// Additional context
    pub details: Option<String>,
    /// Timestamp when the error occurred
    pub timestamp: Option<std::time::SystemTime>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            provider: None,
            model: None,
            details: None,
            timestamp: Some(std::time::SystemTime::now()),
        }
    }

    /// Set the provider
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    /// Set the model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set additional details
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Format the context as a string
    pub fn format(&self) -> String {
        let mut msg = format!("Operation: {}", self.operation);

        if let Some(provider) = &self.provider {
            msg.push_str(&format!(", Provider: {}", provider));
        }

        if let Some(model) = &self.model {
            msg.push_str(&format!(", Model: {}", model));
        }

        if let Some(details) = &self.details {
            msg.push_str(&format!(", Details: {}", details));
        }

        msg
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

/// Enhanced error with context
#[derive(Debug)]
pub struct ContextualError {
    /// The underlying error
    pub error: LlmError,
    /// The context
    pub context: ErrorContext,
}

impl ContextualError {
    /// Create a new contextual error
    pub fn new(error: LlmError, context: ErrorContext) -> Self {
        Self { error, context }
    }

    /// Get a detailed error message
    pub fn detailed_message(&self) -> String {
        format!("{} | Context: {}", self.error, self.context.format())
    }
}

impl fmt::Display for ContextualError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.detailed_message())
    }
}

impl std::error::Error for ContextualError {}

/// Extension trait for adding context to errors
pub trait ErrorContextExt {
    /// Add context to an error
    fn with_context(self, context: ErrorContext) -> ContextualError;
}

impl ErrorContextExt for LlmError {
    fn with_context(self, context: ErrorContext) -> ContextualError {
        ContextualError::new(self, context)
    }
}

/// Helper to create common error contexts
pub struct ErrorContextBuilder;

impl ErrorContextBuilder {
    /// Context for completion requests
    pub fn completion(provider: impl Into<String>, model: impl Into<String>) -> ErrorContext {
        ErrorContext::new("LLM Completion")
            .with_provider(provider)
            .with_model(model)
    }

    /// Context for embedding requests
    pub fn embedding(provider: impl Into<String>, model: impl Into<String>) -> ErrorContext {
        ErrorContext::new("Embedding Generation")
            .with_provider(provider)
            .with_model(model)
    }

    /// Context for streaming requests
    pub fn streaming(provider: impl Into<String>, model: impl Into<String>) -> ErrorContext {
        ErrorContext::new("Streaming Completion")
            .with_provider(provider)
            .with_model(model)
    }

    /// Context for provider initialization
    pub fn initialization(provider: impl Into<String>) -> ErrorContext {
        ErrorContext::new("Provider Initialization").with_provider(provider)
    }

    /// Context for rate limiting
    pub fn rate_limit(provider: impl Into<String>) -> ErrorContext {
        ErrorContext::new("Rate Limit Check").with_provider(provider)
    }

    /// Context for caching
    pub fn cache(operation: impl Into<String>) -> ErrorContext {
        ErrorContext::new(format!("Cache {}", operation.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_creation() {
        let context = ErrorContext::new("test operation")
            .with_provider("openai")
            .with_model("gpt-4")
            .with_details("test details");

        assert_eq!(context.operation, "test operation");
        assert_eq!(context.provider, Some("openai".to_string()));
        assert_eq!(context.model, Some("gpt-4".to_string()));
        assert_eq!(context.details, Some("test details".to_string()));
    }

    #[test]
    fn test_error_context_format() {
        let context = ErrorContext::new("test")
            .with_provider("anthropic")
            .with_model("claude-3");

        let formatted = context.format();
        assert!(formatted.contains("test"));
        assert!(formatted.contains("anthropic"));
        assert!(formatted.contains("claude-3"));
    }

    #[test]
    fn test_contextual_error() {
        let error = LlmError::ApiError("test error".to_string());
        let context = ErrorContext::new("test").with_provider("openai");
        let contextual = error.with_context(context);

        let message = contextual.detailed_message();
        assert!(message.contains("test error"));
        assert!(message.contains("openai"));
    }

    #[test]
    fn test_error_context_builder_completion() {
        let context = ErrorContextBuilder::completion("openai", "gpt-4");
        assert_eq!(context.operation, "LLM Completion");
        assert_eq!(context.provider, Some("openai".to_string()));
        assert_eq!(context.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_error_context_builder_embedding() {
        let context = ErrorContextBuilder::embedding("openai", "text-embedding-ada-002");
        assert_eq!(context.operation, "Embedding Generation");
    }

    #[test]
    fn test_error_context_builder_streaming() {
        let context = ErrorContextBuilder::streaming("anthropic", "claude-3");
        assert_eq!(context.operation, "Streaming Completion");
    }
}
