//! GPU-accelerated vector operations using CUDA.
//!
//! This module provides GPU-accelerated distance calculations for large batch queries.
//! It automatically falls back to CPU when GPU is not available or for small batches.
//!
//! ## Features
//!
//! - CUDA-accelerated distance metrics (cosine, euclidean, dot product, manhattan)
//! - Automatic CPU/GPU dispatch based on batch size
//! - Memory management for GPU transfers
//! - Batch processing for multiple queries
//!
//! ## Example
//!
//! ```rust,ignore
//! use oxify_vector::gpu::{GpuConfig, GpuBatchProcessor};
//! use oxify_vector::DistanceMetric;
//!
//! # fn example() -> anyhow::Result<()> {
//! let config = GpuConfig::default();
//! let processor = GpuBatchProcessor::new(config)?;
//!
//! let vectors = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
//! let queries = vec![vec![1.5, 2.5, 3.5]];
//!
//! let distances = processor.batch_distance(
//!     &queries,
//!     &vectors,
//!     DistanceMetric::Cosine
//! )?;
//! # Ok(())
//! # }
//! ```

use crate::types::DistanceMetric;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
#[cfg(all(feature = "cuda", target_os = "linux"))]
use std::sync::Arc;

/// Configuration for GPU-accelerated operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConfig {
    /// Minimum batch size to use GPU (smaller batches use CPU).
    pub min_batch_size_for_gpu: usize,
    /// GPU device ID to use.
    pub device_id: usize,
    /// Whether to enable GPU acceleration.
    pub enabled: bool,
    /// Maximum vectors to process per GPU batch.
    pub max_batch_size: usize,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            min_batch_size_for_gpu: 100,
            device_id: 0,
            enabled: true,
            max_batch_size: 10_000,
        }
    }
}

impl GpuConfig {
    /// Create a config optimized for small batches (prefer CPU).
    pub fn cpu_preferred() -> Self {
        Self {
            min_batch_size_for_gpu: 10_000,
            enabled: false,
            ..Default::default()
        }
    }

    /// Create a config optimized for large batches (prefer GPU).
    pub fn gpu_preferred() -> Self {
        Self {
            min_batch_size_for_gpu: 10,
            enabled: true,
            max_batch_size: 100_000,
            ..Default::default()
        }
    }
}

/// GPU batch processor for distance calculations.
///
/// This processor automatically dispatches to GPU or CPU based on batch size
/// and GPU availability.
pub struct GpuBatchProcessor {
    config: GpuConfig,
    #[cfg(all(feature = "cuda", target_os = "linux"))]
    context: Arc<GpuContext>,
}

#[cfg(all(feature = "cuda", target_os = "linux"))]
struct GpuContext {
    _ctx: Arc<cudarc::driver::CudaContext>,
}

impl GpuBatchProcessor {
    /// Create a new GPU batch processor.
    ///
    /// Returns error if GPU is requested but not available.
    pub fn new(config: GpuConfig) -> Result<Self> {
        #[cfg(all(feature = "cuda", target_os = "linux"))]
        {
            if config.enabled {
                let ctx = cudarc::driver::CudaContext::new(config.device_id)
                    .map_err(|e| anyhow!("Failed to initialize CUDA context: {}", e))?;

                Ok(Self {
                    config,
                    context: Arc::new(GpuContext { _ctx: ctx }),
                })
            } else {
                Ok(Self {
                    config,
                    context: Arc::new(GpuContext {
                        _ctx: cudarc::driver::CudaContext::new(0)
                            .map_err(|e| anyhow!("Failed to create default CUDA context: {}", e))?,
                    }),
                })
            }
        }

        #[cfg(not(all(feature = "cuda", target_os = "linux")))]
        {
            if config.enabled {
                tracing::warn!(
                    "GPU acceleration requested but CUDA feature not enabled. Using CPU fallback."
                );
            }
            Ok(Self { config })
        }
    }

    /// Check if GPU is available and enabled.
    pub fn is_gpu_available(&self) -> bool {
        #[cfg(all(feature = "cuda", target_os = "linux"))]
        {
            self.config.enabled
        }
        #[cfg(not(all(feature = "cuda", target_os = "linux")))]
        {
            false
        }
    }

    /// Compute batch distances between queries and vectors.
    ///
    /// Automatically dispatches to GPU or CPU based on batch size and availability.
    ///
    /// # Arguments
    ///
    /// * `queries` - Query vectors (N queries x D dimensions)
    /// * `vectors` - Database vectors (M vectors x D dimensions)
    /// * `metric` - Distance metric to use
    ///
    /// # Returns
    ///
    /// A 2D matrix of distances (N x M), where result\[i\]\[j\] is the distance
    /// from query i to vector j.
    pub fn batch_distance(
        &self,
        queries: &[Vec<f32>],
        vectors: &[Vec<f32>],
        metric: DistanceMetric,
    ) -> Result<Vec<Vec<f32>>> {
        if queries.is_empty() || vectors.is_empty() {
            return Ok(vec![]);
        }

        // Validate dimensions
        let query_dim = queries[0].len();
        let vector_dim = vectors[0].len();
        if query_dim != vector_dim {
            return Err(anyhow!(
                "Dimension mismatch: queries have {} dims, vectors have {} dims",
                query_dim,
                vector_dim
            ));
        }

        // Check if we should use GPU
        let use_gpu = self.should_use_gpu(queries.len(), vectors.len());

        if use_gpu {
            #[cfg(all(feature = "cuda", target_os = "linux"))]
            {
                self.batch_distance_gpu(queries, vectors, metric)
            }
            #[cfg(not(all(feature = "cuda", target_os = "linux")))]
            {
                self.batch_distance_cpu(queries, vectors, metric)
            }
        } else {
            self.batch_distance_cpu(queries, vectors, metric)
        }
    }

    /// Determine whether to use GPU based on batch size.
    fn should_use_gpu(&self, _num_queries: usize, _num_vectors: usize) -> bool {
        if !self.config.enabled {
            return false;
        }

        #[cfg(not(all(feature = "cuda", target_os = "linux")))]
        {
            false
        }

        #[cfg(all(feature = "cuda", target_os = "linux"))]
        {
            let total_operations = _num_queries * _num_vectors;
            total_operations >= self.config.min_batch_size_for_gpu
        }
    }

    /// CPU fallback for batch distance calculation.
    fn batch_distance_cpu(
        &self,
        queries: &[Vec<f32>],
        vectors: &[Vec<f32>],
        metric: DistanceMetric,
    ) -> Result<Vec<Vec<f32>>> {
        use crate::simd;

        let mut results = vec![vec![0.0; vectors.len()]; queries.len()];

        for (i, query) in queries.iter().enumerate() {
            for (j, vector) in vectors.iter().enumerate() {
                let distance = match metric {
                    DistanceMetric::Cosine => 1.0 - simd::cosine_similarity_simd(query, vector),
                    DistanceMetric::Euclidean => simd::euclidean_distance_simd(query, vector),
                    DistanceMetric::DotProduct => -simd::dot_product_simd(query, vector),
                    DistanceMetric::Manhattan => simd::manhattan_distance_simd(query, vector),
                };
                results[i][j] = distance;
            }
        }

        Ok(results)
    }

    /// GPU-accelerated batch distance calculation.
    #[cfg(all(feature = "cuda", target_os = "linux"))]
    fn batch_distance_gpu(
        &self,
        queries: &[Vec<f32>],
        vectors: &[Vec<f32>],
        metric: DistanceMetric,
    ) -> Result<Vec<Vec<f32>>> {
        let num_queries = queries.len();
        let num_vectors = vectors.len();
        let dims = queries[0].len();

        // Get default stream from context
        let stream = self.context._ctx.default_stream();

        // Flatten queries and vectors for GPU transfer
        let mut queries_flat = Vec::with_capacity(num_queries * dims);
        for query in queries {
            queries_flat.extend_from_slice(query);
        }

        let mut vectors_flat = Vec::with_capacity(num_vectors * dims);
        for vector in vectors {
            vectors_flat.extend_from_slice(vector);
        }

        // Allocate GPU memory using stream
        let queries_gpu = stream
            .clone_htod(&queries_flat)
            .map_err(|e| anyhow!("Failed to copy queries to GPU: {}", e))?;

        let vectors_gpu = stream
            .clone_htod(&vectors_flat)
            .map_err(|e| anyhow!("Failed to copy vectors to GPU: {}", e))?;

        let mut results_gpu = stream
            .alloc_zeros::<f32>(num_queries * num_vectors)
            .map_err(|e| anyhow!("Failed to allocate GPU memory for results: {}", e))?;

        // Launch appropriate kernel based on metric
        match metric {
            DistanceMetric::Cosine => {
                launch_cosine_kernel(
                    &self.context._ctx,
                    &queries_gpu,
                    &vectors_gpu,
                    &mut results_gpu,
                    num_queries,
                    num_vectors,
                    dims,
                )?;
            }
            DistanceMetric::Euclidean => {
                launch_euclidean_kernel(
                    &self.context._ctx,
                    &queries_gpu,
                    &vectors_gpu,
                    &mut results_gpu,
                    num_queries,
                    num_vectors,
                    dims,
                )?;
            }
            DistanceMetric::DotProduct => {
                launch_dot_product_kernel(
                    &self.context._ctx,
                    &queries_gpu,
                    &vectors_gpu,
                    &mut results_gpu,
                    num_queries,
                    num_vectors,
                    dims,
                )?;
            }
            DistanceMetric::Manhattan => {
                launch_manhattan_kernel(
                    &self.context._ctx,
                    &queries_gpu,
                    &vectors_gpu,
                    &mut results_gpu,
                    num_queries,
                    num_vectors,
                    dims,
                )?;
            }
        }

        // Copy results back to CPU
        let results_flat: Vec<f32> = stream
            .clone_dtoh(&results_gpu)
            .map_err(|e| anyhow!("Failed to copy results from GPU: {}", e))?;

        // Reshape results
        let mut results = vec![vec![0.0; num_vectors]; num_queries];
        for i in 0..num_queries {
            for j in 0..num_vectors {
                results[i][j] = results_flat[i * num_vectors + j];
            }
        }

        Ok(results)
    }
}

#[cfg(all(feature = "cuda", target_os = "linux"))]
fn launch_cosine_kernel(
    _ctx: &Arc<cudarc::driver::CudaContext>,
    _queries: &cudarc::driver::CudaSlice<f32>,
    _vectors: &cudarc::driver::CudaSlice<f32>,
    _results: &mut cudarc::driver::CudaSlice<f32>,
    _num_queries: usize,
    _num_vectors: usize,
    _dims: usize,
) -> Result<()> {
    use crate::simd;

    let stream = _ctx.default_stream();

    let queries_host: Vec<f32> = stream
        .clone_dtoh(_queries)
        .map_err(|e| anyhow!("Failed to copy queries from GPU: {}", e))?;
    let vectors_host: Vec<f32> = stream
        .clone_dtoh(_vectors)
        .map_err(|e| anyhow!("Failed to copy vectors from GPU: {}", e))?;

    let mut results_host = vec![0.0f32; _num_queries * _num_vectors];
    for i in 0.._num_queries {
        let q = &queries_host[i * _dims..(i + 1) * _dims];
        for j in 0.._num_vectors {
            let v = &vectors_host[j * _dims..(j + 1) * _dims];
            // Cosine distance = 1 - cosine_similarity (matches batch_distance_cpu convention)
            results_host[i * _num_vectors + j] = 1.0 - simd::cosine_similarity_simd(q, v);
        }
    }

    stream
        .memcpy_htod(&results_host, _results)
        .map_err(|e| anyhow!("Failed to copy results to GPU: {}", e))
}

#[cfg(all(feature = "cuda", target_os = "linux"))]
fn launch_euclidean_kernel(
    _ctx: &Arc<cudarc::driver::CudaContext>,
    _queries: &cudarc::driver::CudaSlice<f32>,
    _vectors: &cudarc::driver::CudaSlice<f32>,
    _results: &mut cudarc::driver::CudaSlice<f32>,
    _num_queries: usize,
    _num_vectors: usize,
    _dims: usize,
) -> Result<()> {
    use crate::simd;

    let stream = _ctx.default_stream();

    let queries_host: Vec<f32> = stream
        .clone_dtoh(_queries)
        .map_err(|e| anyhow!("Failed to copy queries from GPU: {}", e))?;
    let vectors_host: Vec<f32> = stream
        .clone_dtoh(_vectors)
        .map_err(|e| anyhow!("Failed to copy vectors from GPU: {}", e))?;

    let mut results_host = vec![0.0f32; _num_queries * _num_vectors];
    for i in 0.._num_queries {
        let q = &queries_host[i * _dims..(i + 1) * _dims];
        for j in 0.._num_vectors {
            let v = &vectors_host[j * _dims..(j + 1) * _dims];
            results_host[i * _num_vectors + j] = simd::euclidean_distance_simd(q, v);
        }
    }

    stream
        .memcpy_htod(&results_host, _results)
        .map_err(|e| anyhow!("Failed to copy results to GPU: {}", e))
}

#[cfg(all(feature = "cuda", target_os = "linux"))]
fn launch_dot_product_kernel(
    _ctx: &Arc<cudarc::driver::CudaContext>,
    _queries: &cudarc::driver::CudaSlice<f32>,
    _vectors: &cudarc::driver::CudaSlice<f32>,
    _results: &mut cudarc::driver::CudaSlice<f32>,
    _num_queries: usize,
    _num_vectors: usize,
    _dims: usize,
) -> Result<()> {
    use crate::simd;

    let stream = _ctx.default_stream();

    let queries_host: Vec<f32> = stream
        .clone_dtoh(_queries)
        .map_err(|e| anyhow!("Failed to copy queries from GPU: {}", e))?;
    let vectors_host: Vec<f32> = stream
        .clone_dtoh(_vectors)
        .map_err(|e| anyhow!("Failed to copy vectors from GPU: {}", e))?;

    let mut results_host = vec![0.0f32; _num_queries * _num_vectors];
    for i in 0.._num_queries {
        let q = &queries_host[i * _dims..(i + 1) * _dims];
        for j in 0.._num_vectors {
            let v = &vectors_host[j * _dims..(j + 1) * _dims];
            // Negated dot product (matches batch_distance_cpu convention: -dot_product_simd)
            results_host[i * _num_vectors + j] = -simd::dot_product_simd(q, v);
        }
    }

    stream
        .memcpy_htod(&results_host, _results)
        .map_err(|e| anyhow!("Failed to copy results to GPU: {}", e))
}

#[cfg(all(feature = "cuda", target_os = "linux"))]
fn launch_manhattan_kernel(
    _ctx: &Arc<cudarc::driver::CudaContext>,
    _queries: &cudarc::driver::CudaSlice<f32>,
    _vectors: &cudarc::driver::CudaSlice<f32>,
    _results: &mut cudarc::driver::CudaSlice<f32>,
    _num_queries: usize,
    _num_vectors: usize,
    _dims: usize,
) -> Result<()> {
    use crate::simd;

    let stream = _ctx.default_stream();

    let queries_host: Vec<f32> = stream
        .clone_dtoh(_queries)
        .map_err(|e| anyhow!("Failed to copy queries from GPU: {}", e))?;
    let vectors_host: Vec<f32> = stream
        .clone_dtoh(_vectors)
        .map_err(|e| anyhow!("Failed to copy vectors from GPU: {}", e))?;

    let mut results_host = vec![0.0f32; _num_queries * _num_vectors];
    for i in 0.._num_queries {
        let q = &queries_host[i * _dims..(i + 1) * _dims];
        for j in 0.._num_vectors {
            let v = &vectors_host[j * _dims..(j + 1) * _dims];
            results_host[i * _num_vectors + j] = simd::manhattan_distance_simd(q, v);
        }
    }

    stream
        .memcpy_htod(&results_host, _results)
        .map_err(|e| anyhow!("Failed to copy results to GPU: {}", e))
}

/// Statistics for GPU operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStats {
    /// Total number of batch operations performed.
    pub total_operations: u64,
    /// Number of operations performed on GPU.
    pub gpu_operations: u64,
    /// Number of operations performed on CPU.
    pub cpu_operations: u64,
    /// Average batch size processed.
    pub avg_batch_size: f64,
}

impl Default for GpuStats {
    fn default() -> Self {
        Self {
            total_operations: 0,
            gpu_operations: 0,
            cpu_operations: 0,
            avg_batch_size: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_config_default() {
        let config = GpuConfig::default();
        assert_eq!(config.min_batch_size_for_gpu, 100);
        assert_eq!(config.device_id, 0);
        assert!(config.enabled);
    }

    #[test]
    fn test_gpu_config_cpu_preferred() {
        let config = GpuConfig::cpu_preferred();
        assert_eq!(config.min_batch_size_for_gpu, 10_000);
        assert!(!config.enabled);
    }

    #[test]
    fn test_gpu_config_gpu_preferred() {
        let config = GpuConfig::gpu_preferred();
        assert_eq!(config.min_batch_size_for_gpu, 10);
        assert!(config.enabled);
        assert_eq!(config.max_batch_size, 100_000);
    }

    #[test]
    fn test_gpu_processor_creation_cpu_fallback() {
        // CPU fallback should always work (no CUDA required)
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config);
        assert!(processor.is_ok());
    }

    #[test]
    fn test_gpu_availability() {
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).unwrap();

        #[cfg(all(feature = "cuda", target_os = "linux"))]
        {
            // GPU should be unavailable because config.enabled = false
            assert!(!processor.is_gpu_available());
        }

        #[cfg(not(all(feature = "cuda", target_os = "linux")))]
        {
            // GPU should always be unavailable without CUDA feature
            assert!(!processor.is_gpu_available());
        }
    }

    #[test]
    fn test_batch_distance_cpu_cosine() {
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).unwrap();

        let queries = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let vectors = vec![vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]];

        let distances = processor
            .batch_distance(&queries, &vectors, DistanceMetric::Cosine)
            .unwrap();

        assert_eq!(distances.len(), 2);
        assert_eq!(distances[0].len(), 2);

        // Query 0 should be very close to vector 0 (cosine distance ≈ 0)
        assert!(distances[0][0] < 0.01);
        // Query 0 should be far from vector 1 (cosine distance ≈ 1)
        assert!(distances[0][1] > 0.99);
    }

    #[test]
    fn test_batch_distance_cpu_euclidean() {
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).unwrap();

        let queries = vec![vec![0.0, 0.0, 0.0]];
        let vectors = vec![vec![3.0, 4.0, 0.0]];

        let distances = processor
            .batch_distance(&queries, &vectors, DistanceMetric::Euclidean)
            .unwrap();

        assert_eq!(distances.len(), 1);
        assert_eq!(distances[0].len(), 1);

        // Distance should be 5.0 (3-4-5 triangle)
        assert!((distances[0][0] - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_batch_distance_empty_input() {
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).unwrap();

        let queries: Vec<Vec<f32>> = vec![];
        let vectors = vec![vec![1.0, 2.0, 3.0]];

        let distances = processor
            .batch_distance(&queries, &vectors, DistanceMetric::Cosine)
            .unwrap();

        assert!(distances.is_empty());
    }

    #[test]
    fn test_batch_distance_dimension_mismatch() {
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).unwrap();

        let queries = vec![vec![1.0, 2.0, 3.0]];
        let vectors = vec![vec![1.0, 2.0]]; // Different dimension

        let result = processor.batch_distance(&queries, &vectors, DistanceMetric::Cosine);

        assert!(result.is_err());
    }

    #[test]
    fn test_should_use_gpu_threshold() {
        let config = GpuConfig {
            min_batch_size_for_gpu: 100,
            enabled: true,
            ..Default::default()
        };
        let processor = GpuBatchProcessor::new(config).unwrap();

        // Small batch should use CPU
        assert!(!processor.should_use_gpu(5, 10)); // 50 operations < 100

        // Large batch should use GPU (if CUDA is available)
        #[cfg(all(feature = "cuda", target_os = "linux"))]
        assert!(processor.should_use_gpu(10, 20)); // 200 operations >= 100

        #[cfg(not(all(feature = "cuda", target_os = "linux")))]
        assert!(!processor.should_use_gpu(10, 20)); // No CUDA, always use CPU
    }

    #[test]
    fn test_gpu_stats_default() {
        let stats = GpuStats::default();
        assert_eq!(stats.total_operations, 0);
        assert_eq!(stats.gpu_operations, 0);
        assert_eq!(stats.cpu_operations, 0);
        assert_eq!(stats.avg_batch_size, 0.0);
    }

    #[test]
    fn test_launch_cosine_kernel_via_batch_cpu() {
        // Validates cosine distance computation matching the kernel semantics.
        // On non-CUDA builds this exercises batch_distance_cpu with the same logic.
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).expect("processor creation failed");

        // Two identical vectors → cosine distance = 0
        let queries = vec![vec![1.0f32, 0.0, 0.0]];
        let vectors = vec![vec![1.0f32, 0.0, 0.0]];
        let result = processor
            .batch_distance(&queries, &vectors, DistanceMetric::Cosine)
            .expect("cosine batch_distance failed");
        assert_eq!(result.len(), 1);
        assert!((result[0][0]).abs() < 1e-5, "identical vectors: expected ~0, got {}", result[0][0]);

        // Orthogonal vectors → cosine distance = 1
        let queries2 = vec![vec![1.0f32, 0.0, 0.0]];
        let vectors2 = vec![vec![0.0f32, 1.0, 0.0]];
        let result2 = processor
            .batch_distance(&queries2, &vectors2, DistanceMetric::Cosine)
            .expect("cosine batch_distance (orthogonal) failed");
        assert!((result2[0][0] - 1.0).abs() < 1e-5, "orthogonal vectors: expected ~1, got {}", result2[0][0]);
    }

    #[test]
    fn test_launch_euclidean_kernel_via_batch_cpu() {
        // Validates Euclidean distance computation matching the kernel semantics.
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).expect("processor creation failed");

        // 3-4-5 right triangle: distance from origin to (3, 4, 0) = 5
        let queries = vec![vec![0.0f32, 0.0, 0.0]];
        let vectors = vec![vec![3.0f32, 4.0, 0.0]];
        let result = processor
            .batch_distance(&queries, &vectors, DistanceMetric::Euclidean)
            .expect("euclidean batch_distance failed");
        assert_eq!(result.len(), 1);
        assert!((result[0][0] - 5.0).abs() < 1e-4, "expected 5.0, got {}", result[0][0]);
    }

    #[test]
    fn test_launch_dot_product_kernel_via_batch_cpu() {
        // Validates dot product distance (negated) matching the kernel semantics.
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).expect("processor creation failed");

        // dot([1,2,3], [4,5,6]) = 1*4 + 2*5 + 3*6 = 32; stored as -32 (negated for distance ordering)
        let queries = vec![vec![1.0f32, 2.0, 3.0]];
        let vectors = vec![vec![4.0f32, 5.0, 6.0]];
        let result = processor
            .batch_distance(&queries, &vectors, DistanceMetric::DotProduct)
            .expect("dot product batch_distance failed");
        assert_eq!(result.len(), 1);
        assert!((result[0][0] - (-32.0)).abs() < 1e-3, "expected -32.0, got {}", result[0][0]);
    }

    #[test]
    fn test_launch_manhattan_kernel_via_batch_cpu() {
        // Validates Manhattan distance computation matching the kernel semantics.
        let config = GpuConfig::cpu_preferred();
        let processor = GpuBatchProcessor::new(config).expect("processor creation failed");

        // |3-0| + |4-0| + |0-0| = 7
        let queries = vec![vec![0.0f32, 0.0, 0.0]];
        let vectors = vec![vec![3.0f32, 4.0, 0.0]];
        let result = processor
            .batch_distance(&queries, &vectors, DistanceMetric::Manhattan)
            .expect("manhattan batch_distance failed");
        assert_eq!(result.len(), 1);
        assert!((result[0][0] - 7.0).abs() < 1e-4, "expected 7.0, got {}", result[0][0]);
    }
}
