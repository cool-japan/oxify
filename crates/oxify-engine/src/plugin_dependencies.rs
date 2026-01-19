//! Plugin Dependency Resolution and Auto-Install
//!
//! Provides dependency resolution, circular dependency detection,
//! and automatic installation of missing plugin dependencies.
//!
//! # Features
//!
//! - Dependency graph construction and validation
//! - Circular dependency detection
//! - Topological sorting for install order
//! - Automatic dependency resolution
//! - Transitive dependency handling

use crate::plugin_manifest::{ManifestError, PluginManifest};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

/// Dependency resolution errors
#[derive(Error, Debug)]
pub enum DependencyError {
    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Dependency not found: {0}")]
    DependencyNotFound(String),

    #[error(
        "Version conflict: {plugin} requires {dependency} {required}, but {found} is available"
    )]
    VersionConflict {
        plugin: String,
        dependency: String,
        required: String,
        found: String,
    },

    #[error("Invalid dependency specification: {0}")]
    InvalidSpec(String),

    #[error("Manifest error: {0}")]
    ManifestError(#[from] ManifestError),
}

/// Dependency node in the resolution graph
#[derive(Debug, Clone)]
pub struct DependencyNode {
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// Dependencies (name -> version requirement)
    pub dependencies: HashMap<String, String>,
    /// Is this plugin already installed?
    pub installed: bool,
}

impl DependencyNode {
    /// Create a new dependency node from a manifest
    pub fn from_manifest(manifest: PluginManifest, installed: bool) -> Self {
        let dependencies = manifest.dependencies.clone();
        Self {
            manifest,
            dependencies,
            installed,
        }
    }

    /// Get the plugin name
    pub fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    /// Get the plugin version
    pub fn version(&self) -> &str {
        &self.manifest.plugin.version
    }
}

/// Dependency resolution graph
pub struct DependencyGraph {
    /// All nodes in the graph (plugin name -> node)
    nodes: HashMap<String, DependencyNode>,
    /// Available plugins that can be installed (name -> manifest)
    available: HashMap<String, PluginManifest>,
}

impl DependencyGraph {
    /// Create a new dependency graph
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            available: HashMap::new(),
        }
    }

    /// Add an installed plugin to the graph
    pub fn add_installed(&mut self, manifest: PluginManifest) {
        let node = DependencyNode::from_manifest(manifest.clone(), true);
        self.nodes.insert(manifest.plugin.name.clone(), node);
    }

    /// Add an available plugin (can be installed)
    pub fn add_available(&mut self, manifest: PluginManifest) {
        self.available
            .insert(manifest.plugin.name.clone(), manifest);
    }

    /// Add a plugin to install (marks as not installed)
    pub fn add_to_install(&mut self, manifest: PluginManifest) {
        let node = DependencyNode::from_manifest(manifest.clone(), false);
        self.nodes.insert(manifest.plugin.name.clone(), node);
    }

    /// Resolve dependencies for a plugin
    pub fn resolve(&mut self, plugin_name: &str) -> Result<Vec<String>, DependencyError> {
        // Build the dependency tree recursively
        let mut to_resolve = VecDeque::new();
        to_resolve.push_back(plugin_name.to_string());
        let mut resolved = HashSet::new();

        while let Some(current) = to_resolve.pop_front() {
            if resolved.contains(&current) {
                continue;
            }

            // Get the node (either from nodes or available)
            let node = if let Some(node) = self.nodes.get(&current) {
                node.clone()
            } else if let Some(manifest) = self.available.get(&current) {
                let node = DependencyNode::from_manifest(manifest.clone(), false);
                self.nodes.insert(current.clone(), node.clone());
                node
            } else {
                return Err(DependencyError::DependencyNotFound(current));
            };

            // Add dependencies to resolve queue
            for dep_name in node.dependencies.keys() {
                // Skip oxify-engine as it's always available
                if dep_name == "oxify-engine" {
                    continue;
                }
                if !resolved.contains(dep_name) {
                    to_resolve.push_back(dep_name.clone());
                }
            }

            resolved.insert(current);
        }

        // Validate versions
        self.validate_versions()?;

        // Detect circular dependencies
        self.detect_cycles()?;

        // Compute topological order (install order)
        self.topological_sort()
    }

    /// Validate that all version requirements are satisfied
    fn validate_versions(&self) -> Result<(), DependencyError> {
        for node in self.nodes.values() {
            for (dep_name, version_req) in &node.dependencies {
                // Skip oxify-engine
                if dep_name == "oxify-engine" {
                    continue;
                }

                if let Some(dep_node) = self.nodes.get(dep_name) {
                    if !dep_node.manifest.satisfies_version(version_req) {
                        return Err(DependencyError::VersionConflict {
                            plugin: node.name().to_string(),
                            dependency: dep_name.clone(),
                            required: version_req.clone(),
                            found: dep_node.version().to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Detect circular dependencies using DFS
    fn detect_cycles(&self) -> Result<(), DependencyError> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for node_name in self.nodes.keys() {
            if !visited.contains(node_name) {
                self.dfs_detect_cycle(node_name, &mut visited, &mut rec_stack, &mut path)?;
            }
        }

        Ok(())
    }

    /// DFS helper for cycle detection
    fn dfs_detect_cycle(
        &self,
        node_name: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Result<(), DependencyError> {
        visited.insert(node_name.to_string());
        rec_stack.insert(node_name.to_string());
        path.push(node_name.to_string());

        if let Some(node) = self.nodes.get(node_name) {
            for dep_name in node.dependencies.keys() {
                // Skip oxify-engine
                if dep_name == "oxify-engine" {
                    continue;
                }

                if !visited.contains(dep_name) {
                    self.dfs_detect_cycle(dep_name, visited, rec_stack, path)?;
                } else if rec_stack.contains(dep_name) {
                    // Found a cycle
                    let cycle_start = path.iter().position(|p| p == dep_name).unwrap();
                    let cycle_path: Vec<_> = path[cycle_start..].to_vec();
                    return Err(DependencyError::CircularDependency(cycle_path.join(" -> ")));
                }
            }
        }

        rec_stack.remove(node_name);
        path.pop();
        Ok(())
    }

    /// Compute topological sort for install order using Kahn's algorithm
    fn topological_sort(&self) -> Result<Vec<String>, DependencyError> {
        // Calculate in-degrees
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();

        for node in self.nodes.values() {
            in_degree.entry(node.name().to_string()).or_insert(0);
            adj_list.entry(node.name().to_string()).or_default();

            for dep_name in node.dependencies.keys() {
                // Skip oxify-engine and already installed plugins
                if dep_name == "oxify-engine" {
                    continue;
                }
                if let Some(dep_node) = self.nodes.get(dep_name) {
                    if dep_node.installed {
                        continue;
                    }
                }

                *in_degree.entry(node.name().to_string()).or_insert(0) += 1;
                adj_list
                    .entry(dep_name.clone())
                    .or_default()
                    .push(node.name().to_string());
            }
        }

        // Kahn's algorithm
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(name, _)| name.clone())
            .collect();

        let mut sorted = Vec::new();

        while let Some(node_name) = queue.pop_front() {
            // Only include nodes that need to be installed
            if let Some(node) = self.nodes.get(&node_name) {
                if !node.installed {
                    sorted.push(node_name.clone());
                }
            }

            if let Some(neighbors) = adj_list.get(&node_name) {
                for neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor.clone());
                        }
                    }
                }
            }
        }

        Ok(sorted)
    }

    /// Get all plugins that need to be installed
    pub fn get_install_list(&self) -> Vec<&DependencyNode> {
        self.nodes.values().filter(|node| !node.installed).collect()
    }

    /// Check if a plugin is in the graph
    pub fn contains(&self, name: &str) -> bool {
        self.nodes.contains_key(name)
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Dependency resolver
pub struct DependencyResolver {
    /// Dependency graph
    graph: DependencyGraph,
}

impl DependencyResolver {
    /// Create a new dependency resolver
    pub fn new() -> Self {
        Self {
            graph: DependencyGraph::new(),
        }
    }

    /// Add installed plugins
    pub fn add_installed_plugins(&mut self, manifests: Vec<PluginManifest>) {
        for manifest in manifests {
            self.graph.add_installed(manifest);
        }
    }

    /// Add available plugins (can be installed from registry)
    pub fn add_available_plugins(&mut self, manifests: Vec<PluginManifest>) {
        for manifest in manifests {
            self.graph.add_available(manifest);
        }
    }

    /// Resolve dependencies for a plugin and return install order
    pub fn resolve(&mut self, manifest: PluginManifest) -> Result<Vec<String>, DependencyError> {
        let plugin_name = manifest.plugin.name.clone();
        self.graph.add_to_install(manifest);
        self.graph.resolve(&plugin_name)
    }

    /// Get the dependency graph
    pub fn graph(&self) -> &DependencyGraph {
        &self.graph
    }

    /// Get a mutable reference to the dependency graph
    pub fn graph_mut(&mut self) -> &mut DependencyGraph {
        &mut self.graph
    }
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{PluginCapabilities, PluginInfo};

    fn create_test_manifest(name: &str, version: &str, deps: Vec<(&str, &str)>) -> PluginManifest {
        let dependencies: HashMap<String, String> = deps
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        PluginManifest {
            plugin: PluginInfo {
                name: name.to_string(),
                version: version.to_string(),
                description: None,
                author: None,
                license: None,
                homepage: None,
                repository: None,
                keywords: vec![],
                category: None,
            },
            capabilities: PluginCapabilities::default(),
            config: Default::default(),
            dependencies,
            hooks: Default::default(),
        }
    }

    #[test]
    fn test_dependency_node_creation() {
        let manifest = create_test_manifest("test", "1.0.0", vec![]);
        let node = DependencyNode::from_manifest(manifest, false);

        assert_eq!(node.name(), "test");
        assert_eq!(node.version(), "1.0.0");
        assert!(!node.installed);
    }

    #[test]
    fn test_dependency_graph_add_installed() {
        let mut graph = DependencyGraph::new();
        let manifest = create_test_manifest("plugin-a", "1.0.0", vec![]);

        graph.add_installed(manifest);
        assert!(graph.contains("plugin-a"));
    }

    #[test]
    fn test_dependency_graph_add_available() {
        let mut graph = DependencyGraph::new();
        let manifest = create_test_manifest("plugin-b", "2.0.0", vec![]);

        graph.add_available(manifest.clone());
        assert_eq!(graph.available.len(), 1);
        assert!(graph.available.contains_key("plugin-b"));
    }

    #[test]
    fn test_simple_dependency_resolution() {
        let mut graph = DependencyGraph::new();

        // Plugin A depends on B
        let manifest_a = create_test_manifest("plugin-a", "1.0.0", vec![("plugin-b", ">=1.0.0")]);
        let manifest_b = create_test_manifest("plugin-b", "1.0.0", vec![]);

        graph.add_available(manifest_b);
        graph.add_to_install(manifest_a);

        let result = graph.resolve("plugin-a");
        assert!(result.is_ok());

        let install_order = result.unwrap();
        assert_eq!(install_order.len(), 2);
        // B should be installed before A
        assert_eq!(install_order[0], "plugin-b");
        assert_eq!(install_order[1], "plugin-a");
    }

    #[test]
    fn test_transitive_dependencies() {
        let mut graph = DependencyGraph::new();

        // A depends on B, B depends on C
        let manifest_a = create_test_manifest("plugin-a", "1.0.0", vec![("plugin-b", ">=1.0.0")]);
        let manifest_b = create_test_manifest("plugin-b", "1.0.0", vec![("plugin-c", ">=1.0.0")]);
        let manifest_c = create_test_manifest("plugin-c", "1.0.0", vec![]);

        graph.add_available(manifest_b);
        graph.add_available(manifest_c);
        graph.add_to_install(manifest_a);

        let result = graph.resolve("plugin-a");
        assert!(result.is_ok());

        let install_order = result.unwrap();
        assert_eq!(install_order.len(), 3);
        // C -> B -> A
        assert_eq!(install_order[0], "plugin-c");
        assert_eq!(install_order[1], "plugin-b");
        assert_eq!(install_order[2], "plugin-a");
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut graph = DependencyGraph::new();

        // A depends on B, B depends on A (circular)
        let manifest_a = create_test_manifest("plugin-a", "1.0.0", vec![("plugin-b", ">=1.0.0")]);
        let manifest_b = create_test_manifest("plugin-b", "1.0.0", vec![("plugin-a", ">=1.0.0")]);

        graph.add_available(manifest_b);
        graph.add_to_install(manifest_a);

        let result = graph.resolve("plugin-a");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DependencyError::CircularDependency(_)
        ));
    }

    #[test]
    fn test_version_conflict_detection() {
        let mut graph = DependencyGraph::new();

        // A requires B >=2.0.0, but B 1.0.0 is available
        let manifest_a = create_test_manifest("plugin-a", "1.0.0", vec![("plugin-b", ">=2.0.0")]);
        let manifest_b = create_test_manifest("plugin-b", "1.0.0", vec![]);

        graph.add_available(manifest_b);
        graph.add_to_install(manifest_a);

        let result = graph.resolve("plugin-a");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DependencyError::VersionConflict { .. }
        ));
    }

    #[test]
    fn test_skip_installed_dependencies() {
        let mut graph = DependencyGraph::new();

        // A depends on B, but B is already installed
        let manifest_a = create_test_manifest("plugin-a", "1.0.0", vec![("plugin-b", ">=1.0.0")]);
        let manifest_b = create_test_manifest("plugin-b", "1.5.0", vec![]);

        graph.add_installed(manifest_b);
        graph.add_to_install(manifest_a);

        let result = graph.resolve("plugin-a");
        assert!(result.is_ok());

        let install_order = result.unwrap();
        // Only A should need to be installed
        assert_eq!(install_order.len(), 1);
        assert_eq!(install_order[0], "plugin-a");
    }

    #[test]
    fn test_dependency_resolver() {
        let mut resolver = DependencyResolver::new();

        // Add installed plugins
        let installed = vec![create_test_manifest("base-plugin", "1.0.0", vec![])];
        resolver.add_installed_plugins(installed);

        // Add available plugins
        let available = vec![create_test_manifest(
            "util-plugin",
            "1.0.0",
            vec![("base-plugin", ">=1.0.0")],
        )];
        resolver.add_available_plugins(available);

        // Resolve a new plugin that depends on util-plugin
        let new_plugin =
            create_test_manifest("my-plugin", "1.0.0", vec![("util-plugin", ">=1.0.0")]);
        let result = resolver.resolve(new_plugin);

        assert!(result.is_ok());
        let install_order = result.unwrap();
        assert_eq!(install_order.len(), 2);
    }

    #[test]
    fn test_missing_dependency() {
        let mut graph = DependencyGraph::new();

        // A depends on B, but B is not available
        let manifest_a = create_test_manifest("plugin-a", "1.0.0", vec![("plugin-b", ">=1.0.0")]);
        graph.add_to_install(manifest_a);

        let result = graph.resolve("plugin-a");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DependencyError::DependencyNotFound(_)
        ));
    }

    #[test]
    fn test_oxify_engine_always_available() {
        let mut graph = DependencyGraph::new();

        // Plugin depends on oxify-engine (should not fail)
        let manifest = create_test_manifest("plugin-a", "1.0.0", vec![("oxify-engine", ">=0.1.0")]);
        graph.add_to_install(manifest);

        let result = graph.resolve("plugin-a");
        assert!(result.is_ok());
    }

    #[test]
    fn test_complex_dependency_tree() {
        let mut graph = DependencyGraph::new();

        // Complex tree:
        // A depends on B, C
        // B depends on D
        // C depends on D, E
        // D depends on F
        let manifest_a =
            create_test_manifest("a", "1.0.0", vec![("b", ">=1.0.0"), ("c", ">=1.0.0")]);
        let manifest_b = create_test_manifest("b", "1.0.0", vec![("d", ">=1.0.0")]);
        let manifest_c =
            create_test_manifest("c", "1.0.0", vec![("d", ">=1.0.0"), ("e", ">=1.0.0")]);
        let manifest_d = create_test_manifest("d", "1.0.0", vec![("f", ">=1.0.0")]);
        let manifest_e = create_test_manifest("e", "1.0.0", vec![]);
        let manifest_f = create_test_manifest("f", "1.0.0", vec![]);

        graph.add_available(manifest_b);
        graph.add_available(manifest_c);
        graph.add_available(manifest_d);
        graph.add_available(manifest_e);
        graph.add_available(manifest_f);
        graph.add_to_install(manifest_a);

        let result = graph.resolve("a");
        assert!(result.is_ok());

        let install_order = result.unwrap();
        assert_eq!(install_order.len(), 6);

        // F should come before D, D before B and C, B and C before A, E before C
        let f_pos = install_order.iter().position(|n| n == "f").unwrap();
        let d_pos = install_order.iter().position(|n| n == "d").unwrap();
        let b_pos = install_order.iter().position(|n| n == "b").unwrap();
        let c_pos = install_order.iter().position(|n| n == "c").unwrap();
        let e_pos = install_order.iter().position(|n| n == "e").unwrap();
        let a_pos = install_order.iter().position(|n| n == "a").unwrap();

        assert!(f_pos < d_pos);
        assert!(d_pos < b_pos);
        assert!(d_pos < c_pos);
        assert!(e_pos < c_pos);
        assert!(b_pos < a_pos);
        assert!(c_pos < a_pos);
    }
}
