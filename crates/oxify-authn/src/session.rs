//! Session management for stateful authentication
//!
//! Provides session creation, validation, and management for scenarios
//! where stateful sessions are preferred over stateless JWTs.
//!
//! # Features
//! - Session creation with unique IDs
//! - Session validation and refresh
//! - Session invalidation (logout)
//! - Multi-device session tracking
//! - Session expiration handling
//! - Optional IP and User-Agent binding
//!
//! # Example
//!
//! ```no_run
//! use oxify_authn::session::{SessionManager, SessionConfig, SessionInfo};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = SessionConfig::default();
//! let manager = SessionManager::new(config);
//!
//! // Create a session for user
//! let session_info = SessionInfo {
//!     user_id: "user123".to_string(),
//!     ip_address: Some("192.168.1.1".to_string()),
//!     user_agent: Some("Mozilla/5.0".to_string()),
//!     device_name: Some("Chrome on macOS".to_string()),
//! };
//!
//! let session = manager.create_session(session_info).await?;
//! println!("Session ID: {}", session.id);
//!
//! // Validate session
//! let valid = manager.validate_session(&session.id).await?;
//! assert!(valid.is_some());
//!
//! // Invalidate session (logout)
//! manager.invalidate_session(&session.id).await?;
//! # Ok(())
//! # }
//! ```

use crate::types::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Session timeout in seconds (default: 1 hour)
    pub timeout_secs: u64,
    /// Maximum sessions per user (0 = unlimited)
    pub max_sessions_per_user: usize,
    /// Whether to bind sessions to IP address
    pub bind_to_ip: bool,
    /// Whether to validate user agent
    pub validate_user_agent: bool,
    /// Sliding window: refresh expiration on activity
    pub sliding_window: bool,
    /// Inactivity timeout in seconds (0 = use `timeout_secs`)
    pub inactivity_timeout_secs: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 3600,            // 1 hour
            max_sessions_per_user: 5,      // 5 concurrent sessions
            bind_to_ip: false,             // Disabled by default
            validate_user_agent: false,    // Disabled by default
            sliding_window: true,          // Refresh on activity
            inactivity_timeout_secs: 1800, // 30 minutes
        }
    }
}

impl SessionConfig {
    /// Create a new builder for `SessionConfig`
    #[must_use]
    pub fn builder() -> SessionConfigBuilder {
        SessionConfigBuilder::new()
    }

    /// Create a strict configuration for enhanced security
    #[must_use]
    pub fn strict() -> Self {
        Self {
            timeout_secs: 1800,           // 30 minutes
            max_sessions_per_user: 3,     // 3 concurrent sessions
            bind_to_ip: true,             // IP binding enabled
            validate_user_agent: true,    // User agent validation enabled
            sliding_window: false,        // No sliding window
            inactivity_timeout_secs: 600, // 10 minutes
        }
    }

    /// Create a relaxed configuration for less sensitive applications
    #[must_use]
    pub fn relaxed() -> Self {
        Self {
            timeout_secs: 86400,            // 24 hours
            max_sessions_per_user: 10,      // 10 concurrent sessions
            bind_to_ip: false,              // IP binding disabled
            validate_user_agent: false,     // User agent validation disabled
            sliding_window: true,           // Sliding window enabled
            inactivity_timeout_secs: 43200, // 12 hours
        }
    }
}

/// Builder for `SessionConfig`
#[derive(Debug, Clone)]
pub struct SessionConfigBuilder {
    timeout_secs: u64,
    max_sessions_per_user: usize,
    bind_to_ip: bool,
    validate_user_agent: bool,
    sliding_window: bool,
    inactivity_timeout_secs: u64,
}

impl SessionConfigBuilder {
    /// Create a new builder with default values
    #[must_use]
    pub fn new() -> Self {
        let default = SessionConfig::default();
        Self {
            timeout_secs: default.timeout_secs,
            max_sessions_per_user: default.max_sessions_per_user,
            bind_to_ip: default.bind_to_ip,
            validate_user_agent: default.validate_user_agent,
            sliding_window: default.sliding_window,
            inactivity_timeout_secs: default.inactivity_timeout_secs,
        }
    }

    /// Set session timeout in seconds
    #[must_use]
    pub fn timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set maximum sessions per user
    #[must_use]
    pub fn max_sessions_per_user(mut self, max_sessions: usize) -> Self {
        self.max_sessions_per_user = max_sessions;
        self
    }

    /// Set whether to bind sessions to IP address
    #[must_use]
    pub fn bind_to_ip(mut self, bind: bool) -> Self {
        self.bind_to_ip = bind;
        self
    }

    /// Set whether to validate user agent
    #[must_use]
    pub fn validate_user_agent(mut self, validate: bool) -> Self {
        self.validate_user_agent = validate;
        self
    }

    /// Set whether to use sliding window expiration
    #[must_use]
    pub fn sliding_window(mut self, sliding: bool) -> Self {
        self.sliding_window = sliding;
        self
    }

    /// Set inactivity timeout in seconds
    #[must_use]
    pub fn inactivity_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.inactivity_timeout_secs = timeout_secs;
        self
    }

    /// Build the `SessionConfig`
    #[must_use]
    pub fn build(self) -> SessionConfig {
        SessionConfig {
            timeout_secs: self.timeout_secs,
            max_sessions_per_user: self.max_sessions_per_user,
            bind_to_ip: self.bind_to_ip,
            validate_user_agent: self.validate_user_agent,
            sliding_window: self.sliding_window,
            inactivity_timeout_secs: self.inactivity_timeout_secs,
        }
    }
}

impl Default for SessionConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Session information provided when creating a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// User identifier
    pub user_id: String,
    /// Client IP address (optional)
    pub ip_address: Option<String>,
    /// Client User-Agent (optional)
    pub user_agent: Option<String>,
    /// Device name for display (optional)
    pub device_name: Option<String>,
}

/// A session record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID
    pub id: String,
    /// User identifier
    pub user_id: String,
    /// Session creation time
    pub created_at: DateTime<Utc>,
    /// Session expiration time
    pub expires_at: DateTime<Utc>,
    /// Last activity time
    pub last_activity: DateTime<Utc>,
    /// Client IP address (if bound)
    pub ip_address: Option<String>,
    /// Client User-Agent (if bound)
    pub user_agent: Option<String>,
    /// Device name for display
    pub device_name: Option<String>,
    /// Whether session is revoked
    pub revoked: bool,
}

impl Session {
    /// Check if session is expired
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if session is valid (not expired and not revoked)
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.is_expired() && !self.revoked
    }
}

/// Session manager for creating and managing sessions
///
/// Note: This is an in-memory implementation suitable for single-instance deployments.
/// For distributed systems, implement the `SessionStore` trait with Redis or database backing.
pub struct SessionManager {
    config: SessionConfig,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    user_sessions: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl SessionManager {
    /// Create a new session manager
    #[must_use]
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            user_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new session for a user
    pub async fn create_session(&self, info: SessionInfo) -> Result<Session> {
        // Check max sessions per user
        if self.config.max_sessions_per_user > 0 {
            let user_sessions = self.user_sessions.read().await;
            if let Some(sessions) = user_sessions.get(&info.user_id) {
                if sessions.len() >= self.config.max_sessions_per_user {
                    // Remove oldest session
                    drop(user_sessions);
                    self.remove_oldest_session(&info.user_id).await?;
                }
            }
        }

        let now = Utc::now();
        let session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: info.user_id.clone(),
            created_at: now,
            expires_at: now + Duration::seconds(self.config.timeout_secs as i64),
            last_activity: now,
            ip_address: info.ip_address,
            user_agent: info.user_agent,
            device_name: info.device_name,
            revoked: false,
        };

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session.id.clone(), session.clone());
        }

        // Track user sessions
        {
            let mut user_sessions = self.user_sessions.write().await;
            user_sessions
                .entry(info.user_id)
                .or_default()
                .push(session.id.clone());
        }

        Ok(session)
    }

    /// Validate a session and optionally refresh it
    pub async fn validate_session(&self, session_id: &str) -> Result<Option<Session>> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            if session.revoked {
                return Ok(None);
            }

            if session.is_expired() {
                return Ok(None);
            }

            // Check inactivity timeout
            if self.config.inactivity_timeout_secs > 0 {
                let inactivity = Utc::now() - session.last_activity;
                if inactivity.num_seconds() > self.config.inactivity_timeout_secs as i64 {
                    session.revoked = true;
                    return Ok(None);
                }
            }

            // Sliding window: refresh expiration
            if self.config.sliding_window {
                session.last_activity = Utc::now();
                session.expires_at =
                    Utc::now() + Duration::seconds(self.config.timeout_secs as i64);
            } else {
                session.last_activity = Utc::now();
            }

            return Ok(Some(session.clone()));
        }

        Ok(None)
    }

    /// Validate session with IP and User-Agent binding
    pub async fn validate_session_strict(
        &self,
        session_id: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<Option<Session>> {
        let session = self.validate_session(session_id).await?;

        if let Some(ref session) = session {
            // Check IP binding
            if self.config.bind_to_ip {
                if let (Some(session_ip), Some(request_ip)) = (&session.ip_address, ip_address) {
                    if session_ip != request_ip {
                        return Ok(None);
                    }
                }
            }

            // Check User-Agent
            if self.config.validate_user_agent {
                if let (Some(session_ua), Some(request_ua)) = (&session.user_agent, user_agent) {
                    if session_ua != request_ua {
                        return Ok(None);
                    }
                }
            }
        }

        Ok(session)
    }

    /// Invalidate (revoke) a session
    pub async fn invalidate_session(&self, session_id: &str) -> Result<bool> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            session.revoked = true;
            return Ok(true);
        }

        Ok(false)
    }

    /// Invalidate all sessions for a user (logout from all devices)
    pub async fn invalidate_all_user_sessions(&self, user_id: &str) -> Result<usize> {
        let user_sessions = self.user_sessions.read().await;
        let session_ids = user_sessions.get(user_id).cloned().unwrap_or_default();
        drop(user_sessions);

        let mut count = 0;
        for session_id in session_ids {
            if self.invalidate_session(&session_id).await? {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Get all active sessions for a user
    pub async fn get_user_sessions(&self, user_id: &str) -> Result<Vec<Session>> {
        let sessions = self.sessions.read().await;
        let user_sessions = self.user_sessions.read().await;

        let session_ids = user_sessions.get(user_id).cloned().unwrap_or_default();

        let mut result = Vec::new();
        for session_id in session_ids {
            if let Some(session) = sessions.get(&session_id) {
                if session.is_valid() {
                    result.push(session.clone());
                }
            }
        }

        Ok(result)
    }

    /// Cleanup expired sessions
    pub async fn cleanup_expired(&self) -> usize {
        let mut sessions = self.sessions.write().await;
        let mut user_sessions = self.user_sessions.write().await;

        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.is_expired() || s.revoked)
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired.len();

        for session_id in &expired {
            if let Some(session) = sessions.remove(session_id) {
                if let Some(user_sids) = user_sessions.get_mut(&session.user_id) {
                    user_sids.retain(|id| id != session_id);
                }
            }
        }

        count
    }

    /// Remove oldest session for a user
    async fn remove_oldest_session(&self, user_id: &str) -> Result<()> {
        let sessions = self.sessions.read().await;
        let user_sessions_guard = self.user_sessions.read().await;

        if let Some(session_ids) = user_sessions_guard.get(user_id) {
            let oldest = session_ids
                .iter()
                .filter_map(|id| sessions.get(id))
                .min_by_key(|s| s.created_at)
                .map(|s| s.id.clone());

            drop(sessions);
            drop(user_sessions_guard);

            if let Some(oldest_id) = oldest {
                self.invalidate_session(&oldest_id).await?;
            }
        }

        Ok(())
    }

    /// Get session count for a user
    pub async fn get_session_count(&self, user_id: &str) -> usize {
        let user_sessions = self.user_sessions.read().await;
        user_sessions.get(user_id).map_or(0, std::vec::Vec::len)
    }

    /// Get total active session count
    pub async fn get_total_session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.values().filter(|s| s.is_valid()).count()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(SessionConfig::default())
    }
}

/// Trait for custom session storage implementations
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    /// Store a session
    async fn store(&self, session: &Session) -> Result<()>;

    /// Get a session by ID
    async fn get(&self, session_id: &str) -> Result<Option<Session>>;

    /// Update a session
    async fn update(&self, session: &Session) -> Result<()>;

    /// Delete a session
    async fn delete(&self, session_id: &str) -> Result<()>;

    /// Get all sessions for a user
    async fn get_by_user(&self, user_id: &str) -> Result<Vec<Session>>;

    /// Delete all sessions for a user
    async fn delete_by_user(&self, user_id: &str) -> Result<usize>;

    /// Cleanup expired sessions
    async fn cleanup_expired(&self) -> Result<usize>;
}

/// In-memory session store implementation
pub struct InMemorySessionStore {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl InMemorySessionStore {
    /// Create a new in-memory session store
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SessionStore for InMemorySessionStore {
    async fn store(&self, session: &Session) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn get(&self, session_id: &str) -> Result<Option<Session>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned())
    }

    async fn update(&self, session: &Session) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn delete(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        Ok(())
    }

    async fn get_by_user(&self, user_id: &str) -> Result<Vec<Session>> {
        let sessions = self.sessions.read().await;
        Ok(sessions
            .values()
            .filter(|s| s.user_id == user_id && s.is_valid())
            .cloned()
            .collect())
    }

    async fn delete_by_user(&self, user_id: &str) -> Result<usize> {
        let mut sessions = self.sessions.write().await;
        let to_remove: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.user_id == user_id)
            .map(|(id, _)| id.clone())
            .collect();
        let count = to_remove.len();
        for id in to_remove {
            sessions.remove(&id);
        }
        Ok(count)
    }

    async fn cleanup_expired(&self) -> Result<usize> {
        let mut sessions = self.sessions.write().await;
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.is_expired() || s.revoked)
            .map(|(id, _)| id.clone())
            .collect();
        let count = expired.len();
        for id in expired {
            sessions.remove(&id);
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_creation() {
        let manager = SessionManager::default();

        let info = SessionInfo {
            user_id: "user1".to_string(),
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("Test Agent".to_string()),
            device_name: Some("Test Device".to_string()),
        };

        let session = manager.create_session(info).await.unwrap();

        assert!(!session.id.is_empty());
        assert_eq!(session.user_id, "user1");
        assert!(session.is_valid());
    }

    #[tokio::test]
    async fn test_session_validation() {
        let manager = SessionManager::default();

        let info = SessionInfo {
            user_id: "user1".to_string(),
            ip_address: None,
            user_agent: None,
            device_name: None,
        };

        let session = manager.create_session(info).await.unwrap();

        let validated = manager.validate_session(&session.id).await.unwrap();
        assert!(validated.is_some());

        // Invalid session ID
        let validated = manager.validate_session("nonexistent").await.unwrap();
        assert!(validated.is_none());
    }

    #[tokio::test]
    async fn test_session_invalidation() {
        let manager = SessionManager::default();

        let info = SessionInfo {
            user_id: "user1".to_string(),
            ip_address: None,
            user_agent: None,
            device_name: None,
        };

        let session = manager.create_session(info).await.unwrap();

        // Invalidate
        let result = manager.invalidate_session(&session.id).await.unwrap();
        assert!(result);

        // Should no longer be valid
        let validated = manager.validate_session(&session.id).await.unwrap();
        assert!(validated.is_none());
    }

    #[tokio::test]
    async fn test_max_sessions_per_user() {
        let config = SessionConfig {
            max_sessions_per_user: 2,
            ..Default::default()
        };
        let manager = SessionManager::new(config);

        // Create 3 sessions for same user
        for i in 0..3 {
            let info = SessionInfo {
                user_id: "user1".to_string(),
                ip_address: None,
                user_agent: None,
                device_name: Some(format!("Device {i}")),
            };
            manager.create_session(info).await.unwrap();
        }

        // Should only have 2 active sessions
        let sessions = manager.get_user_sessions("user1").await.unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_invalidate_all_user_sessions() {
        let manager = SessionManager::default();

        // Create multiple sessions
        for _ in 0..3 {
            let info = SessionInfo {
                user_id: "user1".to_string(),
                ip_address: None,
                user_agent: None,
                device_name: None,
            };
            manager.create_session(info).await.unwrap();
        }

        // Invalidate all
        let count = manager.invalidate_all_user_sessions("user1").await.unwrap();
        assert_eq!(count, 3);

        // Should have no active sessions
        let sessions = manager.get_user_sessions("user1").await.unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_session_store_trait() {
        let store = InMemorySessionStore::new();

        let session = Session {
            id: "test-session-1".to_string(),
            user_id: "user1".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
            last_activity: Utc::now(),
            ip_address: None,
            user_agent: None,
            device_name: None,
            revoked: false,
        };

        // Store
        store.store(&session).await.unwrap();

        // Get
        let retrieved = store.get(&session.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().user_id, "user1");

        // Delete
        store.delete(&session.id).await.unwrap();
        let retrieved = store.get(&session.id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let store = InMemorySessionStore::new();

        // Create expired session
        let expired_session = Session {
            id: "expired-session".to_string(),
            user_id: "user1".to_string(),
            created_at: Utc::now() - Duration::hours(2),
            expires_at: Utc::now() - Duration::hours(1),
            last_activity: Utc::now() - Duration::hours(2),
            ip_address: None,
            user_agent: None,
            device_name: None,
            revoked: false,
        };

        // Create valid session
        let valid_session = Session {
            id: "valid-session".to_string(),
            user_id: "user1".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
            last_activity: Utc::now(),
            ip_address: None,
            user_agent: None,
            device_name: None,
            revoked: false,
        };

        store.store(&expired_session).await.unwrap();
        store.store(&valid_session).await.unwrap();

        // Cleanup
        let count = store.cleanup_expired().await.unwrap();
        assert_eq!(count, 1);

        // Expired should be gone
        assert!(store.get("expired-session").await.unwrap().is_none());
        // Valid should remain
        assert!(store.get("valid-session").await.unwrap().is_some());
    }

    #[test]
    fn test_session_config_builder() {
        let config = SessionConfig::builder()
            .timeout_secs(7200)
            .max_sessions_per_user(10)
            .bind_to_ip(true)
            .validate_user_agent(true)
            .sliding_window(false)
            .inactivity_timeout_secs(3600)
            .build();

        assert_eq!(config.timeout_secs, 7200);
        assert_eq!(config.max_sessions_per_user, 10);
        assert!(config.bind_to_ip);
        assert!(config.validate_user_agent);
        assert!(!config.sliding_window);
        assert_eq!(config.inactivity_timeout_secs, 3600);
    }

    #[test]
    fn test_session_config_presets() {
        // Test strict preset
        let strict = SessionConfig::strict();
        assert_eq!(strict.timeout_secs, 1800);
        assert_eq!(strict.max_sessions_per_user, 3);
        assert!(strict.bind_to_ip);
        assert!(strict.validate_user_agent);

        // Test relaxed preset
        let relaxed = SessionConfig::relaxed();
        assert_eq!(relaxed.timeout_secs, 86400);
        assert_eq!(relaxed.max_sessions_per_user, 10);
        assert!(!relaxed.bind_to_ip);
        assert!(!relaxed.validate_user_agent);
    }
}
