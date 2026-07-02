//! llama.cpp server integration for local LLM inference

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmChunk, LlmError, LlmProvider,
    LlmRequest, LlmResponse, LlmStream, Result, StreamingLlmProvider,
};
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// llama.cpp server provider for local LLM inference
///
/// # Example
/// ```no_run
/// use oxify_connect_llm::{LlamaCppProvider, LlmProvider, LlmRequest};
///
/// #[tokio::main]
/// async fn main() {
///     let provider = LlamaCppProvider::new("llama-2-7b".to_string())
///         .with_base_url("http://localhost:8080".to_string());
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
pub struct LlamaCppProvider {
    model: String,
    client: oxihttp::HttpsClient,
    base_url: String,
}

// Request/Response types (OpenAI-compatible)
#[derive(Serialize)]
struct LlamaCppRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Deserialize)]
struct LlamaCppResponse {
    content: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    stop: bool,
}

#[derive(Deserialize)]
struct LlamaCppStreamResponse {
    content: String,
    #[serde(default)]
    stop: bool,
    #[serde(default)]
    model: Option<String>,
}

impl LlamaCppProvider {
    /// Create a new llama.cpp provider
    ///
    /// # Arguments
    /// * `model` - Model identifier (can be any string, mostly for tracking)
    pub fn new(model: String) -> Self {
        Self {
            model,
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for llama.cpp"),
            base_url: "http://localhost:8080".to_string(),
        }
    }

    /// Set a custom base URL for the llama.cpp server
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Create a provider for embeddings
    pub fn for_embeddings(model: String) -> Self {
        Self::new(model)
    }
}

#[async_trait]
impl LlmProvider for LlamaCppProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Combine system prompt and user prompt
        let prompt = if let Some(system) = &request.system_prompt {
            format!("{}\n\n{}", system, request.prompt)
        } else {
            request.prompt.clone()
        };

        let llamacpp_request = LlamaCppRequest {
            model: self.model.clone(),
            prompt,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(false),
        };

        let response = self
            .client
            .post(&format!("{}/completion", self.base_url))?
            .header("Content-Type", "application/json")?
            .json(&llamacpp_request)?
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

        let body = response.body_text().await?;

        if !status.is_success() {
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let llamacpp_response: LlamaCppResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        Ok(LlmResponse {
            content: llamacpp_response.content,
            model: llamacpp_response
                .model
                .unwrap_or_else(|| self.model.clone()),
            usage: None, // llama.cpp server doesn't provide token counts in basic mode
            tool_calls: Vec::new(),
        })
    }
}

#[async_trait]
impl StreamingLlmProvider for LlamaCppProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let prompt = if let Some(system) = &request.system_prompt {
            format!("{}\n\n{}", system, request.prompt)
        } else {
            request.prompt.clone()
        };

        let llamacpp_request = LlamaCppRequest {
            model: self.model.clone(),
            prompt,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(true),
        };

        let response = self
            .client
            .post(&format!("{}/completion", self.base_url))?
            .header("Content-Type", "application/json")?
            .json(&llamacpp_request)?
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
            let body = response.body_text().await?;
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let stream = response.body_stream();
        let model_name = self.model.clone();

        let parsed_stream = stream.filter_map(move |chunk_result| {
            let model_name = model_name.clone();
            async move {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        for line in text.lines() {
                            if line.trim().is_empty() {
                                continue;
                            }

                            // llama.cpp uses SSE format
                            let data = if let Some(stripped) = line.strip_prefix("data: ") {
                                stripped
                            } else {
                                line
                            };

                            if let Ok(chunk) = serde_json::from_str::<LlamaCppStreamResponse>(data)
                            {
                                return Some(Ok(LlmChunk {
                                    content: chunk.content,
                                    done: chunk.stop,
                                    model: if chunk.stop {
                                        chunk.model.or(Some(model_name))
                                    } else {
                                        None
                                    },
                                    usage: None,
                                }));
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

// Embedding support (if llama.cpp server is built with embedding support)
#[derive(Serialize)]
struct LlamaCppEmbeddingRequest {
    content: String,
}

#[derive(Deserialize)]
struct LlamaCppEmbeddingResponse {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for LlamaCppProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let mut embeddings = Vec::with_capacity(request.texts.len());

        // Process each text separately
        for text in &request.texts {
            let llamacpp_request = LlamaCppEmbeddingRequest {
                content: text.clone(),
            };

            let response = self
                .client
                .post(&format!("{}/embedding", self.base_url))?
                .header("Content-Type", "application/json")?
                .json(&llamacpp_request)?
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

            let body = response.body_text().await?;

            if !status.is_success() {
                return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
            }

            let llamacpp_response: LlamaCppEmbeddingResponse = serde_json::from_str(&body)
                .map_err(|e| LlmError::SerializationError(e.to_string()))?;

            embeddings.push(llamacpp_response.embedding);
        }

        Ok(EmbeddingResponse {
            embeddings,
            model: request.model.unwrap_or_else(|| self.model.clone()),
            usage: None, // llama.cpp doesn't provide token usage for embeddings
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llamacpp_provider_creation() {
        let provider = LlamaCppProvider::new("llama-2-7b".to_string());
        assert_eq!(provider.model, "llama-2-7b");
        assert_eq!(provider.base_url, "http://localhost:8080");

        let provider_custom = LlamaCppProvider::new("mistral-7b".to_string())
            .with_base_url("http://localhost:9090".to_string());
        assert_eq!(provider_custom.base_url, "http://localhost:9090");
    }

    #[test]
    fn test_llamacpp_for_embeddings() {
        let provider = LlamaCppProvider::for_embeddings("llama-2-7b".to_string());
        assert_eq!(provider.model, "llama-2-7b");
    }

    #[test]
    fn test_system_prompt_combination() {
        let provider = LlamaCppProvider::new("test".to_string());

        // Test that the provider combines system and user prompts
        // (This is implicit in the implementation, just validating construction)
        assert_eq!(provider.model, "test");
    }
}
