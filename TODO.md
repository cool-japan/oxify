# OxiFY - Development TODO

## v0.2.7 Additions (2026-05-31)
1. **GitHub Actions MCP server** (`oxify-mcp`, feature `github-actions`) — `GitHubActionsServer`; 8 tools (list/get/trigger/cancel/rerun workflows and runs, list artifacts); reqwest-based GitHub REST API; `from_env()` reads `GITHUB_TOKEN`; 13 new tests (99 total in crate)
2. **Local filesystem object store** (`oxify-connect-storage`, feature `local`) — `LocalFsProvider` backed by `object_store::local::LocalFileSystem::new_with_prefix`; built-in sandboxing; bucket-as-subdirectory layout; `from_env()` reads `LOCAL_STORE_ROOT`; 11 new tests (24 total in crate)
3. **SaaS data connectors** (new `oxify-connect-data` crate, 20th workspace member) — 3 new trait abstractions + providers: `GoogleSheetsProvider` (feature `google-sheets`, `SpreadsheetExecutor` trait, 5 methods), `AirtableProvider` (feature `airtable`, `TableExecutor` trait, 6 methods), `NotionProvider` (feature `notion`, `KnowledgeBaseExecutor` trait, 6 methods); 44 tests total

## Phase 0: Security & Server Foundation (OxiRS Porting) ✅ COMPLETE

**Goal**: Accelerate development by porting battle-tested security and server components from OxiRS.

### Completed Tasks
- [x] `oxify-authz`: Port ReBAC authorization engine (Zanzibar-style)
- [x] `oxify-authz`: Hybrid PostgreSQL + in-memory architecture
- [x] `oxify-authz`: Implement relation tuples and permission checks
- [x] `oxify-authz`: Add comprehensive test suite (16 tests)
- [x] `oxify-authn`: Port JWT authentication with HS256/RS256
- [x] `oxify-authn`: Port OAuth2/OIDC client with PKCE support
- [x] `oxify-authn`: Port Argon2 password manager
- [x] `oxify-authn`: Add password strength validation
- [x] `oxify-authn`: Add comprehensive test suite (16 tests)
- [x] `oxify-server`: Port Axum HTTP server runtime
- [x] `oxify-server`: Port middleware (auth, logging, CORS, compression)
- [x] `oxify-server`: Port graceful shutdown with signal handling
- [x] `oxify-server`: Add comprehensive test suite (12 tests)
- [x] `oxify-vector`: Port vector similarity search
- [x] `oxify-vector`: Support Cosine, Euclidean, Dot Product, Manhattan metrics
- [x] `oxify-vector`: Add parallel search with Rayon
- [x] `oxify-vector`: Add comprehensive test suite (8 tests)
- [x] Create complete integration example demonstrating all components
- [x] Create comprehensive README files for all new crates
- [x] Update top-level README and architecture documentation
- [x] Achieve zero warnings policy across all ported code

**Time Savings**: 8.5 hours actual vs 10-14 weeks from scratch (10x acceleration)

**Lines of Code**: 3,450+ lines of production-ready code

**Test Coverage**: 52 tests passing, all green

## Phase 1: CeleRS Integration (Prerequisite) ✅ COMPLETE

- [x] CeleRS core traits and types
- [x] CeleRS worker runtime functional
- [x] CeleRS Redis broker stable
- [ ] Integration layer between OxiFY and CeleRS (Future work)

## Phase 2: The Brain ✅ COMPLETE

### Goal
Defined DAGs (code-based) can be executed in parallel with vector search support.

### Tasks
- [x] `oxify-model`: Design Workflow data structures
- [x] `oxify-model`: Implement Node types (Start, End, LLM, etc.)
- [x] `oxify-model`: Implement Edge connections
- [x] `oxify-model`: Add execution context
- [x] `oxify-engine`: Implement topological sort
- [x] `oxify-engine`: Implement DAG executor
- [x] `oxify-engine`: Add parallel node execution (level-based)
- [x] `oxify-engine`: Implement execution state management
- [x] `oxify-connect-llm`: Implement OpenAI client
- [x] `oxify-connect-llm`: Implement Anthropic client
- [x] `oxify-connect-llm`: Add error handling for API calls
- [x] Create simple workflow execution example (simple_workflow.rs)
- [x] Create RAG workflow example (rag_workflow.rs)
- [x] Add workflow validation tests

## Phase 3: The Face ✅ API COMPLETE | 🚧 UI IN PROGRESS

### API Implementation ✅ COMPLETE
- [x] `oxify-api`: Design REST API schema ✅ COMPLETE
- [x] `oxify-api`: Implement workflow CRUD endpoints ✅ COMPLETE (30+ endpoints)
- [x] `oxify-api`: Implement execution endpoints ✅ COMPLETE
- [x] `oxify-api`: Add SSE for real-time execution updates ✅ COMPLETE
- [x] `oxify-api`: Add authentication middleware (using oxify-authn) ✅ COMPLETE
- [x] `oxify-api`: Add authorization checks (using oxify-authz) ✅ COMPLETE
- [x] `oxify-api`: Add rate limiting ✅ COMPLETE (Token bucket, 500 req/min)
- [x] Add API documentation (OpenAPI/Swagger) ✅ COMPLETE (OpenAPI 3.0 JSON endpoint)
- [x] Schedule management endpoints ✅ COMPLETE (cron-based scheduling)
- [x] Webhook management endpoints ✅ COMPLETE (HMAC signature verification)
- [x] Checkpoint/resume endpoints ✅ COMPLETE (workflow pause/resume)
- [x] Secret management endpoints ✅ COMPLETE (encrypted storage)
- [x] Version management endpoints ✅ COMPLETE (workflow versioning)
- [x] Statistics and metrics endpoints ✅ COMPLETE (execution stats)

**Ready to use**:
- JWT authentication middleware from `oxify-authn`
- ReBAC authorization checks from `oxify-authz`
- Server runtime from `oxify-server`
- Vector search from `oxify-vector`

### Web UI (Rust + Axum + Askama + HTMX)

**Tech Stack:**
- **Axum**: HTTP server (already used in oxify-api)
- **Askama**: Type-safe Jinja2-style templates (compile-time checked)
- **HTMX**: Hypermedia-driven interactions (minimal JS)
- **Tailwind CSS**: Utility-first styling (via CDN or build)
- **tower-livereload**: Hot reload in development

#### Setup & Infrastructure
- [ ] `oxify-ui`: Create new crate for web UI
  - [ ] Askama templates directory structure (`templates/`)
  - [ ] Static file serving (`/static/`)
  - [ ] tower-livereload integration for hot reload
  - [ ] Tailwind CSS setup (CDN for dev, compiled for prod)
  - [ ] Base layout template with HTMX/Alpine.js includes
  - [ ] Error page templates (404, 500, etc.)

#### DAG Visual Editor
- [ ] Canvas-based DAG editor (minimal JS library: Cytoscape.js or custom SVG)
  - [ ] Drag-and-drop node creation (HTMX + JS hybrid)
  - [ ] Visual edge connections via SVG paths
  - [ ] Minimap and zoom controls
  - [ ] Node search and filtering (HTMX instant search)
  - [ ] Auto-layout via server-side dagre/graphlib
  - [ ] Undo/redo via server state + HTMX
  - [ ] Keyboard shortcuts (Alpine.js)
  - [ ] Copy/paste nodes
  - [ ] Node grouping/collapsing

#### Workflow Management Views
- [ ] Workflow list page (HTMX infinite scroll or pagination)
  - [ ] Workflow cards with DAG preview (server-rendered SVG)
  - [ ] Search and filter (hx-trigger="keyup changed delay:300ms")
  - [ ] Sort by various fields (hx-get with query params)
  - [ ] Bulk operations (HTMX multi-select + hx-delete)
  - [ ] Workflow templates gallery
  - [ ] Import/export workflows (file upload + download)

#### Execution Monitoring Dashboard
- [ ] Real-time execution visualization
  - [ ] SSE for live updates (hx-ext="sse")
  - [ ] Per-node execution status indicators
  - [ ] Progress bars via HTMX polling or SSE
  - [ ] Cancel/pause/resume controls (hx-post)
  - [ ] Historical executions timeline
  - [ ] Token usage and cost tracking
  - [ ] Performance metrics charts (Chart.js or simple SVG)

#### Node Configuration Forms
- [ ] Dynamic forms based on node type (HTMX partials)
  - [ ] Server-side validation with Askama error display
  - [ ] Template variable autocomplete (hx-get suggestions)
  - [ ] LLM model selection dropdown
  - [ ] Vector DB connection testing (hx-post + swap)
  - [ ] Code editor (CodeMirror 6 minimal embed)
  - [ ] Preview/test node in isolation

#### Common Features
- [ ] Dark mode support (Tailwind dark: classes + localStorage)
- [ ] Responsive design (Tailwind breakpoints)
- [ ] Accessibility (semantic HTML, ARIA attributes)
- [ ] Multi-language support (Askama + fluent-rs)
- [ ] User onboarding tour (Alpine.js component)
- [ ] Keyboard-driven workflow (Alpine.js keybindings)
- [ ] Toast notifications (HTMX out-of-band swaps)
- [ ] Modal dialogs (HTMX + Alpine.js)

#### Development Experience
- [ ] `cargo watch -x run` for backend hot reload
- [ ] tower-livereload for browser auto-refresh
- [ ] Askama template compile-time checking
- [ ] Type-safe route handlers with extractors

## Node Type Implementations

### LLM Nodes ✅ COMPLETE (Enhanced)
- [x] `oxify-connect-llm`: OpenAI GPT-3.5/4 support
- [x] `oxify-connect-llm`: Anthropic Claude support
- [x] `oxify-connect-llm`: Local model support (Ollama) ✅ NEW
- [x] `oxify-connect-llm`: OpenAI embeddings (text-embedding-ada-002) ✅ NEW
- [x] `oxify-connect-llm`: Ollama embeddings (nomic-embed-text, etc.) ✅ NEW
- [x] `oxify-engine`: Real LLM execution (OpenAI, Anthropic, Ollama) ✅ NEW
- [x] `oxify-engine`: LLM response caching (1-hour TTL) ✅ NEW
- [x] `oxify-connect-llm`: Add streaming support ✅ NEW (OpenAI + Anthropic SSE)
- [x] `oxify-connect-llm`: Implement prompt template engine

### Vector Database Nodes ✅ COMPLETE (Enhanced)
- [x] `oxify-connect-vector`: Qdrant client implementation
- [x] `oxify-connect-vector`: pgvector client implementation
- [x] `oxify-connect-vector`: VectorProvider trait abstraction
- [x] `oxify-connect-vector`: Collection management (create, exists)
- [x] `oxify-connect-vector`: Search with filters and score thresholds
- [x] `oxify-connect-vector`: Insert and delete operations
- [x] `oxify-connect-vector`: Embedding generation integration ✅ NEW
- [x] `oxify-connect-vector`: EmbeddingVectorStore (text→embedding→insert) ✅ NEW
- [x] `oxify-connect-vector`: Search by text (automatic embedding) ✅ NEW
- [x] `oxify-engine`: Real Qdrant search execution ✅ NEW
- [x] `oxify-engine`: Real pgvector search execution ✅ NEW
- [x] `oxify-engine`: Automatic embedding generation for queries ✅ NEW
- [x] `oxify-connect-vector`: Implement hybrid search ✅ NEW (BM25 + RRF)
- [x] Add vector store management endpoints ✅ COMPLETE (API defined: create/list/get/delete collection, insert/search/delete vectors)

### Code Execution Nodes ✅ COMPLETE
- [x] `oxify-engine`: Design safe Rust script execution ✅ NEW
- [x] `oxify-engine`: Add WebAssembly runtime support (optional) ✅ NEW
- [x] `oxify-engine`: Implement sandboxing/isolation ✅ NEW
- [x] `oxify-engine`: Add resource limits (CPU, memory, time) ✅ NEW
- [x] Create code execution examples ✅ (examples/code_execution_workflow.rs)

### Conditional Nodes ✅ COMPLETE
- [x] `oxify-engine`: Implement expression evaluator ✅
- [x] `oxify-engine`: Add support for JSONPath queries ✅
- [x] `oxify-engine`: Implement conditional routing ✅
- [x] Support for comparisons, logical operators ✅
- [x] Access to node results and variables ✅
- [x] Add conditional node examples ✅ (examples/conditional_workflow.rs)

### MCP Integration / Tool Nodes ✅ MOSTLY COMPLETE
- [x] `oxify-engine`: HTTP tool executor (GET/POST/PUT/PATCH/DELETE) ✅ NEW
- [x] `oxify-engine`: JSON request/response handling ✅ NEW
- [x] `oxify-mcp`: MCP Protocol Implementation ✅ COMPLETE
  - [x] McpClient trait and DefaultMcpClient implementation
  - [x] McpServer trait for building custom servers
  - [x] McpRegistry for managing multiple servers
  - [x] Stdio transport (launch MCP servers as subprocesses) ✅
  - [x] HTTP transport (connect to remote MCP servers) ✅
  - [x] Tool discovery and schema parsing ✅
  - [x] Tool invocation with parameter validation ✅
  - [x] Result parsing and error handling ✅
- [x] `oxify-mcp`: Implement MCP server (expose workflows as MCP tools) ✅ COMPLETE
  - [x] Serve OxiFY workflows via MCP protocol (WorkflowServer)
  - [x] Auto-generate tool schemas from workflow metadata
  - [x] Custom input schemas and descriptions support
  - [x] Pluggable executor for workflow execution
  - [ ] Integration examples with Claude Desktop, Cline, Zed (documentation)
- [x] `oxify-mcp`: Built-in MCP servers ✅ COMPLETE
  - [x] Filesystem server (read, write, list, delete files) ✅
  - [x] Web browser server (fetch URLs, scrape) ✅
  - [x] Database server (PostgreSQL queries with sqlx) ✅ COMPLETE
  - [x] Shell server (execute safe shell commands) ✅
  - [x] Git server (clone, commit, push, pull) ✅
- [x] `oxify-mcp`: Authentication & Load Balancing ✅ COMPLETE
  - [x] ApiKey, Basic, Bearer, CustomHeader auth methods
  - [x] CredentialStore for multi-server credentials
  - [x] AuthenticatedHttpTransport
  - [x] Round-robin, Least Connections, Random, Weighted load balancing
  - [x] Server health tracking (Healthy/Degraded/Unhealthy)
  - [x] Failover with invoke_tool_with_failover
  - [x] Server metrics (request/error counts, response times)
  - [x] Tag-based and group-based server selection
- [x] `oxify-mcp`: Testing ✅ COMPLETE (60 unit tests)
- [x] Create MCP integration examples ✅
  - [ ] Claude Desktop integration example
  - [x] Multi-server orchestration example ✅ (crates/oxify-mcp/examples/multi_server_orchestration.rs)
  - [ ] Custom MCP server implementation guide

## Storage & Persistence

- [x] Design database schema for workflows ✅ COMPLETE (PostgreSQL with JSONB)
- [x] Design database schema for executions ✅ COMPLETE (PostgreSQL with JSONB)
- [x] Implement workflow versioning ✅ COMPLETE
- [x] Add execution history tracking ✅ COMPLETE (Database-backed)
- [x] Implement result storage backend ✅ COMPLETE (Dual-mode: In-memory/Database)
- [x] Add workflow import/export (JSON) ✅ COMPLETE

## Advanced Features

### Workflow Enhancements
- [x] Add sub-workflow support (workflow composition) ✅ NEW
- [x] Implement parallel node execution ✅
- [x] Add loop/iteration nodes ✅ NEW (ForEach, While, Repeat)
- [x] Implement error handling nodes (try-catch) ✅ NEW (Try-Catch-Finally)
- [x] Add human-in-the-loop nodes (approval gates) ✅ COMPLETE (ApprovalStore, FormStore)

### Execution Features ✅ COMPLETE
- [x] Per-node retry logic with exponential backoff ✅ NEW
- [x] Retry count tracking in execution results ✅ NEW
- [x] Add workflow pause/resume capability ✅ COMPLETE
- [x] Implement checkpoint/recovery system ✅ COMPLETE
- [x] Add execution rollback mechanism ✅ COMPLETE
  - [x] ExecutionSnapshot for point-in-time state capture
  - [x] RollbackManager with configurable history
  - [x] Automatic snapshot support (every N nodes)
  - [x] Rollback to specific snapshot or N steps
  - [x] 15 comprehensive tests
  - [x] API endpoints: create/list/delete snapshots, rollback, summary
- [x] Implement execution scheduling (cron-like) ✅ COMPLETE
- [x] Add workflow triggers (webhooks, events) ✅ COMPLETE

### Optimization ✅ COMPLETE
- [x] Implement execution plan caching ✅ COMPLETE (100-entry LRU cache with workflow hash validation)
- [x] Add intelligent node execution batching ✅ COMPLETE (oxify-model/batching.rs - BatchAnalyzer with BatchPlan)
- [x] Optimize variable passing between nodes ✅ COMPLETE (oxify-engine VariableStore with Arc-backed storage)
- [x] Add execution cost estimation ✅ COMPLETE (oxify-model/cost.rs - CostEstimator with per-model pricing)
- [x] Implement execution time prediction ✅ COMPLETE (oxify-model/prediction.rs - TimePredictor with historical data)

## CLI Tool ✅ COMPLETE

- [x] `oxify-model`: Comprehensive workflow validation ✅ (cycles, orphans, start/end, conditionals)
- [x] `oxify-cli`: Implement workflow validation command ✅ COMPLETE
- [x] `oxify-cli`: Add local execution mode ✅ COMPLETE (`oxify run` command)
- [x] `oxify-cli`: Add workflow scaffolding commands ✅ COMPLETE (`oxify scaffold` command)
- [x] `oxify-cli`: Add workflow testing framework ✅ COMPLETE (`oxify test` command)
- [x] `oxify-cli`: Add schedule management ✅ COMPLETE (`oxify schedule` subcommand)
- [x] `oxify-cli`: Add webhook management ✅ COMPLETE (`oxify webhook` subcommand)
- [x] `oxify-cli`: Add checkpoint management ✅ COMPLETE (`oxify checkpoint` subcommand)
- [x] `oxify-cli`: Add secret management ✅ COMPLETE (`oxify secret` subcommand)
- [x] `oxify-cli`: Add version management ✅ COMPLETE (`oxify version` subcommand)
- [x] `oxify-cli`: Add cost estimation ✅ COMPLETE (`oxify cost` command)
- [x] `oxify-cli`: Add workflow analysis ✅ COMPLETE (`oxify analyze` command)
- [x] `oxify-cli`: Add visualization ✅ COMPLETE (`oxify visualize` command)
- [x] `oxify-cli`: Add statistics tracking ✅ COMPLETE (`oxify stats` subcommand)
- [x] `oxify-cli`: Add shell completion generation ✅ COMPLETE
- [ ] `oxify-cli`: Add deployment commands (Future enhancement)

## Performance & Scalability

### Performance Targets
- [ ] **Execution Performance:**
  - [ ] <100ms overhead per workflow execution
  - [ ] <10ms per node execution overhead
  - [ ] Support 1000+ node workflows
  - [ ] 100+ concurrent executions per server

- [ ] **API Performance:**
  - [ ] <50ms p95 for workflow CRUD operations
  - [ ] <100ms p95 for execution start
  - [ ] 10,000+ req/sec throughput (with horizontal scaling)
  - [ ] <1ms JWT validation (with caching)
  - [ ] <100μs ReBAC permission checks (with caching)

- [ ] **Database Performance:**
  - [ ] <10ms workflow queries (with indexing)
  - [ ] <20ms execution history queries
  - [ ] Support 1M+ workflows
  - [ ] Support 100M+ executions
  - [ ] <5ms vector similarity search (Qdrant/pgvector)

### Optimization Strategies
- [x] **Caching:** ✅ COMPLETE
  - [x] LLM response caching (1-hour TTL) ✅
  - [x] Execution plan caching (100-entry LRU) ✅
  - [x] Vector search result caching ✅ NEW (oxify-storage/vector_cache.rs)
  - [x] Workflow compilation caching ✅ (oxify-storage/cache.rs)
  - [x] JWT public key caching (JWKS) ✅ NEW (oxify-storage/jwks_cache.rs)
  - [x] L1 (in-memory) + L2 (Redis) caching ✅ NEW (oxify-storage/redis_cache.rs)

- [x] **Connection Pooling:** ✅ COMPLETE
  - [x] Database connection pooling (sqlx::PgPool) ✅ (oxify-storage/pool.rs)
  - [x] HTTP connection pooling (reqwest client reuse in RestConnector) ✅
  - [ ] Vector DB connection pooling

- [ ] **Horizontal Scaling:**
  - [ ] Stateless API servers (12-factor)
  - [ ] Load balancing (nginx/HAProxy)
  - [x] Session sharing via Redis ✅ NEW (`oxify-storage` RedisSessionStore)
  - [x] Distributed tracing (OpenTelemetry) ✅ NEW (`oxify-engine` — `#[tracing::instrument]` on `execute_with_config`, `execute_node_with_retry`, `execute_node`; `tracing-opentelemetry` bridge in `init_tracing`; `trace_workflow_async`/`trace_node_async` helpers; `build_otel_provider` for composable setup; `otel` feature)
  - [x] Health checks and readiness probes ✅ NEW (`/livez`, `/readyz` endpoints)

- [x] **Resource Limits:** ✅ COMPLETE
  - [x] Per-workflow memory limits (ResourceLimits, ResourceEnforcer)
  - [x] Per-workflow timeout limits (max_execution_time_secs)
  - [x] Per-user execution quotas (UserQuota, UserQuotaManager with tiers)
  - [x] Token budget limits for LLM calls (TokenBudget with reservation system)
  - [x] API call limits, concurrent node limits, total node limits
  - [x] Warning thresholds for approaching limits
  - [x] 10 comprehensive tests

## Integration & Ecosystem

### Pre-built Integrations
- [ ] **Communication:**
  - [x] Slack integration (send messages, read channels) ✅ NEW (`oxify-connect-comm`)
  - [x] Discord integration ✅ NEW (`oxify-connect-comm` DiscordProvider, feature `discord` — Bot API v10, DM support, guild channel listing)
  - [x] Email (SMTP/SendGrid/SES) ✅ NEW (`oxify-connect-comm` lettre-backed)
  - [x] SMS (Twilio) ✅ NEW (`oxify-connect-comm` TwilioProvider, feature `twilio` — Twilio Messaging API, basic auth, form-encoded body; `from_env()` reads `TWILIO_ACCOUNT_SID`/`TWILIO_AUTH_TOKEN`/`TWILIO_FROM_NUMBER`)
  - [x] Push notifications (OneSignal) ✅ NEW (`oxify-connect-comm` OneSignalProvider, feature `onesignal` — JSON REST API, `Authorization: Basic {key}` header, player IDs + segments; `errors[]`-on-200 detection; `from_env()` reads `ONESIGNAL_APP_ID`/`ONESIGNAL_REST_API_KEY`)
  - [x] Push notifications (Firebase FCM) ✅ NEW (`oxify-connect-comm` FirebaseFcmProvider, feature `firebase` — FCM v1 API; RS256 JWT + Google OAuth2 token exchange; token caching with RwLock; `Recipient::User`→device token, `Recipient::Channel`→topic; `from_env()` reads `FIREBASE_PROJECT_ID`/`FIREBASE_SERVICE_ACCOUNT_JSON`)

- [ ] **Developer Tools:**
  - [x] GitHub Actions integration ✅ NEW (`oxify-mcp` GitHubActionsServer, feature `github-actions` — 8 tools: list_workflows/get_workflow/list_runs/get_run/trigger_workflow/cancel_run/rerun_failed_jobs/list_artifacts; `Authorization: Bearer` header; query-param filtering for runs (branch, status); `from_env()` reads `GITHUB_TOKEN`/`GITHUB_DEFAULT_OWNER`/`GITHUB_DEFAULT_REPO`)
  - [ ] GitHub API (issues, PRs, commits)
  - [x] GitLab API ✅ NEW (`oxify-mcp` GitLabServer, feature `gitlab` — 8 tools: list/get project, list/create issues, list/create MRs, get file, search)
  - [x] Jira integration ✅ NEW (`oxify-mcp` JiraServer, feature `jira` — 8 tools: list/get project, search/get/create issues, add comment, list/transition)
  - [x] Linear integration ✅ NEW (`oxify-mcp` LinearServer, feature `linear` — 8 tools: list_teams/list_issues/get_issue/create_issue/update_issue/list_projects/create_comment/search_issues; GraphQL API with raw API-key auth)

- [ ] **Data Sources:**
  - [x] REST API connector (generic) ✅ COMPLETE
    - [x] All HTTP methods (GET, POST, PUT, PATCH, DELETE)
    - [x] Auth: Bearer, API Key, Basic, OAuth2, Custom
    - [x] Rate limiting with time window
    - [x] Retry with exponential backoff
    - [x] Response caching with TTL
    - [x] Request templates with variable substitution
    - [x] 8 comprehensive tests
  - [x] GraphQL connector ✅ NEW (`oxify-connect-graphql` crate — `HttpGraphQlProvider` implementing `GraphQlExecutor` trait; `AuthConfig` (None/Bearer/ApiKey/Basic/Custom); exponential retry; inspects `errors[]` on 200; optional `introspection` feature; `from_env()` reads `GRAPHQL_ENDPOINT`/`GRAPHQL_BEARER_TOKEN`)
  - [x] Database connectors (PostgreSQL, MySQL, MongoDB) ✅ NEW (`oxify-connect-db` crate — `SqlExecutor` trait (fetch_all/execute/health_check) with `PostgresProvider` (feature `postgres`) + `MySqlProvider` (feature `mysql`); `DocumentStore` trait (insert_one/find/update_one/delete_one/count) with `MongoProvider` (feature `mongodb-store`); dynamic JSON param binding; column-type-aware row→JSON extraction; `from_env()` reads `DATABASE_URL`/`MONGODB_URI`/`MONGODB_DATABASE`)
  - [x] Google Sheets integration ✅ NEW (`oxify-connect-data` GoogleSheetsProvider, feature `google-sheets` — `SpreadsheetExecutor` trait; get_values/update_values/append_values/clear_range/batch_get; Google Sheets API v4; `Authorization: Bearer` OAuth2 token; `from_env()` reads `GOOGLE_SHEETS_ACCESS_TOKEN`)
  - [x] Airtable integration ✅ NEW (`oxify-connect-data` AirtableProvider, feature `airtable` — `TableExecutor` trait; list_records/get_record/create_record/update_record/delete_record/search_records; Airtable REST API v0; filter formula query param; `from_env()` reads `AIRTABLE_API_KEY`)
  - [x] Notion API ✅ NEW (`oxify-connect-data` NotionProvider, feature `notion` — `KnowledgeBaseExecutor` trait; search/get_page/create_page/update_page/query_database/get_database; Notion API v1; `Notion-Version: 2022-06-28` header; `from_env()` reads `NOTION_API_KEY`)

- [ ] **AI/ML Platforms:**
  - [x] Hugging Face integration ✅ NEW (`oxify-connect-llm` HuggingFaceProvider — LLM, streaming, embeddings via HF Inference Router)
  - [x] Replicate integration ✅ NEW (`oxify-connect-llm` ReplicateProvider — async prediction lifecycle, polling, SSE streaming; version-pinned and model-based prediction URLs)
  - [x] AWS SageMaker ✅ NEW (`oxify-connect-llm` SageMakerProvider — TGI/Llama request format; AWS Sig v4 signing via `aws_sigv4` module; `invocations` endpoint; `with_base_url()` seam; `from_env()` reads `AWS_SAGEMAKER_REGION`/`AWS_SAGEMAKER_ENDPOINT`; `AwsCredentials` from env)
  - [x] Google Vertex AI ✅ NEW (`oxify-connect-llm` VertexAiProvider — implements `LlmProvider` + `StreamingLlmProvider` + `EmbeddingProvider`; identical `generateContent` schema to Gemini; `Authorization: Bearer` OAuth2 token auth; publishers path `projects/{}/locations/{}/publishers/google/models/{}:generateContent`; `from_env()` reads `GOOGLE_VERTEX_ACCESS_TOKEN`/`GOOGLE_VERTEX_PROJECT`/`GOOGLE_VERTEX_LOCATION`)

- [ ] **Storage:**
  - [x] AWS S3 integration ✅ NEW (`oxify-connect-storage` S3StoreProvider)
  - [x] Google Cloud Storage ✅ NEW (`oxify-connect-storage` GcsStoreProvider, feature `gcs` — `GoogleCloudStorageBuilder`; service account key path and JSON)
  - [x] Azure Blob Storage ✅ NEW (`oxify-connect-storage` AzureBlobStoreProvider, feature `azure` — `MicrosoftAzureBuilder`; account key + custom endpoint)
  - [x] Local filesystem (with sandboxing) ✅ NEW (`oxify-connect-storage` LocalFsProvider, feature `local` — `object_store::local::LocalFileSystem::new_with_prefix` for built-in path sandboxing; bucket-as-subdirectory layout; `create_if_missing` flag; `presigned_url`→Unsupported; `from_env()` reads `LOCAL_STORE_ROOT`)

### Extensibility
- [ ] **Plugin System:**
  - [ ] Custom node type registration
  - [ ] Plugin manifest format (TOML/YAML)
  - [ ] Hot-reload plugins in development
  - [ ] Plugin marketplace/registry
  - [ ] Plugin versioning and dependencies
  - [ ] WASM plugin support (run untrusted code safely)

- [x] **Webhook Triggers:** ✅ COMPLETE
  - [x] Webhook endpoint generation ✅ COMPLETE
  - [x] Signature verification (HMAC) ✅ COMPLETE
  - [x] Payload transformation ✅ COMPLETE
    - [x] PayloadTransform with fluent builder API
    - [x] Operations: extract, rename, add_constant, remove, filter, template
    - [x] String transforms: uppercase, lowercase, trim, replace, split, regex
    - [x] Value mapping with defaults
    - [x] JSONPath-like field extraction
    - [x] TransformPipeline for chaining transforms
    - [x] 16 comprehensive tests
  - [x] Automatic workflow triggering ✅ COMPLETE
  - [x] Retry logic for webhook failures ✅ COMPLETE
    - [x] WebhookRetryConfig with exponential backoff
    - [x] WebhookRetryManager for queue management
    - [x] RetryState tracking per event
    - [x] Configurable max retries, delays, jitter
    - [x] 10 comprehensive tests
  - [x] Webhook delivery service ✅ COMPLETE
    - [x] WebhookDeliveryService with retry integration
    - [x] HTTP delivery with configurable timeouts
    - [x] Automatic HMAC signature generation
    - [x] Delivery statistics tracking
    - [x] Event queue management
    - [x] 8 comprehensive tests

- [ ] **Event-Driven Architecture:**
  - [ ] Workflow triggers (on schedule, on event, on webhook)
  - [x] Event bus integration (NATS) ✅ NEW (`oxify-engine` NatsBridge, feature `nats`)
  - [x] Event bus integration (Kafka) ✅ NEW (`oxify-engine` KafkaBridge, feature `kafka`)
  - [x] Event bus integration (RabbitMQ) ✅ NEW (`oxify-engine` RabbitMqBridge, feature `rabbitmq`)
  - [ ] Pub/sub pattern support
  - [ ] Event sourcing for executions

## Documentation

- [ ] Write comprehensive user guide
- [ ] Create node type reference documentation
- [ ] Add workflow design best practices
- [ ] Create video tutorials
- [ ] Add example workflows library (RAG, agents, etc.)
- [ ] Write deployment guide

## Testing

- [ ] Add unit tests for all node types
- [ ] Add integration tests for workflows
- [ ] Add E2E tests for API
- [ ] Add UI component tests
- [ ] Create performance benchmarks
- [ ] Test with real LLM providers

## DevOps & Deployment

- [ ] Create Docker images
- [ ] Create Kubernetes manifests
- [ ] Add Helm charts
- [ ] Set up CI/CD pipeline
- [ ] Add health checks and monitoring
- [ ] Create deployment documentation

## Security

- [ ] Add API key management for LLM providers
- [ ] Implement secrets management
- [ ] Add workflow execution sandboxing
- [ ] Implement rate limiting per workflow
- [ ] Add audit logging
- [ ] Security audit and penetration testing

## Future Ideas (Post v1.0)

### Collaboration & Social
- [ ] **Collaborative workflow editing:**
  - [ ] Real-time collaborative editing (CRDT or OT)
  - [ ] Cursor presence indicators
  - [ ] Comments and annotations on nodes
  - [ ] Change history and blame
  - [ ] Workflow sharing (view-only, edit, admin)
  - [ ] Team workspaces

- [ ] **Workflow marketplace:**
  - [ ] Public workflow templates library
  - [ ] User-contributed workflows
  - [ ] Workflow ratings and reviews
  - [ ] Workflow categories and tags
  - [ ] One-click workflow installation
  - [ ] Paid premium workflows (with revenue sharing)

### AI-Powered Features
- [ ] **Workflow Generation:**
  - [ ] Natural language to workflow (LLM-powered)
  - [ ] "Create a RAG workflow with Qdrant and GPT-4" → auto-generate
  - [ ] Workflow suggestions based on description
  - [ ] Auto-complete workflow patterns

- [ ] **Intelligent Optimization:**
  - [ ] Automatic workflow optimization (remove redundant nodes)
  - [ ] Performance bottleneck detection
  - [ ] Cost optimization suggestions (cheaper LLM models)
  - [ ] Parallelization opportunities detection
  - [ ] Caching recommendations

- [ ] **Workflow Analytics with AI:**
  - [ ] Anomaly detection in executions
  - [ ] Failure prediction (ML model)
  - [ ] Success rate prediction before execution
  - [ ] Automatic error categorization
  - [ ] Smart retry strategies (learn from failures)

### Advanced Testing & Quality
- [ ] **A/B Testing for workflows:**
  - [ ] Run two workflow versions in parallel
  - [ ] Traffic splitting (50/50, 90/10, etc.)
  - [ ] Statistical significance testing
  - [ ] Automatic winner selection
  - [ ] Gradual rollout (canary deployments)

- [ ] **Workflow Simulation:**
  - [ ] Monte Carlo simulation for probabilistic workflows
  - [ ] Load testing (simulate 1000s of executions)
  - [ ] Cost estimation before production
  - [ ] Failure scenario testing

### Enterprise Features
- [ ] **Multi-tenancy support:**
  - [ ] Tenant isolation (database-level or schema-level)
  - [ ] Per-tenant rate limiting
  - [ ] Per-tenant resource quotas
  - [ ] Tenant-level analytics
  - [ ] Cross-tenant workflow sharing (with explicit permission)

- [ ] **Compliance & Governance:**
  - [ ] SOC 2 compliance toolkit
  - [ ] GDPR compliance features (data export, deletion)
  - [ ] HIPAA compliance mode (encrypted storage, audit logs)
  - [ ] PCI DSS compliance for payment workflows
  - [ ] Data residency controls (EU/US/Asia regions)

- [ ] **Advanced Security:**
  - [ ] Secrets management (HashiCorp Vault integration)
  - [ ] Key rotation policies
  - [ ] Zero-trust security model
  - [ ] IP allow/deny lists
  - [ ] API key scope limitations
  - [ ] Workflow execution sandboxing (gVisor, Firecracker)

### Developer Experience
- [ ] **IDE Integrations:**
  - [ ] VS Code extension (workflow editor, debugger)
  - [ ] IntelliJ plugin
  - [ ] Vim/Neovim plugin
  - [ ] Emacs mode

- [x] **TypeScript/WASM Bindings:** ✅ COMPLETE
  - [x] wasm-bindgen integration for oxify-model
  - [x] WasmWorkflow for workflow manipulation (JSON/YAML roundtrip)
  - [x] WasmWorkflowBuilder for fluent workflow construction
  - [x] Node types: LLM, Code, Retriever, IfElse, Switch, Tool, Loop
  - [x] WasmWorkflowUtils for utility functions (UUID, JSON/YAML conversion)
  - [x] 11 comprehensive tests
  - [x] Compile with `wasm-pack build --features wasm`
  - [x] TypeScript type definitions (.d.ts generation) ✅ COMPLETE
    - [x] generate_typescript_definitions() function
    - [x] Comprehensive type coverage for all workflow types
    - [x] 6 tests for type definition generation

- [ ] **SDK Generation:**
  - [ ] TypeScript SDK (auto-generated from OpenAPI)
  - [ ] Python SDK
  - [ ] Go SDK
  - [ ] Rust SDK (native)
  - [ ] Java SDK

- [ ] **Workflow-as-Code:**
  - [ ] Define workflows in Rust (compile-time validation)
  - [ ] Define workflows in Python (runtime execution)
  - [ ] Define workflows in TypeScript (for Web UI)
  - [ ] Workflow DSL (custom language)

---

## Completed Features Summary

### Core API Features ✅
- **OpenAPI Documentation Endpoint:** `GET /api-docs/openapi.json` with full schema definitions
- **Rate Limiting Middleware:** Token bucket algorithm with configurable limits (100-1000 req/min)
- **Execution Scheduling:** Full cron-based scheduling system with timezone support
- **Webhook Triggers:** HMAC signature verification, event filtering, statistics tracking

### Workflow Engine Features ✅
- **Loop/Iteration Nodes:** ForEach, While, Repeat with safety limits
- **Error Handling Nodes:** Try-Catch-Finally with error propagation
- **Sub-Workflow Execution:** Variable mappings and context inheritance
- **Ollama Streaming:** Token-by-token streaming support

### Storage Layer Features ✅
- **Vector Search Caching:** SHA-256 hash-based with LRU eviction
- **JWKS Caching:** Per-issuer with automatic refresh
- **Two-Level Cache:** L1 (in-memory) + L2 (Redis) architecture
- **Batch Operations:** Bulk updates and deletions with pagination
- **Cache Warming:** Proactive population on startup
- **Prometheus Metrics:** Full metrics export support
- **Automated Maintenance:** Scheduled VACUUM, ANALYZE, and cleanup

---

## Summary of Current Status (As of 2026-03-29)

**OxiFY v0.2.0** is a production-ready LLM workflow orchestration platform with comprehensive features:

### ✅ Core Infrastructure (COMPLETE)
- **Security & Auth**: ReBAC (oxify-authz), JWT/OAuth2 (oxify-authn), password management
- **API Server**: Full REST API with 30+ endpoints, OpenAPI 3.0 docs, rate limiting
- **Storage**: PostgreSQL-backed workflow/execution storage with versioning
- **Vector Search**: In-memory and distributed vector search (oxify-vector)

### ✅ Workflow Engine (COMPLETE)
- **Execution**: DAG executor with topological sort, parallel execution, retry logic
- **Node Types**: LLM, Vector, Code, Conditional, Loop, Try-Catch, Sub-workflow, HTTP Tool
- **LLM Providers**: OpenAI, Anthropic, Ollama (with streaming support)
- **Vector DBs**: Qdrant, pgvector (with hybrid search, BM25 + RRF)
- **Advanced Features**: Checkpointing, pause/resume, scheduling, webhooks

### ✅ CLI Tool (COMPLETE)
- **Commands**: run, test, scaffold, visualize, analyze, cost, schedule, webhook, checkpoint
- **Workflow Management**: validate, create, update, delete, list, export/import
- **Execution**: local execution, remote API calls, stats tracking
- **Development**: template scaffolding, shell completion generation

### 🚧 Web UI (NOT STARTED)
- React Flow DAG editor
- Real-time execution monitoring
- Workflow management interface

### 📊 Statistics
- **Lines of Code**: 18,200+ production code
- **Crates**: 12 workspace crates
- **Tests**: 129+ passing tests
- **API Endpoints**: 30+ REST endpoints
- **Node Types**: 15+ node types
- **CLI Commands**: 50+ commands
- **Caching Systems**: 6 cache types (LLM, execution plan, vector, workflow, JWKS, two-level)
- **Storage Features**: Caching, warming, metrics export, automated maintenance, batch ops
- **Zero Warnings**: All code compiles cleanly

---

---

## v0.2.1 Additions (2026-04-27)

### ✅ New Features
- **NATS Broker Bridge** (`oxify-engine` + `nats` feature) — distributed event pub/sub
- **k8s Health Probes** (`/livez`, `/readyz` in `oxify-api`) + `ReadinessRegistry`
- **Redis SessionStore** (`oxify-storage`) — stateless horizontal scaling
- **GitHub MCP Server** (`oxify-mcp` + `github` feature) — 8 GitHub tools via MCP
- **`oxify-connect-comm`** — new crate: Slack + SMTP + Mock message providers
- **`oxify-connect-storage`** — new crate: S3/MinIO + in-memory object storage
- **Azure Computer Vision** (`oxify-connect-vision` + `azure-vision` feature) — 5th OCR provider
- **`oxify tui`** CLI command — ratatui terminal UI (Dashboard/Workflows/Logs)
- **`oxify generate`** CLI command — NL→workflow generation with LLM + validation retry

### 📊 Updated Statistics
- **Crates**: 17 workspace crates (up from 15)
- **Tests**: 2,584+ passing (up from ~800)
- **Zero Warnings**: All code compiles cleanly across all features

**Last Updated:** 2026-04-27
**Document Version:** 2.1

---

## v0.2.2 Additions (2026-05-31)

### ✅ New Features
- **Kafka Bridge** (`oxify-engine` + `kafka` feature) — `KafkaBridge`: EventBus→Kafka publish-only bridge; SASL/Plain + SASL/SCRAM auth; workflow_id as partition key
- **RabbitMQ Bridge** (`oxify-engine` + `rabbitmq` feature) — `RabbitMqBridge`: EventBus→RabbitMQ AMQP topic-exchange bridge; durable exchange support; routing key `{prefix}.{event_type}.{workflow_id}`
- **GitLab MCP Server** (`oxify-mcp` + `gitlab` feature) — `GitLabServer`: 8 tools (list/get project, list/create issues, list/create MRs, get file with base64 decode, search code)
- **Jira MCP Server** (`oxify-mcp` + `jira` feature) — `JiraServer`: 8 tools (list/get project, JQL search, get/create issues, add comment, list/transition statuses; ADF body wrapping)
- **HuggingFace LLM Provider** (`oxify-connect-llm`) — `HuggingFaceProvider`: implements `LlmProvider` + `StreamingLlmProvider` + `EmbeddingProvider` via HF Inference Router (OpenAI-compatible); `with_base_url()` for custom endpoints; 429 rate-limit detection

### 🔧 Pre-existing Fix
- **`oxify-authn` benchmark** — added `required-features` to `[[bench]]` to prevent compile errors when optional feature modules (session, ratelimit, apikey, metrics) are not enabled

### 📊 Updated Statistics
- **Crates**: 17 workspace crates (unchanged)
- **Tests (default features)**: 2,545+ passing (doc-test suite included)
- **Tests (with kafka+rabbitmq)**: +316 (engine suite with new bridge unit tests)
- **Tests (with gitlab+jira)**: +98 (mcp suite with 16 new GitLab+Jira unit tests)
- **Tests (connect-llm)**: 244 (209 unit + 6 new HuggingFace + 29 doc-tests)
- **Zero Warnings**: All code compiles cleanly across all feature combinations

**Last Updated:** 2026-05-31
**Document Version:** 2.2

---

## v0.2.3 Additions (2026-05-31)

### ✅ New Features
- **Discord Provider** (`oxify-connect-comm` + `discord` feature) — `DiscordProvider`: Bot API v10; `send_message` to channel or DM (auto-creates DM channel); `list_channels` for a guild (text + announcement channels); `DiscordConfig` with `from_env()` reading `DISCORD_BOT_TOKEN` / `DISCORD_GUILD_ID`
- **Google Cloud Storage** (`oxify-connect-storage` + `gcs` feature) — `GcsStoreProvider`: `GoogleCloudStorageBuilder`; service account via key file path or JSON string; `from_env()` reading `GCS_BUCKET` / `GOOGLE_APPLICATION_CREDENTIALS` / `GCS_SERVICE_ACCOUNT_KEY`
- **Azure Blob Storage** (`oxify-connect-storage` + `azure` feature) — `AzureBlobStoreProvider`: `MicrosoftAzureBuilder`; account key auth; custom endpoint for Azurite; `from_env()` reading `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_CONTAINER` / `AZURE_STORAGE_KEY` / `AZURE_STORAGE_ENDPOINT`
- **Replicate LLM Provider** (`oxify-connect-llm`) — `ReplicateProvider`: async prediction lifecycle with polling (`poll_to_completion`); SSE streaming via `urls.stream`; supports version-pinned (`owner/model:sha256:ver`) and model-based (`owner/model`) prediction URLs; 429 rate-limit detection; `output_to_text` collapses token arrays

### 📊 Updated Statistics
- **Tests (discord feature)**: 17 unit + 2 doc-tests passing (connect-comm)
- **Tests (gcs+azure features)**: 19 unit + 1 doc-test passing (connect-storage)
- **Tests (connect-llm)**: 228 unit + 29 doc-tests passing (includes 10 new Replicate tests)
- **Zero Warnings**: All code compiles cleanly across all feature combinations

**Last Updated:** 2026-05-31
**Document Version:** 2.3

---

## v0.2.4 Additions (2026-05-31)

### ✅ New Features
- **`oxify-connect-graphql`** (new crate, 18th workspace member) — `HttpGraphQlProvider` implementing `GraphQlExecutor` trait; `AuthConfig` enum (None/Bearer/ApiKey/Basic/Custom) with `bearer()`/`api_key()`/`basic()` constructors; exponential retry on 408/429/5xx; GraphQL-specific error inspection (`errors[]` on 200 response); optional `introspection` feature; `from_env()` reads `GRAPHQL_ENDPOINT`/`GRAPHQL_BEARER_TOKEN`
- **Twilio SMS Provider** (`oxify-connect-comm` + `twilio` feature) — `TwilioProvider`: Twilio Messaging API (POST form-encoded body); HTTP Basic auth (`AccountSid:AuthToken`); `from_env()` reads `TWILIO_ACCOUNT_SID`/`TWILIO_AUTH_TOKEN`/`TWILIO_FROM_NUMBER`; `list_channels` → `Unsupported`
- **Linear MCP Server** (`oxify-mcp` + `linear` feature) — `LinearServer`: 8 GraphQL tools (list_teams, list_issues, get_issue, create_issue, update_issue, list_projects, create_comment, search_issues); raw API-key auth (`Authorization: {key}`); GraphQL `errors[]` detection on 200; `from_env()` reads `LINEAR_API_KEY`

### 📊 Updated Statistics
- **Crates**: 18 workspace crates (up from 17 — new `oxify-connect-graphql`)
- **Tests (connect-graphql, all-features)**: 26 unit + 1 doc-test passing
- **Tests (connect-comm, twilio feature)**: 18 unit + 2 doc-tests passing
- **Tests (oxify-mcp, linear feature)**: 94 unit + 1 ignored passing
- **Zero Warnings**: All code compiles cleanly across all feature combinations

**Last Updated:** 2026-05-31
**Document Version:** 2.4

---

## v0.2.5 Additions (2026-05-31)

### ✅ New Features
- **`oxify-connect-db`** (new crate, 19th workspace member) — `SqlExecutor` trait (`fetch_all`/`execute`/`health_check`); `PostgresProvider` (feature `postgres`, sqlx PgPool, PgArguments dynamic binding, column-type-aware row→JSON extraction for INT2/4/8/FLOAT4/8/BOOL/TEXT/BYTEA/JSON/JSONB/UUID/TIMESTAMP/fallback→string); `MySqlProvider` (feature `mysql`, same pattern for MySQL type names); `DocumentStore` trait (5 methods); `MongoProvider` (feature `mongodb-store`, mongodb 3.7.0, `serde_json::Value`↔`bson::Document` conversion, cursor iteration); `DbConfig`/`MongoConfig` with `from_env()`. `mongodb = "3.7.0"` added to workspace deps.
- **Vertex AI LLM Provider** (`oxify-connect-llm`) — `VertexAiProvider` implementing `LlmProvider` + `StreamingLlmProvider` + `EmbeddingProvider`; Google publishers path (`projects/{}/locations/{}/publishers/google/models/{}:generateContent`); `Authorization: Bearer {token}` header auth; SSE streaming with `alt=sse`; `embedContent` endpoint for embeddings; `from_env()` reads `GOOGLE_VERTEX_ACCESS_TOKEN` / `GOOGLE_VERTEX_PROJECT` / `GOOGLE_VERTEX_LOCATION`; `with_base_url()` seam for testing.
- **OneSignal Push Provider** (`oxify-connect-comm` + `onesignal` feature) — `OneSignalProvider` implementing `MessageProvider`; `Recipient::User` → `include_player_ids`, `Recipient::Channel` → `included_segments`; `Authorization: Basic {key}` raw header (not base64); JSON body; `errors[]`-on-200 detection → `CommError::Provider`; `list_channels` → `Unsupported`; `from_env()` reads `ONESIGNAL_APP_ID`/`ONESIGNAL_REST_API_KEY`.

### 📊 Updated Statistics
- **Crates**: 19 workspace crates (up from 18 — new `oxify-connect-db`)
- **Tests (connect-db, all-backends)**: 33 unit + 1 doc-test passing; 5 ignored (live DB required)
- **Tests (connect-llm, default)**: 237 unit + 29 doc-tests passing (includes 8 new VertexAi tests)
- **Tests (connect-comm, onesignal feature)**: 20 unit + 2 doc-tests passing (12 new OneSignal tests)
- **Zero Warnings**: All code compiles cleanly across all feature combinations

**Last Updated:** 2026-05-31
**Document Version:** 2.5

---

## v0.2.6 Additions (2026-05-31)

### ✅ New Features

- **AWS Signature V4 module** (both `oxify-connect-llm` and `oxify-connect-vision`) — standalone `aws_sigv4` module per crate; HMAC-SHA256 4-step key derivation; `sign_request` returns `Authorization`/`x-amz-date`/`x-amz-security-token` headers; `AwsCredentials { access_key_id, secret_access_key, session_token }` with `from_env()`; `hmac = "0.12"`, `sha2 = "0.10"`, `hex = "0.4"` added to workspace deps.
- **AWS Bedrock upgrade** (`oxify-connect-llm`) — `BedrockProvider` now uses real SigV4 signing (removed the `tracing::warn!` stub); credentials read from struct or env fallback.
- **AWS SageMaker LLM Provider** (`oxify-connect-llm`) — `SageMakerProvider` implementing `LlmProvider`; TGI/Llama format (`{"inputs": ..., "parameters": {...}}`); SigV4-signed invocations endpoint; builder pattern (`with_credentials`, `with_model_hint`, `with_base_url`); `from_env()` reads `AWS_SAGEMAKER_REGION`/`AWS_SAGEMAKER_ENDPOINT`; `AwsCredentials` exported from crate root; 8 wiremock tests.
- **AWS Textract OCR Provider** (`oxify-connect-vision`, feature `aws-textract`) — `TextractProvider` implementing `VisionProvider`; `DetectDocumentText` API (LINE-type block extraction); `AnalyzeDocument` with FORMS/TABLES features; SigV4-signed POST to `textract.{region}.amazonaws.com`; base64-encoded image bytes; `with_base_url()` seam; `from_env()` reads `AWS_TEXTRACT_REGION`/`AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`; 13 tests.
- **Firebase FCM Push Provider** (`oxify-connect-comm`, feature `firebase`) — `FirebaseFcmProvider` implementing `MessageProvider`; RS256 JWT signing via `jsonwebtoken`; Google OAuth2 token exchange (`urn:ietf:params:oauth:grant-type:jwt-bearer`); access token caching with `tokio::sync::RwLock<Option<CachedToken>>` and 5-minute refresh guard; FCM v1 API (`projects/{id}/messages:send`); `Recipient::User`→device token, `Recipient::Channel`→topic; `from_env()` reads `FIREBASE_PROJECT_ID`/`FIREBASE_SERVICE_ACCOUNT_JSON`; `jsonwebtoken = "9.3"` added to workspace deps; 12 tests.
- **OpenTelemetry async span wiring** (`oxify-engine`, `otel` feature) — `#[tracing::instrument]` added to `execute_with_config`, `execute_node_with_retry`, `execute_node` with semantic span names (`oxify.workflow.execute`, `oxify.node.execute`, `oxify.node.step`) and structured fields (`workflow.id`, `workflow.name`, `node.id`); `init_tracing` now installs `tracing-opentelemetry` bridge layer via `tracing-subscriber` registry; `build_otel_provider` for composable setup; `trace_workflow_async`/`trace_node_async` async helpers; workflow completion duration logged via `tracing::info!`.

### 📊 Updated Statistics
- **Crates**: 19 workspace crates (unchanged)
- **Tests (connect-llm, default)**: 289 passing (+ SageMaker + Bedrock + SigV4 tests)
- **Tests (connect-vision, aws-textract feature)**: 354 passing (13 new Textract + 11 SigV4 tests)
- **Tests (connect-comm, firebase feature)**: 21 new passing (FCM + JWT tests)
- **Tests (oxify-engine, otel feature)**: 318 passing (+9 new async trace tests)
- **Zero Warnings**: All code compiles cleanly across all feature combinations

**Last Updated:** 2026-05-31
**Document Version:** 2.6
