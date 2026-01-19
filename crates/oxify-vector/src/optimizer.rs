//! Query Optimizer
//!
//! Automatically selects the best search strategy based on:
//! - Dataset size (brute force vs HNSW vs IVF-PQ)
//! - Query characteristics (batch size, filter selectivity)
//! - Performance requirements (accuracy vs speed trade-off)
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::optimizer::{QueryOptimizer, OptimizerConfig, SearchStrategy};
//!
//! let config = OptimizerConfig::default();
//! let optimizer = QueryOptimizer::new(config);
//!
//! // Recommend strategy based on dataset size
//! let strategy = optimizer.recommend_strategy(1_000_000, 0.95);
//! assert_eq!(strategy, SearchStrategy::IvfPq);
//! ```

use serde::{Deserialize, Serialize};

/// Search strategy recommendation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchStrategy {
    /// Brute-force exact search (best for small datasets < 10K vectors)
    BruteForce,
    /// HNSW approximate search (best for medium datasets 10K-1M vectors)
    Hnsw,
    /// IVF-PQ approximate search (best for large datasets > 1M vectors)
    IvfPq,
    /// Distributed search across multiple shards
    Distributed,
}

/// Optimizer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerConfig {
    /// Size threshold for switching from brute-force to HNSW
    pub brute_force_threshold: usize,
    /// Size threshold for switching from HNSW to IVF-PQ
    pub hnsw_threshold: usize,
    /// Size threshold for switching to distributed search
    pub distributed_threshold: usize,
    /// Minimum recall requirement (0.0 to 1.0)
    pub min_recall: f32,
    /// Whether to enable query plan caching
    pub enable_caching: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            brute_force_threshold: 10_000,
            hnsw_threshold: 1_000_000,
            distributed_threshold: 10_000_000,
            min_recall: 0.90,
            enable_caching: true,
        }
    }
}

impl OptimizerConfig {
    /// Create config optimized for high accuracy
    pub fn high_accuracy() -> Self {
        Self {
            brute_force_threshold: 50_000, // Use exact search longer
            hnsw_threshold: 5_000_000,
            distributed_threshold: 10_000_000,
            min_recall: 0.98,
            enable_caching: true,
        }
    }

    /// Create config optimized for speed
    pub fn high_speed() -> Self {
        Self {
            brute_force_threshold: 5_000, // Switch to ANN earlier
            hnsw_threshold: 500_000,
            distributed_threshold: 5_000_000,
            min_recall: 0.80,
            enable_caching: true,
        }
    }

    /// Create config optimized for memory efficiency
    pub fn memory_efficient() -> Self {
        Self {
            brute_force_threshold: 10_000,
            hnsw_threshold: 100_000, // Use IVF-PQ earlier for compression
            distributed_threshold: 10_000_000,
            min_recall: 0.90,
            enable_caching: false, // Disable caching to save memory
        }
    }
}

/// Query optimizer for selecting search strategies
#[derive(Debug, Clone)]
pub struct QueryOptimizer {
    config: OptimizerConfig,
}

impl QueryOptimizer {
    /// Create a new query optimizer
    pub fn new(config: OptimizerConfig) -> Self {
        Self { config }
    }

    /// Recommend search strategy based on dataset size and recall requirement
    ///
    /// # Arguments
    /// * `num_vectors` - Number of vectors in the dataset
    /// * `required_recall` - Required recall (0.0 to 1.0)
    pub fn recommend_strategy(&self, num_vectors: usize, required_recall: f32) -> SearchStrategy {
        // For very high recall requirements, use exact search if feasible
        if required_recall >= 0.99 && num_vectors < self.config.brute_force_threshold * 2 {
            return SearchStrategy::BruteForce;
        }

        // Select based on dataset size
        if num_vectors < self.config.brute_force_threshold {
            SearchStrategy::BruteForce
        } else if num_vectors < self.config.hnsw_threshold {
            SearchStrategy::Hnsw
        } else if num_vectors < self.config.distributed_threshold {
            SearchStrategy::IvfPq
        } else {
            SearchStrategy::Distributed
        }
    }

    /// Estimate whether pre-filtering or post-filtering is more efficient
    ///
    /// # Arguments
    /// * `num_vectors` - Total number of vectors
    /// * `filter_selectivity` - Estimated fraction of vectors matching filter (0.0 to 1.0)
    ///
    /// # Returns
    /// `true` if pre-filtering is recommended, `false` for post-filtering
    pub fn recommend_prefiltering(&self, num_vectors: usize, filter_selectivity: f32) -> bool {
        // Pre-filtering is better when filter is highly selective (< 10% match)
        // Post-filtering is better when most vectors match

        // For small datasets, post-filtering is fine
        if num_vectors < 1000 {
            return false;
        }

        // Use pre-filtering if filter removes > 90% of vectors
        filter_selectivity < 0.10
    }

    /// Estimate optimal batch size for batch search
    ///
    /// # Arguments
    /// * `num_queries` - Number of queries to process
    /// * `num_vectors` - Number of vectors in dataset
    ///
    /// # Returns
    /// Recommended batch size
    pub fn recommend_batch_size(&self, num_queries: usize, num_vectors: usize) -> usize {
        // For small query counts, no batching needed
        if num_queries < 10 {
            return num_queries;
        }

        // Balance between parallelism and memory usage
        // Larger datasets → smaller batches to avoid memory pressure
        let base_batch_size = if num_vectors < 100_000 {
            1000
        } else if num_vectors < 1_000_000 {
            500
        } else {
            100
        };

        base_batch_size.min(num_queries)
    }

    /// Estimate search cost (relative units)
    ///
    /// Returns estimated computational cost for comparison between strategies.
    #[allow(dead_code)]
    pub fn estimate_cost(&self, strategy: SearchStrategy, num_vectors: usize, k: usize) -> f64 {
        match strategy {
            SearchStrategy::BruteForce => {
                // O(n * d) where d is dimension (assume 768)
                num_vectors as f64 * 768.0 * k as f64
            }
            SearchStrategy::Hnsw => {
                // O(log(n) * ef_search) - sub-linear
                let ef_search = 50.0;
                (num_vectors as f64).log2() * ef_search * k as f64
            }
            SearchStrategy::IvfPq => {
                // O(nprobe * cluster_size) - depends on nprobe
                let nprobe = 16.0;
                let avg_cluster_size = num_vectors as f64 / 256.0;
                nprobe * avg_cluster_size * k as f64
            }
            SearchStrategy::Distributed => {
                // Similar to IVF-PQ but with network overhead
                let nprobe = 16.0;
                let avg_cluster_size = num_vectors as f64 / 256.0;
                nprobe * avg_cluster_size * k as f64 * 1.5 // 50% network overhead
            }
        }
    }

    /// Get optimizer configuration
    pub fn config(&self) -> &OptimizerConfig {
        &self.config
    }
}

/// Query plan for execution
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// Recommended search strategy
    pub strategy: SearchStrategy,
    /// Whether to use pre-filtering
    pub use_prefiltering: bool,
    /// Recommended batch size
    pub batch_size: usize,
    /// Estimated cost
    pub estimated_cost: f64,
}

impl QueryPlan {
    /// Create a query plan
    pub fn new(
        strategy: SearchStrategy,
        use_prefiltering: bool,
        batch_size: usize,
        estimated_cost: f64,
    ) -> Self {
        Self {
            strategy,
            use_prefiltering,
            batch_size,
            estimated_cost,
        }
    }

    /// Create an optimized query plan
    pub fn optimize(
        optimizer: &QueryOptimizer,
        num_vectors: usize,
        num_queries: usize,
        k: usize,
        filter_selectivity: Option<f32>,
        required_recall: f32,
    ) -> Self {
        let strategy = optimizer.recommend_strategy(num_vectors, required_recall);
        let use_prefiltering = if let Some(selectivity) = filter_selectivity {
            optimizer.recommend_prefiltering(num_vectors, selectivity)
        } else {
            false
        };
        let batch_size = optimizer.recommend_batch_size(num_queries, num_vectors);
        let estimated_cost = optimizer.estimate_cost(strategy, num_vectors, k);

        Self::new(strategy, use_prefiltering, batch_size, estimated_cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommend_strategy_small_dataset() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());
        let strategy = optimizer.recommend_strategy(5_000, 0.95);
        assert_eq!(strategy, SearchStrategy::BruteForce);
    }

    #[test]
    fn test_recommend_strategy_medium_dataset() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());
        let strategy = optimizer.recommend_strategy(100_000, 0.95);
        assert_eq!(strategy, SearchStrategy::Hnsw);
    }

    #[test]
    fn test_recommend_strategy_large_dataset() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());
        let strategy = optimizer.recommend_strategy(2_000_000, 0.95);
        assert_eq!(strategy, SearchStrategy::IvfPq);
    }

    #[test]
    fn test_recommend_strategy_very_large_dataset() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());
        let strategy = optimizer.recommend_strategy(20_000_000, 0.95);
        assert_eq!(strategy, SearchStrategy::Distributed);
    }

    #[test]
    fn test_recommend_strategy_high_recall() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());
        // High recall requirement should prefer exact search for small enough datasets
        let strategy = optimizer.recommend_strategy(15_000, 0.99);
        assert_eq!(strategy, SearchStrategy::BruteForce);
    }

    #[test]
    fn test_recommend_prefiltering_selective() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());
        // Highly selective filter (5% match) should use pre-filtering
        let use_prefilter = optimizer.recommend_prefiltering(100_000, 0.05);
        assert!(use_prefilter);
    }

    #[test]
    fn test_recommend_prefiltering_not_selective() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());
        // Non-selective filter (50% match) should use post-filtering
        let use_prefilter = optimizer.recommend_prefiltering(100_000, 0.50);
        assert!(!use_prefilter);
    }

    #[test]
    fn test_recommend_batch_size() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());

        // Small query count
        let batch_size = optimizer.recommend_batch_size(5, 100_000);
        assert_eq!(batch_size, 5);

        // Large query count, small dataset
        let batch_size = optimizer.recommend_batch_size(2000, 50_000);
        assert_eq!(batch_size, 1000);

        // Large query count, large dataset
        let batch_size = optimizer.recommend_batch_size(2000, 2_000_000);
        assert_eq!(batch_size, 100);
    }

    #[test]
    fn test_estimate_cost() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());

        // Brute force should be most expensive for large datasets
        let brute_cost = optimizer.estimate_cost(SearchStrategy::BruteForce, 100_000, 10);
        let hnsw_cost = optimizer.estimate_cost(SearchStrategy::Hnsw, 100_000, 10);

        assert!(brute_cost > hnsw_cost);
    }

    #[test]
    fn test_high_accuracy_config() {
        let config = OptimizerConfig::high_accuracy();
        assert_eq!(config.brute_force_threshold, 50_000);
        assert_eq!(config.min_recall, 0.98);
    }

    #[test]
    fn test_high_speed_config() {
        let config = OptimizerConfig::high_speed();
        assert_eq!(config.brute_force_threshold, 5_000);
        assert_eq!(config.min_recall, 0.80);
    }

    #[test]
    fn test_memory_efficient_config() {
        let config = OptimizerConfig::memory_efficient();
        assert_eq!(config.hnsw_threshold, 100_000);
        assert!(!config.enable_caching);
    }

    #[test]
    fn test_query_plan_optimize() {
        let optimizer = QueryOptimizer::new(OptimizerConfig::default());

        // Create optimized plan
        let plan = QueryPlan::optimize(
            &optimizer,
            100_000,    // num_vectors
            50,         // num_queries
            10,         // k
            Some(0.05), // filter_selectivity
            0.95,       // required_recall
        );

        assert_eq!(plan.strategy, SearchStrategy::Hnsw);
        assert!(plan.use_prefiltering); // Selective filter
        assert!(plan.batch_size <= 50);
        assert!(plan.estimated_cost > 0.0);
    }
}
