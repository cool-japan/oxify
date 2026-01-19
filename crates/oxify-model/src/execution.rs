use crate::{NodeId, WorkflowId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Unique identifier for a workflow execution
pub type ExecutionId = Uuid;

/// Context for workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExecutionContext {
    /// Unique execution identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub execution_id: ExecutionId,

    /// The workflow being executed
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub workflow_id: WorkflowId,

    /// When the execution started
    pub started_at: DateTime<Utc>,

    /// When the execution completed (if finished)
    pub completed_at: Option<DateTime<Utc>>,

    /// Current execution state
    pub state: ExecutionState,

    /// Node execution results
    #[cfg_attr(feature = "openapi", schema(value_type = HashMap<String, NodeExecutionResult>))]
    pub node_results: HashMap<NodeId, NodeExecutionResult>,

    /// Global variables/context available to all nodes
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// Checkpoint data for resume capability
    #[serde(default)]
    pub checkpoint: Option<ExecutionCheckpoint>,
}

impl ExecutionContext {
    pub fn new(workflow_id: WorkflowId) -> Self {
        Self {
            execution_id: Uuid::new_v4(),
            workflow_id,
            started_at: Utc::now(),
            completed_at: None,
            state: ExecutionState::Running,
            node_results: HashMap::new(),
            variables: HashMap::new(),
            checkpoint: None,
        }
    }

    /// Create a checkpoint of the current execution state
    pub fn create_checkpoint(&mut self) -> ExecutionCheckpoint {
        let checkpoint = ExecutionCheckpoint {
            timestamp: Utc::now(),
            completed_nodes: self.node_results.keys().copied().collect(),
            variables: self.variables.clone(),
            state: self.state.clone(),
        };
        self.checkpoint = Some(checkpoint.clone());
        checkpoint
    }

    /// Resume from a checkpoint
    pub fn resume_from_checkpoint(
        checkpoint: ExecutionCheckpoint,
        workflow_id: WorkflowId,
    ) -> Self {
        let variables = checkpoint.variables.clone();
        let state = checkpoint.state.clone();
        Self {
            execution_id: Uuid::new_v4(),
            workflow_id,
            started_at: checkpoint.timestamp,
            completed_at: None,
            state,
            node_results: HashMap::new(), // Will be restored by engine
            variables,
            checkpoint: Some(checkpoint),
        }
    }

    /// Check if execution can be resumed
    pub fn can_resume(&self) -> bool {
        self.checkpoint.is_some() && matches!(self.state, ExecutionState::Paused)
    }

    /// Pause execution
    pub fn pause(&mut self) {
        self.state = ExecutionState::Paused;
        self.create_checkpoint();
    }

    /// Resume paused execution
    pub fn resume(&mut self) {
        if self.state == ExecutionState::Paused {
            self.state = ExecutionState::Running;
        }
    }

    /// Cancel execution
    pub fn cancel(&mut self) {
        self.state = ExecutionState::Cancelled;
        self.mark_completed();
    }

    /// Mark execution as completed
    pub fn mark_completed(&mut self) {
        if self.completed_at.is_none() {
            self.completed_at = Some(Utc::now());
        }
    }

    /// Record the result of a node execution
    pub fn record_node_result(&mut self, node_id: NodeId, result: NodeExecutionResult) {
        self.node_results.insert(node_id, result);
    }

    /// Get the result of a previous node execution
    pub fn get_node_result(&self, node_id: &NodeId) -> Option<&NodeExecutionResult> {
        self.node_results.get(node_id)
    }

    /// Set a variable in the execution context
    pub fn set_variable(&mut self, key: String, value: serde_json::Value) {
        self.variables.insert(key, value);
    }

    /// Get a variable from the execution context
    pub fn get_variable(&self, key: &str) -> Option<&serde_json::Value> {
        self.variables.get(key)
    }
}

/// State of workflow execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum ExecutionState {
    /// Execution is currently running
    Running,

    /// Execution completed successfully
    Completed,

    /// Execution failed
    Failed(String),

    /// Execution was cancelled
    Cancelled,

    /// Execution is paused
    Paused,
}

/// Result of executing a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct NodeExecutionResult {
    /// When this node started executing
    pub started_at: DateTime<Utc>,

    /// When this node finished executing
    pub completed_at: Option<DateTime<Utc>>,

    /// The result of the execution
    pub result: ExecutionResult,

    /// Number of retry attempts made (0 means no retries)
    #[serde(default)]
    pub retry_count: u32,

    /// Execution metrics (token usage, costs, etc.)
    #[serde(default)]
    pub metrics: Option<NodeMetrics>,
}

/// Execution metrics for a node
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct NodeMetrics {
    /// Execution duration in milliseconds
    pub duration_ms: Option<u64>,

    /// Token usage for LLM nodes
    #[serde(default)]
    pub token_usage: Option<TokenUsage>,

    /// Estimated cost in USD (for LLM API calls)
    #[serde(default)]
    pub cost_usd: Option<f64>,

    /// API calls made by this node
    #[serde(default)]
    pub api_calls: u32,

    /// Bytes transferred (input + output)
    #[serde(default)]
    pub bytes_transferred: u64,

    /// Memory usage in bytes (if tracked)
    #[serde(default)]
    pub memory_bytes: Option<u64>,

    /// Custom metrics (provider-specific)
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Token usage for LLM nodes
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TokenUsage {
    /// Input/prompt tokens
    pub input_tokens: u32,

    /// Output/completion tokens
    pub output_tokens: u32,

    /// Total tokens (input + output)
    pub total_tokens: u32,

    /// Cached tokens (if applicable)
    #[serde(default)]
    pub cached_tokens: Option<u32>,
}

impl TokenUsage {
    /// Create new token usage record
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
            cached_tokens: None,
        }
    }

    /// Estimate cost based on provider pricing
    pub fn estimate_cost(&self, input_price_per_1k: f64, output_price_per_1k: f64) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1000.0) * input_price_per_1k;
        let output_cost = (self.output_tokens as f64 / 1000.0) * output_price_per_1k;
        input_cost + output_cost
    }
}

impl NodeExecutionResult {
    pub fn new() -> Self {
        Self {
            started_at: Utc::now(),
            completed_at: None,
            result: ExecutionResult::Pending,
            retry_count: 0,
            metrics: None,
        }
    }

    pub fn complete(mut self, result: ExecutionResult) -> Self {
        let completed = Utc::now();
        let duration_ms = (completed - self.started_at).num_milliseconds() as u64;
        self.completed_at = Some(completed);
        self.result = result;

        // Auto-populate duration if metrics exist
        if let Some(ref mut metrics) = self.metrics {
            metrics.duration_ms = Some(duration_ms);
        } else {
            self.metrics = Some(NodeMetrics {
                duration_ms: Some(duration_ms),
                ..Default::default()
            });
        }

        self
    }

    /// Add metrics to the execution result
    pub fn with_metrics(mut self, metrics: NodeMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Add token usage to the execution result
    pub fn with_token_usage(mut self, usage: TokenUsage) -> Self {
        if let Some(ref mut metrics) = self.metrics {
            metrics.token_usage = Some(usage);
        } else {
            self.metrics = Some(NodeMetrics {
                token_usage: Some(usage),
                ..Default::default()
            });
        }
        self
    }

    /// Get execution duration in milliseconds
    pub fn duration_ms(&self) -> Option<u64> {
        self.metrics.as_ref().and_then(|m| m.duration_ms)
    }

    /// Get total token count
    pub fn total_tokens(&self) -> Option<u32> {
        self.metrics
            .as_ref()
            .and_then(|m| m.token_usage.as_ref())
            .map(|t| t.total_tokens)
    }

    /// Get estimated cost
    pub fn cost_usd(&self) -> Option<f64> {
        self.metrics.as_ref().and_then(|m| m.cost_usd)
    }
}

impl Default for NodeExecutionResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a node execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum ExecutionResult {
    /// Node hasn't executed yet
    Pending,

    /// Node executed successfully with output
    Success(serde_json::Value),

    /// Node execution failed
    Failure(String),

    /// Node execution was skipped (e.g., conditional branch not taken)
    Skipped,
}

/// Checkpoint for resumable execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExecutionCheckpoint {
    /// When this checkpoint was created
    pub timestamp: DateTime<Utc>,

    /// Nodes that have been completed
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<String>))]
    pub completed_nodes: Vec<NodeId>,

    /// Variables at checkpoint time
    pub variables: HashMap<String, serde_json::Value>,

    /// Execution state at checkpoint
    pub state: ExecutionState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_context() {
        let workflow_id = Uuid::new_v4();
        let mut ctx = ExecutionContext::new(workflow_id);

        let node_id = Uuid::new_v4();
        let result = NodeExecutionResult::new().complete(ExecutionResult::Success(
            serde_json::json!({"output": "test"}),
        ));

        ctx.record_node_result(node_id, result);

        assert!(ctx.get_node_result(&node_id).is_some());
        assert_eq!(ctx.state, ExecutionState::Running);
    }

    #[test]
    fn test_execution_context_new() {
        let workflow_id = Uuid::new_v4();
        let ctx = ExecutionContext::new(workflow_id);

        assert_eq!(ctx.workflow_id, workflow_id);
        assert_eq!(ctx.state, ExecutionState::Running);
        assert_eq!(ctx.node_results.len(), 0);
        assert_eq!(ctx.variables.len(), 0);
        assert!(ctx.completed_at.is_none());
        assert!(ctx.checkpoint.is_none());
    }

    #[test]
    fn test_execution_context_pause_resume() {
        let workflow_id = Uuid::new_v4();
        let mut ctx = ExecutionContext::new(workflow_id);

        assert_eq!(ctx.state, ExecutionState::Running);
        assert!(!ctx.can_resume());

        ctx.pause();
        assert_eq!(ctx.state, ExecutionState::Paused);
        assert!(ctx.can_resume());
        assert!(ctx.checkpoint.is_some());

        ctx.resume();
        assert_eq!(ctx.state, ExecutionState::Running);
    }

    #[test]
    fn test_execution_context_cancel() {
        let workflow_id = Uuid::new_v4();
        let mut ctx = ExecutionContext::new(workflow_id);

        ctx.cancel();
        assert_eq!(ctx.state, ExecutionState::Cancelled);
        assert!(ctx.completed_at.is_some());
    }

    #[test]
    fn test_execution_context_mark_completed() {
        let workflow_id = Uuid::new_v4();
        let mut ctx = ExecutionContext::new(workflow_id);

        assert!(ctx.completed_at.is_none());

        ctx.mark_completed();
        assert!(ctx.completed_at.is_some());

        let first_completion = ctx.completed_at.unwrap();
        ctx.mark_completed(); // Should not update
        assert_eq!(ctx.completed_at.unwrap(), first_completion);
    }

    #[test]
    fn test_execution_context_variables() {
        let workflow_id = Uuid::new_v4();
        let mut ctx = ExecutionContext::new(workflow_id);

        ctx.set_variable("key1".to_string(), serde_json::json!("value1"));
        ctx.set_variable("key2".to_string(), serde_json::json!(42));

        assert_eq!(ctx.get_variable("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(ctx.get_variable("key2"), Some(&serde_json::json!(42)));
        assert_eq!(ctx.get_variable("key3"), None);
    }

    #[test]
    fn test_execution_context_checkpoint() {
        let workflow_id = Uuid::new_v4();
        let mut ctx = ExecutionContext::new(workflow_id);

        ctx.set_variable("var1".to_string(), serde_json::json!("test"));

        let checkpoint = ctx.create_checkpoint();

        assert_eq!(checkpoint.variables.len(), 1);
        assert_eq!(checkpoint.state, ExecutionState::Running);
        assert!(ctx.checkpoint.is_some());
    }

    #[test]
    fn test_execution_context_resume_from_checkpoint() {
        let workflow_id = Uuid::new_v4();
        let mut original_ctx = ExecutionContext::new(workflow_id);

        original_ctx.set_variable("var1".to_string(), serde_json::json!("test"));
        let checkpoint = original_ctx.create_checkpoint();

        let resumed_ctx = ExecutionContext::resume_from_checkpoint(checkpoint, workflow_id);

        assert_eq!(resumed_ctx.workflow_id, workflow_id);
        assert_eq!(resumed_ctx.variables.len(), 1);
        assert_eq!(
            resumed_ctx.get_variable("var1"),
            Some(&serde_json::json!("test"))
        );
    }

    #[test]
    fn test_node_execution_result_new() {
        let result = NodeExecutionResult::new();

        assert_eq!(result.retry_count, 0);
        assert!(result.completed_at.is_none());
        assert!(result.metrics.is_none());
        assert_eq!(result.result, ExecutionResult::Pending);
    }

    #[test]
    fn test_node_execution_result_complete() {
        let result = NodeExecutionResult::new().complete(ExecutionResult::Success(
            serde_json::json!({"data": "test"}),
        ));

        assert!(result.completed_at.is_some());
        assert!(matches!(result.result, ExecutionResult::Success(_)));
    }

    #[test]
    fn test_node_execution_result_with_metrics() {
        let metrics = NodeMetrics {
            duration_ms: Some(100),
            token_usage: Some(TokenUsage {
                input_tokens: 50,
                output_tokens: 30,
                total_tokens: 80,
                cached_tokens: None,
            }),
            cost_usd: Some(0.001),
            api_calls: 1,
            bytes_transferred: 1024,
            memory_bytes: Some(128),
            custom: Default::default(),
        };

        let result = NodeExecutionResult::new().with_metrics(metrics.clone());

        assert!(result.metrics.is_some());
        let result_metrics = result.metrics.unwrap();
        assert_eq!(result_metrics.duration_ms, Some(100));
        assert_eq!(result_metrics.cost_usd, Some(0.001));
        assert_eq!(result_metrics.api_calls, 1);
        assert_eq!(result_metrics.bytes_transferred, 1024);
    }

    #[test]
    fn test_execution_result_variants() {
        assert!(matches!(ExecutionResult::Pending, ExecutionResult::Pending));
        assert!(matches!(
            ExecutionResult::Success(serde_json::json!(null)),
            ExecutionResult::Success(_)
        ));
        assert!(matches!(
            ExecutionResult::Failure("test".to_string()),
            ExecutionResult::Failure(_)
        ));
        assert!(matches!(ExecutionResult::Skipped, ExecutionResult::Skipped));
    }

    #[test]
    fn test_execution_state_variants() {
        assert_eq!(ExecutionState::Running, ExecutionState::Running);
        assert_eq!(ExecutionState::Completed, ExecutionState::Completed);
        assert_eq!(ExecutionState::Cancelled, ExecutionState::Cancelled);
        assert_eq!(ExecutionState::Paused, ExecutionState::Paused);
        assert_eq!(
            ExecutionState::Failed("error".to_string()),
            ExecutionState::Failed("error".to_string())
        );
    }

    #[test]
    fn test_token_usage() {
        let token_usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cached_tokens: None,
        };

        assert_eq!(token_usage.input_tokens, 100);
        assert_eq!(token_usage.output_tokens, 50);
        assert_eq!(token_usage.total_tokens, 150);
        assert_eq!(token_usage.cached_tokens, None);
    }

    #[test]
    fn test_node_metrics_default() {
        let metrics = NodeMetrics::default();

        assert_eq!(metrics.duration_ms, None);
        assert_eq!(metrics.token_usage, None);
        assert_eq!(metrics.cost_usd, None);
        assert_eq!(metrics.api_calls, 0);
        assert_eq!(metrics.bytes_transferred, 0);
        assert_eq!(metrics.memory_bytes, None);
    }
}
