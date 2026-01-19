//! HTTP response caching implementation for performance optimization.
//!
//! Provides ETag-based caching, Cache-Control headers, and in-memory LRU cache
//! for hot endpoints to improve response times and reduce server load.

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tracing::{debug, trace};

/// Cache entry storing response data and metadata.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Response body
    pub body: Vec<u8>,
    /// Response status code
    pub status: StatusCode,
    /// Response headers
    pub headers: Vec<(String, String)>,
    /// ETag value
    pub etag: String,
    /// When this entry was created
    pub created_at: Instant,
    /// Time-to-live in seconds
    pub ttl: Duration,
    /// Number of times this entry was served
    pub hit_count: u64,
}

impl CacheEntry {
    /// Check if this cache entry is expired.
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }

    /// Check if this cache entry is still valid.
    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }
}

/// LRU (Least Recently Used) cache for storing HTTP responses.
pub struct LruCache {
    /// Cache storage
    entries: HashMap<String, CacheEntry>,
    /// Access order tracking (front = most recent)
    access_order: VecDeque<String>,
    /// Maximum number of entries
    max_size: usize,
    /// Cache statistics
    stats: CacheStats,
}

impl LruCache {
    /// Create a new LRU cache with the specified maximum size.
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_order: VecDeque::new(),
            max_size,
            stats: CacheStats::default(),
        }
    }

    /// Get a cache entry by key.
    pub fn get(&mut self, key: &str) -> Option<&CacheEntry> {
        // Check if entry exists and is valid
        let is_expired = self
            .entries
            .get(key)
            .map(|entry| entry.is_expired())
            .unwrap_or(false);

        if is_expired {
            // Entry expired, remove it
            self.remove(key);
            self.stats.misses += 1;
            trace!(key = %key, "Cache miss (expired)");
            return None;
        }

        if let Some(entry) = self.entries.get(key) {
            // Move to front (most recently used)
            if let Some(pos) = self.access_order.iter().position(|k| k == key) {
                self.access_order.remove(pos);
                self.access_order.push_front(key.to_string());
            }
            self.stats.hits += 1;
            trace!(key = %key, "Cache hit");
            return Some(entry);
        }

        self.stats.misses += 1;
        trace!(key = %key, "Cache miss");
        None
    }

    /// Insert a cache entry.
    pub fn insert(&mut self, key: String, entry: CacheEntry) {
        // Remove if already exists
        if self.entries.contains_key(&key) {
            self.remove(&key);
        }

        // Evict least recently used if at capacity
        while self.entries.len() >= self.max_size && !self.access_order.is_empty() {
            if let Some(lru_key) = self.access_order.pop_back() {
                self.entries.remove(&lru_key);
                self.stats.evictions += 1;
                debug!(key = %lru_key, "Cache eviction");
            }
        }

        // Insert new entry
        self.access_order.push_front(key.clone());
        self.entries.insert(key.clone(), entry);
        debug!(key = %key, "Cache insert");
    }

    /// Remove a cache entry by key.
    pub fn remove(&mut self, key: &str) {
        self.entries.remove(key);
        if let Some(pos) = self.access_order.iter().position(|k| k == key) {
            self.access_order.remove(pos);
        }
    }

    /// Clear all entries from the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.access_order.clear();
        debug!("Cache cleared");
    }

    /// Get cache statistics.
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove expired entries.
    pub fn cleanup_expired(&mut self) {
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired_keys {
            self.remove(&key);
            debug!(key = %key, "Removed expired cache entry");
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Number of evictions
    pub evictions: u64,
}

impl CacheStats {
    /// Calculate hit rate (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Calculate miss rate (0.0 to 1.0).
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }
}

/// HTTP response cache manager.
pub struct ResponseCache {
    /// LRU cache storage
    cache: Arc<RwLock<LruCache>>,
    /// Cache configuration
    config: CacheConfig,
}

impl ResponseCache {
    /// Create a new response cache with configuration.
    pub fn new(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(config.max_entries))),
            config,
        }
    }

    /// Get a cached response by key.
    pub fn get(&self, key: &str) -> Option<CacheEntry> {
        let mut cache = self.cache.write().ok()?;
        cache.get(key).cloned()
    }

    /// Insert a response into the cache.
    pub fn insert(&self, key: String, entry: CacheEntry) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key, entry);
        }
    }

    /// Get cache statistics.
    pub fn stats(&self) -> Option<CacheStats> {
        self.cache.read().ok().map(|cache| cache.stats().clone())
    }

    /// Clear the cache.
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// Cleanup expired entries.
    pub fn cleanup_expired(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.cleanup_expired();
        }
    }

    /// Check if a path is cacheable based on configuration.
    pub fn is_cacheable_path(&self, path: &str) -> bool {
        // Don't cache excluded paths
        if self
            .config
            .excluded_paths
            .iter()
            .any(|p| path.starts_with(p))
        {
            return false;
        }

        // Only cache included paths if specified
        if !self.config.included_paths.is_empty() {
            return self
                .config
                .included_paths
                .iter()
                .any(|p| path.starts_with(p));
        }

        true
    }
}

/// Cache configuration.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of cache entries
    pub max_entries: usize,
    /// Default TTL for cache entries
    pub default_ttl: Duration,
    /// Paths to include in caching (empty = all paths)
    pub included_paths: Vec<String>,
    /// Paths to exclude from caching
    pub excluded_paths: Vec<String>,
    /// Enable ETag support
    pub enable_etag: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            default_ttl: Duration::from_secs(300), // 5 minutes
            included_paths: vec![],
            excluded_paths: vec![
                "/health".to_string(),
                "/ready".to_string(),
                "/live".to_string(),
                "/metrics".to_string(),
            ],
            enable_etag: true,
        }
    }
}

impl CacheConfig {
    /// Create a new cache configuration builder.
    pub fn builder() -> CacheConfigBuilder {
        CacheConfigBuilder::default()
    }
}

/// Cache configuration builder.
#[derive(Debug, Default)]
pub struct CacheConfigBuilder {
    max_entries: Option<usize>,
    default_ttl: Option<Duration>,
    included_paths: Vec<String>,
    excluded_paths: Vec<String>,
    enable_etag: Option<bool>,
}

impl CacheConfigBuilder {
    /// Set maximum number of cache entries.
    pub fn max_entries(mut self, max_entries: usize) -> Self {
        self.max_entries = Some(max_entries);
        self
    }

    /// Set default TTL for cache entries.
    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    /// Add a path to include in caching.
    pub fn include_path(mut self, path: impl Into<String>) -> Self {
        self.included_paths.push(path.into());
        self
    }

    /// Add a path to exclude from caching.
    pub fn exclude_path(mut self, path: impl Into<String>) -> Self {
        self.excluded_paths.push(path.into());
        self
    }

    /// Enable or disable ETag support.
    pub fn enable_etag(mut self, enable: bool) -> Self {
        self.enable_etag = Some(enable);
        self
    }

    /// Build the cache configuration.
    pub fn build(self) -> CacheConfig {
        let default = CacheConfig::default();
        CacheConfig {
            max_entries: self.max_entries.unwrap_or(default.max_entries),
            default_ttl: self.default_ttl.unwrap_or(default.default_ttl),
            included_paths: if self.included_paths.is_empty() {
                default.included_paths
            } else {
                self.included_paths
            },
            excluded_paths: if self.excluded_paths.is_empty() {
                default.excluded_paths
            } else {
                self.excluded_paths
            },
            enable_etag: self.enable_etag.unwrap_or(default.enable_etag),
        }
    }
}

/// Generate ETag from response body.
pub fn generate_etag(body: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    body.hash(&mut hasher);
    format!("\"{:x}\"", hasher.finish())
}

/// Cache middleware for HTTP responses.
///
/// This middleware caches GET requests and serves cached responses with ETag support.
pub async fn cache_middleware(cache: Arc<ResponseCache>, req: Request, next: Next) -> Response {
    // Only cache GET requests
    if req.method() != axum::http::Method::GET {
        return next.run(req).await;
    }

    let path = req.uri().path();

    // Check if path is cacheable
    if !cache.is_cacheable_path(path) {
        return next.run(req).await;
    }

    let cache_key = path.to_string();

    // Check for If-None-Match header (ETag conditional request)
    let if_none_match = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Try to get from cache
    if let Some(entry) = cache.get(&cache_key) {
        // Check If-None-Match for conditional request
        if let Some(client_etag) = if_none_match {
            if client_etag == entry.etag {
                // ETag matches, return 304 Not Modified
                return StatusCode::NOT_MODIFIED.into_response();
            }
        }

        // Return cached response
        let mut response = Response::builder()
            .status(entry.status)
            .body(Body::from(entry.body.clone()))
            .unwrap();

        // Add cached headers
        for (key, value) in &entry.headers {
            if let Ok(header_value) = HeaderValue::from_str(value) {
                response.headers_mut().insert(
                    axum::http::HeaderName::from_bytes(key.as_bytes()).unwrap(),
                    header_value,
                );
            }
        }

        // Add ETag
        if cache.config.enable_etag {
            if let Ok(etag_value) = HeaderValue::from_str(&entry.etag) {
                response.headers_mut().insert(header::ETAG, etag_value);
            }
        }

        // Add X-Cache header
        response
            .headers_mut()
            .insert("X-Cache", HeaderValue::from_static("HIT"));

        return response;
    }

    // Cache miss, proceed with request
    let response = next.run(req).await;

    // Only cache successful responses
    if !response.status().is_success() {
        return response;
    }

    // Clone status and headers before consuming response
    let status = response.status();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    // Extract body (this consumes the response)
    let (parts, body) = response.into_parts();
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => {
            // Failed to read body, return error response
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to cache response",
            )
                .into_response();
        }
    };

    // Generate ETag
    let etag = if cache.config.enable_etag {
        generate_etag(&body_bytes)
    } else {
        String::new()
    };

    // Create cache entry
    let entry = CacheEntry {
        body: body_bytes.clone(),
        status,
        headers: headers.clone(),
        etag: etag.clone(),
        created_at: Instant::now(),
        ttl: cache.config.default_ttl,
        hit_count: 0,
    };

    // Store in cache
    cache.insert(cache_key, entry);

    // Reconstruct response
    let mut response = Response::from_parts(parts, Body::from(body_bytes));

    // Add ETag header
    if cache.config.enable_etag {
        if let Ok(etag_value) = HeaderValue::from_str(&etag) {
            response.headers_mut().insert(header::ETAG, etag_value);
        }
    }

    // Add X-Cache header
    response
        .headers_mut()
        .insert("X-Cache", HeaderValue::from_static("MISS"));

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache_basic() {
        let mut cache = LruCache::new(2);

        let entry1 = CacheEntry {
            body: b"test1".to_vec(),
            status: StatusCode::OK,
            headers: vec![],
            etag: "etag1".to_string(),
            created_at: Instant::now(),
            ttl: Duration::from_secs(60),
            hit_count: 0,
        };

        cache.insert("key1".to_string(), entry1);
        assert_eq!(cache.len(), 1);

        let retrieved = cache.get("key1");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_lru_cache_eviction() {
        let mut cache = LruCache::new(2);

        let entry1 = CacheEntry {
            body: b"test1".to_vec(),
            status: StatusCode::OK,
            headers: vec![],
            etag: "etag1".to_string(),
            created_at: Instant::now(),
            ttl: Duration::from_secs(60),
            hit_count: 0,
        };

        let entry2 = entry1.clone();
        let entry3 = entry1.clone();

        cache.insert("key1".to_string(), entry1);
        cache.insert("key2".to_string(), entry2);
        cache.insert("key3".to_string(), entry3);

        // key1 should be evicted
        assert_eq!(cache.len(), 2);
        assert!(cache.get("key1").is_none());
        assert!(cache.get("key2").is_some());
        assert!(cache.get("key3").is_some());
    }

    #[test]
    fn test_lru_cache_lru_order() {
        let mut cache = LruCache::new(2);

        let entry = CacheEntry {
            body: b"test".to_vec(),
            status: StatusCode::OK,
            headers: vec![],
            etag: "etag".to_string(),
            created_at: Instant::now(),
            ttl: Duration::from_secs(60),
            hit_count: 0,
        };

        cache.insert("key1".to_string(), entry.clone());
        cache.insert("key2".to_string(), entry.clone());

        // Access key1 to make it most recently used
        cache.get("key1");

        // Insert key3, should evict key2 (not key1)
        cache.insert("key3".to_string(), entry);

        assert!(cache.get("key1").is_some());
        assert!(cache.get("key2").is_none());
        assert!(cache.get("key3").is_some());
    }

    #[test]
    fn test_cache_entry_expiration() {
        let entry = CacheEntry {
            body: b"test".to_vec(),
            status: StatusCode::OK,
            headers: vec![],
            etag: "etag".to_string(),
            created_at: Instant::now() - Duration::from_secs(100),
            ttl: Duration::from_secs(10),
            hit_count: 0,
        };

        assert!(entry.is_expired());
        assert!(!entry.is_valid());
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut stats = CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);

        stats.hits = 7;
        stats.misses = 3;
        assert_eq!(stats.hit_rate(), 0.7);

        stats.hits = 0;
        stats.misses = 10;
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.max_entries, 1000);
        assert_eq!(config.default_ttl, Duration::from_secs(300));
        assert!(config.enable_etag);
        assert!(config.excluded_paths.contains(&"/health".to_string()));
    }

    #[test]
    fn test_cache_config_builder() {
        let config = CacheConfig::builder()
            .max_entries(500)
            .default_ttl(Duration::from_secs(60))
            .include_path("/api")
            .exclude_path("/admin")
            .enable_etag(false)
            .build();

        assert_eq!(config.max_entries, 500);
        assert_eq!(config.default_ttl, Duration::from_secs(60));
        assert!(config.included_paths.contains(&"/api".to_string()));
        assert!(config.excluded_paths.contains(&"/admin".to_string()));
        assert!(!config.enable_etag);
    }

    #[test]
    fn test_response_cache_is_cacheable_path() {
        let config = CacheConfig::builder()
            .exclude_path("/health")
            .exclude_path("/metrics")
            .build();

        let cache = ResponseCache::new(config);

        assert!(!cache.is_cacheable_path("/health"));
        assert!(!cache.is_cacheable_path("/metrics"));
        assert!(cache.is_cacheable_path("/api/users"));
    }

    #[test]
    fn test_response_cache_with_included_paths() {
        let config = CacheConfig::builder().include_path("/api").build();

        let cache = ResponseCache::new(config);

        assert!(cache.is_cacheable_path("/api/users"));
        assert!(!cache.is_cacheable_path("/admin"));
    }

    #[test]
    fn test_generate_etag() {
        let body1 = b"Hello, World!";
        let body2 = b"Hello, World!";
        let body3 = b"Different content";

        let etag1 = generate_etag(body1);
        let etag2 = generate_etag(body2);
        let etag3 = generate_etag(body3);

        // Same content should produce same ETag
        assert_eq!(etag1, etag2);
        // Different content should produce different ETag
        assert_ne!(etag1, etag3);
    }

    #[test]
    fn test_lru_cache_clear() {
        let mut cache = LruCache::new(5);

        let entry = CacheEntry {
            body: b"test".to_vec(),
            status: StatusCode::OK,
            headers: vec![],
            etag: "etag".to_string(),
            created_at: Instant::now(),
            ttl: Duration::from_secs(60),
            hit_count: 0,
        };

        cache.insert("key1".to_string(), entry.clone());
        cache.insert("key2".to_string(), entry);

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_lru_cache_cleanup_expired() {
        let mut cache = LruCache::new(5);

        let expired_entry = CacheEntry {
            body: b"test".to_vec(),
            status: StatusCode::OK,
            headers: vec![],
            etag: "etag".to_string(),
            created_at: Instant::now() - Duration::from_secs(100),
            ttl: Duration::from_secs(10),
            hit_count: 0,
        };

        let valid_entry = CacheEntry {
            body: b"test".to_vec(),
            status: StatusCode::OK,
            headers: vec![],
            etag: "etag".to_string(),
            created_at: Instant::now(),
            ttl: Duration::from_secs(60),
            hit_count: 0,
        };

        cache.insert("expired".to_string(), expired_entry);
        cache.insert("valid".to_string(), valid_entry);

        assert_eq!(cache.len(), 2);

        cache.cleanup_expired();

        assert_eq!(cache.len(), 1);
        assert!(cache.get("valid").is_some());
        assert!(cache.get("expired").is_none());
    }
}
