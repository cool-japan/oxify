//! Retry utilities for handling transient database errors
//!
//! Provides exponential backoff retry logic for operations that may encounter
//! transient database errors (connection timeouts, serialization failures, etc.).

use crate::Result;
use std::future::Future;
use std::time::Duration;

/// Retry configuration
///
/// # Examples
/// ```
/// # use oxify_storage::retry::RetryConfig;
/// # use std::time::Duration;
/// let config = RetryConfig {
///     max_attempts: 5,
///     initial_backoff: Duration::from_millis(50),
///     max_backoff: Duration::from_secs(10),
///     backoff_multiplier: 2.0,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial backoff duration before first retry
    pub initial_backoff: Duration,
    /// Maximum backoff duration (backoff is capped at this value)
    pub max_backoff: Duration,
    /// Multiplier for exponential backoff
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
    /// Create a retry config for quick operations (fast retry)
    pub fn fast() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_secs(1),
            backoff_multiplier: 2.0,
        }
    }

    /// Create a retry config for slow operations (patient retry)
    pub fn slow() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

/// Retry a database operation with exponential backoff
///
/// Only retries on transient errors (as determined by `StorageError::is_retryable()`).
/// Non-retryable errors are returned immediately.
///
/// # Examples
/// ```ignore
/// use oxify_storage::retry::{retry_on_transient_error, RetryConfig};
///
/// let result = retry_on_transient_error(
///     || async {
///         // Your database operation here
///         sqlx::query("SELECT 1")
///             .execute(&pool)
///             .await?;
///         Ok(())
///     },
///     RetryConfig::default(),
/// )
/// .await?;
/// ```
#[tracing::instrument(skip(operation, config), fields(max_attempts = config.max_attempts))]
pub async fn retry_on_transient_error<F, Fut, T>(operation: F, config: RetryConfig) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0;
    let mut backoff = config.initial_backoff;

    loop {
        attempt += 1;

        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(
                        "Operation succeeded on attempt {}/{}",
                        attempt,
                        config.max_attempts
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                if attempt >= config.max_attempts {
                    tracing::error!("Operation failed after {} attempts: {}", attempt, e);
                    return Err(e);
                }

                if !e.is_retryable() {
                    tracing::warn!(
                        "Non-retryable error encountered on attempt {}: {}",
                        attempt,
                        e
                    );
                    return Err(e);
                }

                tracing::warn!(
                    "Retryable error on attempt {}/{}, retrying after {:?}: {}",
                    attempt,
                    config.max_attempts,
                    backoff,
                    e
                );

                tokio::time::sleep(backoff).await;
                backoff = std::cmp::min(
                    Duration::from_secs_f64(backoff.as_secs_f64() * config.backoff_multiplier),
                    config.max_backoff,
                );
            }
        }
    }
}

/// Retry statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct RetryStats {
    /// Total number of operations
    pub total_operations: u64,
    /// Total number of successful operations
    pub successful_operations: u64,
    /// Total number of failed operations (after all retries exhausted)
    pub failed_operations: u64,
    /// Total number of retries performed
    pub total_retries: u64,
}

impl RetryStats {
    /// Create new retry statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            return 0.0;
        }
        self.successful_operations as f64 / self.total_operations as f64
    }

    /// Calculate average retries per operation
    pub fn avg_retries_per_operation(&self) -> f64 {
        if self.total_operations == 0 {
            return 0.0;
        }
        self.total_retries as f64 / self.total_operations as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageError;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_immediately() {
        let config = RetryConfig::fast();

        let result = retry_on_transient_error(|| async { Ok(42) }, config).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_succeeds_eventually() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let config = RetryConfig {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_secs(1),
            backoff_multiplier: 2.0,
        };

        let result = retry_on_transient_error(
            || {
                let c = counter_clone.clone();
                async move {
                    let count = c.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        // Simulate transient error
                        Err(StorageError::Database(sqlx::Error::PoolTimedOut))
                    } else {
                        Ok(42)
                    }
                }
            },
            config,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_attempts() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let config = RetryConfig {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_secs(1),
            backoff_multiplier: 2.0,
        };

        let result = retry_on_transient_error(
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    // Always fail with transient error
                    Err::<i32, _>(StorageError::Database(sqlx::Error::PoolTimedOut))
                }
            },
            config,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_stops_on_non_retryable_error() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let config = RetryConfig::fast();

        let result = retry_on_transient_error(
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    // Non-retryable error (validation error)
                    Err::<i32, _>(StorageError::validation("Invalid input"))
                }
            },
            config,
        )
        .await;

        assert!(result.is_err());
        // Should only try once - no retries for non-retryable errors
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_backoff, Duration::from_millis(100));
        assert_eq!(config.max_backoff, Duration::from_secs(5));
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_retry_config_fast() {
        let config = RetryConfig::fast();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_backoff, Duration::from_millis(10));
        assert_eq!(config.max_backoff, Duration::from_secs(1));
    }

    #[test]
    fn test_retry_config_slow() {
        let config = RetryConfig::slow();
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_backoff, Duration::from_millis(500));
        assert_eq!(config.max_backoff, Duration::from_secs(30));
    }

    #[test]
    fn test_retry_stats() {
        let mut stats = RetryStats::new();
        assert_eq!(stats.success_rate(), 0.0);
        assert_eq!(stats.avg_retries_per_operation(), 0.0);

        stats.total_operations = 100;
        stats.successful_operations = 95;
        stats.failed_operations = 5;
        stats.total_retries = 20;

        assert_eq!(stats.success_rate(), 0.95);
        assert_eq!(stats.avg_retries_per_operation(), 0.2);
    }
}
