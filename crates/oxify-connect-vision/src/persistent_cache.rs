//! Persistent caching backends for OCR results.
//!
//! This module provides persistent storage options for OCR results:
//! - Redis backend for distributed caching
//! - SQLite backend for local persistent cache
//! - Cache statistics and metrics
//! - Configurable eviction policies (LRU, LFU)

use crate::errors::Result;
use crate::types::OcrResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Cache eviction policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvictionPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// First In First Out
    Fifo,
}

/// Cache backend trait for persistent storage.
#[async_trait]
pub trait CacheBackend: Send + Sync {
    /// Get a cached result by key.
    async fn get(&self, key: &str) -> Result<Option<OcrResult>>;

    /// Store a result in cache.
    async fn put(&self, key: &str, value: &OcrResult, ttl: Option<Duration>) -> Result<()>;

    /// Check if a key exists in cache.
    async fn contains(&self, key: &str) -> Result<bool>;

    /// Remove a key from cache.
    async fn remove(&self, key: &str) -> Result<()>;

    /// Clear all cached entries.
    async fn clear(&self) -> Result<()>;

    /// Get cache statistics.
    async fn stats(&self) -> Result<CacheStats>;

    /// Get the number of entries in cache.
    async fn len(&self) -> Result<usize>;

    /// Check if cache is empty.
    async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total number of entries
    pub total_entries: usize,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Hit rate (0.0 to 1.0)
    pub hit_rate: f64,
    /// Total size in bytes (if available)
    pub size_bytes: Option<u64>,
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
}

impl CacheStats {
    /// Create new empty stats.
    pub fn new(eviction_policy: EvictionPolicy) -> Self {
        Self {
            total_entries: 0,
            hits: 0,
            misses: 0,
            hit_rate: 0.0,
            size_bytes: None,
            eviction_policy,
        }
    }

    /// Calculate hit rate.
    pub fn calculate_hit_rate(&mut self) {
        let total = self.hits + self.misses;
        self.hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
    }
}

/// Redis cache backend configuration.
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,
    /// Key prefix for cache entries
    pub key_prefix: String,
    /// Default TTL for entries
    pub default_ttl: Duration,
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            key_prefix: "oxify:vision:".to_string(),
            default_ttl: Duration::from_secs(3600),
            eviction_policy: EvictionPolicy::Lru,
        }
    }
}

impl RedisConfig {
    /// Create a new Redis configuration.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    /// Set key prefix.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = prefix.into();
        self
    }

    /// Set default TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Set eviction policy.
    pub fn with_eviction_policy(mut self, policy: EvictionPolicy) -> Self {
        self.eviction_policy = policy;
        self
    }
}

/// Redis cache backend (stub - full implementation requires redis crate).
pub struct RedisBackend {
    _config: RedisConfig,
}

impl RedisBackend {
    /// Create a new Redis backend.
    pub async fn new(_config: RedisConfig) -> Result<Self> {
        Err(crate::errors::VisionError::config(
            "Redis backend not yet implemented - requires redis crate integration",
        ))
    }
}

#[async_trait]
impl CacheBackend for RedisBackend {
    async fn get(&self, _key: &str) -> Result<Option<OcrResult>> {
        Err(crate::errors::VisionError::config("Redis not available"))
    }

    async fn put(&self, _key: &str, _value: &OcrResult, _ttl: Option<Duration>) -> Result<()> {
        Err(crate::errors::VisionError::config("Redis not available"))
    }

    async fn contains(&self, _key: &str) -> Result<bool> {
        Err(crate::errors::VisionError::config("Redis not available"))
    }

    async fn remove(&self, _key: &str) -> Result<()> {
        Err(crate::errors::VisionError::config("Redis not available"))
    }

    async fn clear(&self) -> Result<()> {
        Err(crate::errors::VisionError::config("Redis not available"))
    }

    async fn stats(&self) -> Result<CacheStats> {
        Err(crate::errors::VisionError::config("Redis not available"))
    }

    async fn len(&self) -> Result<usize> {
        Err(crate::errors::VisionError::config("Redis not available"))
    }
}

/// SQLite cache backend configuration.
#[derive(Debug, Clone)]
pub struct SqliteConfig {
    /// Database file path
    pub db_path: PathBuf,
    /// Maximum number of entries
    pub max_entries: usize,
    /// Default TTL for entries
    pub default_ttl: Duration,
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        let cache_dir = crate::downloader::default_cache_dir();
        Self {
            db_path: cache_dir.join("cache.db"),
            max_entries: 10000,
            default_ttl: Duration::from_secs(86400), // 24 hours
            eviction_policy: EvictionPolicy::Lru,
        }
    }
}

impl SqliteConfig {
    /// Create a new SQLite configuration.
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            ..Default::default()
        }
    }

    /// Set maximum entries.
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Set default TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Set eviction policy.
    pub fn with_eviction_policy(mut self, policy: EvictionPolicy) -> Self {
        self.eviction_policy = policy;
        self
    }
}

/// SQLite cache backend (stub - full implementation requires rusqlite crate).
pub struct SqliteBackend {
    _config: SqliteConfig,
}

impl SqliteBackend {
    /// Create a new SQLite backend.
    pub async fn new(_config: SqliteConfig) -> Result<Self> {
        Err(crate::errors::VisionError::config(
            "SQLite backend not yet implemented - requires rusqlite crate integration",
        ))
    }
}

#[async_trait]
impl CacheBackend for SqliteBackend {
    async fn get(&self, _key: &str) -> Result<Option<OcrResult>> {
        Err(crate::errors::VisionError::config("SQLite not available"))
    }

    async fn put(&self, _key: &str, _value: &OcrResult, _ttl: Option<Duration>) -> Result<()> {
        Err(crate::errors::VisionError::config("SQLite not available"))
    }

    async fn contains(&self, _key: &str) -> Result<bool> {
        Err(crate::errors::VisionError::config("SQLite not available"))
    }

    async fn remove(&self, _key: &str) -> Result<()> {
        Err(crate::errors::VisionError::config("SQLite not available"))
    }

    async fn clear(&self) -> Result<()> {
        Err(crate::errors::VisionError::config("SQLite not available"))
    }

    async fn stats(&self) -> Result<CacheStats> {
        Err(crate::errors::VisionError::config("SQLite not available"))
    }

    async fn len(&self) -> Result<usize> {
        Err(crate::errors::VisionError::config("SQLite not available"))
    }
}

/// Persistent cache wrapper that uses a backend.
pub struct PersistentCache {
    backend: Box<dyn CacheBackend>,
    default_ttl: Duration,
}

impl PersistentCache {
    /// Create a cache with a custom backend.
    pub fn new(backend: Box<dyn CacheBackend>, default_ttl: Duration) -> Self {
        Self {
            backend,
            default_ttl,
        }
    }

    /// Create a Redis-backed cache.
    pub async fn redis(config: RedisConfig) -> Result<Self> {
        let ttl = config.default_ttl;
        let backend = RedisBackend::new(config).await?;
        Ok(Self::new(Box::new(backend), ttl))
    }

    /// Create a SQLite-backed cache.
    pub async fn sqlite(config: SqliteConfig) -> Result<Self> {
        let ttl = config.default_ttl;
        let backend = SqliteBackend::new(config).await?;
        Ok(Self::new(Box::new(backend), ttl))
    }

    /// Get a cached result.
    pub async fn get(&self, key: &str) -> Result<Option<OcrResult>> {
        self.backend.get(key).await
    }

    /// Store a result in cache.
    pub async fn put(&self, key: &str, value: &OcrResult) -> Result<()> {
        self.backend.put(key, value, Some(self.default_ttl)).await
    }

    /// Store a result with custom TTL.
    pub async fn put_with_ttl(&self, key: &str, value: &OcrResult, ttl: Duration) -> Result<()> {
        self.backend.put(key, value, Some(ttl)).await
    }

    /// Check if a key exists.
    pub async fn contains(&self, key: &str) -> Result<bool> {
        self.backend.contains(key).await
    }

    /// Remove a key.
    pub async fn remove(&self, key: &str) -> Result<()> {
        self.backend.remove(key).await
    }

    /// Clear all entries.
    pub async fn clear(&self) -> Result<()> {
        self.backend.clear().await
    }

    /// Get cache statistics.
    pub async fn stats(&self) -> Result<CacheStats> {
        self.backend.stats().await
    }

    /// Get number of entries.
    pub async fn len(&self) -> Result<usize> {
        self.backend.len().await
    }

    /// Check if cache is empty.
    pub async fn is_empty(&self) -> Result<bool> {
        self.backend.is_empty().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eviction_policy() {
        let policies = vec![
            EvictionPolicy::Lru,
            EvictionPolicy::Lfu,
            EvictionPolicy::Fifo,
        ];

        for policy in policies {
            let _ = format!("{:?}", policy);
        }
    }

    #[test]
    fn test_cache_stats() {
        let mut stats = CacheStats::new(EvictionPolicy::Lru);
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate, 0.0);

        stats.hits = 80;
        stats.misses = 20;
        stats.calculate_hit_rate();
        assert!((stats.hit_rate - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_redis_config() {
        let config = RedisConfig::new("redis://localhost:6379")
            .with_prefix("test:")
            .with_ttl(Duration::from_secs(1800))
            .with_eviction_policy(EvictionPolicy::Lfu);

        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.key_prefix, "test:");
        assert_eq!(config.default_ttl, Duration::from_secs(1800));
        assert_eq!(config.eviction_policy, EvictionPolicy::Lfu);
    }

    #[test]
    fn test_sqlite_config() {
        let config = SqliteConfig::new(PathBuf::from("/tmp/test.db"))
            .with_max_entries(5000)
            .with_ttl(Duration::from_secs(7200))
            .with_eviction_policy(EvictionPolicy::Fifo);

        assert_eq!(config.db_path, PathBuf::from("/tmp/test.db"));
        assert_eq!(config.max_entries, 5000);
        assert_eq!(config.default_ttl, Duration::from_secs(7200));
        assert_eq!(config.eviction_policy, EvictionPolicy::Fifo);
    }

    #[test]
    fn test_redis_config_default() {
        let config = RedisConfig::default();
        assert!(config.url.contains("redis://"));
        assert!(!config.key_prefix.is_empty());
    }

    #[test]
    fn test_sqlite_config_default() {
        let config = SqliteConfig::default();
        assert!(config.db_path.to_string_lossy().contains("cache.db"));
        assert!(config.max_entries > 0);
    }

    #[tokio::test]
    async fn test_redis_backend_stub() {
        let config = RedisConfig::default();
        let result = RedisBackend::new(config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sqlite_backend_stub() {
        let config = SqliteConfig::default();
        let result = SqliteBackend::new(config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_persistent_cache_redis_stub() {
        let config = RedisConfig::default();
        let result = PersistentCache::redis(config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_persistent_cache_sqlite_stub() {
        let config = SqliteConfig::default();
        let result = PersistentCache::sqlite(config).await;
        assert!(result.is_err());
    }
}
