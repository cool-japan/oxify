# OxiFY - LLM Workflow Orchestration Platform

[![Crates.io](https://img.shields.io/crates/v/oxify.svg)](https://crates.io/crates/oxify)
[![Documentation](https://docs.rs/oxify/badge.svg)](https://docs.rs/oxify)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../../LICENSE)

**OxiFY** is a graph-based LLM workflow orchestration platform built in Rust, designed to compose complex AI applications using directed acyclic graphs (DAGs). This meta-crate provides unified access to all OxiFY components.

## Features

- **Graph-Based Workflows**: Define LLM applications as visual DAGs
- **Type-Safe Execution**: Compile-time guarantees for workflow structure
- **Multi-Provider Support**: OpenAI, Anthropic, local models, and more
- **Vector Database Integration**: Qdrant, in-memory vector search for RAG workflows
- **Vision/OCR Processing**: Multi-provider OCR with Tesseract, Surya, PaddleOCR
- **MCP Support**: Native support for Model Context Protocol
- **REST API**: Full-featured API for workflow management
- **Pure Rust**: No C/Fortran dependencies (COOLJAPAN Policy)

## Quick Start

Add OxiFY to your `Cargo.toml`:

```toml
[dependencies]
oxify = "0.1"
tokio = { version = "1", features = ["full"] }
```

### Using the Prelude

The prelude provides convenient access to commonly used types:

```rust
use oxify::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build a workflow
    let workflow = WorkflowBuilder::new("simple-chat")
        .description("A simple chat workflow")
        .build()?;

    // Create an LLM node configuration
    let llm_config = LlmConfig {
        provider: "openai".to_string(),
        model: "gpt-4".to_string(),
        system_prompt: Some("You are a helpful assistant.".to_string()),
        prompt_template: "{{user_input}}".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(1000),
        extra_params: serde_json::Value::Null,
    };

    Ok(())
}
```

### Direct Module Access

You can also access individual modules directly:

```rust
use oxify::model::{Workflow, Node, NodeKind, Edge};
use oxify::engine::{Engine, ExecutionConfig};
use oxify::vector::{HnswIndex, HnswConfig, DistanceMetric};
use oxify::connect_llm::{LlmRequest, LlmResponse};
```

## Module Overview

This meta-crate re-exports all OxiFY library crates:

| Module | Crate | Description |
|--------|-------|-------------|
| [`model`](https://docs.rs/oxify-model) | `oxify-model` | Domain models for workflows, nodes, edges, and execution |
| [`vector`](https://docs.rs/oxify-vector) | `oxify-vector` | High-performance vector search with HNSW indexing |
| [`authn`](https://docs.rs/oxify-authn) | `oxify-authn` | Authentication (OAuth2, API keys, JWT tokens) |
| [`authz`](https://docs.rs/oxify-authz) | `oxify-authz` | ReBAC authorization (Zanzibar-style) |
| [`server`](https://docs.rs/oxify-server) | `oxify-server` | Axum-based HTTP server infrastructure |
| [`mcp`](https://docs.rs/oxify-mcp) | `oxify-mcp` | Model Context Protocol implementation |
| [`connect_llm`](https://docs.rs/oxify-connect-llm) | `oxify-connect-llm` | LLM provider integrations (OpenAI, Anthropic, Ollama) |
| [`connect_vector`](https://docs.rs/oxify-connect-vector) | `oxify-connect-vector` | Vector database integrations (Qdrant) |
| [`connect_vision`](https://docs.rs/oxify-connect-vision) | `oxify-connect-vision` | Vision/OCR integrations |
| [`storage`](https://docs.rs/oxify-storage) | `oxify-storage` | Persistent storage layer |
| [`engine`](https://docs.rs/oxify-engine) | `oxify-engine` | Workflow execution engine |

## Architecture

```
+-------------------------------------------------------------+
|                      OxiFY Platform                          |
+-------------------------------------------------------------+
|  API Layer (oxify-server)                                   |
|    +-> Authentication (oxify-authn)                         |
|    +-> Authorization (oxify-authz)                          |
|    +-> Middleware (CORS, logging, compression)              |
+-------------------------------------------------------------+
|  Workflow Engine (oxify-engine)                             |
|    +-> DAG Execution                                        |
|    +-> Node Processors (LLM, Vision, Retriever, Code)       |
|    +-> Plugin System                                        |
+-------------------------------------------------------------+
|  Connector Layer                                            |
|    +-> LLM Clients (oxify-connect-llm)                      |
|    +-> Vision/OCR (oxify-connect-vision)                    |
|    +-> Vector DB (oxify-connect-vector)                     |
|    +-> Vector Search (oxify-vector)                         |
+-------------------------------------------------------------+
```

## Examples

### Creating a Workflow

```rust
use oxify::model::{Workflow, Node, NodeKind, Edge, LlmConfig};

fn create_chat_workflow() -> Workflow {
    let mut workflow = Workflow::new("chat-bot".to_string());

    // Create nodes
    let start = Node::new("Start".to_string(), NodeKind::Start);
    let llm = Node::new("LLM".to_string(), NodeKind::Llm(LlmConfig {
        provider: "openai".to_string(),
        model: "gpt-4".to_string(),
        system_prompt: Some("You are helpful.".to_string()),
        prompt_template: "{{input}}".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(1000),
        extra_params: serde_json::Value::Null,
    }));
    let end = Node::new("End".to_string(), NodeKind::End);

    let start_id = start.id;
    let llm_id = llm.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(llm);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, llm_id));
    workflow.add_edge(Edge::new(llm_id, end_id));

    workflow.validate().expect("Invalid workflow");
    workflow
}
```

### Vector Search with HNSW

```rust
use oxify::vector::{HnswIndex, HnswConfig, DistanceMetric, SearchResult};

fn vector_search_example() {
    // Create HNSW index
    let config = HnswConfig {
        m: 16,
        ef_construction: 200,
        ef_search: 50,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let mut index = HnswIndex::new(384, config);

    // Add vectors
    let vectors = vec![
        vec![0.1, 0.2, 0.3], // ... 384 dimensions
        vec![0.4, 0.5, 0.6],
    ];

    for (i, vec) in vectors.iter().enumerate() {
        index.insert(i as u64, vec.clone());
    }

    // Search
    let query = vec![0.15, 0.25, 0.35]; // ... 384 dimensions
    let results = index.search(&query, 10);
}
```

### Using LLM Providers

```rust
use oxify::connect_llm::{LlmRequest, LlmResponse, OpenAIProvider, LlmProvider};

async fn llm_example() -> Result<(), Box<dyn std::error::Error>> {
    let provider = OpenAIProvider::new("your-api-key".to_string());

    let request = LlmRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            // ... messages
        ],
        temperature: Some(0.7),
        max_tokens: Some(1000),
        ..Default::default()
    };

    let response = provider.complete(&request).await?;
    println!("Response: {}", response.content);

    Ok(())
}
```

## Node Types

OxiFY supports 16+ workflow node types:

| Category | Node Types |
|----------|-----------|
| **Core** | Start, End |
| **LLM** | GPT-3.5/4, Claude 3/3.5, Ollama |
| **Vector** | Qdrant, In-memory with hybrid search |
| **Vision** | Tesseract, Surya, PaddleOCR |
| **Control** | IfElse, Switch, Conditional |
| **Loops** | ForEach, While, Repeat |
| **Error Handling** | Try-Catch-Finally |
| **Advanced** | Sub-workflow, Code execution, HTTP Tool |

## Supported Providers

### LLM Providers
- OpenAI (GPT-3.5, GPT-4, GPT-4-Turbo)
- Anthropic (Claude 3 Opus, Sonnet, Haiku)
- Ollama (Local models: Llama, Mistral, etc.)
- Gemini, Mistral, Cohere, Bedrock

### Vector Databases
- Qdrant (cloud and self-hosted)
- In-memory HNSW index

### Embedding Providers
- OpenAI (text-embedding-ada-002, text-embedding-3-small/large)
- Ollama (local embeddings)

### Vision/OCR Providers
- Tesseract OCR
- Surya
- PaddleOCR

## Performance

- **LLM Response Caching**: 1-hour TTL for cost savings
- **Execution Plan Caching**: 100-entry LRU cache
- **Rate Limiting**: Configurable (default 500 req/min)
- **Parallel Execution**: Level-based DAG parallelism
- **SIMD Acceleration**: Vector operations use SIMD when available
- **Retry Logic**: Exponential backoff with configurable limits

## Individual Crates

If you only need specific functionality, you can depend on individual crates:

```toml
# Just the workflow model
oxify-model = "0.1"

# Just vector search
oxify-vector = "0.1"

# Just LLM connections
oxify-connect-llm = "0.1"
```

## Binary Applications

The following binary applications are available separately:

- **oxify-api**: REST API server
- **oxify-cli**: Command-line workflow runner
- **oxify-ui**: Web-based workflow editor (coming soon)

## Development Status

**Version 0.1.0** - Production-ready!

- Core Infrastructure: Complete
- LLM Workflow Engine: Complete
- API Layer: Complete
- CLI Tool: Complete
- Web UI: In Progress

## Related Projects

OxiFY is part of the COOLJAPAN ecosystem:

- [SciRS2](https://github.com/cool-japan/scirs) - Scientific computing in Pure Rust
- [NumRS2](https://github.com/cool-japan/numrs) - Numerical computing library
- [ToRSh](https://github.com/cool-japan/torsh) - PyTorch-like tensor library
- [OxiRS](https://github.com/cool-japan/oxirs) - Semantic web platform

## License

Apache-2.0 - See [LICENSE](../../LICENSE) file for details.

## Author

COOLJAPAN OU (Team Kitasan)
