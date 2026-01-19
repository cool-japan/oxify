//! Index Persistence Example
//!
//! Shows how to save and load indexes for faster startup.
//!
//! Run with: cargo run --example save_and_load

use oxify_vector::persistence::{load_index, save_index};
use oxify_vector::{HnswConfig, HnswIndex};
use std::collections::HashMap;
use tempfile::TempDir;

fn main() -> anyhow::Result<()> {
    println!("=== Index Persistence ===\n");

    // Create temp directory
    let temp_dir = TempDir::new()?;
    let path = temp_dir.path().join("index.json");

    // Build index
    let mut embeddings = HashMap::new();
    for i in 0..1_000 {
        embeddings.insert(
            format!("doc_{}", i),
            vec![i as f32 * 0.001, (i + 1) as f32 * 0.001],
        );
    }

    let mut index = HnswIndex::new(HnswConfig::default());
    index.build(&embeddings)?;
    println!("Built index with {} vectors", index.get_stats().num_vectors);

    // Save
    save_index(&index, &path)?;
    println!("Saved to: {}", path.display());

    // Load
    let loaded: HnswIndex = load_index(&path)?;
    println!(
        "Loaded index with {} vectors",
        loaded.get_stats().num_vectors
    );

    // Verify
    let query = vec![0.5, 0.5];
    let results = loaded.search(&query, 3)?;
    println!("\nSearch works! Top result: {}", results[0].entity_id);

    Ok(())
}
