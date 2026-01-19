//! Workflow builder for fluent workflow construction
//!
//! This module provides a builder pattern for constructing workflows
//! with a clean, fluent API.

use crate::{
    ApprovalConfig, Condition, Edge, FormConfig, LlmConfig, LoopConfig, McpConfig, Node, NodeId,
    NodeKind, ParallelConfig, RetryConfig, ScriptConfig, SubWorkflowConfig, SwitchConfig,
    TimeoutConfig, TryCatchConfig, VectorConfig, Workflow,
};

/// Builder for constructing workflows
pub struct WorkflowBuilder {
    workflow: Workflow,
    last_node_id: Option<NodeId>,
}

impl WorkflowBuilder {
    /// Create a new workflow builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            workflow: Workflow::new(name.into()),
            last_node_id: None,
        }
    }

    /// Set the workflow description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.workflow.metadata.description = Some(description.into());
        self
    }

    /// Set the workflow version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.workflow.metadata.version = version.into();
        self
    }

    /// Add a tag to the workflow
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.workflow.metadata.tags.push(tag.into());
        self
    }

    /// Add multiple tags to the workflow
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.workflow.metadata.tags.extend(tags);
        self
    }

    /// Add a start node
    pub fn start(mut self, name: impl Into<String>) -> Self {
        let node = Node::new(name.into(), NodeKind::Start);
        self.last_node_id = Some(node.id);
        self.workflow.add_node(node);
        self
    }

    /// Add an end node
    pub fn end(mut self, name: impl Into<String>) -> Self {
        let node = Node::new(name.into(), NodeKind::End);
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add an LLM node
    pub fn llm(mut self, name: impl Into<String>, config: LlmConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::LLM(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a code execution node
    pub fn code(mut self, name: impl Into<String>, config: ScriptConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::Code(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a retriever (vector search) node
    pub fn retriever(mut self, name: impl Into<String>, config: VectorConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::Retriever(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add an if-else conditional node
    pub fn if_else(mut self, name: impl Into<String>, condition: Condition) -> Self {
        let node = Node::new(name.into(), NodeKind::IfElse(condition));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a tool (MCP) node
    pub fn tool(mut self, name: impl Into<String>, config: McpConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::Tool(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a loop node
    pub fn loop_node(mut self, name: impl Into<String>, config: LoopConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::Loop(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a try-catch node
    pub fn try_catch(mut self, name: impl Into<String>, config: TryCatchConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::TryCatch(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a sub-workflow node
    pub fn sub_workflow(mut self, name: impl Into<String>, config: SubWorkflowConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::SubWorkflow(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a switch node
    pub fn switch(mut self, name: impl Into<String>, config: SwitchConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::Switch(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a parallel execution node
    pub fn parallel(mut self, name: impl Into<String>, config: ParallelConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::Parallel(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add an approval node
    pub fn approval(mut self, name: impl Into<String>, config: ApprovalConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::Approval(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a form input node
    pub fn form(mut self, name: impl Into<String>, config: FormConfig) -> Self {
        let node = Node::new(name.into(), NodeKind::Form(config));
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add a custom node (low-level API)
    pub fn node(mut self, node: Node) -> Self {
        let node_id = node.id;
        self.workflow.add_node(node);

        // Auto-connect from last node if exists
        if let Some(from_id) = self.last_node_id {
            self.workflow.add_edge(Edge::new(from_id, node_id));
        }

        self.last_node_id = Some(node_id);
        self
    }

    /// Add an edge between two nodes by their indices (0-based)
    pub fn connect(mut self, from_index: usize, to_index: usize) -> Self {
        if from_index < self.workflow.nodes.len() && to_index < self.workflow.nodes.len() {
            let from_id = self.workflow.nodes[from_index].id;
            let to_id = self.workflow.nodes[to_index].id;
            self.workflow.add_edge(Edge::new(from_id, to_id));
        }
        self
    }

    /// Add an edge between two nodes by their IDs
    pub fn connect_ids(mut self, from_id: NodeId, to_id: NodeId) -> Self {
        self.workflow.add_edge(Edge::new(from_id, to_id));
        self
    }

    /// Get the ID of the last added node
    pub fn last_node_id(&self) -> Option<NodeId> {
        self.last_node_id
    }

    /// Get the ID of a node by its index (0-based)
    pub fn node_id_at(&self, index: usize) -> Option<NodeId> {
        self.workflow.nodes.get(index).map(|n| n.id)
    }

    /// Build the workflow
    pub fn build(self) -> Workflow {
        self.workflow
    }
}

/// Node builder for configuring individual nodes with retry and timeout
pub struct NodeBuilder {
    node: Node,
}

impl NodeBuilder {
    /// Create a new node builder
    pub fn new(name: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            node: Node::new(name.into(), kind),
        }
    }

    /// Set retry configuration
    pub fn retry(mut self, config: RetryConfig) -> Self {
        self.node.retry_config = Some(config);
        self
    }

    /// Set timeout configuration
    pub fn timeout(mut self, config: TimeoutConfig) -> Self {
        self.node.timeout_config = Some(config);
        self
    }

    /// Set node position in visual editor
    pub fn position(mut self, x: f64, y: f64) -> Self {
        self.node.position = Some((x, y));
        self
    }

    /// Build the node
    pub fn build(self) -> Node {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_builder_basic() {
        let workflow = WorkflowBuilder::new("Test Workflow")
            .description("A test workflow")
            .version("1.0.0")
            .tag("test")
            .start("Start")
            .end("End")
            .build();

        assert_eq!(workflow.metadata.name, "Test Workflow");
        assert_eq!(
            workflow.metadata.description,
            Some("A test workflow".to_string())
        );
        assert_eq!(workflow.metadata.version, "1.0.0");
        assert_eq!(workflow.metadata.tags, vec!["test"]);
        assert_eq!(workflow.nodes.len(), 2);
        assert_eq!(workflow.edges.len(), 1);
    }

    #[test]
    fn test_workflow_builder_with_llm() {
        let llm_config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "Hello {{input}}".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::json!({}),
        };

        let workflow = WorkflowBuilder::new("LLM Workflow")
            .start("Start")
            .llm("Generate", llm_config)
            .end("End")
            .build();

        assert_eq!(workflow.nodes.len(), 3);
        assert_eq!(workflow.edges.len(), 2);

        // Check that LLM node exists
        let llm_node = &workflow.nodes[1];
        assert_eq!(llm_node.name, "Generate");
        assert!(matches!(llm_node.kind, NodeKind::LLM(_)));
    }

    #[test]
    fn test_workflow_builder_with_code() {
        let script_config = ScriptConfig {
            runtime: "rust".to_string(),
            code: "println!(\"Hello\");".to_string(),
            inputs: vec![],
            output: "result".to_string(),
        };

        let workflow = WorkflowBuilder::new("Code Workflow")
            .start("Start")
            .code("Execute", script_config)
            .end("End")
            .build();

        assert_eq!(workflow.nodes.len(), 3);
        assert_eq!(workflow.edges.len(), 2);
    }

    #[test]
    fn test_workflow_builder_custom_connections() {
        let workflow = WorkflowBuilder::new("Custom Connections")
            .start("Start")
            .end("End")
            .connect(0, 1) // Connect start to end
            .build();

        assert_eq!(workflow.edges.len(), 2); // Auto-connect + manual connect
    }

    #[test]
    fn test_node_builder() {
        let retry_config = RetryConfig {
            max_retries: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        };

        let timeout_config = TimeoutConfig {
            execution_timeout_ms: 60000,
            idle_timeout_ms: None,
            timeout_action: crate::TimeoutAction::Fail,
        };

        let node = NodeBuilder::new("Test Node", NodeKind::Start)
            .retry(retry_config)
            .timeout(timeout_config)
            .position(100.0, 200.0)
            .build();

        assert_eq!(node.name, "Test Node");
        assert!(node.retry_config.is_some());
        assert!(node.timeout_config.is_some());
        assert_eq!(node.position, Some((100.0, 200.0)));
    }

    #[test]
    fn test_workflow_builder_multiple_tags() {
        let workflow = WorkflowBuilder::new("Tagged Workflow")
            .tags(vec!["tag1".to_string(), "tag2".to_string()])
            .tag("tag3")
            .build();

        assert_eq!(workflow.metadata.tags.len(), 3);
        assert!(workflow.metadata.tags.contains(&"tag1".to_string()));
        assert!(workflow.metadata.tags.contains(&"tag2".to_string()));
        assert!(workflow.metadata.tags.contains(&"tag3".to_string()));
    }

    #[test]
    fn test_workflow_builder_get_node_ids() {
        let builder = WorkflowBuilder::new("Test").start("Start").end("End");

        assert!(builder.last_node_id().is_some());
        assert!(builder.node_id_at(0).is_some());
        assert!(builder.node_id_at(1).is_some());
        assert!(builder.node_id_at(2).is_none());
    }

    #[test]
    fn test_workflow_builder_if_else() {
        use uuid::Uuid;

        let true_branch_id = Uuid::new_v4();
        let false_branch_id = Uuid::new_v4();

        let condition = Condition {
            expression: "{{value}} > 10".to_string(),
            true_branch: true_branch_id,
            false_branch: false_branch_id,
        };

        let workflow = WorkflowBuilder::new("Conditional Workflow")
            .start("Start")
            .if_else("Check Value", condition)
            .end("End")
            .build();

        assert_eq!(workflow.nodes.len(), 3);
        assert!(matches!(workflow.nodes[1].kind, NodeKind::IfElse(_)));
    }

    #[test]
    fn test_workflow_builder_auto_connect() {
        let llm_config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::json!({}),
        };

        let workflow = WorkflowBuilder::new("Auto Connect Test")
            .start("Start")
            .llm("LLM1", llm_config.clone())
            .llm("LLM2", llm_config)
            .end("End")
            .build();

        // Should have 4 nodes and 3 auto-connected edges
        assert_eq!(workflow.nodes.len(), 4);
        assert_eq!(workflow.edges.len(), 3);
    }
}
