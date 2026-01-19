//! Secret storage with encryption
//!
//! Provides secure storage for secrets with AES-256-GCM encryption

use crate::{DatabasePool, EncryptionService, Result, StorageError};
use oxify_model::{
    AccessControl, EncryptionMetadata, Secret, SecretAction, SecretAuditLog, SecretId, SecretView,
};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// Secret storage with encryption
pub struct SecretStore {
    pool: DatabasePool,
    encryption: Arc<EncryptionService>,
}

impl SecretStore {
    /// Create a new secret store
    pub fn new(pool: DatabasePool, encryption_key: Vec<u8>) -> Self {
        Self {
            pool,
            encryption: Arc::new(EncryptionService::new(encryption_key)),
        }
    }

    /// Create a new secret (encrypts the value before storage)
    #[tracing::instrument(skip(self, value), fields(secret_name = %name, owner_id = %owner_id))]
    pub async fn create(&self, name: String, value: String, owner_id: Uuid) -> Result<Secret> {
        // Validate secret name
        if name.trim().is_empty() {
            return Err(StorageError::ValidationError(
                "Secret name cannot be empty".to_string(),
            ));
        }

        // Encrypt the secret value
        let (encrypted_value, encryption_metadata) = self
            .encryption
            .encrypt(&value)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        let secret_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        // Insert into database
        sqlx::query(
            r"
            INSERT INTO secrets (
                id, name, encrypted_value,
                encryption_algorithm, encryption_kdf, encryption_salt, encryption_iv, encryption_key_version,
                allowed_workflows, allowed_users, ip_whitelist, require_mfa,
                owner_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ",
        )
        .bind(secret_id)
        .bind(&name)
        .bind(&encrypted_value)
        .bind(&encryption_metadata.algorithm)
        .bind(&encryption_metadata.kdf)
        .bind(&encryption_metadata.salt)
        .bind(&encryption_metadata.iv)
        .bind(encryption_metadata.key_version as i32)
        .bind(Vec::<String>::new()) // allowed_workflows
        .bind(Vec::<String>::new()) // allowed_users
        .bind(Vec::<String>::new()) // ip_whitelist
        .bind(false) // require_mfa
        .bind(owner_id)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await?;

        // Log the creation
        self.log_access_internal(
            secret_id,
            Some(owner_id),
            None,
            SecretAction::Create,
            true,
            None,
        )
        .await?;

        Ok(Secret {
            id: secret_id,
            name,
            description: None,
            encrypted_value,
            encryption: encryption_metadata,
            tags: Vec::new(),
            owner_id,
            created_at: now,
            updated_at: now,
            last_accessed_at: None,
            expires_at: None,
            access_control: AccessControl::default(),
        })
    }

    /// Get a secret by ID and decrypt it
    #[tracing::instrument(skip(self), fields(secret_id = ?id, user_id = %user_id))]
    pub async fn get(&self, id: &SecretId, user_id: &Uuid) -> Result<Option<(Secret, String)>> {
        let row = sqlx::query(
            r"
            SELECT
                id, name, description, encrypted_value,
                encryption_algorithm, encryption_kdf, encryption_salt, encryption_iv, encryption_key_version,
                tags, owner_id, created_at, updated_at, last_accessed_at, expires_at,
                allowed_workflows, allowed_users, ip_whitelist, require_mfa
            FROM secrets
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await?;

        if let Some(row) = row {
            let encrypted_value: Vec<u8> = row.get("encrypted_value");

            let encryption_metadata = EncryptionMetadata {
                algorithm: row.get("encryption_algorithm"),
                kdf: row.get("encryption_kdf"),
                salt: row.get("encryption_salt"),
                iv: row.get("encryption_iv"),
                key_version: row.get::<i32, _>("encryption_key_version") as u32,
            };

            // Decrypt the value
            let decrypted_value = self
                .encryption
                .decrypt(&encrypted_value, &encryption_metadata)
                .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

            let secret = Secret {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                encrypted_value,
                encryption: encryption_metadata,
                tags: row.get::<Vec<String>, _>("tags"),
                owner_id: row.get("owner_id"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                last_accessed_at: row.get("last_accessed_at"),
                expires_at: row.get("expires_at"),
                access_control: AccessControl {
                    allowed_workflows: row
                        .get::<Vec<String>, _>("allowed_workflows")
                        .iter()
                        .filter_map(|s| Uuid::parse_str(s).ok())
                        .collect(),
                    allowed_users: row
                        .get::<Vec<String>, _>("allowed_users")
                        .iter()
                        .filter_map(|s| Uuid::parse_str(s).ok())
                        .collect(),
                    ip_whitelist: row.get("ip_whitelist"),
                    require_mfa: row.get("require_mfa"),
                },
            };

            // Update last accessed timestamp
            sqlx::query("UPDATE secrets SET last_accessed_at = NOW() WHERE id = $1")
                .bind(id)
                .execute(self.pool.pool())
                .await?;

            // Log the access
            self.log_access_internal(*id, Some(*user_id), None, SecretAction::Read, true, None)
                .await?;

            Ok(Some((secret, decrypted_value)))
        } else {
            Ok(None)
        }
    }

    /// List secrets for an owner (returns safe views)
    pub async fn list_by_owner(&self, owner_id: &Uuid) -> Result<Vec<SecretView>> {
        let rows = sqlx::query(
            r"
            SELECT
                id, name, description, tags, owner_id, created_at, updated_at,
                last_accessed_at, expires_at
            FROM secrets
            WHERE owner_id = $1
            ORDER BY name
            ",
        )
        .bind(owner_id)
        .fetch_all(self.pool.pool())
        .await?;

        let views = rows
            .iter()
            .map(|row| {
                let expires_at: Option<chrono::DateTime<chrono::Utc>> = row.get("expires_at");
                let is_expired = if let Some(exp) = expires_at {
                    chrono::Utc::now() > exp
                } else {
                    false
                };

                SecretView {
                    id: row.get("id"),
                    name: row.get("name"),
                    description: row.get("description"),
                    tags: row.get("tags"),
                    owner_id: row.get("owner_id"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                    last_accessed_at: row.get("last_accessed_at"),
                    expires_at,
                    is_expired,
                }
            })
            .collect();

        Ok(views)
    }

    /// Update a secret's value (re-encrypts with new encryption)
    #[tracing::instrument(skip(self, new_value), fields(secret_id = ?id, user_id = %user_id))]
    pub async fn update(&self, id: &SecretId, new_value: String, user_id: &Uuid) -> Result<bool> {
        // Encrypt the new value
        let (encrypted_value, encryption_metadata) = self
            .encryption
            .encrypt(&new_value)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        let now = chrono::Utc::now();

        let result = sqlx::query(
            r"
            UPDATE secrets SET
                encrypted_value = $1,
                encryption_algorithm = $2,
                encryption_kdf = $3,
                encryption_salt = $4,
                encryption_iv = $5,
                encryption_key_version = $6,
                updated_at = $7
            WHERE id = $8
            ",
        )
        .bind(&encrypted_value)
        .bind(&encryption_metadata.algorithm)
        .bind(&encryption_metadata.kdf)
        .bind(&encryption_metadata.salt)
        .bind(&encryption_metadata.iv)
        .bind(encryption_metadata.key_version as i32)
        .bind(now)
        .bind(id)
        .execute(self.pool.pool())
        .await?;

        // Log the update
        self.log_access_internal(*id, Some(*user_id), None, SecretAction::Update, true, None)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete a secret
    pub async fn delete(&self, id: &SecretId, user_id: &Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM secrets WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await?;

        // Log the deletion
        self.log_access_internal(*id, Some(*user_id), None, SecretAction::Delete, true, None)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Log secret access for audit trail (internal method)
    async fn log_access_internal(
        &self,
        secret_id: SecretId,
        user_id: Option<Uuid>,
        workflow_id: Option<Uuid>,
        action: SecretAction,
        success: bool,
        error_message: Option<String>,
    ) -> Result<()> {
        let log_id = Uuid::new_v4();
        let action_str = action.to_string();
        let timestamp = chrono::Utc::now();

        sqlx::query(
            r"
            INSERT INTO secret_audit_logs (
                id, secret_id, user_id, workflow_id, action, success, error_message, timestamp
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(log_id)
        .bind(secret_id)
        .bind(user_id)
        .bind(workflow_id)
        .bind(&action_str)
        .bind(success)
        .bind(&error_message)
        .bind(timestamp)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Get audit logs for a secret
    pub async fn get_audit_logs(
        &self,
        secret_id: &SecretId,
        limit: i64,
    ) -> Result<Vec<SecretAuditLog>> {
        let rows = sqlx::query(
            r"
            SELECT id, secret_id, user_id, workflow_id, action, ip_address, success, error_message, timestamp
            FROM secret_audit_logs
            WHERE secret_id = $1
            ORDER BY timestamp DESC
            LIMIT $2
            ",
        )
        .bind(secret_id)
        .bind(limit)
        .fetch_all(self.pool.pool())
        .await?;

        let logs = rows
            .iter()
            .map(|row| {
                let action_str: String = row.get("action");
                let action = match action_str.as_str() {
                    "CREATE" => SecretAction::Create,
                    "READ" => SecretAction::Read,
                    "UPDATE" => SecretAction::Update,
                    "DELETE" => SecretAction::Delete,
                    "LIST" => SecretAction::List,
                    "ROTATE" => SecretAction::Rotate,
                    _ => SecretAction::Read,
                };

                SecretAuditLog {
                    id: row.get("id"),
                    secret_id: row.get("secret_id"),
                    user_id: row.get("user_id"),
                    workflow_id: row.get("workflow_id"),
                    action,
                    ip_address: row.get("ip_address"),
                    success: row.get("success"),
                    error_message: row.get("error_message"),
                    timestamp: row.get("timestamp"),
                }
            })
            .collect();

        Ok(logs)
    }

    /// Rotate encryption key for all secrets
    /// This re-encrypts all secrets with a new master key
    /// Returns the number of secrets re-encrypted
    ///
    /// # Warning
    /// This is a potentially dangerous operation. Make sure you have:
    /// 1. A backup of the database
    /// 2. The correct old key to decrypt existing secrets
    /// 3. The new key properly stored and backed up
    pub async fn rotate_encryption_key(&self, old_key: Vec<u8>, new_key: Vec<u8>) -> Result<u64> {
        // Create encryption services for old and new keys
        let old_encryption = EncryptionService::new(old_key);
        let new_encryption = EncryptionService::new(new_key);

        // Get all secrets
        let rows = sqlx::query(
            r"
            SELECT id, encrypted_value,
                   encryption_algorithm, encryption_kdf, encryption_salt, encryption_iv, encryption_key_version
            FROM secrets
            ",
        )
        .fetch_all(self.pool.pool())
        .await?;

        let mut rotated_count = 0u64;

        // Process each secret
        for row in rows {
            let secret_id: Uuid = row.get("id");
            let encrypted_value: Vec<u8> = row.get("encrypted_value");

            // Reconstruct encryption metadata
            let old_metadata = EncryptionMetadata {
                algorithm: row.get("encryption_algorithm"),
                kdf: row.get("encryption_kdf"),
                salt: row.get("encryption_salt"),
                iv: row.get("encryption_iv"),
                key_version: row.get::<i32, _>("encryption_key_version") as u32,
            };

            // Decrypt with old key
            let plaintext = old_encryption
                .decrypt(&encrypted_value, &old_metadata)
                .map_err(|e| {
                    crate::StorageError::EncryptionError(format!(
                        "Failed to decrypt secret {secret_id}: {e}"
                    ))
                })?;

            // Re-encrypt with new key
            let (new_encrypted_value, new_metadata) =
                new_encryption.encrypt(&plaintext).map_err(|e| {
                    crate::StorageError::EncryptionError(format!(
                        "Failed to re-encrypt secret {secret_id}: {e}"
                    ))
                })?;

            // Update database
            let now = chrono::Utc::now();
            sqlx::query(
                r"
                UPDATE secrets SET
                    encrypted_value = $1,
                    encryption_algorithm = $2,
                    encryption_kdf = $3,
                    encryption_salt = $4,
                    encryption_iv = $5,
                    encryption_key_version = $6,
                    updated_at = $7
                WHERE id = $8
                ",
            )
            .bind(&new_encrypted_value)
            .bind(&new_metadata.algorithm)
            .bind(&new_metadata.kdf)
            .bind(&new_metadata.salt)
            .bind(&new_metadata.iv)
            .bind(new_metadata.key_version as i32)
            .bind(now)
            .bind(secret_id)
            .execute(self.pool.pool())
            .await?;

            rotated_count += 1;
        }

        Ok(rotated_count)
    }
}
