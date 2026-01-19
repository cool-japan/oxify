//! Query result caching for improved search performance.
//!
//! This module provides caching mechanisms for vector search results to avoid
//! recomputing identical or similar queries. Particularly useful for production
//! RAG systems with repeated queries.
//!
//! ## Features
//!
//! - LRU (Least Recently Used) eviction
//! - TTL (Time-To-Live) expiration
//! - Approximate query matching with configurable tolerance
//! - Cache statistics and monitoring
//! - Thread-safe concurrent access
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::cache::{QueryCache, CacheConfig};
//! use oxify_vector::{SearchResult, DistanceMetric};
//!
//! # fn example() -> anyhow::Result<()> {
//! let config = CacheConfig::default();
//! let mut cache = QueryCache::new(config);
//!
//! let query = vec![1.0, 2.0, 3.0];
//! let results = vec![
//!     SearchResult {
//!         entity_id: "doc1".to_string(),
//!         score: 0.95,
//!         distance: 0.05,
//!         rank: 1,
//!     },
//! ];
//!
//! // Cache the results
//! cache.put(&query, DistanceMetric::Cosine, 10, results.clone());
//!
//! // Retrieve from cache
//! if let Some(cached) = cache.get(&query, DistanceMetric::Cosine, 10) {
//!     println!("Cache hit! Found {} results", cached.len());
//! }
//! # Ok(())
//! # }
//! ```

use crate::types::{DistanceMetric, SearchResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Configuration for query result caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum number of cached queries.
    pub max_entries: usize,
    /// Time-to-live for cached results.
    pub ttl: Duration,
    /// Tolerance for approximate query matching (0.0 = exact match).
    pub similarity_threshold: f32,
    /// Whether to enable approximate matching.
    pub enable_approximate_matching: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl: Duration::from_secs(300), // 5 minutes
            similarity_threshold: 0.99,    // 99% similar
            enable_approximate_matching: false,
        }
    }
}

impl CacheConfig {
    /// Create a config optimized for high hit rate (more entries, longer TTL).
    pub fn high_hit_rate() -> Self {
        Self {
            max_entries: 10_000,
            ttl: Duration::from_secs(3600), // 1 hour
            similarity_threshold: 0.95,
            enable_approximate_matching: true,
        }
    }

    /// Create a config optimized for low memory (fewer entries, shorter TTL).
    pub fn low_memory() -> Self {
        Self {
            max_entries: 100,
            ttl: Duration::from_secs(60), // 1 minute
            similarity_threshold: 0.99,
            enable_approximate_matching: false,
        }
    }

    /// Create a config with exact matching only.
    pub fn exact_match_only() -> Self {
        Self {
            max_entries: 1000,
            ttl: Duration::from_secs(300),
            similarity_threshold: 1.0,
            enable_approximate_matching: false,
        }
    }
}

/// Query cache key combining query vector, metric, and k.
#[derive(Debug, Clone, PartialEq)]
struct CacheKey {
    query_hash: u64,
    metric: DistanceMetric,
    k: usize,
}

impl Hash for CacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.query_hash.hash(state);
        // Hash the metric by discriminant
        std::mem::discriminant(&self.metric).hash(state);
        self.k.hash(state);
    }
}

impl Eq for CacheKey {}

/// Cached query entry with results and metadata.
#[derive(Debug, Clone)]
struct CacheEntry {
    results: Vec<SearchResult>,
    inserted_at: Instant,
    last_accessed: Instant,
    access_count: u64,
    query: Vec<f32>, // Store for approximate matching
}

impl CacheEntry {
    fn new(query: Vec<f32>, results: Vec<SearchResult>) -> Self {
        let now = Instant::now();
        Self {
            results,
            inserted_at: now,
            last_accessed: now,
            access_count: 0,
            query,
        }
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.inserted_at.elapsed() > ttl
    }

    fn touch(&mut self) {
        self.last_accessed = Instant::now();
        self.access_count += 1;
    }
}

/// Thread-safe query result cache with LRU eviction.
pub struct QueryCache {
    config: CacheConfig,
    cache: Arc<RwLock<HashMap<CacheKey, CacheEntry>>>,
    access_order: Arc<RwLock<VecDeque<CacheKey>>>,
    stats: Arc<RwLock<CacheStats>>,
}

impl QueryCache {
    /// Create a new query cache with the given configuration.
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            access_order: Arc::new(RwLock::new(VecDeque::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Get cached results for a query.
    ///
    /// Returns `None` if the query is not cached or has expired.
    pub fn get(
        &self,
        query: &[f32],
        metric: DistanceMetric,
        k: usize,
    ) -> Option<Vec<SearchResult>> {
        let key = self.make_key(query, metric, k);

        // Try exact match first
        if let Some(entry) = self.get_exact(&key) {
            return Some(entry);
        }

        // Try approximate match if enabled
        if self.config.enable_approximate_matching {
            if let Some(entry) = self.get_approximate(query, metric, k) {
                return Some(entry);
            }
        }

        // Update stats for miss
        if let Ok(mut stats) = self.stats.write() {
            stats.misses += 1;
        }

        None
    }

    /// Get exact cache match.
    fn get_exact(&self, key: &CacheKey) -> Option<Vec<SearchResult>> {
        let mut cache = self.cache.write().ok()?;
        let mut access_order = self.access_order.write().ok()?;

        if let Some(entry) = cache.get_mut(key) {
            // Check if expired
            if entry.is_expired(self.config.ttl) {
                cache.remove(key);
                access_order.retain(|k| k != key);
                if let Ok(mut stats) = self.stats.write() {
                    stats.expirations += 1;
                }
                return None;
            }

            // Update access info
            entry.touch();

            // Move to front of access order (LRU)
            access_order.retain(|k| k != key);
            access_order.push_back(key.clone());

            // Update stats
            if let Ok(mut stats) = self.stats.write() {
                stats.hits += 1;
            }

            return Some(entry.results.clone());
        }

        None
    }

    /// Get approximate cache match based on similarity threshold.
    fn get_approximate(
        &self,
        query: &[f32],
        metric: DistanceMetric,
        k: usize,
    ) -> Option<Vec<SearchResult>> {
        let best_key = {
            let cache = self.cache.read().ok()?;

            // Find most similar cached query
            let mut best_match: Option<(CacheKey, f32)> = None;

            for (cache_key, entry) in cache.iter() {
                // Only consider same metric and k
                if cache_key.metric != metric || cache_key.k != k {
                    continue;
                }

                // Skip expired entries
                if entry.is_expired(self.config.ttl) {
                    continue;
                }

                // Compute similarity
                let similarity = cosine_similarity(&entry.query, query);

                if similarity >= self.config.similarity_threshold {
                    if let Some((_, best_sim)) = &best_match {
                        if similarity > *best_sim {
                            best_match = Some((cache_key.clone(), similarity));
                        }
                    } else {
                        best_match = Some((cache_key.clone(), similarity));
                    }
                }
            }

            best_match.map(|(key, _)| key)
        }; // cache read lock is released here

        if let Some(key) = best_key {
            return self.get_exact(&key);
        }

        None
    }

    /// Store query results in the cache.
    pub fn put(
        &mut self,
        query: &[f32],
        metric: DistanceMetric,
        k: usize,
        results: Vec<SearchResult>,
    ) {
        let key = self.make_key(query, metric, k);
        let entry = CacheEntry::new(query.to_vec(), results);

        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut access_order = match self.access_order.write() {
            Ok(a) => a,
            Err(_) => return,
        };

        // Evict if at capacity
        if cache.len() >= self.config.max_entries && !cache.contains_key(&key) {
            if let Some(oldest_key) = access_order.pop_front() {
                cache.remove(&oldest_key);
                if let Ok(mut stats) = self.stats.write() {
                    stats.evictions += 1;
                }
            }
        }

        cache.insert(key.clone(), entry);
        access_order.push_back(key);

        if let Ok(mut stats) = self.stats.write() {
            stats.inserts += 1;
        }
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
        if let Ok(mut access_order) = self.access_order.write() {
            access_order.clear();
        }
        if let Ok(mut stats) = self.stats.write() {
            *stats = CacheStats::default();
        }
    }

    /// Remove expired entries from the cache.
    pub fn evict_expired(&mut self) -> usize {
        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return 0,
        };

        let mut access_order = match self.access_order.write() {
            Ok(a) => a,
            Err(_) => return 0,
        };

        let mut expired_keys = Vec::new();

        for (key, entry) in cache.iter() {
            if entry.is_expired(self.config.ttl) {
                expired_keys.push(key.clone());
            }
        }

        let count = expired_keys.len();

        for key in expired_keys {
            cache.remove(&key);
            access_order.retain(|k| k != &key);
        }

        if let Ok(mut stats) = self.stats.write() {
            stats.expirations += count as u64;
        }

        count
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        self.stats.read().unwrap().clone()
    }

    /// Get current cache size.
    pub fn len(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Create a cache key from query parameters.
    fn make_key(&self, query: &[f32], metric: DistanceMetric, k: usize) -> CacheKey {
        CacheKey {
            query_hash: hash_f32_slice(query),
            metric,
            k,
        }
    }
}

/// Statistics for cache performance monitoring.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of cache hits.
    pub hits: u64,
    /// Number of cache misses.
    pub misses: u64,
    /// Number of cache inserts.
    pub inserts: u64,
    /// Number of cache evictions (LRU).
    pub evictions: u64,
    /// Number of cache expirations (TTL).
    pub expirations: u64,
}

impl CacheStats {
    /// Calculate hit rate as a percentage.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }

    /// Calculate miss rate as a percentage.
    pub fn miss_rate(&self) -> f64 {
        100.0 - self.hit_rate()
    }
}

/// Hash a float slice for cache key generation.
fn hash_f32_slice(slice: &[f32]) -> u64 {
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();

    // Hash length first
    slice.len().hash(&mut hasher);

    // Hash each float as bits
    for &val in slice {
        val.to_bits().hash(&mut hasher);
    }

    hasher.finish()
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a.sqrt() * norm_b.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.max_entries, 1000);
        assert_eq!(config.ttl, Duration::from_secs(300));
        assert!(!config.enable_approximate_matching);
    }

    #[test]
    fn test_cache_config_presets() {
        let high_hit = CacheConfig::high_hit_rate();
        assert_eq!(high_hit.max_entries, 10_000);
        assert!(high_hit.enable_approximate_matching);

        let low_mem = CacheConfig::low_memory();
        assert_eq!(low_mem.max_entries, 100);
        assert_eq!(low_mem.ttl, Duration::from_secs(60));

        let exact = CacheConfig::exact_match_only();
        assert_eq!(exact.similarity_threshold, 1.0);
        assert!(!exact.enable_approximate_matching);
    }

    #[test]
    fn test_query_cache_basic() {
        let config = CacheConfig::default();
        let mut cache = QueryCache::new(config);

        let query = vec![1.0, 2.0, 3.0];
        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        // Initially empty
        assert!(cache.is_empty());

        // Put and get
        cache.put(&query, DistanceMetric::Cosine, 10, results.clone());
        assert_eq!(cache.len(), 1);

        let cached = cache.get(&query, DistanceMetric::Cosine, 10);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
    }

    #[test]
    fn test_query_cache_miss() {
        let config = CacheConfig::default();
        let cache = QueryCache::new(config);

        let query = vec![1.0, 2.0, 3.0];
        let cached = cache.get(&query, DistanceMetric::Cosine, 10);
        assert!(cached.is_none());

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
    }

    #[test]
    fn test_query_cache_different_k() {
        let config = CacheConfig::default();
        let mut cache = QueryCache::new(config);

        let query = vec![1.0, 2.0, 3.0];
        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        cache.put(&query, DistanceMetric::Cosine, 10, results.clone());

        // Same query but different k should miss
        let cached = cache.get(&query, DistanceMetric::Cosine, 20);
        assert!(cached.is_none());
    }

    #[test]
    fn test_query_cache_different_metric() {
        let config = CacheConfig::default();
        let mut cache = QueryCache::new(config);

        let query = vec![1.0, 2.0, 3.0];
        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        cache.put(&query, DistanceMetric::Cosine, 10, results.clone());

        // Same query but different metric should miss
        let cached = cache.get(&query, DistanceMetric::Euclidean, 10);
        assert!(cached.is_none());
    }

    #[test]
    fn test_query_cache_lru_eviction() {
        let config = CacheConfig {
            max_entries: 2,
            ..Default::default()
        };
        let mut cache = QueryCache::new(config);

        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        // Add 3 queries (should evict oldest)
        cache.put(&[1.0], DistanceMetric::Cosine, 10, results.clone());
        cache.put(&[2.0], DistanceMetric::Cosine, 10, results.clone());
        cache.put(&[3.0], DistanceMetric::Cosine, 10, results.clone());

        assert_eq!(cache.len(), 2);

        // First query should be evicted
        let cached = cache.get(&[1.0], DistanceMetric::Cosine, 10);
        assert!(cached.is_none());

        // Last two should be present
        assert!(cache.get(&[2.0], DistanceMetric::Cosine, 10).is_some());
        assert!(cache.get(&[3.0], DistanceMetric::Cosine, 10).is_some());
    }

    #[test]
    fn test_query_cache_clear() {
        let config = CacheConfig::default();
        let mut cache = QueryCache::new(config);

        let query = vec![1.0, 2.0, 3.0];
        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        cache.put(&query, DistanceMetric::Cosine, 10, results);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_query_cache_stats() {
        let config = CacheConfig::default();
        let mut cache = QueryCache::new(config);

        let query = vec![1.0, 2.0, 3.0];
        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        cache.put(&query, DistanceMetric::Cosine, 10, results);
        let stats = cache.stats();
        assert_eq!(stats.inserts, 1);

        // Hit
        cache.get(&query, DistanceMetric::Cosine, 10);
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);

        // Miss
        cache.get(&[9.0], DistanceMetric::Cosine, 10);
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);

        assert_eq!(stats.hit_rate(), 50.0);
        assert_eq!(stats.miss_rate(), 50.0);
    }

    #[test]
    fn test_hash_f32_slice() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let c = vec![1.0, 2.0, 3.1];

        assert_eq!(hash_f32_slice(&a), hash_f32_slice(&b));
        assert_ne!(hash_f32_slice(&a), hash_f32_slice(&c));
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.01);
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let stats = CacheStats {
            hits: 75,
            misses: 25,
            inserts: 100,
            evictions: 0,
            expirations: 0,
        };

        assert_eq!(stats.hit_rate(), 75.0);
        assert_eq!(stats.miss_rate(), 25.0);
    }

    #[test]
    fn test_cache_entry_expiration() {
        let query = vec![1.0, 2.0, 3.0];
        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        let entry = CacheEntry::new(query, results);

        // Should not be expired immediately
        assert!(!entry.is_expired(Duration::from_secs(1)));

        // Should be expired after a very short TTL
        std::thread::sleep(Duration::from_millis(10));
        assert!(entry.is_expired(Duration::from_millis(1)));
    }

    #[test]
    fn test_approximate_matching_disabled() {
        let config = CacheConfig::exact_match_only();
        let mut cache = QueryCache::new(config);

        let query1 = vec![1.0, 0.0, 0.0];
        let query2 = vec![0.99, 0.01, 0.0]; // Very similar but not exact

        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        cache.put(&query1, DistanceMetric::Cosine, 10, results);

        // Should not match due to disabled approximate matching
        let cached = cache.get(&query2, DistanceMetric::Cosine, 10);
        assert!(cached.is_none());
    }

    #[test]
    fn test_approximate_matching_enabled() {
        let config = CacheConfig {
            enable_approximate_matching: true,
            similarity_threshold: 0.95,
            ..Default::default()
        };
        let mut cache = QueryCache::new(config);

        let query1 = vec![1.0, 0.0, 0.0];
        let query2 = vec![0.99, 0.14, 0.0]; // >95% similar

        let results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.95,
            distance: 0.05,
            rank: 1,
        }];

        cache.put(&query1, DistanceMetric::Cosine, 10, results);

        // Should match due to approximate matching
        let cached = cache.get(&query2, DistanceMetric::Cosine, 10);
        assert!(cached.is_some());
    }
}
