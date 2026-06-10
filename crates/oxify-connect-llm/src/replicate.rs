//! Replicate AI inference provider
//!
//! Replicate uses an async prediction lifecycle: POST a prediction, then either
//! poll for completion or consume an SSE stream. This is fundamentally different
//! from OpenAI-compatible providers which return synchronously.
//!
//! Model identifiers come in two forms:
//! - Versioned: `owner/name:sha256:abc123` — routes to `/v1/predictions`
//! - Model-based: `owner/name` — routes to `/v1/models/owner/name/predictions`

use crate::{
    LlmChunk, LlmError, LlmProvider, LlmRequest, LlmResponse, LlmStream, Result, StreamUsage,
    StreamingLlmProvider, Usage,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ===== Provider struct =====

/// Replicate AI provider
///
/// Supports versioned models (`owner/name:sha256:hash`) and model-based routing
/// (`owner/name`). Uses polling for non-streaming completion and SSE for streaming.
pub struct ReplicateProvider {
    api_token: String,
    model: String,
    client: reqwest::Client,
    /// Base URL — override for testing. Default: `https://api.replicate.com/v1`
    base_url: String,
    /// Milliseconds between polling attempts. Default: `250`
    poll_interval_ms: u64,
    /// Maximum polling attempts before returning `LlmError::Timeout`. Default: `240` (~60 s)
    max_poll_attempts: u32,
}

// ===== Serde request types =====

#[derive(Serialize)]
struct CreatePredictionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    input: ReplicateInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct ReplicateInput {
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

// ===== Serde response types =====

#[derive(Deserialize)]
struct PredictionResponse {
    status: String,
    output: Option<serde_json::Value>,
    error: Option<String>,
    urls: PredictionUrls,
    metrics: Option<PredictionMetrics>,
}

#[derive(Deserialize, Default)]
struct PredictionUrls {
    get: String,
    #[serde(default)]
    stream: Option<String>,
}

#[derive(Deserialize)]
struct PredictionMetrics {
    #[serde(default)]
    input_token_count: Option<u32>,
    #[serde(default)]
    output_token_count: Option<u32>,
}

// ===== Constructor / builder =====

impl ReplicateProvider {
    /// Create a new Replicate provider with default polling settings.
    ///
    /// `model` may be `"owner/name:sha256:hash"` (versioned) or `"owner/name"` (latest).
    pub fn new(api_token: String, model: String) -> Self {
        Self {
            api_token,
            model,
            client: reqwest::Client::new(),
            base_url: "https://api.replicate.com/v1".to_string(),
            poll_interval_ms: 250,
            max_poll_attempts: 240,
        }
    }

    /// Override the base URL (useful for testing or proxies).
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Override polling parameters.
    ///
    /// Total timeout ≈ `interval_ms × max_attempts` milliseconds.
    pub fn with_poll_settings(mut self, interval_ms: u64, max_attempts: u32) -> Self {
        self.poll_interval_ms = interval_ms;
        self.max_poll_attempts = max_attempts;
        self
    }
}

// ===== Private helpers =====

impl ReplicateProvider {
    /// Returns `true` when the model string contains a version separator (`:`).
    fn has_version(&self) -> bool {
        self.model.contains(':')
    }

    /// Extracts the version suffix from a versioned model string.
    ///
    /// - `"owner/name:sha256:abc"` → `Some("sha256:abc")`
    /// - `"owner/name:abc123"` → `Some("abc123")`
    /// - `"owner/name"` → `None`
    fn extract_version(&self) -> Option<String> {
        if self.has_version() {
            let idx = self
                .model
                .find(':')
                .expect("invariant: has_version() confirmed ':' present");
            Some(self.model[idx + 1..].to_string())
        } else {
            None
        }
    }

    /// Returns the correct endpoint URL for creating a new prediction.
    fn prediction_create_url(&self) -> String {
        if self.has_version() {
            format!("{}/predictions", self.base_url)
        } else {
            format!("{}/models/{}/predictions", self.base_url, self.model)
        }
    }

    /// Authorization header value for every Replicate request.
    fn auth_header(&self) -> String {
        format!("Token {}", self.api_token)
    }

    /// Poll `get_url` until the prediction reaches a terminal state.
    async fn poll_to_completion(&self, get_url: &str) -> Result<PredictionResponse> {
        for attempt in 0..self.max_poll_attempts {
            let resp = self
                .client
                .get(get_url)
                .header("Authorization", self.auth_header())
                .send()
                .await?
                .json::<PredictionResponse>()
                .await
                .map_err(|e| LlmError::SerializationError(e.to_string()))?;

            match resp.status.as_str() {
                "succeeded" => return Ok(resp),
                "failed" | "canceled" => {
                    return Err(LlmError::ApiError(
                        resp.error
                            .unwrap_or_else(|| format!("Prediction {}", resp.status)),
                    ))
                }
                _ => {
                    if attempt + 1 < self.max_poll_attempts {
                        tokio::time::sleep(std::time::Duration::from_millis(self.poll_interval_ms))
                            .await;
                    }
                }
            }
        }

        Err(LlmError::Timeout(std::time::Duration::from_millis(
            self.poll_interval_ms * self.max_poll_attempts as u64,
        )))
    }

    /// Collapse the token-array (or plain string) output from a completed prediction.
    fn output_to_text(output: Option<serde_json::Value>) -> String {
        match output {
            None => String::new(),
            Some(serde_json::Value::Array(tokens)) => tokens
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(""),
            Some(serde_json::Value::String(s)) => s,
            Some(other) => other.to_string(),
        }
    }

    /// Extract 429 retry-after duration from response headers.
    fn retry_after_from_headers(headers: &reqwest::header::HeaderMap) -> LlmError {
        let retry_after = headers
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .map(std::time::Duration::from_secs);
        LlmError::RateLimited(retry_after)
    }
}

// ===== LlmProvider impl =====

#[async_trait]
impl LlmProvider for ReplicateProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let body = CreatePredictionRequest {
            version: self.extract_version(),
            input: ReplicateInput {
                prompt: request.prompt.clone(),
                system_prompt: request.system_prompt.clone(),
                max_tokens: request.max_tokens,
                temperature: request.temperature,
            },
            stream: None,
        };

        let http_resp = self
            .client
            .post(self.prediction_create_url())
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await?;

        if http_resp.status().as_u16() == 429 {
            return Err(Self::retry_after_from_headers(http_resp.headers()));
        }

        if !http_resp.status().is_success() {
            let status = http_resp.status().as_u16();
            let body_text = http_resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError(format!("HTTP {status}: {body_text}")));
        }

        let prediction: PredictionResponse = http_resp
            .json()
            .await
            .map_err(|e| LlmError::SerializationError(e.to_string()))?;

        let completed = self.poll_to_completion(&prediction.urls.get).await?;
        let content = Self::output_to_text(completed.output);

        let usage = completed.metrics.map(|m| Usage {
            prompt_tokens: m.input_token_count.unwrap_or(0),
            completion_tokens: m.output_token_count.unwrap_or(0),
            total_tokens: m
                .input_token_count
                .unwrap_or(0)
                .saturating_add(m.output_token_count.unwrap_or(0)),
        });

        Ok(LlmResponse {
            content,
            model: self.model.clone(),
            usage,
            tool_calls: vec![],
        })
    }
}

// ===== StreamingLlmProvider impl =====

/// State threaded through the SSE unfold for Replicate streaming.
struct SseState {
    stream: reqwest::Response,
    /// Partial SSE line buffer — bytes may arrive split across HTTP frames.
    buffer: String,
    /// Remembered event type from the last `event:` line.
    pending_event: Option<String>,
    /// Whether we have signalled the terminal `done` chunk.
    finished: bool,
}

#[async_trait]
impl StreamingLlmProvider for ReplicateProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let body = CreatePredictionRequest {
            version: self.extract_version(),
            input: ReplicateInput {
                prompt: request.prompt.clone(),
                system_prompt: request.system_prompt.clone(),
                max_tokens: request.max_tokens,
                temperature: request.temperature,
            },
            stream: Some(true),
        };

        let http_resp = self
            .client
            .post(self.prediction_create_url())
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await?;

        if http_resp.status().as_u16() == 429 {
            return Err(Self::retry_after_from_headers(http_resp.headers()));
        }

        if !http_resp.status().is_success() {
            let status = http_resp.status().as_u16();
            let body_text = http_resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError(format!("HTTP {status}: {body_text}")));
        }

        let prediction: PredictionResponse = http_resp
            .json()
            .await
            .map_err(|e| LlmError::SerializationError(e.to_string()))?;

        // Replicate provides the SSE stream URL immediately in the POST response.
        // If it is absent (e.g. older model), fall back to polling + single chunk.
        let stream_url = match prediction.urls.stream {
            Some(ref url) if !url.is_empty() => url.clone(),
            _ => {
                // Fallback: poll to completion, emit one chunk + done.
                let completed = self.poll_to_completion(&prediction.urls.get).await?;
                let content = Self::output_to_text(completed.output);
                let usage = completed.metrics.map(|m| StreamUsage {
                    prompt_tokens: Some(m.input_token_count.unwrap_or(0)),
                    completion_tokens: Some(m.output_token_count.unwrap_or(0)),
                    total_tokens: Some(
                        m.input_token_count
                            .unwrap_or(0)
                            .saturating_add(m.output_token_count.unwrap_or(0)),
                    ),
                });

                let chunks: Vec<Result<LlmChunk>> = vec![
                    Ok(LlmChunk {
                        content: content.clone(),
                        done: false,
                        model: Some(self.model.clone()),
                        usage: None,
                    }),
                    Ok(LlmChunk {
                        content: String::new(),
                        done: true,
                        model: Some(self.model.clone()),
                        usage,
                    }),
                ];

                return Ok(Box::pin(futures::stream::iter(chunks)));
            }
        };

        // Connect to the SSE stream.
        let sse_resp = self
            .client
            .get(&stream_url)
            .header("Authorization", self.auth_header())
            .header("Accept", "text/event-stream")
            .send()
            .await?;

        if !sse_resp.status().is_success() {
            let status = sse_resp.status().as_u16();
            let body_text = sse_resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError(format!(
                "SSE HTTP {status}: {body_text}"
            )));
        }

        // Drive a stateful unfold over the byte stream.
        let initial_state = SseState {
            stream: sse_resp,
            buffer: String::new(),
            pending_event: None,
            finished: false,
        };

        let parsed_stream = futures::stream::try_unfold(initial_state, |mut state| async move {
            if state.finished {
                return Ok(None);
            }

            loop {
                // Attempt to parse buffered complete lines before fetching new bytes.
                if let Some(newline_pos) = state.buffer.find('\n') {
                    let line = state.buffer[..newline_pos]
                        .trim_end_matches('\r')
                        .to_string();
                    state.buffer = state.buffer[newline_pos + 1..].to_string();

                    // Parse SSE line structure.
                    if let Some(event_type) = line.strip_prefix("event:") {
                        state.pending_event = Some(event_type.trim().to_string());
                        continue;
                    }

                    if let Some(data_payload) = line.strip_prefix("data:") {
                        let payload = data_payload.trim().to_string();
                        let event_type = state.pending_event.take().unwrap_or_default();

                        match event_type.as_str() {
                            "output" => {
                                if !payload.is_empty() {
                                    return Ok(Some((
                                        LlmChunk {
                                            content: payload,
                                            done: false,
                                            model: None,
                                            usage: None,
                                        },
                                        state,
                                    )));
                                }
                            }
                            "done" => {
                                state.finished = true;
                                return Ok(Some((
                                    LlmChunk {
                                        content: String::new(),
                                        done: true,
                                        model: None,
                                        usage: None,
                                    },
                                    state,
                                )));
                            }
                            "error" => {
                                return Err(LlmError::ApiError(format!(
                                    "Replicate SSE error: {payload}"
                                )));
                            }
                            _ => {
                                // Unknown or empty event type — skip silently.
                                continue;
                            }
                        }

                        continue;
                    }

                    // Empty line or non-SSE content — skip.
                    continue;
                }

                // No complete line in buffer; fetch more bytes.
                match state.stream.chunk().await {
                    Ok(Some(bytes)) => {
                        let text = String::from_utf8_lossy(&bytes);
                        state.buffer.push_str(&text);
                    }
                    Ok(None) => {
                        // Stream ended without an explicit `done` event.
                        if !state.finished {
                            state.finished = true;
                            return Ok(Some((
                                LlmChunk {
                                    content: String::new(),
                                    done: true,
                                    model: None,
                                    usage: None,
                                },
                                state,
                            )));
                        }
                        return Ok(None);
                    }
                    Err(e) => return Err(LlmError::NetworkError(e)),
                }
            }
        });

        Ok(Box::pin(parsed_stream))
    }
}

// ===== Unit tests =====

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction & defaults ──────────────────────────────────────────────

    #[test]
    fn test_replicate_new_defaults() {
        let p = ReplicateProvider::new("tok".to_string(), "meta/llama2:sha256:abc".to_string());
        assert_eq!(p.base_url, "https://api.replicate.com/v1");
        assert_eq!(p.poll_interval_ms, 250);
        assert_eq!(p.max_poll_attempts, 240);
    }

    #[test]
    fn test_replicate_with_base_url() {
        let p = ReplicateProvider::new("tok".to_string(), "owner/model".to_string())
            .with_base_url("http://localhost:8080".to_string());
        assert_eq!(p.base_url, "http://localhost:8080");
    }

    #[test]
    fn test_replicate_with_poll_settings() {
        let p = ReplicateProvider::new("tok".to_string(), "owner/model".to_string())
            .with_poll_settings(500, 120);
        assert_eq!(p.poll_interval_ms, 500);
        assert_eq!(p.max_poll_attempts, 120);
    }

    // ── has_version ──────────────────────────────────────────────────────────

    #[test]
    fn test_replicate_has_version_with_colon() {
        let p = ReplicateProvider::new("tok".to_string(), "owner/model:sha256:abc123".to_string());
        assert!(p.has_version());
    }

    #[test]
    fn test_replicate_has_version_without_colon() {
        let p = ReplicateProvider::new("tok".to_string(), "owner/model".to_string());
        assert!(!p.has_version());
    }

    // ── extract_version ──────────────────────────────────────────────────────

    #[test]
    fn test_replicate_extract_version_pinned() {
        let p = ReplicateProvider::new("tok".to_string(), "meta/llama3:sha256:abc123".to_string());
        assert_eq!(p.extract_version(), Some("sha256:abc123".to_string()));
    }

    #[test]
    fn test_replicate_extract_version_short_hash() {
        let p = ReplicateProvider::new("tok".to_string(), "owner/model:abc123".to_string());
        assert_eq!(p.extract_version(), Some("abc123".to_string()));
    }

    #[test]
    fn test_replicate_extract_version_model_only() {
        let p = ReplicateProvider::new("tok".to_string(), "meta/llama3".to_string());
        assert_eq!(p.extract_version(), None);
    }

    // ── prediction_create_url ────────────────────────────────────────────────

    #[test]
    fn test_replicate_prediction_create_url_versioned() {
        let p = ReplicateProvider::new("tok".to_string(), "owner/model:sha256:v1".to_string());
        let url = p.prediction_create_url();
        assert!(url.ends_with("/predictions"));
        assert!(!url.contains("/models/"));
    }

    #[test]
    fn test_replicate_prediction_create_url_model_based() {
        let p = ReplicateProvider::new("tok".to_string(), "owner/model".to_string());
        assert!(p
            .prediction_create_url()
            .contains("/models/owner/model/predictions"));
    }

    #[test]
    fn test_replicate_prediction_create_url_custom_base() {
        let p = ReplicateProvider::new("tok".to_string(), "owner/model".to_string())
            .with_base_url("http://test:9000".to_string());
        assert_eq!(
            p.prediction_create_url(),
            "http://test:9000/models/owner/model/predictions"
        );
    }

    // ── output_to_text ───────────────────────────────────────────────────────

    #[test]
    fn test_replicate_output_to_text_array() {
        let val = serde_json::json!(["Hello", " ", "world"]);
        assert_eq!(ReplicateProvider::output_to_text(Some(val)), "Hello world");
    }

    #[test]
    fn test_replicate_output_to_text_string() {
        let val = serde_json::Value::String("direct output".to_string());
        assert_eq!(
            ReplicateProvider::output_to_text(Some(val)),
            "direct output"
        );
    }

    #[test]
    fn test_replicate_output_to_text_none() {
        assert_eq!(ReplicateProvider::output_to_text(None), "");
    }

    #[test]
    fn test_replicate_output_to_text_empty_array() {
        let val = serde_json::json!([]);
        assert_eq!(ReplicateProvider::output_to_text(Some(val)), "");
    }

    #[test]
    fn test_replicate_output_to_text_mixed_array_skips_non_strings() {
        // Non-string values in the token array should be silently skipped.
        let val = serde_json::json!(["Hello", 42, " world", null]);
        assert_eq!(ReplicateProvider::output_to_text(Some(val)), "Hello world");
    }

    // ── auth_header ──────────────────────────────────────────────────────────

    #[test]
    fn test_replicate_auth_header_format() {
        let p = ReplicateProvider::new("my_secret_token".to_string(), "owner/model".to_string());
        assert_eq!(p.auth_header(), "Token my_secret_token");
    }

    // ── serialization round-trips ────────────────────────────────────────────

    #[test]
    fn test_create_prediction_request_serializes_without_none_fields() {
        let req = CreatePredictionRequest {
            version: None,
            input: ReplicateInput {
                prompt: "Say hi".to_string(),
                system_prompt: None,
                max_tokens: None,
                temperature: None,
            },
            stream: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        // None fields must be absent — not serialized as `null`.
        assert!(!json.contains("version"));
        assert!(!json.contains("system_prompt"));
        assert!(!json.contains("max_tokens"));
        assert!(!json.contains("temperature"));
        assert!(!json.contains("stream"));
        assert!(json.contains("\"prompt\":\"Say hi\""));
    }

    #[test]
    fn test_create_prediction_request_serializes_with_all_fields() {
        let req = CreatePredictionRequest {
            version: Some("sha256:abc".to_string()),
            input: ReplicateInput {
                prompt: "Hello".to_string(),
                system_prompt: Some("Be helpful".to_string()),
                max_tokens: Some(512),
                temperature: Some(0.7),
            },
            stream: Some(true),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["version"], "sha256:abc");
        assert_eq!(json["input"]["system_prompt"], "Be helpful");
        assert_eq!(json["input"]["max_tokens"], 512);
        assert!((json["input"]["temperature"].as_f64().unwrap() - 0.7).abs() < f64::EPSILON);
        assert_eq!(json["stream"], true);
    }
}
