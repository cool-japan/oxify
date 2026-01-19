//! Conditional expression evaluation for workflow branching
//!
//! Supports JavaScript-like expressions with variable substitution and JSONPath queries.

use crate::{EngineError, Result};
use evalexpr::{ContextWithMutableVariables, HashMapContext, Value as EvalValue};
use oxify_model::ExecutionContext;
use serde_json::Value;
use serde_json_path::JsonPath;
use std::str::FromStr;

/// Conditional expression evaluator
pub struct ConditionalEvaluator {
    context: HashMapContext,
    exec_ctx: ExecutionContext,
}

impl ConditionalEvaluator {
    /// Create a new evaluator with variables from execution context
    pub fn new(exec_ctx: &ExecutionContext) -> Result<Self> {
        let mut context = HashMapContext::new();

        // Add all variables from execution context
        for (key, value) in &exec_ctx.variables {
            let eval_value = json_to_eval_value(value)?;
            context
                .set_value(key.clone(), eval_value)
                .map_err(|e| EngineError::ExecutionError(e.to_string()))?;
        }

        // Add node results as accessible variables
        for (node_id, node_result) in &exec_ctx.node_results {
            if let oxify_model::ExecutionResult::Success(output) = &node_result.result {
                let var_name = format!("node_{}", node_id.to_string().replace('-', "_"));
                let eval_value = json_to_eval_value(output)?;
                context
                    .set_value(var_name, eval_value)
                    .map_err(|e| EngineError::ExecutionError(e.to_string()))?;
            }
        }

        Ok(Self {
            context,
            exec_ctx: exec_ctx.clone(),
        })
    }

    /// Evaluate a boolean expression
    pub fn evaluate(&self, expression: &str) -> Result<bool> {
        // First, resolve any JSONPath queries in the expression
        let resolved_expr = self.resolve_jsonpath(expression)?;

        // Evaluate the expression
        let result =
            evalexpr::eval_boolean_with_context(&resolved_expr, &self.context).map_err(|e| {
                EngineError::ExecutionError(format!("Expression evaluation failed: {}", e))
            })?;

        Ok(result)
    }

    /// Resolve JSONPath queries in expression
    /// Format: $.path.to.field or $variable.path.to.field
    fn resolve_jsonpath(&self, expression: &str) -> Result<String> {
        let mut result = expression.to_string();

        // Find JSONPath patterns like $.path or $var.path
        let re = regex::Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)+)")
            .map_err(|e| EngineError::TemplateError(e.to_string()))?;

        for cap in re.captures_iter(expression) {
            let path_expr = cap.get(1).unwrap().as_str();
            let parts: Vec<&str> = path_expr.split('.').collect();

            if parts.is_empty() {
                continue;
            }

            // Get the root variable
            let var_name = parts[0];
            let json_path = parts[1..].join(".");

            // First try to get from node results
            let json_value = if var_name.starts_with("node_") {
                // Extract UUID from variable name (e.g., "node_a1b2c3d4_..." -> UUID)
                let uuid_str = var_name.strip_prefix("node_").unwrap().replace('_', "-");
                if let Ok(node_id) = uuid::Uuid::parse_str(&uuid_str) {
                    if let Some(node_result) = self.exec_ctx.node_results.get(&node_id) {
                        if let oxify_model::ExecutionResult::Success(output) = &node_result.result {
                            output.clone()
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                // Get from variables
                if let Some(var_value) = self.exec_ctx.variables.get(var_name) {
                    var_value.clone()
                } else {
                    continue;
                }
            };

            // Apply JSONPath query
            if !json_path.is_empty() {
                let path = JsonPath::from_str(&format!("$.{}", json_path))
                    .map_err(|e| EngineError::TemplateError(format!("Invalid JSONPath: {}", e)))?;

                let query_result = path.query(&json_value);

                if let Some(first_result) = query_result.first() {
                    let replacement = match first_result {
                        Value::String(s) => format!("\"{}\"", s),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => first_result.to_string(),
                    };
                    result = result.replace(&format!("${}", path_expr), &replacement);
                }
            } else {
                // No path, just use the variable value
                let replacement = match &json_value {
                    Value::String(s) => format!("\"{}\"", s),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => json_value.to_string(),
                };
                result = result.replace(&format!("${}", path_expr), &replacement);
            }
        }

        Ok(result)
    }
}

/// Convert serde_json::Value to evalexpr::Value
fn json_to_eval_value(value: &Value) -> Result<EvalValue> {
    match value {
        Value::Null => Ok(EvalValue::Empty),
        Value::Bool(b) => Ok(EvalValue::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(EvalValue::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(EvalValue::Float(f))
            } else {
                Err(EngineError::ExecutionError("Invalid number".to_string()))
            }
        }
        Value::String(s) => Ok(EvalValue::String(s.clone())),
        Value::Array(_) => Ok(EvalValue::Empty), // Arrays not directly supported
        Value::Object(_) => Ok(EvalValue::Empty), // Objects stored as empty, accessed via JSONPath
    }
}

/// Convert evalexpr::Value to serde_json::Value
#[allow(dead_code)]
fn eval_value_to_json(value: &EvalValue) -> Result<Value> {
    match value {
        EvalValue::Empty => Ok(Value::Null),
        EvalValue::Boolean(b) => Ok(Value::Bool(*b)),
        EvalValue::Int(i) => Ok(Value::Number((*i).into())),
        EvalValue::Float(f) => {
            if let Some(n) = serde_json::Number::from_f64(*f) {
                Ok(Value::Number(n))
            } else {
                Err(EngineError::ExecutionError(
                    "Invalid float value".to_string(),
                ))
            }
        }
        EvalValue::String(s) => Ok(Value::String(s.clone())),
        _ => Ok(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{ExecutionResult, NodeExecutionResult};
    use uuid::Uuid;

    #[test]
    fn test_simple_boolean_expression() {
        let mut ctx = ExecutionContext::new(Uuid::new_v4());
        ctx.set_variable("x".to_string(), serde_json::json!(10));
        ctx.set_variable("y".to_string(), serde_json::json!(5));

        let evaluator = ConditionalEvaluator::new(&ctx).unwrap();

        assert!(evaluator.evaluate("x > y").unwrap());
        assert!(!evaluator.evaluate("x < y").unwrap());
        assert!(evaluator.evaluate("x == 10").unwrap());
        assert!(evaluator.evaluate("x > 5 && y < 10").unwrap());
        assert!(!evaluator.evaluate("x > 5 && y > 10").unwrap());
    }

    #[test]
    fn test_string_comparison() {
        let mut ctx = ExecutionContext::new(Uuid::new_v4());
        ctx.set_variable("status".to_string(), serde_json::json!("success"));

        let evaluator = ConditionalEvaluator::new(&ctx).unwrap();

        assert!(evaluator.evaluate("status == \"success\"").unwrap());
        assert!(!evaluator.evaluate("status == \"failed\"").unwrap());
    }

    #[test]
    fn test_jsonpath_query() {
        let mut ctx = ExecutionContext::new(Uuid::new_v4());

        let node_id = Uuid::new_v4();
        let mut node_result = NodeExecutionResult::new();
        node_result = node_result.complete(ExecutionResult::Success(serde_json::json!({
            "user": {
                "age": 25,
                "name": "Alice"
            }
        })));

        ctx.record_node_result(node_id, node_result);

        let evaluator = ConditionalEvaluator::new(&ctx).unwrap();

        // JSONPath query
        let var_name = format!("node_{}", node_id.to_string().replace('-', "_"));
        let expr = format!("${}.user.age > 20", var_name);
        assert!(evaluator.evaluate(&expr).unwrap());

        let expr2 = format!("${}.user.age < 20", var_name);
        assert!(!evaluator.evaluate(&expr2).unwrap());
    }

    #[test]
    fn test_complex_expression() {
        let mut ctx = ExecutionContext::new(Uuid::new_v4());
        ctx.set_variable("score".to_string(), serde_json::json!(85));
        ctx.set_variable("passed".to_string(), serde_json::json!(true));

        let evaluator = ConditionalEvaluator::new(&ctx).unwrap();

        assert!(evaluator.evaluate("score >= 80 && passed == true").unwrap());
        assert!(evaluator
            .evaluate("(score > 90) || (score >= 80 && passed)")
            .unwrap());
    }
}
