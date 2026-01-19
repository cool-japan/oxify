use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use oxify_connect_vector::{
    parallel::{parallel_batch_insert, parallel_batch_search, ParallelConfig},
    InsertRequest, MockVectorProvider, SearchRequest, VectorProvider,
};
use serde_json::json;
use std::hint::black_box;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn bench_parallel_insert(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("parallel_insert");

    for &size in &[100, 500, 1000] {
        for &concurrency in &[1, 5, 10, 20] {
            let provider = Arc::new(MockVectorProvider::new());
            rt.block_on(async {
                provider.create_collection("bench", 128).await.unwrap();
            });

            let mut requests = Vec::new();
            for i in 0..size {
                requests.push(InsertRequest {
                    collection: "bench".to_string(),
                    id: format!("doc_{}", i),
                    vector: vec![0.1; 128],
                    payload: json!({"index": i}),
                });
            }

            let config = ParallelConfig {
                max_concurrent: concurrency,
                chunk_size: 50,
            };

            group.bench_function(
                BenchmarkId::new(format!("size_{}", size), concurrency),
                |b| {
                    let provider = provider.clone();
                    let requests = requests.clone();
                    b.to_async(&rt).iter(|| {
                        let provider = provider.clone();
                        let requests = requests.clone();
                        async move {
                            black_box(
                                parallel_batch_insert(provider, requests, config)
                                    .await
                                    .unwrap(),
                            )
                        }
                    });
                },
            );
        }
    }

    group.finish();
}

fn bench_parallel_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("parallel_search");

    for &num_queries in &[10, 50, 100] {
        for &concurrency in &[1, 5, 10, 20] {
            let provider = Arc::new(MockVectorProvider::new());
            rt.block_on(async {
                provider.create_collection("bench", 128).await.unwrap();

                // Insert some test data
                for i in 0..1000 {
                    provider
                        .insert(InsertRequest {
                            collection: "bench".to_string(),
                            id: format!("doc_{}", i),
                            vector: vec![i as f32 * 0.001; 128],
                            payload: json!({"index": i}),
                        })
                        .await
                        .unwrap();
                }
            });

            let mut requests = Vec::new();
            for i in 0..num_queries {
                requests.push(SearchRequest {
                    collection: "bench".to_string(),
                    query: vec![i as f32 * 0.001; 128],
                    top_k: 10,
                    score_threshold: None,
                    filter: None,
                });
            }

            let config = ParallelConfig {
                max_concurrent: concurrency,
                chunk_size: 10,
            };

            group.bench_function(
                BenchmarkId::new(format!("queries_{}", num_queries), concurrency),
                |b| {
                    let provider = provider.clone();
                    let requests = requests.clone();
                    b.to_async(&rt).iter(|| {
                        let provider = provider.clone();
                        let requests = requests.clone();
                        async move {
                            black_box(
                                parallel_batch_search(provider, requests, config)
                                    .await
                                    .unwrap(),
                            )
                        }
                    });
                },
            );
        }
    }

    group.finish();
}

fn bench_sequential_vs_parallel(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("sequential_vs_parallel");

    let size = 500;
    let provider = Arc::new(MockVectorProvider::new());
    rt.block_on(async {
        provider.create_collection("bench", 128).await.unwrap();
    });

    let mut requests = Vec::new();
    for i in 0..size {
        requests.push(InsertRequest {
            collection: "bench".to_string(),
            id: format!("doc_{}", i),
            vector: vec![0.1; 128],
            payload: json!({"index": i}),
        });
    }

    // Sequential benchmark
    group.bench_function("sequential", |b| {
        let provider = provider.clone();
        let requests = requests.clone();
        b.to_async(&rt).iter(|| {
            let provider = provider.clone();
            let requests = requests.clone();
            async move {
                for request in requests {
                    let _: () = provider.insert(request).await.unwrap();
                    black_box(());
                }
            }
        });
    });

    // Parallel benchmark
    let config = ParallelConfig {
        max_concurrent: 10,
        chunk_size: 50,
    };

    group.bench_function("parallel_10", |b| {
        let provider = provider.clone();
        let requests = requests.clone();
        b.to_async(&rt).iter(|| {
            let provider = provider.clone();
            let requests = requests.clone();
            async move {
                black_box(
                    parallel_batch_insert(provider, requests, config)
                        .await
                        .unwrap(),
                )
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parallel_insert,
    bench_parallel_search,
    bench_sequential_vs_parallel
);
criterion_main!(benches);
