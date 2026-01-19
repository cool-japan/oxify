//! Adaptive Index Example
//!
//! Demonstrates the AdaptiveIndex that automatically selects and switches
//! between index types based on dataset size and query performance.
//!
//! Run with: cargo run --example adaptive_index

use oxify_vector::{AdaptiveConfig, AdaptiveIndex};
use std::collections::HashMap;
use std::time::Instant;

fn generate_random_vector(dim: usize, seed: usize) -> Vec<f32> {
    (0..dim)
        .map(|i| ((seed * 31 + i * 17) % 1000) as f32 / 1000.0)
        .collect()
}

fn main() -> anyhow::Result<()> {
    println!("=== Adaptive Index Example ===\n");

    // Start with a small dataset
    println!("1. Creating small dataset (100 vectors)...");
    let mut embeddings = HashMap::new();
    for i in 0..100 {
        embeddings.insert(format!("doc_{}", i), generate_random_vector(384, i));
    }

    // Create adaptive index with default config
    let mut index = AdaptiveIndex::new(AdaptiveConfig::default());
    index.build(&embeddings)?;

    println!("   Strategy: {:?}", index.current_strategy());
    println!("   (Small dataset uses BruteForce for accuracy)\n");

    // Perform some searches
    println!("2. Performing searches on small dataset...");
    let query = generate_random_vector(384, 999);
    let start = Instant::now();
    let results = index.search(&query, 5)?;
    let duration = start.elapsed();

    println!("   Found {} results in {:?}", results.len(), duration);
    for (i, result) in results.iter().take(3).enumerate() {
        println!(
            "     {}. {} (score: {:.4})",
            i + 1,
            result.entity_id,
            result.score
        );
    }
    println!();

    // Grow the dataset to trigger auto-upgrade
    println!("3. Growing dataset to 15,000 vectors...");
    println!("   (This should trigger auto-upgrade to HNSW)");
    for i in 100..15000 {
        index.add_vector(format!("doc_{}", i), generate_random_vector(384, i))?;
    }

    println!("   Strategy after growth: {:?}", index.current_strategy());
    println!("   (Large dataset uses HNSW for speed)\n");

    // Perform searches on large dataset
    println!("4. Performing searches on large dataset...");
    let start = Instant::now();
    let results = index.search(&query, 5)?;
    let duration = start.elapsed();

    println!("   Found {} results in {:?}", results.len(), duration);
    for (i, result) in results.iter().take(3).enumerate() {
        println!(
            "     {}. {} (score: {:.4})",
            i + 1,
            result.entity_id,
            result.score
        );
    }
    println!();

    // Show performance statistics
    println!("5. Performance Statistics:");
    let stats = index.stats();
    println!("   Vectors: {}", stats.num_vectors);
    println!("   Dimensions: {}", stats.dimensions);
    println!("   Current Strategy: {:?}", stats.current_strategy);
    println!("   Total Searches: {}", stats.total_searches);
    println!("   Avg Latency: {:.2}ms", stats.avg_latency_ms);
    println!("   P95 Latency: {:.2}ms", stats.p95_latency_ms);
    println!();

    // Demonstrate config presets
    println!("6. Config Presets:");
    println!("   a) High Accuracy Config:");
    let high_acc = AdaptiveConfig::high_accuracy();
    println!("      - Min recall: {}", high_acc.min_recall);
    println!(
        "      - Latency threshold: {}ms",
        high_acc.latency_threshold_ms
    );

    println!("\n   b) Low Latency Config:");
    let low_lat = AdaptiveConfig::low_latency();
    println!("      - Min recall: {}", low_lat.min_recall);
    println!(
        "      - Latency threshold: {}ms",
        low_lat.latency_threshold_ms
    );
    println!();

    // Demonstrate low latency configuration
    println!("7. Creating index with low latency config...");
    let mut fast_index = AdaptiveIndex::new(AdaptiveConfig::low_latency());
    let small_embeddings: HashMap<String, Vec<f32>> = (0..1000)
        .map(|i| (format!("doc_{}", i), generate_random_vector(384, i)))
        .collect();
    fast_index.build(&small_embeddings)?;

    println!("   Strategy: {:?}", fast_index.current_strategy());
    println!("   Config optimized for: Low latency (aggressive ANN usage)");
    println!();

    println!("=== Key Takeaways ===");
    println!("✓ AdaptiveIndex automatically selects the best strategy");
    println!("✓ Seamlessly upgrades from BruteForce → HNSW as data grows");
    println!("✓ Tracks performance metrics for optimization");
    println!("✓ Config presets for different use cases");
    println!("✓ No manual index type management required");

    Ok(())
}
