// Workflow Linter - Code quality and best practices checker
//
// This module provides a comprehensive linting system for workflows.
// Unlike validation (which checks correctness), linting checks for:
// - Code quality and best practices
// - Performance optimization opportunities
// - Security concerns
// - Maintainability issues
// - Resource usage patterns

use crate::{Edge, LoopType, Node, NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Severity level for lint findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LintSeverity {
    /// Informational suggestion
    Info,
    /// Warning about potential issues
    Warning,
    /// Error that should be fixed
    Error,
}

impl std::fmt::Display for LintSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LintSeverity::Info => write!(f, "INFO"),
            LintSeverity::Warning => write!(f, "WARNING"),
            LintSeverity::Error => write!(f, "ERROR"),
        }
    }
}

/// Category of lint rule
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LintCategory {
    /// Performance-related issues
    Performance,
    /// Security concerns
    Security,
    /// Maintainability and code quality
    Maintainability,
    /// Resource usage optimization
    ResourceUsage,
    /// Best practices
    BestPractice,
    /// Reliability concerns
    Reliability,
}

/// A single lint finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintFinding {
    /// Rule that triggered this finding
    pub rule_id: String,
    /// Severity level
    pub severity: LintSeverity,
    /// Category of the rule
    pub category: LintCategory,
    /// Human-readable message
    pub message: String,
    /// Optional node ID where the issue was found
    pub node_id: Option<String>,
    /// Optional suggestion for fixing the issue
    pub suggestion: Option<String>,
    /// Line number (if applicable)
    pub line: Option<usize>,
}

impl LintFinding {
    /// Create a new lint finding
    pub fn new(
        rule_id: impl Into<String>,
        severity: LintSeverity,
        category: LintCategory,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity,
            category,
            message: message.into(),
            node_id: None,
            suggestion: None,
            line: None,
        }
    }

    /// Set the node ID where the issue was found
    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// Add a suggestion for fixing the issue
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Set the line number
    #[allow(dead_code)]
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

/// Result of linting a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintResult {
    /// All findings from linting
    pub findings: Vec<LintFinding>,
    /// Summary statistics
    pub stats: LintStats,
}

/// Statistics about lint results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintStats {
    /// Total number of findings
    pub total: usize,
    /// Number of errors
    pub errors: usize,
    /// Number of warnings
    pub warnings: usize,
    /// Number of info messages
    pub info: usize,
}

impl LintResult {
    /// Create a new lint result
    pub fn new(findings: Vec<LintFinding>) -> Self {
        let errors = findings
            .iter()
            .filter(|f| f.severity == LintSeverity::Error)
            .count();
        let warnings = findings
            .iter()
            .filter(|f| f.severity == LintSeverity::Warning)
            .count();
        let info = findings
            .iter()
            .filter(|f| f.severity == LintSeverity::Info)
            .count();

        Self {
            stats: LintStats {
                total: findings.len(),
                errors,
                warnings,
                info,
            },
            findings,
        }
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.stats.errors > 0
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        self.stats.warnings > 0
    }

    /// Get findings by severity
    pub fn findings_by_severity(&self, severity: LintSeverity) -> Vec<&LintFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .collect()
    }

    /// Get findings by category
    pub fn findings_by_category(&self, category: LintCategory) -> Vec<&LintFinding> {
        self.findings
            .iter()
            .filter(|f| f.category == category)
            .collect()
    }
}

/// Configuration for the workflow linter
#[derive(Debug, Clone)]
pub struct LinterConfig {
    /// Maximum recommended retry count
    pub max_retry_count: u32,
    /// Maximum recommended timeout in milliseconds
    pub max_timeout_ms: u64,
    /// Maximum recommended nodes in a sequence (before suggesting parallelization)
    pub max_sequential_nodes: usize,
    /// Maximum nesting depth for conditional nodes
    pub max_nesting_depth: usize,
    /// Whether to check for naming conventions
    pub check_naming: bool,
    /// Whether to check for security issues
    pub check_security: bool,
    /// Whether to check for performance issues
    pub check_performance: bool,
}

impl Default for LinterConfig {
    fn default() -> Self {
        Self {
            max_retry_count: 5,
            max_timeout_ms: 300_000, // 5 minutes
            max_sequential_nodes: 10,
            max_nesting_depth: 4,
            check_naming: true,
            check_security: true,
            check_performance: true,
        }
    }
}

/// Workflow linter for checking code quality and best practices
pub struct WorkflowLinter {
    config: LinterConfig,
}

impl WorkflowLinter {
    /// Create a new linter with default configuration
    pub fn new() -> Self {
        Self {
            config: LinterConfig::default(),
        }
    }

    /// Create a linter with custom configuration
    pub fn with_config(config: LinterConfig) -> Self {
        Self { config }
    }

    /// Lint a workflow and return findings
    pub fn lint(&self, workflow: &Workflow) -> LintResult {
        let mut findings = Vec::new();

        // Run all lint rules
        findings.extend(self.check_unused_nodes(workflow));
        findings.extend(self.check_missing_error_handling(workflow));
        findings.extend(self.check_excessive_retries(workflow));
        findings.extend(self.check_missing_timeouts(workflow));
        findings.extend(self.check_sequential_opportunities(workflow));
        findings.extend(self.check_deep_nesting(workflow));
        findings.extend(self.check_naming_conventions(workflow));
        findings.extend(self.check_hardcoded_values(workflow));
        findings.extend(self.check_loop_safety(workflow));
        findings.extend(self.check_dead_end_paths(workflow));

        LintResult::new(findings)
    }

    /// Check for nodes that are defined but never used (unreachable)
    fn check_unused_nodes(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();
        let mut reachable = HashSet::new();

        // Find start nodes
        for node in &workflow.nodes {
            if matches!(node.kind, NodeKind::Start) {
                Self::mark_reachable(&node.id, workflow, &mut reachable);
            }
        }

        // Check for unreachable nodes
        for node in &workflow.nodes {
            if !reachable.contains(&node.id) && !matches!(node.kind, NodeKind::Start) {
                findings.push(
                    LintFinding::new(
                        "unreachable-node",
                        LintSeverity::Warning,
                        LintCategory::Maintainability,
                        format!("Node '{}' is unreachable from start nodes", node.name),
                    )
                    .with_node_id(node.id.to_string())
                    .with_suggestion("Remove this node or add edges to connect it to the workflow"),
                );
            }
        }

        findings
    }

    /// Mark all nodes reachable from a given node
    fn mark_reachable(
        node_id: &uuid::Uuid,
        workflow: &Workflow,
        reachable: &mut HashSet<uuid::Uuid>,
    ) {
        if !reachable.insert(*node_id) {
            return; // Already visited
        }

        // Find outgoing edges
        for edge in &workflow.edges {
            if edge.from == *node_id {
                Self::mark_reachable(&edge.to, workflow, reachable);
            }
        }
    }

    /// Check for missing error handling (no try-catch around risky operations)
    fn check_missing_error_handling(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        // Build a map of nodes that have error handling
        let mut protected_nodes = HashSet::new();
        for node in &workflow.nodes {
            if let NodeKind::TryCatch(config) = &node.kind {
                protected_nodes.insert(config.try_expression.as_str());
            }
        }

        // Check risky nodes (LLM, Code, Tool, Retriever)
        for node in &workflow.nodes {
            let is_risky = matches!(
                node.kind,
                NodeKind::LLM(_) | NodeKind::Code(_) | NodeKind::Tool(_) | NodeKind::Retriever(_)
            );

            if is_risky
                && !protected_nodes.contains(node.id.to_string().as_str())
                && node.retry_config.is_none()
            {
                findings.push(
                    LintFinding::new(
                        "missing-error-handling",
                        LintSeverity::Info,
                        LintCategory::Reliability,
                        format!(
                            "Node '{}' has no error handling (no try-catch or retry)",
                            node.name
                        ),
                    )
                    .with_node_id(node.id.to_string())
                    .with_suggestion(
                        "Consider adding retry configuration or wrapping in a TryCatch node",
                    ),
                );
            }
        }

        findings
    }

    /// Check for excessive retry counts
    fn check_excessive_retries(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        for node in &workflow.nodes {
            if let Some(retry) = &node.retry_config {
                if retry.max_retries > self.config.max_retry_count {
                    findings.push(
                        LintFinding::new(
                            "excessive-retries",
                            LintSeverity::Warning,
                            LintCategory::Performance,
                            format!(
                                "Node '{}' has {} retries (recommended max: {})",
                                node.name, retry.max_retries, self.config.max_retry_count
                            ),
                        )
                        .with_node_id(node.id.to_string())
                        .with_suggestion(format!(
                            "Consider reducing max_retries to {}",
                            self.config.max_retry_count
                        )),
                    );
                }
            }
        }

        findings
    }

    /// Check for missing timeouts on long-running operations
    fn check_missing_timeouts(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        for node in &workflow.nodes {
            let needs_timeout = matches!(
                node.kind,
                NodeKind::LLM(_)
                    | NodeKind::Code(_)
                    | NodeKind::Tool(_)
                    | NodeKind::Retriever(_)
                    | NodeKind::Approval(_)
                    | NodeKind::Form(_)
            );

            if needs_timeout && node.timeout_config.is_none() {
                findings.push(
                    LintFinding::new(
                        "missing-timeout",
                        LintSeverity::Info,
                        LintCategory::Reliability,
                        format!("Node '{}' has no timeout configuration", node.name),
                    )
                    .with_node_id(node.id.to_string())
                    .with_suggestion("Consider adding a timeout to prevent hanging executions"),
                );
            }
        }

        findings
    }

    /// Check for opportunities to parallelize sequential operations
    fn check_sequential_opportunities(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        if !self.config.check_performance {
            return findings;
        }

        // Build adjacency list
        let edges_by_source: HashMap<uuid::Uuid, Vec<&Edge>> =
            workflow.edges.iter().fold(HashMap::new(), |mut acc, edge| {
                acc.entry(edge.from).or_default().push(edge);
                acc
            });

        // Find long sequential chains
        for node in &workflow.nodes {
            if matches!(node.kind, NodeKind::Start) {
                let chain_length = Self::find_longest_chain(&node.id, workflow, &edges_by_source);
                if chain_length > self.config.max_sequential_nodes {
                    findings.push(
                        LintFinding::new(
                            "long-sequential-chain",
                            LintSeverity::Info,
                            LintCategory::Performance,
                            format!(
                                "Found sequential chain of {} nodes starting from '{}'",
                                chain_length, node.name
                            ),
                        )
                        .with_node_id(node.id.to_string())
                        .with_suggestion(
                            "Consider using a Parallel node to execute independent operations concurrently",
                        ),
                    );
                }
            }
        }

        findings
    }

    /// Find the longest sequential chain from a node
    fn find_longest_chain(
        node_id: &uuid::Uuid,
        workflow: &Workflow,
        edges_by_source: &HashMap<uuid::Uuid, Vec<&Edge>>,
    ) -> usize {
        let Some(edges) = edges_by_source.get(node_id) else {
            return 0;
        };

        if edges.is_empty() {
            return 0;
        }
        if edges.len() > 1 {
            // Branching - don't count as sequential
            return 0;
        }

        // Get the node to check if it's a branching node
        let target_id = &edges[0].to;
        let target_node = workflow.nodes.iter().find(|n| n.id == *target_id);
        if let Some(node) = target_node {
            if matches!(
                node.kind,
                NodeKind::IfElse(_) | NodeKind::Switch(_) | NodeKind::Parallel(_)
            ) {
                // Branching node - stop counting
                return 1;
            }
        }

        1 + Self::find_longest_chain(target_id, workflow, edges_by_source)
    }

    /// Check for deeply nested conditional structures
    fn check_deep_nesting(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        // Build adjacency list
        let edges_by_source: HashMap<uuid::Uuid, Vec<&Edge>> =
            workflow.edges.iter().fold(HashMap::new(), |mut acc, edge| {
                acc.entry(edge.from).or_default().push(edge);
                acc
            });

        // Check nesting depth for each conditional node
        for node in &workflow.nodes {
            if matches!(node.kind, NodeKind::IfElse(_) | NodeKind::Switch(_)) {
                let depth = Self::calculate_nesting_depth(&node.id, workflow, &edges_by_source, 0);
                if depth > self.config.max_nesting_depth {
                    findings.push(
                        LintFinding::new(
                            "deep-nesting",
                            LintSeverity::Warning,
                            LintCategory::Maintainability,
                            format!(
                                "Node '{}' has nesting depth of {} (max recommended: {})",
                                node.name, depth, self.config.max_nesting_depth
                            ),
                        )
                        .with_node_id(node.id.to_string())
                        .with_suggestion(
                            "Consider refactoring into sub-workflows or flattening the structure",
                        ),
                    );
                }
            }
        }

        findings
    }

    /// Calculate nesting depth from a node
    fn calculate_nesting_depth(
        node_id: &uuid::Uuid,
        workflow: &Workflow,
        edges_by_source: &HashMap<uuid::Uuid, Vec<&Edge>>,
        current_depth: usize,
    ) -> usize {
        let Some(edges) = edges_by_source.get(node_id) else {
            return current_depth;
        };

        if edges.is_empty() {
            return current_depth;
        }

        let mut max_depth = current_depth;
        for edge in edges.iter() {
            let target_node = workflow.nodes.iter().find(|n| n.id == edge.to);
            if let Some(node) = target_node {
                let depth = if matches!(node.kind, NodeKind::IfElse(_) | NodeKind::Switch(_)) {
                    Self::calculate_nesting_depth(
                        &node.id,
                        workflow,
                        edges_by_source,
                        current_depth + 1,
                    )
                } else {
                    Self::calculate_nesting_depth(
                        &node.id,
                        workflow,
                        edges_by_source,
                        current_depth,
                    )
                };
                max_depth = max_depth.max(depth);
            }
        }

        max_depth
    }

    /// Check for naming convention violations
    fn check_naming_conventions(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        if !self.config.check_naming {
            return findings;
        }

        for node in &workflow.nodes {
            // Check for generic names
            let generic_names = ["node", "step", "task", "action", "untitled"];
            let name_lower = node.name.to_lowercase();
            if generic_names.iter().any(|&n| name_lower.contains(n)) && name_lower.len() < 15 {
                findings.push(
                    LintFinding::new(
                        "generic-node-name",
                        LintSeverity::Info,
                        LintCategory::Maintainability,
                        format!("Node has generic name: '{}'", node.name),
                    )
                    .with_node_id(node.id.to_string())
                    .with_suggestion(
                        "Use a more descriptive name that explains what the node does",
                    ),
                );
            }

            // Check for very short names
            if node.name.len() < 3 {
                findings.push(
                    LintFinding::new(
                        "short-node-name",
                        LintSeverity::Info,
                        LintCategory::Maintainability,
                        format!("Node has very short name: '{}'", node.name),
                    )
                    .with_node_id(node.id.to_string())
                    .with_suggestion("Use a longer, more descriptive name"),
                );
            }
        }

        findings
    }

    /// Check for hardcoded values that should be parameterized
    fn check_hardcoded_values(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        if !self.config.check_security {
            return findings;
        }

        for node in &workflow.nodes {
            // Check LLM nodes for hardcoded API keys (common security issue)
            if let NodeKind::LLM(config) = &node.kind {
                let prompt = &config.prompt_template;
                // Look for patterns like "api_key", "secret", "password" in prompts
                let sensitive_patterns = ["api_key", "secret", "password", "token", "credential"];
                for pattern in &sensitive_patterns {
                    if prompt.to_lowercase().contains(pattern) {
                        findings.push(
                            LintFinding::new(
                                "potential-hardcoded-secret",
                                LintSeverity::Error,
                                LintCategory::Security,
                                format!(
                                    "Node '{}' may contain hardcoded secrets in prompt",
                                    node.name
                                ),
                            )
                            .with_node_id(node.id.to_string())
                            .with_suggestion("Use template variables like {{API_KEY}} instead of hardcoding secrets"),
                        );
                        break; // Only report once per node
                    }
                }
            }
        }

        findings
    }

    /// Check for loop safety issues
    fn check_loop_safety(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        for node in &workflow.nodes {
            if let NodeKind::Loop(config) = &node.kind {
                if config.max_iterations > 10000 {
                    let loop_type_name = match &config.loop_type {
                        LoopType::ForEach { .. } => "ForEach",
                        LoopType::While { .. } => "While",
                        LoopType::Repeat { .. } => "Repeat",
                    };
                    findings.push(
                        LintFinding::new(
                            "high-loop-limit",
                            LintSeverity::Warning,
                            LintCategory::Performance,
                            format!(
                                "{} node '{}' has very high iteration limit: {}",
                                loop_type_name, node.name, config.max_iterations
                            ),
                        )
                        .with_node_id(node.id.to_string())
                        .with_suggestion(
                            "Consider reducing max_iterations to prevent resource exhaustion",
                        ),
                    );
                }
            }
        }

        findings
    }

    /// Check for dead-end paths (paths that don't lead to an End node)
    fn check_dead_end_paths(&self, workflow: &Workflow) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        // Build reverse adjacency list (incoming edges)
        let edges_by_target: HashMap<uuid::Uuid, Vec<&Edge>> =
            workflow.edges.iter().fold(HashMap::new(), |mut acc, edge| {
                acc.entry(edge.to).or_default().push(edge);
                acc
            });

        // Find all end nodes
        let end_nodes: Vec<&Node> = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::End))
            .collect();

        // Mark all nodes that can reach an end node
        let mut can_reach_end = HashSet::new();
        for end_node in &end_nodes {
            Self::mark_can_reach_end(&end_node.id, &edges_by_target, &mut can_reach_end);
        }

        // Check for nodes that can't reach any end node
        for node in &workflow.nodes {
            if !matches!(node.kind, NodeKind::End | NodeKind::Start) {
                // Check if this node can reach an end node
                if !can_reach_end.contains(&node.id) {
                    findings.push(
                        LintFinding::new(
                            "dead-end-path",
                            LintSeverity::Warning,
                            LintCategory::Reliability,
                            format!("Node '{}' cannot reach any End node", node.name),
                        )
                        .with_node_id(node.id.to_string())
                        .with_suggestion("Add edges to connect this path to an End node"),
                    );
                }
            }
        }

        findings
    }

    /// Mark all nodes that can reach a given node
    fn mark_can_reach_end(
        node_id: &uuid::Uuid,
        edges_by_target: &HashMap<uuid::Uuid, Vec<&Edge>>,
        can_reach: &mut HashSet<uuid::Uuid>,
    ) {
        if !can_reach.insert(*node_id) {
            return; // Already visited
        }

        // Find incoming edges
        if let Some(incoming) = edges_by_target.get(node_id) {
            for edge in incoming {
                Self::mark_can_reach_end(&edge.from, edges_by_target, can_reach);
            }
        }
    }
}

impl Default for WorkflowLinter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LlmConfig, LoopConfig, LoopType, McpConfig, NodeKind, ParallelConfig, ParallelStrategy,
        RetryConfig,
    };

    // Helper function to create a simple LLM node for testing
    fn create_llm_node(name: &str) -> Node {
        Node::new(
            name.to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test".to_string(),
                temperature: None,
                max_tokens: None,
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        )
    }

    #[test]
    fn test_linter_creation() {
        let linter = WorkflowLinter::new();
        assert_eq!(linter.config.max_retry_count, 5);

        let custom_config = LinterConfig {
            max_retry_count: 3,
            ..Default::default()
        };
        let custom_linter = WorkflowLinter::with_config(custom_config);
        assert_eq!(custom_linter.config.max_retry_count, 3);
    }

    #[test]
    fn test_lint_result_stats() {
        let findings = vec![
            LintFinding::new(
                "test1",
                LintSeverity::Error,
                LintCategory::Security,
                "Error",
            ),
            LintFinding::new(
                "test2",
                LintSeverity::Warning,
                LintCategory::Performance,
                "Warning",
            ),
            LintFinding::new(
                "test3",
                LintSeverity::Info,
                LintCategory::Maintainability,
                "Info",
            ),
        ];

        let result = LintResult::new(findings);
        assert_eq!(result.stats.total, 3);
        assert_eq!(result.stats.errors, 1);
        assert_eq!(result.stats.warnings, 1);
        assert_eq!(result.stats.info, 1);
        assert!(result.has_errors());
        assert!(result.has_warnings());
    }

    #[test]
    fn test_unreachable_nodes() {
        let mut workflow = Workflow::new("test".to_string());
        let start = Node::new("start".to_string(), NodeKind::Start);
        let node1 = create_llm_node("node1");
        let unreachable = create_llm_node("unreachable");

        workflow.nodes = vec![start.clone(), node1.clone(), unreachable];
        workflow.edges = vec![Edge::new(start.id, node1.id)];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let unreachable_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "unreachable-node")
            .collect();
        assert_eq!(unreachable_findings.len(), 1);
    }

    #[test]
    fn test_excessive_retries() {
        let mut workflow = Workflow::new("test".to_string());
        let node = create_llm_node("node1").with_retry(RetryConfig {
            max_retries: 10,
            ..Default::default()
        });

        workflow.nodes = vec![node];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let retry_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "excessive-retries")
            .collect();
        assert_eq!(retry_findings.len(), 1);
        assert_eq!(retry_findings[0].severity, LintSeverity::Warning);
    }

    #[test]
    fn test_missing_timeouts() {
        let mut workflow = Workflow::new("test".to_string());
        let node = create_llm_node("node1");

        workflow.nodes = vec![node];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let timeout_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "missing-timeout")
            .collect();
        assert_eq!(timeout_findings.len(), 1);
    }

    #[test]
    fn test_generic_node_names() {
        let mut workflow = Workflow::new("test".to_string());
        let node = create_llm_node("node");

        workflow.nodes = vec![node];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let naming_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "generic-node-name")
            .collect();
        assert_eq!(naming_findings.len(), 1);
    }

    #[test]
    fn test_hardcoded_secrets() {
        let mut workflow = Workflow::new("test".to_string());
        let node = Node::new(
            "node1".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "Use api_key: sk-1234567890".to_string(),
                temperature: None,
                max_tokens: None,
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        workflow.nodes = vec![node];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let secret_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "potential-hardcoded-secret")
            .collect();
        assert_eq!(secret_findings.len(), 1);
        assert_eq!(secret_findings[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_high_loop_limits() {
        let mut workflow = Workflow::new("test".to_string());

        let foreach_node = Node::new(
            "foreach".to_string(),
            NodeKind::Loop(LoopConfig {
                loop_type: LoopType::ForEach {
                    collection_path: "items".to_string(),
                    item_variable: "item".to_string(),
                    index_variable: Some("i".to_string()),
                    body_expression: "body".to_string(),
                    parallel: false,
                    max_concurrency: None,
                },
                max_iterations: 15000, // Very high
            }),
        );

        workflow.nodes = vec![foreach_node];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let loop_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "high-loop-limit")
            .collect();
        assert_eq!(loop_findings.len(), 1);
    }

    #[test]
    fn test_missing_error_handling() {
        let mut workflow = Workflow::new("test".to_string());
        let node = Node::new(
            "risky".to_string(),
            NodeKind::Tool(McpConfig {
                server_id: "external_api".to_string(),
                tool_name: "call".to_string(),
                parameters: serde_json::json!({}),
            }),
        );

        workflow.nodes = vec![node];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let error_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "missing-error-handling")
            .collect();
        assert_eq!(error_findings.len(), 1);
    }

    #[test]
    fn test_findings_by_severity() {
        let findings = vec![
            LintFinding::new(
                "test1",
                LintSeverity::Error,
                LintCategory::Security,
                "Error",
            ),
            LintFinding::new(
                "test2",
                LintSeverity::Warning,
                LintCategory::Performance,
                "Warning",
            ),
        ];

        let result = LintResult::new(findings);
        let errors = result.findings_by_severity(LintSeverity::Error);
        assert_eq!(errors.len(), 1);

        let warnings = result.findings_by_severity(LintSeverity::Warning);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_findings_by_category() {
        let findings = vec![
            LintFinding::new(
                "test1",
                LintSeverity::Error,
                LintCategory::Security,
                "Security issue",
            ),
            LintFinding::new(
                "test2",
                LintSeverity::Warning,
                LintCategory::Performance,
                "Performance issue",
            ),
        ];

        let result = LintResult::new(findings);
        let security = result.findings_by_category(LintCategory::Security);
        assert_eq!(security.len(), 1);

        let performance = result.findings_by_category(LintCategory::Performance);
        assert_eq!(performance.len(), 1);
    }

    #[test]
    fn test_long_sequential_chain() {
        let mut workflow = Workflow::new("test".to_string());
        let start = Node::new("start".to_string(), NodeKind::Start);
        let mut prev = start.clone();
        workflow.nodes.push(start);

        // Create a chain of 12 nodes
        for i in 0..12 {
            let node = create_llm_node(&format!("node{}", i));
            workflow.edges.push(Edge::new(prev.id, node.id));
            workflow.nodes.push(node.clone());
            prev = node;
        }

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let chain_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "long-sequential-chain")
            .collect();
        assert!(!chain_findings.is_empty());
    }

    #[test]
    fn test_deep_nesting() {
        let mut workflow = Workflow::new("test".to_string());
        let start = Node::new("start".to_string(), NodeKind::Start);
        workflow.nodes.push(start.clone());

        let mut prev = start;
        // Create deeply nested if-else nodes
        for i in 0..6 {
            let then_node = Node::new(format!("then{}", i), NodeKind::End);
            let else_node = Node::new(format!("else{}", i), NodeKind::End);

            let if_node = Node::new(
                format!("if{}", i),
                NodeKind::IfElse(crate::Condition {
                    expression: "true".to_string(),
                    true_branch: then_node.id,
                    false_branch: else_node.id,
                }),
            );
            workflow.edges.push(Edge::new(prev.id, if_node.id));

            workflow.nodes.push(if_node.clone());
            workflow.nodes.push(then_node);
            workflow.nodes.push(else_node);
            prev = if_node;
        }

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let nesting_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "deep-nesting")
            .collect();
        assert!(!nesting_findings.is_empty());
    }

    #[test]
    fn test_dead_end_paths() {
        let mut workflow = Workflow::new("test".to_string());
        let start = Node::new("start".to_string(), NodeKind::Start);
        let node1 = create_llm_node("node1");
        let dead_end = create_llm_node("dead_end");
        let end = Node::new("end".to_string(), NodeKind::End);

        workflow.nodes = vec![start.clone(), node1.clone(), dead_end.clone(), end.clone()];
        workflow.edges = vec![
            Edge::new(start.id, node1.id),
            Edge::new(node1.id, end.id),
            Edge::new(start.id, dead_end.id),
            // Note: dead_end has no outgoing edge to End
        ];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        let dead_end_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "dead-end-path")
            .collect();
        assert_eq!(dead_end_findings.len(), 1);
    }

    #[test]
    fn test_parallel_node_opportunity() {
        let mut workflow = Workflow::new("test".to_string());
        let start = Node::new("start".to_string(), NodeKind::Start);
        let parallel = Node::new(
            "parallel".to_string(),
            NodeKind::Parallel(ParallelConfig {
                tasks: vec![
                    crate::ParallelTask {
                        id: "task1".to_string(),
                        expression: "node1".to_string(),
                        description: None,
                    },
                    crate::ParallelTask {
                        id: "task2".to_string(),
                        expression: "node2".to_string(),
                        description: None,
                    },
                ],
                strategy: ParallelStrategy::WaitAll,
                max_concurrency: None,
                timeout_ms: None,
            }),
        );

        workflow.nodes = vec![start.clone(), parallel.clone()];
        workflow.edges = vec![Edge::new(start.id, parallel.id)];

        let linter = WorkflowLinter::new();
        let result = linter.lint(&workflow);

        // Parallel nodes should not trigger sequential warnings
        let chain_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "long-sequential-chain")
            .collect();
        assert!(chain_findings.is_empty());
    }
}
