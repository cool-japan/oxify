# oxify-connect-vector

Vector database connections and abstractions for the OxiFY workflow engine.

## Overview

**Status: Production-ready** ✅

Provides a unified interface for vector databases with advanced features like hybrid search, caching, reranking, and filtering.

## Features

### Core Capabilities
- **6 Vector Database Providers**: Qdrant, pgvector, ChromaDB, Pinecone, Weaviate, Milvus
- **Batch Operations**: Efficient batch insert with native APIs for all providers
- **Parallel Batch Operations**: High-throughput parallel insert and search operations
- **Update Operations**: Update vectors and/or payloads in-place
- **Collection Management**: Create, check existence, and get statistics
- **Unified Filtering**: Expression-based filter language that works across all providers

### Advanced Search
- **Hybrid Search**: Combine semantic vector search with BM25 keyword search using Reciprocal Rank Fusion
- **ColBERT-style Multi-Vector Search**: Store multiple vectors per document with MaxSim scoring
- **Reranking**: Multiple strategies (Cohere API, MMR, keyword boost, custom scorers)
- **Advanced Caching**: LRU caching for embeddings and search results with TTL

### Data Management
- **Migration Tools**: Export/import collections between providers with verification
- **Batch Processing**: Optimized batch operations for all providers
- **Data Portability**: VectorSnapshot for backup/restore with JSON serialization

### Observability & Quality
- **Metrics Collection**: Comprehensive operation tracking (latency, errors, throughput)
- **Health Monitoring**: Provider health checks with status tracking
- **CI/CD Integration**: Automated testing and performance regression detection
- **Mock Provider**: In-memory provider for testing without a database
- **Zero Warnings**: Production-ready code with 120 comprehensive tests (91 unit + 25 doc + 4 integration)

## Quick Start

### Basic Vector Search

```rust
use oxify_connect_vector::{QdrantProvider, VectorProvider, SearchRequest, InsertRequest};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Qdrant
    let provider = QdrantProvider::new("http://localhost:6334").await?;

    // Create a collection
    provider.create_collection("documents", 384).await?;

    // Insert vectors
    provider.insert(InsertRequest {
        collection: "documents".to_string(),
        id: "doc1".to_string(),
        vector: vec![0.1; 384],
        payload: json!({"title": "Machine Learning Basics"}),
    }).await?;

    // Search
    let results = provider.search(SearchRequest {
        collection: "documents".to_string(),
        query: vec![0.1; 384],
        top_k: 10,
        score_threshold: Some(0.7),
        filter: None,
    }).await?;

    for result in results {
        println!("Score: {:.3}, ID: {}", result.score, result.id);
        println!("Title: {}", result.payload["title"]);
    }

    Ok(())
}
```

### Hybrid Search

Combine semantic vector search with BM25 keyword search for better accuracy:

```rust
use oxify_connect_vector::{
    HybridSearchEngine, HybridSearchParams,
    Bm25Index, Bm25Document, Bm25Params
};

// Create BM25 index
let mut bm25 = Bm25Index::new(Bm25Params::default());
bm25.add_document(Bm25Document {
    id: "doc1".to_string(),
    text: "machine learning algorithms and deep neural networks".to_string(),
});
bm25.add_document(Bm25Document {
    id: "doc2".to_string(),
    text: "natural language processing with transformers".to_string(),
});

// Create hybrid search engine
let engine = HybridSearchEngine::new(
    vector_provider,
    bm25,
    HybridSearchParams {
        semantic_weight: 0.7,  // 70% weight on vector similarity
        keyword_weight: 0.3,   // 30% weight on keyword match
        rrf_k: 60,             // Reciprocal Rank Fusion parameter
    },
);

// Search with both vector and text query
let results = engine.search(
    "documents".to_string(),
    query_vector,
    "machine learning neural networks".to_string(),
    10,
).await?;
```

### Caching for Performance

```rust
use oxify_connect_vector::{EmbeddingCache, SearchCache};
use std::time::Duration;

// Cache embeddings to avoid redundant API calls
let embedding_cache = EmbeddingCache::new(1000, Duration::from_secs(3600));

// Check cache before generating
if let Some(cached) = embedding_cache.get("my text", "text-embedding-3-small") {
    println!("Cache hit! Using cached embedding");
    cached
} else {
    // Generate and cache
    let embedding = generate_embedding("my text").await?;
    embedding_cache.insert("my text", "text-embedding-3-small", embedding.clone());
    embedding
}

// Cache search results
let search_cache = SearchCache::new(1000, Duration::from_secs(600));

// Get cache statistics
let stats = embedding_cache.stats();
println!("Hit rate: {:.2}%", stats.hit_rate * 100.0);
println!("Size: {}/{}", stats.size, stats.capacity);
```

### Advanced Filtering

```rust
use oxify_connect_vector::{FilterExpr, FilterValue};

// Build complex filter expressions
let filter = FilterExpr::And(vec![
    FilterExpr::Eq("category".to_string(), FilterValue::String("tech".to_string())),
    FilterExpr::Or(vec![
        FilterExpr::Gt("score".to_string(), FilterValue::Number(0.8)),
        FilterExpr::In("tags".to_string(), vec![
            FilterValue::String("featured".to_string()),
        ]),
    ]),
]);

// Works across all providers (automatically converted)
let results = provider.search(SearchRequest {
    collection: "documents".to_string(),
    query: query_vector,
    top_k: 10,
    score_threshold: None,
    filter: Some(serde_json::to_value(filter)?),
}).await?;
```

### Reranking

```rust
use oxify_connect_vector::{Reranker, MmrReranker, KeywordBoostReranker, RerankerChain};

// MMR (Maximal Marginal Relevance) for diversity
let mmr = MmrReranker::new(0.7); // Lambda = 0.7 (balance relevance vs diversity)
let reranked = mmr.rerank(results, query_vector, 10).await?;

// Boost results matching keywords
let keyword_boost = KeywordBoostReranker::new(vec!["machine".to_string(), "learning".to_string()], 1.5);
let boosted = keyword_boost.rerank(results, query_vector, 10).await?;

// Chain multiple rerankers
let chain = RerankerChain::new(vec![
    Box::new(keyword_boost),
    Box::new(mmr),
]);
let final_results = chain.rerank(results, query_vector, 10).await?;
```

### Parallel Batch Operations

For high-throughput workloads, use parallel batch operations:

```rust
use oxify_connect_vector::{
    parallel::{parallel_batch_insert, ParallelConfig},
    InsertRequest,
};
use std::sync::Arc;

// Wrap provider in Arc for sharing across tasks
let provider = Arc::new(QdrantProvider::new("http://localhost:6334").await?);

// Prepare bulk insert requests
let mut requests = Vec::new();
for i in 0..10000 {
    requests.push(InsertRequest {
        collection: "documents".to_string(),
        id: format!("doc_{}", i),
        vector: vec![0.1; 384],
        payload: json!({"index": i}),
    });
}

// Configure parallelism
let config = ParallelConfig {
    max_concurrent: 10,  // Process 10 chunks concurrently
    chunk_size: 100,      // 100 vectors per chunk
};

// Insert in parallel (significantly faster than sequential)
let inserted = parallel_batch_insert(provider, requests, config).await?;
println!("Inserted {} vectors", inserted);
```

### Batch Operations

Efficiently insert multiple vectors at once:

```rust
use oxify_connect_vector::BatchInsertRequest;

let vectors = vec![
    ("doc1".to_string(), vec![0.1; 384], json!({"title": "First"})),
    ("doc2".to_string(), vec![0.2; 384], json!({"title": "Second"})),
    ("doc3".to_string(), vec![0.3; 384], json!({"title": "Third"})),
];

let count = provider.batch_insert(BatchInsertRequest {
    collection: "documents".to_string(),
    vectors,
}).await?;

println!("Inserted {} vectors", count);
```

### Update Operations

Update existing vectors and/or their metadata:

```rust
use oxify_connect_vector::UpdateRequest;

// Update vector only
provider.update(UpdateRequest {
    collection: "documents".to_string(),
    id: "doc1".to_string(),
    vector: Some(vec![0.5; 384]),
    payload: None,
}).await?;

// Update payload only
provider.update(UpdateRequest {
    collection: "documents".to_string(),
    id: "doc1".to_string(),
    vector: None,
    payload: Some(json!({"title": "Updated", "category": "tech"})),
}).await?;
```

### Collection Statistics

Get information about your collections:

```rust
let info = provider.collection_info("documents").await?;
println!("Collection: {}", info.name);
println!("Dimension: {}", info.dimension);
println!("Vector count: {}", info.vector_count);
```

### ColBERT-style Multi-Vector Search

Store multiple vectors per document for token-level embeddings:

```rust
use oxify_connect_vector::{ColBERTProvider, MultiVectorInsertRequest, ScoringStrategy};

let colbert = ColBERTProvider::new(provider);

// Insert document with multiple vectors (e.g., token embeddings)
colbert.insert_multi_vector(MultiVectorInsertRequest {
    collection: "docs".to_string(),
    id: "doc1".to_string(),
    vectors: vec![
        vec![0.1; 128], // Token 1 embedding
        vec![0.2; 128], // Token 2 embedding
        vec![0.3; 128], // Token 3 embedding
    ],
    payload: json!({"title": "Document with token embeddings"}),
}).await?;

// Search with multiple query vectors
let results = colbert.search_multi_vector(
    "docs",
    vec![vec![0.1; 128], vec![0.2; 128]], // Query token embeddings
    10,
    Some(0.7),
).await?;
```

### Data Migration

Migrate data between different vector database providers:

```rust
use oxify_connect_vector::migration::*;

// Export collection to snapshot
let snapshot = export_collection(
    &source_provider,
    "my_collection",
    100, // batch size
    None, // no limit
).await?;

// Save to file
snapshot.save_to_file("backup.json")?;

// Import to different provider
let snapshot = VectorSnapshot::load_from_file("backup.json")?;
import_snapshot(&target_provider, snapshot, 100).await?;

// Or migrate directly
migrate_collection(
    &source_provider,
    &target_provider,
    "my_collection",
    MigrationOptions {
        batch_size: 100,
        progress_callback: Some(Box::new(|progress| {
            println!("Progress: {}%", progress.percentage());
        })),
    },
).await?;
```

### Metrics and Monitoring

Track performance and health of vector operations:

```rust
use oxify_connect_vector::{MetricsProvider, HealthCheckProvider};

// Wrap provider with metrics collection
let metrics_provider = MetricsProvider::new(provider);

// Perform operations...
metrics_provider.search(request).await?;
metrics_provider.insert(request).await?;

// Get metrics
let search_stats = metrics_provider.metrics().search_stats();
println!("Searches: {}, Avg latency: {:?}",
    search_stats.count,
    search_stats.avg_duration()
);

// Health monitoring
let health_provider = HealthCheckProvider::new(provider);
let health = health_provider.check_health().await?;
println!("Status: {:?}, Response time: {:?}ms",
    health.status,
    health.response_time_ms
);
```

### Utility Functions

Use built-in vector math utilities:

```rust
use oxify_connect_vector::{
    cosine_similarity, euclidean_distance, manhattan_distance,
    normalize_vector, batch_normalize, is_valid_vector
};

let vec1 = vec![1.0, 2.0, 3.0];
let vec2 = vec![4.0, 5.0, 6.0];

// Compute similarity and distances
let similarity = cosine_similarity(&vec1, &vec2);
let euclidean = euclidean_distance(&vec1, &vec2);
let manhattan = manhattan_distance(&vec1, &vec2);

// Validate and normalize
assert!(is_valid_vector(&vec1)); // Check for NaN/Inf
let normalized = normalize_vector(&vec1);

// Batch operations
let vectors = vec![vec![3.0, 4.0], vec![1.0, 0.0]];
let normalized_batch = batch_normalize(&vectors);
```

### Mock Provider for Testing

```rust
use oxify_connect_vector::{MockVectorProvider, InsertRequest, SearchRequest};
use serde_json::json;

#[tokio::test]
async fn test_vector_search() {
    let provider = MockVectorProvider::new();
    provider.create_collection("test", 128).await.unwrap();

    // Insert test data
    provider.insert(InsertRequest {
        collection: "test".to_string(),
        id: "1".to_string(),
        vector: vec![1.0; 128],
        payload: json!({"text": "test document"}),
    }).await.unwrap();

    // Test search
    let results = provider.search(SearchRequest {
        collection: "test".to_string(),
        query: vec![1.0; 128],
        top_k: 1,
        score_threshold: None,
        filter: None,
    }).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "1");
}

#[tokio::test]
async fn test_error_handling() {
    let provider = MockVectorProvider::new();

    // Force errors for testing
    provider.set_error(VectorError::ConnectionError("Test error".to_string()));

    let result = provider.create_collection("test", 128).await;
    assert!(result.is_err());
}
```

## Supported Vector Databases

| Provider | Status | Features | Best For |
|----------|--------|----------|----------|
| **Qdrant** | ✅ | Full CRUD, filters, gRPC | High performance production |
| **pgvector** | ✅ | PostgreSQL extension, SQL | Existing PostgreSQL setups |
| **ChromaDB** | ✅ | Simple HTTP API, metadata | Quick prototyping |
| **Pinecone** | ✅ | Managed service, namespaces | Serverless deployments |
| **Weaviate** | ✅ | GraphQL, schema | Rich queries, multi-tenancy |
| **Milvus** | ✅ | Distributed, scalable | Large-scale deployments |

## Architecture

```rust
#[async_trait]
pub trait VectorProvider: Send + Sync {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>>;
    async fn insert(&self, request: InsertRequest) -> Result<()>;
    async fn delete(&self, request: DeleteRequest) -> Result<usize>;
    async fn create_collection(&self, name: &str, dimension: usize) -> Result<()>;
    async fn collection_exists(&self, name: &str) -> Result<bool>;
}
```

## Benchmarks & Performance Testing

Run benchmarks to measure performance:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark suite
cargo bench --bench vector_bench   # Search latency, throughput, accuracy
cargo bench --bench hybrid_bench   # Hybrid search, BM25 performance
cargo bench --bench cache_bench    # Cache hit/miss rates
cargo bench --bench colbert_bench  # Multi-vector search performance

# Performance regression testing (compare with main branch)
./perf_regression.sh main HEAD 10  # 10% threshold
```

Available benchmarks:
- **Search latency**: 100, 1k, 10k vector collections
- **Throughput**: Queries/second and inserts/second
- **Accuracy**: Recall@k for k=1,5,10,20
- **Dimension scaling**: 64-1024 dimensions
- **Hybrid search**: BM25, fusion weights, RRF parameters
- **Cache performance**: Hit/miss, eviction, different sizes
- **ColBERT**: Multi-vector search, MaxSim computation

### CI/CD Integration

Automated testing runs on every push:
- Unit tests (60+ tests)
- Integration tests with Docker services (Qdrant, PostgreSQL, ChromaDB)
- Clippy linting (zero warnings enforced)
- rustfmt checks
- Performance regression detection on PRs

See `.github/workflows/` for workflow definitions.

## Testing

```bash
# Run unit tests (no database required)
cargo test

# Run integration tests (requires running databases)
docker-compose up -d  # Start test databases
cargo test -- --ignored

# Run tests with output
cargo test -- --nocapture

# Run clippy (zero warnings enforced)
cargo clippy --all-targets
```

All tests pass with **zero warnings** enforced. Total: 68 tests (60 unit + 8 doc tests).

### Integration Testing

Docker Compose configuration included for integration tests:

```bash
# Start all vector databases
docker-compose up -d

# Run integration tests
cargo test --test integration_test -- --include-ignored

# Stop databases
docker-compose down
```

Supported databases in integration tests:
- Qdrant (ports 6333, 6334)
- PostgreSQL with pgvector (port 5432)
- ChromaDB (port 8000)
- Milvus (ports 19530, 9091)

## Error Handling

```rust
pub enum VectorError {
    DatabaseError(String),    // Database operation failed
    ConfigError(String),      // Invalid configuration
    QueryError(String),       // Query execution error
    ConnectionError(String),  // Connection failed
}

pub type Result<T> = std::result::Result<T, VectorError>;
```

## Integration with oxify-connect-llm

Enable embeddings feature for automatic embedding generation:

```toml
[dependencies]
oxify-connect-vector = { version = "0.1", features = ["embeddings"] }
```

```rust
use oxify_connect_vector::EmbeddingVectorStore;
use oxify_connect_llm::OpenAIEmbedding;

let embedding_provider = OpenAIEmbedding::new("api-key");
let vector_store = EmbeddingVectorStore::new(vector_provider, embedding_provider);

// Insert text (automatically generates embeddings)
vector_store.insert_text(InsertTextRequest {
    collection: "docs".to_string(),
    id: "1".to_string(),
    text: "This is a document".to_string(),
    payload: json!({}),
}).await?;

// Search by text (automatically generates query embedding)
let results = vector_store.search_by_text(
    "docs".to_string(),
    "find similar documents".to_string(),
    10,
    Some(0.7),
).await?;
```

## Performance Tips

1. **Use caching**: Cache embeddings and frequently accessed search results
2. **Hybrid search**: Combine vector and keyword search for better accuracy
3. **Batch operations**: Insert multiple vectors at once when possible (use native batch APIs)
4. **Score threshold**: Filter low-quality results early
5. **Reranking**: Use MMR for diversity, keyword boost for precision
6. **Metrics**: Monitor performance with MetricsProvider to identify bottlenecks
7. **Health checks**: Use HealthCheckProvider to detect provider issues early

## Documentation

Comprehensive documentation available:

- **Performance Testing Guide**: `docs/PERFORMANCE_TESTING.md`
  - How to run benchmarks and interpret results
  - Performance regression testing with `perf_regression.sh`
  - Profiling and optimization techniques
  - CI/CD integration for continuous monitoring

- **Provider Comparison Guide**: `docs/PROVIDER_COMPARISON.md`
  - Detailed comparison of all 6 providers
  - Cost analysis for cloud providers
  - Feature comparison matrix
  - Decision tree to help choose the right provider
  - Migration strategies

- **Integration Testing**: `tests/INTEGRATION_TESTING.md`
  - Docker Compose setup
  - Running integration tests
  - CI/CD integration

## Project Status

✅ **Production-ready** - All phases complete:
- ✅ Phase 1-10: Core features implemented
- ✅ CI/CD: Automated testing and performance regression
- ✅ Documentation: Comprehensive guides and examples
- ✅ Quality: Zero warnings, 68 tests, all passing

## See Also

- `oxify-model`: Data model definitions
- `oxify-connect-llm`: Embedding and LLM providers (OpenAI, Cohere, Ollama)
- `oxify-engine`: Workflow execution engine
- `oxify-storage`: Database abstractions

## Contributing

This crate follows strict quality standards:
- Zero warnings policy (enforced by CI)
- Comprehensive tests (unit + integration + doc tests)
- Performance regression testing
- Full API documentation

## License

Apache-2.0
