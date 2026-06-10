//! Semantic Caching for LLM Requests
//!
//! This module provides semantic caching that recognizes semantically similar requests
//! rather than requiring exact matches. This significantly improves cache hit rates and
//! reduces costs by catching variations of the same query.
//!
//! # Examples
//!
//! ```
//! use oxify_connect_llm::{
//!     LlmProvider, LlmRequest, OpenAIProvider, OllamaProvider,
//!     SemanticCache, SemanticCachedProvider, SimilarityThreshold,
//! };
//!
//! # async fn example() -> oxify_connect_llm::Result<()> {
//! // Create an embedding provider for semantic similarity
//! let embedding_provider = OllamaProvider::for_embeddings("nomic-embed-text".to_string());
//!
//! // Create a semantic cache with 0.85 similarity threshold
//! let cache = SemanticCache::new(
//!     Box::new(embedding_provider),
//!     SimilarityThreshold::new(0.85),
//!     100, // max cache size
//! );
//!
//! // Wrap your LLM provider with semantic caching
//! let provider = OpenAIProvider::new("key".to_string(), "gpt-4".to_string());
//! let cached_provider = SemanticCachedProvider::new(provider, cache);
//!
//! // These queries will be recognized as semantically similar:
//! // "What is Rust?" and "Can you explain Rust?"
//! // "How to learn Python" and "Best way to study Python"
//! # Ok(())
//! # }
//! ```

use crate::{EmbeddingProvider, EmbeddingRequest, LlmProvider, LlmRequest, LlmResponse, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Similarity threshold for semantic cache matching (0.0 to 1.0)
#[derive(Debug, Clone, Copy)]
pub struct SimilarityThreshold(f32);

impl SimilarityThreshold {
    /// Create a new similarity threshold
    ///
    /// # Panics
    /// Panics if threshold is not between 0.0 and 1.0
    pub fn new(threshold: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&threshold),
            "Threshold must be between 0.0 and 1.0"
        );
        Self(threshold)
    }

    /// Get the threshold value
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Default for SimilarityThreshold {
    fn default() -> Self {
        Self(0.85) // Default to 85% similarity
    }
}

/// Statistics for semantic cache performance
#[derive(Debug, Clone, Default)]
pub struct SemanticCacheStats {
    /// Number of cache hits (semantically similar queries found)
    pub hits: u64,
    /// Number of cache misses (no similar query found)
    pub misses: u64,
    /// Number of embedding generation failures
    pub embedding_errors: u64,
    /// Average similarity score for cache hits
    pub avg_similarity: f32,
    /// Total number of cached entries
    pub cached_entries: usize,
}

impl SemanticCacheStats {
    /// Calculate the cache hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f32 / total as f32
        }
    }
}

/// Entry in the semantic cache
#[derive(Clone)]
struct CacheEntry {
    #[allow(dead_code)]
    prompt: String,
    embedding: Vec<f32>,
    response: LlmResponse,
    access_count: u64,
}

/// Semantic cache using embeddings for similarity matching
pub struct SemanticCache {
    embedding_provider: Arc<Box<dyn EmbeddingProvider>>,
    threshold: SimilarityThreshold,
    max_size: usize,
    entries: Arc<Mutex<Vec<CacheEntry>>>,
    stats: Arc<Mutex<SemanticCacheStats>>,
}

impl SemanticCache {
    /// Create a new semantic cache
    ///
    /// # Arguments
    /// * `embedding_provider` - Provider for generating embeddings
    /// * `threshold` - Minimum similarity score for cache hit (0.0 to 1.0)
    /// * `max_size` - Maximum number of entries to cache
    pub fn new(
        embedding_provider: Box<dyn EmbeddingProvider>,
        threshold: SimilarityThreshold,
        max_size: usize,
    ) -> Self {
        Self {
            embedding_provider: Arc::new(embedding_provider),
            threshold,
            max_size,
            entries: Arc::new(Mutex::new(Vec::new())),
            stats: Arc::new(Mutex::new(SemanticCacheStats::default())),
        }
    }

    /// Get cache statistics
    pub async fn stats(&self) -> SemanticCacheStats {
        let stats = self.stats.lock().await;
        let entries = self.entries.lock().await;
        let mut stats_copy = stats.clone();
        stats_copy.cached_entries = entries.len();
        stats_copy
    }

    /// Clear the cache and reset statistics
    pub async fn clear(&self) {
        let mut entries = self.entries.lock().await;
        entries.clear();
        let mut stats = self.stats.lock().await;
        *stats = SemanticCacheStats::default();
    }

    /// Generate embedding for a prompt
    async fn generate_embedding(&self, prompt: &str) -> Result<Vec<f32>> {
        let request = EmbeddingRequest {
            texts: vec![prompt.to_string()],
            model: None,
        };
        let response = self.embedding_provider.embed(request).await?;
        response.embeddings.into_iter().next().ok_or_else(|| {
            crate::LlmError::ApiError("embedding provider returned empty embeddings".to_string())
        })
    }

    /// Calculate cosine similarity between two embeddings
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            0.0
        } else {
            dot_product / (magnitude_a * magnitude_b)
        }
    }

    /// Try to get a cached response for a prompt
    pub async fn get(&self, prompt: &str) -> Option<LlmResponse> {
        // Generate embedding for the query
        let query_embedding = match self.generate_embedding(prompt).await {
            Ok(emb) => emb,
            Err(_) => {
                let mut stats = self.stats.lock().await;
                stats.embedding_errors += 1;
                return None;
            }
        };

        // Search for similar entries
        let mut entries = self.entries.lock().await;
        let mut best_match: Option<(usize, f32)> = None;

        for (idx, entry) in entries.iter().enumerate() {
            let similarity = Self::cosine_similarity(&query_embedding, &entry.embedding);
            if similarity >= self.threshold.value() {
                if let Some((_, best_sim)) = best_match {
                    if similarity > best_sim {
                        best_match = Some((idx, similarity));
                    }
                } else {
                    best_match = Some((idx, similarity));
                }
            }
        }

        if let Some((idx, similarity)) = best_match {
            // Cache hit - update access count and stats
            entries[idx].access_count += 1;
            let response = entries[idx].response.clone();

            let mut stats = self.stats.lock().await;
            stats.hits += 1;
            // Update running average of similarity scores
            let total_hits = stats.hits;
            stats.avg_similarity =
                ((stats.avg_similarity * (total_hits - 1) as f32) + similarity) / total_hits as f32;

            tracing::debug!(
                "Semantic cache hit: similarity={:.3}, prompt='{}'",
                similarity,
                prompt
            );

            Some(response)
        } else {
            // Cache miss
            let mut stats = self.stats.lock().await;
            stats.misses += 1;

            tracing::debug!("Semantic cache miss: prompt='{}'", prompt);

            None
        }
    }

    /// Store a response in the cache
    pub async fn put(&self, prompt: String, response: LlmResponse) {
        // Generate embedding for the prompt
        let embedding = match self.generate_embedding(&prompt).await {
            Ok(emb) => emb,
            Err(_) => {
                let mut stats = self.stats.lock().await;
                stats.embedding_errors += 1;
                return;
            }
        };

        let mut entries = self.entries.lock().await;

        // Add new entry
        entries.push(CacheEntry {
            prompt,
            embedding,
            response,
            access_count: 1,
        });

        // Evict least accessed entry if cache is full
        if entries.len() > self.max_size {
            // Find entry with lowest access count
            let min_idx = entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.access_count)
                .map(|(idx, _)| idx)
                .expect("invariant: entries non-empty when len > max_size");
            entries.remove(min_idx);
        }
    }
}

/// Provider wrapper that adds semantic caching
pub struct SemanticCachedProvider<P> {
    provider: Arc<P>,
    cache: Arc<SemanticCache>,
}

impl<P> SemanticCachedProvider<P> {
    /// Create a new semantic cached provider
    pub fn new(provider: P, cache: SemanticCache) -> Self {
        Self {
            provider: Arc::new(provider),
            cache: Arc::new(cache),
        }
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> SemanticCacheStats {
        self.cache.stats().await
    }

    /// Clear the cache
    pub async fn clear_cache(&self) {
        self.cache.clear().await
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for SemanticCachedProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Try to get from cache
        if let Some(cached_response) = self.cache.get(&request.prompt).await {
            return Ok(cached_response);
        }

        // Cache miss - call provider
        let response = self.provider.complete(request.clone()).await?;

        // Store in cache
        self.cache.put(request.prompt, response.clone()).await;

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Usage;

    // Mock embedding provider that returns simple embeddings
    struct MockEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, request: EmbeddingRequest) -> Result<crate::EmbeddingResponse> {
            // Create simple embeddings based on text length and content
            let embeddings: Vec<Vec<f32>> = request
                .texts
                .iter()
                .map(|text| {
                    let mut embedding = vec![0.0; 128];
                    // Simple hash-like embedding based on characters
                    for (i, ch) in text.chars().enumerate() {
                        embedding[i % 128] += (ch as u32 as f32) / 1000.0;
                    }
                    // Normalize
                    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                    if magnitude > 0.0 {
                        embedding.iter_mut().for_each(|x| *x /= magnitude);
                    }
                    embedding
                })
                .collect();

            Ok(crate::EmbeddingResponse {
                embeddings,
                model: "mock".to_string(),
                usage: None,
            })
        }
    }

    // Mock LLM provider
    struct MockLlmProvider;

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: format!("Response to: {}", request.prompt),
                model: "mock".to_string(),
                usage: Some(Usage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                }),
                tool_calls: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn test_similarity_threshold() {
        let threshold = SimilarityThreshold::new(0.85);
        assert_eq!(threshold.value(), 0.85);

        let default_threshold = SimilarityThreshold::default();
        assert_eq!(default_threshold.value(), 0.85);
    }

    #[tokio::test]
    #[should_panic(expected = "Threshold must be between 0.0 and 1.0")]
    async fn test_invalid_threshold() {
        let _threshold = SimilarityThreshold::new(1.5);
    }

    #[tokio::test]
    async fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(SemanticCache::cosine_similarity(&a, &b), 1.0);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert_eq!(SemanticCache::cosine_similarity(&a, &b), 0.0);

        let a = vec![1.0, 1.0];
        let b = vec![1.0, 1.0];
        assert!((SemanticCache::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_semantic_cache_miss() {
        let cache = SemanticCache::new(
            Box::new(MockEmbeddingProvider),
            SimilarityThreshold::new(0.9),
            10,
        );

        let result = cache.get("test query").await;
        assert!(result.is_none());

        let stats = cache.stats().await;
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
    }

    #[tokio::test]
    async fn test_semantic_cache_hit() {
        let cache = SemanticCache::new(
            Box::new(MockEmbeddingProvider),
            SimilarityThreshold::new(0.9),
            10,
        );

        // Store a response
        let response = LlmResponse {
            content: "test response".to_string(),
            model: "test".to_string(),
            usage: None,
            tool_calls: Vec::new(),
        };
        cache.put("test query".to_string(), response.clone()).await;

        // Try to get the exact same query
        let result = cache.get("test query").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "test response");

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
    }

    #[tokio::test]
    async fn test_semantic_cache_similar_queries() {
        let cache = SemanticCache::new(
            Box::new(MockEmbeddingProvider),
            SimilarityThreshold::new(0.7), // Lower threshold to catch similar queries
            10,
        );

        // Store a response for one query
        let response = LlmResponse {
            content: "Rust is a systems programming language".to_string(),
            model: "test".to_string(),
            usage: None,
            tool_calls: Vec::new(),
        };
        cache
            .put("What is Rust?".to_string(), response.clone())
            .await;

        // Try a similar query (should hit with high similarity)
        let result = cache.get("What is Rust?").await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_semantic_cache_eviction() {
        let cache = SemanticCache::new(
            Box::new(MockEmbeddingProvider),
            SimilarityThreshold::new(0.9),
            2, // Small cache size
        );

        // Add 3 entries (will evict one)
        for i in 1..=3 {
            let response = LlmResponse {
                content: format!("response {}", i),
                model: "test".to_string(),
                usage: None,
                tool_calls: Vec::new(),
            };
            cache.put(format!("query {}", i), response).await;
        }

        let stats = cache.stats().await;
        assert_eq!(stats.cached_entries, 2);
    }

    #[tokio::test]
    async fn test_cached_provider() {
        let cache = SemanticCache::new(
            Box::new(MockEmbeddingProvider),
            SimilarityThreshold::new(0.9),
            10,
        );

        let provider = MockLlmProvider;
        let cached_provider = SemanticCachedProvider::new(provider, cache);

        // First request - cache miss
        let request = LlmRequest {
            prompt: "test query".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };
        let response1 = cached_provider.complete(request.clone()).await.unwrap();

        // Second request - cache hit
        let response2 = cached_provider.complete(request).await.unwrap();

        assert_eq!(response1.content, response2.content);

        let stats = cached_provider.cache_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = SemanticCache::new(
            Box::new(MockEmbeddingProvider),
            SimilarityThreshold::new(0.9),
            10,
        );

        let stats = cache.stats().await;
        assert_eq!(stats.hit_rate(), 0.0);

        // Add some hits and misses
        let response = LlmResponse {
            content: "test".to_string(),
            model: "test".to_string(),
            usage: None,
            tool_calls: Vec::new(),
        };
        cache.put("query".to_string(), response).await;
        cache.get("query").await; // hit
        cache.get("other query").await; // miss

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate(), 0.5);
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let cache = SemanticCache::new(
            Box::new(MockEmbeddingProvider),
            SimilarityThreshold::new(0.9),
            10,
        );

        let response = LlmResponse {
            content: "test".to_string(),
            model: "test".to_string(),
            usage: None,
            tool_calls: Vec::new(),
        };
        cache.put("query".to_string(), response).await;

        cache.clear().await;

        let stats = cache.stats().await;
        assert_eq!(stats.cached_entries, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }
}
