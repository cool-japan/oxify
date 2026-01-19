//! Plugin Loader - Unified plugin loading and lifecycle management
//!
//! Integrates PluginManager, PluginRegistry, PluginSecurityScanner, and WasmPluginLoader
//! to provide a complete plugin loading and execution system.
//!
//! # Features
//!
//! - Automatic plugin discovery from directories
//! - Security scanning before loading
//! - WASM plugin support with sandboxing
//! - Plugin lifecycle management (load, enable, disable, unload)
//! - Dependency resolution and validation

use crate::plugin::PluginRegistry;
use crate::plugin_manifest::{PluginManager, PluginManifest};
use crate::plugin_security::{PluginSecurityScanner, SecurityPolicy, SecurityScanResult};
use crate::plugin_wasm::{WasmPluginConfig, WasmPluginLoader};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

/// Plugin loader errors
#[derive(Error, Debug)]
pub enum PluginLoaderError {
    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Security check failed: {0}")]
    SecurityCheckFailed(String),

    #[error("Failed to load plugin: {0}")]
    LoadFailed(String),

    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),

    #[error("Dependency error: {0}")]
    DependencyError(String),

    #[error("Invalid plugin state: {0}")]
    InvalidState(String),

    #[error("IO error: {0}")]
    IoError(String),
}

/// Plugin loader configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLoaderConfig {
    /// Security policy for plugin loading
    pub security_policy: SecurityPolicy,
    /// WASM configuration
    pub wasm_config: WasmPluginConfig,
    /// Plugin search paths
    pub search_paths: Vec<PathBuf>,
    /// Enable hot-reload
    pub enable_hot_reload: bool,
    /// Hot-reload check interval
    pub hot_reload_interval: Duration,
    /// Auto-discover plugins on startup
    pub auto_discover: bool,
    /// Auto-load discovered plugins
    pub auto_load: bool,
}

impl Default for PluginLoaderConfig {
    fn default() -> Self {
        Self {
            security_policy: SecurityPolicy::default(),
            wasm_config: WasmPluginConfig::default(),
            search_paths: vec![],
            enable_hot_reload: false,
            hot_reload_interval: Duration::from_secs(5),
            auto_discover: true,
            auto_load: false,
        }
    }
}

impl PluginLoaderConfig {
    /// Create a strict configuration
    pub fn strict() -> Self {
        Self {
            security_policy: SecurityPolicy::strict(),
            wasm_config: WasmPluginConfig::strict(),
            search_paths: vec![],
            enable_hot_reload: false,
            hot_reload_interval: Duration::from_secs(5),
            auto_discover: true,
            auto_load: false,
        }
    }

    /// Add a search path
    pub fn with_search_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.search_paths.push(path.into());
        self
    }

    /// Enable hot-reload
    pub fn with_hot_reload(mut self, interval: Duration) -> Self {
        self.enable_hot_reload = true;
        self.hot_reload_interval = interval;
        self
    }
}

/// Plugin load result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLoadResult {
    /// Plugin name
    pub name: String,
    /// Load success
    pub success: bool,
    /// Security scan result
    pub security_scan: Option<SecurityScanResult>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Load timestamp
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

/// Comprehensive plugin loader
pub struct PluginLoader {
    /// Configuration
    config: PluginLoaderConfig,
    /// Plugin manager (manifest-based)
    plugin_manager: Arc<RwLock<PluginManager>>,
    /// Plugin registry (execution-based)
    plugin_registry: Arc<PluginRegistry>,
    /// Security scanner
    security_scanner: PluginSecurityScanner,
    /// WASM loader (for future WASM plugin execution)
    #[allow(dead_code)]
    wasm_loader: Arc<RwLock<Option<WasmPluginLoader>>>,
    /// Load results cache
    load_results: Arc<RwLock<HashMap<String, PluginLoadResult>>>,
}

impl PluginLoader {
    /// Create a new plugin loader
    pub fn new(config: PluginLoaderConfig) -> Self {
        let mut plugin_manager = PluginManager::new();

        // Configure plugin manager
        for path in &config.search_paths {
            plugin_manager.add_search_path(path.clone());
        }

        if config.enable_hot_reload {
            plugin_manager.enable_hot_reload(config.hot_reload_interval);
        }

        // Create security scanner based on config
        let security_scanner = if config.security_policy.min_security_score >= 90 {
            PluginSecurityScanner::strict()
        } else {
            PluginSecurityScanner::new()
        };

        // Create WASM loader if needed
        let wasm_loader = if config.wasm_config.max_memory_pages > 0 {
            Some(WasmPluginLoader::new(config.wasm_config.clone()))
        } else {
            None
        };

        Self {
            config,
            plugin_manager: Arc::new(RwLock::new(plugin_manager)),
            plugin_registry: Arc::new(PluginRegistry::new()),
            security_scanner,
            wasm_loader: Arc::new(RwLock::new(wasm_loader)),
            load_results: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Discover plugins in search paths
    pub async fn discover(&self) -> Result<Vec<PluginManifest>, PluginLoaderError> {
        let manager = self.plugin_manager.read().await;
        manager
            .discover()
            .await
            .map_err(|e| PluginLoaderError::LoadFailed(e.to_string()))
    }

    /// Load a plugin from manifest
    pub async fn load_plugin(
        &self,
        manifest: PluginManifest,
        plugin_path: PathBuf,
    ) -> Result<PluginLoadResult, PluginLoaderError> {
        let plugin_name = manifest.plugin.name.clone();

        // Check if already loaded
        if self.load_results.read().await.contains_key(&plugin_name) {
            return Err(PluginLoaderError::AlreadyLoaded(plugin_name));
        }

        // Security scan
        let security_scan = self
            .security_scanner
            .scan_manifest(&manifest)
            .map_err(|e| PluginLoaderError::SecurityCheckFailed(e.to_string()))?;

        // Check security policy
        if let Err(e) = self.config.security_policy.check(&security_scan) {
            let result = PluginLoadResult {
                name: plugin_name.clone(),
                success: false,
                security_scan: Some(security_scan),
                error: Some(format!("Security check failed: {}", e)),
                loaded_at: chrono::Utc::now(),
            };
            self.load_results
                .write()
                .await
                .insert(plugin_name.clone(), result.clone());
            return Ok(result);
        }

        // Load plugin into manager
        let manager = self.plugin_manager.write().await;
        manager
            .load(manifest.clone(), plugin_path)
            .await
            .map_err(|e| PluginLoaderError::LoadFailed(e.to_string()))?;

        // Create load result
        let result = PluginLoadResult {
            name: plugin_name.clone(),
            success: true,
            security_scan: Some(security_scan),
            error: None,
            loaded_at: chrono::Utc::now(),
        };

        self.load_results
            .write()
            .await
            .insert(plugin_name, result.clone());

        Ok(result)
    }

    /// Load a plugin by name from discovered plugins
    pub async fn load_plugin_by_name(
        &self,
        name: &str,
    ) -> Result<PluginLoadResult, PluginLoaderError> {
        // Discover plugins
        let manifests = self.discover().await?;

        // Find the plugin
        let manifest = manifests
            .into_iter()
            .find(|m| m.plugin.name == name)
            .ok_or_else(|| PluginLoaderError::NotFound(name.to_string()))?;

        // Construct plugin path (simplified - in production, track this properly)
        let plugin_path = self
            .config
            .search_paths
            .first()
            .ok_or_else(|| PluginLoaderError::LoadFailed("No search paths configured".to_string()))?
            .join(name);

        self.load_plugin(manifest, plugin_path).await
    }

    /// Load all discovered plugins
    pub async fn load_all(&self) -> Result<Vec<PluginLoadResult>, PluginLoaderError> {
        let manifests = self.discover().await?;
        let mut results = Vec::new();

        for manifest in manifests {
            let plugin_name = manifest.plugin.name.clone();
            let plugin_path = self
                .config
                .search_paths
                .first()
                .ok_or_else(|| {
                    PluginLoaderError::LoadFailed("No search paths configured".to_string())
                })?
                .join(&plugin_name);

            match self.load_plugin(manifest, plugin_path).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    tracing::warn!("Failed to load plugin {}: {}", plugin_name, e);
                    results.push(PluginLoadResult {
                        name: plugin_name,
                        success: false,
                        security_scan: None,
                        error: Some(e.to_string()),
                        loaded_at: chrono::Utc::now(),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Unload a plugin
    pub async fn unload_plugin(&self, name: &str) -> Result<(), PluginLoaderError> {
        // Unload from manager
        let manager = self.plugin_manager.read().await;
        manager.unload(name).await;

        // Unload from registry
        self.plugin_registry.unregister(name).await;

        // Remove from load results
        self.load_results.write().await.remove(name);

        Ok(())
    }

    /// Get loaded plugin manifests
    pub async fn list_loaded(&self) -> Vec<PluginManifest> {
        let manager = self.plugin_manager.read().await;
        manager.list().await
    }

    /// Get load results
    pub async fn get_load_results(&self) -> Vec<PluginLoadResult> {
        self.load_results.read().await.values().cloned().collect()
    }

    /// Start hot-reload (if enabled)
    pub async fn start_hot_reload(&self) -> Result<(), PluginLoaderError> {
        if !self.config.enable_hot_reload {
            return Ok(());
        }

        let manager = self.plugin_manager.read().await;
        manager
            .start_hot_reload()
            .await
            .map_err(|e| PluginLoaderError::LoadFailed(e.to_string()))
    }

    /// Stop hot-reload
    pub async fn stop_hot_reload(&self) {
        let manager = self.plugin_manager.read().await;
        manager.stop_hot_reload().await;
    }

    /// Get plugin statistics
    pub async fn stats(&self) -> PluginLoaderStats {
        let manager = self.plugin_manager.read().await;
        let plugin_stats = manager.stats().await;

        let load_results = self.load_results.read().await;
        let successful_loads = load_results.values().filter(|r| r.success).count();
        let failed_loads = load_results.values().filter(|r| !r.success).count();

        PluginLoaderStats {
            total_discovered: plugin_stats.total_plugins,
            successful_loads,
            failed_loads,
            enabled_plugins: plugin_stats.ready_plugins,
            disabled_plugins: plugin_stats.disabled_plugins,
            hot_reload_active: self.config.enable_hot_reload,
        }
    }
}

/// Plugin loader statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLoaderStats {
    /// Total plugins discovered
    pub total_discovered: usize,
    /// Successfully loaded plugins
    pub successful_loads: usize,
    /// Failed loads
    pub failed_loads: usize,
    /// Enabled plugins
    pub enabled_plugins: usize,
    /// Disabled plugins
    pub disabled_plugins: usize,
    /// Hot-reload active
    pub hot_reload_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_loader_config_default() {
        let config = PluginLoaderConfig::default();
        assert!(config.auto_discover);
        assert!(!config.auto_load);
        assert!(!config.enable_hot_reload);
    }

    #[test]
    fn test_plugin_loader_config_strict() {
        let config = PluginLoaderConfig::strict();
        assert_eq!(config.security_policy.min_security_score, 90);
        assert_eq!(config.wasm_config.max_memory_pages, 64);
    }

    #[test]
    fn test_plugin_loader_config_builder() {
        let config = PluginLoaderConfig::default()
            .with_search_path("/plugins")
            .with_hot_reload(Duration::from_secs(10));

        assert_eq!(config.search_paths.len(), 1);
        assert!(config.enable_hot_reload);
        assert_eq!(config.hot_reload_interval, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_plugin_loader_creation() {
        let config = PluginLoaderConfig::default();
        let loader = PluginLoader::new(config);

        let stats = loader.stats().await;
        assert_eq!(stats.total_discovered, 0);
        assert_eq!(stats.successful_loads, 0);
    }

    #[tokio::test]
    async fn test_plugin_loader_discover() {
        let config = PluginLoaderConfig::default().with_search_path("/tmp/plugins");
        let loader = PluginLoader::new(config);

        // Discover should not fail even if directory doesn't exist
        let result = loader.discover().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_plugin_load_result() {
        let result = PluginLoadResult {
            name: "test-plugin".to_string(),
            success: true,
            security_scan: None,
            error: None,
            loaded_at: chrono::Utc::now(),
        };

        assert_eq!(result.name, "test-plugin");
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_plugin_loader_stats() {
        let config = PluginLoaderConfig::default();
        let loader = PluginLoader::new(config);

        let stats = loader.stats().await;
        assert_eq!(stats.total_discovered, 0);
        assert_eq!(stats.successful_loads, 0);
        assert_eq!(stats.failed_loads, 0);
        assert!(!stats.hot_reload_active);
    }

    #[tokio::test]
    async fn test_plugin_loader_list_loaded() {
        let config = PluginLoaderConfig::default();
        let loader = PluginLoader::new(config);

        let loaded = loader.list_loaded().await;
        assert_eq!(loaded.len(), 0);
    }

    #[tokio::test]
    async fn test_plugin_loader_get_load_results() {
        let config = PluginLoaderConfig::default();
        let loader = PluginLoader::new(config);

        let results = loader.get_load_results().await;
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_plugin_loader_hot_reload_disabled() {
        let config = PluginLoaderConfig::default();
        let loader = PluginLoader::new(config);

        // Should not fail when hot-reload is disabled
        let result = loader.start_hot_reload().await;
        assert!(result.is_ok());

        loader.stop_hot_reload().await;
    }
}
