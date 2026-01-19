//! Webhook types for event-driven workflow triggering
//!
//! Allows external systems to trigger workflows via HTTP callbacks

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type WebhookId = Uuid;

/// Webhook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Webhook {
    /// Unique webhook identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: WebhookId,

    /// Webhook name
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    /// Workflow to trigger
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub workflow_id: Uuid,

    /// Webhook secret for HMAC verification
    #[serde(skip_serializing)]
    pub secret: String,

    /// Whether webhook is active
    pub enabled: bool,

    /// Event types to listen for (empty = all)
    pub event_types: Vec<String>,

    /// Custom headers to require
    pub required_headers: HashMap<String, String>,

    /// IP whitelist (empty = no restriction)
    pub ip_whitelist: Vec<String>,

    /// Maximum request body size (bytes)
    pub max_body_size: usize,

    /// Timeout for workflow execution (seconds)
    pub timeout_seconds: u32,

    /// Owner user ID
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub owner_id: Uuid,

    /// Creation timestamp
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub created_at: DateTime<Utc>,

    /// Last updated timestamp
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub updated_at: DateTime<Utc>,

    /// Last triggered timestamp
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub last_triggered_at: Option<DateTime<Utc>>,

    /// Total trigger count
    pub trigger_count: u64,

    /// Failed trigger count
    pub failed_count: u64,
}

impl Webhook {
    /// Create a new webhook
    pub fn new(name: String, workflow_id: Uuid, secret: String, owner_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            workflow_id,
            secret,
            enabled: true,
            event_types: Vec::new(),
            required_headers: HashMap::new(),
            ip_whitelist: Vec::new(),
            max_body_size: 1024 * 1024, // 1MB default
            timeout_seconds: 300,       // 5 minutes default
            owner_id,
            created_at: now,
            updated_at: now,
            last_triggered_at: None,
            trigger_count: 0,
            failed_count: 0,
        }
    }

    /// Check if event type matches
    pub fn matches_event(&self, event_type: &str) -> bool {
        self.event_types.is_empty() || self.event_types.contains(&event_type.to_string())
    }

    /// Check if IP is allowed
    pub fn is_ip_allowed(&self, ip: &str) -> bool {
        self.ip_whitelist.is_empty() || self.ip_whitelist.contains(&ip.to_string())
    }

    /// Check required headers
    pub fn validates_headers(&self, headers: &HashMap<String, String>) -> bool {
        for (key, value) in &self.required_headers {
            if headers.get(key) != Some(value) {
                return false;
            }
        }
        true
    }

    /// Increment trigger count
    pub fn increment_trigger(&mut self) {
        self.trigger_count += 1;
        self.last_triggered_at = Some(Utc::now());
    }

    /// Increment failed count
    pub fn increment_failed(&mut self) {
        self.failed_count += 1;
    }

    /// Create safe view without secret
    pub fn to_safe_view(&self) -> WebhookView {
        WebhookView {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            workflow_id: self.workflow_id,
            enabled: self.enabled,
            event_types: self.event_types.clone(),
            owner_id: self.owner_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_triggered_at: self.last_triggered_at,
            trigger_count: self.trigger_count,
            failed_count: self.failed_count,
            success_rate: self.success_rate(),
        }
    }

    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.trigger_count == 0 {
            return 100.0;
        }
        let successful = self.trigger_count - self.failed_count;
        (successful as f64 / self.trigger_count as f64) * 100.0
    }
}

/// Safe webhook view (no secret)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebhookView {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: WebhookId,
    pub name: String,
    pub description: Option<String>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub workflow_id: Uuid,
    pub enabled: bool,
    pub event_types: Vec<String>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub owner_id: Uuid,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub trigger_count: u64,
    pub failed_count: u64,
    pub success_rate: f64,
}

/// Webhook event received
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebhookEvent {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: Uuid,

    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub webhook_id: WebhookId,

    pub event_type: String,

    pub payload: serde_json::Value,

    pub headers: HashMap<String, String>,

    pub source_ip: String,

    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub received_at: DateTime<Utc>,

    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub processed_at: Option<DateTime<Utc>>,

    pub status: WebhookEventStatus,

    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub execution_id: Option<Uuid>,

    pub error_message: Option<String>,
}

/// Webhook event processing status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum WebhookEventStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Rejected,
}

impl std::fmt::Display for WebhookEventStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebhookEventStatus::Pending => write!(f, "PENDING"),
            WebhookEventStatus::Processing => write!(f, "PROCESSING"),
            WebhookEventStatus::Completed => write!(f, "COMPLETED"),
            WebhookEventStatus::Failed => write!(f, "FAILED"),
            WebhookEventStatus::Rejected => write!(f, "REJECTED"),
        }
    }
}

/// Request to create a webhook
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateWebhookRequest {
    pub name: String,
    pub description: Option<String>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub workflow_id: Uuid,
    pub event_types: Vec<String>,
    pub required_headers: Option<HashMap<String, String>>,
    pub ip_whitelist: Option<Vec<String>>,
    pub max_body_size: Option<usize>,
    pub timeout_seconds: Option<u32>,
}

/// Request to update a webhook
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateWebhookRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub event_types: Option<Vec<String>>,
    pub required_headers: Option<HashMap<String, String>>,
    pub ip_whitelist: Option<Vec<String>>,
    pub max_body_size: Option<usize>,
    pub timeout_seconds: Option<u32>,
}

/// Webhook trigger request
#[derive(Debug, Serialize, Deserialize)]
pub struct WebhookTriggerRequest {
    pub event_type: String,
    pub payload: serde_json::Value,
}

/// Response after webhook registration
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebhookRegistrationResponse {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub webhook_id: WebhookId,
    pub webhook_url: String,
    pub secret: String,
    pub created_at: DateTime<Utc>,
}

/// Helper to generate webhook secret
pub fn generate_webhook_secret() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    const SECRET_LEN: usize = 32;

    let mut rng = rand::rng();
    (0..SECRET_LEN)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Helper to verify HMAC signature
pub fn verify_webhook_signature(secret: &str, payload: &[u8], signature: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };

    mac.update(payload);

    let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

    expected == signature
}

/// Helper to create HMAC signature for testing
pub fn create_webhook_signature(secret: &str, payload: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("Invalid secret");
    mac.update(payload);

    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_creation() {
        let workflow_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let webhook = Webhook::new(
            "Test Webhook".to_string(),
            workflow_id,
            "test_secret".to_string(),
            owner_id,
        );

        assert_eq!(webhook.name, "Test Webhook");
        assert_eq!(webhook.workflow_id, workflow_id);
        assert_eq!(webhook.owner_id, owner_id);
        assert!(webhook.enabled);
        assert_eq!(webhook.trigger_count, 0);
        assert_eq!(webhook.failed_count, 0);
    }

    #[test]
    fn test_event_type_matching() {
        let mut webhook = Webhook::new(
            "Test".to_string(),
            Uuid::new_v4(),
            "secret".to_string(),
            Uuid::new_v4(),
        );

        // Empty event_types means match all
        assert!(webhook.matches_event("push"));
        assert!(webhook.matches_event("pull_request"));

        // Specific event types
        webhook.event_types = vec!["push".to_string(), "pull_request".to_string()];
        assert!(webhook.matches_event("push"));
        assert!(webhook.matches_event("pull_request"));
        assert!(!webhook.matches_event("release"));
    }

    #[test]
    fn test_ip_whitelist() {
        let mut webhook = Webhook::new(
            "Test".to_string(),
            Uuid::new_v4(),
            "secret".to_string(),
            Uuid::new_v4(),
        );

        // Empty whitelist means allow all
        assert!(webhook.is_ip_allowed("192.168.1.1"));
        assert!(webhook.is_ip_allowed("10.0.0.1"));

        // With whitelist
        webhook.ip_whitelist = vec!["192.168.1.1".to_string(), "10.0.0.0/8".to_string()];
        assert!(webhook.is_ip_allowed("192.168.1.1"));
        assert!(!webhook.is_ip_allowed("172.16.0.1"));
    }

    #[test]
    fn test_header_validation() {
        let mut webhook = Webhook::new(
            "Test".to_string(),
            Uuid::new_v4(),
            "secret".to_string(),
            Uuid::new_v4(),
        );

        // No required headers
        let headers = HashMap::new();
        assert!(webhook.validates_headers(&headers));

        // With required headers
        webhook
            .required_headers
            .insert("X-Custom-Header".to_string(), "expected-value".to_string());

        // Missing header
        assert!(!webhook.validates_headers(&headers));

        // Wrong value
        let mut headers = HashMap::new();
        headers.insert("X-Custom-Header".to_string(), "wrong-value".to_string());
        assert!(!webhook.validates_headers(&headers));

        // Correct value
        headers.insert("X-Custom-Header".to_string(), "expected-value".to_string());
        assert!(webhook.validates_headers(&headers));
    }

    #[test]
    fn test_trigger_counting() {
        let mut webhook = Webhook::new(
            "Test".to_string(),
            Uuid::new_v4(),
            "secret".to_string(),
            Uuid::new_v4(),
        );

        assert_eq!(webhook.trigger_count, 0);
        assert!(webhook.last_triggered_at.is_none());

        webhook.increment_trigger();
        assert_eq!(webhook.trigger_count, 1);
        assert!(webhook.last_triggered_at.is_some());

        webhook.increment_trigger();
        assert_eq!(webhook.trigger_count, 2);

        webhook.increment_failed();
        assert_eq!(webhook.failed_count, 1);
    }

    #[test]
    fn test_success_rate() {
        let mut webhook = Webhook::new(
            "Test".to_string(),
            Uuid::new_v4(),
            "secret".to_string(),
            Uuid::new_v4(),
        );

        // No triggers yet
        assert_eq!(webhook.success_rate(), 100.0);

        // All successful
        webhook.trigger_count = 10;
        webhook.failed_count = 0;
        assert_eq!(webhook.success_rate(), 100.0);

        // 50% success rate
        webhook.trigger_count = 10;
        webhook.failed_count = 5;
        assert_eq!(webhook.success_rate(), 50.0);

        // All failed
        webhook.trigger_count = 10;
        webhook.failed_count = 10;
        assert_eq!(webhook.success_rate(), 0.0);
    }

    #[test]
    fn test_safe_view() {
        let webhook = Webhook::new(
            "Test Webhook".to_string(),
            Uuid::new_v4(),
            "super_secret_value".to_string(),
            Uuid::new_v4(),
        );

        let view = webhook.to_safe_view();

        assert_eq!(view.id, webhook.id);
        assert_eq!(view.name, webhook.name);
        assert_eq!(view.workflow_id, webhook.workflow_id);
        // Note: secret is not in the view struct
    }

    #[test]
    fn test_webhook_secret_generation() {
        let secret1 = generate_webhook_secret();
        let secret2 = generate_webhook_secret();

        // Secrets should be 32 characters
        assert_eq!(secret1.len(), 32);
        assert_eq!(secret2.len(), 32);

        // Secrets should be different
        assert_ne!(secret1, secret2);

        // Should only contain alphanumeric characters
        assert!(secret1.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_signature_verification() {
        let secret = "test_secret_key";
        let payload = b"test payload data";

        // Create signature
        let signature = create_webhook_signature(secret, payload);

        // Verify should pass
        assert!(verify_webhook_signature(secret, payload, &signature));

        // Wrong secret should fail
        assert!(!verify_webhook_signature(
            "wrong_secret",
            payload,
            &signature
        ));

        // Wrong payload should fail
        assert!(!verify_webhook_signature(
            secret,
            b"wrong payload",
            &signature
        ));

        // Wrong signature format should fail
        assert!(!verify_webhook_signature(
            secret,
            payload,
            "invalid_signature"
        ));
    }

    #[test]
    fn test_webhook_event_status_display() {
        assert_eq!(WebhookEventStatus::Pending.to_string(), "PENDING");
        assert_eq!(WebhookEventStatus::Processing.to_string(), "PROCESSING");
        assert_eq!(WebhookEventStatus::Completed.to_string(), "COMPLETED");
        assert_eq!(WebhookEventStatus::Failed.to_string(), "FAILED");
        assert_eq!(WebhookEventStatus::Rejected.to_string(), "REJECTED");
    }
}
