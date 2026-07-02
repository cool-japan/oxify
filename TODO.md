# OxiFY - Development TODO

## Known Issues

- [x] ~~`oxify-engine` does not compile under `--features wasm`~~ **RESOLVED 2026-07-02** (`crates/oxify-engine/src/plugin_wasm.rs`).
  - **Was:** `cargo build -p oxify-engine --features wasm` failed with ~20 `E0277` errors — `WasmNodePlugin`'s `std::sync::Mutex<WasmContext>` gave the struct `Send` but not `Sync` (wasmer 7.2's VM internals — raw `*mut c_void`, `NonNull<VMContext>`, a non-`Send` `dyn FnOnce(StoreMut) -> ...` callback — are not thread-safe), and `NodePlugin: Send + Sync` requires it. The v0.2.10 changelog entry below's "`Store: Send+Sync`" assumption did not hold for the resolved wasmer 7.2.
  - **Fix:** thread-affine actor. `WasmNodePlugin::from_wasm_file` now spawns one dedicated `std::thread` that owns the `wasmer::Store`/`WasmPlugin` for its entire lifetime and never lets them cross a thread boundary; `WasmNodePlugin` itself holds only a `std::sync::mpsc::Sender<WasmRequest>` (trivially `Send + Sync`, since the request/reply payloads are plain `Node`/`ExecutionContext`/`Result<ExecutionResult, String>` data). `async fn execute` sends a request and `.await`s a `tokio::sync::oneshot` reply — the future never touches wasmer state, so it's `Send` with **zero `unsafe impl`**. An init handshake preserves the original eager load-error semantics; `Drop` disconnects the channel so the actor thread exits and is joined. The now-redundant `tokio::task::block_in_place` (would panic off a tokio runtime) was removed. The compile-time `test_wasm_node_plugin_is_send_sync` proof passes unmodified. `cargo nextest run -p oxify-engine --features wasm`: 326 passed (0 regressions from the default-feature 326).

- [ ] **`oxify-model`'s `python` feature is incompatible with `cargo test`/`nextest`** (`crates/oxify-model/Cargo.toml:32`) — discovered 2026-07-02 while running full `--all-features` verification; pre-existing, unrelated to any work in this session.
  - **Symptom:** `cargo nextest run --workspace --all-features` (or any `cargo test`/`--no-run` that activates oxify-model's `python` feature) fails to *link* the `oxify-model` test binary with dozens of `undefined reference to 'PyErr_Fetch'` / `PyImport_Import` / `Py_IsInitialized` / etc. errors, even though `cargo build`/`cargo clippy --all-targets` for the same feature set succeed.
  - **Root cause:** `pyo3 = { version = "0.29", features = ["extension-module", "abi3-py38"], ... }` is unconditional under the `python` feature. `extension-module` deliberately tells pyo3 **not** to link `libpython` — that's correct for a `.so` Python will `dlopen()` (Python supplies the symbols at load time), but it is fundamentally incompatible with building a *standalone* test executable, which needs those symbols resolved at link time. This is a documented pyo3 limitation, not an oxify bug introduced by this or any prior session — a real, functional Python (3.11, `Py_ENABLE_SHARED=1`) with `libpython3.11.so` is present on this machine, confirming the failure is the feature configuration, not a missing system dependency.
  - **Impact:** `--all-features` workspace-wide build/clippy are unaffected (verified green); only test-binary linking for the `python` feature specifically fails. Every other oxify-model feature (`openapi`, `wasm`, `typescript`, default) tests cleanly (412/412). Workaround used for this session's verification: `cargo nextest run --workspace --all-features --exclude oxify-model` + `cargo nextest run -p oxify-model --features openapi,wasm,typescript` separately.
  - **Priority:** P3 | **Scope:** small | **Cross-project:** pyo3
  - **Approach:** split the Cargo feature in two, per pyo3's own recommendation — keep `python = ["pyo3"]` for library consumers/maturin builds (as today), and add a test-only feature (e.g. `python-test`) that depends on `pyo3` **without** `extension-module` (so it links against libpython normally), then have `[dev-dependencies]`/CI select `python-test` instead of `python` when running `cargo test`/`nextest` for this crate.
  - **Risk:** low — purely a Cargo feature/test-harness config change, no runtime behavior affected.

## v0.2.11 Additions (2026-07-02)

Closed out the remaining tracked `TODO.md` backlog (the "Stubs to implement" list below, now resolved) plus the wasm Send/Sync fix above. All five landed independently (disjoint files) and were verified together: `cargo build`/`clippy --all-features -D warnings` clean workspace-wide; full `nextest --all-features` green (2702 passed, 38 skipped — all pre-existing `#[ignore]`d external-resource tests — excluding the pre-existing/unrelated `oxify-model` `python`-feature link issue documented above).

1. **Checkpoint pause/resume, fully wired end-to-end** (`oxify-api`) — the `checkpoint_handlers`/`checkpoint_types` module (and every route/OpenAPI registration) was fully re-enabled after being disabled; discovered along the way that `oxify-storage::checkpoint_store` was *also* disabled and still on the retired `sqlx` API, so it was ported to `oxisql` too (necessary, not optional, to make the API compile). `resume_execution` bridges `oxify_storage::ExecutionCheckpoint` → `oxify_engine::checkpoint::ExecutionCheckpoint` (`created_at: DateTime<Utc>` → `SystemTime` via `.into()`) and calls the engine's `execute_from_checkpoint` (widened from `pub(super)` to `pub`), which already skipped completed levels/nodes — mirrors `execute_workflow`'s `tokio::spawn` + `execution_store.update` + metrics pattern, returns `202 Accepted`. New test proves already-completed nodes are not re-executed on resume (shared-counter check). `AppState` gained `checkpoint_store: Option<Arc<DatabaseCheckpointStore>>`. +4 tests in oxify-api (110 total).
2. **`web_scrape` real CSS selector support + `web_screenshot` implemented** (`oxify-mcp`) — CSS selectors now parsed via `scraper` 0.27 (pure Rust, unconditional dependency); malformed selectors return a clean error instead of passing through raw HTML. `web_screenshot` implemented for real behind a new off-by-default `headless-browser` feature using `chromiumoxide` 0.9 (async CDP client) — launches/connects Chrome, navigates, captures a PNG, returns it base64-encoded; every failure mode (no Chrome binary, navigation, capture) is a clean `McpError`, never a panic; browser is torn down on both success and error paths. Without the feature, the original honest "not implemented" error is unchanged. +4 tests (89 total default, 88+1 `#[ignore]`d with the feature).
3. **WebSocket binary MessagePack + per-workflow broadcast scoping** (`oxify-server`) — `Message::Binary` now decodes via `rmp-serde` 1.3.1 into the same `WsMessage` enum as the JSON/text path (shared dispatch helper, zero behavior change for text). New `Subscribe`/`Unsubscribe { workflow_id }` message variants let clients declare interest; `WsConnectionManager` gained a subscription map + `broadcast_to_workflow`, and `unregister` now purges stale subscriptions on disconnect. `WorkflowEdit` broadcasts are now scoped to subscribers instead of fanning out to every connection. +5 tests (170 total).
4. **`quota_store` + `redis_cache` re-enabled on oxisql** (`oxify-storage`) — ported the 1059-line disabled `quota_store.rs` (user/workflow execution quotas, token/cost limits, hourly/daily/monthly resets — 24 `sqlx` sites) to `oxisql`, reusing the already-existing `20251201000004__quotas.sql` migration (no schema changes needed). Two Limbo-specific landmines handled at every touch point: no `FromValue for Uuid` (read as `String`, parse explicitly) and bare `datetime('now')` not being RFC3339 (every timestamp now written via `.to_rfc3339()` and parsed back explicitly). Also hit the same Limbo bound-`LIMIT` panic documented in the sqlx→oxisql migration — worked around the same way (inline the trusted integer literal). `redis_cache.rs` unblocked once `quota_store` compiled again (plus two stale test literals updated to the current all-`String` `WorkflowRow` shape). `cache.rs`'s temporary stub `UserQuota`/`WorkflowQuota` types replaced with the real ones. +9 new DB-level quota tests (153 passed, 2 pre-existing `#[ignore]`d live-Redis tests unchanged).

### New optional dependencies
- `scraper = "0.27"` (`oxify-mcp`, unconditional — pure Rust)
- `chromiumoxide = "0.9"` + `futures` (`oxify-mcp`, optional, `headless-browser` feature, off by default)
- `rmp-serde = "1.3"` (`oxify-server`, unconditional — pure Rust)

## v0.2.10 Additions (2026-06-10)
1. **Real end-to-end SSE** (engine→api→ui) — `Engine::execute_with_context(&self, workflow, ctx, config)` new entrypoint (executor.rs) preserves caller's `execution_id` through the event bus so all `WorkflowEvent`s carry one consistent id; `oxify-api` `AppState` gains `Arc<EventBus>` built via `EngineBuilder::with_event_bus`; `execute_workflow` bug-fixed (was minting 3 different execution ids — create/update/engine — so stored execution was never updated; now one id end-to-end); `sse.rs::stream_execution` rewritten to subscribe to the bus via `broadcast::Receiver::recv` in a `tokio::select!` loop (replaced 500ms polling loop that was reading a store written only at the end); `oxify-ui` `ApiClient::stream_execution` opens `GET /api/v1/executions/{id}/stream` and the `execution_stream` handler proxies upstream events, transforming JSON payloads to OOB-swap HTML `<div id="execution-status" hx-swap-oob="true">` (zero template changes); mock branch preserved for dev mode. +50 tests in oxify-api, +86 in oxify-ui.
2. **Plugin WASM activation** (`oxify-engine`) — `WasmNodePlugin` adapter (`plugin_wasm.rs`) implements `NodePlugin` via `std::sync::Mutex<WasmContext { store, plugin }>` (wasmer 7.1 `Store: Send+Sync` → no `unsafe`; synchronous wasmer call runs inside the lock, no `.await` while locked → `Send` future); `PluginLoader::with_registry(config, Arc<PluginRegistry>)` constructor shares the engine's live registry (previously created an isolated private one and never registered anything executable); `load_plugin` now registers sandboxed WASM plugins into the shared registry (wasm-feature: `WasmNodePlugin::from_wasm_file`; no-wasm: returns a clear error); dead `#[allow(dead_code)] wasm_loader` field removed; `PluginCapabilities::wasm_module: Option<String>` added (backward-compatible `#[serde(default)]`, falls back to `<name>.wasm` convention); `Engine::load_plugins_from(&self, dir) -> Result<Vec<PluginLoadResult>>` discovers + registers manifests end-to-end; `#[tokio::test(flavor = "multi_thread")]` WASM E2E test (required because `block_in_place` panics on current-thread runtime). +326 tests in oxify-engine (default), +325 with `--features wasm`.
3. **`oxify-celers` — new crate (21st workspace member)** — bridges OxiFY workflow execution to the CeleRS distributed task queue (`$HOME/work/celers`); path dep `celers = { path = "../celers/crates/celers", default-features = false }` (CeleRS is not published); `OxifyWorkflowTask` implements `celers_core::Task` with `Input=WorkflowTaskInput { workflow, variables }` / `Output=ExecutionContext` and builds `Engine::execute` inside the task body; `OxifyCelersClient<B,R>` submits workflows (`serde_json→SerializedTask→enqueue`) and polls results with bounded exponential backoff; **critical fix over stock `celers::Worker`**: custom `run_loop<B,R>` stores results via `ResultStore::store_result` (stock Worker dequeues + executes + acks but never stores results); features: `default=[]` (no celers/redis pulled), `redis`, `worker`, `test-utils`; E2E hermetic test with `MockBroker` + in-process `MockResultStore` + real engine (no live Redis needed). 5 tests (4 default + 1 ignored live-Redis).

## v0.2.9 Additions (2026-06-10)
1. **oxify-engine: lib.rs split** — 3120-line monolith split into `lib.rs` (1598), `executor.rs` (525), `node_executor.rs` (1174); all public APIs preserved via re-exports; 315 tests pass; zero warnings
2. **Event sourcing** (`oxify-engine/src/event_store.rs`, 540 lines) — `EventStore` async trait, `InMemoryEventStore` (AtomicU64 seq), `FileEventStore` (JSONL append + replay), `attach_to_bus()` mirroring nats_bridge pattern; `EventRecorder` shutdown handle; 6 tests using `temp_dir()`; `check_triggers` stub replaced with real "workflow.trigger.fired" pub/sub implementation
3. **Plugin system wired** (`NodeKind::Custom(CustomConfig)`) — `CustomConfig { plugin_id, plugin_version, config }` added to oxify-model; 25 files across 3 crates updated (oxify-model ×13, oxify-engine ×5, oxify-cli ×4 — all explicit arms, no wildcards); `Engine` gets `plugin_registry: Arc<PluginRegistry>`; `EngineBuilder::with_plugin_registry()`; executor dispatch in `node_executor.rs`; purple `#8B5CF6` visualization; 3 integration tests
4. **LRU flaky-test fixes** (`oxify-storage/cache.rs`, `oxify-vector/embeddings.rs`) — both `evict_lru`/`evict_oldest` methods changed from `min_by_key(timestamp)` to `min_by_key(seq)` using monotonic `u64` counters; `true LRU` access-bump in `cache.rs`; `Arc<AtomicU64>` in `embeddings.rs` (preserves `Clone`); deterministic eviction tests + 1000-iteration stress loop confirmed non-flaky over 20 consecutive runs
5. **oxify-ui template gallery + real node validation** — `/templates` page + `/htmx/templates` + `/htmx/templates/{id}` endpoints; 5 built-in templates (LLM Pipeline, RAG Pipeline, Data Extraction, Webhook Triggered, Vision OCR) with parameter tables; `node_validate` htmx handler replaced with real `validate_node_config()` (covers 14 node types, temperature bounds, loop iteration limits, XSS-safe HTML error output); `src/validation.rs` extracted; 61 tests (28 unit + 7 handler + 3 mock + 18 integration)

## v0.2.8 Additions (2026-06-10)
1. **GitHub API extended tools** (`oxify-mcp`, feature `github`) — 7 new tools added to `GitHubServer` (total now 15): `get_issue`, `comment_on_issue`, `close_issue`, `get_pr`, `merge_pr`, `list_commits`, `get_commit`; octocrab 0.53 API; 13 new tests (106 total for crate); file: `servers/github.rs` now 1308 lines
2. **Docker/k8s/Helm/CI deployment** — 20 production-quality artifacts: `Dockerfile` (multi-stage, port 3000, non-root user, static file copy), `docker-compose.yml` (oxify-ui + postgres + redis + qdrant), `k8s/` (namespace, configmap, secret, deployment, service, ingress, hpa, pvc), `helm/` (Chart.yaml, values.yaml, `_helpers.tpl`, deployment/service/ingress/hpa/configmap/secret templates), `.github/workflows/ci.yml` (test + feature-tests + security audit + docker build/push jobs)
3. **oxify-ui completion** — Error pages (404, 500 templates + Rust handlers + `.fallback()`), dark mode (Tailwind dark: variants across all templates + localStorage JS toggle + FOUC prevention + `aria-pressed`), HTMX infinite scroll pagination for workflow list (intersection observer sentinel), workflow import (multipart JSON/YAML) + export (JSON/YAML download), SVG workflow preview generator (`src/svg.rs` — Kahn's algorithm topological layout + cubic Bezier edges, 365 lines, 5 tests), PWA manifest.json, `/health`+`/readyz`+`/livez` k8s probe endpoints, accessibility (ARIA roles, live regions, `aria-busy` HTMX hooks)

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
  - [x] Workflow templates gallery ✅ NEW (5 built-in templates, parameter detail view, "Use Template" button prefills new-workflow form)
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
- [x] Dark mode support (Tailwind dark: classes + localStorage) ✅ NEW (all templates updated with dark: variants; JS toggle in base.html with FOUC prevention; `aria-pressed` state)
- [ ] Responsive design (Tailwind breakpoints)
- [x] Accessibility (semantic HTML, ARIA attributes) ✅ NEW (`role="main"`, `role="navigation"`, `aria-live="polite"`, `aria-busy` on HTMX regions, `aria-label` on icon-only buttons, `aria-current="page"`, `aria-hidden` on decorative SVGs)
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
  - [x] GitHub API (issues, PRs, commits) ✅ NEW (`oxify-mcp` GitHubServer extended to 15 tools — `get_issue`, `comment_on_issue`, `close_issue`, `get_pr`, `merge_pr`, `list_commits`, `get_commit`; octocrab 0.53 API)
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
  - [x] Custom node type registration ✅ NEW (`NodeKind::Custom(CustomConfig)`, `PluginRegistry`, `Engine::with_plugin_registry()`, executor dispatch)
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

- [x] Create Docker images ✅ NEW (multi-stage Dockerfile, port 3000, non-root `oxify` user)
- [x] Create Kubernetes manifests ✅ NEW (`k8s/` — namespace, configmap, secret, deployment with HPA, service, ingress, pvc)
- [x] Add Helm charts ✅ NEW (`helm/` — Chart.yaml, values.yaml, full template suite with `_helpers.tpl`)
- [x] Set up CI/CD pipeline ✅ NEW (`.github/workflows/ci.yml` — test, feature-tests, security audit, multi-arch docker build/push to ghcr.io)
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

---

## Pure Rust Migration (COOLJAPAN Policy)

- [x] **(LOW priority — already pure-Rust, consistency-only) Replace `flate2` with `oxiarc-deflate`.** _(2026-06-05: both direct `flate2` declarations + all 3 call sites migrated to `oxiarc-deflate` 0.3.2 — see note below.)_
  - **DONE (direct deps).** `oxiarc-deflate = "0.3.2"` added to `[workspace.dependencies]`. authn keeps the `optional` + `saml`-gated shape (`oxiarc-deflate = { workspace = true, optional = true }`, `saml = ["quick-xml", "oxiarc-deflate", "url"]`); cli uses `oxiarc-deflate = { workspace = true }`. SAML site (`saml.rs::encode_for_redirect`) now uses `Deflater::new(6).deflate(.., finish=true)` → **raw RFC 1951 DEFLATE** (no gzip/zlib wrapper, correct for the HTTP-Redirect binding). CLI site (`workflow.rs` package archive) now uses `streaming::GzipStreamEncoder::new(file, 6)` wrapped by `tar::Builder`, finalized via `tar.into_inner()` → `enc.finish()` (writes the gzip trailer). Level 6 preserves flate2's previous `Compression::default()`. Added round-trip tests: SAML deflate→base64→raw-inflate→original (asserts payload is NOT gzip), and CLI gzip+tar archive→gzip-magic+untar→original. `cargo build / nextest (83 passed) / clippy --all-targets -D warnings`, all green with `--features saml`.
  - **NOTE — residual transitive `flate2` (out of scope):** `cargo tree -i flate2` shows the only remaining `flate2` is pulled in transitively by `tower-http` → `async-compression` → `compression-codecs` (HTTP compression layer), not by any direct oxify dependency. So `cargo tree | grep flate2` is non-empty, but no oxify crate declares or uses `flate2` directly anymore. Removing the transitive copy would require replacing/reconfiguring `tower-http`, which is a separate effort.
  - **Not urgent, not a Pure-Rust-purity violation.** `flate2` 1.1.x as resolved here is already pure-Rust (miniz_oxide + zlib-rs backend; no C in the lock file). This item exists solely for OxiARC ecosystem consistency, so it sits at the lowest priority — do not treat it as an urgent C-dependency removal.
  - Two declarations and two call sites:
    - (a) `crates/oxify-authn/Cargo.toml` (~line 52): `flate2 = { version = "1.1", optional = true }`, gated behind the `saml` feature (~line 75: `saml = ["quick-xml", "flate2", "url"]`). `saml` is **not** in the default feature set (`default = ["jwt", "oauth", "password"]`), so this dependency is off by default. One call site: `crates/oxify-authn/src/saml.rs:44` — `use flate2::{write::DeflateEncoder, Compression};` (SAML HTTP-Redirect binding, **raw DEFLATE** with no gzip header).
    - (b) `crates/oxify-cli/Cargo.toml` (~line 34): `flate2 = "1.1"` — **unconditional** (not optional). One call site: `crates/oxify-cli/src/commands/workflow.rs:859-860` — `flate2::write::GzEncoder` + `flate2::Compression` (workflow archive export, **gzip**, paired with `tar::Builder`).
  - Replacement: `oxiarc-deflate` — use its raw-DEFLATE encoder for the authn `DeflateEncoder` (raw stream, no gzip wrapper) and its gzip encoder for the CLI `GzEncoder`. Roughly 3 call sites total; the change is mechanical (swap the encoder type + `Write` plumbing, keep the same compression semantics).
  - **Acceptance:** `oxify-authn` (built with the `saml` feature) and `oxify-cli` compile and their test suites pass; the SAML redirect-binding round-trip (deflate-encode → base64 → decode) and the CLI gzip archive output remain byte-valid and consumable by standard tools; `cargo tree | grep flate2` returns empty across the workspace.

## Stubs to implement (added 2026-06-22 by /cooljapan-stub-check) — ALL RESOLVED 2026-07-02

See "v0.2.11 Additions" near the top of this file for full implementation detail. Superseded the
duplicate 2026-06-12 dated copy of this same list (removed).

- [x] **oxify** `oxify-api`: `crates/oxify-api/src/checkpoint_handlers.rs:162` — `resume_execution` now bridges the storage checkpoint to the engine and calls `execute_from_checkpoint`; already-completed nodes are proven not to re-execute (new test).
- [x] **oxify** `oxify-mcp`: `crates/oxify-mcp/src/servers/web.rs:133` — CSS selectors parsed for real via the `scraper` crate; malformed selectors return a clean `McpError`.
- [x] **oxify** `oxify-mcp`: `crates/oxify-mcp/src/servers/web.rs:154` — headless screenshot implemented via `chromiumoxide`, feature-gated behind off-by-default `headless-browser`.
- [x] **oxify** `oxify-server`: `crates/oxify-server/src/websocket.rs:428` — `Message::Binary` now decodes via `rmp-serde` into `WsMessage`, sharing the same dispatch as the JSON/text path.
- [x] **oxify** `oxify-server`: `crates/oxify-server/src/websocket.rs:359` — `WsConnectionManager` gained `Subscribe`/`Unsubscribe` + a subscription map; `WorkflowEdit` broadcasts are scoped to subscribers, with cleanup on disconnect.
- [x] **oxify** `oxify-storage`: `crates/oxify-storage/src/cache.rs:49` — `quota_store` ported to `oxisql-sqlite-compat` and re-enabled; `redis_cache` (depended on it) re-enabled alongside it.
