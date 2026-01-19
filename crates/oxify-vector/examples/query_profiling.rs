//! Query Profiling Example
//!
//! Demonstrates how to use the profiling tools to analyze and optimize vector search queries.

use oxify_vector::{
    IndexHealthChecker, ProfilingConfig, QueryProfiler, SearchConfig, VectorSearchIndex,
};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    println!("=== Query Profiling Example ===\n");

    // Create sample embeddings
    let mut embeddings = HashMap::new();
    for i in 0..10000 {
        let vec: Vec<f32> = (0..768).map(|j| ((i + j) % 100) as f32 / 100.0).collect();
        embeddings.insert(format!("doc_{}", i), vec);
    }

    // Build search index
    let config = SearchConfig::default();
    let mut index = VectorSearchIndex::new(config);
    println!("Building index with {} vectors...", embeddings.len());
    index.build(&embeddings)?;
    println!("Index built successfully!\n");

    // Create a query profiler
    let profiling_config = ProfilingConfig {
        slow_query_threshold_ms: 50,
        detailed_timing: true,
        enable_recommendations: true,
        memory_profiling: false,
    };
    let mut profiler = QueryProfiler::new(profiling_config);

    // Profile a search query
    println!("=== Profiling Search Query ===");
    let query: Vec<f32> = (0..768).map(|i| (i % 100) as f32 / 100.0).collect();
    let k = 10;

    let profile = profiler.profile_search(|| index.search(&query, k))?;

    println!("Query completed in: {:?}", profile.total_duration);
    println!("Results returned: {}", profile.result_count);
    println!("Is slow query: {}", profile.is_slow_query);
    println!("Bottleneck: {:?}", profile.bottleneck);

    if !profile.recommendations.is_empty() {
        println!("\nRecommendations:");
        for (i, rec) in profile.recommendations.iter().enumerate() {
            println!(
                "  {}. [{}] {}: {}",
                i + 1,
                match rec.impact {
                    oxify_vector::ImpactLevel::High => "HIGH",
                    oxify_vector::ImpactLevel::Medium => "MEDIUM",
                    oxify_vector::ImpactLevel::Low => "LOW",
                },
                rec.category,
                rec.description
            );
        }
    }

    // Check index health
    println!("\n=== Index Health Check ===");
    let health_checker = IndexHealthChecker::new();
    let stats = index.get_stats();
    let avg_query_time_ms = profile.total_duration.as_secs_f64() * 1000.0;

    let health_recs =
        health_checker.check_health(stats.num_entities, stats.dimensions, avg_query_time_ms);

    if health_recs.is_empty() {
        println!("Index health: ✓ Good");
    } else {
        println!("Index health: ⚠ Recommendations available");
        for (i, rec) in health_recs.iter().enumerate() {
            println!(
                "  {}. [{}] {}: {}",
                i + 1,
                match rec.impact {
                    oxify_vector::ImpactLevel::High => "HIGH",
                    oxify_vector::ImpactLevel::Medium => "MEDIUM",
                    oxify_vector::ImpactLevel::Low => "LOW",
                },
                rec.category,
                rec.description
            );
        }
    }

    println!("\n=== Profile Multiple Queries ===");
    let queries: Vec<Vec<f32>> = (0..5)
        .map(|i| {
            (0..768)
                .map(|j| ((i * 10 + j) % 100) as f32 / 100.0)
                .collect()
        })
        .collect();

    for (i, query) in queries.iter().enumerate() {
        let profile = profiler.profile_search(|| index.search(query, 20))?;
        println!(
            "Query {}: {:?} ({} results)",
            i + 1,
            profile.total_duration,
            profile.result_count
        );
    }

    println!("\n=== Summary ===");
    println!("✓ Query profiling helps identify performance bottlenecks");
    println!("✓ Index health checks provide optimization recommendations");
    println!("✓ Use profiling to tune your search parameters and index strategy");

    Ok(())
}
