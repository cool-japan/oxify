//! Multi-Index Search Example
//!
//! Demonstrates searching across multiple indexes in parallel,
//! useful for federated search, multi-tenant scenarios, and temporal sharding.
//!
//! Run with: cargo run --example multi_index_search

use oxify_vector::{
    MultiIndexConfig, MultiIndexSearch, ScoreMergeStrategy, SearchConfig, VectorSearchIndex,
};
use std::collections::HashMap;
use std::time::Instant;

fn generate_random_vector(dim: usize, seed: usize) -> Vec<f32> {
    (0..dim)
        .map(|i| ((seed * 31 + i * 17) % 1000) as f32 / 1000.0)
        .collect()
}

fn main() -> anyhow::Result<()> {
    println!("=== Multi-Index Search Example ===\n");

    // Scenario: Multi-tenant search with separate indexes per tenant
    println!("1. Creating separate indexes for different tenants...");

    // Tenant A index (e.g., Company A's documents)
    let mut tenant_a_embeddings = HashMap::new();
    for i in 0..500 {
        tenant_a_embeddings.insert(
            format!("tenant_a_doc_{}", i),
            generate_random_vector(128, i),
        );
    }
    let mut index_a = VectorSearchIndex::new(SearchConfig::default());
    index_a.build(&tenant_a_embeddings)?;
    println!("   Tenant A: {} vectors", tenant_a_embeddings.len());

    // Tenant B index (e.g., Company B's documents)
    let mut tenant_b_embeddings = HashMap::new();
    for i in 0..300 {
        tenant_b_embeddings.insert(
            format!("tenant_b_doc_{}", i),
            generate_random_vector(128, i + 1000),
        );
    }
    let mut index_b = VectorSearchIndex::new(SearchConfig::default());
    index_b.build(&tenant_b_embeddings)?;
    println!("   Tenant B: {} vectors", tenant_b_embeddings.len());

    // Tenant C index (e.g., Company C's documents)
    let mut tenant_c_embeddings = HashMap::new();
    for i in 0..400 {
        tenant_c_embeddings.insert(
            format!("tenant_c_doc_{}", i),
            generate_random_vector(128, i + 2000),
        );
    }
    let mut index_c = VectorSearchIndex::new(SearchConfig::default());
    index_c.build(&tenant_c_embeddings)?;
    println!("   Tenant C: {} vectors", tenant_c_embeddings.len());
    println!();

    // Search across all tenants
    println!("2. Searching across all tenant indexes (parallel)...");
    let multi_search = MultiIndexSearch::new();
    let query = generate_random_vector(128, 5000);
    let k = 10;

    let start = Instant::now();
    let results = multi_search.search(&[&index_a, &index_b, &index_c], &query, k)?;
    let duration = start.elapsed();

    println!("   Found {} results in {:?}", results.len(), duration);
    for (i, result) in results.iter().take(5).enumerate() {
        println!(
            "     {}. {} (score: {:.4})",
            i + 1,
            result.entity_id,
            result.score
        );
    }
    println!();

    // Demonstrate different merge strategies
    println!("3. Testing different score merge strategies...");

    println!("\n   a) Max Strategy (take highest score):");
    let config_max = MultiIndexConfig {
        parallel: true,
        deduplicate: true,
        merge_strategy: ScoreMergeStrategy::Max,
    };
    let multi_max = MultiIndexSearch::with_config(config_max);
    let results_max = multi_max.search(&[&index_a, &index_b, &index_c], &query, k)?;
    println!(
        "      Top result: {} (score: {:.4})",
        results_max[0].entity_id, results_max[0].score
    );

    println!("\n   b) Average Strategy (average all scores):");
    let config_avg = MultiIndexConfig {
        parallel: true,
        deduplicate: true,
        merge_strategy: ScoreMergeStrategy::Average,
    };
    let multi_avg = MultiIndexSearch::with_config(config_avg);
    let results_avg = multi_avg.search(&[&index_a, &index_b, &index_c], &query, k)?;
    println!(
        "      Top result: {} (score: {:.4})",
        results_avg[0].entity_id, results_avg[0].score
    );

    println!("\n   c) Min Strategy (take lowest score):");
    let config_min = MultiIndexConfig {
        parallel: true,
        deduplicate: true,
        merge_strategy: ScoreMergeStrategy::Min,
    };
    let multi_min = MultiIndexSearch::with_config(config_min);
    let results_min = multi_min.search(&[&index_a, &index_b, &index_c], &query, k)?;
    println!(
        "      Top result: {} (score: {:.4})",
        results_min[0].entity_id, results_min[0].score
    );
    println!();

    // Batch search example
    println!("4. Batch search across multiple indexes...");
    let queries = vec![
        generate_random_vector(128, 6000),
        generate_random_vector(128, 7000),
        generate_random_vector(128, 8000),
    ];

    let start = Instant::now();
    let batch_results = multi_search.batch_search(&[&index_a, &index_b, &index_c], &queries, k)?;
    let duration = start.elapsed();

    println!("   Processed {} queries in {:?}", queries.len(), duration);
    println!("   Results per query:");
    for (i, results) in batch_results.iter().enumerate() {
        println!(
            "     Query {}: {} results (top: {})",
            i + 1,
            results.len(),
            results[0].entity_id
        );
    }
    println!();

    // Parallel vs Sequential comparison
    println!("5. Parallel vs Sequential comparison...");

    let config_parallel = MultiIndexConfig {
        parallel: true,
        deduplicate: true,
        merge_strategy: ScoreMergeStrategy::Max,
    };
    let multi_parallel = MultiIndexSearch::with_config(config_parallel);

    let config_sequential = MultiIndexConfig {
        parallel: false,
        deduplicate: true,
        merge_strategy: ScoreMergeStrategy::Max,
    };
    let multi_sequential = MultiIndexSearch::with_config(config_sequential);

    let start = Instant::now();
    let _ = multi_parallel.search(&[&index_a, &index_b, &index_c], &query, k)?;
    let parallel_duration = start.elapsed();

    let start = Instant::now();
    let _ = multi_sequential.search(&[&index_a, &index_b, &index_c], &query, k)?;
    let sequential_duration = start.elapsed();

    println!("   Parallel:   {:?}", parallel_duration);
    println!("   Sequential: {:?}", sequential_duration);
    println!(
        "   Speedup:    {:.2}x",
        sequential_duration.as_secs_f64() / parallel_duration.as_secs_f64()
    );
    println!();

    // Use case scenarios
    println!("=== Use Case Scenarios ===");
    println!("✓ Multi-tenant search: Separate indexes per customer/organization");
    println!("✓ Temporal sharding: Different indexes for time periods (2025, 2026, 2027)");
    println!("✓ Federated search: Search across different data sources");
    println!("✓ Geographic distribution: Indexes per region/datacenter");
    println!("✓ A/B testing: Compare results from different index configurations");
    println!();

    println!("=== Key Features ===");
    println!("✓ Parallel search across multiple indexes for speed");
    println!("✓ Automatic result merging and deduplication");
    println!("✓ Multiple score merge strategies (Max, Min, Average, First)");
    println!("✓ Batch search support for high throughput");
    println!("✓ Configurable parallel/sequential execution");

    Ok(())
}
