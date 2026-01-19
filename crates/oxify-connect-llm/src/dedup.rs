//! Request deduplication for LLM providers.
//!
//! Prevents duplicate in-flight requests by caching ongoing requests and sharing results
//! with all waiting callers. This is useful for preventing expensive duplicate API calls
//! when multiple callers request the same thing simultaneously.
//!
//! # Example
//!
//! ```rust,no_run
//! use oxify_connect_llm::{DedupProvider, LlmProvider, LlmRequest};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let provider: Box<dyn LlmProvider> = todo!();
//! let dedup = DedupProvider::new(provider);
//!
//! let request = LlmRequest {
//!     prompt: "Hello, world!".to_string(),
//!     system_prompt: None,
//!     temperature: None,
//!     max_tokens: None,
//!     tools: Vec::new(),
//!     images: Vec::new(),
//! };
//!
//! // If multiple callers make the same request simultaneously,
//! // only one actual API call will be made
//! let response = dedup.complete(request).await?;
//! # Ok(())
//! # }
//! ```

use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse};
use async_trait::async_trait;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::{Mutex, RwLock};

/// Request key for deduplication
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct RequestKey {
    prompt: String,
    system_prompt: Option<String>,
    temperature_str: String,
    max_tokens: Option<u32>,
}

impl From<&LlmRequest> for RequestKey {
    fn from(request: &LlmRequest) -> Self {
        // Convert temperature to string to avoid floating point comparison issues
        let temperature_str = request
            .temperature
            .map(|t| format!("{:.6}", t))
            .unwrap_or_default();

        Self {
            prompt: request.prompt.clone(),
            system_prompt: request.system_prompt.clone(),
            temperature_str,
            max_tokens: request.max_tokens,
        }
    }
}

/// In-flight request tracking
struct InFlightRequest {
    tx: broadcast::Sender<Result<LlmResponse, LlmError>>,
}

/// Request deduplication provider
pub struct DedupProvider {
    provider: Box<dyn LlmProvider>,
    in_flight: Arc<RwLock<HashMap<RequestKey, Arc<Mutex<InFlightRequest>>>>>,
}

impl DedupProvider {
    /// Create a new deduplication provider
    pub fn new(provider: Box<dyn LlmProvider>) -> Self {
        Self {
            provider,
            in_flight: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get statistics about in-flight requests
    pub async fn get_stats(&self) -> DedupStats {
        let in_flight = self.in_flight.read().await;
        DedupStats {
            in_flight_count: in_flight.len(),
        }
    }

    /// Clear all in-flight requests (useful for testing)
    #[allow(dead_code)]
    pub async fn clear(&self) {
        let mut in_flight = self.in_flight.write().await;
        in_flight.clear();
    }
}

/// Deduplication statistics
#[derive(Debug, Clone)]
pub struct DedupStats {
    /// Number of in-flight requests
    pub in_flight_count: usize,
}

#[async_trait]
impl LlmProvider for DedupProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let key = RequestKey::from(&request);

        // Check if there's an in-flight request
        {
            let in_flight = self.in_flight.read().await;
            if let Some(inflight) = in_flight.get(&key) {
                // Subscribe to the existing request
                let inflight = inflight.lock().await;
                let mut rx = inflight.tx.subscribe();
                drop(inflight);
                drop(in_flight);

                // Wait for the result
                return match rx.recv().await {
                    Ok(result) => result,
                    Err(_) => Err(LlmError::Other(
                        "Failed to receive result from in-flight request".to_string(),
                    )),
                };
            }
        }

        // Create a new in-flight request
        let (tx, _) = broadcast::channel(16);
        let inflight = Arc::new(Mutex::new(InFlightRequest { tx: tx.clone() }));

        {
            let mut in_flight = self.in_flight.write().await;
            in_flight.insert(key.clone(), inflight);
        }

        // Make the actual request
        let result = self.provider.complete(request).await;

        // Broadcast the result to all waiting subscribers
        let _ = tx.send(result.clone());

        // Remove from in-flight requests
        {
            let mut in_flight = self.in_flight.write().await;
            in_flight.remove(&key);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Usage;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct MockProvider {
        call_count: Arc<AtomicU32>,
        delay_ms: u64,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);

            // Simulate API delay
            if self.delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
            }

            Ok(LlmResponse {
                content: "Success".to_string(),
                model: "mock".to_string(),
                usage: Some(Usage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                }),
                tool_calls: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn test_dedup_single_request() {
        let call_count = Arc::new(AtomicU32::new(0));
        let mock = MockProvider {
            call_count: Arc::clone(&call_count),
            delay_ms: 0,
        };

        let dedup = DedupProvider::new(Box::new(mock));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let result = dedup.complete(request).await;
        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_dedup_duplicate_requests() {
        let call_count = Arc::new(AtomicU32::new(0));
        let mock = MockProvider {
            call_count: Arc::clone(&call_count),
            delay_ms: 100, // Delay to ensure concurrent requests
        };

        let dedup = Arc::new(DedupProvider::new(Box::new(mock)));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        // Spawn 5 concurrent identical requests
        let mut handles = Vec::new();
        for _ in 0..5 {
            let dedup = Arc::clone(&dedup);
            let request = request.clone();
            handles.push(tokio::spawn(async move { dedup.complete(request).await }));
        }

        // Wait for all requests to complete
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        // Should only have made 1 actual API call
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_dedup_different_requests() {
        let call_count = Arc::new(AtomicU32::new(0));
        let mock = MockProvider {
            call_count: Arc::clone(&call_count),
            delay_ms: 50,
        };

        let dedup = Arc::new(DedupProvider::new(Box::new(mock)));

        // Make 3 different requests
        let request1 = LlmRequest {
            prompt: "test1".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let request2 = LlmRequest {
            prompt: "test2".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let request3 = LlmRequest {
            prompt: "test3".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        // Spawn concurrent requests
        let dedup1 = Arc::clone(&dedup);
        let dedup2 = Arc::clone(&dedup);
        let dedup3 = Arc::clone(&dedup);

        let handle1 = tokio::spawn(async move { dedup1.complete(request1).await });
        let handle2 = tokio::spawn(async move { dedup2.complete(request2).await });
        let handle3 = tokio::spawn(async move { dedup3.complete(request3).await });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();
        let result3 = handle3.await.unwrap();

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert!(result3.is_ok());

        // Should have made 3 API calls (one for each unique request)
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_dedup_stats() {
        let call_count = Arc::new(AtomicU32::new(0));
        let mock = MockProvider {
            call_count: Arc::clone(&call_count),
            delay_ms: 100,
        };

        let dedup = Arc::new(DedupProvider::new(Box::new(mock)));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        // Spawn a request in the background
        let dedup_clone = Arc::clone(&dedup);
        let request_clone = request.clone();
        let _handle = tokio::spawn(async move { dedup_clone.complete(request_clone).await });

        // Give it a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Check stats - should show 1 in-flight request
        let stats = dedup.get_stats().await;
        assert_eq!(stats.in_flight_count, 1);

        // Wait for completion
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Check stats again - should be 0
        let stats = dedup.get_stats().await;
        assert_eq!(stats.in_flight_count, 0);
    }

    #[tokio::test]
    async fn test_dedup_temperature_distinction() {
        let call_count = Arc::new(AtomicU32::new(0));
        let mock = MockProvider {
            call_count: Arc::clone(&call_count),
            delay_ms: 50,
        };

        let dedup = Arc::new(DedupProvider::new(Box::new(mock)));

        // Same prompt but different temperatures should be different requests
        let request1 = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let request2 = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: Some(0.9),
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let dedup1 = Arc::clone(&dedup);
        let dedup2 = Arc::clone(&dedup);

        let handle1 = tokio::spawn(async move { dedup1.complete(request1).await });
        let handle2 = tokio::spawn(async move { dedup2.complete(request2).await });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // Should have made 2 API calls (different temperatures)
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }
}
