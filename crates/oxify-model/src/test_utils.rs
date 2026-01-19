//! Testing Utilities - Helper functions for creating test workflows and mock data
//!
//! This module provides convenient functions to create test workflows, nodes,
//! and analytics data for testing purposes. This is especially useful when
//! writing tests for applications that use oxify-model.
//!
//! # Example
//!
//! ```
//! use oxify_model::test_utils::{create_test_workflow, create_test_analytics};
//!
//! # fn example() {
//! // Create a simple test workflow
//! let workflow = create_test_workflow("test", 5);
//! assert_eq!(workflow.nodes.len(), 5);
//!
//! // Create mock analytics data
//! let analytics = create_test_analytics("test", 100, 0.85);
//! assert_eq!(analytics.execution_stats.total_executions, 100);
//! assert_eq!(analytics.execution_stats.success_rate, 0.85);
//! # }
//! ```

use crate::{
    analytics::{
        AnalyticsPeriod, ExecutionStats, PerformanceMetrics, PeriodType, WorkflowAnalytics,
    },
    execution::{ExecutionContext, ExecutionResult, NodeExecutionResult, NodeMetrics, TokenUsage},
    node::{LlmConfig, LoopConfig, Node, NodeKind},
    workflow::{Workflow, WorkflowMetadata},
    Edge, WorkflowBuilder,
};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

/// Create a simple test workflow with the specified number of nodes
///
/// This creates a linear workflow with:
/// - 1 Start node
/// - N-2 LLM nodes (where N is the node_count parameter)
/// - 1 End node
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_test_workflow;
///
/// let workflow = create_test_workflow("my_test", 5);
/// assert_eq!(workflow.nodes.len(), 5);
/// ```
pub fn create_test_workflow(name: &str, node_count: usize) -> Workflow {
    let mut builder = WorkflowBuilder::new(name);

    // Start node
    builder = builder.start("Start");

    // Add intermediate LLM nodes
    for i in 1..node_count.saturating_sub(1) {
        let llm_config = LlmConfig {
            provider: "test".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: format!("Process step {}", i),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };
        builder = builder.llm(format!("LLM_{}", i), llm_config);
    }

    // End node
    builder = builder.end("End");

    builder.build()
}

/// Create a branching test workflow with a switch node
///
/// This creates a workflow with:
/// - 1 Start node
/// - 1 LLM node
/// - 1 Switch node (multi-branch routing)
/// - 1 End node
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_branching_workflow;
///
/// let workflow = create_branching_workflow("branching_test");
/// assert!(workflow.nodes.len() >= 4);
/// ```
pub fn create_branching_workflow(name: &str) -> Workflow {
    use crate::node::{SwitchCase, SwitchConfig};

    let mut builder = WorkflowBuilder::new(name);

    // Start node
    builder = builder.start("Start");

    // First LLM node
    let llm_config = LlmConfig {
        provider: "test".to_string(),
        model: "gpt-4".to_string(),
        system_prompt: None,
        prompt_template: "Initial processing".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(1000),
        tools: vec![],
        images: vec![],
        extra_params: serde_json::Value::Null,
    };
    builder = builder.llm("Process", llm_config);

    // Switch branching node
    let switch_config = SwitchConfig {
        switch_on: "{{status}}".to_string(),
        cases: vec![
            SwitchCase {
                match_value: "success".to_string(),
                action: "Process success".to_string(),
            },
            SwitchCase {
                match_value: "error".to_string(),
                action: "Handle error".to_string(),
            },
        ],
        default_case: Some("Default handling".to_string()),
    };
    builder = builder.switch("Router", switch_config);

    // End node
    builder = builder.end("End");

    builder.build()
}

/// Create mock analytics data for testing
///
/// # Arguments
///
/// * `workflow_name` - Name of the workflow
/// * `total_executions` - Total number of executions
/// * `success_rate` - Success rate (0.0 to 1.0)
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_test_analytics;
///
/// let analytics = create_test_analytics("test", 100, 0.90);
/// assert_eq!(analytics.execution_stats.total_executions, 100);
/// assert_eq!(analytics.execution_stats.success_rate, 0.90);
/// ```
pub fn create_test_analytics(
    workflow_name: &str,
    total_executions: u64,
    success_rate: f64,
) -> WorkflowAnalytics {
    let successful = (total_executions as f64 * success_rate) as u64;
    let failed = total_executions - successful;

    WorkflowAnalytics {
        workflow_id: Uuid::new_v4(),
        workflow_name: workflow_name.to_string(),
        period: AnalyticsPeriod {
            start: Utc::now(),
            end: Utc::now(),
            period_type: PeriodType::Daily,
        },
        execution_stats: ExecutionStats {
            total_executions,
            successful_executions: successful,
            failed_executions: failed,
            cancelled_executions: 0,
            success_rate,
            failure_rate: 1.0 - success_rate,
            executions_per_hour: total_executions as f64 / 24.0,
        },
        performance_metrics: PerformanceMetrics {
            avg_duration_ms: 1500.0,
            p50_duration_ms: 1200,
            p95_duration_ms: 3000,
            p99_duration_ms: 4500,
            min_duration_ms: 500,
            max_duration_ms: 5000,
            total_tokens: total_executions * 1000,
            avg_tokens: 1000.0,
            total_cost_usd: total_executions as f64 * 0.01,
            avg_cost_usd: 0.01,
        },
        node_analytics: vec![],
        error_patterns: vec![],
        updated_at: Utc::now(),
    }
}

/// Create a test execution context with the given workflow ID
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_test_execution_context;
///
/// let context = create_test_execution_context();
/// assert_eq!(context.state, oxify_model::ExecutionState::Running);
/// ```
pub fn create_test_execution_context() -> ExecutionContext {
    ExecutionContext::new(Uuid::new_v4())
}

/// Create a test node execution result with success status
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_test_node_result;
///
/// let result = create_test_node_result(true);
/// assert!(result.completed_at.is_some());
/// ```
pub fn create_test_node_result(success: bool) -> NodeExecutionResult {
    let result = NodeExecutionResult::new();

    if success {
        result
            .with_metrics(NodeMetrics {
                duration_ms: Some(100),
                token_usage: Some(TokenUsage {
                    input_tokens: 50,
                    output_tokens: 100,
                    total_tokens: 150,
                    cached_tokens: None,
                }),
                cost_usd: Some(0.001),
                api_calls: 1,
                bytes_transferred: 1024,
                memory_bytes: None,
                custom: HashMap::new(),
            })
            .complete(ExecutionResult::Success(
                serde_json::json!({"status": "success"}),
            ))
    } else {
        result.complete(ExecutionResult::Failure("Test error".to_string()))
    }
}

/// Create a test workflow metadata with the given name
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_test_metadata;
///
/// let metadata = create_test_metadata("my_workflow", "1.0.0");
/// assert_eq!(metadata.name, "my_workflow");
/// assert_eq!(metadata.version, "1.0.0");
/// ```
pub fn create_test_metadata(name: &str, version: &str) -> WorkflowMetadata {
    WorkflowMetadata {
        id: Uuid::new_v4(),
        name: name.to_string(),
        description: Some(format!("Test workflow: {}", name)),
        version: version.to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        tags: vec!["test".to_string()],
        parent_id: None,
        change_description: None,
        schedule: None,
    }
}

/// Create a simple LLM node for testing
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_test_llm_node;
///
/// let node = create_test_llm_node("test_node", "Test prompt");
/// assert_eq!(node.name, "test_node");
/// ```
pub fn create_test_llm_node(name: &str, prompt: &str) -> Node {
    Node {
        id: Uuid::new_v4(),
        name: name.to_string(),
        kind: NodeKind::LLM(LlmConfig {
            provider: "test".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: prompt.to_string(),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
        position: None,
        retry_config: None,
        timeout_config: None,
    }
}

/// Create a test edge connecting two nodes
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_test_edge;
/// use uuid::Uuid;
///
/// let from = Uuid::new_v4();
/// let to = Uuid::new_v4();
/// let edge = create_test_edge(from, to, Some("success"));
/// assert_eq!(edge.from, from);
/// assert_eq!(edge.to, to);
/// ```
pub fn create_test_edge(from: Uuid, to: Uuid, label: Option<&str>) -> Edge {
    Edge {
        id: Uuid::new_v4(),
        from,
        to,
        label: label.map(String::from),
        condition: None,
    }
}

/// Create a batch of test workflows with different configurations
///
/// This creates a collection of workflows for testing batch operations.
///
/// # Arguments
///
/// * `count` - Number of workflows to create
/// * `nodes_per_workflow` - Number of nodes in each workflow
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_test_workflow_batch;
///
/// let workflows = create_test_workflow_batch(3, 5);
/// assert_eq!(workflows.len(), 3);
/// assert_eq!(workflows[0].nodes.len(), 5);
/// ```
pub fn create_test_workflow_batch(count: usize, nodes_per_workflow: usize) -> Vec<Workflow> {
    (0..count)
        .map(|i| create_test_workflow(&format!("test_workflow_{}", i), nodes_per_workflow))
        .collect()
}

/// Create a workflow with a loop for testing iteration
///
/// # Example
///
/// ```
/// use oxify_model::test_utils::create_loop_workflow;
/// use oxify_model::LoopConfig;
///
/// let workflow = create_loop_workflow("loop_test");
/// assert!(!workflow.nodes.is_empty());
/// ```
pub fn create_loop_workflow(name: &str) -> Workflow {
    use crate::node::LoopType;

    let mut builder = WorkflowBuilder::new(name);

    let loop_config = LoopConfig {
        loop_type: LoopType::ForEach {
            collection_path: "{{items}}".to_string(),
            item_variable: "item".to_string(),
            index_variable: None,
            body_expression: "process item".to_string(),
            parallel: false,
            max_concurrency: None,
        },
        max_iterations: 10,
    };

    builder = builder
        .start("Start")
        .loop_node("Loop", loop_config)
        .end("End");

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::ExecutionState;

    #[test]
    fn test_create_test_workflow() {
        let workflow = create_test_workflow("test", 5);
        assert_eq!(workflow.metadata.name, "test");
        assert_eq!(workflow.nodes.len(), 5);
        assert!(!workflow.edges.is_empty());
    }

    #[test]
    fn test_create_test_workflow_minimum() {
        let workflow = create_test_workflow("minimal", 2);
        assert_eq!(workflow.nodes.len(), 2);
    }

    #[test]
    fn test_create_branching_workflow() {
        let workflow = create_branching_workflow("branch");
        assert!(workflow.nodes.len() >= 4);
    }

    #[test]
    fn test_create_test_analytics() {
        let analytics = create_test_analytics("test", 100, 0.85);
        assert_eq!(analytics.execution_stats.total_executions, 100);
        assert_eq!(analytics.execution_stats.success_rate, 0.85);
        assert_eq!(analytics.execution_stats.successful_executions, 85);
        assert_eq!(analytics.execution_stats.failed_executions, 15);
    }

    #[test]
    fn test_create_test_execution_context() {
        let context = create_test_execution_context();
        assert_eq!(context.state, ExecutionState::Running);
    }

    #[test]
    fn test_create_test_node_result_success() {
        let result = create_test_node_result(true);
        assert!(result.completed_at.is_some());
        assert!(matches!(result.result, ExecutionResult::Success(_)));
        assert!(result.metrics.is_some());
    }

    #[test]
    fn test_create_test_node_result_failure() {
        let result = create_test_node_result(false);
        assert!(result.completed_at.is_some());
        assert!(matches!(result.result, ExecutionResult::Failure(_)));
    }

    #[test]
    fn test_create_test_metadata() {
        let metadata = create_test_metadata("my_workflow", "1.0.0");
        assert_eq!(metadata.name, "my_workflow");
        assert_eq!(metadata.version, "1.0.0");
        assert!(metadata.description.is_some());
        assert!(!metadata.tags.is_empty());
    }

    #[test]
    fn test_create_test_llm_node() {
        let node = create_test_llm_node("test", "prompt");
        assert_eq!(node.name, "test");
        match node.kind {
            NodeKind::LLM(config) => {
                assert_eq!(config.prompt_template, "prompt");
            }
            _ => panic!("Expected LLM node"),
        }
    }

    #[test]
    fn test_create_test_edge() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let edge = create_test_edge(from, to, Some("success"));
        assert_eq!(edge.from, from);
        assert_eq!(edge.to, to);
        assert_eq!(edge.label, Some("success".to_string()));
    }

    #[test]
    fn test_create_test_workflow_batch() {
        let workflows = create_test_workflow_batch(3, 5);
        assert_eq!(workflows.len(), 3);
        for workflow in workflows {
            assert_eq!(workflow.nodes.len(), 5);
        }
    }

    #[test]
    fn test_create_loop_workflow() {
        let workflow = create_loop_workflow("loop");
        assert!(!workflow.nodes.is_empty());
        assert_eq!(workflow.metadata.name, "loop");
    }

    #[test]
    fn test_analytics_calculations() {
        let analytics = create_test_analytics("test", 200, 0.75);
        assert_eq!(analytics.execution_stats.successful_executions, 150);
        assert_eq!(analytics.execution_stats.failed_executions, 50);
        assert_eq!(analytics.execution_stats.failure_rate, 0.25);
    }
}
