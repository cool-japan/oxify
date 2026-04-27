//! Workflow optimization engine
//!
//! Analyzes workflows and suggests optimizations for better performance,
//! cost efficiency, and reliability.

use oxify_model::{NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Optimization suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Optimization {
    /// Optimization category
    pub category: OptimizationCategory,

    /// Priority level
    pub priority: Priority,

    /// Title of the optimization
    pub title: String,

    /// Detailed description
    pub description: String,

    /// Node IDs affected by this optimization
    pub affected_nodes: Vec<uuid::Uuid>,

    /// Expected impact
    pub impact: Impact,

    /// Suggested action
    pub action: String,
}

/// Optimization category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationCategory {
    Performance,
    Cost,
    Reliability,
    Maintainability,
    Security,
}

/// Priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Expected impact of optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Impact {
    /// Estimated time saved (percentage)
    pub time_savings: Option<f32>,

    /// Estimated cost reduction (percentage)
    pub cost_reduction: Option<f32>,

    /// Reliability improvement description
    pub reliability_improvement: Option<String>,
}

/// Workflow optimizer
pub struct WorkflowOptimizer {
    /// Enable strict mode (more aggressive suggestions)
    strict_mode: bool,
}

impl Default for WorkflowOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowOptimizer {
    /// Create a new optimizer
    pub fn new() -> Self {
        Self { strict_mode: false }
    }

    /// Create optimizer with strict mode
    pub fn with_strict_mode() -> Self {
        Self { strict_mode: true }
    }

    /// Analyze workflow and generate optimization suggestions
    pub fn optimize(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut optimizations = Vec::new();

        // Check for performance optimizations
        optimizations.extend(self.check_parallel_opportunities(workflow));
        optimizations.extend(self.check_caching_opportunities(workflow));
        optimizations.extend(self.check_redundant_nodes(workflow));

        // Check for cost optimizations
        optimizations.extend(self.check_model_selection(workflow));
        optimizations.extend(self.check_batching_opportunities(workflow));

        // Check for reliability optimizations
        optimizations.extend(self.check_error_handling(workflow));
        optimizations.extend(self.check_retry_policies(workflow));
        optimizations.extend(self.check_timeout_settings(workflow));

        // Check for maintainability
        optimizations.extend(self.check_naming_conventions(workflow));
        optimizations.extend(self.check_complexity(workflow));

        // Sort by priority
        optimizations.sort_by_key(|x| std::cmp::Reverse(x.priority));

        optimizations
    }

    /// Check for opportunities to parallelize sequential operations
    fn check_parallel_opportunities(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        // Build dependency graph
        let mut dependencies: HashMap<uuid::Uuid, HashSet<uuid::Uuid>> = HashMap::new();
        for edge in &workflow.edges {
            dependencies.entry(edge.to).or_default().insert(edge.from);
        }

        // Find nodes that could be parallelized
        let mut sequential_groups: Vec<Vec<uuid::Uuid>> = Vec::new();
        let mut current_group = Vec::new();

        for node in &workflow.nodes {
            let deps = dependencies.get(&node.id).map(|s| s.len()).unwrap_or(0);

            if deps <= 1 && !matches!(node.kind, NodeKind::Start | NodeKind::End) {
                current_group.push(node.id);
            } else if !current_group.is_empty() {
                if current_group.len() >= 2 {
                    sequential_groups.push(current_group.clone());
                }
                current_group.clear();
            }
        }

        // Suggest parallelization for groups
        for group in sequential_groups {
            if group.len() >= 2 {
                let group_len = group.len();
                opts.push(Optimization {
                    category: OptimizationCategory::Performance,
                    priority: if group_len >= 4 {
                        Priority::High
                    } else {
                        Priority::Medium
                    },
                    title: format!("Parallelize {} sequential operations", group_len),
                    description: format!(
                        "Found {} nodes that could potentially execute in parallel. \
                        Consider using a Parallel node to improve execution time.",
                        group_len
                    ),
                    affected_nodes: group,
                    impact: Impact {
                        time_savings: Some(30.0 + (group_len as f32 * 10.0)),
                        cost_reduction: None,
                        reliability_improvement: None,
                    },
                    action: "Wrap these nodes in a Parallel execution block".to_string(),
                });
            }
        }

        opts
    }

    /// Check for caching opportunities
    fn check_caching_opportunities(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        // Count LLM nodes
        let llm_nodes: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::LLM(_)))
            .collect();

        if llm_nodes.len() >= 3 {
            opts.push(Optimization {
                category: OptimizationCategory::Cost,
                priority: Priority::High,
                title: "Enable response caching for LLM calls".to_string(),
                description: format!(
                    "Found {} LLM nodes. Enabling caching for repeated prompts could \
                    significantly reduce costs and improve response times.",
                    llm_nodes.len()
                ),
                affected_nodes: llm_nodes.iter().map(|n| n.id).collect(),
                impact: Impact {
                    time_savings: Some(40.0),
                    cost_reduction: Some(60.0),
                    reliability_improvement: Some("Reduced API rate limiting".to_string()),
                },
                action: "Enable LLM response caching in engine configuration".to_string(),
            });
        }

        opts
    }

    /// Check for redundant or duplicate nodes
    fn check_redundant_nodes(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        // Group nodes by kind and configuration
        let mut node_signatures: HashMap<String, Vec<uuid::Uuid>> = HashMap::new();

        for node in &workflow.nodes {
            let signature = format!("{:?}", node.kind);
            node_signatures.entry(signature).or_default().push(node.id);
        }

        // Find duplicates
        for (signature, nodes) in node_signatures {
            if nodes.len() >= 2 && !signature.contains("Start") && !signature.contains("End") {
                opts.push(Optimization {
                    category: OptimizationCategory::Maintainability,
                    priority: Priority::Low,
                    title: format!("Potential duplicate nodes ({})", nodes.len()),
                    description: format!(
                        "Found {} nodes with similar configurations. Consider \
                        consolidating them or using loops/sub-workflows.",
                        nodes.len()
                    ),
                    affected_nodes: nodes,
                    impact: Impact {
                        time_savings: None,
                        cost_reduction: None,
                        reliability_improvement: Some("Easier to maintain".to_string()),
                    },
                    action: "Review nodes for consolidation opportunities".to_string(),
                });
            }
        }

        opts
    }

    /// Check for optimal model selection
    fn check_model_selection(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        for node in &workflow.nodes {
            if let NodeKind::LLM(llm_config) = &node.kind {
                // Suggest cheaper models for simple tasks
                if llm_config.model.contains("gpt-4") && self.strict_mode {
                    opts.push(Optimization {
                        category: OptimizationCategory::Cost,
                        priority: Priority::Medium,
                        title: format!("Consider cheaper model for node '{}'", node.name),
                        description: format!(
                            "Node '{}' uses '{}'. For simpler tasks, consider using \
                            GPT-3.5-Turbo or Claude Haiku for 90% cost reduction.",
                            node.name, llm_config.model
                        ),
                        affected_nodes: vec![node.id],
                        impact: Impact {
                            time_savings: None,
                            cost_reduction: Some(90.0),
                            reliability_improvement: None,
                        },
                        action: "Evaluate if a cheaper model meets requirements".to_string(),
                    });
                }
            }
        }

        opts
    }

    /// Check for batching opportunities
    fn check_batching_opportunities(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        // Count retriever nodes
        let retriever_nodes: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Retriever(_)))
            .collect();

        if retriever_nodes.len() >= 2 {
            opts.push(Optimization {
                category: OptimizationCategory::Performance,
                priority: Priority::High,
                title: "Batch vector search operations".to_string(),
                description: format!(
                    "Found {} vector search nodes. Batching these operations could \
                    improve performance by 3-5x.",
                    retriever_nodes.len()
                ),
                affected_nodes: retriever_nodes.iter().map(|n| n.id).collect(),
                impact: Impact {
                    time_savings: Some(70.0),
                    cost_reduction: Some(40.0),
                    reliability_improvement: None,
                },
                action: "Enable automatic batching for vector searches".to_string(),
            });
        }

        opts
    }

    /// Check for proper error handling
    fn check_error_handling(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        // Check if critical nodes have error handling
        let has_try_catch = workflow
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::TryCatch(_)));

        let critical_node_count = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::LLM(_) | NodeKind::Retriever(_)))
            .count();

        if !has_try_catch && critical_node_count > 0 {
            opts.push(Optimization {
                category: OptimizationCategory::Reliability,
                priority: Priority::High,
                title: "Add error handling".to_string(),
                description: format!(
                    "Workflow has {} critical nodes but no error handling. \
                    Add TryCatch blocks to improve reliability.",
                    critical_node_count
                ),
                affected_nodes: Vec::new(),
                impact: Impact {
                    time_savings: None,
                    cost_reduction: None,
                    reliability_improvement: Some("Graceful error recovery".to_string()),
                },
                action: "Wrap critical operations in TryCatch blocks".to_string(),
            });
        }

        opts
    }

    /// Check for retry policies
    fn check_retry_policies(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        // Count nodes that could benefit from retries
        let mut nodes_without_retry = Vec::new();

        for node in &workflow.nodes {
            if matches!(node.kind, NodeKind::LLM(_) | NodeKind::Retriever(_)) {
                // Check if node has retry config (would need to check in actual NodeKind)
                nodes_without_retry.push(node.id);
            }
        }

        if !nodes_without_retry.is_empty() {
            opts.push(Optimization {
                category: OptimizationCategory::Reliability,
                priority: Priority::Medium,
                title: "Configure retry policies".to_string(),
                description: format!(
                    "{} nodes could benefit from automatic retry on transient failures.",
                    nodes_without_retry.len()
                ),
                affected_nodes: nodes_without_retry,
                impact: Impact {
                    time_savings: None,
                    cost_reduction: None,
                    reliability_improvement: Some("Handle transient failures".to_string()),
                },
                action: "Add retry configuration with exponential backoff".to_string(),
            });
        }

        opts
    }

    /// Check for timeout settings
    fn check_timeout_settings(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        // Check for nodes that might need timeouts
        let nodes_needing_timeout = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Loop(_) | NodeKind::SubWorkflow(_)))
            .map(|n| n.id)
            .collect::<Vec<_>>();

        if !nodes_needing_timeout.is_empty() && self.strict_mode {
            opts.push(Optimization {
                category: OptimizationCategory::Reliability,
                priority: Priority::Medium,
                title: "Add timeout protection".to_string(),
                description: "Loop and sub-workflow nodes should have timeout limits \
                             to prevent infinite execution."
                    .to_string(),
                affected_nodes: nodes_needing_timeout,
                impact: Impact {
                    time_savings: None,
                    cost_reduction: Some(100.0), // Prevent runaway costs
                    reliability_improvement: Some("Prevent hanging executions".to_string()),
                },
                action: "Configure max execution time for long-running operations".to_string(),
            });
        }

        opts
    }

    /// Check naming conventions
    fn check_naming_conventions(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        let poorly_named = workflow
            .nodes
            .iter()
            .filter(|n| {
                let name = n.name.to_lowercase();
                name == "node" || name == "untitled" || name.starts_with("node_")
            })
            .map(|n| n.id)
            .collect::<Vec<_>>();

        if !poorly_named.is_empty() {
            opts.push(Optimization {
                category: OptimizationCategory::Maintainability,
                priority: Priority::Low,
                title: "Improve node naming".to_string(),
                description: format!(
                    "{} nodes have generic names. Use descriptive names for better maintainability.",
                    poorly_named.len()
                ),
                affected_nodes: poorly_named,
                impact: Impact {
                    time_savings: None,
                    cost_reduction: None,
                    reliability_improvement: Some("Easier debugging and maintenance".to_string()),
                },
                action: "Rename nodes with descriptive, action-oriented names".to_string(),
            });
        }

        opts
    }

    /// Check workflow complexity
    fn check_complexity(&self, workflow: &Workflow) -> Vec<Optimization> {
        let mut opts = Vec::new();

        let node_count = workflow.nodes.len();
        let edge_count = workflow.edges.len();

        // Calculate cyclomatic complexity (simplified)
        let complexity = edge_count - node_count + 2;

        if complexity > 20 {
            opts.push(Optimization {
                category: OptimizationCategory::Maintainability,
                priority: Priority::High,
                title: "High workflow complexity".to_string(),
                description: format!(
                    "Workflow has complexity score of {}. Consider breaking it into \
                    smaller sub-workflows for better maintainability.",
                    complexity
                ),
                affected_nodes: Vec::new(),
                impact: Impact {
                    time_savings: None,
                    cost_reduction: None,
                    reliability_improvement: Some("Easier to test and debug".to_string()),
                },
                action: "Refactor into smaller, focused sub-workflows".to_string(),
            });
        }

        if node_count > 30 {
            opts.push(Optimization {
                category: OptimizationCategory::Maintainability,
                priority: Priority::Medium,
                title: "Large workflow detected".to_string(),
                description: format!(
                    "Workflow has {} nodes. Large workflows are harder to maintain and debug.",
                    node_count
                ),
                affected_nodes: Vec::new(),
                impact: Impact {
                    time_savings: None,
                    cost_reduction: None,
                    reliability_improvement: Some("Modular architecture".to_string()),
                },
                action: "Consider splitting into multiple workflows".to_string(),
            });
        }

        opts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimizer_basic() {
        let workflow = Workflow::new("Test Workflow".to_string());
        let optimizer = WorkflowOptimizer::new();

        let optimizations = optimizer.optimize(&workflow);

        // Empty workflow should have no optimizations
        assert!(optimizations.is_empty());
    }

    #[test]
    fn test_optimization_priority_ordering() {
        let workflow = Workflow::new("Test".to_string());
        let optimizer = WorkflowOptimizer::with_strict_mode();

        let optimizations = optimizer.optimize(&workflow);

        // Verify optimizations are sorted by priority
        for i in 1..optimizations.len() {
            assert!(optimizations[i - 1].priority >= optimizations[i].priority);
        }
    }
}
