//! Mistral AI LLM provider

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage, LlmChunk, LlmError,
    LlmProvider, LlmRequest, LlmResponse, LlmStream, Result, StreamUsage, StreamingLlmProvider,
    Usage,
};
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Mistral AI provider
pub struct MistralProvider {
    api_key: String,
    model: String,
    client: oxihttp::HttpsClient,
    base_url: String,
}

#[derive(Serialize)]
struct MistralRequest {
    model: String,
    messages: Vec<MistralMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct MistralMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MistralResponse {
    choices: Vec<MistralChoice>,
    usage: MistralUsage,
    model: String,
}

#[derive(Deserialize)]
struct MistralChoice {
    message: MistralMessage,
}

#[derive(Deserialize)]
struct MistralUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

impl MistralProvider {
    /// Create a new Mistral provider
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for Mistral"),
            base_url: "https://api.mistral.ai/v1".to_string(),
        }
    }

    /// Create a provider specifically for embeddings
    pub fn for_embeddings(api_key: String) -> Self {
        Self::new(api_key, "mistral-embed".to_string())
    }

    /// Set custom base URL
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

#[async_trait]
impl LlmProvider for MistralProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let mut messages = Vec::new();

        // Add system message if provided
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(MistralMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            });
        }

        // Add user message
        messages.push(MistralMessage {
            role: "user".to_string(),
            content: request.prompt.clone(),
        });

        let mistral_request = MistralRequest {
            model: self.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let response = self
            .client
            .post(&format!("{}/chat/completions", self.base_url))?
            .header("Authorization", &format!("Bearer {}", self.api_key))?
            .header("Content-Type", "application/json")?
            .json(&mistral_request)?
            .send()
            .await?;

        let status = response.status();

        if status == 429 {
            // Extract Retry-After header if present
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);

            return Err(LlmError::RateLimited(retry_after));
        }

        let body = response.body_text().await?;

        if !status.is_success() {
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let mistral_response: MistralResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        if mistral_response.choices.is_empty() {
            return Err(LlmError::ApiError("No choices in response".to_string()));
        }

        Ok(LlmResponse {
            content: mistral_response.choices[0].message.content.clone(),
            model: mistral_response.model,
            usage: Some(Usage {
                prompt_tokens: mistral_response.usage.prompt_tokens,
                completion_tokens: mistral_response.usage.completion_tokens,
                total_tokens: mistral_response.usage.total_tokens,
            }),
            tool_calls: Vec::new(),
        })
    }
}

// ===== Mistral Streaming Implementation =====

#[derive(Deserialize)]
struct MistralStreamChunk {
    choices: Vec<MistralStreamChoice>,
    #[serde(default)]
    usage: Option<MistralUsage>,
    model: String,
}

#[derive(Deserialize)]
struct MistralStreamChoice {
    delta: MistralDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct MistralDelta {
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl StreamingLlmProvider for MistralProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let mut messages = Vec::new();

        if let Some(system_prompt) = &request.system_prompt {
            messages.push(MistralMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            });
        }

        messages.push(MistralMessage {
            role: "user".to_string(),
            content: request.prompt.clone(),
        });

        let mistral_request = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "stream": true
        });

        let response = self
            .client
            .post(&format!("{}/chat/completions", self.base_url))?
            .header("Authorization", &format!("Bearer {}", self.api_key))?
            .header("Content-Type", "application/json")?
            .json(&mistral_request)?
            .send()
            .await?;

        let status = response.status();
        if status == 429 {
            // Extract Retry-After header if present
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);

            return Err(LlmError::RateLimited(retry_after));
        }

        if !status.is_success() {
            let body = response.body_text().await?;
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let stream = response.body_stream();

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

                            if let Ok(chunk) = serde_json::from_str::<MistralStreamChunk>(data) {
                                if let Some(choice) = chunk.choices.first() {
                                    let is_done = choice.finish_reason.is_some();
                                    let content = choice.delta.content.clone().unwrap_or_default();

                                    let usage = chunk.usage.as_ref().map(|u| StreamUsage {
                                        prompt_tokens: Some(u.prompt_tokens),
                                        completion_tokens: Some(u.completion_tokens),
                                        total_tokens: Some(u.total_tokens),
                                    });

                                    if !content.is_empty() || is_done {
                                        return Some(Ok(LlmChunk {
                                            content,
                                            done: is_done,
                                            model: if is_done { Some(chunk.model) } else { None },
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

// ===== Mistral Embeddings Implementation =====

#[derive(Serialize)]
struct MistralEmbeddingRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Deserialize)]
struct MistralEmbeddingResponse {
    data: Vec<MistralEmbeddingData>,
    model: String,
    usage: MistralEmbeddingUsage,
}

#[derive(Deserialize)]
struct MistralEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct MistralEmbeddingUsage {
    prompt_tokens: u32,
    total_tokens: u32,
}

#[async_trait]
impl EmbeddingProvider for MistralProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let model = request.model.unwrap_or_else(|| self.model.clone());

        let mistral_request = MistralEmbeddingRequest {
            input: request.texts,
            model,
        };

        let response = self
            .client
            .post(&format!("{}/embeddings", self.base_url))?
            .header("Authorization", &format!("Bearer {}", self.api_key))?
            .header("Content-Type", "application/json")?
            .json(&mistral_request)?
            .send()
            .await?;

        let status = response.status();

        if status == 429 {
            // Extract Retry-After header if present
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);

            return Err(LlmError::RateLimited(retry_after));
        }

        let body = response.body_text().await?;

        if !status.is_success() {
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let mistral_response: MistralEmbeddingResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        // Sort by index to ensure correct order
        let mut data = mistral_response.data;
        data.sort_by_key(|d| d.index);

        Ok(EmbeddingResponse {
            embeddings: data.into_iter().map(|d| d.embedding).collect(),
            model: mistral_response.model,
            usage: Some(EmbeddingUsage {
                prompt_tokens: mistral_response.usage.prompt_tokens,
                total_tokens: mistral_response.usage.total_tokens,
            }),
        })
    }
}
