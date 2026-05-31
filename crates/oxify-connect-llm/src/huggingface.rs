//! Hugging Face Inference Router LLM provider (OpenAI-compatible)
//!
//! Uses the HF Inference Router at https://router.huggingface.co/v1 which provides
//! an OpenAI-compatible endpoint for text generation, streaming, and embeddings.

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage, LlmChunk, LlmError,
    LlmProvider, LlmRequest, LlmResponse, LlmStream, Result, StreamUsage, StreamingLlmProvider,
    Usage,
};
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Hugging Face Inference Router provider
///
/// Connects to the HF Inference Router which exposes an OpenAI-compatible API
/// for text generation, streaming completions, and embeddings.
pub struct HuggingFaceProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
    base_url: String,
}

// ===== Chat completion request/response types =====

#[derive(Debug, Serialize, Deserialize)]
struct HfMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct HfRequest {
    model: String,
    messages: Vec<HfMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(default)]
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct HfResponse {
    choices: Vec<HfChoice>,
    #[serde(default)]
    usage: Option<HfUsage>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct HfChoice {
    message: HfMessage,
}

#[derive(Debug, Deserialize)]
struct HfUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ===== Streaming types =====

#[derive(Debug, Deserialize)]
struct HfStreamChunk {
    choices: Vec<HfStreamChoice>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<HfStreamUsage>,
}

#[derive(Debug, Deserialize)]
struct HfStreamChoice {
    delta: HfStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HfStreamDelta {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct HfStreamUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
    #[serde(default)]
    total_tokens: Option<u32>,
}

// ===== Embeddings types =====

#[derive(Debug, Serialize)]
struct HfEmbeddingRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct HfEmbeddingResponse {
    data: Vec<HfEmbeddingData>,
    #[serde(default)]
    usage: Option<HfEmbeddingUsage>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct HfEmbeddingData {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct HfEmbeddingUsage {
    prompt_tokens: u32,
    total_tokens: u32,
}

impl HuggingFaceProvider {
    /// Create a new Hugging Face Inference Router provider
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: reqwest::Client::new(),
            base_url: "https://router.huggingface.co/v1".to_string(),
        }
    }

    /// Create a provider configured for embeddings using the BGE small model
    pub fn for_embeddings(api_key: String) -> Self {
        Self::new(api_key, "BAAI/bge-small-en-v1.5".to_string())
    }

    /// Override the base URL (useful for testing or self-hosted deployments)
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Convert an `LlmRequest` into the HF message format, prepending an optional system message
    fn to_messages(&self, request: &LlmRequest) -> Vec<HfMessage> {
        let mut messages = Vec::new();
        if let Some(system) = &request.system_prompt {
            messages.push(HfMessage {
                role: "system".to_string(),
                content: system.clone(),
            });
        }
        messages.push(HfMessage {
            role: "user".to_string(),
            content: request.prompt.clone(),
        });
        messages
    }
}

// ===== LlmProvider implementation =====

#[async_trait]
impl LlmProvider for HuggingFaceProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let hf_request = HfRequest {
            model: self.model.clone(),
            messages: self.to_messages(&request),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&hf_request)
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

        let hf_response: HfResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        if hf_response.choices.is_empty() {
            return Err(LlmError::ApiError(
                "Empty response from HuggingFace".to_string(),
            ));
        }

        Ok(LlmResponse {
            content: hf_response.choices[0].message.content.clone(),
            model: hf_response.model,
            usage: hf_response.usage.map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
            tool_calls: Vec::new(),
        })
    }
}

// ===== StreamingLlmProvider implementation =====

#[async_trait]
impl StreamingLlmProvider for HuggingFaceProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let messages = self.to_messages(&request);

        let hf_request = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "stream": true
        });

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&hf_request)
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

        let parsed_stream = stream.filter_map(|chunk_result| async move {
            match chunk_result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                return Some(Ok(LlmChunk {
                                    content: String::new(),
                                    done: true,
                                    model: None,
                                    usage: None,
                                }));
                            }

                            if let Ok(chunk) = serde_json::from_str::<HfStreamChunk>(data) {
                                // HF may send a final usage-only chunk with empty choices
                                if chunk.choices.is_empty() {
                                    if let Some(u) = chunk.usage {
                                        return Some(Ok(LlmChunk {
                                            content: String::new(),
                                            done: true,
                                            model: chunk.model,
                                            usage: Some(StreamUsage {
                                                prompt_tokens: u.prompt_tokens,
                                                completion_tokens: u.completion_tokens,
                                                total_tokens: u.total_tokens,
                                            }),
                                        }));
                                    }
                                    continue;
                                }

                                if let Some(choice) = chunk.choices.first() {
                                    let is_done = choice.finish_reason.is_some();
                                    let content = choice.delta.content.clone();

                                    let usage = chunk.usage.as_ref().map(|u| StreamUsage {
                                        prompt_tokens: u.prompt_tokens,
                                        completion_tokens: u.completion_tokens,
                                        total_tokens: u.total_tokens,
                                    });

                                    if !content.is_empty() || is_done {
                                        return Some(Ok(LlmChunk {
                                            content,
                                            done: is_done,
                                            model: if is_done { chunk.model } else { None },
                                            usage,
                                        }));
                                    }
                                }
                            }
                        }
                    }
                    None
                }
                Err(e) => Some(Err(LlmError::NetworkError(e))),
            }
        });

        Ok(Box::pin(parsed_stream))
    }
}

// ===== EmbeddingProvider implementation =====

#[async_trait]
impl EmbeddingProvider for HuggingFaceProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let model = request.model.unwrap_or_else(|| self.model.clone());

        let hf_request = HfEmbeddingRequest {
            input: request.texts,
            model,
        };

        let response = self
            .client
            .post(format!("{}/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&hf_request)
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

        let hf_response: HfEmbeddingResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        // Sort by index to preserve correct ordering
        let mut data = hf_response.data;
        data.sort_by_key(|d| d.index);

        Ok(EmbeddingResponse {
            embeddings: data.into_iter().map(|d| d.embedding).collect(),
            model: hf_response.model,
            usage: hf_response.usage.map(|u| EmbeddingUsage {
                prompt_tokens: u.prompt_tokens,
                total_tokens: u.total_tokens,
            }),
        })
    }
}

// ===== Unit tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_huggingface_provider_creation() {
        let provider =
            HuggingFaceProvider::new("test-key".to_string(), "meta-llama/Llama-3-8B".to_string());
        assert_eq!(provider.model, "meta-llama/Llama-3-8B");
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.base_url, "https://router.huggingface.co/v1");
    }

    #[test]
    fn test_huggingface_with_base_url() {
        let provider = HuggingFaceProvider::new("key".to_string(), "model".to_string())
            .with_base_url("https://my.endpoint.com/v1".to_string());
        assert_eq!(provider.base_url, "https://my.endpoint.com/v1");
    }

    #[test]
    fn test_huggingface_for_embeddings() {
        let provider = HuggingFaceProvider::for_embeddings("key".to_string());
        assert_eq!(provider.model, "BAAI/bge-small-en-v1.5");
        assert_eq!(provider.base_url, "https://router.huggingface.co/v1");
    }

    #[test]
    fn test_to_messages_with_system() {
        let provider = HuggingFaceProvider::new("key".to_string(), "model".to_string());
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
    fn test_to_messages_no_system() {
        let provider = HuggingFaceProvider::new("key".to_string(), "model".to_string());
        let request = LlmRequest {
            prompt: "Hi".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };
        let messages = provider.to_messages(&request);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
    }

    #[test]
    fn test_default_model_for_embeddings() {
        let provider = HuggingFaceProvider::for_embeddings("key".to_string());
        assert!(!provider.model.is_empty());
        assert!(
            provider.model.contains('/'),
            "model should be in org/name format"
        );
    }
}
