# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - v0.2.1

### Added

#### Event-Driven Architecture (`oxify-engine`)
- NATS broker bridge (`nats_bridge.rs`) — bidirectional sync between in-process `EventBus` and external NATS subjects; feature-gated (`nats`); sliding-window publish filter, `UserPass`/`JwtNkey` credentials, graceful shutdown

#### Horizontal Scaling (`oxify-server`, `oxify-storage`, `oxify-api`)
- `ReadinessChecker` trait + `ReadinessRegistry` with concurrent checks and per-checker timeout (`oxify-server/src/readiness.rs`)
- `/livez` (liveness) and `/readyz` (readiness) endpoints — k8s probe-friendly; `/health` kept as backward-compat alias
- `SessionStore` async trait + `InMemorySessionStore` (dev/test) + `RedisSessionStore` (feature `redis-cache`) for stateless multi-instance deployments

#### GitHub MCP Server (`oxify-mcp`)
- Full GitHub API server (`github.rs`) exposing 8 tools via MCP: `list_repos`, `get_repo`, `list_issues`, `create_issue`, `list_prs`, `create_pr`, `get_file`, `search_code`; feature-gated (`github`); backed by `octocrab`

#### Communications Connector (`oxify-connect-comm`) — NEW CRATE
- `MessageProvider` async trait with `send_message` / `list_channels`
- `SlackProvider` — Bot Token auth, `chat.postMessage` + `conversations.list`
- `SmtpProvider` — `lettre` SMTP with plain/HTML multipart (`smtp` feature)
- `MockMessageProvider` — thread-safe recording for tests

#### Object Storage Connector (`oxify-connect-storage`) — NEW CRATE
- `ObjectStoreProvider` async trait: `put_object`, `get_object`, `delete_object`, `list_objects`, `presigned_url`
- `MemoryStoreProvider` — full in-memory implementation for dev/testing
- `S3StoreProvider` — backed by `object_store` crate, supports AWS S3 + MinIO (`aws` feature)

#### Azure Computer Vision (`oxify-connect-vision`)
- `AzureVisionProvider` — Computer Vision Read API v2024-02-01, API-key auth, sliding-window rate limiter (20 RPS), cost tracking ($0.001/call); feature-gated (`azure-vision`)

#### TUI Mode (`oxify-cli`)
- New `oxify tui` subcommand — ratatui + crossterm terminal UI with Dashboard / Workflow List / Logs views
- Vim-style keybindings (j/k/g/G navigation, Tab to cycle views, r to refresh)
- Live API polling from `oxify-api` with graceful degradation on network failure

#### AI Workflow Generator (`oxify-cli`)
- New `oxify generate --description "..."` command — calls LLM (OpenAI/Anthropic/Ollama), extracts JSON, validates with `WorkflowValidator`, retries with error feedback (up to `--max-retries` attempts)
- Outputs to stdout or file in JSON or YAML format

### Changed

#### XML / HTML stack moved to OxiXML
- `oxify-mcp`: `web_scrape` no longer depends on `scraper`. HTML is parsed by `oxixml-html` (HTML5 tokenizer + tree builder over an `oxixml-dom` arena), and CSS selectors are handled by a new in-crate engine (`servers/css_select`) covering type/`*`/`#id`/`.class`, `[attr]` with `=`, `~=`, `^=`, `$=`, `*=`, the `:first-child`, `:last-child`, `:nth-child(An+B)` and `:not(...)` pseudo-classes, the descendant/`>`/`+`/`~` combinators, and selector lists. The selector string is caller-supplied, so the engine enforces hard caps on length, list size, compound count, qualifier count and identifier length, forbids nested `:not()`, and reports anything malformed or outside that subset as `McpError::InvalidRequest` — the pre-existing error contract. Matching is one arena walk per call with right-to-left, set-based constraint evaluation, so no selector can trigger exponential backtracking. Element text keeps each descendant text node a separate fragment (trimmed, empties dropped, space-joined), preserving the previous extraction semantics.
- `oxify-authn`: the optional `saml` XML reader is now `oxixml-quickxml-compat`, aliased to the `quick-xml` dependency name; `src/saml.rs` and the `saml` feature are unchanged.
- Dependencies removed: `scraper` (and its `html5ever` / `selectors` / `cssparser` tree) and `quick-xml`. Added: `oxixml-html`, `oxixml-dom`, `oxixml-quickxml-compat`.

### Fixed
- Pre-existing clippy lints across `oxify-model`, `oxify-authz`, `oxify-vector`, `oxify-engine`, `oxify-server`, `oxify-storage`, `oxify-connect-vision` (collapsible_match, sort_by_key, manual_checked_div, redundant bounds, deprecated pyo3 derive)

### Statistics
- Tests: **2,584 passing** (up from ~800)
- New crates: 2 (`oxify-connect-comm`, `oxify-connect-storage`)
- Zero warnings across entire workspace

---

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
