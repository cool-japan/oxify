//! Benchmarks for HTTP response caching performance.
//!
//! These benchmarks measure the performance improvement from caching.

use oxify_server::cache::{CacheConfig, CacheEntry, LruCache, ResponseCache};
use std::time::{Duration, Instant};

/// Simulate expensive computation (e.g., database query, API call)
fn expensive_operation() -> Vec<u8> {
    // Simulate 10ms latency
    std::thread::sleep(Duration::from_millis(10));
    b"expensive result".to_vec()
}

/// Benchmark LRU cache operations
fn bench_lru_cache() {
    let mut cache = LruCache::new(1000);

    let entry = CacheEntry {
        body: b"test".to_vec(),
        status: axum::http::StatusCode::OK,
        headers: vec![],
        etag: "etag".to_string(),
        created_at: Instant::now(),
        ttl: Duration::from_secs(60),
        hit_count: 0,
    };

    // Benchmark insertions
    let start = Instant::now();
    for i in 0..1000 {
        cache.insert(format!("key{}", i), entry.clone());
    }
    let insert_duration = start.elapsed();
    println!("LRU Cache: 1000 insertions in {:?}", insert_duration);

    // Benchmark lookups (cache hits)
    let start = Instant::now();
    for i in 0..1000 {
        cache.get(&format!("key{}", i));
    }
    let lookup_duration = start.elapsed();
    println!("LRU Cache: 1000 lookups in {:?}", lookup_duration);

    // Calculate ops/sec
    let ops_per_sec = 1000.0 / lookup_duration.as_secs_f64();
    println!("LRU Cache: {:.0} ops/sec", ops_per_sec);
}

/// Benchmark cache hit vs miss performance
fn bench_cache_hit_vs_miss() {
    let config = CacheConfig::default();
    let cache = ResponseCache::new(config);

    // Warm up cache
    let entry = CacheEntry {
        body: expensive_operation(),
        status: axum::http::StatusCode::OK,
        headers: vec![],
        etag: "etag1".to_string(),
        created_at: Instant::now(),
        ttl: Duration::from_secs(60),
        hit_count: 0,
    };
    cache.insert("/api/data".to_string(), entry);

    // Benchmark cache miss (with expensive operation)
    let start = Instant::now();
    let _result = expensive_operation();
    let miss_duration = start.elapsed();
    println!("Cache MISS (with expensive op): {:?}", miss_duration);

    // Benchmark cache hit (no expensive operation)
    let start = Instant::now();
    let _cached = cache.get("/api/data");
    let hit_duration = start.elapsed();
    println!("Cache HIT: {:?}", hit_duration);

    // Calculate speedup
    let speedup = miss_duration.as_micros() as f64 / hit_duration.as_micros() as f64;
    println!("Cache speedup: {:.1}x faster", speedup);
}

/// Benchmark cache statistics tracking
fn bench_cache_stats() {
    let config = CacheConfig::default();
    let cache = ResponseCache::new(config);

    let entry = CacheEntry {
        body: b"test".to_vec(),
        status: axum::http::StatusCode::OK,
        headers: vec![],
        etag: "etag".to_string(),
        created_at: Instant::now(),
        ttl: Duration::from_secs(60),
        hit_count: 0,
    };

    // Insert 100 entries
    for i in 0..100 {
        cache.insert(format!("/api/data{}", i), entry.clone());
    }

    // Perform 1000 lookups (mix of hits and misses)
    let start = Instant::now();
    for i in 0..1000 {
        cache.get(&format!("/api/data{}", i % 150)); // 66% hit rate
    }
    let duration = start.elapsed();

    if let Some(stats) = cache.stats() {
        println!("Cache Stats:");
        println!("  Hits: {}", stats.hits);
        println!("  Misses: {}", stats.misses);
        println!("  Hit Rate: {:.1}%", stats.hit_rate() * 100.0);
        println!("  Duration: {:?}", duration);
    }
}

fn main() {
    println!("=== HTTP Response Caching Benchmarks ===\n");

    println!("--- LRU Cache Operations ---");
    bench_lru_cache();
    println!();

    println!("--- Cache Hit vs Miss Performance ---");
    bench_cache_hit_vs_miss();
    println!();

    println!("--- Cache Statistics ---");
    bench_cache_stats();
    println!();

    println!("=== Benchmarks Complete ===");
}
