use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_connect_vector::{InsertRequest, MockVectorProvider, SearchRequest, VectorProvider};
use serde_json::json;
use std::hint::black_box;
use tokio::runtime::Runtime;

/// Benchmark search latency with different collection sizes
fn bench_search_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("search_latency");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let provider = MockVectorProvider::new();

            // Setup: create collection and insert vectors
            rt.block_on(async {
                provider.create_collection("test", 128).await.unwrap();

                for i in 0..size {
                    let vector = (0..128).map(|j| ((i + j) as f32) / 100.0).collect();
                    provider
                        .insert(InsertRequest {
                            collection: "test".to_string(),
                            id: format!("vec_{}", i),
                            vector,
                            payload: json!({"index": i}),
                        })
                        .await
                        .unwrap();
                }
            });

            // Benchmark: search
            b.to_async(&rt).iter(|| async {
                let query: Vec<f32> = (0..128).map(|i| (i as f32) / 100.0).collect();
                let results = provider
                    .search(SearchRequest {
                        collection: "test".to_string(),
                        query: black_box(query),
                        top_k: 10,
                        score_threshold: None,
                        filter: None,
                    })
                    .await
                    .unwrap();
                black_box(results);
            });
        });
    }

    group.finish();
}

/// Benchmark throughput (queries per second)
fn bench_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("throughput");

    let provider = MockVectorProvider::new();

    // Setup: create collection with 1000 vectors
    rt.block_on(async {
        provider.create_collection("test", 128).await.unwrap();

        for i in 0..1000 {
            let vector = (0..128).map(|j| ((i + j) as f32) / 100.0).collect();
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("vec_{}", i),
                    vector,
                    payload: json!({"index": i}),
                })
                .await
                .unwrap();
        }
    });

    group.throughput(Throughput::Elements(1));
    group.bench_function("queries_per_second", |b| {
        b.to_async(&rt).iter(|| async {
            let query: Vec<f32> = (0..128).map(|i| (i as f32) / 100.0).collect();
            let results = provider
                .search(SearchRequest {
                    collection: "test".to_string(),
                    query: black_box(query),
                    top_k: 10,
                    score_threshold: None,
                    filter: None,
                })
                .await
                .unwrap();
            black_box(results);
        });
    });

    group.finish();
}

/// Benchmark insert throughput
fn bench_insert_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("insert_throughput");

    group.throughput(Throughput::Elements(1));
    group.bench_function("inserts_per_second", |b| {
        let provider = MockVectorProvider::new();

        rt.block_on(async {
            provider.create_collection("test", 128).await.unwrap();
        });

        let mut counter = 0;

        b.to_async(&rt).iter(|| {
            let provider = provider.clone();
            async move {
                let vector = (0..128).map(|i| (i as f32) / 100.0).collect();
                counter += 1;
                provider
                    .insert(InsertRequest {
                        collection: "test".to_string(),
                        id: format!("vec_{}", counter),
                        vector: black_box(vector),
                        payload: json!({"index": counter}),
                    })
                    .await
                    .unwrap();
            }
        });
    });

    group.finish();
}

/// Benchmark accuracy metrics (recall@k)
fn bench_accuracy(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("accuracy");

    let provider = MockVectorProvider::new();

    // Setup: create collection with clustered vectors
    rt.block_on(async {
        provider.create_collection("test", 128).await.unwrap();

        // Create 3 clusters of vectors
        for cluster in 0..3 {
            for i in 0..100 {
                let base = cluster as f32 * 10.0;
                let vector = (0..128)
                    .map(|j| base + (i as f32 + j as f32) / 100.0)
                    .collect();
                provider
                    .insert(InsertRequest {
                        collection: "test".to_string(),
                        id: format!("cluster_{}_vec_{}", cluster, i),
                        vector,
                        payload: json!({"cluster": cluster}),
                    })
                    .await
                    .unwrap();
            }
        }
    });

    for k in [1, 5, 10, 20].iter() {
        group.bench_with_input(BenchmarkId::new("recall", k), k, |b, &k| {
            b.to_async(&rt).iter(|| async {
                // Query with a vector from cluster 0
                let query: Vec<f32> = (0..128).map(|i| (i as f32) / 100.0).collect();
                let results = provider
                    .search(SearchRequest {
                        collection: "test".to_string(),
                        query: black_box(query),
                        top_k: k,
                        score_threshold: None,
                        filter: None,
                    })
                    .await
                    .unwrap();

                // Calculate recall: how many results are from cluster 0
                let relevant = results
                    .iter()
                    .filter(|r| r.payload["cluster"].as_u64() == Some(0))
                    .count();

                let recall = relevant as f64 / k as f64;
                black_box(recall);
            });
        });
    }

    group.finish();
}

/// Benchmark different vector dimensions
fn bench_dimensions(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("dimensions");

    for dim in [64, 128, 256, 512, 1024].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(dim), dim, |b, &dim| {
            let provider = MockVectorProvider::new();

            rt.block_on(async {
                provider.create_collection("test", dim).await.unwrap();

                // Insert 100 vectors
                for i in 0..100 {
                    let vector = (0..dim).map(|j| ((i + j) as f32) / 100.0).collect();
                    provider
                        .insert(InsertRequest {
                            collection: "test".to_string(),
                            id: format!("vec_{}", i),
                            vector,
                            payload: json!({"index": i}),
                        })
                        .await
                        .unwrap();
                }
            });

            b.to_async(&rt).iter(|| async {
                let query: Vec<f32> = (0..dim).map(|i| (i as f32) / 100.0).collect();
                let results = provider
                    .search(SearchRequest {
                        collection: "test".to_string(),
                        query: black_box(query),
                        top_k: 10,
                        score_threshold: None,
                        filter: None,
                    })
                    .await
                    .unwrap();
                black_box(results);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_search_latency,
    bench_throughput,
    bench_insert_throughput,
    bench_accuracy,
    bench_dimensions
);
criterion_main!(benches);
