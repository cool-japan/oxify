//! Benchmarks for oxify-vector
//!
//! Run with: cargo bench --package oxify-vector

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_vector::{
    simd::{
        cosine_similarity_simd, dot_product_simd, euclidean_distance_simd, manhattan_distance_simd,
        quantized_dot_product_simd, quantized_euclidean_squared_simd,
        quantized_manhattan_distance_simd,
    },
    BinaryQuantizationConfig, BinaryQuantizedIndex, BinaryQuantizer, DistanceMetric,
    DistributedIndex, Filter, FilterValue, FourBitQuantizedIndex, FourBitQuantizer,
    GpuBatchProcessor, GpuConfig, HnswConfig, HnswIndex, Metadata, OptimizerConfig,
    QuantizationConfig, QuantizedVectorIndex, QueryOptimizer, ScalarQuantizer, SearchConfig,
    ShardConfig, VectorSearchIndex,
};

#[cfg(feature = "fp16")]
use oxify_vector::quantization::{Fp16QuantizedIndex, Fp16Quantizer};
use rand::Rng;
use std::collections::HashMap;
use std::hint::black_box;

/// Generate random embeddings
fn generate_embeddings(count: usize, dimensions: usize) -> HashMap<String, Vec<f32>> {
    let mut rng = rand::rng();
    let mut embeddings = HashMap::new();

    for i in 0..count {
        let vec: Vec<f32> = (0..dimensions)
            .map(|_| rng.random_range(-1.0..1.0))
            .collect();
        embeddings.insert(format!("doc_{}", i), vec);
    }

    embeddings
}

/// Generate random query
fn generate_query(dimensions: usize) -> Vec<f32> {
    let mut rng = rand::rng();
    (0..dimensions)
        .map(|_| rng.random_range(-1.0..1.0))
        .collect()
}

/// Generate test metadata
fn generate_metadata(count: usize) -> HashMap<String, Metadata> {
    let mut rng = rand::rng();
    let mut metadata = HashMap::new();
    let types = ["article", "book", "paper", "report"];

    for i in 0..count {
        let mut m = HashMap::new();
        m.insert(
            "type".to_string(),
            FilterValue::String(types[i % types.len()].to_string()),
        );
        m.insert(
            "year".to_string(),
            FilterValue::Int(rng.random_range(2016..2026)),
        );
        m.insert(
            "score".to_string(),
            FilterValue::Float(rng.random_range(0.0..1.0)),
        );
        metadata.insert(format!("doc_{}", i), m);
    }

    metadata
}

/// Benchmark exact search with different dataset sizes
fn bench_exact_search(c: &mut Criterion) {
    let dimensions = 768; // Typical embedding dimension
    let k = 10;

    let mut group = c.benchmark_group("exact_search");

    for size in [100, 1000, 5000, 10000] {
        let embeddings = generate_embeddings(size, dimensions);
        let query = generate_query(dimensions);

        let mut index = VectorSearchIndex::new(SearchConfig {
            metric: DistanceMetric::Cosine,
            parallel: false,
            normalize: true,
        });
        index.build(&embeddings).unwrap();

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("sequential", size), &size, |b, _| {
            b.iter(|| index.search(black_box(&query), black_box(k)))
        });
    }

    group.finish();
}

/// Benchmark parallel vs sequential search
fn bench_parallel_search(c: &mut Criterion) {
    let dimensions = 768;
    let size = 10000;
    let k = 10;

    let embeddings = generate_embeddings(size, dimensions);
    let query = generate_query(dimensions);

    let mut group = c.benchmark_group("parallel_vs_sequential");

    // Sequential
    let mut seq_index = VectorSearchIndex::new(SearchConfig {
        metric: DistanceMetric::Cosine,
        parallel: false,
        normalize: true,
    });
    seq_index.build(&embeddings).unwrap();

    group.bench_function("sequential_10k", |b| {
        b.iter(|| seq_index.search(black_box(&query), black_box(k)))
    });

    // Parallel
    let mut par_index = VectorSearchIndex::new(SearchConfig {
        metric: DistanceMetric::Cosine,
        parallel: true,
        normalize: true,
    });
    par_index.build(&embeddings).unwrap();

    group.bench_function("parallel_10k", |b| {
        b.iter(|| par_index.search(black_box(&query), black_box(k)))
    });

    group.finish();
}

/// Benchmark different distance metrics
fn bench_distance_metrics(c: &mut Criterion) {
    let dimensions = 768;
    let size = 5000;
    let k = 10;

    let embeddings = generate_embeddings(size, dimensions);
    let query = generate_query(dimensions);

    let mut group = c.benchmark_group("distance_metrics");

    for (metric_name, metric) in [
        ("cosine", DistanceMetric::Cosine),
        ("euclidean", DistanceMetric::Euclidean),
        ("dot_product", DistanceMetric::DotProduct),
        ("manhattan", DistanceMetric::Manhattan),
    ] {
        let mut index = VectorSearchIndex::new(SearchConfig {
            metric,
            parallel: true,
            normalize: metric == DistanceMetric::Cosine,
        });
        index.build(&embeddings).unwrap();

        group.bench_function(metric_name, |b| {
            b.iter(|| index.search(black_box(&query), black_box(k)))
        });
    }

    group.finish();
}

/// Benchmark HNSW index
fn bench_hnsw(c: &mut Criterion) {
    let dimensions = 768;
    let k = 10;

    let mut group = c.benchmark_group("hnsw");

    for size in [1000, 5000, 10000] {
        let embeddings = generate_embeddings(size, dimensions);
        let query = generate_query(dimensions);

        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("search", size), &size, |b, _| {
            b.iter(|| index.search(black_box(&query), black_box(k)))
        });
    }

    group.finish();
}

/// Benchmark HNSW vs exact search
fn bench_hnsw_vs_exact(c: &mut Criterion) {
    let dimensions = 768;
    let size = 10000;
    let k = 10;

    let embeddings = generate_embeddings(size, dimensions);
    let query = generate_query(dimensions);

    let mut group = c.benchmark_group("hnsw_vs_exact");

    // Exact search (parallel)
    let mut exact_index = VectorSearchIndex::new(SearchConfig {
        metric: DistanceMetric::Cosine,
        parallel: true,
        normalize: true,
    });
    exact_index.build(&embeddings).unwrap();

    group.bench_function("exact_10k", |b| {
        b.iter(|| exact_index.search(black_box(&query), black_box(k)))
    });

    // HNSW search
    let mut hnsw_index = HnswIndex::new(HnswConfig::default());
    hnsw_index.build(&embeddings).unwrap();

    group.bench_function("hnsw_10k", |b| {
        b.iter(|| hnsw_index.search(black_box(&query), black_box(k)))
    });

    group.finish();
}

/// Benchmark index building
fn bench_build_index(c: &mut Criterion) {
    let dimensions = 768;

    let mut group = c.benchmark_group("build_index");

    for size in [1000, 5000] {
        let embeddings = generate_embeddings(size, dimensions);

        // Exact index build
        group.bench_with_input(BenchmarkId::new("exact", size), &size, |b, _| {
            b.iter(|| {
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(black_box(&embeddings)).unwrap()
            })
        });

        // HNSW index build
        group.bench_with_input(BenchmarkId::new("hnsw", size), &size, |b, _| {
            b.iter(|| {
                let mut index = HnswIndex::new(HnswConfig::default());
                index.build(black_box(&embeddings)).unwrap()
            })
        });
    }

    group.finish();
}

/// Benchmark filtered search
fn bench_filtered_search(c: &mut Criterion) {
    let dimensions = 768;
    let size = 10000;
    let k = 10;

    let embeddings = generate_embeddings(size, dimensions);
    let metadata = generate_metadata(size);
    let query = generate_query(dimensions);

    let mut index = VectorSearchIndex::new(SearchConfig::default());
    index.build(&embeddings).unwrap();
    index.set_metadata_batch(metadata);

    let mut group = c.benchmark_group("filtered_search");

    // No filter
    group.bench_function("no_filter", |b| {
        let filter = Filter::new();
        b.iter(|| index.filtered_search(black_box(&query), black_box(k), black_box(&filter)))
    });

    // Single condition filter (25% selectivity)
    group.bench_function("single_filter", |b| {
        let filter = Filter::new().eq("type", "article");
        b.iter(|| index.filtered_search(black_box(&query), black_box(k), black_box(&filter)))
    });

    // Combined filter (more selective)
    group.bench_function("combined_filter", |b| {
        let filter = Filter::new().eq("type", "article").gte("year", 2022i64);
        b.iter(|| index.filtered_search(black_box(&query), black_box(k), black_box(&filter)))
    });

    // Pre-filtered search
    group.bench_function("prefiltered", |b| {
        let filter = Filter::new().eq("type", "article");
        b.iter(|| index.prefiltered_search(black_box(&query), black_box(k), black_box(&filter)))
    });

    group.finish();
}

/// Benchmark batch search
fn bench_batch_search(c: &mut Criterion) {
    let dimensions = 768;
    let size = 5000;
    let k = 10;

    let embeddings = generate_embeddings(size, dimensions);
    let queries: Vec<Vec<f32>> = (0..100).map(|_| generate_query(dimensions)).collect();

    let mut index = VectorSearchIndex::new(SearchConfig::default());
    index.build(&embeddings).unwrap();

    let mut group = c.benchmark_group("batch_search");

    for batch_size in [10, 50, 100] {
        let batch = queries[..batch_size].to_vec();

        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("queries", batch_size),
            &batch_size,
            |b, _| b.iter(|| index.batch_search(black_box(&batch), black_box(k))),
        );
    }

    group.finish();
}

/// Calculate recall@k: what percentage of ground truth top-k are found
fn calculate_recall(ground_truth: &[String], results: &[String]) -> f32 {
    let gt_set: std::collections::HashSet<_> = ground_truth.iter().collect();
    let found = results.iter().filter(|id| gt_set.contains(id)).count();
    found as f32 / ground_truth.len() as f32
}

/// Benchmark recall accuracy for HNSW with different ef_search values
fn bench_recall_accuracy(c: &mut Criterion) {
    let dimensions = 384;
    let size = 5000;
    let num_queries = 100;

    println!("\n=== Recall Accuracy Benchmark ===");
    println!("Dataset size: {}, Dimensions: {}", size, dimensions);
    println!("Number of queries: {}", num_queries);

    let embeddings = generate_embeddings(size, dimensions);
    let queries: Vec<Vec<f32>> = (0..num_queries)
        .map(|_| generate_query(dimensions))
        .collect();

    // Build exact search index for ground truth
    let mut exact_index = VectorSearchIndex::new(SearchConfig {
        metric: DistanceMetric::Cosine,
        parallel: true,
        normalize: true,
    });
    exact_index.build(&embeddings).unwrap();

    // Generate ground truth for recall@10 and recall@100
    println!("\nGenerating ground truth with exact search...");
    let ground_truth_10: Vec<Vec<String>> = queries
        .iter()
        .map(|q| {
            exact_index
                .search(q, 10)
                .unwrap()
                .iter()
                .map(|r| r.entity_id.clone())
                .collect()
        })
        .collect();

    let ground_truth_100: Vec<Vec<String>> = queries
        .iter()
        .map(|q| {
            exact_index
                .search(q, 100)
                .unwrap()
                .iter()
                .map(|r| r.entity_id.clone())
                .collect()
        })
        .collect();

    println!("Ground truth generated.");

    // Test different HNSW configurations
    let configs = vec![
        ("default", HnswConfig::default()),
        ("fast", HnswConfig::fast()),
        ("high_recall", HnswConfig::high_recall()),
        (
            "custom_ef16",
            HnswConfig {
                ef_search: 16,
                ..HnswConfig::default()
            },
        ),
        (
            "custom_ef32",
            HnswConfig {
                ef_search: 32,
                ..HnswConfig::default()
            },
        ),
        (
            "custom_ef64",
            HnswConfig {
                ef_search: 64,
                ..HnswConfig::default()
            },
        ),
        (
            "custom_ef128",
            HnswConfig {
                ef_search: 128,
                ..HnswConfig::default()
            },
        ),
    ];

    println!(
        "\n{:<15} {:<10} {:<10} {:<12} {:<12}",
        "Config", "Recall@10", "Recall@100", "Avg Time(μs)", "ef_search"
    );
    println!("{}", "-".repeat(65));

    for (name, config) in &configs {
        let mut hnsw_index = HnswIndex::new(config.clone());
        hnsw_index.build(&embeddings).unwrap();

        // Calculate recall@10
        let mut recall_10_sum = 0.0;
        let mut recall_100_sum = 0.0;
        let mut total_time = std::time::Duration::ZERO;

        for (i, query) in queries.iter().enumerate() {
            let start = std::time::Instant::now();
            let results_10 = hnsw_index.search(query, 10).unwrap();
            let elapsed = start.elapsed();
            total_time += elapsed;

            let result_ids_10: Vec<String> =
                results_10.iter().map(|r| r.entity_id.clone()).collect();
            recall_10_sum += calculate_recall(&ground_truth_10[i], &result_ids_10);

            let results_100 = hnsw_index.search(query, 100).unwrap();
            let result_ids_100: Vec<String> =
                results_100.iter().map(|r| r.entity_id.clone()).collect();
            recall_100_sum += calculate_recall(&ground_truth_100[i], &result_ids_100);
        }

        let avg_recall_10 = recall_10_sum / num_queries as f32;
        let avg_recall_100 = recall_100_sum / num_queries as f32;
        let avg_time_us = total_time.as_micros() / num_queries as u128;

        println!(
            "{:<15} {:<10.2}% {:<10.2}% {:<12} {:<12}",
            name,
            avg_recall_10 * 100.0,
            avg_recall_100 * 100.0,
            avg_time_us,
            config.ef_search
        );
    }

    println!("\n{}", "=".repeat(65));
    println!("Target: >99% recall@10 with <10ms latency (10000μs)");
    println!("{}", "=".repeat(65));

    // Benchmark recall for different ef_search values
    let mut group = c.benchmark_group("recall_accuracy");

    for (name, config) in configs {
        let mut hnsw_index = HnswIndex::new(config);
        hnsw_index.build(&embeddings).unwrap();

        group.bench_function(name, |b| {
            b.iter(|| {
                let query = black_box(&queries[0]);
                hnsw_index.search(query, black_box(10))
            })
        });
    }

    group.finish();
}

/// Benchmark distributed search across shards
fn bench_distributed_search(c: &mut Criterion) {
    let dimensions = 768;
    let k = 10;

    let mut group = c.benchmark_group("distributed_search");

    // Benchmark different shard counts
    for num_shards in [1, 2, 4, 8] {
        let size = 10000;
        let embeddings = generate_embeddings(size, dimensions);
        let query = generate_query(dimensions);

        let shard_config = ShardConfig::new(num_shards, 1);
        let search_config = SearchConfig::default();
        let mut index = DistributedIndex::new(shard_config, search_config);
        index.build(&embeddings).unwrap();

        group.bench_with_input(
            BenchmarkId::new("shards", num_shards),
            &num_shards,
            |b, _| b.iter(|| index.search(black_box(&query), black_box(k))),
        );
    }

    group.finish();
}

/// Benchmark distributed vs centralized search
fn bench_distributed_vs_centralized(c: &mut Criterion) {
    let dimensions = 768;
    let size = 10000;
    let k = 10;

    let embeddings = generate_embeddings(size, dimensions);
    let query = generate_query(dimensions);

    let mut group = c.benchmark_group("distributed_vs_centralized");

    // Centralized (regular) search
    let mut centralized = VectorSearchIndex::new(SearchConfig::default());
    centralized.build(&embeddings).unwrap();

    group.bench_function("centralized", |b| {
        b.iter(|| centralized.search(black_box(&query), black_box(k)))
    });

    // Distributed with 4 shards
    let shard_config = ShardConfig::new(4, 1);
    let mut distributed = DistributedIndex::new(shard_config, SearchConfig::default());
    distributed.build(&embeddings).unwrap();

    group.bench_function("distributed_4_shards", |b| {
        b.iter(|| distributed.search(black_box(&query), black_box(k)))
    });

    group.finish();
}

/// Benchmark distributed batch search
fn bench_distributed_batch(c: &mut Criterion) {
    let dimensions = 768;
    let size = 5000;
    let k = 10;

    let embeddings = generate_embeddings(size, dimensions);
    let queries: Vec<Vec<f32>> = (0..100).map(|_| generate_query(dimensions)).collect();

    let shard_config = ShardConfig::new(4, 1);
    let mut index = DistributedIndex::new(shard_config, SearchConfig::default());
    index.build(&embeddings).unwrap();

    c.bench_function("distributed_batch_search_100", |b| {
        b.iter(|| index.batch_search(black_box(&queries), black_box(k)))
    });
}

/// Benchmark distributed filtered search
fn bench_distributed_filtered(c: &mut Criterion) {
    let dimensions = 768;
    let size = 10000;
    let k = 10;

    let embeddings = generate_embeddings(size, dimensions);
    let metadata = generate_metadata(size);
    let query = generate_query(dimensions);

    let shard_config = ShardConfig::new(4, 1);
    let mut index = DistributedIndex::new(shard_config, SearchConfig::default());
    index.build(&embeddings).unwrap();
    index.batch_set_metadata(&metadata);

    let mut group = c.benchmark_group("distributed_filtered");

    // No filter
    group.bench_function("no_filter", |b| {
        let filter = Filter::new();
        b.iter(|| index.filtered_search(black_box(&query), black_box(k), black_box(&filter)))
    });

    // Single filter
    group.bench_function("single_filter", |b| {
        let filter = Filter::new().eq("type", "article");
        b.iter(|| index.filtered_search(black_box(&query), black_box(k), black_box(&filter)))
    });

    group.finish();
}

/// Benchmark SIMD optimizations (AVX2 vs auto-vectorization)
fn bench_simd_optimizations(c: &mut Criterion) {
    let dimensions = 768; // Typical embedding dimension

    // Generate test vectors
    let v1 = generate_query(dimensions);
    let v2 = generate_query(dimensions);

    let mut group = c.benchmark_group("simd_optimizations");

    // Benchmark cosine similarity
    group.bench_function("cosine_similarity", |b| {
        b.iter(|| cosine_similarity_simd(black_box(&v1), black_box(&v2)))
    });

    // Benchmark euclidean distance
    group.bench_function("euclidean_distance", |b| {
        b.iter(|| euclidean_distance_simd(black_box(&v1), black_box(&v2)))
    });

    // Benchmark dot product
    group.bench_function("dot_product", |b| {
        b.iter(|| dot_product_simd(black_box(&v1), black_box(&v2)))
    });

    // Benchmark manhattan distance
    group.bench_function("manhattan_distance", |b| {
        b.iter(|| manhattan_distance_simd(black_box(&v1), black_box(&v2)))
    });

    group.finish();

    // Benchmark with different vector sizes
    let mut group = c.benchmark_group("simd_vector_sizes");

    for size in [128, 384, 768, 1024, 1536] {
        let v1 = generate_query(size);
        let v2 = generate_query(size);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("cosine", size), &size, |b, _| {
            b.iter(|| cosine_similarity_simd(black_box(&v1), black_box(&v2)))
        });
    }

    group.finish();
}

/// Benchmark incremental index updates
fn bench_incremental_updates(c: &mut Criterion) {
    let dimensions = 768;
    let initial_size = 5000;

    let embeddings = generate_embeddings(initial_size, dimensions);
    let mut index = VectorSearchIndex::new(SearchConfig::default());
    index.build(&embeddings).unwrap();

    let mut group = c.benchmark_group("incremental_updates");

    // Benchmark add_vector
    group.bench_function("add_single", |b| {
        let vector = generate_query(dimensions);
        b.iter(|| {
            let mut idx = index.clone();
            idx.add_vector(black_box("new_doc".to_string()), black_box(vector.clone()))
        })
    });

    // Benchmark add_vectors (batch)
    for batch_size in [10, 100, 1000] {
        let new_embeddings: HashMap<String, Vec<f32>> = (0..batch_size)
            .map(|i| (format!("new_doc_{}", i), generate_query(dimensions)))
            .collect();

        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("add_batch", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    let mut idx = index.clone();
                    idx.add_vectors(black_box(&new_embeddings))
                })
            },
        );
    }

    // Benchmark remove_vector
    group.bench_function("remove_single", |b| {
        b.iter(|| {
            let mut idx = index.clone();
            idx.remove_vector(black_box("doc_0"))
        })
    });

    // Benchmark update_vector
    group.bench_function("update_single", |b| {
        let new_vector = generate_query(dimensions);
        b.iter(|| {
            let mut idx = index.clone();
            idx.update_vector(black_box("doc_0"), black_box(new_vector.clone()))
        })
    });

    group.finish();
}

/// Benchmark query optimizer
fn bench_query_optimizer(c: &mut Criterion) {
    let optimizer = QueryOptimizer::new(OptimizerConfig::default());

    let mut group = c.benchmark_group("query_optimizer");

    // Benchmark strategy recommendation
    group.bench_function("recommend_strategy", |b| {
        b.iter(|| optimizer.recommend_strategy(black_box(100_000), black_box(0.95)))
    });

    // Benchmark prefiltering recommendation
    group.bench_function("recommend_prefiltering", |b| {
        b.iter(|| optimizer.recommend_prefiltering(black_box(100_000), black_box(0.05)))
    });

    // Benchmark batch size recommendation
    group.bench_function("recommend_batch_size", |b| {
        b.iter(|| optimizer.recommend_batch_size(black_box(1000), black_box(100_000)))
    });

    // Benchmark cost estimation
    group.bench_function("estimate_cost", |b| {
        use oxify_vector::SearchStrategy;
        b.iter(|| {
            optimizer.estimate_cost(
                black_box(SearchStrategy::Hnsw),
                black_box(100_000),
                black_box(10),
            )
        })
    });

    group.finish();
}

/// Benchmark scalar quantization
fn bench_scalar_quantization(c: &mut Criterion) {
    let dimensions = 768;
    let size = 1000;

    // Generate training data
    let vectors: Vec<Vec<f32>> = (0..size).map(|_| generate_query(dimensions)).collect();

    let mut group = c.benchmark_group("scalar_quantization");

    // Benchmark quantizer fitting
    group.bench_function("fit", |b| {
        b.iter(|| {
            let mut quantizer = ScalarQuantizer::new(QuantizationConfig::default());
            quantizer.fit(black_box(&vectors))
        })
    });

    // Benchmark quantization
    let mut quantizer = ScalarQuantizer::new(QuantizationConfig::default());
    quantizer.fit(&vectors).unwrap();

    let test_vector = generate_query(dimensions);

    group.bench_function("quantize_single", |b| {
        b.iter(|| quantizer.quantize(black_box(&test_vector)))
    });

    // Benchmark batch quantization
    for batch_size in [10, 100, 1000] {
        let batch: Vec<Vec<f32>> = (0..batch_size)
            .map(|_| generate_query(dimensions))
            .collect();

        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("quantize_batch", batch_size),
            &batch_size,
            |b, _| b.iter(|| quantizer.quantize_batch(black_box(&batch))),
        );
    }

    // Benchmark dequantization
    let quantized = quantizer.quantize(&test_vector);

    group.bench_function("dequantize_single", |b| {
        b.iter(|| quantizer.dequantize(black_box(&quantized)))
    });

    // Benchmark quantized distance computation
    let quantized_a = quantizer.quantize(&vectors[0]);
    let quantized_b = quantizer.quantize(&vectors[1]);

    group.bench_function("quantized_distance", |b| {
        b.iter(|| quantizer.quantized_distance(black_box(&quantized_a), black_box(&quantized_b)))
    });

    group.finish();
}

/// Benchmark quantized index
fn bench_quantized_index(c: &mut Criterion) {
    let dimensions = 768;
    let k = 10;

    let mut group = c.benchmark_group("quantized_index");

    // Benchmark different dataset sizes
    for size in [1000, 5000, 10000] {
        let vectors: Vec<(String, Vec<f32>)> = (0..size)
            .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
            .collect();

        let query = generate_query(dimensions);

        // Build quantized index
        let mut quantized_index = QuantizedVectorIndex::new(QuantizationConfig::default());
        quantized_index.build(&vectors).unwrap();

        // Build regular index for comparison
        let embeddings: HashMap<String, Vec<f32>> = vectors
            .iter()
            .map(|(id, v)| (id.clone(), v.clone()))
            .collect();
        let mut regular_index = VectorSearchIndex::new(SearchConfig::default());
        regular_index.build(&embeddings).unwrap();

        group.throughput(Throughput::Elements(size as u64));

        // Benchmark quantized search
        group.bench_with_input(BenchmarkId::new("quantized", size), &size, |b, _| {
            b.iter(|| quantized_index.search(black_box(&query), black_box(k)))
        });

        // Benchmark regular search for comparison
        group.bench_with_input(BenchmarkId::new("regular", size), &size, |b, _| {
            b.iter(|| regular_index.search(black_box(&query), black_box(k)))
        });
    }

    group.finish();
}

/// Benchmark memory efficiency of quantization
fn bench_quantization_memory(c: &mut Criterion) {
    let dimensions = 768;
    let size = 10000;

    println!("\n=== Quantization Memory Benchmark ===");
    println!("Dataset size: {}, Dimensions: {}", size, dimensions);

    let vectors: Vec<(String, Vec<f32>)> = (0..size)
        .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
        .collect();

    // Build quantized index
    let mut quantized_index = QuantizedVectorIndex::new(QuantizationConfig::default());
    quantized_index.build(&vectors).unwrap();

    let stats = quantized_index.stats();

    println!("\nMemory Usage:");
    println!(
        "  Original (float32):  {} bytes ({:.2} MB)",
        stats.original_bytes,
        stats.original_bytes as f64 / 1_000_000.0
    );
    println!(
        "  Quantized (uint8):   {} bytes ({:.2} MB)",
        stats.quantized_bytes,
        stats.quantized_bytes as f64 / 1_000_000.0
    );
    println!("  Compression ratio:   {:.2}x", stats.compression_ratio);
    println!(
        "  Memory savings:      {:.1}%",
        stats.memory_savings * 100.0
    );
    println!("{}", "=".repeat(45));

    // Minimal benchmark to satisfy criterion
    c.bench_function("quantization_memory_stats", |b| {
        b.iter(|| {
            let mut idx = QuantizedVectorIndex::new(QuantizationConfig::default());
            idx.build(black_box(&vectors)).unwrap();
            idx.stats()
        })
    });
}

/// Benchmark binary quantization operations
fn bench_binary_quantization(c: &mut Criterion) {
    let dimensions = 768;
    let size = 1000;

    // Generate training data
    let vectors: Vec<Vec<f32>> = (0..size).map(|_| generate_query(dimensions)).collect();

    let mut group = c.benchmark_group("binary_quantization");

    // Benchmark quantizer fitting
    group.bench_function("fit", |b| {
        b.iter(|| {
            let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
            quantizer.fit(black_box(&vectors))
        })
    });

    // Benchmark binary quantization
    let mut quantizer = BinaryQuantizer::new(BinaryQuantizationConfig::default());
    quantizer.fit(&vectors).unwrap();

    let test_vector = generate_query(dimensions);

    group.bench_function("quantize_single", |b| {
        b.iter(|| quantizer.quantize(black_box(&test_vector)))
    });

    // Benchmark batch quantization
    for batch_size in [10, 100, 1000] {
        let batch: Vec<Vec<f32>> = (0..batch_size)
            .map(|_| generate_query(dimensions))
            .collect();

        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("quantize_batch", batch_size),
            &batch_size,
            |b, _| b.iter(|| quantizer.quantize_batch(black_box(&batch))),
        );
    }

    // Benchmark dequantization
    let binary = quantizer.quantize(&test_vector);

    group.bench_function("dequantize_single", |b| {
        b.iter(|| quantizer.dequantize(black_box(&binary)))
    });

    // Benchmark Hamming distance
    let binary_a = quantizer.quantize(&vectors[0]);
    let binary_b = quantizer.quantize(&vectors[1]);

    group.bench_function("hamming_distance", |b| {
        b.iter(|| quantizer.hamming_distance(black_box(&binary_a), black_box(&binary_b)))
    });

    // Benchmark Hamming similarity
    group.bench_function("hamming_similarity", |b| {
        b.iter(|| quantizer.hamming_similarity(black_box(&binary_a), black_box(&binary_b)))
    });

    group.finish();
}

/// Benchmark binary quantized index
fn bench_binary_quantized_index(c: &mut Criterion) {
    let dimensions = 768;
    let k = 10;

    let mut group = c.benchmark_group("binary_quantized_index");

    // Benchmark different dataset sizes
    for size in [1000, 5000, 10000] {
        let vectors: Vec<(String, Vec<f32>)> = (0..size)
            .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
            .collect();

        let query = generate_query(dimensions);

        // Build binary quantized index
        let mut binary_index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
        binary_index.build(&vectors).unwrap();

        // Build regular index for comparison
        let embeddings: HashMap<String, Vec<f32>> = vectors
            .iter()
            .map(|(id, v)| (id.clone(), v.clone()))
            .collect();
        let mut regular_index = VectorSearchIndex::new(SearchConfig::default());
        regular_index.build(&embeddings).unwrap();

        group.throughput(Throughput::Elements(size as u64));

        // Benchmark binary quantized search
        group.bench_with_input(BenchmarkId::new("binary_quantized", size), &size, |b, _| {
            b.iter(|| binary_index.search(black_box(&query), black_box(k)))
        });

        // Benchmark regular search for comparison
        group.bench_with_input(BenchmarkId::new("regular", size), &size, |b, _| {
            b.iter(|| regular_index.search(black_box(&query), black_box(k)))
        });
    }

    group.finish();
}

/// Benchmark memory efficiency of binary quantization
fn bench_binary_quantization_memory(c: &mut Criterion) {
    let dimensions = 768;
    let size = 10000;

    println!("\n=== Binary Quantization Memory Benchmark ===");
    println!("Dataset size: {}, Dimensions: {}", size, dimensions);

    let vectors: Vec<(String, Vec<f32>)> = (0..size)
        .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
        .collect();

    // Build binary quantized index
    let mut binary_index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
    binary_index.build(&vectors).unwrap();

    let stats = binary_index.stats();

    println!("\nMemory Usage:");
    println!(
        "  Original (float32):  {} bytes ({:.2} MB)",
        stats.original_bytes,
        stats.original_bytes as f64 / 1_000_000.0
    );
    println!(
        "  Binary (1-bit):      {} bytes ({:.2} MB)",
        stats.binary_bytes,
        stats.binary_bytes as f64 / 1_000_000.0
    );
    println!("  Compression ratio:   {:.2}x", stats.compression_ratio);
    println!(
        "  Memory savings:      {:.2}%",
        stats.memory_savings * 100.0
    );
    println!("{}", "=".repeat(45));

    // Minimal benchmark to satisfy criterion
    c.bench_function("binary_quantization_memory_stats", |b| {
        b.iter(|| {
            let mut idx = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
            idx.build(black_box(&vectors)).unwrap();
            idx.stats()
        })
    });
}

/// Compare scalar vs binary quantization
fn bench_quantization_comparison(c: &mut Criterion) {
    let dimensions = 768;
    let size = 5000;
    let k = 10;

    let vectors: Vec<(String, Vec<f32>)> = (0..size)
        .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
        .collect();

    let query = generate_query(dimensions);

    // Build scalar quantized index
    let mut scalar_index = QuantizedVectorIndex::new(QuantizationConfig::default());
    scalar_index.build(&vectors).unwrap();

    // Build binary quantized index
    let mut binary_index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
    binary_index.build(&vectors).unwrap();

    let mut group = c.benchmark_group("quantization_comparison");
    group.throughput(Throughput::Elements(size as u64));

    // Benchmark scalar quantized search
    group.bench_function("scalar_8bit", |b| {
        b.iter(|| scalar_index.search(black_box(&query), black_box(k)))
    });

    // Benchmark binary quantized search
    group.bench_function("binary_1bit", |b| {
        b.iter(|| binary_index.search(black_box(&query), black_box(k)))
    });

    // Print memory comparison
    println!("\n=== Quantization Comparison (5K vectors, 768 dims) ===");
    let scalar_stats = scalar_index.stats();
    let binary_stats = binary_index.stats();

    println!(
        "Scalar (8-bit):  {:.2} MB, {:.1}x compression",
        scalar_stats.quantized_bytes as f64 / 1_000_000.0,
        scalar_stats.compression_ratio
    );
    println!(
        "Binary (1-bit):  {:.2} MB, {:.1}x compression",
        binary_stats.binary_bytes as f64 / 1_000_000.0,
        binary_stats.compression_ratio
    );
    println!(
        "Binary vs Scalar: {:.1}x smaller",
        scalar_stats.quantized_bytes as f64 / binary_stats.binary_bytes as f64
    );
    println!("{}", "=".repeat(55));

    group.finish();
}

/// Benchmark quantized SIMD operations
fn bench_quantized_simd(c: &mut Criterion) {
    // Test different vector sizes
    let dimensions = [128, 384, 768, 1536];

    for &dim in &dimensions {
        let mut group = c.benchmark_group(format!("quantized_simd_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));

        // Generate random u8 vectors
        let mut rng = rand::rng();
        let a: Vec<u8> = (0..dim).map(|_| rng.random_range(0..=255)).collect();
        let b: Vec<u8> = (0..dim).map(|_| rng.random_range(0..=255)).collect();

        // Benchmark Manhattan distance
        group.bench_function("manhattan", |bench| {
            bench.iter(|| quantized_manhattan_distance_simd(black_box(&a), black_box(&b)))
        });

        // Benchmark dot product
        group.bench_function("dot_product", |bench| {
            bench.iter(|| quantized_dot_product_simd(black_box(&a), black_box(&b)))
        });

        // Benchmark Euclidean squared distance
        group.bench_function("euclidean_squared", |bench| {
            bench.iter(|| quantized_euclidean_squared_simd(black_box(&a), black_box(&b)))
        });

        group.finish();
    }
}

/// Benchmark FP16 quantization operations
#[cfg(feature = "fp16")]
fn bench_fp16_quantization(c: &mut Criterion) {
    let dimensions = 768;
    let size = 1000;

    let vectors: Vec<Vec<f32>> = (0..size).map(|_| generate_query(dimensions)).collect();

    let quantizer = Fp16Quantizer::new();

    c.bench_function("fp16_quantize_single", |b| {
        b.iter(|| quantizer.quantize(black_box(&vectors[0])))
    });

    c.bench_function("fp16_dequantize_single", |b| {
        let quantized = quantizer.quantize(&vectors[0]);
        b.iter(|| quantizer.dequantize(black_box(&quantized)))
    });

    c.bench_function("fp16_quantize_batch", |b| {
        b.iter(|| quantizer.quantize_batch(black_box(&vectors)))
    });

    let quantized_vectors = quantizer.quantize_batch(&vectors);
    c.bench_function("fp16_dequantize_batch", |b| {
        b.iter(|| quantizer.dequantize_batch(black_box(&quantized_vectors)))
    });

    let a = quantizer.quantize(&vectors[0]);
    let b_vec = quantizer.quantize(&vectors[1]);
    c.bench_function("fp16_distance", |b| {
        b.iter(|| quantizer.fp16_distance(black_box(&a), black_box(&b_vec)))
    });
}

/// Benchmark FP16 quantized index search
#[cfg(feature = "fp16")]
fn bench_fp16_quantized_index(c: &mut Criterion) {
    let dimensions = 768;
    let k = 10;

    let mut group = c.benchmark_group("fp16_quantized_index");

    for size in [1000, 5000, 10000] {
        let vectors: Vec<(String, Vec<f32>)> = (0..size)
            .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
            .collect();

        let query = generate_query(dimensions);

        // Build FP16 quantized index
        let mut index = Fp16QuantizedIndex::new();
        index.build(&vectors).unwrap();

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| index.search(black_box(&query), black_box(k)))
        });
    }

    group.finish();
}

/// Benchmark FP16 memory efficiency
#[cfg(feature = "fp16")]
fn bench_fp16_quantization_memory(c: &mut Criterion) {
    let dimensions = 768;
    let size = 5000;

    let vectors: Vec<(String, Vec<f32>)> = (0..size)
        .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
        .collect();

    c.bench_function("fp16_quantization_memory_build", |b| {
        b.iter(|| {
            let mut idx = Fp16QuantizedIndex::new();
            idx.build(black_box(&vectors))
        })
    });

    c.bench_function("fp16_quantization_memory_stats", |b| {
        b.iter(|| {
            let mut idx = Fp16QuantizedIndex::new();
            idx.build(black_box(&vectors)).unwrap();
            idx.stats()
        })
    });
}

/// Compare FP16 vs scalar vs binary quantization
#[cfg(feature = "fp16")]
fn bench_fp16_quantization_comparison(c: &mut Criterion) {
    let dimensions = 768;
    let size = 5000;
    let k = 10;

    let vectors: Vec<(String, Vec<f32>)> = (0..size)
        .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
        .collect();

    let query = generate_query(dimensions);

    // Build FP16 quantized index
    let mut fp16_index = Fp16QuantizedIndex::new();
    fp16_index.build(&vectors).unwrap();

    // Build scalar quantized index
    let mut scalar_index = QuantizedVectorIndex::new(QuantizationConfig::default());
    scalar_index.build(&vectors).unwrap();

    // Build binary quantized index
    let mut binary_index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
    binary_index.build(&vectors).unwrap();

    let mut group = c.benchmark_group("fp16_quantization_comparison");
    group.throughput(Throughput::Elements(size as u64));

    // Benchmark FP16 quantized search
    group.bench_function("fp16_16bit", |b| {
        b.iter(|| fp16_index.search(black_box(&query), black_box(k)))
    });

    // Benchmark scalar quantized search
    group.bench_function("scalar_8bit", |b| {
        b.iter(|| scalar_index.search(black_box(&query), black_box(k)))
    });

    // Benchmark binary quantized search
    group.bench_function("binary_1bit", |b| {
        b.iter(|| binary_index.search(black_box(&query), black_box(k)))
    });

    // Print memory comparison
    println!("\n=== FP16 Quantization Comparison (5K vectors, 768 dims) ===");
    let fp16_stats = fp16_index.stats();
    let scalar_stats = scalar_index.stats();
    let binary_stats = binary_index.stats();

    println!(
        "Original (f32):  {:.2} MB (baseline)",
        fp16_stats.original_bytes as f64 / 1_000_000.0
    );
    println!(
        "FP16 (16-bit):   {:.2} MB, {:.1}x compression, {:.1}% memory savings",
        fp16_stats.fp16_bytes as f64 / 1_000_000.0,
        fp16_stats.compression_ratio,
        fp16_stats.memory_savings * 100.0
    );
    println!(
        "Scalar (8-bit):  {:.2} MB, {:.1}x compression, {:.1}% memory savings",
        scalar_stats.quantized_bytes as f64 / 1_000_000.0,
        scalar_stats.compression_ratio,
        scalar_stats.memory_savings * 100.0
    );
    println!(
        "Binary (1-bit):  {:.2} MB, {:.1}x compression, {:.1}% memory savings",
        binary_stats.binary_bytes as f64 / 1_000_000.0,
        binary_stats.compression_ratio,
        binary_stats.memory_savings * 100.0
    );
    println!("{}", "=".repeat(65));

    group.finish();
}

/// Benchmark 4-bit quantization operations
fn bench_fourbit_quantization(c: &mut Criterion) {
    let dimensions = 768;
    let size = 1000;

    let vectors: Vec<Vec<f32>> = (0..size).map(|_| generate_query(dimensions)).collect();

    let mut quantizer = FourBitQuantizer::new();
    quantizer.fit(&vectors).unwrap();

    c.bench_function("fourbit_fit", |b| {
        b.iter(|| {
            let mut q = FourBitQuantizer::new();
            q.fit(black_box(&vectors))
        })
    });

    c.bench_function("fourbit_quantize_single", |b| {
        b.iter(|| quantizer.quantize(black_box(&vectors[0])))
    });

    c.bench_function("fourbit_dequantize_single", |b| {
        let quantized = quantizer.quantize(&vectors[0]);
        b.iter(|| quantizer.dequantize(black_box(&quantized)))
    });

    c.bench_function("fourbit_quantize_batch", |b| {
        b.iter(|| quantizer.quantize_batch(black_box(&vectors)))
    });

    let quantized_vectors = quantizer.quantize_batch(&vectors);
    c.bench_function("fourbit_dequantize_batch", |b| {
        b.iter(|| quantizer.dequantize_batch(black_box(&quantized_vectors)))
    });

    let a = quantizer.quantize(&vectors[0]);
    let b_vec = quantizer.quantize(&vectors[1]);
    c.bench_function("fourbit_distance", |b| {
        b.iter(|| quantizer.quantized_distance(black_box(&a), black_box(&b_vec)))
    });
}

/// Benchmark 4-bit quantized index search
fn bench_fourbit_quantized_index(c: &mut Criterion) {
    let dimensions = 768;
    let k = 10;

    let mut group = c.benchmark_group("fourbit_quantized_index");

    for size in [1000, 5000, 10000] {
        let vectors: Vec<(String, Vec<f32>)> = (0..size)
            .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
            .collect();

        let query = generate_query(dimensions);

        // Build 4-bit quantized index
        let mut index = FourBitQuantizedIndex::new();
        index.build(&vectors).unwrap();

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| index.search(black_box(&query), black_box(k)))
        });
    }

    group.finish();
}

/// Benchmark 4-bit memory efficiency
fn bench_fourbit_quantization_memory(c: &mut Criterion) {
    let dimensions = 768;
    let size = 5000;

    let vectors: Vec<(String, Vec<f32>)> = (0..size)
        .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
        .collect();

    c.bench_function("fourbit_quantization_memory_build", |b| {
        b.iter(|| {
            let mut idx = FourBitQuantizedIndex::new();
            idx.build(black_box(&vectors))
        })
    });

    c.bench_function("fourbit_quantization_memory_stats", |b| {
        b.iter(|| {
            let mut idx = FourBitQuantizedIndex::new();
            idx.build(black_box(&vectors)).unwrap();
            idx.stats()
        })
    });
}

/// Compare all quantization methods: binary (1-bit) vs 4-bit vs scalar (8-bit) vs FP16 (16-bit) vs float32
fn bench_fourbit_quantization_comparison(c: &mut Criterion) {
    let dimensions = 768;
    let size = 5000;
    let k = 10;

    let vectors: Vec<(String, Vec<f32>)> = (0..size)
        .map(|i| (format!("doc_{}", i), generate_query(dimensions)))
        .collect();

    let query = generate_query(dimensions);

    // Build binary quantized index (1-bit)
    let mut binary_index = BinaryQuantizedIndex::new(BinaryQuantizationConfig::default());
    binary_index.build(&vectors).unwrap();

    // Build 4-bit quantized index
    let mut fourbit_index = FourBitQuantizedIndex::new();
    fourbit_index.build(&vectors).unwrap();

    // Build scalar quantized index (8-bit)
    let mut scalar_index = QuantizedVectorIndex::new(QuantizationConfig::default());
    scalar_index.build(&vectors).unwrap();

    let mut group = c.benchmark_group("fourbit_quantization_comparison");
    group.throughput(Throughput::Elements(size as u64));

    // Benchmark binary quantized search (1-bit)
    group.bench_function("binary_1bit", |b| {
        b.iter(|| binary_index.search(black_box(&query), black_box(k)))
    });

    // Benchmark 4-bit quantized search
    group.bench_function("fourbit_4bit", |b| {
        b.iter(|| fourbit_index.search(black_box(&query), black_box(k)))
    });

    // Benchmark scalar quantized search (8-bit)
    group.bench_function("scalar_8bit", |b| {
        b.iter(|| scalar_index.search(black_box(&query), black_box(k)))
    });

    // Benchmark FP16 quantized search (16-bit) if feature is enabled
    #[cfg(feature = "fp16")]
    {
        let mut fp16_index = Fp16QuantizedIndex::new();
        fp16_index.build(&vectors).unwrap();

        group.bench_function("fp16_16bit", |b| {
            b.iter(|| fp16_index.search(black_box(&query), black_box(k)))
        });
    }

    // Print memory comparison
    println!("\n=== Quantization Comparison (5K vectors, 768 dims) ===");
    let binary_stats = binary_index.stats();
    let fourbit_stats = fourbit_index.stats();
    let scalar_stats = scalar_index.stats();

    println!(
        "Original (f32):  {:.2} MB (baseline)",
        binary_stats.original_bytes as f64 / 1_000_000.0
    );
    println!(
        "Binary (1-bit):  {:.2} MB, {:.1}x compression, {:.1}% memory savings",
        binary_stats.binary_bytes as f64 / 1_000_000.0,
        binary_stats.compression_ratio,
        binary_stats.memory_savings * 100.0
    );
    println!(
        "4-bit:           {:.2} MB, {:.1}x compression, {:.1}% memory savings",
        fourbit_stats.quantized_bytes as f64 / 1_000_000.0,
        fourbit_stats.compression_ratio,
        fourbit_stats.memory_savings * 100.0
    );
    println!(
        "Scalar (8-bit):  {:.2} MB, {:.1}x compression, {:.1}% memory savings",
        scalar_stats.quantized_bytes as f64 / 1_000_000.0,
        scalar_stats.compression_ratio,
        scalar_stats.memory_savings * 100.0
    );

    #[cfg(feature = "fp16")]
    {
        let mut fp16_index = Fp16QuantizedIndex::new();
        fp16_index.build(&vectors).unwrap();
        let fp16_stats = fp16_index.stats();
        println!(
            "FP16 (16-bit):   {:.2} MB, {:.1}x compression, {:.1}% memory savings",
            fp16_stats.fp16_bytes as f64 / 1_000_000.0,
            fp16_stats.compression_ratio,
            fp16_stats.memory_savings * 100.0
        );
    }

    println!("\nMemory Efficiency Trade-offs:");
    println!("  Binary:  Highest compression, lowest accuracy");
    println!("  4-bit:   Good compression, moderate accuracy");
    println!("  8-bit:   Moderate compression, good accuracy");
    #[cfg(feature = "fp16")]
    println!("  FP16:    Low compression, high accuracy");
    println!("  float32: No compression, perfect accuracy");

    group.finish();
}

/// Benchmark GPU batch processing
fn bench_gpu_batch_processing(c: &mut Criterion) {
    let dimensions = 768;
    let num_vectors = 1000;

    let vectors: Vec<Vec<f32>> = (0..num_vectors)
        .map(|_| generate_query(dimensions))
        .collect();

    let mut group = c.benchmark_group("gpu_batch_processing");

    // Test different batch sizes
    for num_queries in [10, 50, 100, 500] {
        let queries: Vec<Vec<f32>> = (0..num_queries)
            .map(|_| generate_query(dimensions))
            .collect();

        group.throughput(Throughput::Elements((num_queries * num_vectors) as u64));

        // CPU-preferred (always uses CPU)
        let cpu_config = GpuConfig::cpu_preferred();
        let cpu_processor = GpuBatchProcessor::new(cpu_config).unwrap();

        group.bench_with_input(
            BenchmarkId::new("cpu", num_queries),
            &num_queries,
            |b, _| {
                b.iter(|| {
                    cpu_processor.batch_distance(
                        black_box(&queries),
                        black_box(&vectors),
                        DistanceMetric::Cosine,
                    )
                })
            },
        );

        // GPU-preferred (uses GPU if available)
        let gpu_config = GpuConfig::gpu_preferred();
        let gpu_processor = GpuBatchProcessor::new(gpu_config).unwrap();

        group.bench_with_input(
            BenchmarkId::new("gpu", num_queries),
            &num_queries,
            |b, _| {
                b.iter(|| {
                    gpu_processor.batch_distance(
                        black_box(&queries),
                        black_box(&vectors),
                        DistanceMetric::Cosine,
                    )
                })
            },
        );
    }

    group.finish();
}

/// Benchmark GPU vs CPU for different distance metrics
fn bench_gpu_distance_metrics(c: &mut Criterion) {
    let dimensions = 768;
    let num_vectors = 1000;
    let num_queries = 100;

    let vectors: Vec<Vec<f32>> = (0..num_vectors)
        .map(|_| generate_query(dimensions))
        .collect();

    let queries: Vec<Vec<f32>> = (0..num_queries)
        .map(|_| generate_query(dimensions))
        .collect();

    let cpu_config = GpuConfig::cpu_preferred();
    let cpu_processor = GpuBatchProcessor::new(cpu_config).unwrap();

    let metrics = vec![
        ("cosine", DistanceMetric::Cosine),
        ("euclidean", DistanceMetric::Euclidean),
        ("dot_product", DistanceMetric::DotProduct),
        ("manhattan", DistanceMetric::Manhattan),
    ];

    let mut group = c.benchmark_group("gpu_distance_metrics");
    group.throughput(Throughput::Elements((num_queries * num_vectors) as u64));

    for (name, metric) in metrics {
        group.bench_function(name, |b| {
            b.iter(|| {
                cpu_processor.batch_distance(black_box(&queries), black_box(&vectors), metric)
            })
        });
    }

    group.finish();
}

/// Benchmark GPU scalability with increasing dataset size
fn bench_gpu_scalability(c: &mut Criterion) {
    let dimensions = 768;
    let num_queries = 100;

    let cpu_config = GpuConfig::cpu_preferred();
    let cpu_processor = GpuBatchProcessor::new(cpu_config).unwrap();

    let mut group = c.benchmark_group("gpu_scalability");

    for num_vectors in [100, 500, 1000, 5000] {
        let vectors: Vec<Vec<f32>> = (0..num_vectors)
            .map(|_| generate_query(dimensions))
            .collect();

        let queries: Vec<Vec<f32>> = (0..num_queries)
            .map(|_| generate_query(dimensions))
            .collect();

        group.throughput(Throughput::Elements((num_queries * num_vectors) as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_vectors),
            &num_vectors,
            |b, _| {
                b.iter(|| {
                    cpu_processor.batch_distance(
                        black_box(&queries),
                        black_box(&vectors),
                        DistanceMetric::Cosine,
                    )
                })
            },
        );
    }

    group.finish();
}

/// Benchmark GPU automatic dispatch threshold
fn bench_gpu_dispatch_threshold(c: &mut Criterion) {
    let dimensions = 768;

    let mut group = c.benchmark_group("gpu_dispatch_threshold");

    // Test around the default threshold of 100 operations
    for num_ops in [50, 100, 200, 500] {
        // Create queries and vectors that result in num_ops total operations
        let num_queries = (num_ops as f64).sqrt() as usize;
        let num_vectors = num_ops / num_queries;

        let vectors: Vec<Vec<f32>> = (0..num_vectors)
            .map(|_| generate_query(dimensions))
            .collect();

        let queries: Vec<Vec<f32>> = (0..num_queries)
            .map(|_| generate_query(dimensions))
            .collect();

        let config = GpuConfig::default(); // threshold = 100
        let processor = GpuBatchProcessor::new(config).unwrap();

        group.throughput(Throughput::Elements(num_ops as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_ops), &num_ops, |b, _| {
            b.iter(|| {
                processor.batch_distance(
                    black_box(&queries),
                    black_box(&vectors),
                    DistanceMetric::Cosine,
                )
            })
        });
    }

    group.finish();
}

#[cfg(feature = "fp16")]
criterion_group!(
    benches,
    bench_exact_search,
    bench_parallel_search,
    bench_distance_metrics,
    bench_hnsw,
    bench_hnsw_vs_exact,
    bench_build_index,
    bench_filtered_search,
    bench_batch_search,
    bench_recall_accuracy,
    bench_distributed_search,
    bench_distributed_vs_centralized,
    bench_distributed_batch,
    bench_distributed_filtered,
    bench_simd_optimizations,
    bench_incremental_updates,
    bench_query_optimizer,
    bench_scalar_quantization,
    bench_quantized_index,
    bench_quantization_memory,
    bench_binary_quantization,
    bench_binary_quantized_index,
    bench_binary_quantization_memory,
    bench_quantization_comparison,
    bench_quantized_simd,
    bench_fourbit_quantization,
    bench_fourbit_quantized_index,
    bench_fourbit_quantization_memory,
    bench_fourbit_quantization_comparison,
    bench_fp16_quantization,
    bench_fp16_quantized_index,
    bench_fp16_quantization_memory,
    bench_fp16_quantization_comparison,
    bench_gpu_batch_processing,
    bench_gpu_distance_metrics,
    bench_gpu_scalability,
    bench_gpu_dispatch_threshold,
);

#[cfg(not(feature = "fp16"))]
criterion_group!(
    benches,
    bench_exact_search,
    bench_parallel_search,
    bench_distance_metrics,
    bench_hnsw,
    bench_hnsw_vs_exact,
    bench_build_index,
    bench_filtered_search,
    bench_batch_search,
    bench_recall_accuracy,
    bench_distributed_search,
    bench_distributed_vs_centralized,
    bench_distributed_batch,
    bench_distributed_filtered,
    bench_simd_optimizations,
    bench_incremental_updates,
    bench_query_optimizer,
    bench_scalar_quantization,
    bench_quantized_index,
    bench_quantization_memory,
    bench_binary_quantization,
    bench_binary_quantized_index,
    bench_binary_quantization_memory,
    bench_quantization_comparison,
    bench_quantized_simd,
    bench_fourbit_quantization,
    bench_fourbit_quantized_index,
    bench_fourbit_quantization_memory,
    bench_fourbit_quantization_comparison,
    bench_gpu_batch_processing,
    bench_gpu_distance_metrics,
    bench_gpu_scalability,
    bench_gpu_dispatch_threshold,
);

criterion_main!(benches);
