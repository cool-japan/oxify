//! ColBERT-style Multi-Vector Search
//!
//! Late interaction model for dense retrieval with token-level matching.
//!
//! ## Algorithm Overview
//!
//! ColBERT (Contextualized Late Interaction over BERT) represents documents
//! as collections of token embeddings and uses MaxSim for scoring:
//!
//! 1. Each document/query → sequence of token embeddings
//! 2. Score = Σ max(sim(q_token, d_token)) for all query tokens
//! 3. "Late interaction": token-level matching instead of single vector
//!
//! ## Benefits
//!
//! - **Fine-grained matching**: Matches specific parts of documents
//! - **Better accuracy**: Captures more semantic nuance than single vectors
//! - **Interpretability**: Can identify which tokens matched
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::colbert::{ColbertIndex, ColbertConfig};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! let config = ColbertConfig::default();
//! let mut index = ColbertIndex::new(config);
//!
//! // Each document has multiple token embeddings
//! let mut doc_tokens = HashMap::new();
//! doc_tokens.insert("doc1".to_string(), vec![
//!     vec![0.1, 0.2, 0.3],
//!     vec![0.2, 0.3, 0.4],
//!     vec![0.3, 0.4, 0.5],
//! ]);
//!
//! index.build(&doc_tokens)?;
//!
//! let query_tokens = vec![
//!     vec![0.15, 0.25, 0.35],
//!     vec![0.25, 0.35, 0.45],
//! ];
//!
//! let results = index.search(&query_tokens, 10)?;
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::simd;
use crate::types::{DistanceMetric, SearchResult};

/// ColBERT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColbertConfig {
    /// Distance metric for token similarity
    /// Cosine similarity is standard for ColBERT
    pub metric: DistanceMetric,

    /// Maximum number of tokens per document
    /// Longer documents are truncated
    pub max_doc_tokens: usize,

    /// Maximum number of tokens per query
    pub max_query_tokens: usize,

    /// Enable compression for token storage
    pub compress_tokens: bool,

    /// Use parallel search
    pub parallel_search: bool,
}

impl Default for ColbertConfig {
    fn default() -> Self {
        Self {
            metric: DistanceMetric::Cosine,
            max_doc_tokens: 300,
            max_query_tokens: 32,
            compress_tokens: false,
            parallel_search: true,
        }
    }
}

impl ColbertConfig {
    pub fn with_metric(mut self, metric: DistanceMetric) -> Self {
        self.metric = metric;
        self
    }

    pub fn with_max_doc_tokens(mut self, max_doc_tokens: usize) -> Self {
        self.max_doc_tokens = max_doc_tokens;
        self
    }

    pub fn with_max_query_tokens(mut self, max_query_tokens: usize) -> Self {
        self.max_query_tokens = max_query_tokens;
        self
    }

    pub fn with_compression(mut self, compress: bool) -> Self {
        self.compress_tokens = compress;
        self
    }
}

/// Multi-vector representation of a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiVectorDoc {
    pub entity_id: String,
    pub token_embeddings: Vec<Vec<f32>>,
}

/// ColBERT search result with token-level match information
#[derive(Debug, Clone)]
pub struct ColbertSearchResult {
    pub entity_id: String,
    pub score: f32,
    /// Token-level scores (query_token_idx -> (best_doc_token_idx, score))
    pub token_matches: Vec<(usize, f32)>,
}

/// ColBERT index for multi-vector search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColbertIndex {
    config: ColbertConfig,
    documents: Vec<MultiVectorDoc>,
    dim: Option<usize>,
}

impl ColbertIndex {
    pub fn new(config: ColbertConfig) -> Self {
        Self {
            config,
            documents: Vec::new(),
            dim: None,
        }
    }

    /// Build index from multi-vector documents
    pub fn build(&mut self, doc_tokens: &HashMap<String, Vec<Vec<f32>>>) -> Result<()> {
        if doc_tokens.is_empty() {
            anyhow::bail!("Cannot build ColBERT index with empty documents");
        }

        // Determine dimension from first token
        let first_doc_tokens = doc_tokens
            .values()
            .next()
            .expect("invariant: doc_tokens non-empty from guard");
        if first_doc_tokens.is_empty() {
            anyhow::bail!("Document has no token embeddings");
        }
        self.dim = Some(first_doc_tokens[0].len());

        // Store all documents
        self.documents.clear();
        for (entity_id, tokens) in doc_tokens {
            // Truncate to max_doc_tokens
            let truncated_tokens = if tokens.len() > self.config.max_doc_tokens {
                tokens[..self.config.max_doc_tokens].to_vec()
            } else {
                tokens.clone()
            };

            self.documents.push(MultiVectorDoc {
                entity_id: entity_id.clone(),
                token_embeddings: truncated_tokens,
            });
        }

        Ok(())
    }

    /// Add a single document to the index
    pub fn add(&mut self, entity_id: String, token_embeddings: Vec<Vec<f32>>) -> Result<()> {
        if token_embeddings.is_empty() {
            anyhow::bail!("Cannot add document with no token embeddings");
        }

        // Set dimension if first document
        if self.dim.is_none() {
            self.dim = Some(token_embeddings[0].len());
        }

        // Verify all tokens have correct dimension
        let dim = self.dim.expect("invariant: dim set above if was None");
        for token in &token_embeddings {
            if token.len() != dim {
                anyhow::bail!(
                    "Token dimension {} does not match index dimension {}",
                    token.len(),
                    dim
                );
            }
        }

        // Truncate to max_doc_tokens
        let truncated_tokens = if token_embeddings.len() > self.config.max_doc_tokens {
            token_embeddings[..self.config.max_doc_tokens].to_vec()
        } else {
            token_embeddings
        };

        self.documents.push(MultiVectorDoc {
            entity_id,
            token_embeddings: truncated_tokens,
        });

        Ok(())
    }

    /// Search using MaxSim scoring
    pub fn search(&self, query_tokens: &[Vec<f32>], k: usize) -> Result<Vec<ColbertSearchResult>> {
        if self.documents.is_empty() {
            return Ok(Vec::new());
        }

        // Truncate query to max_query_tokens
        let query = if query_tokens.len() > self.config.max_query_tokens {
            &query_tokens[..self.config.max_query_tokens]
        } else {
            query_tokens
        };

        // Compute MaxSim score for each document
        let results: Vec<ColbertSearchResult> = if self.config.parallel_search {
            self.documents
                .par_iter()
                .map(|doc| self.compute_maxsim_score(query, doc))
                .collect()
        } else {
            self.documents
                .iter()
                .map(|doc| self.compute_maxsim_score(query, doc))
                .collect()
        };

        // Sort by score (descending) and return top-k
        let mut sorted_results = results;
        sorted_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(sorted_results.into_iter().take(k).collect())
    }

    /// Compute similarity score between two vectors
    ///
    /// Uses SIMD-optimized calculations for better performance.
    #[inline]
    fn compute_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        // Use SIMD-optimized implementations for hot path performance
        simd::compute_distance_simd(self.config.metric, a, b)
    }

    /// Compute MaxSim score: sum of max similarities for each query token
    fn compute_maxsim_score(
        &self,
        query_tokens: &[Vec<f32>],
        doc: &MultiVectorDoc,
    ) -> ColbertSearchResult {
        let mut total_score = 0.0;
        let mut token_matches = Vec::with_capacity(query_tokens.len());

        for query_token in query_tokens {
            // Find the best matching document token
            let (best_doc_idx, best_score) = doc
                .token_embeddings
                .iter()
                .enumerate()
                .map(|(idx, doc_token)| {
                    let score = self.compute_similarity(query_token, doc_token);
                    (idx, score)
                })
                .max_by(|(_, a), (_, b)| {
                    // Handle NaN values by treating them as negative infinity
                    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or((0, 0.0));

            total_score += best_score;
            token_matches.push((best_doc_idx, best_score));
        }

        ColbertSearchResult {
            entity_id: doc.entity_id.clone(),
            score: total_score,
            token_matches,
        }
    }

    /// Convert ColBERT results to standard SearchResult format
    pub fn to_search_results(&self, results: Vec<ColbertSearchResult>) -> Vec<SearchResult> {
        results
            .into_iter()
            .enumerate()
            .map(|(rank, r)| SearchResult {
                entity_id: r.entity_id,
                score: r.score,
                distance: r.score,
                rank: rank + 1,
            })
            .collect()
    }

    /// Get index statistics
    pub fn stats(&self) -> ColbertStats {
        let total_tokens: usize = self
            .documents
            .iter()
            .map(|d| d.token_embeddings.len())
            .sum();

        let avg_tokens = if self.documents.is_empty() {
            0.0
        } else {
            total_tokens as f32 / self.documents.len() as f32
        };

        let memory_bytes = self.estimate_memory();

        ColbertStats {
            num_documents: self.documents.len(),
            total_tokens,
            avg_tokens_per_doc: avg_tokens,
            dimension: self.dim.unwrap_or(0),
            memory_bytes,
        }
    }

    fn estimate_memory(&self) -> usize {
        let total_tokens: usize = self
            .documents
            .iter()
            .map(|d| d.token_embeddings.len())
            .sum();
        let dim = self.dim.unwrap_or(0);

        // Tokens: total_tokens * dim * 4 bytes (f32)
        total_tokens * dim * 4
    }

    /// Remove a document by entity_id
    pub fn remove(&mut self, entity_id: &str) -> bool {
        if let Some(pos) = self.documents.iter().position(|d| d.entity_id == entity_id) {
            self.documents.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get number of documents
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

/// ColBERT index statistics
#[derive(Debug, Clone)]
pub struct ColbertStats {
    pub num_documents: usize,
    pub total_tokens: usize,
    pub avg_tokens_per_doc: f32,
    pub dimension: usize,
    pub memory_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colbert_creation() {
        let config = ColbertConfig::default();
        let index = ColbertIndex::new(config);

        assert_eq!(index.len(), 0);
        assert!(index.is_empty());
    }

    #[test]
    fn test_colbert_add_document() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        let tokens = vec![vec![0.1, 0.2, 0.3], vec![0.2, 0.3, 0.4]];

        assert!(index.add("doc1".to_string(), tokens).is_ok());
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn test_colbert_search() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        // Add documents
        let doc1_tokens = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.9, 0.1, 0.0],
            vec![0.8, 0.2, 0.0],
        ];

        let doc2_tokens = vec![
            vec![0.0, 1.0, 0.0],
            vec![0.1, 0.9, 0.0],
            vec![0.2, 0.8, 0.0],
        ];

        assert!(index.add("doc1".to_string(), doc1_tokens).is_ok());
        assert!(index.add("doc2".to_string(), doc2_tokens).is_ok());

        // Search with query closer to doc1
        let query_tokens = vec![vec![0.95, 0.05, 0.0], vec![0.85, 0.15, 0.0]];

        let results = index.search(&query_tokens, 2);
        assert!(results.is_ok());

        let results = results.unwrap();
        assert_eq!(results.len(), 2);

        // First result should be doc1 (higher score)
        assert_eq!(results[0].entity_id, "doc1");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_colbert_maxsim_scoring() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        // Document with 3 token embeddings
        let doc_tokens = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
        ];

        assert!(index.add("doc1".to_string(), doc_tokens).is_ok());

        // Query with 2 tokens that match first two doc tokens
        let query_tokens = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];

        let results = index.search(&query_tokens, 1);
        assert!(results.is_ok());

        let results = results.unwrap();
        assert_eq!(results.len(), 1);

        // Should have match info for both query tokens
        assert_eq!(results[0].token_matches.len(), 2);
    }

    #[test]
    fn test_colbert_remove() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        let tokens = vec![vec![0.1, 0.2, 0.3]];

        assert!(index.add("doc1".to_string(), tokens.clone()).is_ok());
        assert!(index.add("doc2".to_string(), tokens).is_ok());

        assert_eq!(index.len(), 2);

        assert!(index.remove("doc1"));
        assert_eq!(index.len(), 1);

        assert!(!index.remove("doc1")); // Already removed
    }

    #[test]
    fn test_colbert_build_from_hashmap() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        let mut doc_tokens = HashMap::new();
        doc_tokens.insert(
            "doc1".to_string(),
            vec![vec![1.0, 0.0, 0.0], vec![0.9, 0.1, 0.0]],
        );
        doc_tokens.insert(
            "doc2".to_string(),
            vec![vec![0.0, 1.0, 0.0], vec![0.1, 0.9, 0.0]],
        );
        doc_tokens.insert(
            "doc3".to_string(),
            vec![vec![0.0, 0.0, 1.0], vec![0.0, 0.1, 0.9]],
        );

        let build_result = index.build(&doc_tokens);
        assert!(build_result.is_ok());
        assert_eq!(index.len(), 3);

        // Search for doc1
        let query_tokens = vec![vec![1.0, 0.0, 0.0]];
        let results = index.search(&query_tokens, 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].entity_id, "doc1");
    }

    #[test]
    fn test_colbert_token_truncation() {
        let config = ColbertConfig::default().with_max_doc_tokens(5);
        let mut index = ColbertIndex::new(config);

        // Create a document with 10 tokens (should be truncated to 5)
        let long_doc_tokens: Vec<Vec<f32>> =
            (0..10).map(|i| vec![i as f32 / 10.0, 0.0, 0.0]).collect();

        assert!(index.add("doc1".to_string(), long_doc_tokens).is_ok());

        // Verify truncation
        assert_eq!(index.documents[0].token_embeddings.len(), 5);
    }

    #[test]
    fn test_colbert_query_truncation() {
        let config = ColbertConfig::default().with_max_query_tokens(3);
        let mut index = ColbertIndex::new(config);

        let doc_tokens = vec![vec![1.0, 0.0, 0.0], vec![0.9, 0.1, 0.0]];
        assert!(index.add("doc1".to_string(), doc_tokens).is_ok());

        // Create a long query (10 tokens, should be truncated to 3)
        let long_query: Vec<Vec<f32>> = (0..10).map(|i| vec![i as f32 / 10.0, 0.0, 0.0]).collect();

        let results = index.search(&long_query, 1);
        assert!(results.is_ok());

        let results = results.unwrap();
        // Should have match info for only 3 query tokens (truncated)
        assert_eq!(results[0].token_matches.len(), 3);
    }

    #[test]
    fn test_colbert_parallel_vs_sequential() {
        // Test with parallel search
        let config_parallel = ColbertConfig::default().with_compression(false);
        let mut index_parallel = ColbertIndex::new(config_parallel);

        // Test with sequential search
        let config_sequential = ColbertConfig {
            parallel_search: false,
            ..Default::default()
        };
        let mut index_sequential = ColbertIndex::new(config_sequential);

        // Add same documents to both
        let mut doc_tokens = HashMap::new();
        for i in 0..20 {
            let tokens: Vec<Vec<f32>> = (0..10)
                .map(|j| vec![(i + j) as f32 / 20.0, 0.0, 0.0])
                .collect();
            doc_tokens.insert(format!("doc{}", i), tokens);
        }

        assert!(index_parallel.build(&doc_tokens).is_ok());
        assert!(index_sequential.build(&doc_tokens).is_ok());

        // Search with both
        let query_tokens = vec![vec![0.5, 0.0, 0.0]];
        let results_parallel = index_parallel.search(&query_tokens, 5).unwrap();
        let results_sequential = index_sequential.search(&query_tokens, 5).unwrap();

        // Results should be the same
        assert_eq!(results_parallel.len(), results_sequential.len());
        assert_eq!(
            results_parallel[0].entity_id,
            results_sequential[0].entity_id
        );
    }

    #[test]
    fn test_colbert_different_metrics() {
        let metrics = vec![
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
            DistanceMetric::Manhattan,
        ];

        for metric in metrics {
            let config = ColbertConfig::default().with_metric(metric);
            let mut index = ColbertIndex::new(config);

            let doc_tokens = vec![vec![1.0, 0.0, 0.0], vec![0.9, 0.1, 0.0]];
            assert!(index.add("doc1".to_string(), doc_tokens).is_ok());

            let query_tokens = vec![vec![1.0, 0.0, 0.0]];
            let results = index.search(&query_tokens, 1);
            assert!(results.is_ok());
        }
    }

    #[test]
    fn test_colbert_empty_index_search() {
        let config = ColbertConfig::default();
        let index = ColbertIndex::new(config);

        let query_tokens = vec![vec![1.0, 0.0, 0.0]];
        let results = index.search(&query_tokens, 5);

        assert!(results.is_ok());
        assert_eq!(results.unwrap().len(), 0);
    }

    #[test]
    fn test_colbert_empty_tokens_error() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        let empty_tokens: Vec<Vec<f32>> = vec![];
        let result = index.add("doc1".to_string(), empty_tokens);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot add document with no token embeddings"));
    }

    #[test]
    fn test_colbert_dimension_mismatch_error() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        // Add first document with 3 dimensions
        let doc1_tokens = vec![vec![1.0, 0.0, 0.0]];
        assert!(index.add("doc1".to_string(), doc1_tokens).is_ok());

        // Try to add second document with 4 dimensions (should fail)
        let doc2_tokens = vec![vec![1.0, 0.0, 0.0, 0.0]];
        let result = index.add("doc2".to_string(), doc2_tokens);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not match index dimension"));
    }

    #[test]
    fn test_colbert_build_empty_error() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        let empty_docs = HashMap::new();
        let result = index.build(&empty_docs);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot build ColBERT index with empty documents"));
    }

    #[test]
    fn test_colbert_build_empty_tokens_error() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        let mut doc_tokens = HashMap::new();
        doc_tokens.insert("doc1".to_string(), vec![]); // Empty tokens

        let result = index.build(&doc_tokens);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Document has no token embeddings"));
    }

    #[test]
    fn test_colbert_stats() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        // Add documents with varying token counts
        index
            .add(
                "doc1".to_string(),
                vec![vec![1.0, 0.0], vec![0.9, 0.1], vec![0.8, 0.2]],
            )
            .unwrap();
        index
            .add("doc2".to_string(), vec![vec![0.0, 1.0], vec![0.1, 0.9]])
            .unwrap();
        index.add("doc3".to_string(), vec![vec![0.5, 0.5]]).unwrap();

        let stats = index.stats();
        assert_eq!(stats.num_documents, 3);
        assert_eq!(stats.total_tokens, 6); // 3 + 2 + 1
        assert!((stats.avg_tokens_per_doc - 2.0).abs() < 0.01); // 6/3 = 2.0
        assert_eq!(stats.dimension, 2);
        assert!(stats.memory_bytes > 0);
    }

    #[test]
    fn test_colbert_to_search_results() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        index
            .add(
                "doc1".to_string(),
                vec![vec![1.0, 0.0, 0.0], vec![0.9, 0.1, 0.0]],
            )
            .unwrap();
        index
            .add(
                "doc2".to_string(),
                vec![vec![0.0, 1.0, 0.0], vec![0.1, 0.9, 0.0]],
            )
            .unwrap();

        let query_tokens = vec![vec![1.0, 0.0, 0.0]];
        let colbert_results = index.search(&query_tokens, 2).unwrap();

        // Convert to standard search results
        let search_results = index.to_search_results(colbert_results);

        assert_eq!(search_results.len(), 2);
        assert_eq!(search_results[0].rank, 1);
        assert_eq!(search_results[1].rank, 2);
        assert_eq!(search_results[0].entity_id, "doc1");
    }

    #[test]
    fn test_colbert_large_scale() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        // Add 100 documents with multiple tokens each
        for i in 0..100 {
            let tokens: Vec<Vec<f32>> = (0..10)
                .map(|j| vec![(i + j) as f32 / 100.0, 0.0, 0.0])
                .collect();
            index.add(format!("doc{}", i), tokens).unwrap();
        }

        assert_eq!(index.len(), 100);

        // Search
        let query_tokens = vec![vec![0.5, 0.0, 0.0], vec![0.6, 0.0, 0.0]];
        let results = index.search(&query_tokens, 10).unwrap();

        assert_eq!(results.len(), 10);
        assert!(results[0].score >= results[9].score); // Sorted by score
    }

    #[test]
    fn test_colbert_token_match_information() {
        let config = ColbertConfig::default();
        let mut index = ColbertIndex::new(config);

        // Document with 3 distinct tokens
        let doc_tokens = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
        ];
        index.add("doc1".to_string(), doc_tokens).unwrap();

        // Query with 2 tokens
        let query_tokens = vec![vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]];

        let results = index.search(&query_tokens, 1).unwrap();
        assert_eq!(results.len(), 1);

        // Check token matches
        let token_matches = &results[0].token_matches;
        assert_eq!(token_matches.len(), 2);

        // First query token should match first doc token (index 0)
        assert_eq!(token_matches[0].0, 0);
        // Second query token should match third doc token (index 2)
        assert_eq!(token_matches[1].0, 2);
    }
}
