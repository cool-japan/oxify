//! Loop execution engine
//!
//! Executes loop nodes (ForEach, While, Repeat) within a workflow.

use crate::conditional::ConditionalEvaluator;
use crate::EngineError;
use oxify_model::{ExecutionContext, LoopConfig, LoopType};
use serde_json::Value;

pub struct LoopExecutor;

impl LoopExecutor {
    /// Execute a loop and return the results
    pub async fn execute(
        config: &LoopConfig,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Value>, EngineError> {
        match &config.loop_type {
            LoopType::ForEach {
                collection_path,
                item_variable,
                index_variable,
                body_expression,
                ..
            } => {
                Self::execute_foreach(
                    collection_path,
                    item_variable,
                    index_variable.as_deref(),
                    body_expression,
                    ctx,
                    config.max_iterations,
                )
                .await
            }
            LoopType::While {
                condition,
                body_expression,
                counter_variable,
            } => {
                Self::execute_while(
                    condition,
                    body_expression,
                    counter_variable.as_deref(),
                    ctx,
                    config.max_iterations,
                )
                .await
            }
            LoopType::Repeat {
                count,
                body_expression,
                index_variable,
            } => {
                Self::execute_repeat(
                    count,
                    body_expression,
                    index_variable.as_deref(),
                    ctx,
                    config.max_iterations,
                )
                .await
            }
        }
    }

    async fn execute_foreach(
        collection_path: &str,
        item_variable: &str,
        index_variable: Option<&str>,
        body_expression: &str,
        ctx: &ExecutionContext,
        max_iterations: usize,
    ) -> Result<Vec<Value>, EngineError> {
        // Get collection from context
        let collection = Self::get_variable(ctx, collection_path)?;

        // Collection must be an array
        let items = collection.as_array().ok_or_else(|| {
            EngineError::ExecutionError(format!(
                "Variable '{}' is not an array (got: {})",
                collection_path, collection
            ))
        })?;

        // Check max iterations
        if items.len() > max_iterations {
            return Err(EngineError::ExecutionError(format!(
                "Collection size {} exceeds max_iterations {}",
                items.len(),
                max_iterations
            )));
        }

        let mut results = Vec::new();

        for (idx, item) in items.iter().enumerate() {
            // Create loop context with item and index variables
            let mut loop_ctx = ctx.clone();
            loop_ctx
                .variables
                .insert(item_variable.to_string(), item.clone());

            if let Some(idx_var) = index_variable {
                loop_ctx
                    .variables
                    .insert(idx_var.to_string(), Value::Number(idx.into()));
            }

            // Execute body expression
            let result = Self::execute_expression(body_expression, &loop_ctx).await?;
            results.push(result);
        }

        Ok(results)
    }

    async fn execute_while(
        condition: &str,
        body_expression: &str,
        counter_variable: Option<&str>,
        ctx: &ExecutionContext,
        max_iterations: usize,
    ) -> Result<Vec<Value>, EngineError> {
        let mut results = Vec::new();
        let mut counter = 0usize;

        loop {
            // Check max iterations
            if counter >= max_iterations {
                return Err(EngineError::ExecutionError(format!(
                    "While loop exceeded max_iterations {}",
                    max_iterations
                )));
            }

            // Create loop context with counter
            let mut loop_ctx = ctx.clone();
            if let Some(counter_var) = counter_variable {
                loop_ctx
                    .variables
                    .insert(counter_var.to_string(), Value::Number(counter.into()));
            }

            // Evaluate condition
            let evaluator = ConditionalEvaluator::new(&loop_ctx).map_err(|e| {
                EngineError::ExecutionError(format!("Failed to create evaluator: {}", e))
            })?;

            let condition_met = evaluator.evaluate(condition).map_err(|e| {
                EngineError::ExecutionError(format!("Condition evaluation failed: {}", e))
            })?;

            if !condition_met {
                break;
            }

            // Execute body expression
            let result = Self::execute_expression(body_expression, &loop_ctx).await?;
            results.push(result);

            counter += 1;
        }

        Ok(results)
    }

    async fn execute_repeat(
        count_expr: &str,
        body_expression: &str,
        index_variable: Option<&str>,
        ctx: &ExecutionContext,
        max_iterations: usize,
    ) -> Result<Vec<Value>, EngineError> {
        // Resolve count expression (can be template like "{{count}}")
        let count_str = Self::resolve_template(count_expr, ctx)?;

        // Parse count
        let count: usize = count_str.parse().map_err(|e| {
            EngineError::ExecutionError(format!(
                "Failed to parse count '{}' as integer: {}",
                count_str, e
            ))
        })?;

        // Check max iterations
        if count > max_iterations {
            return Err(EngineError::ExecutionError(format!(
                "Repeat count {} exceeds max_iterations {}",
                count, max_iterations
            )));
        }

        let mut results = Vec::new();

        for idx in 0..count {
            // Create loop context with index variable
            let mut loop_ctx = ctx.clone();
            if let Some(idx_var) = index_variable {
                loop_ctx
                    .variables
                    .insert(idx_var.to_string(), Value::Number(idx.into()));
            }

            // Execute body expression
            let result = Self::execute_expression(body_expression, &loop_ctx).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Execute a body expression (can be template string or simple value)
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

    /// Get a variable from context by path
    fn get_variable(ctx: &ExecutionContext, path: &str) -> Result<Value, EngineError> {
        // Try direct variable lookup first
        if let Some(value) = ctx.variables.get(path) {
            return Ok(value.clone());
        }

        // Try node result lookup (format: node_id.field)
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() == 2 {
            // Try to parse first part as node result key
            // For now, just return error
            return Err(EngineError::VariableNotFound(path.to_string()));
        }

        Err(EngineError::VariableNotFound(path.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_context() -> ExecutionContext {
        ExecutionContext::new(Uuid::new_v4())
    }

    #[tokio::test]
    async fn test_foreach_loop() {
        let mut ctx = create_test_context();
        ctx.variables.insert(
            "items".to_string(),
            serde_json::json!(["apple", "banana", "cherry"]),
        );

        let config = LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: "items".to_string(),
                item_variable: "item".to_string(),
                index_variable: Some("idx".to_string()),
                body_expression: "{{item}}".to_string(),
                parallel: false,
                max_concurrency: None,
            },
            max_iterations: 1000,
        };

        let results = LoopExecutor::execute(&config, &ctx).await.unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0], Value::String("apple".to_string()));
        assert_eq!(results[1], Value::String("banana".to_string()));
        assert_eq!(results[2], Value::String("cherry".to_string()));
    }

    #[tokio::test]
    async fn test_repeat_loop() {
        let mut ctx = create_test_context();
        ctx.variables
            .insert("count".to_string(), serde_json::json!(5));

        let config = LoopConfig {
            loop_type: LoopType::Repeat {
                count: "{{count}}".to_string(),
                body_expression: "iteration {{idx}}".to_string(),
                index_variable: Some("idx".to_string()),
            },
            max_iterations: 1000,
        };

        let results = LoopExecutor::execute(&config, &ctx).await.unwrap();

        assert_eq!(results.len(), 5);
        assert_eq!(results[0], Value::String("iteration 0".to_string()));
        assert_eq!(results[4], Value::String("iteration 4".to_string()));
    }

    #[tokio::test]
    async fn test_while_loop() {
        let mut ctx = create_test_context();
        ctx.variables
            .insert("max".to_string(), serde_json::json!(3));

        let config = LoopConfig {
            loop_type: LoopType::While {
                condition: "counter < 3".to_string(),
                body_expression: "count: {{counter}}".to_string(),
                counter_variable: Some("counter".to_string()),
            },
            max_iterations: 1000,
        };

        let results = LoopExecutor::execute(&config, &ctx).await.unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0], Value::String("count: 0".to_string()));
        assert_eq!(results[2], Value::String("count: 2".to_string()));
    }

    #[tokio::test]
    async fn test_max_iterations_limit() {
        let mut ctx = create_test_context();
        ctx.variables.insert(
            "items".to_string(),
            serde_json::json!(vec!["a"; 1500]), // 1500 items
        );

        let config = LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: "items".to_string(),
                item_variable: "item".to_string(),
                index_variable: None,
                body_expression: "{{item}}".to_string(),
                parallel: false,
                max_concurrency: None,
            },
            max_iterations: 1000,
        };

        let result = LoopExecutor::execute(&config, &ctx).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("exceeds max_iterations"));
    }
}
