//! Retry utilities for handling transient failures
//!
//! This module provides helpers for retrying operations with exponential backoff.

use crate::{Result, VectorError};
use std::future::Future;
use std::time::Duration;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    pub max_attempts: u32,
    /// Initial backoff duration (default: 100ms)
    pub initial_backoff: Duration,
    /// Maximum backoff duration (default: 5s)
    pub max_backoff: Duration,
    /// Backoff multiplier (default: 2.0)
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of attempts
    pub fn max_attempts(mut self, max: u32) -> Self {
        self.max_attempts = max;
        self
    }

    /// Set the initial backoff duration
    pub fn initial_backoff(mut self, duration: Duration) -> Self {
        self.initial_backoff = duration;
        self
    }

    /// Set the maximum backoff duration
    pub fn max_backoff(mut self, duration: Duration) -> Self {
        self.max_backoff = duration;
        self
    }

    /// Set the backoff multiplier
    pub fn backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }
}

/// Retry an async operation with exponential backoff
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation` - Async operation to retry
///
/// # Example
/// ```no_run
/// use oxify_connect_vector::retry::{retry_with_backoff, RetryConfig};
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = RetryConfig::new()
///     .max_attempts(5)
///     .initial_backoff(Duration::from_millis(50));
///
/// let result = retry_with_backoff(config, || async {
///     // Your async operation here
///     Ok::<_, oxify_connect_vector::VectorError>(42)
/// }).await?;
/// # Ok(())
/// # }
/// ```
pub async fn retry_with_backoff<F, Fut, T>(config: RetryConfig, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0;
    let mut backoff = config.initial_backoff;

    loop {
        attempt += 1;

        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                // Check if we should retry
                if !is_retryable_error(&e) || attempt >= config.max_attempts {
                    return Err(e);
                }

                // Log retry attempt (in production, use proper logging)
                tracing::debug!(
                    "Retry attempt {}/{} after error: {}. Waiting {:?}",
                    attempt,
                    config.max_attempts,
                    e,
                    backoff
                );

                // Wait before retrying
                tokio::time::sleep(backoff).await;

                // Calculate next backoff with exponential increase
                backoff = std::cmp::min(
                    Duration::from_secs_f64(backoff.as_secs_f64() * config.backoff_multiplier),
                    config.max_backoff,
                );
            }
        }
    }
}

/// Check if an error is retryable (transient)
fn is_retryable_error(error: &VectorError) -> bool {
    match error {
        VectorError::ConnectionError(_) => true,
        VectorError::QueryError(msg) => {
            // Retry on timeout, connection reset, etc.
            msg.contains("timeout")
                || msg.contains("connection")
                || msg.contains("temporary")
                || msg.contains("transient")
        }
        VectorError::DatabaseError(msg) => {
            // Retry on database connection issues
            msg.contains("connection") || msg.contains("timeout")
        }
        VectorError::ConfigError(_) => false, // Config errors are not retryable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success_on_first_attempt() {
        let config = RetryConfig::new();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, VectorError>(42)
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryConfig::new().max_attempts(3);
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(config, move || {
            let count = call_count_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst) + 1;
                if current < 3 {
                    Err(VectorError::ConnectionError(
                        "temporary failure".to_string(),
                    ))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_max_attempts_exceeded() {
        let config = RetryConfig::new()
            .max_attempts(2)
            .initial_backoff(Duration::from_millis(1));
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(VectorError::ConnectionError(
                    "persistent failure".to_string(),
                ))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_non_retryable_error() {
        let config = RetryConfig::new();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(VectorError::ConfigError("invalid config".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should not retry
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error(&VectorError::ConnectionError(
            "test".to_string()
        )));
        assert!(is_retryable_error(&VectorError::QueryError(
            "timeout error".to_string()
        )));
        assert!(is_retryable_error(&VectorError::DatabaseError(
            "connection lost".to_string()
        )));
        assert!(!is_retryable_error(&VectorError::ConfigError(
            "invalid".to_string()
        )));
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::new()
            .max_attempts(5)
            .initial_backoff(Duration::from_millis(50))
            .max_backoff(Duration::from_secs(10))
            .backoff_multiplier(3.0);

        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_backoff, Duration::from_millis(50));
        assert_eq!(config.max_backoff, Duration::from_secs(10));
        assert_eq!(config.backoff_multiplier, 3.0);
    }
}
