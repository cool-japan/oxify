//! Query result caching example.
//!
//! This example demonstrates:
//! - Query result caching to avoid recomputation
//! - LRU eviction when cache is full
//! - TTL-based cache expiration
//! - Approximate query matching
//! - Cache statistics and monitoring

use oxify_vector::{
    CacheConfig, CacheStats, DistanceMetric, QueryCache, SearchConfig, VectorSearchIndex,
};
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    println!("=== Query Result Caching Example ===\n");

    // Generate sample dataset
    let num_vectors = 10_000;
    let dims = 768;

    println!("Dataset:");
    println!("  Vectors: {}", num_vectors);
    println!("  Dimensions: {}", dims);
    println!();

    let embeddings = generate_embeddings(num_vectors, dims);

    // Build search index
    let config = SearchConfig::default();
    let mut index = VectorSearchIndex::new(config);
    index.build(&embeddings)?;

    println!("Index built successfully");
    println!();

    // Example 1: Basic caching
    println!("1. Basic Query Caching");
    println!("   Creating cache with default config...");

    let cache_config = CacheConfig::default();
    let mut cache = QueryCache::new(cache_config);

    let query = generate_query(dims);

    // First search (cache miss)
    let start = Instant::now();
    let results = index.search(&query, 10)?;
    let search_time = start.elapsed();

    cache.put(&query, DistanceMetric::Cosine, 10, results.clone());

    println!("   First search (cache miss): {:.2?}", search_time);
    println!("   Results: {} documents", results.len());

    // Second search (cache hit)
    let start = Instant::now();
    let _cached_results = cache
        .get(&query, DistanceMetric::Cosine, 10)
        .expect("Should be cached");
    let cache_time = start.elapsed();

    println!("   Second search (cache hit): {:.2?}", cache_time);
    println!(
        "   Speedup: {:.1}x",
        search_time.as_nanos() as f64 / cache_time.as_nanos() as f64
    );

    let stats = cache.stats();
    print_cache_stats(&stats);
    println!();

    // Example 2: LRU eviction
    println!("2. LRU Eviction");
    println!("   Creating cache with max 3 entries...");

    let small_cache_config = CacheConfig {
        max_entries: 3,
        ..Default::default()
    };
    let mut small_cache = QueryCache::new(small_cache_config);

    // Add 5 queries (should evict oldest 2)
    for i in 0..5 {
        let q = generate_query(dims);
        let r = index.search(&q, 10)?;
        small_cache.put(&q, DistanceMetric::Cosine, 10, r);
        println!(
            "   Added query {}, cache size: {}",
            i + 1,
            small_cache.len()
        );
    }

    let stats = small_cache.stats();
    println!("   Total inserts: {}", stats.inserts);
    println!("   Total evictions: {}", stats.evictions);
    println!();

    // Example 3: TTL expiration
    println!("3. TTL-Based Expiration");
    println!("   Creating cache with 100ms TTL...");

    let ttl_config = CacheConfig {
        ttl: Duration::from_millis(100),
        ..Default::default()
    };
    let mut ttl_cache = QueryCache::new(ttl_config);

    let query = generate_query(dims);
    let results = index.search(&query, 10)?;
    ttl_cache.put(&query, DistanceMetric::Cosine, 10, results);

    // Immediately should be cached
    let cached = ttl_cache.get(&query, DistanceMetric::Cosine, 10);
    println!(
        "   Immediate lookup: {}",
        if cached.is_some() { "HIT" } else { "MISS" }
    );

    // Wait for expiration
    std::thread::sleep(Duration::from_millis(150));

    let cached = ttl_cache.get(&query, DistanceMetric::Cosine, 10);
    println!(
        "   After 150ms: {}",
        if cached.is_some() {
            "HIT"
        } else {
            "MISS (expired)"
        }
    );

    let stats = ttl_cache.stats();
    println!("   Expirations: {}", stats.expirations);
    println!();

    // Example 4: Approximate matching
    println!("4. Approximate Query Matching");
    println!("   Creating cache with 95% similarity threshold...");

    let approx_config = CacheConfig::high_hit_rate();
    let mut approx_cache = QueryCache::new(approx_config);

    let query1 = vec![1.0; dims];
    let results = index.search(&query1, 10)?;
    approx_cache.put(&query1, DistanceMetric::Cosine, 10, results);

    // Very similar query (should match approximately)
    let mut query2 = query1.clone();
    // Change ~5% of values slightly
    for value in query2.iter_mut().take(dims / 20) {
        *value *= 0.98;
    }

    let start = Instant::now();
    let _search_result = index.search(&query2, 10)?;
    let search_time = start.elapsed();

    let start = Instant::now();
    let cached = approx_cache.get(&query2, DistanceMetric::Cosine, 10);
    let cache_time = start.elapsed();

    println!("   Search time: {:.2?}", search_time);
    println!("   Cache lookup: {:.2?}", cache_time);
    println!(
        "   Approximate match: {}",
        if cached.is_some() { "HIT" } else { "MISS" }
    );

    if cached.is_some() {
        println!(
            "   Speedup: {:.1}x",
            search_time.as_nanos() as f64 / cache_time.as_nanos() as f64
        );
    }
    println!();

    // Example 5: Performance comparison with repeated queries
    println!("5. Performance Impact on Repeated Queries");
    println!("   Simulating 100 queries (50% repeat rate)...");

    let perf_config = CacheConfig::default();
    let mut perf_cache = QueryCache::new(perf_config);

    let unique_queries: Vec<Vec<f32>> = (0..50).map(|_| generate_query(dims)).collect();

    let mut total_search_time = Duration::ZERO;
    let mut total_cache_time = Duration::ZERO;

    for i in 0..100 {
        // 50% chance to reuse a query
        let query = if i % 2 == 0 {
            &unique_queries[i / 2]
        } else {
            &unique_queries[(i / 2) % unique_queries.len()]
        };

        // Check cache first
        let cache_start = Instant::now();
        let cached = perf_cache.get(query, DistanceMetric::Cosine, 10);
        total_cache_time += cache_start.elapsed();

        if cached.is_none() {
            // Cache miss - perform search
            let search_start = Instant::now();
            let results = index.search(query, 10)?;
            total_search_time += search_start.elapsed();

            perf_cache.put(query, DistanceMetric::Cosine, 10, results);
        }
    }

    let stats = perf_cache.stats();
    println!("   Queries executed: 100");
    println!("   Cache hits: {}", stats.hits);
    println!("   Cache misses: {}", stats.misses);
    println!("   Hit rate: {:.1}%", stats.hit_rate());
    println!("   Total search time: {:.2?}", total_search_time);
    println!("   Total cache time: {:.2?}", total_cache_time);
    println!(
        "   Average search time: {:.2?}",
        total_search_time / stats.misses as u32
    );
    println!(
        "   Average cache time: {:.2?}",
        total_cache_time / (stats.hits + stats.misses) as u32
    );
    println!();

    // Example 6: Cache configuration presets
    println!("6. Cache Configuration Presets");
    println!();

    println!("   Default config:");
    let default_config = CacheConfig::default();
    print_config(&default_config);

    println!("   High hit rate config:");
    let high_hit_config = CacheConfig::high_hit_rate();
    print_config(&high_hit_config);

    println!("   Low memory config:");
    let low_mem_config = CacheConfig::low_memory();
    print_config(&low_mem_config);

    println!("   Exact match only config:");
    let exact_config = CacheConfig::exact_match_only();
    print_config(&exact_config);

    // Summary
    println!("=== Summary ===");
    println!();
    println!("Query caching is highly effective for:");
    println!("  - Applications with repeated queries");
    println!("  - High QPS scenarios with query patterns");
    println!("  - RAG systems with common user questions");
    println!("  - Reducing latency for frequently accessed data");
    println!();
    println!("Configuration tips:");
    println!("  - Use high_hit_rate() for read-heavy workloads");
    println!("  - Use low_memory() for constrained environments");
    println!("  - Enable approximate matching for similar queries");
    println!("  - Adjust TTL based on data freshness requirements");

    Ok(())
}

fn generate_embeddings(count: usize, dimensions: usize) -> HashMap<String, Vec<f32>> {
    let mut rng = rand::rng();
    let mut embeddings = HashMap::new();

    for i in 0..count {
        let vec: Vec<f32> = (0..dimensions)
            .map(|_| rng.random_range(0.0..1.0))
            .collect();
        embeddings.insert(format!("doc_{}", i), vec);
    }

    embeddings
}

fn generate_query(dimensions: usize) -> Vec<f32> {
    let mut rng = rand::rng();
    (0..dimensions)
        .map(|_| rng.random_range(0.0..1.0))
        .collect()
}

fn print_cache_stats(stats: &CacheStats) {
    println!("   Cache Statistics:");
    println!("     Hits: {}", stats.hits);
    println!("     Misses: {}", stats.misses);
    println!("     Hit rate: {:.1}%", stats.hit_rate());
    println!("     Inserts: {}", stats.inserts);
    println!("     Evictions: {}", stats.evictions);
    println!("     Expirations: {}", stats.expirations);
}

fn print_config(config: &CacheConfig) {
    println!("     Max entries: {}", config.max_entries);
    println!("     TTL: {:?}", config.ttl);
    println!(
        "     Similarity threshold: {:.2}",
        config.similarity_threshold
    );
    println!(
        "     Approximate matching: {}",
        config.enable_approximate_matching
    );
    println!();
}
