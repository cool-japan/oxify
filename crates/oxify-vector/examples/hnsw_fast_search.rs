//! HNSW Approximate Nearest Neighbor Search
//!
//! Demonstrates fast approximate search for larger datasets.
//!
//! Run with: cargo run --example hnsw_fast_search

use oxify_vector::{HnswConfig, HnswIndex};
use std::collections::HashMap;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    println!("=== HNSW Approximate Search ===\n");

    // Create a dataset with 5,000 vectors
    println!("Building index with 5,000 vectors...");
    let mut embeddings = HashMap::new();
    for i in 0..5_000 {
        let vec = vec![
            (i as f32 * 0.01).sin(),
            (i as f32 * 0.01).cos(),
            (i as f32 * 0.02).sin(),
        ];
        embeddings.insert(format!("doc_{}", i), vec);
    }

    // Build HNSW index
    let config = HnswConfig::default();
    let mut index = HnswIndex::new(config);

    let start = Instant::now();
    index.build(&embeddings)?;
    println!("Build time: {:?}\n", start.elapsed());

    // Search
    let query = vec![0.5, 0.5, 0.5];
    let start = Instant::now();
    let results = index.search(&query, 10)?;
    println!("Search time: {:?}", start.elapsed());

    println!("\nTop 5 results:");
    for (i, result) in results.iter().take(5).enumerate() {
        println!(
            "  {}. {} (score: {:.4})",
            i + 1,
            result.entity_id,
            result.score
        );
    }

    // Show stats
    let stats = index.get_stats();
    println!("\nIndex stats:");
    println!("  Vectors: {}", stats.num_vectors);
    println!("  Dimensions: {}", stats.dimensions);

    Ok(())
}
