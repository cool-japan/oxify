//! Cohere LLM provider

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage, LlmChunk, LlmError,
    LlmProvider, LlmRequest, LlmResponse, LlmStream, Result, StreamUsage, StreamingLlmProvider,
    Usage,
};
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Cohere provider
pub struct CohereProvider {
    api_key: String,
    model: String,
    client: oxihttp::HttpsClient,
    base_url: String,
}

#[derive(Serialize)]
struct CohereRequest {
    message: String,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preamble: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct CohereResponse {
    text: String,
    meta: Option<CohereMeta>,
}

#[derive(Deserialize)]
struct CohereMeta {
    billed_units: Option<CohereBilledUnits>,
}

#[derive(Deserialize)]
struct CohereBilledUnits {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

impl CohereProvider {
    /// Create a new Cohere provider
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for Cohere"),
            base_url: "https://api.cohere.ai/v1".to_string(),
        }
    }

    /// Create a provider specifically for embeddings
    pub fn for_embeddings(api_key: String) -> Self {
        Self::new(api_key, "embed-english-v3.0".to_string())
    }

    /// Set custom base URL
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

#[async_trait]
impl LlmProvider for CohereProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let cohere_request = CohereRequest {
            message: request.prompt.clone(),
            model: self.model.clone(),
            preamble: request.system_prompt,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let response = self
            .client
            .post(&format!("{}/chat", self.base_url))?
            .header("Authorization", &format!("Bearer {}", self.api_key))?
            .header("Content-Type", "application/json")?
            .json(&cohere_request)?
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

        let cohere_response: CohereResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        let usage = cohere_response.meta.and_then(|m| m.billed_units).map(|bu| {
            let input = bu.input_tokens.unwrap_or(0);
            let output = bu.output_tokens.unwrap_or(0);
            Usage {
                prompt_tokens: input,
                completion_tokens: output,
                total_tokens: input + output,
            }
        });

        Ok(LlmResponse {
            content: cohere_response.text,
            model: self.model.clone(),
            usage,
            tool_calls: Vec::new(),
        })
    }
}

// ===== Cohere Streaming Implementation =====

#[derive(Serialize)]
struct CohereStreamRequest {
    message: String,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preamble: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Deserialize)]
#[serde(tag = "event_type")]
enum CohereStreamEvent {
    #[serde(rename = "text-generation")]
    TextGeneration { text: String },
    #[serde(rename = "stream-end")]
    StreamEnd { response: CohereStreamEndResponse },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct CohereStreamEndResponse {
    meta: Option<CohereStreamMeta>,
}

#[derive(Deserialize)]
struct CohereStreamMeta {
    billed_units: Option<CohereBilledUnits>,
}

#[async_trait]
impl StreamingLlmProvider for CohereProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let cohere_request = CohereStreamRequest {
            message: request.prompt.clone(),
            model: self.model.clone(),
            preamble: request.system_prompt,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: true,
        };

        let response = self
            .client
            .post(&format!("{}/chat", self.base_url))?
            .header("Authorization", &format!("Bearer {}", self.api_key))?
            .header("Content-Type", "application/json")?
            .json(&cohere_request)?
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

                            if let Ok(event) = serde_json::from_str::<CohereStreamEvent>(line) {
                                match event {
                                    CohereStreamEvent::TextGeneration { text } => {
                                        return Some(Ok(LlmChunk {
                                            content: text,
                                            done: false,
                                            model: None,
                                            usage: None,
                                        }));
                                    }
                                    CohereStreamEvent::StreamEnd { response } => {
                                        let usage =
                                            response.meta.and_then(|m| m.billed_units).map(|bu| {
                                                let input = bu.input_tokens.unwrap_or(0);
                                                let output = bu.output_tokens.unwrap_or(0);
                                                StreamUsage {
                                                    prompt_tokens: Some(input),
                                                    completion_tokens: Some(output),
                                                    total_tokens: Some(input + output),
                                                }
                                            });

                                        return Some(Ok(LlmChunk {
                                            content: String::new(),
                                            done: true,
                                            model: Some(model_name),
                                            usage,
                                        }));
                                    }
                                    CohereStreamEvent::Other => {}
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

// ===== Cohere Embeddings Implementation =====

#[derive(Serialize)]
struct CohereEmbeddingRequest {
    texts: Vec<String>,
    model: String,
    input_type: String,
}

#[derive(Deserialize)]
struct CohereEmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
    meta: Option<CohereEmbeddingMeta>,
}

#[derive(Deserialize)]
struct CohereEmbeddingMeta {
    billed_units: Option<CohereEmbeddingBilledUnits>,
}

#[derive(Deserialize)]
struct CohereEmbeddingBilledUnits {
    input_tokens: Option<u32>,
}

#[async_trait]
impl EmbeddingProvider for CohereProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let model = request.model.unwrap_or_else(|| self.model.clone());

        let cohere_request = CohereEmbeddingRequest {
            texts: request.texts,
            model: model.clone(),
            input_type: "search_document".to_string(),
        };

        let response = self
            .client
            .post(&format!("{}/embed", self.base_url))?
            .header("Authorization", &format!("Bearer {}", self.api_key))?
            .header("Content-Type", "application/json")?
            .json(&cohere_request)?
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

        let cohere_response: CohereEmbeddingResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        let usage = cohere_response.meta.and_then(|m| m.billed_units).map(|bu| {
            let input = bu.input_tokens.unwrap_or(0);
            EmbeddingUsage {
                prompt_tokens: input,
                total_tokens: input,
            }
        });

        Ok(EmbeddingResponse {
            embeddings: cohere_response.embeddings,
            model,
            usage,
        })
    }
}
