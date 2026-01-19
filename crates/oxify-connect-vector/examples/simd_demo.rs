//! SIMD-optimized vector operations demo
//!
//! This example demonstrates the performance benefits of SIMD-accelerated
//! vector operations. Run with `--features simd` to enable SIMD optimizations.
//!
//! ```bash
//! cargo run --example simd_demo --features simd
//! ```

use oxify_connect_vector::{
    cosine_similarity, cosine_similarity_optimized, dot_product, dot_product_optimized,
    euclidean_distance, euclidean_distance_optimized,
};
use std::time::Instant;

fn main() {
    println!("SIMD Vector Operations Demo");
    println!("============================\n");

    // Create test vectors (1024 dimensions)
    let dim = 1024;
    let a: Vec<f32> = (0..dim).map(|i| (i as f32) / 100.0).collect();
    let b: Vec<f32> = (0..dim).map(|i| (i as f32 + 1.0) / 100.0).collect();

    println!("Vector dimension: {}", dim);
    println!();

    // Benchmark cosine similarity
    println!("Cosine Similarity:");
    benchmark_function("  Standard", || cosine_similarity(&a, &b));
    benchmark_function("  Optimized (SIMD)", || cosine_similarity_optimized(&a, &b));
    println!();

    // Benchmark dot product
    println!("Dot Product:");
    benchmark_function("  Standard", || dot_product(&a, &b));
    benchmark_function("  Optimized (SIMD)", || dot_product_optimized(&a, &b));
    println!();

    // Benchmark Euclidean distance
    println!("Euclidean Distance:");
    benchmark_function("  Standard", || euclidean_distance(&a, &b));
    benchmark_function("  Optimized (SIMD)", || {
        euclidean_distance_optimized(&a, &b)
    });
    println!();

    #[cfg(feature = "simd")]
    {
        println!("✅ SIMD optimizations are ENABLED");
        println!("   You should see significant performance improvements above.");
    }

    #[cfg(not(feature = "simd"))]
    {
        println!("ℹ️  SIMD optimizations are DISABLED");
        println!("   Run with --features simd to enable SIMD acceleration.");
    }
}

fn benchmark_function<F, T>(name: &str, mut f: F)
where
    F: FnMut() -> T,
{
    // Warmup
    for _ in 0..100 {
        let _ = f();
    }

    // Benchmark
    let iterations = 10000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = f();
    }
    let elapsed = start.elapsed();

    let avg_nanos = elapsed.as_nanos() / iterations;
    println!("{}: {} ns/op", name, avg_nanos);
}
