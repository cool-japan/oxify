//! API Key storage with encryption
//!
//! Provides secure storage for LLM provider API keys with AES-256-GCM encryption

use crate::{DatabasePool, EncryptionService, Result, StorageError};
use chrono::{DateTime, Utc};
use oxify_model::EncryptionMetadata;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// LLM Provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    OpenAi,
    Anthropic,
    Ollama,
    AzureOpenAi,
    Cohere,
    Huggingface,
    Custom,
}

impl LlmProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmProvider::OpenAi => "openai",
            LlmProvider::Anthropic => "anthropic",
            LlmProvider::Ollama => "ollama",
            LlmProvider::AzureOpenAi => "azure_openai",
            LlmProvider::Cohere => "cohere",
            LlmProvider::Huggingface => "huggingface",
            LlmProvider::Custom => "custom",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "openai" => Some(LlmProvider::OpenAi),
            "anthropic" => Some(LlmProvider::Anthropic),
            "ollama" => Some(LlmProvider::Ollama),
            "azure_openai" => Some(LlmProvider::AzureOpenAi),
            "cohere" => Some(LlmProvider::Cohere),
            "huggingface" => Some(LlmProvider::Huggingface),
            "custom" => Some(LlmProvider::Custom),
            _ => None,
        }
    }

    pub fn default_base_url(&self) -> Option<&'static str> {
        match self {
            LlmProvider::OpenAi => Some("https://api.openai.com/v1"),
            LlmProvider::Anthropic => Some("https://api.anthropic.com/v1"),
            LlmProvider::Ollama => Some("http://localhost:11434"),
            LlmProvider::AzureOpenAi => None, // Must be configured per deployment
            LlmProvider::Cohere => Some("https://api.cohere.ai/v1"),
            LlmProvider::Huggingface => Some("https://api-inference.huggingface.co"),
            LlmProvider::Custom => None,
        }
    }
}

impl std::fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// API Key record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub provider: LlmProvider,
    pub provider_url: Option<String>,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub usage_count: u64,
    pub usage_limit: Option<u64>,
    pub is_active: bool,
    pub is_default: bool,
    pub allowed_workflows: Vec<Uuid>,
    pub allowed_users: Vec<Uuid>,
    pub rate_limit_per_minute: Option<i32>,
}

/// API Key view (without sensitive data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyView {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub provider: LlmProvider,
    pub provider_url: Option<String>,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub usage_count: u64,
    pub usage_limit: Option<u64>,
    pub is_active: bool,
    pub is_default: bool,
    pub is_expired: bool,
    pub rate_limit_per_minute: Option<i32>,
}

/// API Key usage log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyUsageLog {
    pub id: Uuid,
    pub api_key_id: Uuid,
    pub workflow_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
    pub model: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
    pub estimated_cost: Option<f64>,
    pub request_type: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub latency_ms: Option<i32>,
    pub timestamp: DateTime<Utc>,
}

/// API Key usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyUsageStats {
    pub api_key_id: Uuid,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub avg_latency_ms: Option<f64>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// API Key storage with encryption
pub struct ApiKeyStore {
    pool: DatabasePool,
    encryption: Arc<EncryptionService>,
}

impl ApiKeyStore {
    /// Create a new API key store
    pub fn new(pool: DatabasePool, encryption_key: Vec<u8>) -> Self {
        Self {
            pool,
            encryption: Arc::new(EncryptionService::new(encryption_key)),
        }
    }

    /// Create a new API key (encrypts the key before storage)
    pub async fn create(
        &self,
        name: String,
        api_key: String,
        provider: LlmProvider,
        owner_id: Uuid,
        description: Option<String>,
        provider_url: Option<String>,
    ) -> Result<ApiKey> {
        // Encrypt the API key
        let (encrypted_key, encryption_metadata) = self
            .encryption
            .encrypt(&api_key)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        let id = Uuid::new_v4();
        let now = Utc::now();
        let provider_str = provider.as_str();

        sqlx::query(
            r"
            INSERT INTO api_keys (
                id, name, description, provider, provider_url,
                encrypted_key, encryption_algorithm, encryption_kdf, encryption_salt, encryption_iv, encryption_key_version,
                owner_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ",
        )
        .bind(id)
        .bind(&name)
        .bind(&description)
        .bind(provider_str)
        .bind(&provider_url)
        .bind(&encrypted_key)
        .bind(&encryption_metadata.algorithm)
        .bind(&encryption_metadata.kdf)
        .bind(&encryption_metadata.salt)
        .bind(&encryption_metadata.iv)
        .bind(encryption_metadata.key_version as i32)
        .bind(owner_id)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await?;

        Ok(ApiKey {
            id,
            name,
            description,
            provider,
            provider_url,
            owner_id,
            created_at: now,
            updated_at: now,
            last_used_at: None,
            expires_at: None,
            usage_count: 0,
            usage_limit: None,
            is_active: true,
            is_default: false,
            allowed_workflows: Vec::new(),
            allowed_users: Vec::new(),
            rate_limit_per_minute: None,
        })
    }

    /// Get an API key by ID and decrypt it
    pub async fn get(&self, id: &Uuid, user_id: &Uuid) -> Result<Option<(ApiKey, String)>> {
        let row = sqlx::query(
            r"
            SELECT
                id, name, description, provider, provider_url,
                encrypted_key, encryption_algorithm, encryption_kdf, encryption_salt, encryption_iv, encryption_key_version,
                owner_id, created_at, updated_at, last_used_at, expires_at,
                usage_count, usage_limit, is_active, is_default,
                allowed_workflows, allowed_users, rate_limit_per_minute
            FROM api_keys
            WHERE id = $1 AND (owner_id = $2 OR $2 = ANY(allowed_users::uuid[]))
            ",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(self.pool.pool())
        .await?;

        if let Some(row) = row {
            let encrypted_key: Vec<u8> = row.get("encrypted_key");

            let encryption_metadata = oxify_model::EncryptionMetadata {
                algorithm: row.get("encryption_algorithm"),
                kdf: row.get("encryption_kdf"),
                salt: row.get("encryption_salt"),
                iv: row.get("encryption_iv"),
                key_version: row.get::<i32, _>("encryption_key_version") as u32,
            };

            // Decrypt the API key
            let decrypted_key = self
                .encryption
                .decrypt(&encrypted_key, &encryption_metadata)
                .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

            let provider_str: String = row.get("provider");
            let provider = LlmProvider::from_str(&provider_str).unwrap_or(LlmProvider::Custom);

            let api_key = ApiKey {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                provider,
                provider_url: row.get("provider_url"),
                owner_id: row.get("owner_id"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                last_used_at: row.get("last_used_at"),
                expires_at: row.get("expires_at"),
                usage_count: row.get::<i64, _>("usage_count") as u64,
                usage_limit: row.get::<Option<i64>, _>("usage_limit").map(|v| v as u64),
                is_active: row.get("is_active"),
                is_default: row.get("is_default"),
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
                rate_limit_per_minute: row.get("rate_limit_per_minute"),
            };

            // Update last used timestamp
            sqlx::query("UPDATE api_keys SET last_used_at = NOW(), usage_count = usage_count + 1 WHERE id = $1")
                .bind(id)
                .execute(self.pool.pool())
                .await?;

            Ok(Some((api_key, decrypted_key)))
        } else {
            Ok(None)
        }
    }

    /// Get the default API key for a provider
    pub async fn get_default_for_provider(
        &self,
        provider: LlmProvider,
        owner_id: &Uuid,
    ) -> Result<Option<(ApiKey, String)>> {
        let row = sqlx::query(
            r"
            SELECT id
            FROM api_keys
            WHERE provider = $1 AND owner_id = $2 AND is_default = true AND is_active = true
            ",
        )
        .bind(provider.as_str())
        .bind(owner_id)
        .fetch_optional(self.pool.pool())
        .await?;

        if let Some(row) = row {
            let id: Uuid = row.get("id");
            self.get(&id, owner_id).await
        } else {
            Ok(None)
        }
    }

    /// List API keys for an owner (returns safe views without decrypted keys)
    pub async fn list_by_owner(&self, owner_id: &Uuid) -> Result<Vec<ApiKeyView>> {
        let rows = sqlx::query(
            r"
            SELECT
                id, name, description, provider, provider_url,
                owner_id, created_at, updated_at, last_used_at, expires_at,
                usage_count, usage_limit, is_active, is_default, rate_limit_per_minute
            FROM api_keys
            WHERE owner_id = $1
            ORDER BY created_at DESC
            ",
        )
        .bind(owner_id)
        .fetch_all(self.pool.pool())
        .await?;

        let views = rows
            .iter()
            .map(|row| {
                let provider_str: String = row.get("provider");
                let provider = LlmProvider::from_str(&provider_str).unwrap_or(LlmProvider::Custom);
                let expires_at: Option<DateTime<Utc>> = row.get("expires_at");
                let is_expired = expires_at.is_some_and(|exp| Utc::now() > exp);

                ApiKeyView {
                    id: row.get("id"),
                    name: row.get("name"),
                    description: row.get("description"),
                    provider,
                    provider_url: row.get("provider_url"),
                    owner_id: row.get("owner_id"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                    last_used_at: row.get("last_used_at"),
                    expires_at,
                    usage_count: row.get::<i64, _>("usage_count") as u64,
                    usage_limit: row.get::<Option<i64>, _>("usage_limit").map(|v| v as u64),
                    is_active: row.get("is_active"),
                    is_default: row.get("is_default"),
                    is_expired,
                    rate_limit_per_minute: row.get("rate_limit_per_minute"),
                }
            })
            .collect();

        Ok(views)
    }

    /// List API keys by provider
    pub async fn list_by_provider(
        &self,
        provider: LlmProvider,
        owner_id: &Uuid,
    ) -> Result<Vec<ApiKeyView>> {
        let rows = sqlx::query(
            r"
            SELECT
                id, name, description, provider, provider_url,
                owner_id, created_at, updated_at, last_used_at, expires_at,
                usage_count, usage_limit, is_active, is_default, rate_limit_per_minute
            FROM api_keys
            WHERE provider = $1 AND owner_id = $2
            ORDER BY is_default DESC, created_at DESC
            ",
        )
        .bind(provider.as_str())
        .bind(owner_id)
        .fetch_all(self.pool.pool())
        .await?;

        let views = rows
            .iter()
            .map(|row| {
                let provider_str: String = row.get("provider");
                let provider = LlmProvider::from_str(&provider_str).unwrap_or(LlmProvider::Custom);
                let expires_at: Option<DateTime<Utc>> = row.get("expires_at");
                let is_expired = expires_at.is_some_and(|exp| Utc::now() > exp);

                ApiKeyView {
                    id: row.get("id"),
                    name: row.get("name"),
                    description: row.get("description"),
                    provider,
                    provider_url: row.get("provider_url"),
                    owner_id: row.get("owner_id"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                    last_used_at: row.get("last_used_at"),
                    expires_at,
                    usage_count: row.get::<i64, _>("usage_count") as u64,
                    usage_limit: row.get::<Option<i64>, _>("usage_limit").map(|v| v as u64),
                    is_active: row.get("is_active"),
                    is_default: row.get("is_default"),
                    is_expired,
                    rate_limit_per_minute: row.get("rate_limit_per_minute"),
                }
            })
            .collect();

        Ok(views)
    }

    /// Update an API key's value (re-encrypts with new encryption)
    pub async fn update_key(
        &self,
        id: &Uuid,
        new_api_key: String,
        owner_id: &Uuid,
    ) -> Result<bool> {
        // Encrypt the new key
        let (encrypted_key, encryption_metadata) = self
            .encryption
            .encrypt(&new_api_key)
            .map_err(|e| StorageError::EncryptionError(e.to_string()))?;

        let result = sqlx::query(
            r"
            UPDATE api_keys SET
                encrypted_key = $1,
                encryption_algorithm = $2,
                encryption_kdf = $3,
                encryption_salt = $4,
                encryption_iv = $5,
                encryption_key_version = $6,
                updated_at = NOW()
            WHERE id = $7 AND owner_id = $8
            ",
        )
        .bind(&encrypted_key)
        .bind(&encryption_metadata.algorithm)
        .bind(&encryption_metadata.kdf)
        .bind(&encryption_metadata.salt)
        .bind(&encryption_metadata.iv)
        .bind(encryption_metadata.key_version as i32)
        .bind(id)
        .bind(owner_id)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Update API key metadata (name, description, etc.)
    #[allow(clippy::too_many_arguments)]
    pub async fn update_metadata(
        &self,
        id: &Uuid,
        name: Option<String>,
        description: Option<String>,
        is_active: Option<bool>,
        expires_at: Option<DateTime<Utc>>,
        usage_limit: Option<u64>,
        rate_limit_per_minute: Option<i32>,
        owner_id: &Uuid,
    ) -> Result<bool> {
        let result = sqlx::query(
            r"
            UPDATE api_keys SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                is_active = COALESCE($4, is_active),
                expires_at = COALESCE($5, expires_at),
                usage_limit = COALESCE($6, usage_limit),
                rate_limit_per_minute = COALESCE($7, rate_limit_per_minute),
                updated_at = NOW()
            WHERE id = $1 AND owner_id = $8
            ",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(is_active)
        .bind(expires_at)
        .bind(usage_limit.map(|v| v as i64))
        .bind(rate_limit_per_minute)
        .bind(owner_id)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Set an API key as the default for its provider
    pub async fn set_default(&self, id: &Uuid, owner_id: &Uuid) -> Result<bool> {
        // First, get the provider of this key
        let row = sqlx::query(
            r"
            SELECT provider FROM api_keys WHERE id = $1 AND owner_id = $2
            ",
        )
        .bind(id)
        .bind(owner_id)
        .fetch_optional(self.pool.pool())
        .await?;

        let provider: String = match row {
            Some(r) => r.get("provider"),
            None => return Ok(false),
        };

        // Unset all defaults for this provider
        sqlx::query(
            r"
            UPDATE api_keys SET is_default = false
            WHERE provider = $1 AND owner_id = $2 AND is_default = true
            ",
        )
        .bind(&provider)
        .bind(owner_id)
        .execute(self.pool.pool())
        .await?;

        // Set this key as default
        let result = sqlx::query(
            r"
            UPDATE api_keys SET is_default = true, updated_at = NOW()
            WHERE id = $1 AND owner_id = $2
            ",
        )
        .bind(id)
        .bind(owner_id)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete an API key
    pub async fn delete(&self, id: &Uuid, owner_id: &Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM api_keys WHERE id = $1 AND owner_id = $2")
            .bind(id)
            .bind(owner_id)
            .execute(self.pool.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Log API key usage
    #[allow(clippy::too_many_arguments)]
    pub async fn log_usage(
        &self,
        api_key_id: &Uuid,
        workflow_id: Option<Uuid>,
        execution_id: Option<Uuid>,
        model: Option<String>,
        input_tokens: Option<i32>,
        output_tokens: Option<i32>,
        request_type: Option<String>,
        success: bool,
        error_message: Option<String>,
        latency_ms: Option<i32>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let total_tokens = match (input_tokens, output_tokens) {
            (Some(i), Some(o)) => Some(i + o),
            _ => None,
        };

        // Calculate estimated cost based on tokens (simplified pricing)
        let estimated_cost = total_tokens.map(|t| f64::from(t) * 0.00002); // $0.02 per 1K tokens avg

        sqlx::query(
            r"
            INSERT INTO api_key_usage_logs (
                id, api_key_id, workflow_id, execution_id, model,
                input_tokens, output_tokens, total_tokens, estimated_cost,
                request_type, success, error_message, latency_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ",
        )
        .bind(id)
        .bind(api_key_id)
        .bind(workflow_id)
        .bind(execution_id)
        .bind(model)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(total_tokens)
        .bind(estimated_cost)
        .bind(request_type)
        .bind(success)
        .bind(error_message)
        .bind(latency_ms)
        .execute(self.pool.pool())
        .await?;

        Ok(id)
    }

    /// Get usage logs for an API key
    pub async fn get_usage_logs(
        &self,
        api_key_id: &Uuid,
        limit: i64,
    ) -> Result<Vec<ApiKeyUsageLog>> {
        let rows = sqlx::query(
            r"
            SELECT id, api_key_id, workflow_id, execution_id, model,
                   input_tokens, output_tokens, total_tokens, estimated_cost,
                   request_type, success, error_message, latency_ms, timestamp
            FROM api_key_usage_logs
            WHERE api_key_id = $1
            ORDER BY timestamp DESC
            LIMIT $2
            ",
        )
        .bind(api_key_id)
        .bind(limit)
        .fetch_all(self.pool.pool())
        .await?;

        let logs = rows
            .iter()
            .map(|row| {
                let estimated_cost: Option<rust_decimal::Decimal> = row.get("estimated_cost");
                ApiKeyUsageLog {
                    id: row.get("id"),
                    api_key_id: row.get("api_key_id"),
                    workflow_id: row.get("workflow_id"),
                    execution_id: row.get("execution_id"),
                    model: row.get("model"),
                    input_tokens: row.get("input_tokens"),
                    output_tokens: row.get("output_tokens"),
                    total_tokens: row.get("total_tokens"),
                    estimated_cost: estimated_cost.map(|d| d.to_string().parse().unwrap_or(0.0)),
                    request_type: row.get("request_type"),
                    success: row.get("success"),
                    error_message: row.get("error_message"),
                    latency_ms: row.get("latency_ms"),
                    timestamp: row.get("timestamp"),
                }
            })
            .collect();

        Ok(logs)
    }

    /// Get usage statistics for an API key
    pub async fn get_usage_stats(&self, api_key_id: &Uuid) -> Result<Option<ApiKeyUsageStats>> {
        #[derive(sqlx::FromRow)]
        struct StatsRow {
            total_requests: i64,
            successful_requests: i64,
            failed_requests: i64,
            total_tokens: Option<i64>,
            total_cost: Option<rust_decimal::Decimal>,
            avg_latency_ms: Option<f64>,
            last_used_at: Option<DateTime<Utc>>,
        }

        let row = sqlx::query_as::<_, StatsRow>(
            r"
            SELECT
                COUNT(*) as total_requests,
                SUM(CASE WHEN success THEN 1 ELSE 0 END) as successful_requests,
                SUM(CASE WHEN NOT success THEN 1 ELSE 0 END) as failed_requests,
                SUM(total_tokens)::BIGINT as total_tokens,
                SUM(estimated_cost) as total_cost,
                AVG(latency_ms)::FLOAT8 as avg_latency_ms,
                MAX(timestamp) as last_used_at
            FROM api_key_usage_logs
            WHERE api_key_id = $1
            ",
        )
        .bind(api_key_id)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(ApiKeyUsageStats {
                api_key_id: *api_key_id,
                total_requests: row.total_requests as u64,
                successful_requests: row.successful_requests as u64,
                failed_requests: row.failed_requests as u64,
                total_tokens: row.total_tokens.unwrap_or(0) as u64,
                total_cost: row
                    .total_cost
                    .map_or(0.0, |d| d.to_string().parse().unwrap_or(0.0)),
                avg_latency_ms: row.avg_latency_ms,
                last_used_at: row.last_used_at,
            })),
            None => Ok(None),
        }
    }

    /// Check if an API key has exceeded its usage limit
    pub async fn check_usage_limit(&self, id: &Uuid) -> Result<bool> {
        #[derive(sqlx::FromRow)]
        struct LimitRow {
            usage_count: i64,
            usage_limit: Option<i64>,
        }

        let row = sqlx::query_as::<_, LimitRow>(
            r"
            SELECT usage_count, usage_limit
            FROM api_keys
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => match row.usage_limit {
                Some(limit) => Ok(row.usage_count < limit),
                None => Ok(true), // No limit
            },
            None => Ok(false), // Key not found
        }
    }

    /// Rotate encryption key for all API keys
    /// This re-encrypts all API keys with a new master key
    /// Returns the number of API keys re-encrypted
    ///
    /// # Warning
    /// This is a potentially dangerous operation. Make sure you have:
    /// 1. A backup of the database
    /// 2. The correct old key to decrypt existing API keys
    /// 3. The new key properly stored and backed up
    pub async fn rotate_encryption_key(&self, old_key: Vec<u8>, new_key: Vec<u8>) -> Result<u64> {
        // Create encryption services for old and new keys
        let old_encryption = EncryptionService::new(old_key);
        let new_encryption = EncryptionService::new(new_key);

        // Get all API keys
        let rows = sqlx::query(
            r"
            SELECT id, encrypted_api_key,
                   encryption_algorithm, encryption_kdf, encryption_salt, encryption_iv, encryption_key_version
            FROM api_keys
            ",
        )
        .fetch_all(self.pool.pool())
        .await?;

        let mut rotated_count = 0u64;

        // Process each API key
        for row in rows {
            let api_key_id: Uuid = row.get("id");
            let encrypted_value: Vec<u8> = row.get("encrypted_api_key");

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
                        "Failed to decrypt API key {api_key_id}: {e}"
                    ))
                })?;

            // Re-encrypt with new key
            let (new_encrypted_value, new_metadata) =
                new_encryption.encrypt(&plaintext).map_err(|e| {
                    crate::StorageError::EncryptionError(format!(
                        "Failed to re-encrypt API key {api_key_id}: {e}"
                    ))
                })?;

            // Update database
            let now = chrono::Utc::now();
            sqlx::query(
                r"
                UPDATE api_keys SET
                    encrypted_api_key = $1,
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
            .bind(api_key_id)
            .execute(self.pool.pool())
            .await?;

            rotated_count += 1;
        }

        Ok(rotated_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_conversion() {
        assert_eq!(LlmProvider::OpenAi.as_str(), "openai");
        assert_eq!(LlmProvider::Anthropic.as_str(), "anthropic");
        assert_eq!(LlmProvider::from_str("openai"), Some(LlmProvider::OpenAi));
        assert_eq!(
            LlmProvider::from_str("anthropic"),
            Some(LlmProvider::Anthropic)
        );
        assert_eq!(LlmProvider::from_str("invalid"), None);
    }

    #[test]
    fn test_provider_default_urls() {
        assert_eq!(
            LlmProvider::OpenAi.default_base_url(),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(
            LlmProvider::Anthropic.default_base_url(),
            Some("https://api.anthropic.com/v1")
        );
        assert_eq!(
            LlmProvider::Ollama.default_base_url(),
            Some("http://localhost:11434")
        );
        assert_eq!(LlmProvider::Custom.default_base_url(), None);
    }
}
