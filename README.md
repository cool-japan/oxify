# OxiFY - LLM Workflow Orchestration Platform

**OxiFY** is a graph-based LLM workflow orchestration platform built in Rust, designed to compose complex AI applications using directed acyclic graphs (DAGs). It provides a type-safe, modular approach to building LLM-powered applications.

## Features

- **Graph-Based Workflows**: Define LLM applications as visual DAGs
- **Type-Safe Execution**: Compile-time guarantees for workflow structure
- **Multi-Provider Support**: OpenAI, Anthropic, local models, and more
- **Vector Database Integration**: Qdrant, pgvector for RAG workflows
- **Vision/OCR Processing**: Multi-provider OCR with Tesseract, Surya, PaddleOCR
- **MCP Support**: Native support for Model Context Protocol
- **REST API**: Full-featured API for workflow management
- **Distributed Execution**: Built on CeleRS for scalable task processing

## Architecture

OxiFY is organized as a workspace with multiple crates:

### Core Components (Production-Ready)

- **oxify-authz**: ReBAC (Zanzibar-style) authorization engine
- **oxify-authn**: JWT/OAuth2/password authentication
- **oxify-server**: Axum-based HTTP server with middleware
- **oxify-vector**: In-memory vector search for RAG

### LLM Workflow Components

- **oxify-model**: Domain models (Workflow, Node, Edge)
- **oxify-engine**: DAG execution engine with CeleRS integration
- **oxify-api**: REST/gRPC API built with Axum
- **oxify-connect-llm**: LLM provider clients (OpenAI, Anthropic)
- **oxify-connect-vector**: Vector database clients (Qdrant, pgvector)
- **oxify-connect-vision**: Vision/OCR providers (Tesseract, Surya, PaddleOCR)
- **oxify-mcp**: Model Context Protocol implementation
- **oxify-cli**: Local workflow runner and development tool

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    OxiFY Platform                       │
├─────────────────────────────────────────────────────────┤
│  API Layer (oxify-server)                               │
│    ├─> Authentication (oxify-authn)                     │
│    ├─> Authorization (oxify-authz)                      │
│    └─> Middleware (CORS, logging, compression)          │
├─────────────────────────────────────────────────────────┤
│  Workflow Engine (oxify-engine)                         │
│    ├─> DAG Execution                                    │
│    ├─> Node Processors (LLM, Vision, Retriever, Code)   │
│    └─> CeleRS Integration                               │
├─────────────────────────────────────────────────────────┤
│  Connector Layer                                        │
│    ├─> LLM Clients (oxify-connect-llm)                  │
│    ├─> Vision/OCR (oxify-connect-vision)                │
│    ├─> Vector DB (oxify-connect-vector)                 │
│    └─> Vector Search (oxify-vector)                     │
└─────────────────────────────────────────────────────────┘
```

## Node Types

OxiFY supports various node types for building workflows:

- **Start**: Entry point of the workflow
- **End**: Exit point of the workflow
- **LLM**: Invoke language models (GPT-4, Claude, etc.)
- **Retriever**: Query vector databases for context
- **Vision**: OCR/document processing (Tesseract, Surya, PaddleOCR)
- **Code**: Execute Rust scripts or WebAssembly
- **IfElse**: Conditional branching based on expressions
- **Tool**: Invoke MCP tools

## Quick Start

### Installation

**From GitHub:**
```bash
git clone https://github.com/cool-japan/oxify.git
cd oxify
cargo build --release
```

**As a Cargo dependency:**
```toml
# Add to your Cargo.toml
[dependencies]
oxify-model = { git = "https://github.com/cool-japan/oxify.git" }
oxify-engine = { git = "https://github.com/cool-japan/oxify.git" }
```

**Development build:**
```bash
# Clone and build in debug mode
git clone https://github.com/cool-japan/oxify.git
cd oxify
cargo build

# Run tests
cargo test --all
```

### Run Complete Server Example

Try the production-ready server with JWT auth and vector search:

```bash
cd examples/complete-server
cargo run --bin server
```

Visit `http://localhost:3000` and try the endpoints:

```bash
# Login to get JWT token
curl -X POST http://localhost:3000/login \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","password":"secret"}'

# Search vectors
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":[0.15,0.25,0.35,0.45],"k":3}'
```

See `examples/complete-server/README.md` for full documentation.

### Define a Workflow

```rust
use oxify_model::{Workflow, Node, NodeKind, Edge, LlmConfig};

let mut workflow = Workflow::new("Simple Chat".to_string());

// Create nodes
let start = Node::new("Start".to_string(), NodeKind::Start);
let llm = Node::new("LLM".to_string(), NodeKind::LLM(LlmConfig {
    provider: "openai".to_string(),
    model: "gpt-4".to_string(),
    system_prompt: Some("You are a helpful assistant.".to_string()),
    prompt_template: "{{user_input}}".to_string(),
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

// Create edges
workflow.add_edge(Edge::new(start_id, llm_id));
workflow.add_edge(Edge::new(llm_id, end_id));

// Validate
workflow.validate().unwrap();
```

### Run the API Server

```bash
cargo run --bin oxify-api
```

### Use the CLI

```bash
# Validate a workflow
cargo run --bin oxify -- validate --workflow ./workflows/chat.json

# Run a workflow locally
cargo run --bin oxify -- run --workflow ./workflows/chat.json --input '{"user_input": "Hello!"}'
```

## Workflow Execution

OxiFY uses a topological sort algorithm to determine execution order:

1. Parse workflow definition
2. Validate DAG structure (no cycles)
3. Determine execution order via topological sort
4. Execute nodes in order, passing outputs to dependent nodes
5. Store results in execution context

Each node execution can be:
- Executed locally (CLI mode)
- Queued as a CeleRS task (distributed mode)

## Model Context Protocol (MCP)

OxiFY natively supports MCP, allowing it to:
- Act as an MCP server, exposing workflows as tools
- Invoke MCP tools from other servers
- Enable agent-to-agent communication

## Development Status

**Version 0.1.0** - Production-ready with comprehensive features!

### Core Infrastructure ✅ COMPLETE
- ✅ **Security**: ReBAC authorization (Zanzibar-style) with PostgreSQL + in-memory hybrid
- ✅ **Authentication**: JWT (HS256/RS256), OAuth2/OIDC with PKCE, Argon2 password hashing
- ✅ **Server**: Axum HTTP server with middleware (auth, CORS, compression, rate limiting)
- ✅ **Vector Search**: In-memory and distributed vector search with multiple metrics
- ✅ **Storage**: PostgreSQL-backed workflow/execution storage with versioning

**Note**: Security and server components ported from [OxiRS](https://github.com/cool-japan/oxirs), a battle-tested semantic web platform (8.5 hours vs 10-14 weeks savings).

### LLM Workflow Engine ✅ COMPLETE
- ✅ **Core Engine**: DAG executor with topological sort, parallel execution, retry logic
- ✅ **Node Types**: 15+ node types including LLM, Vector, Code, Loop, Try-Catch, Sub-workflow
- ✅ **LLM Providers**: OpenAI (GPT-3.5/4), Anthropic (Claude), Ollama (local models)
- ✅ **Embeddings**: OpenAI embeddings (text-embedding-ada-002), Ollama embeddings
- ✅ **Vector Databases**: Qdrant and pgvector with hybrid search (BM25 + RRF)
- ✅ **Streaming**: Real-time LLM streaming via SSE (OpenAI, Anthropic, Ollama)
- ✅ **Advanced Features**: Checkpointing, pause/resume, scheduling (cron), webhooks
- ✅ **Caching**: LLM response caching (1-hour TTL), execution plan caching (LRU)

### API Layer ✅ COMPLETE
- ✅ **REST API**: 30+ endpoints with full CRUD operations
- ✅ **Documentation**: OpenAPI 3.0 specification with Swagger UI support
- ✅ **Authentication**: JWT middleware with role-based access control
- ✅ **Authorization**: ReBAC integration for fine-grained permissions
- ✅ **Rate Limiting**: Token bucket algorithm (configurable, default 500 req/min)
- ✅ **Real-time**: Server-Sent Events (SSE) for execution streaming
- ✅ **Endpoints**: Workflows, Executions, Schedules, Webhooks, Checkpoints, Secrets, Versions

### CLI Tool ✅ COMPLETE
- ✅ **Workflow Management**: Create, validate, test, visualize, analyze workflows
- ✅ **Execution**: Local and remote execution with variable substitution
- ✅ **Development**: Scaffolding, templates, cost estimation
- ✅ **Operations**: Scheduling, webhooks, checkpoints, secrets, statistics
- ✅ **Shell Integration**: Completion scripts for bash/zsh/fish/powershell

### Web UI 🚧 NOT STARTED
- ⏳ React Flow DAG visual editor
- ⏳ Real-time execution monitoring dashboard
- ⏳ Workflow management interface

## Roadmap

### Phase 0: Security & Server Foundation ✅ COMPLETE
- [x] ReBAC authorization engine (Zanzibar-style, ported from OxiRS)
- [x] JWT/OAuth2 authentication (ported from OxiRS)
- [x] Axum HTTP server with middleware (ported from OxiRS)
- [x] Vector search for RAG (ported from OxiRS)
- [x] Complete integration example
- [x] **Time Savings**: 8.5 hours actual vs 10-14 weeks from scratch

### Phase 1: The Backbone (CeleRS Foundation) ✅ COMPLETE
- [x] Task queue infrastructure
- [x] Distributed execution architecture
- [x] Worker runtime patterns established

### Phase 2: The Brain (OxiFY Engine) ✅ COMPLETE
- [x] Workflow data structures (Workflow, Node, Edge)
- [x] DAG execution engine with topological sort
- [x] Parallel node execution (level-based)
- [x] LLM provider integrations (OpenAI, Anthropic, Ollama)
- [x] Vector database support (Qdrant, pgvector)
- [x] Advanced node types (Loop, Try-Catch, Sub-workflow, Code, Conditional)
- [x] Retry logic with exponential backoff
- [x] Execution caching (LLM responses, execution plans)
- [x] Checkpointing and pause/resume

### Phase 3: The Face ⚡ API COMPLETE | 🚧 UI IN PROGRESS
- [x] **REST API**: 30+ endpoints for workflow management ✅ COMPLETE
- [x] **OpenAPI Documentation**: Swagger UI support ✅ COMPLETE
- [x] **Authentication & Authorization**: JWT + ReBAC ✅ COMPLETE
- [x] **Real-time Streaming**: SSE for execution updates ✅ COMPLETE
- [x] **Rate Limiting**: Token bucket algorithm ✅ COMPLETE
- [x] **Advanced Features**: Scheduling, webhooks, secrets ✅ COMPLETE
- [ ] **Web UI**: React Flow DAG editor 🚧 NOT STARTED
- [ ] **Monitoring Dashboard**: Real-time execution visualization 🚧 NOT STARTED

### Phase 4: Production & Ecosystem (Future)
- [ ] Docker images and Kubernetes manifests
- [ ] CI/CD pipeline with GitHub Actions
- [ ] Comprehensive documentation and tutorials
- [ ] Example workflow library (RAG, agents, automation)
- [ ] Plugin system for custom node types
- [ ] Multi-tenancy support
- [ ] Advanced analytics and cost tracking

## Statistics

### Codebase Metrics (as of 2026-01-19)
- **Lines of Code**: 18,000+ lines of production Rust code
- **Workspace Crates**: 13 specialized crates
  - Security: `oxify-authz`, `oxify-authn`
  - API: `oxify-server`, `oxify-api`
  - Engine: `oxify-engine`, `oxify-vector`
  - Connectors: `oxify-connect-llm`, `oxify-connect-vector`, `oxify-connect-vision`
  - Supporting: `oxify-model`, `oxify-storage`, `oxify-mcp`, `oxify-cli`
- **Tests**: 100+ passing tests with comprehensive coverage
- **Zero Warnings**: All code compiles cleanly (NO WARNINGS POLICY)

### Feature Metrics
- **API Endpoints**: 30+ REST endpoints
- **Node Types**: 16+ workflow node types
  - Core: Start, End
  - LLM: GPT-3.5/4, Claude 3/3.5, Ollama
  - Vector: Qdrant, pgvector with hybrid search
  - Vision: Tesseract, Surya, PaddleOCR
  - Control: IfElse, Switch, Conditional
  - Loops: ForEach, While, Repeat
  - Error Handling: Try-Catch-Finally
  - Advanced: Sub-workflow, Code execution, HTTP Tool
- **CLI Commands**: 55+ commands across 13 subcommands
- **Supported LLM Providers**: 3 (OpenAI, Anthropic, Ollama)
- **Supported Vector DBs**: 2 (Qdrant, pgvector)
- **Supported Vision/OCR Providers**: 4 (Mock, Tesseract, Surya, PaddleOCR)
- **Embedding Providers**: 2 (OpenAI, Ollama)

### Performance Characteristics
- **LLM Response Caching**: 1-hour TTL for cost savings
- **Execution Plan Caching**: 100-entry LRU cache
- **Rate Limiting**: 500 requests/minute (configurable)
- **Parallel Execution**: Level-based parallelism in DAG
- **Retry Logic**: Exponential backoff with configurable limits
- **Checkpointing**: Database-backed state persistence

## Integration with CeleRS

OxiFY uses CeleRS as its distributed execution backend:

```
┌─────────────┐
│   OxiFY   │  (Defines WHAT to execute)
│   Engine    │
└──────┬──────┘
       │
       │ Enqueues nodes as tasks
       ▼
┌─────────────┐
│   CeleRS    │  (Defines HOW to execute)
│   Broker    │
└──────┬──────┘
       │
       │ Workers consume tasks
       ▼
┌─────────────┐
│   Node      │
│  Execution  │
└─────────────┘
```

## License

Apache-2.0 - See [LICENSE](LICENSE) file for details.
