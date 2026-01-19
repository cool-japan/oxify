//! Workflow health checker for identifying potential issues before execution
//!
//! This module provides tools to analyze workflows and identify potential
//! performance issues, design problems, and optimization opportunities.

use oxify_model::{NodeKind, Workflow};
use std::collections::HashSet;

/// Health check severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthSeverity {
    /// Informational message
    Info,
    /// Warning that should be reviewed
    Warning,
    /// Error that may cause execution failure
    Error,
    /// Critical issue that will likely cause failure
    Critical,
}

/// Health check issue
#[derive(Debug, Clone)]
pub struct HealthIssue {
    /// Severity level
    pub severity: HealthSeverity,
    /// Category of the issue
    pub category: String,
    /// Description of the issue
    pub description: String,
    /// Recommendation for fixing
    pub recommendation: String,
    /// Nodes affected (if applicable)
    pub affected_nodes: Vec<String>,
}

impl HealthIssue {
    /// Create a new health issue
    pub fn new(
        severity: HealthSeverity,
        category: impl Into<String>,
        description: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            category: category.into(),
            description: description.into(),
            recommendation: recommendation.into(),
            affected_nodes: Vec::new(),
        }
    }

    /// Add affected nodes to this issue
    pub fn with_nodes(mut self, nodes: Vec<String>) -> Self {
        self.affected_nodes = nodes;
        self
    }
}

/// Workflow health report
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Issues found during health check
    pub issues: Vec<HealthIssue>,
    /// Overall health score (0-100)
    pub health_score: u8,
    /// Whether the workflow is considered healthy
    pub is_healthy: bool,
}

impl HealthReport {
    /// Create a new health report
    pub fn new(issues: Vec<HealthIssue>) -> Self {
        let is_healthy = !issues
            .iter()
            .any(|i| matches!(i.severity, HealthSeverity::Error | HealthSeverity::Critical));

        // Calculate health score based on issues
        let health_score = Self::calculate_health_score(&issues);

        Self {
            issues,
            health_score,
            is_healthy,
        }
    }

    fn calculate_health_score(issues: &[HealthIssue]) -> u8 {
        let mut score: u8 = 100;

        for issue in issues {
            let penalty: u8 = match issue.severity {
                HealthSeverity::Info => 0,
                HealthSeverity::Warning => 5,
                HealthSeverity::Error => 15,
                HealthSeverity::Critical => 30,
            };
            score = score.saturating_sub(penalty);
        }

        score
    }

    /// Get issues by severity
    pub fn get_issues_by_severity(&self, severity: HealthSeverity) -> Vec<&HealthIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == severity)
            .collect()
    }

    /// Get count of issues by severity
    pub fn count_by_severity(&self, severity: HealthSeverity) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == severity)
            .count()
    }
}

/// Workflow health checker
pub struct HealthChecker;

impl HealthChecker {
    /// Create a new health checker
    pub fn new() -> Self {
        Self
    }

    /// Perform comprehensive health check on a workflow
    pub fn check(&self, workflow: &Workflow) -> HealthReport {
        let mut issues = Vec::new();

        // Run all health checks
        self.check_node_count(workflow, &mut issues);
        self.check_dag_depth(workflow, &mut issues);
        self.check_parallel_width(workflow, &mut issues);
        self.check_node_names(workflow, &mut issues);
        self.check_isolated_nodes(workflow, &mut issues);
        self.check_terminal_nodes(workflow, &mut issues);
        self.check_loop_safety(workflow, &mut issues);
        self.check_retry_config(workflow, &mut issues);
        self.check_llm_configuration(workflow, &mut issues);

        HealthReport::new(issues)
    }

    /// Check if node count is reasonable
    fn check_node_count(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        let node_count = workflow.nodes.len();

        if node_count == 0 {
            issues.push(HealthIssue::new(
                HealthSeverity::Critical,
                "Structure",
                "Workflow has no nodes",
                "Add at least a start and end node to the workflow",
            ));
        } else if node_count == 1 {
            issues.push(HealthIssue::new(
                HealthSeverity::Error,
                "Structure",
                "Workflow has only one node",
                "Add processing nodes between start and end",
            ));
        } else if node_count > 1000 {
            issues.push(HealthIssue::new(
                HealthSeverity::Warning,
                "Performance",
                format!(
                    "Workflow has {} nodes, which may impact performance",
                    node_count
                ),
                "Consider splitting into sub-workflows or simplifying the workflow",
            ));
        } else if node_count > 500 {
            issues.push(HealthIssue::new(
                HealthSeverity::Info,
                "Performance",
                format!("Workflow has {} nodes", node_count),
                "Monitor execution performance and consider optimizations if needed",
            ));
        }
    }

    /// Check DAG depth for performance
    fn check_dag_depth(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        // Calculate maximum depth from start to end
        let mut depths = std::collections::HashMap::new();

        // Find start nodes
        let start_nodes: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Start))
            .map(|n| n.id)
            .collect();

        for start_id in start_nodes {
            depths.insert(start_id, 0);
        }

        // BFS to calculate depths
        let mut queue: Vec<_> = depths.keys().copied().collect();
        let mut visited = HashSet::new();

        while let Some(node_id) = queue.pop() {
            if !visited.insert(node_id) {
                continue;
            }

            let current_depth = *depths.get(&node_id).unwrap_or(&0);

            // Find outgoing edges
            for edge in &workflow.edges {
                if edge.from == node_id {
                    let new_depth = current_depth + 1;
                    depths
                        .entry(edge.to)
                        .and_modify(|d| *d = (*d).max(new_depth))
                        .or_insert(new_depth);
                    queue.push(edge.to);
                }
            }
        }

        let max_depth = depths.values().max().copied().unwrap_or(0);

        if max_depth > 50 {
            issues.push(HealthIssue::new(
                HealthSeverity::Warning,
                "Performance",
                format!(
                    "Workflow has a depth of {}, which may serialize execution",
                    max_depth
                ),
                "Flatten the workflow by identifying opportunities for parallel execution",
            ));
        } else if max_depth > 20 {
            issues.push(HealthIssue::new(
                HealthSeverity::Info,
                "Performance",
                format!("Workflow has a depth of {}", max_depth),
                "Consider opportunities to reduce sequential dependencies",
            ));
        }
    }

    /// Check parallel width for resource usage
    fn check_parallel_width(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        // Count nodes at each level
        let mut levels = std::collections::HashMap::new();

        for node in &workflow.nodes {
            // Simple heuristic: count incoming edges
            let incoming = workflow.edges.iter().filter(|e| e.to == node.id).count();
            let level = incoming; // Simplified level calculation
            *levels.entry(level).or_insert(0) += 1;
        }

        let max_width = levels.values().max().copied().unwrap_or(0);

        if max_width > 100 {
            issues.push(HealthIssue::new(
                HealthSeverity::Warning,
                "Resource Usage",
                format!(
                    "Workflow has up to {} parallel nodes at one level",
                    max_width
                ),
                "Configure max_concurrent_nodes in ExecutionConfig to limit parallel execution",
            ));
        } else if max_width > 50 {
            issues.push(HealthIssue::new(
                HealthSeverity::Info,
                "Resource Usage",
                format!(
                    "Workflow has up to {} parallel nodes at one level",
                    max_width
                ),
                "Ensure sufficient system resources for parallel execution",
            ));
        }
    }

    /// Check for duplicate or empty node names
    fn check_node_names(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        let mut name_counts = std::collections::HashMap::new();
        let mut empty_names = Vec::new();

        for node in &workflow.nodes {
            if node.name.trim().is_empty() {
                empty_names.push(format!("{}", node.id));
            } else {
                *name_counts.entry(&node.name).or_insert(0) += 1;
            }
        }

        if !empty_names.is_empty() {
            issues.push(
                HealthIssue::new(
                    HealthSeverity::Warning,
                    "Naming",
                    format!("{} nodes have empty names", empty_names.len()),
                    "Provide descriptive names for all nodes to improve debugging",
                )
                .with_nodes(empty_names),
            );
        }

        for (name, count) in name_counts {
            if count > 1 {
                issues.push(HealthIssue::new(
                    HealthSeverity::Warning,
                    "Naming",
                    format!("Node name '{}' is used {} times", name, count),
                    "Use unique names for nodes to avoid confusion",
                ));
            }
        }
    }

    /// Check for isolated nodes (no incoming or outgoing edges)
    fn check_isolated_nodes(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        let mut connected_nodes = HashSet::new();

        for edge in &workflow.edges {
            connected_nodes.insert(edge.from);
            connected_nodes.insert(edge.to);
        }

        let isolated: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| !connected_nodes.contains(&n.id))
            .filter(|n| !matches!(n.kind, NodeKind::Start | NodeKind::End))
            .map(|n| n.name.clone())
            .collect();

        if !isolated.is_empty() {
            issues.push(
                HealthIssue::new(
                    HealthSeverity::Error,
                    "Structure",
                    format!("{} isolated nodes detected", isolated.len()),
                    "Connect all nodes to the workflow DAG or remove unused nodes",
                )
                .with_nodes(isolated),
            );
        }
    }

    /// Check for proper terminal nodes (start and end)
    fn check_terminal_nodes(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        let start_count = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Start))
            .count();

        let end_count = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::End))
            .count();

        if start_count == 0 {
            issues.push(HealthIssue::new(
                HealthSeverity::Critical,
                "Structure",
                "Workflow has no Start node",
                "Add a Start node to mark the workflow entry point",
            ));
        } else if start_count > 1 {
            issues.push(HealthIssue::new(
                HealthSeverity::Warning,
                "Structure",
                format!("Workflow has {} Start nodes", start_count),
                "Typically, workflows should have exactly one Start node",
            ));
        }

        if end_count == 0 {
            issues.push(HealthIssue::new(
                HealthSeverity::Error,
                "Structure",
                "Workflow has no End node",
                "Add an End node to mark the workflow completion point",
            ));
        } else if end_count > 1 {
            issues.push(HealthIssue::new(
                HealthSeverity::Info,
                "Structure",
                format!("Workflow has {} End nodes (multiple paths)", end_count),
                "Multiple end nodes are valid for conditional workflows",
            ));
        }
    }

    /// Check loop nodes for safety limits
    fn check_loop_safety(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        let loop_nodes: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Loop(_)))
            .collect();

        if loop_nodes.len() > 10 {
            issues.push(HealthIssue::new(
                HealthSeverity::Warning,
                "Complexity",
                format!("Workflow has {} loop nodes", loop_nodes.len()),
                "Many loops may indicate complex logic; consider simplifying or using sub-workflows",
            ));
        }
    }

    /// Check retry configurations
    fn check_retry_config(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        let nodes_with_retries: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| n.retry_config.is_some())
            .collect();

        if nodes_with_retries.is_empty() {
            issues.push(HealthIssue::new(
                HealthSeverity::Info,
                "Resilience",
                "No nodes have retry configuration",
                "Consider adding retry logic to LLM and API nodes for improved resilience",
            ));
        }

        for node in nodes_with_retries {
            if let Some(ref retry_config) = node.retry_config {
                if retry_config.max_retries > 10 {
                    issues.push(HealthIssue::new(
                        HealthSeverity::Warning,
                        "Resilience",
                        format!(
                            "Node '{}' has {} max retries, which may cause long delays",
                            node.name, retry_config.max_retries
                        ),
                        "Consider reducing max retries to 3-5 for faster failure detection",
                    ));
                }
            }
        }
    }

    /// Check LLM node configurations
    fn check_llm_configuration(&self, workflow: &Workflow, issues: &mut Vec<HealthIssue>) {
        for node in &workflow.nodes {
            if let NodeKind::LLM(ref config) = node.kind {
                // Check for empty prompts
                if config.prompt_template.trim().is_empty() {
                    issues.push(HealthIssue::new(
                        HealthSeverity::Error,
                        "Configuration",
                        format!("LLM node '{}' has empty prompt template", node.name),
                        "Provide a meaningful prompt template for LLM execution",
                    ));
                }

                // Check for very long prompts
                if config.prompt_template.len() > 10000 {
                    issues.push(HealthIssue::new(
                        HealthSeverity::Warning,
                        "Performance",
                        format!(
                            "LLM node '{}' has a very long prompt ({} chars)",
                            node.name,
                            config.prompt_template.len()
                        ),
                        "Long prompts may hit token limits and increase costs",
                    ));
                }

                // Check token limits
                if let Some(max_tokens) = config.max_tokens {
                    if max_tokens > 4000 {
                        issues.push(HealthIssue::new(
                            HealthSeverity::Info,
                            "Cost",
                            format!(
                                "LLM node '{}' has max_tokens set to {} (high cost)",
                                node.name, max_tokens
                            ),
                            "Review if such high token limits are necessary",
                        ));
                    }
                }
            }
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, LlmConfig, Node, RetryConfig};

    #[test]
    fn test_empty_workflow() {
        let workflow = Workflow::new("test".to_string());
        let checker = HealthChecker::new();
        let report = checker.check(&workflow);

        assert!(!report.is_healthy);
        assert!(report.count_by_severity(HealthSeverity::Critical) > 0);
    }

    #[test]
    fn test_workflow_with_isolated_nodes() {
        let mut workflow = Workflow::new("test".to_string());

        // Add start and end
        let start = Node::new("start".to_string(), NodeKind::Start);
        let end = Node::new("end".to_string(), NodeKind::End);
        let start_id = start.id;
        let end_id = end.id;
        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Add isolated node (LLM node with no connections)
        let isolated = Node::new(
            "isolated".to_string(),
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
        workflow.add_node(isolated);

        let checker = HealthChecker::new();
        let report = checker.check(&workflow);

        assert!(!report.is_healthy);
        let errors = report.get_issues_by_severity(HealthSeverity::Error);
        assert!(errors.iter().any(|i| i.category == "Structure"));
    }

    #[test]
    fn test_workflow_with_duplicate_names() {
        let mut workflow = Workflow::new("test".to_string());

        let node1 = Node::new("duplicate".to_string(), NodeKind::Start);
        let node2 = Node::new("duplicate".to_string(), NodeKind::End);

        workflow.add_node(node1);
        workflow.add_node(node2);

        let checker = HealthChecker::new();
        let report = checker.check(&workflow);

        let warnings = report.get_issues_by_severity(HealthSeverity::Warning);
        assert!(warnings.iter().any(|i| i.category == "Naming"));
    }

    #[test]
    fn test_workflow_with_excessive_retries() {
        let mut workflow = Workflow::new("test".to_string());

        let mut node = Node::new("llm".to_string(), NodeKind::Start);
        node.retry_config = Some(RetryConfig {
            max_retries: 20,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 60000,
        });

        workflow.add_node(node);

        let checker = HealthChecker::new();
        let report = checker.check(&workflow);

        let warnings = report.get_issues_by_severity(HealthSeverity::Warning);
        assert!(warnings.iter().any(|i| i.category == "Resilience"));
    }

    #[test]
    fn test_workflow_with_empty_prompt() {
        let mut workflow = Workflow::new("test".to_string());

        let node = Node::new(
            "llm".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "".to_string(),
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );

        workflow.add_node(node);

        let checker = HealthChecker::new();
        let report = checker.check(&workflow);

        let errors = report.get_issues_by_severity(HealthSeverity::Error);
        assert!(errors.iter().any(|i| i.category == "Configuration"));
    }

    #[test]
    fn test_health_score_calculation() {
        let mut workflow = Workflow::new("test".to_string());

        // Add start and end
        let start = Node::new("start".to_string(), NodeKind::Start);
        let end = Node::new("end".to_string(), NodeKind::End);
        let start_id = start.id;
        let end_id = end.id;
        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        let checker = HealthChecker::new();
        let report = checker.check(&workflow);

        assert!(report.health_score >= 80); // Should be healthy
        assert!(report.is_healthy);
    }
}
