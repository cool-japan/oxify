//! Graceful degradation support for partial workflow execution
//!
//! This module provides utilities for handling workflow failures gracefully,
//! allowing partial results to be collected and analyzed.

use oxify_model::{ExecutionContext, ExecutionResult, NodeExecutionResult, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Configuration for graceful degradation
#[derive(Debug, Clone)]
pub struct DegradationConfig {
    /// Continue execution on non-critical node failures
    pub continue_on_failure: bool,

    /// Nodes that are considered critical (execution stops if they fail)
    pub critical_nodes: HashSet<NodeId>,

    /// Maximum number of failures before stopping
    pub max_failures: Option<usize>,

    /// Collect partial results even on failure
    pub collect_partial_results: bool,

    /// Strategy for handling failed dependencies
    pub dependency_strategy: DependencyStrategy,
}

impl Default for DegradationConfig {
    fn default() -> Self {
        Self {
            continue_on_failure: false,
            critical_nodes: HashSet::new(),
            max_failures: None,
            collect_partial_results: true,
            dependency_strategy: DependencyStrategy::Skip,
        }
    }
}

impl DegradationConfig {
    /// Create a new degradation config
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable continue on failure
    pub fn with_continue_on_failure(mut self) -> Self {
        self.continue_on_failure = true;
        self
    }

    /// Mark nodes as critical
    pub fn with_critical_nodes(mut self, nodes: HashSet<NodeId>) -> Self {
        self.critical_nodes = nodes;
        self
    }

    /// Add a single critical node
    pub fn add_critical_node(mut self, node: NodeId) -> Self {
        self.critical_nodes.insert(node);
        self
    }

    /// Set maximum failures before stopping
    pub fn with_max_failures(mut self, max: usize) -> Self {
        self.max_failures = Some(max);
        self
    }

    /// Set dependency strategy
    pub fn with_dependency_strategy(mut self, strategy: DependencyStrategy) -> Self {
        self.dependency_strategy = strategy;
        self
    }

    /// Check if a node is critical
    pub fn is_critical(&self, node_id: &NodeId) -> bool {
        self.critical_nodes.contains(node_id)
    }

    /// Check if execution should continue after a failure
    pub fn should_continue(&self, failure_count: usize) -> bool {
        if !self.continue_on_failure {
            return false;
        }

        if let Some(max) = self.max_failures {
            failure_count < max
        } else {
            true
        }
    }
}

/// Strategy for handling failed dependencies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyStrategy {
    /// Skip nodes whose dependencies failed
    Skip,
    /// Execute nodes even if dependencies failed (use default values)
    ExecuteWithDefaults,
    /// Mark as failed immediately
    FailImmediately,
}

/// Partial execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialResult {
    /// Execution context with partial results
    pub context: ExecutionContext,

    /// Nodes that succeeded
    pub successful_nodes: Vec<NodeId>,

    /// Nodes that failed
    pub failed_nodes: Vec<FailedNode>,

    /// Nodes that were skipped
    pub skipped_nodes: Vec<NodeId>,

    /// Total nodes in workflow
    pub total_nodes: usize,

    /// Completion percentage (0.0 - 100.0)
    pub completion_percentage: f64,

    /// Whether the workflow completed despite failures
    pub gracefully_degraded: bool,
}

impl PartialResult {
    /// Create from execution context
    pub fn from_context(context: ExecutionContext, total_nodes: usize) -> Self {
        let mut successful = Vec::new();
        let mut failed = Vec::new();

        for (node_id, result) in &context.node_results {
            match &result.result {
                ExecutionResult::Success(_) => successful.push(*node_id),
                ExecutionResult::Failure(error) => failed.push(FailedNode {
                    node_id: *node_id,
                    error: error.clone(),
                }),
                _ => {}
            }
        }

        let completion = (context.node_results.len() as f64 / total_nodes as f64) * 100.0;
        let gracefully_degraded = !failed.is_empty() && !successful.is_empty();

        Self {
            context,
            successful_nodes: successful.clone(),
            failed_nodes: failed,
            skipped_nodes: Vec::new(),
            total_nodes,
            completion_percentage: completion,
            gracefully_degraded,
        }
    }

    /// Get success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_nodes == 0 {
            return 0.0;
        }
        self.successful_nodes.len() as f64 / self.total_nodes as f64
    }

    /// Get failure rate (0.0 - 1.0)
    pub fn failure_rate(&self) -> f64 {
        if self.total_nodes == 0 {
            return 0.0;
        }
        self.failed_nodes.len() as f64 / self.total_nodes as f64
    }

    /// Check if any nodes failed
    pub fn has_failures(&self) -> bool {
        !self.failed_nodes.is_empty()
    }

    /// Get failed node IDs
    pub fn failed_node_ids(&self) -> Vec<NodeId> {
        self.failed_nodes.iter().map(|f| f.node_id).collect()
    }
}

/// Information about a failed node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedNode {
    /// Node ID that failed
    pub node_id: NodeId,
    /// Error message
    pub error: String,
}

/// Degradation analyzer for workflow execution
pub struct DegradationAnalyzer {
    config: DegradationConfig,
}

impl DegradationAnalyzer {
    /// Create a new degradation analyzer
    pub fn new(config: DegradationConfig) -> Self {
        Self { config }
    }

    /// Analyze execution context for partial results
    pub fn analyze(&self, context: &ExecutionContext, total_nodes: usize) -> PartialResult {
        PartialResult::from_context(context.clone(), total_nodes)
    }

    /// Check if a node's dependencies are satisfied
    pub fn dependencies_satisfied(
        &self,
        _node_id: &NodeId,
        dependencies: &[NodeId],
        context: &ExecutionContext,
    ) -> bool {
        for dep in dependencies {
            if let Some(result) = context.get_node_result(dep) {
                match result.result {
                    ExecutionResult::Success(_) => continue,
                    ExecutionResult::Failure(_) => {
                        // Dependency failed - check strategy
                        match self.config.dependency_strategy {
                            DependencyStrategy::Skip => return false,
                            DependencyStrategy::ExecuteWithDefaults => continue,
                            DependencyStrategy::FailImmediately => return false,
                        }
                    }
                    _ => return false,
                }
            } else {
                // Dependency not executed yet
                return false;
            }
        }
        true
    }

    /// Create a fallback result for skipped nodes
    pub fn create_fallback_result(&self, _reason: &str) -> NodeExecutionResult {
        NodeExecutionResult::new().complete(ExecutionResult::Skipped)
    }

    /// Get degradation statistics
    pub fn get_stats(&self, partial: &PartialResult) -> DegradationStats {
        DegradationStats {
            total_nodes: partial.total_nodes,
            successful_nodes: partial.successful_nodes.len(),
            failed_nodes: partial.failed_nodes.len(),
            skipped_nodes: partial.skipped_nodes.len(),
            completion_percentage: partial.completion_percentage,
            success_rate: partial.success_rate(),
            failure_rate: partial.failure_rate(),
            gracefully_degraded: partial.gracefully_degraded,
        }
    }
}

/// Statistics about graceful degradation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationStats {
    /// Total nodes in workflow
    pub total_nodes: usize,
    /// Number of successful nodes
    pub successful_nodes: usize,
    /// Number of failed nodes
    pub failed_nodes: usize,
    /// Number of skipped nodes
    pub skipped_nodes: usize,
    /// Completion percentage
    pub completion_percentage: f64,
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
    /// Failure rate (0.0 - 1.0)
    pub failure_rate: f64,
    /// Whether graceful degradation occurred
    pub gracefully_degraded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_degradation_config_default() {
        let config = DegradationConfig::default();
        assert!(!config.continue_on_failure);
        assert!(config.critical_nodes.is_empty());
        assert!(config.max_failures.is_none());
        assert!(config.collect_partial_results);
    }

    #[test]
    fn test_degradation_config_builder() {
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        let config = DegradationConfig::new()
            .with_continue_on_failure()
            .add_critical_node(node1)
            .add_critical_node(node2)
            .with_max_failures(5)
            .with_dependency_strategy(DependencyStrategy::ExecuteWithDefaults);

        assert!(config.continue_on_failure);
        assert_eq!(config.critical_nodes.len(), 2);
        assert!(config.is_critical(&node1));
        assert!(config.is_critical(&node2));
        assert_eq!(config.max_failures, Some(5));
    }

    #[test]
    fn test_degradation_config_should_continue() {
        let config = DegradationConfig::new()
            .with_continue_on_failure()
            .with_max_failures(3);

        assert!(config.should_continue(0));
        assert!(config.should_continue(2));
        assert!(!config.should_continue(3));
        assert!(!config.should_continue(5));
    }

    #[test]
    fn test_partial_result_from_context() {
        let workflow_id = Uuid::new_v4();
        let mut context = ExecutionContext::new(workflow_id);

        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        // Add successful result
        context.record_node_result(
            node1,
            NodeExecutionResult::new().complete(ExecutionResult::Success(serde_json::json!({}))),
        );

        // Add failed result
        context.record_node_result(
            node2,
            NodeExecutionResult::new().complete(ExecutionResult::Failure("error".to_string())),
        );

        let partial = PartialResult::from_context(context, 3);

        assert_eq!(partial.successful_nodes.len(), 1);
        assert_eq!(partial.failed_nodes.len(), 1);
        assert!(partial.gracefully_degraded);
        assert!((partial.completion_percentage - 66.66).abs() < 1.0);
    }

    #[test]
    fn test_partial_result_success_rate() {
        let workflow_id = Uuid::new_v4();
        let mut context = ExecutionContext::new(workflow_id);

        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        context.record_node_result(
            node1,
            NodeExecutionResult::new().complete(ExecutionResult::Success(serde_json::json!({}))),
        );

        context.record_node_result(
            node2,
            NodeExecutionResult::new().complete(ExecutionResult::Failure("error".to_string())),
        );

        let partial = PartialResult::from_context(context, 4);

        assert!((partial.success_rate() - 0.25).abs() < 0.01);
        assert!((partial.failure_rate() - 0.25).abs() < 0.01);
    }

    #[test]
    fn test_degradation_analyzer() {
        let config = DegradationConfig::new().with_continue_on_failure();
        let analyzer = DegradationAnalyzer::new(config);

        let workflow_id = Uuid::new_v4();
        let context = ExecutionContext::new(workflow_id);

        let partial = analyzer.analyze(&context, 5);
        assert_eq!(partial.total_nodes, 5);
        assert_eq!(partial.successful_nodes.len(), 0);
    }

    #[test]
    fn test_degradation_stats() {
        let config = DegradationConfig::new();
        let analyzer = DegradationAnalyzer::new(config);

        let workflow_id = Uuid::new_v4();
        let mut context = ExecutionContext::new(workflow_id);

        let node1 = Uuid::new_v4();
        context.record_node_result(
            node1,
            NodeExecutionResult::new().complete(ExecutionResult::Success(serde_json::json!({}))),
        );

        let partial = analyzer.analyze(&context, 2);
        let stats = analyzer.get_stats(&partial);

        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.successful_nodes, 1);
        assert!((stats.success_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_dependency_strategy() {
        assert_eq!(DependencyStrategy::Skip, DependencyStrategy::Skip);
        assert_ne!(
            DependencyStrategy::Skip,
            DependencyStrategy::ExecuteWithDefaults
        );
    }

    #[test]
    fn test_failed_node_ids() {
        let workflow_id = Uuid::new_v4();
        let mut context = ExecutionContext::new(workflow_id);

        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        context.record_node_result(
            node1,
            NodeExecutionResult::new().complete(ExecutionResult::Failure("error1".to_string())),
        );

        context.record_node_result(
            node2,
            NodeExecutionResult::new().complete(ExecutionResult::Failure("error2".to_string())),
        );

        let partial = PartialResult::from_context(context, 2);
        let failed_ids = partial.failed_node_ids();

        assert_eq!(failed_ids.len(), 2);
        assert!(failed_ids.contains(&node1));
        assert!(failed_ids.contains(&node2));
    }
}
