//! Workflow MCP server - exposes OxiFY workflows as MCP tools
//!
//! This server allows OxiFY workflows to be invoked via the MCP protocol,
//! enabling integration with Claude Desktop, Cline, Zed, and other MCP clients.

use crate::{McpError, McpServer, Result};
use async_trait::async_trait;
use oxify_model::Workflow;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Type alias for workflow executor function
pub type WorkflowExecutor = Arc<
    dyn Fn(Workflow, Value) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> + Send + Sync,
>;

/// Configuration for workflow server
#[derive(Debug, Clone)]
pub struct WorkflowServerConfig {
    /// Prefix for tool names (e.g., "oxify_" -> "oxify_my_workflow")
    pub tool_prefix: String,
    /// Whether to include workflow description in tool schema
    pub include_descriptions: bool,
    /// Maximum execution timeout in seconds
    pub execution_timeout_secs: u64,
}

impl Default for WorkflowServerConfig {
    fn default() -> Self {
        Self {
            tool_prefix: "workflow_".to_string(),
            include_descriptions: true,
            execution_timeout_secs: 300,
        }
    }
}

/// Registered workflow with metadata
struct RegisteredWorkflow {
    workflow: Workflow,
    /// Custom input schema (if not auto-generated)
    custom_input_schema: Option<Value>,
    /// Custom description override
    custom_description: Option<String>,
}

/// Built-in MCP server for executing OxiFY workflows
///
/// This server exposes workflows as MCP tools, allowing external clients
/// like Claude Desktop to invoke workflow executions.
///
/// # Features
/// - Auto-generates tool schemas from workflow metadata
/// - Supports custom input schemas per workflow
/// - Configurable tool naming prefix
/// - Thread-safe workflow registration
///
/// # Example
/// ```ignore
/// use oxify_mcp::servers::WorkflowServer;
/// use oxify_model::Workflow;
///
/// let server = WorkflowServer::new();
/// server.register_workflow(my_workflow).await;
///
/// // Server can now be used via MCP protocol
/// let tools = server.list_tools().await?;
/// ```
pub struct WorkflowServer {
    /// Registered workflows (keyed by tool name)
    workflows: Arc<RwLock<HashMap<String, RegisteredWorkflow>>>,
    /// Server configuration
    config: WorkflowServerConfig,
    /// Optional executor for running workflows
    executor: Option<WorkflowExecutor>,
}

impl WorkflowServer {
    /// Create a new workflow server with default configuration
    pub fn new() -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            config: WorkflowServerConfig::default(),
            executor: None,
        }
    }

    /// Create a new workflow server with custom configuration
    pub fn with_config(config: WorkflowServerConfig) -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            config,
            executor: None,
        }
    }

    /// Set the workflow executor function
    ///
    /// The executor is called when a workflow tool is invoked.
    /// It receives the workflow definition and input variables.
    pub fn with_executor(mut self, executor: WorkflowExecutor) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Generate tool name from workflow
    fn tool_name(&self, workflow: &Workflow) -> String {
        let name = workflow
            .metadata
            .name
            .to_lowercase()
            .replace([' ', '-'], "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();

        format!("{}{}", self.config.tool_prefix, name)
    }

    /// Register a workflow as an MCP tool
    pub async fn register_workflow(&self, workflow: Workflow) -> Result<String> {
        let tool_name = self.tool_name(&workflow);

        let registered = RegisteredWorkflow {
            workflow,
            custom_input_schema: None,
            custom_description: None,
        };

        let mut workflows = self.workflows.write().await;
        workflows.insert(tool_name.clone(), registered);

        tracing::info!(tool_name = %tool_name, "Registered workflow as MCP tool");

        Ok(tool_name)
    }

    /// Register a workflow with custom schema
    pub async fn register_workflow_with_schema(
        &self,
        workflow: Workflow,
        input_schema: Value,
        description: Option<String>,
    ) -> Result<String> {
        let tool_name = self.tool_name(&workflow);

        let registered = RegisteredWorkflow {
            workflow,
            custom_input_schema: Some(input_schema),
            custom_description: description,
        };

        let mut workflows = self.workflows.write().await;
        workflows.insert(tool_name.clone(), registered);

        tracing::info!(tool_name = %tool_name, "Registered workflow with custom schema as MCP tool");

        Ok(tool_name)
    }

    /// Unregister a workflow
    pub async fn unregister_workflow(&self, tool_name: &str) -> Result<()> {
        let mut workflows = self.workflows.write().await;

        if workflows.remove(tool_name).is_some() {
            tracing::info!(tool_name = %tool_name, "Unregistered workflow MCP tool");
            Ok(())
        } else {
            Err(McpError::ToolNotFound(tool_name.to_string()))
        }
    }

    /// Get list of registered tool names
    pub async fn list_registered_tools(&self) -> Vec<String> {
        let workflows = self.workflows.read().await;
        workflows.keys().cloned().collect()
    }

    /// Get workflow by tool name
    pub async fn get_workflow(&self, tool_name: &str) -> Option<Workflow> {
        let workflows = self.workflows.read().await;
        workflows.get(tool_name).map(|r| r.workflow.clone())
    }

    /// Generate input schema from workflow
    fn generate_input_schema(&self, workflow: &Workflow) -> Value {
        // Collect all variables used in the workflow
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        // Add a generic "input" property for workflow input
        properties.insert(
            "input".to_string(),
            json!({
                "type": "object",
                "description": "Input variables for the workflow execution"
            }),
        );

        // Check for template variables in node configurations
        for node in &workflow.nodes {
            if let oxify_model::NodeKind::LLM(config) = &node.kind {
                // Extract template variables like {{variable}}
                let template_vars = extract_template_variables(&config.prompt_template);
                for var in template_vars {
                    if !properties.contains_key(&var) {
                        properties.insert(
                            var.clone(),
                            json!({
                                "type": "string",
                                "description": format!("Template variable for workflow: {}", var)
                            }),
                        );
                        required.push(json!(var));
                    }
                }
            }
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    }

    /// Generate tool description from workflow
    fn generate_description(&self, workflow: &Workflow) -> String {
        let base = workflow
            .metadata
            .description
            .clone()
            .unwrap_or_else(|| format!("Execute the {} workflow", workflow.metadata.name));

        format!(
            "{}. Nodes: {}, Edges: {}",
            base,
            workflow.nodes.len(),
            workflow.edges.len()
        )
    }
}

impl Default for WorkflowServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract template variables from a string ({{variable}})
fn extract_template_variables(text: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second '{'
            let mut var_name = String::new();
            while let Some(&next) = chars.peek() {
                if next == '}' {
                    chars.next();
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        if !var_name.is_empty() {
                            vars.push(var_name.trim().to_string());
                        }
                        break;
                    }
                } else {
                    var_name.push(chars.next().unwrap());
                }
            }
        }
    }

    vars
}

#[async_trait]
impl McpServer for WorkflowServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let workflows = self.workflows.read().await;

        let registered = workflows
            .get(name)
            .ok_or_else(|| McpError::ToolNotFound(name.to_string()))?;

        let workflow = registered.workflow.clone();
        drop(workflows); // Release the lock before execution

        tracing::info!(
            tool_name = %name,
            workflow_id = %workflow.metadata.id,
            "Executing workflow via MCP"
        );

        // If we have an executor, use it
        if let Some(executor) = &self.executor {
            let result = executor(workflow, arguments).await?;
            return Ok(result);
        }

        // Without an executor, return workflow info (useful for testing)
        Ok(json!({
            "status": "accepted",
            "workflow_id": workflow.metadata.id.to_string(),
            "workflow_name": workflow.metadata.name,
            "message": "Workflow execution request accepted. No executor configured - configure with_executor() for actual execution.",
            "input": arguments,
            "nodes": workflow.nodes.len(),
            "edges": workflow.edges.len()
        }))
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        let workflows = self.workflows.read().await;
        let mut tools = Vec::new();

        for (tool_name, registered) in workflows.iter() {
            let description = registered
                .custom_description
                .clone()
                .unwrap_or_else(|| self.generate_description(&registered.workflow));

            let input_schema = registered
                .custom_input_schema
                .clone()
                .unwrap_or_else(|| self.generate_input_schema(&registered.workflow));

            tools.push(json!({
                "name": tool_name,
                "description": description,
                "inputSchema": input_schema
            }));
        }

        Ok(tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, Node, NodeKind};

    fn create_test_workflow(name: &str) -> Workflow {
        let mut workflow = Workflow::new(name.to_string());
        workflow.metadata.description = Some("A test workflow".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start.id;

        let end = Node::new("End".to_string(), NodeKind::End);
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        workflow
    }

    #[tokio::test]
    async fn test_register_workflow() {
        let server = WorkflowServer::new();
        let workflow = create_test_workflow("Test Workflow");

        let tool_name = server.register_workflow(workflow).await.unwrap();

        assert_eq!(tool_name, "workflow_test_workflow");

        let tools = server.list_registered_tools().await;
        assert_eq!(tools.len(), 1);
        assert!(tools.contains(&"workflow_test_workflow".to_string()));
    }

    #[tokio::test]
    async fn test_list_tools() {
        let server = WorkflowServer::new();

        server
            .register_workflow(create_test_workflow("Workflow One"))
            .await
            .unwrap();
        server
            .register_workflow(create_test_workflow("Workflow Two"))
            .await
            .unwrap();

        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"workflow_workflow_one"));
        assert!(names.contains(&"workflow_workflow_two"));
    }

    #[tokio::test]
    async fn test_call_tool_without_executor() {
        let server = WorkflowServer::new();
        let workflow = create_test_workflow("Execute Me");

        let tool_name = server.register_workflow(workflow).await.unwrap();

        let result = server
            .call_tool(&tool_name, json!({"input": {"key": "value"}}))
            .await
            .unwrap();

        assert_eq!(result["status"], "accepted");
        assert_eq!(result["workflow_name"], "Execute Me");
    }

    #[tokio::test]
    async fn test_call_tool_with_executor() {
        let executor: WorkflowExecutor = Arc::new(|workflow, args| {
            Box::pin(async move {
                Ok(json!({
                    "executed": true,
                    "workflow_name": workflow.metadata.name,
                    "received_args": args
                }))
            })
        });

        let server = WorkflowServer::new().with_executor(executor);
        let workflow = create_test_workflow("Custom Execute");

        let tool_name = server.register_workflow(workflow).await.unwrap();

        let result = server
            .call_tool(&tool_name, json!({"test": "data"}))
            .await
            .unwrap();

        assert_eq!(result["executed"], true);
        assert_eq!(result["workflow_name"], "Custom Execute");
        assert_eq!(result["received_args"]["test"], "data");
    }

    #[tokio::test]
    async fn test_unregister_workflow() {
        let server = WorkflowServer::new();
        let workflow = create_test_workflow("To Remove");

        let tool_name = server.register_workflow(workflow).await.unwrap();
        assert_eq!(server.list_registered_tools().await.len(), 1);

        server.unregister_workflow(&tool_name).await.unwrap();
        assert_eq!(server.list_registered_tools().await.len(), 0);
    }

    #[tokio::test]
    async fn test_tool_not_found() {
        let server = WorkflowServer::new();

        let result = server.call_tool("nonexistent_tool", json!({})).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::ToolNotFound(name) => assert_eq!(name, "nonexistent_tool"),
            _ => panic!("Expected ToolNotFound error"),
        }
    }

    #[tokio::test]
    async fn test_custom_config() {
        let config = WorkflowServerConfig {
            tool_prefix: "oxify_".to_string(),
            include_descriptions: true,
            execution_timeout_secs: 60,
        };

        let server = WorkflowServer::with_config(config);
        let workflow = create_test_workflow("My Workflow");

        let tool_name = server.register_workflow(workflow).await.unwrap();

        assert_eq!(tool_name, "oxify_my_workflow");
    }

    #[tokio::test]
    async fn test_register_with_custom_schema() {
        let server = WorkflowServer::new();
        let workflow = create_test_workflow("Typed Workflow");

        let custom_schema = json!({
            "type": "object",
            "properties": {
                "message": {"type": "string"},
                "count": {"type": "integer"}
            },
            "required": ["message"]
        });

        let tool_name = server
            .register_workflow_with_schema(
                workflow,
                custom_schema.clone(),
                Some("Custom description".to_string()),
            )
            .await
            .unwrap();

        let tools = server.list_tools().await.unwrap();
        let tool = tools.iter().find(|t| t["name"] == tool_name).unwrap();

        assert_eq!(tool["description"], "Custom description");
        assert_eq!(tool["inputSchema"], custom_schema);
    }

    #[test]
    fn test_extract_template_variables() {
        let vars = extract_template_variables("Hello {{name}}, your order {{order_id}} is ready!");
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&"name".to_string()));
        assert!(vars.contains(&"order_id".to_string()));
    }

    #[test]
    fn test_extract_template_variables_empty() {
        let vars = extract_template_variables("No variables here");
        assert!(vars.is_empty());
    }

    #[test]
    fn test_extract_template_variables_whitespace() {
        let vars = extract_template_variables("Hello {{ name }}, {{ count }} items");
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&"name".to_string()));
        assert!(vars.contains(&"count".to_string()));
    }
}
