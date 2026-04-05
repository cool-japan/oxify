//! GPU-accelerated batch vector search example.
//!
//! This example demonstrates:
//! - GPU configuration for batch processing
//! - Automatic CPU/GPU dispatch based on batch size
//! - Performance comparison between CPU and GPU modes
//! - Batch distance calculation

use oxify_vector::{DistanceMetric, GpuBatchProcessor, GpuConfig};

fn main() -> anyhow::Result<()> {
    println!("=== GPU-Accelerated Batch Vector Search ===\n");

    // Generate sample data
    let num_vectors = 1000;
    let num_queries = 100;
    let dims = 768;

    println!("Dataset:");
    println!("  Vectors: {}", num_vectors);
    println!("  Queries: {}", num_queries);
    println!("  Dimensions: {}", dims);
    println!();

    let vectors = generate_vectors(num_vectors, dims);
    let queries = generate_vectors(num_queries, dims);

    // Example 1: CPU-preferred configuration (small batches)
    println!("1. CPU-Preferred Configuration");
    println!("   - Forces CPU execution for all batch sizes");
    let cpu_config = GpuConfig::cpu_preferred();
    let cpu_processor = GpuBatchProcessor::new(cpu_config)?;

    println!("   GPU Available: {}", cpu_processor.is_gpu_available());

    let start = std::time::Instant::now();
    let cpu_distances = cpu_processor.batch_distance(&queries, &vectors, DistanceMetric::Cosine)?;
    let cpu_time = start.elapsed();

    println!(
        "   Result shape: {} x {}",
        cpu_distances.len(),
        cpu_distances[0].len()
    );
    println!("   Time: {:.2?}", cpu_time);
    println!();

    // Example 2: GPU-preferred configuration (large batches)
    println!("2. GPU-Preferred Configuration");
    println!("   - Uses GPU for batches >= 10 operations");
    let gpu_config = GpuConfig::gpu_preferred();
    let gpu_processor = GpuBatchProcessor::new(gpu_config)?;

    println!("   GPU Available: {}", gpu_processor.is_gpu_available());

    let start = std::time::Instant::now();
    let gpu_distances = gpu_processor.batch_distance(&queries, &vectors, DistanceMetric::Cosine)?;
    let gpu_time = start.elapsed();

    println!(
        "   Result shape: {} x {}",
        gpu_distances.len(),
        gpu_distances[0].len()
    );
    println!("   Time: {:.2?}", gpu_time);

    #[cfg(feature = "cuda")]
    {
        let speedup = cpu_time.as_secs_f64() / gpu_time.as_secs_f64();
        println!("   Speedup: {:.2}x", speedup);
    }

    #[cfg(not(feature = "cuda"))]
    {
        println!("   Note: CUDA feature not enabled, using CPU fallback");
    }
    println!();

    // Example 3: Custom configuration with threshold
    println!("3. Custom Configuration");
    println!("   - GPU threshold: 1000 operations");
    let custom_config = GpuConfig {
        min_batch_size_for_gpu: 1000,
        enabled: true,
        device_id: 0,
        max_batch_size: 50_000,
    };
    let custom_processor = GpuBatchProcessor::new(custom_config)?;

    println!("   Configuration:");
    println!("     Min batch size for GPU: 1000");
    println!("     Max batch size: 50,000");

    // Small batch (should use CPU)
    let small_queries = &queries[..10];
    let start = std::time::Instant::now();
    let small_distances =
        custom_processor.batch_distance(small_queries, &vectors, DistanceMetric::Cosine)?;
    let small_time = start.elapsed();

    println!("   Small batch (10 queries):");
    println!(
        "     Result shape: {} x {}",
        small_distances.len(),
        small_distances[0].len()
    );
    println!("     Time: {:.2?}", small_time);
    println!("     Used: CPU (below threshold)");

    // Large batch (should use GPU if available)
    let start = std::time::Instant::now();
    let large_distances =
        custom_processor.batch_distance(&queries, &vectors, DistanceMetric::Cosine)?;
    let large_time = start.elapsed();

    println!("   Large batch (100 queries):");
    println!(
        "     Result shape: {} x {}",
        large_distances.len(),
        large_distances[0].len()
    );
    println!("     Time: {:.2?}", large_time);

    #[cfg(feature = "cuda")]
    println!("     Used: GPU (above threshold)");

    #[cfg(not(feature = "cuda"))]
    println!("     Used: CPU (CUDA not enabled)");
    println!();

    // Example 4: Different distance metrics
    println!("4. Distance Metrics Comparison");
    let test_queries = &queries[..10];
    let test_vectors = &vectors[..100];

    let metrics = vec![
        DistanceMetric::Cosine,
        DistanceMetric::Euclidean,
        DistanceMetric::DotProduct,
        DistanceMetric::Manhattan,
    ];

    for metric in metrics {
        let start = std::time::Instant::now();
        let distances = cpu_processor.batch_distance(test_queries, test_vectors, metric)?;
        let time = start.elapsed();

        println!("   {:?}:", metric);
        println!("     Time: {:.2?}", time);
        println!("     Sample distance: {:.4}", distances[0][0]);
    }
    println!();

    // Summary
    println!("=== Summary ===");
    println!();
    println!("GPU acceleration provides significant speedup for large batch queries.");
    println!("Key benefits:");
    println!("  - Automatic CPU/GPU dispatch based on batch size");
    println!("  - Seamless fallback to CPU when GPU unavailable");
    println!("  - Support for all distance metrics");
    println!("  - Configurable thresholds for optimal performance");
    println!();

    #[cfg(not(feature = "cuda"))]
    {
        println!("To enable GPU acceleration:");
        println!("  1. Install CUDA toolkit (11.0 or later)");
        println!("  2. Build with: cargo build --features cuda");
        println!("  3. Run with: cargo run --features cuda --example gpu_acceleration");
    }

    Ok(())
}

fn generate_vectors(count: usize, dims: usize) -> Vec<Vec<f32>> {
    use rand::{Rng, RngExt};
    let mut rng = rand::rng();

    (0..count)
        .map(|_| (0..dims).map(|_| rng.random_range(0.0..1.0)).collect())
        .collect()
}
