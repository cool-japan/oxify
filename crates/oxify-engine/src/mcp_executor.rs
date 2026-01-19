//! MCP (Model Context Protocol) executor for Tool nodes
//!
//! This module provides integration with oxify-mcp to execute MCP tools
//! through various server transports (HTTP, stdio, filesystem, database, etc.)
//!
//! # Features
//! - Execute tools from remote MCP servers (HTTP/stdio transport)
//! - Execute tools from local MCP servers (FilesystemServer, WorkflowServer, etc.)
//! - Automatic server discovery and caching
//! - HTTP fallback for simple REST API calls

use oxify_model::ExecutionResult;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use oxify_mcp::{
    DefaultMcpClient, HttpTransport, McpClient, McpRequest, McpServer, Result as McpResult,
    StdioTransport,
};

/// MCP executor handles tool invocations via MCP servers
pub struct McpExecutor {
    /// Cached HTTP clients per server_id
    http_clients: Arc<RwLock<HashMap<String, DefaultMcpClient<HttpTransport>>>>,

    /// Cached stdio clients per server_id (for local servers)
    #[allow(dead_code)]
    stdio_clients: Arc<RwLock<HashMap<String, DefaultMcpClient<StdioTransport>>>>,

    /// Local MCP servers (FilesystemServer, WorkflowServer, etc.)
    local_servers: Arc<RwLock<HashMap<String, Arc<dyn McpServer>>>>,
}

impl McpExecutor {
    /// Create a new MCP executor
    pub fn new() -> Self {
        Self {
            http_clients: Arc::new(RwLock::new(HashMap::new())),
            stdio_clients: Arc::new(RwLock::new(HashMap::new())),
            local_servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a local MCP server
    ///
    /// Local servers are executed directly without network transport.
    /// This is useful for built-in servers like FilesystemServer, WorkflowServer, etc.
    ///
    /// # Example
    /// ```ignore
    /// use oxify_mcp::{FilesystemServer, WorkflowServer};
    ///
    /// let executor = McpExecutor::new();
    /// executor.register_local_server("fs", FilesystemServer::new("/tmp")).await;
    /// executor.register_local_server("workflows", WorkflowServer::new()).await;
    /// ```
    #[allow(dead_code)]
    pub async fn register_local_server<S: McpServer + 'static>(
        &self,
        server_id: impl Into<String>,
        server: S,
    ) {
        let server_id = server_id.into();
        let mut servers = self.local_servers.write().await;
        servers.insert(server_id.clone(), Arc::new(server));
        info!("Registered local MCP server: {}", server_id);
    }

    /// Unregister a local MCP server
    #[allow(dead_code)]
    pub async fn unregister_local_server(&self, server_id: &str) -> bool {
        let mut servers = self.local_servers.write().await;
        let removed = servers.remove(server_id).is_some();
        if removed {
            info!("Unregistered local MCP server: {}", server_id);
        }
        removed
    }

    /// List registered local servers
    #[allow(dead_code)]
    pub async fn list_local_servers(&self) -> Vec<String> {
        let servers = self.local_servers.read().await;
        servers.keys().cloned().collect()
    }

    /// Check if a local server is registered
    #[allow(dead_code)]
    pub async fn has_local_server(&self, server_id: &str) -> bool {
        let servers = self.local_servers.read().await;
        servers.contains_key(server_id)
    }

    /// Execute a tool via MCP
    ///
    /// Server resolution priority:
    /// 1. Local registered servers (server_id matches registered name)
    /// 2. HTTP MCP servers (server_id starts with http:// or https://)
    /// 3. HTTP fallback (for REST API compatibility)
    pub async fn execute_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        parameters: Value,
    ) -> ExecutionResult {
        debug!(
            "Executing MCP tool: server={}, tool={}, params={}",
            server_id, tool_name, parameters
        );

        // 1. Check for local registered server first
        {
            let servers = self.local_servers.read().await;
            if let Some(server) = servers.get(server_id) {
                return self
                    .execute_local_tool(server.clone(), tool_name, parameters)
                    .await;
            }
        }

        // 2. Check for HTTP MCP server
        if server_id.starts_with("http://") || server_id.starts_with("https://") {
            return self
                .execute_http_tool(server_id, tool_name, parameters)
                .await;
        }

        // 3. Fallback to HTTP client for simple HTTP requests
        // This maintains backward compatibility with existing HTTP-based tools
        self.execute_http_fallback(server_id, tool_name, parameters)
            .await
    }

    /// Execute tool on a local MCP server
    async fn execute_local_tool(
        &self,
        server: Arc<dyn McpServer>,
        tool_name: &str,
        parameters: Value,
    ) -> ExecutionResult {
        debug!("Executing tool on local MCP server: {}", tool_name);

        match server.call_tool(tool_name, parameters).await {
            Ok(result) => {
                info!("Local MCP tool executed successfully: {}", tool_name);
                ExecutionResult::Success(result)
            }
            Err(e) => {
                error!("Local MCP tool execution failed: {}", e);
                ExecutionResult::Failure(format!("Tool execution failed: {}", e))
            }
        }
    }

    /// Execute tool via HTTP MCP server
    async fn execute_http_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        parameters: Value,
    ) -> ExecutionResult {
        // Get or create HTTP client
        let mut clients = self.http_clients.write().await;

        if !clients.contains_key(server_id) {
            // Create new HTTP transport
            let transport = HttpTransport::new(server_id.to_string());
            let mut client = DefaultMcpClient::new(transport, server_id.to_string());

            // Initialize MCP connection
            if let Err(e) = client.initialize().await {
                error!("Failed to initialize MCP client: {}", e);
                return ExecutionResult::Failure(format!("MCP initialization failed: {}", e));
            }

            clients.insert(server_id.to_string(), client);
            info!("Initialized MCP HTTP client for {}", server_id);
        }

        let client = clients.get_mut(server_id).unwrap();

        // Invoke the tool
        let request = McpRequest {
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            parameters,
        };

        match client.invoke_tool(request).await {
            Ok(response) => {
                info!("MCP tool executed successfully: {}", tool_name);
                ExecutionResult::Success(response.result)
            }
            Err(e) => {
                error!("MCP tool execution failed: {}", e);
                ExecutionResult::Failure(format!("Tool execution failed: {}", e))
            }
        }
    }

    /// HTTP fallback for backward compatibility
    /// Handles simple HTTP requests when server_id doesn't start with http(s)://
    async fn execute_http_fallback(
        &self,
        server_id: &str,
        tool_name: &str,
        parameters: Value,
    ) -> ExecutionResult {
        // Parse tool_name as "METHOD /path"
        let parts: Vec<&str> = tool_name.split_whitespace().collect();
        if parts.len() != 2 {
            return ExecutionResult::Failure(format!(
                "Invalid tool_name format. Expected 'METHOD /path', got '{}'",
                tool_name
            ));
        }

        let method = parts[0].to_uppercase();
        let path = parts[1];
        let url = format!("{}{}", server_id.trim_end_matches('/'), path);

        let client = reqwest::Client::new();
        let mut request_builder = match method.as_str() {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            "DELETE" => client.delete(&url),
            _ => {
                return ExecutionResult::Failure(format!("Unsupported HTTP method: {}", method));
            }
        };

        // Add JSON body for POST/PUT/PATCH
        if matches!(method.as_str(), "POST" | "PUT" | "PATCH") {
            request_builder = request_builder.json(&parameters);
        }

        // Execute HTTP request
        match request_builder.send().await {
            Ok(response) => {
                let status = response.status();
                let status_code = status.as_u16();

                match response.text().await {
                    Ok(body) => {
                        // Try to parse as JSON
                        let body_value = serde_json::from_str::<Value>(&body)
                            .unwrap_or_else(|_| serde_json::json!({"body": body}));

                        if status.is_success() {
                            ExecutionResult::Success(serde_json::json!({
                                "status": status_code,
                                "data": body_value
                            }))
                        } else {
                            ExecutionResult::Failure(format!(
                                "HTTP {} error: {}",
                                status_code, body_value
                            ))
                        }
                    }
                    Err(e) => ExecutionResult::Failure(format!("Failed to read response: {}", e)),
                }
            }
            Err(e) => ExecutionResult::Failure(format!("HTTP request failed: {}", e)),
        }
    }

    /// List available tools from an MCP server
    ///
    /// This method discovers tools from both local and remote MCP servers.
    #[allow(dead_code)]
    pub async fn list_tools(&self, server_id: &str) -> McpResult<Vec<serde_json::Value>> {
        // 1. Check for local registered server first
        {
            let servers = self.local_servers.read().await;
            if let Some(server) = servers.get(server_id) {
                let tools = server
                    .list_tools()
                    .await
                    .map_err(|e| oxify_mcp::McpError::ServerError(e.to_string()))?;
                return Ok(tools);
            }
        }

        // 2. Check for HTTP MCP server
        if server_id.starts_with("http://") || server_id.starts_with("https://") {
            let mut clients = self.http_clients.write().await;

            if !clients.contains_key(server_id) {
                let transport = HttpTransport::new(server_id.to_string());
                let mut client = DefaultMcpClient::new(transport, server_id.to_string());
                client.initialize().await?;
                clients.insert(server_id.to_string(), client);
            }

            let client = clients.get_mut(server_id).unwrap();
            let tools = client.list_tools(server_id).await?;

            return Ok(tools
                .into_iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema
                    })
                })
                .collect());
        }

        // 3. For non-HTTP, non-registered servers, return empty list
        Ok(vec![])
    }

    /// List tools from all registered local servers
    #[allow(dead_code)]
    pub async fn list_all_local_tools(&self) -> HashMap<String, Vec<serde_json::Value>> {
        let servers = self.local_servers.read().await;
        let mut all_tools = HashMap::new();

        for (server_id, server) in servers.iter() {
            if let Ok(tools) = server.list_tools().await {
                all_tools.insert(server_id.clone(), tools);
            }
        }

        all_tools
    }
}

impl Default for McpExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_mcp::FilesystemServer;

    #[tokio::test]
    async fn test_mcp_executor_creation() {
        let executor = McpExecutor::new();
        assert!(executor.http_clients.read().await.is_empty());
        assert!(executor.local_servers.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_http_fallback_invalid_format() {
        let executor = McpExecutor::new();
        // Use non-http URL to trigger fallback path
        let result = executor
            .execute_tool("api.example.com", "invalid", serde_json::json!({}))
            .await;

        match result {
            ExecutionResult::Failure(msg) => {
                assert!(msg.contains("Invalid tool_name format"));
            }
            _ => panic!("Expected failure for invalid tool_name format"),
        }
    }

    #[tokio::test]
    async fn test_http_fallback_unsupported_method() {
        let executor = McpExecutor::new();
        // Use non-http URL to trigger fallback path
        let result = executor
            .execute_tool("api.example.com", "INVALID /path", serde_json::json!({}))
            .await;

        match result {
            ExecutionResult::Failure(msg) => {
                assert!(msg.contains("Unsupported HTTP method"));
            }
            _ => panic!("Expected failure for unsupported HTTP method"),
        }
    }

    #[tokio::test]
    async fn test_register_local_server() {
        let executor = McpExecutor::new();
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-register");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let fs_server = FilesystemServer::new(temp_dir.clone());
        executor.register_local_server("fs", fs_server).await;

        assert!(executor.has_local_server("fs").await);
        assert!(!executor.has_local_server("nonexistent").await);

        let servers = executor.list_local_servers().await;
        assert!(servers.contains(&"fs".to_string()));

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_unregister_local_server() {
        let executor = McpExecutor::new();
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-unregister");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let fs_server = FilesystemServer::new(temp_dir.clone());
        executor.register_local_server("fs", fs_server).await;
        assert!(executor.has_local_server("fs").await);

        let removed = executor.unregister_local_server("fs").await;
        assert!(removed);
        assert!(!executor.has_local_server("fs").await);

        // Second unregister should return false
        let removed_again = executor.unregister_local_server("fs").await;
        assert!(!removed_again);

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_execute_local_tool() {
        let executor = McpExecutor::new();
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-execute");
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create a test file
        std::fs::write(temp_dir.join("test.txt"), "Hello, MCP!").unwrap();

        let fs_server = FilesystemServer::new(temp_dir.clone());
        executor.register_local_server("fs", fs_server).await;

        // Execute fs_read tool
        let result = executor
            .execute_tool(
                "fs",
                "fs_read",
                serde_json::json!({
                    "path": "test.txt"
                }),
            )
            .await;

        match result {
            ExecutionResult::Success(value) => {
                assert_eq!(value["content"], "Hello, MCP!");
            }
            ExecutionResult::Failure(msg) => {
                panic!("Expected success, got failure: {}", msg);
            }
            _ => panic!("Expected success result"),
        }

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_list_local_tools() {
        let executor = McpExecutor::new();
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-list-tools");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let fs_server = FilesystemServer::new(temp_dir.clone());
        executor.register_local_server("fs", fs_server).await;

        let tools = executor.list_tools("fs").await.unwrap();
        assert!(!tools.is_empty());

        // Check that fs_read tool exists
        let has_fs_read = tools.iter().any(|t| t["name"] == "fs_read");
        assert!(has_fs_read, "Expected fs_read tool to be listed");

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_list_all_local_tools() {
        let executor = McpExecutor::new();
        let temp_dir1 = std::env::temp_dir().join("oxify-mcp-test-all-tools-1");
        let temp_dir2 = std::env::temp_dir().join("oxify-mcp-test-all-tools-2");
        std::fs::create_dir_all(&temp_dir1).unwrap();
        std::fs::create_dir_all(&temp_dir2).unwrap();

        executor
            .register_local_server("fs1", FilesystemServer::new(temp_dir1.clone()))
            .await;
        executor
            .register_local_server("fs2", FilesystemServer::new(temp_dir2.clone()))
            .await;

        let all_tools = executor.list_all_local_tools().await;
        assert_eq!(all_tools.len(), 2);
        assert!(all_tools.contains_key("fs1"));
        assert!(all_tools.contains_key("fs2"));

        // Cleanup
        std::fs::remove_dir_all(&temp_dir1).unwrap();
        std::fs::remove_dir_all(&temp_dir2).unwrap();
    }

    #[tokio::test]
    async fn test_local_server_priority() {
        // Local servers should take priority over HTTP fallback
        let executor = McpExecutor::new();
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-priority");
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Register a local server with an ID that could be confused with a URL
        let fs_server = FilesystemServer::new(temp_dir.clone());
        executor
            .register_local_server("api.example.com", fs_server)
            .await;

        // Create a test file
        std::fs::write(temp_dir.join("test.txt"), "Priority test").unwrap();

        // This should use the local server, not HTTP fallback
        let result = executor
            .execute_tool(
                "api.example.com",
                "fs_read",
                serde_json::json!({
                    "path": "test.txt"
                }),
            )
            .await;

        match result {
            ExecutionResult::Success(value) => {
                assert_eq!(value["content"], "Priority test");
            }
            ExecutionResult::Failure(msg) => {
                panic!("Expected success from local server, got failure: {}", msg);
            }
            _ => panic!("Expected success result"),
        }

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).unwrap();
    }
}
