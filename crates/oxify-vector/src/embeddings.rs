//! Embedding Management
//!
//! Provides embedding generation from various providers (OpenAI, local models)
//! with caching and batch processing support.
//!
//! ## Features
//!
//! - **Multiple Providers**: OpenAI, local models, custom implementations
//! - **Caching**: TTL-based cache to reduce redundant API calls
//! - **Batch Processing**: Efficient batch embedding generation
//! - **Async Support**: Non-blocking embedding generation
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::embeddings::{EmbeddingProvider, MockEmbeddingProvider};
//!
//! # fn example() -> anyhow::Result<()> {
//! // Use mock provider for testing
//! let provider = MockEmbeddingProvider::new(384);
//!
//! let text = "Hello, world!";
//! let embedding = provider.embed(text)?;
//!
//! assert_eq!(embedding.len(), 384);
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// Trait for embedding providers
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts (batch processing)
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|text| self.embed(text)).collect()
    }

    /// Get the dimension of embeddings produced by this provider
    fn dimension(&self) -> usize;

    /// Get the provider name
    fn name(&self) -> &str;
}

/// Mock embedding provider for testing
///
/// Generates deterministic embeddings based on text hash
#[derive(Debug, Clone)]
pub struct MockEmbeddingProvider {
    dimension: usize,
}

impl MockEmbeddingProvider {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    fn hash_text(&self, text: &str) -> u64 {
        // Simple hash function for deterministic embeddings
        let mut hash: u64 = 5381;
        for byte in text.as_bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(*byte as u64);
        }
        hash
    }
}

impl EmbeddingProvider for MockEmbeddingProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let hash = self.hash_text(text);
        let mut embedding = Vec::with_capacity(self.dimension);

        for i in 0..self.dimension {
            let val = ((hash.wrapping_add(i as u64) % 1000) as f32) / 1000.0;
            embedding.push(val);
        }

        // Normalize to unit vector
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        "mock"
    }
}

/// OpenAI embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key for OpenAI
    pub api_key: String,
    /// Model name (e.g., "text-embedding-ada-002", "text-embedding-3-small")
    pub model: String,
    /// API endpoint (defaults to OpenAI's endpoint)
    pub endpoint: Option<String>,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "text-embedding-3-small".to_string(),
            endpoint: None,
        }
    }
}

/// OpenAI embedding provider (stub for future implementation)
///
/// Note: Requires `reqwest` dependency for actual API calls
/// This is a placeholder implementation
#[derive(Debug, Clone)]
pub struct OpenAIEmbeddingProvider {
    #[allow(dead_code)]
    config: OpenAIConfig,
    dimension: usize,
}

impl OpenAIEmbeddingProvider {
    pub fn new(config: OpenAIConfig) -> Result<Self> {
        // Determine dimension based on model
        let dimension = match config.model.as_str() {
            "text-embedding-ada-002" => 1536,
            "text-embedding-3-small" => 1536,
            "text-embedding-3-large" => 3072,
            _ => anyhow::bail!("Unknown OpenAI model: {}", config.model),
        };

        Ok(Self { config, dimension })
    }
}

impl EmbeddingProvider for OpenAIEmbeddingProvider {
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        // Placeholder: In production, this would make an HTTP request to OpenAI API
        // For now, return a mock embedding
        anyhow::bail!(
            "OpenAI provider requires HTTP client implementation (add reqwest dependency)"
        )
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        "openai"
    }
}

/// Cache entry with TTL
#[derive(Debug, Clone)]
struct CacheEntry {
    embedding: Vec<f32>,
    created_at: SystemTime,
    seq: u64,
}

/// Embedding cache with TTL
#[derive(Debug, Clone)]
pub struct EmbeddingCache {
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
    ttl: Duration,
    max_entries: usize,
    next_seq: Arc<AtomicU64>,
}

impl EmbeddingCache {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl,
            max_entries,
            next_seq: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get embedding from cache if available and not expired
    pub fn get(&self, text: &str) -> Option<Vec<f32>> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = cache.get(text) {
            let elapsed = SystemTime::now()
                .duration_since(entry.created_at)
                .unwrap_or(Duration::MAX);

            if elapsed < self.ttl {
                return Some(entry.embedding.clone());
            }
        }
        None
    }

    /// Store embedding in cache
    pub fn put(&self, text: String, embedding: Vec<f32>) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());

        // Evict oldest entries if cache is full
        if cache.len() >= self.max_entries {
            self.evict_oldest(&mut cache);
        }

        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        cache.insert(
            text,
            CacheEntry {
                embedding,
                created_at: SystemTime::now(),
                seq,
            },
        );
    }

    /// Clear all entries from cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.clear();
    }

    /// Get cache size
    pub fn size(&self) -> usize {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.len()
    }

    fn evict_oldest(&self, cache: &mut HashMap<String, CacheEntry>) {
        if cache.is_empty() {
            return;
        }

        // Find entry with lowest seq (oldest insertion order)
        let oldest_key = cache
            .iter()
            .min_by_key(|(_, entry)| entry.seq)
            .map(|(key, _)| key.clone());

        if let Some(key) = oldest_key {
            cache.remove(&key);
        }
    }
}

impl Default for EmbeddingCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(3600), 10000) // 1 hour TTL, 10k entries
    }
}

/// Cached embedding provider
///
/// Wraps any embedding provider with caching
pub struct CachedEmbeddingProvider<P: EmbeddingProvider> {
    provider: P,
    cache: EmbeddingCache,
}

impl<P: EmbeddingProvider> CachedEmbeddingProvider<P> {
    pub fn new(provider: P, cache: EmbeddingCache) -> Self {
        Self { provider, cache }
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    pub fn cache_size(&self) -> usize {
        self.cache.size()
    }
}

impl<P: EmbeddingProvider> EmbeddingProvider for CachedEmbeddingProvider<P> {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Check cache first
        if let Some(embedding) = self.cache.get(text) {
            return Ok(embedding);
        }

        // Generate embedding
        let embedding = self.provider.embed(text)?;

        // Store in cache
        self.cache.put(text.to_string(), embedding.clone());

        Ok(embedding)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        let mut uncached_indices = Vec::new();
        let mut uncached_texts = Vec::new();

        // Check cache for each text
        for (i, text) in texts.iter().enumerate() {
            if let Some(embedding) = self.cache.get(text) {
                results.push(Some(embedding));
            } else {
                results.push(None);
                uncached_indices.push(i);
                uncached_texts.push(*text);
            }
        }

        // Generate embeddings for uncached texts
        if !uncached_texts.is_empty() {
            let new_embeddings = self.provider.embed_batch(&uncached_texts)?;

            for (idx, embedding) in uncached_indices.iter().zip(new_embeddings.iter()) {
                self.cache.put(texts[*idx].to_string(), embedding.clone());
                results[*idx] = Some(embedding.clone());
            }
        }

        // Unwrap all results
        results
            .into_iter()
            .map(|opt| opt.context("Missing embedding"))
            .collect()
    }

    fn dimension(&self) -> usize {
        self.provider.dimension()
    }

    fn name(&self) -> &str {
        self.provider.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_provider() {
        let provider = MockEmbeddingProvider::new(384);

        assert_eq!(provider.dimension(), 384);
        assert_eq!(provider.name(), "mock");

        let embedding = provider.embed("Hello, world!").unwrap();
        assert_eq!(embedding.len(), 384);

        // Embeddings should be normalized
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_mock_provider_deterministic() {
        let provider = MockEmbeddingProvider::new(128);

        let embedding1 = provider.embed("test").unwrap();
        let embedding2 = provider.embed("test").unwrap();

        // Same text should produce same embedding
        assert_eq!(embedding1, embedding2);
    }

    #[test]
    fn test_mock_provider_different_texts() {
        let provider = MockEmbeddingProvider::new(128);

        let embedding1 = provider.embed("hello").unwrap();
        let embedding2 = provider.embed("world").unwrap();

        // Different texts should produce different embeddings
        assert_ne!(embedding1, embedding2);
    }

    #[test]
    fn test_mock_provider_batch() {
        let provider = MockEmbeddingProvider::new(256);

        let texts = vec!["text1", "text2", "text3"];
        let embeddings = provider.embed_batch(&texts).unwrap();

        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].len(), 256);
        assert_eq!(embeddings[1].len(), 256);
        assert_eq!(embeddings[2].len(), 256);
    }

    #[test]
    fn test_openai_provider_creation() {
        let config = OpenAIConfig {
            api_key: "test-key".to_string(),
            model: "text-embedding-ada-002".to_string(),
            endpoint: None,
        };

        let provider = OpenAIEmbeddingProvider::new(config).unwrap();
        assert_eq!(provider.dimension(), 1536);
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_provider_unknown_model() {
        let config = OpenAIConfig {
            api_key: "test-key".to_string(),
            model: "unknown-model".to_string(),
            endpoint: None,
        };

        let result = OpenAIEmbeddingProvider::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_embedding_cache() {
        let cache = EmbeddingCache::new(Duration::from_secs(10), 100);

        // Initially empty
        assert_eq!(cache.size(), 0);
        assert!(cache.get("test").is_none());

        // Store and retrieve
        cache.put("test".to_string(), vec![1.0, 2.0, 3.0]);
        assert_eq!(cache.size(), 1);

        let embedding = cache.get("test").unwrap();
        assert_eq!(embedding, vec![1.0, 2.0, 3.0]);

        // Clear cache
        cache.clear();
        assert_eq!(cache.size(), 0);
        assert!(cache.get("test").is_none());
    }

    #[test]
    fn test_embedding_cache_max_entries() {
        let cache = EmbeddingCache::new(Duration::from_secs(10), 3);

        cache.put("key1".to_string(), vec![1.0]);
        cache.put("key2".to_string(), vec![2.0]);
        cache.put("key3".to_string(), vec![3.0]);

        assert_eq!(cache.size(), 3);

        // Adding 4th entry should evict oldest
        cache.put("key4".to_string(), vec![4.0]);
        assert_eq!(cache.size(), 3);

        // key1 should be evicted
        assert!(cache.get("key1").is_none());
        assert!(cache.get("key4").is_some());
    }

    #[test]
    fn test_cached_provider() {
        let mock_provider = MockEmbeddingProvider::new(128);
        let cache = EmbeddingCache::new(Duration::from_secs(10), 100);
        let cached_provider = CachedEmbeddingProvider::new(mock_provider, cache);

        assert_eq!(cached_provider.cache_size(), 0);

        // First call - not cached
        let embedding1 = cached_provider.embed("test").unwrap();
        assert_eq!(cached_provider.cache_size(), 1);

        // Second call - should be cached
        let embedding2 = cached_provider.embed("test").unwrap();
        assert_eq!(cached_provider.cache_size(), 1);

        assert_eq!(embedding1, embedding2);
    }

    #[test]
    fn test_cached_provider_batch() {
        let mock_provider = MockEmbeddingProvider::new(64);
        let cache = EmbeddingCache::new(Duration::from_secs(10), 100);
        let cached_provider = CachedEmbeddingProvider::new(mock_provider, cache);

        let texts = vec!["text1", "text2", "text3"];

        // First batch call - nothing cached
        let embeddings1 = cached_provider.embed_batch(&texts).unwrap();
        assert_eq!(cached_provider.cache_size(), 3);

        // Second batch call - all cached
        let embeddings2 = cached_provider.embed_batch(&texts).unwrap();
        assert_eq!(cached_provider.cache_size(), 3);

        assert_eq!(embeddings1, embeddings2);
    }

    #[test]
    fn test_cached_provider_partial_cache() {
        let mock_provider = MockEmbeddingProvider::new(32);
        let cache = EmbeddingCache::new(Duration::from_secs(10), 100);
        let cached_provider = CachedEmbeddingProvider::new(mock_provider, cache);

        // Cache some texts
        cached_provider.embed("text1").unwrap();
        cached_provider.embed("text2").unwrap();
        assert_eq!(cached_provider.cache_size(), 2);

        // Batch with mix of cached and uncached
        let texts = vec!["text1", "text2", "text3", "text4"];
        let embeddings = cached_provider.embed_batch(&texts).unwrap();

        assert_eq!(embeddings.len(), 4);
        assert_eq!(cached_provider.cache_size(), 4);
    }

    #[test]
    fn test_cache_clear() {
        let mock_provider = MockEmbeddingProvider::new(16);
        let cache = EmbeddingCache::new(Duration::from_secs(10), 100);
        let cached_provider = CachedEmbeddingProvider::new(mock_provider, cache);

        cached_provider.embed("test1").unwrap();
        cached_provider.embed("test2").unwrap();
        assert_eq!(cached_provider.cache_size(), 2);

        cached_provider.clear_cache();
        assert_eq!(cached_provider.cache_size(), 0);
    }

    #[test]
    fn test_embedding_cache_eviction_order() {
        // max_entries=3, very long TTL so expiry never fires
        let cache = EmbeddingCache::new(Duration::from_secs(3600), 3);

        // Insert "a" (seq=0), "b" (seq=1), "c" (seq=2)
        cache.put("a".to_string(), vec![0.1]);
        cache.put("b".to_string(), vec![0.2]);
        cache.put("c".to_string(), vec![0.3]);

        // Insert "d" — cache full; "a" has seq=0 (lowest) and must be evicted
        cache.put("d".to_string(), vec![0.4]);

        assert_eq!(cache.size(), 3);
        assert!(cache.get("a").is_none(), "\"a\" should have been evicted");
        assert!(cache.get("b").is_some(), "\"b\" should still be present");
        assert!(cache.get("c").is_some(), "\"c\" should still be present");
        assert!(cache.get("d").is_some(), "\"d\" should be present");
    }

    #[test]
    fn test_embedding_cache_eviction_stress() {
        let max_entries = 5usize;
        let cache = EmbeddingCache::new(Duration::from_secs(3600), max_entries);

        for i in 0..1000usize {
            cache.put(format!("key_{i}"), vec![i as f32]);
            // Cache must never exceed max_entries
            assert!(
                cache.size() <= max_entries,
                "after inserting key_{i}: cache size {} exceeded max_entries {max_entries}",
                cache.size()
            );
        }
    }
}
