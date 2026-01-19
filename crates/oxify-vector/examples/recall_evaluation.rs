//! Example demonstrating recall evaluation for ANN indexes
//!
//! This example shows how to:
//! - Measure recall@k for HNSW approximate search
//! - Compare different HNSW configurations
//! - Evaluate precision and nDCG metrics
//! - Understand accuracy vs speed trade-offs
//!
//! Run with: cargo run --example recall_evaluation

use oxify_vector::{
    EvaluationConfig, HnswConfig, HnswIndex, RecallEvaluator, SearchConfig, VectorSearchIndex,
};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    println!("=== OxiFY Vector: Recall Evaluation Demo ===\n");

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

    // ========== 2. Build Exact Search Index (Ground Truth) ==========
    println!("\n2. Building exact search index (ground truth)");
    println!("{}", "-".repeat(70));

    let mut exact_index = VectorSearchIndex::new(SearchConfig::default());
    exact_index.build(&embeddings)?;
    println!(
        "  ✓ Exact search index built ({} vectors)",
        exact_index.len()
    );

    // ========== 3. Build HNSW Index with Default Config ==========
    println!("\n3. Building HNSW index with default configuration");
    println!("{}", "-".repeat(70));

    let config_default = HnswConfig::default();
    let mut hnsw_default = HnswIndex::new(config_default.clone());
    hnsw_default.build(&embeddings)?;
    println!(
        "  ✓ HNSW index built (M={}, ef_construction={}, ef_search={})",
        config_default.m, config_default.ef_construction, config_default.ef_search
    );

    // ========== 4. Evaluate Single Query ==========
    println!("\n4. Evaluating single query");
    println!("{}", "-".repeat(70));

    let config = EvaluationConfig::default();
    let evaluator = RecallEvaluator::new(config.clone());

    // Create a test query
    let test_query: Vec<f32> = (0..dimensions).map(|d| (d as f32 * 0.01).cos()).collect();

    let metrics = evaluator.evaluate_single_query(
        &test_query,
        |q, k| exact_index.search(q, k),
        |q, k| hnsw_default.search(q, k),
    )?;

    println!("  Recall and Precision at different k values:");
    println!(
        "  {:>6} | {:>10} | {:>10} | {:>10} | {:>5}",
        "k", "Recall@k", "Precision@k", "F1 Score", "nDCG"
    );
    println!("  {}", "-".repeat(60));
    for m in &metrics {
        let f1 = m.f1_score();
        let ndcg = m.ndcg_at_k.unwrap_or(0.0);
        println!(
            "  {:>6} | {:>9.2}% | {:>9.2}% | {:>9.2}% | {:>5.3}",
            m.k,
            m.recall_at_k * 100.0,
            m.precision_at_k * 100.0,
            f1 * 100.0,
            ndcg
        );
    }

    // ========== 5. Batch Evaluation with Multiple Queries ==========
    println!("\n5. Batch evaluation with multiple queries");
    println!("{}", "-".repeat(70));

    // Create 20 test queries
    let test_queries: Vec<Vec<f32>> = (0..20)
        .map(|i| {
            (0..dimensions)
                .map(|d| ((i * d) as f32 * 0.002).sin())
                .collect()
        })
        .collect();

    let quick_config = EvaluationConfig {
        k_values: vec![10, 20, 50],
        calculate_ndcg: true,
        num_test_queries: 20,
    };
    let quick_evaluator = RecallEvaluator::new(quick_config);

    let aggregated = quick_evaluator.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| hnsw_default.search(q, k),
    )?;

    println!("  Average metrics across 20 queries:");
    println!(
        "  {:>6} | {:>10} | {:>10} | {:>10} | {:>10}",
        "k", "Avg Recall", "Avg Precision", "Std Recall", "Avg nDCG"
    );
    println!("  {}", "-".repeat(70));
    for m in &aggregated {
        let ndcg = m.avg_ndcg.unwrap_or(0.0);
        println!(
            "  {:>6} | {:>9.2}% | {:>9.2}% | {:>9.2}% | {:>9.3}",
            m.k,
            m.avg_recall * 100.0,
            m.avg_precision * 100.0,
            m.std_recall * 100.0,
            ndcg
        );
    }

    // ========== 6. Compare Different HNSW Configurations ==========
    println!("\n6. Comparing different HNSW configurations");
    println!("{}", "-".repeat(70));

    // Build high-recall HNSW
    let config_high_recall = HnswConfig::high_recall();
    let mut hnsw_high_recall = HnswIndex::new(config_high_recall.clone());
    hnsw_high_recall.build(&embeddings)?;
    println!(
        "  ✓ Built high-recall HNSW (ef_search={})",
        config_high_recall.ef_search
    );

    // Build fast HNSW
    let config_fast = HnswConfig::fast();
    let mut hnsw_fast = HnswIndex::new(config_fast.clone());
    hnsw_fast.build(&embeddings)?;
    println!("  ✓ Built fast HNSW (ef_search={})", config_fast.ef_search);

    println!("\n  Comparing configurations at k=20:");
    println!(
        "  {:>20} | {:>10} | {:>10}",
        "Configuration", "Recall@20", "Precision@20"
    );
    println!("  {}", "-".repeat(50));

    // Evaluate default config
    let eval_config = EvaluationConfig {
        k_values: vec![20],
        calculate_ndcg: false,
        num_test_queries: 20,
    };
    let eval = RecallEvaluator::new(eval_config.clone());

    let metrics_default = eval.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| hnsw_default.search(q, k),
    )?;

    let metrics_high_recall = eval.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| hnsw_high_recall.search(q, k),
    )?;

    let metrics_fast = eval.evaluate_batch(
        &test_queries,
        |q, k| exact_index.search(q, k),
        |q, k| hnsw_fast.search(q, k),
    )?;

    println!(
        "  {:>20} | {:>9.2}% | {:>9.2}%",
        "Default",
        metrics_default[0].avg_recall * 100.0,
        metrics_default[0].avg_precision * 100.0
    );
    println!(
        "  {:>20} | {:>9.2}% | {:>9.2}%",
        "High Recall",
        metrics_high_recall[0].avg_recall * 100.0,
        metrics_high_recall[0].avg_precision * 100.0
    );
    println!(
        "  {:>20} | {:>9.2}% | {:>9.2}%",
        "Fast",
        metrics_fast[0].avg_recall * 100.0,
        metrics_fast[0].avg_precision * 100.0
    );

    // ========== 7. Key Takeaways ==========
    println!("\n7. Key Takeaways");
    println!("{}", "-".repeat(70));
    println!("  • Higher ef_search → Better recall but slower queries");
    println!("  • Recall@k measures how many true top-k neighbors are found");
    println!("  • Precision@k measures accuracy of retrieved results");
    println!("  • nDCG@k considers ranking quality, not just presence");
    println!("  • Use RecallEvaluator to find optimal config for your use case");
    println!("\n  Recommendation:");
    println!("  - Start with default config and measure recall");
    println!("  - If recall < 90%, increase ef_search");
    println!("  - If queries are too slow, decrease ef_search or use IVF-PQ");
    println!("  - Always measure on your actual dataset and queries!");

    Ok(())
}
