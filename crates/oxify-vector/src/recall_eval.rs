//! Recall Evaluation for ANN Indexes
//!
//! Tools for evaluating the quality of Approximate Nearest Neighbor (ANN) indexes
//! by comparing their results against ground truth exact search.
//!
//! ## Features
//!
//! - **Ground Truth Generation**: Generate exact search results for comparison
//! - **Recall@k Calculation**: Measure how many true nearest neighbors are found
//! - **Precision@k**: Measure accuracy of retrieved results
//! - **nDCG@k**: Normalized Discounted Cumulative Gain for ranking quality
//! - **Configuration Comparison**: Compare different index configurations
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::recall_eval::{RecallEvaluator, EvaluationConfig};
//! use oxify_vector::{HnswIndex, HnswConfig, SearchConfig, VectorSearchIndex};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create test dataset
//! let mut embeddings = HashMap::new();
//! for i in 0..1000 {
//!     let vec = vec![i as f32 * 0.01, (i * 2) as f32 * 0.01, (i * 3) as f32 * 0.01];
//!     embeddings.insert(format!("doc{}", i), vec);
//! }
//!
//! // Build exact and approximate indexes
//! let mut exact_index = VectorSearchIndex::new(SearchConfig::default());
//! exact_index.build(&embeddings)?;
//!
//! let mut hnsw_index = HnswIndex::new(HnswConfig::default());
//! hnsw_index.build(&embeddings)?;
//!
//! // Evaluate recall
//! let config = EvaluationConfig::default();
//! let evaluator = RecallEvaluator::new(config);
//!
//! let query = vec![0.5, 1.0, 1.5];
//! let metrics = evaluator.evaluate_single_query(
//!     &query,
//!     |q, k| exact_index.search(q, k),
//!     |q, k| hnsw_index.search(q, k),
//! )?;
//!
//! // metrics is a Vec<QueryMetrics>, one for each k value
//! for m in &metrics {
//!     println!("Recall@{}: {:.2}%", m.k, m.recall_at_k * 100.0);
//!     println!("Precision@{}: {:.2}%", m.k, m.precision_at_k * 100.0);
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::types::SearchResult;

/// Evaluation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationConfig {
    /// Number of top results to consider for recall calculation
    pub k_values: Vec<usize>,
    /// Whether to calculate nDCG in addition to recall/precision
    pub calculate_ndcg: bool,
    /// Number of test queries to use (for batch evaluation)
    pub num_test_queries: usize,
}

impl Default for EvaluationConfig {
    fn default() -> Self {
        Self {
            k_values: vec![1, 5, 10, 20, 50, 100],
            calculate_ndcg: true,
            num_test_queries: 100,
        }
    }
}

impl EvaluationConfig {
    /// Create config for quick evaluation
    pub fn quick() -> Self {
        Self {
            k_values: vec![10, 20],
            calculate_ndcg: false,
            num_test_queries: 10,
        }
    }

    /// Create config for comprehensive evaluation
    pub fn comprehensive() -> Self {
        Self {
            k_values: vec![1, 5, 10, 20, 50, 100, 200],
            calculate_ndcg: true,
            num_test_queries: 1000,
        }
    }
}

/// Evaluation metrics for a single query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetrics {
    /// k value (number of results)
    pub k: usize,
    /// Recall@k: Fraction of true top-k results found
    pub recall_at_k: f32,
    /// Precision@k: Fraction of returned results that are relevant
    pub precision_at_k: f32,
    /// nDCG@k: Normalized Discounted Cumulative Gain
    pub ndcg_at_k: Option<f32>,
    /// Number of true positives found
    pub true_positives: usize,
    /// Number of false positives found
    pub false_positives: usize,
}

impl QueryMetrics {
    /// Calculate F1 score (harmonic mean of precision and recall)
    pub fn f1_score(&self) -> f32 {
        if self.precision_at_k + self.recall_at_k == 0.0 {
            0.0
        } else {
            2.0 * (self.precision_at_k * self.recall_at_k)
                / (self.precision_at_k + self.recall_at_k)
        }
    }
}

/// Aggregated evaluation metrics across multiple queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetrics {
    /// k value
    pub k: usize,
    /// Average recall@k
    pub avg_recall: f32,
    /// Average precision@k
    pub avg_precision: f32,
    /// Average nDCG@k
    pub avg_ndcg: Option<f32>,
    /// Standard deviation of recall
    pub std_recall: f32,
    /// Standard deviation of precision
    pub std_precision: f32,
    /// Number of queries evaluated
    pub num_queries: usize,
}

/// Recall evaluator for comparing ANN and exact search
#[derive(Debug, Clone)]
pub struct RecallEvaluator {
    config: EvaluationConfig,
}

impl RecallEvaluator {
    /// Create a new recall evaluator
    pub fn new(config: EvaluationConfig) -> Self {
        Self { config }
    }

    /// Evaluate a single query against ground truth
    ///
    /// # Arguments
    /// * `query` - Query vector
    /// * `exact_search` - Function that performs exact search (ground truth)
    /// * `ann_search` - Function that performs approximate search
    ///
    /// # Returns
    /// Metrics for each k value in the configuration
    pub fn evaluate_single_query<F, G>(
        &self,
        query: &[f32],
        exact_search: F,
        ann_search: G,
    ) -> Result<Vec<QueryMetrics>>
    where
        F: Fn(&[f32], usize) -> Result<Vec<SearchResult>>,
        G: Fn(&[f32], usize) -> Result<Vec<SearchResult>>,
    {
        let mut metrics = Vec::new();

        for &k in &self.config.k_values {
            // Get ground truth
            let ground_truth = exact_search(query, k)?;
            let ground_truth_ids: HashSet<&str> =
                ground_truth.iter().map(|r| r.entity_id.as_str()).collect();

            // Get ANN results
            let ann_results = ann_search(query, k)?;
            let ann_ids: HashSet<&str> = ann_results.iter().map(|r| r.entity_id.as_str()).collect();

            // Calculate recall and precision
            let true_positives = ground_truth_ids.intersection(&ann_ids).count();
            let false_positives = ann_results.len().saturating_sub(true_positives);

            let recall_at_k = if !ground_truth_ids.is_empty() {
                true_positives as f32 / ground_truth_ids.len() as f32
            } else {
                0.0
            };

            let precision_at_k = if !ann_results.is_empty() {
                true_positives as f32 / ann_results.len() as f32
            } else {
                0.0
            };

            // Calculate nDCG if enabled
            let ndcg_at_k = if self.config.calculate_ndcg {
                Some(self.calculate_ndcg(&ground_truth, &ann_results, k))
            } else {
                None
            };

            metrics.push(QueryMetrics {
                k,
                recall_at_k,
                precision_at_k,
                ndcg_at_k,
                true_positives,
                false_positives,
            });
        }

        Ok(metrics)
    }

    /// Evaluate multiple queries and aggregate results
    ///
    /// # Arguments
    /// * `queries` - List of query vectors
    /// * `exact_search` - Function that performs exact search (ground truth)
    /// * `ann_search` - Function that performs approximate search
    ///
    /// # Returns
    /// Aggregated metrics for each k value
    pub fn evaluate_batch<F, G>(
        &self,
        queries: &[Vec<f32>],
        exact_search: F,
        ann_search: G,
    ) -> Result<Vec<AggregatedMetrics>>
    where
        F: Fn(&[f32], usize) -> Result<Vec<SearchResult>>,
        G: Fn(&[f32], usize) -> Result<Vec<SearchResult>>,
    {
        let mut all_metrics: Vec<Vec<QueryMetrics>> = Vec::new();

        // Evaluate each query
        for query in queries.iter().take(self.config.num_test_queries) {
            let query_metrics = self.evaluate_single_query(query, &exact_search, &ann_search)?;
            all_metrics.push(query_metrics);
        }

        // Aggregate results for each k
        let mut aggregated = Vec::new();
        for &k in &self.config.k_values {
            let metrics_for_k: Vec<&QueryMetrics> = all_metrics
                .iter()
                .filter_map(|qm| qm.iter().find(|m| m.k == k))
                .collect();

            if metrics_for_k.is_empty() {
                continue;
            }

            let recalls: Vec<f32> = metrics_for_k.iter().map(|m| m.recall_at_k).collect();
            let precisions: Vec<f32> = metrics_for_k.iter().map(|m| m.precision_at_k).collect();

            let avg_recall = recalls.iter().sum::<f32>() / recalls.len() as f32;
            let avg_precision = precisions.iter().sum::<f32>() / precisions.len() as f32;

            // Calculate standard deviations
            let variance_recall = recalls
                .iter()
                .map(|r| (r - avg_recall).powi(2))
                .sum::<f32>()
                / recalls.len() as f32;
            let std_recall = variance_recall.sqrt();

            let variance_precision = precisions
                .iter()
                .map(|p| (p - avg_precision).powi(2))
                .sum::<f32>()
                / precisions.len() as f32;
            let std_precision = variance_precision.sqrt();

            let avg_ndcg = if self.config.calculate_ndcg {
                let ndcgs: Vec<f32> = metrics_for_k.iter().filter_map(|m| m.ndcg_at_k).collect();
                if !ndcgs.is_empty() {
                    Some(ndcgs.iter().sum::<f32>() / ndcgs.len() as f32)
                } else {
                    None
                }
            } else {
                None
            };

            aggregated.push(AggregatedMetrics {
                k,
                avg_recall,
                avg_precision,
                avg_ndcg,
                std_recall,
                std_precision,
                num_queries: metrics_for_k.len(),
            });
        }

        Ok(aggregated)
    }

    /// Calculate Normalized Discounted Cumulative Gain (nDCG@k)
    fn calculate_ndcg(
        &self,
        ground_truth: &[SearchResult],
        ann_results: &[SearchResult],
        k: usize,
    ) -> f32 {
        if ground_truth.is_empty() || ann_results.is_empty() {
            return 0.0;
        }

        // Create relevance map from ground truth (position-based relevance)
        let relevance_map: std::collections::HashMap<&str, f32> = ground_truth
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let relevance = (k - i) as f32; // Higher relevance for higher-ranked items
                (r.entity_id.as_str(), relevance)
            })
            .collect();

        // Calculate DCG for ANN results
        let dcg: f32 = ann_results
            .iter()
            .take(k)
            .enumerate()
            .map(|(i, result)| {
                let relevance = relevance_map.get(result.entity_id.as_str()).unwrap_or(&0.0);
                let discount = ((i + 2) as f32).log2(); // +2 because we start from position 1, not 0
                relevance / discount
            })
            .sum();

        // Calculate IDCG (ideal DCG) - what we'd get with perfect ranking
        let idcg: f32 = ground_truth
            .iter()
            .take(k)
            .enumerate()
            .map(|(i, _)| {
                let relevance = (k - i) as f32;
                let discount = ((i + 2) as f32).log2();
                relevance / discount
            })
            .sum();

        if idcg == 0.0 {
            0.0
        } else {
            dcg / idcg
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SearchResult;

    fn create_search_results(ids: &[&str], scores: &[f32]) -> Vec<SearchResult> {
        ids.iter()
            .zip(scores.iter())
            .enumerate()
            .map(|(rank, (id, score))| SearchResult {
                entity_id: id.to_string(),
                score: *score,
                distance: 1.0 - score, // Approximate distance from score
                rank: rank + 1,
            })
            .collect()
    }

    #[test]
    fn test_perfect_recall() {
        let config = EvaluationConfig {
            k_values: vec![3],
            calculate_ndcg: false,
            num_test_queries: 10,
        };
        let evaluator = RecallEvaluator::new(config);

        let query = vec![1.0, 2.0, 3.0];

        let exact_fn = |_q: &[f32], _k: usize| {
            Ok(create_search_results(
                &["doc1", "doc2", "doc3"],
                &[0.9, 0.8, 0.7],
            ))
        };

        let ann_fn = |_q: &[f32], _k: usize| {
            Ok(create_search_results(
                &["doc1", "doc2", "doc3"],
                &[0.9, 0.8, 0.7],
            ))
        };

        let metrics = evaluator
            .evaluate_single_query(&query, exact_fn, ann_fn)
            .unwrap();

        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].k, 3);
        assert!((metrics[0].recall_at_k - 1.0).abs() < 1e-6);
        assert!((metrics[0].precision_at_k - 1.0).abs() < 1e-6);
        assert_eq!(metrics[0].true_positives, 3);
        assert_eq!(metrics[0].false_positives, 0);
    }

    #[test]
    fn test_partial_recall() {
        let config = EvaluationConfig {
            k_values: vec![3],
            calculate_ndcg: false,
            num_test_queries: 10,
        };
        let evaluator = RecallEvaluator::new(config);

        let query = vec![1.0, 2.0, 3.0];

        let exact_fn = |_q: &[f32], _k: usize| {
            Ok(create_search_results(
                &["doc1", "doc2", "doc3"],
                &[0.9, 0.8, 0.7],
            ))
        };

        // ANN returns only 2 out of 3 correct results
        let ann_fn = |_q: &[f32], _k: usize| {
            Ok(create_search_results(
                &["doc1", "doc2", "doc4"],
                &[0.9, 0.8, 0.6],
            ))
        };

        let metrics = evaluator
            .evaluate_single_query(&query, exact_fn, ann_fn)
            .unwrap();

        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].recall_at_k - 2.0 / 3.0).abs() < 1e-6); // 2 out of 3
        assert!((metrics[0].precision_at_k - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(metrics[0].true_positives, 2);
        assert_eq!(metrics[0].false_positives, 1);
    }

    #[test]
    fn test_zero_recall() {
        let config = EvaluationConfig {
            k_values: vec![3],
            calculate_ndcg: false,
            num_test_queries: 10,
        };
        let evaluator = RecallEvaluator::new(config);

        let query = vec![1.0, 2.0, 3.0];

        let exact_fn = |_q: &[f32], _k: usize| {
            Ok(create_search_results(
                &["doc1", "doc2", "doc3"],
                &[0.9, 0.8, 0.7],
            ))
        };

        // ANN returns completely different results
        let ann_fn = |_q: &[f32], _k: usize| {
            Ok(create_search_results(
                &["doc4", "doc5", "doc6"],
                &[0.6, 0.5, 0.4],
            ))
        };

        let metrics = evaluator
            .evaluate_single_query(&query, exact_fn, ann_fn)
            .unwrap();

        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].recall_at_k - 0.0).abs() < 1e-6);
        assert!((metrics[0].precision_at_k - 0.0).abs() < 1e-6);
        assert_eq!(metrics[0].true_positives, 0);
        assert_eq!(metrics[0].false_positives, 3);
    }

    #[test]
    fn test_f1_score() {
        let metrics = QueryMetrics {
            k: 10,
            recall_at_k: 0.8,
            precision_at_k: 0.6,
            ndcg_at_k: None,
            true_positives: 8,
            false_positives: 2,
        };

        let f1 = metrics.f1_score();
        let expected_f1 = 2.0 * (0.8 * 0.6) / (0.8 + 0.6);
        assert!((f1 - expected_f1).abs() < 1e-6);
    }

    #[test]
    fn test_f1_score_zero() {
        let metrics = QueryMetrics {
            k: 10,
            recall_at_k: 0.0,
            precision_at_k: 0.0,
            ndcg_at_k: None,
            true_positives: 0,
            false_positives: 10,
        };

        let f1 = metrics.f1_score();
        assert_eq!(f1, 0.0);
    }

    #[test]
    fn test_ndcg_perfect() {
        let config = EvaluationConfig {
            k_values: vec![3],
            calculate_ndcg: true,
            num_test_queries: 10,
        };
        let evaluator = RecallEvaluator::new(config);

        let ground_truth = create_search_results(&["doc1", "doc2", "doc3"], &[1.0, 0.9, 0.8]);
        let ann_results = create_search_results(&["doc1", "doc2", "doc3"], &[1.0, 0.9, 0.8]);

        let ndcg = evaluator.calculate_ndcg(&ground_truth, &ann_results, 3);
        assert!((ndcg - 1.0).abs() < 1e-6); // Perfect ranking should have nDCG = 1.0
    }

    #[test]
    fn test_ndcg_reversed() {
        let config = EvaluationConfig {
            k_values: vec![3],
            calculate_ndcg: true,
            num_test_queries: 10,
        };
        let evaluator = RecallEvaluator::new(config);

        let ground_truth = create_search_results(&["doc1", "doc2", "doc3"], &[1.0, 0.9, 0.8]);
        let ann_results = create_search_results(&["doc3", "doc2", "doc1"], &[0.8, 0.9, 1.0]); // Reversed

        let ndcg = evaluator.calculate_ndcg(&ground_truth, &ann_results, 3);
        assert!(ndcg > 0.0 && ndcg < 1.0); // Not perfect but not zero
    }

    #[test]
    fn test_batch_evaluation() {
        let config = EvaluationConfig {
            k_values: vec![3],
            calculate_ndcg: false,
            num_test_queries: 2,
        };
        let evaluator = RecallEvaluator::new(config);

        let queries = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];

        let exact_fn = |_q: &[f32], _k: usize| {
            Ok(create_search_results(
                &["doc1", "doc2", "doc3"],
                &[0.9, 0.8, 0.7],
            ))
        };

        let ann_fn = |_q: &[f32], _k: usize| {
            Ok(create_search_results(
                &["doc1", "doc2", "doc4"],
                &[0.9, 0.8, 0.6],
            ))
        };

        let aggregated = evaluator
            .evaluate_batch(&queries, exact_fn, ann_fn)
            .unwrap();

        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].k, 3);
        assert_eq!(aggregated[0].num_queries, 2);
        assert!((aggregated[0].avg_recall - 2.0 / 3.0).abs() < 1e-6);
        assert!((aggregated[0].avg_precision - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_multiple_k_values() {
        let config = EvaluationConfig {
            k_values: vec![1, 2, 3],
            calculate_ndcg: false,
            num_test_queries: 10,
        };
        let evaluator = RecallEvaluator::new(config);

        let query = vec![1.0, 2.0, 3.0];

        let exact_fn = |_q: &[f32], k: usize| {
            let all_results = create_search_results(&["doc1", "doc2", "doc3"], &[0.9, 0.8, 0.7]);
            Ok(all_results.into_iter().take(k).collect())
        };

        let ann_fn = |_q: &[f32], k: usize| {
            let all_results = create_search_results(&["doc1", "doc4", "doc5"], &[0.9, 0.7, 0.6]);
            Ok(all_results.into_iter().take(k).collect())
        };

        let metrics = evaluator
            .evaluate_single_query(&query, exact_fn, ann_fn)
            .unwrap();

        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].k, 1);
        assert_eq!(metrics[1].k, 2);
        assert_eq!(metrics[2].k, 3);

        // At k=1, we found 1/1 = 100% recall
        assert!((metrics[0].recall_at_k - 1.0).abs() < 1e-6);

        // At k=2, we found 1/2 = 50% recall
        assert!((metrics[1].recall_at_k - 0.5).abs() < 1e-6);

        // At k=3, we found 1/3 = 33.3% recall
        assert!((metrics[2].recall_at_k - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_evaluation_config_presets() {
        let quick = EvaluationConfig::quick();
        assert_eq!(quick.k_values.len(), 2);
        assert!(!quick.calculate_ndcg);
        assert_eq!(quick.num_test_queries, 10);

        let comprehensive = EvaluationConfig::comprehensive();
        assert_eq!(comprehensive.k_values.len(), 7);
        assert!(comprehensive.calculate_ndcg);
        assert_eq!(comprehensive.num_test_queries, 1000);
    }
}
