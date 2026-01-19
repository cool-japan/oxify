//! Caching for OCR results.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::types::OcrResult;

/// Cache key for OCR results.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CacheKey {
    /// Hash of the image data.
    image_hash: u64,
    /// Provider name.
    provider: String,
    /// Output format.
    output_format: String,
    /// Language setting.
    language: Option<String>,
}

impl CacheKey {
    /// Create a new cache key.
    pub fn new(
        image_data: &[u8],
        provider: &str,
        output_format: &str,
        language: Option<&str>,
    ) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        image_data.hash(&mut hasher);
        let image_hash = hasher.finish();

        Self {
            image_hash,
            provider: provider.to_string(),
            output_format: output_format.to_string(),
            language: language.map(|s| s.to_string()),
        }
    }
}

/// Cached OCR result with expiration.
#[derive(Debug, Clone)]
struct CachedEntry {
    result: OcrResult,
    created_at: Instant,
    ttl: Duration,
}

impl CachedEntry {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// In-memory cache for OCR results.
#[derive(Debug, Clone)]
pub struct VisionCache {
    cache: Arc<Mutex<HashMap<CacheKey, CachedEntry>>>,
    default_ttl: Duration,
    max_size: usize,
}

impl Default for VisionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl VisionCache {
    /// Create a new cache with default settings.
    /// Default TTL: 1 hour, Max size: 100 entries.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            default_ttl: Duration::from_secs(3600),
            max_size: 100,
        }
    }

    /// Create a cache with custom TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            default_ttl: ttl,
            max_size: 100,
        }
    }

    /// Create a cache with custom max size.
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            default_ttl: Duration::from_secs(3600),
            max_size,
        }
    }

    /// Get a cached result.
    pub fn get(&self, key: &CacheKey) -> Option<OcrResult> {
        let mut cache = self.cache.lock().unwrap();

        if let Some(entry) = cache.get(key) {
            if entry.is_expired() {
                cache.remove(key);
                return None;
            }
            return Some(entry.result.clone());
        }

        None
    }

    /// Store a result in the cache.
    pub fn set(&self, key: CacheKey, result: OcrResult) {
        self.set_with_ttl(key, result, self.default_ttl);
    }

    /// Store a result with custom TTL.
    pub fn set_with_ttl(&self, key: CacheKey, result: OcrResult, ttl: Duration) {
        let mut cache = self.cache.lock().unwrap();

        // Evict expired entries if at capacity
        if cache.len() >= self.max_size {
            self.evict_expired(&mut cache);
        }

        // If still at capacity, remove oldest entry
        if cache.len() >= self.max_size {
            if let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, v)| v.created_at)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&oldest_key);
            }
        }

        cache.insert(
            key,
            CachedEntry {
                result,
                created_at: Instant::now(),
                ttl,
            },
        );
    }

    /// Remove expired entries from the cache.
    fn evict_expired(&self, cache: &mut HashMap<CacheKey, CachedEntry>) {
        cache.retain(|_, entry| !entry.is_expired());
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Get the number of cached entries.
    pub fn len(&self) -> usize {
        let cache = self.cache.lock().unwrap();
        cache.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap();
        let total = cache.len();
        let expired = cache.values().filter(|e| e.is_expired()).count();

        CacheStats {
            total_entries: total,
            expired_entries: expired,
            active_entries: total - expired,
            max_size: self.max_size,
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
    pub max_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let cache = VisionCache::new();
        let key = CacheKey::new(b"test image", "mock", "markdown", None);
        let result = OcrResult::from_text("Hello");

        cache.set(key.clone(), result.clone());

        let cached = cache.get(&key).unwrap();
        assert_eq!(cached.text, "Hello");
    }

    #[test]
    fn test_cache_miss() {
        let cache = VisionCache::new();
        let key = CacheKey::new(b"test image", "mock", "markdown", None);

        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_expiration() {
        let cache = VisionCache::with_ttl(Duration::from_millis(1));
        let key = CacheKey::new(b"test image", "mock", "markdown", None);
        let result = OcrResult::from_text("Hello");

        cache.set(key.clone(), result);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(10));

        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_max_size() {
        let cache = VisionCache::with_max_size(2);

        for i in 0..5 {
            let key = CacheKey::new(format!("image{}", i).as_bytes(), "mock", "markdown", None);
            cache.set(key, OcrResult::from_text(format!("Result {}", i)));
        }

        // Should only have 2 entries
        assert!(cache.len() <= 2);
    }

    #[test]
    fn test_cache_clear() {
        let cache = VisionCache::new();
        let key = CacheKey::new(b"test image", "mock", "markdown", None);
        cache.set(key, OcrResult::from_text("Hello"));

        cache.clear();
        assert!(cache.is_empty());
    }
}
