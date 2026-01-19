//! Example demonstrating LSH (Locality Sensitive Hashing) for ANN search
//!
//! This example shows how to:
//! - Build an LSH index for approximate nearest neighbor search
//! - Configure LSH parameters (num_tables, num_bits, num_probes)
//! - Compare LSH with exact search and HNSW
//! - Understand LSH trade-offs (speed vs accuracy vs memory)
//!
//! Run with: cargo run --example lsh_search

use oxify_vector::{
    EvaluationConfig, HnswConfig, HnswIndex, LshConfig, LshIndex, RecallEvaluator, SearchConfig,
    VectorSearchIndex,
};
use std::collections::HashMap;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    println!("=== OxiFY Vector: LSH Search Demo ===\n");

    // ========== 1. Create Test Dataset ==========
    println!("1. Creating test dataset (1000 vectors, 128 dimensions)");
    println!("{}", "-".repeat(70));

    let num_vectors = 1000;
    let dimensions = 128;

    let mut embeddings = HashMap::new();
    for i in 0..num_vectors {
        let vec: Vec<f32> = (0..dimensions)
            .map(|d| ((i * d) as f32 * 0.001).sin())
            .collect();
        embeddings.insert(format!("doc{}", i), vec);
    }
    println!(
        "  Created {} vectors with {} dimensions",
        num_vectors, dimensions
    );

    // ========== 2. Build LSH Index ==========
    println!("\n2. Building LSH index with default configuration");
    println!("{}", "-".repeat(70));

    let config_lsh = LshConfig::default();
    let mut lsh_index = LshIndex::new(config_lsh.clone());
    lsh_index.build(&embeddings)?;

    let stats = lsh_index.stats();
    println!("  ✓ LSH index built");
    println!("    - Tables: {}", stats.num_tables);
    println!("    - Bits per hash: {}", stats.num_bits);
    println!("    - Total buckets: {}", stats.total_buckets);
    println!("    - Avg bucket size: {:.2}", stats.avg_bucket_size);
    println!("    - Max bucket size: {}", stats.max_bucket_size);

    // ========== 3. Build Comparison Indexes ==========
    println!("\n3. Building comparison indexes (exact search and HNSW)");
    println!("{}", "-".repeat(70));

    let mut exact_index = VectorSearchIndex::new(SearchConfig::default());
    exact_index.build(&embeddings)?;
    println!("  ✓ Exact search index built");

    let mut hnsw_index = HnswIndex::new(HnswConfig::default());
    hnsw_index.build(&embeddings)?;
    println!("  ✓ HNSW index built");

    // ========== 4. Search Performance Comparison ==========
    println!("\n4. Search performance comparison");
    println!("{}", "-".repeat(70));

    let test_query: Vec<f32> = (0..dimensions).map(|d| (d as f32 * 0.01).cos()).collect();
    let k = 20;

    // LSH search
    let start = Instant::now();
    let lsh_results = lsh_index.search(&test_query, k)?;
    let lsh_time = start.elapsed();
    println!(
        "  LSH search:    {:>8.2?} ({} results)",
        lsh_time,
        lsh_results.len()
    );

    // HNSW search
    let start = Instant::now();
    let hnsw_results = hnsw_index.search(&test_query, k)?;
    let hnsw_time = start.elapsed();
    println!(
        "  HNSW search:   {:>8.2?} ({} results)",
        hnsw_time,
        hnsw_results.len()
    );

    // Exact search
    let start = Instant::now();
    let exact_results = exact_index.search(&test_query, k)?;
    let exact_time = start.elapsed();
    println!(
        "  Exact search:  {:>8.2?} ({} results)",
        exact_time,
        exact_results.len()
    );

    println!("\n  Speedup vs exact search:");
    println!(
        "    LSH:  {:.1}x faster",
        exact_time.as_secs_f64() / lsh_time.as_secs_f64()
    );
    println!(
        "    HNSW: {:.1}x faster",
        exact_time.as_secs_f64() / hnsw_time.as_secs_f64()
    );

    // ========== 5. Recall Evaluation ==========
    println!("\n5. Recall evaluation (LSH vs HNSW)");
    println!("{}", "-".repeat(70));

    let eval_config = EvaluationConfig {
        k_values: vec![10, 20],
        calculate_ndcg: false,
        num_test_queries: 20,
    };
    let evaluator = RecallEvaluator::new(eval_config);

    // Create test queries
    let test_queries: Vec<Vec<f32>> = (0..20)
        .map(|i| {
            (0..dimensions)
                .map(|d| ((i * d) as f32 * 0.002).sin())
                .collect()
        })
        .collect();

    // Evaluate LSH
    let lsh_metrics = evaluator.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| lsh_index.search(q, k),
    )?;

    // Evaluate HNSW
    let hnsw_metrics = evaluator.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| hnsw_index.search(q, k),
    )?;

    println!(
        "  {:>6} | {:>15} | {:>15}",
        "k", "LSH Recall", "HNSW Recall"
    );
    println!("  {}", "-".repeat(45));
    for (lsh_m, hnsw_m) in lsh_metrics.iter().zip(hnsw_metrics.iter()) {
        println!(
            "  {:>6} | {:>14.2}% | {:>14.2}%",
            lsh_m.k,
            lsh_m.avg_recall * 100.0,
            hnsw_m.avg_recall * 100.0
        );
    }

    // ========== 6. Configuration Comparison ==========
    println!("\n6. Comparing different LSH configurations");
    println!("{}", "-".repeat(70));

    // Fast config
    let config_fast = LshConfig::fast();
    let mut lsh_fast = LshIndex::new(config_fast.clone());
    lsh_fast.build(&embeddings)?;
    println!(
        "  ✓ Built fast LSH (tables={}, probes={})",
        config_fast.num_tables, config_fast.num_probes
    );

    // High recall config
    let config_high = LshConfig::high_recall();
    let mut lsh_high = LshIndex::new(config_high.clone());
    lsh_high.build(&embeddings)?;
    println!(
        "  ✓ Built high-recall LSH (tables={}, probes={})",
        config_high.num_tables, config_high.num_probes
    );

    println!("\n  Comparing at k=20:");
    println!(
        "  {:>20} | {:>10} | {:>10}",
        "Configuration", "Recall@20", "Avg Time"
    );
    println!("  {}", "-".repeat(50));

    // Evaluate fast config
    let start = Instant::now();
    let fast_metrics = evaluator.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| lsh_fast.search(q, k),
    )?;
    let fast_time = start.elapsed().as_secs_f64() / test_queries.len() as f64;

    // Evaluate default config
    let start = Instant::now();
    let default_metrics = evaluator.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| lsh_index.search(q, k),
    )?;
    let default_time = start.elapsed().as_secs_f64() / test_queries.len() as f64;

    // Evaluate high recall config
    let start = Instant::now();
    let high_metrics = evaluator.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| lsh_high.search(q, k),
    )?;
    let high_time = start.elapsed().as_secs_f64() / test_queries.len() as f64;

    println!(
        "  {:>20} | {:>9.2}% | {:>8.2?}",
        "Fast",
        fast_metrics[1].avg_recall * 100.0,
        std::time::Duration::from_secs_f64(fast_time)
    );
    println!(
        "  {:>20} | {:>9.2}% | {:>8.2?}",
        "Default",
        default_metrics[1].avg_recall * 100.0,
        std::time::Duration::from_secs_f64(default_time)
    );
    println!(
        "  {:>20} | {:>9.2}% | {:>8.2?}",
        "High Recall",
        high_metrics[1].avg_recall * 100.0,
        std::time::Duration::from_secs_f64(high_time)
    );

    // ========== 7. Key Takeaways ==========
    println!("\n7. Key Takeaways: When to Use LSH");
    println!("{}", "-".repeat(70));
    println!("  ✓ LSH is fast and simple - good for quick prototyping");
    println!("  ✓ Works well for high-dimensional data (>100 dims)");
    println!("  ✓ Trade-offs:");
    println!("    - More tables → Better recall but more memory");
    println!("    - More bits → More buckets but better precision");
    println!("    - More probes → Better recall but slower queries");
    println!("\n  When to choose LSH over HNSW:");
    println!("    • Need predictable query time (no graph traversal)");
    println!("    • Want simpler implementation and debugging");
    println!("    • Can tolerate slightly lower recall for speed");
    println!("\n  When to choose HNSW over LSH:");
    println!("    • Need highest recall (95%+)");
    println!("    • Want better memory efficiency");
    println!("    • Dataset size is moderate (<10M vectors)");
    println!("\n  Both approaches are valid - choose based on your requirements!");

    Ok(())
}
