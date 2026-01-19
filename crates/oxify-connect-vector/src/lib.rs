//! Vector database connections for OxiFY
//!
//! This crate provides abstractions and implementations for vector databases:
//! - Qdrant: High-performance vector search engine
//! - pgvector: PostgreSQL extension for vector similarity search

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod bm25;
pub mod cache;
pub mod chromadb;
pub mod colbert;
pub mod filter;
pub mod health;
pub mod hybrid;
pub mod metrics;
pub mod migration;
pub mod milvus;
pub mod mock;
pub mod parallel;
// pgvector module disabled - SQLite migration
// pub mod pgvector;
pub mod pinecone;
pub mod qdrant;
pub mod ratelimit;
pub mod rerank;
pub mod retry;
pub mod sparse;
pub mod weaviate;

pub use bm25::{Bm25Document, Bm25Index, Bm25Params};
pub use cache::{EmbeddingCache, SearchCache};
pub use chromadb::ChromaDBProvider;
pub use colbert::{
    compute_maxsim_score, ColBERTProvider, MultiVectorInsertRequest, MultiVectorSearchResult,
    ScoringStrategy,
};
pub use filter::{post_filter, FilterExpr, FilterValue};
pub use health::{
    default_health_check, HealthCheck, HealthCheckProvider, HealthCheckResult, HealthMonitor,
    HealthStatus,
};
pub use hybrid::{HybridSearchEngine, HybridSearchParams};
pub use metrics::{
    BatchOperationStats, MetricsProvider, OperationStats, OperationTimer, VectorMetrics,
};
pub use migration::{
    export_collection, import_snapshot, migrate_collection, verify_migration, MigrationOptions,
    MigrationProgress, MigrationVerification, VectorRecord, VectorSnapshot,
};
pub use milvus::MilvusProvider;
pub use mock::MockVectorProvider;
pub use parallel::{
    parallel_batch_delete, parallel_batch_delete_with_limit, parallel_batch_insert,
    parallel_batch_insert_with_limit, parallel_batch_search, parallel_batch_search_with_limit,
    parallel_batch_update, parallel_batch_update_with_limit, ParallelConfig,
};
// pgvector disabled - SQLite migration
// pub use pgvector::{DistanceMetric, PgVectorProvider, PgVectorProviderBuilder};
pub use pinecone::PineconeProvider;
pub use qdrant::QdrantProvider;
pub use ratelimit::RateLimiter;
pub use rerank::{
    CohereReranker, CustomReranker, KeywordBoostReranker, MmrReranker, Reranker, RerankerChain,
};
pub use retry::{retry_with_backoff, RetryConfig};
pub use sparse::{
    batch_to_dense, batch_to_sparse, densify_with_threshold, sparse_cosine_similarity,
    sparse_dot_product, sparse_euclidean_distance, sparse_jaccard_similarity, SparseVector,
};
pub use weaviate::WeaviateProvider;

pub type Result<T> = std::result::Result<T, VectorError>;

#[derive(Error, Debug)]
pub enum VectorError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub collection: String,
    pub query: Vec<f32>,
    pub top_k: usize,
    pub score_threshold: Option<f64>,
    pub filter: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f64,
    pub payload: serde_json::Value,
    pub vector: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertRequest {
    pub collection: String,
    pub id: String,
    pub vector: Vec<f32>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub collection: String,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInsertRequest {
    pub collection: String,
    pub vectors: Vec<(String, Vec<f32>, serde_json::Value)>, // (id, vector, payload)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRequest {
    pub collection: String,
    pub id: String,
    pub vector: Option<Vec<f32>>,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    pub dimension: usize,
    pub vector_count: usize,
}

/// Trait for vector database providers
#[async_trait]
pub trait VectorProvider: Send + Sync {
    /// Search for similar vectors
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>>;

    /// Insert vectors into the database
    async fn insert(&self, request: InsertRequest) -> Result<()>;

    /// Delete vectors by ID
    async fn delete(&self, request: DeleteRequest) -> Result<usize>;

    /// Create a new collection/table
    async fn create_collection(&self, name: &str, dimension: usize) -> Result<()>;

    /// Check if collection exists
    async fn collection_exists(&self, name: &str) -> Result<bool>;

    /// Batch insert vectors (default implementation uses single inserts)
    async fn batch_insert(&self, request: BatchInsertRequest) -> Result<usize> {
        let mut count = 0;
        for (id, vector, payload) in request.vectors {
            self.insert(InsertRequest {
                collection: request.collection.clone(),
                id,
                vector,
                payload,
            })
            .await?;
            count += 1;
        }
        Ok(count)
    }

    /// Update a vector and/or its payload
    async fn update(&self, request: UpdateRequest) -> Result<()> {
        // Default implementation: delete and re-insert
        // Providers can override with more efficient implementations
        if request.vector.is_none() && request.payload.is_none() {
            return Err(VectorError::QueryError(
                "Update requires either vector or payload".to_string(),
            ));
        }

        // This is a naive implementation - providers should override
        Err(VectorError::QueryError(
            "Update not supported by this provider".to_string(),
        ))
    }

    /// Get collection information
    async fn collection_info(&self, _name: &str) -> Result<CollectionInfo> {
        Err(VectorError::QueryError(
            "Collection info not supported by this provider".to_string(),
        ))
    }

    /// Batch update vectors and/or payloads (default implementation uses single updates)
    async fn batch_update(&self, requests: Vec<UpdateRequest>) -> Result<usize> {
        let mut count = 0;
        for request in requests {
            self.update(request).await?;
            count += 1;
        }
        Ok(count)
    }
}

// ===== Embedding-Aware Helpers =====

#[cfg(feature = "embeddings")]
use oxify_connect_llm::{EmbeddingProvider, EmbeddingRequest};

/// Request to insert text with automatic embedding generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertTextRequest {
    pub collection: String,
    pub id: String,
    pub text: String,
    pub payload: serde_json::Value,
}

/// Helper struct that combines vector store with embedding generation
#[cfg(feature = "embeddings")]
pub struct EmbeddingVectorStore<V, E>
where
    V: VectorProvider,
    E: EmbeddingProvider,
{
    vector_store: V,
    embedding_provider: E,
}

#[cfg(feature = "embeddings")]
impl<V, E> EmbeddingVectorStore<V, E>
where
    V: VectorProvider,
    E: EmbeddingProvider,
{
    pub fn new(vector_store: V, embedding_provider: E) -> Self {
        Self {
            vector_store,
            embedding_provider,
        }
    }

    /// Insert text by generating embeddings automatically
    pub async fn insert_text(&self, request: InsertTextRequest) -> Result<()> {
        // Generate embedding for the text
        let embedding_response = self
            .embedding_provider
            .embed(EmbeddingRequest {
                texts: vec![request.text.clone()],
                model: None,
            })
            .await
            .map_err(|e| VectorError::QueryError(format!("Embedding generation failed: {}", e)))?;

        if embedding_response.embeddings.is_empty() {
            return Err(VectorError::QueryError(
                "No embeddings generated".to_string(),
            ));
        }

        // Insert into vector store
        self.vector_store
            .insert(InsertRequest {
                collection: request.collection,
                id: request.id,
                vector: embedding_response.embeddings[0].clone(),
                payload: request.payload,
            })
            .await
    }

    /// Search by text (generates embedding automatically)
    pub async fn search_by_text(
        &self,
        collection: String,
        query_text: String,
        top_k: usize,
        score_threshold: Option<f64>,
    ) -> Result<Vec<SearchResult>> {
        // Generate embedding for the query text
        let embedding_response = self
            .embedding_provider
            .embed(EmbeddingRequest {
                texts: vec![query_text],
                model: None,
            })
            .await
            .map_err(|e| VectorError::QueryError(format!("Embedding generation failed: {}", e)))?;

        if embedding_response.embeddings.is_empty() {
            return Err(VectorError::QueryError(
                "No embeddings generated".to_string(),
            ));
        }

        // Search vector store
        self.vector_store
            .search(SearchRequest {
                collection,
                query: embedding_response.embeddings[0].clone(),
                top_k,
                score_threshold,
                filter: None,
            })
            .await
    }
}

// ===== Utility Functions =====

/// Compute dot product between two vectors
///
/// Returns the dot product, or 0.0 if vectors have different dimensions or are empty.
///
/// # Examples
/// ```
/// use oxify_connect_vector::dot_product;
///
/// let a = vec![1.0, 2.0, 3.0];
/// let b = vec![4.0, 5.0, 6.0];
/// let result = dot_product(&a, &b);
/// assert!((result - 32.0).abs() < 1e-6);
/// ```
pub fn dot_product(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    a.iter().zip(b.iter()).map(|(x, y)| (x * y) as f64).sum()
}

/// Compute Manhattan distance (L1 distance) between two vectors
///
/// Returns the sum of absolute differences, or f64::MAX if vectors have different dimensions or are empty.
///
/// # Examples
/// ```
/// use oxify_connect_vector::manhattan_distance;
///
/// let a = vec![1.0, 2.0, 3.0];
/// let b = vec![4.0, 5.0, 6.0];
/// let result = manhattan_distance(&a, &b);
/// assert!((result - 9.0).abs() < 1e-6);
/// ```
pub fn manhattan_distance(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::MAX;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| ((x - y).abs()) as f64)
        .sum()
}

/// Check if a vector is normalized (unit length)
///
/// Returns true if the vector's magnitude is approximately 1.0 (within epsilon = 1e-6).
///
/// # Examples
/// ```
/// use oxify_connect_vector::is_normalized;
///
/// let normalized = vec![0.6, 0.8];
/// assert!(is_normalized(&normalized));
///
/// let not_normalized = vec![1.0, 1.0];
/// assert!(!is_normalized(&not_normalized));
/// ```
pub fn is_normalized(vector: &[f32]) -> bool {
    if vector.is_empty() {
        return false;
    }

    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    (magnitude - 1.0).abs() < 1e-6
}

/// Check if a vector contains valid values (no NaN or infinite values)
///
/// Returns true if all values are finite numbers.
///
/// # Examples
/// ```
/// use oxify_connect_vector::is_valid_vector;
///
/// let valid = vec![1.0, 2.0, 3.0];
/// assert!(is_valid_vector(&valid));
///
/// let invalid = vec![1.0, f32::NAN, 3.0];
/// assert!(!is_valid_vector(&invalid));
///
/// let infinite = vec![1.0, f32::INFINITY, 3.0];
/// assert!(!is_valid_vector(&infinite));
/// ```
pub fn is_valid_vector(vector: &[f32]) -> bool {
    vector.iter().all(|x| x.is_finite())
}

/// Normalize multiple vectors in batch
///
/// Returns a vector of normalized vectors. Each vector is normalized to unit length.
/// If a vector has zero magnitude, it is returned unchanged.
///
/// # Examples
/// ```
/// use oxify_connect_vector::batch_normalize;
///
/// let vectors = vec![
///     vec![3.0, 4.0],
///     vec![1.0, 0.0],
///     vec![0.0, 5.0],
/// ];
/// let normalized = batch_normalize(&vectors);
/// assert_eq!(normalized.len(), 3);
/// assert!((normalized[0][0] - 0.6).abs() < 1e-6);
/// assert!((normalized[0][1] - 0.8).abs() < 1e-6);
/// ```
pub fn batch_normalize(vectors: &[Vec<f32>]) -> Vec<Vec<f32>> {
    vectors.iter().map(|v| normalize_vector(v)).collect()
}

/// Compute pairwise cosine similarities between a query vector and multiple vectors
///
/// Returns a vector of similarity scores in the same order as the input vectors.
///
/// # Examples
/// ```
/// use oxify_connect_vector::batch_cosine_similarity;
///
/// let query = vec![1.0, 0.0, 0.0];
/// let vectors = vec![
///     vec![1.0, 0.0, 0.0],
///     vec![0.0, 1.0, 0.0],
///     vec![0.5, 0.5, 0.0],
/// ];
/// let similarities = batch_cosine_similarity(&query, &vectors);
/// assert_eq!(similarities.len(), 3);
/// assert!((similarities[0] - 1.0).abs() < 1e-6);
/// assert!((similarities[1] - 0.0).abs() < 1e-6);
/// ```
pub fn batch_cosine_similarity(query: &[f32], vectors: &[Vec<f32>]) -> Vec<f64> {
    vectors
        .iter()
        .map(|v| cosine_similarity(query, v))
        .collect()
}

/// Helper function to compute cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        0.0
    } else {
        (dot_product / (magnitude_a * magnitude_b)) as f64
    }
}

/// Helper function to compute Euclidean distance between two vectors
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::MAX;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let diff = x - y;
            (diff * diff) as f64
        })
        .sum::<f64>()
        .sqrt()
}

/// Helper function to normalize a vector to unit length
pub fn normalize_vector(vector: &[f32]) -> Vec<f32> {
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude == 0.0 {
        return vector.to_vec();
    }

    vector.iter().map(|x| x / magnitude).collect()
}

// ===== SIMD-Optimized Utility Functions =====

#[cfg(feature = "simd")]
use oxify_vector::simd::{
    cosine_similarity_simd, dot_product_simd, euclidean_distance_simd, manhattan_distance_simd,
};

/// Compute dot product between two vectors (SIMD-optimized when simd feature is enabled)
///
/// This function automatically uses SIMD instructions (AVX-512, AVX2, or NEON) when available
/// for significantly improved performance.
///
/// # Performance
/// - **x86_64**: Up to 8-16x faster with AVX2/AVX-512
/// - **aarch64**: Up to 4x faster with NEON
/// - **Fallback**: Auto-vectorization on other platforms
///
/// # Examples
/// ```
/// use oxify_connect_vector::dot_product_optimized;
///
/// let a = vec![1.0, 2.0, 3.0];
/// let b = vec![4.0, 5.0, 6.0];
/// let result = dot_product_optimized(&a, &b);
/// assert!((result - 32.0).abs() < 1e-6);
/// ```
#[cfg(feature = "simd")]
pub fn dot_product_optimized(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    dot_product_simd(a, b) as f64
}

#[cfg(not(feature = "simd"))]
pub fn dot_product_optimized(a: &[f32], b: &[f32]) -> f64 {
    dot_product(a, b)
}

/// Compute cosine similarity between two vectors (SIMD-optimized when simd feature is enabled)
///
/// This function automatically uses SIMD instructions for improved performance.
///
/// # Examples
/// ```
/// use oxify_connect_vector::cosine_similarity_optimized;
///
/// let a = vec![1.0, 0.0, 0.0];
/// let b = vec![1.0, 0.0, 0.0];
/// let result = cosine_similarity_optimized(&a, &b);
/// assert!((result - 1.0).abs() < 1e-6);
/// ```
#[cfg(feature = "simd")]
pub fn cosine_similarity_optimized(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    cosine_similarity_simd(a, b) as f64
}

#[cfg(not(feature = "simd"))]
pub fn cosine_similarity_optimized(a: &[f32], b: &[f32]) -> f64 {
    cosine_similarity(a, b)
}

/// Compute Euclidean distance between two vectors (SIMD-optimized when simd feature is enabled)
///
/// # Examples
/// ```
/// use oxify_connect_vector::euclidean_distance_optimized;
///
/// let a = vec![0.0, 0.0];
/// let b = vec![3.0, 4.0];
/// let result = euclidean_distance_optimized(&a, &b);
/// assert!((result - 5.0).abs() < 1e-6);
/// ```
#[cfg(feature = "simd")]
pub fn euclidean_distance_optimized(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::MAX;
    }
    euclidean_distance_simd(a, b) as f64
}

#[cfg(not(feature = "simd"))]
pub fn euclidean_distance_optimized(a: &[f32], b: &[f32]) -> f64 {
    euclidean_distance(a, b)
}

/// Compute Manhattan distance between two vectors (SIMD-optimized when simd feature is enabled)
///
/// # Examples
/// ```
/// use oxify_connect_vector::manhattan_distance_optimized;
///
/// let a = vec![1.0, 2.0, 3.0];
/// let b = vec![4.0, 5.0, 6.0];
/// let result = manhattan_distance_optimized(&a, &b);
/// assert!((result - 9.0).abs() < 1e-6);
/// ```
#[cfg(feature = "simd")]
pub fn manhattan_distance_optimized(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::MAX;
    }
    manhattan_distance_simd(a, b) as f64
}

#[cfg(not(feature = "simd"))]
pub fn manhattan_distance_optimized(a: &[f32], b: &[f32]) -> f64 {
    manhattan_distance(a, b)
}

/// Compute pairwise cosine similarities with SIMD optimization
///
/// Returns a vector of similarity scores using SIMD acceleration when available.
///
/// # Examples
/// ```
/// use oxify_connect_vector::batch_cosine_similarity_optimized;
///
/// let query = vec![1.0, 0.0, 0.0];
/// let vectors = vec![
///     vec![1.0, 0.0, 0.0],
///     vec![0.0, 1.0, 0.0],
/// ];
/// let similarities = batch_cosine_similarity_optimized(&query, &vectors);
/// assert_eq!(similarities.len(), 2);
/// assert!((similarities[0] - 1.0).abs() < 1e-6);
/// ```
pub fn batch_cosine_similarity_optimized(query: &[f32], vectors: &[Vec<f32>]) -> Vec<f64> {
    vectors
        .iter()
        .map(|v| cosine_similarity_optimized(query, v))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        assert!((dot_product(&a, &b) - 32.0).abs() < 1e-6);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((dot_product(&a, &b) - 0.0).abs() < 1e-6);

        // Empty vectors
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(dot_product(&a, &b), 0.0);

        // Different dimensions
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(dot_product(&a, &b), 0.0);
    }

    #[test]
    fn test_manhattan_distance() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        assert!((manhattan_distance(&a, &b) - 9.0).abs() < 1e-6);

        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert!((manhattan_distance(&a, &b) - 7.0).abs() < 1e-6);

        // Same vectors
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!((manhattan_distance(&a, &b) - 0.0).abs() < 1e-6);

        // Empty vectors
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(manhattan_distance(&a, &b), f64::MAX);
    }

    #[test]
    fn test_is_normalized() {
        let normalized = vec![0.6, 0.8];
        assert!(is_normalized(&normalized));

        let normalized = vec![1.0, 0.0, 0.0];
        assert!(is_normalized(&normalized));

        let not_normalized = vec![1.0, 1.0];
        assert!(!is_normalized(&not_normalized));

        let not_normalized = vec![3.0, 4.0];
        assert!(!is_normalized(&not_normalized));

        // Empty vector
        let empty: Vec<f32> = vec![];
        assert!(!is_normalized(&empty));
    }

    #[test]
    fn test_is_valid_vector() {
        let valid = vec![1.0, 2.0, 3.0];
        assert!(is_valid_vector(&valid));

        let valid = vec![-1.0, 0.0, 1.0];
        assert!(is_valid_vector(&valid));

        let invalid_nan = vec![1.0, f32::NAN, 3.0];
        assert!(!is_valid_vector(&invalid_nan));

        let invalid_inf = vec![1.0, f32::INFINITY, 3.0];
        assert!(!is_valid_vector(&invalid_inf));

        let invalid_neg_inf = vec![1.0, f32::NEG_INFINITY, 3.0];
        assert!(!is_valid_vector(&invalid_neg_inf));

        // Empty vector is valid
        let empty: Vec<f32> = vec![];
        assert!(is_valid_vector(&empty));
    }

    #[test]
    fn test_batch_normalize() {
        let vectors = vec![vec![3.0, 4.0], vec![1.0, 0.0], vec![0.0, 5.0]];
        let normalized = batch_normalize(&vectors);

        assert_eq!(normalized.len(), 3);

        // Check first vector
        assert!((normalized[0][0] - 0.6).abs() < 1e-6);
        assert!((normalized[0][1] - 0.8).abs() < 1e-6);

        // Check second vector (already normalized)
        assert!((normalized[1][0] - 1.0).abs() < 1e-6);
        assert!((normalized[1][1] - 0.0).abs() < 1e-6);

        // Check third vector
        assert!((normalized[2][0] - 0.0).abs() < 1e-6);
        assert!((normalized[2][1] - 1.0).abs() < 1e-6);

        // All should be unit length
        for v in &normalized {
            let magnitude: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((magnitude - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_batch_cosine_similarity() {
        let query = vec![1.0, 0.0, 0.0];
        let vectors = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.5, 0.5, 0.0],
        ];

        let similarities = batch_cosine_similarity(&query, &vectors);

        assert_eq!(similarities.len(), 3);
        assert!((similarities[0] - 1.0).abs() < 1e-6);
        assert!((similarities[1] - 0.0).abs() < 1e-6);
        assert!((similarities[2] - 0.7071067811865475).abs() < 1e-6); // cos(45°) ≈ 0.707

        // Empty vectors
        let empty_query = vec![1.0];
        let empty_vectors: Vec<Vec<f32>> = vec![];
        let result = batch_cosine_similarity(&empty_query, &empty_vectors);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 1e-6);

        let a = vec![1.0, 1.0];
        let b = vec![1.0, 1.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((euclidean_distance(&a, &b) - 1.0).abs() < 1e-6);

        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert!((euclidean_distance(&a, &b) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_vector() {
        let v = vec![3.0, 4.0];
        let normalized = normalize_vector(&v);
        assert!((normalized[0] - 0.6).abs() < 1e-6);
        assert!((normalized[1] - 0.8).abs() < 1e-6);

        // Verify unit length
        let magnitude: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-6);
    }

    // SIMD-optimized function tests
    #[test]
    fn test_dot_product_optimized() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let result = dot_product_optimized(&a, &b);
        let expected = dot_product(&a, &b);
        assert!((result - expected).abs() < 1e-5);
        assert!((result - 32.0).abs() < 1e-5);

        // Empty vectors
        let empty_a: Vec<f32> = vec![];
        let empty_b: Vec<f32> = vec![];
        assert_eq!(dot_product_optimized(&empty_a, &empty_b), 0.0);

        // Different dimensions
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(dot_product_optimized(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_optimized() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let result = cosine_similarity_optimized(&a, &b);
        let expected = cosine_similarity(&a, &b);
        assert!((result - expected).abs() < 1e-5);
        assert!((result - 1.0).abs() < 1e-5);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let result = cosine_similarity_optimized(&a, &b);
        let expected = cosine_similarity(&a, &b);
        assert!((result - expected).abs() < 1e-5);
        assert!((result - 0.0).abs() < 1e-5);

        // Test with normalized vectors
        let a = vec![0.6, 0.8];
        let b = vec![0.8, 0.6];
        let result = cosine_similarity_optimized(&a, &b);
        let expected = cosine_similarity(&a, &b);
        assert!((result - expected).abs() < 1e-5);
    }

    #[test]
    fn test_euclidean_distance_optimized() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let result = euclidean_distance_optimized(&a, &b);
        let expected = euclidean_distance(&a, &b);
        assert!((result - expected).abs() < 1e-5);
        assert!((result - 1.0).abs() < 1e-5);

        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        let result = euclidean_distance_optimized(&a, &b);
        let expected = euclidean_distance(&a, &b);
        assert!((result - expected).abs() < 1e-5);
        assert!((result - 5.0).abs() < 1e-5);

        // Empty vectors
        let empty_a: Vec<f32> = vec![];
        let empty_b: Vec<f32> = vec![];
        assert_eq!(euclidean_distance_optimized(&empty_a, &empty_b), f64::MAX);
    }

    #[test]
    fn test_manhattan_distance_optimized() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let result = manhattan_distance_optimized(&a, &b);
        let expected = manhattan_distance(&a, &b);
        assert!((result - expected).abs() < 1e-5);
        assert!((result - 9.0).abs() < 1e-5);

        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        let result = manhattan_distance_optimized(&a, &b);
        let expected = manhattan_distance(&a, &b);
        assert!((result - expected).abs() < 1e-5);
        assert!((result - 7.0).abs() < 1e-5);

        // Same vectors
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let result = manhattan_distance_optimized(&a, &b);
        assert!((result - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_batch_cosine_similarity_optimized() {
        let query = vec![1.0, 0.0, 0.0];
        let vectors = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.5, 0.5, 0.0],
        ];

        let result = batch_cosine_similarity_optimized(&query, &vectors);
        let expected = batch_cosine_similarity(&query, &vectors);

        assert_eq!(result.len(), expected.len());
        for (r, e) in result.iter().zip(expected.iter()) {
            assert!((r - e).abs() < 1e-5);
        }

        assert!((result[0] - 1.0).abs() < 1e-5);
        assert!((result[1] - 0.0).abs() < 1e-5);

        // Empty vectors
        let empty_query = vec![1.0];
        let empty_vectors: Vec<Vec<f32>> = vec![];
        let result = batch_cosine_similarity_optimized(&empty_query, &empty_vectors);
        assert_eq!(result.len(), 0);
    }
}
