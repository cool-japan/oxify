//! Cost estimation for workflow execution
//!
//! This module provides cost estimation for LLM calls, vector operations,
//! and overall workflow execution costs.

use crate::{LlmConfig, Node, NodeKind, VectorConfig, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Cost breakdown for a workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    /// Total estimated cost in USD
    pub total_usd: f64,

    /// Cost breakdown by node
    pub node_costs: HashMap<String, NodeCost>,

    /// Cost breakdown by category
    pub category_costs: CategoryCosts,

    /// Token usage estimates
    pub token_estimates: TokenEstimates,
}

/// Cost for a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCost {
    /// Node name
    pub node_name: String,

    /// Node type
    pub node_type: String,

    /// Estimated cost in USD
    pub cost_usd: f64,

    /// Number of expected executions (for loops, retries, etc.)
    pub expected_executions: u32,

    /// Breakdown of cost components
    pub components: Vec<CostComponent>,
}

/// Individual cost component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostComponent {
    /// Component name (e.g., "input_tokens", "output_tokens", "api_call")
    pub name: String,

    /// Cost in USD
    pub cost_usd: f64,

    /// Quantity (tokens, API calls, etc.)
    pub quantity: f64,

    /// Unit (e.g., "tokens", "calls", "MB")
    pub unit: String,
}

/// Cost breakdown by category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryCosts {
    /// Total LLM costs
    pub llm_total: f64,

    /// Total vector database costs
    pub vector_total: f64,

    /// Total code execution costs
    pub code_total: f64,

    /// Total tool/MCP costs
    pub tool_total: f64,

    /// Other costs
    pub other_total: f64,
}

/// Token usage estimates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEstimates {
    /// Total input tokens across all LLM nodes
    pub total_input_tokens: u64,

    /// Total output tokens across all LLM nodes
    pub total_output_tokens: u64,

    /// Total tokens (input + output)
    pub total_tokens: u64,
}

/// Pricing information for LLM models
#[derive(Debug, Clone)]
pub struct ModelPricing {
    /// Cost per 1M input tokens in USD
    pub input_cost_per_million: f64,

    /// Cost per 1M output tokens in USD
    pub output_cost_per_million: f64,
}

impl ModelPricing {
    /// Get pricing for a model
    pub fn for_model(provider: &str, model: &str) -> Self {
        match (
            provider.to_lowercase().as_str(),
            model.to_lowercase().as_str(),
        ) {
            // OpenAI GPT-4 models
            ("openai", m) if m.contains("gpt-4-turbo") => Self {
                input_cost_per_million: 10.0,
                output_cost_per_million: 30.0,
            },
            ("openai", m) if m.contains("gpt-4") => Self {
                input_cost_per_million: 30.0,
                output_cost_per_million: 60.0,
            },
            // OpenAI GPT-3.5 models
            ("openai", m) if m.contains("gpt-3.5-turbo") => Self {
                input_cost_per_million: 0.5,
                output_cost_per_million: 1.5,
            },
            // Anthropic Claude models
            ("anthropic", m) if m.contains("claude-3-opus") => Self {
                input_cost_per_million: 15.0,
                output_cost_per_million: 75.0,
            },
            ("anthropic", m) if m.contains("claude-3-sonnet") => Self {
                input_cost_per_million: 3.0,
                output_cost_per_million: 15.0,
            },
            ("anthropic", m) if m.contains("claude-3-haiku") => Self {
                input_cost_per_million: 0.25,
                output_cost_per_million: 1.25,
            },
            // Local/Ollama models (free but with compute costs)
            ("ollama", _) | ("local", _) => Self {
                input_cost_per_million: 0.0,
                output_cost_per_million: 0.0,
            },
            // Default pricing (conservative estimate)
            _ => Self {
                input_cost_per_million: 5.0,
                output_cost_per_million: 15.0,
            },
        }
    }

    /// Calculate cost for given token counts
    pub fn calculate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_cost_per_million;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_cost_per_million;
        input_cost + output_cost
    }
}

/// Cost estimator for workflows
pub struct CostEstimator;

impl CostEstimator {
    /// Estimate cost for a workflow
    pub fn estimate(workflow: &Workflow) -> CostEstimate {
        let mut node_costs = HashMap::new();
        let mut llm_total = 0.0;
        let mut vector_total = 0.0;
        let mut code_total = 0.0;
        let mut tool_total = 0.0;
        let mut other_total = 0.0;
        let mut total_input_tokens = 0u64;
        let mut total_output_tokens = 0u64;

        for node in &workflow.nodes {
            let node_cost = Self::estimate_node_cost(node);

            // Update category totals
            match &node.kind {
                NodeKind::LLM(_) => llm_total += node_cost.cost_usd,
                NodeKind::Retriever(_) => vector_total += node_cost.cost_usd,
                NodeKind::Code(_) => code_total += node_cost.cost_usd,
                NodeKind::Tool(_) => tool_total += node_cost.cost_usd,
                _ => other_total += node_cost.cost_usd,
            }

            // Update token estimates
            for component in &node_cost.components {
                match component.name.as_str() {
                    "input_tokens" => total_input_tokens += component.quantity as u64,
                    "output_tokens" => total_output_tokens += component.quantity as u64,
                    _ => {}
                }
            }

            node_costs.insert(node.id.to_string(), node_cost);
        }

        let total_usd = llm_total + vector_total + code_total + tool_total + other_total;

        CostEstimate {
            total_usd,
            node_costs,
            category_costs: CategoryCosts {
                llm_total,
                vector_total,
                code_total,
                tool_total,
                other_total,
            },
            token_estimates: TokenEstimates {
                total_input_tokens,
                total_output_tokens,
                total_tokens: total_input_tokens + total_output_tokens,
            },
        }
    }

    /// Estimate cost for a single node
    fn estimate_node_cost(node: &Node) -> NodeCost {
        let mut components = Vec::new();
        let expected_executions = Self::estimate_executions(node);

        let cost_usd = match &node.kind {
            NodeKind::LLM(config) => {
                let (input_tokens, output_tokens) = Self::estimate_llm_tokens(config);
                let pricing = ModelPricing::for_model(&config.provider, &config.model);

                let input_cost = pricing.calculate_cost(input_tokens, 0);
                let output_cost = pricing.calculate_cost(0, output_tokens);

                components.push(CostComponent {
                    name: "input_tokens".to_string(),
                    cost_usd: input_cost,
                    quantity: input_tokens as f64,
                    unit: "tokens".to_string(),
                });

                components.push(CostComponent {
                    name: "output_tokens".to_string(),
                    cost_usd: output_cost,
                    quantity: output_tokens as f64,
                    unit: "tokens".to_string(),
                });

                (input_cost + output_cost) * expected_executions as f64
            }
            NodeKind::Retriever(config) => {
                let vector_cost = Self::estimate_vector_cost(config);
                components.push(CostComponent {
                    name: "vector_search".to_string(),
                    cost_usd: vector_cost,
                    quantity: config.top_k as f64,
                    unit: "results".to_string(),
                });
                vector_cost * expected_executions as f64
            }
            NodeKind::Code(_) => {
                // Estimate compute cost (very rough estimate)
                let compute_cost = 0.0001; // $0.0001 per execution
                components.push(CostComponent {
                    name: "compute".to_string(),
                    cost_usd: compute_cost,
                    quantity: 1.0,
                    unit: "execution".to_string(),
                });
                compute_cost * expected_executions as f64
            }
            NodeKind::Tool(_) => {
                // Estimate tool/API call cost
                let api_cost = 0.001; // $0.001 per call
                components.push(CostComponent {
                    name: "api_call".to_string(),
                    cost_usd: api_cost,
                    quantity: 1.0,
                    unit: "call".to_string(),
                });
                api_cost * expected_executions as f64
            }
            _ => {
                // Start, End, IfElse, etc. have no cost
                0.0
            }
        };

        NodeCost {
            node_name: node.name.clone(),
            node_type: match &node.kind {
                NodeKind::Start => "Start".to_string(),
                NodeKind::End => "End".to_string(),
                NodeKind::LLM(_) => "LLM".to_string(),
                NodeKind::Retriever(_) => "Retriever".to_string(),
                NodeKind::Code(_) => "Code".to_string(),
                NodeKind::IfElse(_) => "IfElse".to_string(),
                NodeKind::Tool(_) => "Tool".to_string(),
                NodeKind::Loop(_) => "Loop".to_string(),
                NodeKind::TryCatch(_) => "TryCatch".to_string(),
                NodeKind::SubWorkflow(_) => "SubWorkflow".to_string(),
                NodeKind::Switch(_) => "Switch".to_string(),
                NodeKind::Parallel(_) => "Parallel".to_string(),
                NodeKind::Approval(_) => "Approval".to_string(),
                NodeKind::Form(_) => "Form".to_string(),
                NodeKind::Vision(_) => "Vision".to_string(),
            },
            cost_usd,
            expected_executions,
            components,
        }
    }

    /// Estimate number of executions for a node (considering retries, loops, etc.)
    fn estimate_executions(node: &Node) -> u32 {
        let mut executions = 1u32;

        // Account for retries
        if let Some(retry_config) = &node.retry_config {
            // Assume 30% failure rate requiring retries
            let avg_retries = (retry_config.max_retries as f32 * 0.3).ceil() as u32;
            executions += avg_retries;
        }

        executions
    }

    /// Estimate token usage for an LLM node
    fn estimate_llm_tokens(config: &LlmConfig) -> (u64, u64) {
        // Estimate input tokens based on prompt template length
        let system_prompt_tokens = config
            .system_prompt
            .as_ref()
            .map(|s| Self::estimate_token_count(s))
            .unwrap_or(0);

        let user_prompt_tokens = Self::estimate_token_count(&config.prompt_template);
        let input_tokens = system_prompt_tokens + user_prompt_tokens + 100; // +100 for context

        // Estimate output tokens
        let output_tokens = config.max_tokens.unwrap_or(1000) as u64;

        (input_tokens, output_tokens)
    }

    /// Rough estimate of token count from text (1 token ≈ 4 characters)
    fn estimate_token_count(text: &str) -> u64 {
        (text.len() as f64 / 4.0).ceil() as u64
    }

    /// Estimate cost for vector database operations
    fn estimate_vector_cost(config: &VectorConfig) -> f64 {
        match config.db_type.to_lowercase().as_str() {
            "qdrant" => {
                // Qdrant cloud pricing: ~$0.0001 per 1000 searches
                (config.top_k as f64 / 1000.0) * 0.0001
            }
            "pgvector" => {
                // PostgreSQL compute cost estimate
                0.00001 // $0.00001 per query
            }
            _ => 0.00001, // Default estimate
        }
    }
}

impl CostEstimate {
    /// Format cost estimate as a human-readable string
    pub fn format_summary(&self) -> String {
        format!(
            "Total Cost: ${:.4}\n\
             LLM: ${:.4} | Vector: ${:.4} | Code: ${:.4} | Tools: ${:.4}\n\
             Tokens: {} input, {} output ({} total)",
            self.total_usd,
            self.category_costs.llm_total,
            self.category_costs.vector_total,
            self.category_costs.code_total,
            self.category_costs.tool_total,
            self.token_estimates.total_input_tokens,
            self.token_estimates.total_output_tokens,
            self.token_estimates.total_tokens
        )
    }

    /// Get the most expensive nodes
    pub fn top_expensive_nodes(&self, limit: usize) -> Vec<&NodeCost> {
        let mut costs: Vec<&NodeCost> = self.node_costs.values().collect();
        costs.sort_by(|a, b| {
            b.cost_usd
                .partial_cmp(&a.cost_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        costs.into_iter().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkflowBuilder;

    #[test]
    fn test_model_pricing_openai() {
        let pricing = ModelPricing::for_model("openai", "gpt-4");
        assert_eq!(pricing.input_cost_per_million, 30.0);
        assert_eq!(pricing.output_cost_per_million, 60.0);
    }

    #[test]
    fn test_model_pricing_anthropic() {
        let pricing = ModelPricing::for_model("anthropic", "claude-3-opus");
        assert_eq!(pricing.input_cost_per_million, 15.0);
        assert_eq!(pricing.output_cost_per_million, 75.0);
    }

    #[test]
    fn test_model_pricing_local() {
        let pricing = ModelPricing::for_model("ollama", "llama2");
        assert_eq!(pricing.input_cost_per_million, 0.0);
        assert_eq!(pricing.output_cost_per_million, 0.0);
    }

    #[test]
    fn test_calculate_cost() {
        let pricing = ModelPricing::for_model("openai", "gpt-3.5-turbo");
        let cost = pricing.calculate_cost(1000, 500);

        // Expected: (1000/1M * 0.5) + (500/1M * 1.5) = 0.0005 + 0.00075 = 0.00125
        assert!((cost - 0.00125).abs() < 0.0001);
    }

    #[test]
    fn test_estimate_token_count() {
        let text = "Hello, world!"; // 13 characters
        let tokens = CostEstimator::estimate_token_count(text);
        assert_eq!(tokens, 4); // 13 / 4 = 3.25, ceil = 4
    }

    #[test]
    fn test_estimate_simple_workflow() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "Generate",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-3.5-turbo".to_string(),
                    system_prompt: Some("You are a helpful assistant".to_string()),
                    prompt_template: "Say hello".to_string(),
                    temperature: Some(0.7),
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let estimate = CostEstimator::estimate(&workflow);

        assert!(estimate.total_usd > 0.0);
        assert!(estimate.category_costs.llm_total > 0.0);
        assert_eq!(estimate.category_costs.vector_total, 0.0);
        assert!(estimate.token_estimates.total_tokens > 0);
    }

    #[test]
    fn test_estimate_with_vector() {
        let workflow = WorkflowBuilder::new("RAG")
            .start("Start")
            .retriever(
                "Search",
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

        let estimate = CostEstimator::estimate(&workflow);

        assert!(estimate.category_costs.vector_total > 0.0);
        assert_eq!(estimate.category_costs.llm_total, 0.0);
    }

    #[test]
    fn test_cost_estimate_summary() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "LLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test".to_string(),
                    temperature: None,
                    max_tokens: Some(500),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let estimate = CostEstimator::estimate(&workflow);
        let summary = estimate.format_summary();

        assert!(summary.contains("Total Cost:"));
        assert!(summary.contains("Tokens:"));
    }

    #[test]
    fn test_top_expensive_nodes() {
        let workflow = WorkflowBuilder::new("Multi-LLM")
            .start("Start")
            .llm(
                "GPT4",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "expensive call".to_string(),
                    temperature: None,
                    max_tokens: Some(2000),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .llm(
                "GPT3.5",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-3.5-turbo".to_string(),
                    system_prompt: None,
                    prompt_template: "cheap call".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let estimate = CostEstimator::estimate(&workflow);
        let top = estimate.top_expensive_nodes(1);

        assert_eq!(top.len(), 1);
        assert_eq!(top[0].node_name, "GPT4");
    }

    #[test]
    fn test_estimate_with_retry() {
        let llm_config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: None,
            max_tokens: Some(100),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        };

        let node_with_retry = Node::new("LLM".to_string(), NodeKind::LLM(llm_config)).with_retry(
            crate::RetryConfig {
                max_retries: 3,
                initial_delay_ms: 1000,
                backoff_multiplier: 2.0,
                max_delay_ms: 30000,
            },
        );

        let cost = CostEstimator::estimate_node_cost(&node_with_retry);

        // Should have higher expected executions due to retries
        assert!(cost.expected_executions > 1);
    }
}
