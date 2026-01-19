//! Redis-based L2 cache for distributed authorization
//!
//! Provides shared caching across multiple API servers with:
//! - Distributed cache invalidation
//! - TTL-based expiration
//! - Pub/sub for cache updates
//! - High availability support

use crate::{RelationTuple, Subject};
use deadpool_redis::{Config, Pool, Runtime};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum RedisCacheError {
    #[error("Redis connection error: {0}")]
    ConnectionError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Cache operation failed: {0}")]
    OperationError(String),
}

pub type Result<T> = std::result::Result<T, RedisCacheError>;

/// Redis cache configuration
#[derive(Debug, Clone)]
pub struct RedisCacheConfig {
    /// Redis connection URL (e.g., "redis://localhost:6379")
    pub url: String,

    /// Default TTL for cached entries (default: 300 seconds)
    pub default_ttl: Duration,

    /// Key prefix for cache entries
    pub key_prefix: String,

    /// Enable pub/sub for cache invalidation
    pub enable_pubsub: bool,

    /// Pub/sub channel name for invalidation events
    pub invalidation_channel: String,

    /// Connection pool size
    pub pool_size: u32,
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            default_ttl: Duration::from_secs(300),
            key_prefix: "authz:".to_string(),
            enable_pubsub: true,
            invalidation_channel: "authz:invalidate".to_string(),
            pool_size: 10,
        }
    }
}

impl RedisCacheConfig {
    pub fn with_url(mut self, url: String) -> Self {
        self.url = url;
        self
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    pub fn with_key_prefix(mut self, prefix: String) -> Self {
        self.key_prefix = prefix;
        self
    }
}

/// Cached permission check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPermission {
    pub allowed: bool,
    pub cached_at: i64, // Unix timestamp
}

/// Cache key for permission checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCacheKey {
    pub resource_type: String,
    pub resource_id: String,
    pub relation: String,
    pub subject: Subject,
}

impl PermissionCacheKey {
    pub fn new(
        resource_type: String,
        resource_id: String,
        relation: String,
        subject: Subject,
    ) -> Self {
        Self {
            resource_type,
            resource_id,
            relation,
            subject,
        }
    }

    /// Generate Redis key string
    pub fn to_redis_key(&self, prefix: &str) -> String {
        format!(
            "{}check:{}:{}:{}:{}",
            prefix,
            self.resource_type,
            self.resource_id,
            self.relation,
            subject_to_key(&self.subject)
        )
    }
}

fn subject_to_key(subject: &Subject) -> String {
    match subject {
        Subject::User(id) => format!("user:{}", id),
        Subject::UserSet {
            namespace,
            object_id,
            relation,
        } => {
            format!("userset:{}:{}:{}", namespace, object_id, relation)
        }
    }
}

/// Redis cache for authorization (L2 cache)
pub struct RedisCache {
    config: RedisCacheConfig,
    pool: Pool,
    stats: Arc<RwLock<RedisCacheStatsInternal>>,
}

/// Internal statistics tracking with atomic operations
#[derive(Debug, Default)]
struct RedisCacheStatsInternal {
    hit_count: u64,
    miss_count: u64,
}

impl RedisCache {
    /// Create a new Redis cache instance
    pub fn new(config: RedisCacheConfig) -> Result<Self> {
        let redis_config = Config::from_url(&config.url);
        let pool = redis_config
            .create_pool(Some(Runtime::Tokio1))
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        Ok(Self {
            config,
            pool,
            stats: Arc::new(RwLock::new(RedisCacheStatsInternal::default())),
        })
    }

    /// Connect to Redis server (validate connection)
    pub async fn connect(&mut self) -> Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        // Verify connection with PING
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        Ok(())
    }

    /// Get cached permission result
    pub async fn get_permission(&self, key: &PermissionCacheKey) -> Result<Option<bool>> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        let redis_key = key.to_redis_key(&self.config.key_prefix);

        let result: Option<String> = conn
            .get(&redis_key)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        match result {
            Some(json_str) => {
                let cached: CachedPermission = serde_json::from_str(&json_str)
                    .map_err(|e| RedisCacheError::SerializationError(e.to_string()))?;

                // Update stats: cache hit
                let mut stats = self.stats.write().await;
                stats.hit_count += 1;

                Ok(Some(cached.allowed))
            }
            None => {
                // Update stats: cache miss
                let mut stats = self.stats.write().await;
                stats.miss_count += 1;

                Ok(None)
            }
        }
    }

    /// Set cached permission result
    pub async fn set_permission(
        &self,
        key: &PermissionCacheKey,
        allowed: bool,
        ttl: Option<Duration>,
    ) -> Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        let redis_key = key.to_redis_key(&self.config.key_prefix);
        let ttl_secs = ttl.unwrap_or(self.config.default_ttl).as_secs() as i64;

        let cached = CachedPermission {
            allowed,
            cached_at: chrono::Utc::now().timestamp(),
        };

        let json_str = serde_json::to_string(&cached)
            .map_err(|e| RedisCacheError::SerializationError(e.to_string()))?;

        // Set with TTL using SETEX
        let _: () = conn
            .set_ex(&redis_key, json_str, ttl_secs as u64)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        Ok(())
    }

    /// Invalidate cache entry
    pub async fn invalidate(&self, key: &PermissionCacheKey) -> Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        let redis_key = key.to_redis_key(&self.config.key_prefix);

        let _: () = conn
            .del(&redis_key)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        // Optionally publish invalidation event
        if self.config.enable_pubsub {
            let msg = serde_json::to_string(&key)
                .map_err(|e| RedisCacheError::SerializationError(e.to_string()))?;

            let _: () = conn
                .publish(&self.config.invalidation_channel, msg)
                .await
                .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;
        }

        Ok(())
    }

    /// Invalidate all cache entries for a subject
    pub async fn invalidate_subject(&self, subject: &Subject) -> Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        let subject_key = subject_to_key(subject);
        let pattern = format!("{}check:*:*:*:{}", self.config.key_prefix, subject_key);

        // Use SCAN to find all matching keys
        let keys: Vec<String> = redis::cmd("SCAN")
            .arg(0)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        // Delete all matching keys
        if !keys.is_empty() {
            let _: () = conn
                .del(&keys)
                .await
                .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;
        }

        Ok(())
    }

    /// Invalidate all cache entries for a resource
    pub async fn invalidate_resource(&self, namespace: &str, object_id: &str) -> Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        let pattern = format!(
            "{}check:{}:{}:*:*",
            self.config.key_prefix, namespace, object_id
        );

        // Use SCAN to find all matching keys
        let keys: Vec<String> = redis::cmd("SCAN")
            .arg(0)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        // Delete all matching keys
        if !keys.is_empty() {
            let _: () = conn
                .del(&keys)
                .await
                .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;
        }

        Ok(())
    }

    /// Invalidate all cache entries affected by a tuple write
    pub async fn invalidate_tuple(&self, tuple: &RelationTuple) -> Result<()> {
        // Invalidate all permissions for the resource
        self.invalidate_resource(&tuple.namespace, &tuple.object_id)
            .await?;

        // Invalidate all permissions for the subject
        self.invalidate_subject(&tuple.subject).await?;

        Ok(())
    }

    /// Clear all cache entries (use with caution)
    pub async fn clear_all(&self) -> Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        let pattern = format!("{}*", self.config.key_prefix);

        // Use SCAN to find all matching keys
        let keys: Vec<String> = redis::cmd("SCAN")
            .arg(0)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(1000)
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        // Delete all matching keys
        if !keys.is_empty() {
            let _: () = conn
                .del(&keys)
                .await
                .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;
        }

        Ok(())
    }

    /// Get cache statistics
    pub async fn stats(&self) -> Result<RedisCacheStats> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        // Get number of keys with our prefix
        let pattern = format!("{}*", self.config.key_prefix);
        let keys: Vec<String> = redis::cmd("SCAN")
            .arg(0)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(1000)
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        let total_keys = keys.len();

        // Get hit/miss stats
        let stats = self.stats.read().await;
        let hit_count = stats.hit_count;
        let miss_count = stats.miss_count;
        let total_checks = hit_count + miss_count;
        let hit_rate = if total_checks > 0 {
            hit_count as f64 / total_checks as f64
        } else {
            0.0
        };

        // Get memory usage from Redis INFO
        let info: String = redis::cmd("INFO")
            .arg("memory")
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        // Parse memory usage from INFO output
        let memory_usage_bytes = info
            .lines()
            .find(|line| line.starts_with("used_memory:"))
            .and_then(|line| line.split(':').nth(1))
            .and_then(|val| val.trim().parse::<usize>().ok())
            .unwrap_or(0);

        Ok(RedisCacheStats {
            total_keys,
            hit_count,
            miss_count,
            hit_rate,
            memory_usage_bytes,
        })
    }

    /// Health check
    pub async fn health_check(&self) -> Result<bool> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| RedisCacheError::ConnectionError(e.to_string()))?;

        // PING Redis server
        let response: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisCacheError::OperationError(e.to_string()))?;

        Ok(response == "PONG")
    }
}

/// Redis cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisCacheStats {
    pub total_keys: usize,
    pub hit_count: u64,
    pub miss_count: u64,
    pub hit_rate: f64,
    pub memory_usage_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_redis_cache_creation() {
        let config = RedisCacheConfig::default();
        let cache = RedisCache::new(config);
        assert!(cache.is_ok());
    }

    #[tokio::test]
    async fn test_permission_cache_key() {
        let resource_type = "document".to_string();
        let resource_id = "123".to_string();
        let subject = Subject::User("alice".to_string());

        let key =
            PermissionCacheKey::new(resource_type, resource_id, "viewer".to_string(), subject);

        let redis_key = key.to_redis_key("authz:");
        assert!(redis_key.contains("document"));
        assert!(redis_key.contains("123"));
        assert!(redis_key.contains("viewer"));
    }

    #[tokio::test]
    #[ignore = "Requires Redis server running at localhost:6379"]
    async fn test_redis_cache_integration() {
        let config = RedisCacheConfig::default();
        let mut cache = RedisCache::new(config).unwrap();

        // Try to connect, skip test if Redis is not available
        if cache.connect().await.is_err() {
            return;
        }

        let resource_type = "document".to_string();
        let resource_id = "123".to_string();
        let subject = Subject::User("alice".to_string());
        let key =
            PermissionCacheKey::new(resource_type, resource_id, "viewer".to_string(), subject);

        // Test set
        assert!(cache.set_permission(&key, true, None).await.is_ok());

        // Test get (should find the cached value)
        let result = cache.get_permission(&key).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(true));

        // Test invalidate
        assert!(cache.invalidate(&key).await.is_ok());

        // Test get after invalidate (should be None)
        let result = cache.get_permission(&key).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Test stats
        let stats = cache.stats().await;
        assert!(stats.is_ok());
    }
}
