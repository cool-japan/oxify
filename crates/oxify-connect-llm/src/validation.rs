//! Request validation utilities
//!
//! This module provides validation utilities to catch errors before making API calls,
//! helping to save costs and improve user experience.

use crate::{EmbeddingRequest, LlmError, LlmRequest, Result};

/// Validation rules for LLM requests
#[derive(Debug, Clone)]
pub struct ValidationRules {
    /// Maximum prompt length in characters
    pub max_prompt_length: Option<usize>,
    /// Minimum prompt length in characters
    pub min_prompt_length: usize,
    /// Maximum number of tokens
    pub max_tokens_limit: Option<u32>,
    /// Require non-empty prompt
    pub require_prompt: bool,
    /// Maximum temperature value
    pub max_temperature: f64,
    /// Minimum temperature value
    pub min_temperature: f64,
    /// Maximum number of images
    pub max_images: usize,
    /// Maximum number of tools
    pub max_tools: usize,
}

impl Default for ValidationRules {
    fn default() -> Self {
        Self {
            max_prompt_length: Some(1_000_000), // 1M chars
            min_prompt_length: 1,
            max_tokens_limit: Some(200_000), // 200K tokens
            require_prompt: true,
            max_temperature: 2.0,
            min_temperature: 0.0,
            max_images: 20,
            max_tools: 100,
        }
    }
}

impl ValidationRules {
    /// Create strict validation rules
    pub fn strict() -> Self {
        Self {
            max_prompt_length: Some(100_000), // 100K chars
            min_prompt_length: 1,
            max_tokens_limit: Some(100_000), // 100K tokens
            require_prompt: true,
            max_temperature: 1.5,
            min_temperature: 0.0,
            max_images: 10,
            max_tools: 50,
        }
    }

    /// Create lenient validation rules
    pub fn lenient() -> Self {
        Self {
            max_prompt_length: None,
            min_prompt_length: 0,
            max_tokens_limit: None,
            require_prompt: false,
            max_temperature: 2.0,
            min_temperature: 0.0,
            max_images: 100,
            max_tools: 200,
        }
    }

    /// Validate an LLM request
    pub fn validate_llm_request(&self, request: &LlmRequest) -> Result<()> {
        // Validate prompt
        if self.require_prompt && request.prompt.trim().is_empty() {
            return Err(LlmError::InvalidRequest(
                "Prompt cannot be empty".to_string(),
            ));
        }

        if request.prompt.len() < self.min_prompt_length {
            return Err(LlmError::InvalidRequest(format!(
                "Prompt too short: {} chars (minimum: {})",
                request.prompt.len(),
                self.min_prompt_length
            )));
        }

        if let Some(max_len) = self.max_prompt_length {
            if request.prompt.len() > max_len {
                return Err(LlmError::InvalidRequest(format!(
                    "Prompt too long: {} chars (maximum: {})",
                    request.prompt.len(),
                    max_len
                )));
            }
        }

        // Validate temperature
        if let Some(temp) = request.temperature {
            if temp < self.min_temperature || temp > self.max_temperature {
                return Err(LlmError::InvalidRequest(format!(
                    "Temperature out of range: {} (must be between {} and {})",
                    temp, self.min_temperature, self.max_temperature
                )));
            }
        }

        // Validate max_tokens
        if let Some(max_tokens) = request.max_tokens {
            if max_tokens == 0 {
                return Err(LlmError::InvalidRequest(
                    "max_tokens must be greater than 0".to_string(),
                ));
            }

            if let Some(limit) = self.max_tokens_limit {
                if max_tokens > limit {
                    return Err(LlmError::InvalidRequest(format!(
                        "max_tokens too large: {} (maximum: {})",
                        max_tokens, limit
                    )));
                }
            }
        }

        // Validate images
        if request.images.len() > self.max_images {
            return Err(LlmError::InvalidRequest(format!(
                "Too many images: {} (maximum: {})",
                request.images.len(),
                self.max_images
            )));
        }

        // Validate tools
        if request.tools.len() > self.max_tools {
            return Err(LlmError::InvalidRequest(format!(
                "Too many tools: {} (maximum: {})",
                request.tools.len(),
                self.max_tools
            )));
        }

        // Validate tool definitions
        for tool in &request.tools {
            if tool.name.trim().is_empty() {
                return Err(LlmError::InvalidRequest(
                    "Tool name cannot be empty".to_string(),
                ));
            }
            if tool.description.trim().is_empty() {
                return Err(LlmError::InvalidRequest(format!(
                    "Tool '{}' must have a description",
                    tool.name
                )));
            }
        }

        Ok(())
    }

    /// Validate an embedding request
    pub fn validate_embedding_request(&self, request: &EmbeddingRequest) -> Result<()> {
        if request.texts.is_empty() {
            return Err(LlmError::InvalidRequest(
                "Embedding request must contain at least one text".to_string(),
            ));
        }

        for (i, text) in request.texts.iter().enumerate() {
            if text.trim().is_empty() {
                return Err(LlmError::InvalidRequest(format!(
                    "Text at index {} cannot be empty",
                    i
                )));
            }

            if let Some(max_len) = self.max_prompt_length {
                if text.len() > max_len {
                    return Err(LlmError::InvalidRequest(format!(
                        "Text at index {} too long: {} chars (maximum: {})",
                        i,
                        text.len(),
                        max_len
                    )));
                }
            }
        }

        Ok(())
    }
}

/// Validates LLM requests before sending them to providers
pub struct RequestValidator {
    rules: ValidationRules,
}

impl Default for RequestValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestValidator {
    /// Create a new validator with default rules
    pub fn new() -> Self {
        Self {
            rules: ValidationRules::default(),
        }
    }

    /// Create a validator with custom rules
    pub fn with_rules(rules: ValidationRules) -> Self {
        Self { rules }
    }

    /// Validate an LLM request
    pub fn validate(&self, request: &LlmRequest) -> Result<()> {
        self.rules.validate_llm_request(request)
    }

    /// Validate an embedding request
    pub fn validate_embedding(&self, request: &EmbeddingRequest) -> Result<()> {
        self.rules.validate_embedding_request(request)
    }

    /// Get a reference to the validation rules
    pub fn rules(&self) -> &ValidationRules {
        &self.rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;

    #[test]
    fn test_validate_valid_request() {
        let validator = RequestValidator::new();
        let request = LlmRequest {
            prompt: "Hello, world!".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: vec![],
            images: vec![],
        };

        assert!(validator.validate(&request).is_ok());
    }

    #[test]
    fn test_validate_empty_prompt() {
        let validator = RequestValidator::new();
        let request = LlmRequest {
            prompt: "".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        let result = validator.validate(&request);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::InvalidRequest(_)));
    }

    #[test]
    fn test_validate_temperature_out_of_range() {
        let validator = RequestValidator::new();
        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: Some(3.0),
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        let result = validator.validate(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_zero_max_tokens() {
        let validator = RequestValidator::new();
        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: Some(0),
            tools: vec![],
            images: vec![],
        };

        let result = validator.validate(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_too_many_tools() {
        let validator = RequestValidator::with_rules(ValidationRules {
            max_tools: 2,
            ..ValidationRules::default()
        });

        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![
                Tool {
                    name: "tool1".to_string(),
                    description: "desc1".to_string(),
                    parameters: serde_json::json!({}),
                },
                Tool {
                    name: "tool2".to_string(),
                    description: "desc2".to_string(),
                    parameters: serde_json::json!({}),
                },
                Tool {
                    name: "tool3".to_string(),
                    description: "desc3".to_string(),
                    parameters: serde_json::json!({}),
                },
            ],
            images: vec![],
        };

        let result = validator.validate(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_tool_without_name() {
        let validator = RequestValidator::new();
        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![Tool {
                name: "".to_string(),
                description: "description".to_string(),
                parameters: serde_json::json!({}),
            }],
            images: vec![],
        };

        let result = validator.validate(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_embedding_request() {
        let validator = RequestValidator::new();
        let request = EmbeddingRequest {
            texts: vec!["Hello".to_string(), "World".to_string()],
            model: None,
        };

        assert!(validator.validate_embedding(&request).is_ok());
    }

    #[test]
    fn test_validate_empty_embedding_request() {
        let validator = RequestValidator::new();
        let request = EmbeddingRequest {
            texts: vec![],
            model: None,
        };

        let result = validator.validate_embedding(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_rules_strict() {
        let rules = ValidationRules::strict();
        assert!(rules.max_prompt_length.is_some());
        assert_eq!(rules.max_prompt_length.unwrap(), 100_000);
    }

    #[test]
    fn test_validation_rules_lenient() {
        let rules = ValidationRules::lenient();
        assert!(rules.max_prompt_length.is_none());
        assert_eq!(rules.min_prompt_length, 0);
    }
}
