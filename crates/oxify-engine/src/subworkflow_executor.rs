//! Sub-workflow execution engine
//!
//! Executes sub-workflows (workflow composition) within a parent workflow.

use crate::{Engine, EngineError};
use oxify_model::{ExecutionContext, SubWorkflowConfig, Workflow};
use serde_json::Value;

pub struct SubWorkflowExecutor;

impl SubWorkflowExecutor {
    /// Execute a sub-workflow
    pub fn execute<'a>(
        config: &'a SubWorkflowConfig,
        parent_ctx: &'a ExecutionContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<SubWorkflowResult, EngineError>> + Send + 'a>,
    > {
        Box::pin(Self::execute_impl(config, parent_ctx))
    }

    async fn execute_impl(
        config: &SubWorkflowConfig,
        parent_ctx: &ExecutionContext,
    ) -> Result<SubWorkflowResult, EngineError> {
        // Load workflow from JSON file
        let workflow = Workflow::from_json_file(&config.workflow_path)
            .map_err(|e| EngineError::ExecutionError(format!("Failed to load workflow: {}", e)))?;

        // Validate sub-workflow
        workflow.validate().map_err(|e| {
            EngineError::ValidationError(format!("Sub-workflow validation failed: {}", e))
        })?;

        // Create sub-workflow context
        let mut sub_ctx = ExecutionContext::new(workflow.metadata.id);

        // Inherit parent context if configured
        if config.inherit_context {
            sub_ctx.variables = parent_ctx.variables.clone();
        }

        // Apply input mappings
        for (sub_var_name, parent_var_expr) in &config.input_mappings {
            let resolved_value = Self::resolve_template(parent_var_expr, parent_ctx)?;
            sub_ctx
                .variables
                .insert(sub_var_name.clone(), resolved_value);
        }

        // Execute sub-workflow sequentially (to avoid nested tokio::spawn)
        let engine = Engine::new();
        let result_ctx = engine.execute_sequential(&workflow).await?;

        // Extract output
        let output = if let Some(output_var) = &config.output_variable {
            // Extract specific variable
            result_ctx
                .variables
                .get(output_var)
                .cloned()
                .unwrap_or(Value::Null)
        } else {
            // Return all variables
            serde_json::to_value(&result_ctx.variables).unwrap_or(Value::Null)
        };

        Ok(SubWorkflowResult {
            output,
            sub_workflow_id: workflow.metadata.id,
            execution_state: result_ctx.state,
            node_count: result_ctx.node_results.len(),
        })
    }

    /// Resolve template variables like {{variable_name}}
    fn resolve_template(template: &str, ctx: &ExecutionContext) -> Result<Value, EngineError> {
        // Check if the template is a simple variable reference
        let re = regex::Regex::new(r"^\{\{([^}]+)\}\}$")
            .map_err(|e| EngineError::TemplateError(e.to_string()))?;

        if let Some(cap) = re.captures(template) {
            let var_name = cap.get(1).expect("invariant: capture group 1 present in regex").as_str().trim();
            return ctx
                .variables
                .get(var_name)
                .cloned()
                .ok_or_else(|| EngineError::VariableNotFound(var_name.to_string()));
        }

        // Otherwise, resolve as a string with embedded variables
        let mut result = template.to_string();
        let re = regex::Regex::new(r"\{\{([^}]+)\}\}")
            .map_err(|e| EngineError::TemplateError(e.to_string()))?;

        for cap in re.captures_iter(template) {
            let var_name = cap.get(1).expect("invariant: capture group 1 present in regex").as_str().trim();

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

        Ok(Value::String(result))
    }
}

/// Result of sub-workflow execution
#[derive(Debug)]
pub struct SubWorkflowResult {
    pub output: Value,
    pub sub_workflow_id: oxify_model::WorkflowId,
    pub execution_state: oxify_model::ExecutionState,
    pub node_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, Node, NodeKind};
    use std::collections::HashMap;
    use std::fs;
    use uuid::Uuid;

    fn create_test_context() -> ExecutionContext {
        ExecutionContext::new(Uuid::new_v4())
    }

    #[tokio::test]
    async fn test_simple_subworkflow() {
        // Create a simple sub-workflow
        let mut sub_workflow = Workflow::new("Sub Workflow".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start.id;
        sub_workflow.add_node(start);

        let end = Node::new("End".to_string(), NodeKind::End);
        let end_id = end.id;
        sub_workflow.add_node(end);

        sub_workflow.add_edge(Edge::new(start_id, end_id));

        // Save sub-workflow to temp file
        let temp_path = "/tmp/test_subworkflow.json";
        sub_workflow.to_json_file(temp_path).unwrap();

        // Create parent context
        let ctx = create_test_context();

        // Configure sub-workflow execution
        let config = SubWorkflowConfig {
            workflow_path: temp_path.to_string(),
            input_mappings: HashMap::new(),
            output_variable: None,
            inherit_context: false,
        };

        // Execute sub-workflow
        let result = SubWorkflowExecutor::execute(&config, &ctx).await;
        assert!(result.is_ok());

        let sub_result = result.unwrap();
        assert_eq!(sub_result.node_count, 2); // Start + End

        // Cleanup
        fs::remove_file(temp_path).ok();
    }

    #[tokio::test]
    async fn test_subworkflow_with_input_mappings() {
        // Create a simple sub-workflow
        let mut sub_workflow = Workflow::new("Sub Workflow With Input".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start.id;
        sub_workflow.add_node(start);

        let end = Node::new("End".to_string(), NodeKind::End);
        let end_id = end.id;
        sub_workflow.add_node(end);

        sub_workflow.add_edge(Edge::new(start_id, end_id));

        // Save sub-workflow to temp file
        let temp_path = "/tmp/test_subworkflow_input.json";
        sub_workflow.to_json_file(temp_path).unwrap();

        // Create parent context with variables
        let mut ctx = create_test_context();
        ctx.variables.insert(
            "parent_value".to_string(),
            Value::String("test".to_string()),
        );

        // Configure sub-workflow with input mapping
        let mut input_mappings = HashMap::new();
        input_mappings.insert("sub_value".to_string(), "{{parent_value}}".to_string());

        let config = SubWorkflowConfig {
            workflow_path: temp_path.to_string(),
            input_mappings,
            output_variable: None,
            inherit_context: false,
        };

        // Execute sub-workflow
        let result = SubWorkflowExecutor::execute(&config, &ctx).await;
        assert!(result.is_ok());

        // Cleanup
        fs::remove_file(temp_path).ok();
    }

    #[tokio::test]
    async fn test_subworkflow_with_inheritance() {
        // Create a simple sub-workflow
        let mut sub_workflow = Workflow::new("Sub Workflow Inherit".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start.id;
        sub_workflow.add_node(start);

        let end = Node::new("End".to_string(), NodeKind::End);
        let end_id = end.id;
        sub_workflow.add_node(end);

        sub_workflow.add_edge(Edge::new(start_id, end_id));

        // Save sub-workflow
        let temp_path = "/tmp/test_subworkflow_inherit.json";
        sub_workflow.to_json_file(temp_path).unwrap();

        // Create parent context with variables
        let mut ctx = create_test_context();
        ctx.variables
            .insert("inherited_var".to_string(), Value::Number(42.into()));

        // Configure with context inheritance
        let config = SubWorkflowConfig {
            workflow_path: temp_path.to_string(),
            input_mappings: HashMap::new(),
            output_variable: None,
            inherit_context: true,
        };

        // Execute sub-workflow
        let result = SubWorkflowExecutor::execute(&config, &ctx).await;
        assert!(result.is_ok());

        // Cleanup
        fs::remove_file(temp_path).ok();
    }
}
