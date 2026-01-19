//! Prompt compression utilities for optimizing token usage
//!
//! This module provides tools to compress prompts, estimate token counts,
//! and warn about model limits to reduce costs and improve performance.

use std::collections::HashMap;

/// Token limits for common models
pub struct ModelLimits;

impl ModelLimits {
    /// Get the token limit for a given model
    pub fn get_limit(model: &str) -> Option<u32> {
        let limits: HashMap<&str, u32> = [
            // OpenAI models
            ("gpt-4", 8192),
            ("gpt-4-32k", 32768),
            ("gpt-4-turbo", 128000),
            ("gpt-4-turbo-preview", 128000),
            ("gpt-4o", 128000),
            ("gpt-4o-mini", 128000),
            ("o1-preview", 128000),
            ("o1-mini", 128000),
            ("gpt-3.5-turbo", 4096),
            ("gpt-3.5-turbo-16k", 16384),
            // Anthropic models
            ("claude-3-opus-20240229", 200000),
            ("claude-3-sonnet-20240229", 200000),
            ("claude-3-haiku-20240307", 200000),
            ("claude-3-5-sonnet-20241022", 200000),
            ("claude-3-5-haiku-20241022", 200000),
            ("claude-2.1", 200000),
            ("claude-2.0", 100000),
            ("claude-instant-1.2", 100000),
            // Google models
            ("gemini-pro", 32760),
            ("gemini-1.5-pro", 1048576),
            ("gemini-1.5-flash", 1048576),
            ("gemini-2.0-flash", 1048576),
            // Cohere models
            ("command", 4096),
            ("command-r", 128000),
            ("command-r-plus", 128000),
            // Mistral models
            ("mistral-large-latest", 32000),
            ("mistral-medium-latest", 32000),
            ("mistral-small-latest", 32000),
            ("open-mixtral-8x7b", 32000),
        ]
        .iter()
        .copied()
        .collect();

        limits.get(model).copied()
    }

    /// Check if a model has a known limit
    pub fn has_limit(model: &str) -> bool {
        Self::get_limit(model).is_some()
    }
}

/// Compression statistics
#[derive(Debug, Clone, Default)]
pub struct CompressionStats {
    /// Original text length in characters
    pub original_length: usize,
    /// Compressed text length in characters
    pub compressed_length: usize,
    /// Estimated original token count
    pub estimated_original_tokens: u32,
    /// Estimated compressed token count
    pub estimated_compressed_tokens: u32,
    /// Compression ratio (compressed_length / original_length)
    pub compression_ratio: f64,
    /// Token savings (estimated_original_tokens - estimated_compressed_tokens)
    pub token_savings: u32,
}

impl CompressionStats {
    /// Create new compression statistics
    pub fn new(
        original: &str,
        compressed: &str,
        original_tokens: u32,
        compressed_tokens: u32,
    ) -> Self {
        let original_length = original.len();
        let compressed_length = compressed.len();

        Self {
            original_length,
            compressed_length,
            estimated_original_tokens: original_tokens,
            estimated_compressed_tokens: compressed_tokens,
            compression_ratio: if original_length > 0 {
                compressed_length as f64 / original_length as f64
            } else {
                1.0
            },
            token_savings: original_tokens.saturating_sub(compressed_tokens),
        }
    }
}

/// Prompt compressor for optimizing token usage
pub struct PromptCompressor {
    /// Whether to remove extra whitespace
    remove_whitespace: bool,
    /// Whether to remove empty lines
    remove_empty_lines: bool,
    /// Whether to trim line endings
    trim_lines: bool,
}

impl Default for PromptCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptCompressor {
    /// Create a new prompt compressor with default settings
    pub fn new() -> Self {
        Self {
            remove_whitespace: true,
            remove_empty_lines: true,
            trim_lines: true,
        }
    }

    /// Configure whitespace removal
    pub fn with_whitespace_removal(mut self, remove: bool) -> Self {
        self.remove_whitespace = remove;
        self
    }

    /// Configure empty line removal
    pub fn with_empty_line_removal(mut self, remove: bool) -> Self {
        self.remove_empty_lines = remove;
        self
    }

    /// Configure line trimming
    pub fn with_line_trimming(mut self, trim: bool) -> Self {
        self.trim_lines = trim;
        self
    }

    /// Estimate token count for a text
    ///
    /// Uses a simple heuristic: ~4 characters per token for English text.
    /// This is approximate and may vary by language and tokenizer.
    pub fn estimate_tokens(text: &str) -> u32 {
        // Simple heuristic: average of 4 characters per token
        // This is conservative and works reasonably well for English
        ((text.len() as f64) / 4.0).ceil() as u32
    }

    /// Compress a prompt to reduce token usage
    ///
    /// # Arguments
    /// * `text` - The text to compress
    ///
    /// # Returns
    /// A tuple of (compressed_text, compression_stats)
    pub fn compress(&self, text: &str) -> (String, CompressionStats) {
        let original_tokens = Self::estimate_tokens(text);
        let mut result = text.to_string();

        // Remove extra whitespace between words
        if self.remove_whitespace {
            result = self.normalize_whitespace(&result);
        }

        // Process lines
        if self.remove_empty_lines || self.trim_lines {
            let lines: Vec<String> = result
                .lines()
                .filter_map(|line| {
                    let processed = if self.trim_lines { line.trim() } else { line };

                    if self.remove_empty_lines && processed.is_empty() {
                        None
                    } else {
                        Some(processed.to_string())
                    }
                })
                .collect();

            result = lines.join("\n");
        }

        // Final cleanup: ensure no trailing whitespace
        result = result.trim().to_string();

        let compressed_tokens = Self::estimate_tokens(&result);
        let stats = CompressionStats::new(text, &result, original_tokens, compressed_tokens);

        (result, stats)
    }

    /// Normalize whitespace by replacing multiple spaces with single space
    fn normalize_whitespace(&self, text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        let mut prev_was_space = false;

        for ch in text.chars() {
            if ch.is_whitespace() && ch != '\n' {
                if !prev_was_space {
                    result.push(' ');
                    prev_was_space = true;
                }
            } else {
                result.push(ch);
                prev_was_space = false;
            }
        }

        result
    }

    /// Check if a prompt exceeds the model's token limit
    ///
    /// # Arguments
    /// * `text` - The text to check
    /// * `model` - The model name
    ///
    /// # Returns
    /// `Some(tokens)` if the limit is exceeded, `None` otherwise
    pub fn check_limit(text: &str, model: &str) -> Option<u32> {
        let estimated_tokens = Self::estimate_tokens(text);

        if let Some(limit) = ModelLimits::get_limit(model) {
            if estimated_tokens > limit {
                return Some(estimated_tokens);
            }
        }

        None
    }

    /// Get a warning message if the prompt exceeds the model limit
    ///
    /// # Arguments
    /// * `text` - The text to check
    /// * `model` - The model name
    ///
    /// # Returns
    /// A warning message if the limit is exceeded, `None` otherwise
    pub fn get_limit_warning(text: &str, model: &str) -> Option<String> {
        if let Some(tokens) = Self::check_limit(text, model) {
            if let Some(limit) = ModelLimits::get_limit(model) {
                return Some(format!(
                    "Prompt exceeds model limit: {} tokens (limit: {} tokens for {})",
                    tokens, limit, model
                ));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimation() {
        let text = "Hello, world!";
        let tokens = PromptCompressor::estimate_tokens(text);
        // "Hello, world!" is 13 chars, so ~3.25 tokens, rounds to 4
        assert_eq!(tokens, 4);

        let long_text = "This is a longer text with multiple words and punctuation.";
        let long_tokens = PromptCompressor::estimate_tokens(long_text);
        // 59 chars / 4 = 14.75, rounds to 15
        assert_eq!(long_tokens, 15);
    }

    #[test]
    fn test_whitespace_compression() {
        let compressor = PromptCompressor::new();
        let text = "Hello    world   with    extra    spaces";
        let (compressed, stats) = compressor.compress(text);

        assert_eq!(compressed, "Hello world with extra spaces");
        assert!(stats.compressed_length < stats.original_length);
        assert!(stats.compression_ratio < 1.0);
    }

    #[test]
    fn test_empty_line_removal() {
        let compressor = PromptCompressor::new();
        let text = "Line 1\n\n\nLine 2\n\nLine 3";
        let (compressed, _) = compressor.compress(text);

        assert_eq!(compressed, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_line_trimming() {
        let compressor = PromptCompressor::new();
        let text = "  Line 1  \n  Line 2  \n  Line 3  ";
        let (compressed, _) = compressor.compress(text);

        assert_eq!(compressed, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_compression_disabled() {
        let compressor = PromptCompressor::new()
            .with_whitespace_removal(false)
            .with_empty_line_removal(false)
            .with_line_trimming(false);

        let text = "Hello    world\n\n\ntest";
        let (compressed, _) = compressor.compress(text);

        // Should only trim the final string
        assert_eq!(compressed, "Hello    world\n\n\ntest");
    }

    #[test]
    fn test_model_limits() {
        assert_eq!(ModelLimits::get_limit("gpt-4"), Some(8192));
        assert_eq!(ModelLimits::get_limit("gpt-4-32k"), Some(32768));
        assert_eq!(
            ModelLimits::get_limit("claude-3-opus-20240229"),
            Some(200000)
        );
        assert_eq!(ModelLimits::get_limit("unknown-model"), None);
    }

    #[test]
    fn test_limit_checking() {
        // Create a text that's definitely over 4096 tokens (~16384 chars)
        let text = "x".repeat(20000);
        let result = PromptCompressor::check_limit(&text, "gpt-3.5-turbo");

        assert!(result.is_some());
        assert!(result.unwrap() > 4096);
    }

    #[test]
    fn test_limit_warning() {
        let text = "x".repeat(20000);
        let warning = PromptCompressor::get_limit_warning(&text, "gpt-3.5-turbo");

        assert!(warning.is_some());
        assert!(warning.unwrap().contains("exceeds model limit"));
    }

    #[test]
    fn test_no_limit_warning() {
        let text = "Short text";
        let warning = PromptCompressor::get_limit_warning(text, "gpt-4");

        assert!(warning.is_none());
    }

    #[test]
    fn test_compression_stats() {
        let compressor = PromptCompressor::new();
        let text = "Hello    world   with    many    spaces\n\n\nand empty lines";
        let (_, stats) = compressor.compress(text);

        assert!(stats.original_length > stats.compressed_length);
        assert!(stats.estimated_original_tokens > stats.estimated_compressed_tokens);
        assert!(stats.token_savings > 0);
        assert!(stats.compression_ratio < 1.0);
        assert!(stats.compression_ratio > 0.0);
    }

    #[test]
    fn test_compression_preserves_content() {
        let compressor = PromptCompressor::new();
        let text = "Important   data:  value1,   value2,   value3";
        let (compressed, _) = compressor.compress(text);

        // Content should be preserved, just whitespace reduced
        assert!(compressed.contains("Important data: value1, value2, value3"));
    }

    #[test]
    fn test_model_limit_has_limit() {
        assert!(ModelLimits::has_limit("gpt-4"));
        assert!(ModelLimits::has_limit("claude-3-opus-20240229"));
        assert!(!ModelLimits::has_limit("unknown-model"));
    }
}
