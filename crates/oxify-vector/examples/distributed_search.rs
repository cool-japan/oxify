//! Example: Distributed Vector Search with Sharding
//!
//! This example demonstrates how to use the distributed search functionality
//! to scale vector search across multiple shards.
//!
//! Run with:
//! ```bash
//! cargo run --example distributed_search --features parallel
//! ```

use oxify_vector::{DistributedIndex, SearchConfig, ShardConfig};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    println!("=== Distributed Vector Search Example ===\n");

    // Create embeddings (simulating 1000 documents)
    let mut embeddings = HashMap::new();
    for i in 0..1000 {
        let doc_id = format!("doc{}", i);
        let embedding = vec![
            (i as f32 * 0.001).sin(),
            (i as f32 * 0.002).cos(),
            (i as f32 * 0.003).sin(),
            (i as f32 * 0.001).cos(),
        ];
        embeddings.insert(doc_id, embedding);
    }

    println!("Created {} document embeddings", embeddings.len());

    // Configure distributed index with 3 shards and 2 replicas
    let shard_config = ShardConfig::new(3, 2).with_virtual_nodes(150); // Use 150 virtual nodes for better distribution

    let search_config = SearchConfig::default();

    println!("\nConfiguring distributed index:");
    println!("  - Number of shards: {}", shard_config.num_shards);
    println!("  - Number of replicas: {}", shard_config.num_replicas);
    println!(
        "  - Virtual nodes per shard: {}",
        shard_config.virtual_nodes
    );

    // Build distributed index
    let mut index = DistributedIndex::new(shard_config, search_config);

    println!("\nBuilding distributed index...");
    index.build(&embeddings)?;
    println!("✓ Index built successfully");

    // Get and display statistics
    let stats = index.get_stats()?;
    println!("\nDistributed Index Statistics:");
    println!("  - Total vectors: {}", stats.total_vectors);
    println!("  - Number of shards: {}", stats.num_shards);
    println!("  - Number of replicas: {}", stats.num_replicas);
    println!("  - Average shard size: {:.1}", stats.avg_shard_size);
    println!("  - Max shard size: {}", stats.max_shard_size);
    println!("  - Min shard size: {}", stats.min_shard_size);
    println!("  - Balance ratio: {:.3}", stats.balance_ratio);

    println!("\n  Shard distribution:");
    for (i, size) in stats.shard_sizes.iter().enumerate() {
        println!("    Shard {}: {} vectors", i, size);
    }

    // Perform search across all shards
    println!("\n=== Distributed Search ===");
    let query = vec![0.5, 0.3, 0.2, 0.1];
    let k = 5;

    println!("Searching for top-{} similar documents...", k);
    let results = index.search(&query, k)?;

    println!("\nTop {} Results:", k);
    for (i, result) in results.iter().enumerate() {
        println!(
            "  {}. {} - Score: {:.4}, Distance: {:.4}",
            i + 1,
            result.entity_id,
            result.score,
            result.distance
        );
    }

    // Demonstrate fan-out search
    println!("\n=== Fan-Out Search Performance ===");
    println!("The distributed search fans out to all shards in parallel,");
    println!("then merges and re-ranks the results from each shard.");
    println!("This allows horizontal scaling to billions of vectors.");

    println!("\n✓ Example completed successfully!");

    Ok(())
}
