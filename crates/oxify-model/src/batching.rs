//! Intelligent node execution batching
//!
//! This module provides analysis and strategies for batching node executions
//! to improve workflow performance through parallelization.

use crate::{Node, NodeId, NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Batch execution plan for a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPlan {
    /// Execution batches in topological order
    pub batches: Vec<ExecutionBatch>,

    /// Total number of nodes
    pub total_nodes: usize,

    /// Maximum parallelism (largest batch size)
    pub max_parallelism: usize,

    /// Estimated speedup factor
    pub speedup_factor: f64,

    /// Batch statistics
    pub stats: BatchStats,
}

/// A batch of nodes that can be executed in parallel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBatch {
    /// Batch level (0-indexed)
    pub level: usize,

    /// Nodes in this batch
    pub nodes: Vec<NodeId>,

    /// Estimated execution time for this batch (ms)
    pub estimated_time_ms: u64,

    /// Whether this batch can benefit from parallel execution
    pub parallelizable: bool,
}

/// Statistics about batching strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStats {
    /// Number of batches
    pub batch_count: usize,

    /// Average batch size
    pub avg_batch_size: f64,

    /// Number of sequential-only batches
    pub sequential_batches: usize,

    /// Number of parallel batches
    pub parallel_batches: usize,

    /// Parallelization efficiency (0.0 to 1.0)
    pub efficiency: f64,
}

/// Node batching analyzer
pub struct BatchAnalyzer;

impl BatchAnalyzer {
    /// Analyze a workflow and generate a batch execution plan
    pub fn analyze(workflow: &Workflow) -> BatchPlan {
        // Build dependency graph
        let dependencies = Self::build_dependency_graph(workflow);

        // Compute in-degrees for topological sorting
        let in_degrees = Self::compute_in_degrees(workflow, &dependencies);

        // Generate batches using level-based topological sort
        let batches = Self::generate_batches(workflow, &dependencies, in_degrees);

        // Calculate statistics
        let stats = Self::calculate_stats(&batches);

        // Calculate speedup factor
        let speedup_factor = Self::calculate_speedup(&batches, workflow.nodes.len());

        // Find max parallelism
        let max_parallelism = batches.iter().map(|b| b.nodes.len()).max().unwrap_or(0);

        BatchPlan {
            total_nodes: workflow.nodes.len(),
            max_parallelism,
            speedup_factor,
            batches,
            stats,
        }
    }

    /// Build dependency graph (node -> list of nodes that depend on it)
    fn build_dependency_graph(workflow: &Workflow) -> HashMap<NodeId, Vec<NodeId>> {
        let mut graph: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        // Initialize with all nodes
        for node in &workflow.nodes {
            graph.entry(node.id).or_default();
        }

        // Add edges
        for edge in &workflow.edges {
            graph.entry(edge.from).or_default().push(edge.to);
        }

        graph
    }

    /// Compute in-degrees for each node
    fn compute_in_degrees(
        workflow: &Workflow,
        dependencies: &HashMap<NodeId, Vec<NodeId>>,
    ) -> HashMap<NodeId, usize> {
        let mut in_degrees: HashMap<NodeId, usize> = HashMap::new();

        // Initialize all nodes with 0 in-degree
        for node in &workflow.nodes {
            in_degrees.insert(node.id, 0);
        }

        // Count incoming edges
        for children in dependencies.values() {
            for &child_id in children {
                *in_degrees.entry(child_id).or_insert(0) += 1;
            }
        }

        in_degrees
    }

    /// Generate execution batches using level-based topological sort
    fn generate_batches(
        workflow: &Workflow,
        dependencies: &HashMap<NodeId, Vec<NodeId>>,
        mut in_degrees: HashMap<NodeId, usize>,
    ) -> Vec<ExecutionBatch> {
        let mut batches = Vec::new();
        let mut processed = HashSet::new();
        let mut current_level = 0;

        // Create a map for quick node lookup
        let node_map: HashMap<NodeId, &Node> = workflow.nodes.iter().map(|n| (n.id, n)).collect();

        while processed.len() < workflow.nodes.len() {
            // Find all nodes with in-degree 0 (ready to execute)
            let ready_nodes: Vec<NodeId> = in_degrees
                .iter()
                .filter(|(&id, &degree)| degree == 0 && !processed.contains(&id))
                .map(|(&id, _)| id)
                .collect();

            if ready_nodes.is_empty() {
                // No more nodes to process (shouldn't happen with valid DAG)
                break;
            }

            // Estimate time for this batch (max of all node times)
            let estimated_time_ms = ready_nodes
                .iter()
                .filter_map(|id| node_map.get(id))
                .map(|node| Self::estimate_node_time(node))
                .max()
                .unwrap_or(100);

            // Check if batch is parallelizable
            let parallelizable = ready_nodes.len() > 1
                && ready_nodes.iter().all(|id| {
                    if let Some(node) = node_map.get(id) {
                        Self::is_parallelizable(node)
                    } else {
                        false
                    }
                });

            batches.push(ExecutionBatch {
                level: current_level,
                nodes: ready_nodes.clone(),
                estimated_time_ms,
                parallelizable,
            });

            // Mark nodes as processed and update in-degrees
            for &node_id in &ready_nodes {
                processed.insert(node_id);
                in_degrees.remove(&node_id);

                // Reduce in-degree of children
                if let Some(children) = dependencies.get(&node_id) {
                    for &child_id in children {
                        if let Some(degree) = in_degrees.get_mut(&child_id) {
                            *degree = degree.saturating_sub(1);
                        }
                    }
                }
            }

            current_level += 1;
        }

        batches
    }

    /// Estimate execution time for a node (simplified)
    fn estimate_node_time(node: &Node) -> u64 {
        match &node.kind {
            NodeKind::Start | NodeKind::End => 10,
            NodeKind::LLM(_) => 3000,
            NodeKind::Retriever(_) => 500,
            NodeKind::Code(_) => 1000,
            NodeKind::Tool(_) => 2000,
            NodeKind::IfElse(_) | NodeKind::Switch(_) => 50,
            NodeKind::Loop(_) => 100,
            NodeKind::TryCatch(_) => 100,
            NodeKind::SubWorkflow(_) => 5000,
            NodeKind::Parallel(_) => 200,
            NodeKind::Approval(_) => 60000,
            NodeKind::Form(_) => 120000,
            NodeKind::Vision(_) => 3000,
            // Plugin execution time is unknown — use generic compute estimate
            NodeKind::Custom(_) => 1000,
        }
    }

    /// Check if a node can be safely parallelized
    fn is_parallelizable(node: &Node) -> bool {
        // Most nodes can be parallelized if they don't have data dependencies
        // Exceptions: nodes that require sequential execution or have side effects
        !matches!(
            node.kind,
            NodeKind::Approval(_) | NodeKind::Form(_) | NodeKind::Custom(_)
        )
    }

    /// Calculate batch statistics
    fn calculate_stats(batches: &[ExecutionBatch]) -> BatchStats {
        let batch_count = batches.len();

        let total_nodes: usize = batches.iter().map(|b| b.nodes.len()).sum();
        let avg_batch_size = if batch_count > 0 {
            total_nodes as f64 / batch_count as f64
        } else {
            0.0
        };

        let sequential_batches = batches.iter().filter(|b| !b.parallelizable).count();
        let parallel_batches = batches.iter().filter(|b| b.parallelizable).count();

        // Efficiency: ratio of nodes that can run in parallel
        let parallel_nodes: usize = batches
            .iter()
            .filter(|b| b.parallelizable)
            .map(|b| b.nodes.len())
            .sum();

        let efficiency = if total_nodes > 0 {
            parallel_nodes as f64 / total_nodes as f64
        } else {
            0.0
        };

        BatchStats {
            batch_count,
            avg_batch_size,
            sequential_batches,
            parallel_batches,
            efficiency,
        }
    }

    /// Calculate estimated speedup factor from batching
    fn calculate_speedup(batches: &[ExecutionBatch], total_nodes: usize) -> f64 {
        if total_nodes == 0 {
            return 1.0;
        }

        // Sequential time: sum of all node times
        let sequential_time: u64 =
            batches.iter().flat_map(|b| b.nodes.iter()).count() as u64 * 1000; // Assume avg 1s per node

        // Parallel time: sum of batch times (max within each batch)
        let parallel_time: u64 = batches.iter().map(|b| b.estimated_time_ms).sum();

        if parallel_time > 0 {
            sequential_time as f64 / parallel_time as f64
        } else {
            1.0
        }
    }

    /// Get nodes that can be batched together
    pub fn find_batch_opportunities(workflow: &Workflow) -> Vec<BatchOpportunity> {
        let plan = Self::analyze(workflow);
        let node_map: HashMap<NodeId, &Node> = workflow.nodes.iter().map(|n| (n.id, n)).collect();

        let mut opportunities = Vec::new();

        for batch in &plan.batches {
            if batch.parallelizable && batch.nodes.len() > 1 {
                let node_names: Vec<String> = batch
                    .nodes
                    .iter()
                    .filter_map(|id| node_map.get(id).map(|n| n.name.clone()))
                    .collect();

                opportunities.push(BatchOpportunity {
                    level: batch.level,
                    node_count: batch.nodes.len(),
                    node_names,
                    estimated_speedup: batch.nodes.len() as f64 * 0.8, // Conservative estimate
                    description: format!(
                        "Level {} has {} nodes that can run in parallel",
                        batch.level,
                        batch.nodes.len()
                    ),
                });
            }
        }

        opportunities
    }
}

/// A batching opportunity identified in the workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOpportunity {
    /// Execution level
    pub level: usize,

    /// Number of nodes in batch
    pub node_count: usize,

    /// Names of nodes in batch
    pub node_names: Vec<String>,

    /// Estimated speedup from batching
    pub estimated_speedup: f64,

    /// Human-readable description
    pub description: String,
}

impl BatchPlan {
    /// Format batch plan as human-readable string
    pub fn format_summary(&self) -> String {
        format!(
            "Batch Execution Plan:\n\
             Total Nodes: {} | Batches: {} | Max Parallelism: {}\n\
             Speedup Factor: {:.2}x | Efficiency: {:.0}%\n\
             Parallel Batches: {} | Sequential Batches: {}\n\
             Average Batch Size: {:.1}",
            self.total_nodes,
            self.stats.batch_count,
            self.max_parallelism,
            self.speedup_factor,
            self.stats.efficiency * 100.0,
            self.stats.parallel_batches,
            self.stats.sequential_batches,
            self.stats.avg_batch_size
        )
    }

    /// Get the critical path (longest batch sequence)
    pub fn critical_path(&self) -> Vec<&ExecutionBatch> {
        self.batches.iter().collect()
    }

    /// Get all parallel batches
    pub fn parallel_batches(&self) -> Vec<&ExecutionBatch> {
        self.batches.iter().filter(|b| b.parallelizable).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, LlmConfig, WorkflowBuilder};

    #[test]
    fn test_linear_workflow_batching() {
        let workflow = WorkflowBuilder::new("Linear")
            .start("Start")
            .llm(
                "LLM1",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test1".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .llm(
                "LLM2",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test2".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let plan = BatchAnalyzer::analyze(&workflow);

        // Linear workflow should have 4 batches (Start, LLM1, LLM2, End)
        assert_eq!(plan.batches.len(), 4);
        assert_eq!(plan.total_nodes, 4);
        assert_eq!(plan.max_parallelism, 1); // All sequential
    }

    #[test]
    fn test_parallel_workflow_batching() {
        let mut workflow = WorkflowBuilder::new("Parallel").start("Start").build();

        let start_id = workflow.nodes[0].id;

        // Add two parallel LLM nodes
        let llm1 = Node::new(
            "LLM1".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test1".to_string(),
                temperature: None,
                max_tokens: Some(100),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        let llm2 = Node::new(
            "LLM2".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test2".to_string(),
                temperature: None,
                max_tokens: Some(100),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        let end = Node::new("End".to_string(), NodeKind::End);

        workflow.add_edge(Edge::new(start_id, llm1.id));
        workflow.add_edge(Edge::new(start_id, llm2.id));
        workflow.add_edge(Edge::new(llm1.id, end.id));
        workflow.add_edge(Edge::new(llm2.id, end.id));

        workflow.nodes.push(llm1);
        workflow.nodes.push(llm2);
        workflow.nodes.push(end);

        let plan = BatchAnalyzer::analyze(&workflow);

        // Should have 3 batches: [Start], [LLM1, LLM2], [End]
        assert_eq!(plan.batches.len(), 3);
        assert_eq!(plan.max_parallelism, 2); // LLM1 and LLM2 in parallel

        // Second batch should be parallelizable
        assert!(plan.batches[1].parallelizable);
        assert_eq!(plan.batches[1].nodes.len(), 2);
    }

    #[test]
    fn test_batch_opportunities() {
        let mut workflow = WorkflowBuilder::new("Parallel").start("Start").build();

        let start_id = workflow.nodes[0].id;

        // Add three parallel nodes
        let llm1 = Node::new(
            "LLM1".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test1".to_string(),
                temperature: None,
                max_tokens: Some(100),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        let llm2 = Node::new(
            "LLM2".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test2".to_string(),
                temperature: None,
                max_tokens: Some(100),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        let llm3 = Node::new(
            "LLM3".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test3".to_string(),
                temperature: None,
                max_tokens: Some(100),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        let end = Node::new("End".to_string(), NodeKind::End);

        workflow.add_edge(Edge::new(start_id, llm1.id));
        workflow.add_edge(Edge::new(start_id, llm2.id));
        workflow.add_edge(Edge::new(start_id, llm3.id));
        workflow.add_edge(Edge::new(llm1.id, end.id));
        workflow.add_edge(Edge::new(llm2.id, end.id));
        workflow.add_edge(Edge::new(llm3.id, end.id));

        workflow.nodes.push(llm1);
        workflow.nodes.push(llm2);
        workflow.nodes.push(llm3);
        workflow.nodes.push(end);

        let opportunities = BatchAnalyzer::find_batch_opportunities(&workflow);

        // Should find one opportunity with 3 nodes
        assert!(!opportunities.is_empty());
        assert_eq!(opportunities[0].node_count, 3);
    }

    #[test]
    fn test_batch_plan_summary() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .end("End")
            .build();

        let plan = BatchAnalyzer::analyze(&workflow);
        let summary = plan.format_summary();

        assert!(summary.contains("Batch Execution Plan"));
        assert!(summary.contains("Total Nodes: 2"));
    }

    #[test]
    fn test_speedup_calculation() {
        let mut workflow = WorkflowBuilder::new("Parallel").start("Start").build();

        let start_id = workflow.nodes[0].id;

        // Add 4 parallel nodes
        for i in 0..4 {
            let llm = Node::new(
                format!("LLM{}", i),
                NodeKind::LLM(LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: format!("test{}", i),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                }),
            );

            workflow.add_edge(Edge::new(start_id, llm.id));
            workflow.nodes.push(llm);
        }

        let end = Node::new("End".to_string(), NodeKind::End);
        for i in 1..=4 {
            workflow.add_edge(Edge::new(workflow.nodes[i].id, end.id));
        }
        workflow.nodes.push(end);

        let plan = BatchAnalyzer::analyze(&workflow);

        // Speedup should be > 1 due to parallelization
        assert!(plan.speedup_factor > 1.0);
    }

    #[test]
    fn test_parallel_batches_filter() {
        let mut workflow = WorkflowBuilder::new("Mixed").start("Start").build();

        let start_id = workflow.nodes[0].id;

        let llm1 = Node::new(
            "LLM1".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test1".to_string(),
                temperature: None,
                max_tokens: Some(100),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        let llm2 = Node::new(
            "LLM2".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test2".to_string(),
                temperature: None,
                max_tokens: Some(100),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        let end = Node::new("End".to_string(), NodeKind::End);

        workflow.add_edge(Edge::new(start_id, llm1.id));
        workflow.add_edge(Edge::new(start_id, llm2.id));
        workflow.add_edge(Edge::new(llm1.id, end.id));
        workflow.add_edge(Edge::new(llm2.id, end.id));

        workflow.nodes.push(llm1);
        workflow.nodes.push(llm2);
        workflow.nodes.push(end);

        let plan = BatchAnalyzer::analyze(&workflow);
        let parallel = plan.parallel_batches();

        // Should have at least one parallel batch
        assert!(!parallel.is_empty());
    }
}
