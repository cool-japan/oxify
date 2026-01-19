//! Circuit Breaker pattern for LLM providers.
//!
//! Prevents cascading failures by temporarily disabling providers that are experiencing
//! too many failures. The circuit breaker has three states:
//!
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Too many failures detected, requests fail fast
//! - **Half-Open**: Testing if the provider has recovered
//!
//! # Example
//!
//! ```rust,no_run
//! use oxify_connect_llm::{CircuitBreakerProvider, CircuitBreakerConfig};
//! # use oxify_connect_llm::{LlmProvider, LlmRequest};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let provider: Box<dyn LlmProvider> = todo!();
//! let config = CircuitBreakerConfig::default()
//!     .with_failure_threshold(5)
//!     .with_recovery_timeout_secs(60);
//!
//! let circuit_breaker = CircuitBreakerProvider::new(provider, config);
//!
//! let request = LlmRequest {
//!     prompt: "Hello, world!".to_string(),
//!     system_prompt: None,
//!     temperature: None,
//!     max_tokens: None,
//!     tools: Vec::new(),
//!     images: Vec::new(),
//! };
//! let response = circuit_breaker.complete(request).await?;
//! # Ok(())
//! # }
//! ```

use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed,
    /// Too many failures - fail fast
    Open,
    /// Testing recovery - allow one request through
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit (default: 5)
    pub failure_threshold: u32,
    /// Time to wait before attempting recovery (default: 30s)
    pub recovery_timeout: Duration,
    /// Number of successful requests to close the circuit from half-open (default: 2)
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

impl CircuitBreakerConfig {
    /// Set the failure threshold
    pub fn with_failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Set the recovery timeout in seconds
    pub fn with_recovery_timeout_secs(mut self, secs: u64) -> Self {
        self.recovery_timeout = Duration::from_secs(secs);
        self
    }

    /// Set the success threshold
    pub fn with_success_threshold(mut self, threshold: u32) -> Self {
        self.success_threshold = threshold;
        self
    }
}

/// Internal circuit breaker state
#[derive(Debug)]
struct CircuitBreakerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
}

impl CircuitBreakerState {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
        }
    }

    fn record_success(&mut self, config: &CircuitBreakerConfig) {
        match self.state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= config.success_threshold {
                    // Close the circuit after enough successes
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                    self.last_failure_time = None;
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle it
                self.state = CircuitState::Closed;
                self.failure_count = 0;
                self.success_count = 0;
                self.last_failure_time = None;
            }
        }
    }

    fn record_failure(&mut self, config: &CircuitBreakerConfig) {
        self.last_failure_time = Some(Instant::now());

        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= config.failure_threshold {
                    // Open the circuit after too many failures
                    self.state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                // Failed during recovery - reopen the circuit
                self.state = CircuitState::Open;
                self.success_count = 0;
            }
            CircuitState::Open => {
                // Already open, no action needed
            }
        }
    }

    fn check_and_update_state(&mut self, config: &CircuitBreakerConfig) -> CircuitState {
        if self.state == CircuitState::Open {
            if let Some(last_failure) = self.last_failure_time {
                if last_failure.elapsed() >= config.recovery_timeout {
                    // Try to recover - move to half-open
                    self.state = CircuitState::HalfOpen;
                    self.success_count = 0;
                }
            }
        }
        self.state
    }
}

/// Circuit breaker wrapper for LLM providers
pub struct CircuitBreakerProvider {
    provider: Box<dyn LlmProvider>,
    config: CircuitBreakerConfig,
    state: Arc<Mutex<CircuitBreakerState>>,
}

impl CircuitBreakerProvider {
    /// Create a new circuit breaker provider with default config
    pub fn new(provider: Box<dyn LlmProvider>, config: CircuitBreakerConfig) -> Self {
        Self {
            provider,
            config,
            state: Arc::new(Mutex::new(CircuitBreakerState::new())),
        }
    }

    /// Get the current circuit state
    pub async fn get_state(&self) -> CircuitState {
        self.state.lock().await.state
    }

    /// Get failure statistics
    pub async fn get_stats(&self) -> (CircuitState, u32, u32) {
        let state = self.state.lock().await;
        (state.state, state.failure_count, state.success_count)
    }

    /// Manually reset the circuit breaker
    pub async fn reset(&self) {
        let mut state = self.state.lock().await;
        state.state = CircuitState::Closed;
        state.failure_count = 0;
        state.success_count = 0;
        state.last_failure_time = None;
    }
}

#[async_trait]
impl LlmProvider for CircuitBreakerProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        // Check if circuit should be opened or half-opened
        let current_state = {
            let mut state = self.state.lock().await;
            state.check_and_update_state(&self.config)
        };

        // Fail fast if circuit is open
        if current_state == CircuitState::Open {
            return Err(LlmError::Other(
                "Circuit breaker is open - provider temporarily disabled".to_string(),
            ));
        }

        // Try the request
        match self.provider.complete(request).await {
            Ok(response) => {
                // Record success
                self.state.lock().await.record_success(&self.config);
                Ok(response)
            }
            Err(err) => {
                // Record failure
                self.state.lock().await.record_failure(&self.config);
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmResponse, Usage};

    struct MockProvider {
        fail_count: Arc<Mutex<u32>>,
        max_failures: u32,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
            let mut count = self.fail_count.lock().await;
            *count += 1;

            if *count <= self.max_failures {
                Err(LlmError::RateLimited(None))
            } else {
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
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let mock = MockProvider {
            fail_count: Arc::new(Mutex::new(0)),
            max_failures: 10,
        };

        let config = CircuitBreakerConfig::default().with_failure_threshold(3);
        let circuit = CircuitBreakerProvider::new(Box::new(mock), config);

        // First 3 requests should fail and trigger the circuit
        for _ in 0..3 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = circuit.complete(request).await;
        }

        assert_eq!(circuit.get_state().await, CircuitState::Open);

        // Next request should fail fast
        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };
        let result = circuit.complete(request).await;
        assert!(matches!(result, Err(LlmError::Other(_))));
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovers() {
        let mock = MockProvider {
            fail_count: Arc::new(Mutex::new(0)),
            max_failures: 3,
        };

        let config = CircuitBreakerConfig::default()
            .with_failure_threshold(3)
            .with_recovery_timeout_secs(1)
            .with_success_threshold(2);

        let circuit = CircuitBreakerProvider::new(Box::new(mock), config);

        // Trigger circuit opening
        for _ in 0..3 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = circuit.complete(request).await;
        }

        assert_eq!(circuit.get_state().await, CircuitState::Open);

        // Wait for recovery timeout
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Next request should move to half-open
        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };
        let result = circuit.complete(request).await;
        assert!(result.is_ok()); // Should succeed now

        // One more success to close the circuit
        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };
        let result = circuit.complete(request).await;
        assert!(result.is_ok());

        assert_eq!(circuit.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_reset() {
        let mock = MockProvider {
            fail_count: Arc::new(Mutex::new(0)),
            max_failures: 10,
        };

        let config = CircuitBreakerConfig::default().with_failure_threshold(2);
        let circuit = CircuitBreakerProvider::new(Box::new(mock), config);

        // Trigger circuit opening
        for _ in 0..2 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = circuit.complete(request).await;
        }

        assert_eq!(circuit.get_state().await, CircuitState::Open);

        // Manual reset
        circuit.reset().await;

        assert_eq!(circuit.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_config_builder() {
        let config = CircuitBreakerConfig::default()
            .with_failure_threshold(10)
            .with_recovery_timeout_secs(60)
            .with_success_threshold(3);

        assert_eq!(config.failure_threshold, 10);
        assert_eq!(config.recovery_timeout, Duration::from_secs(60));
        assert_eq!(config.success_threshold, 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker_stats() {
        let mock = MockProvider {
            fail_count: Arc::new(Mutex::new(0)),
            max_failures: 10,
        };

        let config = CircuitBreakerConfig::default().with_failure_threshold(3);
        let circuit = CircuitBreakerProvider::new(Box::new(mock), config);

        // Record 2 failures
        for _ in 0..2 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = circuit.complete(request).await;
        }

        let (state, failure_count, _) = circuit.get_stats().await;
        assert_eq!(state, CircuitState::Closed);
        assert_eq!(failure_count, 2);
    }
}
