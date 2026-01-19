//! Hybrid Search: Vector + Keyword (BM25)
//!
//! Combines semantic vector search with lexical keyword search for improved results.
//!
//! ## Algorithm
//!
//! - **Vector Search:** Semantic similarity using embeddings
//! - **Keyword Search:** BM25 scoring for lexical matching
//! - **Fusion:** Reciprocal Rank Fusion (RRF) or weighted combination
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::hybrid::{HybridIndex, HybridConfig};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create documents with text and embeddings
//! let mut embeddings = HashMap::new();
//! embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
//! embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
//!
//! let mut texts = HashMap::new();
//! texts.insert("doc1".to_string(), "rust programming language".to_string());
//! texts.insert("doc2".to_string(), "python machine learning".to_string());
//!
//! // Build hybrid index
//! let config = HybridConfig::default();
//! let mut index = HybridIndex::new(config);
//! index.build(&embeddings, &texts)?;
//!
//! // Hybrid search
//! let query_vector = vec![0.15, 0.25, 0.35];
//! let query_text = "rust programming";
//! let results = index.search(&query_vector, query_text, 2)?;
//! # Ok(())
//! # }
//! ```

use crate::search::VectorSearchIndex;
use crate::types::{DistanceMetric, SearchConfig, SearchResult};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// BM25 parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25Config {
    /// Term frequency saturation parameter (default: 1.2)
    pub k1: f32,
    /// Document length normalization parameter (default: 0.75)
    pub b: f32,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }
}

/// Hybrid search configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridConfig {
    /// Vector search weight (0.0 to 1.0)
    pub alpha: f32,
    /// Distance metric for vector search
    pub metric: DistanceMetric,
    /// BM25 parameters
    pub bm25: Bm25Config,
    /// RRF constant (typically 60)
    pub rrf_k: f32,
    /// Normalize vectors
    pub normalize: bool,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            alpha: 0.5,
            metric: DistanceMetric::Cosine,
            bm25: Bm25Config::default(),
            rrf_k: 60.0,
            normalize: true,
        }
    }
}

impl HybridConfig {
    /// Create config favoring vector search
    pub fn vector_heavy() -> Self {
        Self {
            alpha: 0.7,
            ..Default::default()
        }
    }

    /// Create config favoring keyword search
    pub fn keyword_heavy() -> Self {
        Self {
            alpha: 0.3,
            ..Default::default()
        }
    }
}

/// BM25 index for keyword search
struct Bm25Index {
    config: Bm25Config,
    /// Document texts (entity_id -> text)
    documents: HashMap<String, String>,
    /// Inverted index (term -> entity_ids with term frequency)
    inverted_index: HashMap<String, HashMap<String, usize>>,
    /// Document lengths (entity_id -> word count)
    doc_lengths: HashMap<String, usize>,
    /// Average document length
    avg_doc_length: f32,
    /// Total number of documents
    num_docs: usize,
}

impl Bm25Index {
    fn new(config: Bm25Config) -> Self {
        Self {
            config,
            documents: HashMap::new(),
            inverted_index: HashMap::new(),
            doc_lengths: HashMap::new(),
            avg_doc_length: 0.0,
            num_docs: 0,
        }
    }

    fn build(&mut self, texts: &HashMap<String, String>) {
        self.documents = texts.clone();
        self.num_docs = texts.len();

        // Tokenize and build inverted index
        let mut total_length = 0;

        for (entity_id, text) in texts {
            let tokens = self.tokenize(text);
            let doc_len = tokens.len();
            self.doc_lengths.insert(entity_id.clone(), doc_len);
            total_length += doc_len;

            // Count term frequencies
            let mut term_counts: HashMap<String, usize> = HashMap::new();
            for token in tokens {
                *term_counts.entry(token).or_insert(0) += 1;
            }

            // Update inverted index
            for (term, count) in term_counts {
                self.inverted_index
                    .entry(term)
                    .or_default()
                    .insert(entity_id.clone(), count);
            }
        }

        self.avg_doc_length = if self.num_docs > 0 {
            total_length as f32 / self.num_docs as f32
        } else {
            0.0
        };
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() > 1)
            .map(|s| s.to_string())
            .collect()
    }

    fn search(&self, query: &str, k: usize) -> Vec<(String, f32)> {
        let query_tokens = self.tokenize(query);
        let mut scores: HashMap<String, f32> = HashMap::new();

        for token in &query_tokens {
            if let Some(postings) = self.inverted_index.get(token) {
                // Calculate IDF
                let df = postings.len() as f32;
                let idf = ((self.num_docs as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();

                for (entity_id, &tf) in postings {
                    let doc_len = *self.doc_lengths.get(entity_id).unwrap_or(&1) as f32;
                    let tf_f = tf as f32;

                    // BM25 formula
                    let numerator = tf_f * (self.config.k1 + 1.0);
                    let denominator = tf_f
                        + self.config.k1
                            * (1.0 - self.config.b
                                + self.config.b * (doc_len / self.avg_doc_length));

                    let score = idf * (numerator / denominator);
                    *scores.entry(entity_id.clone()).or_insert(0.0) += score;
                }
            }
        }

        // Sort by score descending
        let mut results: Vec<(String, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        results
    }
}

/// Hybrid search index combining vector and keyword search
pub struct HybridIndex {
    config: HybridConfig,
    vector_index: VectorSearchIndex,
    bm25_index: Bm25Index,
    entity_ids: Vec<String>,
    is_built: bool,
}

impl HybridIndex {
    /// Create a new hybrid search index
    pub fn new(config: HybridConfig) -> Self {
        info!(
            "Initialized hybrid index: alpha={}, metric={:?}",
            config.alpha, config.metric
        );

        let vector_config = SearchConfig {
            metric: config.metric,
            parallel: true,
            normalize: config.normalize,
        };

        Self {
            vector_index: VectorSearchIndex::new(vector_config),
            bm25_index: Bm25Index::new(config.bm25.clone()),
            config,
            entity_ids: Vec::new(),
            is_built: false,
        }
    }

    /// Build hybrid index from embeddings and texts
    pub fn build(
        &mut self,
        embeddings: &HashMap<String, Vec<f32>>,
        texts: &HashMap<String, String>,
    ) -> Result<()> {
        if embeddings.is_empty() {
            return Err(anyhow!("Cannot build index from empty embeddings"));
        }

        // Verify all embeddings have corresponding texts
        for entity_id in embeddings.keys() {
            if !texts.contains_key(entity_id) {
                return Err(anyhow!(
                    "Missing text for entity '{}'. All embeddings must have corresponding texts.",
                    entity_id
                ));
            }
        }

        info!("Building hybrid index for {} entities", embeddings.len());

        self.entity_ids = embeddings.keys().cloned().collect();

        // Build vector index
        self.vector_index.build(embeddings)?;

        // Build BM25 index
        self.bm25_index.build(texts);

        self.is_built = true;
        info!("Hybrid index built successfully");
        Ok(())
    }

    /// Hybrid search combining vector and keyword results
    ///
    /// Uses Reciprocal Rank Fusion (RRF) to combine results.
    pub fn search(
        &self,
        query_vector: &[f32],
        query_text: &str,
        k: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        debug!(
            "Hybrid search: k={}, alpha={}, query_text='{}'",
            k, self.config.alpha, query_text
        );

        // Get more candidates for fusion
        let expanded_k = (k * 3).min(self.entity_ids.len());

        // Vector search
        let vector_results = self.vector_index.search(query_vector, expanded_k)?;

        // BM25 search
        let bm25_results = self.bm25_index.search(query_text, expanded_k);

        // Combine using RRF
        let results = self.reciprocal_rank_fusion(&vector_results, &bm25_results, k);

        debug!("Hybrid search returned {} results", results.len());
        Ok(results)
    }

    /// Combine results using Reciprocal Rank Fusion
    fn reciprocal_rank_fusion(
        &self,
        vector_results: &[SearchResult],
        bm25_results: &[(String, f32)],
        k: usize,
    ) -> Vec<HybridSearchResult> {
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut bm25_scores: HashMap<String, f32> = HashMap::new();

        // Calculate RRF scores from vector results
        for (rank, result) in vector_results.iter().enumerate() {
            let rrf_score = 1.0 / (self.config.rrf_k + rank as f32 + 1.0);
            *rrf_scores.entry(result.entity_id.clone()).or_insert(0.0) +=
                self.config.alpha * rrf_score;
            vector_scores.insert(result.entity_id.clone(), result.score);
        }

        // Calculate RRF scores from BM25 results
        for (rank, (entity_id, score)) in bm25_results.iter().enumerate() {
            let rrf_score = 1.0 / (self.config.rrf_k + rank as f32 + 1.0);
            *rrf_scores.entry(entity_id.clone()).or_insert(0.0) +=
                (1.0 - self.config.alpha) * rrf_score;
            bm25_scores.insert(entity_id.clone(), *score);
        }

        // Sort by combined RRF score
        let mut results: Vec<(String, f32)> = rrf_scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Convert to HybridSearchResult
        results
            .into_iter()
            .take(k)
            .enumerate()
            .map(|(rank, (entity_id, combined_score))| HybridSearchResult {
                entity_id: entity_id.clone(),
                combined_score,
                vector_score: vector_scores.get(&entity_id).copied(),
                bm25_score: bm25_scores.get(&entity_id).copied(),
                rank: rank + 1,
            })
            .collect()
    }

    /// Search using weighted linear combination instead of RRF
    pub fn weighted_search(
        &self,
        query_vector: &[f32],
        query_text: &str,
        k: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        let expanded_k = (k * 3).min(self.entity_ids.len());

        // Vector search
        let vector_results = self.vector_index.search(query_vector, expanded_k)?;

        // BM25 search
        let bm25_results = self.bm25_index.search(query_text, expanded_k);

        // Normalize and combine scores
        let mut combined_scores: HashMap<String, (Option<f32>, Option<f32>)> = HashMap::new();

        // Normalize vector scores (already in 0-1 for cosine)
        let max_vector_score = vector_results.first().map(|r| r.score).unwrap_or(1.0);
        for result in &vector_results {
            let norm_score = if max_vector_score > 0.0 {
                result.score / max_vector_score
            } else {
                0.0
            };
            combined_scores.insert(result.entity_id.clone(), (Some(norm_score), None));
        }

        // Normalize BM25 scores
        let max_bm25_score = bm25_results.first().map(|(_, s)| *s).unwrap_or(1.0);
        for (entity_id, score) in &bm25_results {
            let norm_score = if max_bm25_score > 0.0 {
                score / max_bm25_score
            } else {
                0.0
            };
            combined_scores
                .entry(entity_id.clone())
                .and_modify(|(_, b)| *b = Some(norm_score))
                .or_insert((None, Some(norm_score)));
        }

        // Calculate weighted combination
        let mut results: Vec<HybridSearchResult> = combined_scores
            .into_iter()
            .map(|(entity_id, (v_score, b_score))| {
                let v = v_score.unwrap_or(0.0);
                let b = b_score.unwrap_or(0.0);
                let combined = self.config.alpha * v + (1.0 - self.config.alpha) * b;

                HybridSearchResult {
                    entity_id,
                    combined_score: combined,
                    vector_score: v_score,
                    bm25_score: b_score,
                    rank: 0, // Will be set below
                }
            })
            .collect();

        // Sort by combined score
        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Set ranks
        for (i, result) in results.iter_mut().enumerate() {
            result.rank = i + 1;
        }

        results.truncate(k);
        Ok(results)
    }

    /// Vector-only search (alpha = 1.0)
    pub fn vector_search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        self.vector_index.search(query, k)
    }

    /// Keyword-only search (alpha = 0.0)
    pub fn keyword_search(&self, query: &str, k: usize) -> Result<Vec<HybridSearchResult>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        let results = self.bm25_index.search(query, k);

        Ok(results
            .into_iter()
            .enumerate()
            .map(|(rank, (entity_id, score))| HybridSearchResult {
                entity_id,
                combined_score: score,
                vector_score: None,
                bm25_score: Some(score),
                rank: rank + 1,
            })
            .collect())
    }

    /// Get index statistics
    pub fn get_stats(&self) -> HybridStats {
        HybridStats {
            num_documents: self.entity_ids.len(),
            vocabulary_size: self.bm25_index.inverted_index.len(),
            avg_doc_length: self.bm25_index.avg_doc_length,
            alpha: self.config.alpha,
            is_built: self.is_built,
        }
    }

    /// Set alpha parameter (vector vs keyword weight)
    pub fn set_alpha(&mut self, alpha: f32) {
        self.config.alpha = alpha.clamp(0.0, 1.0);
    }
}

/// Hybrid search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    /// Entity ID
    pub entity_id: String,
    /// Combined score from RRF or weighted combination
    pub combined_score: f32,
    /// Vector similarity score (if available)
    pub vector_score: Option<f32>,
    /// BM25 score (if available)
    pub bm25_score: Option<f32>,
    /// Rank in results (1-indexed)
    pub rank: usize,
}

/// Hybrid index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridStats {
    /// Number of documents in the index
    pub num_documents: usize,
    /// Number of unique terms in vocabulary
    pub vocabulary_size: usize,
    /// Average document length (in tokens)
    pub avg_doc_length: f32,
    /// Current alpha setting
    pub alpha: f32,
    /// Whether index is built
    pub is_built: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn create_test_data() -> (HashMap<String, Vec<f32>>, HashMap<String, String>) {
        let mut embeddings = HashMap::new();
        let mut texts = HashMap::new();

        // Tech document - similar vectors
        embeddings.insert("doc1".to_string(), vec![0.9, 0.1, 0.0]);
        texts.insert(
            "doc1".to_string(),
            "rust programming language systems programming".to_string(),
        );

        embeddings.insert("doc2".to_string(), vec![0.8, 0.2, 0.0]);
        texts.insert(
            "doc2".to_string(),
            "rust cargo package manager dependencies".to_string(),
        );

        // ML document
        embeddings.insert("doc3".to_string(), vec![0.1, 0.9, 0.0]);
        texts.insert(
            "doc3".to_string(),
            "python machine learning deep learning neural networks".to_string(),
        );

        embeddings.insert("doc4".to_string(), vec![0.0, 0.8, 0.2]);
        texts.insert(
            "doc4".to_string(),
            "python data science pandas numpy analysis".to_string(),
        );

        // Mixed
        embeddings.insert("doc5".to_string(), vec![0.5, 0.5, 0.0]);
        texts.insert(
            "doc5".to_string(),
            "rust machine learning inference performance".to_string(),
        );

        (embeddings, texts)
    }

    #[test]
    fn test_hybrid_config_default() {
        let config = HybridConfig::default();
        assert_eq!(config.alpha, 0.5);
        assert_eq!(config.rrf_k, 60.0);
    }

    #[test]
    fn test_hybrid_build() {
        let (embeddings, texts) = create_test_data();
        let mut index = HybridIndex::new(HybridConfig::default());

        assert!(index.build(&embeddings, &texts).is_ok());
        assert!(index.is_built);

        let stats = index.get_stats();
        assert_eq!(stats.num_documents, 5);
        assert!(stats.vocabulary_size > 0);
    }

    #[test]
    fn test_hybrid_search() {
        let (embeddings, texts) = create_test_data();
        let mut index = HybridIndex::new(HybridConfig::default());
        index.build(&embeddings, &texts).unwrap();

        // Search for rust programming
        let query_vector = vec![0.85, 0.15, 0.0];
        let query_text = "rust programming";
        let results = index.search(&query_vector, query_text, 3).unwrap();

        assert_eq!(results.len(), 3);
        // doc1 or doc2 should be top results (both vector and keyword match)
        assert!(results[0].entity_id == "doc1" || results[0].entity_id == "doc2");
    }

    #[test]
    fn test_weighted_search() {
        let (embeddings, texts) = create_test_data();
        let mut index = HybridIndex::new(HybridConfig::default());
        index.build(&embeddings, &texts).unwrap();

        let query_vector = vec![0.85, 0.15, 0.0];
        let query_text = "rust programming";
        let results = index.weighted_search(&query_vector, query_text, 3).unwrap();

        assert_eq!(results.len(), 3);
        // Results should have both vector and BM25 scores
        assert!(results[0].vector_score.is_some() || results[0].bm25_score.is_some());
    }

    #[test]
    fn test_vector_only_search() {
        let (embeddings, texts) = create_test_data();
        let mut index = HybridIndex::new(HybridConfig::default());
        index.build(&embeddings, &texts).unwrap();

        let query_vector = vec![0.85, 0.15, 0.0];
        let results = index.vector_search(&query_vector, 3).unwrap();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_keyword_only_search() {
        let (embeddings, texts) = create_test_data();
        let mut index = HybridIndex::new(HybridConfig::default());
        index.build(&embeddings, &texts).unwrap();

        let results = index.keyword_search("python machine learning", 3).unwrap();

        assert_eq!(results.len(), 3);
        // doc3 should be top (best keyword match)
        assert!(results[0].entity_id == "doc3" || results[0].entity_id == "doc5");
    }

    #[test]
    fn test_alpha_adjustment() {
        let (embeddings, texts) = create_test_data();
        let mut index = HybridIndex::new(HybridConfig::default());
        index.build(&embeddings, &texts).unwrap();

        index.set_alpha(0.8);
        let stats = index.get_stats();
        assert_eq!(stats.alpha, 0.8);

        // Clamp to valid range
        index.set_alpha(1.5);
        let stats = index.get_stats();
        assert_eq!(stats.alpha, 1.0);
    }

    #[test]
    fn test_bm25_scoring() {
        let (embeddings, texts) = create_test_data();
        let mut index = HybridIndex::new(HybridConfig::default());
        index.build(&embeddings, &texts).unwrap();

        // Search for specific term
        let results = index.keyword_search("rust", 5).unwrap();

        // doc1, doc2, doc5 contain "rust"
        let rust_docs: HashSet<&str> = results.iter().map(|r| r.entity_id.as_str()).collect();
        assert!(rust_docs.contains("doc1"));
        assert!(rust_docs.contains("doc2"));
        assert!(rust_docs.contains("doc5"));
    }

    #[test]
    fn test_empty_query() {
        let (embeddings, texts) = create_test_data();
        let mut index = HybridIndex::new(HybridConfig::default());
        index.build(&embeddings, &texts).unwrap();

        // Empty keyword query
        let results = index.keyword_search("", 3).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_missing_text_error() {
        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![0.9, 0.1, 0.0]);

        let texts: HashMap<String, String> = HashMap::new(); // Empty

        let mut index = HybridIndex::new(HybridConfig::default());
        assert!(index.build(&embeddings, &texts).is_err());
    }
}
