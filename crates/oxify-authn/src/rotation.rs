//! Token Rotation
//!
//! Provides refresh token rotation, one-time use tokens, and sliding window expiration
//! to enhance security by limiting token reuse and preventing token theft.
//!
//! # Features
//!
//! - **Refresh Token Rotation**: Automatically rotate refresh tokens on each use
//! - **One-Time Use Tokens**: Ensure refresh tokens can only be used once
//! - **Sliding Window Expiration**: Extend token lifetime on activity
//! - **Token Family Tracking**: Detect token reuse attacks
//!
//! # Example
//!
//! ```
//! use oxify_authn::rotation::{RotationManager, RefreshTokenMetadata};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = RotationManager::new();
//!
//! // Issue a refresh token
//! let token_id = "token123";
//! let metadata = RefreshTokenMetadata::new("user123", token_id);
//! manager.issue_token(metadata).await?;
//!
//! // Use the token (will be rotated)
//! match manager.use_token(token_id).await? {
//!     Some(new_token_id) => println!("Token rotated: {}", new_token_id),
//!     None => println!("Token invalid or already used"),
//! }
//! # Ok(())
//! # }
//! ```

use crate::types::AuthError;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Refresh token metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshTokenMetadata {
    /// Token ID
    token_id: String,
    /// User ID associated with this token
    user_id: String,
    /// Token family ID (for tracking rotation chains)
    family_id: String,
    /// Parent token ID (for rotation tracking)
    parent_token_id: Option<String>,
    /// Issue timestamp
    issued_at: DateTime<Utc>,
    /// Expiration timestamp
    expires_at: DateTime<Utc>,
    /// Last used timestamp (for sliding window)
    last_used_at: Option<DateTime<Utc>>,
    /// Number of times this token has been used
    use_count: usize,
    /// Maximum allowed uses (1 for one-time use)
    max_uses: usize,
    /// Whether this token has been revoked
    revoked: bool,
    /// Reason for revocation (if any)
    revoke_reason: Option<String>,
}

impl RefreshTokenMetadata {
    /// Create a new refresh token metadata
    pub fn new(user_id: impl Into<String>, token_id: impl Into<String>) -> Self {
        let family_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        Self {
            token_id: token_id.into(),
            user_id: user_id.into(),
            family_id,
            parent_token_id: None,
            issued_at: now,
            expires_at: now + Duration::days(30), // Default 30 days
            last_used_at: None,
            use_count: 0,
            max_uses: 1, // One-time use by default
            revoked: false,
            revoke_reason: None,
        }
    }

    /// Create a rotated token from this token
    #[must_use]
    pub fn rotate(&self, new_token_id: impl Into<String>) -> Self {
        let now = Utc::now();

        Self {
            token_id: new_token_id.into(),
            user_id: self.user_id.clone(),
            family_id: self.family_id.clone(),
            parent_token_id: Some(self.token_id.clone()),
            issued_at: now,
            expires_at: now + Duration::days(30),
            last_used_at: None,
            use_count: 0,
            max_uses: self.max_uses,
            revoked: false,
            revoke_reason: None,
        }
    }

    /// Get the token ID
    #[must_use]
    pub fn token_id(&self) -> &str {
        &self.token_id
    }

    /// Get the user ID
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get the family ID
    #[must_use]
    pub fn family_id(&self) -> &str {
        &self.family_id
    }

    /// Get the parent token ID
    #[must_use]
    pub fn parent_token_id(&self) -> Option<&str> {
        self.parent_token_id.as_deref()
    }

    /// Get the issued timestamp
    #[must_use]
    pub fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }

    /// Get the expiration timestamp
    #[must_use]
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Get the last used timestamp
    #[must_use]
    pub fn last_used_at(&self) -> Option<DateTime<Utc>> {
        self.last_used_at
    }

    /// Get the use count
    #[must_use]
    pub fn use_count(&self) -> usize {
        self.use_count
    }

    /// Get the maximum allowed uses
    #[must_use]
    pub fn max_uses(&self) -> usize {
        self.max_uses
    }

    /// Check if the token is revoked
    #[must_use]
    pub fn is_revoked(&self) -> bool {
        self.revoked
    }

    /// Check if the token is expired
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if the token can be used
    #[must_use]
    pub fn can_use(&self) -> bool {
        !self.revoked && !self.is_expired() && self.use_count < self.max_uses
    }

    /// Mark the token as used
    pub fn mark_used(&mut self) {
        self.use_count += 1;
        self.last_used_at = Some(Utc::now());
    }

    /// Revoke the token
    pub fn revoke(&mut self, reason: impl Into<String>) {
        self.revoked = true;
        self.revoke_reason = Some(reason.into());
    }

    /// Set the expiration time
    pub fn set_expires_at(&mut self, expires_at: DateTime<Utc>) {
        self.expires_at = expires_at;
    }

    /// Set the maximum uses
    pub fn set_max_uses(&mut self, max_uses: usize) {
        self.max_uses = max_uses;
    }

    /// Set the family ID (for linking to existing token families)
    pub fn set_family_id(&mut self, family_id: impl Into<String>) {
        self.family_id = family_id.into();
    }

    /// Extend expiration with sliding window
    pub fn extend_expiration(&mut self, duration: Duration) {
        let now = Utc::now();
        if self.last_used_at.is_some() {
            // Sliding window: extend from last use
            self.expires_at = now + duration;
        }
    }
}

/// Token rotation configuration
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// Enable automatic token rotation
    pub auto_rotate: bool,
    /// Enable one-time use tokens
    pub one_time_use: bool,
    /// Enable sliding window expiration
    pub sliding_window: bool,
    /// Sliding window duration (if enabled)
    pub sliding_window_duration: Duration,
    /// Default token lifetime
    pub token_lifetime: Duration,
    /// Grace period for token rotation (to handle race conditions)
    pub rotation_grace_period: Duration,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            auto_rotate: true,
            one_time_use: true,
            sliding_window: false,
            sliding_window_duration: Duration::hours(24),
            token_lifetime: Duration::days(30),
            rotation_grace_period: Duration::minutes(5),
        }
    }
}

impl RotationConfig {
    /// Create a new builder for `RotationConfig`
    #[must_use]
    pub fn builder() -> RotationConfigBuilder {
        RotationConfigBuilder::new()
    }

    /// Create a strict configuration (one-time use, auto-rotate)
    #[must_use]
    pub fn strict() -> Self {
        Self {
            auto_rotate: true,
            one_time_use: true,
            sliding_window: false,
            token_lifetime: Duration::days(7),
            rotation_grace_period: Duration::minutes(1),
            ..Default::default()
        }
    }

    /// Create a relaxed configuration (reusable tokens, no auto-rotation)
    #[must_use]
    pub fn relaxed() -> Self {
        Self {
            auto_rotate: false,
            one_time_use: false,
            sliding_window: true,
            sliding_window_duration: Duration::days(7),
            token_lifetime: Duration::days(90),
            rotation_grace_period: Duration::minutes(10),
        }
    }
}

/// Builder for `RotationConfig`
#[derive(Debug, Clone)]
pub struct RotationConfigBuilder {
    auto_rotate: bool,
    one_time_use: bool,
    sliding_window: bool,
    sliding_window_duration: Duration,
    token_lifetime: Duration,
    rotation_grace_period: Duration,
}

impl RotationConfigBuilder {
    /// Create a new builder with default values
    #[must_use]
    pub fn new() -> Self {
        let default = RotationConfig::default();
        Self {
            auto_rotate: default.auto_rotate,
            one_time_use: default.one_time_use,
            sliding_window: default.sliding_window,
            sliding_window_duration: default.sliding_window_duration,
            token_lifetime: default.token_lifetime,
            rotation_grace_period: default.rotation_grace_period,
        }
    }

    /// Enable or disable automatic token rotation
    #[must_use]
    pub fn auto_rotate(mut self, enable: bool) -> Self {
        self.auto_rotate = enable;
        self
    }

    /// Enable or disable one-time use tokens
    #[must_use]
    pub fn one_time_use(mut self, enable: bool) -> Self {
        self.one_time_use = enable;
        self
    }

    /// Enable or disable sliding window expiration
    #[must_use]
    pub fn sliding_window(mut self, enable: bool) -> Self {
        self.sliding_window = enable;
        self
    }

    /// Set sliding window duration
    #[must_use]
    pub fn sliding_window_duration(mut self, duration: Duration) -> Self {
        self.sliding_window_duration = duration;
        self
    }

    /// Set default token lifetime
    #[must_use]
    pub fn token_lifetime(mut self, duration: Duration) -> Self {
        self.token_lifetime = duration;
        self
    }

    /// Set rotation grace period
    #[must_use]
    pub fn rotation_grace_period(mut self, duration: Duration) -> Self {
        self.rotation_grace_period = duration;
        self
    }

    /// Build the `RotationConfig`
    #[must_use]
    pub fn build(self) -> RotationConfig {
        RotationConfig {
            auto_rotate: self.auto_rotate,
            one_time_use: self.one_time_use,
            sliding_window: self.sliding_window,
            sliding_window_duration: self.sliding_window_duration,
            token_lifetime: self.token_lifetime,
            rotation_grace_period: self.rotation_grace_period,
        }
    }
}

impl Default for RotationConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Token rotation manager
pub struct RotationManager {
    /// Token store (in-memory for now, trait-based for extensibility)
    store: Arc<Mutex<HashMap<String, RefreshTokenMetadata>>>,
    /// Configuration
    config: RotationConfig,
}

impl RotationManager {
    /// Create a new rotation manager with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(RotationConfig::default())
    }

    /// Create a new rotation manager with custom configuration
    #[must_use]
    pub fn with_config(config: RotationConfig) -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Issue a new refresh token
    pub async fn issue_token(
        &self,
        mut metadata: RefreshTokenMetadata,
    ) -> Result<String, AuthError> {
        // Apply configuration
        if self.config.one_time_use {
            metadata.set_max_uses(1);
        }

        let now = Utc::now();
        metadata.set_expires_at(now + self.config.token_lifetime);

        let token_id = metadata.token_id().to_string();

        let mut store = self.store.lock().unwrap();
        store.insert(token_id.clone(), metadata);

        Ok(token_id)
    }

    /// Use a refresh token (may rotate it)
    pub async fn use_token(&self, token_id: &str) -> Result<Option<String>, AuthError> {
        let mut store = self.store.lock().unwrap();

        // First, check if token exists and get necessary info
        let (should_revoke_family, family_id) = {
            let metadata = store
                .get(token_id)
                .ok_or_else(|| AuthError::InvalidToken("Token not found".into()))?;

            // Check if token can be used
            if metadata.can_use() {
                (false, String::new())
            } else if metadata.is_revoked() {
                return Err(AuthError::TokenRevoked);
            } else if metadata.is_expired() {
                return Err(AuthError::TokenExpired);
            } else if metadata.use_count >= metadata.max_uses {
                // Token reuse detected - need to revoke entire family
                (true, metadata.family_id.clone())
            } else {
                return Err(AuthError::InvalidToken("Token cannot be used".into()));
            }
        };

        // Revoke family if needed
        if should_revoke_family {
            Self::revoke_family_internal(&mut store, &family_id, "Token reuse detected");
            return Err(AuthError::InvalidToken("Token already used".into()));
        }

        // Now get mutable reference and update
        let metadata = store
            .get_mut(token_id)
            .ok_or_else(|| AuthError::InvalidToken("Token not found".into()))?;

        // Mark as used
        metadata.mark_used();

        // Apply sliding window if enabled
        if self.config.sliding_window {
            metadata.extend_expiration(self.config.sliding_window_duration);
        }

        // Rotate token if enabled
        if self.config.auto_rotate && metadata.use_count >= metadata.max_uses {
            let new_token_id = Uuid::new_v4().to_string();
            let new_metadata = metadata.rotate(&new_token_id);
            let grace_expires = Utc::now() + self.config.rotation_grace_period;

            // Set expiration before losing the borrow
            metadata.set_expires_at(grace_expires);

            // Insert new token
            store.insert(new_token_id.clone(), new_metadata);

            Ok(Some(new_token_id))
        } else {
            Ok(None)
        }
    }

    /// Validate a token without using it
    pub async fn validate_token(&self, token_id: &str) -> Result<RefreshTokenMetadata, AuthError> {
        let store = self.store.lock().unwrap();

        let metadata = store
            .get(token_id)
            .ok_or_else(|| AuthError::InvalidToken("Token not found".into()))?;

        if !metadata.can_use() {
            if metadata.is_revoked() {
                return Err(AuthError::TokenRevoked);
            } else if metadata.is_expired() {
                return Err(AuthError::TokenExpired);
            }
            return Err(AuthError::InvalidToken("Token cannot be used".into()));
        }

        Ok(metadata.clone())
    }

    /// Revoke a specific token
    pub async fn revoke_token(&self, token_id: &str, reason: &str) -> Result<(), AuthError> {
        let mut store = self.store.lock().unwrap();

        let metadata = store
            .get_mut(token_id)
            .ok_or_else(|| AuthError::InvalidToken("Token not found".into()))?;

        metadata.revoke(reason);
        Ok(())
    }

    /// Revoke an entire token family (e.g., after detecting reuse)
    pub async fn revoke_family(&self, family_id: &str, reason: &str) -> Result<usize, AuthError> {
        let mut store = self.store.lock().unwrap();
        Ok(Self::revoke_family_internal(&mut store, family_id, reason))
    }

    /// Internal implementation of family revocation
    fn revoke_family_internal(
        store: &mut HashMap<String, RefreshTokenMetadata>,
        family_id: &str,
        reason: &str,
    ) -> usize {
        let mut revoked_count = 0;

        for metadata in store.values_mut() {
            if metadata.family_id == family_id && !metadata.is_revoked() {
                metadata.revoke(reason);
                revoked_count += 1;
            }
        }

        revoked_count
    }

    /// Revoke all tokens for a user
    pub async fn revoke_user_tokens(
        &self,
        user_id: &str,
        reason: &str,
    ) -> Result<usize, AuthError> {
        let mut store = self.store.lock().unwrap();
        let mut revoked_count = 0;

        for metadata in store.values_mut() {
            if metadata.user_id == user_id && !metadata.is_revoked() {
                metadata.revoke(reason);
                revoked_count += 1;
            }
        }

        Ok(revoked_count)
    }

    /// Get all tokens for a user (for debugging/admin purposes)
    pub async fn get_user_tokens(&self, user_id: &str) -> Vec<RefreshTokenMetadata> {
        let store = self.store.lock().unwrap();

        store
            .values()
            .filter(|m| m.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Cleanup expired tokens
    pub async fn cleanup_expired(&self) -> usize {
        let mut store = self.store.lock().unwrap();
        let initial_count = store.len();

        store.retain(|_, metadata| !metadata.is_expired());

        initial_count - store.len()
    }

    /// Get token statistics
    pub async fn get_stats(&self) -> RotationStats {
        let store = self.store.lock().unwrap();

        let total = store.len();
        let active = store.values().filter(|m| m.can_use()).count();
        let revoked = store.values().filter(|m| m.is_revoked()).count();
        let expired = store.values().filter(|m| m.is_expired()).count();

        RotationStats {
            total,
            active,
            revoked,
            expired,
        }
    }
}

impl Default for RotationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Token rotation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationStats {
    /// Total number of tokens
    pub total: usize,
    /// Number of active (usable) tokens
    pub active: usize,
    /// Number of revoked tokens
    pub revoked: usize,
    /// Number of expired tokens
    pub expired: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_metadata_creation() {
        let metadata = RefreshTokenMetadata::new("user123", "token123");
        assert_eq!(metadata.user_id(), "user123");
        assert_eq!(metadata.token_id(), "token123");
        assert_eq!(metadata.use_count(), 0);
        assert_eq!(metadata.max_uses(), 1);
        assert!(!metadata.is_revoked());
        assert!(!metadata.is_expired());
        assert!(metadata.can_use());
    }

    #[tokio::test]
    async fn test_token_rotation() {
        let metadata = RefreshTokenMetadata::new("user123", "token1");
        let family_id = metadata.family_id().to_string();

        let rotated = metadata.rotate("token2");
        assert_eq!(rotated.token_id(), "token2");
        assert_eq!(rotated.family_id(), family_id);
        assert_eq!(rotated.parent_token_id(), Some("token1"));
    }

    #[tokio::test]
    async fn test_token_expiration() {
        let mut metadata = RefreshTokenMetadata::new("user123", "token123");
        let past = Utc::now() - Duration::days(1);
        metadata.set_expires_at(past);

        assert!(metadata.is_expired());
        assert!(!metadata.can_use());
    }

    #[tokio::test]
    async fn test_token_revocation() {
        let mut metadata = RefreshTokenMetadata::new("user123", "token123");
        metadata.revoke("User requested");

        assert!(metadata.is_revoked());
        assert!(!metadata.can_use());
    }

    #[tokio::test]
    async fn test_token_use() {
        let mut metadata = RefreshTokenMetadata::new("user123", "token123");
        assert_eq!(metadata.use_count(), 0);

        metadata.mark_used();
        assert_eq!(metadata.use_count(), 1);
        assert!(metadata.last_used_at().is_some());
    }

    #[tokio::test]
    async fn test_rotation_manager_issue() {
        let manager = RotationManager::new();
        let metadata = RefreshTokenMetadata::new("user123", "token123");

        let token_id = manager.issue_token(metadata).await.unwrap();
        assert_eq!(token_id, "token123");
    }

    #[tokio::test]
    async fn test_rotation_manager_use_one_time() {
        let manager = RotationManager::new();
        let metadata = RefreshTokenMetadata::new("user123", "token123");

        manager.issue_token(metadata).await.unwrap();

        // First use - should rotate
        let result = manager.use_token("token123").await.unwrap();
        assert!(result.is_some());
        let _new_token_id = result.unwrap();

        // Second use of old token - should fail (after grace period)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // The old token should still be usable during grace period
        // But after grace period expires, it should be invalid
    }

    #[tokio::test]
    async fn test_rotation_manager_validate() {
        let manager = RotationManager::new();
        let metadata = RefreshTokenMetadata::new("user123", "token123");

        manager.issue_token(metadata).await.unwrap();

        // Validate without using
        let validated = manager.validate_token("token123").await.unwrap();
        assert_eq!(validated.use_count(), 0);

        // Token should still be usable
        assert!(manager.validate_token("token123").await.is_ok());
    }

    #[tokio::test]
    async fn test_rotation_manager_revoke() {
        let manager = RotationManager::new();
        let metadata = RefreshTokenMetadata::new("user123", "token123");

        manager.issue_token(metadata).await.unwrap();

        // Revoke token
        manager.revoke_token("token123", "Testing").await.unwrap();

        // Should fail to use
        assert!(manager.use_token("token123").await.is_err());
    }

    #[tokio::test]
    async fn test_rotation_manager_revoke_family() {
        let manager = RotationManager::new();
        let metadata = RefreshTokenMetadata::new("user123", "token1");
        let family_id = metadata.family_id().to_string();

        manager.issue_token(metadata.clone()).await.unwrap();

        // Create a rotated token in the same family
        let mut rotated = metadata.rotate("token2");
        rotated.set_family_id(family_id.clone());
        manager.issue_token(rotated).await.unwrap();

        // Revoke the family
        let revoked_count = manager
            .revoke_family(&family_id, "Family compromised")
            .await
            .unwrap();
        assert_eq!(revoked_count, 2);

        // Both tokens should be revoked
        assert!(manager.use_token("token1").await.is_err());
        assert!(manager.use_token("token2").await.is_err());
    }

    #[tokio::test]
    async fn test_rotation_manager_revoke_user() {
        let manager = RotationManager::new();

        let metadata1 = RefreshTokenMetadata::new("user123", "token1");
        let metadata2 = RefreshTokenMetadata::new("user123", "token2");

        manager.issue_token(metadata1).await.unwrap();
        manager.issue_token(metadata2).await.unwrap();

        // Revoke all tokens for user
        let revoked_count = manager
            .revoke_user_tokens("user123", "Password changed")
            .await
            .unwrap();
        assert_eq!(revoked_count, 2);

        // Both tokens should be revoked
        assert!(manager.use_token("token1").await.is_err());
        assert!(manager.use_token("token2").await.is_err());
    }

    #[tokio::test]
    async fn test_rotation_manager_cleanup() {
        let manager = RotationManager::new();
        let mut metadata = RefreshTokenMetadata::new("user123", "token123");

        // Set expiration in the past
        metadata.set_expires_at(Utc::now() - Duration::days(1));

        // Manually insert expired token (issue_token would reset expiration)
        {
            let mut store = manager.store.lock().unwrap();
            store.insert("token123".to_string(), metadata);
        }

        // Cleanup
        let cleaned = manager.cleanup_expired().await;
        assert_eq!(cleaned, 1);

        // Token should be gone
        assert!(manager.validate_token("token123").await.is_err());
    }

    #[tokio::test]
    async fn test_rotation_manager_stats() {
        let manager = RotationManager::new();

        let metadata1 = RefreshTokenMetadata::new("user123", "token1");
        manager.issue_token(metadata1).await.unwrap();

        let mut metadata2 = RefreshTokenMetadata::new("user123", "token2");
        metadata2.set_expires_at(Utc::now() - Duration::days(1));

        // Manually insert expired token (issue_token would reset expiration)
        {
            let mut store = manager.store.lock().unwrap();
            store.insert("token2".to_string(), metadata2);
        }

        manager.revoke_token("token1", "Testing").await.unwrap();

        let stats = manager.get_stats().await;
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 0);
        assert_eq!(stats.revoked, 1);
        assert_eq!(stats.expired, 1);
    }

    #[tokio::test]
    async fn test_rotation_config_presets() {
        let strict = RotationConfig::strict();
        assert!(strict.auto_rotate);
        assert!(strict.one_time_use);
        assert!(!strict.sliding_window);

        let relaxed = RotationConfig::relaxed();
        assert!(!relaxed.auto_rotate);
        assert!(!relaxed.one_time_use);
        assert!(relaxed.sliding_window);
    }

    #[tokio::test]
    async fn test_sliding_window_expiration() {
        let config = RotationConfig {
            sliding_window: true,
            sliding_window_duration: Duration::days(60), // Longer than token_lifetime
            one_time_use: false,
            auto_rotate: false,
            token_lifetime: Duration::days(30),
            ..Default::default()
        };

        let manager = RotationManager::with_config(config);
        let mut metadata = RefreshTokenMetadata::new("user123", "token123");
        metadata.set_max_uses(10); // Allow multiple uses

        manager.issue_token(metadata).await.unwrap();

        // Get original expiry (set by issue_token to now + 30 days)
        let original_expiry = manager
            .validate_token("token123")
            .await
            .unwrap()
            .expires_at();

        // Wait a bit to ensure time difference
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Use token - should extend to now + 60 days
        manager.use_token("token123").await.unwrap();

        // Expiration should be extended
        let new_expiry = manager
            .validate_token("token123")
            .await
            .unwrap()
            .expires_at();
        assert!(new_expiry > original_expiry);
    }

    #[tokio::test]
    async fn test_get_user_tokens() {
        let manager = RotationManager::new();

        let metadata1 = RefreshTokenMetadata::new("user123", "token1");
        let metadata2 = RefreshTokenMetadata::new("user123", "token2");
        let metadata3 = RefreshTokenMetadata::new("user456", "token3");

        manager.issue_token(metadata1).await.unwrap();
        manager.issue_token(metadata2).await.unwrap();
        manager.issue_token(metadata3).await.unwrap();

        let user_tokens = manager.get_user_tokens("user123").await;
        assert_eq!(user_tokens.len(), 2);
    }

    #[test]
    fn test_rotation_config_builder() {
        let config = RotationConfig::builder()
            .auto_rotate(false)
            .one_time_use(false)
            .sliding_window(true)
            .sliding_window_duration(Duration::days(14))
            .token_lifetime(Duration::days(60))
            .rotation_grace_period(Duration::minutes(15))
            .build();

        assert!(!config.auto_rotate);
        assert!(!config.one_time_use);
        assert!(config.sliding_window);
        assert_eq!(config.sliding_window_duration, Duration::days(14));
        assert_eq!(config.token_lifetime, Duration::days(60));
        assert_eq!(config.rotation_grace_period, Duration::minutes(15));
    }
}
