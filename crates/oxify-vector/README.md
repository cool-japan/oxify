# oxify-vector

In-memory vector similarity search for RAG (Retrieval-Augmented Generation) in OxiFY.

## Overview

`oxify-vector` provides fast, efficient vector similarity search for building RAG workflows. It uses exact search algorithms with optional parallel processing, making it ideal for small to medium datasets (<100k vectors).

**Ported from**: [OxiRS](https://github.com/cool-japan/oxirs) - Battle-tested in production semantic web applications.

## Features

### Core Search
- **Multiple Distance Metrics**: Cosine, Euclidean, Dot Product, Manhattan
- **Parallel Search**: Multi-threaded search using Rayon
- **Exact Search**: Brute-force search for guaranteed best results
- **Incremental Updates**: Add, remove, and update vectors without rebuilding

### Advanced Algorithms
- **HNSW Index**: Hierarchical Navigable Small World for fast approximate search
- **IVF-PQ Index**: Inverted File with Product Quantization for memory-efficient large-scale search
- **Distributed Index**: Consistent hashing across multiple shards
- **ColBERT**: Multi-vector search for token-level matching

### Optimizations
- **Query Optimizer**: Automatic strategy selection (brute-force vs HNSW vs IVF-PQ)
- **Scalar Quantization**: 4x memory reduction (float32 → uint8) with minimal accuracy loss
- **SIMD Acceleration**: AVX2 optimizations for distance computations
- **Multi-Index Search**: Search across multiple indexes in parallel

### Filtering & Search
- **Filtered Search**: Metadata-based filtering with pre/post-filtering strategies
- **Hybrid Search**: Vector + BM25 keyword search with RRF fusion
- **Batch Search**: Process multiple queries efficiently
- **Radius Search**: Find all neighbors within a distance threshold

### Integration
- **Embeddings**: OpenAI and Ollama embedding providers with caching
- **Persistence**: Save/load indexes to disk with optional memory-mapping
- **OpenTelemetry**: Optional distributed tracing support
- **Type-Safe**: Strongly-typed API with compile-time guarantees

## Installation

```toml
[dependencies]
oxify-vector = { path = "../crates/engine/oxify-vector" }

# Or with parallel search
oxify-vector = { path = "../crates/engine/oxify-vector", features = ["parallel"] }
```

### Feature Flags

- `parallel`: Enable multi-threaded search using Rayon

## Quick Start

### Basic Vector Search

```rust
use oxify_vector::{VectorSearchIndex, SearchConfig, DistanceMetric};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create embeddings (typically from an embedding model)
    let mut embeddings = HashMap::new();
    embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3, 0.4]);
    embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4, 0.5]);
    embeddings.insert("doc3".to_string(), vec![0.3, 0.4, 0.5, 0.6]);

    // Configure search
    let mut config = SearchConfig::default();
    config.metric = DistanceMetric::Cosine;

    // Build index
    let mut index = VectorSearchIndex::new(config);
    index.build(&embeddings)?;

    // Search for similar vectors
    let query = vec![0.15, 0.25, 0.35, 0.45];
    let results = index.search(&query, 2)?;

    for result in results {
        println!("Entity: {}, Score: {:.4}", result.entity_id, result.score);
    }

    Ok(())
}
```

## Core Components

### `VectorSearchIndex`

Main interface for vector search operations.

```rust
pub struct VectorSearchIndex {
    config: SearchConfig,
    entity_ids: Vec<String>,
    embedding_matrix: Option<Vec<Vec<f32>>>,
}

impl VectorSearchIndex {
    pub fn new(config: SearchConfig) -> Self;
    pub fn build(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()>;
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>>;
    pub fn add(&mut self, entity_id: String, embedding: Vec<f32>) -> Result<()>;
    pub fn remove(&mut self, entity_id: &str) -> Result<()>;
}
```

### `SearchConfig`

Configuration for search behavior.

```rust
pub struct SearchConfig {
    pub metric: DistanceMetric,
    pub normalize: bool,
    pub parallel: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            metric: DistanceMetric::Cosine,
            normalize: true,   // Auto-normalize for cosine similarity
            parallel: false,   // Single-threaded by default
        }
    }
}
```

### `DistanceMetric`

Supported distance/similarity metrics.

```rust
pub enum DistanceMetric {
    Cosine,      // Cosine similarity (default for RAG)
    Euclidean,   // L2 distance
    DotProduct,  // Dot product similarity
    Manhattan,   // L1 distance (Manhattan/Taxicab)
}
```

### `SearchResult`

Result from a similarity search.

```rust
pub struct SearchResult {
    pub entity_id: String,
    pub score: f32,      // Higher is better (similarity score)
    pub distance: f32,   // Lower is better (distance)
    pub rank: usize,     // 1-indexed rank
}
```

## Distance Metrics

### Cosine Similarity (Recommended for RAG)

Measures the cosine of the angle between vectors. Range: [-1, 1], higher is more similar.

**Use when**: Magnitude doesn't matter, only direction (typical for text embeddings)

```rust
let mut config = SearchConfig::default();
config.metric = DistanceMetric::Cosine;
```

**Example**:
- `query: [0.5, 0.5, 0.0]`
- `doc1: [1.0, 1.0, 0.0]` → score: 1.0 (identical direction)
- `doc2: [0.0, 1.0, 0.0]` → score: 0.71 (45° angle)
- `doc3: [-1.0, -1.0, 0.0]` → score: -1.0 (opposite direction)

### Euclidean Distance (L2)

Straight-line distance between vectors. Range: [0, ∞), lower is more similar.

**Use when**: Absolute magnitude matters

```rust
let mut config = SearchConfig::default();
config.metric = DistanceMetric::Euclidean;
config.normalize = false;  // Preserve magnitude
```

**Example**:
- `query: [1.0, 1.0]`
- `doc1: [1.0, 1.0]` → distance: 0.0 (identical)
- `doc2: [2.0, 2.0]` → distance: 1.41
- `doc3: [0.0, 0.0]` → distance: 1.41

### Dot Product

Inner product of vectors. Range: (-∞, ∞), higher is more similar.

**Use when**: Combining similarity and magnitude

```rust
let mut config = SearchConfig::default();
config.metric = DistanceMetric::DotProduct;
config.normalize = false;
```

### Manhattan Distance (L1)

Sum of absolute differences. Range: [0, ∞), lower is more similar.

**Use when**: Grid-based distances or robustness to outliers

```rust
let mut config = SearchConfig::default();
config.metric = DistanceMetric::Manhattan;
```

## RAG Workflow Example

```rust
use oxify_vector::{VectorSearchIndex, SearchConfig, DistanceMetric};
use std::collections::HashMap;

#[derive(Debug)]
struct Document {
    id: String,
    content: String,
    embedding: Vec<f32>,
}

async fn rag_pipeline(
    query: &str,
    documents: Vec<Document>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // 1. Build vector index
    let mut embeddings = HashMap::new();
    for doc in &documents {
        embeddings.insert(doc.id.clone(), doc.embedding.clone());
    }

    let mut config = SearchConfig::default();
    config.metric = DistanceMetric::Cosine;

    let mut index = VectorSearchIndex::new(config);
    index.build(&embeddings)?;

    // 2. Get query embedding (from embedding model)
    let query_embedding = get_embedding(query).await?;

    // 3. Search for relevant documents
    let results = index.search(&query_embedding, 3)?;

    // 4. Retrieve document content
    let mut context_docs = Vec::new();
    for result in results {
        if let Some(doc) = documents.iter().find(|d| d.id == result.entity_id) {
            context_docs.push(doc.content.clone());
        }
    }

    Ok(context_docs)
}

async fn get_embedding(text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    // Call embedding API (OpenAI, Cohere, etc.)
    // For example: text-embedding-ada-002
    Ok(vec![0.1, 0.2, 0.3, 0.4])  // Placeholder
}
```

## New Features

### Adaptive Index (NEW)

The easiest way to use oxify-vector - automatically selects the best index type and optimizes performance:

```rust
use oxify_vector::{AdaptiveIndex, AdaptiveConfig};
use std::collections::HashMap;

// Create adaptive index - starts simple, upgrades as needed
let mut index = AdaptiveIndex::new(AdaptiveConfig::default());

// Build from embeddings
let mut embeddings = HashMap::new();
embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
index.build(&embeddings)?;

// Search - automatically uses best strategy
let query = vec![0.15, 0.25, 0.35];
let results = index.search(&query, 10)?;

// Add more data - may trigger automatic upgrade
for i in 0..10000 {
    index.add_vector(format!("doc_{}", i), vec![/* ... */])?;
}

// Check current strategy and performance
let stats = index.stats();
println!("Strategy: {:?}", stats.current_strategy);
println!("Avg latency: {:.2}ms", stats.avg_latency_ms);
println!("P95 latency: {:.2}ms", stats.p95_latency_ms);
```

**Features:**
- **Automatic upgrades**: Starts with brute-force, upgrades to HNSW as dataset grows
- **Performance tracking**: Monitors latency and optimizes automatically
- **Simple API**: One interface for all index types
- **Configurable**: Presets for high accuracy or low latency

**Configuration presets:**
```rust
// High accuracy (slower, more accurate)
let config = AdaptiveConfig::high_accuracy();
let mut index = AdaptiveIndex::new(config);

// Low latency (faster, good enough accuracy)
let config = AdaptiveConfig::low_latency();
let mut index = AdaptiveIndex::new(config);
```

**When to use:**
- You want optimal performance without manual tuning
- Dataset size changes over time
- You need automatic performance optimization
- You want a simple "it just works" API

### Incremental Index Updates

Add, remove, and update vectors without rebuilding the entire index:

```rust
use oxify_vector::{VectorSearchIndex, SearchConfig};
use std::collections::HashMap;

let mut index = VectorSearchIndex::new(SearchConfig::default());

// Build initial index
let mut embeddings = HashMap::new();
embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
index.build(&embeddings)?;

// Add a single vector
index.add_vector("doc3".to_string(), vec![0.3, 0.4, 0.5])?;

// Add multiple vectors
let mut new_docs = HashMap::new();
new_docs.insert("doc4".to_string(), vec![0.4, 0.5, 0.6]);
new_docs.insert("doc5".to_string(), vec![0.5, 0.6, 0.7]);
index.add_vectors(&new_docs)?;

// Update an existing vector
index.update_vector("doc1", vec![0.9, 0.9, 0.9])?;

// Remove a vector
index.remove_vector("doc2")?;
```

### Query Optimizer

Automatically select the best search strategy based on dataset size and requirements:

```rust
use oxify_vector::optimizer::{QueryOptimizer, OptimizerConfig, SearchStrategy};

let optimizer = QueryOptimizer::new(OptimizerConfig::default());

// Recommend strategy based on dataset size and required recall
let num_vectors = 100_000;
let required_recall = 0.95;
let strategy = optimizer.recommend_strategy(num_vectors, required_recall);

match strategy {
    SearchStrategy::BruteForce => println!("Use exact search (< 10K vectors)"),
    SearchStrategy::Hnsw => println!("Use HNSW (10K - 1M vectors)"),
    SearchStrategy::IvfPq => println!("Use IVF-PQ (> 1M vectors)"),
    SearchStrategy::Distributed => println!("Use distributed search (> 10M vectors)"),
}

// Optimize pre/post-filtering
let filter_selectivity = 0.05; // 5% of vectors match filter
let use_prefilter = optimizer.recommend_prefiltering(num_vectors, filter_selectivity);

// Optimize batch size
let batch_size = optimizer.recommend_batch_size(1000, num_vectors);
```

**Presets for common scenarios:**

```rust
use oxify_vector::OptimizerConfig;

// High accuracy (use exact search longer, higher recall threshold)
let config = OptimizerConfig::high_accuracy();

// High speed (switch to ANN earlier, lower recall threshold)
let config = OptimizerConfig::high_speed();

// Memory efficient (use quantization earlier, disable caching)
let config = OptimizerConfig::memory_efficient();
```

### Scalar Quantization

Reduce memory usage by 75% with minimal accuracy loss:

```rust
use oxify_vector::quantization::{QuantizedVectorIndex, QuantizationConfig};

// Generate dataset
let vectors: Vec<(String, Vec<f32>)> = (0..10000)
    .map(|i| (format!("doc_{}", i), vec![/* ... */]))
    .collect();

// Build quantized index
let mut index = QuantizedVectorIndex::new(QuantizationConfig::default());
index.build(&vectors)?;

// Get statistics
let stats = index.stats();
println!("Original size: {} bytes", stats.original_bytes);
println!("Quantized size: {} bytes", stats.quantized_bytes);
println!("Compression: {:.2}x", stats.compression_ratio);
println!("Memory saved: {:.1}%", stats.memory_savings * 100.0);

// Search (automatically uses quantized distance)
let query = vec![0.5, 0.5, 0.5];
let results = index.search(&query, 10)?;
```

**Benefits:**
- **Memory**: 4x reduction (float32 → uint8)
- **Speed**: Faster distance computations with integer math
- **Accuracy**: ~1-2% recall degradation for most datasets

### Multi-Index Search

Search across multiple indexes in parallel and merge results:

```rust
use oxify_vector::{MultiIndexSearch, MultiIndexConfig, ScoreMergeStrategy};
use std::collections::HashMap;

// Create multiple indexes (e.g., different data shards or time periods)
let mut index1 = VectorSearchIndex::new(SearchConfig::default());
let mut index2 = VectorSearchIndex::new(SearchConfig::default());

// Build indexes...
index1.build(&embeddings1)?;
index2.build(&embeddings2)?;

// Configure multi-index search
let config = MultiIndexConfig {
    parallel: true,                           // Search indexes in parallel
    deduplicate: true,                        // Remove duplicate entity_ids
    merge_strategy: ScoreMergeStrategy::Max,  // Take max score for duplicates
};

let multi_search = MultiIndexSearch::with_config(config);

// Search across both indexes
let query = vec![0.5, 0.5, 0.5];
let results = multi_search.search(&[&index1, &index2], &query, 10)?;
```

**Score merge strategies:**
- `ScoreMergeStrategy::Max` - Take highest score (recommended)
- `ScoreMergeStrategy::Min` - Take lowest score
- `ScoreMergeStrategy::Average` - Average scores
- `ScoreMergeStrategy::First` - Take first occurrence

## Parallel Search

Enable parallel processing for large datasets:

```toml
[dependencies]
oxify-vector = { path = "../crates/engine/oxify-vector", features = ["parallel"] }
```

```rust
let mut config = SearchConfig::default();
config.parallel = true;  // Use Rayon for parallel search

let mut index = VectorSearchIndex::new(config);
index.build(&embeddings)?;

// Search uses all CPU cores
let results = index.search(&query, 10)?;
```

**Performance**:
- Single-threaded: ~1ms for 1k vectors
- Parallel (8 cores): ~0.2ms for 1k vectors
- Speedup scales with core count

## Integration with LLM Workflows

### Axum API Endpoint

```rust
use oxify_vector::{VectorSearchIndex, SearchConfig};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
struct AppState {
    vector_index: Arc<RwLock<VectorSearchIndex>>,
}

#[derive(Deserialize)]
struct SearchRequest {
    query: Vec<f32>,
    k: usize,
}

#[derive(Serialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

async fn search_handler(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let index = state.vector_index.read().await;

    let results = index
        .search(&req.query, req.k)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    Ok(Json(SearchResponse { results }))
}
```

### Dynamic Index Updates

```rust
use oxify_vector::VectorSearchIndex;
use tokio::sync::RwLock;
use std::sync::Arc;

async fn add_document(
    index: Arc<RwLock<VectorSearchIndex>>,
    doc_id: String,
    embedding: Vec<f32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut index = index.write().await;
    index.add(doc_id, embedding)?;
    Ok(())
}

async fn remove_document(
    index: Arc<RwLock<VectorSearchIndex>>,
    doc_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut index = index.write().await;
    index.remove(doc_id)?;
    Ok(())
}
```

## Limitations

### When to Use

- **Small to medium datasets**: <100k vectors
- **Development and prototyping**: Fast iteration without external dependencies
- **Exact search required**: Guaranteed best results
- **Low latency**: Sub-millisecond search

### When NOT to Use

- **Large datasets**: >100k vectors (use Qdrant, Pinecone, Weaviate instead)
- **Approximate search**: If 99% accuracy is acceptable (use HNSW, IVF instead)
- **Persistence**: Index is in-memory only (serialize/deserialize manually)
- **Distributed search**: No multi-node support

## Scaling to Production

For production RAG at scale, integrate with external vector databases:

```rust
// oxify-vector for development
#[cfg(debug_assertions)]
use oxify_vector::VectorSearchIndex;

// Qdrant for production
#[cfg(not(debug_assertions))]
use oxify_connect_vector::QdrantClient;
```

Or use `oxify-connect-vector` for Qdrant/pgvector:

```rust
use oxify_connect_vector::{VectorProvider, QdrantProvider};

let provider = QdrantProvider::new("http://localhost:6334").await?;
provider.search("collection_name", &query, 10).await?;
```

## Performance Benchmarks

**Hardware**: Intel i7-12700K (12 cores), 32GB RAM

| Vectors | Dimensions | Metric | Parallel | Time |
|---------|------------|--------|----------|------|
| 1k | 768 | Cosine | No | 1.2ms |
| 1k | 768 | Cosine | Yes | 0.3ms |
| 10k | 768 | Cosine | No | 12ms |
| 10k | 768 | Cosine | Yes | 2.5ms |
| 100k | 768 | Cosine | No | 120ms |
| 100k | 768 | Cosine | Yes | 25ms |

**Memory usage**: ~4 bytes/dimension/vector (768D = 3KB per vector)

## Testing

Run the test suite:

```bash
cd crates/engine/oxify-vector
cargo test

# Run with parallel feature
cargo test --features parallel
```

All tests pass with zero warnings.

## Dependencies

Core dependencies:
- `rayon` - Parallel processing (optional)

No external vector database required.

## Migration from Other Libraries

### From FAISS

```python
# FAISS (Python)
import faiss
index = faiss.IndexFlatL2(dimension)
index.add(embeddings)
distances, indices = index.search(query, k)
```

```rust
// oxify-vector (Rust)
let mut index = VectorSearchIndex::new(SearchConfig {
    metric: DistanceMetric::Euclidean,
    ..Default::default()
});
index.build(&embeddings)?;
let results = index.search(&query, k)?;
```

### From ChromaDB

```python
# ChromaDB (Python)
collection = client.create_collection("docs")
collection.add(documents=docs, embeddings=embeddings, ids=ids)
results = collection.query(query_embeddings=[query], n_results=k)
```

```rust
// oxify-vector (Rust)
let mut index = VectorSearchIndex::new(SearchConfig::default());
index.build(&embeddings)?;
let results = index.search(&query, k)?;
```

## Implemented Features

All core features are production-ready:

- ✅ **Adaptive Index**: Automatic performance optimization (NEW)
- ✅ Approximate search algorithms (HNSW, IVF-PQ)
- ✅ Serialization/deserialization for persistence
- ✅ Filtered search (metadata filtering)
- ✅ Hybrid search (vector + BM25 keyword)
- ✅ Batch search optimization
- ✅ Incremental index updates
- ✅ Query optimizer for automatic strategy selection
- ✅ Scalar quantization for memory efficiency
- ✅ Multi-index search
- ✅ Distributed search with sharding
- ✅ ColBERT multi-vector search
- ✅ SIMD optimizations (AVX2)
- ✅ OpenTelemetry tracing support

## Future Enhancements

- [ ] GPU acceleration (CUDA/ROCm)
- [ ] Product Quantization (PQ) for extreme compression
- [ ] Learned indexes (AI-optimized data structures)
- [ ] Streaming index updates (real-time ingestion)

## License

Apache-2.0

## Attribution

Ported from [OxiRS](https://github.com/cool-japan/oxirs) with permission. Original implementation by the OxiLabs team.
