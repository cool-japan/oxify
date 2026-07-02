//! LLM provider connections for OxiFY

mod aws_sigv4;
mod batch;
mod bedrock;
mod cache;
mod circuit_breaker;
mod cohere;
mod compression;
mod dedup;
mod errors;
mod fallback;
mod gemini;
mod health_check;
mod helpers;
mod huggingface;
mod interceptor;
mod llamacpp;
mod load_balancer;
mod mistral;
mod observability;
mod otel;
mod priority_queue;
mod prompt_engineering;
mod rate_limit;
mod recommender;
#[cfg(feature = "redis-cache")]
mod redis_budget;
#[cfg(feature = "redis-cache")]
mod redis_cache;
mod replicate;
mod response_utils;
mod retry;
mod sagemaker;
mod selector;
mod semantic_cache;
mod streaming;
mod templates;
mod timeout;
mod usage;
mod validation;
mod vertexai;
mod vllm;
mod workflow;

pub use aws_sigv4::AwsCredentials;
pub use batch::{BatchConfig, BatchProvider, BatchStats, EmbeddingBatchProvider};
pub use bedrock::BedrockProvider;
pub use cache::{CacheStats, CachedProvider, LlmCache};
pub use circuit_breaker::{CircuitBreakerConfig, CircuitBreakerProvider, CircuitState};
pub use cohere::CohereProvider;
pub use compression::{CompressionStats, ModelLimits, PromptCompressor};
pub use dedup::{DedupProvider, DedupStats};
pub use errors::{ContextualError, ErrorContext, ErrorContextBuilder, ErrorContextExt};
pub use fallback::FallbackProvider;
pub use gemini::GeminiProvider;
pub use health_check::{HealthCheckConfig, HealthCheckProvider, HealthStats, HealthStatus};
pub use helpers::{LlmRequestBuilder, ModelUtils, QuickRequest, TokenUtils};
pub use huggingface::HuggingFaceProvider;
pub use interceptor::{
    ContentLengthInterceptor, EmbeddingInterceptorProvider, EmbeddingRequestInterceptor,
    EmbeddingResponseInterceptor, InterceptorProvider, LoggingInterceptor, RequestInterceptor,
    ResponseInterceptor, SanitizationInterceptor,
};
pub use llamacpp::LlamaCppProvider;
pub use load_balancer::{LoadBalancer, LoadBalancerStats, LoadBalancingStrategy};
pub use mistral::MistralProvider;
pub use observability::{Metrics, MetricsProvider, ObservableProvider};
pub use otel::{
    OtelEmbeddingProvider, OtelProvider, ResponseAttributes, SpanAttributes, TraceEvent,
};
pub use priority_queue::{
    PriorityQueueConfig, PriorityQueueProvider, PriorityQueueStats, RequestPriority,
};
pub use prompt_engineering::{
    ChainOfThought, Example, FewShotPrompt, InstructionPrompt, Role, RolePrompt, SystemPrompts,
};
pub use rate_limit::{RateLimitConfig, RateLimitProvider, RateLimitStats};
pub use recommender::{
    AlternativeModel, BudgetConstraint, ModelRecommendation, ModelRecommender, OptimizationGoal,
    RecommendationRequest, UseCase,
};
#[cfg(feature = "redis-cache")]
pub use redis_budget::{RedisBudgetStats, RedisBudgetStore};
#[cfg(feature = "redis-cache")]
pub use redis_cache::{
    RedisCache, RedisCacheStats, RedisCachedEmbeddingProvider, RedisCachedProvider,
};
pub use replicate::ReplicateProvider;
pub use response_utils::{CodeBlock, ResponseUtils};
pub use retry::{RetryConfig, RetryProvider};
pub use sagemaker::SageMakerProvider;
pub use selector::{ProviderMetadata, ProviderSelector, SelectionCriteria};
pub use semantic_cache::{
    SemanticCache, SemanticCacheStats, SemanticCachedProvider, SimilarityThreshold,
};
pub use streaming::{LlmChunk, LlmStream, StreamUsage, StreamingLlmProvider};
pub use templates::{PromptTemplate, TemplateLibrary};
pub use timeout::{TimeoutConfig, TimeoutProvider};
pub use usage::{
    BudgetLimit, BudgetProvider, ModelPricing, TrackedProvider, UsageStats, UsageTracker,
};
pub use validation::{RequestValidator, ValidationRules};
pub use vertexai::VertexAiProvider;
pub use vllm::VllmProvider;
pub use workflow::{WorkflowEmbeddingProvider, WorkflowProvider, WorkflowStats, WorkflowTracker};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, LlmError>;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] oxihttp::OxiHttpError),

    #[error("Rate limited (retry after {0:?})")]
    RateLimited(Option<std::time::Duration>),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Request timed out after {0:?}")]
    Timeout(std::time::Duration),

    #[error("Other error: {0}")]
    Other(String),
}

impl Clone for LlmError {
    fn clone(&self) -> Self {
        match self {
            Self::ApiError(s) => Self::ApiError(s.clone()),
            Self::ConfigError(s) => Self::ConfigError(s.clone()),
            Self::SerializationError(s) => Self::SerializationError(s.clone()),
            Self::NetworkError(e) => Self::ApiError(format!("Network error: {}", e)),
            Self::RateLimited(d) => Self::RateLimited(*d),
            Self::InvalidRequest(s) => Self::InvalidRequest(s.clone()),
            Self::Timeout(d) => Self::Timeout(*d),
            Self::Other(s) => Self::Other(s.clone()),
        }
    }
}

impl LlmError {
    /// Get the suggested retry delay if this is a rate limit error
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        match self {
            LlmError::RateLimited(retry_after) => *retry_after,
            _ => None,
        }
    }
}

/// Tool/Function definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Tool/Function call made by the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Image input for vision models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInput {
    /// Image data (base64 encoded) or URL
    pub data: String,
    /// Image type: "url" or "base64"
    pub source_type: ImageSourceType,
    /// Media type (e.g., "image/png", "image/jpeg")
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageSourceType {
    Url,
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub images: Vec<ImageInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<Usage>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Trait for LLM providers
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse>;
}

// ===== Embedding Support =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub texts: Vec<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
    pub usage: Option<EmbeddingUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

/// Trait for embedding providers
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse>;
}

// ===== OpenAI Provider =====

/// OpenAI provider implementation
pub struct OpenAIProvider {
    api_key: String,
    model: String,
    client: oxihttp::HttpsClient,
    base_url: String,
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAITool>,
}

#[derive(Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunction,
}

#[derive(Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum OpenAIMessageContent {
    Text(String),
    Parts(Vec<OpenAIContentPart>),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum OpenAIContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenAIImageUrl },
}

#[derive(Serialize, Deserialize)]
struct OpenAIImageUrl {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: OpenAIMessageContent,
}

#[derive(Deserialize)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    tool_type: String,
    function: OpenAIFunctionCall,
}

#[derive(Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<Choice>,
    usage: OpenAIUsage,
    model: String,
}

#[derive(Deserialize)]
struct Choice {
    message: OpenAIResponseMessage,
}

#[derive(Deserialize)]
struct OpenAIResponseMessage {
    #[allow(dead_code)]
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAIToolCall>,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAIError {
    error: OpenAIErrorDetail,
}

#[derive(Deserialize)]
struct OpenAIErrorDetail {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
}

impl OpenAIProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client"),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Create a provider specifically for embeddings
    pub fn for_embeddings(api_key: String) -> Self {
        Self::new(api_key, "text-embedding-ada-002".to_string())
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let mut messages = Vec::new();

        // Add system message if provided
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: OpenAIMessageContent::Text(system_prompt.clone()),
            });
        }

        // Add user message (with images if provided)
        let user_content = if request.images.is_empty() {
            OpenAIMessageContent::Text(request.prompt.clone())
        } else {
            let mut parts = vec![OpenAIContentPart::Text {
                text: request.prompt.clone(),
            }];

            for image in &request.images {
                let url = match image.source_type {
                    ImageSourceType::Url => image.data.clone(),
                    ImageSourceType::Base64 => {
                        let media_type = image.media_type.as_deref().unwrap_or("image/jpeg");
                        format!("data:{};base64,{}", media_type, image.data)
                    }
                };

                parts.push(OpenAIContentPart::ImageUrl {
                    image_url: OpenAIImageUrl { url, detail: None },
                });
            }

            OpenAIMessageContent::Parts(parts)
        };

        messages.push(OpenAIMessage {
            role: "user".to_string(),
            content: user_content,
        });

        // Convert tools to OpenAI format
        let tools: Vec<OpenAITool> = request
            .tools
            .iter()
            .map(|t| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect();

        let openai_request = OpenAIRequest {
            model: self.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools,
        };

        let response = self
            .client
            .post(&format!("{}/chat/completions", self.base_url))?
            .header("Authorization", &format!("Bearer {}", self.api_key))?
            .header("Content-Type", "application/json")?
            .json(&openai_request)?
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
            // Try to parse error response
            if let Ok(error) = serde_json::from_str::<OpenAIError>(&body) {
                return Err(LlmError::ApiError(format!(
                    "{}: {}",
                    error.error.error_type, error.error.message
                )));
            }
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let openai_response: OpenAIResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        if openai_response.choices.is_empty() {
            return Err(LlmError::ApiError("No choices in response".to_string()));
        }

        let message = &openai_response.choices[0].message;

        // Parse tool calls if present
        let tool_calls: Vec<ToolCall> = message
            .tool_calls
            .iter()
            .filter_map(|tc| {
                serde_json::from_str(&tc.function.arguments)
                    .ok()
                    .map(|args| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: args,
                    })
            })
            .collect();

        Ok(LlmResponse {
            content: message.content.clone().unwrap_or_default(),
            model: openai_response.model,
            usage: Some(Usage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                completion_tokens: openai_response.usage.completion_tokens,
                total_tokens: openai_response.usage.total_tokens,
            }),
            tool_calls,
        })
    }
}

#[derive(Serialize)]
struct OpenAIEmbeddingRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Deserialize)]
struct OpenAIEmbeddingResponse {
    data: Vec<OpenAIEmbeddingData>,
    model: String,
    usage: OpenAIEmbeddingUsage,
}

#[derive(Deserialize)]
struct OpenAIEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct OpenAIEmbeddingUsage {
    prompt_tokens: u32,
    total_tokens: u32,
}

#[async_trait]
impl EmbeddingProvider for OpenAIProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let model = request.model.unwrap_or_else(|| self.model.clone());

        let openai_request = OpenAIEmbeddingRequest {
            input: request.texts,
            model,
        };

        let response = self
            .client
            .post(&format!("{}/embeddings", self.base_url))?
            .header("Authorization", &format!("Bearer {}", self.api_key))?
            .header("Content-Type", "application/json")?
            .json(&openai_request)?
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
            if let Ok(error) = serde_json::from_str::<OpenAIError>(&body) {
                return Err(LlmError::ApiError(format!(
                    "{}: {}",
                    error.error.error_type, error.error.message
                )));
            }
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let openai_response: OpenAIEmbeddingResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        // Sort by index to ensure correct order
        let mut data = openai_response.data;
        data.sort_by_key(|d| d.index);

        Ok(EmbeddingResponse {
            embeddings: data.into_iter().map(|d| d.embedding).collect(),
            model: openai_response.model,
            usage: Some(EmbeddingUsage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                total_tokens: openai_response.usage.total_tokens,
            }),
        })
    }
}

// ===== OpenAI Streaming Implementation =====

use futures::stream::StreamExt;

#[derive(Deserialize)]
struct OpenAIStreamChunk {
    choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
    model: String,
}

#[derive(Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIDelta {
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl StreamingLlmProvider for OpenAIProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let mut messages = Vec::new();

        if let Some(system_prompt) = &request.system_prompt {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: OpenAIMessageContent::Text(system_prompt.clone()),
            });
        }

        messages.push(OpenAIMessage {
            role: "user".to_string(),
            content: OpenAIMessageContent::Text(request.prompt.clone()),
        });

        // Note: Streaming with tools/vision is more complex, basic implementation without tool/vision support
        let openai_request = serde_json::json!({
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
            .json(&openai_request)?
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

                            if let Ok(chunk) = serde_json::from_str::<OpenAIStreamChunk>(data) {
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

// ===== Anthropic Provider =====

/// Anthropic (Claude) provider implementation
pub struct AnthropicProvider {
    api_key: String,
    model: String,
    client: oxihttp::HttpsClient,
    base_url: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AnthropicMessageContent {
    Text(String),
    Blocks(Vec<AnthropicInputBlock>),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicInputBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: AnthropicImageSource },
}

#[derive(Serialize, Deserialize)]
struct AnthropicImageSource {
    #[serde(rename = "type")]
    source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicMessageContent,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    usage: AnthropicUsage,
    model: String,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
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
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client"),
            base_url: "https://api.anthropic.com/v1".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Convert tools to Anthropic format
        let tools: Vec<AnthropicTool> = request
            .tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect();

        // Build user message content (with images if provided)
        let user_content = if request.images.is_empty() {
            AnthropicMessageContent::Text(request.prompt.clone())
        } else {
            let mut blocks = vec![AnthropicInputBlock::Text {
                text: request.prompt.clone(),
            }];

            for image in &request.images {
                let source = match image.source_type {
                    ImageSourceType::Url => AnthropicImageSource {
                        source_type: "url".to_string(),
                        media_type: None,
                        data: None,
                        url: Some(image.data.clone()),
                    },
                    ImageSourceType::Base64 => AnthropicImageSource {
                        source_type: "base64".to_string(),
                        media_type: image.media_type.clone(),
                        data: Some(image.data.clone()),
                        url: None,
                    },
                };

                blocks.push(AnthropicInputBlock::Image { source });
            }

            AnthropicMessageContent::Blocks(blocks)
        };

        let anthropic_request = AnthropicRequest {
            model: self.model.clone(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: user_content,
            }],
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature,
            system: request.system_prompt,
            tools,
        };

        let response = self
            .client
            .post(&format!("{}/messages", self.base_url))?
            .header("x-api-key", &self.api_key)?
            .header("anthropic-version", "2023-06-01")?
            .header("Content-Type", "application/json")?
            .json(&anthropic_request)?
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

        let anthropic_response: AnthropicResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        if anthropic_response.content.is_empty() {
            return Err(LlmError::ApiError("No content in response".to_string()));
        }

        // Extract text content and tool calls
        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        for block in anthropic_response.content {
            match block {
                AnthropicContentBlock::Text { text } => {
                    if !text_content.is_empty() {
                        text_content.push('\n');
                    }
                    text_content.push_str(&text);
                }
                AnthropicContentBlock::ToolUse { id, name, input } => {
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
            model: anthropic_response.model,
            usage: Some(Usage {
                prompt_tokens: anthropic_response.usage.input_tokens,
                completion_tokens: anthropic_response.usage.output_tokens,
                total_tokens: anthropic_response.usage.input_tokens
                    + anthropic_response.usage.output_tokens,
            }),
            tool_calls,
        })
    }
}

// ===== Anthropic Streaming Implementation =====

use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Deserialize)]
#[serde(tag = "type")]
enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: AnthropicStreamMessage },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { delta: AnthropicDelta },
    #[serde(rename = "message_delta")]
    MessageDelta {
        #[allow(dead_code)]
        delta: AnthropicStopDelta,
        usage: AnthropicUsage,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct AnthropicStreamMessage {
    model: String,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum AnthropicDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct AnthropicStopDelta {
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Clone)]
struct AnthropicStreamState {
    model: Option<String>,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

#[async_trait]
impl StreamingLlmProvider for AnthropicProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let anthropic_request = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": request.prompt}],
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "temperature": request.temperature,
            "system": request.system_prompt,
            "stream": true
        });

        let response = self
            .client
            .post(&format!("{}/messages", self.base_url))?
            .header("x-api-key", &self.api_key)?
            .header("anthropic-version", "2023-06-01")?
            .header("Content-Type", "application/json")?
            .json(&anthropic_request)?
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
        let state = Arc::new(Mutex::new(AnthropicStreamState {
            model: None,
            input_tokens: None,
            output_tokens: None,
        }));

        let parsed_stream = stream.filter_map(move |chunk_result| {
            let state = Arc::clone(&state);
            async move {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        for line in text.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if let Ok(event) =
                                    serde_json::from_str::<AnthropicStreamEvent>(data)
                                {
                                    match event {
                                        AnthropicStreamEvent::MessageStart { message } => {
                                            let mut s = state.lock().await;
                                            s.model = Some(message.model);
                                            s.input_tokens = Some(message.usage.input_tokens);
                                        }
                                        AnthropicStreamEvent::ContentBlockDelta { delta } => {
                                            if let AnthropicDelta::TextDelta { text } = delta {
                                                return Some(Ok(LlmChunk {
                                                    content: text,
                                                    done: false,
                                                    model: None,
                                                    usage: None,
                                                }));
                                            }
                                        }
                                        AnthropicStreamEvent::MessageDelta { usage, .. } => {
                                            let mut s = state.lock().await;
                                            s.output_tokens = Some(usage.output_tokens);
                                        }
                                        AnthropicStreamEvent::MessageStop => {
                                            let s = state.lock().await;
                                            let usage = match (s.input_tokens, s.output_tokens) {
                                                (Some(input), Some(output)) => Some(StreamUsage {
                                                    prompt_tokens: Some(input),
                                                    completion_tokens: Some(output),
                                                    total_tokens: Some(input + output),
                                                }),
                                                _ => None,
                                            };

                                            return Some(Ok(LlmChunk {
                                                content: String::new(),
                                                done: true,
                                                model: s.model.clone(),
                                                usage,
                                            }));
                                        }
                                        AnthropicStreamEvent::Other => {}
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

// ===== Ollama Provider =====

/// Ollama (local model) provider implementation
pub struct OllamaProvider {
    model: String,
    client: oxihttp::HttpsClient,
    base_url: String,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
    model: String,
    #[serde(default)]
    #[allow(dead_code)]
    done: bool,
}

#[derive(Deserialize)]
struct OllamaStreamResponse {
    response: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    done: bool,
}

impl OllamaProvider {
    pub fn new(model: String) -> Self {
        Self {
            model,
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client"),
            base_url: "http://localhost:11434".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Create a provider specifically for embeddings (e.g., "nomic-embed-text")
    pub fn for_embeddings(model: String) -> Self {
        Self::new(model)
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let ollama_request = OllamaRequest {
            model: self.model.clone(),
            prompt: request.prompt.clone(),
            system: request.system_prompt,
            temperature: request.temperature,
            stream: false,
        };

        let response = self
            .client
            .post(&format!("{}/api/generate", self.base_url))?
            .header("Content-Type", "application/json")?
            .json(&ollama_request)?
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.body_text().await?;
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let body = response.body_text().await?;
        let ollama_response: OllamaResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::SerializationError(e.to_string()))?;

        Ok(LlmResponse {
            content: ollama_response.response,
            model: ollama_response.model,
            usage: None, // Ollama doesn't provide token usage by default
            tool_calls: Vec::new(),
        })
    }
}

// ===== Ollama Streaming Implementation =====

#[async_trait]
impl StreamingLlmProvider for OllamaProvider {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        let ollama_request = OllamaRequest {
            model: self.model.clone(),
            prompt: request.prompt.clone(),
            system: request.system_prompt,
            temperature: request.temperature,
            stream: true,
        };

        let response = self
            .client
            .post(&format!("{}/api/generate", self.base_url))?
            .header("Content-Type", "application/json")?
            .json(&ollama_request)?
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.body_text().await?;
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let stream = response.body_stream();

        let parsed_stream = stream.filter_map(|chunk_result| async move {
            match chunk_result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    // Ollama returns newline-delimited JSON
                    for line in text.lines() {
                        if line.trim().is_empty() {
                            continue;
                        }

                        if let Ok(chunk) = serde_json::from_str::<OllamaStreamResponse>(line) {
                            if chunk.done {
                                // Final chunk - includes model name
                                return Some(Ok(LlmChunk {
                                    content: chunk.response,
                                    done: true,
                                    model: chunk.model,
                                    usage: None, // Ollama doesn't provide token usage in stream
                                }));
                            } else {
                                // Content chunk
                                return Some(Ok(LlmChunk {
                                    content: chunk.response,
                                    done: false,
                                    model: None,
                                    usage: None,
                                }));
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

// ===== Ollama Embeddings Implementation =====

#[derive(Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let model = request.model.unwrap_or_else(|| self.model.clone());

        let mut embeddings = Vec::with_capacity(request.texts.len());

        // Ollama embeddings API processes one text at a time
        for text in &request.texts {
            let ollama_request = OllamaEmbeddingRequest {
                model: model.clone(),
                prompt: text.clone(),
            };

            let response = self
                .client
                .post(&format!("{}/api/embeddings", self.base_url))?
                .header("Content-Type", "application/json")?
                .json(&ollama_request)?
                .send()
                .await?;

            let status = response.status();

            if !status.is_success() {
                let body = response.body_text().await?;
                return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
            }

            let body = response.body_text().await?;
            let ollama_response: OllamaEmbeddingResponse = serde_json::from_str(&body)
                .map_err(|e| LlmError::SerializationError(e.to_string()))?;

            embeddings.push(ollama_response.embedding);
        }

        Ok(EmbeddingResponse {
            embeddings,
            model,
            usage: None, // Ollama doesn't provide token usage for embeddings
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OpenAIProvider::new("test_key".to_string(), "gpt-4".to_string());
        assert_eq!(provider.model, "gpt-4");
        assert_eq!(provider.base_url, "https://api.openai.com/v1");

        let provider =
            AnthropicProvider::new("test_key".to_string(), "claude-3-opus-20240229".to_string());
        assert_eq!(provider.model, "claude-3-opus-20240229");

        let ollama_provider = OllamaProvider::new("llama2".to_string());
        assert_eq!(ollama_provider.model, "llama2");
        assert_eq!(ollama_provider.base_url, "http://localhost:11434");
    }
}
