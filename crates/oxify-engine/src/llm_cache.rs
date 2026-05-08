//! LLM response caching for the engine

use oxify_connect_llm::{LlmRequest, LlmResponse};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Cache key for LLM requests
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    provider: String,
    model: String,
    prompt: String,
    system_prompt: Option<String>,
    temperature: Option<u32>, // Store as u32 (temperature * 1000) for hashing
    max_tokens: Option<u32>,
}

impl CacheKey {
    fn from_request(provider: &str, model: &str, request: &LlmRequest) -> Self {
        Self {
            provider: provider.to_string(),
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
}

impl CachedResponse {
    fn is_expired(&self, ttl: Duration) -> bool {
        self.inserted_at.elapsed() > ttl
    }
}

/// Global LLM response cache
pub struct EngineLlmCache {
    cache: Arc<Mutex<HashMap<CacheKey, CachedResponse>>>,
    ttl: Duration,
    max_size: usize,
}

impl Default for EngineLlmCache {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineLlmCache {
    /// Create a new cache with default settings (1 hour TTL, 1000 entries max)
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(3600),
            max_size: 1000,
        }
    }

    /// Get a cached response if it exists and hasn't expired
    pub fn get(&self, provider: &str, model: &str, request: &LlmRequest) -> Option<LlmResponse> {
        let key = CacheKey::from_request(provider, model, request);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(cached) = cache.get(&key) {
            if !cached.is_expired(self.ttl) {
                return Some(cached.response.clone());
            } else {
                // Remove expired entry
                cache.remove(&key);
            }
        }

        None
    }

    /// Store a response in the cache
    pub fn put(&self, provider: &str, model: &str, request: &LlmRequest, response: LlmResponse) {
        let key = CacheKey::from_request(provider, model, request);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());

        // Simple eviction: remove first entry if cache is full
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
            },
        );
    }

    /// Get cache size
    #[allow(dead_code)]
    pub fn size(&self) -> usize {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}
