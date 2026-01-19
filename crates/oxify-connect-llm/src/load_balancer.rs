//! Load balancer for distributing requests across multiple LLM providers.
//!
//! Supports multiple load balancing strategies:
//! - **Round Robin**: Distributes requests evenly across providers
//! - **Random**: Randomly selects a provider for each request
//! - **Weighted**: Distributes based on configured weights
//!
//! # Example
//!
//! ```rust,no_run
//! use oxify_connect_llm::{LoadBalancer, LoadBalancingStrategy, LlmProvider};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let provider1: Box<dyn LlmProvider> = todo!();
//! # let provider2: Box<dyn LlmProvider> = todo!();
//! # let provider3: Box<dyn LlmProvider> = todo!();
//! // Create load balancer with multiple providers
//! let lb = LoadBalancer::new(vec![provider1, provider2, provider3])
//!     .with_strategy(LoadBalancingStrategy::RoundRobin);
//!
//! // Requests will be distributed across all providers
//! # Ok(())
//! # }
//! ```

use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Load balancing strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalancingStrategy {
    /// Round-robin distribution
    RoundRobin,
    /// Random selection
    Random,
    /// Weighted distribution (requires weights to be set)
    Weighted,
}

/// Load balancer that distributes requests across multiple providers
pub struct LoadBalancer {
    providers: Vec<ProviderWithWeight>,
    strategy: LoadBalancingStrategy,
    counter: Arc<AtomicUsize>,
}

struct ProviderWithWeight {
    provider: Box<dyn LlmProvider>,
    weight: u32,
}

impl LoadBalancer {
    /// Create a new load balancer with default round-robin strategy
    pub fn new(providers: Vec<Box<dyn LlmProvider>>) -> Self {
        if providers.is_empty() {
            panic!("LoadBalancer requires at least one provider");
        }

        let providers_with_weight = providers
            .into_iter()
            .map(|p| ProviderWithWeight {
                provider: p,
                weight: 1,
            })
            .collect();

        Self {
            providers: providers_with_weight,
            strategy: LoadBalancingStrategy::RoundRobin,
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create a load balancer with weighted providers
    pub fn with_weights(providers: Vec<(Box<dyn LlmProvider>, u32)>) -> Self {
        if providers.is_empty() {
            panic!("LoadBalancer requires at least one provider");
        }

        let providers_with_weight = providers
            .into_iter()
            .map(|(p, w)| ProviderWithWeight {
                provider: p,
                weight: w,
            })
            .collect();

        Self {
            providers: providers_with_weight,
            strategy: LoadBalancingStrategy::Weighted,
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Set the load balancing strategy
    pub fn with_strategy(mut self, strategy: LoadBalancingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Get the number of providers
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Get load balancer statistics
    pub fn get_stats(&self) -> LoadBalancerStats {
        let total_weight: u32 = self.providers.iter().map(|p| p.weight).sum();
        LoadBalancerStats {
            provider_count: self.providers.len(),
            strategy: self.strategy,
            total_weight,
            request_count: self.counter.load(Ordering::SeqCst),
        }
    }

    /// Select a provider based on the load balancing strategy
    fn select_provider(&self) -> &dyn LlmProvider {
        match self.strategy {
            LoadBalancingStrategy::RoundRobin => {
                let index = self.counter.fetch_add(1, Ordering::SeqCst);
                &*self.providers[index % self.providers.len()].provider
            }
            LoadBalancingStrategy::Random => {
                // Use counter with a multiplier for pseudo-random distribution
                let index = self.counter.fetch_add(1, Ordering::SeqCst);
                // Use a large prime multiplier to get better distribution
                let pseudo_random = index.wrapping_mul(2654435761);
                &*self.providers[pseudo_random % self.providers.len()].provider
            }
            LoadBalancingStrategy::Weighted => {
                let total_weight: u32 = self.providers.iter().map(|p| p.weight).sum();
                let counter = self.counter.fetch_add(1, Ordering::SeqCst);
                // Use modulo for weighted selection
                let mut target_weight = (counter as u32) % total_weight;

                for provider in &self.providers {
                    if target_weight < provider.weight {
                        return &*provider.provider;
                    }
                    target_weight -= provider.weight;
                }

                // Fallback (should never happen)
                &*self.providers[0].provider
            }
        }
    }
}

/// Load balancer statistics
#[derive(Debug, Clone)]
pub struct LoadBalancerStats {
    /// Number of providers in the pool
    pub provider_count: usize,
    /// Current load balancing strategy
    pub strategy: LoadBalancingStrategy,
    /// Total weight (for weighted strategy)
    pub total_weight: u32,
    /// Total number of requests processed
    pub request_count: usize,
}

#[async_trait]
impl LlmProvider for LoadBalancer {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let provider = self.select_provider();
        provider.complete(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Usage;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct MockProvider {
        id: u32,
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(LlmResponse {
                content: format!("Response from provider {}", self.id),
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
    async fn test_load_balancer_round_robin() {
        let count1 = Arc::new(AtomicU32::new(0));
        let count2 = Arc::new(AtomicU32::new(0));
        let count3 = Arc::new(AtomicU32::new(0));

        let provider1 = MockProvider {
            id: 1,
            call_count: Arc::clone(&count1),
        };
        let provider2 = MockProvider {
            id: 2,
            call_count: Arc::clone(&count2),
        };
        let provider3 = MockProvider {
            id: 3,
            call_count: Arc::clone(&count3),
        };

        let lb = LoadBalancer::new(vec![
            Box::new(provider1),
            Box::new(provider2),
            Box::new(provider3),
        ])
        .with_strategy(LoadBalancingStrategy::RoundRobin);

        // Make 9 requests - should be evenly distributed
        for _ in 0..9 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = lb.complete(request).await;
        }

        // Each provider should have received 3 requests
        assert_eq!(count1.load(Ordering::SeqCst), 3);
        assert_eq!(count2.load(Ordering::SeqCst), 3);
        assert_eq!(count3.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_load_balancer_random() {
        let count1 = Arc::new(AtomicU32::new(0));
        let count2 = Arc::new(AtomicU32::new(0));

        let provider1 = MockProvider {
            id: 1,
            call_count: Arc::clone(&count1),
        };
        let provider2 = MockProvider {
            id: 2,
            call_count: Arc::clone(&count2),
        };

        let lb = LoadBalancer::new(vec![Box::new(provider1), Box::new(provider2)])
            .with_strategy(LoadBalancingStrategy::Random);

        // Make many requests - both providers should receive some
        for _ in 0..100 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = lb.complete(request).await;
        }

        let total = count1.load(Ordering::SeqCst) + count2.load(Ordering::SeqCst);
        assert_eq!(total, 100);

        // Both should have received at least some requests (statistically)
        assert!(count1.load(Ordering::SeqCst) > 0);
        assert!(count2.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn test_load_balancer_weighted() {
        let count1 = Arc::new(AtomicU32::new(0));
        let count2 = Arc::new(AtomicU32::new(0));

        let provider1 = MockProvider {
            id: 1,
            call_count: Arc::clone(&count1),
        };
        let provider2 = MockProvider {
            id: 2,
            call_count: Arc::clone(&count2),
        };

        // Provider 1 has weight 3, provider 2 has weight 1
        // So provider 1 should get ~75% of requests
        let lb =
            LoadBalancer::with_weights(vec![(Box::new(provider1), 3), (Box::new(provider2), 1)]);

        // Make many requests
        for _ in 0..1000 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = lb.complete(request).await;
        }

        let total = count1.load(Ordering::SeqCst) + count2.load(Ordering::SeqCst);
        assert_eq!(total, 1000);

        let count1_val = count1.load(Ordering::SeqCst);
        let count2_val = count2.load(Ordering::SeqCst);

        // Provider 1 should get roughly 75% of requests (allow some variance)
        assert!(
            count1_val > 650 && count1_val < 850,
            "count1: {}",
            count1_val
        );
        assert!(
            count2_val > 150 && count2_val < 350,
            "count2: {}",
            count2_val
        );
    }

    #[tokio::test]
    async fn test_load_balancer_stats() {
        let provider1 = MockProvider {
            id: 1,
            call_count: Arc::new(AtomicU32::new(0)),
        };
        let provider2 = MockProvider {
            id: 2,
            call_count: Arc::new(AtomicU32::new(0)),
        };

        let lb = LoadBalancer::new(vec![Box::new(provider1), Box::new(provider2)])
            .with_strategy(LoadBalancingStrategy::RoundRobin);

        let stats = lb.get_stats();
        assert_eq!(stats.provider_count, 2);
        assert_eq!(stats.strategy, LoadBalancingStrategy::RoundRobin);
        assert_eq!(stats.total_weight, 2); // Both providers have weight 1

        // Make some requests
        for _ in 0..10 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = lb.complete(request).await;
        }

        let stats = lb.get_stats();
        assert_eq!(stats.request_count, 10);
    }

    #[test]
    fn test_load_balancer_provider_count() {
        let provider1 = MockProvider {
            id: 1,
            call_count: Arc::new(AtomicU32::new(0)),
        };
        let provider2 = MockProvider {
            id: 2,
            call_count: Arc::new(AtomicU32::new(0)),
        };
        let provider3 = MockProvider {
            id: 3,
            call_count: Arc::new(AtomicU32::new(0)),
        };

        let lb = LoadBalancer::new(vec![
            Box::new(provider1),
            Box::new(provider2),
            Box::new(provider3),
        ]);

        assert_eq!(lb.provider_count(), 3);
    }
}
