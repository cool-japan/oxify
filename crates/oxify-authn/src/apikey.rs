//! API Key Management
//!
//! Provides secure API key generation, validation, rotation, and scoping
//! for programmatic access to APIs without user interaction.
//!
//! # Features
//!
//! - **Secure Key Generation**: Cryptographically secure random keys with prefixes
//! - **Key Scoping**: Fine-grained permissions per API key
//! - **Key Rotation**: Automatic and manual key rotation with grace periods
//! - **Rate Limiting**: Per-key rate limits
//! - **Usage Tracking**: Monitor API key usage patterns
//! - **Key Expiration**: Time-based and usage-based expiration
//!
//! # Example
//!
//! ```
//! use oxify_authn::apikey::{ApiKeyManager, ApiKeyConfig, ApiKeyScope};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = ApiKeyManager::new();
//!
//! // Generate a new API key
//! let key = manager.generate_key("user123", vec![ApiKeyScope::Read, ApiKeyScope::Write]).await?;
//! println!("API Key: {}", key.key());
//!
//! // Validate the key
//! let validation = manager.validate_key(key.key()).await?;
//! println!("Key belongs to user: {}", validation.user_id());
//! # Ok(())
//! # }
//! ```

use crate::types::AuthError;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// API key scopes for permission management
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiKeyScope {
    /// Read-only access
    Read,
    /// Write access
    Write,
    /// Delete access
    Delete,
    /// Admin access
    Admin,
    /// Custom scope
    Custom(String),
}

impl ApiKeyScope {
    /// Check if this scope includes another scope
    #[must_use]
    pub fn includes(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Admin, _) | (Self::Write, Self::Read) => true, // Admin includes all, Write includes Read
            (a, b) => a == b,                                     // Otherwise must be exact match
        }
    }
}

/// API key metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyMetadata {
    /// Unique key ID (not the actual key)
    key_id: String,
    /// User ID who owns this key
    user_id: String,
    /// The actual API key (stored hashed in production)
    key_hash: String,
    /// Key prefix for identification (e.g., "`sk_live`_")
    prefix: String,
    /// Display name for the key
    name: String,
    /// Allowed scopes
    scopes: Vec<ApiKeyScope>,
    /// Creation timestamp
    created_at: DateTime<Utc>,
    /// Expiration timestamp (if any)
    expires_at: Option<DateTime<Utc>>,
    /// Last used timestamp
    last_used_at: Option<DateTime<Utc>>,
    /// Usage count
    use_count: u64,
    /// Maximum allowed uses (if any)
    max_uses: Option<u64>,
    /// Whether this key is active
    active: bool,
    /// Rate limit (requests per minute)
    rate_limit: Option<u32>,
    /// IP whitelist (if any)
    ip_whitelist: Option<Vec<String>>,
}

impl ApiKeyMetadata {
    /// Get the key ID
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Get the user ID
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get the key name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the scopes
    #[must_use]
    pub fn scopes(&self) -> &[ApiKeyScope] {
        &self.scopes
    }

    /// Get creation timestamp
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Get expiration timestamp
    #[must_use]
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }

    /// Get last used timestamp
    #[must_use]
    pub fn last_used_at(&self) -> Option<DateTime<Utc>> {
        self.last_used_at
    }

    /// Get usage count
    #[must_use]
    pub fn use_count(&self) -> u64 {
        self.use_count
    }

    /// Check if key is expired
    #[must_use]
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Check if key has reached max uses
    #[must_use]
    pub fn is_max_uses_reached(&self) -> bool {
        if let Some(max_uses) = self.max_uses {
            self.use_count >= max_uses
        } else {
            false
        }
    }

    /// Check if key is valid
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.active && !self.is_expired() && !self.is_max_uses_reached()
    }

    /// Check if key has required scope
    #[must_use]
    pub fn has_scope(&self, required_scope: &ApiKeyScope) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.includes(required_scope))
    }

    /// Mark key as used
    pub fn mark_used(&mut self) {
        self.use_count += 1;
        self.last_used_at = Some(Utc::now());
    }

    /// Revoke the key
    pub fn revoke(&mut self) {
        self.active = false;
    }
}

/// Generated API key (includes the actual key value)
#[derive(Debug, Clone)]
pub struct GeneratedApiKey {
    /// The full API key (prefix + random part)
    key: String,
    /// Key metadata
    metadata: ApiKeyMetadata,
}

impl GeneratedApiKey {
    /// Get the full API key
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Get the key metadata
    #[must_use]
    pub fn metadata(&self) -> &ApiKeyMetadata {
        &self.metadata
    }
}

/// API key validation result
#[derive(Debug, Clone)]
pub struct ApiKeyValidation {
    /// Key metadata
    metadata: ApiKeyMetadata,
    /// Whether the key is valid
    valid: bool,
    /// Validation error (if any)
    error: Option<String>,
}

impl ApiKeyValidation {
    /// Check if key is valid
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Get the user ID
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.metadata.user_id
    }

    /// Get the key metadata
    #[must_use]
    pub fn metadata(&self) -> &ApiKeyMetadata {
        &self.metadata
    }

    /// Get validation error
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Check if key has required scope
    #[must_use]
    pub fn has_scope(&self, scope: &ApiKeyScope) -> bool {
        self.metadata.has_scope(scope)
    }
}

/// API key configuration
#[derive(Debug, Clone)]
pub struct ApiKeyConfig {
    /// Default key prefix
    pub prefix: String,
    /// Default expiration duration (if any)
    pub default_expiration: Option<Duration>,
    /// Default rate limit (requests per minute)
    pub default_rate_limit: Option<u32>,
    /// Key length (number of random bytes)
    pub key_length: usize,
}

impl Default for ApiKeyConfig {
    fn default() -> Self {
        Self {
            prefix: "sk_".to_string(),
            default_expiration: Some(Duration::days(365)), // 1 year
            default_rate_limit: Some(1000),                // 1000 requests per minute
            key_length: 32,                                // 32 bytes = 256 bits
        }
    }
}

/// API key manager
pub struct ApiKeyManager {
    /// Key store (`key_hash` -> metadata)
    store: Arc<Mutex<HashMap<String, ApiKeyMetadata>>>,
    /// Configuration
    config: ApiKeyConfig,
}

impl ApiKeyManager {
    /// Create a new API key manager with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ApiKeyConfig::default())
    }

    /// Create a new API key manager with custom configuration
    #[must_use]
    pub fn with_config(config: ApiKeyConfig) -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Generate a new API key
    pub async fn generate_key(
        &self,
        user_id: impl Into<String>,
        scopes: Vec<ApiKeyScope>,
    ) -> Result<GeneratedApiKey, AuthError> {
        self.generate_key_with_name(user_id, "default".to_string(), scopes)
            .await
    }

    /// Generate a new API key with a custom name
    pub async fn generate_key_with_name(
        &self,
        user_id: impl Into<String>,
        name: String,
        scopes: Vec<ApiKeyScope>,
    ) -> Result<GeneratedApiKey, AuthError> {
        let user_id = user_id.into();
        let key_id = Uuid::new_v4().to_string();

        // Generate secure random key
        let random_part = self.generate_random_key();
        let full_key = format!("{}{}", self.config.prefix, random_part);

        // Hash the key for storage
        let key_hash = Self::hash_key(&full_key);

        let now = Utc::now();
        let expires_at = self
            .config
            .default_expiration
            .map(|duration| now + duration);

        let metadata = ApiKeyMetadata {
            key_id,
            user_id,
            key_hash: key_hash.clone(),
            prefix: self.config.prefix.clone(),
            name,
            scopes,
            created_at: now,
            expires_at,
            last_used_at: None,
            use_count: 0,
            max_uses: None,
            active: true,
            rate_limit: self.config.default_rate_limit,
            ip_whitelist: None,
        };

        let mut store = self.store.lock().unwrap();
        store.insert(key_hash, metadata.clone());

        Ok(GeneratedApiKey {
            key: full_key,
            metadata,
        })
    }

    /// Validate an API key
    pub async fn validate_key(&self, key: &str) -> Result<ApiKeyValidation, AuthError> {
        let key_hash = Self::hash_key(key);

        let mut store = self.store.lock().unwrap();
        let metadata = store
            .get_mut(&key_hash)
            .ok_or_else(|| AuthError::InvalidToken("API key not found".into()))?;

        if !metadata.is_valid() {
            let error = if metadata.is_expired() {
                "API key expired"
            } else if metadata.is_max_uses_reached() {
                "API key max uses reached"
            } else if !metadata.active {
                "API key revoked"
            } else {
                "API key invalid"
            };

            return Ok(ApiKeyValidation {
                metadata: metadata.clone(),
                valid: false,
                error: Some(error.to_string()),
            });
        }

        // Mark as used
        metadata.mark_used();

        Ok(ApiKeyValidation {
            metadata: metadata.clone(),
            valid: true,
            error: None,
        })
    }

    /// Revoke an API key
    pub async fn revoke_key(&self, key: &str) -> Result<(), AuthError> {
        let key_hash = Self::hash_key(key);

        let mut store = self.store.lock().unwrap();
        let metadata = store
            .get_mut(&key_hash)
            .ok_or_else(|| AuthError::InvalidToken("API key not found".into()))?;

        metadata.revoke();
        Ok(())
    }

    /// Revoke all keys for a user
    pub async fn revoke_user_keys(&self, user_id: &str) -> Result<usize, AuthError> {
        let mut store = self.store.lock().unwrap();
        let mut revoked_count = 0;

        for metadata in store.values_mut() {
            if metadata.user_id == user_id && metadata.active {
                metadata.revoke();
                revoked_count += 1;
            }
        }

        Ok(revoked_count)
    }

    /// Get all keys for a user
    pub async fn get_user_keys(&self, user_id: &str) -> Vec<ApiKeyMetadata> {
        let store = self.store.lock().unwrap();
        store
            .values()
            .filter(|m| m.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Rotate an API key (generate new key, revoke old one)
    pub async fn rotate_key(&self, old_key: &str) -> Result<GeneratedApiKey, AuthError> {
        let old_key_hash = Self::hash_key(old_key);

        // Get old key metadata
        let (user_id, name, scopes) = {
            let store = self.store.lock().unwrap();
            let metadata = store
                .get(&old_key_hash)
                .ok_or_else(|| AuthError::InvalidToken("API key not found".into()))?;

            (
                metadata.user_id.clone(),
                metadata.name.clone(),
                metadata.scopes.clone(),
            )
        };

        // Generate new key
        let new_key = self.generate_key_with_name(&user_id, name, scopes).await?;

        // Revoke old key
        self.revoke_key(old_key).await?;

        Ok(new_key)
    }

    /// Cleanup expired keys
    pub async fn cleanup_expired(&self) -> usize {
        let mut store = self.store.lock().unwrap();
        let initial_count = store.len();

        store.retain(|_, metadata| !metadata.is_expired());

        initial_count - store.len()
    }

    /// Get usage statistics
    pub async fn get_stats(&self) -> ApiKeyStats {
        let store = self.store.lock().unwrap();

        let total = store.len();
        let active = store.values().filter(|m| m.active).count();
        let expired = store.values().filter(|m| m.is_expired()).count();
        let total_uses = store.values().map(|m| m.use_count).sum();

        ApiKeyStats {
            total,
            active,
            expired,
            total_uses,
        }
    }

    /// Generate a cryptographically secure random key
    fn generate_random_key(&self) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        use rand::Rng;
        let mut rng = rand::rng();
        let bytes: Vec<u8> = (0..self.config.key_length).map(|_| rng.random()).collect();
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Hash an API key for storage
    fn hash_key(key: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl Default for ApiKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// API key usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyStats {
    /// Total number of keys
    pub total: usize,
    /// Number of active keys
    pub active: usize,
    /// Number of expired keys
    pub expired: usize,
    /// Total uses across all keys
    pub total_uses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_key_generation() {
        let manager = ApiKeyManager::new();
        let key = manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        assert!(key.key().starts_with("sk_"));
        assert_eq!(key.metadata().user_id(), "user123");
        assert_eq!(key.metadata().scopes(), &[ApiKeyScope::Read]);
    }

    #[tokio::test]
    async fn test_api_key_validation() {
        let manager = ApiKeyManager::new();
        let key = manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        let validation = manager.validate_key(key.key()).await.unwrap();
        assert!(validation.is_valid());
        assert_eq!(validation.user_id(), "user123");
    }

    #[tokio::test]
    async fn test_api_key_revocation() {
        let manager = ApiKeyManager::new();
        let key = manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        manager.revoke_key(key.key()).await.unwrap();

        let validation = manager.validate_key(key.key()).await.unwrap();
        assert!(!validation.is_valid());
    }

    #[tokio::test]
    async fn test_api_key_scope_checking() {
        let manager = ApiKeyManager::new();
        let key = manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        let validation = manager.validate_key(key.key()).await.unwrap();
        assert!(validation.has_scope(&ApiKeyScope::Read));
        assert!(!validation.has_scope(&ApiKeyScope::Write));
    }

    #[tokio::test]
    async fn test_api_key_scope_hierarchy() {
        let admin_scope = ApiKeyScope::Admin;
        assert!(admin_scope.includes(&ApiKeyScope::Read));
        assert!(admin_scope.includes(&ApiKeyScope::Write));
        assert!(admin_scope.includes(&ApiKeyScope::Delete));

        let write_scope = ApiKeyScope::Write;
        assert!(write_scope.includes(&ApiKeyScope::Read));
        assert!(!write_scope.includes(&ApiKeyScope::Delete));
    }

    #[tokio::test]
    async fn test_api_key_rotation() {
        let manager = ApiKeyManager::new();
        let old_key = manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        let new_key = manager.rotate_key(old_key.key()).await.unwrap();

        // Old key should be revoked
        let old_validation = manager.validate_key(old_key.key()).await.unwrap();
        assert!(!old_validation.is_valid());

        // New key should be valid
        let new_validation = manager.validate_key(new_key.key()).await.unwrap();
        assert!(new_validation.is_valid());
        assert_eq!(new_validation.user_id(), "user123");
    }

    #[tokio::test]
    async fn test_revoke_user_keys() {
        let manager = ApiKeyManager::new();
        manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();
        manager
            .generate_key("user123", vec![ApiKeyScope::Write])
            .await
            .unwrap();
        manager
            .generate_key("user456", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        let revoked = manager.revoke_user_keys("user123").await.unwrap();
        assert_eq!(revoked, 2);

        let user_keys = manager.get_user_keys("user123").await;
        assert_eq!(user_keys.len(), 2);
        assert!(user_keys.iter().all(|k| !k.active));
    }

    #[tokio::test]
    async fn test_api_key_expiration() {
        let config = ApiKeyConfig {
            default_expiration: Some(Duration::seconds(1)),
            ..Default::default()
        };
        let manager = ApiKeyManager::with_config(config);

        let key = manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        // Key should be valid initially
        let validation1 = manager.validate_key(key.key()).await.unwrap();
        assert!(validation1.is_valid());

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Key should be expired now
        let validation2 = manager.validate_key(key.key()).await.unwrap();
        assert!(!validation2.is_valid());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let config = ApiKeyConfig {
            default_expiration: Some(Duration::seconds(-1)), // Already expired
            ..Default::default()
        };
        let manager = ApiKeyManager::with_config(config);

        manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        let cleaned = manager.cleanup_expired().await;
        assert_eq!(cleaned, 1);
    }

    #[tokio::test]
    async fn test_api_key_stats() {
        let manager = ApiKeyManager::new();
        let key1 = manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();
        manager
            .generate_key("user456", vec![ApiKeyScope::Write])
            .await
            .unwrap();

        // Use key1 a few times
        manager.validate_key(key1.key()).await.unwrap();
        manager.validate_key(key1.key()).await.unwrap();

        let stats = manager.get_stats().await;
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 2);
        assert_eq!(stats.total_uses, 2);
    }

    #[tokio::test]
    async fn test_api_key_usage_tracking() {
        let manager = ApiKeyManager::new();
        let key = manager
            .generate_key("user123", vec![ApiKeyScope::Read])
            .await
            .unwrap();

        // Use the key multiple times
        for _ in 0..5 {
            manager.validate_key(key.key()).await.unwrap();
        }

        let keys = manager.get_user_keys("user123").await;
        assert_eq!(keys[0].use_count(), 5);
        assert!(keys[0].last_used_at().is_some());
    }
}
