//! Model Context Protocol implementation for OxiFY

pub mod auth;
pub mod registry;
pub mod servers;
mod transport;

pub use auth::{
    ApiKeyAuth, AuthConfig, AuthMethod, AuthenticatedHttpTransport, BasicAuth, BearerAuth,
    CredentialStore, CustomHeaderAuth,
};
pub use registry::{
    LoadBalanceConfig, LoadBalanceStrategy, McpRegistry, RegistryStats, ServerEntry, ServerHealth,
    ServerMetrics,
};
pub use servers::{
    DatabaseConfig, DatabaseServer, DatabaseType, ExecuteResult, FilesystemServer, GitServer,
    QueryResult, ShellServer, StatementResult, TransactionResult, WebServer, WorkflowExecutor,
    WorkflowServer, WorkflowServerConfig,
};
pub use transport::{HttpTransport, McpTransport, StdioTransport};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, McpError>;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Tool execution error: {0}")]
    ToolExecutionError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub server_id: String,
    pub tool_name: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Trait for MCP server communication
#[async_trait]
pub trait McpClient: Send + Sync {
    async fn invoke_tool(&mut self, request: McpRequest) -> Result<McpResponse>;
    async fn list_tools(&mut self, server_id: &str) -> Result<Vec<ToolSchema>>;
}

/// Trait for implementing MCP servers
#[async_trait]
pub trait McpServer: Send + Sync {
    /// Call a tool provided by this server
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value>;

    /// List all tools provided by this server
    async fn list_tools(&self) -> Result<Vec<serde_json::Value>>;
}

/// MCP client implementation using transport layer
pub struct DefaultMcpClient<T: McpTransport> {
    transport: T,
    #[allow(dead_code)]
    server_id: String,
}

impl<T: McpTransport> DefaultMcpClient<T> {
    pub fn new(transport: T, server_id: String) -> Self {
        Self {
            transport,
            server_id,
        }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        let request = json!({
            "method": "initialize",
            "params": {
                "protocolVersion": "1.0",
                "clientInfo": {
                    "name": "oxify",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        let _response = self.transport.send_request(request).await?;
        Ok(())
    }

    pub async fn close(&mut self) -> Result<()> {
        self.transport.close().await
    }
}

#[async_trait]
impl<T: McpTransport> McpClient for DefaultMcpClient<T> {
    async fn invoke_tool(&mut self, request: McpRequest) -> Result<McpResponse> {
        let rpc_request = json!({
            "method": "tools/call",
            "params": {
                "name": request.tool_name,
                "arguments": request.parameters
            }
        });

        let response = self.transport.send_request(rpc_request).await?;

        if let Some(error) = response.get("error") {
            return Err(McpError::ServerError(format!(
                "Tool invocation failed: {}",
                error
            )));
        }

        let result = response
            .get("result")
            .ok_or_else(|| McpError::ProtocolError("Missing result field".to_string()))?
            .clone();

        Ok(McpResponse { result })
    }

    async fn list_tools(&mut self, _server_id: &str) -> Result<Vec<ToolSchema>> {
        let request = json!({
            "method": "tools/list",
            "params": {}
        });

        let response = self.transport.send_request(request).await?;

        if let Some(error) = response.get("error") {
            return Err(McpError::ServerError(format!(
                "Failed to list tools: {}",
                error
            )));
        }

        let result = response
            .get("result")
            .and_then(|r| r.get("tools"))
            .ok_or_else(|| McpError::ProtocolError("Missing tools in response".to_string()))?;

        let tools: Vec<ToolSchema> = serde_json::from_value(result.clone())
            .map_err(|e| McpError::ProtocolError(format!("Failed to parse tools: {}", e)))?;

        Ok(tools)
    }
}
