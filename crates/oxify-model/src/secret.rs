//! Secret management types
//!
//! Secure storage and access control for sensitive credentials

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type SecretId = Uuid;

/// Secret metadata and encrypted value
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Secret {
    /// Unique secret identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: SecretId,

    /// Secret name (must be unique per user/workspace)
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    /// Encrypted secret value
    #[serde(skip_serializing)]
    pub encrypted_value: Vec<u8>,

    /// Encryption metadata
    pub encryption: EncryptionMetadata,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// User or workspace ID that owns this secret
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub owner_id: Uuid,

    /// Creation timestamp
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub created_at: DateTime<Utc>,

    /// Last updated timestamp
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub updated_at: DateTime<Utc>,

    /// Last accessed timestamp
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub last_accessed_at: Option<DateTime<Utc>>,

    /// Expiration date (optional)
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub expires_at: Option<DateTime<Utc>>,

    /// Access control rules
    pub access_control: AccessControl,
}

/// Encryption metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EncryptionMetadata {
    /// Algorithm used (e.g., "AES-256-GCM")
    pub algorithm: String,

    /// Key derivation function
    pub kdf: String,

    /// Salt for key derivation (base64)
    pub salt: String,

    /// Initialization vector (base64)
    pub iv: String,

    /// Key version for rotation
    pub key_version: u32,
}

/// Access control for secrets
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Default)]
pub struct AccessControl {
    /// Allowed workflow IDs (empty means all)
    pub allowed_workflows: Vec<Uuid>,

    /// Allowed user IDs (empty means owner only)
    pub allowed_users: Vec<Uuid>,

    /// IP whitelist (empty means no restriction)
    pub ip_whitelist: Vec<String>,

    /// Require MFA for access
    pub require_mfa: bool,
}

impl Secret {
    /// Create a new secret (value must be encrypted before)
    pub fn new(
        name: String,
        encrypted_value: Vec<u8>,
        encryption: EncryptionMetadata,
        owner_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            encrypted_value,
            encryption,
            tags: Vec::new(),
            owner_id,
            created_at: now,
            updated_at: now,
            last_accessed_at: None,
            expires_at: None,
            access_control: AccessControl::default(),
        }
    }

    /// Check if secret is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Check if workflow has access
    pub fn can_access_workflow(&self, workflow_id: &Uuid) -> bool {
        if self.access_control.allowed_workflows.is_empty() {
            return true;
        }
        self.access_control.allowed_workflows.contains(workflow_id)
    }

    /// Check if user has access
    pub fn can_access_user(&self, user_id: &Uuid) -> bool {
        if user_id == &self.owner_id {
            return true;
        }
        if self.access_control.allowed_users.is_empty() {
            return false;
        }
        self.access_control.allowed_users.contains(user_id)
    }

    /// Update last accessed timestamp
    pub fn mark_accessed(&mut self) {
        self.last_accessed_at = Some(Utc::now());
    }

    /// Create a safe view without sensitive data
    pub fn to_safe_view(&self) -> SecretView {
        SecretView {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            tags: self.tags.clone(),
            owner_id: self.owner_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_accessed_at: self.last_accessed_at,
            expires_at: self.expires_at,
            is_expired: self.is_expired(),
        }
    }
}

/// Safe view of a secret (no encrypted value)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SecretView {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: SecretId,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub owner_id: Uuid,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub last_accessed_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub expires_at: Option<DateTime<Utc>>,
    pub is_expired: bool,
}

/// Secret reference in workflow context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretReference {
    /// Secret ID or name
    pub identifier: String,

    /// Whether identifier is ID or name
    pub is_id: bool,

    /// Target variable name in context
    pub target_variable: String,
}

/// Audit log entry for secret access
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SecretAuditLog {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: Uuid,

    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub secret_id: SecretId,

    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub user_id: Option<Uuid>,

    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub workflow_id: Option<Uuid>,

    pub action: SecretAction,

    pub ip_address: Option<String>,

    pub success: bool,

    pub error_message: Option<String>,

    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub timestamp: DateTime<Utc>,
}

/// Actions that can be performed on secrets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SecretAction {
    Create,
    Read,
    Update,
    Delete,
    List,
    Rotate,
}

impl std::fmt::Display for SecretAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretAction::Create => write!(f, "CREATE"),
            SecretAction::Read => write!(f, "READ"),
            SecretAction::Update => write!(f, "UPDATE"),
            SecretAction::Delete => write!(f, "DELETE"),
            SecretAction::List => write!(f, "LIST"),
            SecretAction::Rotate => write!(f, "ROTATE"),
        }
    }
}

/// Request to create a secret
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateSecretRequest {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Request to update a secret
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateSecretRequest {
    pub value: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub expires_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_secret() -> Secret {
        let encryption = EncryptionMetadata {
            algorithm: "AES-256-GCM".to_string(),
            kdf: "PBKDF2".to_string(),
            salt: "base64salt".to_string(),
            iv: "base64iv".to_string(),
            key_version: 1,
        };

        Secret::new(
            "test_secret".to_string(),
            vec![1, 2, 3, 4, 5],
            encryption,
            Uuid::new_v4(),
        )
    }

    #[test]
    fn test_secret_creation() {
        let secret = create_test_secret();
        assert_eq!(secret.name, "test_secret");
        assert_eq!(secret.encrypted_value, vec![1, 2, 3, 4, 5]);
        assert_eq!(secret.encryption.algorithm, "AES-256-GCM");
        assert_eq!(secret.tags.len(), 0);
        assert_eq!(secret.description, None);
        assert_eq!(secret.last_accessed_at, None);
        assert_eq!(secret.expires_at, None);
    }

    #[test]
    fn test_secret_not_expired_when_no_expiration() {
        let secret = create_test_secret();
        assert!(!secret.is_expired());
    }

    #[test]
    fn test_secret_not_expired_when_future_expiration() {
        let mut secret = create_test_secret();
        secret.expires_at = Some(Utc::now() + Duration::days(1));
        assert!(!secret.is_expired());
    }

    #[test]
    fn test_secret_expired_when_past_expiration() {
        let mut secret = create_test_secret();
        secret.expires_at = Some(Utc::now() - Duration::days(1));
        assert!(secret.is_expired());
    }

    #[test]
    fn test_workflow_access_allowed_when_empty_list() {
        let secret = create_test_secret();
        let workflow_id = Uuid::new_v4();
        assert!(secret.can_access_workflow(&workflow_id));
    }

    #[test]
    fn test_workflow_access_allowed_when_in_list() {
        let mut secret = create_test_secret();
        let workflow_id = Uuid::new_v4();
        secret.access_control.allowed_workflows.push(workflow_id);
        assert!(secret.can_access_workflow(&workflow_id));
    }

    #[test]
    fn test_workflow_access_denied_when_not_in_list() {
        let mut secret = create_test_secret();
        let allowed_workflow = Uuid::new_v4();
        let other_workflow = Uuid::new_v4();
        secret
            .access_control
            .allowed_workflows
            .push(allowed_workflow);
        assert!(!secret.can_access_workflow(&other_workflow));
    }

    #[test]
    fn test_user_access_allowed_for_owner() {
        let owner_id = Uuid::new_v4();
        let encryption = EncryptionMetadata {
            algorithm: "AES-256-GCM".to_string(),
            kdf: "PBKDF2".to_string(),
            salt: "base64salt".to_string(),
            iv: "base64iv".to_string(),
            key_version: 1,
        };

        let secret = Secret::new(
            "test_secret".to_string(),
            vec![1, 2, 3, 4, 5],
            encryption,
            owner_id,
        );

        assert!(secret.can_access_user(&owner_id));
    }

    #[test]
    fn test_user_access_denied_for_non_owner_when_empty_list() {
        let secret = create_test_secret();
        let other_user = Uuid::new_v4();
        assert!(!secret.can_access_user(&other_user));
    }

    #[test]
    fn test_user_access_allowed_when_in_list() {
        let mut secret = create_test_secret();
        let user_id = Uuid::new_v4();
        secret.access_control.allowed_users.push(user_id);
        assert!(secret.can_access_user(&user_id));
    }

    #[test]
    fn test_user_access_denied_when_not_in_list() {
        let mut secret = create_test_secret();
        let allowed_user = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        secret.access_control.allowed_users.push(allowed_user);
        assert!(!secret.can_access_user(&other_user));
    }

    #[test]
    fn test_mark_accessed_updates_timestamp() {
        let mut secret = create_test_secret();
        assert_eq!(secret.last_accessed_at, None);

        secret.mark_accessed();
        assert!(secret.last_accessed_at.is_some());

        let first_access = secret.last_accessed_at.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        secret.mark_accessed();
        let second_access = secret.last_accessed_at.unwrap();
        assert!(second_access > first_access);
    }

    #[test]
    fn test_safe_view_excludes_encrypted_value() {
        let mut secret = create_test_secret();
        secret.description = Some("Test description".to_string());
        secret.tags.push("tag1".to_string());
        secret.tags.push("tag2".to_string());

        let view = secret.to_safe_view();

        assert_eq!(view.id, secret.id);
        assert_eq!(view.name, secret.name);
        assert_eq!(view.description, secret.description);
        assert_eq!(view.tags, secret.tags);
        assert_eq!(view.owner_id, secret.owner_id);
        assert_eq!(view.created_at, secret.created_at);
        assert_eq!(view.updated_at, secret.updated_at);
        assert_eq!(view.last_accessed_at, secret.last_accessed_at);
        assert_eq!(view.expires_at, secret.expires_at);
        assert_eq!(view.is_expired, secret.is_expired());
    }

    #[test]
    fn test_safe_view_reflects_expiration_status() {
        let mut secret = create_test_secret();
        secret.expires_at = Some(Utc::now() - Duration::days(1));

        let view = secret.to_safe_view();
        assert!(view.is_expired);
    }

    #[test]
    fn test_secret_action_display() {
        assert_eq!(SecretAction::Create.to_string(), "CREATE");
        assert_eq!(SecretAction::Read.to_string(), "READ");
        assert_eq!(SecretAction::Update.to_string(), "UPDATE");
        assert_eq!(SecretAction::Delete.to_string(), "DELETE");
        assert_eq!(SecretAction::List.to_string(), "LIST");
        assert_eq!(SecretAction::Rotate.to_string(), "ROTATE");
    }

    #[test]
    fn test_access_control_default() {
        let ac = AccessControl::default();
        assert_eq!(ac.allowed_workflows.len(), 0);
        assert_eq!(ac.allowed_users.len(), 0);
        assert_eq!(ac.ip_whitelist.len(), 0);
        assert!(!ac.require_mfa);
    }

    #[test]
    fn test_encryption_metadata_fields() {
        let encryption = EncryptionMetadata {
            algorithm: "AES-256-GCM".to_string(),
            kdf: "PBKDF2".to_string(),
            salt: "base64salt".to_string(),
            iv: "base64iv".to_string(),
            key_version: 1,
        };

        assert_eq!(encryption.algorithm, "AES-256-GCM");
        assert_eq!(encryption.kdf, "PBKDF2");
        assert_eq!(encryption.salt, "base64salt");
        assert_eq!(encryption.iv, "base64iv");
        assert_eq!(encryption.key_version, 1);
    }

    #[test]
    fn test_secret_reference() {
        let reference = SecretReference {
            identifier: "my-api-key".to_string(),
            is_id: false,
            target_variable: "api_key".to_string(),
        };

        assert_eq!(reference.identifier, "my-api-key");
        assert!(!reference.is_id);
        assert_eq!(reference.target_variable, "api_key");
    }

    #[test]
    fn test_create_secret_request() {
        let request = CreateSecretRequest {
            name: "test_secret".to_string(),
            value: "secret_value".to_string(),
            description: Some("Test description".to_string()),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            expires_at: None,
        };

        assert_eq!(request.name, "test_secret");
        assert_eq!(request.value, "secret_value");
        assert_eq!(request.description, Some("Test description".to_string()));
        assert_eq!(request.tags.len(), 2);
    }

    #[test]
    fn test_update_secret_request() {
        let request = UpdateSecretRequest {
            value: Some("new_value".to_string()),
            description: Some("New description".to_string()),
            tags: Some(vec!["new_tag".to_string()]),
            expires_at: None,
        };

        assert_eq!(request.value, Some("new_value".to_string()));
        assert_eq!(request.description, Some("New description".to_string()));
        assert!(request.tags.is_some());
    }
}
