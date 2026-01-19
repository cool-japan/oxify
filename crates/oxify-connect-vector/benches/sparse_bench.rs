use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_connect_vector::{
    cosine_similarity, densify_with_threshold, sparse_cosine_similarity, sparse_dot_product,
    sparse_euclidean_distance, SparseVector,
};
use std::hint::black_box;

/// Benchmark sparse vs dense dot product
fn bench_dot_product_sparse_vs_dense(c: &mut Criterion) {
    let mut group = c.benchmark_group("dot_product_sparse_vs_dense");

    for sparsity in [0.9, 0.95, 0.99].iter() {
        let dim = 10000;
        let nnz = ((1.0 - sparsity) * dim as f64) as usize;

        // Create sparse vectors
        let elements1: Vec<(usize, f32)> = (0..nnz).map(|i| (i * 10, (i as f32) / 100.0)).collect();
        let elements2: Vec<(usize, f32)> =
            (0..nnz).map(|i| (i * 10 + 1, (i as f32) / 100.0)).collect();

        let sparse1 = SparseVector::new(elements1, dim);
        let sparse2 = SparseVector::new(elements2, dim);

        let dense1 = sparse1.to_dense();
        let dense2 = sparse2.to_dense();

        group.throughput(Throughput::Elements(dim as u64));

        group.bench_with_input(
            BenchmarkId::new(format!("sparse_{}%", (sparsity * 100.0) as usize), dim),
            &(&sparse1, &sparse2),
            |bencher, (s1, s2)| {
                bencher.iter(|| {
                    black_box(sparse_dot_product(s1, s2));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new(format!("dense_{}%", (sparsity * 100.0) as usize), dim),
            &(&dense1, &dense2),
            |bencher, (d1, d2)| {
                bencher.iter(|| {
                    black_box(oxify_connect_vector::dot_product(d1, d2));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark sparse vs dense cosine similarity
fn bench_cosine_similarity_sparse_vs_dense(c: &mut Criterion) {
    let mut group = c.benchmark_group("cosine_similarity_sparse_vs_dense");

    for sparsity in [0.9, 0.95, 0.99].iter() {
        let dim = 10000;
        let nnz = ((1.0 - sparsity) * dim as f64) as usize;

        // Create sparse vectors
        let elements1: Vec<(usize, f32)> = (0..nnz).map(|i| (i * 10, (i as f32) / 100.0)).collect();
        let elements2: Vec<(usize, f32)> =
            (0..nnz).map(|i| (i * 10 + 1, (i as f32) / 100.0)).collect();

        let sparse1 = SparseVector::new(elements1, dim);
        let sparse2 = SparseVector::new(elements2, dim);

        let dense1 = sparse1.to_dense();
        let dense2 = sparse2.to_dense();

        group.throughput(Throughput::Elements(dim as u64));

        group.bench_with_input(
            BenchmarkId::new(format!("sparse_{}%", (sparsity * 100.0) as usize), dim),
            &(&sparse1, &sparse2),
            |bencher, (s1, s2)| {
                bencher.iter(|| {
                    black_box(sparse_cosine_similarity(s1, s2));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new(format!("dense_{}%", (sparsity * 100.0) as usize), dim),
            &(&dense1, &dense2),
            |bencher, (d1, d2)| {
                bencher.iter(|| {
                    black_box(cosine_similarity(d1, d2));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark sparse vs dense Euclidean distance
fn bench_euclidean_distance_sparse_vs_dense(c: &mut Criterion) {
    let mut group = c.benchmark_group("euclidean_distance_sparse_vs_dense");

    for sparsity in [0.9, 0.95, 0.99].iter() {
        let dim = 10000;
        let nnz = ((1.0 - sparsity) * dim as f64) as usize;

        // Create sparse vectors
        let elements1: Vec<(usize, f32)> = (0..nnz).map(|i| (i * 10, (i as f32) / 100.0)).collect();
        let elements2: Vec<(usize, f32)> =
            (0..nnz).map(|i| (i * 10 + 1, (i as f32) / 100.0)).collect();

        let sparse1 = SparseVector::new(elements1, dim);
        let sparse2 = SparseVector::new(elements2, dim);

        let dense1 = sparse1.to_dense();
        let dense2 = sparse2.to_dense();

        group.throughput(Throughput::Elements(dim as u64));

        group.bench_with_input(
            BenchmarkId::new(format!("sparse_{}%", (sparsity * 100.0) as usize), dim),
            &(&sparse1, &sparse2),
            |bencher, (s1, s2)| {
                bencher.iter(|| {
                    black_box(sparse_euclidean_distance(s1, s2));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new(format!("dense_{}%", (sparsity * 100.0) as usize), dim),
            &(&dense1, &dense2),
            |bencher, (d1, d2)| {
                bencher.iter(|| {
                    black_box(oxify_connect_vector::euclidean_distance(d1, d2));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark conversion between sparse and dense
fn bench_sparse_dense_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("sparse_dense_conversion");

    for sparsity in [0.9, 0.95, 0.99].iter() {
        let dim = 10000;
        let nnz = ((1.0 - sparsity) * dim as f64) as usize;

        // Create dense vector
        let mut dense = vec![0.0; dim];
        for i in 0..nnz {
            dense[i * 10] = (i as f32) / 100.0;
        }

        let sparse = SparseVector::from_dense(&dense);

        group.throughput(Throughput::Elements(dim as u64));

        group.bench_with_input(
            BenchmarkId::new(format!("to_sparse_{}%", (sparsity * 100.0) as usize), dim),
            &dense,
            |bencher, d| {
                bencher.iter(|| {
                    black_box(SparseVector::from_dense(d));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new(format!("to_dense_{}%", (sparsity * 100.0) as usize), dim),
            &sparse,
            |bencher, s| {
                bencher.iter(|| {
                    black_box(s.to_dense());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark thresholding
fn bench_threshold_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("threshold_conversion");

    let dim = 10000;
    let dense: Vec<f32> = (0..dim)
        .map(|i| {
            if i % 10 == 0 {
                (i as f32) / 100.0
            } else {
                0.001 // Small noise
            }
        })
        .collect();

    for threshold in [0.01, 0.1, 1.0].iter() {
        group.bench_with_input(
            BenchmarkId::new("threshold", threshold),
            threshold,
            |bencher, &thresh| {
                bencher.iter(|| {
                    black_box(densify_with_threshold(&dense, thresh));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_dot_product_sparse_vs_dense,
    bench_cosine_similarity_sparse_vs_dense,
    bench_euclidean_distance_sparse_vs_dense,
    bench_sparse_dense_conversion,
    bench_threshold_conversion
);
criterion_main!(benches);
