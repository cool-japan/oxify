//! Access control module for API key management, rate limiting, and quotas.
//!
//! This module provides:
//! - API key generation and validation
//! - Rate limiting (requests per minute/hour/day)
//! - Usage quotas (total requests, bytes processed)
//! - Permission-based access control

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Permission levels for access control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Read-only access (process images)
    Read,
    /// Write access (manage API keys)
    Write,
    /// Admin access (all operations)
    Admin,
}

impl Permission {
    /// Check if this permission includes another permission
    pub fn includes(&self, other: Permission) -> bool {
        match self {
            Permission::Admin => true,
            Permission::Write => matches!(other, Permission::Read | Permission::Write),
            Permission::Read => matches!(other, Permission::Read),
        }
    }
}

/// API key with associated permissions and quotas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique key identifier
    pub key: String,

    /// Human-readable name
    pub name: String,

    /// Permissions granted to this key
    pub permissions: Vec<Permission>,

    /// Rate limit (requests per minute)
    pub rate_limit_per_minute: Option<u64>,

    /// Rate limit (requests per hour)
    pub rate_limit_per_hour: Option<u64>,

    /// Rate limit (requests per day)
    pub rate_limit_per_day: Option<u64>,

    /// Total quota (maximum total requests)
    pub total_quota: Option<u64>,

    /// Bytes quota (maximum bytes processed)
    pub bytes_quota: Option<u64>,

    /// Whether the key is active
    pub active: bool,

    /// Creation timestamp (Unix timestamp in seconds)
    pub created_at: u64,

    /// Expiration timestamp (Unix timestamp in seconds, None = never expires)
    pub expires_at: Option<u64>,

    /// Optional metadata
    pub metadata: HashMap<String, String>,
}

impl ApiKey {
    /// Create a new API key
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            key: generate_api_key(),
            name: name.into(),
            permissions: vec![Permission::Read],
            rate_limit_per_minute: None,
            rate_limit_per_hour: None,
            rate_limit_per_day: None,
            total_quota: None,
            bytes_quota: None,
            active: true,
            created_at: current_timestamp(),
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Set permissions
    pub fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.permissions = permissions;
        self
    }

    /// Set rate limit per minute
    pub fn with_rate_limit_per_minute(mut self, limit: u64) -> Self {
        self.rate_limit_per_minute = Some(limit);
        self
    }

    /// Set rate limit per hour
    pub fn with_rate_limit_per_hour(mut self, limit: u64) -> Self {
        self.rate_limit_per_hour = Some(limit);
        self
    }

    /// Set rate limit per day
    pub fn with_rate_limit_per_day(mut self, limit: u64) -> Self {
        self.rate_limit_per_day = Some(limit);
        self
    }

    /// Set total quota
    pub fn with_total_quota(mut self, quota: u64) -> Self {
        self.total_quota = Some(quota);
        self
    }

    /// Set bytes quota
    pub fn with_bytes_quota(mut self, quota: u64) -> Self {
        self.bytes_quota = Some(quota);
        self
    }

    /// Set expiration time
    pub fn with_expiration(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if the key has a specific permission
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.permissions.iter().any(|p| p.includes(permission))
    }

    /// Check if the key is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            current_timestamp() >= expires_at
        } else {
            false
        }
    }

    /// Check if the key is valid (active and not expired)
    pub fn is_valid(&self) -> bool {
        self.active && !self.is_expired()
    }
}

/// Generate a random API key
fn generate_api_key() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let mut hasher = RandomState::new().build_hasher();
    let timestamp = current_timestamp();
    hasher.write_u64(timestamp);

    // Generate a pseudo-random key
    let hash = hasher.finish();
    format!("oxify_{:016x}{:016x}", hash, timestamp)
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Usage statistics for an API key
#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    /// Total requests made
    pub total_requests: Arc<AtomicU64>,

    /// Total bytes processed
    pub total_bytes: Arc<AtomicU64>,

    /// Requests in the current minute
    pub requests_this_minute: Arc<AtomicU64>,

    /// Requests in the current hour
    pub requests_this_hour: Arc<AtomicU64>,

    /// Requests in the current day
    pub requests_this_day: Arc<AtomicU64>,

    /// Timestamp of last request
    pub last_request_at: Arc<AtomicU64>,

    /// Timestamp for minute window
    pub minute_window_start: Arc<AtomicU64>,

    /// Timestamp for hour window
    pub hour_window_start: Arc<AtomicU64>,

    /// Timestamp for day window
    pub day_window_start: Arc<AtomicU64>,
}

impl UsageStats {
    /// Create new usage stats
    pub fn new() -> Self {
        let now = current_timestamp();
        Self {
            total_requests: Arc::new(AtomicU64::new(0)),
            total_bytes: Arc::new(AtomicU64::new(0)),
            requests_this_minute: Arc::new(AtomicU64::new(0)),
            requests_this_hour: Arc::new(AtomicU64::new(0)),
            requests_this_day: Arc::new(AtomicU64::new(0)),
            last_request_at: Arc::new(AtomicU64::new(0)),
            minute_window_start: Arc::new(AtomicU64::new(now)),
            hour_window_start: Arc::new(AtomicU64::new(now)),
            day_window_start: Arc::new(AtomicU64::new(now)),
        }
    }

    /// Record a request
    pub fn record_request(&self, bytes: u64) {
        let now = current_timestamp();

        // Update total counters
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.last_request_at.store(now, Ordering::Relaxed);

        // Reset windows if needed
        self.reset_windows_if_needed(now);

        // Increment window counters
        self.requests_this_minute.fetch_add(1, Ordering::Relaxed);
        self.requests_this_hour.fetch_add(1, Ordering::Relaxed);
        self.requests_this_day.fetch_add(1, Ordering::Relaxed);
    }

    /// Reset windows if they've expired
    fn reset_windows_if_needed(&self, now: u64) {
        // Reset minute window
        let minute_start = self.minute_window_start.load(Ordering::Relaxed);
        if now - minute_start >= 60 {
            self.requests_this_minute.store(0, Ordering::Relaxed);
            self.minute_window_start.store(now, Ordering::Relaxed);
        }

        // Reset hour window
        let hour_start = self.hour_window_start.load(Ordering::Relaxed);
        if now - hour_start >= 3600 {
            self.requests_this_hour.store(0, Ordering::Relaxed);
            self.hour_window_start.store(now, Ordering::Relaxed);
        }

        // Reset day window
        let day_start = self.day_window_start.load(Ordering::Relaxed);
        if now - day_start >= 86400 {
            self.requests_this_day.store(0, Ordering::Relaxed);
            self.day_window_start.store(now, Ordering::Relaxed);
        }
    }

    /// Get current statistics
    pub fn get(&self) -> UsageSnapshot {
        UsageSnapshot {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            requests_this_minute: self.requests_this_minute.load(Ordering::Relaxed),
            requests_this_hour: self.requests_this_hour.load(Ordering::Relaxed),
            requests_this_day: self.requests_this_day.load(Ordering::Relaxed),
            last_request_at: self.last_request_at.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub total_requests: u64,
    pub total_bytes: u64,
    pub requests_this_minute: u64,
    pub requests_this_hour: u64,
    pub requests_this_day: u64,
    pub last_request_at: u64,
}

/// Access control error
#[derive(Debug, Clone, thiserror::Error)]
pub enum AccessError {
    /// Invalid API key
    #[error("Invalid API key")]
    InvalidKey,

    /// API key expired
    #[error("API key expired")]
    Expired,

    /// API key inactive
    #[error("API key inactive")]
    Inactive,

    /// Insufficient permissions
    #[error("Insufficient permissions: {0}")]
    InsufficientPermissions(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// Quota exceeded
    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),
}

/// Access controller for managing API keys and enforcing limits
pub struct AccessController {
    keys: Arc<RwLock<HashMap<String, ApiKey>>>,
    usage: Arc<RwLock<HashMap<String, UsageStats>>>,
}

impl AccessController {
    /// Create a new access controller
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            usage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new API key
    pub fn create_key(&self, name: impl Into<String>) -> ApiKey {
        let key = ApiKey::new(name);
        let mut keys = self.keys.write().unwrap();
        let mut usage = self.usage.write().unwrap();

        keys.insert(key.key.clone(), key.clone());
        usage.insert(key.key.clone(), UsageStats::new());

        key
    }

    /// Create a custom API key
    pub fn create_custom_key(&self, key: ApiKey) -> String {
        let key_str = key.key.clone();
        let mut keys = self.keys.write().unwrap();
        let mut usage = self.usage.write().unwrap();

        keys.insert(key_str.clone(), key);
        usage.insert(key_str.clone(), UsageStats::new());

        key_str
    }

    /// Get an API key
    pub fn get_key(&self, key: &str) -> Option<ApiKey> {
        self.keys.read().unwrap().get(key).cloned()
    }

    /// Revoke an API key
    pub fn revoke_key(&self, key: &str) -> bool {
        if let Some(api_key) = self.keys.write().unwrap().get_mut(key) {
            api_key.active = false;
            true
        } else {
            false
        }
    }

    /// Delete an API key
    pub fn delete_key(&self, key: &str) -> bool {
        let mut keys = self.keys.write().unwrap();
        let mut usage = self.usage.write().unwrap();

        keys.remove(key).is_some() || usage.remove(key).is_some()
    }

    /// Validate an API key and check permissions
    pub fn validate(
        &self,
        key: &str,
        permission: Permission,
        bytes: u64,
    ) -> Result<(), AccessError> {
        // Get the API key
        let api_key = self.get_key(key).ok_or(AccessError::InvalidKey)?;

        // Check if key is valid
        if !api_key.active {
            return Err(AccessError::Inactive);
        }

        if api_key.is_expired() {
            return Err(AccessError::Expired);
        }

        // Check permissions
        if !api_key.has_permission(permission) {
            return Err(AccessError::InsufficientPermissions(format!(
                "Required: {:?}",
                permission
            )));
        }

        // Get usage stats
        let usage = self.usage.read().unwrap();
        let stats = usage.get(key).ok_or(AccessError::InvalidKey)?;

        // Reset windows if needed
        stats.reset_windows_if_needed(current_timestamp());

        // Check rate limits
        if let Some(limit) = api_key.rate_limit_per_minute {
            let current = stats.requests_this_minute.load(Ordering::Relaxed);
            if current >= limit {
                return Err(AccessError::RateLimitExceeded(format!(
                    "{} requests per minute",
                    limit
                )));
            }
        }

        if let Some(limit) = api_key.rate_limit_per_hour {
            let current = stats.requests_this_hour.load(Ordering::Relaxed);
            if current >= limit {
                return Err(AccessError::RateLimitExceeded(format!(
                    "{} requests per hour",
                    limit
                )));
            }
        }

        if let Some(limit) = api_key.rate_limit_per_day {
            let current = stats.requests_this_day.load(Ordering::Relaxed);
            if current >= limit {
                return Err(AccessError::RateLimitExceeded(format!(
                    "{} requests per day",
                    limit
                )));
            }
        }

        // Check total quota
        if let Some(quota) = api_key.total_quota {
            let current = stats.total_requests.load(Ordering::Relaxed);
            if current >= quota {
                return Err(AccessError::QuotaExceeded(format!(
                    "Total quota of {} requests",
                    quota
                )));
            }
        }

        // Check bytes quota
        if let Some(quota) = api_key.bytes_quota {
            let current = stats.total_bytes.load(Ordering::Relaxed);
            if current + bytes > quota {
                return Err(AccessError::QuotaExceeded(format!(
                    "Bytes quota of {} bytes",
                    quota
                )));
            }
        }

        // Record the request
        stats.record_request(bytes);

        Ok(())
    }

    /// Get usage statistics for a key
    pub fn get_usage(&self, key: &str) -> Option<UsageSnapshot> {
        self.usage.read().unwrap().get(key).map(|stats| stats.get())
    }

    /// List all API keys
    pub fn list_keys(&self) -> Vec<ApiKey> {
        self.keys.read().unwrap().values().cloned().collect()
    }

    /// Get statistics for all keys
    pub fn get_all_usage(&self) -> HashMap<String, UsageSnapshot> {
        self.usage
            .read()
            .unwrap()
            .iter()
            .map(|(key, stats)| (key.clone(), stats.get()))
            .collect()
    }
}

impl Default for AccessController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_includes() {
        assert!(Permission::Admin.includes(Permission::Read));
        assert!(Permission::Admin.includes(Permission::Write));
        assert!(Permission::Admin.includes(Permission::Admin));

        assert!(Permission::Write.includes(Permission::Read));
        assert!(Permission::Write.includes(Permission::Write));
        assert!(!Permission::Write.includes(Permission::Admin));

        assert!(Permission::Read.includes(Permission::Read));
        assert!(!Permission::Read.includes(Permission::Write));
        assert!(!Permission::Read.includes(Permission::Admin));
    }

    #[test]
    fn test_api_key_creation() {
        let key = ApiKey::new("test-key");
        assert_eq!(key.name, "test-key");
        assert!(key.key.starts_with("oxify_"));
        assert!(key.active);
        assert!(!key.is_expired());
    }

    #[test]
    fn test_api_key_with_permissions() {
        let key = ApiKey::new("test").with_permissions(vec![Permission::Read, Permission::Write]);

        assert!(key.has_permission(Permission::Read));
        assert!(key.has_permission(Permission::Write));
        assert!(!key.has_permission(Permission::Admin));
    }

    #[test]
    fn test_api_key_with_rate_limits() {
        let key = ApiKey::new("test")
            .with_rate_limit_per_minute(100)
            .with_rate_limit_per_hour(1000);

        assert_eq!(key.rate_limit_per_minute, Some(100));
        assert_eq!(key.rate_limit_per_hour, Some(1000));
    }

    #[test]
    fn test_api_key_with_quotas() {
        let key = ApiKey::new("test")
            .with_total_quota(5000)
            .with_bytes_quota(1_000_000);

        assert_eq!(key.total_quota, Some(5000));
        assert_eq!(key.bytes_quota, Some(1_000_000));
    }

    #[test]
    fn test_api_key_expiration() {
        let past = current_timestamp() - 3600;
        let future = current_timestamp() + 3600;

        let expired_key = ApiKey::new("test").with_expiration(past);
        assert!(expired_key.is_expired());

        let valid_key = ApiKey::new("test").with_expiration(future);
        assert!(!valid_key.is_expired());
    }

    #[test]
    fn test_usage_stats_record_request() {
        let stats = UsageStats::new();
        stats.record_request(1024);

        let snapshot = stats.get();
        assert_eq!(snapshot.total_requests, 1);
        assert_eq!(snapshot.total_bytes, 1024);
        assert_eq!(snapshot.requests_this_minute, 1);
    }

    #[test]
    fn test_access_controller_create_key() {
        let controller = AccessController::new();
        let key = controller.create_key("test-key");

        assert_eq!(key.name, "test-key");
        assert!(controller.get_key(&key.key).is_some());
    }

    #[test]
    fn test_access_controller_revoke_key() {
        let controller = AccessController::new();
        let key = controller.create_key("test");

        assert!(controller.revoke_key(&key.key));

        let revoked = controller.get_key(&key.key).unwrap();
        assert!(!revoked.active);
    }

    #[test]
    fn test_access_controller_delete_key() {
        let controller = AccessController::new();
        let key = controller.create_key("test");

        assert!(controller.delete_key(&key.key));
        assert!(controller.get_key(&key.key).is_none());
    }

    #[test]
    fn test_access_controller_validate_invalid_key() {
        let controller = AccessController::new();
        let result = controller.validate("invalid-key", Permission::Read, 0);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AccessError::InvalidKey));
    }

    #[test]
    fn test_access_controller_validate_inactive_key() {
        let controller = AccessController::new();
        let key = controller.create_key("test");
        controller.revoke_key(&key.key);

        let result = controller.validate(&key.key, Permission::Read, 0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AccessError::Inactive));
    }

    #[test]
    fn test_access_controller_validate_permissions() {
        let controller = AccessController::new();
        let key = ApiKey::new("test").with_permissions(vec![Permission::Read]);
        let key_str = controller.create_custom_key(key);

        let result = controller.validate(&key_str, Permission::Write, 0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AccessError::InsufficientPermissions(_)
        ));
    }

    #[test]
    fn test_access_controller_validate_rate_limit() {
        let controller = AccessController::new();
        let key = ApiKey::new("test").with_rate_limit_per_minute(1);
        let key_str = controller.create_custom_key(key);

        // First request should succeed
        assert!(controller.validate(&key_str, Permission::Read, 0).is_ok());

        // Second request should fail
        let result = controller.validate(&key_str, Permission::Read, 0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AccessError::RateLimitExceeded(_)
        ));
    }

    #[test]
    fn test_access_controller_validate_quota() {
        let controller = AccessController::new();
        let key = ApiKey::new("test").with_total_quota(1);
        let key_str = controller.create_custom_key(key);

        // First request should succeed
        assert!(controller.validate(&key_str, Permission::Read, 0).is_ok());

        // Second request should fail
        let result = controller.validate(&key_str, Permission::Read, 0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AccessError::QuotaExceeded(_)));
    }

    #[test]
    fn test_access_controller_get_usage() {
        let controller = AccessController::new();
        let key = controller.create_key("test");

        controller
            .validate(&key.key, Permission::Read, 1024)
            .unwrap();

        let usage = controller.get_usage(&key.key).unwrap();
        assert_eq!(usage.total_requests, 1);
        assert_eq!(usage.total_bytes, 1024);
    }

    #[test]
    fn test_access_controller_list_keys() {
        let controller = AccessController::new();
        controller.create_key("key1");
        controller.create_key("key2");

        let keys = controller.list_keys();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_generate_api_key_format() {
        let key = generate_api_key();
        assert!(key.starts_with("oxify_"));
        assert!(key.len() > 10);
    }
}
