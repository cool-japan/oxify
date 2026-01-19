//! Redis-based distributed cache for LLM responses
//!
//! This module provides a Redis-backed cache implementation that allows
//! caching LLM responses across multiple instances of your application.
//! This is particularly useful for distributed systems where you want to
//! share cached responses between different servers or processes.
//!
//! # Features
//! - Distributed caching using Redis
//! - Configurable TTL (time-to-live)
//! - Automatic serialization/deserialization
//! - Connection pooling with redis ConnectionManager
//! - Cache hit/miss statistics
//! - Thread-safe with async/await support
//!
//! # Example
//! ```rust,no_run
//! use oxify_connect_llm::{RedisCachedProvider, OpenAIProvider, LlmProvider, LlmRequest};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = OpenAIProvider::new(
//!         "your-api-key".to_string(),
//!         "gpt-4".to_string(),
//!     );
//!
//!     // Wrap with Redis cache (1 hour TTL)
//!     let cached = RedisCachedProvider::new(
//!         Box::new(provider),
//!         "gpt-4".to_string(),
//!         "redis://127.0.0.1:6379",
//!         3600, // 1 hour TTL
//!     ).await?;
//!
//!     let request = LlmRequest {
//!         prompt: "Hello, world!".to_string(),
//!         system_prompt: None,
//!         temperature: Some(0.7),
//!         max_tokens: Some(100),
//!         tools: vec![],
//!         images: vec![],
//!     };
//!
//!     // First call hits the API, subsequent calls use cache
//!     let response = cached.complete(request.clone()).await?;
//!     println!("Response: {}", response.content);
//!
//!     // Check cache statistics
//!     let stats = cached.stats().await;
//!     println!("Cache hits: {}, misses: {}", stats.hits, stats.misses);
//!
//!     Ok(())
//! }
//! ```

#[cfg(feature = "redis-cache")]
use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmProvider, LlmRequest, LlmResponse,
    Result,
};
#[cfg(feature = "redis-cache")]
use async_trait::async_trait;
#[cfg(feature = "redis-cache")]
use redis::AsyncCommands;
#[cfg(feature = "redis-cache")]
use std::sync::Arc;
#[cfg(feature = "redis-cache")]
use tokio::sync::Mutex;

#[cfg(feature = "redis-cache")]
/// Statistics for Redis cache operations
#[derive(Debug, Clone, Default)]
pub struct RedisCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub errors: u64,
}

#[cfg(feature = "redis-cache")]
impl RedisCacheStats {
    /// Calculate cache hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[cfg(feature = "redis-cache")]
/// Redis cache for LLM responses
pub struct RedisCache {
    #[allow(dead_code)]
    client: redis::Client,
    connection: Arc<Mutex<redis::aio::ConnectionManager>>,
    ttl_seconds: u64,
    stats: Arc<Mutex<RedisCacheStats>>,
}

#[cfg(feature = "redis-cache")]
impl RedisCache {
    /// Create a new Redis cache
    ///
    /// # Arguments
    /// * `redis_url` - Redis connection URL (e.g., "redis://127.0.0.1:6379")
    /// * `ttl_seconds` - Time-to-live for cached entries in seconds (default: 3600 = 1 hour)
    pub async fn new(redis_url: &str, ttl_seconds: u64) -> Result<Self> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| crate::LlmError::Other(format!("Redis connection error: {}", e)))?;

        let connection = redis::aio::ConnectionManager::new(client.clone())
            .await
            .map_err(|e| crate::LlmError::Other(format!("Redis connection error: {}", e)))?;

        Ok(Self {
            client,
            connection: Arc::new(Mutex::new(connection)),
            ttl_seconds,
            stats: Arc::new(Mutex::new(RedisCacheStats::default())),
        })
    }

    /// Generate cache key from request
    fn cache_key(&self, model: &str, request: &LlmRequest) -> String {
        let request_str = serde_json::to_string(request).unwrap_or_default();
        let hash = format!("{:x}", md5::compute(request_str.as_bytes()));
        format!("llm:{}:{}", model, hash)
    }

    /// Generate cache key for embedding request
    fn embedding_cache_key(&self, model: &str, request: &EmbeddingRequest) -> String {
        let request_str = serde_json::to_string(request).unwrap_or_default();
        let hash = format!("{:x}", md5::compute(request_str.as_bytes()));
        format!("embedding:{}:{}", model, hash)
    }

    /// Get cached LLM response
    pub async fn get(&self, model: &str, request: &LlmRequest) -> Option<LlmResponse> {
        let key = self.cache_key(model, request);
        let mut conn = self.connection.lock().await;

        match conn.get::<_, String>(&key).await {
            Ok(cached) => {
                if let Ok(response) = serde_json::from_str::<LlmResponse>(&cached) {
                    let mut stats = self.stats.lock().await;
                    stats.hits += 1;
                    Some(response)
                } else {
                    let mut stats = self.stats.lock().await;
                    stats.errors += 1;
                    None
                }
            }
            Err(_) => {
                let mut stats = self.stats.lock().await;
                stats.misses += 1;
                None
            }
        }
    }

    /// Set cached LLM response
    pub async fn set(&self, model: &str, request: &LlmRequest, response: &LlmResponse) {
        let key = self.cache_key(model, request);
        if let Ok(value) = serde_json::to_string(response) {
            let mut conn = self.connection.lock().await;
            let _: std::result::Result<(), redis::RedisError> =
                conn.set_ex(&key, value, self.ttl_seconds).await;
        }
    }

    /// Get cached embedding response
    pub async fn get_embedding(
        &self,
        model: &str,
        request: &EmbeddingRequest,
    ) -> Option<EmbeddingResponse> {
        let key = self.embedding_cache_key(model, request);
        let mut conn = self.connection.lock().await;

        match conn.get::<_, String>(&key).await {
            Ok(cached) => {
                if let Ok(response) = serde_json::from_str::<EmbeddingResponse>(&cached) {
                    let mut stats = self.stats.lock().await;
                    stats.hits += 1;
                    Some(response)
                } else {
                    let mut stats = self.stats.lock().await;
                    stats.errors += 1;
                    None
                }
            }
            Err(_) => {
                let mut stats = self.stats.lock().await;
                stats.misses += 1;
                None
            }
        }
    }

    /// Set cached embedding response
    pub async fn set_embedding(
        &self,
        model: &str,
        request: &EmbeddingRequest,
        response: &EmbeddingResponse,
    ) {
        let key = self.embedding_cache_key(model, request);
        if let Ok(value) = serde_json::to_string(response) {
            let mut conn = self.connection.lock().await;
            let _: std::result::Result<(), redis::RedisError> =
                conn.set_ex(&key, value, self.ttl_seconds).await;
        }
    }

    /// Get cache statistics
    pub async fn stats(&self) -> RedisCacheStats {
        self.stats.lock().await.clone()
    }

    /// Clear all statistics
    pub async fn clear_stats(&self) {
        let mut stats = self.stats.lock().await;
        *stats = RedisCacheStats::default();
    }

    /// Clear all cached entries (use with caution!)
    pub async fn clear_all(&self) -> Result<()> {
        let mut conn = self.connection.lock().await;
        redis::cmd("FLUSHDB")
            .query_async::<()>(&mut *conn)
            .await
            .map_err(|e: redis::RedisError| {
                crate::LlmError::Other(format!("Redis error: {}", e))
            })?;
        Ok(())
    }
}

#[cfg(feature = "redis-cache")]
/// Redis-cached LLM provider wrapper
pub struct RedisCachedProvider {
    inner: Box<dyn LlmProvider>,
    model: String,
    cache: Arc<RedisCache>,
}

#[cfg(feature = "redis-cache")]
impl RedisCachedProvider {
    /// Create a new Redis-cached provider
    pub async fn new(
        inner: Box<dyn LlmProvider>,
        model: String,
        redis_url: &str,
        ttl_seconds: u64,
    ) -> Result<Self> {
        let cache = RedisCache::new(redis_url, ttl_seconds).await?;
        Ok(Self {
            inner,
            model,
            cache: Arc::new(cache),
        })
    }

    /// Get cache statistics
    pub async fn stats(&self) -> RedisCacheStats {
        self.cache.stats().await
    }

    /// Clear cache statistics
    pub async fn clear_stats(&self) {
        self.cache.clear_stats().await;
    }
}

#[cfg(feature = "redis-cache")]
#[async_trait]
impl LlmProvider for RedisCachedProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Try to get from cache first
        if let Some(cached) = self.cache.get(&self.model, &request).await {
            return Ok(cached);
        }

        // Cache miss - call the underlying provider
        let response = self.inner.complete(request.clone()).await?;

        // Store in cache for future requests
        self.cache.set(&self.model, &request, &response).await;

        Ok(response)
    }
}

#[cfg(feature = "redis-cache")]
/// Redis-cached embedding provider wrapper
pub struct RedisCachedEmbeddingProvider {
    inner: Box<dyn EmbeddingProvider>,
    model: String,
    cache: Arc<RedisCache>,
}

#[cfg(feature = "redis-cache")]
impl RedisCachedEmbeddingProvider {
    /// Create a new Redis-cached embedding provider
    pub async fn new(
        inner: Box<dyn EmbeddingProvider>,
        model: String,
        redis_url: &str,
        ttl_seconds: u64,
    ) -> Result<Self> {
        let cache = RedisCache::new(redis_url, ttl_seconds).await?;
        Ok(Self {
            inner,
            model,
            cache: Arc::new(cache),
        })
    }

    /// Get cache statistics
    pub async fn stats(&self) -> RedisCacheStats {
        self.cache.stats().await
    }

    /// Clear cache statistics
    pub async fn clear_stats(&self) {
        self.cache.clear_stats().await;
    }
}

#[cfg(feature = "redis-cache")]
#[async_trait]
impl EmbeddingProvider for RedisCachedEmbeddingProvider {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        // Try to get from cache first
        if let Some(cached) = self.cache.get_embedding(&self.model, &request).await {
            return Ok(cached);
        }

        // Cache miss - call the underlying provider
        let response = self.inner.embed(request.clone()).await?;

        // Store in cache for future requests
        self.cache
            .set_embedding(&self.model, &request, &response)
            .await;

        Ok(response)
    }
}

#[cfg(all(test, feature = "redis-cache"))]
mod tests {
    use super::*;
    use crate::{OpenAIProvider, Usage};

    #[tokio::test]
    #[ignore] // Requires Redis server running
    async fn test_redis_cache_basic() {
        let cache = RedisCache::new("redis://127.0.0.1:6379", 60).await.unwrap();

        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: vec![],
            images: vec![],
        };

        let response = LlmResponse {
            content: "Test response".to_string(),
            model: "gpt-4".to_string(),
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
            tool_calls: vec![],
        };

        // First get should be a miss
        assert!(cache.get("gpt-4", &request).await.is_none());

        // Set the value
        cache.set("gpt-4", &request, &response).await;

        // Second get should be a hit
        let cached = cache.get("gpt-4", &request).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().content, "Test response");

        // Check stats
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    #[ignore] // Requires Redis server running
    async fn test_redis_cache_stats() {
        let cache = RedisCache::new("redis://127.0.0.1:6379", 60).await.unwrap();

        let request = LlmRequest {
            prompt: "Test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
        };

        // Multiple misses
        cache.get("gpt-4", &request).await;
        cache.get("gpt-4", &request).await;

        let stats = cache.stats().await;
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[tokio::test]
    #[ignore] // Requires Redis server running
    async fn test_redis_cached_provider() {
        let provider = OpenAIProvider::new("test_key".to_string(), "gpt-4".to_string());

        let cached = RedisCachedProvider::new(
            Box::new(provider),
            "gpt-4".to_string(),
            "redis://127.0.0.1:6379",
            60,
        )
        .await
        .unwrap();

        let stats = cached.stats().await;
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }
}
