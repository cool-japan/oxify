//! Execution time prediction for workflows
//!
//! This module provides predictions for workflow execution times based on
//! historical data, node complexity, and expected behavior.

use crate::{ExecutionStats, LlmConfig, Node, NodeKind, VectorConfig, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Execution time prediction for a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeEstimate {
    /// Estimated minimum execution time
    pub min_duration_ms: u64,

    /// Estimated average execution time
    pub avg_duration_ms: u64,

    /// Estimated maximum execution time (including retries, worst case)
    pub max_duration_ms: u64,

    /// Critical path through the workflow
    pub critical_path: Vec<String>,

    /// Time breakdown by node
    pub node_times: HashMap<String, NodeTime>,

    /// Confidence level (0.0 to 1.0)
    pub confidence: f64,
}

/// Time estimate for a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTime {
    /// Node name
    pub node_name: String,

    /// Node type
    pub node_type: String,

    /// Minimum execution time (ms)
    pub min_ms: u64,

    /// Average execution time (ms)
    pub avg_ms: u64,

    /// Maximum execution time (ms)
    pub max_ms: u64,

    /// Number of expected executions
    pub expected_executions: u32,

    /// Whether this node is on the critical path
    pub is_critical: bool,
}

/// Historical execution data for improving predictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalData {
    /// Node type -> average execution time mapping
    pub node_type_averages: HashMap<String, u64>,

    /// Provider -> average API latency mapping
    pub provider_latencies: HashMap<String, u64>,

    /// Specific node ID -> execution times
    pub node_execution_history: HashMap<String, Vec<u64>>,
}

impl HistoricalData {
    /// Create empty historical data
    pub fn new() -> Self {
        Self {
            node_type_averages: HashMap::new(),
            provider_latencies: HashMap::new(),
            node_execution_history: HashMap::new(),
        }
    }

    /// Update historical data from execution stats
    pub fn update_from_stats(&mut self, _stats: &ExecutionStats) {
        // Update provider latencies if available
        // This is a simplified version - real implementation would parse from stats
    }

    /// Get average time for a node type
    pub fn get_node_type_average(&self, node_type: &str) -> Option<u64> {
        self.node_type_averages.get(node_type).copied()
    }

    /// Get average latency for a provider
    pub fn get_provider_latency(&self, provider: &str) -> Option<u64> {
        self.provider_latencies.get(provider).copied()
    }
}

impl Default for HistoricalData {
    fn default() -> Self {
        Self::new()
    }
}

/// Time prediction engine
pub struct TimePredictor {
    /// Historical execution data
    historical_data: HistoricalData,
}

impl TimePredictor {
    /// Create a new time predictor
    pub fn new() -> Self {
        Self {
            historical_data: HistoricalData::new(),
        }
    }

    /// Create predictor with historical data
    pub fn with_historical_data(historical_data: HistoricalData) -> Self {
        Self { historical_data }
    }

    /// Predict execution time for a workflow
    pub fn predict(&self, workflow: &Workflow) -> TimeEstimate {
        let mut node_times = HashMap::new();
        let mut total_min = 0u64;
        let mut total_avg = 0u64;
        let mut total_max = 0u64;

        // Predict time for each node
        for node in &workflow.nodes {
            let node_time = self.predict_node_time(node);
            total_min += node_time.min_ms;
            total_avg += node_time.avg_ms;
            total_max += node_time.max_ms;
            node_times.insert(node.id.to_string(), node_time);
        }

        // Find critical path (simplified - assumes linear for now)
        let critical_path = workflow
            .nodes
            .iter()
            .map(|n| n.name.clone())
            .collect::<Vec<_>>();

        // Calculate confidence based on available historical data
        let confidence = self.calculate_confidence(workflow);

        TimeEstimate {
            min_duration_ms: total_min,
            avg_duration_ms: total_avg,
            max_duration_ms: total_max,
            critical_path,
            node_times,
            confidence,
        }
    }

    /// Predict time for a single node
    fn predict_node_time(&self, node: &Node) -> NodeTime {
        let (min_ms, avg_ms, max_ms) = match &node.kind {
            NodeKind::Start | NodeKind::End => (1, 5, 10),

            NodeKind::LLM(config) => self.predict_llm_time(config, node),

            NodeKind::Retriever(config) => self.predict_vector_time(config),

            NodeKind::Code(_) => {
                // Code execution time varies widely
                (100, 500, 5000)
            }

            NodeKind::Tool(_) => {
                // API call time
                (200, 1000, 5000)
            }

            NodeKind::IfElse(_) => {
                // Condition evaluation is fast
                (1, 10, 50)
            }

            NodeKind::Switch(_) => {
                // Switch evaluation
                (1, 10, 50)
            }

            NodeKind::Loop(_) => {
                // Loop overhead (not including body execution)
                (10, 50, 200)
            }

            NodeKind::TryCatch(_) => {
                // Try-catch overhead
                (5, 20, 100)
            }

            NodeKind::SubWorkflow(_) => {
                // Sub-workflow execution (depends on sub-workflow)
                (100, 5000, 30000)
            }

            NodeKind::Parallel(_) => {
                // Parallel execution overhead
                (50, 200, 1000)
            }

            NodeKind::Approval(_) => {
                // Human approval can take very long
                (1000, 60000, 3600000) // 1s to 1 hour
            }

            NodeKind::Form(_) => {
                // Form submission time
                (5000, 120000, 600000) // 5s to 10 minutes
            }

            NodeKind::Vision(_) => {
                // Vision/OCR processing time
                (500, 3000, 15000) // 0.5s to 15s depending on image size
            }
        };

        let expected_executions = Self::estimate_executions(node);

        NodeTime {
            node_name: node.name.clone(),
            node_type: self.get_node_type_string(&node.kind),
            min_ms: min_ms * expected_executions as u64,
            avg_ms: avg_ms * expected_executions as u64,
            max_ms: max_ms * expected_executions as u64,
            expected_executions,
            is_critical: false, // Would be set by critical path analysis
        }
    }

    /// Predict LLM execution time
    fn predict_llm_time(&self, config: &LlmConfig, _node: &Node) -> (u64, u64, u64) {
        // Check historical data first
        if let Some(avg) = self.historical_data.get_provider_latency(&config.provider) {
            return (avg / 2, avg, avg * 2);
        }

        // Estimate based on provider and model
        let base_latency = match config.provider.to_lowercase().as_str() {
            "openai" => {
                if config.model.contains("gpt-4") {
                    (3000, 8000, 20000) // GPT-4 is slower
                } else {
                    (1000, 3000, 10000) // GPT-3.5 is faster
                }
            }
            "anthropic" => {
                if config.model.contains("opus") {
                    (2000, 6000, 15000)
                } else if config.model.contains("sonnet") {
                    (1000, 4000, 12000)
                } else {
                    (500, 2000, 8000) // Haiku is fastest
                }
            }
            "ollama" | "local" => {
                // Local models depend on hardware
                (500, 2000, 10000)
            }
            _ => (2000, 5000, 15000), // Default estimate
        };

        // Adjust for token count
        let max_tokens = config.max_tokens.unwrap_or(1000);
        let token_multiplier = (max_tokens as f64 / 1000.0).max(0.5);

        (
            (base_latency.0 as f64 * token_multiplier) as u64,
            (base_latency.1 as f64 * token_multiplier) as u64,
            (base_latency.2 as f64 * token_multiplier) as u64,
        )
    }

    /// Predict vector search time
    fn predict_vector_time(&self, config: &VectorConfig) -> (u64, u64, u64) {
        match config.db_type.to_lowercase().as_str() {
            "qdrant" => {
                // Qdrant is very fast
                let base = 50 + (config.top_k * 5) as u64;
                (base / 2, base, base * 3)
            }
            "pgvector" => {
                // pgvector can be slower depending on index
                let base = 100 + (config.top_k * 10) as u64;
                (base / 2, base, base * 5)
            }
            _ => {
                let base = 100 + (config.top_k * 10) as u64;
                (base / 2, base, base * 3)
            }
        }
    }

    /// Estimate number of executions (considering retries)
    fn estimate_executions(node: &Node) -> u32 {
        let mut executions = 1u32;

        if let Some(retry_config) = &node.retry_config {
            // Assume 30% failure rate requiring retries
            let avg_retries = (retry_config.max_retries as f32 * 0.3).ceil() as u32;
            executions += avg_retries;
        }

        executions
    }

    /// Calculate confidence level based on available data
    fn calculate_confidence(&self, workflow: &Workflow) -> f64 {
        if workflow.nodes.is_empty() {
            return 0.0;
        }

        let mut total_confidence = 0.0;

        for node in &workflow.nodes {
            let node_confidence = match &node.kind {
                // High confidence for simple nodes
                NodeKind::Start | NodeKind::End | NodeKind::IfElse(_) | NodeKind::Switch(_) => 0.9,

                // Medium confidence for LLM/Vector (depends on historical data)
                NodeKind::LLM(_) | NodeKind::Retriever(_) => {
                    if self
                        .historical_data
                        .node_execution_history
                        .contains_key(&node.id.to_string())
                    {
                        0.8 // Higher confidence with historical data
                    } else {
                        0.5 // Lower confidence without data
                    }
                }

                // Lower confidence for variable-time operations
                NodeKind::Code(_) | NodeKind::Tool(_) | NodeKind::SubWorkflow(_) => 0.4,

                // Very low confidence for human-in-the-loop
                NodeKind::Approval(_) | NodeKind::Form(_) => 0.2,

                // Medium confidence for control flow
                NodeKind::Loop(_) | NodeKind::TryCatch(_) | NodeKind::Parallel(_) => 0.5,

                // Medium confidence for vision/OCR
                NodeKind::Vision(_) => 0.6,
            };

            total_confidence += node_confidence;
        }

        (total_confidence / workflow.nodes.len() as f64).min(1.0)
    }

    /// Get node type as string
    fn get_node_type_string(&self, kind: &NodeKind) -> String {
        match kind {
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
        }
    }
}

impl Default for TimePredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeEstimate {
    /// Format as human-readable string
    pub fn format_summary(&self) -> String {
        let min_duration = Duration::from_millis(self.min_duration_ms);
        let avg_duration = Duration::from_millis(self.avg_duration_ms);
        let max_duration = Duration::from_millis(self.max_duration_ms);

        format!(
            "Estimated Time: {:?} - {:?} (avg: {:?})\n\
             Critical Path: {}\n\
             Confidence: {:.0}%",
            min_duration,
            max_duration,
            avg_duration,
            self.critical_path.join(" → "),
            self.confidence * 100.0
        )
    }

    /// Get nodes on the critical path
    pub fn critical_path_nodes(&self) -> Vec<&NodeTime> {
        self.node_times
            .values()
            .filter(|nt| nt.is_critical)
            .collect()
    }

    /// Get slowest nodes
    pub fn slowest_nodes(&self, limit: usize) -> Vec<&NodeTime> {
        let mut times: Vec<&NodeTime> = self.node_times.values().collect();
        times.sort_by(|a, b| b.avg_ms.cmp(&a.avg_ms));
        times.into_iter().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkflowBuilder;

    #[test]
    fn test_time_predictor_new() {
        let predictor = TimePredictor::new();
        assert!(predictor.historical_data.node_type_averages.is_empty());
    }

    #[test]
    fn test_predict_simple_workflow() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "Generate",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-3.5-turbo".to_string(),
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

        let predictor = TimePredictor::new();
        let estimate = predictor.predict(&workflow);

        assert!(estimate.avg_duration_ms > 0);
        assert!(estimate.min_duration_ms < estimate.avg_duration_ms);
        assert!(estimate.avg_duration_ms < estimate.max_duration_ms);
        assert!(estimate.confidence > 0.0 && estimate.confidence <= 1.0);
    }

    #[test]
    fn test_predict_with_vector_search() {
        let workflow = WorkflowBuilder::new("RAG")
            .start("Start")
            .retriever(
                "Search",
                VectorConfig {
                    db_type: "qdrant".to_string(),
                    collection: "docs".to_string(),
                    query: "test".to_string(),
                    top_k: 5,
                    score_threshold: Some(0.7),
                },
            )
            .end("End")
            .build();

        let predictor = TimePredictor::new();
        let estimate = predictor.predict(&workflow);

        assert!(estimate.avg_duration_ms > 0);
        assert_eq!(estimate.node_times.len(), 3); // Start, Retriever, End
    }

    #[test]
    fn test_estimate_format_summary() {
        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .end("End")
            .build();

        let predictor = TimePredictor::new();
        let estimate = predictor.predict(&workflow);
        let summary = estimate.format_summary();

        assert!(summary.contains("Estimated Time:"));
        assert!(summary.contains("Critical Path:"));
        assert!(summary.contains("Confidence:"));
    }

    #[test]
    fn test_slowest_nodes() {
        let workflow = WorkflowBuilder::new("Multi-LLM")
            .start("Start")
            .llm(
                "GPT4",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test".to_string(),
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
                    prompt_template: "test".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let predictor = TimePredictor::new();
        let estimate = predictor.predict(&workflow);
        let slowest = estimate.slowest_nodes(1);

        assert_eq!(slowest.len(), 1);
        // GPT-4 with more tokens should be slowest
        assert_eq!(slowest[0].node_name, "GPT4");
    }

    #[test]
    fn test_node_with_retry_prediction() {
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

        let node = Node::new("LLM".to_string(), NodeKind::LLM(llm_config)).with_retry(
            crate::RetryConfig {
                max_retries: 3,
                initial_delay_ms: 1000,
                backoff_multiplier: 2.0,
                max_delay_ms: 30000,
            },
        );

        let predictor = TimePredictor::new();
        let time = predictor.predict_node_time(&node);

        // Should have higher expected executions due to retries
        assert!(time.expected_executions > 1);
    }

    #[test]
    fn test_historical_data() {
        let mut historical_data = HistoricalData::new();
        historical_data
            .node_type_averages
            .insert("LLM".to_string(), 5000);
        historical_data
            .provider_latencies
            .insert("openai".to_string(), 4000);

        assert_eq!(historical_data.get_node_type_average("LLM"), Some(5000));
        assert_eq!(historical_data.get_provider_latency("openai"), Some(4000));
        assert_eq!(historical_data.get_node_type_average("Code"), None);
    }

    #[test]
    fn test_predictor_with_historical_data() {
        let mut historical_data = HistoricalData::new();
        historical_data
            .provider_latencies
            .insert("openai".to_string(), 2000);

        let predictor = TimePredictor::with_historical_data(historical_data);

        let workflow = WorkflowBuilder::new("Test")
            .start("Start")
            .llm(
                "GPT",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test".to_string(),
                    temperature: None,
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let estimate = predictor.predict(&workflow);
        assert!(estimate.avg_duration_ms > 0);
    }
}
