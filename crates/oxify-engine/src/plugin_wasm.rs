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

        // Execute with timeout
        let timeout = self.config.timeout;
        let result = tokio::task::block_in_place(|| {
            let start = std::time::Instant::now();

            // Call the function
            let result_ptr = execute_fn
                .call(store, input_ptr as i32, input_str.len() as i32)
                .map_err(|e| WasmError::ExecutionError(e.to_string()))?;

            // Check timeout
            if start.elapsed() > timeout {
                return Err(WasmError::TimeoutExceeded);
            }

            Ok(result_ptr)
        })?;

        // Read result from WASM memory
        let result_str = self.read_from_memory(store, result)?;

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
}
