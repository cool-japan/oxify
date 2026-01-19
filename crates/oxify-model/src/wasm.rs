//! WebAssembly bindings for oxify-model
//!
//! This module provides TypeScript/JavaScript bindings for workflow manipulation
//! using wasm-bindgen. Compile with `wasm-pack build` to generate npm package.
//!
//! # Usage from TypeScript
//!
//! ```typescript
//! import { WasmWorkflow, WasmWorkflowBuilder } from 'oxify-model';
//!
//! // Create a workflow using the builder
//! const builder = new WasmWorkflowBuilder("my-workflow");
//! builder
//!     .description("A sample workflow")
//!     .addTag("demo")
//!     .startNode("Start")
//!     .llmNode("Process", "openai", "gpt-4", "Process this: {{input}}")
//!     .endNode("End");
//!
//! const workflow = builder.build();
//! console.log(workflow.toJson());
//! ```

use crate::{
    builder::WorkflowBuilder as RustWorkflowBuilder, Condition, Edge, LlmConfig, LoopConfig,
    LoopType, McpConfig, Node, NodeKind, ScriptConfig, SwitchCase, SwitchConfig, VectorConfig,
    Workflow,
};
use serde_json::Value;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

/// Result type for WASM operations
pub type WasmResult<T> = Result<T, JsValue>;

/// Workflow wrapper for WebAssembly
#[wasm_bindgen]
pub struct WasmWorkflow {
    inner: Workflow,
}

#[wasm_bindgen]
impl WasmWorkflow {
    /// Create a new workflow with a name
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str) -> Self {
        Self {
            inner: Workflow::new(name.to_string()),
        }
    }

    /// Parse a workflow from JSON string
    #[wasm_bindgen(js_name = "fromJson")]
    pub fn from_json(json: &str) -> WasmResult<WasmWorkflow> {
        let workflow: Workflow =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self { inner: workflow })
    }

    /// Parse a workflow from YAML string
    #[wasm_bindgen(js_name = "fromYaml")]
    pub fn from_yaml(yaml: &str) -> WasmResult<WasmWorkflow> {
        let workflow: Workflow =
            serde_yaml::from_str(yaml).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self { inner: workflow })
    }

    /// Serialize workflow to JSON string
    #[wasm_bindgen(js_name = "toJson")]
    pub fn to_json(&self) -> WasmResult<String> {
        serde_json::to_string_pretty(&self.inner).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Serialize workflow to YAML string
    #[wasm_bindgen(js_name = "toYaml")]
    pub fn to_yaml(&self) -> WasmResult<String> {
        serde_yaml::to_string(&self.inner).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Get the workflow ID
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.inner.metadata.id.to_string()
    }

    /// Get the workflow name
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.inner.metadata.name.clone()
    }

    /// Set the workflow name
    #[wasm_bindgen(setter)]
    pub fn set_name(&mut self, name: &str) {
        self.inner.metadata.name = name.to_string();
    }

    /// Get the workflow description
    #[wasm_bindgen(getter)]
    pub fn description(&self) -> Option<String> {
        self.inner.metadata.description.clone()
    }

    /// Set the workflow description
    #[wasm_bindgen(setter)]
    pub fn set_description(&mut self, description: &str) {
        self.inner.metadata.description = Some(description.to_string());
    }

    /// Get the workflow version
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> String {
        self.inner.metadata.version.clone()
    }

    /// Set the workflow version
    #[wasm_bindgen(setter)]
    pub fn set_version(&mut self, version: &str) {
        self.inner.metadata.version = version.to_string();
    }

    /// Get the number of nodes
    #[wasm_bindgen(js_name = "nodeCount")]
    pub fn node_count(&self) -> usize {
        self.inner.nodes.len()
    }

    /// Get the number of edges
    #[wasm_bindgen(js_name = "edgeCount")]
    pub fn edge_count(&self) -> usize {
        self.inner.edges.len()
    }

    /// Get all node IDs as JSON array
    #[wasm_bindgen(js_name = "getNodeIds")]
    pub fn get_node_ids(&self) -> String {
        let ids: Vec<String> = self.inner.nodes.iter().map(|n| n.id.to_string()).collect();
        serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get a node by ID as JSON
    #[wasm_bindgen(js_name = "getNode")]
    pub fn get_node(&self, id: &str) -> WasmResult<String> {
        let uuid = Uuid::parse_str(id).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let node = self
            .inner
            .nodes
            .iter()
            .find(|n| n.id == uuid)
            .ok_or_else(|| JsValue::from_str("Node not found"))?;
        serde_json::to_string(node).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Get tags as JSON array
    #[wasm_bindgen(js_name = "getTags")]
    pub fn get_tags(&self) -> String {
        serde_json::to_string(&self.inner.metadata.tags).unwrap_or_else(|_| "[]".to_string())
    }

    /// Add a tag
    #[wasm_bindgen(js_name = "addTag")]
    pub fn add_tag(&mut self, tag: &str) {
        if !self.inner.metadata.tags.contains(&tag.to_string()) {
            self.inner.metadata.tags.push(tag.to_string());
        }
    }

    /// Remove a tag
    #[wasm_bindgen(js_name = "removeTag")]
    pub fn remove_tag(&mut self, tag: &str) {
        self.inner.metadata.tags.retain(|t| t != tag);
    }

    /// Validate the workflow
    #[wasm_bindgen]
    pub fn validate(&self) -> WasmResult<bool> {
        match self.inner.validate() {
            Ok(_) => Ok(true),
            Err(e) => Err(JsValue::from_str(&e.to_string())),
        }
    }

    /// Get validation errors as JSON array
    #[wasm_bindgen(js_name = "getValidationErrors")]
    pub fn get_validation_errors(&self) -> String {
        match self.inner.validate() {
            Ok(_) => "[]".to_string(),
            Err(report) => {
                // report is a String from the validate() method
                serde_json::to_string(&vec![report]).unwrap_or_else(|_| "[]".to_string())
            }
        }
    }

    /// Clone the workflow
    #[wasm_bindgen(js_name = "clone")]
    pub fn clone_workflow(&self) -> WasmWorkflow {
        WasmWorkflow {
            inner: self.inner.clone(),
        }
    }

    /// Add a node directly from JSON
    #[wasm_bindgen(js_name = "addNodeFromJson")]
    pub fn add_node_from_json(&mut self, node_json: &str) -> WasmResult<String> {
        let node: Node =
            serde_json::from_str(node_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let id = node.id.to_string();
        self.inner.add_node(node);
        Ok(id)
    }

    /// Add an edge between two nodes
    #[wasm_bindgen(js_name = "addEdge")]
    pub fn add_edge(&mut self, from_id: &str, to_id: &str) -> WasmResult<()> {
        let from = Uuid::parse_str(from_id).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let to = Uuid::parse_str(to_id).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner.add_edge(Edge::new(from, to));
        Ok(())
    }
}

/// Fluent builder for creating workflows in WebAssembly
#[wasm_bindgen]
pub struct WasmWorkflowBuilder {
    inner: RustWorkflowBuilder,
}

#[wasm_bindgen]
impl WasmWorkflowBuilder {
    /// Create a new workflow builder
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str) -> Self {
        Self {
            inner: RustWorkflowBuilder::new(name),
        }
    }

    /// Set the workflow description
    #[wasm_bindgen]
    pub fn description(mut self, desc: &str) -> Self {
        self.inner = self.inner.description(desc);
        self
    }

    /// Set the workflow version
    #[wasm_bindgen]
    pub fn version(mut self, version: &str) -> Self {
        self.inner = self.inner.version(version);
        self
    }

    /// Add a tag
    #[wasm_bindgen(js_name = "addTag")]
    pub fn add_tag(mut self, tag: &str) -> Self {
        self.inner = self.inner.tag(tag);
        self
    }

    /// Add multiple tags from JSON array
    #[wasm_bindgen(js_name = "addTags")]
    pub fn add_tags(mut self, tags_json: &str) -> WasmResult<WasmWorkflowBuilder> {
        let tags: Vec<String> =
            serde_json::from_str(tags_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner = self.inner.tags(tags);
        Ok(self)
    }

    /// Add a start node
    #[wasm_bindgen(js_name = "startNode")]
    pub fn start_node(mut self, name: &str) -> Self {
        self.inner = self.inner.start(name);
        self
    }

    /// Add an end node
    #[wasm_bindgen(js_name = "endNode")]
    pub fn end_node(mut self, name: &str) -> Self {
        self.inner = self.inner.end(name);
        self
    }

    /// Add an LLM node
    #[wasm_bindgen(js_name = "llmNode")]
    pub fn llm_node(mut self, name: &str, provider: &str, model: &str, prompt: &str) -> Self {
        let config = LlmConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            system_prompt: None,
            prompt_template: prompt.to_string(),
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::json!({}),
        };
        self.inner = self.inner.llm(name, config);
        self
    }

    /// Add an LLM node with system prompt
    #[wasm_bindgen(js_name = "llmNodeWithSystem")]
    pub fn llm_node_with_system(
        mut self,
        name: &str,
        provider: &str,
        model: &str,
        prompt: &str,
        system_prompt: &str,
    ) -> Self {
        let config = LlmConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            system_prompt: Some(system_prompt.to_string()),
            prompt_template: prompt.to_string(),
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::json!({}),
        };
        self.inner = self.inner.llm(name, config);
        self
    }

    /// Add a code execution node
    #[wasm_bindgen(js_name = "codeNode")]
    pub fn code_node(mut self, name: &str, runtime: &str, code: &str) -> Self {
        let config = ScriptConfig {
            runtime: runtime.to_string(),
            code: code.to_string(),
            inputs: vec![],
            output: "result".to_string(),
        };
        self.inner = self.inner.code(name, config);
        self
    }

    /// Add a vector retriever node
    #[wasm_bindgen(js_name = "retrieverNode")]
    pub fn retriever_node(
        mut self,
        name: &str,
        db_type: &str,
        collection: &str,
        query: &str,
        top_k: u32,
    ) -> Self {
        let config = VectorConfig {
            db_type: db_type.to_string(),
            collection: collection.to_string(),
            query: query.to_string(),
            top_k: top_k as usize,
            score_threshold: None,
        };
        self.inner = self.inner.retriever(name, config);
        self
    }

    /// Add an if-else conditional node (requires branch IDs to be set later)
    #[wasm_bindgen(js_name = "ifElseNode")]
    pub fn if_else_node(mut self, name: &str, condition: &str) -> Self {
        let cond = Condition {
            expression: condition.to_string(),
            true_branch: Uuid::nil(),  // Placeholder
            false_branch: Uuid::nil(), // Placeholder
        };
        self.inner = self.inner.if_else(name, cond);
        self
    }

    /// Add a tool (MCP) node
    #[wasm_bindgen(js_name = "toolNode")]
    pub fn tool_node(
        mut self,
        name: &str,
        server_id: &str,
        tool_name: &str,
        parameters_json: &str,
    ) -> WasmResult<WasmWorkflowBuilder> {
        let params: Value =
            serde_json::from_str(parameters_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let config = McpConfig {
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            parameters: params,
        };
        self.inner = self.inner.tool(name, config);
        Ok(self)
    }

    /// Add a switch node
    #[wasm_bindgen(js_name = "switchNode")]
    pub fn switch_node(
        mut self,
        name: &str,
        switch_on: &str,
        cases_json: &str,
    ) -> WasmResult<WasmWorkflowBuilder> {
        let cases: Vec<SwitchCaseJs> =
            serde_json::from_str(cases_json).map_err(|e| JsValue::from_str(&e.to_string()))?;

        let rust_cases: Vec<SwitchCase> = cases
            .into_iter()
            .map(|c| SwitchCase {
                match_value: c.value,
                action: c.action.unwrap_or_default(),
            })
            .collect();

        let config = SwitchConfig {
            switch_on: switch_on.to_string(),
            cases: rust_cases,
            default_case: None,
        };
        self.inner = self.inner.switch(name, config);
        Ok(self)
    }

    /// Add a foreach loop node
    #[wasm_bindgen(js_name = "forEachNode")]
    pub fn for_each_node(
        mut self,
        name: &str,
        collection: &str,
        item_var: &str,
        parallel: Option<bool>,
        max_concurrency: Option<u32>,
    ) -> Self {
        let config = LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: collection.to_string(),
                item_variable: item_var.to_string(),
                index_variable: None,
                body_expression: String::new(),
                parallel: parallel.unwrap_or(false),
                max_concurrency: max_concurrency.map(|v| v as usize),
            },
            max_iterations: 1000,
        };
        self.inner = self.inner.loop_node(name, config);
        self
    }

    /// Add a while loop node
    #[wasm_bindgen(js_name = "whileNode")]
    pub fn while_node(mut self, name: &str, condition: &str, max_iterations: u32) -> Self {
        let config = LoopConfig {
            loop_type: LoopType::While {
                condition: condition.to_string(),
                body_expression: String::new(),
                counter_variable: None,
            },
            max_iterations: max_iterations as usize,
        };
        self.inner = self.inner.loop_node(name, config);
        self
    }

    /// Add a repeat loop node
    #[wasm_bindgen(js_name = "repeatNode")]
    pub fn repeat_node(mut self, name: &str, count: u32) -> Self {
        let config = LoopConfig {
            loop_type: LoopType::Repeat {
                count: count.to_string(),
                body_expression: String::new(),
                index_variable: None,
            },
            max_iterations: count as usize,
        };
        self.inner = self.inner.loop_node(name, config);
        self
    }

    /// Connect two nodes by index
    #[wasm_bindgen]
    pub fn connect(mut self, from_idx: usize, to_idx: usize) -> Self {
        self.inner = self.inner.connect(from_idx, to_idx);
        self
    }

    /// Build the workflow
    #[wasm_bindgen]
    pub fn build(self) -> WasmWorkflow {
        WasmWorkflow {
            inner: self.inner.build(),
        }
    }
}

/// Helper struct for parsing switch cases from JS
#[derive(serde::Deserialize)]
struct SwitchCaseJs {
    value: String,
    action: Option<String>,
}

/// Utility functions for workflow manipulation
#[wasm_bindgen]
pub struct WasmWorkflowUtils;

#[wasm_bindgen]
impl WasmWorkflowUtils {
    /// Generate a new UUID
    #[wasm_bindgen(js_name = "generateId")]
    pub fn generate_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Validate a UUID string
    #[wasm_bindgen(js_name = "isValidUuid")]
    pub fn is_valid_uuid(id: &str) -> bool {
        Uuid::parse_str(id).is_ok()
    }

    /// Convert JSON to YAML
    #[wasm_bindgen(js_name = "jsonToYaml")]
    pub fn json_to_yaml(json: &str) -> WasmResult<String> {
        let value: Value =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_yaml::to_string(&value).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Convert YAML to JSON
    #[wasm_bindgen(js_name = "yamlToJson")]
    pub fn yaml_to_json(yaml: &str) -> WasmResult<String> {
        let value: Value =
            serde_yaml::from_str(yaml).map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&value).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

/// Node kind enumeration for JavaScript
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum WasmNodeKind {
    Start,
    End,
    LLM,
    Retriever,
    Code,
    IfElse,
    Tool,
    Switch,
    Parallel,
    Approval,
    Form,
    Loop,
    TryCatch,
    SubWorkflow,
    Vision,
}

impl From<&NodeKind> for WasmNodeKind {
    fn from(kind: &NodeKind) -> Self {
        match kind {
            NodeKind::Start => WasmNodeKind::Start,
            NodeKind::End => WasmNodeKind::End,
            NodeKind::LLM(_) => WasmNodeKind::LLM,
            NodeKind::Retriever(_) => WasmNodeKind::Retriever,
            NodeKind::Code(_) => WasmNodeKind::Code,
            NodeKind::IfElse(_) => WasmNodeKind::IfElse,
            NodeKind::Tool(_) => WasmNodeKind::Tool,
            NodeKind::Switch(_) => WasmNodeKind::Switch,
            NodeKind::Parallel(_) => WasmNodeKind::Parallel,
            NodeKind::Approval(_) => WasmNodeKind::Approval,
            NodeKind::Form(_) => WasmNodeKind::Form,
            NodeKind::Loop(_) => WasmNodeKind::Loop,
            NodeKind::TryCatch(_) => WasmNodeKind::TryCatch,
            NodeKind::SubWorkflow(_) => WasmNodeKind::SubWorkflow,
            NodeKind::Vision(_) => WasmNodeKind::Vision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_workflow_creation() {
        let workflow = WasmWorkflow::new("test-workflow");
        assert_eq!(workflow.name(), "test-workflow");
    }

    #[test]
    fn test_wasm_workflow_json_roundtrip() {
        let mut workflow = WasmWorkflow::new("test");
        workflow.set_description("A test workflow");
        workflow.set_version("1.0.0");
        workflow.add_tag("test");

        let json = workflow.to_json().unwrap();
        let parsed = WasmWorkflow::from_json(&json).unwrap();

        assert_eq!(parsed.name(), "test");
        assert_eq!(parsed.description(), Some("A test workflow".to_string()));
        assert_eq!(parsed.version(), "1.0.0");
    }

    #[test]
    fn test_wasm_workflow_yaml_roundtrip() {
        let workflow = WasmWorkflow::new("test");
        let yaml = workflow.to_yaml().unwrap();
        let parsed = WasmWorkflow::from_yaml(&yaml).unwrap();
        assert_eq!(parsed.name(), "test");
    }

    #[test]
    fn test_wasm_builder_simple() {
        let workflow = WasmWorkflowBuilder::new("simple")
            .description("A simple workflow")
            .version("1.0.0")
            .add_tag("demo")
            .start_node("Start")
            .llm_node("Process", "openai", "gpt-4", "Hello {{input}}")
            .end_node("End")
            .build();

        assert_eq!(workflow.name(), "simple");
        assert_eq!(workflow.node_count(), 3); // start, llm, end
    }

    #[test]
    fn test_wasm_builder_with_code() {
        let workflow = WasmWorkflowBuilder::new("code-workflow")
            .start_node("Start")
            .code_node("Code", "rhai", "let result = 1 + 1; result")
            .end_node("End")
            .build();

        assert_eq!(workflow.node_count(), 3);
    }

    #[test]
    fn test_wasm_utils_uuid() {
        let id = WasmWorkflowUtils::generate_id();
        assert!(WasmWorkflowUtils::is_valid_uuid(&id));
        assert!(!WasmWorkflowUtils::is_valid_uuid("not-a-uuid"));
    }

    #[test]
    fn test_wasm_utils_json_yaml_conversion() {
        let json = r#"{"key": "value", "number": 42}"#;
        let yaml = WasmWorkflowUtils::json_to_yaml(json).unwrap();
        assert!(yaml.contains("key:"));
        assert!(yaml.contains("value"));

        let back_to_json = WasmWorkflowUtils::yaml_to_json(&yaml).unwrap();
        let original: Value = serde_json::from_str(json).unwrap();
        let converted: Value = serde_json::from_str(&back_to_json).unwrap();
        assert_eq!(original, converted);
    }

    #[test]
    fn test_workflow_tags() {
        let mut workflow = WasmWorkflow::new("tagged");
        workflow.add_tag("tag1");
        workflow.add_tag("tag2");
        workflow.add_tag("tag1"); // Duplicate, should not be added

        let tags_json = workflow.get_tags();
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap();
        assert_eq!(tags.len(), 2);

        workflow.remove_tag("tag1");
        let tags_json = workflow.get_tags();
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap();
        assert_eq!(tags.len(), 1);
        assert!(tags.contains(&"tag2".to_string()));
    }

    #[test]
    fn test_workflow_clone() {
        let original = WasmWorkflow::new("original");
        let cloned = original.clone_workflow();
        assert_eq!(original.name(), cloned.name());
        assert_eq!(original.id(), cloned.id());
    }

    #[test]
    fn test_wasm_builder_loop_nodes() {
        let workflow = WasmWorkflowBuilder::new("loop-workflow")
            .start_node("Start")
            .for_each_node("ForEach", "items", "item", None, None)
            .while_node("While", "count < 10", 100)
            .repeat_node("Repeat", 5)
            .end_node("End")
            .build();

        assert_eq!(workflow.node_count(), 5);
    }

    #[test]
    fn test_wasm_builder_parallel_foreach() {
        let workflow = WasmWorkflowBuilder::new("parallel-foreach")
            .start_node("Start")
            .for_each_node("ParallelLoop", "items", "item", Some(true), Some(10))
            .end_node("End")
            .build();

        assert_eq!(workflow.node_count(), 3);

        // Verify parallel configuration was set correctly
        let inner = workflow.inner;
        if let Some(node) = inner.nodes.iter().find(|n| n.name == "ParallelLoop") {
            if let crate::NodeKind::Loop(config) = &node.kind {
                if let crate::LoopType::ForEach {
                    parallel,
                    max_concurrency,
                    ..
                } = &config.loop_type
                {
                    assert!(*parallel);
                    assert_eq!(*max_concurrency, Some(10));
                } else {
                    panic!("Expected ForEach loop");
                }
            } else {
                panic!("Expected Loop node");
            }
        } else {
            panic!("ParallelLoop node not found");
        }
    }

    #[test]
    fn test_wasm_builder_retriever() {
        let workflow = WasmWorkflowBuilder::new("rag-workflow")
            .start_node("Start")
            .retriever_node("Search", "qdrant", "documents", "{{query}}", 5)
            .llm_node(
                "Process",
                "openai",
                "gpt-4",
                "Context: {{Search.results}}\nQuestion: {{query}}",
            )
            .end_node("End")
            .build();

        assert_eq!(workflow.node_count(), 4);
    }
}
