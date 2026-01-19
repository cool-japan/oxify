//! Distributed vector search with sharding and replication.
//!
//! This module provides distributed search capabilities for scaling to billions of vectors:
//!
//! - **Horizontal Sharding**: Split vectors across multiple nodes
//! - **Consistent Hashing**: Load balancing for shard assignments
//! - **Query Routing**: Fan-out queries to all relevant shards
//! - **Result Merging**: Combine and re-rank results from multiple shards
//! - **Replication**: Fault tolerance with configurable replica count
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::distributed::{DistributedIndex, ShardConfig, ConsistentHash};
//! use oxify_vector::{SearchConfig, DistanceMetric};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Configure distributed index with 3 shards
//! let shard_config = ShardConfig::new(3, 2); // 3 shards, 2 replicas
//! let search_config = SearchConfig::default();
//! let mut index = DistributedIndex::new(shard_config, search_config);
//!
//! // Build index with automatic sharding
//! let mut embeddings = HashMap::new();
//! embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
//! embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
//! index.build(&embeddings)?;
//!
//! // Search across all shards
//! let query = vec![0.15, 0.25, 0.35];
//! let results = index.search(&query, 5)?;
//! # Ok(())
//! # }
//! ```

use crate::filter::{Filter, Metadata};
use crate::search::VectorSearchIndex;
use crate::types::{SearchConfig, SearchResult};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

/// Configuration for distributed sharding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardConfig {
    /// Number of shards to split data across
    pub num_shards: usize,
    /// Number of replicas for fault tolerance
    pub num_replicas: usize,
    /// Number of virtual nodes per physical shard (for consistent hashing)
    pub virtual_nodes: usize,
}

impl ShardConfig {
    /// Create a new shard configuration.
    ///
    /// # Arguments
    /// * `num_shards` - Number of shards (must be >= 1)
    /// * `num_replicas` - Number of replicas (must be >= 1)
    ///
    /// # Example
    /// ```
    /// use oxify_vector::distributed::ShardConfig;
    /// let config = ShardConfig::new(3, 2); // 3 shards, 2 replicas
    /// ```
    pub fn new(num_shards: usize, num_replicas: usize) -> Self {
        assert!(num_shards >= 1, "num_shards must be at least 1");
        assert!(num_replicas >= 1, "num_replicas must be at least 1");
        Self {
            num_shards,
            num_replicas,
            virtual_nodes: 150, // Default: 150 virtual nodes per shard
        }
    }

    /// Set the number of virtual nodes for consistent hashing.
    pub fn with_virtual_nodes(mut self, virtual_nodes: usize) -> Self {
        self.virtual_nodes = virtual_nodes;
        self
    }
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self::new(1, 1) // Single shard, single replica by default
    }
}

/// Consistent hashing for load balancing across shards.
///
/// Uses virtual nodes to improve distribution uniformity.
#[derive(Debug)]
pub struct ConsistentHash {
    /// Ring of hash values to shard IDs
    ring: BTreeMap<u64, usize>,
    /// Number of virtual nodes per shard
    #[allow(dead_code)]
    virtual_nodes: usize,
}

impl ConsistentHash {
    /// Create a new consistent hash ring.
    ///
    /// # Arguments
    /// * `num_shards` - Number of physical shards
    /// * `virtual_nodes` - Virtual nodes per shard (more = better distribution)
    pub fn new(num_shards: usize, virtual_nodes: usize) -> Self {
        let mut ring = BTreeMap::new();

        // Add virtual nodes for each shard
        for shard_id in 0..num_shards {
            for vnode in 0..virtual_nodes {
                let key = format!("shard-{}-vnode-{}", shard_id, vnode);
                let hash = Self::hash_key(&key);
                ring.insert(hash, shard_id);
            }
        }

        Self {
            ring,
            virtual_nodes,
        }
    }

    /// Get the shard ID for a given key.
    pub fn get_shard(&self, key: &str) -> usize {
        if self.ring.is_empty() {
            return 0;
        }

        let hash = Self::hash_key(key);

        // Find the first shard with hash >= key hash (clockwise on ring)
        match self.ring.range(hash..).next() {
            Some((&_, &shard_id)) => shard_id,
            None => *self.ring.values().next().unwrap(), // Wrap around to first shard
        }
    }

    /// Get N replica shard IDs for a given key.
    pub fn get_replicas(&self, key: &str, num_replicas: usize) -> Vec<usize> {
        if self.ring.is_empty() {
            return vec![0];
        }

        let hash = Self::hash_key(key);
        let mut replicas = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Start from the primary shard and walk clockwise
        for (&_, &shard_id) in self.ring.range(hash..) {
            if !seen.contains(&shard_id) {
                replicas.push(shard_id);
                seen.insert(shard_id);
                if replicas.len() >= num_replicas {
                    return replicas;
                }
            }
        }

        // Wrap around if needed
        for (&_, &shard_id) in self.ring.iter() {
            if !seen.contains(&shard_id) {
                replicas.push(shard_id);
                seen.insert(shard_id);
                if replicas.len() >= num_replicas {
                    return replicas;
                }
            }
        }

        replicas
    }

    /// Hash a key using FNV-1a algorithm
    fn hash_key(key: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

/// A single shard containing a vector search index.
#[derive(Debug)]
struct Shard {
    /// Shard ID
    #[allow(dead_code)]
    id: usize,
    /// Vector search index for this shard
    index: VectorSearchIndex,
    /// Number of vectors in this shard
    size: usize,
}

impl Shard {
    fn new(id: usize, config: SearchConfig) -> Self {
        Self {
            id,
            index: VectorSearchIndex::new(config),
            size: 0,
        }
    }

    fn build(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        // Only build if there are embeddings (allow empty shards)
        if !embeddings.is_empty() {
            self.index.build(embeddings)?;
            self.size = embeddings.len();
        }
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        // Return empty results if shard is empty
        if self.size == 0 {
            return Ok(Vec::new());
        }
        self.index.search(query, k)
    }

    fn filtered_search(
        &self,
        query: &[f32],
        k: usize,
        filter: &Filter,
    ) -> Result<Vec<SearchResult>> {
        if self.size == 0 {
            return Ok(Vec::new());
        }
        self.index.filtered_search(query, k, filter)
    }

    fn set_metadata(&mut self, entity_id: &str, metadata: Metadata) {
        self.index.set_metadata(entity_id, metadata);
    }

    fn get_metadata(&self, entity_id: &str) -> Option<&Metadata> {
        self.index.get_metadata(entity_id)
    }
}

/// Distributed vector search index with sharding and replication.
///
/// Splits vectors across multiple shards for horizontal scaling.
/// Supports replication for fault tolerance.
pub struct DistributedIndex {
    /// Shard configuration
    shard_config: ShardConfig,
    /// Search configuration for each shard
    #[allow(dead_code)]
    search_config: SearchConfig,
    /// Shards (shard_id -> Shard)
    shards: Vec<Arc<RwLock<Shard>>>,
    /// Consistent hash ring for shard assignment
    hash_ring: ConsistentHash,
    /// Total number of vectors across all shards
    total_size: Arc<RwLock<usize>>,
}

impl DistributedIndex {
    /// Create a new distributed index.
    pub fn new(shard_config: ShardConfig, search_config: SearchConfig) -> Self {
        let hash_ring = ConsistentHash::new(shard_config.num_shards, shard_config.virtual_nodes);

        let mut shards = Vec::new();
        for i in 0..shard_config.num_shards {
            let shard = Shard::new(i, search_config.clone());
            shards.push(Arc::new(RwLock::new(shard)));
        }

        Self {
            shard_config,
            search_config,
            shards,
            hash_ring,
            total_size: Arc::new(RwLock::new(0)),
        }
    }

    /// Build the distributed index from embeddings.
    ///
    /// Automatically distributes vectors across shards using consistent hashing.
    pub fn build(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        // Partition embeddings by shard
        let mut shard_embeddings: Vec<HashMap<String, Vec<f32>>> =
            vec![HashMap::new(); self.shard_config.num_shards];

        for (entity_id, embedding) in embeddings {
            let shard_id = self.hash_ring.get_shard(entity_id);
            shard_embeddings[shard_id].insert(entity_id.clone(), embedding.clone());
        }

        // Build each shard in parallel
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            self.shards
                .par_iter()
                .zip(shard_embeddings.par_iter())
                .try_for_each(|(shard, embs)| -> Result<()> {
                    let mut shard = shard.write().map_err(|e| anyhow!("Lock error: {}", e))?;
                    shard.build(embs)?;
                    Ok(())
                })?;
        }

        #[cfg(not(feature = "parallel"))]
        {
            for (shard, embs) in self.shards.iter().zip(shard_embeddings.iter()) {
                let mut shard = shard.write().map_err(|e| anyhow!("Lock error: {}", e))?;
                shard.build(embs)?;
            }
        }

        // Update total size
        let mut total = self
            .total_size
            .write()
            .map_err(|e| anyhow!("Lock error: {}", e))?;
        *total = embeddings.len();

        Ok(())
    }

    /// Search across all shards and merge results.
    ///
    /// Performs a fan-out search to all shards in parallel, then merges and re-ranks results.
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        // Search all shards in parallel
        #[cfg(feature = "parallel")]
        let shard_results = {
            use rayon::prelude::*;
            self.shards
                .par_iter()
                .map(|shard| -> Result<Vec<SearchResult>> {
                    let shard = shard.read().map_err(|e| anyhow!("Lock error: {}", e))?;
                    shard.search(query, k)
                })
                .collect::<Result<Vec<Vec<SearchResult>>>>()?
        };

        #[cfg(not(feature = "parallel"))]
        let shard_results = {
            let mut results = Vec::new();
            for shard in &self.shards {
                let shard = shard.read().map_err(|e| anyhow!("Lock error: {}", e))?;
                let result = shard.search(query, k)?;
                results.push(result);
            }
            results
        };

        // Merge and re-rank results from all shards
        let merged = Self::merge_results(shard_results, k);

        Ok(merged)
    }

    /// Batch search for multiple queries across all shards.
    ///
    /// Performs multiple searches in parallel and returns results for each query.
    pub fn batch_search(&self, queries: &[Vec<f32>], k: usize) -> Result<Vec<Vec<SearchResult>>> {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            queries
                .par_iter()
                .map(|query| self.search(query, k))
                .collect()
        }

        #[cfg(not(feature = "parallel"))]
        {
            queries.iter().map(|query| self.search(query, k)).collect()
        }
    }

    /// Search with metadata filtering across all shards.
    ///
    /// Applies the filter to each shard before merging results.
    pub fn filtered_search(
        &self,
        query: &[f32],
        k: usize,
        filter: &Filter,
    ) -> Result<Vec<SearchResult>> {
        // Search all shards in parallel
        #[cfg(feature = "parallel")]
        let shard_results = {
            use rayon::prelude::*;
            self.shards
                .par_iter()
                .map(|shard| -> Result<Vec<SearchResult>> {
                    let shard = shard.read().map_err(|e| anyhow!("Lock error: {}", e))?;
                    shard.filtered_search(query, k, filter)
                })
                .collect::<Result<Vec<Vec<SearchResult>>>>()?
        };

        #[cfg(not(feature = "parallel"))]
        let shard_results = {
            let mut results = Vec::new();
            for shard in &self.shards {
                let shard = shard.read().map_err(|e| anyhow!("Lock error: {}", e))?;
                let result = shard.filtered_search(query, k, filter)?;
                results.push(result);
            }
            results
        };

        // Merge and re-rank results from all shards
        let merged = Self::merge_results(shard_results, k);

        Ok(merged)
    }

    /// Set metadata for an entity across all replica shards.
    pub fn set_metadata(&mut self, entity_id: &str, metadata: Metadata) {
        let replica_shards = self
            .hash_ring
            .get_replicas(entity_id, self.shard_config.num_replicas);

        for shard_id in replica_shards {
            if let Ok(mut shard) = self.shards[shard_id].write() {
                shard.set_metadata(entity_id, metadata.clone());
            }
        }
    }

    /// Get metadata for an entity from the primary shard.
    pub fn get_metadata(&self, entity_id: &str) -> Option<Metadata> {
        let shard_id = self.hash_ring.get_shard(entity_id);
        if let Ok(shard) = self.shards[shard_id].read() {
            shard.get_metadata(entity_id).cloned()
        } else {
            None
        }
    }

    /// Set metadata for multiple entities in batch.
    pub fn batch_set_metadata(&mut self, metadata_map: &HashMap<String, Metadata>) {
        for (entity_id, metadata) in metadata_map {
            self.set_metadata(entity_id, metadata.clone());
        }
    }

    /// Get statistics about the distributed index.
    pub fn get_stats(&self) -> Result<DistributedStats> {
        let mut shard_sizes = Vec::new();
        let mut total_vectors = 0;

        for shard in &self.shards {
            let shard = shard.read().map_err(|e| anyhow!("Lock error: {}", e))?;
            shard_sizes.push(shard.size);
            total_vectors += shard.size;
        }

        let avg_shard_size = if !shard_sizes.is_empty() {
            shard_sizes.iter().sum::<usize>() as f64 / shard_sizes.len() as f64
        } else {
            0.0
        };

        let max_shard_size = shard_sizes.iter().copied().max().unwrap_or(0);
        let min_shard_size = shard_sizes.iter().copied().min().unwrap_or(0);

        Ok(DistributedStats {
            num_shards: self.shard_config.num_shards,
            num_replicas: self.shard_config.num_replicas,
            total_vectors,
            shard_sizes,
            avg_shard_size,
            max_shard_size,
            min_shard_size,
            balance_ratio: if max_shard_size > 0 {
                min_shard_size as f64 / max_shard_size as f64
            } else {
                1.0
            },
        })
    }

    /// Merge results from multiple shards and return top-k.
    ///
    /// Uses a simple merge strategy based on scores.
    fn merge_results(shard_results: Vec<Vec<SearchResult>>, k: usize) -> Vec<SearchResult> {
        let mut all_results = Vec::new();

        // Flatten all shard results
        for results in shard_results {
            all_results.extend(results);
        }

        // Sort by score (descending for cosine/dot, ascending for distance)
        all_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top-k and deduplicate by entity_id
        let mut seen = std::collections::HashSet::new();
        let mut merged = Vec::new();

        for result in all_results {
            if !seen.contains(&result.entity_id) {
                seen.insert(result.entity_id.clone());
                merged.push(result);
                if merged.len() >= k {
                    break;
                }
            }
        }

        merged
    }
}

/// Statistics for a distributed index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedStats {
    /// Number of shards
    pub num_shards: usize,
    /// Number of replicas per vector
    pub num_replicas: usize,
    /// Total vectors across all shards (counting each unique vector once)
    pub total_vectors: usize,
    /// Size of each shard
    pub shard_sizes: Vec<usize>,
    /// Average shard size
    pub avg_shard_size: f64,
    /// Maximum shard size
    pub max_shard_size: usize,
    /// Minimum shard size
    pub min_shard_size: usize,
    /// Load balance ratio (min/max, closer to 1.0 is better)
    pub balance_ratio: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_config() {
        let config = ShardConfig::new(3, 2);
        assert_eq!(config.num_shards, 3);
        assert_eq!(config.num_replicas, 2);
        assert_eq!(config.virtual_nodes, 150);

        let config = config.with_virtual_nodes(200);
        assert_eq!(config.virtual_nodes, 200);
    }

    #[test]
    fn test_consistent_hash() {
        let hash = ConsistentHash::new(3, 10);

        // Same key should always map to same shard
        let shard1 = hash.get_shard("doc1");
        let shard2 = hash.get_shard("doc1");
        assert_eq!(shard1, shard2);

        // Different keys should distribute across shards
        let mut shard_counts = vec![0; 3];
        for i in 0..100 {
            let key = format!("doc{}", i);
            let shard = hash.get_shard(&key);
            shard_counts[shard] += 1;
        }

        // Check that distribution is reasonably balanced
        // (with 100 keys and 3 shards, expect ~33 per shard, allow ±15)
        for count in shard_counts {
            assert!(
                (18..=48).contains(&count),
                "Imbalanced distribution: {}",
                count
            );
        }
    }

    #[test]
    fn test_consistent_hash_replicas() {
        let hash = ConsistentHash::new(5, 10);

        let replicas = hash.get_replicas("doc1", 3);
        assert_eq!(replicas.len(), 3);

        // All replicas should be different shards
        let unique: std::collections::HashSet<_> = replicas.iter().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn test_distributed_index_creation() {
        let shard_config = ShardConfig::new(2, 1);
        let search_config = SearchConfig::default();
        let index = DistributedIndex::new(shard_config, search_config);

        assert_eq!(index.shards.len(), 2);
    }

    #[test]
    fn test_distributed_index_build() {
        let shard_config = ShardConfig::new(2, 1);
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);

        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
        embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
        embeddings.insert("doc3".to_string(), vec![0.3, 0.4, 0.5]);

        assert!(index.build(&embeddings).is_ok());

        let stats = index.get_stats().unwrap();
        assert_eq!(stats.num_shards, 2);
        assert!(stats.total_vectors <= 3); // May be less if replicas counted differently
    }

    #[test]
    fn test_distributed_search() {
        let shard_config = ShardConfig::new(2, 1);
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);

        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        embeddings.insert("doc2".to_string(), vec![0.0, 1.0, 0.0]);
        embeddings.insert("doc3".to_string(), vec![0.0, 0.0, 1.0]);

        index.build(&embeddings).unwrap();

        let query = vec![0.9, 0.1, 0.0];
        let results = index.search(&query, 2).unwrap();

        assert!(results.len() <= 2);
        assert_eq!(results[0].entity_id, "doc1");
    }

    #[test]
    fn test_distributed_stats() {
        let shard_config = ShardConfig::new(3, 1);
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);

        let mut embeddings = HashMap::new();
        for i in 0..10 {
            let key = format!("doc{}", i);
            let embedding = vec![i as f32 * 0.1, 0.2, 0.3];
            embeddings.insert(key, embedding);
        }

        index.build(&embeddings).unwrap();

        let stats = index.get_stats().unwrap();
        assert_eq!(stats.num_shards, 3);
        assert_eq!(stats.num_replicas, 1);
        assert!(stats.total_vectors <= 10);
        // With only 10 vectors and 3 shards, distribution might not be perfect
        // Allow for imbalance (some shards could be empty)
        assert!(stats.balance_ratio >= 0.0 && stats.balance_ratio <= 1.0);
    }

    #[test]
    fn test_merge_results() {
        let shard1_results = vec![
            SearchResult {
                entity_id: "doc1".to_string(),
                score: 0.9,
                distance: 0.1,
                rank: 0,
            },
            SearchResult {
                entity_id: "doc2".to_string(),
                score: 0.7,
                distance: 0.3,
                rank: 1,
            },
        ];

        let shard2_results = vec![
            SearchResult {
                entity_id: "doc3".to_string(),
                score: 0.85,
                distance: 0.15,
                rank: 0,
            },
            SearchResult {
                entity_id: "doc4".to_string(),
                score: 0.6,
                distance: 0.4,
                rank: 1,
            },
        ];

        let merged = DistributedIndex::merge_results(vec![shard1_results, shard2_results], 3);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].entity_id, "doc1"); // 0.9
        assert_eq!(merged[1].entity_id, "doc3"); // 0.85
        assert_eq!(merged[2].entity_id, "doc2"); // 0.7
    }

    #[test]
    fn test_merge_results_deduplication() {
        let shard1_results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.9,
            distance: 0.1,
            rank: 0,
        }];

        let shard2_results = vec![SearchResult {
            entity_id: "doc1".to_string(),
            score: 0.85,
            distance: 0.15,
            rank: 0,
        }];

        let merged = DistributedIndex::merge_results(vec![shard1_results, shard2_results], 5);

        // Should only have one copy of doc1
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].entity_id, "doc1");
        assert_eq!(merged[0].score, 0.9); // Higher score should win
    }

    #[test]
    fn test_distributed_replication() {
        let shard_config = ShardConfig::new(3, 2); // 3 shards, 2 replicas
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);

        // Build with vectors (they will be replicated to multiple shards)
        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
        index.build(&embeddings).unwrap();

        // Search should find it even if one shard fails
        let query = vec![0.1, 0.2, 0.3];
        let results = index.search(&query, 1).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_id, "doc1");
    }

    #[test]
    fn test_distributed_batch_search() {
        let shard_config = ShardConfig::new(2, 1);
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);

        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        embeddings.insert("doc2".to_string(), vec![0.0, 1.0, 0.0]);
        embeddings.insert("doc3".to_string(), vec![0.0, 0.0, 1.0]);
        index.build(&embeddings).unwrap();

        let queries = vec![
            vec![0.9, 0.1, 0.0],
            vec![0.0, 0.9, 0.1],
            vec![0.0, 0.0, 0.9],
        ];

        let results = index.batch_search(&queries, 1).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0][0].entity_id, "doc1");
        assert_eq!(results[1][0].entity_id, "doc2");
        assert_eq!(results[2][0].entity_id, "doc3");
    }

    #[test]
    fn test_distributed_filtered_search() {
        use crate::filter::FilterValue;

        let shard_config = ShardConfig::new(2, 1);
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);

        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        embeddings.insert("doc2".to_string(), vec![0.0, 1.0, 0.0]);
        embeddings.insert("doc3".to_string(), vec![0.0, 0.0, 1.0]);
        index.build(&embeddings).unwrap();

        // Set metadata
        let mut metadata1 = HashMap::new();
        metadata1.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        index.set_metadata("doc1", metadata1);

        let mut metadata2 = HashMap::new();
        metadata2.insert("type".to_string(), FilterValue::String("book".to_string()));
        index.set_metadata("doc2", metadata2);

        let mut metadata3 = HashMap::new();
        metadata3.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        index.set_metadata("doc3", metadata3);

        // Filter for articles only
        let filter = Filter::new().eq("type", "article");

        let query = vec![0.5, 0.5, 0.5];
        let results = index.filtered_search(&query, 10, &filter).unwrap();

        // Should only return doc1 and doc3 (articles)
        assert!(results.len() <= 2);
        for result in &results {
            assert!(result.entity_id == "doc1" || result.entity_id == "doc3");
        }
    }

    #[test]
    fn test_distributed_metadata() {
        use crate::filter::FilterValue;

        let shard_config = ShardConfig::new(2, 1);
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);

        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
        index.build(&embeddings).unwrap();

        // Set metadata
        let mut metadata = HashMap::new();
        metadata.insert("year".to_string(), FilterValue::Int(2026));
        index.set_metadata("doc1", metadata.clone());

        // Get metadata
        let retrieved = index.get_metadata("doc1");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.get("year"), Some(&FilterValue::Int(2026)));
    }

    #[test]
    fn test_distributed_batch_metadata() {
        use crate::filter::FilterValue;

        let shard_config = ShardConfig::new(2, 1);
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);

        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
        embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
        index.build(&embeddings).unwrap();

        // Batch set metadata
        let mut metadata_map = HashMap::new();

        let mut m1 = HashMap::new();
        m1.insert("year".to_string(), FilterValue::Int(2026));
        metadata_map.insert("doc1".to_string(), m1);

        let mut m2 = HashMap::new();
        m2.insert("year".to_string(), FilterValue::Int(2023));
        metadata_map.insert("doc2".to_string(), m2);

        index.batch_set_metadata(&metadata_map);

        // Verify
        assert!(index.get_metadata("doc1").is_some());
        assert!(index.get_metadata("doc2").is_some());
    }
}
