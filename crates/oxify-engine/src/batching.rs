//! Intelligent node execution batching
//!
//! Groups similar operations for efficient batch execution:
//! - Multiple LLM calls to the same provider
//! - Multiple vector database searches
//! - Parallel API calls
//! - Reduces overhead and improves throughput

use oxify_model::{Node, NodeId, NodeKind};
use std::collections::HashMap;

/// Batch group identifier
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum BatchGroup {
    /// LLM calls to the same provider
    LlmProvider(String),

    /// Vector searches on the same database
    VectorDatabase(String),

    /// Tool/MCP calls to the same server
    ToolServer(String),

    /// Generic parallel execution group
    Parallel,

    /// No batching (execute individually)
    None,
}

/// Batch execution plan
#[derive(Debug, Clone)]
pub struct BatchPlan {
    /// Batched node groups
    pub batches: Vec<Batch>,

    /// Nodes that cannot be batched
    pub individual_nodes: Vec<NodeId>,
}

/// A batch of nodes that can be executed together
#[derive(Debug, Clone)]
pub struct Batch {
    /// Batch group identifier
    pub group: BatchGroup,

    /// Nodes in this batch
    pub nodes: Vec<NodeId>,

    /// Estimated speedup factor (1.0 = no speedup)
    pub speedup_factor: f32,
}

impl Batch {
    /// Create a new batch
    pub fn new(group: BatchGroup, nodes: Vec<NodeId>) -> Self {
        let speedup_factor = calculate_speedup_factor(&group, nodes.len());

        Self {
            group,
            nodes,
            speedup_factor,
        }
    }

    /// Get the batch size
    pub fn size(&self) -> usize {
        self.nodes.len()
    }
}

/// Calculate speedup factor for a batch
fn calculate_speedup_factor(group: &BatchGroup, batch_size: usize) -> f32 {
    if batch_size <= 1 {
        return 1.0;
    }

    match group {
        BatchGroup::LlmProvider(_) => {
            // LLM batching can save on connection overhead
            // Speedup increases with batch size but plateaus
            1.0 + (batch_size as f32 * 0.15).min(0.5)
        }
        BatchGroup::VectorDatabase(_) => {
            // Vector DB batching is very efficient
            1.0 + (batch_size as f32 * 0.25).min(0.75)
        }
        BatchGroup::ToolServer(_) => {
            // Tool batching has moderate benefit
            1.0 + (batch_size as f32 * 0.2).min(0.6)
        }
        BatchGroup::Parallel => {
            // Parallel execution scales linearly (up to a point)
            (batch_size as f32).min(8.0)
        }
        BatchGroup::None => 1.0,
    }
}

/// Node batch analyzer
pub struct BatchAnalyzer {
    /// Minimum batch size to consider batching
    pub min_batch_size: usize,

    /// Maximum batch size to prevent overloading
    pub max_batch_size: usize,
}

impl Default for BatchAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchAnalyzer {
    /// Create a new batch analyzer with default settings
    pub fn new() -> Self {
        Self {
            min_batch_size: 2,
            max_batch_size: 10,
        }
    }

    /// Create analyzer with custom settings
    pub fn with_limits(min_batch_size: usize, max_batch_size: usize) -> Self {
        Self {
            min_batch_size,
            max_batch_size,
        }
    }

    /// Analyze nodes and create a batch execution plan
    pub fn analyze(&self, nodes: &[&Node]) -> BatchPlan {
        // Group nodes by batch type
        let mut groups: HashMap<BatchGroup, Vec<NodeId>> = HashMap::new();

        for node in nodes {
            let group = self.classify_node(node);
            groups.entry(group).or_default().push(node.id);
        }

        // Create batches from groups
        let mut batches = Vec::new();
        let mut individual_nodes = Vec::new();

        for (group, node_ids) in groups {
            if matches!(group, BatchGroup::None) {
                individual_nodes.extend(node_ids);
            } else if node_ids.len() >= self.min_batch_size {
                // Split into batches if exceeds max size
                for chunk in node_ids.chunks(self.max_batch_size) {
                    batches.push(Batch::new(group.clone(), chunk.to_vec()));
                }
            } else {
                // Too small to batch efficiently
                individual_nodes.extend(node_ids);
            }
        }

        BatchPlan {
            batches,
            individual_nodes,
        }
    }

    /// Classify a node into a batch group
    fn classify_node(&self, node: &Node) -> BatchGroup {
        match &node.kind {
            NodeKind::LLM(config) => {
                // Group by provider for connection pooling
                BatchGroup::LlmProvider(config.provider.clone())
            }

            NodeKind::Retriever(config) => {
                // Group by database type
                BatchGroup::VectorDatabase(config.db_type.clone())
            }

            NodeKind::Tool(config) => {
                // Group by MCP server
                BatchGroup::ToolServer(config.server_id.clone())
            }

            NodeKind::Code(_) | NodeKind::IfElse(_) | NodeKind::Switch(_) => {
                // Control flow nodes should execute in parallel
                BatchGroup::Parallel
            }

            // Nodes that shouldn't be batched
            NodeKind::Start
            | NodeKind::End
            | NodeKind::Loop(_)
            | NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Parallel(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_) => BatchGroup::None,
        }
    }

    /// Calculate estimated time savings from batching
    pub fn estimate_time_savings(&self, plan: &BatchPlan) -> f32 {
        let mut total_speedup = 0.0;

        for batch in &plan.batches {
            // Time saved = (normal_time - batched_time) / normal_time
            // = 1 - (1 / speedup_factor)
            let time_saved_ratio = 1.0 - (1.0 / batch.speedup_factor);
            total_speedup += time_saved_ratio * batch.size() as f32;
        }

        // Normalize by total number of nodes
        let total_nodes =
            plan.batches.iter().map(|b| b.size()).sum::<usize>() + plan.individual_nodes.len();

        if total_nodes > 0 {
            total_speedup / total_nodes as f32
        } else {
            0.0
        }
    }
}

/// Batch execution statistics
#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    /// Total nodes executed
    pub total_nodes: usize,

    /// Nodes executed in batches
    pub batched_nodes: usize,

    /// Number of batches created
    pub batch_count: usize,

    /// Estimated time saved (0.0-1.0)
    pub estimated_time_savings: f32,

    /// Average batch size
    pub average_batch_size: f32,
}

impl BatchStats {
    /// Create statistics from a batch plan
    pub fn from_plan(plan: &BatchPlan, analyzer: &BatchAnalyzer) -> Self {
        let batched_nodes: usize = plan.batches.iter().map(|b| b.size()).sum();
        let batch_count = plan.batches.len();
        let total_nodes = batched_nodes + plan.individual_nodes.len();

        let average_batch_size = if batch_count > 0 {
            batched_nodes as f32 / batch_count as f32
        } else {
            0.0
        };

        let estimated_time_savings = analyzer.estimate_time_savings(plan);

        Self {
            total_nodes,
            batched_nodes,
            batch_count,
            estimated_time_savings,
            average_batch_size,
        }
    }

    /// Get batching efficiency (0.0-1.0)
    pub fn efficiency(&self) -> f32 {
        if self.total_nodes > 0 {
            self.batched_nodes as f32 / self.total_nodes as f32
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{LlmConfig, VectorConfig};

    #[test]
    fn test_batch_classification() {
        let analyzer = BatchAnalyzer::new();

        let llm_node = Node::new(
            "LLM".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test".to_string(),
                temperature: Some(0.7),
                max_tokens: Some(1000),
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );

        let group = analyzer.classify_node(&llm_node);
        assert_eq!(group, BatchGroup::LlmProvider("openai".to_string()));
    }

    #[test]
    fn test_batch_plan_creation() {
        let analyzer = BatchAnalyzer::new();

        let nodes = [
            Node::new(
                "LLM 1".to_string(),
                NodeKind::LLM(LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test1".to_string(),
                    temperature: Some(0.7),
                    max_tokens: Some(1000),
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                }),
            ),
            Node::new(
                "LLM 2".to_string(),
                NodeKind::LLM(LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test2".to_string(),
                    temperature: Some(0.7),
                    max_tokens: Some(1000),
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                }),
            ),
            Node::new(
                "Vector".to_string(),
                NodeKind::Retriever(VectorConfig {
                    db_type: "qdrant".to_string(),
                    collection: "docs".to_string(),
                    query: "test".to_string(),
                    top_k: 5,
                    score_threshold: Some(0.7),
                }),
            ),
        ];

        let node_refs: Vec<&Node> = nodes.iter().collect();
        let plan = analyzer.analyze(&node_refs);

        // Should have 1 batch (2 OpenAI LLM nodes) and 1 individual (vector node)
        assert_eq!(plan.batches.len(), 1);
        assert_eq!(plan.individual_nodes.len(), 1);
        assert_eq!(plan.batches[0].size(), 2);
    }

    #[test]
    fn test_speedup_calculation() {
        let llm_group = BatchGroup::LlmProvider("openai".to_string());
        let speedup_2 = calculate_speedup_factor(&llm_group, 2);
        let speedup_10 = calculate_speedup_factor(&llm_group, 10);

        assert!(speedup_2 > 1.0);
        assert!(speedup_10 > speedup_2);
        assert!(speedup_10 <= 1.5); // Plateaus at 0.5 extra
    }

    #[test]
    fn test_batch_stats() {
        let analyzer = BatchAnalyzer::new();

        let nodes = [
            Node::new(
                "LLM 1".to_string(),
                NodeKind::LLM(LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test1".to_string(),
                    temperature: Some(0.7),
                    max_tokens: Some(1000),
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                }),
            ),
            Node::new(
                "LLM 2".to_string(),
                NodeKind::LLM(LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test2".to_string(),
                    temperature: Some(0.7),
                    max_tokens: Some(1000),
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                }),
            ),
        ];

        let node_refs: Vec<&Node> = nodes.iter().collect();
        let plan = analyzer.analyze(&node_refs);
        let stats = BatchStats::from_plan(&plan, &analyzer);

        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.batched_nodes, 2);
        assert_eq!(stats.batch_count, 1);
        assert!(stats.efficiency() > 0.9); // Most nodes batched
        assert!(stats.estimated_time_savings > 0.0);
    }

    #[test]
    fn test_max_batch_size_splitting() {
        let analyzer = BatchAnalyzer::with_limits(2, 3);

        // Create 5 similar LLM nodes
        let mut nodes = Vec::new();
        for i in 0..5 {
            nodes.push(Node::new(
                format!("LLM {}", i),
                NodeKind::LLM(LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: format!("test{}", i),
                    temperature: Some(0.7),
                    max_tokens: Some(1000),
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                }),
            ));
        }

        let node_refs: Vec<&Node> = nodes.iter().collect();
        let plan = analyzer.analyze(&node_refs);

        // Should split into 2 batches (3 + 2)
        assert_eq!(plan.batches.len(), 2);
        assert_eq!(plan.batches[0].size(), 3);
        assert_eq!(plan.batches[1].size(), 2);
    }
}
