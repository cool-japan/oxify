//! Vector search types and configuration
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY (simplified for LLM workflow focus)
//! License: MIT OR Apache-2.0 (compatible with OxiRS)

use serde::{Deserialize, Serialize};

/// Distance metric for vector search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistanceMetric {
    /// Cosine similarity (normalized dot product)
    Cosine,
    /// Euclidean distance (L2 norm)
    Euclidean,
    /// Dot product similarity
    DotProduct,
    /// Manhattan distance (L1 norm)
    Manhattan,
}

/// Vector search configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Distance metric to use
    pub metric: DistanceMetric,
    /// Enable parallel search
    pub parallel: bool,
    /// Normalize vectors before search
    pub normalize: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            metric: DistanceMetric::Cosine,
            parallel: true,
            normalize: true,
        }
    }
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Entity ID
    pub entity_id: String,
    /// Similarity score (higher is better)
    pub score: f32,
    /// Distance (lower is better, depends on metric)
    pub distance: f32,
    /// Rank in results (1-indexed)
    pub rank: usize,
}

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Number of entities in index
    pub num_entities: usize,
    /// Embedding dimensions
    pub dimensions: usize,
    /// Whether index is built
    pub is_built: bool,
    /// Distance metric
    pub metric: DistanceMetric,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_metric() {
        let metric = DistanceMetric::Cosine;
        assert_eq!(metric, DistanceMetric::Cosine);
    }

    #[test]
    fn test_search_config_default() {
        let config = SearchConfig::default();
        assert_eq!(config.metric, DistanceMetric::Cosine);
        assert!(config.parallel);
        assert!(config.normalize);
    }
}
