//! Locality Sensitive Hashing (LSH) for Approximate Nearest Neighbor Search
//!
//! LSH is an ANN algorithm that uses hash functions to map similar vectors
//! to the same buckets with high probability. This implementation uses
//! random projection LSH for cosine similarity.
//!
//! ## Features
//!
//! - **Random Projection LSH**: Hash functions based on random hyperplanes
//! - **Multi-table Hashing**: Multiple hash tables for better recall
//! - **Multi-probe Search**: Query nearby buckets to improve accuracy
//! - **Configurable Parameters**: num_tables, num_bits, num_probes
//!
//! ## Algorithm
//!
//! 1. Generate random projection vectors (hyperplanes)
//! 2. For each vector, compute hash by checking which side of hyperplanes it's on
//! 3. Store vectors in hash buckets
//! 4. At query time, hash the query and retrieve candidates from matching buckets
//! 5. Optionally probe nearby buckets (flip hash bits) for better recall
//! 6. Rank candidates by actual similarity
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::lsh::{LshIndex, LshConfig};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create embeddings
//! let mut embeddings = HashMap::new();
//! for i in 0..1000 {
//!     let vec = vec![i as f32 * 0.01, (i * 2) as f32 * 0.01, (i * 3) as f32 * 0.01];
//!     embeddings.insert(format!("doc{}", i), vec);
//! }
//!
//! // Build LSH index
//! let config = LshConfig::default();
//! let mut index = LshIndex::new(config);
//! index.build(&embeddings)?;
//!
//! // Search
//! let query = vec![0.5, 1.0, 1.5];
//! let results = index.search(&query, 10)?;
//! # Ok(())
//! # }
//! ```

use anyhow::{anyhow, Result};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use crate::simd::cosine_similarity_simd;
use crate::types::SearchResult;

/// LSH configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LshConfig {
    /// Number of hash tables (more tables = better recall, more memory)
    pub num_tables: usize,
    /// Number of bits per hash (hash length)
    pub num_bits: usize,
    /// Number of probes (how many nearby buckets to check)
    pub num_probes: usize,
    /// Random seed for reproducibility
    pub seed: u64,
}

impl Default for LshConfig {
    fn default() -> Self {
        Self {
            num_tables: 10,
            num_bits: 16,
            num_probes: 3,
            seed: 42,
        }
    }
}

impl LshConfig {
    /// Create config optimized for high recall
    pub fn high_recall() -> Self {
        Self {
            num_tables: 20,
            num_bits: 20,
            num_probes: 10,
            seed: 42,
        }
    }

    /// Create config optimized for speed (low memory/probes)
    pub fn fast() -> Self {
        Self {
            num_tables: 5,
            num_bits: 12,
            num_probes: 1,
            seed: 42,
        }
    }

    /// Create config optimized for memory efficiency
    pub fn memory_efficient() -> Self {
        Self {
            num_tables: 5,
            num_bits: 10,
            num_probes: 5,
            seed: 42,
        }
    }
}

/// Hash value type (bit vector represented as u64)
type HashValue = u64;

/// A single hash table
#[derive(Debug, Clone)]
struct HashTable {
    /// Random projection vectors (hyperplanes)
    projections: Vec<Vec<f32>>,
    /// Buckets: hash_value -> list of vector indices
    buckets: HashMap<HashValue, Vec<usize>>,
}

impl HashTable {
    fn new(num_bits: usize, dimensions: usize, rng: &mut impl RngExt) -> Self {
        // Generate random projection vectors
        let projections: Vec<Vec<f32>> = (0..num_bits)
            .map(|_| {
                (0..dimensions)
                    .map(|_| rng.random_range(-1.0..1.0))
                    .collect()
            })
            .collect();

        Self {
            projections,
            buckets: HashMap::new(),
        }
    }

    /// Compute hash value for a vector
    fn hash(&self, vector: &[f32]) -> HashValue {
        let mut hash_val: HashValue = 0;

        for (i, projection) in self.projections.iter().enumerate() {
            // Dot product with projection vector
            let dot: f32 = vector
                .iter()
                .zip(projection.iter())
                .map(|(v, p)| v * p)
                .sum();

            // Set bit if dot product is positive
            if dot > 0.0 {
                hash_val |= 1u64 << i;
            }
        }

        hash_val
    }

    /// Insert a vector index into the hash table
    fn insert(&mut self, vector: &[f32], index: usize) {
        let hash_val = self.hash(vector);
        self.buckets.entry(hash_val).or_default().push(index);
    }

    /// Query the hash table and return candidate indices
    fn query(&self, vector: &[f32], num_probes: usize) -> Vec<usize> {
        let hash_val = self.hash(vector);
        let mut candidates = Vec::new();

        // Get exact matches
        if let Some(bucket) = self.buckets.get(&hash_val) {
            candidates.extend(bucket);
        }

        // Multi-probe: flip bits to probe nearby buckets
        if num_probes > 1 {
            for probe in 1..num_probes.min(self.projections.len()) {
                // Flip the probe-th bit
                let flipped_hash = hash_val ^ (1u64 << probe);
                if let Some(bucket) = self.buckets.get(&flipped_hash) {
                    candidates.extend(bucket);
                }
            }
        }

        candidates
    }
}

/// LSH Index for approximate nearest neighbor search
#[derive(Debug, Clone)]
pub struct LshIndex {
    config: LshConfig,
    tables: Vec<HashTable>,
    vectors: Vec<Vec<f32>>,
    entity_ids: Vec<String>,
    dimensions: usize,
    is_built: bool,
}

impl LshIndex {
    /// Create a new LSH index
    pub fn new(config: LshConfig) -> Self {
        info!(
            "Initialized LSH index: num_tables={}, num_bits={}, num_probes={}",
            config.num_tables, config.num_bits, config.num_probes
        );

        Self {
            config,
            tables: Vec::new(),
            vectors: Vec::new(),
            entity_ids: Vec::new(),
            dimensions: 0,
            is_built: false,
        }
    }

    /// Build LSH index from embeddings
    pub fn build(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        if embeddings.is_empty() {
            return Err(anyhow!("Cannot build index from empty embeddings"));
        }

        info!("Building LSH index for {} entities", embeddings.len());

        // Get dimensions from first vector
        self.dimensions = embeddings.values().next().expect("invariant: embeddings non-empty from guard").len();

        // Validate all vectors have same dimension
        for (id, vec) in embeddings {
            if vec.len() != self.dimensions {
                return Err(anyhow!(
                    "Dimension mismatch for entity {}: expected {}, got {}",
                    id,
                    self.dimensions,
                    vec.len()
                ));
            }
        }

        // Store vectors and entity IDs
        self.vectors.clear();
        self.entity_ids.clear();
        for (id, vec) in embeddings {
            self.vectors.push(vec.clone());
            self.entity_ids.push(id.clone());
        }

        // Initialize random number generator with seed
        let mut rng = StdRng::seed_from_u64(self.config.seed);

        // Create hash tables
        self.tables.clear();
        for table_idx in 0..self.config.num_tables {
            debug!(
                "Building hash table {}/{}",
                table_idx + 1,
                self.config.num_tables
            );

            let mut table = HashTable::new(self.config.num_bits, self.dimensions, &mut rng);

            // Insert all vectors into this table
            for (idx, vector) in self.vectors.iter().enumerate() {
                table.insert(vector, idx);
            }

            self.tables.push(table);
        }

        self.is_built = true;
        info!("LSH index built successfully");
        Ok(())
    }

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        if query.len() != self.dimensions {
            return Err(anyhow!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimensions,
                query.len()
            ));
        }

        debug!("LSH search for k={}", k);

        // Collect candidate indices from all tables
        let mut candidate_set: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for table in &self.tables {
            let candidates = table.query(query, self.config.num_probes);
            candidate_set.extend(candidates);
        }

        debug!("Found {} unique candidates", candidate_set.len());

        // Compute actual similarities for candidates
        let mut scored_candidates: Vec<(usize, f32)> = candidate_set
            .into_iter()
            .map(|idx| {
                let similarity = cosine_similarity_simd(query, &self.vectors[idx]);
                (idx, similarity)
            })
            .collect();

        // Sort by similarity (descending)
        scored_candidates
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top k and convert to SearchResult
        let results: Vec<SearchResult> = scored_candidates
            .into_iter()
            .take(k)
            .enumerate()
            .map(|(rank, (idx, score))| SearchResult {
                entity_id: self.entity_ids[idx].clone(),
                score,
                distance: 1.0 - score,
                rank: rank + 1,
            })
            .collect();

        debug!("Returning {} results", results.len());
        Ok(results)
    }

    /// Get number of vectors in index
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Get index statistics
    pub fn stats(&self) -> LshStats {
        let total_buckets: usize = self.tables.iter().map(|t| t.buckets.len()).sum();
        let avg_bucket_size: f32 = if total_buckets > 0 {
            let total_entries: usize = self
                .tables
                .iter()
                .flat_map(|t| t.buckets.values())
                .map(|b| b.len())
                .sum();
            total_entries as f32 / total_buckets as f32
        } else {
            0.0
        };

        let max_bucket_size: usize = self
            .tables
            .iter()
            .flat_map(|t| t.buckets.values())
            .map(|b| b.len())
            .max()
            .unwrap_or(0);

        LshStats {
            num_vectors: self.vectors.len(),
            num_tables: self.tables.len(),
            num_bits: self.config.num_bits,
            total_buckets,
            avg_bucket_size,
            max_bucket_size,
            dimensions: self.dimensions,
        }
    }
}

/// LSH index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LshStats {
    /// Number of vectors in index
    pub num_vectors: usize,
    /// Number of hash tables
    pub num_tables: usize,
    /// Number of bits per hash
    pub num_bits: usize,
    /// Total number of buckets across all tables
    pub total_buckets: usize,
    /// Average bucket size
    pub avg_bucket_size: f32,
    /// Maximum bucket size
    pub max_bucket_size: usize,
    /// Vector dimensions
    pub dimensions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_embeddings(n: usize, dims: usize) -> HashMap<String, Vec<f32>> {
        let mut embeddings = HashMap::new();
        for i in 0..n {
            let vec: Vec<f32> = (0..dims).map(|d| ((i * d) as f32 * 0.01).sin()).collect();
            embeddings.insert(format!("doc{}", i), vec);
        }
        embeddings
    }

    #[test]
    fn test_lsh_build() {
        let embeddings = create_test_embeddings(100, 64);
        let mut index = LshIndex::new(LshConfig::default());
        assert!(index.build(&embeddings).is_ok());
        assert_eq!(index.len(), 100);
        assert!(index.is_built);
    }

    #[test]
    fn test_lsh_search() {
        let embeddings = create_test_embeddings(100, 64);
        let mut index = LshIndex::new(LshConfig::default());
        index.build(&embeddings).unwrap();

        let query: Vec<f32> = (0..64).map(|d| (d as f32 * 0.01).sin()).collect();
        let results = index.search(&query, 10).unwrap();

        // LSH may not always find exactly k results due to probabilistic nature
        assert!(!results.is_empty());
        assert!(results.len() <= 10);
        // Results should be sorted by score (descending)
        if results.len() > 1 {
            assert!(results[0].score >= results[results.len() - 1].score);
        }
    }

    #[test]
    fn test_lsh_empty_embeddings() {
        let embeddings = HashMap::new();
        let mut index = LshIndex::new(LshConfig::default());
        assert!(index.build(&embeddings).is_err());
    }

    #[test]
    fn test_lsh_dimension_mismatch() {
        let mut embeddings = HashMap::new();
        embeddings.insert("doc1".to_string(), vec![1.0, 2.0, 3.0]);
        embeddings.insert("doc2".to_string(), vec![1.0, 2.0]); // Wrong dimension

        let mut index = LshIndex::new(LshConfig::default());
        assert!(index.build(&embeddings).is_err());
    }

    #[test]
    fn test_lsh_search_before_build() {
        let index = LshIndex::new(LshConfig::default());
        let query = vec![1.0, 2.0, 3.0];
        assert!(index.search(&query, 10).is_err());
    }

    #[test]
    fn test_lsh_query_dimension_mismatch() {
        let embeddings = create_test_embeddings(100, 64);
        let mut index = LshIndex::new(LshConfig::default());
        index.build(&embeddings).unwrap();

        let wrong_query = vec![1.0, 2.0]; // Wrong dimension
        assert!(index.search(&wrong_query, 10).is_err());
    }

    #[test]
    fn test_lsh_stats() {
        let embeddings = create_test_embeddings(100, 64);
        let mut index = LshIndex::new(LshConfig::default());
        index.build(&embeddings).unwrap();

        let stats = index.stats();
        assert_eq!(stats.num_vectors, 100);
        assert_eq!(stats.num_tables, 10);
        assert_eq!(stats.dimensions, 64);
        assert!(stats.total_buckets > 0);
        assert!(stats.avg_bucket_size > 0.0);
    }

    #[test]
    fn test_lsh_config_presets() {
        let high_recall = LshConfig::high_recall();
        assert_eq!(high_recall.num_tables, 20);
        assert_eq!(high_recall.num_probes, 10);

        let fast = LshConfig::fast();
        assert_eq!(fast.num_tables, 5);
        assert_eq!(fast.num_probes, 1);

        let memory = LshConfig::memory_efficient();
        assert_eq!(memory.num_tables, 5);
        assert_eq!(memory.num_bits, 10);
    }

    #[test]
    fn test_hash_table_hash() {
        let mut rng = StdRng::seed_from_u64(42);
        let table = HashTable::new(8, 3, &mut rng);

        let vec1 = vec![1.0, 2.0, 3.0];
        let vec2 = vec![1.0, 2.0, 3.0];
        let vec3 = vec![-1.0, -2.0, -3.0];

        // Same vectors should have same hash
        assert_eq!(table.hash(&vec1), table.hash(&vec2));

        // Different vectors may have different hashes (not guaranteed but likely)
        // This is a probabilistic test
        let hash1 = table.hash(&vec1);
        let hash3 = table.hash(&vec3);
        // Opposite vectors should likely have different hashes
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_multiprobe_increases_candidates() {
        let embeddings = create_test_embeddings(50, 32);

        // Build with 1 probe
        let config_1probe = LshConfig {
            num_tables: 5,
            num_bits: 10,
            num_probes: 1,
            seed: 42,
        };
        let mut index_1probe = LshIndex::new(config_1probe);
        index_1probe.build(&embeddings).unwrap();

        // Build with 5 probes
        let config_5probe = LshConfig {
            num_tables: 5,
            num_bits: 10,
            num_probes: 5,
            seed: 42,
        };
        let mut index_5probe = LshIndex::new(config_5probe);
        index_5probe.build(&embeddings).unwrap();

        let query: Vec<f32> = (0..32).map(|d| (d as f32 * 0.02).cos()).collect();

        let results_1probe = index_1probe.search(&query, 20).unwrap();
        let results_5probe = index_5probe.search(&query, 20).unwrap();

        // More probes should generally find more candidates (and thus return more results if k is large)
        // This is probabilistic but should hold in most cases
        assert!(results_5probe.len() >= results_1probe.len());
    }
}
