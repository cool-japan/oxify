//! Basic Vector Search Example
//!
//! Demonstrates the fundamental operations: building an index and searching.
//!
//! Run with: cargo run --example basic_usage

use oxify_vector::{DistanceMetric, SearchConfig, VectorSearchIndex};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    println!("=== Basic Vector Search ===\n");

    // Create embeddings
    let mut embeddings = HashMap::new();
    embeddings.insert("rust".to_string(), vec![0.9, 0.1, 0.0]);
    embeddings.insert("python".to_string(), vec![0.1, 0.9, 0.0]);
    embeddings.insert("javascript".to_string(), vec![0.0, 0.1, 0.9]);

    // Build index
    let config = SearchConfig {
        metric: DistanceMetric::Cosine,
        parallel: false,
        normalize: true,
    };
    let mut index = VectorSearchIndex::new(config);
    index.build(&embeddings)?;

    // Search
    let query = vec![0.85, 0.15, 0.0];
    let results = index.search(&query, 2)?;

    println!("Query: {:?}", query);
    println!("\nResults:");
    for (i, result) in results.iter().enumerate() {
        println!(
            "  {}. {} (score: {:.4})",
            i + 1,
            result.entity_id,
            result.score
        );
    }

    Ok(())
}
