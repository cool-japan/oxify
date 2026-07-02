//! Google Gemini LLM provider

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmChunk, LlmError, LlmProvider,
    LlmRequest, LlmResponse, LlmStream, Result, StreamUsage, StreamingLlmProvider, Usage,
};
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Google Gemini provider
pub struct GeminiProvider {
    api_key: String,
    model: String,
    client: oxihttp::HttpsClient,
    base_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
}

#[derive(Serialize, Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
    #[serde(default)]
    role: String,
}

#[derive(Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsage {
    prompt_token_count: u32,
    candidates_token_count: u32,
    total_token_count: u32,
}

impl GeminiProvider {
    /// Create a new Gemini provider
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for Gemini"),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        }
    }

    /// Create a provider specifically for embeddings
    pub fn for_embeddings(api_key: String) -> Self {
        Self::new(api_key, "text-embedding-004".to_string())
    }

    /// Set custom base URL
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let mut gemini_request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: request.prompt.clone(),
                }],
                role: "user".to_string(),
            }],
            generation_config: Some(GeminiGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
            system_instruction: None,
        };

        if let Some(system_prompt) = request.system_prompt {
            gemini_request.system_instruction = Some(GeminiSystemInstruction {
                parts: vec![GeminiPart {
                    text: system_prompt,
                }],
            });
        }

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let response = self
            .client
            .post(&url)?
            .header("Content-Type", "application/json")?
            .json(&gemini_request)?
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

        let gemini_response: GeminiResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        if gemini_response.candidates.is_empty() {
            return Err(LlmError::ApiError("No candidates in response".to_string()));
        }

        let content = &gemini_response.candidates[0].content;
        if content.parts.is_empty() {
            return Err(LlmError::ApiError("No parts in content".to_string()));
        }

        let usage = gemini_response.usage_metadata.map(|u| Usage {
            prompt_tokens: u.prompt_token_count,
            completion_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
        });

        Ok(LlmResponse {
            content: content.parts[0].text.clone(),
            model: self.model.clone(),
            usage,
            tool_calls: Vec::new(),
        })
    }
}

// ===== Gemini Streaming Implementation =====

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiStreamResponse {
    candidates: Vec<GeminiCandidate>,
    usage_metadata: Option<GeminiUsage>,
}

#[async_trait]
impl StreamingLlmProvider for GeminiProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let mut gemini_request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: request.prompt.clone(),
                }],
                role: "user".to_string(),
            }],
            generation_config: Some(GeminiGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
            system_instruction: None,
        };

        if let Some(system_prompt) = request.system_prompt {
            gemini_request.system_instruction = Some(GeminiSystemInstruction {
                parts: vec![GeminiPart {
                    text: system_prompt,
                }],
            });
        }

        let url = format!(
            "{}/models/{}:streamGenerateContent?key={}&alt=sse",
            self.base_url, self.model, self.api_key
        );

        let response = self
            .client
            .post(&url)?
            .header("Content-Type", "application/json")?
            .json(&gemini_request)?
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
                            if let Some(data) = line.strip_prefix("data: ") {
                                if let Ok(response) =
                                    serde_json::from_str::<GeminiStreamResponse>(data)
                                {
                                    if let Some(candidate) = response.candidates.first() {
                                        if let Some(part) = candidate.content.parts.first() {
                                            let usage = response.usage_metadata.as_ref().map(|u| {
                                                StreamUsage {
                                                    prompt_tokens: Some(u.prompt_token_count),
                                                    completion_tokens: Some(
                                                        u.candidates_token_count,
                                                    ),
                                                    total_tokens: Some(u.total_token_count),
                                                }
                                            });

                                            return Some(Ok(LlmChunk {
                                                content: part.text.clone(),
                                                done: response.usage_metadata.is_some(),
                                                model: if response.usage_metadata.is_some() {
                                                    Some(model_name)
                                                } else {
                                                    None
                                                },
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
            }
        });

        Ok(Box::pin(parsed_stream))
    }
}

// ===== Gemini Embeddings Implementation =====

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiEmbeddingRequest {
    content: GeminiEmbeddingContent,
}

#[derive(Serialize)]
struct GeminiEmbeddingContent {
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiEmbeddingResponse {
    embedding: GeminiEmbedding,
}

#[derive(Deserialize)]
struct GeminiEmbedding {
    values: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for GeminiProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let model = request.model.unwrap_or_else(|| self.model.clone());

        let mut embeddings = Vec::with_capacity(request.texts.len());

        // Gemini embeddings API processes one text at a time
        for text in &request.texts {
            let gemini_request = GeminiEmbeddingRequest {
                content: GeminiEmbeddingContent {
                    parts: vec![GeminiPart { text: text.clone() }],
                },
            };

            let url = format!(
                "{}/models/{}:embedContent?key={}",
                self.base_url, model, self.api_key
            );

            let response = self
                .client
                .post(&url)?
                .header("Content-Type", "application/json")?
                .json(&gemini_request)?
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

            let gemini_response: GeminiEmbeddingResponse = serde_json::from_str(&body)
                .map_err(|e| LlmError::SerializationError(e.to_string()))?;

            embeddings.push(gemini_response.embedding.values);
        }

        Ok(EmbeddingResponse {
            embeddings,
            model,
            usage: None, // Gemini doesn't provide token usage for embeddings
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LlmRequest;
    use futures::StreamExt;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_request(prompt: &str) -> LlmRequest {
        LlmRequest {
            prompt: prompt.to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        }
    }

    /// End-to-end regression guard for the `body_stream()` SSE parsing path.
    ///
    /// Drives `complete_stream` against a mock server that returns a realistic
    /// Gemini `alt=sse` response (two `data:` events, the second carrying
    /// `usageMetadata`) and asserts the chunks are parsed correctly. This
    /// exercises the `.body_stream()` rename introduced during the reqwest ->
    /// oxihttp migration against a real HTTP server.
    #[tokio::test]
    async fn test_complete_stream_sse_parsing() {
        let server = MockServer::start().await;

        // A realistic Gemini `alt=sse` event: a `data:` line carrying the
        // generated text plus `usageMetadata` (which marks the terminal chunk),
        // followed by the SSE event-terminating blank line.
        let sse_body = concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello world\"}],",
            "\"role\":\"model\"}}],",
            "\"usageMetadata\":{\"promptTokenCount\":5,\"candidatesTokenCount\":2,",
            "\"totalTokenCount\":7}}\n",
            "\n",
        );

        Mock::given(method("POST"))
            .and(path_regex(r".*:streamGenerateContent$"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let provider = GeminiProvider::new("test_key".to_string(), "gemini-1.5-flash".to_string())
            .with_base_url(server.uri());

        let mut stream = provider
            .complete_stream(make_request("Say hi"))
            .await
            .expect("complete_stream should return a stream");

        let mut collected_content = String::new();
        let mut last_chunk: Option<LlmChunk> = None;
        while let Some(item) = stream.next().await {
            let chunk = item.expect("stream item should be Ok");
            collected_content.push_str(&chunk.content);
            last_chunk = Some(chunk);
        }

        // The `.body_stream()` path must surface the SSE payload and the
        // `data: ` parser must reconstruct the generated text and usage.
        assert_eq!(collected_content, "Hello world");

        let last = last_chunk.expect("stream should yield at least one chunk");
        assert!(
            last.done,
            "final chunk (carrying usageMetadata) should be done"
        );
        assert_eq!(last.model.as_deref(), Some("gemini-1.5-flash"));
        let usage = last.usage.expect("final chunk should carry usage");
        assert_eq!(usage.total_tokens, Some(7));
        assert_eq!(usage.prompt_tokens, Some(5));
        assert_eq!(usage.completion_tokens, Some(2));
    }
}
