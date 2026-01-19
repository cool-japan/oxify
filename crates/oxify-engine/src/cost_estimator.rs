//! Execution cost estimation and tracking
//!
//! Estimates and tracks execution costs for workflows, including:
//! - LLM token usage and costs
//! - Vector database operations
//! - API calls
//! - Total execution cost

use oxify_model::{Node, NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Cost breakdown for a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCost {
    /// Node ID
    pub node_id: uuid::Uuid,

    /// Node name
    pub node_name: String,

    /// Estimated input tokens (for LLMs)
    pub estimated_input_tokens: u32,

    /// Estimated output tokens (for LLMs)
    pub estimated_output_tokens: u32,

    /// Cost in USD
    pub cost_usd: f64,

    /// Cost breakdown by operation type
    pub operations: Vec<CostOperation>,
}

/// Individual cost operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostOperation {
    /// Operation type (e.g., "llm_call", "vector_search", "api_call")
    pub operation_type: String,

    /// Description
    pub description: String,

    /// Cost in USD
    pub cost_usd: f64,

    /// Quantity (tokens, requests, etc.)
    pub quantity: u32,
}

/// Total execution cost estimate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCostEstimate {
    /// Total estimated cost in USD
    pub total_cost_usd: f64,

    /// Total input tokens (all LLM nodes)
    pub total_input_tokens: u32,

    /// Total output tokens (all LLM nodes)
    pub total_output_tokens: u32,

    /// Cost breakdown by node
    pub node_costs: Vec<NodeCost>,

    /// Cost breakdown by category
    pub category_costs: HashMap<String, f64>,
}

impl ExecutionCostEstimate {
    /// Create a new empty cost estimate
    pub fn new() -> Self {
        Self {
            total_cost_usd: 0.0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            node_costs: Vec::new(),
            category_costs: HashMap::new(),
        }
    }

    /// Add a node cost to the estimate
    pub fn add_node_cost(&mut self, node_cost: NodeCost) {
        self.total_cost_usd += node_cost.cost_usd;
        self.total_input_tokens += node_cost.estimated_input_tokens;
        self.total_output_tokens += node_cost.estimated_output_tokens;

        // Update category costs
        for op in &node_cost.operations {
            *self
                .category_costs
                .entry(op.operation_type.clone())
                .or_insert(0.0) += op.cost_usd;
        }

        self.node_costs.push(node_cost);
    }
}

impl Default for ExecutionCostEstimate {
    fn default() -> Self {
        Self::new()
    }
}

/// LLM pricing per model (per 1M tokens)
#[derive(Debug, Clone)]
pub struct LlmPricing {
    /// Input token cost per 1M tokens (USD)
    pub input_cost_per_million: f64,

    /// Output token cost per 1M tokens (USD)
    pub output_cost_per_million: f64,
}

impl LlmPricing {
    /// Get pricing for a specific model
    pub fn for_model(model: &str) -> Self {
        match model {
            // OpenAI GPT-4 Turbo
            "gpt-4-turbo" | "gpt-4-turbo-preview" => Self {
                input_cost_per_million: 10.0,
                output_cost_per_million: 30.0,
            },

            // OpenAI GPT-4
            "gpt-4" | "gpt-4-0613" => Self {
                input_cost_per_million: 30.0,
                output_cost_per_million: 60.0,
            },

            // OpenAI GPT-3.5 Turbo
            "gpt-3.5-turbo" | "gpt-3.5-turbo-0125" => Self {
                input_cost_per_million: 0.5,
                output_cost_per_million: 1.5,
            },

            // Anthropic Claude Sonnet
            "claude-3-5-sonnet-20241022" | "claude-3-5-sonnet-latest" => Self {
                input_cost_per_million: 3.0,
                output_cost_per_million: 15.0,
            },

            // Anthropic Claude Opus
            "claude-3-opus-20240229" | "claude-3-opus-latest" => Self {
                input_cost_per_million: 15.0,
                output_cost_per_million: 75.0,
            },

            // Anthropic Claude Haiku
            "claude-3-haiku-20240307" | "claude-3-haiku-latest" => Self {
                input_cost_per_million: 0.25,
                output_cost_per_million: 1.25,
            },

            // Ollama and local models are free
            _ if model.starts_with("llama") || model.starts_with("mistral") => Self {
                input_cost_per_million: 0.0,
                output_cost_per_million: 0.0,
            },

            // Default pricing (assume GPT-3.5 Turbo equivalent)
            _ => Self {
                input_cost_per_million: 0.5,
                output_cost_per_million: 1.5,
            },
        }
    }

    /// Calculate cost for given token counts
    pub fn calculate_cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_cost_per_million;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_cost_per_million;
        input_cost + output_cost
    }
}

/// Cost estimator for workflows
pub struct CostEstimator {
    /// Average tokens per prompt (used for estimation)
    pub avg_prompt_tokens: u32,

    /// Average tokens per response (used for estimation)
    pub avg_response_tokens: u32,
}

impl Default for CostEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl CostEstimator {
    /// Create a new cost estimator with default values
    pub fn new() -> Self {
        Self {
            avg_prompt_tokens: 500,    // Default average prompt
            avg_response_tokens: 1000, // Default average response
        }
    }

    /// Create estimator with custom token averages
    pub fn with_averages(avg_prompt_tokens: u32, avg_response_tokens: u32) -> Self {
        Self {
            avg_prompt_tokens,
            avg_response_tokens,
        }
    }

    /// Estimate cost for a workflow
    pub fn estimate_workflow(&self, workflow: &Workflow) -> ExecutionCostEstimate {
        let mut estimate = ExecutionCostEstimate::new();

        for node in &workflow.nodes {
            let node_cost = self.estimate_node(node);
            estimate.add_node_cost(node_cost);
        }

        estimate
    }

    /// Estimate cost for a single node
    fn estimate_node(&self, node: &Node) -> NodeCost {
        match &node.kind {
            NodeKind::LLM(config) => self.estimate_llm_node(node, config),
            NodeKind::Retriever(config) => self.estimate_retriever_node(node, config),
            NodeKind::Tool(_) => self.estimate_tool_node(node),
            NodeKind::SubWorkflow(_) => self.estimate_subworkflow_node(node),
            NodeKind::Code(_) => self.estimate_code_node(node),
            _ => NodeCost {
                node_id: node.id,
                node_name: node.name.clone(),
                estimated_input_tokens: 0,
                estimated_output_tokens: 0,
                cost_usd: 0.0,
                operations: Vec::new(),
            },
        }
    }

    /// Estimate cost for LLM node
    fn estimate_llm_node(&self, node: &Node, config: &oxify_model::LlmConfig) -> NodeCost {
        let pricing = LlmPricing::for_model(&config.model);

        // Estimate tokens based on prompt template and max_tokens
        let input_tokens = self.estimate_prompt_tokens(&config.prompt_template);
        let output_tokens = config.max_tokens.unwrap_or(self.avg_response_tokens);

        let cost = pricing.calculate_cost(input_tokens, output_tokens);

        NodeCost {
            node_id: node.id,
            node_name: node.name.clone(),
            estimated_input_tokens: input_tokens,
            estimated_output_tokens: output_tokens,
            cost_usd: cost,
            operations: vec![CostOperation {
                operation_type: "llm_call".to_string(),
                description: format!("{} ({} model)", node.name, config.model),
                cost_usd: cost,
                quantity: input_tokens + output_tokens,
            }],
        }
    }

    /// Estimate cost for retriever node
    fn estimate_retriever_node(&self, node: &Node, config: &oxify_model::VectorConfig) -> NodeCost {
        // Vector search costs are typically minimal
        let cost = match config.db_type.as_str() {
            "qdrant" => 0.0001, // Very cheap
            "pgvector" => 0.0001,
            _ => 0.0,
        };

        NodeCost {
            node_id: node.id,
            node_name: node.name.clone(),
            estimated_input_tokens: 0,
            estimated_output_tokens: 0,
            cost_usd: cost,
            operations: vec![CostOperation {
                operation_type: "vector_search".to_string(),
                description: format!("{} search (top_k={})", config.db_type, config.top_k),
                cost_usd: cost,
                quantity: config.top_k as u32,
            }],
        }
    }

    /// Estimate cost for tool/MCP node
    fn estimate_tool_node(&self, node: &Node) -> NodeCost {
        // Tool calls are typically cheap (just API overhead)
        let cost = 0.0001;

        NodeCost {
            node_id: node.id,
            node_name: node.name.clone(),
            estimated_input_tokens: 0,
            estimated_output_tokens: 0,
            cost_usd: cost,
            operations: vec![CostOperation {
                operation_type: "tool_call".to_string(),
                description: node.name.clone(),
                cost_usd: cost,
                quantity: 1,
            }],
        }
    }

    /// Estimate cost for code execution node
    fn estimate_code_node(&self, node: &Node) -> NodeCost {
        // Code execution is very cheap (just CPU time)
        let cost = 0.00001;

        NodeCost {
            node_id: node.id,
            node_name: node.name.clone(),
            estimated_input_tokens: 0,
            estimated_output_tokens: 0,
            cost_usd: cost,
            operations: vec![CostOperation {
                operation_type: "code_execution".to_string(),
                description: node.name.clone(),
                cost_usd: cost,
                quantity: 1,
            }],
        }
    }

    /// Estimate cost for sub-workflow node
    fn estimate_subworkflow_node(&self, node: &Node) -> NodeCost {
        // Sub-workflow cost is hard to estimate without loading the workflow
        // Use a placeholder cost
        NodeCost {
            node_id: node.id,
            node_name: node.name.clone(),
            estimated_input_tokens: 0,
            estimated_output_tokens: 0,
            cost_usd: 0.001, // Placeholder
            operations: vec![CostOperation {
                operation_type: "subworkflow".to_string(),
                description: node.name.clone(),
                cost_usd: 0.001,
                quantity: 1,
            }],
        }
    }

    /// Estimate tokens for a prompt template
    fn estimate_prompt_tokens(&self, template: &str) -> u32 {
        // Simple estimation: 1 token ≈ 4 characters
        // This is a rough approximation
        let char_count = template.len() as u32;
        let estimated = char_count / 4;

        // Add some overhead for system prompt and formatting
        estimated + 100
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{LlmConfig, VectorConfig, WorkflowMetadata};

    #[test]
    fn test_llm_pricing() {
        let gpt4_pricing = LlmPricing::for_model("gpt-4");
        assert_eq!(gpt4_pricing.input_cost_per_million, 30.0);
        assert_eq!(gpt4_pricing.output_cost_per_million, 60.0);

        let cost = gpt4_pricing.calculate_cost(1000, 500);
        // 1000 input tokens = 0.001M * $30 = $0.03
        // 500 output tokens = 0.0005M * $60 = $0.03
        // Total = $0.06
        assert!((cost - 0.06).abs() < 0.0001);
    }

    #[test]
    fn test_ollama_free() {
        let ollama_pricing = LlmPricing::for_model("llama3.1");
        assert_eq!(ollama_pricing.input_cost_per_million, 0.0);
        assert_eq!(ollama_pricing.output_cost_per_million, 0.0);

        let cost = ollama_pricing.calculate_cost(10000, 5000);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_workflow_cost_estimation() {
        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test Workflow".to_string()),
            nodes: vec![
                Node::new("Start".to_string(), NodeKind::Start),
                Node::new(
                    "LLM Call".to_string(),
                    NodeKind::LLM(LlmConfig {
                        provider: "openai".to_string(),
                        model: "gpt-3.5-turbo".to_string(),
                        system_prompt: None,
                        prompt_template: "Hello, world!".to_string(),
                        temperature: Some(0.7),
                        max_tokens: Some(1000),
                        tools: Vec::new(),
                        images: Vec::new(),
                        extra_params: serde_json::Value::Null,
                    }),
                ),
                Node::new(
                    "Vector Search".to_string(),
                    NodeKind::Retriever(VectorConfig {
                        db_type: "qdrant".to_string(),
                        collection: "docs".to_string(),
                        query: "test".to_string(),
                        top_k: 5,
                        score_threshold: Some(0.7),
                    }),
                ),
                Node::new("End".to_string(), NodeKind::End),
            ],
            edges: vec![],
        };

        let estimator = CostEstimator::new();
        let estimate = estimator.estimate_workflow(&workflow);

        // Should have costs for LLM and vector search
        assert!(estimate.total_cost_usd > 0.0);
        assert!(estimate.total_input_tokens > 0);
        assert!(estimate.total_output_tokens > 0);
        assert_eq!(estimate.node_costs.len(), 4); // All nodes

        // LLM should be the most expensive
        let llm_cost = estimate
            .node_costs
            .iter()
            .find(|c| c.node_name == "LLM Call")
            .unwrap();
        assert!(llm_cost.cost_usd > 0.0);
    }

    #[test]
    fn test_cost_estimate_accumulation() {
        let mut estimate = ExecutionCostEstimate::new();

        let node_cost1 = NodeCost {
            node_id: uuid::Uuid::new_v4(),
            node_name: "Node 1".to_string(),
            estimated_input_tokens: 100,
            estimated_output_tokens: 200,
            cost_usd: 0.01,
            operations: vec![CostOperation {
                operation_type: "llm_call".to_string(),
                description: "Test".to_string(),
                cost_usd: 0.01,
                quantity: 300,
            }],
        };

        let node_cost2 = NodeCost {
            node_id: uuid::Uuid::new_v4(),
            node_name: "Node 2".to_string(),
            estimated_input_tokens: 150,
            estimated_output_tokens: 250,
            cost_usd: 0.02,
            operations: vec![CostOperation {
                operation_type: "llm_call".to_string(),
                description: "Test 2".to_string(),
                cost_usd: 0.02,
                quantity: 400,
            }],
        };

        estimate.add_node_cost(node_cost1);
        estimate.add_node_cost(node_cost2);

        assert_eq!(estimate.total_input_tokens, 250);
        assert_eq!(estimate.total_output_tokens, 450);
        assert!((estimate.total_cost_usd - 0.03).abs() < 0.0001);
        assert_eq!(estimate.node_costs.len(), 2);
        assert_eq!(estimate.category_costs.get("llm_call"), Some(&0.03));
    }
}
