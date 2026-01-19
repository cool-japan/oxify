//! Try-catch error handling executor
//!
//! Executes try-catch-finally blocks for error handling in workflows.

use crate::EngineError;
use oxify_model::{ExecutionContext, TryCatchConfig};
use serde_json::Value;

pub struct TryCatchExecutor;

impl TryCatchExecutor {
    /// Execute a try-catch-finally block
    pub async fn execute(
        config: &TryCatchConfig,
        ctx: &ExecutionContext,
    ) -> Result<TryCatchResult, EngineError> {
        let mut result = TryCatchResult {
            try_result: None,
            catch_result: None,
            finally_result: None,
            error: None,
            succeeded: false,
        };

        // Execute try block
        match Self::execute_expression(&config.try_expression, ctx).await {
            Ok(value) => {
                result.try_result = Some(value);
                result.succeeded = true;
            }
            Err(e) => {
                result.error = Some(e.to_string());

                // Execute catch block if present
                if let Some(catch_expr) = &config.catch_expression {
                    // Create catch context with error variable
                    let mut catch_ctx = ctx.clone();
                    catch_ctx
                        .variables
                        .insert(config.error_variable.clone(), Value::String(e.to_string()));

                    match Self::execute_expression(catch_expr, &catch_ctx).await {
                        Ok(value) => {
                            result.catch_result = Some(value);
                            // If catch succeeds and rethrow is false, mark as succeeded
                            if !config.rethrow {
                                result.succeeded = true;
                            }
                        }
                        Err(catch_err) => {
                            // Catch block itself failed
                            result.error = Some(format!(
                                "Try failed: {}. Catch also failed: {}",
                                e, catch_err
                            ));
                        }
                    }
                }
            }
        }

        // Execute finally block if present (always executes)
        if let Some(finally_expr) = &config.finally_expression {
            match Self::execute_expression(finally_expr, ctx).await {
                Ok(value) => {
                    result.finally_result = Some(value);
                }
                Err(e) => {
                    // Finally block failed - this is always an error
                    result.error = Some(format!(
                        "{}. Finally block failed: {}",
                        result.error.as_deref().unwrap_or(""),
                        e
                    ));
                    result.succeeded = false;
                }
            }
        }

        Ok(result)
    }

    /// Execute an expression (template string or value)
    async fn execute_expression(
        expression: &str,
        ctx: &ExecutionContext,
    ) -> Result<Value, EngineError> {
        // Resolve template variables
        let resolved = Self::resolve_template(expression, ctx)?;

        // Try to parse as JSON, otherwise return as string
        if let Ok(json_value) = serde_json::from_str::<Value>(&resolved) {
            Ok(json_value)
        } else {
            Ok(Value::String(resolved))
        }
    }

    /// Resolve template variables like {{variable_name}}
    fn resolve_template(template: &str, ctx: &ExecutionContext) -> Result<String, EngineError> {
        let mut result = template.to_string();

        let re = regex::Regex::new(r"\{\{([^}]+)\}\}")
            .map_err(|e| EngineError::TemplateError(e.to_string()))?;

        for cap in re.captures_iter(template) {
            let var_name = cap.get(1).unwrap().as_str().trim();

            if let Some(value) = ctx.variables.get(var_name) {
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => value.to_string(),
                };
                result = result.replace(&cap[0], &value_str);
            } else {
                return Err(EngineError::VariableNotFound(var_name.to_string()));
            }
        }

        Ok(result)
    }
}

/// Result of try-catch-finally execution
#[derive(Debug)]
pub struct TryCatchResult {
    pub try_result: Option<Value>,
    pub catch_result: Option<Value>,
    pub finally_result: Option<Value>,
    pub error: Option<String>,
    pub succeeded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_context() -> ExecutionContext {
        ExecutionContext::new(Uuid::new_v4())
    }

    #[tokio::test]
    async fn test_try_success() {
        let ctx = create_test_context();

        let config = TryCatchConfig {
            try_expression: "success".to_string(),
            catch_expression: None,
            finally_expression: None,
            rethrow: false,
            error_variable: "error".to_string(),
        };

        let result = TryCatchExecutor::execute(&config, &ctx).await.unwrap();

        assert!(result.succeeded);
        assert!(result.try_result.is_some());
        assert!(result.catch_result.is_none());
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_try_failure_with_catch() {
        let ctx = create_test_context();

        let config = TryCatchConfig {
            try_expression: "{{nonexistent}}".to_string(),
            catch_expression: Some("caught error: {{error}}".to_string()),
            finally_expression: None,
            rethrow: false,
            error_variable: "error".to_string(),
        };

        let result = TryCatchExecutor::execute(&config, &ctx).await.unwrap();

        // Should succeed because catch handled the error
        assert!(result.succeeded);
        assert!(result.try_result.is_none());
        assert!(result.catch_result.is_some());
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_try_failure_with_rethrow() {
        let ctx = create_test_context();

        let config = TryCatchConfig {
            try_expression: "{{nonexistent}}".to_string(),
            catch_expression: Some("logging error".to_string()),
            finally_expression: None,
            rethrow: true,
            error_variable: "error".to_string(),
        };

        let result = TryCatchExecutor::execute(&config, &ctx).await.unwrap();

        // Should fail because rethrow is true
        assert!(!result.succeeded);
        assert!(result.catch_result.is_some());
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_finally_always_executes() {
        let mut ctx = create_test_context();
        ctx.variables
            .insert("value".to_string(), Value::String("test".to_string()));

        let config = TryCatchConfig {
            try_expression: "{{value}}".to_string(),
            catch_expression: None,
            finally_expression: Some("cleanup done".to_string()),
            rethrow: false,
            error_variable: "error".to_string(),
        };

        let result = TryCatchExecutor::execute(&config, &ctx).await.unwrap();

        assert!(result.succeeded);
        assert!(result.try_result.is_some());
        assert!(result.finally_result.is_some());
    }

    #[tokio::test]
    async fn test_finally_executes_on_error() {
        let ctx = create_test_context();

        let config = TryCatchConfig {
            try_expression: "{{nonexistent}}".to_string(),
            catch_expression: Some("handled".to_string()),
            finally_expression: Some("cleanup done".to_string()),
            rethrow: false,
            error_variable: "error".to_string(),
        };

        let result = TryCatchExecutor::execute(&config, &ctx).await.unwrap();

        assert!(result.succeeded);
        assert!(result.catch_result.is_some());
        assert!(result.finally_result.is_some());
    }
}
