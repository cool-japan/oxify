//! ColBERT-style multi-vector search
//!
//! This module implements ColBERT-style late interaction search where:
//! - Each document can have multiple vectors (e.g., token-level embeddings)
//! - Query can also have multiple vectors
//! - MaxSim scoring: For each query vector, find max similarity with any document vector
//! - Sum of MaxSim scores gives final document score
//!
//! # Example
//! ```no_run
//! use oxify_connect_vector::{ColBERTProvider, MockVectorProvider};
//! use oxify_connect_vector::colbert::MultiVectorInsertRequest;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = MockVectorProvider::new();
//! let colbert = ColBERTProvider::new(provider);
//!
//! // Insert document with multiple vectors (e.g., token embeddings)
//! colbert.insert_multi_vector(MultiVectorInsertRequest {
//!     collection: "docs".to_string(),
//!     id: "doc1".to_string(),
//!     vectors: vec![
//!         vec![0.1, 0.2, 0.3],
//!         vec![0.4, 0.5, 0.6],
//!         vec![0.7, 0.8, 0.9],
//!     ],
//!     payload: serde_json::json!({"title": "Example"}),
//! }).await?;
//!
//! // Search with multiple query vectors
//! let results = colbert.search_multi_vector(
//!     "docs",
//!     vec![vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6]],
//!     10,
//!     None,
//! ).await?;
//! # Ok(())
//! # }
//! ```

use crate::{cosine_similarity, InsertRequest, Result, SearchRequest, VectorError, VectorProvider};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request to insert a document with multiple vectors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiVectorInsertRequest {
    pub collection: String,
    pub id: String,
    pub vectors: Vec<Vec<f32>>,
    pub payload: serde_json::Value,
}

/// Search result with MaxSim score breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiVectorSearchResult {
    pub id: String,
    pub score: f64,
    pub payload: serde_json::Value,
    /// Individual MaxSim scores for each query vector
    pub maxsim_scores: Vec<f64>,
}

/// Scoring strategy for multi-vector search
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum ScoringStrategy {
    /// Sum of MaxSim scores (default ColBERT)
    #[default]
    MaxSimSum,
    /// Average of MaxSim scores
    MaxSimAverage,
    /// Maximum of MaxSim scores
    MaxSimMax,
}

/// Compute MaxSim score between query vectors and document vectors
///
/// For each query vector, find the maximum similarity with any document vector.
/// Then aggregate according to the scoring strategy.
pub fn compute_maxsim_score(
    query_vectors: &[Vec<f32>],
    doc_vectors: &[Vec<f32>],
    strategy: ScoringStrategy,
) -> (f64, Vec<f64>) {
    if query_vectors.is_empty() || doc_vectors.is_empty() {
        return (0.0, vec![]);
    }

    let mut maxsim_scores = Vec::with_capacity(query_vectors.len());

    // For each query vector, find max similarity with any doc vector
    for query_vec in query_vectors {
        let max_sim = doc_vectors
            .iter()
            .map(|doc_vec| cosine_similarity(query_vec, doc_vec))
            .fold(f64::NEG_INFINITY, f64::max);

        maxsim_scores.push(max_sim);
    }

    // Aggregate scores according to strategy
    let final_score = match strategy {
        ScoringStrategy::MaxSimSum => maxsim_scores.iter().sum(),
        ScoringStrategy::MaxSimAverage => {
            maxsim_scores.iter().sum::<f64>() / maxsim_scores.len() as f64
        }
        ScoringStrategy::MaxSimMax => maxsim_scores
            .iter()
            .fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
    };

    (final_score, maxsim_scores)
}

/// Provider that adds multi-vector (ColBERT-style) search capabilities
///
/// This wraps any VectorProvider and stores multiple vectors per document
/// by creating sub-documents with a naming convention: {id}_vec_{index}
pub struct ColBERTProvider<P: VectorProvider> {
    inner: P,
    scoring_strategy: ScoringStrategy,
}

impl<P: VectorProvider> ColBERTProvider<P> {
    /// Create a new ColBERT provider with default MaxSimSum strategy
    pub fn new(provider: P) -> Self {
        Self {
            inner: provider,
            scoring_strategy: ScoringStrategy::MaxSimSum,
        }
    }

    /// Create a new ColBERT provider with custom scoring strategy
    pub fn with_strategy(provider: P, strategy: ScoringStrategy) -> Self {
        Self {
            inner: provider,
            scoring_strategy: strategy,
        }
    }

    /// Get reference to inner provider
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Insert a document with multiple vectors
    pub async fn insert_multi_vector(&self, request: MultiVectorInsertRequest) -> Result<usize> {
        if request.vectors.is_empty() {
            return Err(VectorError::QueryError(
                "MultiVectorInsertRequest must have at least one vector".to_string(),
            ));
        }

        // Store metadata about number of vectors
        let mut metadata = request.payload.clone();
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert(
                "_multi_vector_count".to_string(),
                serde_json::json!(request.vectors.len()),
            );
            obj.insert("_multi_vector_parent".to_string(), serde_json::json!(true));
        }

        // Insert parent document with first vector and metadata
        self.inner
            .insert(InsertRequest {
                collection: request.collection.clone(),
                id: request.id.clone(),
                vector: request.vectors[0].clone(),
                payload: metadata,
            })
            .await?;

        // Insert remaining vectors as child documents
        for (idx, vector) in request.vectors.iter().enumerate().skip(1) {
            let child_id = format!("{}_vec_{}", request.id, idx);
            let child_payload = serde_json::json!({
                "_multi_vector_parent_id": request.id,
                "_multi_vector_index": idx,
            });

            self.inner
                .insert(InsertRequest {
                    collection: request.collection.clone(),
                    id: child_id,
                    vector: vector.clone(),
                    payload: child_payload,
                })
                .await?;
        }

        Ok(request.vectors.len())
    }

    /// Search with multiple query vectors using MaxSim scoring
    pub async fn search_multi_vector(
        &self,
        collection: &str,
        query_vectors: Vec<Vec<f32>>,
        top_k: usize,
        score_threshold: Option<f64>,
    ) -> Result<Vec<MultiVectorSearchResult>> {
        if query_vectors.is_empty() {
            return Err(VectorError::QueryError(
                "Query must have at least one vector".to_string(),
            ));
        }

        // Search with each query vector to get candidates
        // We fetch more results than needed to ensure we have enough after MaxSim scoring
        let search_k = (top_k * 5).max(100);

        let mut all_results = Vec::new();
        for query_vec in &query_vectors {
            let results = self
                .inner
                .search(SearchRequest {
                    collection: collection.to_string(),
                    query: query_vec.clone(),
                    top_k: search_k,
                    score_threshold: None, // Apply threshold after MaxSim
                    filter: None,
                })
                .await?;
            all_results.extend(results);
        }

        // Group results by parent document ID
        let mut doc_vectors: HashMap<String, Vec<Vec<f32>>> = HashMap::new();
        let mut doc_payloads: HashMap<String, serde_json::Value> = HashMap::new();

        for result in all_results {
            // Check if this is a child document
            let parent_id = if let Some(parent) = result.payload.get("_multi_vector_parent_id") {
                parent.as_str().unwrap_or(&result.id).to_string()
            } else {
                result.id.clone()
            };

            // Store the vector
            if let Some(vector) = result.vector {
                doc_vectors
                    .entry(parent_id.clone())
                    .or_default()
                    .push(vector);
            }

            // Store payload (only from parent documents)
            if result.payload.get("_multi_vector_parent").is_some() {
                doc_payloads.insert(parent_id.clone(), result.payload.clone());
            }
        }

        // Compute MaxSim scores for each document
        let mut scored_results = Vec::new();
        for (doc_id, vectors) in doc_vectors {
            let (score, maxsim_scores) =
                compute_maxsim_score(&query_vectors, &vectors, self.scoring_strategy);

            // Apply score threshold if specified
            if let Some(threshold) = score_threshold {
                if score < threshold {
                    continue;
                }
            }

            // Get payload (use empty if not found)
            let mut payload = doc_payloads
                .get(&doc_id)
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            // Remove internal metadata fields
            if let Some(obj) = payload.as_object_mut() {
                obj.remove("_multi_vector_count");
                obj.remove("_multi_vector_parent");
            }

            scored_results.push(MultiVectorSearchResult {
                id: doc_id,
                score,
                payload,
                maxsim_scores,
            });
        }

        // Sort by score descending
        scored_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top k
        scored_results.truncate(top_k);

        Ok(scored_results)
    }

    /// Delete a multi-vector document (removes parent and all child vectors)
    pub async fn delete_multi_vector(&self, collection: &str, id: &str) -> Result<usize> {
        // Search for all vectors belonging to this document
        // First, try to get parent to know how many vectors there are
        let parent_results = self
            .inner
            .search(SearchRequest {
                collection: collection.to_string(),
                query: vec![0.0; 128], // Dummy query
                top_k: 1000,
                score_threshold: None,
                filter: None,
            })
            .await?;

        let mut ids_to_delete = vec![id.to_string()];

        // Find child vectors
        for result in parent_results {
            if let Some(parent_id) = result.payload.get("_multi_vector_parent_id") {
                if parent_id.as_str() == Some(id) {
                    ids_to_delete.push(result.id);
                }
            }
        }

        // Delete all
        let deleted = self
            .inner
            .delete(crate::DeleteRequest {
                collection: collection.to_string(),
                ids: ids_to_delete,
            })
            .await?;

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockVectorProvider;

    #[test]
    fn test_compute_maxsim_score_sum() {
        let query_vectors = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];

        let doc_vectors = vec![
            vec![1.0, 0.0, 0.0], // Perfect match for query 1
            vec![0.0, 0.8, 0.6], // High match for query 2 (normalized)
            vec![0.5, 0.5, 0.0],
        ];

        let (score, maxsim_scores) =
            compute_maxsim_score(&query_vectors, &doc_vectors, ScoringStrategy::MaxSimSum);

        assert_eq!(maxsim_scores.len(), 2);
        // First query should match perfectly with first doc vector
        assert!((maxsim_scores[0] - 1.0).abs() < 1e-6);
        // Second query should have high similarity with second doc vector
        assert!(maxsim_scores[1] > 0.8);
        // Sum should be > 1.8
        assert!(score > 1.8);
    }

    #[test]
    fn test_compute_maxsim_score_average() {
        let query_vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let doc_vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0]];

        let (score, _) =
            compute_maxsim_score(&query_vectors, &doc_vectors, ScoringStrategy::MaxSimAverage);

        // Both query vectors perfectly match doc vectors, average should be 1.0
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_maxsim_score_max() {
        let query_vectors = vec![
            vec![1.0, 0.0], // Perfect match
            vec![0.5, 0.5], // Partial match
        ];
        let doc_vectors = vec![vec![1.0, 0.0]];

        let (score, _) =
            compute_maxsim_score(&query_vectors, &doc_vectors, ScoringStrategy::MaxSimMax);

        // Max should be 1.0 from the perfect match
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_maxsim_score_empty() {
        let query_vectors: Vec<Vec<f32>> = vec![];
        let doc_vectors = vec![vec![1.0, 0.0]];

        let (score, maxsim_scores) =
            compute_maxsim_score(&query_vectors, &doc_vectors, ScoringStrategy::MaxSimSum);

        assert_eq!(score, 0.0);
        assert!(maxsim_scores.is_empty());
    }

    #[tokio::test]
    async fn test_colbert_insert_and_search() {
        let mock = MockVectorProvider::new();
        mock.create_collection("test", 3).await.unwrap();

        let colbert = ColBERTProvider::new(mock);

        // Insert a document with 3 vectors
        let count = colbert
            .insert_multi_vector(MultiVectorInsertRequest {
                collection: "test".to_string(),
                id: "doc1".to_string(),
                vectors: vec![
                    vec![1.0, 0.0, 0.0],
                    vec![0.0, 1.0, 0.0],
                    vec![0.0, 0.0, 1.0],
                ],
                payload: serde_json::json!({"title": "Test Doc"}),
            })
            .await
            .unwrap();

        assert_eq!(count, 3);

        // Search with 2 query vectors
        let results = colbert
            .search_multi_vector(
                "test",
                vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]],
                10,
                None,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
        assert_eq!(results[0].maxsim_scores.len(), 2);
        // Both query vectors should find perfect matches
        assert!((results[0].maxsim_scores[0] - 1.0).abs() < 1e-6);
        assert!((results[0].maxsim_scores[1] - 1.0).abs() < 1e-6);
        // Sum should be 2.0
        assert!((results[0].score - 2.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_colbert_empty_vectors_error() {
        let mock = MockVectorProvider::new();
        mock.create_collection("test", 3).await.unwrap();

        let colbert = ColBERTProvider::new(mock);

        let result = colbert
            .insert_multi_vector(MultiVectorInsertRequest {
                collection: "test".to_string(),
                id: "doc1".to_string(),
                vectors: vec![],
                payload: serde_json::json!({}),
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_colbert_scoring_strategies() {
        let mock = MockVectorProvider::new();
        mock.create_collection("test", 2).await.unwrap();

        // Test with MaxSimSum
        let colbert_sum = ColBERTProvider::new(mock.clone());
        colbert_sum
            .insert_multi_vector(MultiVectorInsertRequest {
                collection: "test".to_string(),
                id: "doc1".to_string(),
                vectors: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
                payload: serde_json::json!({}),
            })
            .await
            .unwrap();

        let results_sum = colbert_sum
            .search_multi_vector("test", vec![vec![1.0, 0.0], vec![0.0, 1.0]], 10, None)
            .await
            .unwrap();
        assert!((results_sum[0].score - 2.0).abs() < 1e-6);

        // Test with MaxSimAverage
        let colbert_avg =
            ColBERTProvider::with_strategy(mock.clone(), ScoringStrategy::MaxSimAverage);
        let results_avg = colbert_avg
            .search_multi_vector("test", vec![vec![1.0, 0.0], vec![0.0, 1.0]], 10, None)
            .await
            .unwrap();
        assert!((results_avg[0].score - 1.0).abs() < 1e-6);

        // Test with MaxSimMax
        let colbert_max = ColBERTProvider::with_strategy(mock, ScoringStrategy::MaxSimMax);
        let results_max = colbert_max
            .search_multi_vector("test", vec![vec![1.0, 0.0], vec![0.0, 1.0]], 10, None)
            .await
            .unwrap();
        assert!((results_max[0].score - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_colbert_multiple_documents() {
        let mock = MockVectorProvider::new();
        mock.create_collection("test", 2).await.unwrap();

        let colbert = ColBERTProvider::new(mock);

        // Insert two documents
        colbert
            .insert_multi_vector(MultiVectorInsertRequest {
                collection: "test".to_string(),
                id: "doc1".to_string(),
                vectors: vec![vec![1.0, 0.0], vec![0.9, 0.1]],
                payload: serde_json::json!({"title": "Doc 1"}),
            })
            .await
            .unwrap();

        colbert
            .insert_multi_vector(MultiVectorInsertRequest {
                collection: "test".to_string(),
                id: "doc2".to_string(),
                vectors: vec![vec![0.0, 1.0], vec![0.1, 0.9]],
                payload: serde_json::json!({"title": "Doc 2"}),
            })
            .await
            .unwrap();

        // Search - should return both documents
        let results = colbert
            .search_multi_vector("test", vec![vec![1.0, 0.0]], 10, None)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        // doc1 should score higher because its vectors are closer to the query
        assert_eq!(results[0].id, "doc1");
        assert!(results[0].score > results[1].score);
    }

    #[tokio::test]
    async fn test_colbert_score_threshold() {
        let mock = MockVectorProvider::new();
        mock.create_collection("test", 2).await.unwrap();

        let colbert = ColBERTProvider::new(mock);

        colbert
            .insert_multi_vector(MultiVectorInsertRequest {
                collection: "test".to_string(),
                id: "doc1".to_string(),
                vectors: vec![vec![1.0, 0.0]],
                payload: serde_json::json!({}),
            })
            .await
            .unwrap();

        colbert
            .insert_multi_vector(MultiVectorInsertRequest {
                collection: "test".to_string(),
                id: "doc2".to_string(),
                vectors: vec![vec![0.0, 1.0]],
                payload: serde_json::json!({}),
            })
            .await
            .unwrap();

        // Search with high threshold - should only return doc1
        let results = colbert
            .search_multi_vector("test", vec![vec![1.0, 0.0]], 10, Some(0.9))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
    }
}
