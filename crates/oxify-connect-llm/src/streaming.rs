//! Streaming LLM support with Server-Sent Events (SSE)

use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::{LlmRequest, Result};

/// A chunk of streamed LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmChunk {
    /// The text content of this chunk
    pub content: String,

    /// Whether this is the final chunk
    pub done: bool,

    /// Model identifier (only in final chunk)
    pub model: Option<String>,

    /// Usage statistics (only in final chunk)
    pub usage: Option<StreamUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamUsage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// Type alias for streaming response
pub type LlmStream = Pin<Box<dyn Stream<Item = Result<LlmChunk>> + Send>>;

/// Trait for LLM providers that support streaming
#[async_trait]
pub trait StreamingLlmProvider: Send + Sync {
    /// Stream completion responses token-by-token
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream>;
}
