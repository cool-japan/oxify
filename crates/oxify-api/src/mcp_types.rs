//! MCP API types for request/response handling

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterMcpServerRequest {
    pub server_id: String,
    pub server_type: String,
    pub endpoint: Option<String>,
    pub command: Option<Vec<String>>,
    pub weight: Option<u32>,
    pub tags: Option<Vec<String>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterMcpServerResponse {
    pub server_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnregisterMcpServerRequest {
    pub server_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnregisterMcpServerResponse {
    pub server_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMcpServersResponse {
    pub servers: Vec<McpServerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub server_id: String,
    pub transport_type: String,
    pub health: String,
    pub weight: u32,
    pub tags: Vec<String>,
    pub metrics: ServerMetricsInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMetricsInfo {
    pub request_count: u64,
    pub error_count: u64,
    pub avg_response_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMcpToolsRequest {
    pub server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMcpToolsResponse {
    pub tools: Vec<McpToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub server_id: String,
    pub tool_name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeMcpToolRequest {
    pub server_id: String,
    pub tool_name: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeMcpToolResponse {
    pub execution_id: Uuid,
    pub result: serde_json::Value,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistryStatsResponse {
    pub total_servers: usize,
    pub total_tools: usize,
    pub servers_by_health: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}
