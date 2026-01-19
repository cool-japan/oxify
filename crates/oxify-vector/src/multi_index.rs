//! Multi-Index Search
//!
//! Search across multiple indexes in parallel and combine results.
//!
//! ## Use Cases
//! - Federated search across data shards
//! - Searching different index types (exact + approximate)
//! - Temporal data with separate indexes per time period
//! - Multi-tenant scenarios with per-tenant indexes
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::{MultiIndexSearch, VectorSearchIndex, SearchConfig};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create multiple indexes
//! let mut index1 = VectorSearchIndex::new(SearchConfig::default());
//! let mut embeddings1 = HashMap::new();
//! embeddings1.insert("doc1".to_string(), vec![1.0, 0.0]);
//! index1.build(&embeddings1)?;
//!
//! let mut index2 = VectorSearchIndex::new(SearchConfig::default());
//! let mut embeddings2 = HashMap::new();
//! embeddings2.insert("doc2".to_string(), vec![0.0, 1.0]);
//! index2.build(&embeddings2)?;
//!
//! // Search across both indexes
//! let multi_search = MultiIndexSearch::new();
//! let query = vec![0.5, 0.5];
//! let results = multi_search.search(&[&index1, &index2], &query, 10)?;
//!
//! // Results are merged and sorted by score
//! for result in results {
//!     println!("{}: score = {:.4}", result.entity_id, result.score);
//! }
//! # Ok(())
//! # }
//! ```

use crate::search::VectorSearchIndex;
use crate::types::SearchResult;
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// Configuration for multi-index search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiIndexConfig {
    /// Whether to search indexes in parallel
    pub parallel: bool,
    /// Whether to deduplicate results across indexes
    pub deduplicate: bool,
    /// How to merge scores from different indexes
    pub merge_strategy: ScoreMergeStrategy,
}

impl Default for MultiIndexConfig {
    fn default() -> Self {
        Self {
            parallel: true,
            deduplicate: true,
            merge_strategy: ScoreMergeStrategy::Max,
        }
    }
}

/// Strategy for merging scores when same entity appears in multiple indexes
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ScoreMergeStrategy {
    /// Take maximum score
    Max,
    /// Take minimum score
    Min,
    /// Average scores
    Average,
    /// Take first occurrence
    First,
}

/// Multi-index search coordinator
#[derive(Debug, Clone)]
pub struct MultiIndexSearch {
    config: MultiIndexConfig,
}

impl MultiIndexSearch {
    /// Create a new multi-index search with default config
    pub fn new() -> Self {
        Self {
            config: MultiIndexConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: MultiIndexConfig) -> Self {
        Self { config }
    }

    /// Search across multiple indexes
    pub fn search(
        &self,
        indexes: &[&VectorSearchIndex],
        query: &[f32],
        k: usize,
    ) -> Result<Vec<SearchResult>> {
        if indexes.is_empty() {
            return Err(anyhow!("Cannot search across zero indexes"));
        }

        info!("Searching across {} indexes", indexes.len());

        // Search each index
        let all_results: Vec<Vec<SearchResult>> = if self.config.parallel {
            indexes
                .par_iter()
                .map(|index| index.search(query, k).unwrap_or_default())
                .collect()
        } else {
            indexes
                .iter()
                .map(|index| index.search(query, k).unwrap_or_default())
                .collect()
        };

        // Merge results
        let merged = self.merge_results(all_results, k);

        info!("Multi-index search returned {} results", merged.len());
        Ok(merged)
    }

    /// Batch search across multiple indexes
    pub fn batch_search(
        &self,
        indexes: &[&VectorSearchIndex],
        queries: &[Vec<f32>],
        k: usize,
    ) -> Result<Vec<Vec<SearchResult>>> {
        if indexes.is_empty() {
            return Err(anyhow!("Cannot search across zero indexes"));
        }

        info!(
            "Batch searching {} queries across {} indexes",
            queries.len(),
            indexes.len()
        );

        // Search all queries
        let results: Vec<Vec<SearchResult>> = if self.config.parallel {
            queries
                .par_iter()
                .map(|query| self.search(indexes, query, k).unwrap_or_default())
                .collect()
        } else {
            queries
                .iter()
                .map(|query| self.search(indexes, query, k).unwrap_or_default())
                .collect()
        };

        Ok(results)
    }

    /// Merge results from multiple indexes
    fn merge_results(&self, all_results: Vec<Vec<SearchResult>>, k: usize) -> Vec<SearchResult> {
        if !self.config.deduplicate {
            // Simple concatenation and sorting
            let mut merged: Vec<SearchResult> = all_results.into_iter().flatten().collect();
            merged.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            merged.truncate(k);

            // Re-rank
            for (i, result) in merged.iter_mut().enumerate() {
                result.rank = i + 1;
            }

            return merged;
        }

        // Deduplicate by entity_id and merge scores
        let mut entity_scores: HashMap<String, Vec<f32>> = HashMap::new();
        let mut entity_distance: HashMap<String, Vec<f32>> = HashMap::new();

        for results in all_results {
            for result in results {
                entity_scores
                    .entry(result.entity_id.clone())
                    .or_default()
                    .push(result.score);
                entity_distance
                    .entry(result.entity_id.clone())
                    .or_default()
                    .push(result.distance);
            }
        }

        // Merge scores according to strategy
        let mut merged: Vec<SearchResult> = entity_scores
            .into_iter()
            .map(|(entity_id, scores)| {
                let merged_score = match self.config.merge_strategy {
                    ScoreMergeStrategy::Max => {
                        scores.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                    }
                    ScoreMergeStrategy::Min => scores.iter().copied().fold(f32::INFINITY, f32::min),
                    ScoreMergeStrategy::Average => scores.iter().sum::<f32>() / scores.len() as f32,
                    ScoreMergeStrategy::First => scores[0],
                };

                let merged_distance = match self.config.merge_strategy {
                    ScoreMergeStrategy::Max => entity_distance[&entity_id]
                        .iter()
                        .copied()
                        .fold(f32::INFINITY, f32::min),
                    ScoreMergeStrategy::Min => entity_distance[&entity_id]
                        .iter()
                        .copied()
                        .fold(f32::NEG_INFINITY, f32::max),
                    ScoreMergeStrategy::Average => {
                        entity_distance[&entity_id].iter().sum::<f32>()
                            / entity_distance[&entity_id].len() as f32
                    }
                    ScoreMergeStrategy::First => entity_distance[&entity_id][0],
                };

                SearchResult {
                    entity_id,
                    score: merged_score,
                    distance: merged_distance,
                    rank: 0, // Will be set later
                }
            })
            .collect();

        // Sort by merged score
        merged.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top-k and set ranks
        merged.truncate(k);
        for (i, result) in merged.iter_mut().enumerate() {
            result.rank = i + 1;
        }

        debug!("Merged and deduplicated to {} results", merged.len());
        merged
    }

    /// Get configuration
    pub fn config(&self) -> &MultiIndexConfig {
        &self.config
    }
}

impl Default for MultiIndexSearch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SearchConfig;
    use std::collections::HashMap;

    fn create_test_index(id_prefix: &str, count: usize, dim: usize) -> VectorSearchIndex {
        let mut embeddings = HashMap::new();
        for i in 0..count {
            let vec: Vec<f32> = (0..dim).map(|j| (i + j) as f32 * 0.1).collect();
            embeddings.insert(format!("{}_{}", id_prefix, i), vec);
        }

        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();
        index
    }

    #[test]
    fn test_multi_index_search() {
        let index1 = create_test_index("doc", 5, 3);
        let index2 = create_test_index("article", 5, 3);

        let multi_search = MultiIndexSearch::new();
        let query = vec![0.1, 0.2, 0.3];
        let results = multi_search.search(&[&index1, &index2], &query, 5).unwrap();

        assert!(results.len() <= 5);
        assert!(!results.is_empty());

        // Verify results are sorted by score
        for i in 1..results.len() {
            assert!(results[i - 1].score >= results[i].score);
        }
    }

    #[test]
    fn test_multi_index_deduplication() {
        // Create two indexes with overlapping entities
        let mut embeddings1 = HashMap::new();
        embeddings1.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        embeddings1.insert("doc2".to_string(), vec![0.9, 0.1, 0.0]);
        let mut index1 = VectorSearchIndex::new(SearchConfig::default());
        index1.build(&embeddings1).unwrap();

        let mut embeddings2 = HashMap::new();
        embeddings2.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]); // Duplicate
        embeddings2.insert("doc3".to_string(), vec![0.8, 0.2, 0.0]);
        let mut index2 = VectorSearchIndex::new(SearchConfig::default());
        index2.build(&embeddings2).unwrap();

        let config = MultiIndexConfig {
            parallel: false,
            deduplicate: true,
            merge_strategy: ScoreMergeStrategy::Max,
        };

        let multi_search = MultiIndexSearch::with_config(config);
        let query = vec![1.0, 0.0, 0.0];
        let results = multi_search
            .search(&[&index1, &index2], &query, 10)
            .unwrap();

        // Should have 3 unique entities (doc1, doc2, doc3)
        assert_eq!(results.len(), 3);

        let entity_ids: Vec<String> = results.iter().map(|r| r.entity_id.clone()).collect();
        assert!(entity_ids.contains(&"doc1".to_string()));
        assert!(entity_ids.contains(&"doc2".to_string()));
        assert!(entity_ids.contains(&"doc3".to_string()));
    }

    #[test]
    fn test_multi_index_no_deduplication() {
        let mut embeddings1 = HashMap::new();
        embeddings1.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        let mut index1 = VectorSearchIndex::new(SearchConfig::default());
        index1.build(&embeddings1).unwrap();

        let mut embeddings2 = HashMap::new();
        embeddings2.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]); // Duplicate
        let mut index2 = VectorSearchIndex::new(SearchConfig::default());
        index2.build(&embeddings2).unwrap();

        let config = MultiIndexConfig {
            parallel: false,
            deduplicate: false,
            merge_strategy: ScoreMergeStrategy::Max,
        };

        let multi_search = MultiIndexSearch::with_config(config);
        let query = vec![1.0, 0.0, 0.0];
        let results = multi_search
            .search(&[&index1, &index2], &query, 10)
            .unwrap();

        // Without deduplication, we get both occurrences
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_merge_strategy_max() {
        let config = MultiIndexConfig {
            parallel: false,
            deduplicate: true,
            merge_strategy: ScoreMergeStrategy::Max,
        };

        let multi_search = MultiIndexSearch::with_config(config);

        // The merge logic is tested indirectly through search
        // This test just verifies the config is set correctly
        assert_eq!(
            multi_search.config().merge_strategy,
            ScoreMergeStrategy::Max
        );
    }

    #[test]
    fn test_batch_search() {
        let index1 = create_test_index("doc", 5, 3);
        let index2 = create_test_index("article", 5, 3);

        let multi_search = MultiIndexSearch::new();
        let queries = vec![
            vec![0.1, 0.2, 0.3],
            vec![0.2, 0.3, 0.4],
            vec![0.3, 0.4, 0.5],
        ];

        let results = multi_search
            .batch_search(&[&index1, &index2], &queries, 3)
            .unwrap();

        assert_eq!(results.len(), 3);
        for result_set in results {
            assert!(result_set.len() <= 3);
        }
    }

    #[test]
    fn test_empty_indexes() {
        let multi_search = MultiIndexSearch::new();
        let query = vec![0.1, 0.2, 0.3];
        let result = multi_search.search(&[], &query, 10);

        assert!(result.is_err());
    }
}
