use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use oxify_connect_vector::{
    colbert::*, ColBERTProvider, MockVectorProvider, MultiVectorInsertRequest, ScoringStrategy,
    VectorProvider,
};
use std::hint::black_box;

fn generate_vectors(num_vectors: usize, dim: usize) -> Vec<Vec<f32>> {
    (0..num_vectors)
        .map(|i| {
            let mut vec = vec![0.0; dim];
            vec[i % dim] = 1.0;
            vec
        })
        .collect()
}

fn bench_maxsim_scoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("maxsim_scoring");

    // Test different numbers of vectors
    for num_query_vecs in [1, 3, 5, 10] {
        for num_doc_vecs in [1, 5, 10, 20] {
            let query_vectors = generate_vectors(num_query_vecs, 128);
            let doc_vectors = generate_vectors(num_doc_vecs, 128);

            group.bench_with_input(
                BenchmarkId::new("sum", format!("q{}_d{}", num_query_vecs, num_doc_vecs)),
                &(&query_vectors, &doc_vectors),
                |b, (q, d)| {
                    b.iter(|| {
                        compute_maxsim_score(black_box(q), black_box(d), ScoringStrategy::MaxSimSum)
                    })
                },
            );
        }
    }

    group.finish();
}

fn bench_maxsim_strategies(c: &mut Criterion) {
    let query_vectors = generate_vectors(5, 128);
    let doc_vectors = generate_vectors(10, 128);

    let mut group = c.benchmark_group("maxsim_strategies");

    group.bench_function("sum", |b| {
        b.iter(|| {
            compute_maxsim_score(
                black_box(&query_vectors),
                black_box(&doc_vectors),
                ScoringStrategy::MaxSimSum,
            )
        })
    });

    group.bench_function("average", |b| {
        b.iter(|| {
            compute_maxsim_score(
                black_box(&query_vectors),
                black_box(&doc_vectors),
                ScoringStrategy::MaxSimAverage,
            )
        })
    });

    group.bench_function("max", |b| {
        b.iter(|| {
            compute_maxsim_score(
                black_box(&query_vectors),
                black_box(&doc_vectors),
                ScoringStrategy::MaxSimMax,
            )
        })
    });

    group.finish();
}

fn bench_colbert_insert(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("colbert_insert");

    // Test different numbers of vectors per document
    for num_vectors in [1, 3, 5, 10, 20] {
        let vectors = generate_vectors(num_vectors, 128);

        group.bench_with_input(
            BenchmarkId::from_parameter(num_vectors),
            &num_vectors,
            |b, _| {
                b.iter(|| {
                    runtime.block_on(async {
                        let mock = MockVectorProvider::new();
                        mock.create_collection("test", 128).await.unwrap();

                        let colbert = ColBERTProvider::new(mock);

                        colbert
                            .insert_multi_vector(black_box(MultiVectorInsertRequest {
                                collection: "test".to_string(),
                                id: "doc1".to_string(),
                                vectors: vectors.clone(),
                                payload: serde_json::json!({"test": true}),
                            }))
                            .await
                            .unwrap();
                    })
                })
            },
        );
    }

    group.finish();
}

fn bench_colbert_search(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("colbert_search");

    // Setup: Insert documents with multiple vectors
    for num_docs in [10, 50, 100] {
        let setup = runtime.block_on(async {
            let mock = MockVectorProvider::new();
            mock.create_collection("test", 128).await.unwrap();

            let colbert = ColBERTProvider::new(mock);

            // Insert documents with 5 vectors each
            for i in 0..num_docs {
                let vectors = generate_vectors(5, 128);
                colbert
                    .insert_multi_vector(MultiVectorInsertRequest {
                        collection: "test".to_string(),
                        id: format!("doc{}", i),
                        vectors,
                        payload: serde_json::json!({"id": i}),
                    })
                    .await
                    .unwrap();
            }

            colbert
        });

        let query_vectors = generate_vectors(3, 128);

        group.bench_with_input(BenchmarkId::from_parameter(num_docs), &num_docs, |b, _| {
            b.iter(|| {
                runtime.block_on(async {
                    setup
                        .search_multi_vector(
                            black_box("test"),
                            black_box(query_vectors.clone()),
                            black_box(10),
                            None,
                        )
                        .await
                        .unwrap();
                })
            })
        });
    }

    group.finish();
}

fn bench_colbert_vector_count_scaling(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("colbert_vector_count_scaling");

    // Setup: Insert documents with varying numbers of vectors
    for vecs_per_doc in [1, 3, 5, 10, 20] {
        let setup = runtime.block_on(async {
            let mock = MockVectorProvider::new();
            mock.create_collection("test", 128).await.unwrap();

            let colbert = ColBERTProvider::new(mock);

            // Insert 50 documents
            for i in 0..50 {
                let vectors = generate_vectors(vecs_per_doc, 128);
                colbert
                    .insert_multi_vector(MultiVectorInsertRequest {
                        collection: "test".to_string(),
                        id: format!("doc{}", i),
                        vectors,
                        payload: serde_json::json!({"id": i}),
                    })
                    .await
                    .unwrap();
            }

            colbert
        });

        let query_vectors = generate_vectors(3, 128);

        group.bench_with_input(
            BenchmarkId::from_parameter(vecs_per_doc),
            &vecs_per_doc,
            |b, _| {
                b.iter(|| {
                    runtime.block_on(async {
                        setup
                            .search_multi_vector(
                                black_box("test"),
                                black_box(query_vectors.clone()),
                                black_box(10),
                                None,
                            )
                            .await
                            .unwrap();
                    })
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_maxsim_scoring,
    bench_maxsim_strategies,
    bench_colbert_insert,
    bench_colbert_search,
    bench_colbert_vector_count_scaling
);
criterion_main!(benches);
