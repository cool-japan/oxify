//! Reranking module for improving search results
//!
//! This module provides reranking capabilities using various methods:
//! - Cohere Rerank API
//! - Custom scoring functions
//! - Cross-encoder style reranking

use crate::{Result, SearchResult, VectorError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Trait for reranking providers
#[async_trait]
pub trait Reranker: Send + Sync {
    /// Rerank search results based on the query
    async fn rerank(
        &self,
        query: &str,
        results: Vec<SearchResult>,
        top_k: Option<usize>,
    ) -> Result<Vec<SearchResult>>;
}

/// Cohere reranking provider
pub struct CohereReranker {
    client: oxihttp::HttpsClient,
    api_key: String,
    model: String,
}

#[derive(Serialize)]
struct CohereRerankRequest {
    query: String,
    documents: Vec<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_n: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_documents: Option<bool>,
}

#[derive(Deserialize)]
struct CohereRerankResponse {
    results: Vec<CohereRerankResult>,
}

#[derive(Deserialize)]
struct CohereRerankResult {
    index: usize,
    relevance_score: f64,
}

impl CohereReranker {
    /// Create a new Cohere reranker
    ///
    /// # Arguments
    /// * `api_key` - Cohere API key
    /// * `model` - Model name (e.g., "rerank-english-v3.0", "rerank-multilingual-v3.0")
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for Cohere reranker"),
            api_key,
            model: model.unwrap_or_else(|| "rerank-english-v3.0".to_string()),
        }
    }
}

#[async_trait]
impl Reranker for CohereReranker {
    async fn rerank(
        &self,
        query: &str,
        results: Vec<SearchResult>,
        top_k: Option<usize>,
    ) -> Result<Vec<SearchResult>> {
        if results.is_empty() {
            return Ok(results);
        }

        // Extract text from payloads
        let documents: Vec<String> = results
            .iter()
            .map(|r| {
                r.payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        r.payload
                            .get("content")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| r.payload.to_string())
            })
            .collect();

        let request = CohereRerankRequest {
            query: query.to_string(),
            documents,
            model: self.model.clone(),
            top_n: top_k,
            return_documents: Some(false),
        };

        let response = self
            .client
            .post("https://api.cohere.ai/v1/rerank")
            .and_then(|builder| {
                builder.header("Authorization", &format!("Bearer {}", self.api_key))
            })
            .and_then(|builder| builder.header("Content-Type", "application/json"))
            .and_then(|builder| builder.json(&request))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        let status = response.status();
        let body = response
            .body_text()
            .await
            .map_err(|e| VectorError::QueryError(e.to_string()))?;

        if !status.is_success() {
            return Err(VectorError::QueryError(format!(
                "Cohere rerank failed: HTTP {}: {}",
                status, body
            )));
        }

        let cohere_response: CohereRerankResponse =
            serde_json::from_str(&body).map_err(|e| VectorError::QueryError(e.to_string()))?;

        // Reorder results based on reranking
        let mut reranked: Vec<SearchResult> = cohere_response
            .results
            .into_iter()
            .filter_map(|r| {
                results.get(r.index).map(|original| SearchResult {
                    id: original.id.clone(),
                    score: r.relevance_score,
                    payload: original.payload.clone(),
                    vector: original.vector.clone(),
                })
            })
            .collect();

        // Sort by new score descending
        reranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(reranked)
    }
}

/// Custom reranking using a scoring function
pub struct CustomReranker<F>
where
    F: Fn(&str, &SearchResult) -> f64 + Send + Sync,
{
    scorer: F,
}

impl<F> CustomReranker<F>
where
    F: Fn(&str, &SearchResult) -> f64 + Send + Sync,
{
    /// Create a new custom reranker with a scoring function
    pub fn new(scorer: F) -> Self {
        Self { scorer }
    }
}

#[async_trait]
impl<F> Reranker for CustomReranker<F>
where
    F: Fn(&str, &SearchResult) -> f64 + Send + Sync,
{
    async fn rerank(
        &self,
        query: &str,
        results: Vec<SearchResult>,
        top_k: Option<usize>,
    ) -> Result<Vec<SearchResult>> {
        let mut reranked: Vec<SearchResult> = results
            .into_iter()
            .map(|r| {
                let new_score = (self.scorer)(query, &r);
                SearchResult {
                    id: r.id,
                    score: new_score,
                    payload: r.payload,
                    vector: r.vector,
                }
            })
            .collect();

        // Sort by score descending
        reranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply top_k limit
        if let Some(k) = top_k {
            reranked.truncate(k);
        }

        Ok(reranked)
    }
}

/// Keyword boosting reranker - boosts results containing query keywords
pub struct KeywordBoostReranker {
    /// Weight multiplier for keyword matches
    boost_weight: f64,
    /// Whether to use exact match only
    exact_match: bool,
}

impl KeywordBoostReranker {
    /// Create a new keyword boost reranker
    pub fn new(boost_weight: f64, exact_match: bool) -> Self {
        Self {
            boost_weight,
            exact_match,
        }
    }

    fn count_keyword_matches(&self, query: &str, text: &str) -> usize {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        if self.exact_match {
            let query_words: Vec<&str> = query_lower.split_whitespace().collect();
            query_words
                .iter()
                .filter(|w| text_lower.split_whitespace().any(|tw| tw == **w))
                .count()
        } else {
            let query_words: Vec<&str> = query_lower.split_whitespace().collect();
            query_words
                .iter()
                .filter(|w| text_lower.contains(*w))
                .count()
        }
    }
}

#[async_trait]
impl Reranker for KeywordBoostReranker {
    async fn rerank(
        &self,
        query: &str,
        results: Vec<SearchResult>,
        top_k: Option<usize>,
    ) -> Result<Vec<SearchResult>> {
        let query_word_count = query.split_whitespace().count() as f64;
        if query_word_count == 0.0 {
            return Ok(results);
        }

        let mut reranked: Vec<SearchResult> = results
            .into_iter()
            .map(|r| {
                let text = r
                    .payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .or_else(|| r.payload.get("content").and_then(|v| v.as_str()))
                    .unwrap_or("");

                let matches = self.count_keyword_matches(query, text);
                let match_ratio = matches as f64 / query_word_count;
                let boost = 1.0 + (match_ratio * self.boost_weight);
                let new_score = r.score * boost;

                SearchResult {
                    id: r.id,
                    score: new_score,
                    payload: r.payload,
                    vector: r.vector,
                }
            })
            .collect();

        // Sort by score descending
        reranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply top_k limit
        if let Some(k) = top_k {
            reranked.truncate(k);
        }

        Ok(reranked)
    }
}

/// MMR (Maximal Marginal Relevance) reranker for diversity
pub struct MmrReranker {
    /// Balance between relevance and diversity (0.0 = max diversity, 1.0 = max relevance)
    lambda: f64,
}

impl MmrReranker {
    /// Create a new MMR reranker
    ///
    /// # Arguments
    /// * `lambda` - Balance parameter (0.0-1.0). Higher values favor relevance.
    pub fn new(lambda: f64) -> Self {
        Self {
            lambda: lambda.clamp(0.0, 1.0),
        }
    }

    fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f64 {
        if v1.len() != v2.len() || v1.is_empty() {
            return 0.0;
        }

        let dot: f64 = v1
            .iter()
            .zip(v2.iter())
            .map(|(a, b)| (*a as f64) * (*b as f64))
            .sum();
        let norm1: f64 = v1.iter().map(|a| (*a as f64).powi(2)).sum::<f64>().sqrt();
        let norm2: f64 = v2.iter().map(|a| (*a as f64).powi(2)).sum::<f64>().sqrt();

        if norm1 == 0.0 || norm2 == 0.0 {
            0.0
        } else {
            dot / (norm1 * norm2)
        }
    }
}

#[async_trait]
impl Reranker for MmrReranker {
    async fn rerank(
        &self,
        _query: &str,
        results: Vec<SearchResult>,
        top_k: Option<usize>,
    ) -> Result<Vec<SearchResult>> {
        if results.is_empty() {
            return Ok(results);
        }

        let k = top_k.unwrap_or(results.len()).min(results.len());
        let mut selected: Vec<SearchResult> = Vec::with_capacity(k);
        let mut remaining: Vec<SearchResult> = results;

        // Select first document (highest relevance) - index 0 since results are sorted
        if !remaining.is_empty() {
            selected.push(remaining.remove(0));
        }

        // Iteratively select documents that maximize MMR
        while selected.len() < k && !remaining.is_empty() {
            let mut best_idx = 0;
            let mut best_mmr = f64::NEG_INFINITY;

            for (idx, candidate) in remaining.iter().enumerate() {
                let relevance = candidate.score;

                // Calculate max similarity to already selected documents
                let max_sim = if let Some(ref cand_vec) = candidate.vector {
                    selected
                        .iter()
                        .filter_map(|s| s.vector.as_ref())
                        .map(|sel_vec| Self::cosine_similarity(cand_vec, sel_vec))
                        .fold(0.0f64, |a, b| a.max(b))
                } else {
                    0.0
                };

                // MMR = λ * relevance - (1 - λ) * max_similarity
                let mmr = self.lambda * relevance - (1.0 - self.lambda) * max_sim;

                if mmr > best_mmr {
                    best_mmr = mmr;
                    best_idx = idx;
                }
            }

            let selected_item = remaining.remove(best_idx);
            selected.push(selected_item);
        }

        Ok(selected)
    }
}

/// Chain multiple rerankers together
pub struct RerankerChain {
    rerankers: Vec<Box<dyn Reranker>>,
}

impl RerankerChain {
    /// Create a new reranker chain
    pub fn new() -> Self {
        Self {
            rerankers: Vec::new(),
        }
    }

    /// Add a reranker to the chain
    pub fn push(mut self, reranker: Box<dyn Reranker>) -> Self {
        self.rerankers.push(reranker);
        self
    }
}

impl Default for RerankerChain {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Reranker for RerankerChain {
    async fn rerank(
        &self,
        query: &str,
        mut results: Vec<SearchResult>,
        top_k: Option<usize>,
    ) -> Result<Vec<SearchResult>> {
        for reranker in &self.rerankers {
            results = reranker.rerank(query, results, None).await?;
        }

        // Apply final top_k
        if let Some(k) = top_k {
            results.truncate(k);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_keyword_boost_reranker() {
        let reranker = KeywordBoostReranker::new(0.5, false);

        let results = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.9,
                payload: serde_json::json!({ "text": "Python programming language" }),
                vector: None,
            },
            SearchResult {
                id: "2".to_string(),
                score: 0.85,
                payload: serde_json::json!({ "text": "Rust programming for systems" }),
                vector: None,
            },
            SearchResult {
                id: "3".to_string(),
                score: 0.8,
                payload: serde_json::json!({ "text": "Machine learning with Rust" }),
                vector: None,
            },
        ];

        let reranked = reranker
            .rerank("Rust programming", results, None)
            .await
            .unwrap();

        // Result with "Rust programming" should be boosted
        assert_eq!(reranked[0].id, "2"); // Has both "Rust" and "programming"
    }

    #[tokio::test]
    async fn test_custom_reranker() {
        let reranker = CustomReranker::new(|_query, result| {
            // Boost based on payload "priority" field
            result
                .payload
                .get("priority")
                .and_then(|v| v.as_f64())
                .map(|p| result.score * (1.0 + p / 10.0))
                .unwrap_or(result.score)
        });

        let results = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.8,
                payload: serde_json::json!({ "priority": 5.0 }),
                vector: None,
            },
            SearchResult {
                id: "2".to_string(),
                score: 0.9,
                payload: serde_json::json!({ "priority": 1.0 }),
                vector: None,
            },
        ];

        let reranked = reranker.rerank("test", results, None).await.unwrap();

        // Result 1 has lower score but higher priority, should be reranked up
        assert_eq!(reranked[0].id, "1"); // 0.8 * 1.5 = 1.2
        assert_eq!(reranked[1].id, "2"); // 0.9 * 1.1 = 0.99
    }

    #[tokio::test]
    async fn test_mmr_reranker() {
        let reranker = MmrReranker::new(0.5);

        let results = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.9,
                payload: serde_json::json!({}),
                vector: Some(vec![1.0, 0.0, 0.0]),
            },
            SearchResult {
                id: "2".to_string(),
                score: 0.85,
                payload: serde_json::json!({}),
                vector: Some(vec![0.99, 0.1, 0.0]), // Very similar to 1
            },
            SearchResult {
                id: "3".to_string(),
                score: 0.8,
                payload: serde_json::json!({}),
                vector: Some(vec![0.0, 1.0, 0.0]), // Different direction
            },
        ];

        let reranked = reranker.rerank("test", results, Some(2)).await.unwrap();

        assert_eq!(reranked.len(), 2);
        // First should be highest relevance
        assert_eq!(reranked[0].id, "1");
        // Second should prefer diversity (doc 3) over similar doc 2
        assert_eq!(reranked[1].id, "3");
    }
}
