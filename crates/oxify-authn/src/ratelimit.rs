//! Rate limiting for authentication endpoints
//!
//! Provides protection against brute force attacks and abuse through
//! configurable rate limiting with sliding window algorithm.
//!
//! # Features
//! - Per-IP rate limiting
//! - Per-user rate limiting
//! - Exponential backoff on failed attempts
//! - Configurable time windows and limits
//! - Automatic cleanup of expired entries
//!
//! # Example
//!
//! ```no_run
//! use oxify_authn::ratelimit::{RateLimiter, RateLimitConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = RateLimitConfig::default();
//! let limiter = RateLimiter::new(config);
//!
//! // Check if request is allowed
//! let result = limiter.check_ip("192.168.1.1").await?;
//! if result.allowed {
//!     println!("Request allowed, {} remaining", result.remaining);
//! } else {
//!     println!("Rate limited, retry after {} seconds", result.retry_after.unwrap_or(0));
//! }
//!
//! // Record a failed login attempt
//! limiter.record_failed_login("user@example.com", "192.168.1.1").await?;
//! # Ok(())
//! # }
//! ```

use crate::types::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per IP per window
    pub ip_limit: usize,
    /// IP rate limit window in seconds
    pub ip_window_secs: u64,
    /// Maximum failed login attempts per user
    pub user_failed_limit: usize,
    /// User failed login window in seconds
    pub user_failed_window_secs: u64,
    /// Enable exponential backoff for failed attempts
    pub exponential_backoff: bool,
    /// Base backoff time in seconds
    pub base_backoff_secs: u64,
    /// Maximum backoff time in seconds
    pub max_backoff_secs: u64,
    /// Cleanup interval in seconds
    pub cleanup_interval_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            ip_limit: 100,                 // 100 requests per window
            ip_window_secs: 60,            // 1 minute window
            user_failed_limit: 5,          // 5 failed attempts
            user_failed_window_secs: 3600, // 1 hour window
            exponential_backoff: true,
            base_backoff_secs: 5,       // 5 seconds base
            max_backoff_secs: 3600,     // 1 hour max
            cleanup_interval_secs: 300, // 5 minutes
        }
    }
}

impl RateLimitConfig {
    /// Create a new builder for `RateLimitConfig`
    #[must_use]
    pub fn builder() -> RateLimitConfigBuilder {
        RateLimitConfigBuilder::new()
    }

    /// Create a strict configuration for sensitive endpoints
    #[must_use]
    pub fn strict() -> Self {
        Self {
            ip_limit: 20,
            ip_window_secs: 60,
            user_failed_limit: 3,
            user_failed_window_secs: 3600,
            exponential_backoff: true,
            base_backoff_secs: 10,
            max_backoff_secs: 7200,
            cleanup_interval_secs: 300,
        }
    }

    /// Create a relaxed configuration for less sensitive endpoints
    #[must_use]
    pub fn relaxed() -> Self {
        Self {
            ip_limit: 500,
            ip_window_secs: 60,
            user_failed_limit: 10,
            user_failed_window_secs: 1800,
            exponential_backoff: false,
            base_backoff_secs: 0,
            max_backoff_secs: 0,
            cleanup_interval_secs: 600,
        }
    }
}

/// Builder for `RateLimitConfig`
#[derive(Debug, Clone)]
pub struct RateLimitConfigBuilder {
    ip_limit: usize,
    ip_window_secs: u64,
    user_failed_limit: usize,
    user_failed_window_secs: u64,
    exponential_backoff: bool,
    base_backoff_secs: u64,
    max_backoff_secs: u64,
    cleanup_interval_secs: u64,
}

impl RateLimitConfigBuilder {
    /// Create a new builder with default values
    #[must_use]
    pub fn new() -> Self {
        let default = RateLimitConfig::default();
        Self {
            ip_limit: default.ip_limit,
            ip_window_secs: default.ip_window_secs,
            user_failed_limit: default.user_failed_limit,
            user_failed_window_secs: default.user_failed_window_secs,
            exponential_backoff: default.exponential_backoff,
            base_backoff_secs: default.base_backoff_secs,
            max_backoff_secs: default.max_backoff_secs,
            cleanup_interval_secs: default.cleanup_interval_secs,
        }
    }

    /// Set maximum requests per IP per window
    #[must_use]
    pub fn ip_limit(mut self, limit: usize) -> Self {
        self.ip_limit = limit;
        self
    }

    /// Set IP rate limit window in seconds
    #[must_use]
    pub fn ip_window_secs(mut self, secs: u64) -> Self {
        self.ip_window_secs = secs;
        self
    }

    /// Set maximum failed login attempts per user
    #[must_use]
    pub fn user_failed_limit(mut self, limit: usize) -> Self {
        self.user_failed_limit = limit;
        self
    }

    /// Set user failed login window in seconds
    #[must_use]
    pub fn user_failed_window_secs(mut self, secs: u64) -> Self {
        self.user_failed_window_secs = secs;
        self
    }

    /// Enable or disable exponential backoff
    #[must_use]
    pub fn exponential_backoff(mut self, enable: bool) -> Self {
        self.exponential_backoff = enable;
        self
    }

    /// Set base backoff time in seconds
    #[must_use]
    pub fn base_backoff_secs(mut self, secs: u64) -> Self {
        self.base_backoff_secs = secs;
        self
    }

    /// Set maximum backoff time in seconds
    #[must_use]
    pub fn max_backoff_secs(mut self, secs: u64) -> Self {
        self.max_backoff_secs = secs;
        self
    }

    /// Set cleanup interval in seconds
    #[must_use]
    pub fn cleanup_interval_secs(mut self, secs: u64) -> Self {
        self.cleanup_interval_secs = secs;
        self
    }

    /// Build the `RateLimitConfig`
    #[must_use]
    pub fn build(self) -> RateLimitConfig {
        RateLimitConfig {
            ip_limit: self.ip_limit,
            ip_window_secs: self.ip_window_secs,
            user_failed_limit: self.user_failed_limit,
            user_failed_window_secs: self.user_failed_window_secs,
            exponential_backoff: self.exponential_backoff,
            base_backoff_secs: self.base_backoff_secs,
            max_backoff_secs: self.max_backoff_secs,
            cleanup_interval_secs: self.cleanup_interval_secs,
        }
    }
}

impl Default for RateLimitConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Number of remaining requests in the current window
    pub remaining: usize,
    /// When the limit resets (for headers)
    pub reset_at: DateTime<Utc>,
    /// Seconds until retry is allowed (if rate limited)
    pub retry_after: Option<u64>,
    /// Current limit
    pub limit: usize,
}

/// Entry tracking request counts
#[derive(Debug, Clone)]
struct RateLimitEntry {
    /// Request timestamps in current window
    requests: Vec<DateTime<Utc>>,
    /// Number of failed attempts (for backoff)
    failed_attempts: usize,
    /// When the entry was last updated
    last_update: DateTime<Utc>,
    /// Locked until (for exponential backoff)
    locked_until: Option<DateTime<Utc>>,
}

impl RateLimitEntry {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
            failed_attempts: 0,
            last_update: Utc::now(),
            locked_until: None,
        }
    }

    /// Clean up old requests outside the window
    fn cleanup(&mut self, window_secs: u64) {
        let cutoff = Utc::now() - Duration::seconds(window_secs as i64);
        self.requests.retain(|t| *t > cutoff);
    }

    /// Add a request
    fn add_request(&mut self) {
        self.requests.push(Utc::now());
        self.last_update = Utc::now();
    }

    /// Record a failed attempt
    fn record_failure(&mut self) {
        self.failed_attempts += 1;
        self.last_update = Utc::now();
    }

    /// Reset failed attempts (on successful login)
    fn reset_failures(&mut self) {
        self.failed_attempts = 0;
        self.locked_until = None;
    }
}

/// Rate limiter
pub struct RateLimiter {
    config: RateLimitConfig,
    /// IP-based rate limits
    ip_limits: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
    /// User-based failed login tracking
    user_limits: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    #[must_use]
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_limits: Arc::new(RwLock::new(HashMap::new())),
            user_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a request from an IP is allowed
    pub async fn check_ip(&self, ip: &str) -> Result<RateLimitResult> {
        let mut limits = self.ip_limits.write().await;

        let entry = limits
            .entry(ip.to_string())
            .or_insert_with(RateLimitEntry::new);

        // Cleanup old requests
        entry.cleanup(self.config.ip_window_secs);

        let reset_at = Utc::now() + Duration::seconds(self.config.ip_window_secs as i64);
        let remaining = self.config.ip_limit.saturating_sub(entry.requests.len());

        if entry.requests.len() >= self.config.ip_limit {
            // Calculate retry_after
            let oldest = entry.requests.first().copied();
            let retry_after = oldest.map(|t| {
                let unlock_time = t + Duration::seconds(self.config.ip_window_secs as i64);
                let wait = unlock_time - Utc::now();
                wait.num_seconds().max(0) as u64
            });

            return Ok(RateLimitResult {
                allowed: false,
                remaining: 0,
                reset_at,
                retry_after,
                limit: self.config.ip_limit,
            });
        }

        // Record this request
        entry.add_request();

        Ok(RateLimitResult {
            allowed: true,
            remaining: remaining.saturating_sub(1),
            reset_at,
            retry_after: None,
            limit: self.config.ip_limit,
        })
    }

    /// Check if a user login attempt is allowed
    pub async fn check_user_login(&self, user_id: &str) -> Result<RateLimitResult> {
        let limits = self.user_limits.read().await;

        if let Some(entry) = limits.get(user_id) {
            // Check if user is locked due to exponential backoff
            if let Some(locked_until) = entry.locked_until {
                if Utc::now() < locked_until {
                    let retry_after = (locked_until - Utc::now()).num_seconds().max(0) as u64;
                    return Ok(RateLimitResult {
                        allowed: false,
                        remaining: 0,
                        reset_at: locked_until,
                        retry_after: Some(retry_after),
                        limit: self.config.user_failed_limit,
                    });
                }
            }

            // Count recent failures
            let cutoff = Utc::now() - Duration::seconds(self.config.user_failed_window_secs as i64);
            let recent_failures = entry.requests.iter().filter(|t| **t > cutoff).count();

            if recent_failures >= self.config.user_failed_limit {
                let reset_at =
                    Utc::now() + Duration::seconds(self.config.user_failed_window_secs as i64);
                return Ok(RateLimitResult {
                    allowed: false,
                    remaining: 0,
                    reset_at,
                    retry_after: Some(self.config.user_failed_window_secs),
                    limit: self.config.user_failed_limit,
                });
            }

            let remaining = self.config.user_failed_limit - recent_failures;
            return Ok(RateLimitResult {
                allowed: true,
                remaining,
                reset_at: Utc::now()
                    + Duration::seconds(self.config.user_failed_window_secs as i64),
                retry_after: None,
                limit: self.config.user_failed_limit,
            });
        }

        Ok(RateLimitResult {
            allowed: true,
            remaining: self.config.user_failed_limit,
            reset_at: Utc::now() + Duration::seconds(self.config.user_failed_window_secs as i64),
            retry_after: None,
            limit: self.config.user_failed_limit,
        })
    }

    /// Record a failed login attempt
    pub async fn record_failed_login(&self, user_id: &str, ip: &str) -> Result<()> {
        // Record for user
        {
            let mut limits = self.user_limits.write().await;
            let entry = limits
                .entry(user_id.to_string())
                .or_insert_with(RateLimitEntry::new);

            entry.add_request(); // Track failed attempt timestamp
            entry.record_failure();

            // Apply exponential backoff if enabled
            if self.config.exponential_backoff && entry.failed_attempts > 0 {
                let backoff = self.calculate_backoff(entry.failed_attempts);
                entry.locked_until = Some(Utc::now() + Duration::seconds(backoff as i64));
            }
        }

        // Also count against IP
        let _ = self.check_ip(ip).await;

        Ok(())
    }

    /// Record a successful login (reset failed attempts)
    pub async fn record_successful_login(&self, user_id: &str) -> Result<()> {
        let mut limits = self.user_limits.write().await;

        if let Some(entry) = limits.get_mut(user_id) {
            entry.reset_failures();
        }

        Ok(())
    }

    /// Calculate exponential backoff time
    fn calculate_backoff(&self, failed_attempts: usize) -> u64 {
        if !self.config.exponential_backoff {
            return 0;
        }

        // Exponential backoff: base * 2^(attempts - 1)
        let exponent = (failed_attempts.saturating_sub(1)) as u32;
        let backoff = self
            .config
            .base_backoff_secs
            .saturating_mul(2u64.saturating_pow(exponent));

        // Cap at maximum
        backoff.min(self.config.max_backoff_secs)
    }

    /// Get current rate limit status for an IP
    pub async fn get_ip_status(&self, ip: &str) -> RateLimitStatus {
        let limits = self.ip_limits.read().await;

        if let Some(entry) = limits.get(ip) {
            let cutoff = Utc::now() - Duration::seconds(self.config.ip_window_secs as i64);
            let count = entry.requests.iter().filter(|t| **t > cutoff).count();

            RateLimitStatus {
                current_count: count,
                limit: self.config.ip_limit,
                remaining: self.config.ip_limit.saturating_sub(count),
                window_secs: self.config.ip_window_secs,
                is_limited: count >= self.config.ip_limit,
            }
        } else {
            RateLimitStatus {
                current_count: 0,
                limit: self.config.ip_limit,
                remaining: self.config.ip_limit,
                window_secs: self.config.ip_window_secs,
                is_limited: false,
            }
        }
    }

    /// Get current failed login status for a user
    pub async fn get_user_status(&self, user_id: &str) -> UserRateLimitStatus {
        let limits = self.user_limits.read().await;

        if let Some(entry) = limits.get(user_id) {
            let cutoff = Utc::now() - Duration::seconds(self.config.user_failed_window_secs as i64);
            let count = entry.requests.iter().filter(|t| **t > cutoff).count();

            UserRateLimitStatus {
                failed_attempts: entry.failed_attempts,
                recent_failures: count,
                limit: self.config.user_failed_limit,
                remaining: self.config.user_failed_limit.saturating_sub(count),
                locked_until: entry.locked_until,
                is_locked: entry.locked_until.is_some_and(|t| Utc::now() < t),
            }
        } else {
            UserRateLimitStatus {
                failed_attempts: 0,
                recent_failures: 0,
                limit: self.config.user_failed_limit,
                remaining: self.config.user_failed_limit,
                locked_until: None,
                is_locked: false,
            }
        }
    }

    /// Cleanup expired entries
    pub async fn cleanup_expired(&self) -> usize {
        let mut count = 0;

        // Cleanup IP limits
        {
            let mut limits = self.ip_limits.write().await;
            let cutoff = Utc::now() - Duration::seconds(self.config.ip_window_secs as i64 * 2);
            let before = limits.len();
            limits.retain(|_, entry| entry.last_update > cutoff);
            count += before - limits.len();
        }

        // Cleanup user limits
        {
            let mut limits = self.user_limits.write().await;
            let cutoff =
                Utc::now() - Duration::seconds(self.config.user_failed_window_secs as i64 * 2);
            let before = limits.len();
            limits.retain(|_, entry| entry.last_update > cutoff);
            count += before - limits.len();
        }

        count
    }

    /// Reset all limits for an IP (admin function)
    pub async fn reset_ip(&self, ip: &str) -> bool {
        let mut limits = self.ip_limits.write().await;
        limits.remove(ip).is_some()
    }

    /// Reset all limits for a user (admin function)
    pub async fn reset_user(&self, user_id: &str) -> bool {
        let mut limits = self.user_limits.write().await;
        limits.remove(user_id).is_some()
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }
}

/// IP rate limit status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    /// Current request count in window
    pub current_count: usize,
    /// Maximum allowed requests
    pub limit: usize,
    /// Remaining requests
    pub remaining: usize,
    /// Window size in seconds
    pub window_secs: u64,
    /// Whether currently rate limited
    pub is_limited: bool,
}

/// User rate limit status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRateLimitStatus {
    /// Total failed attempts
    pub failed_attempts: usize,
    /// Recent failures in current window
    pub recent_failures: usize,
    /// Maximum allowed failures
    pub limit: usize,
    /// Remaining attempts
    pub remaining: usize,
    /// When lock expires (if locked)
    pub locked_until: Option<DateTime<Utc>>,
    /// Whether currently locked
    pub is_locked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ip_rate_limiting() {
        let config = RateLimitConfig {
            ip_limit: 3,
            ip_window_secs: 60,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // First 3 requests should be allowed
        for i in 0..3 {
            let result = limiter.check_ip("192.168.1.1").await.unwrap();
            assert!(result.allowed, "Request {} should be allowed", i + 1);
        }

        // 4th request should be denied
        let result = limiter.check_ip("192.168.1.1").await.unwrap();
        assert!(!result.allowed);
        assert!(result.retry_after.is_some());
    }

    #[tokio::test]
    async fn test_different_ips() {
        let config = RateLimitConfig {
            ip_limit: 2,
            ip_window_secs: 60,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // IP 1
        let result = limiter.check_ip("192.168.1.1").await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 1);

        // IP 2 should have its own limit
        let result = limiter.check_ip("192.168.1.2").await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 1);
    }

    #[tokio::test]
    async fn test_failed_login_tracking() {
        let config = RateLimitConfig {
            user_failed_limit: 3,
            user_failed_window_secs: 60,
            exponential_backoff: false,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Record some failures
        limiter
            .record_failed_login("user1", "192.168.1.1")
            .await
            .unwrap();
        limiter
            .record_failed_login("user1", "192.168.1.1")
            .await
            .unwrap();

        let result = limiter.check_user_login("user1").await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 1);

        // One more failure
        limiter
            .record_failed_login("user1", "192.168.1.1")
            .await
            .unwrap();

        // Should now be blocked
        let result = limiter.check_user_login("user1").await.unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_successful_login_resets() {
        let config = RateLimitConfig {
            user_failed_limit: 3,
            exponential_backoff: false,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Record failures
        limiter
            .record_failed_login("user1", "192.168.1.1")
            .await
            .unwrap();
        limiter
            .record_failed_login("user1", "192.168.1.1")
            .await
            .unwrap();

        let status = limiter.get_user_status("user1").await;
        assert_eq!(status.failed_attempts, 2);

        // Successful login resets
        limiter.record_successful_login("user1").await.unwrap();

        let status = limiter.get_user_status("user1").await;
        assert_eq!(status.failed_attempts, 0);
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let config = RateLimitConfig {
            user_failed_limit: 10,
            exponential_backoff: true,
            base_backoff_secs: 1,
            max_backoff_secs: 60,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // First failure: 1 second backoff
        assert_eq!(limiter.calculate_backoff(1), 1);

        // Second failure: 2 seconds
        assert_eq!(limiter.calculate_backoff(2), 2);

        // Third failure: 4 seconds
        assert_eq!(limiter.calculate_backoff(3), 4);

        // Should cap at max
        assert_eq!(limiter.calculate_backoff(10), 60);
    }

    #[tokio::test]
    async fn test_ip_status() {
        let config = RateLimitConfig {
            ip_limit: 5,
            ip_window_secs: 60,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // No requests yet
        let status = limiter.get_ip_status("192.168.1.1").await;
        assert_eq!(status.current_count, 0);
        assert_eq!(status.remaining, 5);
        assert!(!status.is_limited);

        // Make some requests
        limiter.check_ip("192.168.1.1").await.unwrap();
        limiter.check_ip("192.168.1.1").await.unwrap();

        let status = limiter.get_ip_status("192.168.1.1").await;
        assert_eq!(status.current_count, 2);
        assert_eq!(status.remaining, 3);
    }

    #[tokio::test]
    async fn test_reset_functions() {
        let limiter = RateLimiter::default();

        // Make some requests
        limiter.check_ip("192.168.1.1").await.unwrap();
        limiter
            .record_failed_login("user1", "192.168.1.1")
            .await
            .unwrap();

        // Reset IP
        let reset = limiter.reset_ip("192.168.1.1").await;
        assert!(reset);

        let status = limiter.get_ip_status("192.168.1.1").await;
        assert_eq!(status.current_count, 0);

        // Reset user
        let reset = limiter.reset_user("user1").await;
        assert!(reset);

        let status = limiter.get_user_status("user1").await;
        assert_eq!(status.failed_attempts, 0);
    }

    #[test]
    fn test_config_presets() {
        let strict = RateLimitConfig::strict();
        assert_eq!(strict.ip_limit, 20);
        assert_eq!(strict.user_failed_limit, 3);

        let relaxed = RateLimitConfig::relaxed();
        assert_eq!(relaxed.ip_limit, 500);
        assert!(!relaxed.exponential_backoff);
    }

    #[test]
    fn test_ratelimit_config_builder() {
        let config = RateLimitConfig::builder()
            .ip_limit(150)
            .ip_window_secs(120)
            .user_failed_limit(7)
            .user_failed_window_secs(2400)
            .exponential_backoff(true)
            .base_backoff_secs(15)
            .max_backoff_secs(3600)
            .cleanup_interval_secs(450)
            .build();

        assert_eq!(config.ip_limit, 150);
        assert_eq!(config.ip_window_secs, 120);
        assert_eq!(config.user_failed_limit, 7);
        assert_eq!(config.user_failed_window_secs, 2400);
        assert!(config.exponential_backoff);
        assert_eq!(config.base_backoff_secs, 15);
        assert_eq!(config.max_backoff_secs, 3600);
        assert_eq!(config.cleanup_interval_secs, 450);
    }
}
