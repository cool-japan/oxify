//! Adaptive Index - Automatic Performance Optimization
//!
//! A high-level API that automatically selects and switches between index types
//! based on dataset size and query patterns for optimal performance.
//!
//! ## Features
//!
//! - **Automatic index selection**: Starts with brute-force, upgrades to HNSW/IVF-PQ as needed
//! - **Performance tracking**: Monitors search latency and automatically optimizes
//! - **Seamless transitions**: Transparently switches between index types
//! - **Simple API**: Single interface for all index types
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::adaptive::{AdaptiveIndex, AdaptiveConfig};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create adaptive index with automatic optimization
//! let mut index = AdaptiveIndex::new(AdaptiveConfig::default());
//!
//! // Build from embeddings
//! let mut embeddings = HashMap::new();
//! embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
//! embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
//! index.build(&embeddings)?;
//!
//! // Search - automatically uses best index type
//! let query = vec![0.15, 0.25, 0.35];
//! let results = index.search(&query, 10)?;
//!
//! // Add more vectors - may trigger index upgrade
//! index.add_vector("doc3".to_string(), vec![0.3, 0.4, 0.5])?;
//!
//! // Check current strategy
//! println!("Using strategy: {:?}", index.current_strategy());
//! # Ok(())
//! # }
//! ```

use crate::hnsw::{HnswConfig, HnswIndex};
use crate::optimizer::{OptimizerConfig, QueryOptimizer, SearchStrategy};
use crate::search::VectorSearchIndex;
use crate::types::{DistanceMetric, SearchConfig, SearchResult};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Configuration for adaptive index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveConfig {
    /// Distance metric to use
    pub metric: DistanceMetric,
    /// Whether to normalize vectors
    pub normalize: bool,
    /// Minimum recall target (0.0 to 1.0)
    pub min_recall: f32,
    /// Whether to automatically upgrade index type
    pub auto_upgrade: bool,
    /// Latency threshold for triggering upgrades (milliseconds)
    pub latency_threshold_ms: u64,
    /// Number of searches to track for performance stats
    pub stats_window: usize,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            metric: DistanceMetric::Cosine,
            normalize: true,
            min_recall: 0.95,
            auto_upgrade: true,
            latency_threshold_ms: 10, // 10ms threshold
            stats_window: 100,
        }
    }
}

impl AdaptiveConfig {
    /// Create config optimized for high accuracy
    pub fn high_accuracy() -> Self {
        Self {
            min_recall: 0.99,
            latency_threshold_ms: 50, // More lenient
            ..Default::default()
        }
    }

    /// Create config optimized for low latency
    pub fn low_latency() -> Self {
        Self {
            min_recall: 0.90,
            latency_threshold_ms: 5, // Aggressive
            auto_upgrade: true,
            ..Default::default()
        }
    }
}

/// Internal index implementation
enum IndexImpl {
    BruteForce(VectorSearchIndex),
    Hnsw(HnswIndex),
}

/// Adaptive index that automatically optimizes performance
pub struct AdaptiveIndex {
    config: AdaptiveConfig,
    optimizer: QueryOptimizer,
    index: Option<IndexImpl>,
    num_vectors: usize,
    dimensions: usize,
    /// Performance tracking
    recent_latencies: Vec<Duration>,
    total_searches: usize,
    embeddings_cache: HashMap<String, Vec<f32>>,
}

impl AdaptiveIndex {
    /// Create a new adaptive index
    pub fn new(config: AdaptiveConfig) -> Self {
        let optimizer_config = OptimizerConfig {
            min_recall: config.min_recall,
            ..OptimizerConfig::default()
        };

        Self {
            config,
            optimizer: QueryOptimizer::new(optimizer_config),
            index: None,
            num_vectors: 0,
            dimensions: 0,
            recent_latencies: Vec::new(),
            total_searches: 0,
            embeddings_cache: HashMap::new(),
        }
    }

    /// Build index from embeddings
    pub fn build(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        if embeddings.is_empty() {
            return Err(anyhow!("Cannot build index from empty embeddings"));
        }

        self.num_vectors = embeddings.len();
        self.dimensions = embeddings.values().next().expect("invariant: embeddings non-empty from guard").len();
        self.embeddings_cache = embeddings.clone();

        info!(
            "Building adaptive index with {} vectors, {} dimensions",
            self.num_vectors, self.dimensions
        );

        // Select initial strategy
        let strategy = self
            .optimizer
            .recommend_strategy(self.num_vectors, self.config.min_recall);

        self.build_with_strategy(embeddings, strategy)?;

        Ok(())
    }

    /// Build index with specific strategy
    fn build_with_strategy(
        &mut self,
        embeddings: &HashMap<String, Vec<f32>>,
        strategy: SearchStrategy,
    ) -> Result<()> {
        info!("Building index with strategy: {:?}", strategy);

        match strategy {
            SearchStrategy::BruteForce => {
                let mut index = VectorSearchIndex::new(SearchConfig {
                    metric: self.config.metric,
                    normalize: self.config.normalize,
                    parallel: true,
                });
                index.build(embeddings)?;
                self.index = Some(IndexImpl::BruteForce(index));
            }
            SearchStrategy::Hnsw => {
                let mut index = HnswIndex::new(HnswConfig::default());
                index.build(embeddings)?;
                self.index = Some(IndexImpl::Hnsw(index));
            }
            _ => {
                // Fall back to HNSW for IVF-PQ and Distributed
                warn!("Strategy {:?} not yet implemented, using HNSW", strategy);
                let mut index = HnswIndex::new(HnswConfig::default());
                index.build(embeddings)?;
                self.index = Some(IndexImpl::Hnsw(index));
            }
        }

        Ok(())
    }

    /// Search for k nearest neighbors
    pub fn search(&mut self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        let index = self
            .index
            .as_ref()
            .ok_or_else(|| anyhow!("Index not built"))?;

        let start = Instant::now();

        let results = match index {
            IndexImpl::BruteForce(idx) => idx.search(query, k)?,
            IndexImpl::Hnsw(idx) => idx.search(query, k)?,
        };

        let elapsed = start.elapsed();

        // Track performance
        self.track_search_latency(elapsed);

        // Check if upgrade needed
        if self.config.auto_upgrade {
            self.check_and_upgrade()?;
        }

        Ok(results)
    }

    /// Add a single vector
    pub fn add_vector(&mut self, entity_id: String, embedding: Vec<f32>) -> Result<()> {
        // Add to cache
        self.embeddings_cache
            .insert(entity_id.clone(), embedding.clone());
        self.num_vectors += 1;

        // Add to current index
        if let Some(index) = &mut self.index {
            match index {
                IndexImpl::BruteForce(idx) => {
                    idx.add_vector(entity_id, embedding)?;
                }
                IndexImpl::Hnsw(_) => {
                    // HNSW doesn't support incremental updates efficiently
                    // Rebuild if auto_upgrade is enabled
                    if self.config.auto_upgrade {
                        debug!("HNSW doesn't support incremental updates, checking for rebuild");
                    }
                }
            }
        }

        // Check if strategy change needed
        if self.config.auto_upgrade {
            self.check_and_upgrade()?;
        }

        Ok(())
    }

    /// Add multiple vectors
    pub fn add_vectors(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        for (id, emb) in embeddings {
            self.embeddings_cache.insert(id.clone(), emb.clone());
        }
        self.num_vectors += embeddings.len();

        if let Some(index) = &mut self.index {
            match index {
                IndexImpl::BruteForce(idx) => {
                    idx.add_vectors(embeddings)?;
                }
                IndexImpl::Hnsw(_) => {
                    // HNSW rebuild needed
                    if self.config.auto_upgrade {
                        debug!("HNSW batch insert requires rebuild");
                    }
                }
            }
        }

        if self.config.auto_upgrade {
            self.check_and_upgrade()?;
        }

        Ok(())
    }

    /// Track search latency
    fn track_search_latency(&mut self, duration: Duration) {
        self.total_searches += 1;
        self.recent_latencies.push(duration);

        // Keep only recent window
        if self.recent_latencies.len() > self.config.stats_window {
            self.recent_latencies.remove(0);
        }
    }

    /// Check if index upgrade is needed
    fn check_and_upgrade(&mut self) -> Result<()> {
        // Get current and recommended strategies
        let current_strategy = self.current_strategy();
        let recommended_strategy = self
            .optimizer
            .recommend_strategy(self.num_vectors, self.config.min_recall);

        // Check if upgrade needed based on dataset size
        if current_strategy != recommended_strategy {
            info!(
                "Dataset size changed, upgrading from {:?} to {:?}",
                current_strategy, recommended_strategy
            );
            self.build_with_strategy(&self.embeddings_cache.clone(), recommended_strategy)?;
            return Ok(());
        }

        // Check if upgrade needed based on latency
        if !self.recent_latencies.is_empty() {
            let avg_latency =
                self.recent_latencies.iter().sum::<Duration>() / self.recent_latencies.len() as u32;

            if avg_latency.as_millis() as u64 > self.config.latency_threshold_ms {
                warn!(
                    "Average latency {}ms exceeds threshold {}ms",
                    avg_latency.as_millis(),
                    self.config.latency_threshold_ms
                );

                // Upgrade if not already using best strategy
                if current_strategy == SearchStrategy::BruteForce && self.num_vectors > 1000 {
                    info!("Upgrading to HNSW due to high latency");
                    self.build_with_strategy(&self.embeddings_cache.clone(), SearchStrategy::Hnsw)?;
                }
            }
        }

        Ok(())
    }

    /// Get current search strategy
    pub fn current_strategy(&self) -> SearchStrategy {
        match &self.index {
            Some(IndexImpl::BruteForce(_)) => SearchStrategy::BruteForce,
            Some(IndexImpl::Hnsw(_)) => SearchStrategy::Hnsw,
            None => SearchStrategy::BruteForce, // Default
        }
    }

    /// Get performance statistics
    pub fn stats(&self) -> AdaptiveStats {
        let avg_latency = if !self.recent_latencies.is_empty() {
            self.recent_latencies.iter().sum::<Duration>() / self.recent_latencies.len() as u32
        } else {
            Duration::ZERO
        };

        let p95_latency = if !self.recent_latencies.is_empty() {
            let mut sorted = self.recent_latencies.clone();
            sorted.sort();
            let p95_idx = (sorted.len() as f32 * 0.95) as usize;
            sorted.get(p95_idx).copied().unwrap_or(Duration::ZERO)
        } else {
            Duration::ZERO
        };

        AdaptiveStats {
            num_vectors: self.num_vectors,
            dimensions: self.dimensions,
            current_strategy: self.current_strategy(),
            total_searches: self.total_searches,
            avg_latency_ms: avg_latency.as_secs_f64() * 1000.0,
            p95_latency_ms: p95_latency.as_secs_f64() * 1000.0,
        }
    }

    /// Get number of vectors
    #[inline]
    pub fn len(&self) -> usize {
        self.num_vectors
    }

    /// Check if index is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.num_vectors == 0
    }
}

/// Performance statistics for adaptive index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveStats {
    pub num_vectors: usize,
    pub dimensions: usize,
    pub current_strategy: SearchStrategy,
    pub total_searches: usize,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_embeddings(count: usize, dim: usize) -> HashMap<String, Vec<f32>> {
        let mut embeddings = HashMap::new();
        for i in 0..count {
            let vec: Vec<f32> = (0..dim).map(|j| (i + j) as f32 * 0.1).collect();
            embeddings.insert(format!("doc_{}", i), vec);
        }
        embeddings
    }

    #[test]
    fn test_adaptive_index_small_dataset() {
        let embeddings = create_test_embeddings(100, 3);
        let mut index = AdaptiveIndex::new(AdaptiveConfig::default());

        index.build(&embeddings).unwrap();

        // Should use brute force for small dataset
        assert_eq!(index.current_strategy(), SearchStrategy::BruteForce);

        let query = vec![0.1, 0.2, 0.3];
        let results = index.search(&query, 5).unwrap();

        assert!(results.len() <= 5);
        assert!(!results.is_empty());
    }

    #[test]
    #[ignore = "slow HNSW construction benchmark - run with --ignored"]
    fn test_adaptive_index_medium_dataset() {
        // Use 11000 vectors (just above brute_force_threshold of 10000 to trigger HNSW)
        let embeddings = create_test_embeddings(11000, 3);
        let mut index = AdaptiveIndex::new(AdaptiveConfig::default());

        index.build(&embeddings).unwrap();

        // Should use HNSW for medium dataset
        assert_eq!(index.current_strategy(), SearchStrategy::Hnsw);

        let query = vec![0.1, 0.2, 0.3];
        let results = index.search(&query, 10).unwrap();

        assert!(results.len() <= 10);
    }

    #[test]
    fn test_adaptive_index_incremental_add() {
        let embeddings = create_test_embeddings(50, 3);
        let mut index = AdaptiveIndex::new(AdaptiveConfig::default());

        index.build(&embeddings).unwrap();

        assert_eq!(index.len(), 50);

        // Add vector
        index
            .add_vector("new_doc".to_string(), vec![0.9, 0.9, 0.9])
            .unwrap();

        assert_eq!(index.len(), 51);
    }

    #[test]
    fn test_adaptive_stats() {
        let embeddings = create_test_embeddings(100, 3);
        let mut index = AdaptiveIndex::new(AdaptiveConfig::default());

        index.build(&embeddings).unwrap();

        // Do some searches
        let query = vec![0.1, 0.2, 0.3];
        for _ in 0..10 {
            let _ = index.search(&query, 5);
        }

        let stats = index.stats();
        assert_eq!(stats.num_vectors, 100);
        assert_eq!(stats.dimensions, 3);
        assert_eq!(stats.total_searches, 10);
        assert!(stats.avg_latency_ms >= 0.0);
    }

    #[test]
    fn test_adaptive_config_presets() {
        let high_acc = AdaptiveConfig::high_accuracy();
        assert_eq!(high_acc.min_recall, 0.99);

        let low_lat = AdaptiveConfig::low_latency();
        assert_eq!(low_lat.latency_threshold_ms, 5);
    }

    #[test]
    fn test_adaptive_index_empty() {
        let index = AdaptiveIndex::new(AdaptiveConfig::default());
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }
}
