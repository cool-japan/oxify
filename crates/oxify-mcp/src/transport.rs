//! MCP transport implementations (stdio, HTTP)

use crate::{McpError, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// Transport layer for MCP communication
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC request and receive a response
    async fn send_request(&mut self, request: Value) -> Result<Value>;

    /// Close the transport
    async fn close(&mut self) -> Result<()>;
}

/// Stdio transport - launches MCP server as subprocess
pub struct StdioTransport {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    request_id: u64,
    /// Maximum response line size in bytes (default: 10MB)
    max_response_size: usize,
}

impl StdioTransport {
    /// Default maximum response size (10MB)
    const DEFAULT_MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

    /// Launch an MCP server via stdio
    pub async fn new(command: &str, args: &[&str]) -> Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| McpError::ServerError(format!("Failed to spawn MCP server: {}", e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::ServerError("Failed to get stdin".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::ServerError("Failed to get stdout".to_string()))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            request_id: 1,
            max_response_size: Self::DEFAULT_MAX_RESPONSE_SIZE,
        })
    }

    /// Set maximum response size
    pub fn with_max_response_size(mut self, size: usize) -> Self {
        self.max_response_size = size;
        self
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send_request(&mut self, mut request: Value) -> Result<Value> {
        // Add JSON-RPC fields
        if let Value::Object(ref mut obj) = request {
            obj.insert("jsonrpc".to_string(), Value::String("2.0".to_string()));
            obj.insert("id".to_string(), Value::Number(self.request_id.into()));
            self.request_id += 1;
        }

        // Send request
        let request_str = serde_json::to_string(&request)
            .map_err(|e| McpError::ProtocolError(format!("Failed to serialize request: {}", e)))?;

        self.stdin
            .write_all(request_str.as_bytes())
            .await
            .map_err(|e| McpError::ServerError(format!("Failed to write request: {}", e)))?;

        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|e| McpError::ServerError(format!("Failed to write newline: {}", e)))?;

        self.stdin
            .flush()
            .await
            .map_err(|e| McpError::ServerError(format!("Failed to flush: {}", e)))?;

        // Read response with size limit
        let mut response_line = String::new();
        let bytes_read = self
            .stdout
            .read_line(&mut response_line)
            .await
            .map_err(|e| McpError::ServerError(format!("Failed to read response: {}", e)))?;

        if bytes_read > self.max_response_size {
            return Err(McpError::ProtocolError(format!(
                "Response too large: {} bytes (max: {})",
                bytes_read, self.max_response_size
            )));
        }

        let response: Value = serde_json::from_str(&response_line)
            .map_err(|e| McpError::ProtocolError(format!("Failed to parse response: {}", e)))?;

        Ok(response)
    }

    async fn close(&mut self) -> Result<()> {
        self.child
            .kill()
            .await
            .map_err(|e| McpError::ServerError(format!("Failed to kill child process: {}", e)))
    }
}

/// HTTP transport for remote MCP servers
pub struct HttpTransport {
    client: reqwest::Client,
    base_url: String,
    request_id: u64,
    /// Maximum response size in bytes (default: 10MB)
    max_response_size: usize,
}

impl HttpTransport {
    /// Default maximum response size (10MB)
    const DEFAULT_MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

    /// Create a new HTTP transport
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            base_url,
            request_id: 1,
            max_response_size: Self::DEFAULT_MAX_RESPONSE_SIZE,
        }
    }

    /// Set maximum response size
    pub fn with_max_response_size(mut self, size: usize) -> Self {
        self.max_response_size = size;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        self
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send_request(&mut self, mut request: Value) -> Result<Value> {
        // Add JSON-RPC fields
        if let Value::Object(ref mut obj) = request {
            obj.insert("jsonrpc".to_string(), Value::String("2.0".to_string()));
            obj.insert("id".to_string(), Value::Number(self.request_id.into()));
            self.request_id += 1;
        }

        let response = self
            .client
            .post(&self.base_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| McpError::ServerError(format!("HTTP request failed: {}", e)))?;

        let response_json: Value = response
            .json()
            .await
            .map_err(|e| McpError::ProtocolError(format!("Failed to parse response: {}", e)))?;

        Ok(response_json)
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_transport_constants() {
        assert_eq!(StdioTransport::DEFAULT_MAX_RESPONSE_SIZE, 10 * 1024 * 1024);
    }

    #[test]
    fn test_http_transport_constants() {
        assert_eq!(HttpTransport::DEFAULT_MAX_RESPONSE_SIZE, 10 * 1024 * 1024);
    }

    #[test]
    fn test_http_transport_creation() {
        let transport = HttpTransport::new("http://localhost:3000".to_string());
        assert_eq!(transport.base_url, "http://localhost:3000");
        assert_eq!(transport.request_id, 1);
        assert_eq!(
            transport.max_response_size,
            HttpTransport::DEFAULT_MAX_RESPONSE_SIZE
        );
    }

    #[test]
    fn test_http_transport_with_max_response_size() {
        let transport =
            HttpTransport::new("http://localhost:3000".to_string()).with_max_response_size(1024);
        assert_eq!(transport.max_response_size, 1024);
    }

    #[test]
    fn test_http_transport_with_timeout() {
        let transport = HttpTransport::new("http://localhost:3000".to_string())
            .with_timeout(std::time::Duration::from_secs(5));
        // Just verify it doesn't panic
        assert_eq!(transport.base_url, "http://localhost:3000");
    }

    // Note: Integration tests for actual transport operations would require
    // a running MCP server, so they are omitted from unit tests
}
