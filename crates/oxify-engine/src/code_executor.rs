//! Code execution engine for Rust scripts and WASM modules

use oxify_model::{ExecutionContext, ScriptConfig};
use rhai::{Dynamic, Engine, EvalAltResult, Scope};
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

#[cfg(feature = "wasm")]
use wasmer::{Instance, Module, Store};

#[derive(Error, Debug)]
pub enum CodeExecutionError {
    #[error("Script error: {0}")]
    ScriptError(String),

    #[cfg(feature = "wasm")]
    #[error("WASM compilation error: {0}")]
    WasmCompilationError(String),

    #[cfg(feature = "wasm")]
    #[error("WASM runtime error: {0}")]
    WasmRuntimeError(String),

    #[error("Timeout exceeded")]
    TimeoutExceeded,

    #[error("Input variable not found: {0}")]
    InputNotFound(String),

    #[error("Unsupported runtime: {0}")]
    UnsupportedRuntime(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type Result<T> = std::result::Result<T, CodeExecutionError>;

/// Code executor with support for Rust scripts (Rhai) and WASM
pub struct CodeExecutor {
    /// Maximum execution time (default: 5 seconds)
    timeout: Duration,

    /// Maximum operations for Rhai scripts (prevents infinite loops)
    max_operations: u64,
}

impl Default for CodeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeExecutor {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            max_operations: 1_000_000,
        }
    }

    #[allow(dead_code)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[allow(dead_code)]
    pub fn with_max_operations(mut self, max_operations: u64) -> Self {
        self.max_operations = max_operations;
        self
    }

    /// Execute code based on runtime type
    pub async fn execute(&self, config: &ScriptConfig, ctx: &ExecutionContext) -> Result<Value> {
        match config.runtime.to_lowercase().as_str() {
            "rust" | "rhai" => self.execute_rhai_script(config, ctx).await,
            #[cfg(feature = "wasm")]
            "wasm" | "webassembly" => self.execute_wasm_module(config, ctx).await,
            #[cfg(not(feature = "wasm"))]
            "wasm" | "webassembly" => Err(CodeExecutionError::UnsupportedRuntime(
                "WASM support not enabled. Enable the 'wasm' feature.".to_string(),
            )),
            _ => Err(CodeExecutionError::UnsupportedRuntime(
                config.runtime.clone(),
            )),
        }
    }

    /// Execute Rust script using Rhai interpreter
    async fn execute_rhai_script(
        &self,
        config: &ScriptConfig,
        ctx: &ExecutionContext,
    ) -> Result<Value> {
        // Create Rhai engine with safety limits
        let mut engine = Engine::new();

        // Set operation limits to prevent infinite loops
        engine.set_max_operations(self.max_operations);

        // Set safety limits to prevent abuse
        engine.set_max_expr_depths(32, 32); // Prevent stack overflow
        engine.set_max_string_size(1_000_000); // 1MB string limit
        engine.set_max_array_size(10_000); // Max array size

        // Create scope and bind input variables
        let mut scope = Scope::new();

        // Bind input variables from context
        for input_name in &config.inputs {
            let value = ctx
                .variables
                .get(input_name)
                .ok_or_else(|| CodeExecutionError::InputNotFound(input_name.clone()))?;

            let rhai_value = self.json_to_rhai(value)?;
            scope.push(input_name.clone(), rhai_value);
        }

        // Also bind node results as variables
        for (node_id, node_result) in &ctx.node_results {
            if let oxify_model::ExecutionResult::Success(output) = &node_result.result {
                let var_name = format!("node_{}", node_id.to_string().replace('-', "_"));
                let rhai_value = self.json_to_rhai(output)?;
                scope.push(var_name, rhai_value);
            }
        }

        // Execute script with timeout
        let script = config.code.clone();
        let timeout = self.timeout;

        let result = tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || {
                engine.eval_with_scope::<Dynamic>(&mut scope, &script)
            }),
        )
        .await
        .map_err(|_| CodeExecutionError::TimeoutExceeded)?
        .map_err(|e| CodeExecutionError::ScriptError(format!("Task join error: {}", e)))?
        .map_err(|e: Box<EvalAltResult>| CodeExecutionError::ScriptError(e.to_string()))?;

        // Convert Rhai result back to JSON
        self.rhai_to_json(&result)
    }

    /// Execute WASM module
    #[cfg(feature = "wasm")]
    async fn execute_wasm_module(
        &self,
        config: &ScriptConfig,
        _ctx: &ExecutionContext,
    ) -> Result<Value> {
        // Clone config code for the blocking task
        let wasm_code = config.code.as_bytes().to_vec();
        let timeout = self.timeout;

        // Execute WASM in a blocking task with timeout
        let result = tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || {
                // Create a WASM store and module
                let mut store = Store::default();

                // Compile WASM code
                let module = Module::new(&store, wasm_code.as_slice())
                    .map_err(|e| CodeExecutionError::WasmCompilationError(e.to_string()))?;

                // Instantiate the module
                let instance = Instance::new(&mut store, &module, &wasmer::imports! {})
                    .map_err(|e| CodeExecutionError::WasmRuntimeError(e.to_string()))?;

                // Get the main export function
                let main_func = instance.exports.get_function("main").map_err(|e| {
                    CodeExecutionError::WasmRuntimeError(format!("No 'main' function: {}", e))
                })?;

                // Call the function
                main_func
                    .call(&mut store, &[])
                    .map_err(|e| CodeExecutionError::WasmRuntimeError(e.to_string()))
            }),
        )
        .await
        .map_err(|_| CodeExecutionError::TimeoutExceeded)?
        .map_err(|e| CodeExecutionError::WasmRuntimeError(format!("Task join error: {}", e)))??;

        // Convert result to JSON (simple integer result for now)
        if let Some(val) = result.first() {
            Ok(serde_json::json!({
                "result": format!("{:?}", val),
                "runtime": "wasm"
            }))
        } else {
            Ok(serde_json::json!({
                "result": null,
                "runtime": "wasm"
            }))
        }
    }

    /// Convert JSON value to Rhai Dynamic
    #[allow(clippy::only_used_in_recursion)]
    fn json_to_rhai(&self, value: &Value) -> Result<Dynamic> {
        match value {
            Value::Null => Ok(Dynamic::UNIT),
            Value::Bool(b) => Ok(Dynamic::from(*b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Dynamic::from(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Dynamic::from(f))
                } else {
                    Err(CodeExecutionError::SerializationError(
                        "Invalid number".to_string(),
                    ))
                }
            }
            Value::String(s) => Ok(Dynamic::from(s.clone())),
            Value::Array(arr) => {
                let rhai_arr: rhai::Array = arr
                    .iter()
                    .map(|v| self.json_to_rhai(v))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Dynamic::from(rhai_arr))
            }
            Value::Object(obj) => {
                let mut rhai_map = rhai::Map::new();
                for (k, v) in obj {
                    rhai_map.insert(k.clone().into(), self.json_to_rhai(v)?);
                }
                Ok(Dynamic::from(rhai_map))
            }
        }
    }

    /// Convert Rhai Dynamic to JSON value
    #[allow(clippy::only_used_in_recursion)]
    fn rhai_to_json(&self, value: &Dynamic) -> Result<Value> {
        if value.is_unit() {
            Ok(Value::Null)
        } else if value.is::<bool>() {
            Ok(Value::Bool(value.as_bool().unwrap()))
        } else if value.is::<i64>() {
            Ok(serde_json::json!(value.as_int().unwrap()))
        } else if value.is::<f64>() {
            Ok(serde_json::json!(value.as_float().unwrap()))
        } else if value.is::<rhai::ImmutableString>() {
            Ok(Value::String(value.to_string()))
        } else if value.is::<rhai::Array>() {
            let arr = value.clone().cast::<rhai::Array>();
            let json_arr: Vec<Value> = arr
                .iter()
                .map(|v| self.rhai_to_json(v))
                .collect::<Result<Vec<_>>>()?;
            Ok(Value::Array(json_arr))
        } else if value.is::<rhai::Map>() {
            let map = value.clone().cast::<rhai::Map>();
            let mut json_obj = serde_json::Map::new();
            for (k, v) in map {
                json_obj.insert(k.to_string(), self.rhai_to_json(&v)?);
            }
            Ok(Value::Object(json_obj))
        } else {
            // Fallback: convert to string
            Ok(Value::String(value.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_rhai_simple_math() {
        let executor = CodeExecutor::new();
        let mut ctx = ExecutionContext::new(Uuid::new_v4());
        ctx.set_variable("x".to_string(), serde_json::json!(10));
        ctx.set_variable("y".to_string(), serde_json::json!(20));

        let config = ScriptConfig {
            runtime: "rhai".to_string(),
            code: "x + y".to_string(),
            inputs: vec!["x".to_string(), "y".to_string()],
            output: "result".to_string(),
        };

        let result = executor.execute(&config, &ctx).await.unwrap();
        assert_eq!(result, serde_json::json!(30));
    }

    #[tokio::test]
    async fn test_rhai_string_manipulation() {
        let executor = CodeExecutor::new();
        let mut ctx = ExecutionContext::new(Uuid::new_v4());
        ctx.set_variable("name".to_string(), serde_json::json!("World"));

        let config = ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#""Hello, " + name + "!""#.to_string(),
            inputs: vec!["name".to_string()],
            output: "greeting".to_string(),
        };

        let result = executor.execute(&config, &ctx).await.unwrap();
        assert_eq!(result, serde_json::json!("Hello, World!"));
    }

    #[tokio::test]
    async fn test_rhai_array_operations() {
        let executor = CodeExecutor::new();
        let mut ctx = ExecutionContext::new(Uuid::new_v4());
        ctx.set_variable("numbers".to_string(), serde_json::json!([1, 2, 3, 4, 5]));

        let config = ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#"
                let sum = 0;
                for n in numbers {
                    sum += n;
                }
                sum
            "#
            .to_string(),
            inputs: vec!["numbers".to_string()],
            output: "sum".to_string(),
        };

        let result = executor.execute(&config, &ctx).await.unwrap();
        assert_eq!(result, serde_json::json!(15));
    }

    #[tokio::test]
    async fn test_rhai_operation_limit() {
        let executor = CodeExecutor::new();
        let ctx = ExecutionContext::new(Uuid::new_v4());

        let config = ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#"
                loop {
                    // Infinite loop - will hit operation limit
                }
            "#
            .to_string(),
            inputs: vec![],
            output: "result".to_string(),
        };

        let result = executor.execute(&config, &ctx).await;
        assert!(result.is_err());
        // Should fail with ScriptError due to operation limit
        assert!(matches!(
            result.unwrap_err(),
            CodeExecutionError::ScriptError(_)
        ));
    }

    #[tokio::test]
    async fn test_unsupported_runtime() {
        let executor = CodeExecutor::new();
        let ctx = ExecutionContext::new(Uuid::new_v4());

        let config = ScriptConfig {
            runtime: "python".to_string(),
            code: "print('hello')".to_string(),
            inputs: vec![],
            output: "result".to_string(),
        };

        let result = executor.execute(&config, &ctx).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CodeExecutionError::UnsupportedRuntime(_)
        ));
    }
}
