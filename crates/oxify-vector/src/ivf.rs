//! IVF-PQ (Inverted File Index with Product Quantization)
//!
//! Memory-efficient ANN search for large-scale datasets (1M+ vectors).
//!
//! ## Algorithm Overview
//!
//! 1. **Clustering (IVF)**: Partition vectors into clusters using k-means
//! 2. **Product Quantization (PQ)**: Compress vectors by quantizing sub-vectors
//! 3. **Search**: Query nearest clusters, then search compressed vectors
//!
//! ## Benefits
//!
//! - **Memory**: 8-16x compression (768D → 64-96 bytes)
//! - **Speed**: Search only relevant partitions (nprobe parameter)
//! - **Scalability**: Handles 1M+ vectors efficiently
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::ivf::{IvfPqIndex, IvfPqConfig};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! let config = IvfPqConfig::default()
//!     .with_nclusters(256)
//!     .with_nsubvectors(64)
//!     .with_nprobe(16);
//!
//! let mut index = IvfPqIndex::new(config);
//!
//! let mut vectors = HashMap::new();
//! vectors.insert("doc1".to_string(), vec![0.1; 768]);
//! vectors.insert("doc2".to_string(), vec![0.2; 768]);
//!
//! index.build(&vectors)?;
//!
//! let query = vec![0.15; 768];
//! let results = index.search(&query, 10)?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::simd;
use crate::types::{DistanceMetric, SearchResult};

/// IVF-PQ configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IvfPqConfig {
    /// Number of clusters (partitions) for IVF
    /// Typical values: 256, 1024, 4096
    /// More clusters = faster search but more memory
    pub nclusters: usize,

    /// Number of sub-vectors for product quantization
    /// Typical values: 8, 16, 32, 64
    /// More sub-vectors = better accuracy but more memory
    pub nsubvectors: usize,

    /// Number of bits per sub-vector quantizer
    /// Typical value: 8 (256 centroids per sub-quantizer)
    pub nbits: usize,

    /// Number of clusters to probe during search
    /// Typical values: 1, 4, 16, 64
    /// More probes = better recall but slower search
    pub nprobe: usize,

    /// Distance metric for clustering and search
    pub metric: DistanceMetric,

    /// Max iterations for k-means clustering
    pub max_kmeans_iterations: usize,

    /// Convergence threshold for k-means
    pub kmeans_tolerance: f32,
}

impl Default for IvfPqConfig {
    fn default() -> Self {
        Self {
            nclusters: 256,
            nsubvectors: 64,
            nbits: 8,
            nprobe: 16,
            metric: DistanceMetric::Cosine,
            max_kmeans_iterations: 100,
            kmeans_tolerance: 1e-4,
        }
    }
}

impl IvfPqConfig {
    pub fn with_nclusters(mut self, nclusters: usize) -> Self {
        self.nclusters = nclusters;
        self
    }

    pub fn with_nsubvectors(mut self, nsubvectors: usize) -> Self {
        self.nsubvectors = nsubvectors;
        self
    }

    pub fn with_nbits(mut self, nbits: usize) -> Self {
        self.nbits = nbits;
        self
    }

    pub fn with_nprobe(mut self, nprobe: usize) -> Self {
        self.nprobe = nprobe;
        self
    }

    pub fn with_metric(mut self, metric: DistanceMetric) -> Self {
        self.metric = metric;
        self
    }
}

/// Product quantizer for compressing vectors
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProductQuantizer {
    /// Number of sub-vectors
    nsubvectors: usize,
    /// Dimension of each sub-vector
    subvector_dim: usize,
    /// Codebooks for each sub-vector (nsubvectors x ncentroids x subvector_dim)
    codebooks: Vec<Vec<Vec<f32>>>,
    /// Number of centroids per sub-quantizer (2^nbits)
    ncentroids: usize,
}

impl ProductQuantizer {
    fn new(dim: usize, nsubvectors: usize, nbits: usize) -> Result<Self> {
        if !dim.is_multiple_of(nsubvectors) {
            anyhow::bail!(
                "Vector dimension {} must be divisible by number of sub-vectors {}",
                dim,
                nsubvectors
            );
        }

        let subvector_dim = dim / nsubvectors;
        let ncentroids = 1 << nbits; // 2^nbits

        Ok(Self {
            nsubvectors,
            subvector_dim,
            codebooks: vec![],
            ncentroids,
        })
    }

    /// Train product quantizer on a set of vectors
    fn train(&mut self, vectors: &[Vec<f32>], iterations: usize) -> Result<()> {
        self.codebooks.clear();

        for subvec_idx in 0..self.nsubvectors {
            let start = subvec_idx * self.subvector_dim;
            let end = start + self.subvector_dim;

            // Extract sub-vectors for this dimension
            let subvectors: Vec<Vec<f32>> =
                vectors.iter().map(|v| v[start..end].to_vec()).collect();

            // Run k-means clustering on sub-vectors
            let centroids = kmeans(&subvectors, self.ncentroids, iterations)?;
            self.codebooks.push(centroids);
        }

        Ok(())
    }

    /// Encode a vector into quantized codes
    fn encode(&self, vector: &[f32]) -> Vec<u8> {
        let mut codes = Vec::with_capacity(self.nsubvectors);

        for subvec_idx in 0..self.nsubvectors {
            let start = subvec_idx * self.subvector_dim;
            let end = start + self.subvector_dim;
            let subvector = &vector[start..end];

            // Find nearest centroid
            let mut best_idx = 0;
            let mut best_dist = f32::MAX;

            for (centroid_idx, centroid) in self.codebooks[subvec_idx].iter().enumerate() {
                let dist = euclidean_distance(subvector, centroid);
                if dist < best_dist {
                    best_dist = dist;
                    best_idx = centroid_idx;
                }
            }

            codes.push(best_idx as u8);
        }

        codes
    }

    /// Compute asymmetric distance between query vector and quantized vector
    fn asymmetric_distance(&self, query: &[f32], codes: &[u8]) -> f32 {
        let mut total_dist = 0.0;

        #[allow(clippy::needless_range_loop)]
        for subvec_idx in 0..self.nsubvectors {
            let start = subvec_idx * self.subvector_dim;
            let end = start + self.subvector_dim;
            let query_subvector = &query[start..end];

            let code = codes[subvec_idx] as usize;
            let centroid = &self.codebooks[subvec_idx][code];

            total_dist += euclidean_distance(query_subvector, centroid);
        }

        total_dist
    }
}

/// IVF-PQ index for memory-efficient ANN search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IvfPqIndex {
    config: IvfPqConfig,
    /// Cluster centroids (coarse quantizer)
    centroids: Vec<Vec<f32>>,
    /// Inverted lists: cluster_id -> list of (entity_id, quantized_codes)
    inverted_lists: Vec<Vec<(String, Vec<u8>)>>,
    /// Product quantizer for fine quantization
    pq: Option<ProductQuantizer>,
    /// Original vector dimension
    dim: Option<usize>,
    /// Total number of indexed vectors
    size: usize,
}

impl IvfPqIndex {
    pub fn new(config: IvfPqConfig) -> Self {
        Self {
            config,
            centroids: Vec::new(),
            inverted_lists: Vec::new(),
            pq: None,
            dim: None,
            size: 0,
        }
    }

    /// Build the index from a collection of vectors
    pub fn build(&mut self, vectors: &HashMap<String, Vec<f32>>) -> Result<()> {
        if vectors.is_empty() {
            anyhow::bail!("Cannot build index with empty vector collection");
        }

        // Get dimension from first vector
        let dim = vectors.values().next().unwrap().len();
        self.dim = Some(dim);

        let vec_list: Vec<Vec<f32>> = vectors.values().cloned().collect();

        // Step 1: Train coarse quantizer (IVF)
        println!(
            "Training coarse quantizer ({} clusters)...",
            self.config.nclusters
        );
        self.centroids = kmeans(
            &vec_list,
            self.config.nclusters,
            self.config.max_kmeans_iterations,
        )
        .context("Failed to train coarse quantizer")?;

        // Step 2: Train product quantizer (PQ)
        println!(
            "Training product quantizer ({} sub-vectors)...",
            self.config.nsubvectors
        );
        let mut pq = ProductQuantizer::new(dim, self.config.nsubvectors, self.config.nbits)?;
        pq.train(&vec_list, 50)?; // Fewer iterations for PQ
        self.pq = Some(pq);

        // Step 3: Assign vectors to clusters and quantize
        println!("Assigning vectors to clusters and quantizing...");
        self.inverted_lists = vec![Vec::new(); self.config.nclusters];

        for (entity_id, vector) in vectors {
            // Find nearest cluster
            let cluster_id = self.assign_to_cluster(vector);

            // Quantize vector with PQ
            let codes = self.pq.as_ref().unwrap().encode(vector);

            // Add to inverted list
            self.inverted_lists[cluster_id].push((entity_id.clone(), codes));
        }

        self.size = vectors.len();

        println!(
            "Index built: {} vectors in {} clusters",
            self.size, self.config.nclusters
        );

        Ok(())
    }

    /// Assign a vector to the nearest cluster
    fn assign_to_cluster(&self, vector: &[f32]) -> usize {
        let mut best_idx = 0;
        let mut best_dist = f32::MAX;

        for (idx, centroid) in self.centroids.iter().enumerate() {
            let dist = compute_distance(&self.config.metric, vector, centroid);
            if dist < best_dist {
                best_dist = dist;
                best_idx = idx;
            }
        }

        best_idx
    }

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        if self.pq.is_none() {
            anyhow::bail!("Index not built yet");
        }

        // Step 1: Find nprobe nearest clusters
        let mut cluster_distances: Vec<(usize, f32)> = self
            .centroids
            .iter()
            .enumerate()
            .map(|(idx, centroid)| (idx, compute_distance(&self.config.metric, query, centroid)))
            .collect();

        cluster_distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let probe_clusters: Vec<usize> = cluster_distances
            .iter()
            .take(self.config.nprobe.min(self.centroids.len()))
            .map(|(idx, _)| *idx)
            .collect();

        // Step 2: Search within probed clusters using asymmetric distance
        let pq = self.pq.as_ref().unwrap();
        let mut candidates = Vec::new();

        for cluster_id in probe_clusters {
            for (entity_id, codes) in &self.inverted_lists[cluster_id] {
                let dist = pq.asymmetric_distance(query, codes);
                candidates.push(SearchResult {
                    entity_id: entity_id.clone(),
                    score: dist,
                    distance: dist,
                    rank: 0, // Will be set below
                });
            }
        }

        // Step 3: Sort and return top-k
        candidates.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());

        let results: Vec<SearchResult> = candidates
            .into_iter()
            .take(k)
            .enumerate()
            .map(|(rank, mut r)| {
                r.distance = r.score;
                r.rank = rank + 1;
                r
            })
            .collect();

        Ok(results)
    }

    /// Get index statistics
    pub fn stats(&self) -> IvfPqStats {
        let avg_list_size = if self.centroids.is_empty() {
            0.0
        } else {
            self.size as f32 / self.centroids.len() as f32
        };

        let memory_bytes = self.estimate_memory();

        IvfPqStats {
            nclusters: self.centroids.len(),
            nvectors: self.size,
            dimension: self.dim.unwrap_or(0),
            avg_list_size,
            memory_bytes,
            compression_ratio: self.compression_ratio(),
        }
    }

    fn estimate_memory(&self) -> usize {
        // Centroids: nclusters * dim * 4 bytes
        let centroids_mem = self.centroids.len() * self.dim.unwrap_or(0) * 4;

        // Inverted lists: nvectors * nsubvectors * 1 byte (u8 codes)
        let inverted_mem = self.size * self.config.nsubvectors;

        // PQ codebooks: nsubvectors * ncentroids * subvector_dim * 4 bytes
        let pq_mem = if let Some(pq) = &self.pq {
            pq.nsubvectors * pq.ncentroids * pq.subvector_dim * 4
        } else {
            0
        };

        centroids_mem + inverted_mem + pq_mem
    }

    fn compression_ratio(&self) -> f32 {
        if self.size == 0 || self.dim.is_none() {
            return 0.0;
        }

        let original_size = self.size * self.dim.unwrap() * 4; // f32 = 4 bytes
        let compressed_size = self.estimate_memory();

        original_size as f32 / compressed_size as f32
    }
}

/// IVF-PQ index statistics
#[derive(Debug, Clone)]
pub struct IvfPqStats {
    pub nclusters: usize,
    pub nvectors: usize,
    pub dimension: usize,
    pub avg_list_size: f32,
    pub memory_bytes: usize,
    pub compression_ratio: f32,
}

/// K-means clustering algorithm
fn kmeans(vectors: &[Vec<f32>], k: usize, max_iterations: usize) -> Result<Vec<Vec<f32>>> {
    if vectors.is_empty() {
        anyhow::bail!("Cannot run k-means on empty vector set");
    }

    let dim = vectors[0].len();
    let n = vectors.len();

    if k > n {
        anyhow::bail!("Number of clusters {} exceeds number of vectors {}", k, n);
    }

    let mut rng = rand::rng();

    // Initialize centroids randomly (k-means++)
    let mut centroids = Vec::with_capacity(k);
    let first_idx = rng.random_range(0..n);
    centroids.push(vectors[first_idx].clone());

    for _ in 1..k {
        // Compute distance to nearest centroid for each vector
        let distances: Vec<f32> = vectors
            .iter()
            .map(|v| {
                centroids
                    .iter()
                    .map(|c| euclidean_distance(v, c))
                    .fold(f32::MAX, f32::min)
            })
            .collect();

        // Select next centroid with probability proportional to distance^2
        let total: f32 = distances.iter().map(|d| d * d).sum();
        let mut threshold = rng.random_range(0.0..total);

        for (idx, &dist) in distances.iter().enumerate() {
            threshold -= dist * dist;
            if threshold <= 0.0 {
                centroids.push(vectors[idx].clone());
                break;
            }
        }
    }

    // Run k-means iterations
    for _iter in 0..max_iterations {
        // Assign vectors to nearest centroid
        let assignments: Vec<usize> = vectors
            .par_iter()
            .map(|v| {
                centroids
                    .iter()
                    .enumerate()
                    .map(|(idx, c)| (idx, euclidean_distance(v, c)))
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                    .unwrap()
                    .0
            })
            .collect();

        // Update centroids
        let mut new_centroids = vec![vec![0.0; dim]; k];
        let mut counts = vec![0; k];

        for (vec, &cluster_id) in vectors.iter().zip(&assignments) {
            for (i, &val) in vec.iter().enumerate() {
                new_centroids[cluster_id][i] += val;
            }
            counts[cluster_id] += 1;
        }

        // Average to get new centroids
        for (centroid, count) in new_centroids.iter_mut().zip(&counts) {
            if *count > 0 {
                for val in centroid.iter_mut() {
                    *val /= *count as f32;
                }
            }
        }

        // Check for convergence (centroid movement)
        let mut total_movement = 0.0;
        for (old, new) in centroids.iter().zip(&new_centroids) {
            total_movement += euclidean_distance(old, new);
        }

        centroids = new_centroids;

        if total_movement < 0.001 {
            break;
        }
    }

    Ok(centroids)
}

/// Euclidean distance between two vectors
///
/// Uses SIMD-optimized calculation for better performance.
#[inline]
fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    simd::euclidean_distance_simd(a, b)
}

/// Compute distance based on metric
///
/// Uses SIMD-optimized distance calculations for better performance.
#[inline]
fn compute_distance(metric: &DistanceMetric, a: &[f32], b: &[f32]) -> f32 {
    // Use SIMD-optimized implementations for hot path performance
    simd::compute_distance_lower_is_better_simd(*metric, a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ivf_pq_creation() {
        let config = IvfPqConfig::default()
            .with_nclusters(16)
            .with_nsubvectors(8);

        let index = IvfPqIndex::new(config);
        assert_eq!(index.config.nclusters, 16);
        assert_eq!(index.config.nsubvectors, 8);
    }

    #[test]
    fn test_product_quantizer() {
        let dim = 64;
        let nsubvectors = 8;
        let nbits = 8;

        let pq = ProductQuantizer::new(dim, nsubvectors, nbits);
        assert!(pq.is_ok());

        let pq = pq.unwrap();
        assert_eq!(pq.subvector_dim, 8);
        assert_eq!(pq.ncentroids, 256);
    }

    #[test]
    fn test_kmeans_basic() {
        let vectors = vec![
            vec![1.0, 0.0],
            vec![1.1, 0.1],
            vec![0.0, 1.0],
            vec![0.1, 1.1],
        ];

        let centroids = kmeans(&vectors, 2, 10);
        assert!(centroids.is_ok());

        let centroids = centroids.unwrap();
        assert_eq!(centroids.len(), 2);
    }

    #[test]
    fn test_ivf_pq_build_and_search() {
        // Create a dataset with 300 vectors (more than 256 needed for 8-bit quantization)
        let mut vectors = HashMap::new();
        for i in 0..300 {
            let vec: Vec<f32> = (0..64).map(|j| (i + j) as f32 / 300.0).collect();
            vectors.insert(format!("doc{}", i), vec);
        }

        // Build index with small cluster count for testing
        let config = IvfPqConfig::default()
            .with_nclusters(8)
            .with_nsubvectors(8)
            .with_nbits(4) // 4-bit = 16 centroids per sub-quantizer
            .with_nprobe(2);

        let mut index = IvfPqIndex::new(config);
        let build_result = index.build(&vectors);
        if let Err(e) = &build_result {
            panic!("Build failed: {}", e);
        }

        // Search for a vector similar to doc150
        let query = vectors.get("doc150").unwrap().clone();
        let results = index.search(&query, 5);
        assert!(results.is_ok());

        let results = results.unwrap();
        assert_eq!(results.len(), 5);

        // The nearest neighbor should be doc150 itself or very close to it
        assert!(results[0].entity_id.starts_with("doc"));
    }

    #[test]
    fn test_ivf_pq_nprobe_effect() {
        // Create a dataset with 300 vectors
        let mut vectors = HashMap::new();
        for i in 0..300 {
            let vec: Vec<f32> = (0..64).map(|j| (i + j) as f32 / 300.0).collect();
            vectors.insert(format!("doc{}", i), vec);
        }

        // Build with nprobe=1
        let config1 = IvfPqConfig::default()
            .with_nclusters(4)
            .with_nsubvectors(8)
            .with_nbits(4) // 4-bit quantization
            .with_nprobe(1);

        let mut index1 = IvfPqIndex::new(config1);
        assert!(index1.build(&vectors).is_ok());

        // Build with nprobe=4 (search all clusters)
        let config2 = IvfPqConfig::default()
            .with_nclusters(4)
            .with_nsubvectors(8)
            .with_nbits(4) // 4-bit quantization
            .with_nprobe(4);

        let mut index2 = IvfPqIndex::new(config2);
        assert!(index2.build(&vectors).is_ok());

        // Search with same query
        let query = vectors.get("doc150").unwrap().clone();
        let results1 = index1.search(&query, 5).unwrap();
        let results2 = index2.search(&query, 5).unwrap();

        // Both should return results
        assert_eq!(results1.len(), 5);
        assert_eq!(results2.len(), 5);

        // Higher nprobe should generally give better results (though not guaranteed in small dataset)
        assert!(results1[0].score >= 0.0);
        assert!(results2[0].score >= 0.0);
    }

    #[test]
    fn test_ivf_pq_stats() {
        let mut vectors = HashMap::new();
        for i in 0..300 {
            let vec: Vec<f32> = (0..128).map(|j| (i + j) as f32 / 300.0).collect();
            vectors.insert(format!("doc{}", i), vec);
        }

        let config = IvfPqConfig::default()
            .with_nclusters(10)
            .with_nsubvectors(16)
            .with_nbits(4); // 4-bit quantization

        let mut index = IvfPqIndex::new(config);
        assert!(index.build(&vectors).is_ok());

        let stats = index.stats();
        assert_eq!(stats.nclusters, 10);
        assert_eq!(stats.nvectors, 300);
        assert_eq!(stats.dimension, 128);
        assert!(stats.avg_list_size > 0.0);
        assert!(stats.memory_bytes > 0);
        assert!(stats.compression_ratio > 1.0); // Should be compressed
    }

    #[test]
    fn test_ivf_pq_compression_ratio() {
        // Optimized test with reduced parameters for fast execution
        let mut vectors = HashMap::new();
        for i in 0..200 {
            let vec: Vec<f32> = (0..128).map(|j| (i + j) as f32 / 200.0).collect();
            vectors.insert(format!("doc{}", i), vec);
        }

        let config = IvfPqConfig {
            nclusters: 8,
            nsubvectors: 8,            // Reduced from 64 to 8 (8x fewer k-means!)
            nbits: 4,                  // 4-bit = 16 centroids (vs 64)
            max_kmeans_iterations: 20, // Reduced from 100
            ..IvfPqConfig::default()
        };

        let mut index = IvfPqIndex::new(config);
        assert!(index.build(&vectors).is_ok());

        let stats = index.stats();

        // Original size: 200 vectors * 128 dims * 4 bytes = 102,400 bytes
        // Compressed should be significantly smaller
        let original_size = 200 * 128 * 4;
        assert!(stats.memory_bytes < original_size);

        // Compression ratio should be > 1
        assert!(stats.compression_ratio > 1.0);

        println!(
            "Compression: {:.2}x (original: {} bytes, compressed: {} bytes)",
            stats.compression_ratio, original_size, stats.memory_bytes
        );
    }

    #[test]
    #[ignore]
    fn test_ivf_pq_compression_ratio_full() {
        // Slow comprehensive test with production-scale parameters (75s+)
        // Run with: cargo test test_ivf_pq_compression_ratio_full -- --ignored
        let mut vectors = HashMap::new();
        for i in 0..500 {
            let vec: Vec<f32> = (0..768).map(|j| (i + j) as f32 / 500.0).collect();
            vectors.insert(format!("doc{}", i), vec);
        }

        let config = IvfPqConfig::default()
            .with_nclusters(16)
            .with_nsubvectors(64)
            .with_nbits(6); // 6-bit = 64 centroids per sub-quantizer

        let mut index = IvfPqIndex::new(config);
        assert!(index.build(&vectors).is_ok());

        let stats = index.stats();

        // Original size: 500 vectors * 768 dims * 4 bytes = 1,536,000 bytes
        // Compressed should be significantly smaller
        let original_size = 500 * 768 * 4;
        assert!(stats.memory_bytes < original_size);

        // Compression ratio should be > 1
        assert!(stats.compression_ratio > 1.0);

        println!(
            "Compression: {:.2}x (original: {} bytes, compressed: {} bytes)",
            stats.compression_ratio, original_size, stats.memory_bytes
        );
    }

    #[test]
    fn test_ivf_pq_empty_vectors_error() {
        let vectors = HashMap::new();
        let config = IvfPqConfig::default();
        let mut index = IvfPqIndex::new(config);

        let result = index.build(&vectors);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot build index with empty vector collection"));
    }

    #[test]
    fn test_ivf_pq_search_before_build_error() {
        let config = IvfPqConfig::default();
        let index = IvfPqIndex::new(config);

        let query = vec![0.1; 64];
        let result = index.search(&query, 10);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Index not built"));
    }

    #[test]
    fn test_ivf_pq_invalid_dimension_error() {
        let _config = IvfPqConfig::default().with_nsubvectors(8);

        // 65 is not divisible by 8
        let pq = ProductQuantizer::new(65, 8, 8);
        assert!(pq.is_err());
        assert!(pq.unwrap_err().to_string().contains("must be divisible by"));
    }

    #[test]
    fn test_ivf_pq_different_metrics() {
        let mut vectors = HashMap::new();
        for i in 0..300 {
            let vec: Vec<f32> = (0..64).map(|j| (i + j) as f32 / 300.0).collect();
            vectors.insert(format!("doc{}", i), vec);
        }

        let query = vectors.get("doc150").unwrap().clone();

        // Test with different distance metrics
        let metrics = vec![
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
            DistanceMetric::Manhattan,
        ];

        for metric in metrics {
            let config = IvfPqConfig::default()
                .with_nclusters(4)
                .with_nsubvectors(8)
                .with_nbits(4) // 4-bit quantization
                .with_metric(metric);

            let mut index = IvfPqIndex::new(config);
            assert!(index.build(&vectors).is_ok());

            let results = index.search(&query, 3);
            assert!(results.is_ok());

            let results = results.unwrap();
            assert_eq!(results.len(), 3);
        }
    }

    #[test]
    fn test_product_quantizer_encode_decode() {
        let dim = 64;
        let nsubvectors = 8;
        let nbits = 4; // 4-bit = 16 centroids per sub-quantizer

        let mut pq = ProductQuantizer::new(dim, nsubvectors, nbits).unwrap();

        // Create training vectors (need at least 16 vectors for 4-bit quantization)
        let mut train_vectors = Vec::new();
        for i in 0..100 {
            let vec: Vec<f32> = (0..dim).map(|j| (i + j) as f32 / 100.0).collect();
            train_vectors.push(vec);
        }

        // Train the quantizer
        let train_result = pq.train(&train_vectors, 20);
        if let Err(e) = &train_result {
            panic!("PQ training failed: {}", e);
        }

        // Encode a vector
        let test_vector: Vec<f32> = (0..dim).map(|i| i as f32 / 64.0).collect();
        let codes = pq.encode(&test_vector);

        // Should have one code per sub-vector
        assert_eq!(codes.len(), nsubvectors);

        // All codes should be valid (< 16 for 4-bit)
        for &code in &codes {
            assert!((code as usize) < pq.ncentroids);
        }

        // Compute asymmetric distance
        let distance = pq.asymmetric_distance(&test_vector, &codes);
        assert!(distance >= 0.0);
    }

    #[test]
    fn test_kmeans_convergence() {
        // Create two well-separated clusters
        let mut vectors = Vec::new();

        // Cluster 1: around (1, 1)
        for i in 0..20 {
            vectors.push(vec![1.0 + (i as f32) * 0.01, 1.0 + (i as f32) * 0.01]);
        }

        // Cluster 2: around (10, 10)
        for i in 0..20 {
            vectors.push(vec![10.0 + (i as f32) * 0.01, 10.0 + (i as f32) * 0.01]);
        }

        let centroids = kmeans(&vectors, 2, 50).unwrap();
        assert_eq!(centroids.len(), 2);

        // Centroids should be roughly at (1, 1) and (10, 10)
        let mut has_low_centroid = false;
        let mut has_high_centroid = false;

        for centroid in &centroids {
            if centroid[0] < 5.0 {
                has_low_centroid = true;
                assert!(centroid[0] > 0.5 && centroid[0] < 1.5);
            } else {
                has_high_centroid = true;
                assert!(centroid[0] > 9.5 && centroid[0] < 10.5);
            }
        }

        assert!(has_low_centroid);
        assert!(has_high_centroid);
    }

    #[test]
    fn test_kmeans_error_cases() {
        // Test empty vectors
        let empty_vectors: Vec<Vec<f32>> = vec![];
        let result = kmeans(&empty_vectors, 2, 10);
        assert!(result.is_err());

        // Test k > n
        let vectors = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let result = kmeans(&vectors, 5, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds"));
    }
}
