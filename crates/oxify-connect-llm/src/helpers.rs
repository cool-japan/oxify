//! Helper utilities for common LLM operations
//!
//! This module provides convenient helper functions and utilities to simplify
//! common LLM-related tasks.

use crate::{ImageInput, ImageSourceType, LlmRequest, Tool};

/// Builder for constructing LLM requests easily
pub struct LlmRequestBuilder {
    request: LlmRequest,
}

impl Default for LlmRequestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmRequestBuilder {
    /// Create a new request builder
    pub fn new() -> Self {
        Self {
            request: LlmRequest {
                prompt: String::new(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            },
        }
    }

    /// Set the prompt
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.request.prompt = prompt.into();
        self
    }

    /// Set the system prompt
    pub fn system(mut self, system_prompt: impl Into<String>) -> Self {
        self.request.system_prompt = Some(system_prompt.into());
        self
    }

    /// Set the temperature
    pub fn temperature(mut self, temperature: f64) -> Self {
        self.request.temperature = Some(temperature);
        self
    }

    /// Set max tokens
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.request.max_tokens = Some(max_tokens);
        self
    }

    /// Add a tool/function
    pub fn tool(mut self, tool: Tool) -> Self {
        self.request.tools.push(tool);
        self
    }

    /// Add multiple tools
    pub fn tools(mut self, tools: Vec<Tool>) -> Self {
        self.request.tools.extend(tools);
        self
    }

    /// Add an image from URL
    pub fn image_url(mut self, url: impl Into<String>) -> Self {
        self.request.images.push(ImageInput {
            data: url.into(),
            source_type: ImageSourceType::Url,
            media_type: None,
        });
        self
    }

    /// Add an image from base64 data
    pub fn image_base64(mut self, data: impl Into<String>, media_type: impl Into<String>) -> Self {
        self.request.images.push(ImageInput {
            data: data.into(),
            source_type: ImageSourceType::Base64,
            media_type: Some(media_type.into()),
        });
        self
    }

    /// Build the request
    pub fn build(self) -> LlmRequest {
        self.request
    }
}

/// Quick helpers for common request patterns
pub struct QuickRequest;

impl QuickRequest {
    /// Create a simple text generation request
    pub fn simple(prompt: impl Into<String>) -> LlmRequest {
        LlmRequestBuilder::new().prompt(prompt).build()
    }

    /// Create a request with system prompt and temperature
    pub fn chat(
        prompt: impl Into<String>,
        system: impl Into<String>,
        temperature: f64,
    ) -> LlmRequest {
        LlmRequestBuilder::new()
            .prompt(prompt)
            .system(system)
            .temperature(temperature)
            .build()
    }

    /// Create a code generation request (low temperature)
    pub fn code(prompt: impl Into<String>) -> LlmRequest {
        LlmRequestBuilder::new()
            .prompt(prompt)
            .system("You are an expert programmer. Generate clean, efficient, and well-documented code.")
            .temperature(0.2)
            .build()
    }

    /// Create a creative writing request (high temperature)
    pub fn creative(prompt: impl Into<String>) -> LlmRequest {
        LlmRequestBuilder::new()
            .prompt(prompt)
            .system("You are a creative writer. Generate engaging and imaginative content.")
            .temperature(0.9)
            .build()
    }

    /// Create a summarization request
    pub fn summarize(text: impl Into<String>) -> LlmRequest {
        LlmRequestBuilder::new()
            .prompt(format!("Summarize the following text:\n\n{}", text.into()))
            .system("You are a helpful assistant that creates concise summaries.")
            .temperature(0.3)
            .max_tokens(500)
            .build()
    }

    /// Create a translation request
    pub fn translate(text: impl Into<String>, target_lang: impl Into<String>) -> LlmRequest {
        LlmRequestBuilder::new()
            .prompt(format!(
                "Translate the following text to {}:\n\n{}",
                target_lang.into(),
                text.into()
            ))
            .temperature(0.3)
            .build()
    }

    /// Create a vision request (image analysis)
    pub fn analyze_image(image_url: impl Into<String>, question: impl Into<String>) -> LlmRequest {
        LlmRequestBuilder::new()
            .prompt(question)
            .image_url(image_url)
            .build()
    }
}

/// Token estimation utilities
pub struct TokenUtils;

impl TokenUtils {
    /// Estimate tokens for text (simple heuristic: ~4 chars/token)
    pub fn estimate_tokens(text: &str) -> u32 {
        ((text.len() as f64) / 4.0).ceil() as u32
    }

    /// Estimate cost for a request given pricing
    pub fn estimate_cost(
        prompt: &str,
        estimated_completion_tokens: u32,
        cost_per_1k_input: f64,
        cost_per_1k_output: f64,
    ) -> f64 {
        let prompt_tokens = Self::estimate_tokens(prompt);
        let input_cost = (prompt_tokens as f64 / 1000.0) * cost_per_1k_input;
        let output_cost = (estimated_completion_tokens as f64 / 1000.0) * cost_per_1k_output;
        input_cost + output_cost
    }

    /// Check if text is likely to exceed token limit
    pub fn exceeds_limit(text: &str, limit: u32) -> bool {
        Self::estimate_tokens(text) > limit
    }

    /// Truncate text to fit within token limit (rough approximation)
    pub fn truncate_to_limit(text: &str, limit: u32) -> String {
        let chars_limit = (limit as usize) * 4; // Rough estimate
        if text.len() <= chars_limit {
            text.to_string()
        } else {
            format!("{}...", &text[..chars_limit - 3])
        }
    }
}

/// Model name utilities
pub struct ModelUtils;

impl ModelUtils {
    /// Check if a model name is a GPT model
    pub fn is_gpt(model: &str) -> bool {
        model.starts_with("gpt-") || model.starts_with("o1-")
    }

    /// Check if a model name is a Claude model
    pub fn is_claude(model: &str) -> bool {
        model.starts_with("claude-")
    }

    /// Check if a model name is a Gemini model
    pub fn is_gemini(model: &str) -> bool {
        model.starts_with("gemini-")
    }

    /// Check if a model name is a local model
    pub fn is_local(model: &str) -> bool {
        // Common local model patterns
        model.contains("llama")
            || model.contains("mistral")
            || model.contains("mixtral")
            || model.contains("vicuna")
            || model.contains("alpaca")
    }

    /// Get the provider from model name
    pub fn infer_provider(model: &str) -> Option<&str> {
        if Self::is_gpt(model) {
            Some("openai")
        } else if Self::is_claude(model) {
            Some("anthropic")
        } else if Self::is_gemini(model) {
            Some("google")
        } else if model.starts_with("command") {
            Some("cohere")
        } else if Self::is_local(model) {
            Some("ollama")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder() {
        let request = LlmRequestBuilder::new()
            .prompt("Hello")
            .system("You are helpful")
            .temperature(0.7)
            .max_tokens(100)
            .build();

        assert_eq!(request.prompt, "Hello");
        assert_eq!(request.system_prompt, Some("You are helpful".to_string()));
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(100));
    }

    #[test]
    fn test_quick_request_simple() {
        let request = QuickRequest::simple("Test prompt");
        assert_eq!(request.prompt, "Test prompt");
        assert!(request.system_prompt.is_none());
    }

    #[test]
    fn test_quick_request_chat() {
        let request = QuickRequest::chat("Hello", "You are helpful", 0.8);
        assert_eq!(request.prompt, "Hello");
        assert_eq!(request.system_prompt, Some("You are helpful".to_string()));
        assert_eq!(request.temperature, Some(0.8));
    }

    #[test]
    fn test_quick_request_code() {
        let request = QuickRequest::code("Write a function");
        assert!(request.system_prompt.is_some());
        assert_eq!(request.temperature, Some(0.2));
    }

    #[test]
    fn test_quick_request_creative() {
        let request = QuickRequest::creative("Write a story");
        assert_eq!(request.temperature, Some(0.9));
    }

    #[test]
    fn test_quick_request_summarize() {
        let request = QuickRequest::summarize("Long text here");
        assert!(request.prompt.contains("Summarize"));
        assert_eq!(request.max_tokens, Some(500));
    }

    #[test]
    fn test_token_utils_estimate() {
        let tokens = TokenUtils::estimate_tokens("Hello, world!");
        assert!(tokens > 0);
    }

    #[test]
    fn test_token_utils_estimate_cost() {
        let cost = TokenUtils::estimate_cost("Hello", 100, 0.5, 1.5);
        assert!(cost > 0.0);
    }

    #[test]
    fn test_token_utils_exceeds_limit() {
        let text = "a".repeat(10000);
        assert!(TokenUtils::exceeds_limit(&text, 100));
        assert!(!TokenUtils::exceeds_limit("short", 1000));
    }

    #[test]
    fn test_token_utils_truncate() {
        let text = "a".repeat(1000);
        let truncated = TokenUtils::truncate_to_limit(&text, 10);
        assert!(truncated.len() < text.len());
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_model_utils_is_gpt() {
        assert!(ModelUtils::is_gpt("gpt-4"));
        assert!(ModelUtils::is_gpt("gpt-3.5-turbo"));
        assert!(ModelUtils::is_gpt("o1-preview"));
        assert!(!ModelUtils::is_gpt("claude-3"));
    }

    #[test]
    fn test_model_utils_is_claude() {
        assert!(ModelUtils::is_claude("claude-3-opus"));
        assert!(!ModelUtils::is_claude("gpt-4"));
    }

    #[test]
    fn test_model_utils_is_gemini() {
        assert!(ModelUtils::is_gemini("gemini-pro"));
        assert!(!ModelUtils::is_gemini("gpt-4"));
    }

    #[test]
    fn test_model_utils_infer_provider() {
        assert_eq!(ModelUtils::infer_provider("gpt-4"), Some("openai"));
        assert_eq!(
            ModelUtils::infer_provider("claude-3-opus"),
            Some("anthropic")
        );
        assert_eq!(ModelUtils::infer_provider("gemini-pro"), Some("google"));
        assert_eq!(ModelUtils::infer_provider("command-r"), Some("cohere"));
    }

    #[test]
    fn test_request_builder_with_images() {
        let request = LlmRequestBuilder::new()
            .prompt("Analyze this image")
            .image_url("https://example.com/image.jpg")
            .build();

        assert_eq!(request.images.len(), 1);
        assert_eq!(request.images[0].source_type, ImageSourceType::Url);
    }

    #[test]
    fn test_request_builder_with_tools() {
        let tool = Tool {
            name: "test".to_string(),
            description: "test tool".to_string(),
            parameters: serde_json::json!({}),
        };

        let request = LlmRequestBuilder::new().prompt("Test").tool(tool).build();

        assert_eq!(request.tools.len(), 1);
    }
}
