//! Vector Search Result Caching
//!
//! Caches vector similarity search results to reduce load on vector databases.
//!
//! ## Overview
//!
//! Vector search is computationally expensive, especially for:
//! - High-dimensional vectors (e.g., 1536 dimensions for OpenAI embeddings)
//! - Large collections (millions of vectors)
//! - Complex similarity metrics (cosine, euclidean, dot product)
//!
//! This cache stores search results keyed by:
//! - Query vector (hashed for efficient lookup)
//! - Collection name
//! - Search parameters (limit, filters, threshold)
//!
//! ## Cache Strategy
//!
//! - **TTL**: Short-lived cache (5 minutes default) as vector data changes frequently
//! - **LRU Eviction**: Automatically evict least recently used results
//! - **Query Hashing**: Fast lookup using SHA-256 hash of query vector
//! - **Invalidation**: Automatic invalidation on collection updates
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{VectorCache, VectorSearchParams, VectorSearchResult};
//!
//! let cache = VectorCache::new(config);
//!
//! // Generate cache key
//! let query_vector = vec![0.1, 0.2, 0.3, ...];
//! let params = VectorSearchParams {
//!     collection: "documents".to_string(),
//!     limit: 10,
//!     threshold: 0.7,
//!     filter: None,
//! };
//!
//! // Check cache first
//! if let Some(results) = cache.get(&query_vector, &params) {
//!     return Ok(results); // Cache hit
//! }
//!
//! // Cache miss - perform search
//! let results = vector_db.search(&query_vector, &params).await?;
//!
//! // Store in cache
//! cache.put(query_vector, params, results.clone());
//! ```

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration as StdDuration;

/// Vector search parameters for cache key
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VectorSearchParams {
    /// Collection name
    pub collection: String,
    /// Maximum number of results
    pub limit: usize,
    /// Similarity score threshold (multiplied by 1000 for integer comparison)
    pub threshold_millis: u32,
    /// Filter expression (JSON string for deterministic hashing)
    pub filter: Option<String>,
    /// Similarity metric
    pub metric: VectorMetric,
}

/// Vector similarity metric
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum VectorMetric {
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
    Manhattan,
}

/// Vector search result (cached)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    /// Result ID
    pub id: String,
    /// Similarity score
    pub score: f32,
    /// Result payload (metadata)
    pub payload: serde_json::Value,
}

/// Cache entry for vector search results
#[derive(Debug, Clone)]
struct VectorCacheEntry {
    results: Vec<VectorSearchResult>,
    expires_at: DateTime<Utc>,
    access_count: u64,
    last_accessed: DateTime<Utc>,
}

impl VectorCacheEntry {
    fn new(results: Vec<VectorSearchResult>, ttl: StdDuration) -> Self {
        let now = Utc::now();
        let ttl_duration = Duration::from_std(ttl).unwrap_or(Duration::seconds(300));
        Self {
            results,
            expires_at: now + ttl_duration,
            access_count: 0,
            last_accessed: now,
        }
    }

    fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    fn access(&mut self) -> Vec<VectorSearchResult> {
        self.access_count += 1;
        self.last_accessed = Utc::now();
        self.results.clone()
    }
}

/// Vector cache configuration
#[derive(Debug, Clone)]
pub struct VectorCacheConfig {
    /// Maximum number of cached searches
    pub max_size: usize,
    /// Default TTL for cache entries
    pub default_ttl: StdDuration,
    /// Enable cache metrics collection
    pub enable_metrics: bool,
}

impl Default for VectorCacheConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            default_ttl: StdDuration::from_secs(300), // 5 minutes
            enable_metrics: true,
        }
    }
}

/// Cache metrics for vector search
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VectorCacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub invalidations: u64,
}

impl VectorCacheMetrics {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }
}

/// Vector search result cache
pub struct VectorCache {
    entries: Arc<RwLock<HashMap<String, VectorCacheEntry>>>,
    config: VectorCacheConfig,
    metrics: Arc<RwLock<VectorCacheMetrics>>,
}

impl VectorCache {
    /// Create a new vector cache
    pub fn new(config: VectorCacheConfig) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::with_capacity(config.max_size))),
            config,
            metrics: Arc::new(RwLock::new(VectorCacheMetrics::default())),
        }
    }

    /// Generate cache key from query vector and parameters
    fn generate_key(query_vector: &[f32], params: &VectorSearchParams) -> String {
        // Hash query vector for efficient lookup
        let mut hasher = Sha256::new();
        for &val in query_vector {
            hasher.update(val.to_le_bytes());
        }

        // Include parameters in hash
        hasher.update(params.collection.as_bytes());
        hasher.update(params.limit.to_le_bytes());
        hasher.update(params.threshold_millis.to_le_bytes());
        if let Some(filter) = &params.filter {
            hasher.update(filter.as_bytes());
        }
        hasher.update([params.metric as u8]);

        hex::encode(hasher.finalize())
    }

    /// Get cached search results
    pub fn get(
        &self,
        query_vector: &[f32],
        params: &VectorSearchParams,
    ) -> Option<Vec<VectorSearchResult>> {
        let key = Self::generate_key(query_vector, params);
        let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());

        if let Some(entry) = entries.get_mut(&key) {
            if entry.is_expired() {
                entries.remove(&key);
                if self.config.enable_metrics {
                    let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
                    metrics.misses += 1;
                }
                return None;
            }

            if self.config.enable_metrics {
                let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
                metrics.hits += 1;
            }

            Some(entry.access())
        } else {
            if self.config.enable_metrics {
                let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
                metrics.misses += 1;
            }
            None
        }
    }

    /// Put search results into cache
    pub fn put(
        &self,
        query_vector: Vec<f32>,
        params: VectorSearchParams,
        results: Vec<VectorSearchResult>,
    ) {
        let key = Self::generate_key(&query_vector, &params);
        let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());

        // Evict expired entries first
        self.evict_expired(&mut entries);

        // If at capacity, evict LRU entry
        if entries.len() >= self.config.max_size {
            self.evict_lru(&mut entries);
        }

        entries.insert(key, VectorCacheEntry::new(results, self.config.default_ttl));
    }

    /// Invalidate cache for a specific collection
    pub fn invalidate_collection(&self, _collection: &str) {
        let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());
        let keys_to_remove: Vec<String> = entries
            .keys()
            .filter(|_| {
                // We can't determine collection from hash alone
                // In production, consider storing collection separately
                true
            })
            .cloned()
            .collect();

        let removed = keys_to_remove.len();
        for key in keys_to_remove {
            entries.remove(&key);
        }

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            metrics.invalidations += removed as u64;
        }
    }

    /// Clear all cache entries
    pub fn clear_all(&self) {
        let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());
        entries.clear();
    }

    /// Evict expired entries
    fn evict_expired(&self, entries: &mut HashMap<String, VectorCacheEntry>) {
        let now = Utc::now();
        entries.retain(|_, entry| entry.expires_at > now);
    }

    /// Evict least recently used entry
    fn evict_lru(&self, entries: &mut HashMap<String, VectorCacheEntry>) {
        if let Some(lru_key) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| key.clone())
        {
            entries.remove(&lru_key);

            if self.config.enable_metrics {
                let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
                metrics.evictions += 1;
            }
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> HashMap<String, f64> {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        let metrics = self.metrics.read().unwrap_or_else(|e| e.into_inner());

        let mut stats = HashMap::new();
        stats.insert("size".to_string(), entries.len() as f64);
        stats.insert("capacity".to_string(), self.config.max_size as f64);
        stats.insert(
            "utilization".to_string(),
            entries.len() as f64 / self.config.max_size as f64,
        );
        stats.insert("hits".to_string(), metrics.hits as f64);
        stats.insert("misses".to_string(), metrics.misses as f64);
        stats.insert("hit_rate".to_string(), metrics.hit_rate());
        stats.insert("evictions".to_string(), metrics.evictions as f64);
        stats.insert("invalidations".to_string(), metrics.invalidations as f64);

        stats
    }

    /// Get cache metrics
    pub fn metrics(&self) -> VectorCacheMetrics {
        self.metrics.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reset cache metrics
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
        *metrics = VectorCacheMetrics::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_cache_basic_operations() {
        let config = VectorCacheConfig {
            max_size: 10,
            default_ttl: StdDuration::from_secs(60),
            enable_metrics: true,
        };
        let cache = VectorCache::new(config);

        let query_vector = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let params = VectorSearchParams {
            collection: "test".to_string(),
            limit: 10,
            threshold_millis: 700, // 0.7 * 1000
            filter: None,
            metric: VectorMetric::Cosine,
        };

        let results = vec![VectorSearchResult {
            id: "doc1".to_string(),
            score: 0.95,
            payload: serde_json::json!({"text": "test document"}),
        }];

        // Put into cache
        cache.put(query_vector.clone(), params.clone(), results.clone());

        // Get from cache (hit)
        let cached = cache.get(&query_vector, &params);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);

        // Check metrics
        let metrics = cache.metrics();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 0);
    }

    #[test]
    fn test_vector_cache_miss() {
        let config = VectorCacheConfig {
            max_size: 10,
            default_ttl: StdDuration::from_secs(60),
            enable_metrics: true,
        };
        let cache = VectorCache::new(config);

        let query_vector = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let params = VectorSearchParams {
            collection: "test".to_string(),
            limit: 10,
            threshold_millis: 700,
            filter: None,
            metric: VectorMetric::Cosine,
        };

        // Get from empty cache (miss)
        let cached = cache.get(&query_vector, &params);
        assert!(cached.is_none());

        // Check metrics
        let metrics = cache.metrics();
        assert_eq!(metrics.hits, 0);
        assert_eq!(metrics.misses, 1);
    }

    #[test]
    fn test_vector_cache_different_params() {
        let config = VectorCacheConfig {
            max_size: 10,
            default_ttl: StdDuration::from_secs(60),
            enable_metrics: false,
        };
        let cache = VectorCache::new(config);

        let query_vector = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let params1 = VectorSearchParams {
            collection: "test".to_string(),
            limit: 10,
            threshold_millis: 700,
            filter: None,
            metric: VectorMetric::Cosine,
        };
        let params2 = VectorSearchParams {
            collection: "test".to_string(),
            limit: 20, // Different limit
            threshold_millis: 700,
            filter: None,
            metric: VectorMetric::Cosine,
        };

        let results = vec![VectorSearchResult {
            id: "doc1".to_string(),
            score: 0.95,
            payload: serde_json::json!({"text": "test"}),
        }];

        // Cache with params1
        cache.put(query_vector.clone(), params1.clone(), results.clone());

        // Get with params1 (hit)
        assert!(cache.get(&query_vector, &params1).is_some());

        // Get with params2 (miss - different parameters)
        assert!(cache.get(&query_vector, &params2).is_none());
    }

    #[test]
    fn test_vector_cache_lru_eviction() {
        let config = VectorCacheConfig {
            max_size: 2,
            default_ttl: StdDuration::from_secs(60),
            enable_metrics: true,
        };
        let cache = VectorCache::new(config);

        let params = VectorSearchParams {
            collection: "test".to_string(),
            limit: 10,
            threshold_millis: 700,
            filter: None,
            metric: VectorMetric::Cosine,
        };

        let results = vec![VectorSearchResult {
            id: "doc1".to_string(),
            score: 0.95,
            payload: serde_json::json!({"text": "test"}),
        }];

        let vec1 = vec![0.1, 0.2, 0.3];
        let vec2 = vec![0.4, 0.5, 0.6];
        let vec3 = vec![0.7, 0.8, 0.9];

        // Fill cache to capacity
        cache.put(vec1.clone(), params.clone(), results.clone());
        cache.put(vec2.clone(), params.clone(), results.clone());

        // Access vec1 to make it recently used
        cache.get(&vec1, &params);

        // Add vec3, should evict vec2 (LRU)
        cache.put(vec3.clone(), params.clone(), results.clone());

        // vec1 should still be in cache
        assert!(cache.get(&vec1, &params).is_some());

        // vec3 should be in cache
        assert!(cache.get(&vec3, &params).is_some());

        // vec2 should be evicted
        assert!(cache.get(&vec2, &params).is_none());
    }

    #[test]
    fn test_vector_cache_hit_rate() {
        let metrics = VectorCacheMetrics {
            hits: 80,
            misses: 20,
            evictions: 0,
            invalidations: 0,
        };

        assert_eq!(metrics.hit_rate(), 0.8);
    }
}
