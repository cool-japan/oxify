//! Retry logic with exponential backoff

use oxify_model::RetryConfig;
use std::time::Duration;

/// Execute a function with exponential backoff retry logic
pub async fn retry_with_backoff<F, Fut, T, E>(config: &RetryConfig, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 0;
    let mut delay_ms = config.initial_delay_ms;

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                attempt += 1;

                if attempt > config.max_retries {
                    return Err(err);
                }

                // Sleep with exponential backoff
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                // Calculate next delay with exponential backoff
                delay_ms = (delay_ms as f64 * config.backoff_multiplier) as u64;

                // Cap at max delay
                if delay_ms > config.max_delay_ms {
                    delay_ms = config.max_delay_ms;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success_on_first_attempt() {
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let result = retry_with_backoff(&config, move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, String>(42)
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let result = retry_with_backoff(&config, move || {
            let counter = Arc::clone(&counter);
            async move {
                let count = counter.fetch_add(1, Ordering::SeqCst) + 1;
                if count < 3 {
                    Err("temporary error".to_string())
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
    async fn test_retry_exhaust_attempts() {
        let config = RetryConfig {
            max_retries: 2,
            initial_delay_ms: 1,
            backoff_multiplier: 2.0,
            max_delay_ms: 100,
        };
        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let result = retry_with_backoff(&config, move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, String>("permanent error".to_string())
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 3); // initial + 2 retries
    }

    #[tokio::test]
    async fn test_exponential_backoff_timing() {
        let config = RetryConfig {
            max_retries: 3,
            initial_delay_ms: 10,
            backoff_multiplier: 2.0,
            max_delay_ms: 1000,
        };

        let start = std::time::Instant::now();
        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let _result = retry_with_backoff(&config, move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, String>("error".to_string())
            }
        })
        .await;

        let elapsed = start.elapsed().as_millis();

        // Total expected delay: 10ms + 20ms + 40ms = 70ms
        // Allow some variance
        assert!((60..200).contains(&elapsed), "Elapsed: {}ms", elapsed);
        assert_eq!(call_count.load(Ordering::SeqCst), 4); // initial + 3 retries
    }
}
