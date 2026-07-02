//! Redis-based L2 Caching Layer
//!
//! Provides distributed caching using Redis for multi-instance deployments.
//!
//! ## Architecture
//!
//! The two-level cache architecture provides:
//! - **L1 (In-Memory)**: Fast local cache with LRU eviction
//! - **L2 (Redis)**: Distributed cache shared across all instances
//!
//! ## Cache Flow
//!
//! ```text
//! GET Request:
//!   1. Check L1 cache (local memory) - ~1μs
//!   2. If L1 miss, check L2 cache (Redis) - ~1ms
//!   3. If L2 hit, populate L1 and return
//!   4. If L2 miss, fetch from database - ~10ms
//!   5. Populate both L1 and L2
//!
//! SET Request:
//!   1. Write to L1 cache
//!   2. Write to L2 cache (async)
//!
//! DELETE Request:
//!   1. Invalidate L1 cache
//!   2. Invalidate L2 cache
//! ```
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{Cache, RedisCache, TwoLevelCache, RedisCacheConfig};
//!
//! let l1_cache = Cache::new(cache_config);
//! let redis_config = RedisCacheConfig {
//!     redis_url: "redis://localhost:6379".to_string(),
//!     key_prefix: "oxify:".to_string(),
//!     default_ttl: std::time::Duration::from_secs(300),
//!     ..Default::default()
//! };
//! let l2_cache = RedisCache::new(redis_config).await?;
//!
//! let two_level = TwoLevelCache::new(l1_cache, l2_cache);
//!
//! // Use two-level cache
//! if let Some(workflow) = two_level.get_workflow(&workflow_id).await? {
//!     return Ok(workflow);
//! }
//! ```

#[cfg(feature = "redis-cache")]
use redis::{aio::ConnectionManager, AsyncCommands, RedisError};

use crate::models::WorkflowRow;
use crate::quota_store::{UserQuota, WorkflowQuota};
use crate::Cache;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Redis cache configuration
#[derive(Debug, Clone)]
pub struct RedisCacheConfig {
    /// Redis connection URL (e.g., "redis://localhost:6379")
    pub redis_url: String,
    /// Key prefix for all cache entries
    pub key_prefix: String,
    /// Default TTL for cache entries
    pub default_ttl: Duration,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Maximum number of connection retries
    pub max_retries: u32,
    /// Enable compression for large values (>1KB)
    pub enable_compression: bool,
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            key_prefix: "oxify:".to_string(),
            default_ttl: Duration::from_secs(300), // 5 minutes
            connection_timeout: Duration::from_secs(5),
            max_retries: 3,
            enable_compression: true,
        }
    }
}

#[cfg(feature = "redis-cache")]
/// Redis-backed cache layer (L2)
pub struct RedisCache {
    connection: ConnectionManager,
    config: RedisCacheConfig,
}

#[cfg(feature = "redis-cache")]
impl RedisCache {
    /// Create a new Redis cache
    pub async fn new(config: RedisCacheConfig) -> Result<Self, RedisError> {
        let client = redis::Client::open(config.redis_url.as_str())?;
        let connection = ConnectionManager::new(client).await?;

        Ok(Self { connection, config })
    }

    /// Build cache key with prefix
    fn key(&self, suffix: &str) -> String {
        format!("{}{}", self.config.key_prefix, suffix)
    }

    /// Get value from Redis
    async fn get_value<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>, RedisError> {
        let full_key = self.key(key);
        let data: Option<Vec<u8>> = self.connection.get(&full_key).await?;

        match data {
            Some(bytes) => {
                let value: T = serde_json::from_slice(&bytes).map_err(|e| {
                    RedisError::from(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Deserialization failed: {e}"),
                    ))
                })?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Set value in Redis with TTL
    async fn set_value<T: Serialize>(
        &mut self,
        key: &str,
        value: &T,
        ttl: Option<Duration>,
    ) -> Result<(), RedisError> {
        let full_key = self.key(key);
        let bytes = serde_json::to_vec(value).map_err(|e| {
            RedisError::from(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Serialization failed: {e}"),
            ))
        })?;

        let ttl_secs = ttl.unwrap_or(self.config.default_ttl).as_secs();

        self.connection
            .set_ex::<_, _, ()>(&full_key, bytes, ttl_secs)
            .await?;
        Ok(())
    }

    /// Delete value from Redis
    async fn delete_value(&mut self, key: &str) -> Result<(), RedisError> {
        let full_key = self.key(key);
        self.connection.del::<_, ()>(&full_key).await?;
        Ok(())
    }

    /// Clear all cache entries with the configured prefix
    pub async fn clear_all(&mut self) -> Result<(), RedisError> {
        let pattern = format!("{}*", self.config.key_prefix);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut self.connection)
            .await?;

        if !keys.is_empty() {
            self.connection.del::<_, ()>(keys).await?;
        }

        Ok(())
    }

    /// Get workflow from Redis
    pub async fn get_workflow(&mut self, id: &Uuid) -> Result<Option<WorkflowRow>, RedisError> {
        self.get_value(&format!("workflow:{id}")).await
    }

    /// Set workflow in Redis
    pub async fn set_workflow(
        &mut self,
        id: &Uuid,
        workflow: &WorkflowRow,
        ttl: Option<Duration>,
    ) -> Result<(), RedisError> {
        self.set_value(&format!("workflow:{id}"), workflow, ttl)
            .await
    }

    /// Delete workflow from Redis
    pub async fn delete_workflow(&mut self, id: &Uuid) -> Result<(), RedisError> {
        self.delete_value(&format!("workflow:{id}")).await
    }

    /// Get user quota from Redis
    pub async fn get_user_quota(
        &mut self,
        user_id: &Uuid,
    ) -> Result<Option<UserQuota>, RedisError> {
        self.get_value(&format!("user_quota:{user_id}")).await
    }

    /// Set user quota in Redis
    pub async fn set_user_quota(
        &mut self,
        user_id: &Uuid,
        quota: &UserQuota,
        ttl: Option<Duration>,
    ) -> Result<(), RedisError> {
        self.set_value(&format!("user_quota:{user_id}"), quota, ttl)
            .await
    }

    /// Delete user quota from Redis
    pub async fn delete_user_quota(&mut self, user_id: &Uuid) -> Result<(), RedisError> {
        self.delete_value(&format!("user_quota:{user_id}")).await
    }

    /// Get workflow quota from Redis
    pub async fn get_workflow_quota(
        &mut self,
        workflow_id: &Uuid,
    ) -> Result<Option<WorkflowQuota>, RedisError> {
        self.get_value(&format!("workflow_quota:{workflow_id}"))
            .await
    }

    /// Set workflow quota in Redis
    pub async fn set_workflow_quota(
        &mut self,
        workflow_id: &Uuid,
        quota: &WorkflowQuota,
        ttl: Option<Duration>,
    ) -> Result<(), RedisError> {
        self.set_value(&format!("workflow_quota:{workflow_id}"), quota, ttl)
            .await
    }

    /// Delete workflow quota from Redis
    pub async fn delete_workflow_quota(&mut self, workflow_id: &Uuid) -> Result<(), RedisError> {
        self.delete_value(&format!("workflow_quota:{workflow_id}"))
            .await
    }

    /// Health check - ping Redis
    pub async fn ping(&mut self) -> Result<bool, RedisError> {
        let pong: String = redis::cmd("PING").query_async(&mut self.connection).await?;
        Ok(pong == "PONG")
    }
}

#[cfg(feature = "redis-cache")]
/// Two-level cache combining L1 (in-memory) and L2 (Redis)
pub struct TwoLevelCache {
    l1: Arc<Cache>,
    l2: Arc<tokio::sync::Mutex<RedisCache>>,
}

#[cfg(feature = "redis-cache")]
impl TwoLevelCache {
    /// Create a new two-level cache
    pub fn new(l1: Cache, l2: RedisCache) -> Self {
        Self {
            l1: Arc::new(l1),
            l2: Arc::new(tokio::sync::Mutex::new(l2)),
        }
    }

    /// Get workflow from cache (L1 -> L2 -> Database)
    pub async fn get_workflow(&self, id: &Uuid) -> Result<Option<WorkflowRow>, RedisError> {
        // Check L1 first
        if let Some(workflow) = self.l1.get_workflow(id) {
            return Ok(Some(workflow));
        }

        // Check L2
        let mut l2 = self.l2.lock().await;
        if let Some(workflow) = l2.get_workflow(id).await? {
            // Populate L1
            self.l1.put_workflow(*id, workflow.clone());
            return Ok(Some(workflow));
        }

        Ok(None)
    }

    /// Put workflow into cache (L1 + L2)
    pub async fn put_workflow(&self, id: Uuid, workflow: WorkflowRow) -> Result<(), RedisError> {
        // Write to L1
        self.l1.put_workflow(id, workflow.clone());

        // Write to L2 (async)
        let mut l2 = self.l2.lock().await;
        l2.set_workflow(&id, &workflow, None).await?;

        Ok(())
    }

    /// Invalidate workflow from cache (L1 + L2)
    pub async fn invalidate_workflow(&self, id: &Uuid) -> Result<(), RedisError> {
        // Invalidate L1
        self.l1.invalidate_workflow(id);

        // Invalidate L2
        let mut l2 = self.l2.lock().await;
        l2.delete_workflow(id).await?;

        Ok(())
    }

    /// Get user quota from cache (L1 -> L2)
    pub async fn get_user_quota(&self, user_id: &Uuid) -> Result<Option<UserQuota>, RedisError> {
        // Check L1 first
        if let Some(quota) = self.l1.get_user_quota(user_id) {
            return Ok(Some(quota));
        }

        // Check L2
        let mut l2 = self.l2.lock().await;
        if let Some(quota) = l2.get_user_quota(user_id).await? {
            // Populate L1
            self.l1.put_user_quota(*user_id, quota.clone());
            return Ok(Some(quota));
        }

        Ok(None)
    }

    /// Put user quota into cache (L1 + L2)
    pub async fn put_user_quota(&self, user_id: Uuid, quota: UserQuota) -> Result<(), RedisError> {
        // Write to L1
        self.l1.put_user_quota(user_id, quota.clone());

        // Write to L2 (async)
        let mut l2 = self.l2.lock().await;
        l2.set_user_quota(&user_id, &quota, None).await?;

        Ok(())
    }

    /// Invalidate user quota from cache (L1 + L2)
    pub async fn invalidate_user_quota(&self, user_id: &Uuid) -> Result<(), RedisError> {
        // Invalidate L1
        self.l1.invalidate_user_quota(user_id);

        // Invalidate L2
        let mut l2 = self.l2.lock().await;
        l2.delete_user_quota(user_id).await?;

        Ok(())
    }

    /// Get workflow quota from cache (L1 -> L2)
    pub async fn get_workflow_quota(
        &self,
        workflow_id: &Uuid,
    ) -> Result<Option<WorkflowQuota>, RedisError> {
        // Check L1 first
        if let Some(quota) = self.l1.get_workflow_quota(workflow_id) {
            return Ok(Some(quota));
        }

        // Check L2
        let mut l2 = self.l2.lock().await;
        if let Some(quota) = l2.get_workflow_quota(workflow_id).await? {
            // Populate L1
            self.l1.put_workflow_quota(*workflow_id, quota.clone());
            return Ok(Some(quota));
        }

        Ok(None)
    }

    /// Put workflow quota into cache (L1 + L2)
    pub async fn put_workflow_quota(
        &self,
        workflow_id: Uuid,
        quota: WorkflowQuota,
    ) -> Result<(), RedisError> {
        // Write to L1
        self.l1.put_workflow_quota(workflow_id, quota.clone());

        // Write to L2 (async)
        let mut l2 = self.l2.lock().await;
        l2.set_workflow_quota(&workflow_id, &quota, None).await?;

        Ok(())
    }

    /// Invalidate workflow quota from cache (L1 + L2)
    pub async fn invalidate_workflow_quota(&self, workflow_id: &Uuid) -> Result<(), RedisError> {
        // Invalidate L1
        self.l1.invalidate_workflow_quota(workflow_id);

        // Invalidate L2
        let mut l2 = self.l2.lock().await;
        l2.delete_workflow_quota(workflow_id).await?;

        Ok(())
    }

    /// Clear all caches (L1 + L2)
    pub async fn clear_all(&self) -> Result<(), RedisError> {
        // Clear L1
        self.l1.clear_all();

        // Clear L2
        let mut l2 = self.l2.lock().await;
        l2.clear_all().await?;

        Ok(())
    }

    /// Health check
    pub async fn health_check(&self) -> Result<bool, RedisError> {
        let mut l2 = self.l2.lock().await;
        l2.ping().await
    }
}

#[cfg(all(test, feature = "redis-cache"))]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Redis instance
    async fn test_redis_cache_basic_operations() {
        let config = RedisCacheConfig::default();
        let mut cache = RedisCache::new(config).await.unwrap();

        // Test ping
        assert!(cache.ping().await.unwrap());

        // Test workflow operations
        let workflow_id = Uuid::new_v4();
        let workflow = WorkflowRow {
            id: workflow_id.to_string(),
            name: "test".to_string(),
            description: None,
            definition: "{}".to_string(),
            version: 1,
            tags: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        // Set workflow
        cache
            .set_workflow(&workflow_id, &workflow, None)
            .await
            .unwrap();

        // Get workflow
        let cached = cache.get_workflow(&workflow_id).await.unwrap();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().id, workflow_id.to_string());

        // Delete workflow
        cache.delete_workflow(&workflow_id).await.unwrap();

        // Verify deletion
        let cached = cache.get_workflow(&workflow_id).await.unwrap();
        assert!(cached.is_none());
    }

    #[tokio::test]
    #[ignore] // Requires Redis instance
    async fn test_two_level_cache() {
        use crate::CacheConfig;

        let l1_config = CacheConfig {
            max_size: 10,
            default_ttl: Duration::from_secs(60),
            enable_metrics: true,
        };
        let l1 = Cache::new(l1_config);

        let l2_config = RedisCacheConfig::default();
        let l2 = RedisCache::new(l2_config).await.unwrap();

        let two_level = TwoLevelCache::new(l1, l2);

        let workflow_id = Uuid::new_v4();
        let workflow = WorkflowRow {
            id: workflow_id.to_string(),
            name: "test".to_string(),
            description: None,
            definition: "{}".to_string(),
            version: 1,
            tags: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        // Put into cache
        two_level
            .put_workflow(workflow_id, workflow.clone())
            .await
            .unwrap();

        // Get from cache (should be in L1)
        let cached = two_level.get_workflow(&workflow_id).await.unwrap();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().id, workflow_id.to_string());

        // Invalidate
        two_level.invalidate_workflow(&workflow_id).await.unwrap();

        // Verify invalidation
        let cached = two_level.get_workflow(&workflow_id).await.unwrap();
        assert!(cached.is_none());
    }
}
