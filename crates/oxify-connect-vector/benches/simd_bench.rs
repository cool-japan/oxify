use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_connect_vector::{
    batch_cosine_similarity, batch_cosine_similarity_optimized, cosine_similarity,
    cosine_similarity_optimized, dot_product, dot_product_optimized, euclidean_distance,
    euclidean_distance_optimized, manhattan_distance, manhattan_distance_optimized,
};
use std::hint::black_box;

/// Benchmark cosine similarity (SIMD vs non-SIMD)
fn bench_cosine_similarity(c: &mut Criterion) {
    let mut group = c.benchmark_group("cosine_similarity");

    for size in [64, 128, 256, 512, 1024, 2048].iter() {
        let a: Vec<f32> = (0..*size).map(|i| (i as f32) / 100.0).collect();
        let b: Vec<f32> = (0..*size).map(|i| (i as f32 + 1.0) / 100.0).collect();

        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("standard", size),
            &(&a, &b),
            |bencher, (a, b)| {
                bencher.iter(|| {
                    black_box(cosine_similarity(a, b));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("optimized", size),
            &(&a, &b),
            |bencher, (a, b)| {
                bencher.iter(|| {
                    black_box(cosine_similarity_optimized(a, b));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark dot product (SIMD vs non-SIMD)
fn bench_dot_product(c: &mut Criterion) {
    let mut group = c.benchmark_group("dot_product");

    for size in [64, 128, 256, 512, 1024, 2048].iter() {
        let a: Vec<f32> = (0..*size).map(|i| (i as f32) / 100.0).collect();
        let b: Vec<f32> = (0..*size).map(|i| (i as f32 + 1.0) / 100.0).collect();

        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("standard", size),
            &(&a, &b),
            |bencher, (a, b)| {
                bencher.iter(|| {
                    black_box(dot_product(a, b));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("optimized", size),
            &(&a, &b),
            |bencher, (a, b)| {
                bencher.iter(|| {
                    black_box(dot_product_optimized(a, b));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark euclidean distance (SIMD vs non-SIMD)
fn bench_euclidean_distance(c: &mut Criterion) {
    let mut group = c.benchmark_group("euclidean_distance");

    for size in [64, 128, 256, 512, 1024, 2048].iter() {
        let a: Vec<f32> = (0..*size).map(|i| (i as f32) / 100.0).collect();
        let b: Vec<f32> = (0..*size).map(|i| (i as f32 + 1.0) / 100.0).collect();

        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("standard", size),
            &(&a, &b),
            |bencher, (a, b)| {
                bencher.iter(|| {
                    black_box(euclidean_distance(a, b));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("optimized", size),
            &(&a, &b),
            |bencher, (a, b)| {
                bencher.iter(|| {
                    black_box(euclidean_distance_optimized(a, b));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark manhattan distance (SIMD vs non-SIMD)
fn bench_manhattan_distance(c: &mut Criterion) {
    let mut group = c.benchmark_group("manhattan_distance");

    for size in [64, 128, 256, 512, 1024, 2048].iter() {
        let a: Vec<f32> = (0..*size).map(|i| (i as f32) / 100.0).collect();
        let b: Vec<f32> = (0..*size).map(|i| (i as f32 + 1.0) / 100.0).collect();

        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("standard", size),
            &(&a, &b),
            |bencher, (a, b)| {
                bencher.iter(|| {
                    black_box(manhattan_distance(a, b));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("optimized", size),
            &(&a, &b),
            |bencher, (a, b)| {
                bencher.iter(|| {
                    black_box(manhattan_distance_optimized(a, b));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark batch cosine similarity (SIMD vs non-SIMD)
fn bench_batch_cosine_similarity(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_cosine_similarity");

    let query: Vec<f32> = (0..128).map(|i| (i as f32) / 100.0).collect();

    for batch_size in [10, 100, 1000].iter() {
        let vectors: Vec<Vec<f32>> = (0..*batch_size)
            .map(|i| (0..128).map(|j| ((i + j) as f32) / 100.0).collect())
            .collect();

        group.throughput(Throughput::Elements(*batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("standard", batch_size),
            &(&query, &vectors),
            |bencher, (query, vectors)| {
                bencher.iter(|| {
                    black_box(batch_cosine_similarity(query, vectors));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("optimized", batch_size),
            &(&query, &vectors),
            |bencher, (query, vectors)| {
                bencher.iter(|| {
                    black_box(batch_cosine_similarity_optimized(query, vectors));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_cosine_similarity,
    bench_dot_product,
    bench_euclidean_distance,
    bench_manhattan_distance,
    bench_batch_cosine_similarity
);
criterion_main!(benches);
