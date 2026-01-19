//! Plugin system for workflow extensibility
//!
//! Allows registration of custom node types and execution logic

use oxify_model::{ExecutionContext, ExecutionResult, Node};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Plugin trait for custom node execution
#[async_trait::async_trait]
pub trait NodePlugin: Send + Sync {
    /// Get the plugin name
    fn name(&self) -> &str;

    /// Get the plugin version
    fn version(&self) -> &str;

    /// Get supported node type identifiers
    fn supported_node_types(&self) -> Vec<String>;

    /// Execute a node
    async fn execute(
        &self,
        node: &Node,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, String>;

    /// Validate a node configuration
    fn validate(&self, node: &Node) -> Result<(), String> {
        let _ = node;
        Ok(())
    }

    /// Get plugin metadata
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: self.name().to_string(),
            version: self.version().to_string(),
            description: None,
            author: None,
            homepage: None,
        }
    }
}

/// Plugin metadata
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
}

/// Plugin registry for managing plugins
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Arc<dyn NodePlugin>>>>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a plugin
    pub async fn register(&self, plugin: Arc<dyn NodePlugin>) -> Result<(), String> {
        let name = plugin.name().to_string();

        // Check for duplicate
        if self.plugins.read().await.contains_key(&name) {
            return Err(format!("Plugin '{}' already registered", name));
        }

        self.plugins.write().await.insert(name.clone(), plugin);

        tracing::info!("Registered plugin: {}", name);
        Ok(())
    }

    /// Unregister a plugin
    pub async fn unregister(&self, name: &str) -> bool {
        let removed = self.plugins.write().await.remove(name).is_some();
        if removed {
            tracing::info!("Unregistered plugin: {}", name);
        }
        removed
    }

    /// Get a plugin by name
    pub async fn get(&self, name: &str) -> Option<Arc<dyn NodePlugin>> {
        self.plugins.read().await.get(name).cloned()
    }

    /// List all registered plugins
    pub async fn list(&self) -> Vec<PluginMetadata> {
        self.plugins
            .read()
            .await
            .values()
            .map(|p| p.metadata())
            .collect()
    }

    /// Find a plugin that can handle a node type
    pub async fn find_for_node_type(&self, node_type: &str) -> Option<Arc<dyn NodePlugin>> {
        let plugins = self.plugins.read().await;

        for plugin in plugins.values() {
            if plugin
                .supported_node_types()
                .contains(&node_type.to_string())
            {
                return Some(Arc::clone(plugin));
            }
        }

        None
    }

    /// Execute a node using the appropriate plugin
    pub async fn execute_node(
        &self,
        node: &Node,
        context: &ExecutionContext,
        node_type: &str,
    ) -> Result<ExecutionResult, String> {
        let plugin = self
            .find_for_node_type(node_type)
            .await
            .ok_or_else(|| format!("No plugin found for node type: {}", node_type))?;

        plugin.execute(node, context).await
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Example: Custom HTTP node plugin
pub struct HttpNodePlugin;

#[async_trait::async_trait]
impl NodePlugin for HttpNodePlugin {
    fn name(&self) -> &str {
        "http-node"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn supported_node_types(&self) -> Vec<String> {
        vec!["custom_http".to_string()]
    }

    async fn execute(
        &self,
        node: &Node,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, String> {
        // Extract configuration from node (would be in node.kind)
        let _ = node;

        // Placeholder implementation
        Ok(ExecutionResult::Success(Value::String(
            "HTTP request executed".to_string(),
        )))
    }

    fn validate(&self, node: &Node) -> Result<(), String> {
        // Validate HTTP configuration
        let _ = node;
        Ok(())
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: self.name().to_string(),
            version: self.version().to_string(),
            description: Some("Custom HTTP request node".to_string()),
            author: Some("OxiFY Contributors".to_string()),
            homepage: None,
        }
    }
}

/// Example: Custom database node plugin
pub struct DatabaseNodePlugin;

#[async_trait::async_trait]
impl NodePlugin for DatabaseNodePlugin {
    fn name(&self) -> &str {
        "database-node"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn supported_node_types(&self) -> Vec<String> {
        vec!["custom_database".to_string(), "sql_query".to_string()]
    }

    async fn execute(
        &self,
        node: &Node,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, String> {
        let _ = node;

        // Placeholder implementation
        Ok(ExecutionResult::Success(Value::Array(vec![])))
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: self.name().to_string(),
            version: self.version().to_string(),
            description: Some("Database query execution node".to_string()),
            author: Some("OxiFY Contributors".to_string()),
            homepage: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{NodeKind, WorkflowId};

    #[tokio::test]
    async fn test_plugin_registration() {
        let registry = PluginRegistry::new();

        let plugin = Arc::new(HttpNodePlugin) as Arc<dyn NodePlugin>;
        registry.register(plugin).await.unwrap();

        let plugins = registry.list().await;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "http-node");
    }

    #[tokio::test]
    async fn test_plugin_unregistration() {
        let registry = PluginRegistry::new();

        let plugin = Arc::new(HttpNodePlugin) as Arc<dyn NodePlugin>;
        registry.register(plugin).await.unwrap();

        let removed = registry.unregister("http-node").await;
        assert!(removed);

        let plugins = registry.list().await;
        assert_eq!(plugins.len(), 0);
    }

    #[tokio::test]
    async fn test_find_plugin_for_node_type() {
        let registry = PluginRegistry::new();

        let plugin = Arc::new(DatabaseNodePlugin) as Arc<dyn NodePlugin>;
        registry.register(plugin).await.unwrap();

        let found = registry.find_for_node_type("sql_query").await;
        assert!(found.is_some());

        let not_found = registry.find_for_node_type("nonexistent").await;
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_plugin_execution() {
        let registry = PluginRegistry::new();

        let plugin = Arc::new(HttpNodePlugin) as Arc<dyn NodePlugin>;
        registry.register(plugin).await.unwrap();

        let node = Node::new("Test".to_string(), NodeKind::Start);
        let context = ExecutionContext::new(WorkflowId::new_v4());

        let result = registry
            .execute_node(&node, &context, "custom_http")
            .await
            .unwrap();

        match result {
            ExecutionResult::Success(_) => {}
            _ => panic!("Expected success result"),
        }
    }
}
