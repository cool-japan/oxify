//! Vector Quantization for Memory Optimization
//!
//! Provides scalar (8-bit), 4-bit, and binary (1-bit) quantization.
//!
//! ## Scalar Quantization (8-bit)
//! Compresses float32 vectors to int8/uint8, reducing memory usage by ~4x.
//!
//! **Benefits:**
//! - **Memory**: 4x reduction (float32 → uint8)
//! - **Speed**: Faster distance computations with SIMD
//! - **Scalability**: Fit 4x more vectors in memory
//!
//! **Trade-offs:**
//! - Small accuracy loss (~1-2% recall degradation)
//! - One-time quantization cost during build
//!
//! ## 4-bit Quantization
//! Compresses float32 vectors to 4-bit, reducing memory usage by ~8x.
//!
//! **Benefits:**
//! - **Memory**: 8x reduction (float32 → 4-bit)
//! - **Speed**: Fast distance computations with nibble packing
//! - **Scalability**: Fit 8x more vectors in memory
//! - **Sweet Spot**: Best balance between memory and accuracy
//!
//! **Trade-offs:**
//! - Moderate accuracy loss (~2-4% recall degradation)
//! - Nibble packing/unpacking overhead
//!
//! ## Binary Quantization (1-bit)
//! Compresses float32 vectors to 1-bit, reducing memory usage by ~32x.
//!
//! **Benefits:**
//! - **Memory**: 32x reduction (float32 → 1-bit)
//! - **Speed**: Extremely fast Hamming distance with bitwise operations
//! - **Scalability**: Fit 32x more vectors in memory
//!
//! **Trade-offs:**
//! - Higher accuracy loss (~5-10% recall degradation)
//! - Best for high-dimensional vectors (>128 dims)
//!
//! ## FP16 (Half-Precision) Quantization
//! Compresses float32 vectors to float16 (16-bit), reducing memory usage by 2x.
//!
//! **Benefits:**
//! - **Memory**: 2x reduction (float32 → float16)
//! - **Accuracy**: Minimal accuracy loss (<0.1% recall degradation)
//! - **Speed**: No quantization overhead, direct float16 operations
//! - **Hardware Support**: Native support on modern CPUs/GPUs
//!
//! **Trade-offs:**
//! - Lower compression ratio than 8-bit/4-bit/binary quantization
//! - Requires FP16 hardware support for maximum performance
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::quantization::{ScalarQuantizer, QuantizationConfig};
//!
//! let config = QuantizationConfig::default();
//! let mut quantizer = ScalarQuantizer::new(config);
//!
//! // Fit quantizer to data
//! let vectors = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
//! quantizer.fit(&vectors);
//!
//! // Quantize a vector
//! let quantized = quantizer.quantize(&[1.5, 2.5, 3.5]);
//! assert_eq!(quantized.len(), 3);
//!
//! // Dequantize back to floats
//! let dequantized = quantizer.dequantize(&quantized);
//! assert_eq!(dequantized.len(), 3);
//! ```

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::simd::quantized_manhattan_distance_simd;

/// Quantization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationConfig {
    /// Number of bits per value (currently only 8-bit supported)
    pub bits: u8,
    /// Whether to use signed quantization (int8) or unsigned (uint8)
    pub signed: bool,
}

impl Default for QuantizationConfig {
    fn default() -> Self {
        Self {
            bits: 8,
            signed: false, // uint8 by default
        }
    }
}

/// Scalar quantizer for compressing float32 vectors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarQuantizer {
    config: QuantizationConfig,
    /// Per-dimension minimum values
    min_vals: Vec<f32>,
    /// Per-dimension maximum values
    max_vals: Vec<f32>,
    /// Per-dimension scale factors
    scales: Vec<f32>,
    /// Number of dimensions
    dimensions: usize,
    /// Whether the quantizer has been fitted
    is_fitted: bool,
}

impl ScalarQuantizer {
    /// Create a new scalar quantizer
    pub fn new(config: QuantizationConfig) -> Self {
        Self {
            config,
            min_vals: Vec::new(),
            max_vals: Vec::new(),
            scales: Vec::new(),
            dimensions: 0,
            is_fitted: false,
        }
    }

    /// Fit quantizer to training data
    ///
    /// Computes per-dimension min/max values for quantization.
    pub fn fit(&mut self, vectors: &[Vec<f32>]) -> Result<()> {
        if vectors.is_empty() {
            return Err(anyhow!("Cannot fit quantizer on empty data"));
        }

        let dim = vectors[0].len();
        if vectors.iter().any(|v| v.len() != dim) {
            return Err(anyhow!("All vectors must have the same dimension"));
        }

        self.dimensions = dim;
        self.min_vals = vec![f32::INFINITY; dim];
        self.max_vals = vec![f32::NEG_INFINITY; dim];

        // Compute per-dimension min/max
        for vector in vectors {
            for (i, &val) in vector.iter().enumerate() {
                self.min_vals[i] = self.min_vals[i].min(val);
                self.max_vals[i] = self.max_vals[i].max(val);
            }
        }

        // Compute scale factors
        self.scales = Vec::with_capacity(dim);
        let max_quant_val = if self.config.signed { 127.0 } else { 255.0 };

        for i in 0..dim {
            let range = self.max_vals[i] - self.min_vals[i];
            // Avoid division by zero for constant dimensions
            self.scales.push(if range > 1e-10 {
                max_quant_val / range
            } else {
                1.0
            });
        }

        self.is_fitted = true;
        Ok(())
    }

    /// Quantize a float32 vector to uint8/int8
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8> {
        assert!(self.is_fitted, "Quantizer must be fitted before use");
        assert_eq!(vector.len(), self.dimensions, "Vector dimension mismatch");

        vector
            .iter()
            .enumerate()
            .map(|(i, &val)| {
                // Clip to [min, max]
                let clipped = val.max(self.min_vals[i]).min(self.max_vals[i]);
                // Scale to [0, 255] or [-127, 127]
                let scaled = (clipped - self.min_vals[i]) * self.scales[i];
                // Round and convert to u8
                scaled.round().clamp(0.0, 255.0) as u8
            })
            .collect()
    }

    /// Dequantize a uint8/int8 vector back to float32
    pub fn dequantize(&self, quantized: &[u8]) -> Vec<f32> {
        assert!(self.is_fitted, "Quantizer must be fitted before use");
        assert_eq!(
            quantized.len(),
            self.dimensions,
            "Quantized vector dimension mismatch"
        );

        quantized
            .iter()
            .enumerate()
            .map(|(i, &val)| {
                // Convert back to float and rescale
                let scaled = val as f32 / self.scales[i];
                scaled + self.min_vals[i]
            })
            .collect()
    }

    /// Quantize multiple vectors
    pub fn quantize_batch(&self, vectors: &[Vec<f32>]) -> Vec<Vec<u8>> {
        vectors.iter().map(|v| self.quantize(v)).collect()
    }

    /// Dequantize multiple vectors
    pub fn dequantize_batch(&self, quantized: &[Vec<u8>]) -> Vec<Vec<f32>> {
        quantized.iter().map(|v| self.dequantize(v)).collect()
    }

    /// Compute approximate distance between quantized vectors
    ///
    /// This is faster than dequantizing and computing distance on float32.
    /// Uses SIMD-optimized Manhattan distance for maximum performance.
    pub fn quantized_distance(&self, a: &[u8], b: &[u8]) -> f32 {
        assert_eq!(a.len(), b.len(), "Vector dimension mismatch");

        // Use SIMD-optimized Manhattan distance on quantized values
        quantized_manhattan_distance_simd(a, b) as f32
    }

    /// Get compression ratio
    pub fn compression_ratio(&self) -> f32 {
        // float32 → uint8 = 4x compression
        4.0
    }

    /// Get memory savings percentage
    pub fn memory_savings(&self) -> f32 {
        // 75% memory savings (1/4 of original size)
        0.75
    }

    /// Check if quantizer is fitted
    pub fn is_fitted(&self) -> bool {
        self.is_fitted
    }

    /// Get number of dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Quantized vector index for memory-efficient search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedVectorIndex {
    quantizer: ScalarQuantizer,
    /// Quantized vectors
    quantized_vectors: Vec<Vec<u8>>,
    /// Entity IDs
    entity_ids: Vec<String>,
}

impl QuantizedVectorIndex {
    /// Create a new quantized index
    pub fn new(config: QuantizationConfig) -> Self {
        Self {
            quantizer: ScalarQuantizer::new(config),
            quantized_vectors: Vec::new(),
            entity_ids: Vec::new(),
        }
    }

    /// Build index from float32 vectors
    pub fn build(&mut self, vectors: &[(String, Vec<f32>)]) -> Result<()> {
        if vectors.is_empty() {
            return Err(anyhow!("Cannot build index from empty vectors"));
        }

        // Extract float vectors for fitting
        let float_vecs: Vec<Vec<f32>> = vectors.iter().map(|(_, v)| v.clone()).collect();

        // Fit quantizer
        self.quantizer.fit(&float_vecs)?;

        // Quantize all vectors
        self.entity_ids = vectors.iter().map(|(id, _)| id.clone()).collect();
        self.quantized_vectors = self.quantizer.quantize_batch(&float_vecs);

        Ok(())
    }

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>> {
        if !self.quantizer.is_fitted() {
            return Err(anyhow!("Index not built"));
        }

        // Quantize query
        let quantized_query = self.quantizer.quantize(query);

        // Compute distances to all vectors
        let mut distances: Vec<(usize, f32)> = self
            .quantized_vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (i, self.quantizer.quantized_distance(&quantized_query, v)))
            .collect();

        // Sort by distance (ascending)
        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top-k
        Ok(distances
            .iter()
            .take(k.min(self.entity_ids.len()))
            .map(|(idx, dist)| (self.entity_ids[*idx].clone(), *dist))
            .collect())
    }

    /// Get index statistics
    pub fn stats(&self) -> QuantizedIndexStats {
        let num_vectors = self.quantized_vectors.len();
        let dimensions = self.quantizer.dimensions();
        let original_bytes = num_vectors * dimensions * 4; // float32
        let quantized_bytes = num_vectors * dimensions; // uint8

        QuantizedIndexStats {
            num_vectors,
            dimensions,
            compression_ratio: self.quantizer.compression_ratio(),
            memory_savings: self.quantizer.memory_savings(),
            original_bytes,
            quantized_bytes,
        }
    }
}

/// Statistics for quantized index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedIndexStats {
    pub num_vectors: usize,
    pub dimensions: usize,
    pub compression_ratio: f32,
    pub memory_savings: f32,
    pub original_bytes: usize,
    pub quantized_bytes: usize,
}

// ============================================================================
// Binary Quantization (1-bit per dimension)
// ============================================================================

/// Binary quantization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryQuantizationConfig {
    /// Use mean-based thresholding (vs zero-based)
    pub use_mean_threshold: bool,
}

impl Default for BinaryQuantizationConfig {
    fn default() -> Self {
        Self {
            use_mean_threshold: true,
        }
    }
}

/// Binary quantizer for extreme memory compression (32x reduction)
///
/// Converts float32 vectors to 1-bit by thresholding each dimension.
/// Uses efficient bit-packing and Hamming distance for similarity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryQuantizer {
    config: BinaryQuantizationConfig,
    /// Per-dimension thresholds (mean or zero)
    thresholds: Vec<f32>,
    /// Number of dimensions
    dimensions: usize,
    /// Whether the quantizer has been fitted
    is_fitted: bool,
}

impl BinaryQuantizer {
    /// Create a new binary quantizer
    pub fn new(config: BinaryQuantizationConfig) -> Self {
        Self {
            config,
            thresholds: Vec::new(),
            dimensions: 0,
            is_fitted: false,
        }
    }

    /// Fit quantizer to training data
    ///
    /// Computes per-dimension thresholds (mean or zero).
    pub fn fit(&mut self, vectors: &[Vec<f32>]) -> Result<()> {
        if vectors.is_empty() {
            return Err(anyhow!("Cannot fit quantizer on empty data"));
        }

        let dim = vectors[0].len();
        if vectors.iter().any(|v| v.len() != dim) {
            return Err(anyhow!("All vectors must have the same dimension"));
        }

        self.dimensions = dim;

        if self.config.use_mean_threshold {
            // Compute per-dimension means as thresholds
            self.thresholds = vec![0.0; dim];
            for vector in vectors {
                for (i, &val) in vector.iter().enumerate() {
                    self.thresholds[i] += val;
                }
            }
            let count = vectors.len() as f32;
            for threshold in &mut self.thresholds {
                *threshold /= count;
            }
        } else {
            // Use zero threshold
            self.thresholds = vec![0.0; dim];
        }

        self.is_fitted = true;
        Ok(())
    }

    /// Quantize a float32 vector to binary (packed as u8 array)
    ///
    /// Each bit represents whether the value is above the threshold.
    /// Bits are packed into u8 bytes (8 bits per byte).
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8> {
        assert!(self.is_fitted, "Quantizer must be fitted before use");
        assert_eq!(vector.len(), self.dimensions, "Vector dimension mismatch");

        // Calculate number of bytes needed (ceiling division)
        let num_bytes = self.dimensions.div_ceil(8);
        let mut binary = vec![0u8; num_bytes];

        for (i, &val) in vector.iter().enumerate() {
            if val > self.thresholds[i] {
                let byte_idx = i / 8;
                let bit_idx = i % 8;
                binary[byte_idx] |= 1u8 << bit_idx;
            }
        }

        binary
    }

    /// Dequantize a binary vector back to float32 (approximate)
    ///
    /// Reconstructs as threshold ± 1.0 based on bit values.
    pub fn dequantize(&self, binary: &[u8]) -> Vec<f32> {
        assert!(self.is_fitted, "Quantizer must be fitted before use");
        let expected_bytes = self.dimensions.div_ceil(8);
        assert_eq!(binary.len(), expected_bytes, "Binary vector size mismatch");

        let mut vector = Vec::with_capacity(self.dimensions);
        for i in 0..self.dimensions {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            let bit_set = (binary[byte_idx] >> bit_idx) & 1 == 1;

            // Reconstruct as threshold ± 1.0
            let val = if bit_set {
                self.thresholds[i] + 1.0
            } else {
                self.thresholds[i] - 1.0
            };
            vector.push(val);
        }

        vector
    }

    /// Quantize multiple vectors
    pub fn quantize_batch(&self, vectors: &[Vec<f32>]) -> Vec<Vec<u8>> {
        vectors.iter().map(|v| self.quantize(v)).collect()
    }

    /// Dequantize multiple vectors
    pub fn dequantize_batch(&self, binary: &[Vec<u8>]) -> Vec<Vec<f32>> {
        binary.iter().map(|v| self.dequantize(v)).collect()
    }

    /// Compute Hamming distance between binary vectors
    ///
    /// Hamming distance = number of differing bits (very fast with XOR + popcount).
    #[inline]
    pub fn hamming_distance(&self, a: &[u8], b: &[u8]) -> u32 {
        assert_eq!(a.len(), b.len(), "Binary vector size mismatch");

        a.iter()
            .zip(b.iter())
            .map(|(&x, &y)| (x ^ y).count_ones())
            .sum()
    }

    /// Compute normalized Hamming similarity (0.0 to 1.0)
    ///
    /// Similarity = 1 - (hamming_distance / num_bits)
    #[inline]
    pub fn hamming_similarity(&self, a: &[u8], b: &[u8]) -> f32 {
        let distance = self.hamming_distance(a, b);
        1.0 - (distance as f32 / self.dimensions as f32)
    }

    /// Get compression ratio
    pub fn compression_ratio(&self) -> f32 {
        // float32 (4 bytes) → 1 bit (1/8 byte) = 32x compression
        32.0
    }

    /// Get memory savings percentage
    pub fn memory_savings(&self) -> f32 {
        // 96.875% memory savings (1/32 of original size)
        0.96875
    }

    /// Check if quantizer is fitted
    pub fn is_fitted(&self) -> bool {
        self.is_fitted
    }

    /// Get number of dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Binary quantized vector index for extreme memory efficiency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryQuantizedIndex {
    quantizer: BinaryQuantizer,
    /// Binary quantized vectors
    binary_vectors: Vec<Vec<u8>>,
    /// Entity IDs
    entity_ids: Vec<String>,
}

impl BinaryQuantizedIndex {
    /// Create a new binary quantized index
    pub fn new(config: BinaryQuantizationConfig) -> Self {
        Self {
            quantizer: BinaryQuantizer::new(config),
            binary_vectors: Vec::new(),
            entity_ids: Vec::new(),
        }
    }

    /// Build index from float32 vectors
    pub fn build(&mut self, vectors: &[(String, Vec<f32>)]) -> Result<()> {
        if vectors.is_empty() {
            return Err(anyhow!("Cannot build index from empty vectors"));
        }

        // Extract float vectors for fitting
        let float_vecs: Vec<Vec<f32>> = vectors.iter().map(|(_, v)| v.clone()).collect();

        // Fit quantizer
        self.quantizer.fit(&float_vecs)?;

        // Quantize all vectors
        self.entity_ids = vectors.iter().map(|(id, _)| id.clone()).collect();
        self.binary_vectors = self.quantizer.quantize_batch(&float_vecs);

        Ok(())
    }

    /// Search for k nearest neighbors using Hamming similarity
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>> {
        if !self.quantizer.is_fitted() {
            return Err(anyhow!("Index not built"));
        }

        // Quantize query
        let binary_query = self.quantizer.quantize(query);

        // Compute similarities to all vectors
        let mut similarities: Vec<(usize, f32)> = self
            .binary_vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (i, self.quantizer.hamming_similarity(&binary_query, v)))
            .collect();

        // Sort by similarity (descending - higher is better)
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top-k
        Ok(similarities
            .iter()
            .take(k.min(self.entity_ids.len()))
            .map(|(idx, sim)| (self.entity_ids[*idx].clone(), *sim))
            .collect())
    }

    /// Get index statistics
    pub fn stats(&self) -> BinaryQuantizedIndexStats {
        let num_vectors = self.binary_vectors.len();
        let dimensions = self.quantizer.dimensions();
        let original_bytes = num_vectors * dimensions * 4; // float32
        let binary_bytes = num_vectors * dimensions.div_ceil(8); // 1 bit per dim, packed

        BinaryQuantizedIndexStats {
            num_vectors,
            dimensions,
            compression_ratio: self.quantizer.compression_ratio(),
            memory_savings: self.quantizer.memory_savings(),
            original_bytes,
            binary_bytes,
        }
    }
}

/// Statistics for binary quantized index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryQuantizedIndexStats {
    pub num_vectors: usize,
    pub dimensions: usize,
    pub compression_ratio: f32,
    pub memory_savings: f32,
    pub original_bytes: usize,
    pub binary_bytes: usize,
}

// ============================================================================
// 4-bit Quantization (Nibble packing)
// ============================================================================

/// 4-bit quantizer for balanced memory/accuracy trade-off (8x compression)
///
/// Converts float32 vectors to 4-bit (16 levels) by per-dimension min-max scaling.
/// Packs two 4-bit values into each u8 byte (high and low nibbles).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FourBitQuantizer {
    /// Per-dimension minimum values
    min_vals: Vec<f32>,
    /// Per-dimension maximum values
    max_vals: Vec<f32>,
    /// Per-dimension scale factors
    scales: Vec<f32>,
    /// Number of dimensions
    dimensions: usize,
    /// Whether the quantizer has been fitted
    is_fitted: bool,
}

impl FourBitQuantizer {
    /// Create a new 4-bit quantizer
    pub fn new() -> Self {
        Self {
            min_vals: Vec::new(),
            max_vals: Vec::new(),
            scales: Vec::new(),
            dimensions: 0,
            is_fitted: false,
        }
    }

    /// Fit quantizer to training data
    ///
    /// Computes per-dimension min/max values for 4-bit quantization.
    pub fn fit(&mut self, vectors: &[Vec<f32>]) -> Result<()> {
        if vectors.is_empty() {
            return Err(anyhow!("Cannot fit quantizer on empty data"));
        }

        let dim = vectors[0].len();
        if vectors.iter().any(|v| v.len() != dim) {
            return Err(anyhow!("All vectors must have the same dimension"));
        }

        self.dimensions = dim;
        self.min_vals = vec![f32::INFINITY; dim];
        self.max_vals = vec![f32::NEG_INFINITY; dim];

        // Compute per-dimension min/max
        for vector in vectors {
            for (i, &val) in vector.iter().enumerate() {
                self.min_vals[i] = self.min_vals[i].min(val);
                self.max_vals[i] = self.max_vals[i].max(val);
            }
        }

        // Compute scale factors for 4-bit (0-15)
        self.scales = Vec::with_capacity(dim);
        for i in 0..dim {
            let range = self.max_vals[i] - self.min_vals[i];
            // Avoid division by zero for constant dimensions
            self.scales
                .push(if range > 1e-10 { 15.0 / range } else { 1.0 });
        }

        self.is_fitted = true;
        Ok(())
    }

    /// Quantize a float32 vector to 4-bit (packed as u8 array)
    ///
    /// Two 4-bit values are packed into each u8: high nibble (bits 4-7) and low nibble (bits 0-3).
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8> {
        assert!(self.is_fitted, "Quantizer must be fitted before use");
        assert_eq!(vector.len(), self.dimensions, "Vector dimension mismatch");

        // Calculate number of bytes needed (2 values per byte, ceiling division)
        let num_bytes = self.dimensions.div_ceil(2);
        let mut quantized = vec![0u8; num_bytes];

        for (i, &val) in vector.iter().enumerate() {
            // Clip to [min, max]
            let clipped = val.max(self.min_vals[i]).min(self.max_vals[i]);
            // Scale to [0, 15]
            let scaled = (clipped - self.min_vals[i]) * self.scales[i];
            let nibble = scaled.round().clamp(0.0, 15.0) as u8;

            let byte_idx = i / 2;
            if i % 2 == 0 {
                // Even index: store in low nibble (bits 0-3)
                quantized[byte_idx] |= nibble;
            } else {
                // Odd index: store in high nibble (bits 4-7)
                quantized[byte_idx] |= nibble << 4;
            }
        }

        quantized
    }

    /// Dequantize a 4-bit vector back to float32
    pub fn dequantize(&self, quantized: &[u8]) -> Vec<f32> {
        assert!(self.is_fitted, "Quantizer must be fitted before use");
        let expected_bytes = self.dimensions.div_ceil(2);
        assert_eq!(
            quantized.len(),
            expected_bytes,
            "Quantized vector size mismatch"
        );

        let mut vector = Vec::with_capacity(self.dimensions);
        for i in 0..self.dimensions {
            let byte_idx = i / 2;
            let nibble = if i % 2 == 0 {
                // Even index: extract low nibble
                quantized[byte_idx] & 0x0F
            } else {
                // Odd index: extract high nibble
                (quantized[byte_idx] >> 4) & 0x0F
            };

            // Convert back to float and rescale
            let scaled = nibble as f32 / self.scales[i];
            vector.push(scaled + self.min_vals[i]);
        }

        vector
    }

    /// Quantize multiple vectors
    pub fn quantize_batch(&self, vectors: &[Vec<f32>]) -> Vec<Vec<u8>> {
        vectors.iter().map(|v| self.quantize(v)).collect()
    }

    /// Dequantize multiple vectors
    pub fn dequantize_batch(&self, quantized: &[Vec<u8>]) -> Vec<Vec<f32>> {
        quantized.iter().map(|v| self.dequantize(v)).collect()
    }

    /// Compute approximate distance between 4-bit quantized vectors
    ///
    /// Uses Manhattan distance on nibble values for speed.
    #[inline]
    pub fn quantized_distance(&self, a: &[u8], b: &[u8]) -> f32 {
        assert_eq!(a.len(), b.len(), "Vector size mismatch");

        let mut distance = 0.0f32;
        for i in 0..self.dimensions {
            let byte_idx = i / 2;
            let nibble_a = if i % 2 == 0 {
                a[byte_idx] & 0x0F
            } else {
                (a[byte_idx] >> 4) & 0x0F
            };
            let nibble_b = if i % 2 == 0 {
                b[byte_idx] & 0x0F
            } else {
                (b[byte_idx] >> 4) & 0x0F
            };

            distance += (nibble_a as i32 - nibble_b as i32).abs() as f32;
        }

        distance
    }

    /// Get compression ratio
    pub fn compression_ratio(&self) -> f32 {
        // float32 (4 bytes) → 4-bit (0.5 byte) = 8x compression
        8.0
    }

    /// Get memory savings percentage
    pub fn memory_savings(&self) -> f32 {
        // 87.5% memory savings (1/8 of original size)
        0.875
    }

    /// Check if quantizer is fitted
    pub fn is_fitted(&self) -> bool {
        self.is_fitted
    }

    /// Get number of dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

impl Default for FourBitQuantizer {
    fn default() -> Self {
        Self::new()
    }
}

/// 4-bit quantized vector index for balanced memory efficiency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FourBitQuantizedIndex {
    quantizer: FourBitQuantizer,
    /// 4-bit quantized vectors
    quantized_vectors: Vec<Vec<u8>>,
    /// Entity IDs
    entity_ids: Vec<String>,
}

impl FourBitQuantizedIndex {
    /// Create a new 4-bit quantized index
    pub fn new() -> Self {
        Self {
            quantizer: FourBitQuantizer::new(),
            quantized_vectors: Vec::new(),
            entity_ids: Vec::new(),
        }
    }

    /// Build index from float32 vectors
    pub fn build(&mut self, vectors: &[(String, Vec<f32>)]) -> Result<()> {
        if vectors.is_empty() {
            return Err(anyhow!("Cannot build index from empty vectors"));
        }

        // Extract float vectors for fitting
        let float_vecs: Vec<Vec<f32>> = vectors.iter().map(|(_, v)| v.clone()).collect();

        // Fit quantizer
        self.quantizer.fit(&float_vecs)?;

        // Quantize all vectors
        self.entity_ids = vectors.iter().map(|(id, _)| id.clone()).collect();
        self.quantized_vectors = self.quantizer.quantize_batch(&float_vecs);

        Ok(())
    }

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>> {
        if !self.quantizer.is_fitted() {
            return Err(anyhow!("Index not built"));
        }

        // Quantize query
        let quantized_query = self.quantizer.quantize(query);

        // Compute distances to all vectors
        let mut distances: Vec<(usize, f32)> = self
            .quantized_vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (i, self.quantizer.quantized_distance(&quantized_query, v)))
            .collect();

        // Sort by distance (ascending - lower is better)
        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top-k
        Ok(distances
            .iter()
            .take(k.min(self.entity_ids.len()))
            .map(|(idx, dist)| (self.entity_ids[*idx].clone(), *dist))
            .collect())
    }

    /// Get index statistics
    pub fn stats(&self) -> FourBitQuantizedIndexStats {
        let num_vectors = self.quantized_vectors.len();
        let dimensions = self.quantizer.dimensions();
        let original_bytes = num_vectors * dimensions * 4; // float32
        let quantized_bytes = num_vectors * dimensions.div_ceil(2); // 4-bit (2 per byte)

        FourBitQuantizedIndexStats {
            num_vectors,
            dimensions,
            compression_ratio: self.quantizer.compression_ratio(),
            memory_savings: self.quantizer.memory_savings(),
            original_bytes,
            quantized_bytes,
        }
    }
}

impl Default for FourBitQuantizedIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for 4-bit quantized index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FourBitQuantizedIndexStats {
    pub num_vectors: usize,
    pub dimensions: usize,
    pub compression_ratio: f32,
    pub memory_savings: f32,
    pub original_bytes: usize,
    pub quantized_bytes: usize,
}

// ============================================================================
// FP16 (Half-Precision Float) Quantization
// ============================================================================

#[cfg(feature = "fp16")]
use half::f16;

/// FP16 quantizer for high-accuracy memory reduction (2x compression)
///
/// Converts float32 vectors to float16 (16-bit IEEE 754 half-precision).
/// Provides 2x memory reduction with minimal accuracy loss.
///
/// **When to use FP16:**
/// - Need minimal accuracy loss (< 0.1% recall degradation)
/// - Have modern hardware with FP16 support
/// - Want simple conversion without fitting/calibration
/// - Prefer direct float operations over quantized integer math
#[cfg(feature = "fp16")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fp16Quantizer {
    /// Number of dimensions
    dimensions: usize,
}

#[cfg(feature = "fp16")]
impl Fp16Quantizer {
    /// Create a new FP16 quantizer
    pub fn new() -> Self {
        Self { dimensions: 0 }
    }

    /// Set dimensions (no fitting required for FP16)
    pub fn set_dimensions(&mut self, dimensions: usize) {
        self.dimensions = dimensions;
    }

    /// Convert float32 vector to float16 (packed as u16 array)
    ///
    /// Each f32 is converted to f16 and stored as u16 bits.
    pub fn quantize(&self, vector: &[f32]) -> Vec<u16> {
        // If dimensions not set, allow any size; otherwise verify match
        if self.dimensions > 0 {
            assert_eq!(vector.len(), self.dimensions, "Vector dimension mismatch");
        }

        vector
            .iter()
            .map(|&val| f16::from_f32(val).to_bits())
            .collect()
    }

    /// Convert float16 (u16 bits) back to float32
    pub fn dequantize(&self, quantized: &[u16]) -> Vec<f32> {
        // If dimensions set, verify match; otherwise allow any size
        if self.dimensions > 0 {
            assert_eq!(
                quantized.len(),
                self.dimensions,
                "Quantized vector dimension mismatch"
            );
        }

        quantized
            .iter()
            .map(|&bits| f16::from_bits(bits).to_f32())
            .collect()
    }

    /// Quantize multiple vectors
    pub fn quantize_batch(&self, vectors: &[Vec<f32>]) -> Vec<Vec<u16>> {
        vectors.iter().map(|v| self.quantize(v)).collect()
    }

    /// Dequantize multiple vectors
    pub fn dequantize_batch(&self, quantized: &[Vec<u16>]) -> Vec<Vec<f32>> {
        quantized.iter().map(|v| self.dequantize(v)).collect()
    }

    /// Compute distance directly on FP16 values (after conversion to f32)
    ///
    /// This converts back to f32 for computation. For production use,
    /// consider using SIMD FP16 operations on supported hardware.
    #[inline]
    pub fn fp16_distance(&self, a: &[u16], b: &[u16]) -> f32 {
        assert_eq!(a.len(), b.len(), "Vector dimension mismatch");

        let mut distance = 0.0f32;
        for (&a_bits, &b_bits) in a.iter().zip(b.iter()) {
            let a_val = f16::from_bits(a_bits).to_f32();
            let b_val = f16::from_bits(b_bits).to_f32();
            let diff = a_val - b_val;
            distance += diff * diff;
        }

        distance.sqrt()
    }

    /// Get compression ratio
    pub fn compression_ratio(&self) -> f32 {
        // float32 (4 bytes) → float16 (2 bytes) = 2x compression
        2.0
    }

    /// Get memory savings percentage
    pub fn memory_savings(&self) -> f32 {
        // 50% memory savings (1/2 of original size)
        0.5
    }

    /// Get number of dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(feature = "fp16")]
impl Default for Fp16Quantizer {
    fn default() -> Self {
        Self::new()
    }
}

/// FP16 quantized vector index for high-accuracy memory efficiency
#[cfg(feature = "fp16")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fp16QuantizedIndex {
    quantizer: Fp16Quantizer,
    /// FP16 quantized vectors (stored as u16 bits)
    fp16_vectors: Vec<Vec<u16>>,
    /// Entity IDs
    entity_ids: Vec<String>,
}

#[cfg(feature = "fp16")]
impl Fp16QuantizedIndex {
    /// Create a new FP16 quantized index
    pub fn new() -> Self {
        Self {
            quantizer: Fp16Quantizer::new(),
            fp16_vectors: Vec::new(),
            entity_ids: Vec::new(),
        }
    }

    /// Build index from float32 vectors
    pub fn build(&mut self, vectors: &[(String, Vec<f32>)]) -> Result<()> {
        if vectors.is_empty() {
            return Err(anyhow!("Cannot build index from empty vectors"));
        }

        // Set dimensions from first vector
        let dimensions = vectors[0].1.len();
        self.quantizer.set_dimensions(dimensions);

        // Quantize all vectors
        self.entity_ids = vectors.iter().map(|(id, _)| id.clone()).collect();
        let float_vecs: Vec<Vec<f32>> = vectors.iter().map(|(_, v)| v.clone()).collect();
        self.fp16_vectors = self.quantizer.quantize_batch(&float_vecs);

        Ok(())
    }

    /// Search for k nearest neighbors using Euclidean distance
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>> {
        if self.fp16_vectors.is_empty() {
            return Err(anyhow!("Index not built"));
        }

        // Quantize query
        let fp16_query = self.quantizer.quantize(query);

        // Compute distances to all vectors
        let mut distances: Vec<(usize, f32)> = self
            .fp16_vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (i, self.quantizer.fp16_distance(&fp16_query, v)))
            .collect();

        // Sort by distance (ascending - lower is better)
        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top-k
        Ok(distances
            .iter()
            .take(k.min(self.entity_ids.len()))
            .map(|(idx, dist)| (self.entity_ids[*idx].clone(), *dist))
            .collect())
    }

    /// Get index statistics
    pub fn stats(&self) -> Fp16QuantizedIndexStats {
        let num_vectors = self.fp16_vectors.len();
        let dimensions = self.quantizer.dimensions();
        let original_bytes = num_vectors * dimensions * 4; // float32
        let fp16_bytes = num_vectors * dimensions * 2; // float16

        Fp16QuantizedIndexStats {
            num_vectors,
            dimensions,
            compression_ratio: self.quantizer.compression_ratio(),
            memory_savings: self.quantizer.memory_savings(),
            original_bytes,
            fp16_bytes,
        }
    }
}

#[cfg(feature = "fp16")]
impl Default for Fp16QuantizedIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for FP16 quantized index
#[cfg(feature = "fp16")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fp16QuantizedIndexStats {
    pub num_vectors: usize,
    pub dimensions: usize,
    pub compression_ratio: f32,
    pub memory_savings: f32,
    pub original_bytes: usize,
    pub fp16_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantizer_fit() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = ScalarQuantizer::new(QuantizationConfig::default());
        assert!(quantizer.fit(&vectors).is_ok());
        assert!(quantizer.is_fitted());
        assert_eq!(quantizer.dimensions(), 3);
    }

    #[test]
    fn test_quantize_dequantize() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = ScalarQuantizer::new(QuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let vector = vec![1.0, 2.0, 3.0];
        let quantized = quantizer.quantize(&vector);
        let dequantized = quantizer.dequantize(&quantized);

        // Check dimensions
        assert_eq!(quantized.len(), 3);
        assert_eq!(dequantized.len(), 3);

        // Check approximate reconstruction
        for (orig, deq) in vector.iter().zip(dequantized.iter()) {
            assert!((orig - deq).abs() < 0.1); // Small error due to quantization
        }
    }

    #[test]
    fn test_quantize_batch() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = ScalarQuantizer::new(QuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let quantized = quantizer.quantize_batch(&vectors);
        assert_eq!(quantized.len(), 3);
        assert_eq!(quantized[0].len(), 3);
    }

    #[test]
    fn test_quantized_distance() {
        let vectors = vec![
            vec![0.0, 0.0, 0.0],
            vec![1.0, 1.0, 1.0],
            vec![2.0, 2.0, 2.0],
        ];

        let mut quantizer = ScalarQuantizer::new(QuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let a = quantizer.quantize(&vectors[0]);
        let b = quantizer.quantize(&vectors[1]);
        let c = quantizer.quantize(&vectors[2]);

        let dist_ab = quantizer.quantized_distance(&a, &b);
        let dist_ac = quantizer.quantized_distance(&a, &c);

        // Distance to farther vector should be larger
        assert!(dist_ac > dist_ab);
    }

    #[test]
    fn test_compression_ratio() {
        let quantizer = ScalarQuantizer::new(QuantizationConfig::default());
        assert_eq!(quantizer.compression_ratio(), 4.0);
        assert_eq!(quantizer.memory_savings(), 0.75);
    }

    #[test]
    fn test_quantized_index_build() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0, 1.0, 2.0]),
            ("doc2".to_string(), vec![1.0, 2.0, 3.0]),
            ("doc3".to_string(), vec![2.0, 3.0, 4.0]),
        ];

        let mut index = QuantizedVectorIndex::new(QuantizationConfig::default());
        assert!(index.build(&vectors).is_ok());
    }

    #[test]
    fn test_quantized_index_search() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0, 0.0, 0.0]),
            ("doc2".to_string(), vec![1.0, 1.0, 1.0]),
            ("doc3".to_string(), vec![2.0, 2.0, 2.0]),
        ];

        let mut index = QuantizedVectorIndex::new(QuantizationConfig::default());
        index.build(&vectors).unwrap();

        // Search for nearest to doc2
        let query = vec![1.0, 1.0, 1.0];
        let results = index.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "doc2"); // Closest should be doc2
    }

    #[test]
    fn test_quantized_index_stats() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0; 768]),
            ("doc2".to_string(), vec![1.0; 768]),
            ("doc3".to_string(), vec![2.0; 768]),
        ];

        let mut index = QuantizedVectorIndex::new(QuantizationConfig::default());
        index.build(&vectors).unwrap();

        let stats = index.stats();
        assert_eq!(stats.num_vectors, 3);
        assert_eq!(stats.dimensions, 768);
        assert_eq!(stats.original_bytes, 3 * 768 * 4); // 3 vectors * 768 dims * 4 bytes
        assert_eq!(stats.quantized_bytes, 3 * 768); // 3 vectors * 768 dims * 1 byte
        assert_eq!(stats.compression_ratio, 4.0);
    }

    #[test]
    fn test_fit_empty_vectors() {
        let vectors: Vec<Vec<f32>> = vec![];
        let mut quantizer = ScalarQuantizer::new(QuantizationConfig::default());
        assert!(quantizer.fit(&vectors).is_err());
    }

    #[test]
    #[should_panic(expected = "Quantizer must be fitted")]
    fn test_quantize_unfitted() {
        let quantizer = ScalarQuantizer::new(QuantizationConfig::default());
        quantizer.quantize(&[1.0, 2.0, 3.0]);
    }

    #[test]
    #[should_panic(expected = "Vector dimension mismatch")]
    fn test_quantize_dimension_mismatch() {
        let vectors = vec![vec![0.0, 1.0, 2.0]];
        let mut quantizer = ScalarQuantizer::new(QuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        // Try to quantize vector with wrong dimension
        quantizer.quantize(&[1.0, 2.0]); // 2D instead of 3D
    }

    // ========================================================================
    // Binary Quantization Tests
    // ========================================================================

    #[test]
    fn test_binary_quantizer_fit() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        assert!(quantizer.fit(&vectors).is_ok());
        assert!(quantizer.is_fitted());
        assert_eq!(quantizer.dimensions(), 3);
    }

    #[test]
    fn test_binary_quantize_dequantize() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let vector = vec![1.0, 2.0, 3.0];
        let binary = quantizer.quantize(&vector);
        let dequantized = quantizer.dequantize(&binary);

        // Check dimensions
        assert_eq!(binary.len(), 1); // 3 bits fit in 1 byte
        assert_eq!(dequantized.len(), 3);
    }

    #[test]
    fn test_binary_quantize_large_vector() {
        // Test with vector larger than 8 dimensions (multiple bytes)
        let vectors: Vec<Vec<f32>> = (0..10)
            .map(|_| (0..128).map(|i| i as f32).collect())
            .collect();

        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let vector: Vec<f32> = (0..128).map(|i| i as f32).collect();
        let binary = quantizer.quantize(&vector);

        // 128 bits = 16 bytes
        assert_eq!(binary.len(), 16);
    }

    #[test]
    fn test_binary_hamming_distance() {
        let vectors = vec![
            vec![0.0, 0.0, 0.0, 0.0],
            vec![1.0, 1.0, 1.0, 1.0],
            vec![2.0, 2.0, 2.0, 2.0],
        ];

        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let a = quantizer.quantize(&vectors[0]);
        let b = quantizer.quantize(&vectors[1]);
        let c = quantizer.quantize(&vectors[2]);

        let dist_ab = quantizer.hamming_distance(&a, &b);
        let dist_ac = quantizer.hamming_distance(&a, &c);

        // Check that distances make sense
        assert!(dist_ab <= 4); // Max 4 bits can differ
        assert!(dist_ac <= 4);
    }

    #[test]
    fn test_binary_hamming_similarity() {
        let vectors = vec![
            vec![0.0, 0.0, 0.0, 0.0],
            vec![1.0, 1.0, 1.0, 1.0],
            vec![2.0, 2.0, 2.0, 2.0],
        ];

        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let a = quantizer.quantize(&vectors[0]);
        let b = quantizer.quantize(&vectors[1]);

        let sim = quantizer.hamming_similarity(&a, &b);

        // Similarity should be between 0.0 and 1.0
        assert!((0.0..=1.0).contains(&sim));
    }

    #[test]
    fn test_binary_compression_ratio() {
        let quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        assert_eq!(quantizer.compression_ratio(), 32.0);
        assert_eq!(quantizer.memory_savings(), 0.96875);
    }

    #[test]
    fn test_binary_quantize_batch() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let binary = quantizer.quantize_batch(&vectors);
        assert_eq!(binary.len(), 3);
        assert_eq!(binary[0].len(), 1); // 3 bits fit in 1 byte
    }

    #[test]
    fn test_binary_quantized_index_build() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0, 1.0, 2.0]),
            ("doc2".to_string(), vec![1.0, 2.0, 3.0]),
            ("doc3".to_string(), vec![2.0, 3.0, 4.0]),
        ];

        let mut index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
        assert!(index.build(&vectors).is_ok());
    }

    #[test]
    fn test_binary_quantized_index_search() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0, 0.0, 0.0]),
            ("doc2".to_string(), vec![1.0, 1.0, 1.0]),
            ("doc3".to_string(), vec![2.0, 2.0, 2.0]),
        ];

        let mut index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
        index.build(&vectors).unwrap();

        // Search for nearest to doc2
        let query = vec![1.0, 1.0, 1.0];
        let results = index.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        // Results should be sorted by similarity (descending)
        assert!(results[0].1 >= results[1].1);
    }

    #[test]
    fn test_binary_quantized_index_stats() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0; 768]),
            ("doc2".to_string(), vec![1.0; 768]),
            ("doc3".to_string(), vec![2.0; 768]),
        ];

        let mut index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
        index.build(&vectors).unwrap();

        let stats = index.stats();
        assert_eq!(stats.num_vectors, 3);
        assert_eq!(stats.dimensions, 768);
        assert_eq!(stats.original_bytes, 3 * 768 * 4); // 3 vectors * 768 dims * 4 bytes
        assert_eq!(stats.binary_bytes, 3 * 96); // 3 vectors * 96 bytes (768 bits / 8)
        assert_eq!(stats.compression_ratio, 32.0);
    }

    #[test]
    fn test_binary_fit_empty_vectors() {
        let vectors: Vec<Vec<f32>> = vec![];
        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        assert!(quantizer.fit(&vectors).is_err());
    }

    #[test]
    #[should_panic(expected = "Quantizer must be fitted")]
    fn test_binary_quantize_unfitted() {
        let quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        quantizer.quantize(&[1.0, 2.0, 3.0]);
    }

    #[test]
    #[should_panic(expected = "Vector dimension mismatch")]
    fn test_binary_quantize_dimension_mismatch() {
        let vectors = vec![vec![0.0, 1.0, 2.0]];
        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        // Try to quantize vector with wrong dimension
        quantizer.quantize(&[1.0, 2.0]); // 2D instead of 3D
    }

    #[test]
    fn test_binary_zero_threshold() {
        let vectors = vec![vec![-1.0, 0.0, 1.0], vec![-2.0, 0.0, 2.0]];

        // Use zero threshold instead of mean threshold
        let config = BinaryQuantizationConfig {
            use_mean_threshold: false,
        };

        let mut quantizer = BinaryQuantizer::new(config);
        quantizer.fit(&vectors).unwrap();

        let vector = vec![-1.0, 0.0, 1.0]; // Below, equal, above zero
        let binary = quantizer.quantize(&vector);

        // Check bit pattern: -1.0 → 0, 0.0 → 0, 1.0 → 1
        // Bits: 0, 0, 1 (in little-endian bit order within byte)
        // So byte should be: 0b00000100 = 4
        assert_eq!(binary[0] & 0b00000111, 0b00000100);
    }

    #[test]
    fn test_binary_identical_vectors() {
        let vectors = vec![vec![1.0, 2.0, 3.0], vec![1.0, 2.0, 3.0]];

        let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
        quantizer.fit(&vectors).unwrap();

        let a = quantizer.quantize(&vectors[0]);
        let b = quantizer.quantize(&vectors[1]);

        // Identical vectors should have zero Hamming distance
        let dist = quantizer.hamming_distance(&a, &b);
        assert_eq!(dist, 0);

        // And similarity of 1.0
        let sim = quantizer.hamming_similarity(&a, &b);
        assert_eq!(sim, 1.0);
    }

    // ========================================================================
    // 4-bit Quantization Tests
    // ========================================================================

    #[test]
    fn test_fourbit_quantizer_fit() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = FourBitQuantizer::new();
        assert!(quantizer.fit(&vectors).is_ok());
        assert!(quantizer.is_fitted());
        assert_eq!(quantizer.dimensions(), 3);
    }

    #[test]
    fn test_fourbit_quantize_dequantize() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = FourBitQuantizer::new();
        quantizer.fit(&vectors).unwrap();

        let vector = vec![1.0, 2.0, 3.0];
        let quantized = quantizer.quantize(&vector);
        let dequantized = quantizer.dequantize(&quantized);

        // Check dimensions
        assert_eq!(quantized.len(), 2); // 3 values fit in 2 bytes (1.5 bytes, rounded up)
        assert_eq!(dequantized.len(), 3);

        // Check approximate reconstruction (4-bit has 16 levels)
        for (orig, deq) in vector.iter().zip(dequantized.iter()) {
            assert!((orig - deq).abs() < 0.3); // Moderate error due to 4-bit quantization
        }
    }

    #[test]
    fn test_fourbit_quantize_large_vector() {
        // Test with vector larger than 2 dimensions (multiple bytes)
        let vectors: Vec<Vec<f32>> = (0..10)
            .map(|_| (0..100).map(|i| i as f32).collect())
            .collect();

        let mut quantizer = FourBitQuantizer::new();
        quantizer.fit(&vectors).unwrap();

        let vector: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let quantized = quantizer.quantize(&vector);

        // 100 values = 50 bytes (2 values per byte)
        assert_eq!(quantized.len(), 50);
    }

    #[test]
    fn test_fourbit_odd_dimensions() {
        // Test with odd number of dimensions
        let vectors = vec![
            vec![0.0, 1.0, 2.0, 3.0, 4.0], // 5 dimensions
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
        ];

        let mut quantizer = FourBitQuantizer::new();
        quantizer.fit(&vectors).unwrap();

        let vector = vec![1.5, 2.5, 3.5, 4.5, 5.5];
        let quantized = quantizer.quantize(&vector);

        // 5 values need 3 bytes (2.5 bytes, rounded up)
        assert_eq!(quantized.len(), 3);
    }

    #[test]
    fn test_fourbit_nibble_packing() {
        // Use vectors with range to ensure proper scaling
        let vectors = vec![vec![0.0, 0.0], vec![15.0, 15.0]];

        let mut quantizer = FourBitQuantizer::new();
        quantizer.fit(&vectors).unwrap();

        // Test that nibbles are correctly packed
        // First value (0) in low nibble, second value (15) in high nibble
        let vector = vec![0.0, 15.0];
        let quantized = quantizer.quantize(&vector);

        assert_eq!(quantized.len(), 1); // 2 values in 1 byte
                                        // 0 maps to nibble 0 (0b0000), 15 maps to nibble 15 (0b1111)
                                        // Byte: 0b11110000 = 0xF0 = 240
        assert_eq!(quantized[0], 0xF0);
    }

    #[test]
    fn test_fourbit_compression_ratio() {
        let quantizer = FourBitQuantizer::new();
        assert_eq!(quantizer.compression_ratio(), 8.0);
        assert_eq!(quantizer.memory_savings(), 0.875);
    }

    #[test]
    fn test_fourbit_quantize_batch() {
        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let mut quantizer = FourBitQuantizer::new();
        quantizer.fit(&vectors).unwrap();

        let quantized = quantizer.quantize_batch(&vectors);
        assert_eq!(quantized.len(), 3);
        assert_eq!(quantized[0].len(), 2); // 3 values need 2 bytes
    }

    #[test]
    fn test_fourbit_quantized_distance() {
        let vectors = vec![
            vec![0.0, 0.0, 0.0],
            vec![1.0, 1.0, 1.0],
            vec![2.0, 2.0, 2.0],
        ];

        let mut quantizer = FourBitQuantizer::new();
        quantizer.fit(&vectors).unwrap();

        let a = quantizer.quantize(&vectors[0]);
        let b = quantizer.quantize(&vectors[1]);
        let c = quantizer.quantize(&vectors[2]);

        let dist_ab = quantizer.quantized_distance(&a, &b);
        let dist_ac = quantizer.quantized_distance(&a, &c);

        // Distance to farther vector should be larger
        assert!(dist_ac > dist_ab);
    }

    #[test]
    fn test_fourbit_quantized_index_build() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0, 1.0, 2.0]),
            ("doc2".to_string(), vec![1.0, 2.0, 3.0]),
            ("doc3".to_string(), vec![2.0, 3.0, 4.0]),
        ];

        let mut index = FourBitQuantizedIndex::new();
        assert!(index.build(&vectors).is_ok());
    }

    #[test]
    fn test_fourbit_quantized_index_search() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0, 0.0, 0.0]),
            ("doc2".to_string(), vec![1.0, 1.0, 1.0]),
            ("doc3".to_string(), vec![2.0, 2.0, 2.0]),
        ];

        let mut index = FourBitQuantizedIndex::new();
        index.build(&vectors).unwrap();

        // Search for nearest to doc2
        let query = vec![1.0, 1.0, 1.0];
        let results = index.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "doc2"); // Closest should be doc2
    }

    #[test]
    fn test_fourbit_quantized_index_stats() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0; 768]),
            ("doc2".to_string(), vec![1.0; 768]),
            ("doc3".to_string(), vec![2.0; 768]),
        ];

        let mut index = FourBitQuantizedIndex::new();
        index.build(&vectors).unwrap();

        let stats = index.stats();
        assert_eq!(stats.num_vectors, 3);
        assert_eq!(stats.dimensions, 768);
        assert_eq!(stats.original_bytes, 3 * 768 * 4); // 3 vectors * 768 dims * 4 bytes
        assert_eq!(stats.quantized_bytes, 3 * 384); // 3 vectors * 384 bytes (768 / 2)
        assert_eq!(stats.compression_ratio, 8.0);
    }

    #[test]
    fn test_fourbit_fit_empty_vectors() {
        let vectors: Vec<Vec<f32>> = vec![];
        let mut quantizer = FourBitQuantizer::new();
        assert!(quantizer.fit(&vectors).is_err());
    }

    #[test]
    #[should_panic(expected = "Quantizer must be fitted")]
    fn test_fourbit_quantize_unfitted() {
        let quantizer = FourBitQuantizer::new();
        quantizer.quantize(&[1.0, 2.0, 3.0]);
    }

    #[test]
    #[should_panic(expected = "Vector dimension mismatch")]
    fn test_fourbit_quantize_dimension_mismatch() {
        let vectors = vec![vec![0.0, 1.0, 2.0]];
        let mut quantizer = FourBitQuantizer::new();
        quantizer.fit(&vectors).unwrap();

        // Try to quantize vector with wrong dimension
        quantizer.quantize(&[1.0, 2.0]); // 2D instead of 3D
    }

    // ========================================================================
    // FP16 (Half-Precision Float) Tests
    // ========================================================================

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_quantizer_basic() {
        let mut quantizer = Fp16Quantizer::new();
        quantizer.set_dimensions(3);
        assert_eq!(quantizer.dimensions(), 3);
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_quantize_dequantize() {
        let quantizer = Fp16Quantizer::new();

        let vector = vec![1.0, 2.0, 3.0];
        let quantized = quantizer.quantize(&vector);
        let dequantized = quantizer.dequantize(&quantized);

        // Check dimensions
        assert_eq!(quantized.len(), 3);
        assert_eq!(dequantized.len(), 3);

        // Check high-precision reconstruction (FP16 is very accurate)
        for (orig, deq) in vector.iter().zip(dequantized.iter()) {
            assert!((orig - deq).abs() < 0.001); // Very small error due to FP16 precision
        }
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_quantize_large_vector() {
        let quantizer = Fp16Quantizer::new();

        let vector: Vec<f32> = (0..768).map(|i| i as f32 * 0.1).collect();
        let quantized = quantizer.quantize(&vector);

        // 768 f32 values → 768 u16 values
        assert_eq!(quantized.len(), 768);
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_quantize_batch() {
        let quantizer = Fp16Quantizer::new();

        let vectors = vec![
            vec![0.0, 1.0, 2.0],
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 4.0],
        ];

        let quantized = quantizer.quantize_batch(&vectors);
        assert_eq!(quantized.len(), 3);
        assert_eq!(quantized[0].len(), 3);
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_distance() {
        let quantizer = Fp16Quantizer::new();

        let v0 = vec![0.0, 0.0, 0.0];
        let v1 = vec![1.0, 1.0, 1.0];
        let v2 = vec![2.0, 2.0, 2.0];

        let a = quantizer.quantize(&v0);
        let b = quantizer.quantize(&v1);
        let c = quantizer.quantize(&v2);

        let dist_ab = quantizer.fp16_distance(&a, &b);
        let dist_ac = quantizer.fp16_distance(&a, &c);

        // Distance to farther vector should be larger
        assert!(dist_ac > dist_ab);
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_compression_ratio() {
        let quantizer = Fp16Quantizer::new();
        assert_eq!(quantizer.compression_ratio(), 2.0);
        assert_eq!(quantizer.memory_savings(), 0.5);
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_quantized_index_build() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0, 1.0, 2.0]),
            ("doc2".to_string(), vec![1.0, 2.0, 3.0]),
            ("doc3".to_string(), vec![2.0, 3.0, 4.0]),
        ];

        let mut index = Fp16QuantizedIndex::new();
        assert!(index.build(&vectors).is_ok());
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_quantized_index_search() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0, 0.0, 0.0]),
            ("doc2".to_string(), vec![1.0, 1.0, 1.0]),
            ("doc3".to_string(), vec![2.0, 2.0, 2.0]),
        ];

        let mut index = Fp16QuantizedIndex::new();
        index.build(&vectors).unwrap();

        // Search for nearest to doc2
        let query = vec![1.0, 1.0, 1.0];
        let results = index.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "doc2"); // Closest should be doc2
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_quantized_index_stats() {
        let vectors = vec![
            ("doc1".to_string(), vec![0.0; 768]),
            ("doc2".to_string(), vec![1.0; 768]),
            ("doc3".to_string(), vec![2.0; 768]),
        ];

        let mut index = Fp16QuantizedIndex::new();
        index.build(&vectors).unwrap();

        let stats = index.stats();
        assert_eq!(stats.num_vectors, 3);
        assert_eq!(stats.dimensions, 768);
        assert_eq!(stats.original_bytes, 3 * 768 * 4); // 3 vectors * 768 dims * 4 bytes
        assert_eq!(stats.fp16_bytes, 3 * 768 * 2); // 3 vectors * 768 dims * 2 bytes
        assert_eq!(stats.compression_ratio, 2.0);
    }

    #[test]
    #[cfg(feature = "fp16")]
    fn test_fp16_high_precision() {
        let quantizer = Fp16Quantizer::new();

        // Test various values to ensure FP16 maintains good precision
        let test_values = vec![
            vec![0.1, 0.2, 0.3],
            vec![1.5, 2.5, 3.5],
            vec![100.0, 200.0, 300.0],
            vec![-1.0, -2.0, -3.0],
        ];

        for vector in &test_values {
            let quantized = quantizer.quantize(vector);
            let dequantized = quantizer.dequantize(&quantized);

            for (orig, deq) in vector.iter().zip(dequantized.iter()) {
                // FP16 should maintain <0.1% relative error for most values
                let relative_error = ((orig - deq) / orig).abs();
                assert!(relative_error < 0.001 || orig.abs() < 0.01);
            }
        }
    }

    #[test]
    #[cfg(feature = "fp16")]
    #[should_panic(expected = "Vector dimension mismatch")]
    fn test_fp16_quantize_dimension_mismatch() {
        let mut quantizer = Fp16Quantizer::new();
        quantizer.set_dimensions(3); // Explicitly set dimensions to 3

        let vector1 = vec![1.0, 2.0, 3.0];
        quantizer.quantize(&vector1); // Should work fine

        // Try to quantize vector with wrong dimension
        quantizer.quantize(&[1.0, 2.0]); // 2D instead of 3D - should panic
    }
}
