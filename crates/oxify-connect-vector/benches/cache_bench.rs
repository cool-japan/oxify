use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_connect_vector::{EmbeddingCache, SearchCache, SearchResult};
use serde_json::json;
use std::hint::black_box;

/// Benchmark embedding cache hit performance
fn bench_cache_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_hit");

    let cache = EmbeddingCache::new(1000);

    // Pre-populate cache
    for i in 0..1000 {
        let text = format!("text_{}", i);
        let embedding = vec![i as f32; 128];
        cache.insert(&text, "model", embedding);
    }

    group.throughput(Throughput::Elements(1));
    group.bench_function("embedding_cache_hit", |b| {
        b.iter(|| {
            let result = cache.get(black_box("text_500"), black_box("model"));
            black_box(result);
        });
    });

    group.finish();
}

/// Benchmark embedding cache miss performance
fn bench_cache_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_miss");

    let cache = EmbeddingCache::new(1000);

    group.throughput(Throughput::Elements(1));
    group.bench_function("embedding_cache_miss", |b| {
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let result = cache.get(black_box(&format!("text_{}", counter)), black_box("model"));
            black_box(result);
        });
    });

    group.finish();
}

/// Benchmark cache insertion
fn bench_cache_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_insert");

    group.throughput(Throughput::Elements(1));
    group.bench_function("embedding_cache_insert", |b| {
        let cache = EmbeddingCache::new(10000);
        let mut counter = 0;

        b.iter(|| {
            counter += 1;
            let text = format!("text_{}", counter);
            let embedding = vec![counter as f32; 128];
            cache.insert(black_box(&text), black_box("model"), black_box(embedding));
        });
    });

    group.finish();
}

/// Benchmark cache eviction (LRU)
fn bench_cache_eviction(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_eviction");

    for capacity in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(capacity),
            capacity,
            |b, &capacity| {
                b.iter(|| {
                    let cache = EmbeddingCache::new(capacity);

                    // Fill cache beyond capacity to trigger eviction
                    for i in 0..(capacity * 2) {
                        let text = format!("text_{}", i);
                        let embedding = vec![i as f32; 128];
                        cache.insert(&text, "model", embedding);
                    }

                    black_box(cache);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark search cache performance
fn bench_search_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_cache");

    let cache = SearchCache::new(1000);

    // Pre-populate cache
    for i in 0..1000 {
        let collection = "test".to_string();
        let query = vec![i as f32; 128];
        let results = vec![SearchResult {
            id: format!("doc_{}", i),
            score: 0.9,
            payload: json!({}),
            vector: Some(vec![i as f32; 128]),
        }];
        cache.insert(&collection, &query, 10, results);
    }

    group.throughput(Throughput::Elements(1));
    group.bench_function("search_cache_hit", |b| {
        b.iter(|| {
            let query = vec![500.0; 128];
            let result = cache.get(black_box("test"), black_box(&query), black_box(10));
            black_box(result);
        });
    });

    group.finish();
}

/// Benchmark cache with different sizes
fn bench_cache_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_sizes");

    for capacity in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(capacity),
            capacity,
            |b, &capacity| {
                let cache = EmbeddingCache::new(capacity);

                // Pre-populate half the cache
                for i in 0..(capacity / 2) {
                    let text = format!("text_{}", i);
                    let embedding = vec![i as f32; 128];
                    cache.insert(&text, "model", embedding);
                }

                b.iter(|| {
                    // Mix of hits and misses
                    let text = format!("text_{}", black_box(capacity / 4));
                    let result = cache.get(black_box(&text), black_box("model"));
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark cache stats computation
fn bench_cache_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_stats");

    let cache = EmbeddingCache::new(1000);

    // Populate and access cache to generate stats
    for i in 0..1000 {
        let text = format!("text_{}", i);
        let embedding = vec![i as f32; 128];
        cache.insert(&text, "model", embedding);
    }

    for i in 0..500 {
        let text = format!("text_{}", i);
        cache.get(&text, "model");
    }

    group.throughput(Throughput::Elements(1));
    group.bench_function("get_stats", |b| {
        b.iter(|| {
            let stats = cache.stats();
            black_box(stats);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cache_hit,
    bench_cache_miss,
    bench_cache_insert,
    bench_cache_eviction,
    bench_search_cache,
    bench_cache_sizes,
    bench_cache_stats
);
criterion_main!(benches);
