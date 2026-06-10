//! In-Memory Caching Layer
//!
//! Provides high-performance in-memory caching for frequently accessed data.
//!
//! ## Overview
//!
//! The cache layer reduces database load by storing frequently accessed items in memory:
//! - **Workflow definitions** - Most frequently accessed, rarely change
//! - **User quotas** - Hot path for execution checks
//! - **API keys** - Validated on every request
//! - **Secrets** - Decrypted values cached after first access
//!
//! ## Cache Strategy
//!
//! - **LRU Eviction**: Least recently used items evicted when capacity reached
//! - **TTL Support**: Time-based expiration for cache entries
//! - **Automatic Invalidation**: Write-through invalidation on updates
//! - **Warm-up**: Pre-populate cache with critical data on startup
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{Cache, CacheConfig};
//! use std::time::Duration;
//!
//! let config = CacheConfig {
//!     max_size: 1000,
//!     default_ttl: Duration::from_secs(300), // 5 minutes
//! };
//! let cache = Cache::new(config);
//!
//! // Store workflow in cache
//! cache.put_workflow(workflow_id, workflow.clone());
//!
//! // Retrieve from cache
//! if let Some(workflow) = cache.get_workflow(&workflow_id) {
//!     return Ok(workflow);
//! }
//!
//! // Cache miss - fetch from database
//! let workflow = fetch_from_db(&workflow_id).await?;
//! cache.put_workflow(workflow_id, workflow.clone());
//! ```

use crate::models::WorkflowRow;
use chrono::{DateTime, Duration, Utc};

// Stub types for disabled quota_store module
// TODO: Re-enable when quota_store is migrated to SQLite
#[derive(Debug, Clone)]
pub struct UserQuota {
    pub user_id: uuid::Uuid,
    pub max_executions: i64,
}

#[derive(Debug, Clone)]
pub struct WorkflowQuota {
    pub workflow_id: uuid::Uuid,
    pub max_executions: i64,
}
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries per cache type
    pub max_size: usize,
    /// Default TTL for cache entries
    pub default_ttl: std::time::Duration,
    /// Enable cache metrics collection
    pub enable_metrics: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            default_ttl: std::time::Duration::from_secs(300), // 5 minutes
            enable_metrics: true,
        }
    }
}

/// Cache entry with expiration
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    value: T,
    expires_at: DateTime<Utc>,
    access_count: u64,
    last_accessed: DateTime<Utc>,
    seq: u64,
}

impl<T: Clone> CacheEntry<T> {
    fn new(value: T, ttl: std::time::Duration, seq: u64) -> Self {
        let now = Utc::now();
        let ttl_duration = Duration::from_std(ttl).unwrap_or(Duration::seconds(300));
        Self {
            value,
            expires_at: now + ttl_duration,
            access_count: 0,
            last_accessed: now,
            seq,
        }
    }

    fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    fn access(&mut self, new_seq: u64) -> T {
        self.access_count += 1;
        self.last_accessed = Utc::now();
        self.seq = new_seq;
        self.value.clone()
    }
}

/// LRU cache implementation
struct LruCache<K: std::hash::Hash + Eq, V: Clone> {
    entries: HashMap<K, CacheEntry<V>>,
    max_size: usize,
    next_seq: u64,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> LruCache<K, V> {
    fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(max_size),
            max_size,
            next_seq: 0,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        if let Some(entry) = self.entries.get_mut(key) {
            if entry.is_expired() {
                self.entries.remove(key);
                return None;
            }
            let seq = self.next_seq;
            self.next_seq += 1;
            Some(entry.access(seq))
        } else {
            None
        }
    }

    fn put(&mut self, key: K, value: V, ttl: std::time::Duration) {
        // Evict expired entries first
        self.evict_expired();

        // If at capacity, evict LRU entry
        if self.entries.len() >= self.max_size {
            self.evict_lru();
        }

        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.insert(key, CacheEntry::new(value, ttl, seq));
    }

    fn invalidate(&mut self, key: &K) {
        self.entries.remove(key);
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    #[allow(dead_code)]
    fn size(&self) -> usize {
        self.entries.len()
    }

    fn evict_expired(&mut self) {
        let now = Utc::now();
        self.entries.retain(|_, entry| entry.expires_at > now);
    }

    fn evict_lru(&mut self) {
        if let Some(lru_key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.seq)
            .map(|(key, _)| key.clone())
        {
            self.entries.remove(&lru_key);
        }
    }

    fn stats(&self) -> CacheStats {
        let now = Utc::now();
        let valid_entries = self.entries.values().filter(|e| e.expires_at > now).count();
        let total_accesses = self.entries.values().map(|e| e.access_count).sum::<u64>();

        CacheStats {
            size: self.entries.len(),
            valid_entries,
            expired_entries: self.entries.len() - valid_entries,
            total_accesses,
            capacity: self.max_size,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub size: usize,
    pub valid_entries: usize,
    pub expired_entries: usize,
    pub total_accesses: u64,
    pub capacity: usize,
}

impl CacheStats {
    pub fn utilization(&self) -> f64 {
        if self.capacity == 0 {
            return 0.0;
        }
        self.size as f64 / self.capacity as f64
    }

    pub fn hit_rate(&self, hits: u64, misses: u64) -> f64 {
        let total = hits + misses;
        if total == 0 {
            return 0.0;
        }
        hits as f64 / total as f64
    }
}

/// Cache metrics for monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheMetrics {
    pub workflow_hits: u64,
    pub workflow_misses: u64,
    pub user_quota_hits: u64,
    pub user_quota_misses: u64,
    pub workflow_quota_hits: u64,
    pub workflow_quota_misses: u64,
    pub api_key_hits: u64,
    pub api_key_misses: u64,
    pub evictions: u64,
    pub invalidations: u64,
}

impl CacheMetrics {
    pub fn workflow_hit_rate(&self) -> f64 {
        let total = self.workflow_hits + self.workflow_misses;
        if total == 0 {
            return 0.0;
        }
        self.workflow_hits as f64 / total as f64
    }

    pub fn user_quota_hit_rate(&self) -> f64 {
        let total = self.user_quota_hits + self.user_quota_misses;
        if total == 0 {
            return 0.0;
        }
        self.user_quota_hits as f64 / total as f64
    }

    pub fn overall_hit_rate(&self) -> f64 {
        let total_hits = self.workflow_hits
            + self.user_quota_hits
            + self.workflow_quota_hits
            + self.api_key_hits;
        let total_misses = self.workflow_misses
            + self.user_quota_misses
            + self.workflow_quota_misses
            + self.api_key_misses;
        let total = total_hits + total_misses;
        if total == 0 {
            return 0.0;
        }
        total_hits as f64 / total as f64
    }
}

/// Multi-level cache for different data types
pub struct Cache {
    workflows: Arc<RwLock<LruCache<Uuid, WorkflowRow>>>,
    user_quotas: Arc<RwLock<LruCache<Uuid, UserQuota>>>,
    workflow_quotas: Arc<RwLock<LruCache<Uuid, WorkflowQuota>>>,
    api_keys: Arc<RwLock<LruCache<String, Vec<u8>>>>, // Cached encrypted API keys
    config: CacheConfig,
    metrics: Arc<RwLock<CacheMetrics>>,
}

impl Cache {
    /// Create a new cache with the given configuration
    pub fn new(config: CacheConfig) -> Self {
        Self {
            workflows: Arc::new(RwLock::new(LruCache::new(config.max_size))),
            user_quotas: Arc::new(RwLock::new(LruCache::new(config.max_size))),
            workflow_quotas: Arc::new(RwLock::new(LruCache::new(config.max_size))),
            api_keys: Arc::new(RwLock::new(LruCache::new(config.max_size))),
            config,
            metrics: Arc::new(RwLock::new(CacheMetrics::default())),
        }
    }

    // ==================== Workflow Cache ====================

    /// Get workflow from cache
    pub fn get_workflow(&self, id: &Uuid) -> Option<WorkflowRow> {
        let mut cache = self.workflows.write().unwrap_or_else(|e| e.into_inner());
        let result = cache.get(id);

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            if result.is_some() {
                metrics.workflow_hits += 1;
            } else {
                metrics.workflow_misses += 1;
            }
        }

        result
    }

    /// Put workflow into cache
    pub fn put_workflow(&self, id: Uuid, workflow: WorkflowRow) {
        let mut cache = self.workflows.write().unwrap_or_else(|e| e.into_inner());
        cache.put(id, workflow, self.config.default_ttl);
    }

    /// Invalidate workflow cache entry
    pub fn invalidate_workflow(&self, id: &Uuid) {
        let mut cache = self.workflows.write().unwrap_or_else(|e| e.into_inner());
        cache.invalidate(id);

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            metrics.invalidations += 1;
        }
    }

    // ==================== User Quota Cache ====================

    /// Get user quota from cache
    pub fn get_user_quota(&self, user_id: &Uuid) -> Option<UserQuota> {
        let mut cache = self.user_quotas.write().unwrap_or_else(|e| e.into_inner());
        let result = cache.get(user_id);

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            if result.is_some() {
                metrics.user_quota_hits += 1;
            } else {
                metrics.user_quota_misses += 1;
            }
        }

        result
    }

    /// Put user quota into cache
    pub fn put_user_quota(&self, user_id: Uuid, quota: UserQuota) {
        let mut cache = self.user_quotas.write().unwrap_or_else(|e| e.into_inner());
        cache.put(user_id, quota, self.config.default_ttl);
    }

    /// Invalidate user quota cache entry
    pub fn invalidate_user_quota(&self, user_id: &Uuid) {
        let mut cache = self.user_quotas.write().unwrap_or_else(|e| e.into_inner());
        cache.invalidate(user_id);

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            metrics.invalidations += 1;
        }
    }

    // ==================== Workflow Quota Cache ====================

    /// Get workflow quota from cache
    pub fn get_workflow_quota(&self, workflow_id: &Uuid) -> Option<WorkflowQuota> {
        let mut cache = self
            .workflow_quotas
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let result = cache.get(workflow_id);

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            if result.is_some() {
                metrics.workflow_quota_hits += 1;
            } else {
                metrics.workflow_quota_misses += 1;
            }
        }

        result
    }

    /// Put workflow quota into cache
    pub fn put_workflow_quota(&self, workflow_id: Uuid, quota: WorkflowQuota) {
        let mut cache = self
            .workflow_quotas
            .write()
            .unwrap_or_else(|e| e.into_inner());
        cache.put(workflow_id, quota, self.config.default_ttl);
    }

    /// Invalidate workflow quota cache entry
    pub fn invalidate_workflow_quota(&self, workflow_id: &Uuid) {
        let mut cache = self
            .workflow_quotas
            .write()
            .unwrap_or_else(|e| e.into_inner());
        cache.invalidate(workflow_id);

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            metrics.invalidations += 1;
        }
    }

    // ==================== API Key Cache ====================

    /// Get API key from cache
    pub fn get_api_key(&self, key_hash: &str) -> Option<Vec<u8>> {
        let mut cache = self.api_keys.write().unwrap_or_else(|e| e.into_inner());
        let result = cache.get(&key_hash.to_string());

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            if result.is_some() {
                metrics.api_key_hits += 1;
            } else {
                metrics.api_key_misses += 1;
            }
        }

        result
    }

    /// Put API key into cache
    pub fn put_api_key(&self, key_hash: String, encrypted_key: Vec<u8>) {
        let mut cache = self.api_keys.write().unwrap_or_else(|e| e.into_inner());
        cache.put(key_hash, encrypted_key, self.config.default_ttl);
    }

    /// Invalidate API key cache entry
    pub fn invalidate_api_key(&self, key_hash: &str) {
        let mut cache = self.api_keys.write().unwrap_or_else(|e| e.into_inner());
        cache.invalidate(&key_hash.to_string());

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            metrics.invalidations += 1;
        }
    }

    // ==================== Cache Management ====================

    /// Clear all caches
    pub fn clear_all(&self) {
        self.workflows
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.user_quotas
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.workflow_quotas
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.api_keys
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// Evict expired entries from all caches
    pub fn evict_expired(&self) {
        self.workflows
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .evict_expired();
        self.user_quotas
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .evict_expired();
        self.workflow_quotas
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .evict_expired();
        self.api_keys
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .evict_expired();
    }

    /// Get cache statistics
    pub fn stats(&self) -> HashMap<String, CacheStats> {
        let mut stats = HashMap::new();
        stats.insert(
            "workflows".to_string(),
            self.workflows
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .stats(),
        );
        stats.insert(
            "user_quotas".to_string(),
            self.user_quotas
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .stats(),
        );
        stats.insert(
            "workflow_quotas".to_string(),
            self.workflow_quotas
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .stats(),
        );
        stats.insert(
            "api_keys".to_string(),
            self.api_keys
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .stats(),
        );
        stats
    }

    /// Get cache metrics
    pub fn metrics(&self) -> CacheMetrics {
        self.metrics
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Reset cache metrics
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
        *metrics = CacheMetrics::default();
    }

    /// Export metrics in a format suitable for monitoring systems
    pub fn export_metrics(&self) -> HashMap<String, f64> {
        let metrics = self.metrics.read().unwrap_or_else(|e| e.into_inner());
        let mut export = HashMap::new();

        export.insert("workflow_hits".to_string(), metrics.workflow_hits as f64);
        export.insert(
            "workflow_misses".to_string(),
            metrics.workflow_misses as f64,
        );
        export.insert("workflow_hit_rate".to_string(), metrics.workflow_hit_rate());

        export.insert(
            "user_quota_hits".to_string(),
            metrics.user_quota_hits as f64,
        );
        export.insert(
            "user_quota_misses".to_string(),
            metrics.user_quota_misses as f64,
        );
        export.insert(
            "user_quota_hit_rate".to_string(),
            metrics.user_quota_hit_rate(),
        );

        export.insert(
            "workflow_quota_hits".to_string(),
            metrics.workflow_quota_hits as f64,
        );
        export.insert(
            "workflow_quota_misses".to_string(),
            metrics.workflow_quota_misses as f64,
        );

        export.insert("api_key_hits".to_string(), metrics.api_key_hits as f64);
        export.insert("api_key_misses".to_string(), metrics.api_key_misses as f64);

        export.insert("overall_hit_rate".to_string(), metrics.overall_hit_rate());
        export.insert("evictions".to_string(), metrics.evictions as f64);
        export.insert("invalidations".to_string(), metrics.invalidations as f64);

        // Add size metrics
        let stats = self.stats();
        for (cache_name, cache_stats) in stats {
            export.insert(format!("{cache_name}_size"), cache_stats.size as f64);
            export.insert(
                format!("{cache_name}_utilization"),
                cache_stats.utilization(),
            );
        }

        export
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic_operations() {
        let config = CacheConfig {
            max_size: 10,
            default_ttl: std::time::Duration::from_secs(60),
            enable_metrics: true,
        };
        let cache = Cache::new(config);

        let workflow_id = Uuid::new_v4();
        let workflow = WorkflowRow {
            id: workflow_id.to_string(),
            name: "test".to_string(),
            description: None,
            definition: serde_json::to_string(&serde_json::json!({})).unwrap(),
            version: 1,
            tags: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Put workflow in cache
        cache.put_workflow(workflow_id, workflow.clone());

        // Get workflow from cache (hit)
        let cached = cache.get_workflow(&workflow_id);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().id, workflow_id.to_string());

        // Check metrics
        let metrics = cache.metrics();
        assert_eq!(metrics.workflow_hits, 1);
        assert_eq!(metrics.workflow_misses, 0);

        // Invalidate workflow
        cache.invalidate_workflow(&workflow_id);

        // Get workflow after invalidation (miss)
        let cached = cache.get_workflow(&workflow_id);
        assert!(cached.is_none());

        let metrics = cache.metrics();
        assert_eq!(metrics.workflow_hits, 1);
        assert_eq!(metrics.workflow_misses, 1);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let config = CacheConfig {
            max_size: 2,
            default_ttl: std::time::Duration::from_secs(60),
            enable_metrics: false,
        };
        let cache = Cache::new(config);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let workflow1 = WorkflowRow {
            id: id1.to_string(),
            name: "test1".to_string(),
            description: None,
            definition: serde_json::to_string(&serde_json::json!({})).unwrap(),
            version: 1,
            tags: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        let mut workflow2 = workflow1.clone();
        workflow2.id = id2.to_string();
        workflow2.name = "test2".to_string();

        let mut workflow3 = workflow1.clone();
        workflow3.id = id3.to_string();
        workflow3.name = "test3".to_string();

        // Fill cache to capacity
        cache.put_workflow(id1, workflow1);
        cache.put_workflow(id2, workflow2);

        // Access workflow1 to make it recently used
        cache.get_workflow(&id1);

        // Add workflow3, should evict workflow2 (LRU)
        cache.put_workflow(id3, workflow3);

        // workflow1 should still be in cache
        assert!(cache.get_workflow(&id1).is_some());

        // workflow3 should be in cache
        assert!(cache.get_workflow(&id3).is_some());
    }

    #[test]
    fn test_cache_stats() {
        let config = CacheConfig {
            max_size: 100,
            default_ttl: std::time::Duration::from_secs(60),
            enable_metrics: true,
        };
        let cache = Cache::new(config);

        let stats = cache.stats();
        assert_eq!(stats.get("workflows").unwrap().size, 0);
        assert_eq!(stats.get("workflows").unwrap().capacity, 100);
    }

    #[test]
    fn test_cache_metrics_hit_rate() {
        let metrics = CacheMetrics {
            workflow_hits: 80,
            workflow_misses: 20,
            user_quota_hits: 90,
            user_quota_misses: 10,
            ..Default::default()
        };

        assert_eq!(metrics.workflow_hit_rate(), 0.8);
        assert_eq!(metrics.user_quota_hit_rate(), 0.9);
        assert_eq!(metrics.overall_hit_rate(), 0.85);
    }

    #[test]
    fn test_lru_eviction_deterministic() {
        // max_size=3, very long TTL so expiry never fires during the test
        let mut cache: LruCache<&str, u64> = LruCache::new(3);
        let ttl = std::time::Duration::from_secs(300);

        // Insert A (seq=0), B (seq=1), C (seq=2) — fills cache
        cache.put("A", 1, ttl);
        cache.put("B", 2, ttl);
        cache.put("C", 3, ttl);

        // Access A — bumps A's seq to 3 (next_seq was 3 after the three inserts)
        let val = cache.get(&"A");
        assert!(val.is_some());

        // Insert D — cache is full (3 entries); B has seq=1 (lowest), so B is evicted
        cache.put("D", 4, ttl);

        // D must be present
        assert!(cache.get(&"D").is_some());
        // A must still be present (its seq was bumped above B's)
        assert!(cache.get(&"A").is_some());
        // C must still be present (seq=2, not the lowest after A was accessed)
        assert!(cache.get(&"C").is_some());
        // B must have been evicted (had seq=1, the lowest)
        assert!(cache.get(&"B").is_none());
    }

    #[test]
    fn test_lru_eviction_stress() {
        let max_size = 10usize;
        let mut cache: LruCache<usize, usize> = LruCache::new(max_size);
        let ttl = std::time::Duration::from_secs(300);

        for iteration in 0..1000usize {
            // Insert 10 entries (or overwrite existing ones on first iteration)
            for j in 0..10usize {
                let key = iteration * 10 + j;
                cache.put(key, key, ttl);
            }

            // Access entry 0 of the current batch (bumps its seq to highest)
            let access_key = iteration * 10;
            let _ = cache.get(&access_key);

            // Insert one more entry, triggering an eviction
            let overflow_key = iteration * 10 + 10;
            cache.put(overflow_key, overflow_key, ttl);

            // Cache must never exceed max_size
            assert!(
                cache.size() <= max_size,
                "iteration {iteration}: cache size {} exceeded max_size {max_size}",
                cache.size()
            );
        }
    }
}
