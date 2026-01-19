//! Rate limiting for LLM providers.
//!
//! Prevents hitting API rate limits by throttling requests based on configured limits.
//! Supports both request-based and token-based rate limiting with a token bucket algorithm.
//!
//! # Example
//!
//! ```rust,no_run
//! use oxify_connect_llm::{RateLimitProvider, RateLimitConfig, LlmProvider};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let provider: Box<dyn LlmProvider> = todo!();
//! // Limit to 60 requests per minute
//! let config = RateLimitConfig::new()
//!     .with_requests_per_minute(60)
//!     .with_tokens_per_minute(100_000);
//!
//! let rate_limited = RateLimitProvider::new(provider, config);
//! # Ok(())
//! # }
//! ```

use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per minute (0 = unlimited)
    pub requests_per_minute: u32,
    /// Maximum tokens per minute (0 = unlimited)
    pub tokens_per_minute: u32,
    /// Refill rate for token bucket (requests per second)
    refill_rate: f64,
    /// Refill rate for tokens (tokens per second)
    token_refill_rate: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimitConfig {
    /// Create a new rate limit configuration with no limits
    pub fn new() -> Self {
        Self {
            requests_per_minute: 0,
            tokens_per_minute: 0,
            refill_rate: 0.0,
            token_refill_rate: 0.0,
        }
    }

    /// Set the maximum requests per minute
    pub fn with_requests_per_minute(mut self, rpm: u32) -> Self {
        self.requests_per_minute = rpm;
        self.refill_rate = rpm as f64 / 60.0; // Convert to per second
        self
    }

    /// Set the maximum tokens per minute
    pub fn with_tokens_per_minute(mut self, tpm: u32) -> Self {
        self.tokens_per_minute = tpm;
        self.token_refill_rate = tpm as f64 / 60.0; // Convert to per second
        self
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity: capacity as f64,
            tokens: capacity as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let new_tokens = elapsed * self.refill_rate;

        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    fn try_acquire(&mut self, count: f64) -> bool {
        self.refill();

        if self.tokens >= count {
            self.tokens -= count;
            true
        } else {
            false
        }
    }

    fn wait_time(&mut self, count: f64) -> Duration {
        self.refill();

        if self.tokens >= count {
            Duration::from_secs(0)
        } else {
            let deficit = count - self.tokens;
            let wait_secs = deficit / self.refill_rate;
            Duration::from_secs_f64(wait_secs)
        }
    }
}

/// Rate limiter state
#[derive(Debug)]
struct RateLimiterState {
    request_bucket: Option<TokenBucket>,
    token_bucket: Option<TokenBucket>,
}

impl RateLimiterState {
    fn new(config: &RateLimitConfig) -> Self {
        let request_bucket = if config.requests_per_minute > 0 {
            Some(TokenBucket::new(
                config.requests_per_minute,
                config.refill_rate,
            ))
        } else {
            None
        };

        let token_bucket = if config.tokens_per_minute > 0 {
            Some(TokenBucket::new(
                config.tokens_per_minute,
                config.token_refill_rate,
            ))
        } else {
            None
        };

        Self {
            request_bucket,
            token_bucket,
        }
    }

    async fn acquire(&mut self, estimated_tokens: u32) -> Result<(), Duration> {
        // Check request limit
        if let Some(bucket) = &mut self.request_bucket {
            if !bucket.try_acquire(1.0) {
                return Err(bucket.wait_time(1.0));
            }
        }

        // Check token limit
        if let Some(bucket) = &mut self.token_bucket {
            if !bucket.try_acquire(estimated_tokens as f64) {
                return Err(bucket.wait_time(estimated_tokens as f64));
            }
        }

        Ok(())
    }
}

/// Rate limiting provider wrapper
pub struct RateLimitProvider {
    provider: Box<dyn LlmProvider>,
    state: Arc<Mutex<RateLimiterState>>,
    config: RateLimitConfig,
}

impl RateLimitProvider {
    /// Create a new rate-limited provider
    pub fn new(provider: Box<dyn LlmProvider>, config: RateLimitConfig) -> Self {
        let state = Arc::new(Mutex::new(RateLimiterState::new(&config)));
        Self {
            provider,
            state,
            config,
        }
    }

    /// Get current rate limit statistics
    pub async fn get_stats(&self) -> RateLimitStats {
        let state = self.state.lock().await;

        let available_requests = state
            .request_bucket
            .as_ref()
            .map(|b| b.tokens as u32)
            .unwrap_or(0);

        let available_tokens = state
            .token_bucket
            .as_ref()
            .map(|b| b.tokens as u32)
            .unwrap_or(0);

        RateLimitStats {
            requests_per_minute: self.config.requests_per_minute,
            tokens_per_minute: self.config.tokens_per_minute,
            available_requests,
            available_tokens,
        }
    }

    /// Estimate tokens in a request (simple heuristic: 4 chars per token)
    fn estimate_tokens(request: &LlmRequest) -> u32 {
        let prompt_len = request.prompt.len();
        let system_len = request.system_prompt.as_ref().map(|s| s.len()).unwrap_or(0);
        let total_chars = prompt_len + system_len;

        // Simple heuristic: ~4 characters per token
        ((total_chars / 4) as u32).max(1)
    }
}

/// Rate limit statistics
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    /// Maximum requests per minute
    pub requests_per_minute: u32,
    /// Maximum tokens per minute
    pub tokens_per_minute: u32,
    /// Currently available requests
    pub available_requests: u32,
    /// Currently available tokens
    pub available_tokens: u32,
}

#[async_trait]
impl LlmProvider for RateLimitProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let estimated_tokens = Self::estimate_tokens(&request);

        // Try to acquire rate limit tokens
        loop {
            let result = {
                let mut state = self.state.lock().await;
                state.acquire(estimated_tokens).await
            };

            match result {
                Ok(()) => break,
                Err(wait_time) => {
                    // Wait and retry
                    tokio::time::sleep(wait_time).await;
                }
            }
        }

        // Make the actual request
        self.provider.complete(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Usage;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct MockProvider {
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
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
    async fn test_rate_limit_requests() {
        let call_count = Arc::new(AtomicU32::new(0));
        let mock = MockProvider {
            call_count: Arc::clone(&call_count),
        };

        // Allow 10 requests per minute
        let config = RateLimitConfig::new().with_requests_per_minute(10);
        let rate_limited = RateLimitProvider::new(Box::new(mock), config);

        // Make 5 requests - should all succeed quickly
        let start = Instant::now();
        for _ in 0..5 {
            let request = LlmRequest {
                prompt: "test".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };
            rate_limited.complete(request).await.unwrap();
        }
        let elapsed = start.elapsed();

        assert_eq!(call_count.load(Ordering::SeqCst), 5);
        // Should complete quickly (within 1 second)
        assert!(elapsed < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_rate_limit_stats() {
        let mock = MockProvider {
            call_count: Arc::new(AtomicU32::new(0)),
        };

        let config = RateLimitConfig::new()
            .with_requests_per_minute(60)
            .with_tokens_per_minute(100_000);

        let rate_limited = RateLimitProvider::new(Box::new(mock), config);

        let stats = rate_limited.get_stats().await;
        assert_eq!(stats.requests_per_minute, 60);
        assert_eq!(stats.tokens_per_minute, 100_000);
        assert!(stats.available_requests <= 60);
        assert!(stats.available_tokens <= 100_000);
    }

    #[tokio::test]
    async fn test_rate_limit_config() {
        let config = RateLimitConfig::new()
            .with_requests_per_minute(120)
            .with_tokens_per_minute(200_000);

        assert_eq!(config.requests_per_minute, 120);
        assert_eq!(config.tokens_per_minute, 200_000);
        assert_eq!(config.refill_rate, 2.0); // 120/60
        assert_eq!(config.token_refill_rate, 200_000.0 / 60.0);
    }

    #[tokio::test]
    async fn test_token_estimation() {
        let request = LlmRequest {
            prompt: "Hello world this is a test prompt".to_string(), // ~35 chars
            system_prompt: Some("You are a helpful assistant".to_string()), // ~27 chars
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let tokens = RateLimitProvider::estimate_tokens(&request);
        // Total ~62 chars / 4 = ~15 tokens
        assert!((10..=20).contains(&tokens));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(100, 10.0); // 100 capacity, 10/sec refill

        // Consume all tokens
        assert!(bucket.try_acquire(100.0));
        assert!(!bucket.try_acquire(1.0));

        // Wait a bit and tokens should refill
        std::thread::sleep(Duration::from_millis(500));
        bucket.refill();

        // Should have ~5 tokens now (0.5 sec * 10/sec)
        assert!(bucket.tokens >= 4.0 && bucket.tokens <= 6.0);
    }
}
