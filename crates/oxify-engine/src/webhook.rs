//! Webhook triggers for workflows
//!
//! This module provides webhook endpoint management and workflow triggering.

use oxify_model::Workflow;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Unique identifier for a webhook
pub type WebhookId = Uuid;

/// Webhook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook ID
    pub id: WebhookId,

    /// Webhook name
    pub name: String,

    /// Workflow to trigger
    pub workflow_id: Uuid,

    /// Secret for HMAC signature verification
    #[serde(skip_serializing)]
    pub secret: Option<String>,

    /// Enabled status
    pub enabled: bool,

    /// HTTP headers to extract as workflow variables
    pub header_mappings: HashMap<String, String>,

    /// JSON path expressions to extract from payload
    pub payload_mappings: HashMap<String, String>,

    /// Maximum payload size in bytes (default: 1MB)
    pub max_payload_size: usize,

    /// Timestamp when webhook was created
    pub created_at: std::time::SystemTime,

    /// Timestamp when webhook was last triggered
    pub last_triggered: Option<std::time::SystemTime>,

    /// Total trigger count
    pub trigger_count: u64,
}

impl WebhookConfig {
    /// Create a new webhook config
    pub fn new(name: String, workflow_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            workflow_id,
            secret: None,
            enabled: true,
            header_mappings: HashMap::new(),
            payload_mappings: HashMap::new(),
            max_payload_size: 1024 * 1024, // 1MB
            created_at: std::time::SystemTime::now(),
            last_triggered: None,
            trigger_count: 0,
        }
    }

    /// Set secret for signature verification
    pub fn with_secret(mut self, secret: String) -> Self {
        self.secret = Some(secret);
        self
    }

    /// Add header mapping
    pub fn with_header(mut self, header_name: String, var_name: String) -> Self {
        self.header_mappings.insert(header_name, var_name);
        self
    }

    /// Add payload mapping
    pub fn with_payload(mut self, json_path: String, var_name: String) -> Self {
        self.payload_mappings.insert(json_path, var_name);
        self
    }

    /// Verify HMAC signature
    pub fn verify_signature(&self, payload: &[u8], signature: &str) -> bool {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        if let Some(secret) = &self.secret {
            type HmacSha256 = Hmac<Sha256>;

            if let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) {
                mac.update(payload);

                // Try to decode the signature from hex
                if let Ok(sig_bytes) = hex::decode(signature) {
                    return mac.verify_slice(&sig_bytes).is_ok();
                }

                // Try base64 encoding
                if let Ok(sig_bytes) =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature)
                {
                    return mac.verify_slice(&sig_bytes).is_ok();
                }
            }
            false
        } else {
            // No secret configured, accept any request
            true
        }
    }
}

/// Webhook trigger event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookTrigger {
    /// Webhook ID
    pub webhook_id: WebhookId,

    /// Trigger timestamp
    pub triggered_at: std::time::SystemTime,

    /// HTTP headers
    pub headers: HashMap<String, String>,

    /// Request payload (JSON)
    pub payload: serde_json::Value,

    /// Extracted variables
    pub variables: HashMap<String, String>,

    /// Execution ID (if workflow was triggered)
    pub execution_id: Option<Uuid>,

    /// Success status
    pub success: bool,

    /// Error message (if failed)
    pub error: Option<String>,
}

/// Webhook registry
pub struct WebhookRegistry {
    webhooks: Arc<RwLock<HashMap<WebhookId, (WebhookConfig, Workflow)>>>,
    triggers: Arc<RwLock<Vec<WebhookTrigger>>>,
}

impl WebhookRegistry {
    /// Create a new webhook registry
    pub fn new() -> Self {
        Self {
            webhooks: Arc::new(RwLock::new(HashMap::new())),
            triggers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a webhook
    pub fn register(&self, config: WebhookConfig, workflow: Workflow) -> WebhookId {
        let id = config.id;
        self.webhooks
            .write()
            .unwrap()
            .insert(id, (config, workflow));
        tracing::info!("Registered webhook {}", id);
        id
    }

    /// Unregister a webhook
    pub fn unregister(&self, webhook_id: WebhookId) -> bool {
        let removed = self.webhooks.write().unwrap().remove(&webhook_id).is_some();
        if removed {
            tracing::info!("Unregistered webhook {}", webhook_id);
        }
        removed
    }

    /// Get webhook configuration
    pub fn get(&self, webhook_id: WebhookId) -> Option<(WebhookConfig, Workflow)> {
        self.webhooks.read().unwrap().get(&webhook_id).cloned()
    }

    /// List all webhooks
    pub fn list(&self) -> Vec<WebhookConfig> {
        self.webhooks
            .read()
            .unwrap()
            .values()
            .map(|(config, _)| config.clone())
            .collect()
    }

    /// Process a webhook trigger
    pub async fn trigger(
        &self,
        webhook_id: WebhookId,
        headers: HashMap<String, String>,
        payload: serde_json::Value,
        signature: Option<String>,
    ) -> Result<WebhookTrigger, String> {
        // Get webhook config
        let (mut config, workflow) = self
            .get(webhook_id)
            .ok_or_else(|| "Webhook not found".to_string())?;

        // Check if enabled
        if !config.enabled {
            return Err("Webhook is disabled".to_string());
        }

        // Verify signature
        if let Some(sig) = signature {
            let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();
            if !config.verify_signature(&payload_bytes, &sig) {
                return Err("Invalid signature".to_string());
            }
        } else if config.secret.is_some() {
            return Err("Signature required but not provided".to_string());
        }

        // Extract variables from headers
        let mut variables = HashMap::new();
        for (header_name, var_name) in &config.header_mappings {
            if let Some(value) = headers.get(header_name) {
                variables.insert(var_name.clone(), value.clone());
            }
        }

        // Extract variables from payload (simplified - would use jsonpath in production)
        for (json_path, var_name) in &config.payload_mappings {
            if let Some(value) = extract_json_value(&payload, json_path) {
                variables.insert(var_name.clone(), value);
            }
        }

        // Update trigger stats
        config.last_triggered = Some(std::time::SystemTime::now());
        config.trigger_count += 1;
        self.webhooks
            .write()
            .unwrap()
            .insert(webhook_id, (config.clone(), workflow.clone()));

        // Create trigger event
        let mut trigger = WebhookTrigger {
            webhook_id,
            triggered_at: std::time::SystemTime::now(),
            headers: headers.clone(),
            payload: payload.clone(),
            variables: variables.clone(),
            execution_id: None,
            success: false,
            error: None,
        };

        // Execute workflow (would integrate with Engine in production)
        trigger.execution_id = Some(Uuid::new_v4());
        trigger.success = true;

        // Store trigger event
        self.triggers.write().unwrap().push(trigger.clone());

        Ok(trigger)
    }

    /// Get trigger history
    pub fn get_triggers(&self, webhook_id: Option<WebhookId>) -> Vec<WebhookTrigger> {
        let triggers = self.triggers.read().unwrap();
        if let Some(id) = webhook_id {
            triggers
                .iter()
                .filter(|t| t.webhook_id == id)
                .cloned()
                .collect()
        } else {
            triggers.clone()
        }
    }

    /// Clear old trigger history
    pub fn cleanup_triggers(&self, max_age_seconds: u64) {
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(max_age_seconds);

        self.triggers
            .write()
            .unwrap()
            .retain(|trigger| trigger.triggered_at > cutoff);
    }
}

impl Default for WebhookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract value from JSON using simple path (e.g., "user.name")
fn extract_json_value(value: &serde_json::Value, path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;

    for part in parts {
        current = current.get(part)?;
    }

    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => Some(current.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, Node, NodeKind, WorkflowMetadata};

    #[test]
    fn test_webhook_config_creation() {
        let config = WebhookConfig::new("Test Webhook".to_string(), Uuid::new_v4())
            .with_secret("secret123".to_string())
            .with_header("X-User-ID".to_string(), "user_id".to_string())
            .with_payload("user.email".to_string(), "email".to_string());

        assert_eq!(config.name, "Test Webhook");
        assert_eq!(config.secret, Some("secret123".to_string()));
        assert!(config.enabled);
        assert_eq!(config.header_mappings.len(), 1);
        assert_eq!(config.payload_mappings.len(), 1);
    }

    #[test]
    fn test_webhook_registry() {
        let registry = WebhookRegistry::new();

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test Workflow".to_string()),
            nodes: vec![start.clone(), end.clone()],
            edges: vec![Edge::new(start.id, end.id)],
        };

        let config = WebhookConfig::new("Test Webhook".to_string(), workflow.metadata.id);

        let webhook_id = registry.register(config, workflow);

        let webhooks = registry.list();
        assert_eq!(webhooks.len(), 1);

        let removed = registry.unregister(webhook_id);
        assert!(removed);

        let webhooks = registry.list();
        assert_eq!(webhooks.len(), 0);
    }

    #[test]
    fn test_extract_json_value() {
        let payload = serde_json::json!({
            "user": {
                "name": "John",
                "email": "john@example.com"
            },
            "count": 42
        });

        assert_eq!(
            extract_json_value(&payload, "user.name"),
            Some("John".to_string())
        );
        assert_eq!(
            extract_json_value(&payload, "user.email"),
            Some("john@example.com".to_string())
        );
        assert_eq!(
            extract_json_value(&payload, "count"),
            Some("42".to_string())
        );
        assert_eq!(extract_json_value(&payload, "invalid.path"), None);
    }
}
