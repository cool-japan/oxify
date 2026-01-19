//! Hybrid search combining vector and keyword search
//!
//! This module implements hybrid search using Reciprocal Rank Fusion (RRF)
//! to combine results from semantic vector search and BM25 keyword search.

use crate::bm25::{Bm25Document, Bm25Index};
use crate::{Result, SearchRequest, SearchResult, VectorProvider};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Hybrid search parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchParams {
    /// Weight for semantic search (0.0-1.0)
    pub semantic_weight: f32,
    /// Weight for keyword search (0.0-1.0)
    pub keyword_weight: f32,
    /// RRF parameter (typically 60)
    pub rrf_k: f32,
}

impl Default for HybridSearchParams {
    fn default() -> Self {
        Self {
            semantic_weight: 0.7,
            keyword_weight: 0.3,
            rrf_k: 60.0,
        }
    }
}

/// Hybrid search engine combining vector and BM25 search
pub struct HybridSearchEngine<V: VectorProvider> {
    vector_provider: V,
    bm25_index: Bm25Index,
    params: HybridSearchParams,
}

impl<V: VectorProvider> HybridSearchEngine<V> {
    /// Create a new hybrid search engine
    pub fn new(
        vector_provider: V,
        bm25_documents: Vec<Bm25Document>,
        params: HybridSearchParams,
    ) -> Self {
        let bm25_index = Bm25Index::new(bm25_documents);
        Self {
            vector_provider,
            bm25_index,
            params,
        }
    }

    /// Perform hybrid search
    pub async fn search(
        &self,
        vector_request: SearchRequest,
        text_query: &str,
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        // Get semantic search results
        let semantic_results = self.vector_provider.search(vector_request).await?;

        // Get keyword search results
        let keyword_results = self.bm25_index.search(text_query, top_k * 2);

        // Combine results using RRF
        let hybrid_results =
            self.reciprocal_rank_fusion(&semantic_results, &keyword_results, top_k);

        Ok(hybrid_results)
    }

    /// Reciprocal Rank Fusion (RRF) algorithm
    ///
    /// RRF(d) = Σ (1 / (k + rank_i(d)))
    /// where rank_i(d) is the rank of document d in result set i
    fn reciprocal_rank_fusion(
        &self,
        semantic_results: &[SearchResult],
        keyword_results: &[(String, f32)],
        top_k: usize,
    ) -> Vec<SearchResult> {
        let mut scores: HashMap<String, f32> = HashMap::new();

        // Add semantic search scores
        for (rank, result) in semantic_results.iter().enumerate() {
            let rrf_score = self.params.semantic_weight / (self.params.rrf_k + rank as f32 + 1.0);
            *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        }

        // Add keyword search scores
        for (rank, (id, _score)) in keyword_results.iter().enumerate() {
            let rrf_score = self.params.keyword_weight / (self.params.rrf_k + rank as f32 + 1.0);
            *scores.entry(id.clone()).or_insert(0.0) += rrf_score;
        }

        // Sort by combined score
        let mut results: Vec<(String, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Map back to SearchResult, preferring semantic results for metadata
        results
            .into_iter()
            .take(top_k)
            .map(|(id, score)| {
                // Try to find in semantic results first
                if let Some(sem_result) = semantic_results.iter().find(|r| r.id == id) {
                    SearchResult {
                        id,
                        score: score as f64,
                        payload: sem_result.payload.clone(),
                        vector: sem_result.vector.clone(),
                    }
                } else {
                    // Fall back to BM25 document
                    let doc = self.bm25_index.get_document(&id);
                    SearchResult {
                        id,
                        score: score as f64,
                        payload: doc
                            .map(|d| d.metadata.clone())
                            .unwrap_or(serde_json::Value::Null),
                        vector: None,
                    }
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bm25::Bm25Document;
    use async_trait::async_trait;

    // Mock vector provider for testing
    struct MockVectorProvider {
        results: Vec<SearchResult>,
    }

    #[async_trait]
    impl VectorProvider for MockVectorProvider {
        async fn search(&self, _request: SearchRequest) -> Result<Vec<SearchResult>> {
            Ok(self.results.clone())
        }

        async fn insert(&self, _request: crate::InsertRequest) -> Result<()> {
            Ok(())
        }

        async fn delete(&self, _request: crate::DeleteRequest) -> Result<usize> {
            Ok(0)
        }

        async fn create_collection(&self, _name: &str, _dimension: usize) -> Result<()> {
            Ok(())
        }

        async fn collection_exists(&self, _name: &str) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_hybrid_search() {
        let semantic_results = vec![
            SearchResult {
                id: "doc1".to_string(),
                score: 0.95,
                payload: serde_json::json!({"text": "semantic match"}),
                vector: Some(vec![0.1, 0.2, 0.3]),
            },
            SearchResult {
                id: "doc2".to_string(),
                score: 0.85,
                payload: serde_json::json!({"text": "another match"}),
                vector: Some(vec![0.2, 0.3, 0.4]),
            },
        ];

        let mock_provider = MockVectorProvider {
            results: semantic_results,
        };

        let bm25_docs = vec![
            Bm25Document {
                id: "doc2".to_string(),
                text: "keyword match here".to_string(),
                metadata: serde_json::json!({"text": "keyword"}),
            },
            Bm25Document {
                id: "doc3".to_string(),
                text: "another keyword match".to_string(),
                metadata: serde_json::json!({"text": "more keywords"}),
            },
        ];

        let engine =
            HybridSearchEngine::new(mock_provider, bm25_docs, HybridSearchParams::default());

        let vector_req = SearchRequest {
            collection: "test".to_string(),
            query: vec![0.1, 0.2, 0.3],
            top_k: 5,
            score_threshold: None,
            filter: None,
        };

        let results = engine.search(vector_req, "keyword match", 3).await.unwrap();

        // Should have combined results
        assert!(!results.is_empty());
        // doc2 should rank high as it appears in both semantic and keyword results
        assert!(results.iter().any(|r| r.id == "doc2"));
    }
}
