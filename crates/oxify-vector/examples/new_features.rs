//! Example demonstrating new features in oxify-vector
//!
//! This example showcases:
//! - Incremental index updates (add/remove/update vectors)
//! - Query optimizer for strategy selection
//! - Scalar quantization for memory optimization
//!
//! Run with: cargo run --example new_features

use oxify_vector::{
    AdaptiveConfig, AdaptiveIndex, OptimizerConfig, QuantizationConfig, QuantizedVectorIndex,
    QueryOptimizer, QueryPlan, SearchConfig, VectorSearchIndex,
};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    println!("=== OxiFY Vector: New Features Demo ===\n");

    // ========== 1. Incremental Index Updates ==========
    println!("1. Incremental Index Updates");
    println!("{}", "-".repeat(50));

    // Create initial index
    let mut embeddings = HashMap::new();
    embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
    embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
    embeddings.insert("doc3".to_string(), vec![0.3, 0.4, 0.5]);

    let mut index = VectorSearchIndex::new(SearchConfig::default());
    index.build(&embeddings)?;
    println!("  Initial index size: {}", index.len());

    // Add a single vector
    index.add_vector("doc4".to_string(), vec![0.4, 0.5, 0.6])?;
    println!("  After adding 1 vector: {}", index.len());

    // Add multiple vectors
    let mut new_embeddings = HashMap::new();
    new_embeddings.insert("doc5".to_string(), vec![0.5, 0.6, 0.7]);
    new_embeddings.insert("doc6".to_string(), vec![0.6, 0.7, 0.8]);
    index.add_vectors(&new_embeddings)?;
    println!("  After adding 2 more vectors: {}", index.len());

    // Update a vector
    index.update_vector("doc1", vec![0.9, 0.9, 0.9])?;
    println!("  Updated doc1 with new embedding");

    // Remove a vector
    index.remove_vector("doc2")?;
    println!("  After removing 1 vector: {}", index.len());

    // Search with updated index
    let query = vec![0.5, 0.6, 0.7];
    let results = index.search(&query, 3)?;
    println!("  Top 3 results:");
    for result in results {
        println!("    - {}: score = {:.4}", result.entity_id, result.score);
    }

    // ========== 2. Query Optimizer ==========
    println!("\n2. Query Optimizer");
    println!("{}", "-".repeat(50));

    let optimizer = QueryOptimizer::new(OptimizerConfig::default());

    // Recommend strategy based on dataset size
    let small_dataset = 5_000;
    let medium_dataset = 100_000;
    let large_dataset = 2_000_000;

    println!("  Strategy recommendations:");
    println!(
        "    {} vectors → {:?}",
        small_dataset,
        optimizer.recommend_strategy(small_dataset, 0.95)
    );
    println!(
        "    {} vectors → {:?}",
        medium_dataset,
        optimizer.recommend_strategy(medium_dataset, 0.95)
    );
    println!(
        "    {} vectors → {:?}",
        large_dataset,
        optimizer.recommend_strategy(large_dataset, 0.95)
    );

    // Recommend filtering strategy
    println!("\n  Filtering strategy recommendations:");
    println!(
        "    5% filter selectivity → {} pre-filtering",
        if optimizer.recommend_prefiltering(100_000, 0.05) {
            "use"
        } else {
            "don't use"
        }
    );
    println!(
        "    50% filter selectivity → {} pre-filtering",
        if optimizer.recommend_prefiltering(100_000, 0.50) {
            "use"
        } else {
            "don't use"
        }
    );

    // Recommend batch size
    let batch_size = optimizer.recommend_batch_size(1000, 100_000);
    println!(
        "\n  Recommended batch size for 1000 queries: {}",
        batch_size
    );

    // Create optimized query plan
    println!("\n  Optimized query plan:");
    let plan = QueryPlan::optimize(
        &optimizer,
        100_000,    // num_vectors
        50,         // num_queries
        10,         // k
        Some(0.05), // filter_selectivity
        0.95,       // required_recall
    );
    println!("    Strategy: {:?}", plan.strategy);
    println!("    Use pre-filtering: {}", plan.use_prefiltering);
    println!("    Batch size: {}", plan.batch_size);
    println!("    Estimated cost: {:.2}", plan.estimated_cost);

    // ========== 3. Scalar Quantization ==========
    println!("\n3. Scalar Quantization (Memory Optimization)");
    println!("{}", "-".repeat(50));

    // Generate larger dataset for quantization demo
    let vectors: Vec<(String, Vec<f32>)> = (0..1000)
        .map(|i| {
            let vec = vec![
                (i as f32 * 0.1) % 1.0,
                (i as f32 * 0.2) % 1.0,
                (i as f32 * 0.3) % 1.0,
                (i as f32 * 0.4) % 1.0,
                (i as f32 * 0.5) % 1.0,
            ];
            (format!("doc_{}", i), vec)
        })
        .collect();

    // Build quantized index
    let mut quantized_index = QuantizedVectorIndex::new(QuantizationConfig::default());
    quantized_index.build(&vectors)?;

    let stats = quantized_index.stats();
    println!("  Quantization Statistics:");
    println!("    Vectors: {}", stats.num_vectors);
    println!("    Dimensions: {}", stats.dimensions);
    println!(
        "    Original size: {} bytes ({:.2} KB)",
        stats.original_bytes,
        stats.original_bytes as f64 / 1024.0
    );
    println!(
        "    Quantized size: {} bytes ({:.2} KB)",
        stats.quantized_bytes,
        stats.quantized_bytes as f64 / 1024.0
    );
    println!("    Compression ratio: {:.2}x", stats.compression_ratio);
    println!("    Memory savings: {:.1}%", stats.memory_savings * 100.0);

    // Search with quantized index
    let query = vec![0.5, 0.5, 0.5, 0.5, 0.5];
    let results = quantized_index.search(&query, 5)?;
    println!("\n  Top 5 results from quantized index:");
    for (id, distance) in results {
        println!("    - {}: distance = {:.4}", id, distance);
    }

    // ========== 4. Adaptive Index ==========
    println!("\n4. Adaptive Index (Automatic Optimization)");
    println!("{}", "-".repeat(50));

    // Create adaptive index - it automatically optimizes
    let mut adaptive = AdaptiveIndex::new(AdaptiveConfig::default());

    // Start with small dataset (uses brute-force)
    let mut small_data = HashMap::new();
    for i in 0..100 {
        small_data.insert(
            format!("doc_{}", i),
            vec![
                (i as f32 * 0.1) % 1.0,
                (i as f32 * 0.2) % 1.0,
                (i as f32 * 0.3) % 1.0,
            ],
        );
    }
    adaptive.build(&small_data)?;

    println!("  Initial dataset: {} vectors", adaptive.len());
    println!("  Strategy: {:?}", adaptive.stats().current_strategy);

    // Do some searches
    let query = vec![0.5, 0.5, 0.5];
    for _ in 0..5 {
        let _ = adaptive.search(&query, 5)?;
    }

    // Show performance stats
    let stats = adaptive.stats();
    println!("  Performance after 5 searches:");
    println!("    Avg latency: {:.3}ms", stats.avg_latency_ms);
    println!("    P95 latency: {:.3}ms", stats.p95_latency_ms);

    // Add more data incrementally
    println!("\n  Adding more vectors...");
    for i in 100..500 {
        adaptive.add_vector(
            format!("doc_{}", i),
            vec![
                (i as f32 * 0.1) % 1.0,
                (i as f32 * 0.2) % 1.0,
                (i as f32 * 0.3) % 1.0,
            ],
        )?;
    }

    println!("  Dataset now: {} vectors", adaptive.len());
    println!("  Strategy: {:?}", adaptive.stats().current_strategy);

    // Search again
    let results = adaptive.search(&query, 5)?;
    println!("\n  Top 5 results:");
    for result in results {
        println!("    - {}: score = {:.4}", result.entity_id, result.score);
    }

    // ========== 5. Combined Example ==========
    println!("\n5. Combined Workflow");
    println!("{}", "-".repeat(50));

    // Use optimizer to select strategy
    let num_vectors = vectors.len();
    let strategy = optimizer.recommend_strategy(num_vectors, 0.95);
    println!("  Dataset size: {} vectors", num_vectors);
    println!("  Recommended strategy: {:?}", strategy);

    // Based on strategy, use appropriate index
    match strategy {
        oxify_vector::SearchStrategy::BruteForce => {
            println!("  → Using exact search (small dataset)");

            // Convert to HashMap for VectorSearchIndex
            let embeddings: HashMap<String, Vec<f32>> = vectors
                .iter()
                .map(|(id, v)| (id.clone(), v.clone()))
                .collect();

            let mut index = VectorSearchIndex::new(SearchConfig::default());
            index.build(&embeddings)?;

            // Demonstrate incremental updates
            println!("\n  Demonstrating incremental updates:");
            index.add_vector("new_doc".to_string(), vec![0.8, 0.8, 0.8, 0.8, 0.8])?;
            println!("    Added 1 vector, new size: {}", index.len());

            // Search
            let results = index.search(&query, 3)?;
            println!("\n  Search results:");
            for result in results {
                println!("    - {}: score = {:.4}", result.entity_id, result.score);
            }
        }
        _ => {
            println!("  → Would use HNSW or IVF-PQ for larger datasets");
        }
    }

    println!("\n=== Demo Complete ===");
    Ok(())
}
