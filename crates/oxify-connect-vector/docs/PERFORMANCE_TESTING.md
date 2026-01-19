# Performance Testing & Regression Detection

This document describes the performance testing infrastructure for `oxify-connect-vector`.

## Overview

The crate includes:
1. **Benchmark Suite**: Comprehensive benchmarks for all major features
2. **Performance Regression Testing**: Automated detection of performance degradation
3. **CI/CD Integration**: Continuous performance monitoring in GitHub Actions

## Benchmark Suite

### Available Benchmarks

1. **`vector_bench`**: Core vector search operations
   - Search latency (100, 1k, 10k vectors)
   - Insert throughput
   - Batch operations
   - Dimension scaling (64-1024 dimensions)

2. **`hybrid_bench`**: Hybrid search benchmarks
   - BM25 search performance
   - RRF fusion overhead
   - Combined semantic + keyword search

3. **`cache_bench`**: Caching performance
   - Embedding cache hit/miss rates
   - Search cache performance
   - Cache eviction overhead

4. **`colbert_bench`**: Multi-vector search
   - ColBERT-style MaxSim computation
   - Multi-vector insert/search

### Running Benchmarks Locally

```bash
# Run all benchmarks
cd crates/oxify-connect-vector
cargo bench

# Run specific benchmark
cargo bench --bench vector_bench

# Run specific test within a benchmark
cargo bench --bench vector_bench -- search_latency
```

### Benchmark Output

Benchmarks use [Criterion.rs](https://github.com/bheisler/criterion.rs) and generate:
- HTML reports in `target/criterion/`
- Statistical analysis (mean, median, std dev)
- Performance change detection vs. previous runs

## Performance Regression Testing

### Local Testing

Use the `perf_regression.sh` script to compare performance between branches:

```bash
# Compare current branch with main (10% threshold)
./perf_regression.sh main HEAD 10

# Compare two specific commits
./perf_regression.sh abc123 def456 15

# Compare with custom threshold (5% degradation allowed)
./perf_regression.sh main HEAD 5
```

**Workflow:**
1. Checks out baseline ref (e.g., `main`)
2. Runs all benchmarks and saves as `baseline`
3. Checks out current ref (e.g., your branch)
4. Runs all benchmarks and saves as `current`
5. Compares results using `critcmp`
6. Fails if degradation exceeds threshold

**Exit Codes:**
- `0`: No significant regression
- `1`: Performance regression detected

### CI/CD Integration

#### Standard CI (`.github/workflows/oxify-connect-vector-ci.yml`)

Runs on every push and PR:
- Unit tests
- Integration tests (with Docker services)
- Clippy linting
- Format checking
- Basic benchmarks (no regression testing)

**Services:**
- Qdrant (ports 6333, 6334)
- PostgreSQL with pgvector (port 5432)
- ChromaDB (port 8000)

#### Performance Regression CI (`.github/workflows/oxify-connect-vector-perf.yml`)

Runs on PRs to `main`:
- Benchmarks PR branch
- Benchmarks main branch
- Compares results using `critcmp`
- Uploads comparison report as artifact
- Warns on >10% degradation

**When to use:**
- Before merging performance-critical changes
- After major refactorings
- When investigating performance issues

### Interpreting Results

Example `critcmp` output:

```
group                           baseline              current
-----                           --------              -------
search/100_vectors              1.00    234.5±3.2µs   1.02    239.1±4.1µs
search/1k_vectors               1.00    2.45±0.05ms   1.15    2.82±0.08ms  ⚠️ +15.1%
search/10k_vectors              1.00    28.3±0.9ms    0.98    27.7±1.1ms   ✓ -2.1%
```

**Reading the output:**
- `1.00` = baseline (reference)
- `1.15` = 15% slower than baseline ⚠️
- `0.98` = 2% faster than baseline ✓
- Changes <5% are typically noise
- Changes >10% should be investigated

## Performance Optimization Tips

### 1. Vector Search

```rust
// ❌ Avoid: Small batch sizes
for vector in vectors {
    provider.insert(vector).await?;
}

// ✅ Prefer: Batch operations
provider.batch_insert(BatchInsertRequest {
    collection: "docs".into(),
    vectors: all_vectors,
}).await?;
```

### 2. Caching

```rust
// ✅ Use embedding cache for repeated queries
let cache = EmbeddingCache::new(1000, Duration::from_secs(3600));
let cached_provider = cache.wrap(embedding_provider);
```

### 3. Hybrid Search

```rust
// ✅ Tune BM25 parameters for your data
let bm25_params = Bm25Params {
    k1: 1.2,  // Lower for shorter documents
    b: 0.75,  // Default is usually good
};
```

### 4. Dimension Optimization

- Use 768 dimensions for general-purpose embeddings (BERT)
- Use 1536 dimensions for OpenAI `text-embedding-3-small`
- Use 384 dimensions for faster search with minimal accuracy loss

### 5. Provider-Specific Tips

| Provider | Optimization |
|----------|--------------|
| Qdrant | Use HNSW index with m=16, ef_construct=100 |
| pgvector | Use HNSW index: `CREATE INDEX ON vectors USING hnsw (embedding vector_cosine_ops)` |
| ChromaDB | Enable persistence, use batch operations |
| Pinecone | Use pod-based indexes for best performance |
| Weaviate | Configure `vectorIndexConfig` with appropriate `efConstruction` |
| Milvus | Use IVF_FLAT for accuracy, IVF_SQ8 for speed |

## Baseline Performance Metrics

These are approximate metrics on an AMD Ryzen 7 5800X (8 cores, 16 threads):

| Operation | 100 vectors | 1k vectors | 10k vectors |
|-----------|-------------|------------|-------------|
| Search | 150-250µs | 1.5-2.5ms | 15-30ms |
| Insert | 80-120µs | 800-1200µs | 8-12ms |
| Batch Insert | 50-80µs/vec | 40-70µs/vec | 35-60µs/vec |

| Feature | Latency | Notes |
|---------|---------|-------|
| BM25 Search | 5-15µs | 1000 documents |
| Hybrid Search | 2-4ms | Semantic + BM25 fusion |
| Cache Hit | 10-50ns | Embedding cache |
| Cache Miss | 200-400ms | OpenAI API call |
| ColBERT MaxSim | 100-300µs | 10 query vectors × 20 doc vectors |

**Note:** These metrics are for reference only. Actual performance depends on:
- Hardware (CPU, RAM, SSD speed)
- Vector dimensions
- Data distribution
- Provider backend (local vs. cloud)

## Continuous Performance Monitoring

### Setting Up Alerts

1. Enable GitHub Actions notifications
2. Monitor benchmark artifacts in Actions tab
3. Review performance comparison reports in PRs
4. Set up custom alerts using GitHub API:

```bash
# Example: Get latest benchmark results
gh run download <run-id> -n benchmark-results
```

### Performance Tracking

Track key metrics over time:
- Search latency at 99th percentile
- Batch operation throughput
- Cache hit rates
- Memory usage (use `cargo bloat`)

### Regression Response

When regression is detected:

1. **Investigate**: Review the code changes causing regression
2. **Profile**: Use `cargo flamegraph` to identify hotspots
3. **Optimize**: Apply targeted optimizations
4. **Verify**: Run `perf_regression.sh` to confirm fix
5. **Document**: Update performance notes in PR/commit

## Advanced Performance Analysis

### Profiling with `perf`

```bash
# Record performance data
cargo build --release
perf record --call-graph=dwarf ./target/release/benchmark

# Analyze hotspots
perf report
```

### Flame Graphs

```bash
# Install cargo-flamegraph
cargo install flamegraph

# Generate flame graph
cargo flamegraph --bench vector_bench
# Opens flamegraph.svg in browser
```

### Memory Profiling

```bash
# Check binary size
cargo bloat --release -n 20

# Memory usage with valgrind
valgrind --tool=massif ./target/release/benchmark
ms_print massif.out.<pid>
```

## Resources

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/)
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [critcmp Documentation](https://github.com/BurntSushi/critcmp)
- [Rust Profiling Guide](https://nnethercote.github.io/perf-book/profiling.html)

## License

MIT OR Apache-2.0
