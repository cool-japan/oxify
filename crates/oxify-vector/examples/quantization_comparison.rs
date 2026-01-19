//! Quantization Comparison Example
//!
//! Demonstrates all quantization methods and their trade-offs:
//! - Binary (1-bit): 32x compression, lowest accuracy
//! - 4-bit: 8x compression, moderate accuracy
//! - Scalar (8-bit): 4x compression, good accuracy
//! - FP16 (16-bit): 2x compression, high accuracy (requires "fp16" feature)
//!
//! Run with: cargo run --example quantization_comparison
//! Run with FP16: cargo run --example quantization_comparison --features fp16

use oxify_vector::{
    BinaryQuantizationConfig, BinaryQuantizedIndex, FourBitQuantizedIndex, QuantizationConfig,
    QuantizedVectorIndex,
};
use std::time::Instant;

#[cfg(feature = "fp16")]
use oxify_vector::quantization::Fp16QuantizedIndex;

fn generate_random_vector(dim: usize, seed: usize) -> Vec<f32> {
    (0..dim)
        .map(|i| ((seed * 31 + i * 17) % 1000) as f32 / 1000.0)
        .collect()
}

fn main() -> anyhow::Result<()> {
    println!("=== Quantization Comparison Example ===\n");

    // Generate test dataset
    let num_vectors = 1000;
    let dimensions = 384;
    println!("1. Generating test dataset...");
    println!("   Vectors: {}", num_vectors);
    println!("   Dimensions: {}", dimensions);

    let vectors: Vec<(String, Vec<f32>)> = (0..num_vectors)
        .map(|i| (format!("doc_{}", i), generate_random_vector(dimensions, i)))
        .collect();

    let query = generate_random_vector(dimensions, 9999);
    let k = 10;
    println!();

    // Original (unquantized) baseline
    println!("2. Baseline (Original float32 vectors):");
    let original_bytes = (num_vectors * dimensions * 4) as f64 / 1_000_000.0;
    println!("   Memory: {:.2} MB", original_bytes);
    println!("   Compression: 1.0x (baseline)");
    println!("   Accuracy: 100% (perfect)");
    println!();

    // Binary quantization (1-bit)
    println!("3. Binary Quantization (1-bit):");
    let mut binary_index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());

    let start = Instant::now();
    binary_index.build(&vectors)?;
    let build_time = start.elapsed();

    let start = Instant::now();
    let binary_results = binary_index.search(&query, k)?;
    let search_time = start.elapsed();

    let binary_stats = binary_index.stats();
    println!(
        "   Memory: {:.2} MB",
        binary_stats.binary_bytes as f64 / 1_000_000.0
    );
    println!("   Compression: {:.1}x", binary_stats.compression_ratio);
    println!(
        "   Memory Savings: {:.1}%",
        binary_stats.memory_savings * 100.0
    );
    println!("   Build Time: {:?}", build_time);
    println!("   Search Time: {:?}", search_time);
    println!(
        "   Top result: {} (score: {:.4})",
        binary_results[0].0, binary_results[0].1
    );
    println!("   Use case: Extreme compression, first-stage filtering");
    println!();

    // 4-bit quantization
    println!("4. 4-bit Quantization:");
    let mut fourbit_index = FourBitQuantizedIndex::new();

    let start = Instant::now();
    fourbit_index.build(&vectors)?;
    let build_time = start.elapsed();

    let start = Instant::now();
    let fourbit_results = fourbit_index.search(&query, k)?;
    let search_time = start.elapsed();

    let fourbit_stats = fourbit_index.stats();
    println!(
        "   Memory: {:.2} MB",
        fourbit_stats.quantized_bytes as f64 / 1_000_000.0
    );
    println!("   Compression: {:.1}x", fourbit_stats.compression_ratio);
    println!(
        "   Memory Savings: {:.1}%",
        fourbit_stats.memory_savings * 100.0
    );
    println!("   Build Time: {:?}", build_time);
    println!("   Search Time: {:?}", search_time);
    println!(
        "   Top result: {} (score: {:.4})",
        fourbit_results[0].0, fourbit_results[0].1
    );
    println!("   Use case: Good balance between compression and accuracy");
    println!();

    // Scalar quantization (8-bit)
    println!("5. Scalar Quantization (8-bit):");
    let mut scalar_index = QuantizedVectorIndex::new(QuantizationConfig::default());

    let start = Instant::now();
    scalar_index.build(&vectors)?;
    let build_time = start.elapsed();

    let start = Instant::now();
    let scalar_results = scalar_index.search(&query, k)?;
    let search_time = start.elapsed();

    let scalar_stats = scalar_index.stats();
    println!(
        "   Memory: {:.2} MB",
        scalar_stats.quantized_bytes as f64 / 1_000_000.0
    );
    println!("   Compression: {:.1}x", scalar_stats.compression_ratio);
    println!(
        "   Memory Savings: {:.1}%",
        scalar_stats.memory_savings * 100.0
    );
    println!("   Build Time: {:?}", build_time);
    println!("   Search Time: {:?}", search_time);
    println!(
        "   Top result: {} (score: {:.4})",
        scalar_results[0].0, scalar_results[0].1
    );
    println!("   Use case: Standard quantization, good accuracy/compression");
    println!();

    // FP16 quantization (16-bit) - only if feature enabled
    #[cfg(feature = "fp16")]
    {
        println!("6. FP16 Quantization (16-bit):");
        let mut fp16_index = Fp16QuantizedIndex::new();

        let start = Instant::now();
        fp16_index.build(&vectors)?;
        let build_time = start.elapsed();

        let start = Instant::now();
        let fp16_results = fp16_index.search(&query, k)?;
        let search_time = start.elapsed();

        let fp16_stats = fp16_index.stats();
        println!(
            "   Memory: {:.2} MB",
            fp16_stats.fp16_bytes as f64 / 1_000_000.0
        );
        println!("   Compression: {:.1}x", fp16_stats.compression_ratio);
        println!(
            "   Memory Savings: {:.1}%",
            fp16_stats.memory_savings * 100.0
        );
        println!("   Build Time: {:?}", build_time);
        println!("   Search Time: {:?}", search_time);
        println!(
            "   Top result: {} (score: {:.4})",
            fp16_results[0].0, fp16_results[0].1
        );
        println!("   Use case: High accuracy with moderate compression");
        println!();
    }

    #[cfg(not(feature = "fp16"))]
    {
        println!("6. FP16 Quantization (16-bit): [SKIPPED]");
        println!("   (Enable with --features fp16)");
        println!();
    }

    // Comparison summary
    println!("=== Comparison Summary ===");
    println!();
    println!("Quantization Trade-offs:");
    println!("┌────────────┬─────────────┬─────────────┬──────────────┐");
    println!("│ Method     │ Compression │ Memory Save │ Use Case     │");
    println!("├────────────┼─────────────┼─────────────┼──────────────┤");
    println!("│ Binary     │ 32.0x       │ 96.9%       │ Max compress │");
    println!("│ 4-bit      │ 8.0x        │ 87.5%       │ Balanced     │");
    println!("│ 8-bit      │ 4.0x        │ 75.0%       │ Standard     │");
    #[cfg(feature = "fp16")]
    println!("│ FP16       │ 2.0x        │ 50.0%       │ High accuracy│");
    println!("│ float32    │ 1.0x        │ 0.0%        │ Perfect acc  │");
    println!("└────────────┴─────────────┴─────────────┴──────────────┘");
    println!();

    println!("Selection Guide:");
    println!("• Binary (1-bit):   Memory-constrained, first-stage filtering");
    println!("• 4-bit:            Good balance, moderate accuracy needs");
    println!("• Scalar (8-bit):   Production default, good accuracy");
    #[cfg(feature = "fp16")]
    println!("• FP16 (16-bit):    Minimal accuracy loss, modern hardware");
    println!("• float32:          Development, perfect accuracy required");
    println!();

    println!("Performance Notes:");
    println!("✓ Binary: Fastest search (Hamming distance)");
    println!("✓ 4-bit: Good speed with better accuracy than binary");
    println!("✓ 8-bit: Balanced speed and accuracy (most common)");
    #[cfg(feature = "fp16")]
    println!("✓ FP16: Hardware acceleration on modern CPUs/GPUs");
    println!("✓ float32: Baseline for quality comparison");

    Ok(())
}
