//! Webhook retry logic for failed deliveries
//!
//! This module provides retry functionality for webhook events that failed to deliver.
//! It uses exponential backoff with configurable parameters.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Configuration for webhook retry behavior
#[derive(Debug, Clone)]
pub struct WebhookRetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay before first retry (seconds)
    pub initial_delay_secs: u64,
    /// Maximum delay between retries (seconds)
    pub max_delay_secs: u64,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Add random jitter to prevent thundering herd
    pub jitter: bool,
}

impl Default for WebhookRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_secs: 60, // 1 minute
            max_delay_secs: 3600,   // 1 hour
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl WebhookRetryConfig {
    /// Create a config for immediate retries (for testing)
    pub fn immediate() -> Self {
        Self {
            max_retries: 3,
            initial_delay_secs: 1,
            max_delay_secs: 10,
            backoff_multiplier: 2.0,
            jitter: false,
        }
    }

    /// Create an aggressive retry config
    pub fn aggressive() -> Self {
        Self {
            max_retries: 10,
            initial_delay_secs: 30,
            max_delay_secs: 7200, // 2 hours
            backoff_multiplier: 1.5,
            jitter: true,
        }
    }

    /// Create a conservative retry config
    pub fn conservative() -> Self {
        Self {
            max_retries: 3,
            initial_delay_secs: 300, // 5 minutes
            max_delay_secs: 3600,    // 1 hour
            backoff_multiplier: 3.0,
            jitter: true,
        }
    }

    /// Calculate delay for a given retry attempt
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay_secs = self.initial_delay_secs as f64
            * self
                .backoff_multiplier
                .powi(attempt.saturating_sub(1) as i32);

        let delay_secs = delay_secs.min(self.max_delay_secs as f64);

        let delay_secs = if self.jitter {
            // Add random jitter (±25%)
            let jitter_factor = 0.75 + (rand::random::<f64>() * 0.5);
            delay_secs * jitter_factor
        } else {
            delay_secs
        };

        Duration::seconds(delay_secs as i64)
    }
}

/// Retry state for a webhook event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryState {
    /// Event ID
    pub event_id: Uuid,
    /// Webhook ID
    pub webhook_id: Uuid,
    /// Number of retry attempts made
    pub retry_count: u32,
    /// Time of last retry attempt
    pub last_retry_at: Option<DateTime<Utc>>,
    /// Time for next retry attempt
    pub next_retry_at: Option<DateTime<Utc>>,
    /// Last error message
    pub last_error: Option<String>,
    /// Whether the event has been permanently failed
    pub permanently_failed: bool,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

impl RetryState {
    /// Create a new retry state for an event
    pub fn new(event_id: Uuid, webhook_id: Uuid) -> Self {
        Self {
            event_id,
            webhook_id,
            retry_count: 0,
            last_retry_at: None,
            next_retry_at: Some(Utc::now()),
            last_error: None,
            permanently_failed: false,
            created_at: Utc::now(),
        }
    }

    /// Check if the event should be retried
    pub fn should_retry(&self, config: &WebhookRetryConfig) -> bool {
        !self.permanently_failed && self.retry_count < config.max_retries
    }

    /// Check if the event is ready for retry
    pub fn is_ready_for_retry(&self) -> bool {
        match self.next_retry_at {
            Some(next_retry) => Utc::now() >= next_retry && !self.permanently_failed,
            None => false,
        }
    }

    /// Record a failed retry attempt
    pub fn record_failure(&mut self, error: String, config: &WebhookRetryConfig) {
        self.retry_count += 1;
        self.last_retry_at = Some(Utc::now());
        self.last_error = Some(error);

        if self.retry_count >= config.max_retries {
            self.permanently_failed = true;
            self.next_retry_at = None;
        } else {
            let delay = config.calculate_delay(self.retry_count);
            self.next_retry_at = Some(Utc::now() + delay);
        }
    }

    /// Record a successful delivery
    pub fn record_success(&mut self) {
        self.last_retry_at = Some(Utc::now());
        self.next_retry_at = None;
        self.last_error = None;
    }
}

/// Webhook retry manager
///
/// Tracks retry state for webhook events and provides methods to:
/// - Queue failed events for retry
/// - Get events pending retry
/// - Execute retry logic
pub struct WebhookRetryManager {
    /// Retry states keyed by event ID
    states: Arc<RwLock<HashMap<Uuid, RetryState>>>,
    /// Retry configuration
    config: WebhookRetryConfig,
}

impl WebhookRetryManager {
    /// Create a new retry manager with default config
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            config: WebhookRetryConfig::default(),
        }
    }

    /// Create a new retry manager with custom config
    pub fn with_config(config: WebhookRetryConfig) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get the retry configuration
    pub fn config(&self) -> &WebhookRetryConfig {
        &self.config
    }

    /// Queue an event for retry
    pub async fn queue_for_retry(&self, event_id: Uuid, webhook_id: Uuid) {
        let mut states = self.states.write().await;

        if let std::collections::hash_map::Entry::Vacant(e) = states.entry(event_id) {
            let state = RetryState::new(event_id, webhook_id);
            e.insert(state);
            debug!(event_id = %event_id, "Queued webhook event for retry");
        }
    }

    /// Record a failed delivery attempt
    pub async fn record_failure(&self, event_id: Uuid, error: String) {
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(&event_id) {
            state.record_failure(error.clone(), &self.config);

            if state.permanently_failed {
                warn!(
                    event_id = %event_id,
                    retry_count = state.retry_count,
                    "Webhook event permanently failed after {} attempts",
                    state.retry_count
                );
            } else {
                debug!(
                    event_id = %event_id,
                    retry_count = state.retry_count,
                    next_retry_at = ?state.next_retry_at,
                    "Scheduled webhook retry"
                );
            }
        }
    }

    /// Record a successful delivery
    pub async fn record_success(&self, event_id: Uuid) {
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(&event_id) {
            state.record_success();
            info!(
                event_id = %event_id,
                retry_count = state.retry_count,
                "Webhook event delivered successfully after {} retries",
                state.retry_count
            );
        }

        // Remove successful events from tracking
        states.remove(&event_id);
    }

    /// Get events that are ready for retry
    pub async fn get_events_pending_retry(&self) -> Vec<RetryState> {
        let states = self.states.read().await;

        states
            .values()
            .filter(|state| state.is_ready_for_retry())
            .cloned()
            .collect()
    }

    /// Get all events in the retry queue
    pub async fn get_all_retry_states(&self) -> Vec<RetryState> {
        let states = self.states.read().await;
        states.values().cloned().collect()
    }

    /// Get retry state for a specific event
    pub async fn get_retry_state(&self, event_id: Uuid) -> Option<RetryState> {
        let states = self.states.read().await;
        states.get(&event_id).cloned()
    }

    /// Remove an event from the retry queue
    pub async fn remove(&self, event_id: Uuid) -> Option<RetryState> {
        let mut states = self.states.write().await;
        states.remove(&event_id)
    }

    /// Clear all retry states
    pub async fn clear(&self) {
        let mut states = self.states.write().await;
        states.clear();
    }

    /// Get statistics about the retry queue
    pub async fn get_stats(&self) -> RetryStats {
        let states = self.states.read().await;

        let total = states.len();
        let pending = states.values().filter(|s| s.is_ready_for_retry()).count();
        let permanently_failed = states.values().filter(|s| s.permanently_failed).count();
        let waiting = total - pending - permanently_failed;

        RetryStats {
            total_events: total,
            pending_retry: pending,
            waiting_for_retry: waiting,
            permanently_failed,
        }
    }

    /// Clean up old permanently failed events
    pub async fn cleanup_old_failures(&self, max_age: Duration) -> usize {
        let mut states = self.states.write().await;
        let cutoff = Utc::now() - max_age;

        let to_remove: Vec<Uuid> = states
            .iter()
            .filter(|(_, state)| {
                state.permanently_failed && state.last_retry_at.is_some_and(|t| t < cutoff)
            })
            .map(|(id, _)| *id)
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            states.remove(&id);
        }

        if count > 0 {
            info!("Cleaned up {} old permanently failed webhook events", count);
        }

        count
    }
}

impl Default for WebhookRetryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the retry queue
#[derive(Debug, Clone)]
pub struct RetryStats {
    /// Total events in the retry queue
    pub total_events: usize,
    /// Events ready for retry now
    pub pending_retry: usize,
    /// Events waiting for their next retry time
    pub waiting_for_retry: usize,
    /// Events that have permanently failed
    pub permanently_failed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = WebhookRetryConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay_secs, 60);
        assert_eq!(config.max_delay_secs, 3600);
    }

    #[test]
    fn test_calculate_delay() {
        let config = WebhookRetryConfig {
            max_retries: 5,
            initial_delay_secs: 60,
            max_delay_secs: 3600,
            backoff_multiplier: 2.0,
            jitter: false,
        };

        // First retry: 60 seconds
        let delay1 = config.calculate_delay(1);
        assert_eq!(delay1.num_seconds(), 60);

        // Second retry: 120 seconds (60 * 2)
        let delay2 = config.calculate_delay(2);
        assert_eq!(delay2.num_seconds(), 120);

        // Third retry: 240 seconds (60 * 4)
        let delay3 = config.calculate_delay(3);
        assert_eq!(delay3.num_seconds(), 240);
    }

    #[test]
    fn test_calculate_delay_max() {
        let config = WebhookRetryConfig {
            max_retries: 10,
            initial_delay_secs: 60,
            max_delay_secs: 300, // 5 minutes max
            backoff_multiplier: 2.0,
            jitter: false,
        };

        // After several retries, should cap at max_delay
        let delay = config.calculate_delay(10);
        assert_eq!(delay.num_seconds(), 300);
    }

    #[test]
    fn test_retry_state_should_retry() {
        let config = WebhookRetryConfig {
            max_retries: 3,
            ..WebhookRetryConfig::default()
        };

        let mut state = RetryState::new(Uuid::new_v4(), Uuid::new_v4());
        assert!(state.should_retry(&config));

        state.retry_count = 2;
        assert!(state.should_retry(&config));

        state.retry_count = 3;
        assert!(!state.should_retry(&config));

        state.retry_count = 1;
        state.permanently_failed = true;
        assert!(!state.should_retry(&config));
    }

    #[tokio::test]
    async fn test_retry_manager_queue() {
        let manager = WebhookRetryManager::with_config(WebhookRetryConfig::immediate());
        let event_id = Uuid::new_v4();
        let webhook_id = Uuid::new_v4();

        manager.queue_for_retry(event_id, webhook_id).await;

        let state = manager.get_retry_state(event_id).await;
        assert!(state.is_some());
        assert_eq!(state.unwrap().retry_count, 0);
    }

    #[tokio::test]
    async fn test_retry_manager_record_failure() {
        let manager = WebhookRetryManager::with_config(WebhookRetryConfig::immediate());
        let event_id = Uuid::new_v4();
        let webhook_id = Uuid::new_v4();

        manager.queue_for_retry(event_id, webhook_id).await;
        manager
            .record_failure(event_id, "Connection timeout".to_string())
            .await;

        let state = manager.get_retry_state(event_id).await.unwrap();
        assert_eq!(state.retry_count, 1);
        assert_eq!(state.last_error, Some("Connection timeout".to_string()));
        assert!(!state.permanently_failed);
    }

    #[tokio::test]
    async fn test_retry_manager_record_success() {
        let manager = WebhookRetryManager::with_config(WebhookRetryConfig::immediate());
        let event_id = Uuid::new_v4();
        let webhook_id = Uuid::new_v4();

        manager.queue_for_retry(event_id, webhook_id).await;
        manager
            .record_failure(event_id, "Connection timeout".to_string())
            .await;
        manager.record_success(event_id).await;

        // Should be removed after success
        let state = manager.get_retry_state(event_id).await;
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_retry_manager_permanent_failure() {
        let config = WebhookRetryConfig {
            max_retries: 2,
            initial_delay_secs: 0,
            max_delay_secs: 1,
            backoff_multiplier: 1.0,
            jitter: false,
        };

        let manager = WebhookRetryManager::with_config(config);
        let event_id = Uuid::new_v4();
        let webhook_id = Uuid::new_v4();

        manager.queue_for_retry(event_id, webhook_id).await;

        // Fail twice
        manager
            .record_failure(event_id, "Error 1".to_string())
            .await;
        manager
            .record_failure(event_id, "Error 2".to_string())
            .await;

        let state = manager.get_retry_state(event_id).await.unwrap();
        assert!(state.permanently_failed);
        assert_eq!(state.retry_count, 2);
    }

    #[tokio::test]
    async fn test_retry_manager_stats() {
        let manager = WebhookRetryManager::with_config(WebhookRetryConfig::immediate());

        // Add some events
        for _ in 0..5 {
            manager
                .queue_for_retry(Uuid::new_v4(), Uuid::new_v4())
                .await;
        }

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_events, 5);
    }

    #[tokio::test]
    async fn test_retry_manager_pending_events() {
        let manager = WebhookRetryManager::with_config(WebhookRetryConfig::immediate());

        let event_id = Uuid::new_v4();
        manager.queue_for_retry(event_id, Uuid::new_v4()).await;

        // Newly queued events should be ready immediately
        let pending = manager.get_events_pending_retry().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_id, event_id);
    }
}
