//! Example demonstrating parallel batch operations for high-throughput workloads
//!
//! This example shows how to use parallel_batch_insert and parallel_batch_search
//! to achieve significantly better performance when processing large numbers of vectors.
//!
//! Run with: cargo run --example parallel_operations

use oxify_connect_vector::{
    parallel::{parallel_batch_insert, parallel_batch_search, ParallelConfig},
    InsertRequest, MockVectorProvider, SearchRequest, VectorProvider,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Parallel Batch Operations Example ===\n");

    // Create a mock provider for demonstration
    let provider = Arc::new(MockVectorProvider::new());
    provider.create_collection("documents", 384).await?;

    // ===== Example 1: Parallel Batch Insert =====
    println!("Example 1: Parallel Batch Insert");
    println!("{}", "-".repeat(50));

    let num_vectors = 1000;
    println!("Preparing {} vectors for insertion...", num_vectors);

    let mut insert_requests = Vec::new();
    for i in 0..num_vectors {
        insert_requests.push(InsertRequest {
            collection: "documents".to_string(),
            id: format!("doc_{}", i),
            vector: vec![i as f32 * 0.001; 384],
            payload: json!({
                "title": format!("Document {}", i),
                "category": if i % 3 == 0 { "tech" } else if i % 3 == 1 { "science" } else { "business" },
                "index": i
            }),
        });
    }

    // Sequential insert (for comparison)
    let start = Instant::now();
    let provider_seq = Arc::clone(&provider);
    for (idx, request) in insert_requests.iter().take(100).enumerate() {
        provider_seq.insert(request.clone()).await?;
        if idx == 99 {
            let duration = start.elapsed();
            println!(
                "Sequential: Inserted 100 vectors in {:.2}ms ({:.2} vectors/sec)",
                duration.as_millis(),
                100.0 / duration.as_secs_f64()
            );
        }
    }

    // Parallel insert
    let config = ParallelConfig {
        max_concurrent: 10,
        chunk_size: 100,
    };

    println!("\nParallel insert configuration:");
    println!("  - max_concurrent: {}", config.max_concurrent);
    println!("  - chunk_size: {}", config.chunk_size);

    let start = Instant::now();
    let inserted =
        parallel_batch_insert(Arc::clone(&provider), insert_requests.clone(), config).await?;
    let duration = start.elapsed();

    println!(
        "\nParallel: Inserted {} vectors in {:.2}ms ({:.2} vectors/sec)",
        inserted,
        duration.as_millis(),
        inserted as f64 / duration.as_secs_f64()
    );

    // ===== Example 2: Parallel Batch Search =====
    println!("\n\nExample 2: Parallel Batch Search");
    println!("{}", "-".repeat(50));

    let num_queries = 100;
    println!("Preparing {} search queries...", num_queries);

    let mut search_requests = Vec::new();
    for i in 0..num_queries {
        search_requests.push(SearchRequest {
            collection: "documents".to_string(),
            query: vec![i as f32 * 0.001; 384],
            top_k: 10,
            score_threshold: Some(0.5),
            filter: None,
        });
    }

    // Sequential search (for comparison)
    let start = Instant::now();
    for (idx, request) in search_requests.iter().take(20).enumerate() {
        let _ = provider.search(request.clone()).await?;
        if idx == 19 {
            let duration = start.elapsed();
            println!(
                "Sequential: Executed 20 queries in {:.2}ms ({:.2} queries/sec)",
                duration.as_millis(),
                20.0 / duration.as_secs_f64()
            );
        }
    }

    // Parallel search
    let search_config = ParallelConfig {
        max_concurrent: 20,
        chunk_size: 10,
    };

    println!("\nParallel search configuration:");
    println!("  - max_concurrent: {}", search_config.max_concurrent);
    println!("  - chunk_size: {}", search_config.chunk_size);

    let start = Instant::now();
    let results = parallel_batch_search(
        Arc::clone(&provider),
        search_requests.clone(),
        search_config,
    )
    .await?;
    let duration = start.elapsed();

    println!(
        "\nParallel: Executed {} queries in {:.2}ms ({:.2} queries/sec)",
        results.len(),
        duration.as_millis(),
        results.len() as f64 / duration.as_secs_f64()
    );

    // Show some results
    println!("\nSample results from first query:");
    if let Some(first_results) = results.first() {
        for (i, result) in first_results.iter().take(3).enumerate() {
            println!("  {}. ID: {}, Score: {:.4}", i + 1, result.id, result.score);
        }
    }

    // ===== Example 3: Tuning Concurrency =====
    println!("\n\nExample 3: Performance Tuning");
    println!("{}", "-".repeat(50));
    println!("Testing different concurrency levels...\n");

    let test_requests: Vec<_> = search_requests.iter().take(50).cloned().collect();

    for &concurrency in &[1, 5, 10, 20, 50] {
        let config = ParallelConfig {
            max_concurrent: concurrency,
            chunk_size: 10,
        };

        let start = Instant::now();
        let results =
            parallel_batch_search(Arc::clone(&provider), test_requests.clone(), config).await?;
        let duration = start.elapsed();

        println!(
            "Concurrency {}: {:.2}ms ({:.2} queries/sec)",
            concurrency,
            duration.as_millis(),
            results.len() as f64 / duration.as_secs_f64()
        );
    }

    // ===== Best Practices =====
    println!("\n\n=== Best Practices ===");
    println!("{}", "-".repeat(50));
    println!("1. Start with max_concurrent = 10 and adjust based on your workload");
    println!("2. Larger chunk_size = fewer task spawns but less parallelism");
    println!("3. Smaller chunk_size = more parallelism but higher overhead");
    println!("4. For I/O-bound operations (network calls), higher concurrency helps");
    println!("5. For CPU-bound operations, concurrency = num_cpus is often optimal");
    println!("6. Always benchmark with your actual workload!");

    println!("\n=== Example Complete ===\n");

    Ok(())
}
