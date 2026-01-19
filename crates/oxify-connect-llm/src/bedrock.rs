//! AWS Bedrock LLM provider
//!
//! This provider supports Claude models on AWS Bedrock.
//! Note: Full implementation requires AWS credentials and SigV4 signing.

use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse, Result, ToolCall, Usage};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// AWS Bedrock provider for Claude models
///
/// # Configuration
/// This provider requires AWS credentials to be configured:
/// - AWS_ACCESS_KEY_ID environment variable
/// - AWS_SECRET_ACCESS_KEY environment variable
/// - AWS_SESSION_TOKEN environment variable (optional)
///
/// # Example
/// ```no_run
/// use oxify_connect_llm::BedrockProvider;
///
/// let provider = BedrockProvider::new(
///     "us-east-1".to_string(),
///     "anthropic.claude-3-sonnet-20240229-v1:0".to_string()
/// );
/// ```
pub struct BedrockProvider {
    region: String,
    model_id: String,
    client: reqwest::Client,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
}

#[derive(Serialize)]
struct BedrockRequest {
    #[serde(rename = "anthropic_version")]
    anthropic_version: String,
    max_tokens: u32,
    messages: Vec<BedrockMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<BedrockTool>,
}

#[derive(Serialize)]
struct BedrockTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct BedrockMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct BedrockResponse {
    content: Vec<BedrockContentBlock>,
    usage: BedrockUsage,
    #[serde(default)]
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum BedrockContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Deserialize)]
struct BedrockUsage {
    input_tokens: u32,
    output_tokens: u32,
}

impl BedrockProvider {
    /// Create a new Bedrock provider
    ///
    /// # Arguments
    /// * `region` - AWS region (e.g., "us-east-1")
    /// * `model_id` - Bedrock model ID (e.g., "anthropic.claude-3-sonnet-20240229-v1:0")
    pub fn new(region: String, model_id: String) -> Self {
        Self {
            region,
            model_id,
            client: reqwest::Client::new(),
            access_key_id: None,
            secret_access_key: None,
        }
    }

    /// Create a new Bedrock provider with explicit credentials
    pub fn with_credentials(
        region: String,
        model_id: String,
        access_key_id: String,
        secret_access_key: String,
    ) -> Self {
        Self {
            region,
            model_id,
            client: reqwest::Client::new(),
            access_key_id: Some(access_key_id),
            secret_access_key: Some(secret_access_key),
        }
    }

    /// Create a new Bedrock provider from environment variables
    pub fn from_env(region: String, model_id: String) -> Result<Self> {
        let access_key_id = std::env::var("AWS_ACCESS_KEY_ID").ok();
        let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY").ok();

        if access_key_id.is_none() || secret_access_key.is_none() {
            return Err(LlmError::ConfigError(
                "AWS credentials not found in environment variables. \
                 Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY."
                    .to_string(),
            ));
        }

        Ok(Self {
            region,
            model_id,
            client: reqwest::Client::new(),
            access_key_id,
            secret_access_key,
        })
    }

    /// Get the Bedrock endpoint URL
    fn endpoint_url(&self) -> String {
        format!(
            "https://bedrock-runtime.{}.amazonaws.com/model/{}/invoke",
            self.region, self.model_id
        )
    }
}

#[async_trait]
impl LlmProvider for BedrockProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Check if credentials are available
        let _access_key_id = self
            .access_key_id
            .clone()
            .or_else(|| std::env::var("AWS_ACCESS_KEY_ID").ok())
            .ok_or_else(|| {
                LlmError::ConfigError(
                    "AWS_ACCESS_KEY_ID not found. Use with_credentials() or set environment variable."
                        .to_string(),
                )
            })?;

        let _secret_access_key = self
            .secret_access_key
            .clone()
            .or_else(|| std::env::var("AWS_SECRET_ACCESS_KEY").ok())
            .ok_or_else(|| {
                LlmError::ConfigError(
                    "AWS_SECRET_ACCESS_KEY not found. Use with_credentials() or set environment variable."
                        .to_string(),
                )
            })?;

        // Convert tools to Bedrock format
        let tools: Vec<BedrockTool> = request
            .tools
            .iter()
            .map(|t| BedrockTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect();

        let bedrock_request = BedrockRequest {
            anthropic_version: "bedrock-2023-05-31".to_string(),
            max_tokens: request.max_tokens.unwrap_or(4096),
            messages: vec![BedrockMessage {
                role: "user".to_string(),
                content: request.prompt.clone(),
            }],
            temperature: request.temperature,
            system: request.system_prompt,
            tools,
        };

        let body = serde_json::to_string(&bedrock_request)
            .map_err(|e| LlmError::SerializationError(e.to_string()))?;

        // Note: This is a simplified implementation without full AWS SigV4 signing
        // For production use, consider using the AWS SDK for Rust
        tracing::warn!(
            "AWS Bedrock provider requires proper AWS SigV4 signing for production use. \
             Consider using the AWS SDK for Rust (aws-sdk-bedrockruntime)."
        );

        // Make request (this will fail without proper AWS SigV4 signing)
        let response = self
            .client
            .post(self.endpoint_url())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            // Note: AWS SigV4 signature should be added here
            .body(body)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let error_body = response.text().await?;
            return Err(LlmError::ApiError(format!(
                "AWS Bedrock error (HTTP {}): {}. \
                 Note: This implementation requires proper AWS SigV4 signing. \
                 Use AWS SDK for production: aws-sdk-bedrockruntime",
                status, error_body
            )));
        }

        let response_body = response.text().await?;
        let bedrock_response: BedrockResponse = serde_json::from_str(&response_body)
            .map_err(|e| LlmError::SerializationError(e.to_string()))?;

        // Extract text content and tool calls
        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        for block in bedrock_response.content {
            match block {
                BedrockContentBlock::Text { text } => {
                    if !text_content.is_empty() {
                        text_content.push('\n');
                    }
                    text_content.push_str(&text);
                }
                BedrockContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
            }
        }

        Ok(LlmResponse {
            content: text_content,
            model: self.model_id.clone(),
            usage: Some(Usage {
                prompt_tokens: bedrock_response.usage.input_tokens,
                completion_tokens: bedrock_response.usage.output_tokens,
                total_tokens: bedrock_response.usage.input_tokens
                    + bedrock_response.usage.output_tokens,
            }),
            tool_calls,
        })
    }
}

/// Supported Bedrock Claude models
pub mod models {
    /// Claude 3 Opus on Bedrock
    #[allow(dead_code)]
    pub const CLAUDE_3_OPUS: &str = "anthropic.claude-3-opus-20240229-v1:0";

    /// Claude 3 Sonnet on Bedrock
    #[allow(dead_code)]
    pub const CLAUDE_3_SONNET: &str = "anthropic.claude-3-sonnet-20240229-v1:0";

    /// Claude 3 Haiku on Bedrock
    #[allow(dead_code)]
    pub const CLAUDE_3_HAIKU: &str = "anthropic.claude-3-haiku-20240307-v1:0";

    /// Claude 2.1 on Bedrock
    #[allow(dead_code)]
    pub const CLAUDE_2_1: &str = "anthropic.claude-v2:1";

    /// Claude 2.0 on Bedrock
    #[allow(dead_code)]
    pub const CLAUDE_2_0: &str = "anthropic.claude-v2";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bedrock_provider_creation() {
        let provider = BedrockProvider::new("us-east-1".to_string(), "test-model".to_string());
        assert_eq!(provider.region, "us-east-1");
        assert_eq!(provider.model_id, "test-model");
    }

    #[test]
    fn test_bedrock_endpoint_url() {
        let provider = BedrockProvider::new("us-east-1".to_string(), "test-model".to_string());
        let url = provider.endpoint_url();
        assert!(url.contains("us-east-1"));
        assert!(url.contains("test-model"));
        assert!(url.contains("bedrock-runtime"));
    }

    #[test]
    fn test_bedrock_with_credentials() {
        let provider = BedrockProvider::with_credentials(
            "us-west-2".to_string(),
            "model-id".to_string(),
            "access-key".to_string(),
            "secret-key".to_string(),
        );
        assert_eq!(provider.access_key_id, Some("access-key".to_string()));
        assert_eq!(provider.secret_access_key, Some("secret-key".to_string()));
    }

    #[test]
    fn test_model_constants() {
        assert!(models::CLAUDE_3_OPUS.contains("opus"));
        assert!(models::CLAUDE_3_SONNET.contains("sonnet"));
        assert!(models::CLAUDE_3_HAIKU.contains("haiku"));
    }
}
