//! Server registry for managing multiple MCP servers
//!
//! This module provides a registry for managing multiple MCP servers with
//! load balancing, health checking, and failover capabilities.

use crate::{McpError, McpServer, Result, ToolSchema};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Type alias for the server map to reduce complexity
type ServerMap = Arc<RwLock<HashMap<String, ServerEntry>>>;

/// Load balancing strategy for selecting servers
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoadBalanceStrategy {
    /// Simple round-robin selection
    #[default]
    RoundRobin,
    /// Select server with least active connections
    LeastConnections,
    /// Random selection
    Random,
    /// Weighted round-robin based on server weights
    WeightedRoundRobin,
}

/// Health status of a server
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerHealth {
    #[default]
    Healthy,
    Degraded,
    Unhealthy,
}

/// Server entry with metadata for load balancing
#[derive(Clone)]
pub struct ServerEntry {
    /// The actual MCP server
    pub server: Arc<Box<dyn McpServer>>,
    /// Server weight for weighted load balancing
    pub weight: u32,
    /// Current health status
    pub health: ServerHealth,
    /// Number of active connections
    pub active_connections: Arc<AtomicU64>,
    /// Total request count
    pub request_count: Arc<AtomicU64>,
    /// Total error count
    pub error_count: Arc<AtomicU64>,
    /// Average response time in milliseconds
    pub avg_response_time_ms: Arc<AtomicU64>,
    /// Last health check time
    pub last_health_check: Option<Instant>,
    /// Server group for affinity
    pub group: Option<String>,
    /// Tags for filtering
    pub tags: Vec<String>,
}

impl ServerEntry {
    fn new(server: Arc<Box<dyn McpServer>>) -> Self {
        Self {
            server,
            weight: 1,
            health: ServerHealth::Healthy,
            active_connections: Arc::new(AtomicU64::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            avg_response_time_ms: Arc::new(AtomicU64::new(0)),
            last_health_check: None,
            group: None,
            tags: Vec::new(),
        }
    }

    fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    fn with_group(mut self, group: String) -> Self {
        self.group = Some(group);
        self
    }

    fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// Registry for managing multiple MCP servers (both built-in and external)
/// with load balancing and health checking capabilities
pub struct McpRegistry {
    servers: ServerMap,
    /// Round-robin counter
    rr_counter: AtomicUsize,
    /// Default load balancing strategy
    default_strategy: LoadBalanceStrategy,
    /// Health check interval in seconds
    health_check_interval_secs: u64,
    /// Unhealthy threshold (consecutive failures)
    unhealthy_threshold: u32,
    /// Recovery threshold (consecutive successes)
    recovery_threshold: u32,
}

impl McpRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            rr_counter: AtomicUsize::new(0),
            default_strategy: LoadBalanceStrategy::RoundRobin,
            health_check_interval_secs: 30,
            unhealthy_threshold: 3,
            recovery_threshold: 2,
        }
    }

    /// Create a new registry with custom configuration
    pub fn with_config(
        strategy: LoadBalanceStrategy,
        health_check_interval_secs: u64,
        unhealthy_threshold: u32,
        recovery_threshold: u32,
    ) -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            rr_counter: AtomicUsize::new(0),
            default_strategy: strategy,
            health_check_interval_secs,
            unhealthy_threshold,
            recovery_threshold,
        }
    }

    /// Register a new MCP server
    pub async fn register<S: McpServer + 'static>(
        &self,
        server_id: String,
        server: S,
    ) -> Result<()> {
        let mut servers = self.servers.write().await;
        let entry = ServerEntry::new(Arc::new(Box::new(server)));
        servers.insert(server_id.clone(), entry);
        tracing::info!("Registered MCP server: {}", server_id);
        Ok(())
    }

    /// Register a server with custom weight
    pub async fn register_with_weight<S: McpServer + 'static>(
        &self,
        server_id: String,
        server: S,
        weight: u32,
    ) -> Result<()> {
        let mut servers = self.servers.write().await;
        let entry = ServerEntry::new(Arc::new(Box::new(server))).with_weight(weight);
        servers.insert(server_id.clone(), entry);
        tracing::info!(
            "Registered MCP server: {} with weight {}",
            server_id,
            weight
        );
        Ok(())
    }

    /// Register a server with group affinity
    pub async fn register_with_group<S: McpServer + 'static>(
        &self,
        server_id: String,
        server: S,
        group: String,
    ) -> Result<()> {
        let mut servers = self.servers.write().await;
        let entry = ServerEntry::new(Arc::new(Box::new(server))).with_group(group.clone());
        servers.insert(server_id.clone(), entry);
        tracing::info!("Registered MCP server: {} in group {}", server_id, group);
        Ok(())
    }

    /// Register a server with tags
    pub async fn register_with_tags<S: McpServer + 'static>(
        &self,
        server_id: String,
        server: S,
        tags: Vec<String>,
    ) -> Result<()> {
        let mut servers = self.servers.write().await;
        let entry = ServerEntry::new(Arc::new(Box::new(server))).with_tags(tags.clone());
        servers.insert(server_id.clone(), entry);
        tracing::info!("Registered MCP server: {} with tags {:?}", server_id, tags);
        Ok(())
    }

    /// Unregister a server by ID
    pub async fn unregister(&self, server_id: &str) -> Result<()> {
        let mut servers = self.servers.write().await;
        servers
            .remove(server_id)
            .ok_or_else(|| McpError::ServerError(format!("Server '{}' not found", server_id)))?;
        tracing::info!("Unregistered MCP server: {}", server_id);
        Ok(())
    }

    /// Get a server by ID
    pub async fn get_server(&self, server_id: &str) -> Option<Arc<Box<dyn McpServer>>> {
        let servers = self.servers.read().await;
        servers.get(server_id).map(|entry| entry.server.clone())
    }

    /// Get a server entry by ID (includes metadata)
    pub async fn get_server_entry(&self, server_id: &str) -> Option<ServerEntry> {
        let servers = self.servers.read().await;
        servers.get(server_id).cloned()
    }

    /// Set server health status
    pub async fn set_server_health(&self, server_id: &str, health: ServerHealth) -> Result<()> {
        let mut servers = self.servers.write().await;
        let entry = servers
            .get_mut(server_id)
            .ok_or_else(|| McpError::ServerError(format!("Server '{}' not found", server_id)))?;
        entry.health = health;
        entry.last_health_check = Some(Instant::now());
        tracing::info!("Updated server {} health to {:?}", server_id, health);
        Ok(())
    }

    /// Set server weight
    pub async fn set_server_weight(&self, server_id: &str, weight: u32) -> Result<()> {
        let mut servers = self.servers.write().await;
        let entry = servers
            .get_mut(server_id)
            .ok_or_else(|| McpError::ServerError(format!("Server '{}' not found", server_id)))?;
        entry.weight = weight;
        tracing::info!("Updated server {} weight to {}", server_id, weight);
        Ok(())
    }

    /// Select a server using the specified load balancing strategy
    pub async fn select_server(&self, strategy: Option<LoadBalanceStrategy>) -> Option<String> {
        let strategy = strategy.unwrap_or(self.default_strategy);
        let servers = self.servers.read().await;

        // Filter healthy servers
        let healthy_servers: Vec<(&String, &ServerEntry)> = servers
            .iter()
            .filter(|(_, e)| e.health == ServerHealth::Healthy)
            .collect();

        if healthy_servers.is_empty() {
            return None;
        }

        match strategy {
            LoadBalanceStrategy::RoundRobin => {
                let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) % healthy_servers.len();
                healthy_servers.get(idx).map(|(id, _)| (*id).clone())
            }
            LoadBalanceStrategy::LeastConnections => healthy_servers
                .iter()
                .min_by_key(|(_, e)| e.active_connections.load(Ordering::Relaxed))
                .map(|(id, _)| (*id).clone()),
            LoadBalanceStrategy::Random => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                std::time::Instant::now().hash(&mut hasher);
                let hash = hasher.finish() as usize;
                let idx = hash % healthy_servers.len();
                healthy_servers.get(idx).map(|(id, _)| (*id).clone())
            }
            LoadBalanceStrategy::WeightedRoundRobin => {
                let total_weight: u32 = healthy_servers.iter().map(|(_, e)| e.weight).sum();
                if total_weight == 0 {
                    return healthy_servers.first().map(|(id, _)| (*id).clone());
                }
                let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed);
                let mut position = (idx as u32) % total_weight;

                for (id, entry) in &healthy_servers {
                    if position < entry.weight {
                        return Some((*id).clone());
                    }
                    position -= entry.weight;
                }
                healthy_servers.first().map(|(id, _)| (*id).clone())
            }
        }
    }

    /// Select a server from a specific group
    pub async fn select_server_from_group(&self, group: &str) -> Option<String> {
        let servers = self.servers.read().await;

        let group_servers: Vec<(&String, &ServerEntry)> = servers
            .iter()
            .filter(|(_, e)| e.health == ServerHealth::Healthy && e.group.as_deref() == Some(group))
            .collect();

        if group_servers.is_empty() {
            return None;
        }

        let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) % group_servers.len();
        group_servers.get(idx).map(|(id, _)| (*id).clone())
    }

    /// Select a server by tag
    pub async fn select_server_by_tag(&self, tag: &str) -> Option<String> {
        let servers = self.servers.read().await;

        let tagged_servers: Vec<(&String, &ServerEntry)> = servers
            .iter()
            .filter(|(_, e)| e.health == ServerHealth::Healthy && e.tags.contains(&tag.to_string()))
            .collect();

        if tagged_servers.is_empty() {
            return None;
        }

        let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) % tagged_servers.len();
        tagged_servers.get(idx).map(|(id, _)| (*id).clone())
    }

    /// Invoke a tool with automatic failover
    pub async fn invoke_tool_with_failover(
        &self,
        tool_name: &str,
        arguments: Value,
        max_retries: u32,
    ) -> Result<Value> {
        // Find servers that have this tool
        let server_ids = self.find_tool(tool_name).await?;
        if server_ids.is_empty() {
            return Err(McpError::ToolNotFound(format!(
                "Tool '{}' not found in any server",
                tool_name
            )));
        }

        let mut last_error = None;
        let mut tried_servers: Vec<String> = Vec::new();

        for _ in 0..max_retries.min(server_ids.len() as u32) {
            // Select a healthy server that hasn't been tried
            let servers = self.servers.read().await;
            let available_server = server_ids
                .iter()
                .filter(|id| !tried_servers.contains(id))
                .find(|id| {
                    servers
                        .get(*id)
                        .map(|e| e.health == ServerHealth::Healthy)
                        .unwrap_or(false)
                })
                .cloned();
            drop(servers);

            let server_id = match available_server {
                Some(id) => id,
                None => break, // No more healthy servers to try
            };

            tried_servers.push(server_id.clone());

            // Increment active connections
            {
                let servers = self.servers.read().await;
                if let Some(entry) = servers.get(&server_id) {
                    entry.active_connections.fetch_add(1, Ordering::Relaxed);
                    entry.request_count.fetch_add(1, Ordering::Relaxed);
                }
            }

            let start_time = Instant::now();
            let result = self
                .invoke_tool(&server_id, tool_name, arguments.clone())
                .await;

            // Decrement active connections and update metrics
            {
                let servers = self.servers.read().await;
                if let Some(entry) = servers.get(&server_id) {
                    entry.active_connections.fetch_sub(1, Ordering::Relaxed);
                    let elapsed_ms = start_time.elapsed().as_millis() as u64;
                    // Simple moving average
                    let old_avg = entry.avg_response_time_ms.load(Ordering::Relaxed);
                    let new_avg = (old_avg + elapsed_ms) / 2;
                    entry.avg_response_time_ms.store(new_avg, Ordering::Relaxed);
                }
            }

            match result {
                Ok(value) => return Ok(value),
                Err(e) => {
                    tracing::warn!(
                        "Tool invocation failed on server {}: {}. Trying failover...",
                        server_id,
                        e
                    );

                    // Update error count
                    {
                        let servers = self.servers.read().await;
                        if let Some(entry) = servers.get(&server_id) {
                            entry.error_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }

                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            McpError::ServerError("All servers failed or unavailable".to_string())
        }))
    }

    /// Get all servers with their health status
    pub async fn get_server_health_status(&self) -> HashMap<String, ServerHealth> {
        let servers = self.servers.read().await;
        servers
            .iter()
            .map(|(id, entry)| (id.clone(), entry.health))
            .collect()
    }

    /// Get server metrics
    pub async fn get_server_metrics(&self, server_id: &str) -> Option<ServerMetrics> {
        let servers = self.servers.read().await;
        servers.get(server_id).map(|entry| ServerMetrics {
            server_id: server_id.to_string(),
            health: entry.health,
            weight: entry.weight,
            active_connections: entry.active_connections.load(Ordering::Relaxed),
            request_count: entry.request_count.load(Ordering::Relaxed),
            error_count: entry.error_count.load(Ordering::Relaxed),
            avg_response_time_ms: entry.avg_response_time_ms.load(Ordering::Relaxed),
            group: entry.group.clone(),
            tags: entry.tags.clone(),
        })
    }

    /// Get metrics for all servers
    pub async fn get_all_server_metrics(&self) -> Vec<ServerMetrics> {
        let servers = self.servers.read().await;
        servers
            .iter()
            .map(|(id, entry)| ServerMetrics {
                server_id: id.clone(),
                health: entry.health,
                weight: entry.weight,
                active_connections: entry.active_connections.load(Ordering::Relaxed),
                request_count: entry.request_count.load(Ordering::Relaxed),
                error_count: entry.error_count.load(Ordering::Relaxed),
                avg_response_time_ms: entry.avg_response_time_ms.load(Ordering::Relaxed),
                group: entry.group.clone(),
                tags: entry.tags.clone(),
            })
            .collect()
    }

    /// List servers in a specific group
    pub async fn list_servers_in_group(&self, group: &str) -> Vec<String> {
        let servers = self.servers.read().await;
        servers
            .iter()
            .filter(|(_, e)| e.group.as_deref() == Some(group))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// List servers with a specific tag
    pub async fn list_servers_with_tag(&self, tag: &str) -> Vec<String> {
        let servers = self.servers.read().await;
        servers
            .iter()
            .filter(|(_, e)| e.tags.contains(&tag.to_string()))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get load balancing configuration
    pub fn get_config(&self) -> LoadBalanceConfig {
        LoadBalanceConfig {
            default_strategy: self.default_strategy,
            health_check_interval_secs: self.health_check_interval_secs,
            unhealthy_threshold: self.unhealthy_threshold,
            recovery_threshold: self.recovery_threshold,
        }
    }

    /// List all registered server IDs
    pub async fn list_server_ids(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }

    /// Get all tools from all registered servers
    pub async fn list_all_tools(&self) -> Result<HashMap<String, Vec<ToolSchema>>> {
        let servers = self.servers.read().await;
        let mut all_tools = HashMap::new();

        for (server_id, entry) in servers.iter() {
            let tools_json = entry.server.list_tools().await?;
            let tools: Vec<ToolSchema> = tools_json
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect();
            all_tools.insert(server_id.clone(), tools);
        }

        Ok(all_tools)
    }

    /// Get tools from a specific server
    pub async fn list_tools(&self, server_id: &str) -> Result<Vec<ToolSchema>> {
        let server = self
            .get_server(server_id)
            .await
            .ok_or_else(|| McpError::ServerError(format!("Server '{}' not found", server_id)))?;

        let tools_json = server.list_tools().await?;
        let tools: Vec<ToolSchema> = tools_json
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        Ok(tools)
    }

    /// Invoke a tool on a specific server
    pub async fn invoke_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value> {
        let server = self
            .get_server(server_id)
            .await
            .ok_or_else(|| McpError::ServerError(format!("Server '{}' not found", server_id)))?;

        server.call_tool(tool_name, arguments).await
    }

    /// Find a tool across all servers
    pub async fn find_tool(&self, tool_name: &str) -> Result<Vec<String>> {
        let all_tools = self.list_all_tools().await?;
        let mut server_ids = Vec::new();

        for (server_id, tools) in all_tools {
            if tools.iter().any(|t| t.name == tool_name) {
                server_ids.push(server_id);
            }
        }

        Ok(server_ids)
    }

    /// Get server statistics
    pub async fn get_stats(&self) -> RegistryStats {
        let servers = self.servers.read().await;
        let server_count = servers.len();

        let mut total_tools = 0;
        for entry in servers.values() {
            if let Ok(tools) = entry.server.list_tools().await {
                total_tools += tools.len();
            }
        }

        RegistryStats {
            server_count,
            total_tools,
        }
    }

    /// Clear all registered servers
    pub async fn clear(&self) {
        let mut servers = self.servers.write().await;
        servers.clear();
        tracing::info!("Cleared all registered MCP servers");
    }
}

impl Default for McpRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the registry
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub server_count: usize,
    pub total_tools: usize,
}

/// Server metrics for monitoring and load balancing decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMetrics {
    pub server_id: String,
    pub health: ServerHealth,
    pub weight: u32,
    pub active_connections: u64,
    pub request_count: u64,
    pub error_count: u64,
    pub avg_response_time_ms: u64,
    pub group: Option<String>,
    pub tags: Vec<String>,
}

impl ServerMetrics {
    /// Calculate error rate (0.0 - 1.0)
    pub fn error_rate(&self) -> f64 {
        if self.request_count == 0 {
            return 0.0;
        }
        self.error_count as f64 / self.request_count as f64
    }

    /// Check if server should be marked unhealthy based on error rate
    pub fn should_mark_unhealthy(&self, error_rate_threshold: f64) -> bool {
        self.error_rate() > error_rate_threshold && self.request_count > 10
    }
}

/// Load balancing configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LoadBalanceConfig {
    pub default_strategy: LoadBalanceStrategy,
    pub health_check_interval_secs: u64,
    pub unhealthy_threshold: u32,
    pub recovery_threshold: u32,
}

impl Default for LoadBalanceConfig {
    fn default() -> Self {
        Self {
            default_strategy: LoadBalanceStrategy::RoundRobin,
            health_check_interval_secs: 30,
            unhealthy_threshold: 3,
            recovery_threshold: 2,
        }
    }
}

#[async_trait]
impl McpServer for McpRegistry {
    /// Call a tool on a registered server
    /// Arguments must include "server_id" field
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let server_id = arguments
            .get("server_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("Missing 'server_id' field".to_string()))?
            .to_string();

        self.invoke_tool(&server_id, name, arguments).await
    }

    /// List all tools from all registered servers
    async fn list_tools(&self) -> Result<Vec<Value>> {
        let all_tools = self.list_all_tools().await?;
        let mut tools = Vec::new();

        for (server_id, server_tools) in all_tools {
            for tool in server_tools {
                tools.push(serde_json::json!({
                    "server_id": server_id,
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                }));
            }
        }

        Ok(tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::FilesystemServer;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = McpRegistry::new();
        let server_ids = registry.list_server_ids().await;
        assert_eq!(server_ids.len(), 0);
    }

    #[tokio::test]
    async fn test_register_server() {
        let registry = McpRegistry::new();
        let fs_server = FilesystemServer::new(PathBuf::from("/tmp"));

        registry
            .register("fs".to_string(), fs_server)
            .await
            .unwrap();

        let server_ids = registry.list_server_ids().await;
        assert_eq!(server_ids.len(), 1);
        assert!(server_ids.contains(&"fs".to_string()));
    }

    #[tokio::test]
    async fn test_unregister_server() {
        let registry = McpRegistry::new();
        let fs_server = FilesystemServer::new(PathBuf::from("/tmp"));

        registry
            .register("fs".to_string(), fs_server)
            .await
            .unwrap();

        registry.unregister("fs").await.unwrap();

        let server_ids = registry.list_server_ids().await;
        assert_eq!(server_ids.len(), 0);
    }

    #[tokio::test]
    async fn test_list_tools() {
        let registry = McpRegistry::new();
        let fs_server = FilesystemServer::new(PathBuf::from("/tmp"));

        registry
            .register("fs".to_string(), fs_server)
            .await
            .unwrap();

        let tools = registry.list_tools("fs").await.unwrap();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t.name == "fs_read"));
    }

    #[tokio::test]
    async fn test_find_tool() {
        let registry = McpRegistry::new();
        let fs_server = FilesystemServer::new(PathBuf::from("/tmp"));

        registry
            .register("fs".to_string(), fs_server)
            .await
            .unwrap();

        let servers = registry.find_tool("fs_read").await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0], "fs");
    }

    #[tokio::test]
    async fn test_get_stats() {
        let registry = McpRegistry::new();
        let fs_server = FilesystemServer::new(PathBuf::from("/tmp"));

        registry
            .register("fs".to_string(), fs_server)
            .await
            .unwrap();

        let stats = registry.get_stats().await;
        assert_eq!(stats.server_count, 1);
        assert!(stats.total_tools > 0);
    }

    #[tokio::test]
    async fn test_clear() {
        let registry = McpRegistry::new();
        let fs_server = FilesystemServer::new(PathBuf::from("/tmp"));

        registry
            .register("fs".to_string(), fs_server)
            .await
            .unwrap();

        registry.clear().await;

        let server_ids = registry.list_server_ids().await;
        assert_eq!(server_ids.len(), 0);
    }
}
