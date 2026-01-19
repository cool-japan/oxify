//! API types for secrets management

use oxify_model::{SecretAuditLog, SecretView};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Request to create a new secret
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateSecretRequest {
    /// Secret name (unique identifier)
    pub name: String,
    /// Secret value (will be encrypted)
    pub value: String,
}

/// Response after creating a secret
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateSecretResponse {
    /// Secret ID
    pub id: Uuid,
    /// Success message
    pub message: String,
}

/// Request to update a secret's value
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateSecretRequest {
    /// New secret value (will be re-encrypted)
    pub value: String,
}

/// Response after updating a secret
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateSecretResponse {
    /// Success message
    pub message: String,
}

/// Response containing a decrypted secret
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetSecretResponse {
    /// Secret metadata
    pub secret: SecretView,
    /// Decrypted secret value
    pub value: String,
}

/// Response containing list of secrets (without decrypted values)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListSecretsResponse {
    /// List of secret metadata (no decrypted values)
    pub secrets: Vec<SecretView>,
    /// Total number of secrets
    pub total: usize,
}

/// Response after deleting a secret
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeleteSecretResponse {
    /// Success message
    pub message: String,
}

/// Response containing audit logs for a secret
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetSecretAuditLogsResponse {
    /// List of audit log entries
    pub logs: Vec<SecretAuditLog>,
    /// Total number of logs returned
    pub total: usize,
}
