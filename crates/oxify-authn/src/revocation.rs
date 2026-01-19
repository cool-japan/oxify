//! Token revocation and blacklist management
//!
//! Provides functionality to revoke JWT tokens before their natural expiration.
//! This is essential for implementing logout, token compromise handling, and
//! security incident response.
//!
//! # Features
//! - Token revocation by token ID (jti claim)
//! - User-level token revocation (revoke all tokens for a user)
//! - Automatic cleanup of expired revocation entries
//! - Memory-efficient storage with expiration tracking
//!
//! # Example
//!
//! ```no_run
//! use oxify_authn::revocation::{RevocationManager, RevocationConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = RevocationConfig::default();
//! let manager = RevocationManager::new(config);
//!
//! // Revoke a specific token
//! manager.revoke_token("token-id-123", 3600).await?;
//!
//! // Check if token is revoked
//! let is_revoked = manager.is_revoked("token-id-123").await?;
//! assert!(is_revoked);
//!
//! // Revoke all tokens for a user
//! manager.revoke_user_tokens("user-123").await?;
//! # Ok(())
//! # }
//! ```

use crate::types::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Revocation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationConfig {
    /// Maximum entries to keep (0 = unlimited)
    pub max_entries: usize,
    /// Default revocation duration in seconds
    pub default_ttl_secs: u64,
    /// Cleanup interval in seconds
    pub cleanup_interval_secs: u64,
}

impl Default for RevocationConfig {
    fn default() -> Self {
        Self {
            max_entries: 100_000,       // 100k entries
            default_ttl_secs: 86400,    // 24 hours
            cleanup_interval_secs: 300, // 5 minutes
        }
    }
}

impl RevocationConfig {
    /// Create a new builder for `RevocationConfig`
    #[must_use]
    pub fn builder() -> RevocationConfigBuilder {
        RevocationConfigBuilder::new()
    }
}

/// Builder for `RevocationConfig`
#[derive(Debug, Clone)]
pub struct RevocationConfigBuilder {
    max_entries: usize,
    default_ttl_secs: u64,
    cleanup_interval_secs: u64,
}

impl RevocationConfigBuilder {
    /// Create a new builder with default values
    #[must_use]
    pub fn new() -> Self {
        let default = RevocationConfig::default();
        Self {
            max_entries: default.max_entries,
            default_ttl_secs: default.default_ttl_secs,
            cleanup_interval_secs: default.cleanup_interval_secs,
        }
    }

    /// Set maximum entries to keep (0 = unlimited)
    #[must_use]
    pub fn max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Set default revocation duration in seconds
    #[must_use]
    pub fn default_ttl_secs(mut self, secs: u64) -> Self {
        self.default_ttl_secs = secs;
        self
    }

    /// Set cleanup interval in seconds
    #[must_use]
    pub fn cleanup_interval_secs(mut self, secs: u64) -> Self {
        self.cleanup_interval_secs = secs;
        self
    }

    /// Build the `RevocationConfig`
    #[must_use]
    pub fn build(self) -> RevocationConfig {
        RevocationConfig {
            max_entries: self.max_entries,
            default_ttl_secs: self.default_ttl_secs,
            cleanup_interval_secs: self.cleanup_interval_secs,
        }
    }
}

impl Default for RevocationConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A revoked token entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationEntry {
    /// Token ID (jti claim)
    pub token_id: String,
    /// User ID associated with the token
    pub user_id: Option<String>,
    /// When the token was revoked
    pub revoked_at: DateTime<Utc>,
    /// When the revocation entry expires
    pub expires_at: DateTime<Utc>,
    /// Reason for revocation
    pub reason: RevocationReason,
}

/// Reason for token revocation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RevocationReason {
    /// User logged out
    #[default]
    Logout,
    /// User changed password
    PasswordChange,
    /// Token compromised
    Compromised,
    /// User disabled/deleted
    UserDisabled,
    /// Session expired or invalid
    SessionInvalid,
    /// Administrative action
    AdminAction,
    /// Other reason
    Other(String),
}

/// Token revocation manager
///
/// Note: This is an in-memory implementation suitable for single-instance deployments.
/// For distributed systems, implement the `RevocationStore` trait with Redis or database backing.
pub struct RevocationManager {
    config: RevocationConfig,
    /// Map of `token_id` -> `RevocationEntry`
    revoked_tokens: Arc<RwLock<HashMap<String, RevocationEntry>>>,
    /// Map of `user_id` -> Set of `token_ids`
    user_tokens: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Set of users with all tokens revoked (for "revoke all" optimization)
    revoked_users: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl RevocationManager {
    /// Create a new revocation manager
    #[must_use]
    pub fn new(config: RevocationConfig) -> Self {
        Self {
            config,
            revoked_tokens: Arc::new(RwLock::new(HashMap::new())),
            user_tokens: Arc::new(RwLock::new(HashMap::new())),
            revoked_users: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Revoke a specific token
    pub async fn revoke_token(&self, token_id: &str, ttl_secs: u64) -> Result<()> {
        self.revoke_token_with_reason(token_id, None, ttl_secs, RevocationReason::Logout)
            .await
    }

    /// Revoke a token with user association and reason
    pub async fn revoke_token_with_reason(
        &self,
        token_id: &str,
        user_id: Option<&str>,
        ttl_secs: u64,
        reason: RevocationReason,
    ) -> Result<()> {
        let now = Utc::now();
        let entry = RevocationEntry {
            token_id: token_id.to_string(),
            user_id: user_id.map(std::string::ToString::to_string),
            revoked_at: now,
            expires_at: now + Duration::seconds(ttl_secs as i64),
            reason,
        };

        // Store the entry
        {
            let mut tokens = self.revoked_tokens.write().await;

            // Check max entries
            if self.config.max_entries > 0 && tokens.len() >= self.config.max_entries {
                // Remove oldest expired entry
                Self::cleanup_expired_internal(&mut tokens);
            }

            tokens.insert(token_id.to_string(), entry);
        }

        // Track user -> token mapping
        if let Some(uid) = user_id {
            let mut user_tokens = self.user_tokens.write().await;
            user_tokens
                .entry(uid.to_string())
                .or_default()
                .insert(token_id.to_string());
        }

        Ok(())
    }

    /// Check if a token is revoked
    pub async fn is_revoked(&self, token_id: &str) -> Result<bool> {
        let tokens = self.revoked_tokens.read().await;

        if let Some(entry) = tokens.get(token_id) {
            // Check if entry is still valid (not expired)
            if Utc::now() <= entry.expires_at {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if a token is revoked (considering user-level revocation)
    pub async fn is_revoked_for_user(
        &self,
        token_id: &str,
        user_id: &str,
        token_issued_at: DateTime<Utc>,
    ) -> Result<bool> {
        // Check token-specific revocation
        if self.is_revoked(token_id).await? {
            return Ok(true);
        }

        // Check user-level revocation
        let revoked_users = self.revoked_users.read().await;
        if let Some(revoked_at) = revoked_users.get(user_id) {
            // Token is revoked if it was issued before the user revocation
            if token_issued_at < *revoked_at {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Revoke all tokens for a user
    pub async fn revoke_user_tokens(&self, user_id: &str) -> Result<usize> {
        self.revoke_user_tokens_with_reason(user_id, RevocationReason::Logout)
            .await
    }

    /// Revoke all tokens for a user with a reason
    pub async fn revoke_user_tokens_with_reason(
        &self,
        user_id: &str,
        reason: RevocationReason,
    ) -> Result<usize> {
        let now = Utc::now();

        // Mark user as revoked (all tokens issued before now are invalid)
        {
            let mut revoked_users = self.revoked_users.write().await;
            revoked_users.insert(user_id.to_string(), now);
        }

        // Also revoke specific tokens we know about
        let token_ids = {
            let user_tokens = self.user_tokens.read().await;
            user_tokens.get(user_id).cloned().unwrap_or_default()
        };

        let count = token_ids.len();

        for token_id in token_ids {
            self.revoke_token_with_reason(
                &token_id,
                Some(user_id),
                self.config.default_ttl_secs,
                reason.clone(),
            )
            .await?;
        }

        Ok(count)
    }

    /// Get revocation info for a token
    pub async fn get_revocation_info(&self, token_id: &str) -> Result<Option<RevocationEntry>> {
        let tokens = self.revoked_tokens.read().await;
        Ok(tokens.get(token_id).cloned())
    }

    /// Get all revoked tokens for a user
    pub async fn get_user_revocations(&self, user_id: &str) -> Result<Vec<RevocationEntry>> {
        let tokens = self.revoked_tokens.read().await;

        Ok(tokens
            .values()
            .filter(|e| e.user_id.as_deref() == Some(user_id))
            .cloned()
            .collect())
    }

    /// Cleanup expired entries
    pub async fn cleanup_expired(&self) -> usize {
        let mut tokens = self.revoked_tokens.write().await;
        Self::cleanup_expired_internal(&mut tokens)
    }

    /// Internal cleanup helper
    fn cleanup_expired_internal(tokens: &mut HashMap<String, RevocationEntry>) -> usize {
        let now = Utc::now();
        let expired: Vec<String> = tokens
            .iter()
            .filter(|(_, e)| now > e.expires_at)
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired.len();
        for id in expired {
            tokens.remove(&id);
        }
        count
    }

    /// Get statistics
    pub async fn get_stats(&self) -> RevocationStats {
        let tokens = self.revoked_tokens.read().await;
        let users = self.revoked_users.read().await;

        let now = Utc::now();
        let active = tokens.values().filter(|e| now <= e.expires_at).count();

        RevocationStats {
            total_entries: tokens.len(),
            active_entries: active,
            expired_entries: tokens.len() - active,
            revoked_users: users.len(),
        }
    }
}

impl Default for RevocationManager {
    fn default() -> Self {
        Self::new(RevocationConfig::default())
    }
}

/// Revocation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationStats {
    /// Total entries in the store
    pub total_entries: usize,
    /// Active (non-expired) entries
    pub active_entries: usize,
    /// Expired entries pending cleanup
    pub expired_entries: usize,
    /// Number of users with all tokens revoked
    pub revoked_users: usize,
}

/// Trait for custom revocation storage implementations
#[async_trait::async_trait]
pub trait RevocationStore: Send + Sync {
    /// Add a revocation entry
    async fn add(&self, entry: &RevocationEntry) -> Result<()>;

    /// Check if a token is revoked
    async fn is_revoked(&self, token_id: &str) -> Result<bool>;

    /// Remove a revocation entry
    async fn remove(&self, token_id: &str) -> Result<()>;

    /// Get revocation entry
    async fn get(&self, token_id: &str) -> Result<Option<RevocationEntry>>;

    /// Cleanup expired entries
    async fn cleanup_expired(&self) -> Result<usize>;

    /// Mark all tokens for a user as revoked before a timestamp
    async fn revoke_user(&self, user_id: &str, before: DateTime<Utc>) -> Result<()>;

    /// Check if user tokens are revoked before a timestamp
    async fn is_user_revoked(&self, user_id: &str, token_issued_at: DateTime<Utc>) -> Result<bool>;
}

/// In-memory revocation store
pub struct InMemoryRevocationStore {
    entries: Arc<RwLock<HashMap<String, RevocationEntry>>>,
    user_revocations: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl InMemoryRevocationStore {
    /// Create a new in-memory revocation store
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            user_revocations: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryRevocationStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RevocationStore for InMemoryRevocationStore {
    async fn add(&self, entry: &RevocationEntry) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.insert(entry.token_id.clone(), entry.clone());
        Ok(())
    }

    async fn is_revoked(&self, token_id: &str) -> Result<bool> {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(token_id) {
            if Utc::now() <= entry.expires_at {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn remove(&self, token_id: &str) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.remove(token_id);
        Ok(())
    }

    async fn get(&self, token_id: &str) -> Result<Option<RevocationEntry>> {
        let entries = self.entries.read().await;
        Ok(entries.get(token_id).cloned())
    }

    async fn cleanup_expired(&self) -> Result<usize> {
        let mut entries = self.entries.write().await;
        let now = Utc::now();
        let expired: Vec<String> = entries
            .iter()
            .filter(|(_, e)| now > e.expires_at)
            .map(|(id, _)| id.clone())
            .collect();
        let count = expired.len();
        for id in expired {
            entries.remove(&id);
        }
        Ok(count)
    }

    async fn revoke_user(&self, user_id: &str, before: DateTime<Utc>) -> Result<()> {
        let mut revocations = self.user_revocations.write().await;
        revocations.insert(user_id.to_string(), before);
        Ok(())
    }

    async fn is_user_revoked(&self, user_id: &str, token_issued_at: DateTime<Utc>) -> Result<bool> {
        let revocations = self.user_revocations.read().await;
        if let Some(revoked_at) = revocations.get(user_id) {
            if token_issued_at < *revoked_at {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_revocation() {
        let manager = RevocationManager::default();

        // Token should not be revoked initially
        let is_revoked = manager.is_revoked("token-1").await.unwrap();
        assert!(!is_revoked);

        // Revoke the token
        manager.revoke_token("token-1", 3600).await.unwrap();

        // Token should now be revoked
        let is_revoked = manager.is_revoked("token-1").await.unwrap();
        assert!(is_revoked);
    }

    #[tokio::test]
    async fn test_token_revocation_with_reason() {
        let manager = RevocationManager::default();

        manager
            .revoke_token_with_reason(
                "token-1",
                Some("user-1"),
                3600,
                RevocationReason::PasswordChange,
            )
            .await
            .unwrap();

        let info = manager.get_revocation_info("token-1").await.unwrap();
        assert!(info.is_some());

        let info = info.unwrap();
        assert_eq!(info.reason, RevocationReason::PasswordChange);
        assert_eq!(info.user_id, Some("user-1".to_string()));
    }

    #[tokio::test]
    async fn test_user_token_revocation() {
        let manager = RevocationManager::default();

        // Revoke specific tokens for user
        manager
            .revoke_token_with_reason("token-1", Some("user-1"), 3600, RevocationReason::Logout)
            .await
            .unwrap();
        manager
            .revoke_token_with_reason("token-2", Some("user-1"), 3600, RevocationReason::Logout)
            .await
            .unwrap();

        // Revoke all user tokens
        let count = manager.revoke_user_tokens("user-1").await.unwrap();
        assert_eq!(count, 2);

        // All tokens should be revoked
        assert!(manager.is_revoked("token-1").await.unwrap());
        assert!(manager.is_revoked("token-2").await.unwrap());
    }

    #[tokio::test]
    async fn test_user_level_revocation() {
        let manager = RevocationManager::default();

        // Revoke all tokens for user
        manager.revoke_user_tokens("user-1").await.unwrap();

        // Token issued before revocation should be revoked
        let old_time = Utc::now() - Duration::hours(1);
        let is_revoked = manager
            .is_revoked_for_user("new-token", "user-1", old_time)
            .await
            .unwrap();
        assert!(is_revoked);

        // Token issued after revocation should be valid
        let new_time = Utc::now() + Duration::seconds(1);
        let is_revoked = manager
            .is_revoked_for_user("new-token", "user-1", new_time)
            .await
            .unwrap();
        assert!(!is_revoked);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let manager = RevocationManager::default();

        // Add an entry that's already expired
        {
            let mut tokens = manager.revoked_tokens.write().await;
            tokens.insert(
                "expired-token".to_string(),
                RevocationEntry {
                    token_id: "expired-token".to_string(),
                    user_id: None,
                    revoked_at: Utc::now() - Duration::hours(2),
                    expires_at: Utc::now() - Duration::hours(1),
                    reason: RevocationReason::Logout,
                },
            );
        }

        // Add a valid entry
        manager.revoke_token("valid-token", 3600).await.unwrap();

        // Cleanup
        let count = manager.cleanup_expired().await;
        assert_eq!(count, 1);

        // Expired should be gone
        let info = manager.get_revocation_info("expired-token").await.unwrap();
        assert!(info.is_none());

        // Valid should remain
        assert!(manager.is_revoked("valid-token").await.unwrap());
    }

    #[tokio::test]
    async fn test_revocation_stats() {
        let manager = RevocationManager::default();

        manager.revoke_token("token-1", 3600).await.unwrap();
        manager.revoke_token("token-2", 3600).await.unwrap();
        manager.revoke_user_tokens("user-1").await.unwrap();

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.active_entries, 2);
        assert_eq!(stats.revoked_users, 1);
    }

    #[tokio::test]
    async fn test_revocation_store_trait() {
        let store = InMemoryRevocationStore::new();

        let entry = RevocationEntry {
            token_id: "token-1".to_string(),
            user_id: Some("user-1".to_string()),
            revoked_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
            reason: RevocationReason::Logout,
        };

        // Add
        store.add(&entry).await.unwrap();

        // Check
        assert!(store.is_revoked("token-1").await.unwrap());

        // Get
        let retrieved = store.get("token-1").await.unwrap();
        assert!(retrieved.is_some());

        // Remove
        store.remove("token-1").await.unwrap();
        assert!(!store.is_revoked("token-1").await.unwrap());
    }

    #[tokio::test]
    async fn test_store_user_revocation() {
        let store = InMemoryRevocationStore::new();

        let revocation_time = Utc::now();
        store.revoke_user("user-1", revocation_time).await.unwrap();

        // Token issued before revocation
        let old_time = revocation_time - Duration::hours(1);
        assert!(store.is_user_revoked("user-1", old_time).await.unwrap());

        // Token issued after revocation
        let new_time = revocation_time + Duration::seconds(1);
        assert!(!store.is_user_revoked("user-1", new_time).await.unwrap());
    }

    #[test]
    fn test_revocation_config_builder() {
        let config = RevocationConfig::builder()
            .max_entries(50000)
            .default_ttl_secs(43200)
            .cleanup_interval_secs(600)
            .build();

        assert_eq!(config.max_entries, 50000);
        assert_eq!(config.default_ttl_secs, 43200);
        assert_eq!(config.cleanup_interval_secs, 600);
    }
}
