//! Benchmarks for async optimization utilities
//!
//! Run with: cargo bench --bench async_bench

use oxify_server::async_optimization::{
    batch_futures, spawn_cpu_task, spawn_tracked, track_async, ASYNC_STATS,
};
use std::time::{Duration, Instant};

/// Benchmark CPU-intensive task spawning
async fn bench_spawn_cpu_task() {
    let start = Instant::now();
    let iterations = 100;

    let handles: Vec<_> = (0..iterations)
        .map(|_| {
            spawn_cpu_task(|| {
                let mut sum = 0u64;
                for i in 0..10000 {
                    sum = sum.wrapping_add(i);
                }
                sum
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.await;
    }

    let elapsed = start.elapsed();
    println!(
        "spawn_cpu_task: {} iterations in {:?} ({:.2} ops/sec)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
}

/// Benchmark tracked async execution
async fn bench_track_async() {
    let start = Instant::now();
    let iterations = 1000;

    for _ in 0..iterations {
        let _ = track_async(async {
            tokio::time::sleep(Duration::from_micros(10)).await;
            42
        })
        .await;
    }

    let elapsed = start.elapsed();
    println!(
        "track_async: {} iterations in {:?} ({:.2} ops/sec)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
}

/// Benchmark batch futures execution
async fn bench_batch_futures() {
    let start = Instant::now();
    let batch_size = 100;
    let batches = 10;

    for _ in 0..batches {
        let futures = (0..batch_size).map(|i| async move {
            tokio::time::sleep(Duration::from_micros(10)).await;
            i
        });
        let _results = batch_futures(futures.collect()).await;
    }

    let elapsed = start.elapsed();
    let total = batch_size * batches;
    println!(
        "batch_futures: {} futures in {} batches, {:?} ({:.2} ops/sec)",
        total,
        batches,
        elapsed,
        total as f64 / elapsed.as_secs_f64()
    );
}

/// Benchmark tracked task spawning
async fn bench_spawn_tracked() {
    let start = Instant::now();
    let iterations = 1000;

    let handles: Vec<_> = (0..iterations)
        .map(|_| {
            spawn_tracked(async {
                tokio::time::sleep(Duration::from_micros(10)).await;
                42
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.await;
    }

    let elapsed = start.elapsed();
    println!(
        "spawn_tracked: {} iterations in {:?} ({:.2} ops/sec)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
}

/// Benchmark async stats tracking overhead
async fn bench_stats_overhead() {
    ASYNC_STATS.reset();
    let start = Instant::now();
    let iterations = 100000;

    for _ in 0..iterations {
        ASYNC_STATS.record_spawn();
        ASYNC_STATS.record_completion(Duration::from_micros(10));
    }

    let elapsed = start.elapsed();
    let stats = ASYNC_STATS.stats();
    println!(
        "stats_overhead: {} operations in {:?} ({:.2} ops/sec)",
        iterations * 2, // spawn + completion
        elapsed,
        (iterations * 2) as f64 / elapsed.as_secs_f64()
    );
    println!(
        "  Stats: spawned={}, completed={}, avg={} us",
        stats.tasks_spawned, stats.tasks_completed, stats.avg_execution_time_us
    );
}

#[tokio::main]
async fn main() {
    println!("=== Async Optimization Benchmarks ===\n");

    println!("Running bench_spawn_cpu_task...");
    bench_spawn_cpu_task().await;
    println!();

    println!("Running bench_track_async...");
    bench_track_async().await;
    println!();

    println!("Running bench_batch_futures...");
    bench_batch_futures().await;
    println!();

    println!("Running bench_spawn_tracked...");
    bench_spawn_tracked().await;
    println!();

    println!("Running bench_stats_overhead...");
    bench_stats_overhead().await;
    println!();

    println!("=== Benchmarks Complete ===");
}
