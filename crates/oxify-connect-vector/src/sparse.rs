//! Sparse vector support for efficient high-dimensional data
//!
//! This module provides utilities for working with sparse vectors, which are
//! commonly used in NLP (TF-IDF, bag-of-words) and other domains where most
//! vector elements are zero.
//!
//! # Examples
//!
//! ```
//! use oxify_connect_vector::sparse::{SparseVector, sparse_cosine_similarity};
//!
//! // Create sparse vectors (index, value pairs)
//! let v1 = SparseVector::new(vec![(0, 1.0), (2, 3.0), (5, 2.0)], 10);
//! let v2 = SparseVector::new(vec![(2, 2.0), (3, 1.0), (5, 1.0)], 10);
//!
//! // Compute similarity (only processes non-zero elements)
//! let similarity = sparse_cosine_similarity(&v1, &v2);
//! ```

/// Sparse vector representation using coordinate format (COO)
///
/// Stores only non-zero elements as (index, value) pairs for memory efficiency.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseVector {
    /// Non-zero elements as (index, value) pairs, sorted by index
    pub elements: Vec<(usize, f32)>,
    /// Dimensionality of the vector
    pub dimension: usize,
}

impl SparseVector {
    /// Create a new sparse vector from (index, value) pairs
    ///
    /// # Arguments
    /// * `elements` - Vector of (index, value) pairs
    /// * `dimension` - Total dimensionality of the vector
    ///
    /// # Examples
    /// ```
    /// use oxify_connect_vector::sparse::SparseVector;
    ///
    /// let sparse = SparseVector::new(vec![(0, 1.0), (2, 3.0), (5, 2.0)], 10);
    /// assert_eq!(sparse.dimension, 10);
    /// assert_eq!(sparse.elements.len(), 3);
    /// ```
    pub fn new(mut elements: Vec<(usize, f32)>, dimension: usize) -> Self {
        // Remove zeros and sort by index
        elements.retain(|(_, v)| *v != 0.0);
        elements.sort_by_key(|(idx, _)| *idx);

        // Deduplicate by summing values at same index
        let mut deduped = Vec::new();
        for (idx, val) in elements {
            if let Some((last_idx, last_val)) = deduped.last_mut() {
                if *last_idx == idx {
                    *last_val += val;
                    continue;
                }
            }
            deduped.push((idx, val));
        }

        Self {
            elements: deduped,
            dimension,
        }
    }

    /// Create a sparse vector from a dense vector
    ///
    /// Only stores non-zero elements.
    ///
    /// # Examples
    /// ```
    /// use oxify_connect_vector::sparse::SparseVector;
    ///
    /// let dense = vec![1.0, 0.0, 3.0, 0.0, 0.0, 2.0];
    /// let sparse = SparseVector::from_dense(&dense);
    /// assert_eq!(sparse.elements.len(), 3); // Only 3 non-zero elements
    /// ```
    pub fn from_dense(dense: &[f32]) -> Self {
        let elements: Vec<(usize, f32)> = dense
            .iter()
            .enumerate()
            .filter(|(_, &v)| v != 0.0)
            .map(|(i, &v)| (i, v))
            .collect();

        Self {
            elements,
            dimension: dense.len(),
        }
    }

    /// Convert sparse vector to dense representation
    ///
    /// # Examples
    /// ```
    /// use oxify_connect_vector::sparse::SparseVector;
    ///
    /// let sparse = SparseVector::new(vec![(0, 1.0), (2, 3.0)], 5);
    /// let dense = sparse.to_dense();
    /// assert_eq!(dense, vec![1.0, 0.0, 3.0, 0.0, 0.0]);
    /// ```
    pub fn to_dense(&self) -> Vec<f32> {
        let mut dense = vec![0.0; self.dimension];
        for &(idx, val) in &self.elements {
            if idx < self.dimension {
                dense[idx] = val;
            }
        }
        dense
    }

    /// Get the number of non-zero elements
    pub fn nnz(&self) -> usize {
        self.elements.len()
    }

    /// Get the sparsity ratio (proportion of zero elements)
    ///
    /// Returns a value between 0.0 (dense) and 1.0 (all zeros).
    pub fn sparsity(&self) -> f64 {
        if self.dimension == 0 {
            return 0.0;
        }
        1.0 - (self.elements.len() as f64 / self.dimension as f64)
    }

    /// Compute the L2 norm (Euclidean norm) of the sparse vector
    pub fn norm(&self) -> f32 {
        self.elements.iter().map(|(_, v)| v * v).sum::<f32>().sqrt()
    }

    /// Normalize the sparse vector to unit length
    pub fn normalize(&mut self) {
        let norm = self.norm();
        if norm > 0.0 {
            for (_, v) in &mut self.elements {
                *v /= norm;
            }
        }
    }

    /// Get value at specific index (0.0 if not present)
    pub fn get(&self, index: usize) -> f32 {
        self.elements
            .binary_search_by_key(&index, |(idx, _)| *idx)
            .ok()
            .map(|pos| self.elements[pos].1)
            .unwrap_or(0.0)
    }
}

/// Compute dot product between two sparse vectors
///
/// Only processes non-zero elements, making it efficient for high-dimensional sparse data.
///
/// # Examples
/// ```
/// use oxify_connect_vector::sparse::{SparseVector, sparse_dot_product};
///
/// let v1 = SparseVector::new(vec![(0, 1.0), (2, 3.0), (5, 2.0)], 10);
/// let v2 = SparseVector::new(vec![(2, 2.0), (3, 1.0), (5, 1.0)], 10);
/// let dot = sparse_dot_product(&v1, &v2);
/// assert!((dot - 8.0).abs() < 1e-6); // 3.0*2.0 + 2.0*1.0 = 8.0
/// ```
pub fn sparse_dot_product(a: &SparseVector, b: &SparseVector) -> f64 {
    let mut result = 0.0;
    let mut i = 0;
    let mut j = 0;

    // Merge the two sorted lists
    while i < a.elements.len() && j < b.elements.len() {
        let (idx_a, val_a) = a.elements[i];
        let (idx_b, val_b) = b.elements[j];

        if idx_a == idx_b {
            result += (val_a * val_b) as f64;
            i += 1;
            j += 1;
        } else if idx_a < idx_b {
            i += 1;
        } else {
            j += 1;
        }
    }

    result
}

/// Compute cosine similarity between two sparse vectors
///
/// # Examples
/// ```
/// use oxify_connect_vector::sparse::{SparseVector, sparse_cosine_similarity};
///
/// let v1 = SparseVector::new(vec![(0, 1.0), (1, 0.0), (2, 0.0)], 3);
/// let v2 = SparseVector::new(vec![(0, 1.0), (1, 0.0), (2, 0.0)], 3);
/// let similarity = sparse_cosine_similarity(&v1, &v2);
/// assert!((similarity - 1.0).abs() < 1e-6);
/// ```
pub fn sparse_cosine_similarity(a: &SparseVector, b: &SparseVector) -> f64 {
    let dot = sparse_dot_product(a, b);
    let norm_a = a.norm() as f64;
    let norm_b = b.norm() as f64;

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Compute Euclidean distance between two sparse vectors
///
/// # Examples
/// ```
/// use oxify_connect_vector::sparse::{SparseVector, sparse_euclidean_distance};
///
/// let v1 = SparseVector::new(vec![(0, 0.0), (1, 0.0), (2, 0.0)], 3);
/// let v2 = SparseVector::new(vec![(0, 1.0), (1, 0.0), (2, 0.0)], 3);
/// let distance = sparse_euclidean_distance(&v1, &v2);
/// assert!((distance - 1.0).abs() < 1e-6);
/// ```
pub fn sparse_euclidean_distance(a: &SparseVector, b: &SparseVector) -> f64 {
    // For sparse vectors: ||a - b||^2 = ||a||^2 + ||b||^2 - 2*dot(a,b)
    let norm_a_sq = a.elements.iter().map(|(_, v)| (v * v) as f64).sum::<f64>();
    let norm_b_sq = b.elements.iter().map(|(_, v)| (v * v) as f64).sum::<f64>();
    let dot = sparse_dot_product(a, b);

    (norm_a_sq + norm_b_sq - 2.0 * dot).max(0.0).sqrt()
}

/// Compute Jaccard similarity between two sparse vectors (treating as sets)
///
/// Jaccard similarity = |A ∩ B| / |A ∪ B| where we consider indices with non-zero values.
///
/// # Examples
/// ```
/// use oxify_connect_vector::sparse::{SparseVector, sparse_jaccard_similarity};
///
/// let v1 = SparseVector::new(vec![(0, 1.0), (2, 1.0), (5, 1.0)], 10);
/// let v2 = SparseVector::new(vec![(2, 1.0), (3, 1.0), (5, 1.0)], 10);
/// let jaccard = sparse_jaccard_similarity(&v1, &v2);
/// // Intersection: {2, 5} = 2 elements
/// // Union: {0, 2, 3, 5} = 4 elements
/// // Jaccard = 2/4 = 0.5
/// assert!((jaccard - 0.5).abs() < 1e-6);
/// ```
pub fn sparse_jaccard_similarity(a: &SparseVector, b: &SparseVector) -> f64 {
    let indices_a: std::collections::HashSet<_> = a.elements.iter().map(|(idx, _)| idx).collect();
    let indices_b: std::collections::HashSet<_> = b.elements.iter().map(|(idx, _)| idx).collect();

    let intersection = indices_a.intersection(&indices_b).count();
    let union = indices_a.union(&indices_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Convert a dense vector to sparse format with a threshold
///
/// Only keeps elements with absolute value >= threshold.
///
/// # Examples
/// ```
/// use oxify_connect_vector::sparse::densify_with_threshold;
///
/// let dense = vec![0.1, 0.001, 3.0, 0.002, 2.0];
/// let sparse = densify_with_threshold(&dense, 0.01);
/// assert_eq!(sparse.elements.len(), 3); // Only values >= 0.01
/// ```
pub fn densify_with_threshold(dense: &[f32], threshold: f32) -> SparseVector {
    let elements: Vec<(usize, f32)> = dense
        .iter()
        .enumerate()
        .filter(|(_, &v)| v.abs() >= threshold)
        .map(|(i, &v)| (i, v))
        .collect();

    SparseVector {
        elements,
        dimension: dense.len(),
    }
}

/// Batch convert dense vectors to sparse format
pub fn batch_to_sparse(dense_vectors: &[Vec<f32>]) -> Vec<SparseVector> {
    dense_vectors
        .iter()
        .map(|v| SparseVector::from_dense(v))
        .collect()
}

/// Batch convert sparse vectors to dense format
pub fn batch_to_dense(sparse_vectors: &[SparseVector]) -> Vec<Vec<f32>> {
    sparse_vectors.iter().map(|v| v.to_dense()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparse_vector_creation() {
        let sparse = SparseVector::new(vec![(0, 1.0), (2, 3.0), (5, 2.0)], 10);
        assert_eq!(sparse.dimension, 10);
        assert_eq!(sparse.elements.len(), 3);
        assert_eq!(sparse.elements[0], (0, 1.0));
        assert_eq!(sparse.elements[1], (2, 3.0));
        assert_eq!(sparse.elements[2], (5, 2.0));
    }

    #[test]
    fn test_sparse_vector_from_dense() {
        let dense = vec![1.0, 0.0, 3.0, 0.0, 0.0, 2.0];
        let sparse = SparseVector::from_dense(&dense);
        assert_eq!(sparse.dimension, 6);
        assert_eq!(sparse.elements.len(), 3);
        assert_eq!(sparse.elements[0], (0, 1.0));
        assert_eq!(sparse.elements[1], (2, 3.0));
        assert_eq!(sparse.elements[2], (5, 2.0));
    }

    #[test]
    fn test_sparse_vector_to_dense() {
        let sparse = SparseVector::new(vec![(0, 1.0), (2, 3.0)], 5);
        let dense = sparse.to_dense();
        assert_eq!(dense, vec![1.0, 0.0, 3.0, 0.0, 0.0]);
    }

    #[test]
    fn test_sparse_vector_nnz() {
        let sparse = SparseVector::new(vec![(0, 1.0), (2, 3.0), (5, 2.0)], 10);
        assert_eq!(sparse.nnz(), 3);
    }

    #[test]
    fn test_sparse_vector_sparsity() {
        let sparse = SparseVector::new(vec![(0, 1.0), (2, 3.0)], 10);
        assert!((sparse.sparsity() - 0.8).abs() < 1e-6); // 8/10 = 0.8
    }

    #[test]
    fn test_sparse_vector_norm() {
        let sparse = SparseVector::new(vec![(0, 3.0), (1, 4.0)], 5);
        assert!((sparse.norm() - 5.0).abs() < 1e-6); // sqrt(9 + 16) = 5
    }

    #[test]
    fn test_sparse_vector_normalize() {
        let mut sparse = SparseVector::new(vec![(0, 3.0), (1, 4.0)], 5);
        sparse.normalize();
        assert!((sparse.norm() - 1.0).abs() < 1e-6);
        assert!((sparse.elements[0].1 - 0.6).abs() < 1e-6);
        assert!((sparse.elements[1].1 - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_sparse_vector_get() {
        let sparse = SparseVector::new(vec![(0, 1.0), (2, 3.0), (5, 2.0)], 10);
        assert_eq!(sparse.get(0), 1.0);
        assert_eq!(sparse.get(2), 3.0);
        assert_eq!(sparse.get(5), 2.0);
        assert_eq!(sparse.get(1), 0.0);
        assert_eq!(sparse.get(3), 0.0);
    }

    #[test]
    fn test_sparse_dot_product() {
        let v1 = SparseVector::new(vec![(0, 1.0), (2, 3.0), (5, 2.0)], 10);
        let v2 = SparseVector::new(vec![(2, 2.0), (3, 1.0), (5, 1.0)], 10);
        let dot = sparse_dot_product(&v1, &v2);
        assert!((dot - 8.0).abs() < 1e-6); // 3.0*2.0 + 2.0*1.0 = 8.0
    }

    #[test]
    fn test_sparse_cosine_similarity() {
        let v1 = SparseVector::new(vec![(0, 1.0), (1, 0.0), (2, 0.0)], 3);
        let v2 = SparseVector::new(vec![(0, 1.0), (1, 0.0), (2, 0.0)], 3);
        let similarity = sparse_cosine_similarity(&v1, &v2);
        assert!((similarity - 1.0).abs() < 1e-6);

        let v1 = SparseVector::new(vec![(0, 1.0)], 3);
        let v2 = SparseVector::new(vec![(1, 1.0)], 3);
        let similarity = sparse_cosine_similarity(&v1, &v2);
        assert!((similarity - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_sparse_euclidean_distance() {
        let v1 = SparseVector::new(vec![(0, 0.0), (1, 0.0), (2, 0.0)], 3);
        let v2 = SparseVector::new(vec![(0, 1.0), (1, 0.0), (2, 0.0)], 3);
        let distance = sparse_euclidean_distance(&v1, &v2);
        assert!((distance - 1.0).abs() < 1e-6);

        let v1 = SparseVector::new(vec![(0, 0.0), (1, 0.0)], 2);
        let v2 = SparseVector::new(vec![(0, 3.0), (1, 4.0)], 2);
        let distance = sparse_euclidean_distance(&v1, &v2);
        assert!((distance - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_sparse_jaccard_similarity() {
        let v1 = SparseVector::new(vec![(0, 1.0), (2, 1.0), (5, 1.0)], 10);
        let v2 = SparseVector::new(vec![(2, 1.0), (3, 1.0), (5, 1.0)], 10);
        let jaccard = sparse_jaccard_similarity(&v1, &v2);
        assert!((jaccard - 0.5).abs() < 1e-6); // 2/4 = 0.5
    }

    #[test]
    fn test_densify_with_threshold() {
        let dense = vec![0.1, 0.001, 3.0, 0.002, 2.0];
        let sparse = densify_with_threshold(&dense, 0.01);
        assert_eq!(sparse.elements.len(), 3);
        assert_eq!(sparse.elements[0], (0, 0.1));
        assert_eq!(sparse.elements[1], (2, 3.0));
        assert_eq!(sparse.elements[2], (4, 2.0));
    }

    #[test]
    fn test_batch_conversions() {
        let dense_vectors = vec![
            vec![1.0, 0.0, 3.0],
            vec![0.0, 2.0, 0.0],
            vec![1.0, 1.0, 1.0],
        ];

        let sparse_vectors = batch_to_sparse(&dense_vectors);
        assert_eq!(sparse_vectors.len(), 3);
        assert_eq!(sparse_vectors[0].nnz(), 2);
        assert_eq!(sparse_vectors[1].nnz(), 1);
        assert_eq!(sparse_vectors[2].nnz(), 3);

        let converted_back = batch_to_dense(&sparse_vectors);
        assert_eq!(converted_back, dense_vectors);
    }

    #[test]
    fn test_sparse_vector_deduplication() {
        // Test that duplicate indices are summed
        let sparse = SparseVector::new(vec![(0, 1.0), (0, 2.0), (1, 3.0)], 5);
        assert_eq!(sparse.elements.len(), 2);
        assert_eq!(sparse.elements[0], (0, 3.0)); // 1.0 + 2.0
        assert_eq!(sparse.elements[1], (1, 3.0));
    }
}
