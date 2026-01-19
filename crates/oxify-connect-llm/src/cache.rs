//! LLM response caching for cost optimization

use crate::{LlmProvider, LlmRequest, LlmResponse, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Cache key for LLM requests
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    model: String,
    prompt: String,
    system_prompt: Option<String>,
    temperature: Option<u32>, // Store as u32 (temperature * 1000) for hashing
    max_tokens: Option<u32>,
}

impl CacheKey {
    fn from_request(request: &LlmRequest, model: &str) -> Self {
        Self {
            model: model.to_string(),
            prompt: request.prompt.clone(),
            system_prompt: request.system_prompt.clone(),
            temperature: request.temperature.map(|t| (t * 1000.0) as u32),
            max_tokens: request.max_tokens,
        }
    }
}

/// Cached response with expiration
#[derive(Debug, Clone)]
struct CachedResponse {
    response: LlmResponse,
    inserted_at: Instant,
    ttl: Duration,
}

impl CachedResponse {
    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() > self.ttl
    }
}

/// In-memory LLM response cache
#[derive(Debug)]
pub struct LlmCache {
    cache: Arc<Mutex<HashMap<CacheKey, CachedResponse>>>,
    default_ttl: Duration,
    max_size: usize,
    hits: Arc<AtomicU64>,
    misses: Arc<AtomicU64>,
}

impl Clone for LlmCache {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
            default_ttl: self.default_ttl,
            max_size: self.max_size,
            hits: Arc::clone(&self.hits),
            misses: Arc::clone(&self.misses),
        }
    }
}

impl Default for LlmCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmCache {
    /// Create a new cache with default settings
    /// - TTL: 1 hour
    /// - Max size: 1000 entries
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            default_ttl: Duration::from_secs(3600), // 1 hour
            max_size: 1000,
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new cache with custom settings
    pub fn with_config(ttl: Duration, max_size: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            default_ttl: ttl,
            max_size,
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get a cached response if it exists and hasn't expired
    pub fn get(&self, request: &LlmRequest, model: &str) -> Option<LlmResponse> {
        let key = CacheKey::from_request(request, model);
        let mut cache = self.cache.lock().unwrap();

        if let Some(cached) = cache.get(&key) {
            if !cached.is_expired() {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Some(cached.response.clone());
            } else {
                // Remove expired entry
                cache.remove(&key);
            }
        }

        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Store a response in the cache
    pub fn put(&self, request: &LlmRequest, model: &str, response: LlmResponse) {
        let key = CacheKey::from_request(request, model);
        let mut cache = self.cache.lock().unwrap();

        // Evict oldest entry if cache is full (simple FIFO)
        if cache.len() >= self.max_size {
            if let Some(oldest_key) = cache.keys().next().cloned() {
                cache.remove(&oldest_key);
            }
        }

        cache.insert(
            key,
            CachedResponse {
                response,
                inserted_at: Instant::now(),
                ttl: self.default_ttl,
            },
        );
    }

    /// Clear all cached entries
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Remove expired entries
    pub fn cleanup(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|_, v| !v.is_expired());
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap();
        let total = cache.len();
        let expired = cache.values().filter(|v| v.is_expired()).count();
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);

        CacheStats {
            total_entries: total,
            expired_entries: expired,
            active_entries: total - expired,
            hits,
            misses,
        }
    }

    /// Reset hit/miss counters
    pub fn reset_stats(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
    pub hits: u64,
    pub misses: u64,
}

impl CacheStats {
    /// Calculate hit rate as a percentage (0.0 to 100.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }
}

// ===== CachedProvider Wrapper =====

/// A wrapper that adds caching functionality to any LLM provider
pub struct CachedProvider<P> {
    inner: P,
    cache: LlmCache,
    model_name: String,
}

impl<P> CachedProvider<P> {
    /// Create a new CachedProvider with default cache settings
    pub fn new(provider: P, model_name: String) -> Self {
        Self {
            inner: provider,
            cache: LlmCache::new(),
            model_name,
        }
    }

    /// Create a new CachedProvider with a custom cache
    pub fn with_cache(provider: P, model_name: String, cache: LlmCache) -> Self {
        Self {
            inner: provider,
            cache,
            model_name,
        }
    }

    /// Get a reference to the inner provider
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Get a mutable reference to the inner provider
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }

    /// Get a reference to the cache
    pub fn cache(&self) -> &LlmCache {
        &self.cache
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for CachedProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Check cache first
        if let Some(cached) = self.cache.get(&request, &self.model_name) {
            tracing::debug!(
                model = %self.model_name,
                "Cache hit for LLM request"
            );
            return Ok(cached);
        }

        // Cache miss - call the inner provider
        let response = self.inner.complete(request.clone()).await?;

        // Store in cache
        self.cache.put(&request, &self.model_name, response.clone());

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Usage;

    #[test]
    fn test_cache_hit() {
        let cache = LlmCache::new();

        let request = LlmRequest {
            prompt: "Hello".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: Vec::new(),
            images: Vec::new(),
        };

        let response = LlmResponse {
            content: "Hi there!".to_string(),
            model: "gpt-4".to_string(),
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
            tool_calls: Vec::new(),
        };

        // Cache miss
        assert!(cache.get(&request, "gpt-4").is_none());

        // Store in cache
        cache.put(&request, "gpt-4", response.clone());

        // Cache hit
        let cached = cache.get(&request, "gpt-4").unwrap();
        assert_eq!(cached.content, response.content);
    }

    #[test]
    fn test_cache_expiration() {
        let cache = LlmCache::with_config(Duration::from_millis(500), 100);

        let request = LlmRequest {
            prompt: "Hello".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: Vec::new(),
            images: Vec::new(),
        };

        let response = LlmResponse {
            content: "Hi there!".to_string(),
            model: "gpt-4".to_string(),
            usage: None,
            tool_calls: Vec::new(),
        };

        cache.put(&request, "gpt-4", response);

        // Should be in cache immediately
        assert!(cache.get(&request, "gpt-4").is_some());

        // Wait for expiration (sleep longer than TTL to ensure expiration)
        std::thread::sleep(Duration::from_millis(600));

        // Should be expired
        assert!(cache.get(&request, "gpt-4").is_none());
    }

    #[test]
    fn test_cache_max_size() {
        let cache = LlmCache::with_config(Duration::from_secs(3600), 3);

        for i in 0..5 {
            let request = LlmRequest {
                prompt: format!("Prompt {}", i),
                system_prompt: None,
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: Vec::new(),
                images: Vec::new(),
            };

            let response = LlmResponse {
                content: format!("Response {}", i),
                model: "gpt-4".to_string(),
                usage: None,
                tool_calls: Vec::new(),
            };

            cache.put(&request, "gpt-4", response);
        }

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 3); // Max size enforced
    }

    #[test]
    fn test_cache_cleanup() {
        let cache = LlmCache::with_config(Duration::from_millis(10), 100);

        // Add some entries that will expire
        for i in 0..5 {
            let request = LlmRequest {
                prompt: format!("Prompt {}", i),
                system_prompt: None,
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: Vec::new(),
                images: Vec::new(),
            };

            let response = LlmResponse {
                content: format!("Response {}", i),
                model: "gpt-4".to_string(),
                usage: None,
                tool_calls: Vec::new(),
            };

            cache.put(&request, "gpt-4", response);
        }

        assert_eq!(cache.stats().total_entries, 5);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(20));

        // Cleanup
        cache.cleanup();

        assert_eq!(cache.stats().total_entries, 0);
    }

    #[test]
    fn test_cache_hit_rate() {
        let cache = LlmCache::new();

        let request = LlmRequest {
            prompt: "Hello".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: Vec::new(),
            images: Vec::new(),
        };

        let response = LlmResponse {
            content: "Hi there!".to_string(),
            model: "gpt-4".to_string(),
            usage: None,
            tool_calls: Vec::new(),
        };

        // First get is a miss
        cache.get(&request, "gpt-4");
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().hit_rate(), 0.0);

        // Store in cache
        cache.put(&request, "gpt-4", response);

        // Second get is a hit
        cache.get(&request, "gpt-4");
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hit_rate(), 50.0);

        // Third get is also a hit
        cache.get(&request, "gpt-4");
        assert_eq!(cache.stats().hits, 2);
        assert_eq!(cache.stats().misses, 1);

        // Hit rate should be approximately 66.67%
        let hit_rate = cache.stats().hit_rate();
        assert!(hit_rate > 66.0 && hit_rate < 67.0);

        // Reset stats
        cache.reset_stats();
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().misses, 0);
    }
}
