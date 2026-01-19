//! Batch request processing for efficient LLM operations
//!
//! This module provides batching capabilities to process multiple LLM requests efficiently.
//! It collects requests over a time window and processes them together, reducing overhead.
//!
//! # Example
//!
//! ```rust
//! use oxify_connect_llm::{BatchConfig, BatchProvider, OpenAIProvider, LlmProvider, LlmRequest};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = OpenAIProvider::new("test-key".to_string(), "gpt-4".to_string());
//! let config = BatchConfig {
//!     max_batch_size: 10,
//!     max_wait_ms: 100,
//! };
//! let batch_provider = BatchProvider::new(provider, config);
//!
//! // Multiple concurrent requests will be batched automatically
//! let request = LlmRequest {
//!     prompt: "Hello".to_string(),
//!     system_prompt: None,
//!     temperature: None,
//!     max_tokens: None,
//!     tools: vec![],
//!     images: vec![],
//! };
//! // let response = batch_provider.complete(request).await?;
//! # Ok(())
//! # }
//! ```

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmProvider, LlmRequest, LlmResponse,
    Result,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::Duration;

/// Configuration for batch processing
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of requests to batch together
    pub max_batch_size: usize,
    /// Maximum time to wait for more requests (in milliseconds)
    pub max_wait_ms: u64,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 10,
            max_wait_ms: 100,
        }
    }
}

/// Statistics about batch processing
#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    /// Total number of batches processed
    pub batches_processed: usize,
    /// Total number of individual requests processed
    pub total_requests: usize,
    /// Average batch size
    pub avg_batch_size: f64,
    /// Number of timeouts (batch sent due to max_wait_ms)
    pub timeout_batches: usize,
    /// Number of full batches (batch sent due to max_batch_size)
    pub full_batches: usize,
}

impl BatchStats {
    fn update(&mut self, batch_size: usize, is_timeout: bool) {
        self.batches_processed += 1;
        self.total_requests += batch_size;
        self.avg_batch_size = self.total_requests as f64 / self.batches_processed as f64;
        if is_timeout {
            self.timeout_batches += 1;
        } else {
            self.full_batches += 1;
        }
    }
}

struct BatchRequest {
    request: LlmRequest,
    response_tx: oneshot::Sender<Result<LlmResponse>>,
}

struct BatchWorker<P> {
    provider: Arc<P>,
    config: BatchConfig,
    stats: Arc<Mutex<BatchStats>>,
    rx: mpsc::UnboundedReceiver<BatchRequest>,
}

impl<P: LlmProvider + 'static> BatchWorker<P> {
    async fn run(mut self) {
        let mut pending_requests: Vec<BatchRequest> = Vec::new();

        loop {
            // Wait for first request or process pending batch
            if pending_requests.is_empty() {
                match self.rx.recv().await {
                    Some(batch_req) => pending_requests.push(batch_req),
                    None => break, // Channel closed
                }
            }

            // Collect more requests up to max_batch_size or max_wait_ms
            let start = tokio::time::Instant::now();
            let max_wait = Duration::from_millis(self.config.max_wait_ms);

            while pending_requests.len() < self.config.max_batch_size {
                let remaining = max_wait.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    break;
                }

                match tokio::time::timeout(remaining, self.rx.recv()).await {
                    Ok(Some(batch_req)) => pending_requests.push(batch_req),
                    Ok(None) => break, // Channel closed
                    Err(_) => break,   // Timeout - process current batch
                }
            }

            // Process batch
            if !pending_requests.is_empty() {
                let batch_size = pending_requests.len();
                let is_timeout = batch_size < self.config.max_batch_size;

                // Update stats
                {
                    let mut stats = self.stats.lock().await;
                    stats.update(batch_size, is_timeout);
                }

                // Process each request (in parallel for efficiency)
                let provider = Arc::clone(&self.provider);
                let requests = std::mem::take(&mut pending_requests);

                tokio::spawn(async move {
                    for batch_req in requests {
                        let provider = Arc::clone(&provider);
                        let request = batch_req.request;
                        let response_tx = batch_req.response_tx;

                        tokio::spawn(async move {
                            let result = provider.complete(request).await;
                            let _ = response_tx.send(result);
                        });
                    }
                });
            }
        }
    }
}

/// Batch provider that wraps any LLM provider with batching capabilities
pub struct BatchProvider<P> {
    tx: mpsc::UnboundedSender<BatchRequest>,
    stats: Arc<Mutex<BatchStats>>,
    _phantom: std::marker::PhantomData<P>,
}

impl<P: LlmProvider + 'static> BatchProvider<P> {
    /// Create a new batch provider
    pub fn new(provider: P, config: BatchConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let stats = Arc::new(Mutex::new(BatchStats::default()));

        let worker = BatchWorker {
            provider: Arc::new(provider),
            config,
            stats: Arc::clone(&stats),
            rx,
        };

        tokio::spawn(worker.run());

        Self {
            tx,
            stats,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get current batch processing statistics
    pub async fn stats(&self) -> BatchStats {
        self.stats.lock().await.clone()
    }
}

#[async_trait]
impl<P: LlmProvider + 'static> LlmProvider for BatchProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let (response_tx, response_rx) = oneshot::channel();

        let batch_req = BatchRequest {
            request,
            response_tx,
        };

        self.tx
            .send(batch_req)
            .map_err(|_| crate::LlmError::Other("Batch worker has stopped".to_string()))?;

        response_rx
            .await
            .map_err(|_| crate::LlmError::Other("Response channel closed".to_string()))?
    }
}

// Embedding batch support
struct EmbeddingBatchRequest {
    request: EmbeddingRequest,
    response_tx: oneshot::Sender<Result<EmbeddingResponse>>,
}

struct EmbeddingBatchWorker<P> {
    provider: Arc<P>,
    config: BatchConfig,
    stats: Arc<Mutex<BatchStats>>,
    rx: mpsc::UnboundedReceiver<EmbeddingBatchRequest>,
}

impl<P: EmbeddingProvider + 'static> EmbeddingBatchWorker<P> {
    async fn run(mut self) {
        let mut pending_requests: Vec<EmbeddingBatchRequest> = Vec::new();

        loop {
            if pending_requests.is_empty() {
                match self.rx.recv().await {
                    Some(batch_req) => pending_requests.push(batch_req),
                    None => break,
                }
            }

            let start = tokio::time::Instant::now();
            let max_wait = Duration::from_millis(self.config.max_wait_ms);

            while pending_requests.len() < self.config.max_batch_size {
                let remaining = max_wait.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    break;
                }

                match tokio::time::timeout(remaining, self.rx.recv()).await {
                    Ok(Some(batch_req)) => pending_requests.push(batch_req),
                    Ok(None) => break,
                    Err(_) => break,
                }
            }

            if !pending_requests.is_empty() {
                let batch_size = pending_requests.len();
                let is_timeout = batch_size < self.config.max_batch_size;

                {
                    let mut stats = self.stats.lock().await;
                    stats.update(batch_size, is_timeout);
                }

                let provider = Arc::clone(&self.provider);
                let requests = std::mem::take(&mut pending_requests);

                tokio::spawn(async move {
                    for batch_req in requests {
                        let provider = Arc::clone(&provider);
                        let request = batch_req.request;
                        let response_tx = batch_req.response_tx;

                        tokio::spawn(async move {
                            let result = provider.embed(request).await;
                            let _ = response_tx.send(result);
                        });
                    }
                });
            }
        }
    }
}

/// Batch provider for embeddings
pub struct EmbeddingBatchProvider<P> {
    tx: mpsc::UnboundedSender<EmbeddingBatchRequest>,
    stats: Arc<Mutex<BatchStats>>,
    _phantom: std::marker::PhantomData<P>,
}

impl<P: EmbeddingProvider + 'static> EmbeddingBatchProvider<P> {
    /// Create a new embedding batch provider
    pub fn new(provider: P, config: BatchConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let stats = Arc::new(Mutex::new(BatchStats::default()));

        let worker = EmbeddingBatchWorker {
            provider: Arc::new(provider),
            config,
            stats: Arc::clone(&stats),
            rx,
        };

        tokio::spawn(worker.run());

        Self {
            tx,
            stats,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get current batch processing statistics
    pub async fn stats(&self) -> BatchStats {
        self.stats.lock().await.clone()
    }
}

#[async_trait]
impl<P: EmbeddingProvider + 'static> EmbeddingProvider for EmbeddingBatchProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let (response_tx, response_rx) = oneshot::channel();

        let batch_req = EmbeddingBatchRequest {
            request,
            response_tx,
        };

        self.tx
            .send(batch_req)
            .map_err(|_| crate::LlmError::Other("Batch worker has stopped".to_string()))?;

        response_rx
            .await
            .map_err(|_| crate::LlmError::Other("Response channel closed".to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmResponse, Usage};
    use tokio::time::sleep;

    struct MockProvider {
        delay_ms: u64,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
            if self.delay_ms > 0 {
                sleep(Duration::from_millis(self.delay_ms)).await;
            }
            Ok(LlmResponse {
                content: format!("Response to: {}", request.prompt),
                model: "mock-model".to_string(),
                usage: Some(Usage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                }),
                tool_calls: vec![],
            })
        }
    }

    #[tokio::test]
    async fn test_batch_config_default() {
        let config = BatchConfig::default();
        assert_eq!(config.max_batch_size, 10);
        assert_eq!(config.max_wait_ms, 100);
    }

    #[tokio::test]
    async fn test_batch_provider_single_request() {
        let provider = MockProvider { delay_ms: 0 };
        let config = BatchConfig {
            max_batch_size: 5,
            max_wait_ms: 50,
        };
        let batch_provider = BatchProvider::new(provider, config);

        let request = LlmRequest {
            prompt: "Hello".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        let response = batch_provider.complete(request).await.unwrap();
        assert_eq!(response.content, "Response to: Hello");
        assert_eq!(response.model, "mock-model");

        // Wait a bit for batch processing
        sleep(Duration::from_millis(100)).await;

        let stats = batch_provider.stats().await;
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.batches_processed, 1);
    }

    #[tokio::test]
    async fn test_batch_provider_multiple_requests() {
        let provider = MockProvider { delay_ms: 10 };
        let config = BatchConfig {
            max_batch_size: 3,
            max_wait_ms: 200,
        };
        let batch_provider = Arc::new(BatchProvider::new(provider, config));

        let mut handles = vec![];

        // Send 5 requests concurrently
        for i in 0..5 {
            let bp = Arc::clone(&batch_provider);
            let handle = tokio::spawn(async move {
                let request = LlmRequest {
                    prompt: format!("Request {}", i),
                    system_prompt: None,
                    temperature: None,
                    max_tokens: None,
                    tools: vec![],
                    images: vec![],
                };
                bp.complete(request).await
            });
            handles.push(handle);
        }

        // Wait for all requests to complete
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        // Wait for batch processing to settle
        sleep(Duration::from_millis(300)).await;

        let stats = batch_provider.stats().await;
        assert_eq!(stats.total_requests, 5);
        // Should have at least 2 batches (3 + 2)
        assert!(stats.batches_processed >= 2);
    }

    #[tokio::test]
    async fn test_batch_stats_calculation() {
        let provider = MockProvider { delay_ms: 0 };
        let config = BatchConfig {
            max_batch_size: 2,
            max_wait_ms: 50,
        };
        let batch_provider = Arc::new(BatchProvider::new(provider, config));

        // Send 4 requests (should create 2 batches of size 2)
        let mut handles = vec![];
        for i in 0..4 {
            let bp = Arc::clone(&batch_provider);
            let handle = tokio::spawn(async move {
                let request = LlmRequest {
                    prompt: format!("Request {}", i),
                    system_prompt: None,
                    temperature: None,
                    max_tokens: None,
                    tools: vec![],
                    images: vec![],
                };
                bp.complete(request).await
            });
            handles.push(handle);
            // Small delay to ensure batching
            if i == 1 {
                sleep(Duration::from_millis(10)).await;
            }
        }

        for handle in handles {
            let _ = handle.await.unwrap();
        }

        sleep(Duration::from_millis(200)).await;

        let stats = batch_provider.stats().await;
        assert_eq!(stats.total_requests, 4);
        assert!(stats.avg_batch_size > 0.0);
    }

    #[tokio::test]
    async fn test_batch_timeout_trigger() {
        let provider = MockProvider { delay_ms: 0 };
        let config = BatchConfig {
            max_batch_size: 10, // Large batch size
            max_wait_ms: 50,    // Short wait time
        };
        let batch_provider = BatchProvider::new(provider, config);

        // Send single request - should be processed after timeout
        let request = LlmRequest {
            prompt: "Single request".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        let response = batch_provider.complete(request).await.unwrap();
        assert_eq!(response.content, "Response to: Single request");

        sleep(Duration::from_millis(100)).await;

        let stats = batch_provider.stats().await;
        assert_eq!(stats.timeout_batches, 1);
        assert_eq!(stats.full_batches, 0);
    }
}
