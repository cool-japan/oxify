//! Webhook delivery service with retry support
//!
//! This module provides HTTP delivery of webhook events with automatic retry
//! handling using exponential backoff.
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{WebhookDeliveryService, WebhookDeliveryConfig};
//!
//! let config = WebhookDeliveryConfig::default();
//! let service = WebhookDeliveryService::new(config);
//!
//! // Deliver a webhook event
//! let result = service.deliver(
//!     "https://example.com/webhook",
//!     &payload,
//!     &headers,
//!     Some("webhook_secret"),
//! ).await;
//! ```

use crate::webhook_retry::{RetryState, WebhookRetryConfig, WebhookRetryManager};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for webhook delivery
#[derive(Debug, Clone)]
pub struct WebhookDeliveryConfig {
    /// HTTP client timeout for delivery attempts
    pub timeout_secs: u64,
    /// Maximum request body size (bytes)
    pub max_body_size: usize,
    /// User-Agent header to send
    pub user_agent: String,
    /// Retry configuration
    pub retry_config: WebhookRetryConfig,
    /// Number of concurrent delivery workers
    pub max_concurrent_deliveries: usize,
}

impl Default for WebhookDeliveryConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_body_size: 10 * 1024 * 1024, // 10MB
            user_agent: "OxiFY-Webhook/1.0".to_string(),
            retry_config: WebhookRetryConfig::default(),
            max_concurrent_deliveries: 10,
        }
    }
}

/// Result of a webhook delivery attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    /// Event ID being delivered
    pub event_id: Uuid,
    /// Whether delivery was successful
    pub success: bool,
    /// HTTP status code (if received)
    pub status_code: Option<u16>,
    /// Response body (truncated if large)
    pub response_body: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// Delivery attempt number
    pub attempt: u32,
    /// Time taken for this attempt (ms)
    pub duration_ms: u64,
    /// Timestamp of this attempt
    pub timestamp: DateTime<Utc>,
}

/// Webhook event to deliver
#[derive(Debug, Clone)]
pub struct WebhookDeliveryEvent {
    /// Unique event ID
    pub event_id: Uuid,
    /// Webhook ID this event belongs to
    pub webhook_id: Uuid,
    /// Target URL
    pub url: String,
    /// Payload to send
    pub payload: serde_json::Value,
    /// Headers to include
    pub headers: HashMap<String, String>,
    /// Secret for HMAC signature
    pub secret: Option<String>,
    /// When this event was created
    pub created_at: DateTime<Utc>,
}

/// Delivery statistics
#[derive(Debug, Clone, Default)]
pub struct DeliveryStats {
    /// Total delivery attempts
    pub total_attempts: u64,
    /// Successful deliveries
    pub successful_deliveries: u64,
    /// Failed deliveries (after all retries)
    pub failed_deliveries: u64,
    /// Currently pending deliveries
    pub pending_deliveries: u64,
    /// Average delivery time (ms)
    pub avg_delivery_time_ms: f64,
}

/// Webhook delivery service with retry support
pub struct WebhookDeliveryService {
    /// HTTP client
    client: reqwest::Client,
    /// Service configuration
    config: WebhookDeliveryConfig,
    /// Retry manager
    retry_manager: WebhookRetryManager,
    /// Delivery statistics
    stats: Arc<RwLock<DeliveryStats>>,
    /// Pending events queue
    pending_events: Arc<RwLock<HashMap<Uuid, WebhookDeliveryEvent>>>,
}

impl WebhookDeliveryService {
    /// Create a new delivery service
    pub fn new(config: WebhookDeliveryConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent(&config.user_agent)
            .build()
            .expect("Failed to create HTTP client");

        let retry_manager = WebhookRetryManager::with_config(config.retry_config.clone());

        Self {
            client,
            config,
            retry_manager,
            stats: Arc::new(RwLock::new(DeliveryStats::default())),
            pending_events: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with default configuration
    pub fn default_service() -> Self {
        Self::new(WebhookDeliveryConfig::default())
    }

    /// Get the retry manager
    pub fn retry_manager(&self) -> &WebhookRetryManager {
        &self.retry_manager
    }

    /// Get current delivery statistics
    pub async fn get_stats(&self) -> DeliveryStats {
        self.stats.read().await.clone()
    }

    /// Queue an event for delivery
    pub async fn queue_event(&self, event: WebhookDeliveryEvent) {
        let event_id = event.event_id;
        let webhook_id = event.webhook_id;

        // Add to pending events
        {
            let mut pending = self.pending_events.write().await;
            pending.insert(event_id, event);
        }

        // Queue for retry tracking
        self.retry_manager
            .queue_for_retry(event_id, webhook_id)
            .await;

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.pending_deliveries += 1;
        }

        debug!(event_id = %event_id, "Event queued for delivery");
    }

    /// Deliver a webhook event directly (without retry queue)
    pub async fn deliver_direct(
        &self,
        url: &str,
        payload: &serde_json::Value,
        headers: &HashMap<String, String>,
        secret: Option<&str>,
    ) -> DeliveryResult {
        let event_id = Uuid::new_v4();
        self.deliver_event_internal(event_id, url, payload, headers, secret, 1)
            .await
    }

    /// Deliver a queued event
    pub async fn deliver(&self, event_id: Uuid) -> DeliveryResult {
        let event = {
            let pending = self.pending_events.read().await;
            pending.get(&event_id).cloned()
        };

        let Some(event) = event else {
            return DeliveryResult {
                event_id,
                success: false,
                status_code: None,
                response_body: None,
                error: Some("Event not found in queue".to_string()),
                attempt: 0,
                duration_ms: 0,
                timestamp: Utc::now(),
            };
        };

        let retry_state = self.retry_manager.get_retry_state(event_id).await;
        let attempt = retry_state.map_or(1, |s| s.retry_count + 1);

        let result = self
            .deliver_event_internal(
                event_id,
                &event.url,
                &event.payload,
                &event.headers,
                event.secret.as_deref(),
                attempt,
            )
            .await;

        // Update retry state based on result
        if result.success {
            self.retry_manager.record_success(event_id).await;
            // Remove from pending
            {
                let mut pending = self.pending_events.write().await;
                pending.remove(&event_id);
            }
            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.successful_deliveries += 1;
                stats.pending_deliveries = stats.pending_deliveries.saturating_sub(1);
            }
            info!(event_id = %event_id, "Webhook delivered successfully");
        } else {
            let error = result.error.clone().unwrap_or_default();
            self.retry_manager.record_failure(event_id, error).await;

            // Check if permanently failed
            let state = self.retry_manager.get_retry_state(event_id).await;
            if state.is_some_and(|s| s.permanently_failed) {
                // Remove from pending
                {
                    let mut pending = self.pending_events.write().await;
                    pending.remove(&event_id);
                }
                // Update stats
                {
                    let mut stats = self.stats.write().await;
                    stats.failed_deliveries += 1;
                    stats.pending_deliveries = stats.pending_deliveries.saturating_sub(1);
                }
                warn!(event_id = %event_id, "Webhook permanently failed after all retries");
            }
        }

        // Update attempt stats
        {
            let mut stats = self.stats.write().await;
            stats.total_attempts += 1;
            // Update average delivery time
            let total = stats.successful_deliveries + stats.failed_deliveries;
            if total > 0 {
                stats.avg_delivery_time_ms = (stats.avg_delivery_time_ms
                    * (total.saturating_sub(1)) as f64
                    + result.duration_ms as f64)
                    / total as f64;
            }
        }

        result
    }

    /// Process pending retries
    pub async fn process_pending_retries(&self) -> Vec<DeliveryResult> {
        let pending = self.retry_manager.get_events_pending_retry().await;
        let mut results = Vec::new();

        for state in pending {
            let result = self.deliver(state.event_id).await;
            results.push(result);
        }

        results
    }

    /// Get events pending retry
    pub async fn get_pending_retry_events(&self) -> Vec<RetryState> {
        self.retry_manager.get_events_pending_retry().await
    }

    /// Get all retry states
    pub async fn get_all_retry_states(&self) -> Vec<RetryState> {
        self.retry_manager.get_all_retry_states().await
    }

    /// Clear all pending events and retry states
    pub async fn clear(&self) {
        {
            let mut pending = self.pending_events.write().await;
            pending.clear();
        }
        self.retry_manager.clear().await;
        {
            let mut stats = self.stats.write().await;
            stats.pending_deliveries = 0;
        }
    }

    /// Internal delivery implementation
    async fn deliver_event_internal(
        &self,
        event_id: Uuid,
        url: &str,
        payload: &serde_json::Value,
        headers: &HashMap<String, String>,
        secret: Option<&str>,
        attempt: u32,
    ) -> DeliveryResult {
        let start = std::time::Instant::now();

        // Serialize payload
        let body = match serde_json::to_vec(payload) {
            Ok(b) => b,
            Err(e) => {
                return DeliveryResult {
                    event_id,
                    success: false,
                    status_code: None,
                    response_body: None,
                    error: Some(format!("Failed to serialize payload: {e}")),
                    attempt,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timestamp: Utc::now(),
                };
            }
        };

        // Check body size
        if body.len() > self.config.max_body_size {
            return DeliveryResult {
                event_id,
                success: false,
                status_code: None,
                response_body: None,
                error: Some(format!(
                    "Payload too large: {} bytes (max: {})",
                    body.len(),
                    self.config.max_body_size
                )),
                attempt,
                duration_ms: start.elapsed().as_millis() as u64,
                timestamp: Utc::now(),
            };
        }

        // Build headers
        let mut header_map = HeaderMap::new();
        header_map.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );

        // Add custom headers
        for (key, value) in headers {
            if let (Ok(name), Ok(val)) = (HeaderName::from_str(key), HeaderValue::from_str(value)) {
                header_map.insert(name, val);
            }
        }

        // Add HMAC signature if secret provided
        if let Some(secret) = secret {
            let signature = create_hmac_signature(secret, &body);
            if let Ok(val) = HeaderValue::from_str(&signature) {
                header_map.insert(HeaderName::from_static("x-webhook-signature"), val);
            }
        }

        // Add delivery metadata headers
        if let Ok(val) = HeaderValue::from_str(&event_id.to_string()) {
            header_map.insert(HeaderName::from_static("x-webhook-event-id"), val);
        }
        if let Ok(val) = HeaderValue::from_str(&attempt.to_string()) {
            header_map.insert(HeaderName::from_static("x-webhook-attempt"), val);
        }

        debug!(event_id = %event_id, url = %url, attempt = %attempt, "Attempting webhook delivery");

        // Make the request
        let response = self
            .client
            .post(url)
            .headers(header_map)
            .body(body)
            .send()
            .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match response {
            Ok(resp) => {
                let status = resp.status();
                let status_code = status.as_u16();
                let success = status.is_success();

                // Get response body (truncated)
                let response_body = resp
                    .text()
                    .await
                    .ok()
                    .map(|s| s.chars().take(1000).collect::<String>());

                if success {
                    debug!(
                        event_id = %event_id,
                        status = %status_code,
                        duration_ms = %duration_ms,
                        "Webhook delivery successful"
                    );
                } else {
                    warn!(
                        event_id = %event_id,
                        status = %status_code,
                        duration_ms = %duration_ms,
                        "Webhook delivery failed with HTTP error"
                    );
                }

                DeliveryResult {
                    event_id,
                    success,
                    status_code: Some(status_code),
                    response_body,
                    error: if success {
                        None
                    } else {
                        Some(format!("HTTP {status_code}"))
                    },
                    attempt,
                    duration_ms,
                    timestamp: Utc::now(),
                }
            }
            Err(e) => {
                error!(
                    event_id = %event_id,
                    error = %e,
                    duration_ms = %duration_ms,
                    "Webhook delivery failed with network error"
                );

                DeliveryResult {
                    event_id,
                    success: false,
                    status_code: None,
                    response_body: None,
                    error: Some(e.to_string()),
                    attempt,
                    duration_ms,
                    timestamp: Utc::now(),
                }
            }
        }
    }
}

/// Create HMAC-SHA256 signature
fn create_hmac_signature(secret: &str, payload: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("Invalid secret length");
    mac.update(payload);

    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delivery_config_default() {
        let config = WebhookDeliveryConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_body_size, 10 * 1024 * 1024);
        assert_eq!(config.max_concurrent_deliveries, 10);
    }

    #[test]
    fn test_hmac_signature() {
        let secret = "test_secret";
        let payload = b"test payload";
        let signature = create_hmac_signature(secret, payload);
        assert!(signature.starts_with("sha256="));
        assert_eq!(signature.len(), 71); // "sha256=" + 64 hex chars
    }

    #[tokio::test]
    async fn test_delivery_service_creation() {
        let service = WebhookDeliveryService::default_service();
        let stats = service.get_stats().await;
        assert_eq!(stats.total_attempts, 0);
        assert_eq!(stats.successful_deliveries, 0);
        assert_eq!(stats.failed_deliveries, 0);
    }

    #[tokio::test]
    async fn test_queue_event() {
        let service = WebhookDeliveryService::default_service();

        let event = WebhookDeliveryEvent {
            event_id: Uuid::new_v4(),
            webhook_id: Uuid::new_v4(),
            url: "https://example.com/webhook".to_string(),
            payload: serde_json::json!({"test": "data"}),
            headers: HashMap::new(),
            secret: Some("secret".to_string()),
            created_at: Utc::now(),
        };

        service.queue_event(event.clone()).await;

        let stats = service.get_stats().await;
        assert_eq!(stats.pending_deliveries, 1);

        let retry_state = service.retry_manager.get_retry_state(event.event_id).await;
        assert!(retry_state.is_some());
    }

    #[tokio::test]
    async fn test_clear_service() {
        let service = WebhookDeliveryService::default_service();

        // Queue some events
        for _ in 0..3 {
            let event = WebhookDeliveryEvent {
                event_id: Uuid::new_v4(),
                webhook_id: Uuid::new_v4(),
                url: "https://example.com/webhook".to_string(),
                payload: serde_json::json!({"test": "data"}),
                headers: HashMap::new(),
                secret: None,
                created_at: Utc::now(),
            };
            service.queue_event(event).await;
        }

        let stats = service.get_stats().await;
        assert_eq!(stats.pending_deliveries, 3);

        service.clear().await;

        let stats = service.get_stats().await;
        assert_eq!(stats.pending_deliveries, 0);

        let retry_states = service.get_all_retry_states().await;
        assert!(retry_states.is_empty());
    }

    #[tokio::test]
    async fn test_delivery_event_not_found() {
        let service = WebhookDeliveryService::default_service();
        let result = service.deliver(Uuid::new_v4()).await;
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("not found"));
    }

    #[test]
    fn test_delivery_result_serialization() {
        let result = DeliveryResult {
            event_id: Uuid::new_v4(),
            success: true,
            status_code: Some(200),
            response_body: Some("OK".to_string()),
            error: None,
            attempt: 1,
            duration_ms: 150,
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: DeliveryResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result.event_id, deserialized.event_id);
        assert_eq!(result.success, deserialized.success);
        assert_eq!(result.status_code, deserialized.status_code);
    }

    #[tokio::test]
    async fn test_pending_retry_events() {
        let config = WebhookDeliveryConfig {
            retry_config: WebhookRetryConfig::immediate(),
            ..Default::default()
        };
        let service = WebhookDeliveryService::new(config);

        let event = WebhookDeliveryEvent {
            event_id: Uuid::new_v4(),
            webhook_id: Uuid::new_v4(),
            url: "https://example.com/webhook".to_string(),
            payload: serde_json::json!({"test": "data"}),
            headers: HashMap::new(),
            secret: None,
            created_at: Utc::now(),
        };

        service.queue_event(event.clone()).await;

        // Newly queued events should be ready for retry immediately
        let pending = service.get_pending_retry_events().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_id, event.event_id);
    }
}
