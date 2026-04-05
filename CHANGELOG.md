# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-03-29

### Added

#### Authorization (`oxify-authz`)
- ReBAC (Relationship-Based Access Control) engine - Google Zanzibar implementation
- Hybrid PostgreSQL + in-memory relation tuple store
- gRPC authorization service with Tonic
- Bloom filter for quick negative lookups
- Redis-based distributed L2 cache with Moka local cache
- Comprehensive audit logging with MD5 integrity verification
- Property-based testing with proptest

#### Authentication (`oxify-authn`)
- SAML 2.0 authentication support (optional)
- LDAP directory integration (optional)
- WebAuthn/FIDO2 passwordless authentication (optional)
- TOTP-based multi-factor authentication (optional)
- Session management and token revocation

#### MCP Server (`oxify-mcp`)
- Model Context Protocol server implementation
- Tool/resource/prompt management
- Streaming transport support

#### CLI (`oxify-cli`)
- Full interactive command-line interface
- Workflow management and execution commands

#### UI (`oxify-ui`)
- Web-based management dashboard with HTMX
- Askama templating engine integration
- Workflow visualization and monitoring

### Changed
- Expanded workspace to 15 specialized crates (from 13)
- Updated all dependencies to latest stable versions
- Enhanced test coverage across all crates

### Fixed
- Various stability improvements across all crates

## [0.1.0] - 2026-01-19

### Added

#### Core Infrastructure
- Initial release of OxiFY - Pure Rust AI orchestration and agent framework
- Workspace-based architecture with 13 specialized crates
- SQLite-based storage layer (migrated from PostgreSQL)
- Comprehensive GraphQL API with async-graphql integration

#### Vector Search (`oxify-vector`)
- In-memory vector search with multiple distance metrics (Euclidean, Cosine, Dot Product, Manhattan)
- HNSW (Hierarchical Navigable Small World) graph-based indexing
- IVF (Inverted File Index) with Product Quantization for large-scale search
- FP16 quantization for memory efficiency
- SIMD acceleration (AVX2, AVX-512, NEON)
- GPU acceleration with CUDA support (Linux only)
- Memory-mapped file persistence with zero-copy serialization
- Sparse vector support with efficient CSR storage
- Parallel batch operations
- Comprehensive filtering and metadata support

#### AI Connectors
- **LLM Integration** (`oxify-connect-llm`): OpenAI, Anthropic, Cohere, Ollama support
- **Vector Database** (`oxify-connect-vector`): Qdrant client integration
- **Vision Models** (`oxify-connect-vision`): Image preprocessing, augmentation, normalization

#### Security & Authentication
- **Authentication** (`oxify-authn`): JWT-based auth, API key management, session handling
- **Authorization** (`oxify-authz`): Role-based access control (RBAC), policy enforcement
- Encryption utilities with AES-256-GCM

#### Workflow & Execution
- **Engine** (`oxify-engine`): DAG-based workflow orchestration
- Conditional execution, parallel task execution, retry mechanisms
- Code execution sandbox (Rhai scripting, WebAssembly support)
- Cron-based scheduling

#### Developer Experience
- **CLI** (`oxify-cli`): Interactive command-line interface
- **MCP** (`oxify-mcp`): Model Context Protocol server implementation
- **UI** (`oxify-ui`): Web-based management interface with Axum
- **API** (`oxify-api`): RESTful and GraphQL endpoints

#### Storage & Persistence
- Execution history tracking
- Metrics collection and export
- Cache management with TTL support
- Database maintenance utilities

#### Observability
- OpenTelemetry integration (traces, metrics)
- Structured logging with tracing
- Performance benchmarks with Criterion

### Technical Highlights
- **Pure Rust**: 100% Rust implementation following COOLJAPAN policies
- **Zero-copy**: Memory-mapped files and rkyv serialization
- **Performance**: SIMD optimizations, parallel processing with rayon
- **Testing**: 2520+ comprehensive tests with proptest property-based testing
- **Documentation**: Extensive examples and API documentation

### Dependencies
- Tokio async runtime
- Axum web framework
- SQLx for database operations
- serde for serialization
- rayon for parallelism
- OpenTelemetry for observability

### Notes
- This is the initial public release
- CUDA support requires Linux with NVIDIA GPU
- Feature flags available for optional dependencies (fp16, cuda, mmap, zerocopy, otel)

[0.1.0]: https://github.com/cool-japan/oxify/releases/tag/v0.1.0
