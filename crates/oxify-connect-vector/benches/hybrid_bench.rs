use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_connect_vector::{
    Bm25Document, Bm25Index, HybridSearchEngine, HybridSearchParams, InsertRequest,
    MockVectorProvider, VectorProvider,
};
use serde_json::json;
use std::hint::black_box;
use tokio::runtime::Runtime;

/// Benchmark hybrid search performance
fn bench_hybrid_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("hybrid_search");

    for size in [100, 500, 1000].iter() {
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
                            id: format!("doc_{}", i),
                            vector,
                            payload: json!({
                                "text": format!("document number {} with some sample text", i)
                            }),
                        })
                        .await
                        .unwrap();
                }
            });

            // Build BM25 documents
            let mut bm25_docs = Vec::new();
            for i in 0..size {
                let doc = Bm25Document {
                    id: format!("doc_{}", i),
                    text: format!("document number {} with some sample text", i),
                    metadata: json!({}),
                };
                bm25_docs.push(doc);
            }

            let engine = HybridSearchEngine::new(
                provider.clone(),
                bm25_docs,
                HybridSearchParams {
                    semantic_weight: 0.7,
                    keyword_weight: 0.3,
                    rrf_k: 60.0,
                },
            );

            // Benchmark: hybrid search
            b.to_async(&rt).iter(|| async {
                let query_vector: Vec<f32> = (0..128).map(|i| (i as f32) / 100.0).collect();
                let results = engine
                    .search(
                        black_box(oxify_connect_vector::SearchRequest {
                            collection: "test".to_string(),
                            query: query_vector,
                            top_k: 10,
                            score_threshold: None,
                            filter: None,
                        }),
                        black_box("document sample"),
                        10,
                    )
                    .await
                    .unwrap();
                black_box(results);
            });
        });
    }

    group.finish();
}

/// Benchmark BM25 search performance
fn bench_bm25_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("bm25_search");

    for size in [100, 500, 1000, 5000].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            // Build index
            let mut docs = Vec::new();
            for i in 0..size {
                let doc = Bm25Document {
                    id: format!("doc_{}", i),
                    text: format!(
                        "document number {} with some sample text about topic {}",
                        i,
                        i % 10
                    ),
                    metadata: json!({}),
                };
                docs.push(doc);
            }
            let bm25_index = Bm25Index::new(docs);

            // Benchmark: search
            b.iter(|| {
                let results = bm25_index.search(black_box("document sample topic"), black_box(10));
                black_box(results);
            });
        });
    }

    group.finish();
}

/// Benchmark BM25 index building
fn bench_bm25_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("bm25_build");

    for size in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let mut docs = Vec::new();
                for i in 0..size {
                    let doc = Bm25Document {
                        id: format!("doc_{}", i),
                        text: format!("document number {} with some sample text", i),
                        metadata: json!({}),
                    };
                    docs.push(doc);
                }
                let bm25_index = Bm25Index::new(docs);
                black_box(bm25_index);
            });
        });
    }

    group.finish();
}

/// Benchmark different semantic/keyword weight combinations
fn bench_fusion_weights(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("fusion_weights");

    let provider = MockVectorProvider::new();

    // Setup
    rt.block_on(async {
        provider.create_collection("test", 128).await.unwrap();

        for i in 0..500 {
            let vector = (0..128).map(|j| ((i + j) as f32) / 100.0).collect();
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("doc_{}", i),
                    vector,
                    payload: json!({
                        "text": format!("document number {} with some sample text", i)
                    }),
                })
                .await
                .unwrap();
        }
    });

    let mut bm25_docs = Vec::new();
    for i in 0..500 {
        let doc = Bm25Document {
            id: format!("doc_{}", i),
            text: format!("document number {} with some sample text", i),
            metadata: json!({}),
        };
        bm25_docs.push(doc);
    }

    for &semantic_weight in [0.0, 0.3, 0.5, 0.7, 1.0].iter() {
        let keyword_weight = 1.0 - semantic_weight;
        let label = format!("s{:.1}_k{:.1}", semantic_weight, keyword_weight);

        group.bench_with_input(BenchmarkId::from_parameter(&label), &label, |b, _| {
            let engine = HybridSearchEngine::new(
                provider.clone(),
                bm25_docs.clone(),
                HybridSearchParams {
                    semantic_weight,
                    keyword_weight,
                    rrf_k: 60.0,
                },
            );

            b.to_async(&rt).iter(|| async {
                let query_vector: Vec<f32> = (0..128).map(|i| (i as f32) / 100.0).collect();
                let results = engine
                    .search(
                        black_box(oxify_connect_vector::SearchRequest {
                            collection: "test".to_string(),
                            query: query_vector,
                            top_k: 10,
                            score_threshold: None,
                            filter: None,
                        }),
                        black_box("document sample"),
                        10,
                    )
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
    bench_hybrid_search,
    bench_bm25_search,
    bench_bm25_build,
    bench_fusion_weights
);
criterion_main!(benches);
