//! # OxiFY Vector
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY (simplified for LLM workflow focus)
//! License: MIT OR Apache-2.0 (compatible with OxiRS)
//!
//! In-memory vector search and similarity operations for RAG and embeddings.
//!
//! This crate provides:
//! - Vector similarity search (cosine, euclidean, dot product, manhattan)
//! - Brute-force exact search for small to medium datasets
//! - Batch search for multiple queries
//! - Radius search for finding neighbors within distance
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::{VectorSearchIndex, SearchConfig, DistanceMetric};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create embeddings
//! let mut embeddings = HashMap::new();
//! embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
//! embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
//! embeddings.insert("doc3".to_string(), vec![0.3, 0.4, 0.5]);
//!
//! // Build search index
//! let config = SearchConfig::default();
//! let mut index = VectorSearchIndex::new(config);
//! index.build(&embeddings)?;
//!
//! // Search for similar documents
//! let query = vec![0.15, 0.25, 0.35];
//! let results = index.search(&query, 2)?;
//!
//! for result in results {
//!     println!("{}: score = {:.4}", result.entity_id, result.score);
//! }
//! # Ok(())
//! # }
//! ```

pub mod adaptive;
pub mod cache;
pub mod colbert;
pub mod distributed;
pub mod embeddings;
pub mod filter;
pub mod gpu;
pub mod hnsw;
pub mod hybrid;
pub mod ivf;
pub mod lsh;
pub mod metrics;
pub mod multi_index;
pub mod optimizer;
pub mod otel;
pub mod persistence;
pub mod profiling;
pub mod quantization;
pub mod recall_eval;
pub mod search;
pub mod simd;
pub mod types;

// Re-export commonly used types
pub use adaptive::{AdaptiveConfig, AdaptiveIndex, AdaptiveStats};
pub use cache::{CacheConfig, CacheStats, QueryCache};
pub use colbert::{ColbertConfig, ColbertIndex, ColbertSearchResult, ColbertStats, MultiVectorDoc};
pub use distributed::{ConsistentHash, DistributedIndex, DistributedStats, ShardConfig};
pub use embeddings::{
    CachedEmbeddingProvider, EmbeddingCache, EmbeddingProvider, MockEmbeddingProvider,
    OpenAIConfig, OpenAIEmbeddingProvider,
};
pub use filter::{Filter, FilterCondition, FilterValue, Metadata};
pub use gpu::{GpuBatchProcessor, GpuConfig, GpuStats};
pub use hnsw::{HnswConfig, HnswIndex, HnswStats};
pub use hybrid::{HybridConfig, HybridIndex, HybridSearchResult, HybridStats};
pub use ivf::{IvfPqConfig, IvfPqIndex, IvfPqStats};
pub use lsh::{LshConfig, LshIndex, LshStats};
pub use metrics::{IndexStats as MetricsIndexStats, LatencyTimer, Metrics, SearchStats};
pub use multi_index::{MultiIndexConfig, MultiIndexSearch, ScoreMergeStrategy};
pub use optimizer::{OptimizerConfig, QueryOptimizer, QueryPlan, SearchStrategy};
pub use otel::{init_tracing, shutdown_tracing, TracingConfig};
pub use profiling::{
    Bottleneck, ImpactLevel, IndexHealthChecker, ProfilingConfig, QueryProfile, QueryProfiler,
    Recommendation,
};
pub use quantization::{
    BinaryQuantizationConfig, BinaryQuantizedIndex, BinaryQuantizedIndexStats, BinaryQuantizer,
    FourBitQuantizedIndex, FourBitQuantizedIndexStats, FourBitQuantizer, QuantizationConfig,
    QuantizedIndexStats, QuantizedVectorIndex, ScalarQuantizer,
};
pub use recall_eval::{AggregatedMetrics, EvaluationConfig, QueryMetrics, RecallEvaluator};
pub use search::VectorSearchIndex;
pub use types::{DistanceMetric, IndexStats, SearchConfig, SearchResult};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_basic_search() {
        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        embeddings.insert("doc2".to_string(), vec![0.0, 1.0, 0.0]);

        let config = SearchConfig::default();
        let mut index = VectorSearchIndex::new(config);
        assert!(index.build(&embeddings).is_ok());

        let query = vec![0.9, 0.1, 0.0];
        let results = index.search(&query, 1).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_id, "doc1");
    }
}
