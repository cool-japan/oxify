//! Session Store — pluggable session persistence layer
//!
//! Provides a [`SessionStore`] trait with two built-in backends:
//!
//! * [`InMemorySessionStore`] — in-process `HashMap` with TTL-based expiry.
//!   Suitable for development and single-instance deployments.
//! * [`RedisSessionStore`] — Redis-backed, distributed session store (enabled
//!   via the `redis-cache` Cargo feature).  Uses `redis::aio::ConnectionManager`
//!   for automatic reconnection.
//!
//! ## Session lifecycle
//!
//! ```text
//! put(id, data, ttl)  →  store a new session (or overwrite an existing one)
//! get(id)             →  retrieve session data (returns None if expired)
//! touch(id, ttl)      →  extend TTL without modifying the payload
//! delete(id)          →  remove a session
//! ```

#[cfg(feature = "redis-cache")]
use redis::{aio::ConnectionManager, AsyncCommands};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::RwLock;

// ─── SessionData ─────────────────────────────────────────────────────────────

/// The payload stored for each session.
///
/// `extra` is an open-ended JSON object so that callers can attach arbitrary
/// per-session metadata without changing the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// Authenticated user identifier.
    pub user_id: String,
    /// Caller-defined supplementary data (arbitrary JSON object/array/scalar).
    pub extra: serde_json::Value,
    /// Wall-clock time at which the session was first created.
    pub created_at: DateTime<Utc>,
    /// Optional hard expiry stored alongside the data itself.
    /// The backends enforce their own TTL-based expiry independently of this
    /// field, but callers may use it for additional application-level checks.
    pub expires_at: Option<DateTime<Utc>>,
}

impl SessionData {
    /// Construct a session with `extra = null` and no explicit expiry timestamp.
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            extra: serde_json::Value::Null,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    /// Construct a session that carries an explicit `expires_at` derived from
    /// the current time plus the given TTL.
    pub fn with_ttl(user_id: impl Into<String>, ttl: Duration) -> Self {
        let now = Utc::now();
        let expires_at =
            now + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::seconds(3600));
        Self {
            user_id: user_id.into(),
            extra: serde_json::Value::Null,
            created_at: now,
            expires_at: Some(expires_at),
        }
    }
}

// ─── SessionError ────────────────────────────────────────────────────────────

/// Errors that can arise when interacting with a session store.
#[derive(Error, Debug)]
pub enum SessionError {
    /// The storage backend returned an error (e.g. Redis I/O failure).
    #[error("backend error: {0}")]
    Backend(String),
    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Convenience alias for results returned by [`SessionStore`] methods.
pub type SessionResult<T> = Result<T, SessionError>;

// ─── SessionStore trait ──────────────────────────────────────────────────────

/// Async, object-safe interface for session persistence.
///
/// All methods are async and require `&self`, so implementations must use
/// interior mutability (e.g. `Arc<RwLock<…>>`).
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Persist `data` under `session_id`, overwriting any existing entry.
    ///
    /// The entry **must** expire no later than `ttl` after this call.
    async fn put(&self, session_id: &str, data: SessionData, ttl: Duration) -> SessionResult<()>;

    /// Retrieve the session identified by `session_id`.
    ///
    /// Returns `None` if the session does not exist or has expired.
    async fn get(&self, session_id: &str) -> SessionResult<Option<SessionData>>;

    /// Remove the session identified by `session_id`.
    ///
    /// Silently succeeds if the session does not exist.
    async fn delete(&self, session_id: &str) -> SessionResult<()>;

    /// Extend the lifetime of an existing session by `ttl` from now.
    ///
    /// Returns `true` if the session was found and its TTL was updated,
    /// `false` if no session with the given ID exists.
    async fn touch(&self, session_id: &str, ttl: Duration) -> SessionResult<bool>;
}

// ─── InMemorySessionStore ────────────────────────────────────────────────────

/// In-process session store backed by a `HashMap<String, (SessionData, Instant)>`.
///
/// The `Instant` records the **absolute expiry time** of each entry. Expired
/// entries are lazily evicted on every [`get`][InMemorySessionStore::get] and
/// [`touch`][InMemorySessionStore::touch] call.
///
/// This implementation is safe to clone — all clones share the same underlying
/// data because the `Arc` is cloned, not the `RwLock`.
#[derive(Clone)]
pub struct InMemorySessionStore {
    /// Key → (payload, expiry instant).
    store: Arc<RwLock<HashMap<String, (SessionData, Instant)>>>,
}

impl InMemorySessionStore {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn put(&self, session_id: &str, data: SessionData, ttl: Duration) -> SessionResult<()> {
        let expiry = Instant::now() + ttl;
        let mut guard = self.store.write().await;
        guard.insert(session_id.to_owned(), (data, expiry));
        Ok(())
    }

    async fn get(&self, session_id: &str) -> SessionResult<Option<SessionData>> {
        // Use a read-lock first for the common non-expired path.
        {
            let guard = self.store.read().await;
            match guard.get(session_id) {
                None => return Ok(None),
                Some((data, expiry)) => {
                    if Instant::now() < *expiry {
                        return Ok(Some(data.clone()));
                    }
                    // Entry exists but has expired — fall through to evict it.
                }
            }
        }
        // Expired: evict under write-lock.
        let mut guard = self.store.write().await;
        // Re-check under the write-lock to avoid a TOCTOU race.
        if let Some((_data, expiry)) = guard.get(session_id) {
            if Instant::now() >= *expiry {
                guard.remove(session_id);
            }
        }
        Ok(None)
    }

    async fn delete(&self, session_id: &str) -> SessionResult<()> {
        let mut guard = self.store.write().await;
        guard.remove(session_id);
        Ok(())
    }

    async fn touch(&self, session_id: &str, ttl: Duration) -> SessionResult<bool> {
        let now = Instant::now();
        let mut guard = self.store.write().await;
        match guard.get_mut(session_id) {
            None => Ok(false),
            Some((_, expiry)) => {
                if now >= *expiry {
                    // Already expired — evict and report not found.
                    guard.remove(session_id);
                    Ok(false)
                } else {
                    *expiry = now + ttl;
                    Ok(true)
                }
            }
        }
    }
}

// ─── RedisSessionStore ───────────────────────────────────────────────────────

/// Redis-backed session store.
///
/// Sessions are stored as JSON blobs under keys of the form
/// `{prefix}{session_id}`. TTLs are managed entirely by Redis via `SET … EX`.
///
/// Requires the `redis-cache` Cargo feature.
#[cfg(feature = "redis-cache")]
pub struct RedisSessionStore {
    connection: ConnectionManager,
    /// Prefix prepended to every Redis key (e.g. `"oxify:session:"`).
    prefix: String,
}

#[cfg(feature = "redis-cache")]
impl RedisSessionStore {
    /// Connect to Redis and create a session store.
    ///
    /// # Errors
    ///
    /// Returns a [`SessionError::Backend`] if the connection cannot be established.
    pub async fn new(redis_url: impl AsRef<str>, prefix: impl Into<String>) -> SessionResult<Self> {
        let client = redis::Client::open(redis_url.as_ref())
            .map_err(|e| SessionError::Backend(e.to_string()))?;
        let connection = ConnectionManager::new(client)
            .await
            .map_err(|e| SessionError::Backend(e.to_string()))?;
        Ok(Self {
            connection,
            prefix: prefix.into(),
        })
    }

    /// Build the full Redis key for a session ID.
    fn redis_key(&self, session_id: &str) -> String {
        format!("{}{}", self.prefix, session_id)
    }
}

#[cfg(feature = "redis-cache")]
#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn put(&self, session_id: &str, data: SessionData, ttl: Duration) -> SessionResult<()> {
        let key = self.redis_key(session_id);
        let payload =
            serde_json::to_string(&data).map_err(|e| SessionError::Serialization(e.to_string()))?;

        // Clone the ConnectionManager so we can call async methods on `&mut self`.
        let mut conn = self.connection.clone();
        let ttl_secs = ttl.as_secs().max(1); // Redis requires TTL ≥ 1 s
        conn.set_ex::<_, _, ()>(&key, payload, ttl_secs)
            .await
            .map_err(|e| SessionError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, session_id: &str) -> SessionResult<Option<SessionData>> {
        let key = self.redis_key(session_id);
        let mut conn = self.connection.clone();
        let raw: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| SessionError::Backend(e.to_string()))?;

        match raw {
            None => Ok(None),
            Some(payload) => {
                let data: SessionData = serde_json::from_str(&payload)
                    .map_err(|e| SessionError::Serialization(e.to_string()))?;
                Ok(Some(data))
            }
        }
    }

    async fn delete(&self, session_id: &str) -> SessionResult<()> {
        let key = self.redis_key(session_id);
        let mut conn = self.connection.clone();
        conn.del::<_, ()>(&key)
            .await
            .map_err(|e| SessionError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn touch(&self, session_id: &str, ttl: Duration) -> SessionResult<bool> {
        let key = self.redis_key(session_id);
        let mut conn = self.connection.clone();
        let ttl_secs = ttl.as_secs().max(1) as i64;
        // EXPIRE returns 1 if the key exists and TTL was set, 0 otherwise.
        let updated: i64 = conn
            .expire(&key, ttl_secs)
            .await
            .map_err(|e| SessionError::Backend(e.to_string()))?;
        Ok(updated == 1)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper: create a store pre-populated with one session.
    async fn store_with_entry(
        session_id: &str,
        user_id: &str,
        ttl: Duration,
    ) -> InMemorySessionStore {
        let store = InMemorySessionStore::new();
        let data = SessionData {
            user_id: user_id.to_owned(),
            extra: json!({ "role": "admin" }),
            created_at: Utc::now(),
            expires_at: None,
        };
        store.put(session_id, data, ttl).await.unwrap();
        store
    }

    // ── serialization round-trip ───────────────────────────────────────────

    #[test]
    fn test_session_data_serde_roundtrip() {
        let original = SessionData {
            user_id: "user-42".to_owned(),
            extra: json!({ "plan": "pro", "seats": 5 }),
            created_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::seconds(3600)),
        };

        let serialized = serde_json::to_string(&original).expect("serialization must succeed");
        let deserialized: SessionData =
            serde_json::from_str(&serialized).expect("deserialization must succeed");

        assert_eq!(deserialized.user_id, original.user_id);
        assert_eq!(deserialized.extra, original.extra);
        // DateTime round-trips through JSON (RFC 3339 strings).
        assert_eq!(
            deserialized.created_at.timestamp(),
            original.created_at.timestamp()
        );
    }

    // ── put / get round-trip ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_put_get_roundtrip() {
        let store = InMemorySessionStore::new();
        let id = "sess-001";
        let data = SessionData {
            user_id: "alice".to_owned(),
            extra: json!({ "theme": "dark" }),
            created_at: Utc::now(),
            expires_at: None,
        };

        store
            .put(id, data.clone(), Duration::from_secs(60))
            .await
            .expect("put must succeed");

        let retrieved = store
            .get(id)
            .await
            .expect("get must succeed")
            .expect("session must exist");

        assert_eq!(retrieved.user_id, data.user_id);
        assert_eq!(retrieved.extra, data.extra);
    }

    // ── expiry ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_get_expired_returns_none() {
        let store = InMemorySessionStore::new();
        let id = "sess-expired";
        let data = SessionData::new("bob");

        // Store with a 10ms TTL, then wait 20ms.
        store
            .put(id, data, Duration::from_millis(10))
            .await
            .expect("put must succeed");

        tokio::time::sleep(Duration::from_millis(30)).await;

        let result = store
            .get(id)
            .await
            .expect("get must not return an error on an expired session");
        assert!(result.is_none(), "expired session must return None");
    }

    // ── delete ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_delete_then_get_none() {
        let store = store_with_entry("sess-del", "charlie", Duration::from_secs(60)).await;

        store.delete("sess-del").await.expect("delete must succeed");

        let result = store
            .get("sess-del")
            .await
            .expect("get after delete must not error");
        assert!(result.is_none(), "deleted session must not be retrievable");
    }

    // ── touch ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_touch_extends_ttl() {
        let store = InMemorySessionStore::new();
        let id = "sess-touch";
        let data = SessionData::new("dave");

        // Store with 50ms TTL.
        store
            .put(id, data, Duration::from_millis(50))
            .await
            .expect("put must succeed");

        // Touch with a fresh 1-second TTL before it expires.
        let found = store
            .touch(id, Duration::from_secs(1))
            .await
            .expect("touch must succeed");
        assert!(found, "touch must return true for an existing session");

        // After the original 50ms window the session must still be alive because
        // of the 1-second extension applied above.
        tokio::time::sleep(Duration::from_millis(80)).await;

        let result = store.get(id).await.expect("get must succeed");
        assert!(
            result.is_some(),
            "session must still be alive after TTL was extended"
        );
    }

    #[tokio::test]
    async fn test_in_memory_touch_missing_returns_false() {
        let store = InMemorySessionStore::new();

        let found = store
            .touch("nonexistent-id", Duration::from_secs(60))
            .await
            .expect("touch on missing key must not error");
        assert!(
            !found,
            "touch must return false when the session does not exist"
        );
    }

    // ── delete idempotency ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_delete_nonexistent_is_noop() {
        let store = InMemorySessionStore::new();
        // Must not panic or error.
        store
            .delete("never-existed")
            .await
            .expect("delete of non-existent key must succeed silently");
    }
}
