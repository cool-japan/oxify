//! Sparse vector operations demo
//!
//! This example demonstrates the benefits of sparse vectors for high-dimensional
//! data where most elements are zero (common in NLP with TF-IDF, bag-of-words, etc.)

use oxify_connect_vector::sparse::{
    sparse_cosine_similarity, sparse_dot_product, sparse_jaccard_similarity, SparseVector,
};

fn main() {
    println!("Sparse Vector Operations Demo");
    println!("==============================\n");

    // Example 1: TF-IDF-like vectors (10,000 dimensions, mostly zeros)
    println!("Example 1: TF-IDF Document Vectors");
    println!("-----------------------------------");

    // Document 1: "machine learning algorithms"
    // Only a few terms have non-zero TF-IDF scores
    let doc1 = SparseVector::new(
        vec![
            (47, 0.8),    // "machine"
            (153, 0.9),   // "learning"
            (892, 0.7),   // "algorithms"
            (1543, 0.4),  // "neural"
            (2891, 0.35), // "deep"
        ],
        10000, // vocabulary size
    );

    // Document 2: "deep learning neural networks"
    let doc2 = SparseVector::new(
        vec![
            (153, 0.85),  // "learning"
            (1543, 0.9),  // "neural"
            (2018, 0.75), // "networks"
            (2891, 0.8),  // "deep"
        ],
        10000,
    );

    println!("Document 1: {} non-zero terms out of 10,000", doc1.nnz());
    println!("Document 2: {} non-zero terms out of 10,000", doc2.nnz());
    println!("Sparsity: {:.2}%", doc1.sparsity() * 100.0);
    println!();

    // Compute similarity (only processes overlapping non-zero elements!)
    let similarity = sparse_cosine_similarity(&doc1, &doc2);
    println!("Cosine similarity: {:.4}", similarity);

    let dot = sparse_dot_product(&doc1, &doc2);
    println!("Dot product: {:.4}", dot);
    println!();

    // Example 2: One-hot encoded vectors
    println!("Example 2: One-Hot Categorical Vectors");
    println!("---------------------------------------");

    let category1 = SparseVector::new(vec![(42, 1.0)], 1000); // Category 42 active
    let category2 = SparseVector::new(vec![(42, 1.0)], 1000); // Same category
    let category3 = SparseVector::new(vec![(137, 1.0)], 1000); // Different category

    println!(
        "Same category similarity: {:.4}",
        sparse_cosine_similarity(&category1, &category2)
    );
    println!(
        "Different category similarity: {:.4}",
        sparse_cosine_similarity(&category1, &category3)
    );
    println!();

    // Example 3: Jaccard similarity for set-based comparison
    println!("Example 3: Jaccard Similarity (Set Overlap)");
    println!("--------------------------------------------");

    let set1 = SparseVector::new(vec![(1, 1.0), (2, 1.0), (5, 1.0), (8, 1.0), (10, 1.0)], 100);

    let set2 = SparseVector::new(vec![(2, 1.0), (5, 1.0), (7, 1.0), (9, 1.0), (11, 1.0)], 100);

    let jaccard = sparse_jaccard_similarity(&set1, &set2);
    println!("Set 1 elements: {:?}", set1.elements);
    println!("Set 2 elements: {:?}", set2.elements);
    println!("Jaccard similarity: {:.4}", jaccard);
    println!("(Intersection: 2 elements, Union: 8 elements)");
    println!();

    // Example 4: Memory efficiency comparison
    println!("Example 4: Memory Efficiency");
    println!("-----------------------------");

    let dim = 100000;
    let nnz = 100; // Only 100 non-zero elements

    let sparse = SparseVector::new(
        (0..nnz).map(|i| (i * 1000, (i as f32) / 100.0)).collect(),
        dim,
    );

    let dense = sparse.to_dense();

    let sparse_size = std::mem::size_of::<SparseVector>()
        + sparse.elements.len() * std::mem::size_of::<(usize, f32)>();
    let dense_size = dense.len() * std::mem::size_of::<f32>();

    println!("Sparse vector memory: ~{} bytes", sparse_size);
    println!("Dense vector memory:  {} bytes", dense_size);
    println!(
        "Memory savings: {:.1}x smaller",
        dense_size as f64 / sparse_size as f64
    );
    println!();

    // Example 5: Normalization
    println!("Example 5: Vector Normalization");
    println!("--------------------------------");

    let mut vec = SparseVector::new(vec![(0, 3.0), (1, 4.0)], 10);
    println!("Original norm: {:.4}", vec.norm());

    vec.normalize();
    println!("After normalization: {:.4}", vec.norm());
    println!("Normalized values: {:?}", vec.elements);
}
