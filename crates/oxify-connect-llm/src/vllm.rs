//! vLLM integration for high-throughput LLM inference
//!
//! vLLM is a fast and easy-to-use library for LLM inference and serving,
//! optimized for high throughput with PagedAttention and continuous batching.

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmChunk, LlmError, LlmProvider,
    LlmRequest, LlmResponse, LlmStream, Result, StreamingLlmProvider,
};
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// vLLM provider for high-throughput LLM inference
///
/// vLLM provides OpenAI-compatible APIs with optimized inference performance.
///
/// # Example
/// ```no_run
/// use oxify_connect_llm::{VllmProvider, LlmProvider, LlmRequest};
///
/// #[tokio::main]
/// async fn main() {
///     let provider = VllmProvider::new("meta-llama/Llama-2-7b-hf".to_string())
///         .with_base_url("http://localhost:8000".to_string());
///
///     let request = LlmRequest {
///         prompt: "What is the capital of France?".to_string(),
///         system_prompt: None,
///         temperature: Some(0.7),
///         max_tokens: Some(100),
///         tools: Vec::new(),
///         images: Vec::new(),
///     };
///
///     // let response = provider.complete(request).await.unwrap();
///     // println!("{}", response.content);
/// }
/// ```
pub struct VllmProvider {
    model: String,
    client: reqwest::Client,
    base_url: String,
}

// Request/Response types (OpenAI-compatible)
#[derive(Serialize)]
struct VllmChatRequest {
    model: String,
    messages: Vec<VllmMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct VllmMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct VllmChatResponse {
    choices: Vec<VllmChoice>,
    #[serde(default)]
    usage: Option<VllmUsage>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct VllmChoice {
    message: VllmMessageResponse,
    #[serde(default)]
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct VllmMessageResponse {
    content: String,
    #[serde(default)]
    #[allow(dead_code)]
    role: String,
}

#[derive(Deserialize)]
struct VllmUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// Streaming types
#[derive(Deserialize)]
struct VllmStreamResponse {
    choices: Vec<VllmStreamChoice>,
    #[serde(default)]
    usage: Option<VllmUsage>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct VllmStreamChoice {
    delta: VllmDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct VllmDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    role: Option<String>,
}

impl VllmProvider {
    /// Create a new vLLM provider
    ///
    /// # Arguments
    /// * `model` - Model identifier (e.g., "meta-llama/Llama-2-7b-hf")
    pub fn new(model: String) -> Self {
        Self {
            model,
            client: reqwest::Client::new(),
            base_url: "http://localhost:8000".to_string(),
        }
    }

    /// Set a custom base URL for the vLLM server
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Create a provider for embeddings
    pub fn for_embeddings(model: String) -> Self {
        Self::new(model)
    }

    /// Convert LlmRequest to vLLM messages format
    fn to_messages(&self, request: &LlmRequest) -> Vec<VllmMessage> {
        let mut messages = Vec::new();

        if let Some(system) = &request.system_prompt {
            messages.push(VllmMessage {
                role: "system".to_string(),
                content: system.clone(),
            });
        }

        messages.push(VllmMessage {
            role: "user".to_string(),
            content: request.prompt.clone(),
        });

        messages
    }
}

#[async_trait]
impl LlmProvider for VllmProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let vllm_request = VllmChatRequest {
            model: self.model.clone(),
            messages: self.to_messages(&request),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(false),
        };

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Content-Type", "application/json")
            .json(&vllm_request)
            .send()
            .await?;

        let status = response.status();

        if status == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);

            return Err(LlmError::RateLimited(retry_after));
        }

        let body = response.text().await?;

        if !status.is_success() {
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let vllm_response: VllmChatResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        let choice = vllm_response
            .choices
            .first()
            .ok_or_else(|| LlmError::ApiError("No choices in response".to_string()))?;

        Ok(LlmResponse {
            content: choice.message.content.clone(),
            model: vllm_response.model.unwrap_or_else(|| self.model.clone()),
            usage: vllm_response.usage.map(|u| crate::Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
            tool_calls: Vec::new(),
        })
    }
}

#[async_trait]
impl StreamingLlmProvider for VllmProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let vllm_request = VllmChatRequest {
            model: self.model.clone(),
            messages: self.to_messages(&request),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(true),
        };

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Content-Type", "application/json")
            .json(&vllm_request)
            .send()
            .await?;

        let status = response.status();

        if status == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);

            return Err(LlmError::RateLimited(retry_after));
        }

        if !status.is_success() {
            let body = response.text().await?;
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let stream = response.bytes_stream();
        let model_name = self.model.clone();

        let parsed_stream = stream.filter_map(move |chunk_result| {
            let model_name = model_name.clone();
            async move {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        for line in text.lines() {
                            if line.trim().is_empty() || line == "data: [DONE]" {
                                continue;
                            }

                            // vLLM uses SSE format with "data: " prefix
                            let data = if let Some(stripped) = line.strip_prefix("data: ") {
                                stripped
                            } else {
                                continue;
                            };

                            if let Ok(chunk) = serde_json::from_str::<VllmStreamResponse>(data) {
                                if let Some(choice) = chunk.choices.first() {
                                    let is_done = choice.finish_reason.is_some();
                                    let content = choice.delta.content.clone().unwrap_or_default();

                                    return Some(Ok(LlmChunk {
                                        content,
                                        done: is_done,
                                        model: if is_done {
                                            chunk.model.or(Some(model_name))
                                        } else {
                                            None
                                        },
                                        usage: chunk.usage.map(|u| crate::StreamUsage {
                                            prompt_tokens: Some(u.prompt_tokens),
                                            completion_tokens: Some(u.completion_tokens),
                                            total_tokens: Some(u.total_tokens),
                                        }),
                                    }));
                                }
                            }
                        }
                        None
                    }
                    Err(e) => Some(Err(LlmError::NetworkError(e))),
                }
            }
        });

        Ok(Box::pin(parsed_stream))
    }
}

// Embedding support (vLLM supports embeddings via OpenAI-compatible API)
#[derive(Serialize)]
struct VllmEmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct VllmEmbeddingResponse {
    data: Vec<VllmEmbeddingData>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<VllmEmbeddingUsage>,
}

#[derive(Deserialize)]
struct VllmEmbeddingData {
    embedding: Vec<f32>,
    #[serde(default)]
    #[allow(dead_code)]
    index: u32,
}

#[derive(Deserialize)]
struct VllmEmbeddingUsage {
    prompt_tokens: u32,
    total_tokens: u32,
}

#[async_trait]
impl EmbeddingProvider for VllmProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let vllm_request = VllmEmbeddingRequest {
            model: request.model.clone().unwrap_or_else(|| self.model.clone()),
            input: request.texts.clone(),
        };

        let response = self
            .client
            .post(format!("{}/v1/embeddings", self.base_url))
            .header("Content-Type", "application/json")
            .json(&vllm_request)
            .send()
            .await?;

        let status = response.status();

        if status == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);

            return Err(LlmError::RateLimited(retry_after));
        }

        let body = response.text().await?;

        if !status.is_success() {
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let vllm_response: VllmEmbeddingResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        let embeddings: Vec<Vec<f32>> = vllm_response
            .data
            .into_iter()
            .map(|d| d.embedding)
            .collect();

        Ok(EmbeddingResponse {
            embeddings,
            model: vllm_response.model.unwrap_or_else(|| self.model.clone()),
            usage: vllm_response.usage.map(|u| crate::EmbeddingUsage {
                prompt_tokens: u.prompt_tokens,
                total_tokens: u.total_tokens,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vllm_provider_creation() {
        let provider = VllmProvider::new("meta-llama/Llama-2-7b-hf".to_string());
        assert_eq!(provider.model, "meta-llama/Llama-2-7b-hf");
        assert_eq!(provider.base_url, "http://localhost:8000");

        let provider_custom = VllmProvider::new("mistralai/Mistral-7B-v0.1".to_string())
            .with_base_url("http://localhost:9000".to_string());
        assert_eq!(provider_custom.base_url, "http://localhost:9000");
    }

    #[test]
    fn test_vllm_for_embeddings() {
        let provider = VllmProvider::for_embeddings("intfloat/e5-mistral-7b-instruct".to_string());
        assert_eq!(provider.model, "intfloat/e5-mistral-7b-instruct");
    }

    #[test]
    fn test_message_conversion() {
        let provider = VllmProvider::new("test".to_string());

        let request = LlmRequest {
            prompt: "Hello".to_string(),
            system_prompt: Some("You are helpful".to_string()),
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let messages = provider.to_messages(&request);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, "You are helpful");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, "Hello");
    }

    #[test]
    fn test_message_conversion_no_system() {
        let provider = VllmProvider::new("test".to_string());

        let request = LlmRequest {
            prompt: "Hello".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let messages = provider.to_messages(&request);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello");
    }
}
