//! Rate limiting utilities for parallel operations
//!
//! This module provides rate limiting to prevent overwhelming vector databases
//! during high-throughput parallel operations.
//!
//! ## Example
//!
//! ```rust
//! use oxify_connect_vector::ratelimit::RateLimiter;
//! use std::time::Duration;
//!
//! # async fn example() {
//! // Create a rate limiter allowing 100 requests per second
//! let limiter = RateLimiter::new(100.0);
//!
//! // Acquire permission before making a request
//! limiter.acquire(1).await;
//! // ... make request ...
//!
//! // Acquire multiple permits at once
//! limiter.acquire(5).await;
//! // ... make 5 requests ...
//! # }
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

/// Token bucket rate limiter for controlling request throughput
///
/// Uses the token bucket algorithm to allow bursts while maintaining
/// an average rate limit over time.
#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<RateLimiterState>>,
    rate: f64,
    capacity: f64,
}

struct RateLimiterState {
    tokens: f64,
    last_update: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    ///
    /// * `rate` - Maximum requests per second (e.g., 100.0 for 100 req/s)
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_vector::ratelimit::RateLimiter;
    ///
    /// // Allow 50 requests per second
    /// let limiter = RateLimiter::new(50.0);
    /// ```
    pub fn new(rate: f64) -> Self {
        Self::with_capacity(rate, rate)
    }

    /// Create a new rate limiter with custom burst capacity
    ///
    /// # Arguments
    ///
    /// * `rate` - Maximum requests per second
    /// * `capacity` - Maximum burst size (number of tokens in bucket)
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_vector::ratelimit::RateLimiter;
    ///
    /// // Allow 50 req/s with burst capacity of 100
    /// let limiter = RateLimiter::with_capacity(50.0, 100.0);
    /// ```
    pub fn with_capacity(rate: f64, capacity: f64) -> Self {
        Self {
            state: Arc::new(Mutex::new(RateLimiterState {
                tokens: capacity,
                last_update: Instant::now(),
            })),
            rate,
            capacity,
        }
    }

    /// Acquire permission to perform operations
    ///
    /// Blocks until the specified number of tokens are available.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Number of permits to acquire (typically 1 per request)
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_vector::ratelimit::RateLimiter;
    ///
    /// # async fn example() {
    /// let limiter = RateLimiter::new(100.0);
    ///
    /// // Acquire permission for 1 request
    /// limiter.acquire(1).await;
    ///
    /// // Acquire permission for batch of 10 requests
    /// limiter.acquire(10).await;
    /// # }
    /// ```
    pub async fn acquire(&self, tokens: u32) {
        let tokens = tokens as f64;

        loop {
            let wait_time = {
                let mut state = self.state.lock().await;

                // Refill tokens based on elapsed time
                let now = Instant::now();
                let elapsed = now.duration_since(state.last_update).as_secs_f64();
                state.tokens = (state.tokens + elapsed * self.rate).min(self.capacity);
                state.last_update = now;

                // If we have enough tokens, consume them and return
                if state.tokens >= tokens {
                    state.tokens -= tokens;
                    None
                } else {
                    // Calculate how long to wait for enough tokens
                    let tokens_needed = tokens - state.tokens;
                    let wait_secs = tokens_needed / self.rate;
                    Some(Duration::from_secs_f64(wait_secs))
                }
            };

            match wait_time {
                None => break,
                Some(duration) => sleep(duration).await,
            }
        }
    }

    /// Try to acquire permits without blocking
    ///
    /// Returns true if permits were acquired, false otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_vector::ratelimit::RateLimiter;
    ///
    /// # async fn example() {
    /// let limiter = RateLimiter::new(100.0);
    ///
    /// if limiter.try_acquire(1).await {
    ///     // Permission granted, make request
    /// } else {
    ///     // Rate limit reached, handle accordingly
    /// }
    /// # }
    /// ```
    pub async fn try_acquire(&self, tokens: u32) -> bool {
        let tokens = tokens as f64;
        let mut state = self.state.lock().await;

        // Refill tokens based on elapsed time
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_update).as_secs_f64();
        state.tokens = (state.tokens + elapsed * self.rate).min(self.capacity);
        state.last_update = now;

        // Try to consume tokens
        if state.tokens >= tokens {
            state.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Get the current rate limit (requests per second)
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Get the maximum burst capacity
    pub fn capacity(&self) -> f64 {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let limiter = RateLimiter::new(10.0); // 10 requests per second

        // Should be able to acquire immediately (burst capacity)
        let start = Instant::now();
        limiter.acquire(1).await;
        let elapsed = start.elapsed();

        // Should be nearly instantaneous
        assert!(elapsed.as_millis() < 50);
    }

    #[tokio::test]
    async fn test_rate_limiter_burst() {
        let limiter = RateLimiter::with_capacity(10.0, 20.0); // 10 req/s, burst of 20

        // Should be able to acquire burst capacity immediately
        let start = Instant::now();
        limiter.acquire(20).await;
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 50);
    }

    #[tokio::test]
    async fn test_rate_limiter_refill() {
        let limiter = RateLimiter::new(100.0); // 100 requests per second

        // Exhaust the bucket
        limiter.acquire(100).await;

        // Wait for refill (100ms should give us ~10 tokens at 100/s)
        sleep(Duration::from_millis(100)).await;

        // Should be able to acquire ~10 tokens without waiting
        let start = Instant::now();
        limiter.acquire(10).await;
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 50);
    }

    #[tokio::test]
    async fn test_try_acquire_success() {
        let limiter = RateLimiter::new(10.0);

        // Should succeed initially (bucket is full)
        assert!(limiter.try_acquire(1).await);
    }

    #[tokio::test]
    async fn test_try_acquire_failure() {
        let limiter = RateLimiter::new(10.0);

        // Exhaust the bucket
        limiter.acquire(10).await;

        // Should fail immediately (no tokens available)
        assert!(!limiter.try_acquire(1).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_concurrent() {
        let limiter = Arc::new(RateLimiter::new(50.0)); // 50 req/s

        // Spawn multiple concurrent tasks
        let mut handles = vec![];
        for _ in 0..10 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                limiter_clone.acquire(1).await;
            }));
        }

        // All should complete (using burst capacity)
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_rate_capacity_getters() {
        let limiter = RateLimiter::with_capacity(50.0, 100.0);

        assert!((limiter.rate() - 50.0).abs() < 1e-6);
        assert!((limiter.capacity() - 100.0).abs() < 1e-6);
    }
}
