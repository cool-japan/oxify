//! WASM Plugin Sandboxing
//!
//! Provides secure WASM runtime for plugin execution with resource limits.
//!
//! # Features
//!
//! - Sandboxed execution environment using wasmer
//! - Memory limits and execution timeouts
//! - Host function bindings for safe API access
//! - Plugin state isolation

use oxify_model::{ExecutionContext, ExecutionResult, Node};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

#[cfg(feature = "wasm")]
use wasmer::{imports, Function, Instance, Memory, MemoryType, Module, Store, TypedFunction};

/// WASM plugin errors
#[derive(Error, Debug)]
pub enum WasmError {
    #[error("Failed to load WASM module: {0}")]
    LoadError(String),

    #[error("Failed to instantiate WASM module: {0}")]
    InstantiationError(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Memory limit exceeded")]
    MemoryLimitExceeded,

    #[error("Timeout exceeded")]
    TimeoutExceeded,

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("WASM feature not enabled")]
    FeatureNotEnabled,
}

/// WASM plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmPluginConfig {
    /// Maximum memory in pages (64KB each)
    pub max_memory_pages: u32,
    /// Execution timeout
    pub timeout: Duration,
    /// Enable fuel metering for execution limits
    pub enable_fuel_metering: bool,
    /// Fuel limit (instructions)
    pub fuel_limit: u64,
}

impl Default for WasmPluginConfig {
    fn default() -> Self {
        Self {
            max_memory_pages: 256, // 16MB
            timeout: Duration::from_secs(30),
            enable_fuel_metering: true,
            fuel_limit: 1_000_000,
        }
    }
}

impl WasmPluginConfig {
    /// Create a strict configuration with tight limits
    pub fn strict() -> Self {
        Self {
            max_memory_pages: 64, // 4MB
            timeout: Duration::from_secs(5),
            enable_fuel_metering: true,
            fuel_limit: 100_000,
        }
    }

    /// Create a permissive configuration with loose limits
    pub fn permissive() -> Self {
        Self {
            max_memory_pages: 1024, // 64MB
            timeout: Duration::from_secs(300),
            enable_fuel_metering: true,
            fuel_limit: 10_000_000,
        }
    }
}

/// WASM plugin loader and executor
#[cfg(feature = "wasm")]
pub struct WasmPluginLoader {
    config: WasmPluginConfig,
    store: Store,
}

#[cfg(feature = "wasm")]
impl WasmPluginLoader {
    /// Create a new WASM plugin loader
    pub fn new(config: WasmPluginConfig) -> Self {
        let store = Store::default();

        // Note: Fuel metering would be configured with wasmer::Engine
        // For now, we rely on timeout-based limits

        Self { config, store }
    }

    /// Load a WASM plugin from file
    pub fn load_from_file(&mut self, path: &Path) -> Result<WasmPlugin, WasmError> {
        let wasm_bytes = std::fs::read(path).map_err(|e| WasmError::LoadError(e.to_string()))?;

        self.load_from_bytes(&wasm_bytes)
    }

    /// Load a WASM plugin from bytes
    pub fn load_from_bytes(&mut self, wasm_bytes: &[u8]) -> Result<WasmPlugin, WasmError> {
        // Compile the WASM module
        let module = Module::new(&self.store, wasm_bytes)
            .map_err(|e| WasmError::LoadError(e.to_string()))?;

        // Create memory with limits
        let memory_type = MemoryType::new(1, Some(self.config.max_memory_pages), false);
        let memory = Memory::new(&mut self.store, memory_type)
            .map_err(|e| WasmError::InstantiationError(e.to_string()))?;

        // Create imports with host functions
        let imports = imports! {
            "env" => {
                "memory" => memory.clone(),
                "log" => Function::new_typed(&mut self.store, wasm_host_log),
            },
        };

        // Instantiate the module
        let instance = Instance::new(&mut self.store, &module, &imports)
            .map_err(|e| WasmError::InstantiationError(e.to_string()))?;

        Ok(WasmPlugin {
            instance,
            memory,
            config: self.config.clone(),
        })
    }
}

/// Host function: log from WASM
#[cfg(feature = "wasm")]
fn wasm_host_log(ptr: i32, len: i32) {
    tracing::debug!("WASM log: ptr={}, len={}", ptr, len);
}

/// Loaded WASM plugin instance
#[cfg(feature = "wasm")]
pub struct WasmPlugin {
    instance: Instance,
    memory: Memory,
    config: WasmPluginConfig,
}

#[cfg(feature = "wasm")]
impl WasmPlugin {
    /// Execute the plugin's main function
    pub fn execute(
        &mut self,
        store: &mut Store,
        node: &Node,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, WasmError> {
        // Serialize input
        let input = serde_json::json!({
            "node": node,
            "context": context,
        });

        let input_str = serde_json::to_string(&input)
            .map_err(|e| WasmError::InvalidParameter(e.to_string()))?;

        // Get the execute function
        let execute_fn: TypedFunction<(i32, i32), i32> = self
            .instance
            .exports
            .get_typed_function(store, "execute")
            .map_err(|_| WasmError::FunctionNotFound("execute".to_string()))?;

        // Write input to WASM memory
        let input_ptr = self.write_to_memory(store, input_str.as_bytes())?;

        // Execute with timeout.
        //
        // This runs on the plugin's dedicated actor OS thread (see
        // `WasmNodePlugin`), which is *not* a tokio worker, so the wasmer call is
        // plain synchronous code. The former `tokio::task::block_in_place` wrapper
        // was removed: it panics outside a multi-thread tokio runtime, and there is
        // no runtime at all on the actor thread. Thread affinity — one OS thread per
        // plugin — already prevents any tokio worker from being starved by a
        // long-running wasmer call, which is what `block_in_place` used to guard.
        let timeout = self.config.timeout;
        let start = std::time::Instant::now();

        // Call the function
        let result_ptr = execute_fn
            .call(store, input_ptr as i32, input_str.len() as i32)
            .map_err(|e| WasmError::ExecutionError(e.to_string()))?;

        // Check timeout
        if start.elapsed() > timeout {
            return Err(WasmError::TimeoutExceeded);
        }

        // Read result from WASM memory
        let result_str = self.read_from_memory(store, result_ptr)?;

        // Deserialize result
        let execution_result: ExecutionResult = serde_json::from_str(&result_str)
            .map_err(|e| WasmError::ExecutionError(e.to_string()))?;

        Ok(execution_result)
    }

    /// Write data to WASM memory
    fn write_to_memory(&mut self, store: &mut Store, data: &[u8]) -> Result<usize, WasmError> {
        // Allocate memory (simplified - in production, use proper allocator)
        let ptr = 1024usize; // Fixed offset for simplicity

        // Write data
        let memory_view = self.memory.view(store);
        for (i, byte) in data.iter().enumerate() {
            memory_view
                .write_u8((ptr + i) as u64, *byte)
                .map_err(|_| WasmError::ExecutionError("Failed to write to memory".to_string()))?;
        }

        Ok(ptr)
    }

    /// Read data from WASM memory
    fn read_from_memory(&self, store: &Store, ptr: i32) -> Result<String, WasmError> {
        let memory_view = self.memory.view(store);
        let ptr = ptr as u64;

        // Read length (first 4 bytes)
        let mut len_bytes = [0u8; 4];
        for (i, byte) in len_bytes.iter_mut().enumerate() {
            *byte = memory_view
                .read_u8(ptr + (i as u64))
                .map_err(|_| WasmError::ExecutionError("Failed to read length".to_string()))?;
        }
        let len = u32::from_le_bytes(len_bytes) as usize;

        // Read data
        let mut data = vec![0u8; len];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = memory_view
                .read_u8(ptr + 4 + (i as u64))
                .map_err(|_| WasmError::ExecutionError("Failed to read data".to_string()))?;
        }

        String::from_utf8(data).map_err(|e| WasmError::ExecutionError(e.to_string()))
    }

    /// Get exported functions
    pub fn get_exports(&self, _store: &Store) -> Vec<String> {
        self.instance
            .exports
            .iter()
            .filter_map(|(name, _)| {
                if !name.starts_with("__") {
                    Some(name.to_string())
                } else {
                    None
                }
            })
            .collect()
    }
}

// ── WasmNodePlugin adapter (thread-affine actor) ─────────────────────────────
//
// wasmer's `Store` — and everything reachable through it: `Instance`, `Memory`,
// and the VM's raw `NonNull<…>`/`*mut …` pointers — is neither `Send` nor `Sync`
// in wasmer 7.x, because it carries thread-unsafe VM state. The engine, however,
// requires every `NodePlugin` to be `Send + Sync`: the `PluginRegistry` is shared
// across tokio worker threads inside an `Arc`, and `WorkflowScheduler::start`
// `tokio::spawn`s work that transitively captures it.
//
// We reconcile the two with a *thread-affine actor*. Each `WasmNodePlugin` owns
// exactly one dedicated OS thread that constructs and then forever holds the
// wasmer `Store`/`WasmPlugin`. The wasmer state is born on that thread and never
// leaves it, so no `!Send` value ever crosses a thread boundary — meaning no
// `unsafe`, and specifically no `unsafe impl Send`/`Sync` (which would be an
// actual soundness hole, since those pointers really are thread-unsafe). Callers
// reach the actor only through channels that carry plain `Send` data — `Node`,
// `ExecutionContext`, `Result<ExecutionResult, String>`, and a `oneshot` reply
// handle — so `WasmNodePlugin` itself is trivially `Send + Sync`.

/// A unit of work handed to a [`WasmNodePlugin`]'s actor thread.
///
/// Every field is `Send` and touches no wasmer type, so the whole request — and
/// hence `std::sync::mpsc::Sender<WasmRequest>` — is freely `Send + Sync`.
#[cfg(feature = "wasm")]
struct WasmRequest {
    node: Node,
    context: ExecutionContext,
    reply_tx: tokio::sync::oneshot::Sender<Result<ExecutionResult, String>>,
}

/// A loaded WASM plugin exposed as a live [`crate::plugin::NodePlugin`].
///
/// All wasmer calls happen on a single dedicated OS thread (the "actor"); this
/// handle holds only the `Send + Sync` channel used to talk to it plus the
/// actor's join handle. See the module-level note above for why the actor is
/// required and why the design is sound without any `unsafe`.
#[cfg(feature = "wasm")]
pub struct WasmNodePlugin {
    name: String,
    version: String,
    node_types: Vec<String>,
    /// Outbound channel to the actor thread. `std::sync::mpsc::Sender<T>` is
    /// `Send + Sync` whenever `T: Send`, which `WasmRequest` is.
    request_tx: std::sync::mpsc::Sender<WasmRequest>,
    /// Join handle for the actor thread; taken and joined on drop.
    handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "wasm")]
impl WasmNodePlugin {
    /// Load a WASM file and spawn its dedicated actor thread.
    ///
    /// `name` MUST equal the manifest's `plugin.name` (and thus
    /// `CustomConfig.plugin_id`) so the engine registry can dispatch correctly.
    ///
    /// The wasmer `Store`/`WasmPlugin` are **constructed on the actor thread**,
    /// not here. `wasmer::Store` is `!Send`, so it can neither be captured by a
    /// `std::thread::spawn` closure (which demands `F: Send`) nor otherwise moved
    /// across a thread boundary without `unsafe`. Building it thread-locally
    /// sidesteps that entirely: only `Send` data (an owned path and the config)
    /// crosses into the thread. We then block until the actor reports the outcome
    /// of the load, preserving the eager load-error semantics callers rely on.
    pub fn from_wasm_file(
        config: WasmPluginConfig,
        wasm_path: &Path,
        name: String,
        version: String,
        node_types: Vec<String>,
    ) -> Result<Self, WasmError> {
        let (request_tx, request_rx) = std::sync::mpsc::channel::<WasmRequest>();
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<(), WasmError>>();

        // Only `Send` data crosses into the actor thread: an owned path + config.
        let wasm_path_owned = wasm_path.to_path_buf();
        let thread_name = format!("oxify-wasm-plugin-{name}");
        let handle = std::thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                // Construct the (non-`Send`) wasmer state here, on its owning
                // thread — the one place it will ever legally live.
                let mut loader = WasmPluginLoader::new(config);
                let mut plugin = match loader.load_from_file(&wasm_path_owned) {
                    Ok(plugin) => plugin,
                    Err(e) => {
                        // Surface the load failure to the constructor, then exit.
                        let _ = init_tx.send(Err(e));
                        return;
                    }
                };
                let WasmPluginLoader { mut store, .. } = loader;

                // Announce the successful load, then release the init channel.
                if init_tx.send(Ok(())).is_err() {
                    // The constructor stopped waiting; tear down immediately.
                    return;
                }
                drop(init_tx);

                // Actor loop: serve requests until every `Sender` is dropped. Each
                // request runs the *exact* synchronous wasmer path from
                // `WasmPlugin::execute`; `store`/`plugin` never leave this thread.
                while let Ok(request) = request_rx.recv() {
                    let WasmRequest {
                        node,
                        context,
                        reply_tx,
                    } = request;
                    let result = plugin
                        .execute(&mut store, &node, &context)
                        .map_err(|e| e.to_string());
                    // A dropped receiver just means the caller stopped awaiting.
                    let _ = reply_tx.send(result);
                }
                // Channel disconnected: `store`/`plugin` drop here, on their owning
                // thread — the only place it is sound to drop them.
            })
            .map_err(|e| {
                WasmError::InstantiationError(format!("failed to spawn WASM actor thread: {e}"))
            })?;

        // Block until the actor finishes loading so load errors surface
        // synchronously, exactly as the previous eager implementation did.
        match init_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                name,
                version,
                node_types,
                request_tx,
                handle: Some(handle),
            }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                let _ = handle.join();
                Err(WasmError::InstantiationError(
                    "WASM actor thread terminated during initialization".to_string(),
                ))
            }
        }
    }
}

#[cfg(feature = "wasm")]
impl Drop for WasmNodePlugin {
    fn drop(&mut self) {
        // Disconnect the request channel *now* by swapping our live sender for a
        // fresh, already-disconnected one and dropping the real sender. That makes
        // the actor's `recv()` return `Err`, so its loop exits and the wasmer state
        // is dropped on its owning thread.
        //
        // `execute(&self, …)` borrows `self` through the owning `Arc` for the whole
        // life of its future, so the plugin cannot be dropped while a request is in
        // flight; the actor is therefore always parked in `recv()` at drop time and
        // the join below returns essentially immediately — never blocking on an
        // in-progress wasm call.
        let (disconnected_tx, disconnected_rx) = std::sync::mpsc::channel::<WasmRequest>();
        drop(disconnected_rx);
        let live_tx = std::mem::replace(&mut self.request_tx, disconnected_tx);
        drop(live_tx);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(feature = "wasm")]
#[async_trait::async_trait]
impl crate::plugin::NodePlugin for WasmNodePlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn supported_node_types(&self) -> Vec<String> {
        self.node_types.clone()
    }

    fn validate(&self, _node: &oxify_model::Node) -> Result<(), String> {
        Ok(())
    }

    fn metadata(&self) -> crate::plugin::PluginMetadata {
        crate::plugin::PluginMetadata {
            name: self.name.clone(),
            version: self.version.clone(),
            description: Some("WASM sandboxed plugin".to_string()),
            author: None,
            homepage: None,
        }
    }

    async fn execute(
        &self,
        node: &oxify_model::Node,
        context: &oxify_model::ExecutionContext,
    ) -> Result<oxify_model::ExecutionResult, String> {
        // Hand the work to the actor thread and await its reply. Both the request
        // payload and the `oneshot::Receiver` we await carry only `Send` data, so
        // the generated future is `Send` — exactly what the `#[async_trait]` bound
        // and the scheduler's `tokio::spawn` require.
        //
        // `std::sync::mpsc::Sender::send` on an unbounded channel is synchronous and
        // non-blocking, so there is no `.await` before the reply and no need to
        // offload the send. Both channel-failure paths (actor gone / reply dropped)
        // become a recoverable `Err(String)` rather than a panic.
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.request_tx
            .send(WasmRequest {
                node: node.clone(),
                context: context.clone(),
                reply_tx,
            })
            .map_err(|_| {
                "WASM plugin actor thread is not running (request channel closed)".to_string()
            })?;
        reply_rx.await.map_err(|_| {
            "WASM plugin actor thread dropped the reply channel without responding".to_string()
        })?
    }
}

/// WASM plugin statistics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WasmPluginStats {
    /// Number of executions
    pub executions: u64,
    /// Total execution time
    pub total_execution_time: Duration,
    /// Average execution time
    pub avg_execution_time: Duration,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Fuel consumed (if metering enabled)
    pub fuel_consumed: u64,
}

// Stub implementations when WASM feature is disabled
#[cfg(not(feature = "wasm"))]
#[allow(dead_code)]
pub struct WasmPluginLoader {
    config: WasmPluginConfig,
}

#[cfg(not(feature = "wasm"))]
impl WasmPluginLoader {
    #[allow(dead_code)]
    pub fn new(config: WasmPluginConfig) -> Self {
        Self { config }
    }

    #[allow(dead_code)]
    pub fn load_from_file(&mut self, _path: &Path) -> Result<WasmPlugin, WasmError> {
        Err(WasmError::FeatureNotEnabled)
    }

    #[allow(dead_code)]
    pub fn load_from_bytes(&mut self, _wasm_bytes: &[u8]) -> Result<WasmPlugin, WasmError> {
        Err(WasmError::FeatureNotEnabled)
    }
}

#[cfg(not(feature = "wasm"))]
#[allow(dead_code)]
pub struct WasmPlugin;

#[cfg(not(feature = "wasm"))]
impl WasmPlugin {
    #[allow(dead_code)]
    pub fn execute(
        &mut self,
        _node: &Node,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, WasmError> {
        Err(WasmError::FeatureNotEnabled)
    }

    #[allow(dead_code)]
    pub fn get_exports(&self) -> Vec<String> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_plugin_config_default() {
        let config = WasmPluginConfig::default();
        assert_eq!(config.max_memory_pages, 256);
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.enable_fuel_metering);
    }

    #[test]
    fn test_wasm_plugin_config_strict() {
        let config = WasmPluginConfig::strict();
        assert_eq!(config.max_memory_pages, 64);
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.fuel_limit, 100_000);
    }

    #[test]
    fn test_wasm_plugin_config_permissive() {
        let config = WasmPluginConfig::permissive();
        assert_eq!(config.max_memory_pages, 1024);
        assert_eq!(config.timeout, Duration::from_secs(300));
        assert_eq!(config.fuel_limit, 10_000_000);
    }

    #[test]
    fn test_wasm_plugin_loader_creation() {
        let config = WasmPluginConfig::default();
        let _loader = WasmPluginLoader::new(config);
    }

    #[test]
    fn test_wasm_plugin_stats_default() {
        let stats = WasmPluginStats::default();
        assert_eq!(stats.executions, 0);
        assert_eq!(stats.total_execution_time, Duration::from_secs(0));
    }

    #[cfg(not(feature = "wasm"))]
    #[test]
    fn test_wasm_plugin_loader_without_feature() {
        let config = WasmPluginConfig::default();
        let mut loader = WasmPluginLoader::new(config);

        let result = loader.load_from_bytes(&[]);
        assert!(matches!(result, Err(WasmError::FeatureNotEnabled)));
    }

    // Compile-time proof that WasmNodePlugin is Send + Sync.
    // The `#[test]` attribute ensures the function is called so there is no
    // dead_code lint while also serving as a compile-time trait-bound check:
    // the compiler errors if WasmNodePlugin is not Send+Sync.
    #[cfg(feature = "wasm")]
    #[test]
    fn test_wasm_node_plugin_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<super::WasmNodePlugin>();
    }

    // End-to-end proof that the actor thread survives across calls and that the
    // wasmer `Store`'s instance state persists between them. A tiny hand-written
    // WAT module (compiled on load via wasmer's `wat` feature) keeps a mutable
    // global counter and, on each `execute`, increments it and writes a
    // length-prefixed JSON string `{"Success":N}` (N = counter as one ASCII digit)
    // to a fixed result offset — matching the host's read/write memory layout.
    // Calling `execute` twice on the *same* plugin must observe N == 1 then N == 2,
    // which can only happen if the one dedicated thread — and its single `Store` —
    // is reused across both calls.
    #[cfg(feature = "wasm")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_wasm_node_plugin_execute_twice_persists_state() {
        use crate::plugin::NodePlugin;
        use oxify_model::{ExecutionResult, Node, NodeKind, WorkflowId};

        // Result region: 4-byte little-endian length (13) at offset 4096, followed
        // by the 13-byte body `{"Success":0}` at offset 4100 (digit at 4111). The
        // host writes its input at offset 1024, comfortably clear of this region.
        const WAT: &str = r#"
            (module
              (import "env" "memory" (memory 1))
              (import "env" "log" (func $log (param i32 i32)))
              (global $counter (mut i32) (i32.const 0))
              (data (i32.const 4096) "\0d\00\00\00")
              (data (i32.const 4100) "{\22Success\22:0}")
              (func (export "execute") (param $ptr i32) (param $len i32) (result i32)
                (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
                (i32.store8 (i32.const 4111)
                  (i32.add (i32.const 48) (global.get $counter)))
                (i32.const 4096)))
        "#;

        // Temp file per the workspace test policy (std::env::temp_dir()).
        let path =
            std::env::temp_dir().join(format!("oxify_wasm_actor_{}.wat", uuid::Uuid::new_v4()));
        std::fs::write(&path, WAT).expect("write temp WAT module");

        let plugin = WasmNodePlugin::from_wasm_file(
            WasmPluginConfig::default(),
            &path,
            "actor-test".to_string(),
            "0.1.0".to_string(),
            vec!["actor_test".to_string()],
        )
        .expect("load WASM actor plugin");

        let node = Node::new("actor".to_string(), NodeKind::Start);
        let ctx = ExecutionContext::new(WorkflowId::new_v4());

        // First call: counter -> 1 -> {"Success":1}.
        let first = plugin.execute(&node, &ctx).await.expect("first execute");
        assert_eq!(first, ExecutionResult::Success(serde_json::json!(1)));

        // Second call on the SAME plugin: the actor thread and its Store survived,
        // so the global counter -> 2 -> {"Success":2}.
        let second = plugin.execute(&node, &ctx).await.expect("second execute");
        assert_eq!(second, ExecutionResult::Success(serde_json::json!(2)));

        drop(plugin);
        let _ = std::fs::remove_file(&path);
    }
}
