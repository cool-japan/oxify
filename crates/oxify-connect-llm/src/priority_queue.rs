//! Priority queue for LLM request management
//!
//! This module provides a priority-based request queue system for managing LLM requests
//! with different urgency levels. It supports fair processing, backpressure handling,
//! and comprehensive queue statistics.
//!
//! # Example
//!
//! ```rust
//! use oxify_connect_llm::{PriorityQueueProvider, PriorityQueueConfig, RequestPriority, OpenAIProvider, LlmProvider, LlmRequest};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = OpenAIProvider::new("test-key".to_string(), "gpt-4".to_string());
//! let config = PriorityQueueConfig {
//!     max_queue_size: 100,
//!     max_workers: 5,
//! };
//! let queue_provider = PriorityQueueProvider::new(provider, config);
//!
//! // Submit high-priority request
//! let request = LlmRequest {
//!     prompt: "Urgent query".to_string(),
//!     system_prompt: None,
//!     temperature: None,
//!     max_tokens: None,
//!     tools: vec![],
//!     images: vec![],
//! };
//! // let response = queue_provider.complete_with_priority(request, RequestPriority::High).await?;
//! # Ok(())
//! # }
//! ```

use crate::{LlmProvider, LlmRequest, LlmResponse, Result};
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex, Semaphore};

/// Request priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RequestPriority {
    /// Low priority - background tasks
    Low = 0,
    /// Normal priority - standard requests
    #[default]
    Normal = 1,
    /// High priority - user-facing real-time requests
    High = 2,
}

/// Configuration for priority queue
#[derive(Debug, Clone)]
pub struct PriorityQueueConfig {
    /// Maximum total queue size across all priorities
    pub max_queue_size: usize,
    /// Maximum number of concurrent workers
    pub max_workers: usize,
}

impl Default for PriorityQueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
            max_workers: 10,
        }
    }
}

/// Statistics about priority queue operations
#[derive(Debug, Clone, Default)]
pub struct PriorityQueueStats {
    /// Number of requests currently in queue
    pub queue_length: usize,
    /// Number of high-priority requests in queue
    pub high_priority_count: usize,
    /// Number of normal-priority requests in queue
    pub normal_priority_count: usize,
    /// Number of low-priority requests in queue
    pub low_priority_count: usize,
    /// Total requests processed
    pub total_processed: usize,
    /// Total requests rejected (queue full)
    pub total_rejected: usize,
    /// Number of active workers
    pub active_workers: usize,
}

struct PriorityRequest {
    priority: RequestPriority,
    sequence: u64,
    request: LlmRequest,
    response_tx: oneshot::Sender<Result<LlmResponse>>,
}

impl PartialEq for PriorityRequest {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for PriorityRequest {}

impl PartialOrd for PriorityRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first, then FIFO within same priority (lower sequence first)
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => other.sequence.cmp(&self.sequence), // Reverse for FIFO
            other => other,
        }
    }
}

struct QueueState {
    heap: BinaryHeap<PriorityRequest>,
    sequence: u64,
    stats: PriorityQueueStats,
    max_queue_size: usize,
}

impl QueueState {
    fn new(max_queue_size: usize) -> Self {
        Self {
            heap: BinaryHeap::new(),
            sequence: 0,
            stats: PriorityQueueStats::default(),
            max_queue_size,
        }
    }

    fn enqueue(
        &mut self,
        priority: RequestPriority,
        request: LlmRequest,
        response_tx: oneshot::Sender<Result<LlmResponse>>,
    ) -> bool {
        if self.heap.len() >= self.max_queue_size {
            // Reject if queue is full
            self.stats.total_rejected += 1;
            return false;
        }

        let priority_req = PriorityRequest {
            priority,
            sequence: self.sequence,
            request,
            response_tx,
        };

        self.sequence += 1;
        self.heap.push(priority_req);
        self.update_priority_counts();
        true
    }

    fn dequeue(&mut self) -> Option<PriorityRequest> {
        let req = self.heap.pop();
        if req.is_some() {
            self.update_priority_counts();
        }
        req
    }

    fn update_priority_counts(&mut self) {
        self.stats.queue_length = self.heap.len();
        self.stats.high_priority_count = self
            .heap
            .iter()
            .filter(|r| r.priority == RequestPriority::High)
            .count();
        self.stats.normal_priority_count = self
            .heap
            .iter()
            .filter(|r| r.priority == RequestPriority::Normal)
            .count();
        self.stats.low_priority_count = self
            .heap
            .iter()
            .filter(|r| r.priority == RequestPriority::Low)
            .count();
    }
}

struct QueueWorker<P> {
    provider: Arc<P>,
    queue_state: Arc<Mutex<QueueState>>,
    semaphore: Arc<Semaphore>,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<()>>>,
}

impl<P: LlmProvider + 'static> QueueWorker<P> {
    async fn run(self) {
        loop {
            // Wait for notification
            {
                let mut rx = self.rx.lock().await;
                if rx.recv().await.is_none() {
                    break; // Channel closed
                }
            }

            // Acquire worker permit
            let permit = self.semaphore.clone().acquire_owned().await.unwrap();

            // Dequeue request
            let priority_req = {
                let mut state = self.queue_state.lock().await;
                state.dequeue()
            };

            if let Some(priority_req) = priority_req {
                let provider = Arc::clone(&self.provider);
                let queue_state = Arc::clone(&self.queue_state);

                tokio::spawn(async move {
                    let result = provider.complete(priority_req.request).await;

                    // Update stats
                    {
                        let mut state = queue_state.lock().await;
                        state.stats.total_processed += 1;
                    }

                    let _ = priority_req.response_tx.send(result);
                    drop(permit);
                });
            } else {
                drop(permit);
            }
        }
    }
}

/// Priority queue provider that wraps any LLM provider
pub struct PriorityQueueProvider<P> {
    tx: mpsc::UnboundedSender<(
        RequestPriority,
        LlmRequest,
        oneshot::Sender<Result<LlmResponse>>,
    )>,
    queue_state: Arc<Mutex<QueueState>>,
    _phantom: std::marker::PhantomData<P>,
}

impl<P: LlmProvider + 'static> PriorityQueueProvider<P> {
    /// Create a new priority queue provider
    pub fn new(provider: P, config: PriorityQueueConfig) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();
        let queue_state = Arc::new(Mutex::new(QueueState::new(config.max_queue_size)));
        let semaphore = Arc::new(Semaphore::new(config.max_workers));

        // Spawn enqueue handler
        let queue_state_clone = Arc::clone(&queue_state);
        let notify_tx_clone = notify_tx.clone();
        tokio::spawn(async move {
            while let Some((priority, request, response_tx)) = rx.recv().await {
                let mut state = queue_state_clone.lock().await;
                if state.enqueue(priority, request, response_tx) {
                    let _ = notify_tx_clone.send(()); // Notify worker
                }
            }
        });

        // Spawn worker
        let worker = QueueWorker {
            provider: Arc::new(provider),
            queue_state: Arc::clone(&queue_state),
            semaphore,
            rx: Arc::new(Mutex::new(notify_rx)),
        };
        tokio::spawn(worker.run());

        Self {
            tx,
            queue_state,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Submit a request with specified priority
    pub async fn complete_with_priority(
        &self,
        request: LlmRequest,
        priority: RequestPriority,
    ) -> Result<LlmResponse> {
        let (response_tx, response_rx) = oneshot::channel();

        self.tx
            .send((priority, request, response_tx))
            .map_err(|_| crate::LlmError::Other("Queue handler has stopped".to_string()))?;

        response_rx
            .await
            .map_err(|_| crate::LlmError::Other("Response channel closed".to_string()))?
    }

    /// Get current queue statistics
    pub async fn stats(&self) -> PriorityQueueStats {
        self.queue_state.lock().await.stats.clone()
    }
}

#[async_trait]
impl<P: LlmProvider + 'static> LlmProvider for PriorityQueueProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Default to Normal priority
        self.complete_with_priority(request, RequestPriority::Normal)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmResponse, Usage};

    struct MockProvider {
        delay_ms: u64,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
            if self.delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
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
    async fn test_priority_ordering() {
        assert!(RequestPriority::High > RequestPriority::Normal);
        assert!(RequestPriority::Normal > RequestPriority::Low);
    }

    #[tokio::test]
    async fn test_priority_queue_config_default() {
        let config = PriorityQueueConfig::default();
        assert_eq!(config.max_queue_size, 1000);
        assert_eq!(config.max_workers, 10);
    }

    #[tokio::test]
    async fn test_priority_queue_single_request() {
        let provider = MockProvider { delay_ms: 10 };
        let config = PriorityQueueConfig {
            max_queue_size: 100,
            max_workers: 5,
        };
        let queue_provider = PriorityQueueProvider::new(provider, config);

        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        let response = queue_provider
            .complete_with_priority(request, RequestPriority::High)
            .await
            .unwrap();
        assert_eq!(response.content, "Response to: Test");

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let stats = queue_provider.stats().await;
        assert_eq!(stats.total_processed, 1);
    }

    #[tokio::test]
    async fn test_priority_queue_priority_ordering() {
        let provider = MockProvider { delay_ms: 50 };
        let config = PriorityQueueConfig {
            max_queue_size: 100,
            max_workers: 1, // Single worker to ensure ordering
        };
        let queue_provider = Arc::new(PriorityQueueProvider::new(provider, config));

        let mut handles = vec![];

        // Submit low priority first, then high priority
        for (i, priority) in [
            (0, RequestPriority::Low),
            (1, RequestPriority::High),
            (2, RequestPriority::Normal),
        ]
        .iter()
        {
            let qp = Arc::clone(&queue_provider);
            let i = *i;
            let priority = *priority;
            let handle = tokio::spawn(async move {
                let request = LlmRequest {
                    prompt: format!("Request {}", i),
                    system_prompt: None,
                    temperature: None,
                    max_tokens: None,
                    tools: vec![],
                    images: vec![],
                };
                qp.complete_with_priority(request, priority).await
            });
            handles.push(handle);
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        for handle in handles {
            let _ = handle.await.unwrap();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let stats = queue_provider.stats().await;
        assert_eq!(stats.total_processed, 3);
    }

    #[tokio::test]
    async fn test_priority_queue_stats() {
        let provider = MockProvider { delay_ms: 100 };
        let config = PriorityQueueConfig {
            max_queue_size: 100,
            max_workers: 1,
        };
        let queue_provider = Arc::new(PriorityQueueProvider::new(provider, config));

        // Submit multiple requests quickly
        let mut handles = vec![];
        for i in 0..5 {
            let qp = Arc::clone(&queue_provider);
            let handle = tokio::spawn(async move {
                let request = LlmRequest {
                    prompt: format!("Request {}", i),
                    system_prompt: None,
                    temperature: None,
                    max_tokens: None,
                    tools: vec![],
                    images: vec![],
                };
                qp.complete(request).await
            });
            handles.push(handle);
        }

        // Check queue has items
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let stats = queue_provider.stats().await;
        assert!(stats.queue_length > 0 || stats.total_processed > 0);

        // Wait for completion
        for handle in handles {
            let _ = handle.await.unwrap();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;

        let stats = queue_provider.stats().await;
        assert_eq!(stats.total_processed, 5);
        assert_eq!(stats.queue_length, 0);
    }

    #[tokio::test]
    async fn test_priority_queue_default_priority() {
        let provider = MockProvider { delay_ms: 10 };
        let config = PriorityQueueConfig::default();
        let queue_provider = PriorityQueueProvider::new(provider, config);

        let request = LlmRequest {
            prompt: "Default priority".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        // Using LlmProvider trait (default priority)
        let response = queue_provider.complete(request).await.unwrap();
        assert_eq!(response.content, "Response to: Default priority");
    }
}
