//! Benchmarks for the authorization engine
//!
//! Run with: cargo bench -p oxify-authz
//!
//! Performance targets:
//! - Cached checks: <100μs (p99)
//! - Uncached checks: <3ms (p99)
//! - Transitive checks: <10ms (p99)
//! - Batch checks (100): <50ms total

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_authz::*;
use rand::{Rng, RngExt};
use std::hint::black_box;

/// Benchmark in-memory authorization checks
fn bench_memory_check(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup: Create manager with test data
    let manager = rt.block_on(async {
        let manager = InMemoryRebacManager::new();

        // Add 1000 tuples
        for i in 0..1000 {
            let tuple = RelationTuple::new(
                "document",
                "viewer",
                format!("doc{}", i),
                Subject::User(format!("user{}", i % 100)),
            );
            manager.add_tuple(tuple).await.unwrap();
        }

        manager
    });

    let mut group = c.benchmark_group("memory_check");
    group.throughput(Throughput::Elements(1));

    // Benchmark single check (existing tuple)
    group.bench_function("single_existing", |b| {
        b.to_async(&rt).iter(|| async {
            let request = CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc50".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("user50".to_string()),
                context: None,
            };
            black_box(manager.check(&request).await.unwrap())
        });
    });

    // Benchmark single check (non-existing tuple)
    group.bench_function("single_non_existing", |b| {
        b.to_async(&rt).iter(|| async {
            let request = CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc9999".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("unknown".to_string()),
                context: None,
            };
            black_box(manager.check(&request).await.unwrap())
        });
    });

    group.finish();
}

/// Benchmark batch checks
fn bench_batch_check(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup: Create manager with test data
    let manager = rt.block_on(async {
        let manager = InMemoryRebacManager::new();

        // Add 10000 tuples
        for i in 0..10000 {
            let tuple = RelationTuple::new(
                "document",
                "viewer",
                format!("doc{}", i),
                Subject::User(format!("user{}", i % 1000)),
            );
            manager.add_tuple(tuple).await.unwrap();
        }

        manager
    });

    let mut group = c.benchmark_group("batch_check");

    for batch_size in [10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("mixed", batch_size),
            batch_size,
            |b, &size| {
                // Create batch with 50% existing, 50% non-existing
                let mut rng = rand::rng();
                let requests: Vec<CheckRequest> = (0..size)
                    .map(|i| {
                        if i % 2 == 0 {
                            // Existing tuple
                            let doc_id = rng.random_range(0..10000);
                            CheckRequest {
                                namespace: "document".to_string(),
                                object_id: format!("doc{}", doc_id),
                                relation: "viewer".to_string(),
                                subject: Subject::User(format!("user{}", doc_id % 1000)),
                                context: None,
                            }
                        } else {
                            // Non-existing tuple
                            CheckRequest {
                                namespace: "document".to_string(),
                                object_id: format!("doc{}", rng.random_range(20000..30000)),
                                relation: "viewer".to_string(),
                                subject: Subject::User("unknown".to_string()),
                                context: None,
                            }
                        }
                    })
                    .collect();

                b.to_async(&rt)
                    .iter(|| async { black_box(manager.batch_check(&requests).await.unwrap()) });
            },
        );
    }

    group.finish();
}

/// Benchmark Bloom filter operations
fn bench_bloom_filter(c: &mut Criterion) {
    // Setup: Create Bloom filter with test data
    let bloom = AuthzBloomFilter::with_config(BloomConfig {
        expected_items: 100_000,
        false_positive_rate: 0.01,
    });

    // Add 50000 tuples
    for i in 0..50000 {
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            format!("doc{}", i),
            Subject::User(format!("user{}", i % 1000)),
        );
        bloom.add_tuple(&tuple);
    }

    let mut group = c.benchmark_group("bloom_filter");
    group.throughput(Throughput::Elements(1));

    // Benchmark single lookup (existing)
    group.bench_function("lookup_existing", |b| {
        b.iter(|| {
            let request = CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc25000".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("user0".to_string()),
                context: None,
            };
            black_box(bloom.might_contain(&request))
        });
    });

    // Benchmark single lookup (non-existing)
    group.bench_function("lookup_non_existing", |b| {
        b.iter(|| {
            let request = CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc99999".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("unknown".to_string()),
                context: None,
            };
            black_box(bloom.might_contain(&request))
        });
    });

    // Benchmark batch lookup
    group.throughput(Throughput::Elements(100));
    group.bench_function("batch_100", |b| {
        let requests: Vec<CheckRequest> = (0..100)
            .map(|i| CheckRequest {
                namespace: "document".to_string(),
                object_id: format!("doc{}", i * 500),
                relation: "viewer".to_string(),
                subject: Subject::User(format!("user{}", i % 100)),
                context: None,
            })
            .collect();

        b.iter(|| black_box(bloom.might_contain_batch(&requests)));
    });

    group.finish();
}

/// Benchmark tuple write operations
fn bench_write(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("write");
    group.throughput(Throughput::Elements(1));

    group.bench_function("memory_write", |b| {
        let manager = InMemoryRebacManager::new();
        let mut counter = 0u64;

        b.to_async(&rt).iter(|| {
            counter += 1;
            let tuple = RelationTuple::new(
                "document",
                "viewer",
                format!("doc{}", counter),
                Subject::User(format!("user{}", counter % 100)),
            );
            let m = &manager;
            async move {
                m.add_tuple(tuple).await.unwrap();
                black_box(())
            }
        });
    });

    group.finish();
}

/// Benchmark cache statistics
fn bench_cache_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_stats");

    group.bench_function("hit_rate_calculation", |b| {
        let stats = CacheStats {
            hits: 95000,
            misses: 5000,
            evictions: 1000,
        };

        b.iter(|| black_box(stats.hit_rate()));
    });

    group.bench_function("bloom_stats_calculation", |b| {
        let stats = BloomStats {
            definite_negatives: 50000,
            potential_positives: 50000,
            true_positives: 45000,
            false_positives: 5000,
        };

        b.iter(|| {
            black_box(stats.query_reduction_rate());
            black_box(stats.actual_fp_rate());
        });
    });

    group.finish();
}

/// Benchmark Leopard index operations
fn bench_leopard_index(c: &mut Criterion) {
    use std::collections::HashMap;
    use std::sync::Arc;

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup: Create Leopard index with namespace configs
    let index = rt.block_on(async {
        let mut configs = HashMap::new();
        configs.insert(
            "document".to_string(),
            NamespaceConfig::document_namespace(),
        );
        let index = LeopardIndex::new(Arc::new(configs));

        // Add 10000 tuples with inheritance
        for i in 0..10000 {
            let tuple = RelationTuple::new(
                "document",
                "owner", // This will expand to editor and viewer
                format!("doc{}", i),
                Subject::User(format!("user{}", i % 1000)),
            );
            index.index_tuple(&tuple).await.unwrap();
        }

        index
    });

    let mut group = c.benchmark_group("leopard_index");
    group.throughput(Throughput::Elements(1));

    // Benchmark O(1) lookup (existing - direct)
    group.bench_function("lookup_direct", |b| {
        b.to_async(&rt).iter(|| async {
            let request = CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc5000".to_string(),
                relation: "owner".to_string(),
                subject: Subject::User("user0".to_string()),
                context: None,
            };
            black_box(index.check(&request).await)
        });
    });

    // Benchmark O(1) lookup (existing - inherited)
    group.bench_function("lookup_inherited", |b| {
        b.to_async(&rt).iter(|| async {
            let request = CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc5000".to_string(),
                relation: "viewer".to_string(), // Inherited from owner
                subject: Subject::User("user0".to_string()),
                context: None,
            };
            black_box(index.check(&request).await)
        });
    });

    // Benchmark O(1) lookup (non-existing)
    group.bench_function("lookup_non_existing", |b| {
        b.to_async(&rt).iter(|| async {
            let request = CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc99999".to_string(),
                relation: "owner".to_string(),
                subject: Subject::User("unknown".to_string()),
                context: None,
            };
            black_box(index.check(&request).await)
        });
    });

    // Benchmark expand operation
    group.bench_function("expand", |b| {
        b.to_async(&rt)
            .iter(|| async { black_box(index.expand("document", "doc5000", "viewer").await) });
    });

    group.finish();
}

/// Benchmark Leopard index write operations
fn bench_leopard_write(c: &mut Criterion) {
    use std::collections::HashMap;
    use std::sync::Arc;

    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("leopard_write");
    group.throughput(Throughput::Elements(1));

    group.bench_function("index_with_inheritance", |b| {
        let mut configs = HashMap::new();
        configs.insert(
            "document".to_string(),
            NamespaceConfig::document_namespace(),
        );
        let index = LeopardIndex::new(Arc::new(configs));
        let mut counter = 0u64;

        b.to_async(&rt).iter(|| {
            counter += 1;
            let tuple = RelationTuple::new(
                "document",
                "owner",
                format!("doc{}", counter),
                Subject::User(format!("user{}", counter % 100)),
            );
            let idx = &index;
            async move { black_box(idx.index_tuple(&tuple).await.unwrap()) }
        });
    });

    group.finish();
}

/// Benchmark Redis cache operations (requires Redis server)
fn bench_redis_cache(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Try to create Redis cache, skip if Redis is not available
    let cache_opt = rt.block_on(async {
        let config = RedisCacheConfig::default();
        match RedisCache::new(config) {
            Ok(mut cache) => {
                if cache.connect().await.is_ok() {
                    Some(cache)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    });

    if cache_opt.is_none() {
        // Skip benchmark if Redis is not available
        eprintln!("Skipping Redis cache benchmark: Redis server not available");
        return;
    }

    let cache = cache_opt.unwrap();

    let mut group = c.benchmark_group("redis_cache");
    group.throughput(Throughput::Elements(1));

    // Benchmark SET operation
    group.bench_function("set_permission", |b| {
        let mut counter = 0u64;
        b.to_async(&rt).iter(|| {
            counter += 1;
            let key = PermissionCacheKey::new(
                "document".to_string(),
                format!("doc{}", counter),
                "viewer".to_string(),
                Subject::User(format!("user{}", counter % 100)),
            );
            let c = &cache;
            async move {
                c.set_permission(&key, true, None).await.unwrap();
                black_box(())
            }
        });
    });

    // Benchmark GET operation (cache hit)
    group.bench_function("get_permission_hit", |b| {
        // Warm up cache
        let key = PermissionCacheKey::new(
            "document".to_string(),
            "warmup_doc".to_string(),
            "viewer".to_string(),
            Subject::User("warmup_user".to_string()),
        );
        rt.block_on(async {
            cache.set_permission(&key, true, None).await.unwrap();
        });

        b.to_async(&rt).iter(|| {
            let k = &key;
            let c = &cache;
            async move { black_box(c.get_permission(k).await.unwrap()) }
        });
    });

    // Benchmark GET operation (cache miss)
    group.bench_function("get_permission_miss", |b| {
        let mut counter = 0u64;
        b.to_async(&rt).iter(|| {
            counter += 1;
            let key = PermissionCacheKey::new(
                "document".to_string(),
                format!("nonexistent{}", counter),
                "viewer".to_string(),
                Subject::User("unknown".to_string()),
            );
            let c = &cache;
            async move { black_box(c.get_permission(&key).await.unwrap()) }
        });
    });

    // Benchmark invalidate operation
    group.bench_function("invalidate", |b| {
        let mut counter = 0u64;
        b.to_async(&rt).iter(|| {
            counter += 1;
            let key = PermissionCacheKey::new(
                "document".to_string(),
                format!("doc{}", counter),
                "viewer".to_string(),
                Subject::User(format!("user{}", counter % 100)),
            );
            let c = &cache;
            async move {
                c.invalidate(&key).await.unwrap();
                black_box(())
            }
        });
    });

    group.finish();
}

/// Benchmark edge engine operations
fn bench_edge_engine(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("edge_engine");

    // Benchmark: Edge engine authorization check (in-memory, no sync)
    group.bench_function("check", |b| {
        let engine = rt.block_on(async {
            let config = oxify_authz::edge::EdgeConfig::default();
            let engine = oxify_authz::edge::EdgeEngine::new(config).await.unwrap();

            // Pre-populate with test data
            for i in 0..1000 {
                let tuple = RelationTuple::new(
                    "document",
                    "viewer",
                    format!("doc{}", i),
                    Subject::User(format!("user{}", i % 100)),
                );
                engine.write_tuple(tuple).await.unwrap();
            }
            engine
        });

        b.to_async(&rt).iter(|| async {
            black_box(
                engine
                    .check("document", "doc500", "viewer", "user50")
                    .await
                    .unwrap(),
            )
        });
    });

    // Benchmark: Edge engine write with CRDT
    group.bench_function("write_crdt", |b| {
        let engine = rt.block_on(async {
            let config = oxify_authz::edge::EdgeConfig::default();
            oxify_authz::edge::EdgeEngine::new(config).await.unwrap()
        });

        let mut counter = 0u64;
        b.to_async(&rt).iter(|| {
            let tuple = RelationTuple::new(
                "document",
                "viewer",
                format!("doc{}", counter),
                Subject::User(format!("user{}", counter % 100)),
            );
            counter += 1;
            let e = &engine;
            async move {
                let _: () = e.write_tuple(tuple).await.unwrap();
                black_box(())
            }
        });
    });

    // Benchmark: CRDT merge (conflict resolution)
    group.bench_function("crdt_merge", |b| {
        let engine = rt.block_on(async {
            let config = oxify_authz::edge::EdgeConfig::default();
            oxify_authz::edge::EdgeEngine::new(config).await.unwrap()
        });

        b.to_async(&rt).iter(|| {
            let tuple = RelationTuple::new(
                "document",
                "viewer",
                "doc123",
                Subject::User("alice".to_string()),
            );
            let remote_crdt = oxify_authz::edge::CrdtTuple::new(tuple, "remote-node".to_string());

            let e = &engine;
            async move { black_box(e.merge_remote_tuples(vec![remote_crdt]).await.unwrap()) }
        });
    });

    // Benchmark: Batch merge from remote edge node
    group.bench_function("batch_merge", |b| {
        let engine = rt.block_on(async {
            let config = oxify_authz::edge::EdgeConfig::default();
            oxify_authz::edge::EdgeEngine::new(config).await.unwrap()
        });

        b.to_async(&rt).iter(|| {
            let mut remote_tuples = Vec::new();
            for i in 0..100 {
                let tuple = RelationTuple::new(
                    "document",
                    "viewer",
                    format!("doc{}", i),
                    Subject::User(format!("user{}", i % 20)),
                );
                remote_tuples.push(oxify_authz::edge::CrdtTuple::new(
                    tuple,
                    "remote-node".to_string(),
                ));
            }

            let e = &engine;
            async move { black_box(e.merge_remote_tuples(remote_tuples).await.unwrap()) }
        });
    });

    // Benchmark: Tombstone garbage collection
    group.bench_function("gc_tombstones", |b| {
        let engine = rt.block_on(async {
            let config = oxify_authz::edge::EdgeConfig::default();
            let engine = oxify_authz::edge::EdgeEngine::new(config).await.unwrap();

            // Create some tuples and delete them (create tombstones)
            for i in 0..100 {
                let tuple = RelationTuple::new(
                    "document",
                    "viewer",
                    format!("doc{}", i),
                    Subject::User(format!("user{}", i)),
                );
                engine.write_tuple(tuple.clone()).await.unwrap();
                engine.delete_tuple(tuple).await.unwrap();
            }
            engine
        });

        b.to_async(&rt).iter(|| async {
            let _: () = engine.gc_tombstones(3600).await.unwrap();
            black_box(())
        });
    });

    group.finish();
}

/// Benchmark quantum-safe cryptography operations
fn bench_quantum_crypto(c: &mut Criterion) {
    use oxify_authz::quantum::*;

    let mut group = c.benchmark_group("quantum_crypto");

    // Benchmark keypair generation for different algorithms
    for algo in [
        QuantumAlgorithm::MlDsa65,
        QuantumAlgorithm::HybridEd25519MlDsa,
    ] {
        group.bench_with_input(
            BenchmarkId::new("keypair_gen", format!("{:?}", algo)),
            &algo,
            |b, &algo| {
                b.iter(|| {
                    let _keypair = QuantumKeypair::generate_with_algorithm(algo).unwrap();
                    black_box(())
                });
            },
        );
    }

    // Benchmark signing
    let keypair = QuantumKeypair::generate_with_algorithm(QuantumAlgorithm::MlDsa65).unwrap();
    let data = b"user:alice|document:123|viewer";

    group.bench_function("sign_dilithium", |b| {
        b.iter(|| {
            let sig = keypair.sign(data).unwrap();
            black_box(sig)
        });
    });

    // Benchmark verification
    let signature = keypair.sign(data).unwrap();
    group.bench_function("verify_dilithium", |b| {
        b.iter(|| {
            let valid = keypair.verify(data, &signature).unwrap();
            black_box(valid)
        });
    });

    // Benchmark hybrid signing
    let hybrid_keypair =
        QuantumKeypair::generate_with_algorithm(QuantumAlgorithm::HybridEd25519MlDsa).unwrap();

    group.bench_function("sign_hybrid", |b| {
        b.iter(|| {
            let sig = hybrid_keypair.sign(data).unwrap();
            black_box(sig)
        });
    });

    // Benchmark key rotation
    group.bench_function("key_rotation", |b| {
        b.iter(|| {
            let mut manager = QuantumKeyManager::new(QuantumAlgorithm::MlDsa65).unwrap();
            manager.rotate_keys().unwrap();
            black_box(())
        });
    });

    group.finish();
}

/// Benchmark zero-knowledge proof operations
fn bench_zkp(c: &mut Criterion) {
    use oxify_authz::zkp::*;

    let mut group = c.benchmark_group("zkp");

    // Benchmark proof generation for different schemes
    for scheme in [
        ZkProofScheme::Groth16,
        ZkProofScheme::Plonk,
        ZkProofScheme::Bulletproofs,
    ] {
        group.bench_with_input(
            BenchmarkId::new("prove", format!("{:?}", scheme)),
            &scheme,
            |b, &scheme| {
                b.iter(|| {
                    let mut prover = ZkProver::with_scheme(scheme);
                    let proof = prover
                        .prove_permission("alice", "doc123", "viewer", &["viewer"])
                        .unwrap();
                    black_box(proof)
                });
            },
        );
    }

    // Benchmark proof verification
    let mut prover = ZkProver::new();
    let proof = prover
        .prove_permission("alice", "doc123", "viewer", &["viewer"])
        .unwrap();

    group.bench_function("verify_groth16", |b| {
        b.iter(|| {
            let mut verifier = ZkVerifier::new();
            // Clone proof since verify consumes nonce
            let proof_clone = proof.clone();
            let valid = verifier.verify_permission_proof(&proof_clone).unwrap();
            black_box(valid)
        });
    });

    // Benchmark batch verification (10 proofs)
    let mut batch_prover = ZkProver::new();
    let batch_proofs: Vec<_> = (0..10)
        .map(|i| {
            batch_prover
                .prove_permission(
                    &format!("user{}", i),
                    &format!("doc{}", i),
                    "viewer",
                    &["viewer"],
                )
                .unwrap()
        })
        .collect();

    group.bench_function("batch_verify_10", |b| {
        b.iter(|| {
            let mut verifier = ZkVerifier::new();
            let results = verifier.batch_verify(&batch_proofs).unwrap();
            black_box(results)
        });
    });

    // Benchmark aggregate proofs
    group.bench_function("aggregate_5_proofs", |b| {
        b.iter(|| {
            let mut agg_prover = ZkProver::new();
            let proofs: Vec<_> = (0..5)
                .map(|i| {
                    agg_prover
                        .prove_permission(&format!("user{}", i), "doc", "viewer", &["viewer"])
                        .unwrap()
                })
                .collect();
            let aggregate = AggregateProof::aggregate(proofs).unwrap();
            black_box(aggregate)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_memory_check,
    bench_batch_check,
    bench_bloom_filter,
    bench_write,
    bench_cache_stats,
    bench_leopard_index,
    bench_leopard_write,
    bench_redis_cache,
    bench_edge_engine,
    bench_quantum_crypto,
    bench_zkp,
);

criterion_main!(benches);
