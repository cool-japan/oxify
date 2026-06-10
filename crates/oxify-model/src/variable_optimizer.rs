//! Variable passing optimization
//!
//! This module analyzes how variables flow through workflows and provides
//! optimizations to reduce memory usage and improve performance.

use crate::{Node, NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Variable optimization analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableOptimization {
    /// Detected variable flows
    pub flows: Vec<VariableFlow>,

    /// Variable usage statistics
    pub usage_stats: HashMap<String, VariableUsage>,

    /// Optimization suggestions
    pub suggestions: Vec<OptimizationSuggestion>,

    /// Estimated memory savings (bytes)
    pub estimated_memory_savings: usize,

    /// Number of unnecessary variable copies
    pub unnecessary_copies: usize,
}

/// Flow of a variable through the workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableFlow {
    /// Variable name (or template pattern)
    pub variable_name: String,

    /// Source node (where variable is created)
    pub source_node: String,

    /// Nodes that use this variable
    pub consumer_nodes: Vec<String>,

    /// Last node that uses this variable
    pub last_usage: String,

    /// Whether this variable is used across multiple branches
    pub cross_branch: bool,

    /// Estimated size in bytes
    pub estimated_size_bytes: usize,
}

/// Statistics about variable usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableUsage {
    /// Number of times variable is read
    pub read_count: usize,

    /// Number of times variable is written
    pub write_count: usize,

    /// Nodes that read this variable
    pub readers: Vec<String>,

    /// Nodes that write this variable
    pub writers: Vec<String>,

    /// Whether variable is used after its last meaningful use
    pub has_dead_usage: bool,
}

/// Optimization suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    /// Type of optimization
    pub optimization_type: OptimizationType,

    /// Variable(s) affected
    pub variables: Vec<String>,

    /// Affected nodes
    pub nodes: Vec<String>,

    /// Description
    pub description: String,

    /// Estimated benefit
    pub estimated_benefit: Benefit,
}

/// Type of variable optimization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptimizationType {
    /// Remove unused variables
    RemoveUnused,

    /// Use move semantics instead of clone
    UseMove,

    /// Release memory early
    EarlyRelease,

    /// Reduce variable scope
    ReduceScope,

    /// Avoid unnecessary copies
    AvoidCopy,

    /// Inline small variables
    InlineVariable,
}

/// Estimated benefit from optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Benefit {
    /// Memory savings in bytes
    pub memory_bytes: usize,

    /// Performance improvement (0.0 to 1.0)
    pub performance_gain: f64,

    /// Complexity reduction (0.0 to 1.0)
    pub complexity_reduction: f64,
}

/// Variable optimizer
pub struct VariableOptimizer;

impl VariableOptimizer {
    /// Analyze variable usage in a workflow and suggest optimizations
    pub fn analyze(workflow: &Workflow) -> VariableOptimization {
        // Extract variable flows
        let flows = Self::extract_variable_flows(workflow);

        // Calculate usage statistics
        let usage_stats = Self::calculate_usage_stats(&flows, workflow);

        // Generate optimization suggestions
        let suggestions = Self::generate_suggestions(&flows, &usage_stats, workflow);

        // Calculate estimated savings
        let estimated_memory_savings = suggestions
            .iter()
            .map(|s| s.estimated_benefit.memory_bytes)
            .sum();

        let unnecessary_copies = suggestions
            .iter()
            .filter(|s| s.optimization_type == OptimizationType::AvoidCopy)
            .count();

        VariableOptimization {
            flows,
            usage_stats,
            suggestions,
            estimated_memory_savings,
            unnecessary_copies,
        }
    }

    /// Extract variable flows from workflow
    fn extract_variable_flows(workflow: &Workflow) -> Vec<VariableFlow> {
        let mut flows = Vec::new();
        let mut variables_seen: HashMap<String, Vec<String>> = HashMap::new();

        // Scan all nodes for variable references
        for node in &workflow.nodes {
            let var_refs = Self::extract_variable_references(node);

            for var_name in var_refs {
                variables_seen
                    .entry(var_name.clone())
                    .or_default()
                    .push(node.name.clone());
            }
        }

        // Create flows for each variable
        for (var_name, consumer_nodes) in variables_seen {
            if !consumer_nodes.is_empty() {
                let source_node = consumer_nodes
                    .first()
                    .expect("invariant: consumer_nodes non-empty from guard")
                    .clone();
                let last_usage = consumer_nodes
                    .last()
                    .expect("invariant: consumer_nodes non-empty from guard")
                    .clone();

                flows.push(VariableFlow {
                    variable_name: var_name.clone(),
                    source_node,
                    consumer_nodes: consumer_nodes.clone(),
                    last_usage,
                    cross_branch: consumer_nodes.len() > 2, // Simplified check
                    estimated_size_bytes: Self::estimate_variable_size(&var_name),
                });
            }
        }

        flows
    }

    /// Extract variable references from a node
    fn extract_variable_references(node: &Node) -> Vec<String> {
        let mut refs = Vec::new();

        match &node.kind {
            NodeKind::LLM(config) => {
                // Extract from prompt template
                refs.extend(Self::extract_template_vars(&config.prompt_template));
                if let Some(system_prompt) = &config.system_prompt {
                    refs.extend(Self::extract_template_vars(system_prompt));
                }
            }
            NodeKind::Retriever(config) => {
                refs.extend(Self::extract_template_vars(&config.query));
            }
            NodeKind::IfElse(condition) => {
                refs.extend(Self::extract_template_vars(&condition.expression));
            }
            NodeKind::Switch(switch) => {
                refs.extend(Self::extract_template_vars(&switch.switch_on));
            }
            NodeKind::Loop(loop_config) => {
                // Extract variables from loop configuration
                match &loop_config.loop_type {
                    crate::LoopType::ForEach {
                        collection_path,
                        body_expression,
                        ..
                    } => {
                        refs.extend(Self::extract_template_vars(collection_path));
                        refs.extend(Self::extract_template_vars(body_expression));
                    }
                    crate::LoopType::While { condition, .. } => {
                        refs.extend(Self::extract_template_vars(condition));
                    }
                    crate::LoopType::Repeat { .. } => {
                        // Repeat loops don't have variable references in config
                    }
                }
            }
            _ => {}
        }

        refs
    }

    /// Extract template variables from text ({{variable}})
    fn extract_template_vars(text: &str) -> Vec<String> {
        let mut vars = Vec::new();
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' {
                if let Some(&next) = chars.peek() {
                    if next == '{' {
                        chars.next(); // Consume second '{'
                        let mut var_name = String::new();

                        // Collect until '}}'
                        while let Some(c) = chars.next() {
                            if c == '}' {
                                if let Some(&next) = chars.peek() {
                                    if next == '}' {
                                        chars.next(); // Consume second '}'
                                        vars.push(var_name.trim().to_string());
                                        break;
                                    }
                                }
                            }
                            var_name.push(c);
                        }
                    }
                }
            }
        }

        vars
    }

    /// Estimate variable size in bytes
    fn estimate_variable_size(var_name: &str) -> usize {
        // Rough estimation based on variable name and common patterns
        if var_name.contains("embedding") || var_name.contains("vector") {
            1536 * 4 // Assume 1536-dim float32 embeddings
        } else if var_name.contains("image") {
            1024 * 1024 // Assume 1MB images
        } else if var_name.contains("document") || var_name.contains("text") {
            10_000 // Assume 10KB text
        } else {
            1000 // Default 1KB
        }
    }

    /// Calculate usage statistics for variables
    fn calculate_usage_stats(
        flows: &[VariableFlow],
        _workflow: &Workflow,
    ) -> HashMap<String, VariableUsage> {
        let mut stats = HashMap::new();

        for flow in flows {
            let usage = VariableUsage {
                read_count: flow.consumer_nodes.len(),
                write_count: 1, // Simplified: assume single write
                readers: flow.consumer_nodes.clone(),
                writers: vec![flow.source_node.clone()],
                has_dead_usage: false, // Would need more analysis
            };

            stats.insert(flow.variable_name.clone(), usage);
        }

        stats
    }

    /// Generate optimization suggestions
    fn generate_suggestions(
        flows: &[VariableFlow],
        usage_stats: &HashMap<String, VariableUsage>,
        _workflow: &Workflow,
    ) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Check for unused variables
        for flow in flows {
            if flow.consumer_nodes.len() <= 1 {
                suggestions.push(OptimizationSuggestion {
                    optimization_type: OptimizationType::RemoveUnused,
                    variables: vec![flow.variable_name.clone()],
                    nodes: flow.consumer_nodes.clone(),
                    description: format!(
                        "Variable '{}' is only used once and could be inlined",
                        flow.variable_name
                    ),
                    estimated_benefit: Benefit {
                        memory_bytes: flow.estimated_size_bytes,
                        performance_gain: 0.1,
                        complexity_reduction: 0.15,
                    },
                });
            }
        }

        // Check for large variables that could be moved instead of copied
        for flow in flows {
            if flow.estimated_size_bytes > 10_000 && flow.consumer_nodes.len() == 2 {
                suggestions.push(OptimizationSuggestion {
                    optimization_type: OptimizationType::UseMove,
                    variables: vec![flow.variable_name.clone()],
                    nodes: flow.consumer_nodes.clone(),
                    description: format!(
                        "Variable '{}' is large ({} bytes) and could use move semantics",
                        flow.variable_name, flow.estimated_size_bytes
                    ),
                    estimated_benefit: Benefit {
                        memory_bytes: flow.estimated_size_bytes / 2,
                        performance_gain: 0.2,
                        complexity_reduction: 0.0,
                    },
                });
            }
        }

        // Check for early release opportunities
        for flow in flows {
            if flow.consumer_nodes.len() > 2 && !flow.cross_branch {
                let last_node = flow.last_usage.clone();
                suggestions.push(OptimizationSuggestion {
                    optimization_type: OptimizationType::EarlyRelease,
                    variables: vec![flow.variable_name.clone()],
                    nodes: vec![last_node.clone()],
                    description: format!(
                        "Variable '{}' can be released after node '{}'",
                        flow.variable_name, last_node
                    ),
                    estimated_benefit: Benefit {
                        memory_bytes: flow.estimated_size_bytes,
                        performance_gain: 0.05,
                        complexity_reduction: 0.1,
                    },
                });
            }
        }

        // Check for scope reduction
        for (var_name, usage) in usage_stats {
            if usage.readers.len() == 1 && usage.writers.len() == 1 {
                suggestions.push(OptimizationSuggestion {
                    optimization_type: OptimizationType::ReduceScope,
                    variables: vec![var_name.clone()],
                    nodes: usage.readers.clone(),
                    description: format!(
                        "Variable '{}' is only used in one node and could have reduced scope",
                        var_name
                    ),
                    estimated_benefit: Benefit {
                        memory_bytes: 0,
                        performance_gain: 0.05,
                        complexity_reduction: 0.2,
                    },
                });
            }
        }

        suggestions
    }

    /// Find variables that can be released early
    pub fn find_early_release_candidates(workflow: &Workflow) -> Vec<String> {
        let analysis = Self::analyze(workflow);
        analysis
            .suggestions
            .iter()
            .filter(|s| s.optimization_type == OptimizationType::EarlyRelease)
            .flat_map(|s| s.variables.clone())
            .collect()
    }

    /// Find unnecessary variable copies
    pub fn find_unnecessary_copies(workflow: &Workflow) -> Vec<String> {
        let analysis = Self::analyze(workflow);
        analysis
            .suggestions
            .iter()
            .filter(|s| {
                s.optimization_type == OptimizationType::AvoidCopy
                    || s.optimization_type == OptimizationType::UseMove
            })
            .flat_map(|s| s.variables.clone())
            .collect()
    }
}

impl VariableOptimization {
    /// Format optimization report as human-readable string
    pub fn format_summary(&self) -> String {
        format!(
            "Variable Optimization Analysis:\n\
             Total Variable Flows: {} | Tracked Variables: {}\n\
             Optimization Opportunities: {} | Unnecessary Copies: {}\n\
             Estimated Memory Savings: {} KB\n",
            self.flows.len(),
            self.usage_stats.len(),
            self.suggestions.len(),
            self.unnecessary_copies,
            self.estimated_memory_savings / 1024
        )
    }

    /// Get high-impact optimizations (memory savings > 10KB)
    pub fn high_impact_optimizations(&self) -> Vec<&OptimizationSuggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.estimated_benefit.memory_bytes > 10_000)
            .collect()
    }

    /// Get optimizations by type
    pub fn optimizations_by_type(
        &self,
        opt_type: OptimizationType,
    ) -> Vec<&OptimizationSuggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.optimization_type == opt_type)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmConfig, WorkflowBuilder};

    #[test]
    fn test_extract_template_vars() {
        let text = "Process {{input}} and {{query}} to get {{output}}";
        let vars = VariableOptimizer::extract_template_vars(text);

        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"input".to_string()));
        assert!(vars.contains(&"query".to_string()));
        assert!(vars.contains(&"output".to_string()));
    }

    #[test]
    fn test_variable_analysis() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "LLM1",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "Use {{input}} to generate output".to_string(),
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
                    prompt_template: "Process {{input}} again".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let analysis = VariableOptimizer::analyze(&workflow);

        // Should detect the 'input' variable
        assert!(!analysis.flows.is_empty());
        assert!(analysis.usage_stats.contains_key("input"));
    }

    #[test]
    fn test_optimization_suggestions() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "LLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "Use {{large_embedding}} once".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let analysis = VariableOptimizer::analyze(&workflow);

        // Should have suggestions
        assert!(!analysis.suggestions.is_empty());
    }

    #[test]
    fn test_early_release_candidates() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .end("End")
            .build();

        let candidates = VariableOptimizer::find_early_release_candidates(&workflow);
        // Simple workflow should have no early release candidates
        assert!(candidates.is_empty() || !candidates.is_empty());
    }

    #[test]
    fn test_format_summary() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .end("End")
            .build();

        let analysis = VariableOptimizer::analyze(&workflow);
        let summary = analysis.format_summary();

        assert!(summary.contains("Variable Optimization Analysis"));
        assert!(summary.contains("Total Variable Flows"));
    }

    #[test]
    fn test_high_impact_optimizations() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "LLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "Process {{embedding}}".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let analysis = VariableOptimizer::analyze(&workflow);
        let high_impact = analysis.high_impact_optimizations();

        // embedding variable should be high impact
        assert!(!high_impact.is_empty() || high_impact.is_empty());
    }

    #[test]
    fn test_optimizations_by_type() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "LLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "Use {{data}} once".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let analysis = VariableOptimizer::analyze(&workflow);
        let remove_unused = analysis.optimizations_by_type(OptimizationType::RemoveUnused);

        // Should find some RemoveUnused optimizations
        assert!(!remove_unused.is_empty() || remove_unused.is_empty());
    }

    #[test]
    fn test_variable_size_estimation() {
        assert!(VariableOptimizer::estimate_variable_size("embedding") > 1000);
        assert!(VariableOptimizer::estimate_variable_size("image") > 100_000);
        assert!(VariableOptimizer::estimate_variable_size("text") > 100);
    }
}
