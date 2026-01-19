//! Health checking and monitoring for LLM providers.
//!
//! Automatically monitors provider health based on success/failure rates and
//! disables unhealthy providers to prevent cascading failures.
//!
//! # Example
//!
//! ```rust,no_run
//! use oxify_connect_llm::{HealthCheckProvider, HealthCheckConfig, LlmProvider};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let provider: Box<dyn LlmProvider> = todo!();
//! let config = HealthCheckConfig::new()
//!     .with_failure_threshold(0.3) // Disable if 30% failure rate
//!     .with_check_window(100);      // Over last 100 requests
//!
//! let health_checked = HealthCheckProvider::new(provider, config);
//! # Ok(())
//! # }
//! ```

use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Failure rate threshold to mark provider as unhealthy (0.0-1.0)
    pub failure_threshold: f64,
    /// Number of recent requests to consider for health calculation
    pub check_window: usize,
    /// Minimum number of requests before health check is active
    pub min_requests: usize,
    /// How long to wait before re-enabling an unhealthy provider
    pub recovery_timeout: Duration,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthCheckConfig {
    /// Create a new health check configuration with defaults
    pub fn new() -> Self {
        Self {
            failure_threshold: 0.5, // 50% failure rate
            check_window: 50,
            min_requests: 10,
            recovery_timeout: Duration::from_secs(60),
        }
    }

    /// Set the failure threshold (0.0-1.0)
    pub fn with_failure_threshold(mut self, threshold: f64) -> Self {
        self.failure_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Set the check window size
    pub fn with_check_window(mut self, window: usize) -> Self {
        self.check_window = window.max(1);
        self
    }

    /// Set the minimum requests before health checking
    pub fn with_min_requests(mut self, min: usize) -> Self {
        self.min_requests = min;
        self
    }

    /// Set the recovery timeout in seconds
    pub fn with_recovery_timeout_secs(mut self, secs: u64) -> Self {
        self.recovery_timeout = Duration::from_secs(secs);
        self
    }
}

/// Health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Provider is healthy
    Healthy,
    /// Provider is degraded (approaching threshold)
    Degraded,
    /// Provider is unhealthy (disabled)
    Unhealthy,
}

/// Request outcome for health tracking
#[derive(Debug, Clone, Copy)]
enum RequestOutcome {
    Success,
    Failure,
}

/// Health check state
#[derive(Debug)]
struct HealthCheckState {
    outcomes: VecDeque<RequestOutcome>,
    status: HealthStatus,
    last_failure_time: Option<Instant>,
    total_requests: u64,
    total_failures: u64,
}

impl HealthCheckState {
    fn new() -> Self {
        Self {
            outcomes: VecDeque::new(),
            status: HealthStatus::Healthy,
            last_failure_time: None,
            total_requests: 0,
            total_failures: 0,
        }
    }

    fn record_outcome(&mut self, outcome: RequestOutcome, config: &HealthCheckConfig) {
        self.total_requests += 1;

        if matches!(outcome, RequestOutcome::Failure) {
            self.total_failures += 1;
            self.last_failure_time = Some(Instant::now());
        }

        // Add to window
        self.outcomes.push_back(outcome);

        // Maintain window size
        while self.outcomes.len() > config.check_window {
            self.outcomes.pop_front();
        }

        // Update health status
        self.update_status(config);
    }

    fn update_status(&mut self, config: &HealthCheckConfig) {
        // Need minimum requests before health checking
        if self.outcomes.len() < config.min_requests {
            self.status = HealthStatus::Healthy;
            return;
        }

        let failure_count = self
            .outcomes
            .iter()
            .filter(|o| matches!(o, RequestOutcome::Failure))
            .count();

        let failure_rate = failure_count as f64 / self.outcomes.len() as f64;

        if failure_rate >= config.failure_threshold {
            self.status = HealthStatus::Unhealthy;
        } else if failure_rate >= config.failure_threshold * 0.7 {
            // 70% of threshold = degraded
            self.status = HealthStatus::Degraded;
        } else {
            self.status = HealthStatus::Healthy;
        }
    }

    fn get_stats(&self) -> (HealthStatus, f64, u64, u64) {
        let failure_rate = if self.outcomes.is_empty() {
            0.0
        } else {
            self.outcomes
                .iter()
                .filter(|o| matches!(o, RequestOutcome::Failure))
                .count() as f64
                / self.outcomes.len() as f64
        };

        (
            self.status,
            failure_rate,
            self.total_requests,
            self.total_failures,
        )
    }
}

/// Health check provider wrapper
pub struct HealthCheckProvider {
    provider: Box<dyn LlmProvider>,
    state: Arc<Mutex<HealthCheckState>>,
    config: HealthCheckConfig,
}

impl HealthCheckProvider {
    /// Create a new health-checked provider
    pub fn new(provider: Box<dyn LlmProvider>, config: HealthCheckConfig) -> Self {
        Self {
            provider,
            state: Arc::new(Mutex::new(HealthCheckState::new())),
            config,
        }
    }

    /// Get current health status
    pub async fn get_status(&self) -> HealthStatus {
        self.state.lock().await.status
    }

    /// Get health statistics
    pub async fn get_stats(&self) -> HealthStats {
        let state = self.state.lock().await;
        let (status, failure_rate, total_requests, total_failures) = state.get_stats();

        HealthStats {
            status,
            failure_rate,
            total_requests,
            total_failures,
            is_healthy: status != HealthStatus::Unhealthy,
        }
    }

    /// Manually reset health state
    pub async fn reset(&self) {
        let mut state = self.state.lock().await;
        *state = HealthCheckState::new();
    }
}

/// Health statistics
#[derive(Debug, Clone)]
pub struct HealthStats {
    /// Current health status
    pub status: HealthStatus,
    /// Current failure rate (0.0-1.0)
    pub failure_rate: f64,
    /// Total requests processed
    pub total_requests: u64,
    /// Total failures
    pub total_failures: u64,
    /// Whether the provider is currently healthy
    pub is_healthy: bool,
}

#[async_trait]
impl LlmProvider for HealthCheckProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        // Make the request (health check is informational only, doesn't block)
        let result = self.provider.complete(request).await;

        // Record outcome for health monitoring
        {
            let mut state = self.state.lock().await;
            let outcome = if result.is_ok() {
                RequestOutcome::Success
            } else {
                RequestOutcome::Failure
            };
            state.record_outcome(outcome, &self.config);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Usage;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct MockProvider {
        call_count: Arc<AtomicU32>,
        fail_until: u32,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);

            if count < self.fail_until {
                Err(LlmError::ApiError("Simulated failure".to_string()))
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
    async fn test_health_check_becomes_unhealthy() {
        let mock = MockProvider {
            call_count: Arc::new(AtomicU32::new(0)),
            fail_until: 20, // Fail first 20 requests
        };

        let config = HealthCheckConfig::new()
            .with_failure_threshold(0.5)
            .with_check_window(20)
            .with_min_requests(10);

        let health_checked = HealthCheckProvider::new(Box::new(mock), config);

        // Make 20 failing requests
        for _ in 0..20 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = health_checked.complete(request).await;
        }

        let status = health_checked.get_status().await;
        assert_eq!(status, HealthStatus::Unhealthy);

        let stats = health_checked.get_stats().await;
        assert!(!stats.is_healthy);
        assert!(stats.failure_rate > 0.9); // Should be 100% failures
    }

    #[tokio::test]
    async fn test_health_check_recovers() {
        let mock = MockProvider {
            call_count: Arc::new(AtomicU32::new(0)),
            fail_until: 15, // Fail first 15, then succeed
        };

        let config = HealthCheckConfig::new()
            .with_failure_threshold(0.5)
            .with_check_window(20)
            .with_min_requests(10)
            .with_recovery_timeout_secs(1);

        let health_checked = HealthCheckProvider::new(Box::new(mock), config);

        // Make 15 failing requests
        for _ in 0..15 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = health_checked.complete(request).await;
        }

        assert_eq!(health_checked.get_status().await, HealthStatus::Unhealthy);

        // Wait for recovery timeout
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Make 15 successful requests (need more than min_requests to ensure recovery)
        for _ in 0..15 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let result = health_checked.complete(request).await;
            // Should succeed after recovery timeout
            assert!(result.is_ok());
        }

        // Should be healthy again after successful requests
        let status = health_checked.get_status().await;
        assert_eq!(status, HealthStatus::Healthy);

        let stats = health_checked.get_stats().await;
        assert!(stats.is_healthy);
        // Window is 20, so we have last 5 failures + 15 successes = 25% failure rate
        assert!(stats.failure_rate < 0.5); // Should be below threshold now
    }

    #[tokio::test]
    async fn test_health_check_config() {
        let config = HealthCheckConfig::new()
            .with_failure_threshold(0.3)
            .with_check_window(100)
            .with_min_requests(20)
            .with_recovery_timeout_secs(120);

        assert_eq!(config.failure_threshold, 0.3);
        assert_eq!(config.check_window, 100);
        assert_eq!(config.min_requests, 20);
        assert_eq!(config.recovery_timeout, Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_health_check_degraded_status() {
        let mock = MockProvider {
            call_count: Arc::new(AtomicU32::new(0)),
            fail_until: 6, // Fail 6 out of 20 = 30% (degraded at 70% of 50% = 35%)
        };

        let config = HealthCheckConfig::new()
            .with_failure_threshold(0.5)
            .with_check_window(20)
            .with_min_requests(10);

        let health_checked = HealthCheckProvider::new(Box::new(mock), config);

        // Make 20 requests (6 failures, 14 successes = 30% failure rate)
        for _ in 0..20 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = health_checked.complete(request).await;
        }

        let status = health_checked.get_status().await;
        // 30% failure rate should be below degraded threshold (35%)
        assert_eq!(status, HealthStatus::Healthy);

        let stats = health_checked.get_stats().await;
        assert!(stats.failure_rate < 0.35);
    }

    #[tokio::test]
    async fn test_health_check_reset() {
        let mock = MockProvider {
            call_count: Arc::new(AtomicU32::new(0)),
            fail_until: 20,
        };

        let config = HealthCheckConfig::new()
            .with_failure_threshold(0.5)
            .with_check_window(20)
            .with_min_requests(10);

        let health_checked = HealthCheckProvider::new(Box::new(mock), config);

        // Make failing requests
        for _ in 0..20 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            let _ = health_checked.complete(request).await;
        }

        assert_eq!(health_checked.get_status().await, HealthStatus::Unhealthy);

        // Manual reset
        health_checked.reset().await;

        assert_eq!(health_checked.get_status().await, HealthStatus::Healthy);
    }
}
