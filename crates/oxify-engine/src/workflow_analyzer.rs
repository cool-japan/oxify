//! Workflow optimization and analysis
//!
//! This module provides tools for analyzing and optimizing workflows:
//! - Structural analysis and validation
//! - Optimization recommendations
//! - Workflow complexity metrics
//! - DAG simplification

use oxify_model::{NodeId, NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Optimization recommendation for a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    /// Type of optimization
    pub optimization_type: OptimizationType,

    /// Description
    pub description: String,

    /// Expected improvement (0.0-1.0)
    pub expected_improvement: f64,

    /// Priority (higher is more important)
    pub priority: u8,

    /// Affected nodes
    pub affected_nodes: Vec<NodeId>,
}

/// Type of optimization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptimizationType {
    /// Remove redundant nodes
    RemoveRedundantNodes,

    /// Merge sequential nodes
    MergeSequentialNodes,

    /// Increase parallelism
    IncreaseParallelism,

    /// Add caching
    AddCaching,

    /// Batch similar operations
    BatchOperations,

    /// Simplify conditional logic
    SimplifyConditionals,

    /// Remove dead code
    RemoveDeadCode,
}

/// Workflow complexity metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityMetrics {
    /// Number of nodes
    pub node_count: usize,

    /// Number of edges
    pub edge_count: usize,

    /// Maximum path length
    pub max_path_length: usize,

    /// Average fanout (edges per node)
    pub avg_fanout: f64,

    /// Cyclomatic complexity
    pub cyclomatic_complexity: usize,

    /// Number of decision points (conditionals, switches)
    pub decision_points: usize,

    /// Depth of the DAG (longest path from start to end)
    pub dag_depth: usize,

    /// Width of the DAG (maximum parallel nodes)
    pub dag_width: usize,
}

impl ComplexityMetrics {
    /// Calculate complexity score (0-100, lower is simpler)
    pub fn complexity_score(&self) -> u8 {
        let node_score = (self.node_count as f64 / 10.0).min(25.0);
        let depth_score = (self.dag_depth as f64 / 5.0).min(25.0);
        let decision_score = (self.decision_points as f64 / 3.0).min(25.0);
        let cyclomatic_score = (self.cyclomatic_complexity as f64 / 5.0).min(25.0);

        (node_score + depth_score + decision_score + cyclomatic_score) as u8
    }

    /// Get complexity level
    pub fn complexity_level(&self) -> ComplexityLevel {
        match self.complexity_score() {
            0..=25 => ComplexityLevel::Low,
            26..=50 => ComplexityLevel::Medium,
            51..=75 => ComplexityLevel::High,
            _ => ComplexityLevel::VeryHigh,
        }
    }
}

/// Complexity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplexityLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}

/// Workflow optimizer and analyzer
pub struct WorkflowAnalyzer {
    /// Enable aggressive optimizations
    pub aggressive_mode: bool,

    /// Minimum improvement threshold to suggest
    pub min_improvement_threshold: f64,
}

impl Default for WorkflowAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowAnalyzer {
    /// Create a new workflow analyzer
    pub fn new() -> Self {
        Self {
            aggressive_mode: false,
            min_improvement_threshold: 0.05, // 5% minimum improvement
        }
    }

    /// Analyze workflow and generate optimization recommendations
    pub fn analyze(&self, workflow: &Workflow) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Detect redundant nodes
        recommendations.extend(self.detect_redundant_nodes(workflow));

        // Detect opportunities for batching
        recommendations.extend(self.detect_batching_opportunities(workflow));

        // Detect opportunities for increased parallelism
        recommendations.extend(self.detect_parallelism_opportunities(workflow));

        // Detect caching opportunities
        recommendations.extend(self.detect_caching_opportunities(workflow));

        // Filter by threshold
        recommendations.retain(|r| r.expected_improvement >= self.min_improvement_threshold);

        // Sort by priority
        recommendations.sort_by(|a, b| b.priority.cmp(&a.priority));

        recommendations
    }

    /// Calculate complexity metrics for a workflow
    pub fn calculate_complexity(&self, workflow: &Workflow) -> ComplexityMetrics {
        let node_count = workflow.nodes.len();
        let edge_count = workflow.edges.len();

        let avg_fanout = if node_count > 0 {
            edge_count as f64 / node_count as f64
        } else {
            0.0
        };

        // Count decision points
        let decision_points = workflow
            .nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.kind,
                    NodeKind::IfElse(_) | NodeKind::Switch(_) | NodeKind::Loop(_)
                )
            })
            .count();

        // Cyclomatic complexity = edges - nodes + 2 * connected_components
        // For a DAG, connected_components = 1
        let cyclomatic_complexity = if node_count > 0 {
            (edge_count as i32 - node_count as i32 + 2).max(1) as usize
        } else {
            1
        };

        // Calculate DAG depth (longest path)
        let (dag_depth, dag_width) = self.calculate_dag_dimensions(workflow);

        ComplexityMetrics {
            node_count,
            edge_count,
            max_path_length: dag_depth,
            avg_fanout,
            cyclomatic_complexity,
            decision_points,
            dag_depth,
            dag_width,
        }
    }

    /// Detect redundant nodes (nodes with no effect)
    fn detect_redundant_nodes(&self, workflow: &Workflow) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Find nodes with no outgoing edges (except End node)
        let outgoing: HashMap<NodeId, usize> = {
            let mut map: HashMap<NodeId, usize> = HashMap::new();
            for edge in &workflow.edges {
                *map.entry(edge.from).or_insert(0) += 1;
            }
            map
        };

        for node in &workflow.nodes {
            if !matches!(node.kind, NodeKind::End)
                && outgoing.get(&node.id).copied().unwrap_or(0) == 0
            {
                recommendations.push(OptimizationRecommendation {
                    optimization_type: OptimizationType::RemoveDeadCode,
                    description: format!(
                        "Node '{}' has no outgoing edges and may be dead code",
                        node.name
                    ),
                    expected_improvement: 0.1,
                    priority: 7,
                    affected_nodes: vec![node.id],
                });
            }
        }

        recommendations
    }

    /// Detect batching opportunities
    fn detect_batching_opportunities(
        &self,
        workflow: &Workflow,
    ) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Group nodes by type and provider
        let mut llm_by_provider: HashMap<String, Vec<NodeId>> = HashMap::new();
        let mut retriever_by_db: HashMap<String, Vec<NodeId>> = HashMap::new();

        for node in &workflow.nodes {
            match &node.kind {
                NodeKind::LLM(config) => {
                    llm_by_provider
                        .entry(config.provider.clone())
                        .or_default()
                        .push(node.id);
                }
                NodeKind::Retriever(config) => {
                    retriever_by_db
                        .entry(config.db_type.clone())
                        .or_default()
                        .push(node.id);
                }
                _ => {}
            }
        }

        // Recommend batching if multiple nodes of same type
        for (provider, nodes) in llm_by_provider {
            if nodes.len() >= 3 {
                recommendations.push(OptimizationRecommendation {
                    optimization_type: OptimizationType::BatchOperations,
                    description: format!(
                        "Consider batching {} LLM calls to {} provider",
                        nodes.len(),
                        provider
                    ),
                    expected_improvement: (nodes.len() as f64 * 0.05).min(0.3),
                    priority: 8,
                    affected_nodes: nodes,
                });
            }
        }

        for (db_type, nodes) in retriever_by_db {
            if nodes.len() >= 3 {
                recommendations.push(OptimizationRecommendation {
                    optimization_type: OptimizationType::BatchOperations,
                    description: format!(
                        "Consider batching {} vector searches on {} database",
                        nodes.len(),
                        db_type
                    ),
                    expected_improvement: (nodes.len() as f64 * 0.08).min(0.4),
                    priority: 8,
                    affected_nodes: nodes,
                });
            }
        }

        recommendations
    }

    /// Detect parallelism opportunities
    fn detect_parallelism_opportunities(
        &self,
        workflow: &Workflow,
    ) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Build dependency graph
        let mut dependencies: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
        for edge in &workflow.edges {
            dependencies.entry(edge.to).or_default().insert(edge.from);
        }

        // Find nodes that could be parallelized
        // (nodes that depend on the same parent but not on each other)
        let mut parent_children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for edge in &workflow.edges {
            parent_children.entry(edge.from).or_default().push(edge.to);
        }

        for (_parent, children) in parent_children {
            if children.len() >= 2 {
                // Check if children are independent
                let mut independent = true;
                for i in 0..children.len() {
                    for j in (i + 1)..children.len() {
                        if let Some(deps_i) = dependencies.get(&children[i]) {
                            if deps_i.contains(&children[j]) {
                                independent = false;
                                break;
                            }
                        }
                        if let Some(deps_j) = dependencies.get(&children[j]) {
                            if deps_j.contains(&children[i]) {
                                independent = false;
                                break;
                            }
                        }
                    }
                }

                if independent {
                    recommendations.push(OptimizationRecommendation {
                        optimization_type: OptimizationType::IncreaseParallelism,
                        description: format!(
                            "{} nodes can be executed in parallel",
                            children.len()
                        ),
                        expected_improvement: (children.len() as f64 * 0.1).min(0.5),
                        priority: 9,
                        affected_nodes: children,
                    });
                }
            }
        }

        recommendations
    }

    /// Detect caching opportunities
    fn detect_caching_opportunities(&self, workflow: &Workflow) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Detect LLM nodes that might benefit from caching
        for node in &workflow.nodes {
            if let NodeKind::LLM(config) = &node.kind {
                // Recommend caching for expensive models
                if config.model.contains("gpt-4") || config.model.contains("claude-3-opus") {
                    recommendations.push(OptimizationRecommendation {
                        optimization_type: OptimizationType::AddCaching,
                        description: format!(
                            "Consider caching results for expensive model '{}' in node '{}'",
                            config.model, node.name
                        ),
                        expected_improvement: 0.2,
                        priority: 7,
                        affected_nodes: vec![node.id],
                    });
                }
            }
        }

        recommendations
    }

    /// Calculate DAG dimensions (depth and width)
    fn calculate_dag_dimensions(&self, workflow: &Workflow) -> (usize, usize) {
        // Build adjacency list
        let mut adj: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for edge in &workflow.edges {
            adj.entry(edge.from).or_default().push(edge.to);
        }

        // Find start node
        let start_node = workflow
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Start));

        if start_node.is_none() {
            return (0, 0);
        }

        // BFS to calculate levels
        let mut levels: HashMap<NodeId, usize> = HashMap::new();
        let mut queue = vec![(start_node.unwrap().id, 0)];
        levels.insert(start_node.unwrap().id, 0);

        let mut max_level = 0;
        while let Some((node_id, level)) = queue.pop() {
            max_level = max_level.max(level);

            if let Some(children) = adj.get(&node_id) {
                for &child_id in children {
                    let child_level = level + 1;
                    let entry = levels.entry(child_id).or_insert(child_level);
                    *entry = (*entry).max(child_level);
                    queue.push((child_id, child_level));
                }
            }
        }

        // Calculate width (max nodes at any level)
        let mut level_counts: HashMap<usize, usize> = HashMap::new();
        for level in levels.values() {
            *level_counts.entry(*level).or_insert(0) += 1;
        }

        let max_width = level_counts.values().copied().max().unwrap_or(0);

        (max_level + 1, max_width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, LlmConfig, Node};

    fn create_test_workflow() -> Workflow {
        let mut workflow = Workflow::new("test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let _start_id = start.id;
        workflow.add_node(start);

        let llm = Node::new(
            "LLM".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test".to_string(),
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );
        let llm_id = llm.id;
        workflow.add_node(llm);

        let end = Node::new("End".to_string(), NodeKind::End);
        let end_id = end.id;
        workflow.add_node(end);

        workflow.add_edge(Edge::new(_start_id, llm_id));
        workflow.add_edge(Edge::new(llm_id, end_id));

        workflow
    }

    #[test]
    fn test_complexity_calculation() {
        let workflow = create_test_workflow();
        let optimizer = WorkflowAnalyzer::new();
        let metrics = optimizer.calculate_complexity(&workflow);

        assert_eq!(metrics.node_count, 3);
        assert_eq!(metrics.edge_count, 2);
        assert!(metrics.complexity_score() < 100);
    }

    #[test]
    fn test_complexity_level() {
        let workflow = create_test_workflow();
        let optimizer = WorkflowAnalyzer::new();
        let metrics = optimizer.calculate_complexity(&workflow);

        assert_eq!(metrics.complexity_level(), ComplexityLevel::Low);
    }

    #[test]
    fn test_caching_recommendations() {
        let workflow = create_test_workflow();
        let optimizer = WorkflowAnalyzer::new();
        let recommendations = optimizer.analyze(&workflow);

        // Should recommend caching for GPT-4
        let caching_recs: Vec<_> = recommendations
            .iter()
            .filter(|r| r.optimization_type == OptimizationType::AddCaching)
            .collect();

        assert!(!caching_recs.is_empty());
    }

    #[test]
    fn test_batching_detection() {
        let mut workflow = Workflow::new("test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let _start_id = start.id;
        workflow.add_node(start);

        // Add 3 LLM nodes with same provider
        for i in 0..3 {
            let llm = Node::new(
                format!("LLM {}", i),
                NodeKind::LLM(LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-3.5-turbo".to_string(),
                    system_prompt: None,
                    prompt_template: format!("test {}", i),
                    temperature: Some(0.7),
                    max_tokens: Some(100),
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                }),
            );
            workflow.add_node(llm);
        }

        let optimizer = WorkflowAnalyzer::new();
        let recommendations = optimizer.analyze(&workflow);

        // Should recommend batching
        let batching_recs: Vec<_> = recommendations
            .iter()
            .filter(|r| r.optimization_type == OptimizationType::BatchOperations)
            .collect();

        assert!(!batching_recs.is_empty());
    }

    #[test]
    fn test_dag_dimensions() {
        let workflow = create_test_workflow();
        let optimizer = WorkflowAnalyzer::new();
        let (depth, width) = optimizer.calculate_dag_dimensions(&workflow);

        assert_eq!(depth, 3); // Start -> LLM -> End
        assert_eq!(width, 1); // Linear workflow
    }
}
