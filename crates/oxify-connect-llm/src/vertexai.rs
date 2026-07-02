use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmChunk, LlmError, LlmProvider,
    LlmRequest, LlmResponse, LlmStream, Result, StreamUsage, StreamingLlmProvider, Usage,
};
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct VertexAiProvider {
    access_token: String,
    project_id: String,
    location: String,
    model: String,
    client: oxihttp::HttpsClient,
    base_url: String,
}

impl VertexAiProvider {
    pub fn new(access_token: String, project_id: String, location: String, model: String) -> Self {
        let base_url = format!("https://{location}-aiplatform.googleapis.com/v1");
        Self {
            access_token,
            project_id,
            location,
            model,
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for Vertex AI"),
            base_url,
        }
    }

    pub fn for_embeddings(access_token: String, project_id: String, location: String) -> Self {
        Self::new(
            access_token,
            project_id,
            location,
            "text-embedding-004".to_string(),
        )
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn from_env() -> Result<Self> {
        let access_token = std::env::var("GOOGLE_VERTEX_ACCESS_TOKEN")
            .map_err(|_| LlmError::ConfigError("GOOGLE_VERTEX_ACCESS_TOKEN not set".to_string()))?;
        let project_id = std::env::var("GOOGLE_VERTEX_PROJECT")
            .map_err(|_| LlmError::ConfigError("GOOGLE_VERTEX_PROJECT not set".to_string()))?;
        let location =
            std::env::var("GOOGLE_VERTEX_LOCATION").unwrap_or_else(|_| "us-central1".to_string());
        let model = std::env::var("GOOGLE_VERTEX_MODEL")
            .unwrap_or_else(|_| "gemini-1.5-flash-001".to_string());
        Ok(Self::new(access_token, project_id, location, model))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VxRequest {
    contents: Vec<VxContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<VxGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<VxSystemInstruction>,
}

#[derive(Serialize, Deserialize)]
struct VxContent {
    parts: Vec<VxPart>,
    #[serde(default)]
    role: String,
}

#[derive(Serialize, Deserialize)]
struct VxPart {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VxGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Serialize)]
struct VxSystemInstruction {
    parts: Vec<VxPart>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VxResponse {
    candidates: Vec<VxCandidate>,
    usage_metadata: Option<VxUsage>,
}

#[derive(Deserialize)]
struct VxCandidate {
    content: VxContent,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VxUsage {
    prompt_token_count: u32,
    candidates_token_count: u32,
    total_token_count: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VxStreamResponse {
    candidates: Vec<VxCandidate>,
    usage_metadata: Option<VxUsage>,
}

#[derive(Serialize)]
struct VxEmbedRequest {
    content: VxEmbedContent,
}

#[derive(Serialize)]
struct VxEmbedContent {
    parts: Vec<VxPart>,
}

#[derive(Deserialize)]
struct VxEmbedResponse {
    embedding: VxEmbedding,
}

#[derive(Deserialize)]
struct VxEmbedding {
    values: Vec<f32>,
}

#[async_trait]
impl LlmProvider for VertexAiProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let mut vx_request = VxRequest {
            contents: vec![VxContent {
                parts: vec![VxPart {
                    text: request.prompt.clone(),
                }],
                role: "user".to_string(),
            }],
            generation_config: Some(VxGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
            system_instruction: None,
        };

        if let Some(sys) = request.system_prompt {
            vx_request.system_instruction = Some(VxSystemInstruction {
                parts: vec![VxPart { text: sys }],
            });
        }

        let url = format!(
            "{}/projects/{}/locations/{}/publishers/google/models/{}:generateContent",
            self.base_url, self.project_id, self.location, self.model
        );

        let response = self
            .client
            .post(&url)?
            .header("Content-Type", "application/json")?
            .header("Authorization", &format!("Bearer {}", self.access_token))?
            .json(&vx_request)?
            .send()
            .await?;

        let status = response.status();

        if status.as_u16() == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(std::time::Duration::from_secs);
            return Err(LlmError::RateLimited(retry_after));
        }

        let body = response.body_text().await?;

        if !status.is_success() {
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let parsed = serde_json::from_str::<VxResponse>(&body)
            .map_err(|e| LlmError::SerializationError(e.to_string()))?;

        if parsed.candidates.is_empty() {
            return Err(LlmError::ApiError("No candidates in response".to_string()));
        }

        let content = &parsed.candidates[0].content;
        if content.parts.is_empty() {
            return Err(LlmError::ApiError("No parts in content".to_string()));
        }

        let usage = parsed.usage_metadata.map(|u| Usage {
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

#[async_trait]
impl StreamingLlmProvider for VertexAiProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let mut vx_request = VxRequest {
            contents: vec![VxContent {
                parts: vec![VxPart {
                    text: request.prompt.clone(),
                }],
                role: "user".to_string(),
            }],
            generation_config: Some(VxGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
            system_instruction: None,
        };

        if let Some(sys) = request.system_prompt {
            vx_request.system_instruction = Some(VxSystemInstruction {
                parts: vec![VxPart { text: sys }],
            });
        }

        let url = format!(
            "{}/projects/{}/locations/{}/publishers/google/models/{}:streamGenerateContent?alt=sse",
            self.base_url, self.project_id, self.location, self.model
        );

        let response = self
            .client
            .post(&url)?
            .header("Content-Type", "application/json")?
            .header("Authorization", &format!("Bearer {}", self.access_token))?
            .json(&vx_request)?
            .send()
            .await?;

        let status = response.status();

        if status.as_u16() == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(std::time::Duration::from_secs);
            return Err(LlmError::RateLimited(retry_after));
        }

        if !status.is_success() {
            let body = response.body_text().await?;
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let model_name = self.model.clone();
        let byte_stream = response.body_stream();

        let parsed_stream = byte_stream.filter_map(move |chunk_result| {
            let model = model_name.clone();
            async move {
                let bytes = chunk_result.ok()?;
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(resp) = serde_json::from_str::<VxStreamResponse>(data) {
                            let cand = resp.candidates.first()?;
                            let part = cand.content.parts.first()?;
                            let done = resp.usage_metadata.is_some();
                            let usage = resp.usage_metadata.map(|u| StreamUsage {
                                prompt_tokens: Some(u.prompt_token_count),
                                completion_tokens: Some(u.candidates_token_count),
                                total_tokens: Some(u.total_token_count),
                            });
                            return Some(Ok(LlmChunk {
                                content: part.text.clone(),
                                done,
                                model: if done { Some(model) } else { None },
                                usage,
                            }));
                        }
                    }
                }
                None
            }
        });

        Ok(Box::pin(parsed_stream))
    }
}

#[async_trait]
impl EmbeddingProvider for VertexAiProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let model = request.model.clone().unwrap_or_else(|| self.model.clone());
        let mut all_embeddings = Vec::new();

        for text in &request.texts {
            let embed_req = VxEmbedRequest {
                content: VxEmbedContent {
                    parts: vec![VxPart { text: text.clone() }],
                },
            };

            let url = format!(
                "{}/projects/{}/locations/{}/publishers/google/models/{}:embedContent",
                self.base_url, self.project_id, self.location, model
            );

            let response = self
                .client
                .post(&url)?
                .header("Content-Type", "application/json")?
                .header("Authorization", &format!("Bearer {}", self.access_token))?
                .json(&embed_req)?
                .send()
                .await?;

            let status = response.status();

            if status.as_u16() == 429 {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(std::time::Duration::from_secs);
                return Err(LlmError::RateLimited(retry_after));
            }

            let body = response.body_text().await?;

            if !status.is_success() {
                return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
            }

            let parsed = serde_json::from_str::<VxEmbedResponse>(&body)
                .map_err(|e| LlmError::SerializationError(e.to_string()))?;

            all_embeddings.push(parsed.embedding.values);
        }

        Ok(EmbeddingResponse {
            embeddings: all_embeddings,
            model,
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_new_default_base_url() {
        let p = VertexAiProvider::new(
            "tok".into(),
            "proj".into(),
            "us-central1".into(),
            "gemini-1.5-flash-001".into(),
        );
        assert!(p.base_url.contains("us-central1"));
        assert!(p.base_url.contains("aiplatform.googleapis.com"));
    }

    #[test]
    fn test_with_base_url_override() {
        let p = VertexAiProvider::new("t".into(), "p".into(), "us-central1".into(), "m".into())
            .with_base_url("http://localhost:9999".to_string());
        assert_eq!(p.base_url, "http://localhost:9999");
    }

    #[test]
    fn test_from_env_missing_token() {
        unsafe {
            std::env::remove_var("GOOGLE_VERTEX_ACCESS_TOKEN");
        }
        let result = VertexAiProvider::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::ConfigError(_)));
    }

    #[test]
    fn test_from_env_missing_project() {
        unsafe {
            std::env::set_var("GOOGLE_VERTEX_ACCESS_TOKEN", "tok");
            std::env::remove_var("GOOGLE_VERTEX_PROJECT");
        }
        let result = VertexAiProvider::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::ConfigError(_)));
        unsafe {
            std::env::remove_var("GOOGLE_VERTEX_ACCESS_TOKEN");
        }
    }

    #[test]
    fn test_url_generation() {
        let p = VertexAiProvider::new(
            "tok".into(),
            "my-project".into(),
            "us-east1".into(),
            "gemini-1.5-flash".into(),
        )
        .with_base_url("https://test.example.com/v1".to_string());
        let url = format!(
            "{}/projects/{}/locations/{}/publishers/google/models/{}:generateContent",
            p.base_url, p.project_id, p.location, p.model
        );
        assert!(url.contains("my-project"));
        assert!(url.contains("us-east1"));
        assert!(url.contains("gemini-1.5-flash"));
        assert!(url.ends_with(":generateContent"));
    }

    #[tokio::test]
    async fn test_complete_success() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "candidates": [{"content": {"parts": [{"text": "Hello!"}], "role": "model"}}],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 3,
                "totalTokenCount": 8
            }
        });
        Mock::given(method("POST"))
            .and(path_regex(r".*:generateContent$"))
            .and(header("authorization", "Bearer test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let provider = VertexAiProvider::new(
            "test_token".into(),
            "proj".into(),
            "us-central1".into(),
            "gemini-1.5-flash".into(),
        )
        .with_base_url(server.uri());
        let req = LlmRequest {
            prompt: "Say hi".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };
        let resp = provider.complete(req).await.unwrap();
        assert_eq!(resp.content, "Hello!");
        assert!(resp.usage.is_some());
    }

    #[tokio::test]
    async fn test_complete_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r".*:generateContent$"))
            .respond_with(ResponseTemplate::new(429).append_header("retry-after", "30"))
            .mount(&server)
            .await;
        let provider = VertexAiProvider::new(
            "tok".into(),
            "proj".into(),
            "us-central1".into(),
            "model".into(),
        )
        .with_base_url(server.uri());
        let req = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };
        let err = provider.complete(req).await.unwrap_err();
        assert!(matches!(err, LlmError::RateLimited(Some(_))));
    }

    #[tokio::test]
    async fn test_complete_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r".*:generateContent$"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;
        let provider = VertexAiProvider::new(
            "bad_token".into(),
            "proj".into(),
            "us-central1".into(),
            "model".into(),
        )
        .with_base_url(server.uri());
        let req = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };
        let err = provider.complete(req).await.unwrap_err();
        assert!(matches!(err, LlmError::ApiError(_)));
    }

    #[tokio::test]
    async fn test_embed_success() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "embedding": {"values": [0.1_f32, 0.2_f32, 0.3_f32]}
        });
        Mock::given(method("POST"))
            .and(path_regex(r".*:embedContent$"))
            .and(header("authorization", "Bearer embed_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let provider = VertexAiProvider::for_embeddings(
            "embed_token".into(),
            "proj".into(),
            "us-central1".into(),
        )
        .with_base_url(server.uri());
        let req = EmbeddingRequest {
            texts: vec!["hello world".to_string()],
            model: None,
        };
        let resp = provider.embed(req).await.unwrap();
        assert_eq!(resp.embeddings.len(), 1);
        assert_eq!(resp.embeddings[0].len(), 3);
    }
}
