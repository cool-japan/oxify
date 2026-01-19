//! Workflow versioning and change tracking system
//!
//! This module provides comprehensive version management for workflows,
//! including version history, compatibility checks, and diff generation.

use crate::{Workflow, WorkflowId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Workflow version history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WorkflowVersionEntry {
    /// Version number (e.g., "1.2.3")
    pub version: String,

    /// Workflow ID at this version
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub workflow_id: WorkflowId,

    /// Parent workflow ID (previous version)
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub parent_id: Option<WorkflowId>,

    /// Author of this version
    pub author: String,

    /// Timestamp when this version was created
    pub created_at: DateTime<Utc>,

    /// Description of changes in this version
    pub change_description: String,

    /// Type of change (Major, Minor, Patch)
    pub change_type: ChangeType,

    /// Tags for this version
    #[serde(default)]
    pub tags: Vec<String>,

    /// Whether this version is published/released
    #[serde(default)]
    pub published: bool,

    /// Changelog entries
    #[serde(default)]
    pub changelog: Vec<ChangelogEntry>,
}

/// Type of version change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum ChangeType {
    /// Breaking changes (1.0.0 -> 2.0.0)
    Major,
    /// New features, backward compatible (1.0.0 -> 1.1.0)
    Minor,
    /// Bug fixes (1.0.0 -> 1.0.1)
    Patch,
}

/// Detailed changelog entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ChangelogEntry {
    /// Type of change
    pub entry_type: ChangelogType,

    /// Description of the change
    pub description: String,

    /// Node IDs affected by this change
    #[serde(default)]
    pub affected_nodes: Vec<String>,

    /// Whether this is a breaking change
    #[serde(default)]
    pub breaking: bool,
}

/// Type of changelog entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum ChangelogType {
    /// New feature added
    Added,
    /// Feature modified
    Changed,
    /// Feature deprecated
    Deprecated,
    /// Feature removed
    Removed,
    /// Bug fixed
    Fixed,
    /// Security fix
    Security,
}

/// Complete version history for a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WorkflowVersionHistory {
    /// Root workflow name
    pub workflow_name: String,

    /// All versions, ordered from oldest to newest
    pub versions: Vec<WorkflowVersionEntry>,

    /// Version aliases (e.g., "latest" -> "1.2.3", "stable" -> "1.0.0")
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}

impl WorkflowVersionHistory {
    /// Create a new version history
    pub fn new(workflow_name: String) -> Self {
        Self {
            workflow_name,
            versions: Vec::new(),
            aliases: HashMap::new(),
        }
    }

    /// Add a new version to the history
    pub fn add_version(&mut self, entry: WorkflowVersionEntry) {
        self.versions.push(entry);
        self.sort_versions();
    }

    /// Sort versions by semantic version
    fn sort_versions(&mut self) {
        self.versions.sort_by(|a, b| {
            let a_parts = parse_version(&a.version).unwrap_or((0, 0, 0));
            let b_parts = parse_version(&b.version).unwrap_or((0, 0, 0));
            a_parts.cmp(&b_parts)
        });
    }

    /// Get the latest version
    pub fn latest_version(&self) -> Option<&WorkflowVersionEntry> {
        self.versions.last()
    }

    /// Get a specific version
    pub fn get_version(&self, version: &str) -> Option<&WorkflowVersionEntry> {
        // Check if it's an alias
        let resolved_version = self
            .aliases
            .get(version)
            .map(|s| s.as_str())
            .unwrap_or(version);

        self.versions.iter().find(|v| v.version == resolved_version)
    }

    /// Get all published versions
    pub fn published_versions(&self) -> Vec<&WorkflowVersionEntry> {
        self.versions.iter().filter(|v| v.published).collect()
    }

    /// Get version history between two versions
    pub fn get_history_between(&self, from: &str, to: &str) -> Vec<&WorkflowVersionEntry> {
        let from_idx = self.versions.iter().position(|v| v.version == from);
        let to_idx = self.versions.iter().position(|v| v.version == to);

        match (from_idx, to_idx) {
            (Some(from), Some(to)) if from < to => self.versions[from + 1..=to].iter().collect(),
            _ => Vec::new(),
        }
    }

    /// Set a version alias
    pub fn set_alias(&mut self, alias: String, version: String) {
        self.aliases.insert(alias, version);
    }

    /// Get all breaking changes since a version
    pub fn breaking_changes_since(&self, version: &str) -> Vec<&ChangelogEntry> {
        let from_idx = self.versions.iter().position(|v| v.version == version);

        match from_idx {
            Some(idx) => self.versions[idx + 1..]
                .iter()
                .flat_map(|v| &v.changelog)
                .filter(|e| e.breaking)
                .collect(),
            None => Vec::new(),
        }
    }

    /// Check if upgrade from one version to another requires migration
    pub fn requires_migration(&self, from: &str, to: &str) -> bool {
        let from_parts = parse_version(from).unwrap_or((0, 0, 0));
        let to_parts = parse_version(to).unwrap_or((0, 0, 0));

        // Major version change requires migration
        from_parts.0 != to_parts.0
    }
}

/// Workflow version compatibility checker
#[derive(Debug)]
pub struct VersionCompatibility {
    /// Source version
    pub from_version: String,

    /// Target version
    pub to_version: String,

    /// Whether versions are compatible
    pub compatible: bool,

    /// Whether migration is required
    pub requires_migration: bool,

    /// Compatibility issues
    pub issues: Vec<String>,

    /// Breaking changes
    pub breaking_changes: Vec<String>,
}

impl VersionCompatibility {
    /// Check compatibility between two versions
    pub fn check(from: &str, to: &str, history: &WorkflowVersionHistory) -> Self {
        let from_parts = parse_version(from).unwrap_or((0, 0, 0));
        let to_parts = parse_version(to).unwrap_or((0, 0, 0));

        let mut issues = Vec::new();
        let mut breaking_changes = Vec::new();

        // Check major version difference
        let major_diff = to_parts.0 as i32 - from_parts.0 as i32;
        let requires_migration = major_diff != 0;

        // Downgrading major version is not compatible
        let compatible = if major_diff < 0 {
            issues.push(format!(
                "Downgrading major version from {} to {} is not supported",
                from, to
            ));
            false
        } else if major_diff > 1 {
            issues.push(format!(
                "Skipping major versions (from {} to {}) may have issues",
                from, to
            ));
            true
        } else {
            true
        };

        // Collect breaking changes
        for entry in history.breaking_changes_since(from) {
            breaking_changes.push(entry.description.clone());
        }

        Self {
            from_version: from.to_string(),
            to_version: to.to_string(),
            compatible,
            requires_migration,
            issues,
            breaking_changes,
        }
    }
}

/// Workflow diff between two versions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WorkflowDiff {
    /// Source version
    pub from_version: String,

    /// Target version
    pub to_version: String,

    /// Nodes added
    #[serde(default)]
    pub nodes_added: Vec<String>,

    /// Nodes removed
    #[serde(default)]
    pub nodes_removed: Vec<String>,

    /// Nodes modified
    #[serde(default)]
    pub nodes_modified: Vec<NodeChange>,

    /// Edges added
    #[serde(default)]
    pub edges_added: Vec<EdgeInfo>,

    /// Edges removed
    #[serde(default)]
    pub edges_removed: Vec<EdgeInfo>,

    /// Metadata changes
    #[serde(default)]
    pub metadata_changes: Vec<MetadataChange>,
}

/// Node change detail
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct NodeChange {
    /// Node ID
    pub node_id: String,

    /// Node name
    pub node_name: String,

    /// Fields that changed
    pub changes: Vec<String>,
}

/// Edge information for diff
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EdgeInfo {
    /// Source node ID
    pub from: String,

    /// Target node ID
    pub to: String,
}

/// Metadata change detail
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct MetadataChange {
    /// Field name
    pub field: String,

    /// Old value
    pub old_value: Option<String>,

    /// New value
    pub new_value: Option<String>,
}

impl WorkflowDiff {
    /// Generate a diff between two workflows
    pub fn generate(from: &Workflow, to: &Workflow) -> Self {
        let mut diff = Self {
            from_version: from.metadata.version.clone(),
            to_version: to.metadata.version.clone(),
            nodes_added: Vec::new(),
            nodes_removed: Vec::new(),
            nodes_modified: Vec::new(),
            edges_added: Vec::new(),
            edges_removed: Vec::new(),
            metadata_changes: Vec::new(),
        };

        // Compare nodes
        let from_node_ids: HashMap<_, _> = from.nodes.iter().map(|n| (n.id, n)).collect();
        let to_node_ids: HashMap<_, _> = to.nodes.iter().map(|n| (n.id, n)).collect();

        // Find added nodes
        for (id, node) in &to_node_ids {
            if !from_node_ids.contains_key(id) {
                diff.nodes_added.push(node.name.clone());
            }
        }

        // Find removed nodes
        for (id, node) in &from_node_ids {
            if !to_node_ids.contains_key(id) {
                diff.nodes_removed.push(node.name.clone());
            }
        }

        // Find modified nodes
        for (id, from_node) in &from_node_ids {
            if let Some(to_node) = to_node_ids.get(id) {
                let mut changes = Vec::new();

                if from_node.name != to_node.name {
                    changes.push(format!("name: '{}' -> '{}'", from_node.name, to_node.name));
                }

                if format!("{:?}", from_node.kind) != format!("{:?}", to_node.kind) {
                    changes.push(format!(
                        "kind: '{:?}' -> '{:?}'",
                        from_node.kind, to_node.kind
                    ));
                }

                if !changes.is_empty() {
                    diff.nodes_modified.push(NodeChange {
                        node_id: id.to_string(),
                        node_name: to_node.name.clone(),
                        changes,
                    });
                }
            }
        }

        // Compare edges
        let from_edges: Vec<_> = from
            .edges
            .iter()
            .map(|e| (e.from.to_string(), e.to.to_string()))
            .collect();
        let to_edges: Vec<_> = to
            .edges
            .iter()
            .map(|e| (e.from.to_string(), e.to.to_string()))
            .collect();

        for (from_id, to_id) in &to_edges {
            if !from_edges.contains(&(from_id.clone(), to_id.clone())) {
                diff.edges_added.push(EdgeInfo {
                    from: from_id.clone(),
                    to: to_id.clone(),
                });
            }
        }

        for (from_id, to_id) in &from_edges {
            if !to_edges.contains(&(from_id.clone(), to_id.clone())) {
                diff.edges_removed.push(EdgeInfo {
                    from: from_id.clone(),
                    to: to_id.clone(),
                });
            }
        }

        // Compare metadata
        if from.metadata.name != to.metadata.name {
            diff.metadata_changes.push(MetadataChange {
                field: "name".to_string(),
                old_value: Some(from.metadata.name.clone()),
                new_value: Some(to.metadata.name.clone()),
            });
        }

        if from.metadata.description != to.metadata.description {
            diff.metadata_changes.push(MetadataChange {
                field: "description".to_string(),
                old_value: from.metadata.description.clone(),
                new_value: to.metadata.description.clone(),
            });
        }

        diff
    }

    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        !self.nodes_added.is_empty()
            || !self.nodes_removed.is_empty()
            || !self.nodes_modified.is_empty()
            || !self.edges_added.is_empty()
            || !self.edges_removed.is_empty()
            || !self.metadata_changes.is_empty()
    }

    /// Generate a human-readable summary
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!(
            "Diff from version {} to {}",
            self.from_version, self.to_version
        ));

        if !self.nodes_added.is_empty() {
            lines.push(format!(
                "Added {} nodes: {:?}",
                self.nodes_added.len(),
                self.nodes_added
            ));
        }

        if !self.nodes_removed.is_empty() {
            lines.push(format!(
                "Removed {} nodes: {:?}",
                self.nodes_removed.len(),
                self.nodes_removed
            ));
        }

        if !self.nodes_modified.is_empty() {
            lines.push(format!("Modified {} nodes", self.nodes_modified.len()));
        }

        if !self.edges_added.is_empty() {
            lines.push(format!("Added {} edges", self.edges_added.len()));
        }

        if !self.edges_removed.is_empty() {
            lines.push(format!("Removed {} edges", self.edges_removed.len()));
        }

        if !self.metadata_changes.is_empty() {
            lines.push(format!(
                "Changed {} metadata fields",
                self.metadata_changes.len()
            ));
        }

        if !self.has_changes() {
            lines.push("No changes detected".to_string());
        }

        lines.join("\n")
    }
}

/// Parse semantic version string into (major, minor, patch)
fn parse_version(version: &str) -> Result<(u32, u32, u32), String> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid version format: {}", version));
    }

    let major = parts[0]
        .parse::<u32>()
        .map_err(|_| format!("Invalid major version: {}", parts[0]))?;
    let minor = parts[1]
        .parse::<u32>()
        .map_err(|_| format!("Invalid minor version: {}", parts[1]))?;
    let patch = parts[2]
        .parse::<u32>()
        .map_err(|_| format!("Invalid patch version: {}", parts[2]))?;

    Ok((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Node, NodeKind};

    #[test]
    fn test_version_history_creation() {
        let history = WorkflowVersionHistory::new("My Workflow".to_string());
        assert_eq!(history.workflow_name, "My Workflow");
        assert!(history.versions.is_empty());
        assert!(history.aliases.is_empty());
    }

    #[test]
    fn test_add_version_to_history() {
        let mut history = WorkflowVersionHistory::new("My Workflow".to_string());

        let entry = WorkflowVersionEntry {
            version: "1.0.0".to_string(),
            workflow_id: uuid::Uuid::new_v4(),
            parent_id: None,
            author: "Alice".to_string(),
            created_at: Utc::now(),
            change_description: "Initial version".to_string(),
            change_type: ChangeType::Major,
            tags: vec!["stable".to_string()],
            published: true,
            changelog: vec![],
        };

        history.add_version(entry);
        assert_eq!(history.versions.len(), 1);
        assert_eq!(history.latest_version().unwrap().version, "1.0.0");
    }

    #[test]
    fn test_version_sorting() {
        let mut history = WorkflowVersionHistory::new("My Workflow".to_string());

        // Add versions in random order
        for version in ["1.2.0", "1.0.0", "2.0.0", "1.1.0"] {
            let entry = WorkflowVersionEntry {
                version: version.to_string(),
                workflow_id: uuid::Uuid::new_v4(),
                parent_id: None,
                author: "Alice".to_string(),
                created_at: Utc::now(),
                change_description: "Test".to_string(),
                change_type: ChangeType::Minor,
                tags: vec![],
                published: true,
                changelog: vec![],
            };
            history.add_version(entry);
        }

        // Should be sorted
        assert_eq!(history.versions[0].version, "1.0.0");
        assert_eq!(history.versions[1].version, "1.1.0");
        assert_eq!(history.versions[2].version, "1.2.0");
        assert_eq!(history.versions[3].version, "2.0.0");
    }

    #[test]
    fn test_version_aliases() {
        let mut history = WorkflowVersionHistory::new("My Workflow".to_string());

        let entry = WorkflowVersionEntry {
            version: "1.0.0".to_string(),
            workflow_id: uuid::Uuid::new_v4(),
            parent_id: None,
            author: "Alice".to_string(),
            created_at: Utc::now(),
            change_description: "Initial".to_string(),
            change_type: ChangeType::Major,
            tags: vec![],
            published: true,
            changelog: vec![],
        };
        history.add_version(entry);

        history.set_alias("stable".to_string(), "1.0.0".to_string());

        let version = history.get_version("stable");
        assert!(version.is_some());
        assert_eq!(version.unwrap().version, "1.0.0");
    }

    #[test]
    fn test_version_compatibility_check() {
        let history = WorkflowVersionHistory::new("My Workflow".to_string());

        let compat = VersionCompatibility::check("1.0.0", "1.1.0", &history);
        assert!(compat.compatible);
        assert!(!compat.requires_migration);

        let compat = VersionCompatibility::check("1.0.0", "2.0.0", &history);
        assert!(compat.compatible);
        assert!(compat.requires_migration);

        let compat = VersionCompatibility::check("2.0.0", "1.0.0", &history);
        assert!(!compat.compatible);
    }

    #[test]
    fn test_workflow_diff_generation() {
        let mut workflow_v1 = Workflow::new("Test Workflow".to_string());
        workflow_v1.metadata.version = "1.0.0".to_string();

        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start_node.id;
        workflow_v1.add_node(start_node);

        let end_node = Node::new("End".to_string(), NodeKind::End);
        let end_id = end_node.id;
        workflow_v1.add_node(end_node);

        workflow_v1.add_edge(Edge::new(start_id, end_id));

        // Create v2 with an additional node
        let mut workflow_v2 = workflow_v1.clone();
        workflow_v2.metadata.version = "1.1.0".to_string();

        let process_node = Node::new("Process".to_string(), NodeKind::Start);
        let process_id = process_node.id;
        workflow_v2.add_node(process_node);

        workflow_v2.add_edge(Edge::new(start_id, process_id));
        workflow_v2.add_edge(Edge::new(process_id, end_id));

        // Generate diff
        let diff = WorkflowDiff::generate(&workflow_v1, &workflow_v2);

        assert_eq!(diff.nodes_added.len(), 1);
        assert_eq!(diff.nodes_added[0], "Process");
        assert_eq!(diff.edges_added.len(), 2);
        assert!(diff.has_changes());
    }

    #[test]
    fn test_workflow_diff_no_changes() {
        let mut workflow = Workflow::new("Test Workflow".to_string());
        workflow.metadata.version = "1.0.0".to_string();

        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        workflow.add_node(start_node);

        let diff = WorkflowDiff::generate(&workflow, &workflow);
        assert!(!diff.has_changes());
    }

    #[test]
    fn test_breaking_changes_detection() {
        let mut history = WorkflowVersionHistory::new("My Workflow".to_string());

        let entry1 = WorkflowVersionEntry {
            version: "1.0.0".to_string(),
            workflow_id: uuid::Uuid::new_v4(),
            parent_id: None,
            author: "Alice".to_string(),
            created_at: Utc::now(),
            change_description: "Initial".to_string(),
            change_type: ChangeType::Major,
            tags: vec![],
            published: true,
            changelog: vec![],
        };
        history.add_version(entry1);

        let entry2 = WorkflowVersionEntry {
            version: "2.0.0".to_string(),
            workflow_id: uuid::Uuid::new_v4(),
            parent_id: None,
            author: "Alice".to_string(),
            created_at: Utc::now(),
            change_description: "Breaking change".to_string(),
            change_type: ChangeType::Major,
            tags: vec![],
            published: true,
            changelog: vec![ChangelogEntry {
                entry_type: ChangelogType::Removed,
                description: "Removed old API".to_string(),
                affected_nodes: vec!["node1".to_string()],
                breaking: true,
            }],
        };
        history.add_version(entry2);

        let breaking = history.breaking_changes_since("1.0.0");
        assert_eq!(breaking.len(), 1);
        assert_eq!(breaking[0].description, "Removed old API");
    }

    #[test]
    fn test_get_history_between_versions() {
        let mut history = WorkflowVersionHistory::new("My Workflow".to_string());

        for i in 0..5 {
            let entry = WorkflowVersionEntry {
                version: format!("1.{}.0", i),
                workflow_id: uuid::Uuid::new_v4(),
                parent_id: None,
                author: "Alice".to_string(),
                created_at: Utc::now(),
                change_description: format!("Version {}", i),
                change_type: ChangeType::Minor,
                tags: vec![],
                published: true,
                changelog: vec![],
            };
            history.add_version(entry);
        }

        let between = history.get_history_between("1.0.0", "1.3.0");
        assert_eq!(between.len(), 3);
        assert_eq!(between[0].version, "1.1.0");
        assert_eq!(between[1].version, "1.2.0");
        assert_eq!(between[2].version, "1.3.0");
    }

    #[test]
    fn test_diff_summary() {
        let mut workflow_v1 = Workflow::new("Test".to_string());
        workflow_v1.metadata.version = "1.0.0".to_string();

        let mut workflow_v2 = workflow_v1.clone();
        workflow_v2.metadata.version = "2.0.0".to_string();

        let diff = WorkflowDiff::generate(&workflow_v1, &workflow_v2);
        let summary = diff.summary();

        assert!(summary.contains("1.0.0"));
        assert!(summary.contains("2.0.0"));
    }
}
