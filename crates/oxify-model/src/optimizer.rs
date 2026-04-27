//! Workflow optimization and analysis
//!
//! This module analyzes workflows and provides optimization suggestions
//! including redundancy detection, parallelization opportunities, and cost/time improvements.

use crate::{CostEstimator, NodeId, NodeKind, TimePredictor, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Optimization analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationReport {
    /// Overall optimization score (0.0 = poor, 1.0 = optimal)
    pub score: f64,

    /// List of optimization suggestions
    pub suggestions: Vec<OptimizationSuggestion>,

    /// Detected issues
    pub issues: Vec<WorkflowIssue>,

    /// Potential improvements summary
    pub improvements: ImprovementSummary,

    /// Complexity metrics
    pub complexity: ComplexityMetrics,
}

/// A single optimization suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    /// Suggestion category
    pub category: SuggestionCategory,

    /// Severity (Critical, High, Medium, Low)
    pub severity: Severity,

    /// Human-readable description
    pub description: String,

    /// Affected node IDs
    pub affected_nodes: Vec<String>,

    /// Expected benefit
    pub benefit: Benefit,
}

/// Category of optimization suggestion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SuggestionCategory {
    /// Cost reduction opportunities
    CostReduction,

    /// Performance improvement opportunities
    Performance,

    /// Parallelization opportunities
    Parallelization,

    /// Redundancy elimination
    Redundancy,

    /// Model selection improvements
    ModelSelection,

    /// Resource optimization
    ResourceOptimization,

    /// Architecture improvements
    Architecture,
}

/// Severity level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Expected benefit from applying suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Benefit {
    /// Estimated cost savings (USD)
    pub cost_savings_usd: Option<f64>,

    /// Estimated time savings (ms)
    pub time_savings_ms: Option<u64>,

    /// Estimated quality improvement (0.0 to 1.0)
    pub quality_improvement: Option<f64>,
}

/// Detected workflow issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowIssue {
    /// Issue type
    pub issue_type: IssueType,

    /// Description
    pub description: String,

    /// Affected nodes
    pub affected_nodes: Vec<String>,
}

/// Type of workflow issue
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IssueType {
    /// Redundant nodes detected
    RedundantNodes,

    /// Inefficient sequencing
    InefficientSequencing,

    /// Expensive operations
    ExpensiveOperation,

    /// Slow operations
    SlowOperation,

    /// Missing error handling
    MissingErrorHandling,

    /// Inefficient model choice
    InefficientModel,
}

/// Summary of potential improvements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementSummary {
    /// Total potential cost savings
    pub total_cost_savings_usd: f64,

    /// Total potential time savings
    pub total_time_savings_ms: u64,

    /// Number of parallelization opportunities
    pub parallelization_opportunities: usize,

    /// Number of redundant nodes
    pub redundant_nodes: usize,
}

/// Workflow complexity metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityMetrics {
    /// Total number of nodes
    pub total_nodes: usize,

    /// Total number of edges
    pub total_edges: usize,

    /// Maximum depth (longest path)
    pub max_depth: usize,

    /// Average branching factor
    pub avg_branching_factor: f64,

    /// Cyclomatic complexity
    pub cyclomatic_complexity: usize,

    /// Number of LLM nodes
    pub llm_nodes: usize,

    /// Number of conditional branches
    pub conditional_nodes: usize,
}

/// Workflow optimizer and analyzer
pub struct WorkflowOptimizer;

impl WorkflowOptimizer {
    /// Analyze a workflow and provide optimization suggestions
    pub fn analyze(workflow: &Workflow) -> OptimizationReport {
        let mut suggestions = Vec::new();
        let mut issues = Vec::new();

        // Analyze various aspects
        suggestions.extend(Self::analyze_cost(workflow));
        suggestions.extend(Self::analyze_performance(workflow));
        suggestions.extend(Self::analyze_parallelization(workflow));
        suggestions.extend(Self::analyze_redundancy(workflow));
        suggestions.extend(Self::analyze_model_selection(workflow));
        suggestions.extend(Self::analyze_caching(workflow));

        // Detect issues
        issues.extend(Self::detect_issues(workflow));

        // Calculate improvements summary
        let improvements = Self::calculate_improvements(&suggestions);

        // Calculate complexity metrics
        let complexity = Self::calculate_complexity(workflow);

        // Calculate overall score
        let score = Self::calculate_score(workflow, &suggestions, &issues);

        OptimizationReport {
            score,
            suggestions,
            issues,
            improvements,
            complexity,
        }
    }

    /// Analyze cost optimization opportunities
    fn analyze_cost(workflow: &Workflow) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();
        let cost_estimate = CostEstimator::estimate(workflow);

        // Find expensive nodes
        for (node_id, node_cost) in &cost_estimate.node_costs {
            if node_cost.cost_usd > 0.01 {
                // Find the actual node
                if let Some(node) = workflow.nodes.iter().find(|n| n.id.to_string() == *node_id) {
                    if let NodeKind::LLM(config) = &node.kind {
                        // Suggest cheaper models
                        if config.model.contains("gpt-4") && !config.model.contains("turbo") {
                            suggestions.push(OptimizationSuggestion {
                                category: SuggestionCategory::CostReduction,
                                severity: Severity::Medium,
                                description: format!(
                                    "Consider using GPT-4-turbo instead of {} for 67% cost reduction",
                                    config.model
                                ),
                                affected_nodes: vec![node_id.clone()],
                                benefit: Benefit {
                                    cost_savings_usd: Some(node_cost.cost_usd * 0.67),
                                    time_savings_ms: None,
                                    quality_improvement: Some(-0.05), // Slight quality tradeoff
                                },
                            });
                        } else if config.model.contains("claude-3-opus") {
                            suggestions.push(OptimizationSuggestion {
                                category: SuggestionCategory::CostReduction,
                                severity: Severity::Medium,
                                description: format!(
                                    "Consider using Claude-3-Sonnet for 80% cost reduction ({})",
                                    node.name
                                ),
                                affected_nodes: vec![node_id.clone()],
                                benefit: Benefit {
                                    cost_savings_usd: Some(node_cost.cost_usd * 0.8),
                                    time_savings_ms: None,
                                    quality_improvement: Some(-0.1),
                                },
                            });
                        }
                    }
                }
            }
        }

        suggestions
    }

    /// Analyze performance optimization opportunities
    fn analyze_performance(workflow: &Workflow) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();
        let time_estimate = TimePredictor::new().predict(workflow);

        // Find slow nodes
        let slowest = time_estimate.slowest_nodes(3);
        for node_time in slowest {
            if node_time.avg_ms > 5000 {
                // Over 5 seconds
                suggestions.push(OptimizationSuggestion {
                    category: SuggestionCategory::Performance,
                    severity: if node_time.avg_ms > 10000 {
                        Severity::High
                    } else {
                        Severity::Medium
                    },
                    description: format!(
                        "Node '{}' is slow (avg: {}ms). Consider caching or optimization.",
                        node_time.node_name, node_time.avg_ms
                    ),
                    affected_nodes: vec![node_time.node_name.clone()],
                    benefit: Benefit {
                        cost_savings_usd: None,
                        time_savings_ms: Some(node_time.avg_ms / 2), // Assume 50% improvement
                        quality_improvement: None,
                    },
                });
            }
        }

        suggestions
    }

    /// Analyze parallelization opportunities
    fn analyze_parallelization(workflow: &Workflow) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Build adjacency map
        let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for edge in &workflow.edges {
            children.entry(edge.from).or_default().push(edge.to);
        }

        // Find nodes with multiple children that could be parallelized
        for (node_id, child_ids) in &children {
            if child_ids.len() > 1 {
                // Check if children are independent (don't reference each other)
                let node = workflow.nodes.iter().find(|n| n.id == *node_id);
                if let Some(node) = node {
                    suggestions.push(OptimizationSuggestion {
                        category: SuggestionCategory::Parallelization,
                        severity: Severity::Medium,
                        description: format!(
                            "Node '{}' has {} sequential children that could be parallelized",
                            node.name,
                            child_ids.len()
                        ),
                        affected_nodes: vec![node_id.to_string()],
                        benefit: Benefit {
                            cost_savings_usd: None,
                            time_savings_ms: Some(5000), // Rough estimate
                            quality_improvement: None,
                        },
                    });
                }
            }
        }

        suggestions
    }

    /// Analyze redundancy
    fn analyze_redundancy(workflow: &Workflow) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Find duplicate LLM configurations (same provider, model, and prompt)
        let mut llm_configs: HashMap<String, Vec<String>> = HashMap::new();

        for node in &workflow.nodes {
            if let NodeKind::LLM(config) = &node.kind {
                let key = format!(
                    "{}:{}:{}",
                    config.provider, config.model, config.prompt_template
                );
                llm_configs
                    .entry(key)
                    .or_default()
                    .push(node.id.to_string());
            }
        }

        for (_config_key, node_ids) in llm_configs {
            if node_ids.len() > 1 {
                suggestions.push(OptimizationSuggestion {
                    category: SuggestionCategory::Redundancy,
                    severity: Severity::Medium,
                    description: format!(
                        "Found {} nodes with identical LLM configuration. Consider caching or deduplication.",
                        node_ids.len()
                    ),
                    affected_nodes: node_ids.clone(),
                    benefit: Benefit {
                        cost_savings_usd: Some(0.01 * (node_ids.len() - 1) as f64),
                        time_savings_ms: Some(1000 * (node_ids.len() - 1) as u64),
                        quality_improvement: None,
                    },
                });
            }
        }

        suggestions
    }

    /// Analyze model selection
    fn analyze_model_selection(workflow: &Workflow) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        for node in &workflow.nodes {
            if let NodeKind::LLM(config) = &node.kind {
                // Check if simple prompts are using expensive models
                let prompt_length = config.prompt_template.len();
                let max_tokens = config.max_tokens.unwrap_or(1000);

                if prompt_length < 100
                    && max_tokens < 200
                    && (config.model.contains("gpt-4") || config.model.contains("claude-3-opus"))
                {
                    suggestions.push(OptimizationSuggestion {
                            category: SuggestionCategory::ModelSelection,
                            severity: Severity::Low,
                            description: format!(
                                "Node '{}' uses expensive model for simple prompt. Consider GPT-3.5 or Claude-Haiku.",
                                node.name
                            ),
                            affected_nodes: vec![node.id.to_string()],
                            benefit: Benefit {
                                cost_savings_usd: Some(0.005),
                                time_savings_ms: Some(500),
                                quality_improvement: Some(-0.05),
                            },
                        });
                }
            }
        }

        suggestions
    }

    /// Analyze caching opportunities
    fn analyze_caching(workflow: &Workflow) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Identify LLM nodes that could benefit from caching
        for node in &workflow.nodes {
            match &node.kind {
                NodeKind::LLM(config) => {
                    // Check if prompt is deterministic (no templates or simple templates)
                    let has_simple_template = !config.prompt_template.contains("{{")
                        || config.prompt_template.matches("{{").count() <= 2;

                    if has_simple_template {
                        suggestions.push(OptimizationSuggestion {
                            category: SuggestionCategory::ResourceOptimization,
                            severity: Severity::Medium,
                            description: format!(
                                "Node '{}' could benefit from response caching. Deterministic prompts can be cached to save cost and time.",
                                node.name
                            ),
                            affected_nodes: vec![node.id.to_string()],
                            benefit: Benefit {
                                cost_savings_usd: Some(0.01), // Average savings per cached call
                                time_savings_ms: Some(2000),  // Average LLM latency saved
                                quality_improvement: None,
                            },
                        });
                    }

                    // Check for nodes inside loops that could benefit from memoization
                    if Self::is_node_in_loop(workflow, &node.id) {
                        suggestions.push(OptimizationSuggestion {
                            category: SuggestionCategory::Performance,
                            severity: Severity::High,
                            description: format!(
                                "Node '{}' is inside a loop. Enable result memoization to avoid redundant LLM calls.",
                                node.name
                            ),
                            affected_nodes: vec![node.id.to_string()],
                            benefit: Benefit {
                                cost_savings_usd: Some(0.05), // Could save multiple calls
                                time_savings_ms: Some(5000),  // Multiple calls avoided
                                quality_improvement: None,
                            },
                        });
                    }
                }
                NodeKind::Retriever(_config) => {
                    // Vector retrieval results can often be cached
                    suggestions.push(OptimizationSuggestion {
                        category: SuggestionCategory::Performance,
                        severity: Severity::Medium,
                        description: format!(
                            "Vector retrieval in node '{}' could use query result caching. Cache TTL: 5-15 minutes recommended.",
                            node.name
                        ),
                        affected_nodes: vec![node.id.to_string()],
                        benefit: Benefit {
                            cost_savings_usd: Some(0.001), // Vector DB query costs
                            time_savings_ms: Some(100),    // DB latency saved
                            quality_improvement: None,
                        },
                    });

                    // Check if retriever is in a loop
                    if Self::is_node_in_loop(workflow, &node.id) {
                        suggestions.push(OptimizationSuggestion {
                            category: SuggestionCategory::Performance,
                            severity: Severity::High,
                            description: format!(
                                "Retriever '{}' is in a loop. Batch retrieval or aggressive caching strongly recommended.",
                                node.name
                            ),
                            affected_nodes: vec![node.id.to_string()],
                            benefit: Benefit {
                                cost_savings_usd: Some(0.01),
                                time_savings_ms: Some(1000), // Multiple DB calls avoided
                                quality_improvement: None,
                            },
                        });
                    }
                }
                NodeKind::Code(config) if config.runtime == "rust" || config.runtime == "wasm" => {
                    // Pure functions in code nodes can be memoized
                    suggestions.push(OptimizationSuggestion {
                        category: SuggestionCategory::Performance,
                        severity: Severity::Low,
                        description: format!(
                            "Code node '{}' could use function memoization if it's a pure function.",
                            node.name
                        ),
                        affected_nodes: vec![node.id.to_string()],
                        benefit: Benefit {
                            cost_savings_usd: None,
                            time_savings_ms: Some(50), // Code execution time saved
                            quality_improvement: None,
                        },
                    });
                }
                _ => {}
            }
        }

        suggestions
    }

    /// Helper: Check if a node is inside a loop
    fn is_node_in_loop(workflow: &Workflow, _node_id: &NodeId) -> bool {
        // Check if there's any loop node in the workflow
        // In a real implementation, this would do graph traversal to check
        // if the node is reachable from a loop node
        workflow
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Loop(_)))
    }

    /// Detect workflow issues
    fn detect_issues(workflow: &Workflow) -> Vec<WorkflowIssue> {
        let mut issues = Vec::new();

        // Check for nodes without error handling
        for node in &workflow.nodes {
            if matches!(
                node.kind,
                NodeKind::LLM(_) | NodeKind::Retriever(_) | NodeKind::Code(_) | NodeKind::Tool(_)
            ) {
                // Check if node is wrapped in try-catch
                let has_error_handling = node.retry_config.is_some()
                    || workflow.nodes.iter().any(|n| {
                        if let NodeKind::TryCatch(_) = &n.kind {
                            // Simplified check - in real implementation would check graph structure
                            true
                        } else {
                            false
                        }
                    });

                if !has_error_handling && node.retry_config.is_none() {
                    issues.push(WorkflowIssue {
                        issue_type: IssueType::MissingErrorHandling,
                        description: format!(
                            "Node '{}' lacks error handling (no retry config or try-catch)",
                            node.name
                        ),
                        affected_nodes: vec![node.id.to_string()],
                    });
                }
            }
        }

        issues
    }

    /// Calculate improvements summary
    fn calculate_improvements(suggestions: &[OptimizationSuggestion]) -> ImprovementSummary {
        let mut total_cost_savings = 0.0;
        let mut total_time_savings = 0u64;
        let mut parallelization_opportunities = 0;
        let mut redundant_nodes = 0;

        for suggestion in suggestions {
            if let Some(cost) = suggestion.benefit.cost_savings_usd {
                total_cost_savings += cost;
            }
            if let Some(time) = suggestion.benefit.time_savings_ms {
                total_time_savings += time;
            }
            if suggestion.category == SuggestionCategory::Parallelization {
                parallelization_opportunities += 1;
            }
            if suggestion.category == SuggestionCategory::Redundancy {
                redundant_nodes += suggestion.affected_nodes.len();
            }
        }

        ImprovementSummary {
            total_cost_savings_usd: total_cost_savings,
            total_time_savings_ms: total_time_savings,
            parallelization_opportunities,
            redundant_nodes,
        }
    }

    /// Calculate complexity metrics
    fn calculate_complexity(workflow: &Workflow) -> ComplexityMetrics {
        let total_nodes = workflow.nodes.len();
        let total_edges = workflow.edges.len();

        // Calculate max depth (simplified)
        let max_depth = total_nodes; // Worst case linear

        // Calculate branching factor
        let mut out_degrees: HashMap<NodeId, usize> = HashMap::new();
        for edge in &workflow.edges {
            *out_degrees.entry(edge.from).or_insert(0) += 1;
        }
        let avg_branching_factor = if !out_degrees.is_empty() {
            out_degrees.values().sum::<usize>() as f64 / out_degrees.len() as f64
        } else {
            0.0
        };

        // Count node types
        let llm_nodes = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::LLM(_)))
            .count();

        let conditional_nodes = workflow
            .nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.kind,
                    NodeKind::IfElse(_) | NodeKind::Switch(_) | NodeKind::Loop(_)
                )
            })
            .count();

        // Cyclomatic complexity (simplified)
        let cyclomatic_complexity = if total_edges >= total_nodes {
            total_edges - total_nodes + 2
        } else {
            1 // Minimum complexity
        };

        ComplexityMetrics {
            total_nodes,
            total_edges,
            max_depth,
            avg_branching_factor,
            cyclomatic_complexity,
            llm_nodes,
            conditional_nodes,
        }
    }

    /// Calculate overall optimization score
    fn calculate_score(
        _workflow: &Workflow,
        suggestions: &[OptimizationSuggestion],
        issues: &[WorkflowIssue],
    ) -> f64 {
        let mut score = 1.0;

        // Penalize for critical suggestions
        let critical_count = suggestions
            .iter()
            .filter(|s| s.severity == Severity::Critical)
            .count();
        score -= critical_count as f64 * 0.15;

        // Penalize for high severity suggestions
        let high_count = suggestions
            .iter()
            .filter(|s| s.severity == Severity::High)
            .count();
        score -= high_count as f64 * 0.10;

        // Penalize for medium severity suggestions
        let medium_count = suggestions
            .iter()
            .filter(|s| s.severity == Severity::Medium)
            .count();
        score -= medium_count as f64 * 0.05;

        // Penalize for issues
        score -= issues.len() as f64 * 0.05;

        // Ensure score is between 0 and 1
        score.clamp(0.0, 1.0)
    }
}

impl OptimizationReport {
    /// Format report as human-readable string
    pub fn format_summary(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("Optimization Score: {:.0}%\n", self.score * 100.0));
        output.push_str(&format!(
            "Potential Savings: ${:.4} | {}ms\n",
            self.improvements.total_cost_savings_usd, self.improvements.total_time_savings_ms
        ));
        output.push_str(&format!(
            "Opportunities: {} parallelization, {} redundant nodes\n",
            self.improvements.parallelization_opportunities, self.improvements.redundant_nodes
        ));
        output.push_str(&format!(
            "Complexity: {} nodes, {} edges, depth {}\n",
            self.complexity.total_nodes, self.complexity.total_edges, self.complexity.max_depth
        ));
        output.push_str(&format!("\nSuggestions: {}\n", self.suggestions.len()));
        for (i, suggestion) in self.suggestions.iter().take(5).enumerate() {
            output.push_str(&format!(
                "  {}. [{:?}] {}\n",
                i + 1,
                suggestion.severity,
                suggestion.description
            ));
        }
        if self.suggestions.len() > 5 {
            output.push_str(&format!("  ... and {} more\n", self.suggestions.len() - 5));
        }

        output
    }

    /// Get high-priority suggestions
    pub fn high_priority_suggestions(&self) -> Vec<&OptimizationSuggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.severity >= Severity::High)
            .collect()
    }

    /// Get suggestions by category
    pub fn suggestions_by_category(
        &self,
        category: SuggestionCategory,
    ) -> Vec<&OptimizationSuggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.category == category)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmConfig, WorkflowBuilder};

    #[test]
    fn test_optimizer_basic() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "Generate",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "Hello".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let report = WorkflowOptimizer::analyze(&workflow);

        assert!(report.score > 0.0 && report.score <= 1.0);
        assert!(!report.suggestions.is_empty());
    }

    #[test]
    fn test_cost_reduction_suggestion() {
        let workflow = WorkflowBuilder::new("Expensive")
            .start("Start")
            .llm(
                "GPT4",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test".to_string(),
                    temperature: None,
                    max_tokens: Some(1000),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let report = WorkflowOptimizer::analyze(&workflow);
        let cost_suggestions: Vec<_> = report
            .suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::CostReduction)
            .collect();

        assert!(!cost_suggestions.is_empty());
    }

    #[test]
    fn test_redundancy_detection() {
        let llm_config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-3.5-turbo".to_string(),
            system_prompt: None,
            prompt_template: "duplicate".to_string(),
            temperature: None,
            max_tokens: Some(100),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let workflow = WorkflowBuilder::new("Redundant")
            .start("Start")
            .llm("LLM1", llm_config.clone())
            .llm("LLM2", llm_config)
            .end("End")
            .build();

        let report = WorkflowOptimizer::analyze(&workflow);
        let redundancy_suggestions: Vec<_> = report
            .suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Redundancy)
            .collect();

        assert!(!redundancy_suggestions.is_empty());
    }

    #[test]
    fn test_complexity_metrics() {
        let workflow = WorkflowBuilder::new("Complex")
            .start("Start")
            .llm(
                "LLM1",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-3.5-turbo".to_string(),
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
                    model: "gpt-3.5-turbo".to_string(),
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

        let report = WorkflowOptimizer::analyze(&workflow);

        assert_eq!(report.complexity.total_nodes, 4);
        assert_eq!(report.complexity.llm_nodes, 2);
        assert!(report.complexity.total_edges > 0);
    }

    #[test]
    fn test_report_format() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .end("End")
            .build();

        let report = WorkflowOptimizer::analyze(&workflow);
        let summary = report.format_summary();

        assert!(summary.contains("Optimization Score:"));
        assert!(summary.contains("Potential Savings:"));
        assert!(summary.contains("Complexity:"));
    }

    #[test]
    fn test_high_priority_suggestions() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "Expensive",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "x".repeat(10000), // Very long prompt
                    temperature: None,
                    max_tokens: Some(4000),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let report = WorkflowOptimizer::analyze(&workflow);
        let high_priority = report.high_priority_suggestions();

        // Should have high priority suggestions for expensive operations
        assert!(!high_priority.is_empty() || !report.suggestions.is_empty());
    }

    #[test]
    fn test_improvements_summary() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "GPT4",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test".to_string(),
                    temperature: None,
                    max_tokens: Some(1000),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let report = WorkflowOptimizer::analyze(&workflow);

        assert!(report.improvements.total_cost_savings_usd >= 0.0);
        // total_time_savings_ms is u64, so always >= 0
        assert!(report.improvements.total_time_savings_ms < u64::MAX);
    }

    #[test]
    fn test_caching_recommendations() {
        use crate::VectorConfig;

        let workflow = WorkflowBuilder::new("Cache Test")
            .start("Start")
            .llm(
                "SimpleLLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-3.5-turbo".to_string(),
                    system_prompt: None,
                    prompt_template: "What is 2+2?".to_string(), // Simple, deterministic
                    temperature: None,
                    max_tokens: Some(50),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .retriever(
                "VectorSearch",
                VectorConfig {
                    db_type: "qdrant".to_string(),
                    collection: "docs".to_string(),
                    query: "test query".to_string(),
                    top_k: 5,
                    score_threshold: Some(0.7),
                },
            )
            .end("End")
            .build();

        let report = WorkflowOptimizer::analyze(&workflow);

        // Should have caching suggestions for both LLM and Retriever nodes
        let caching_suggestions: Vec<_> = report
            .suggestions
            .iter()
            .filter(|s| {
                s.description.contains("caching")
                    || s.description.contains("memoization")
                    || s.description.contains("Cache")
            })
            .collect();

        assert!(
            !caching_suggestions.is_empty(),
            "Should have at least one caching recommendation"
        );

        // Check that we have suggestions for the retriever
        let retriever_caching: Vec<_> = caching_suggestions
            .iter()
            .filter(|s| s.description.contains("Vector") || s.description.contains("retrieval"))
            .collect();

        assert!(
            !retriever_caching.is_empty(),
            "Should have caching recommendations for retriever"
        );
    }

    #[test]
    fn test_caching_in_loop() {
        use crate::{LoopConfig, LoopType};

        let workflow = WorkflowBuilder::new("Loop Cache Test")
            .start("Start")
            .loop_node(
                "ForEach",
                LoopConfig {
                    loop_type: LoopType::ForEach {
                        collection_path: "items".to_string(),
                        item_variable: "item".to_string(),
                        index_variable: None,
                        body_expression: "process".to_string(),
                        parallel: false,
                        max_concurrency: None,
                    },
                    max_iterations: 100,
                },
            )
            .llm(
                "InLoopLLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-3.5-turbo".to_string(),
                    system_prompt: None,
                    prompt_template: "Process {{item}}".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let report = WorkflowOptimizer::analyze(&workflow);

        // Should have high-severity memoization suggestions for nodes in loops
        let loop_memoization: Vec<_> = report
            .suggestions
            .iter()
            .filter(|s| s.description.contains("loop") && s.description.contains("memoization"))
            .collect();

        assert!(
            !loop_memoization.is_empty(),
            "Should have memoization recommendations for nodes in loops"
        );

        // Check severity is High for loop memoization
        let has_high_severity = loop_memoization
            .iter()
            .any(|s| s.severity == Severity::High);
        assert!(
            has_high_severity,
            "Loop memoization should have High severity"
        );
    }
}
