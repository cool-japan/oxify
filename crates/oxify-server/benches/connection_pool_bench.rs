//! Benchmarks for connection pooling
//!
//! Run with: cargo bench --bench connection_pool_bench

use oxify_server::connection_pool::{
    ConnectionMetadata, DbPoolConfig, Http2PoolConfig, PoolStatsTracker, RedisPoolConfig,
};
use std::time::Instant;

/// Benchmark pool stats tracker performance
fn bench_pool_stats() {
    let tracker = PoolStatsTracker::new();
    tracker.reset();

    let start = Instant::now();
    let iterations = 100000;

    for _ in 0..iterations {
        tracker.record_create();
        tracker.record_return();
        tracker.record_reuse();
        tracker.record_close();
    }

    let elapsed = start.elapsed();
    let stats = tracker.stats();

    println!(
        "pool_stats: {} operations in {:?} ({:.2} ops/sec)",
        iterations * 4, // create + return + reuse + close
        elapsed,
        (iterations * 4) as f64 / elapsed.as_secs_f64()
    );
    println!(
        "  Stats: created={}, reused={}, closed={}, active={}, idle={}",
        stats.connections_created,
        stats.connections_reused,
        stats.connections_closed,
        stats.active_connections,
        stats.idle_connections
    );
}

/// Benchmark connection metadata tracking
fn bench_connection_metadata() {
    let start = Instant::now();
    let iterations = 100000;

    for i in 0..iterations {
        let mut metadata = ConnectionMetadata::new(format!("conn-{}", i));
        metadata.mark_used();
        let _ = metadata.age();
        let _ = metadata.idle_time();
    }

    let elapsed = start.elapsed();
    println!(
        "connection_metadata: {} connections in {:?} ({:.2} ops/sec)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
}

/// Benchmark HTTP/2 pool config creation
fn bench_http2_config() {
    let start = Instant::now();
    let iterations = 100000;

    for _ in 0..iterations {
        let _ = Http2PoolConfig::new()
            .with_max_idle_per_host(64)
            .with_max_concurrent_streams(200)
            .with_adaptive_window(true);
    }

    let elapsed = start.elapsed();
    println!(
        "http2_config: {} configs created in {:?} ({:.2} ops/sec)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
}

/// Benchmark database pool config creation
fn bench_db_config() {
    let start = Instant::now();
    let iterations = 100000;

    for _ in 0..iterations {
        let _ = DbPoolConfig::new()
            .with_max_connections(200)
            .with_min_idle(20);
    }

    let elapsed = start.elapsed();
    println!(
        "db_config: {} configs created in {:?} ({:.2} ops/sec)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
}

/// Benchmark Redis pool config creation
fn bench_redis_config() {
    let start = Instant::now();
    let iterations = 100000;

    for _ in 0..iterations {
        let _ = RedisPoolConfig::new()
            .with_max_connections(100)
            .with_min_idle(10);
    }

    let elapsed = start.elapsed();
    println!(
        "redis_config: {} configs created in {:?} ({:.2} ops/sec)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
}

fn main() {
    println!("=== Connection Pool Benchmarks ===\n");

    println!("Running bench_pool_stats...");
    bench_pool_stats();
    println!();

    println!("Running bench_connection_metadata...");
    bench_connection_metadata();
    println!();

    println!("Running bench_http2_config...");
    bench_http2_config();
    println!();

    println!("Running bench_db_config...");
    bench_db_config();
    println!();

    println!("Running bench_redis_config...");
    bench_redis_config();
    println!();

    println!("=== Benchmarks Complete ===");
}
