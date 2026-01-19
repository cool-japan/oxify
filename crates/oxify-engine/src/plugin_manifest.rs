//! Plugin Manifest and Discovery System
//!
//! Provides plugin manifest format, discovery, and hot-reload capabilities.
//!
//! # Manifest Format
//!
//! Plugins are defined using TOML manifests:
//!
//! ```toml
//! [plugin]
//! name = "my-custom-node"
//! version = "1.0.0"
//! description = "Custom node for specialized processing"
//! author = "Your Name"
//! license = "MIT"
//! homepage = "https://github.com/example/my-plugin"
//!
//! [plugin.capabilities]
//! node_types = ["custom_processor", "custom_transformer"]
//! supports_streaming = false
//! supports_batching = true
//!
//! [plugin.config]
//! schema = "config-schema.json"
//! defaults = { timeout = 30, retries = 3 }
//!
//! [plugin.dependencies]
//! oxify-engine = ">=0.1.0"
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;

/// Plugin manifest errors
#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum ManifestError {
    #[error("Failed to read manifest file: {0}")]
    ReadError(String),

    #[error("Failed to parse manifest: {0}")]
    ParseError(String),

    #[error("Invalid manifest: {0}")]
    ValidationError(String),

    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Version mismatch: required {required}, found {found}")]
    VersionMismatch { required: String, found: String },

    #[error("Dependency error: {0}")]
    DependencyError(String),
}

/// Plugin manifest structure
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Core plugin information
    pub plugin: PluginInfo,
    /// Plugin capabilities
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    /// Plugin configuration
    #[serde(default)]
    pub config: PluginConfig,
    /// Plugin dependencies
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    /// Plugin hooks
    #[serde(default)]
    pub hooks: PluginHooks,
}

/// Core plugin information
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Unique plugin identifier
    pub name: String,
    /// Plugin version (semver)
    pub version: String,
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
    /// Plugin author
    #[serde(default)]
    pub author: Option<String>,
    /// License identifier
    #[serde(default)]
    pub license: Option<String>,
    /// Homepage URL
    #[serde(default)]
    pub homepage: Option<String>,
    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,
    /// Keywords for search/discovery
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Plugin category
    #[serde(default)]
    pub category: Option<PluginCategory>,
}

/// Plugin categories
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCategory {
    /// Data transformation nodes
    Transform,
    /// Integration nodes (APIs, databases)
    Integration,
    /// AI/ML nodes
    Ai,
    /// Utility nodes
    Utility,
    /// Control flow nodes
    ControlFlow,
    /// Custom category
    Custom,
}

/// Plugin capabilities
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// Node types this plugin provides
    #[serde(default)]
    pub node_types: Vec<String>,
    /// Whether the plugin supports streaming
    #[serde(default)]
    pub supports_streaming: bool,
    /// Whether the plugin supports batching
    #[serde(default)]
    pub supports_batching: bool,
    /// Whether the plugin supports parallel execution
    #[serde(default)]
    pub supports_parallel: bool,
    /// Whether the plugin is sandboxed (WASM)
    #[serde(default)]
    pub sandboxed: bool,
    /// Resource requirements
    #[serde(default)]
    pub resource_requirements: ResourceRequirements,
}

/// Resource requirements for plugin execution
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// Minimum memory in MB
    #[serde(default)]
    pub min_memory_mb: Option<u64>,
    /// Maximum memory in MB
    #[serde(default)]
    pub max_memory_mb: Option<u64>,
    /// CPU cores required
    #[serde(default)]
    pub cpu_cores: Option<u32>,
    /// Requires network access
    #[serde(default)]
    pub requires_network: bool,
    /// Requires filesystem access
    #[serde(default)]
    pub requires_filesystem: bool,
}

/// Plugin configuration schema
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Path to JSON schema for configuration
    #[serde(default)]
    pub schema: Option<String>,
    /// Default configuration values
    #[serde(default)]
    pub defaults: HashMap<String, serde_json::Value>,
    /// Environment variables to pass to plugin
    #[serde(default)]
    pub env_vars: Vec<String>,
}

/// Plugin lifecycle hooks
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginHooks {
    /// Script to run on plugin load
    #[serde(default)]
    pub on_load: Option<String>,
    /// Script to run on plugin unload
    #[serde(default)]
    pub on_unload: Option<String>,
    /// Script to run before node execution
    #[serde(default)]
    pub before_execute: Option<String>,
    /// Script to run after node execution
    #[serde(default)]
    pub after_execute: Option<String>,
}

#[allow(dead_code)]
impl PluginManifest {
    /// Parse manifest from TOML string
    pub fn from_toml(toml_str: &str) -> Result<Self, ManifestError> {
        toml::from_str(toml_str).map_err(|e| ManifestError::ParseError(e.to_string()))
    }

    /// Parse manifest from file
    pub fn from_file(path: &Path) -> Result<Self, ManifestError> {
        let contents =
            std::fs::read_to_string(path).map_err(|e| ManifestError::ReadError(e.to_string()))?;
        Self::from_toml(&contents)
    }

    /// Serialize manifest to TOML string
    pub fn to_toml(&self) -> Result<String, ManifestError> {
        toml::to_string_pretty(self).map_err(|e| ManifestError::ParseError(e.to_string()))
    }

    /// Validate the manifest
    pub fn validate(&self) -> Result<(), ManifestError> {
        // Check required fields
        if self.plugin.name.is_empty() {
            return Err(ManifestError::ValidationError(
                "Plugin name is required".into(),
            ));
        }

        if self.plugin.version.is_empty() {
            return Err(ManifestError::ValidationError(
                "Plugin version is required".into(),
            ));
        }

        // Validate version format (semver)
        if !is_valid_semver(&self.plugin.version) {
            return Err(ManifestError::ValidationError(format!(
                "Invalid version format: {}",
                self.plugin.version
            )));
        }

        // Validate node types
        for node_type in &self.capabilities.node_types {
            if node_type.is_empty() {
                return Err(ManifestError::ValidationError(
                    "Empty node type is not allowed".into(),
                ));
            }
        }

        Ok(())
    }

    /// Check if this plugin satisfies a version requirement
    pub fn satisfies_version(&self, requirement: &str) -> bool {
        check_version_requirement(&self.plugin.version, requirement)
    }
}

/// Simple semver validation
#[allow(dead_code)]
fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u32>().is_ok())
}

/// Simple version requirement check (supports >=, >, <, <=, =)
#[allow(dead_code)]
fn check_version_requirement(version: &str, requirement: &str) -> bool {
    let requirement = requirement.trim();

    if requirement.starts_with(">=") {
        let req_ver = requirement.trim_start_matches(">=").trim();
        compare_versions(version, req_ver) >= 0
    } else if requirement.starts_with('>') {
        let req_ver = requirement.trim_start_matches('>').trim();
        compare_versions(version, req_ver) > 0
    } else if requirement.starts_with("<=") {
        let req_ver = requirement.trim_start_matches("<=").trim();
        compare_versions(version, req_ver) <= 0
    } else if requirement.starts_with('<') {
        let req_ver = requirement.trim_start_matches('<').trim();
        compare_versions(version, req_ver) < 0
    } else if requirement.starts_with('=') {
        let req_ver = requirement.trim_start_matches('=').trim();
        compare_versions(version, req_ver) == 0
    } else {
        // Exact match
        compare_versions(version, requirement) == 0
    }
}

/// Compare two semver versions
#[allow(dead_code)]
fn compare_versions(v1: &str, v2: &str) -> i32 {
    let parse_version =
        |v: &str| -> Vec<u32> { v.split('.').filter_map(|p| p.parse().ok()).collect() };

    let parts1 = parse_version(v1);
    let parts2 = parse_version(v2);

    for i in 0..3 {
        let p1 = parts1.get(i).copied().unwrap_or(0);
        let p2 = parts2.get(i).copied().unwrap_or(0);
        match p1.cmp(&p2) {
            std::cmp::Ordering::Greater => return 1,
            std::cmp::Ordering::Less => return -1,
            std::cmp::Ordering::Equal => continue,
        }
    }
    0
}

/// Loaded plugin with metadata and state
#[allow(dead_code)]
#[derive(Debug)]
pub struct LoadedPlugin {
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// Path to plugin directory
    pub path: PathBuf,
    /// Load timestamp
    pub loaded_at: SystemTime,
    /// Last modified timestamp of manifest
    pub manifest_modified: SystemTime,
    /// Plugin state
    pub state: PluginState,
}

/// Plugin runtime state
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// Plugin is loaded and ready
    Ready,
    /// Plugin is loading
    Loading,
    /// Plugin failed to load
    Failed,
    /// Plugin is disabled
    Disabled,
    /// Plugin needs reload (files changed)
    NeedsReload,
}

/// Plugin discovery and hot-reload manager
#[allow(dead_code)]
pub struct PluginManager {
    /// Loaded plugins
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    /// Plugin search paths
    search_paths: Vec<PathBuf>,
    /// Enable hot-reload
    hot_reload_enabled: bool,
    /// Hot-reload check interval
    reload_interval: Duration,
    /// Hot-reload task handle
    reload_task_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

#[allow(dead_code)]
impl PluginManager {
    /// Create a new plugin manager
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            search_paths: vec![],
            hot_reload_enabled: false,
            reload_interval: Duration::from_secs(5),
            reload_task_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Add a search path for plugins
    pub fn add_search_path(&mut self, path: impl Into<PathBuf>) {
        self.search_paths.push(path.into());
    }

    /// Enable hot-reload with the specified check interval
    pub fn enable_hot_reload(&mut self, interval: Duration) {
        self.hot_reload_enabled = true;
        self.reload_interval = interval;
    }

    /// Start the hot-reload background task
    pub async fn start_hot_reload(&self) -> Result<(), ManifestError> {
        if !self.hot_reload_enabled {
            return Err(ManifestError::ValidationError(
                "Hot-reload is not enabled. Call enable_hot_reload() first.".to_string(),
            ));
        }

        // Check if already running
        if self.reload_task_handle.read().await.is_some() {
            return Ok(()); // Already running
        }

        let plugins = Arc::clone(&self.plugins);
        let interval = self.reload_interval;

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                // Check for plugins that need reloading
                let needs_reload = {
                    let plugins_lock = plugins.read().await;
                    let mut to_reload = Vec::new();

                    for (name, loaded) in plugins_lock.iter() {
                        let manifest_path = loaded.path.join("plugin.toml");
                        if let Ok(metadata) = std::fs::metadata(&manifest_path) {
                            if let Ok(modified) = metadata.modified() {
                                if modified > loaded.manifest_modified {
                                    to_reload.push((name.clone(), loaded.path.clone()));
                                    tracing::info!("Detected changes in plugin: {}", name);
                                }
                            }
                        }
                    }
                    to_reload
                };

                // Reload changed plugins
                for (name, path) in needs_reload {
                    let manifest_path = path.join("plugin.toml");
                    match PluginManifest::from_file(&manifest_path) {
                        Ok(manifest) => {
                            if manifest.validate().is_ok() {
                                let manifest_modified = std::fs::metadata(&manifest_path)
                                    .and_then(|m| m.modified())
                                    .unwrap_or_else(|_| SystemTime::now());

                                let loaded = LoadedPlugin {
                                    manifest: manifest.clone(),
                                    path: path.clone(),
                                    loaded_at: SystemTime::now(),
                                    manifest_modified,
                                    state: PluginState::Ready,
                                };

                                let mut plugins_write = plugins.write().await;
                                plugins_write.insert(name.clone(), loaded);

                                tracing::info!(
                                    "Hot-reloaded plugin: {} v{}",
                                    manifest.plugin.name,
                                    manifest.plugin.version
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to reload plugin {}: {}", name, e);
                        }
                    }
                }
            }
        });

        *self.reload_task_handle.write().await = Some(handle);
        tracing::info!("Started hot-reload task with interval: {:?}", interval);

        Ok(())
    }

    /// Stop the hot-reload background task
    pub async fn stop_hot_reload(&self) {
        if let Some(handle) = self.reload_task_handle.write().await.take() {
            handle.abort();
            tracing::info!("Stopped hot-reload task");
        }
    }

    /// Discover plugins in search paths
    pub async fn discover(&self) -> Result<Vec<PluginManifest>, ManifestError> {
        let mut manifests = Vec::new();

        for path in &self.search_paths {
            if !path.exists() {
                continue;
            }

            // Look for plugin.toml files
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        let manifest_path = entry_path.join("plugin.toml");
                        if manifest_path.exists() {
                            match PluginManifest::from_file(&manifest_path) {
                                Ok(manifest) => {
                                    if manifest.validate().is_ok() {
                                        manifests.push(manifest);
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to load manifest from {:?}: {}",
                                        manifest_path,
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(manifests)
    }

    /// Load a plugin from manifest
    pub async fn load(&self, manifest: PluginManifest, path: PathBuf) -> Result<(), ManifestError> {
        manifest.validate()?;

        // Check dependencies
        for (dep_name, dep_version) in &manifest.dependencies {
            let plugins = self.plugins.read().await;
            if let Some(loaded) = plugins.get(dep_name) {
                if !loaded.manifest.satisfies_version(dep_version) {
                    return Err(ManifestError::DependencyError(format!(
                        "Plugin {} requires {} {}, but found {}",
                        manifest.plugin.name, dep_name, dep_version, loaded.manifest.plugin.version
                    )));
                }
            } else if dep_name != "oxify-engine" {
                // oxify-engine is always available
                return Err(ManifestError::DependencyError(format!(
                    "Plugin {} requires {} which is not loaded",
                    manifest.plugin.name, dep_name
                )));
            }
        }

        let manifest_modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());

        let loaded = LoadedPlugin {
            manifest: manifest.clone(),
            path,
            loaded_at: SystemTime::now(),
            manifest_modified,
            state: PluginState::Ready,
        };

        let mut plugins = self.plugins.write().await;
        plugins.insert(manifest.plugin.name.clone(), loaded);

        tracing::info!(
            "Loaded plugin: {} v{}",
            manifest.plugin.name,
            manifest.plugin.version
        );

        Ok(())
    }

    /// Unload a plugin
    pub async fn unload(&self, name: &str) -> bool {
        let mut plugins = self.plugins.write().await;
        if let Some(loaded) = plugins.remove(name) {
            tracing::info!("Unloaded plugin: {}", loaded.manifest.plugin.name);
            true
        } else {
            false
        }
    }

    /// Get a loaded plugin
    pub async fn get(&self, name: &str) -> Option<PluginManifest> {
        let plugins = self.plugins.read().await;
        plugins.get(name).map(|p| p.manifest.clone())
    }

    /// List all loaded plugins
    pub async fn list(&self) -> Vec<PluginManifest> {
        let plugins = self.plugins.read().await;
        plugins.values().map(|p| p.manifest.clone()).collect()
    }

    /// Check for plugins that need reloading
    pub async fn check_for_reloads(&self) -> Vec<String> {
        if !self.hot_reload_enabled {
            return vec![];
        }

        let mut needs_reload = Vec::new();
        let plugins = self.plugins.read().await;

        for (name, loaded) in plugins.iter() {
            let manifest_path = loaded.path.join("plugin.toml");
            if let Ok(metadata) = std::fs::metadata(&manifest_path) {
                if let Ok(modified) = metadata.modified() {
                    if modified > loaded.manifest_modified {
                        needs_reload.push(name.clone());
                    }
                }
            }
        }

        needs_reload
    }

    /// Reload a plugin
    pub async fn reload(&self, name: &str) -> Result<(), ManifestError> {
        let path = {
            let plugins = self.plugins.read().await;
            plugins
                .get(name)
                .map(|p| p.path.clone())
                .ok_or_else(|| ManifestError::NotFound(name.to_string()))?
        };

        let manifest_path = path.join("plugin.toml");
        let manifest = PluginManifest::from_file(&manifest_path)?;

        // Unload old version
        self.unload(name).await;

        // Load new version
        self.load(manifest, path).await?;

        tracing::info!("Reloaded plugin: {}", name);

        Ok(())
    }

    /// Find plugins by node type
    pub async fn find_by_node_type(&self, node_type: &str) -> Vec<PluginManifest> {
        let plugins = self.plugins.read().await;
        plugins
            .values()
            .filter(|p| {
                p.manifest
                    .capabilities
                    .node_types
                    .contains(&node_type.to_string())
            })
            .map(|p| p.manifest.clone())
            .collect()
    }

    /// Find plugins by category
    pub async fn find_by_category(&self, category: PluginCategory) -> Vec<PluginManifest> {
        let plugins = self.plugins.read().await;
        plugins
            .values()
            .filter(|p| p.manifest.plugin.category == Some(category))
            .map(|p| p.manifest.clone())
            .collect()
    }

    /// Get plugin statistics
    pub async fn stats(&self) -> PluginStats {
        let plugins = self.plugins.read().await;
        let mut stats = PluginStats {
            total_plugins: plugins.len(),
            ..Default::default()
        };

        for loaded in plugins.values() {
            match loaded.state {
                PluginState::Ready => stats.ready_plugins += 1,
                PluginState::Loading => stats.loading_plugins += 1,
                PluginState::Failed => stats.failed_plugins += 1,
                PluginState::Disabled => stats.disabled_plugins += 1,
                PluginState::NeedsReload => stats.needs_reload_plugins += 1,
            }

            stats.total_node_types += loaded.manifest.capabilities.node_types.len();
        }

        stats
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin statistics
#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub struct PluginStats {
    pub total_plugins: usize,
    pub ready_plugins: usize,
    pub loading_plugins: usize,
    pub failed_plugins: usize,
    pub disabled_plugins: usize,
    pub needs_reload_plugins: usize,
    pub total_node_types: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_parsing() {
        let toml = r#"
            [plugin]
            name = "test-plugin"
            version = "1.0.0"
            description = "A test plugin"
            author = "Test Author"

            [capabilities]
            node_types = ["custom_node"]
            supports_streaming = true
        "#;

        let manifest = PluginManifest::from_toml(toml).unwrap();
        assert_eq!(manifest.plugin.name, "test-plugin");
        assert_eq!(manifest.plugin.version, "1.0.0");
        assert!(manifest.capabilities.supports_streaming);
        assert_eq!(manifest.capabilities.node_types, vec!["custom_node"]);
    }

    #[test]
    fn test_manifest_validation() {
        let mut manifest = PluginManifest {
            plugin: PluginInfo {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                author: None,
                license: None,
                homepage: None,
                repository: None,
                keywords: vec![],
                category: None,
            },
            capabilities: Default::default(),
            config: Default::default(),
            dependencies: Default::default(),
            hooks: Default::default(),
        };

        assert!(manifest.validate().is_ok());

        // Test empty name
        manifest.plugin.name = "".to_string();
        assert!(manifest.validate().is_err());

        // Test invalid version
        manifest.plugin.name = "test".to_string();
        manifest.plugin.version = "invalid".to_string();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_version_comparison() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), 0);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), 1);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), -1);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), 1);
        assert_eq!(compare_versions("1.0", "1.0.0"), 0);
    }

    #[test]
    fn test_version_requirements() {
        assert!(check_version_requirement("1.0.0", ">=1.0.0"));
        assert!(check_version_requirement("1.0.1", ">=1.0.0"));
        assert!(!check_version_requirement("0.9.0", ">=1.0.0"));

        assert!(check_version_requirement("0.9.0", "<1.0.0"));
        assert!(!check_version_requirement("1.0.0", "<1.0.0"));

        assert!(check_version_requirement("1.0.0", "=1.0.0"));
        assert!(!check_version_requirement("1.0.1", "=1.0.0"));
    }

    #[test]
    fn test_semver_validation() {
        assert!(is_valid_semver("1.0.0"));
        assert!(is_valid_semver("1.0"));
        assert!(is_valid_semver("0.1.0"));
        assert!(!is_valid_semver("invalid"));
        assert!(!is_valid_semver("1"));
        assert!(!is_valid_semver("1.0.0.0"));
    }

    #[tokio::test]
    async fn test_plugin_manager() {
        let manager = PluginManager::new();

        let manifest = PluginManifest {
            plugin: PluginInfo {
                name: "test-plugin".to_string(),
                version: "1.0.0".to_string(),
                description: Some("Test".to_string()),
                author: None,
                license: None,
                homepage: None,
                repository: None,
                keywords: vec![],
                category: Some(PluginCategory::Utility),
            },
            capabilities: PluginCapabilities {
                node_types: vec!["custom_test".to_string()],
                ..Default::default()
            },
            config: Default::default(),
            dependencies: Default::default(),
            hooks: Default::default(),
        };

        manager.load(manifest, PathBuf::from("/tmp")).await.unwrap();

        let plugins = manager.list().await;
        assert_eq!(plugins.len(), 1);

        let found = manager.find_by_node_type("custom_test").await;
        assert_eq!(found.len(), 1);

        let by_category = manager.find_by_category(PluginCategory::Utility).await;
        assert_eq!(by_category.len(), 1);

        let unloaded = manager.unload("test-plugin").await;
        assert!(unloaded);

        let plugins = manager.list().await;
        assert_eq!(plugins.len(), 0);
    }

    #[tokio::test]
    async fn test_plugin_stats() {
        let manager = PluginManager::new();

        let manifest = PluginManifest {
            plugin: PluginInfo {
                name: "stats-test".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                author: None,
                license: None,
                homepage: None,
                repository: None,
                keywords: vec![],
                category: None,
            },
            capabilities: PluginCapabilities {
                node_types: vec!["node1".to_string(), "node2".to_string()],
                ..Default::default()
            },
            config: Default::default(),
            dependencies: Default::default(),
            hooks: Default::default(),
        };

        manager.load(manifest, PathBuf::from("/tmp")).await.unwrap();

        let stats = manager.stats().await;
        assert_eq!(stats.total_plugins, 1);
        assert_eq!(stats.ready_plugins, 1);
        assert_eq!(stats.total_node_types, 2);
    }

    #[tokio::test]
    async fn test_hot_reload_start_stop() {
        let mut manager = PluginManager::new();
        manager.enable_hot_reload(Duration::from_millis(100));

        // Start hot-reload
        let result = manager.start_hot_reload().await;
        assert!(result.is_ok());

        // Give it a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Stop hot-reload
        manager.stop_hot_reload().await;
    }

    #[tokio::test]
    async fn test_hot_reload_not_enabled() {
        let manager = PluginManager::new();

        // Try to start without enabling
        let result = manager.start_hot_reload().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hot_reload_idempotent() {
        let mut manager = PluginManager::new();
        manager.enable_hot_reload(Duration::from_millis(100));

        // Start twice - second should be no-op
        manager.start_hot_reload().await.unwrap();
        let result = manager.start_hot_reload().await;
        assert!(result.is_ok());

        manager.stop_hot_reload().await;
    }

    #[test]
    fn test_manifest_to_toml() {
        let manifest = PluginManifest {
            plugin: PluginInfo {
                name: "serialization-test".to_string(),
                version: "2.0.0".to_string(),
                description: Some("Test serialization".to_string()),
                author: Some("Test Author".to_string()),
                license: Some("MIT".to_string()),
                homepage: None,
                repository: None,
                keywords: vec!["test".to_string()],
                category: Some(PluginCategory::Transform),
            },
            capabilities: PluginCapabilities {
                node_types: vec!["transform".to_string()],
                supports_streaming: true,
                ..Default::default()
            },
            config: Default::default(),
            dependencies: Default::default(),
            hooks: Default::default(),
        };

        let toml_str = manifest.to_toml().unwrap();
        assert!(toml_str.contains("serialization-test"));
        assert!(toml_str.contains("2.0.0"));

        // Roundtrip test
        let parsed = PluginManifest::from_toml(&toml_str).unwrap();
        assert_eq!(parsed.plugin.name, manifest.plugin.name);
        assert_eq!(parsed.plugin.version, manifest.plugin.version);
    }
}
