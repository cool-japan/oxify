//! Caching module for embeddings and search results
//!
//! This module provides LRU caching for:
//! - Embeddings (to avoid re-computing embeddings for the same text)
//! - Search results (to speed up frequent queries)

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// LRU cache with TTL support
pub struct LruCache<K, V> {
    capacity: usize,
    ttl: Option<Duration>,
    entries: Mutex<HashMap<K, CacheEntry<V>>>,
    order: Mutex<Vec<K>>,
}

struct CacheEntry<V> {
    value: V,
    created_at: Instant,
}

impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    /// Create a new LRU cache with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            ttl: None,
            entries: Mutex::new(HashMap::with_capacity(capacity)),
            order: Mutex::new(Vec::with_capacity(capacity)),
        }
    }

    /// Create a new LRU cache with capacity and TTL
    pub fn with_ttl(capacity: usize, ttl: Duration) -> Self {
        Self {
            capacity,
            ttl: Some(ttl),
            entries: Mutex::new(HashMap::with_capacity(capacity)),
            order: Mutex::new(Vec::with_capacity(capacity)),
        }
    }

    /// Get a value from the cache
    pub fn get(&self, key: &K) -> Option<V> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;

        // Check TTL
        if let Some(ttl) = self.ttl {
            if entry.created_at.elapsed() > ttl {
                drop(entries);
                self.remove(key);
                return None;
            }
        }

        let value = entry.value.clone();

        // Move to front of LRU order
        drop(entries);
        if let Ok(mut order) = self.order.lock() {
            if let Some(pos) = order.iter().position(|k| k == key) {
                let key = order.remove(pos);
                order.push(key);
            }
        }

        Some(value)
    }

    /// Insert a value into the cache
    pub fn insert(&self, key: K, value: V) {
        let mut entries = match self.entries.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        let mut order = match self.order.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        // If key exists, update value and move to front
        if entries.contains_key(&key) {
            entries.insert(
                key.clone(),
                CacheEntry {
                    value,
                    created_at: Instant::now(),
                },
            );
            if let Some(pos) = order.iter().position(|k| k == &key) {
                order.remove(pos);
            }
            order.push(key);
            return;
        }

        // Evict oldest if at capacity
        while entries.len() >= self.capacity {
            if let Some(oldest_key) = order.first().cloned() {
                entries.remove(&oldest_key);
                order.remove(0);
            } else {
                break;
            }
        }

        // Insert new entry
        entries.insert(
            key.clone(),
            CacheEntry {
                value,
                created_at: Instant::now(),
            },
        );
        order.push(key);
    }

    /// Remove a value from the cache
    pub fn remove(&self, key: &K) -> Option<V> {
        let mut entries = self.entries.lock().ok()?;
        let mut order = self.order.lock().ok()?;

        if let Some(pos) = order.iter().position(|k| k == key) {
            order.remove(pos);
        }

        entries.remove(key).map(|e| e.value)
    }

    /// Clear all entries from the cache
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        if let Ok(mut order) = self.order.lock() {
            order.clear();
        }
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let len = self.len();
        CacheStats {
            entries: len,
            capacity: self.capacity,
            utilization: len as f64 / self.capacity as f64,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries in the cache
    pub entries: usize,
    /// Maximum capacity
    pub capacity: usize,
    /// Utilization ratio (0.0 - 1.0)
    pub utilization: f64,
}

/// Cache key for embeddings
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmbeddingCacheKey {
    text_hash: u64,
    model: String,
}

impl EmbeddingCacheKey {
    /// Create a new embedding cache key
    pub fn new(text: &str, model: &str) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        Self {
            text_hash: hasher.finish(),
            model: model.to_string(),
        }
    }
}

/// Embedding cache
pub struct EmbeddingCache {
    cache: LruCache<EmbeddingCacheKey, Vec<f32>>,
}

impl EmbeddingCache {
    /// Create a new embedding cache
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(capacity),
        }
    }

    /// Create a new embedding cache with TTL
    pub fn with_ttl(capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: LruCache::with_ttl(capacity, ttl),
        }
    }

    /// Get an embedding from the cache
    pub fn get(&self, text: &str, model: &str) -> Option<Vec<f32>> {
        let key = EmbeddingCacheKey::new(text, model);
        self.cache.get(&key)
    }

    /// Insert an embedding into the cache
    pub fn insert(&self, text: &str, model: &str, embedding: Vec<f32>) {
        let key = EmbeddingCacheKey::new(text, model);
        self.cache.insert(key, embedding);
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cache.clear();
    }
}

/// Cache key for search results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SearchCacheKey {
    collection: String,
    query_hash: u64,
    top_k: usize,
}

impl SearchCacheKey {
    /// Create a new search cache key
    pub fn new(collection: &str, query: &[f32], top_k: usize) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for val in query {
            val.to_bits().hash(&mut hasher);
        }
        Self {
            collection: collection.to_string(),
            query_hash: hasher.finish(),
            top_k,
        }
    }
}

/// Search result cache
pub struct SearchCache {
    cache: LruCache<SearchCacheKey, Vec<crate::SearchResult>>,
}

impl SearchCache {
    /// Create a new search cache
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(capacity),
        }
    }

    /// Create a new search cache with TTL
    pub fn with_ttl(capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: LruCache::with_ttl(capacity, ttl),
        }
    }

    /// Get search results from the cache
    pub fn get(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
    ) -> Option<Vec<crate::SearchResult>> {
        let key = SearchCacheKey::new(collection, query, top_k);
        self.cache.get(&key)
    }

    /// Insert search results into the cache
    pub fn insert(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
        results: Vec<crate::SearchResult>,
    ) {
        let key = SearchCacheKey::new(collection, query, top_k);
        self.cache.insert(key, results);
    }

    /// Invalidate all entries for a collection
    pub fn invalidate_collection(&self, collection: &str) {
        // This is a simple implementation - for production use,
        // consider using a more efficient data structure
        if let Ok(entries) = self.cache.entries.lock() {
            let keys_to_remove: Vec<SearchCacheKey> = entries
                .keys()
                .filter(|k| k.collection == collection)
                .cloned()
                .collect();
            drop(entries);

            for key in keys_to_remove {
                self.cache.remove(&key);
            }
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache_basic() {
        let cache: LruCache<String, i32> = LruCache::new(3);

        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        cache.insert("c".to_string(), 3);

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), Some(2));
        assert_eq!(cache.get(&"c".to_string()), Some(3));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_lru_cache_eviction() {
        let cache: LruCache<String, i32> = LruCache::new(2);

        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        cache.insert("c".to_string(), 3); // Should evict "a"

        assert_eq!(cache.get(&"a".to_string()), None);
        assert_eq!(cache.get(&"b".to_string()), Some(2));
        assert_eq!(cache.get(&"c".to_string()), Some(3));
    }

    #[test]
    fn test_lru_cache_access_order() {
        let cache: LruCache<String, i32> = LruCache::new(2);

        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);

        // Access "a" to move it to front
        let _ = cache.get(&"a".to_string());

        // Insert "c" - should evict "b" (oldest)
        cache.insert("c".to_string(), 3);

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), None);
        assert_eq!(cache.get(&"c".to_string()), Some(3));
    }

    #[test]
    fn test_embedding_cache() {
        let cache = EmbeddingCache::new(100);

        let text = "Hello, world!";
        let model = "text-embedding-3-small";
        let embedding = vec![0.1, 0.2, 0.3];

        cache.insert(text, model, embedding.clone());

        let cached = cache.get(text, model);
        assert_eq!(cached, Some(embedding));

        // Different model should not match
        let other = cache.get(text, "other-model");
        assert_eq!(other, None);
    }

    #[test]
    fn test_search_cache() {
        let cache = SearchCache::new(100);

        let collection = "test_collection";
        let query = vec![0.1, 0.2, 0.3];
        let top_k = 5;

        let results = vec![crate::SearchResult {
            id: "1".to_string(),
            score: 0.9,
            payload: serde_json::json!({}),
            vector: None,
        }];

        cache.insert(collection, &query, top_k, results.clone());

        let cached = cache.get(collection, &query, top_k);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);

        // Different top_k should not match
        let other = cache.get(collection, &query, 10);
        assert!(other.is_none());
    }

    #[test]
    fn test_cache_stats() {
        let cache: LruCache<String, i32> = LruCache::new(10);

        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);

        let stats = cache.stats();
        assert_eq!(stats.entries, 2);
        assert_eq!(stats.capacity, 10);
        assert!((stats.utilization - 0.2).abs() < 0.001);
    }
}
