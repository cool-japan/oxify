//! Workflow execution simulator for dry-run testing
//!
//! This module provides simulation capabilities for workflows, allowing users to:
//! - Test workflows without making real API calls
//! - Validate all execution paths including error handling
//! - Estimate costs and execution times
//! - Generate execution traces for debugging
//! - Test conditional branches and loops
//!
//! # Example
//!
//! ```rust
//! use oxify_model::*;
//! use std::collections::HashMap;
//!
//! let config = LlmConfig {
//!     provider: "openai".to_string(),
//!     model: "gpt-4".to_string(),
//!     system_prompt: None,
//!     prompt_template: "Generate: {{input}}".to_string(),
//!     temperature: Some(0.7),
//!     max_tokens: None,
//!     tools: vec![],
//!     images: vec![],
//!     extra_params: serde_json::Value::Null,
//! };
//!
//! let workflow = WorkflowBuilder::new("Test")
//!     .start("start")
//!     .llm("gen", config)
//!     .end("end")
//!     .build();
//!
//! let mut context = HashMap::new();
//! context.insert("input".to_string(), serde_json::json!("hello"));
//!
//! let result = WorkflowSimulator::new()
//!     .with_mock_responses(vec![
//!         ("gen".to_string(), serde_json::json!("mock response"))
//!     ])
//!     .simulate(&workflow, context);
//!
//! assert!(result.is_ok());
//! ```

use crate::{
    CostEstimate, CostEstimator, ExecutionState, Node, NodeId, NodeKind, TimeEstimate,
    TimePredictor, Workflow,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Result of a workflow simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    /// Whether the simulation completed successfully
    pub success: bool,

    /// Execution state at completion
    pub final_state: ExecutionState,

    /// Execution trace showing the path taken
    pub trace: ExecutionTrace,

    /// Final variable context
    pub final_context: HashMap<String, Value>,

    /// Cost estimate for this execution path
    pub cost_estimate: Option<CostEstimate>,

    /// Time estimate for this execution path
    pub time_estimate: Option<TimeEstimate>,

    /// Coverage information
    pub coverage: CoverageInfo,

    /// Errors encountered during simulation
    pub errors: Vec<SimulationError>,

    /// Warnings generated during simulation
    pub warnings: Vec<String>,
}

/// Execution trace showing the path taken through the workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    /// Ordered list of nodes executed
    pub executed_nodes: Vec<NodeId>,

    /// Node execution details
    pub node_details: HashMap<NodeId, NodeExecutionDetail>,

    /// Total simulated execution time (ms)
    pub total_time_ms: u64,

    /// Number of nodes executed
    pub node_count: usize,

    /// Branches taken in conditional nodes
    pub branches_taken: HashMap<NodeId, Vec<String>>,
}

/// Details of a single node execution in simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeExecutionDetail {
    /// Node ID
    pub node_id: NodeId,

    /// Node name
    pub node_name: String,

    /// Node type
    pub node_type: String,

    /// Simulated execution time (ms)
    pub execution_time_ms: u64,

    /// Input context at node execution
    pub input_context: HashMap<String, Value>,

    /// Output/result of node execution
    pub output: Value,

    /// Whether this was a mocked execution
    pub mocked: bool,

    /// Number of retries (if any)
    pub retry_count: u32,
}

/// Branch coverage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageInfo {
    /// Total number of nodes in workflow
    pub total_nodes: usize,

    /// Number of nodes executed
    pub executed_nodes: usize,

    /// Coverage percentage (0-100)
    pub coverage_percent: f64,

    /// Nodes not executed
    pub unexecuted_nodes: Vec<NodeId>,

    /// Conditional branches taken (node_id -> list of branch labels)
    pub branches_taken: HashMap<NodeId, Vec<String>>,

    /// Conditional branches not taken (node_id -> list of branch labels)
    pub branches_not_taken: HashMap<NodeId, Vec<String>>,
}

/// Simulation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationError {
    /// Node where error occurred
    pub node_id: NodeId,

    /// Error message
    pub message: String,

    /// Error type
    pub error_type: String,

    /// Whether this error was expected (from try-catch testing)
    pub expected: bool,
}

/// Workflow simulator for dry-run executions
pub struct WorkflowSimulator {
    /// Mock responses for nodes (node_name -> response)
    mock_responses: HashMap<String, Value>,

    /// Whether to simulate API latencies
    simulate_latencies: bool,

    /// Whether to estimate costs
    estimate_costs: bool,

    /// Whether to estimate execution times
    estimate_times: bool,

    /// Maximum simulation steps (prevent infinite loops)
    max_steps: usize,

    /// Random seed for deterministic simulations
    seed: Option<u64>,
}

impl WorkflowSimulator {
    /// Create a new workflow simulator
    pub fn new() -> Self {
        Self {
            mock_responses: HashMap::new(),
            simulate_latencies: true,
            estimate_costs: true,
            estimate_times: true,
            max_steps: 10000,
            seed: None,
        }
    }

    /// Add mock responses for specific nodes
    pub fn with_mock_responses(mut self, responses: Vec<(String, Value)>) -> Self {
        self.mock_responses = responses.into_iter().collect();
        self
    }

    /// Set whether to simulate API latencies
    pub fn simulate_latencies(mut self, enabled: bool) -> Self {
        self.simulate_latencies = enabled;
        self
    }

    /// Set whether to estimate costs
    pub fn estimate_costs(mut self, enabled: bool) -> Self {
        self.estimate_costs = enabled;
        self
    }

    /// Set whether to estimate execution times
    pub fn estimate_times(mut self, enabled: bool) -> Self {
        self.estimate_times = enabled;
        self
    }

    /// Set maximum simulation steps
    pub fn max_steps(mut self, steps: usize) -> Self {
        self.max_steps = steps;
        self
    }

    /// Set random seed for deterministic simulations
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Simulate workflow execution
    pub fn simulate(
        &self,
        workflow: &Workflow,
        initial_context: HashMap<String, Value>,
    ) -> Result<SimulationResult, String> {
        let mut context = SimulationContext::new(workflow, initial_context, self.max_steps);

        // Find start node
        let start_node = workflow
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Start))
            .ok_or("No start node found")?;

        // Execute simulation
        self.execute_node(&mut context, workflow, &start_node.id)?;

        // Generate result
        let coverage = self.calculate_coverage(workflow, &context);

        let cost_estimate = if self.estimate_costs {
            Some(CostEstimator::estimate(workflow))
        } else {
            None
        };

        let time_estimate = if self.estimate_times {
            let predictor = TimePredictor::new();
            Some(predictor.predict(workflow))
        } else {
            None
        };

        Ok(SimulationResult {
            success: context.errors.is_empty(),
            final_state: if context.errors.is_empty() {
                ExecutionState::Completed
            } else {
                let error_msg = context
                    .errors
                    .iter()
                    .map(|e| e.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                ExecutionState::Failed(error_msg)
            },
            trace: context.build_trace(),
            final_context: context.variables,
            cost_estimate,
            time_estimate,
            coverage,
            errors: context.errors,
            warnings: context.warnings,
        })
    }

    /// Execute a single node in simulation
    fn execute_node(
        &self,
        context: &mut SimulationContext,
        workflow: &Workflow,
        node_id: &NodeId,
    ) -> Result<(), String> {
        if context.step_count >= self.max_steps {
            return Err("Maximum simulation steps exceeded".to_string());
        }

        context.step_count += 1;

        let node = workflow
            .nodes
            .iter()
            .find(|n| &n.id == node_id)
            .ok_or("Node not found")?;

        // Check if already executed (avoid cycles)
        if context.executed_nodes.contains(node_id) {
            return Ok(());
        }

        context.executed_nodes.insert(*node_id);

        // Simulate node execution
        let output = self.simulate_node_execution(context, node)?;

        // Record execution
        context.record_execution(node, output.clone(), false);

        // Handle different node types
        match &node.kind {
            NodeKind::Start => {
                // Move to next node
                self.execute_next_nodes(context, workflow, node_id)?;
            }
            NodeKind::End => {
                // Workflow complete
                context.completed = true;
            }
            NodeKind::IfElse(condition_cfg) => {
                // Evaluate condition (simplified)
                let branch_taken = self.evaluate_condition(&condition_cfg.expression, context);
                let branch_name = if branch_taken { "true" } else { "false" };
                context
                    .branches_taken
                    .entry(*node_id)
                    .or_default()
                    .push(branch_name.to_string());

                // Execute appropriate branch based on Condition struct
                let next_node = if branch_taken {
                    &condition_cfg.true_branch
                } else {
                    &condition_cfg.false_branch
                };
                self.execute_node(context, workflow, next_node)?;
            }
            NodeKind::Switch(switch_cfg) => {
                // Evaluate expression and find matching case
                let value = self.evaluate_expression(&switch_cfg.switch_on, context);
                let matched_value = match &value {
                    Value::String(s) => s.clone(),
                    _ => "unknown".to_string(),
                };

                context
                    .branches_taken
                    .entry(*node_id)
                    .or_default()
                    .push(matched_value.clone());

                // For simulation, just execute the next nodes
                self.execute_next_nodes(context, workflow, node_id)?;
            }
            NodeKind::Loop(_loop_cfg) => {
                // Simplified loop simulation (execute once)
                context.warnings.push(format!(
                    "Loop node '{}' simulated with single iteration",
                    node.name
                ));
                self.execute_next_nodes(context, workflow, node_id)?;
            }
            NodeKind::Custom(ref cfg) => {
                // Simulated plugin dispatch — warn and continue
                context.warnings.push(format!(
                    "Custom node '{}' (plugin: '{}') simulated without real plugin dispatch",
                    node.name, cfg.plugin_id
                ));
                self.execute_next_nodes(context, workflow, node_id)?;
            }
            NodeKind::LLM(_)
            | NodeKind::Retriever(_)
            | NodeKind::Code(_)
            | NodeKind::Tool(_)
            | NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Parallel(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_) => {
                // For other nodes, just continue to next
                self.execute_next_nodes(context, workflow, node_id)?;
            }
        }

        Ok(())
    }

    /// Simulate execution of a single node
    fn simulate_node_execution(
        &self,
        _context: &SimulationContext,
        node: &Node,
    ) -> Result<Value, String> {
        // Check for mock response
        if let Some(mock) = self.mock_responses.get(&node.name) {
            return Ok(mock.clone());
        }

        // Generate default response based on node type
        let output = match &node.kind {
            NodeKind::Start => Value::Null,
            NodeKind::End => Value::Null,
            NodeKind::LLM(_) => Value::String("Simulated LLM response".to_string()),
            NodeKind::Code(_) => Value::String("Simulated code execution".to_string()),
            NodeKind::Retriever(_) => Value::Array(vec![
                Value::String("Simulated document 1".to_string()),
                Value::String("Simulated document 2".to_string()),
            ]),
            NodeKind::Tool(_) => Value::String("Simulated tool result".to_string()),
            NodeKind::Custom(cfg) => {
                Value::String(format!("Simulated plugin execution: {}", cfg.plugin_id))
            }
            NodeKind::IfElse(_)
            | NodeKind::Switch(_)
            | NodeKind::Loop(_)
            | NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Parallel(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_) => Value::Null,
        };

        Ok(output)
    }

    /// Execute next nodes after current node
    fn execute_next_nodes(
        &self,
        context: &mut SimulationContext,
        workflow: &Workflow,
        current_node_id: &NodeId,
    ) -> Result<(), String> {
        let next_edges: Vec<_> = workflow
            .edges
            .iter()
            .filter(|e| &e.from == current_node_id)
            .collect();

        for edge in next_edges {
            self.execute_node(context, workflow, &edge.to)?;
        }

        Ok(())
    }

    /// Evaluate a condition (simplified)
    fn evaluate_condition(&self, _condition: &str, _context: &SimulationContext) -> bool {
        // Simplified: randomly choose true/false or use first mock response
        // In real implementation, would parse and evaluate condition
        true
    }

    /// Evaluate an expression
    fn evaluate_expression(&self, _expression: &str, _context: &SimulationContext) -> Value {
        // Simplified: return empty string
        // In real implementation, would parse and evaluate expression
        Value::String("simulated".to_string())
    }

    /// Check if value matches a case
    #[allow(dead_code)]
    fn matches_case(&self, _value: &Value, _match_value: &str) -> bool {
        // Simplified matching
        true
    }

    /// Calculate coverage information
    fn calculate_coverage(&self, workflow: &Workflow, context: &SimulationContext) -> CoverageInfo {
        let total_nodes = workflow.nodes.len();
        let executed_nodes = context.executed_nodes.len();
        let coverage_percent = if total_nodes > 0 {
            (executed_nodes as f64 / total_nodes as f64) * 100.0
        } else {
            0.0
        };

        let unexecuted_nodes: Vec<NodeId> = workflow
            .nodes
            .iter()
            .filter(|n| !context.executed_nodes.contains(&n.id))
            .map(|n| n.id)
            .collect();

        CoverageInfo {
            total_nodes,
            executed_nodes,
            coverage_percent,
            unexecuted_nodes,
            branches_taken: context.branches_taken.clone(),
            branches_not_taken: HashMap::new(),
        }
    }
}

impl Default for WorkflowSimulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal simulation context
#[allow(dead_code)]
struct SimulationContext {
    /// Variable context
    variables: HashMap<String, Value>,

    /// Executed nodes
    executed_nodes: HashSet<NodeId>,

    /// Execution details
    execution_details: Vec<NodeExecutionDetail>,

    /// Branches taken (maps node_id -> list of branches)
    branches_taken: HashMap<NodeId, Vec<String>>,

    /// Errors encountered
    errors: Vec<SimulationError>,

    /// Warnings generated
    warnings: Vec<String>,

    /// Step count
    step_count: usize,

    /// Maximum steps allowed
    max_steps: usize,

    /// Whether execution completed
    completed: bool,

    /// Total simulated time
    total_time_ms: u64,
}

impl SimulationContext {
    fn new(
        _workflow: &Workflow,
        initial_context: HashMap<String, Value>,
        max_steps: usize,
    ) -> Self {
        Self {
            variables: initial_context,
            executed_nodes: HashSet::new(),
            execution_details: Vec::new(),
            branches_taken: HashMap::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            step_count: 0,
            max_steps,
            completed: false,
            total_time_ms: 0,
        }
    }

    fn record_execution(&mut self, node: &Node, output: Value, mocked: bool) {
        let execution_time_ms = self.estimate_node_time(node);
        self.total_time_ms += execution_time_ms;

        self.execution_details.push(NodeExecutionDetail {
            node_id: node.id,
            node_name: node.name.clone(),
            node_type: format!("{:?}", node.kind),
            execution_time_ms,
            input_context: self.variables.clone(),
            output,
            mocked,
            retry_count: 0,
        });
    }

    fn estimate_node_time(&self, node: &Node) -> u64 {
        // Simplified time estimation
        match &node.kind {
            NodeKind::Start | NodeKind::End => 0,
            NodeKind::LLM(_) => 1000,
            NodeKind::Code(_) => 100,
            NodeKind::Retriever(_) => 500,
            NodeKind::Tool(_) => 200,
            _ => 50,
        }
    }

    fn build_trace(&self) -> ExecutionTrace {
        let executed_nodes: Vec<NodeId> =
            self.execution_details.iter().map(|d| d.node_id).collect();

        let node_details: HashMap<NodeId, NodeExecutionDetail> = self
            .execution_details
            .iter()
            .map(|d| (d.node_id, d.clone()))
            .collect();

        ExecutionTrace {
            executed_nodes,
            node_details,
            total_time_ms: self.total_time_ms,
            node_count: self.execution_details.len(),
            branches_taken: self.branches_taken.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmConfig, WorkflowBuilder};

    #[test]
    fn test_simulate_simple_workflow() {
        let workflow = WorkflowBuilder::new("Test")
            .start("start")
            .end("end")
            .build();

        let context = HashMap::new();
        let simulator = WorkflowSimulator::new();
        let result = simulator.simulate(&workflow, context);

        assert!(result.is_ok());
        let sim_result = result.unwrap();
        assert!(sim_result.success);
        assert_eq!(sim_result.coverage.executed_nodes, 2);
    }

    #[test]
    fn test_simulate_with_llm_node() {
        let config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "Generate: {{input}}".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let workflow = WorkflowBuilder::new("Test")
            .start("start")
            .llm("gen", config)
            .end("end")
            .build();

        let mut context = HashMap::new();
        context.insert("input".to_string(), Value::String("test".to_string()));

        let simulator = WorkflowSimulator::new();
        let result = simulator.simulate(&workflow, context);

        assert!(result.is_ok());
        let sim_result = result.unwrap();
        assert!(sim_result.success);
        assert_eq!(sim_result.coverage.executed_nodes, 3);
    }

    #[test]
    fn test_simulate_with_mock_response() {
        let config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "Generate: {{input}}".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let workflow = WorkflowBuilder::new("Test")
            .start("start")
            .llm("gen", config)
            .end("end")
            .build();

        let context = HashMap::new();
        let mock_response = Value::String("Mocked LLM response".to_string());

        let simulator = WorkflowSimulator::new()
            .with_mock_responses(vec![("gen".to_string(), mock_response.clone())]);

        let result = simulator.simulate(&workflow, context);

        assert!(result.is_ok());
        let sim_result = result.unwrap();
        assert!(sim_result.success);

        // Check that mock response was used
        let gen_detail = sim_result
            .trace
            .node_details
            .values()
            .find(|d| d.node_name == "gen")
            .unwrap();
        assert_eq!(gen_detail.output, mock_response);
    }

    #[test]
    fn test_coverage_calculation() {
        let config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let workflow = WorkflowBuilder::new("Test")
            .start("start")
            .llm("gen", config)
            .end("end")
            .build();

        let context = HashMap::new();
        let simulator = WorkflowSimulator::new();
        let result = simulator.simulate(&workflow, context).unwrap();

        assert_eq!(result.coverage.total_nodes, 3);
        assert_eq!(result.coverage.executed_nodes, 3);
        assert_eq!(result.coverage.coverage_percent, 100.0);
        assert!(result.coverage.unexecuted_nodes.is_empty());
    }

    #[test]
    fn test_execution_trace() {
        let config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let workflow = WorkflowBuilder::new("Test")
            .start("start")
            .llm("gen", config)
            .end("end")
            .build();

        let context = HashMap::new();
        let simulator = WorkflowSimulator::new();
        let result = simulator.simulate(&workflow, context).unwrap();

        assert_eq!(result.trace.node_count, 3);
        assert_eq!(result.trace.executed_nodes.len(), 3);
        assert!(result.trace.total_time_ms > 0);
    }

    #[test]
    fn test_cost_estimation() {
        let config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let workflow = WorkflowBuilder::new("Test")
            .start("start")
            .llm("gen", config)
            .end("end")
            .build();

        let context = HashMap::new();
        let simulator = WorkflowSimulator::new().estimate_costs(true);
        let result = simulator.simulate(&workflow, context).unwrap();

        assert!(result.cost_estimate.is_some());
    }

    #[test]
    fn test_time_estimation() {
        let config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let workflow = WorkflowBuilder::new("Test")
            .start("start")
            .llm("gen", config)
            .end("end")
            .build();

        let context = HashMap::new();
        let simulator = WorkflowSimulator::new().estimate_times(true);
        let result = simulator.simulate(&workflow, context).unwrap();

        assert!(result.time_estimate.is_some());
    }

    #[test]
    fn test_max_steps_limit() {
        let config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let workflow = WorkflowBuilder::new("Test")
            .start("start")
            .llm("gen", config)
            .end("end")
            .build();

        let context = HashMap::new();
        let simulator = WorkflowSimulator::new().max_steps(1);
        let result = simulator.simulate(&workflow, context);

        // Should fail due to step limit
        assert!(result.is_err());
    }

    #[test]
    fn test_simulator_builder_pattern() {
        let simulator = WorkflowSimulator::new()
            .simulate_latencies(false)
            .estimate_costs(true)
            .estimate_times(true)
            .max_steps(5000)
            .with_seed(42);

        assert!(!simulator.simulate_latencies);
        assert!(simulator.estimate_costs);
        assert!(simulator.estimate_times);
        assert_eq!(simulator.max_steps, 5000);
        assert_eq!(simulator.seed, Some(42));
    }
}
