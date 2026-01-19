//! Rate limiting middleware using token bucket algorithm
//!
//! Provides per-IP and per-user rate limiting with configurable limits.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests per window
    pub max_requests: u32,
    /// Time window duration
    pub window: Duration,
    /// Refill rate (tokens per second)
    pub refill_rate: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
            refill_rate: 1.67, // ~100 requests per minute
        }
    }
}

impl RateLimitConfig {
    /// Create a new rate limit configuration
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        let refill_rate = max_requests as f64 / window_secs as f64;
        Self {
            max_requests,
            window: Duration::from_secs(window_secs),
            refill_rate,
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    /// Available tokens
    tokens: f64,
    /// Maximum capacity
    capacity: f64,
    /// Last refill time
    last_refill: Instant,
    /// Refill rate (tokens per second)
    refill_rate: f64,
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            tokens: capacity as f64,
            capacity: capacity as f64,
            last_refill: Instant::now(),
            refill_rate,
        }
    }

    fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let new_tokens = elapsed * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    fn remaining(&self) -> u32 {
        self.tokens.floor() as u32
    }
}

/// Rate limiter using token bucket algorithm
#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Mutex<HashMap<String, TokenBucket>>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Check if a request is allowed for the given key
    pub fn check_rate_limit(&self, key: &str) -> RateLimitResult {
        let mut buckets = self.buckets.lock().unwrap();

        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.max_requests, self.config.refill_rate));

        if bucket.try_consume() {
            RateLimitResult::Allowed {
                limit: self.config.max_requests,
                remaining: bucket.remaining(),
                reset: self.config.window.as_secs(),
            }
        } else {
            RateLimitResult::RateLimited {
                limit: self.config.max_requests,
                retry_after: self.config.window.as_secs(),
            }
        }
    }

    /// Clean up old buckets (for memory management)
    pub fn cleanup(&self) {
        let mut buckets = self.buckets.lock().unwrap();
        buckets.retain(|_, bucket| {
            // Remove buckets that have been idle for more than 5 minutes
            bucket.last_refill.elapsed() < Duration::from_secs(300)
        });
    }
}

/// Result of a rate limit check
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed {
        limit: u32,
        remaining: u32,
        reset: u64,
    },
    RateLimited {
        limit: u32,
        retry_after: u64,
    },
}

/// Rate limiting middleware
///
/// Limits requests per IP address using token bucket algorithm.
pub async fn rate_limit_middleware(
    limiter: Arc<RateLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, impl IntoResponse> {
    // Extract IP address from connection info or X-Forwarded-For header
    let ip = extract_ip(&request).unwrap_or_else(|| "unknown".to_string());

    match limiter.check_rate_limit(&ip) {
        RateLimitResult::Allowed {
            limit,
            remaining,
            reset,
        } => {
            let mut response = next.run(request).await;

            // Add rate limit headers
            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            headers.insert("X-RateLimit-Reset", reset.to_string().parse().unwrap());

            Ok(response)
        }
        RateLimitResult::RateLimited { retry_after, .. } => Err((
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", retry_after.to_string())],
            "Rate limit exceeded",
        )),
    }
}

/// Extract IP address from request
fn extract_ip(request: &Request) -> Option<String> {
    // Try X-Forwarded-For header first
    if let Some(forwarded) = request.headers().get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(ip) = forwarded_str.split(',').next() {
                return Some(ip.trim().to_string());
            }
        }
    }

    // Try X-Real-IP header
    if let Some(real_ip) = request.headers().get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            return Some(ip_str.to_string());
        }
    }

    // Fall back to connection info (extension in Axum)
    request
        .extensions()
        .get::<IpAddr>()
        .map(|ip| ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(10, 1.0);
        assert!(bucket.try_consume());
        assert_eq!(bucket.remaining(), 9);
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10, 10.0);
        // Consume all tokens
        for _ in 0..10 {
            assert!(bucket.try_consume());
        }
        assert!(!bucket.try_consume());

        // Wait for refill (simulate by manipulating last_refill)
        bucket.last_refill = Instant::now() - Duration::from_secs(1);
        bucket.refill();
        assert!(bucket.tokens >= 10.0);
    }

    #[test]
    fn test_rate_limiter_basic() {
        let config = RateLimitConfig::new(5, 60);
        let limiter = RateLimiter::new(config);

        // Should allow first 5 requests
        for _ in 0..5 {
            match limiter.check_rate_limit("test-key") {
                RateLimitResult::Allowed { .. } => {}
                RateLimitResult::RateLimited { .. } => panic!("Should be allowed"),
            }
        }

        // Should rate limit the 6th request
        match limiter.check_rate_limit("test-key") {
            RateLimitResult::RateLimited { .. } => {}
            RateLimitResult::Allowed { .. } => panic!("Should be rate limited"),
        }
    }

    #[test]
    fn test_rate_limiter_different_keys() {
        let config = RateLimitConfig::new(1, 60);
        let limiter = RateLimiter::new(config);

        // Different keys should have independent limits
        assert!(matches!(
            limiter.check_rate_limit("key1"),
            RateLimitResult::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check_rate_limit("key2"),
            RateLimitResult::Allowed { .. }
        ));
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window, Duration::from_secs(60));
    }
}
