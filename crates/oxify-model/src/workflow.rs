use crate::{Edge, Node, NodeId, NodeKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Unique identifier for a workflow
pub type WorkflowId = Uuid;

/// Metadata about a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WorkflowMetadata {
    /// Unique workflow identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: WorkflowId,

    /// Display name
    pub name: String,

    /// Description of what this workflow does
    pub description: Option<String>,

    /// Version string
    pub version: String,

    /// When the workflow was created
    pub created_at: DateTime<Utc>,

    /// When the workflow was last modified
    pub updated_at: DateTime<Utc>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Parent workflow ID (for versioning)
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    #[serde(default)]
    pub parent_id: Option<WorkflowId>,

    /// Change description for this version
    #[serde(default)]
    pub change_description: Option<String>,

    /// Scheduling configuration (cron-like)
    #[serde(default)]
    pub schedule: Option<WorkflowSchedule>,
}

impl WorkflowMetadata {
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            version: "0.1.0".to_string(),
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
            parent_id: None,
            change_description: None,
            schedule: None,
        }
    }

    /// Parse semantic version (major.minor.patch)
    pub fn parse_version(&self) -> Result<(u32, u32, u32), String> {
        let parts: Vec<&str> = self.version.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid version format: {}", self.version));
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

    /// Increment major version (breaking changes)
    pub fn bump_major(&mut self) {
        if let Ok((major, _, _)) = self.parse_version() {
            self.version = format!("{}.0.0", major + 1);
            self.updated_at = Utc::now();
        }
    }

    /// Increment minor version (new features)
    pub fn bump_minor(&mut self) {
        if let Ok((major, minor, _)) = self.parse_version() {
            self.version = format!("{}.{}.0", major, minor + 1);
            self.updated_at = Utc::now();
        }
    }

    /// Increment patch version (bug fixes)
    pub fn bump_patch(&mut self) {
        if let Ok((major, minor, patch)) = self.parse_version() {
            self.version = format!("{}.{}.{}", major, minor, patch + 1);
            self.updated_at = Utc::now();
        }
    }
}

/// Complete workflow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Workflow {
    /// Workflow metadata
    pub metadata: WorkflowMetadata,

    /// Nodes in the workflow
    pub nodes: Vec<Node>,

    /// Edges connecting the nodes
    pub edges: Vec<Edge>,
}

impl Workflow {
    pub fn new(name: String) -> Self {
        Self {
            metadata: WorkflowMetadata::new(name),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Add a node to the workflow
    pub fn add_node(&mut self, node: Node) {
        self.nodes.push(node);
    }

    /// Add an edge to the workflow
    pub fn add_edge(&mut self, edge: Edge) {
        self.edges.push(edge);
    }

    /// Find a node by its ID
    pub fn get_node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| &n.id == id)
    }

    /// Find a mutable node by its ID
    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| &n.id == id)
    }

    /// Find all nodes of a specific kind
    pub fn find_nodes_by_kind(&self, kind: &NodeKind) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|n| std::mem::discriminant(&n.kind) == std::mem::discriminant(kind))
            .collect()
    }

    /// Get the start node of the workflow
    pub fn get_start_node(&self) -> Option<&Node> {
        self.nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Start))
    }

    /// Get all end nodes of the workflow
    pub fn get_end_nodes(&self) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::End))
            .collect()
    }

    /// Remove a node and all its associated edges
    pub fn remove_node(&mut self, id: &NodeId) -> bool {
        let node_existed = self.nodes.iter().any(|n| &n.id == id);
        if node_existed {
            self.nodes.retain(|n| &n.id != id);
            self.edges.retain(|e| &e.from != id && &e.to != id);
        }
        node_existed
    }

    /// Remove an edge between two nodes
    pub fn remove_edge(&mut self, from: &NodeId, to: &NodeId) -> bool {
        let edge_count = self.edges.len();
        self.edges.retain(|e| &e.from != from || &e.to != to);
        self.edges.len() < edge_count
    }

    /// Get the number of nodes in the workflow
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of edges in the workflow
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Get all outgoing edges from a node
    pub fn get_outgoing_edges(&self, node_id: &NodeId) -> Vec<&Edge> {
        self.edges.iter().filter(|e| &e.from == node_id).collect()
    }

    /// Get all incoming edges to a node
    pub fn get_incoming_edges(&self, node_id: &NodeId) -> Vec<&Edge> {
        self.edges.iter().filter(|e| &e.to == node_id).collect()
    }

    /// Validate the workflow structure with comprehensive checks
    pub fn validate(&self) -> Result<(), String> {
        use crate::validation::WorkflowValidator;

        match WorkflowValidator::validate(self) {
            Ok(_report) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Export workflow to JSON string
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("JSON serialization error: {}", e))
    }

    /// Export workflow to JSON file
    pub fn to_json_file(&self, path: &str) -> Result<(), String> {
        let json = self.to_json()?;
        std::fs::write(path, json).map_err(|e| format!("File write error: {}", e))
    }

    /// Import workflow from JSON string
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("JSON deserialization error: {}", e))
    }

    /// Import workflow from JSON file
    pub fn from_json_file(path: &str) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| format!("File read error: {}", e))?;
        Self::from_json(&json)
    }

    /// Export workflow to YAML string
    pub fn to_yaml(&self) -> Result<String, String> {
        serde_yaml::to_string(self).map_err(|e| format!("YAML serialization error: {}", e))
    }

    /// Export workflow to YAML file
    pub fn to_yaml_file(&self, path: &str) -> Result<(), String> {
        let yaml = self.to_yaml()?;
        std::fs::write(path, yaml).map_err(|e| format!("File write error: {}", e))
    }

    /// Import workflow from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml).map_err(|e| format!("YAML deserialization error: {}", e))
    }

    /// Import workflow from YAML file
    pub fn from_yaml_file(path: &str) -> Result<Self, String> {
        let yaml = std::fs::read_to_string(path).map_err(|e| format!("File read error: {}", e))?;
        Self::from_yaml(&yaml)
    }

    /// Create a new version of this workflow
    pub fn create_new_version(
        &self,
        change_description: String,
        version_type: VersionBump,
    ) -> Self {
        let mut new_workflow = self.clone();

        // Update metadata for new version
        new_workflow.metadata.id = Uuid::new_v4();
        new_workflow.metadata.parent_id = Some(self.metadata.id);
        new_workflow.metadata.change_description = Some(change_description);
        new_workflow.metadata.created_at = Utc::now();
        new_workflow.metadata.updated_at = Utc::now();

        // Bump version based on type
        match version_type {
            VersionBump::Major => new_workflow.metadata.bump_major(),
            VersionBump::Minor => new_workflow.metadata.bump_minor(),
            VersionBump::Patch => new_workflow.metadata.bump_patch(),
        }

        new_workflow
    }

    /// Check if this workflow is a newer version of another
    pub fn is_newer_than(&self, other: &Workflow) -> Result<bool, String> {
        let (self_major, self_minor, self_patch) = self.metadata.parse_version()?;
        let (other_major, other_minor, other_patch) = other.metadata.parse_version()?;

        Ok(self_major > other_major
            || (self_major == other_major && self_minor > other_minor)
            || (self_major == other_major && self_minor == other_minor && self_patch > other_patch))
    }

    /// Get version as tuple for comparison
    pub fn version_tuple(&self) -> Result<(u32, u32, u32), String> {
        self.metadata.parse_version()
    }
}

/// Type of version bump
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionBump {
    /// Major version (breaking changes): 1.0.0 -> 2.0.0
    Major,
    /// Minor version (new features): 1.0.0 -> 1.1.0
    Minor,
    /// Patch version (bug fixes): 1.0.0 -> 1.0.1
    Patch,
}

/// Workflow scheduling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WorkflowSchedule {
    /// Cron expression (e.g., "0 0 * * *" for daily at midnight)
    pub cron: String,

    /// Timezone for schedule (e.g., "UTC", "America/New_York")
    #[serde(default = "default_timezone")]
    pub timezone: String,

    /// Whether the schedule is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Maximum number of concurrent runs allowed (None = unlimited)
    #[serde(default)]
    pub max_concurrent_runs: Option<u32>,

    /// Retry failed scheduled runs
    #[serde(default)]
    pub retry_on_failure: bool,

    /// Start date/time (schedule won't run before this)
    #[serde(default)]
    pub start_date: Option<DateTime<Utc>>,

    /// End date/time (schedule won't run after this)
    #[serde(default)]
    pub end_date: Option<DateTime<Utc>>,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_enabled() -> bool {
    true
}

impl WorkflowSchedule {
    /// Create a new schedule with a cron expression
    pub fn new(cron: String) -> Self {
        Self {
            cron,
            timezone: default_timezone(),
            enabled: true,
            max_concurrent_runs: None,
            retry_on_failure: false,
            start_date: None,
            end_date: None,
        }
    }

    /// Set timezone
    pub fn with_timezone(mut self, timezone: String) -> Self {
        self.timezone = timezone;
        self
    }

    /// Enable or disable the schedule
    pub fn set_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set maximum concurrent runs
    pub fn with_max_concurrent_runs(mut self, max: u32) -> Self {
        self.max_concurrent_runs = Some(max);
        self
    }

    /// Set date range
    pub fn with_date_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_date = Some(start);
        self.end_date = Some(end);
        self
    }

    /// Check if schedule is currently valid (within date range)
    pub fn is_valid_now(&self) -> bool {
        if !self.enabled {
            return false;
        }

        let now = Utc::now();

        if let Some(start) = self.start_date {
            if now < start {
                return false;
            }
        }

        if let Some(end) = self.end_date {
            if now > end {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, LlmConfig, Node, NodeKind};
    use chrono::Duration;

    #[test]
    fn test_workflow_validation() {
        let mut workflow = Workflow::new("Test Workflow".to_string());

        // Add start node
        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start_node.id;
        workflow.add_node(start_node);

        // Add end node
        let end_node = Node::new("End".to_string(), NodeKind::End);
        let end_id = end_node.id;
        workflow.add_node(end_node);

        // Add edge
        workflow.add_edge(Edge::new(start_id, end_id));

        assert!(workflow.validate().is_ok());
    }

    #[test]
    fn test_workflow_missing_start_node() {
        let workflow = Workflow::new("Test Workflow".to_string());
        assert!(workflow.validate().is_err());
    }

    #[test]
    fn test_workflow_json_serialization() {
        let mut workflow = Workflow::new("Test Workflow".to_string());

        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start_node.id;
        workflow.add_node(start_node);

        let end_node = Node::new("End".to_string(), NodeKind::End);
        let end_id = end_node.id;
        workflow.add_node(end_node);

        workflow.add_edge(Edge::new(start_id, end_id));

        // Test to_json
        let json = workflow.to_json();
        assert!(json.is_ok());

        // Test from_json
        let restored = Workflow::from_json(&json.unwrap());
        assert!(restored.is_ok());

        let restored_workflow = restored.unwrap();
        assert_eq!(restored_workflow.nodes.len(), 2);
        assert_eq!(restored_workflow.edges.len(), 1);
        assert_eq!(restored_workflow.metadata.name, "Test Workflow");
    }

    #[test]
    fn test_workflow_metadata_new() {
        let metadata = WorkflowMetadata::new("Test Workflow".to_string());

        assert_eq!(metadata.name, "Test Workflow");
        assert_eq!(metadata.version, "0.1.0");
        assert!(metadata.description.is_none());
        assert_eq!(metadata.tags.len(), 0);
        assert!(metadata.parent_id.is_none());
        assert!(metadata.change_description.is_none());
        assert!(metadata.schedule.is_none());
    }

    #[test]
    fn test_workflow_metadata_parse_version() {
        let metadata = WorkflowMetadata::new("Test".to_string());
        let (major, minor, patch) = metadata.parse_version().unwrap();

        assert_eq!(major, 0);
        assert_eq!(minor, 1);
        assert_eq!(patch, 0);
    }

    #[test]
    fn test_workflow_metadata_parse_version_invalid() {
        let mut metadata = WorkflowMetadata::new("Test".to_string());
        metadata.version = "invalid".to_string();

        assert!(metadata.parse_version().is_err());
    }

    #[test]
    fn test_workflow_metadata_bump_major() {
        let mut metadata = WorkflowMetadata::new("Test".to_string());
        metadata.version = "1.2.3".to_string();

        metadata.bump_major();

        assert_eq!(metadata.version, "2.0.0");
    }

    #[test]
    fn test_workflow_metadata_bump_minor() {
        let mut metadata = WorkflowMetadata::new("Test".to_string());
        metadata.version = "1.2.3".to_string();

        metadata.bump_minor();

        assert_eq!(metadata.version, "1.3.0");
    }

    #[test]
    fn test_workflow_metadata_bump_patch() {
        let mut metadata = WorkflowMetadata::new("Test".to_string());
        metadata.version = "1.2.3".to_string();

        metadata.bump_patch();

        assert_eq!(metadata.version, "1.2.4");
    }

    #[test]
    fn test_workflow_new() {
        let workflow = Workflow::new("Test Workflow".to_string());

        assert_eq!(workflow.metadata.name, "Test Workflow");
        assert_eq!(workflow.nodes.len(), 0);
        assert_eq!(workflow.edges.len(), 0);
    }

    #[test]
    fn test_workflow_add_node() {
        let mut workflow = Workflow::new("Test".to_string());
        let node = Node::new("Test Node".to_string(), NodeKind::Start);

        workflow.add_node(node);

        assert_eq!(workflow.nodes.len(), 1);
        assert_eq!(workflow.nodes[0].name, "Test Node");
    }

    #[test]
    fn test_workflow_add_edge() {
        let mut workflow = Workflow::new("Test".to_string());
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let edge = Edge::new(from, to);

        workflow.add_edge(edge);

        assert_eq!(workflow.edges.len(), 1);
        assert_eq!(workflow.edges[0].from, from);
        assert_eq!(workflow.edges[0].to, to);
    }

    #[test]
    fn test_workflow_get_node() {
        let mut workflow = Workflow::new("Test".to_string());
        let node = Node::new("Test Node".to_string(), NodeKind::Start);
        let node_id = node.id;

        workflow.add_node(node);

        let found = workflow.get_node(&node_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Test Node");
    }

    #[test]
    fn test_workflow_get_node_not_found() {
        let workflow = Workflow::new("Test".to_string());
        let node_id = Uuid::new_v4();

        let found = workflow.get_node(&node_id);
        assert!(found.is_none());
    }

    #[test]
    fn test_workflow_get_outgoing_edges() {
        let mut workflow = Workflow::new("Test".to_string());
        let from = Uuid::new_v4();
        let to1 = Uuid::new_v4();
        let to2 = Uuid::new_v4();

        workflow.add_edge(Edge::new(from, to1));
        workflow.add_edge(Edge::new(from, to2));
        workflow.add_edge(Edge::new(to1, to2));

        let outgoing = workflow.get_outgoing_edges(&from);
        assert_eq!(outgoing.len(), 2);
    }

    #[test]
    fn test_workflow_get_incoming_edges() {
        let mut workflow = Workflow::new("Test".to_string());
        let from1 = Uuid::new_v4();
        let from2 = Uuid::new_v4();
        let to = Uuid::new_v4();

        workflow.add_edge(Edge::new(from1, to));
        workflow.add_edge(Edge::new(from2, to));
        workflow.add_edge(Edge::new(from1, from2));

        let incoming = workflow.get_incoming_edges(&to);
        assert_eq!(incoming.len(), 2);
    }

    #[test]
    fn test_workflow_yaml_serialization() {
        let mut workflow = Workflow::new("Test Workflow".to_string());

        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start_node.id;
        workflow.add_node(start_node);

        let end_node = Node::new("End".to_string(), NodeKind::End);
        let end_id = end_node.id;
        workflow.add_node(end_node);

        workflow.add_edge(Edge::new(start_id, end_id));

        // Test to_yaml
        let yaml = workflow.to_yaml();
        assert!(yaml.is_ok());

        // Test from_yaml
        let restored = Workflow::from_yaml(&yaml.unwrap());
        assert!(restored.is_ok());

        let restored_workflow = restored.unwrap();
        assert_eq!(restored_workflow.nodes.len(), 2);
        assert_eq!(restored_workflow.edges.len(), 1);
        assert_eq!(restored_workflow.metadata.name, "Test Workflow");
    }

    #[test]
    fn test_workflow_create_new_version_major() {
        let workflow = Workflow::new("Test".to_string());
        let new_version =
            workflow.create_new_version("Breaking changes".to_string(), VersionBump::Major);

        assert_ne!(new_version.metadata.id, workflow.metadata.id);
        assert_eq!(new_version.metadata.parent_id, Some(workflow.metadata.id));
        assert_eq!(new_version.metadata.version, "1.0.0");
        assert_eq!(
            new_version.metadata.change_description,
            Some("Breaking changes".to_string())
        );
    }

    #[test]
    fn test_workflow_create_new_version_minor() {
        let mut workflow = Workflow::new("Test".to_string());
        workflow.metadata.version = "1.0.0".to_string();

        let new_version =
            workflow.create_new_version("New features".to_string(), VersionBump::Minor);

        assert_eq!(new_version.metadata.version, "1.1.0");
    }

    #[test]
    fn test_workflow_create_new_version_patch() {
        let mut workflow = Workflow::new("Test".to_string());
        workflow.metadata.version = "1.0.0".to_string();

        let new_version = workflow.create_new_version("Bug fixes".to_string(), VersionBump::Patch);

        assert_eq!(new_version.metadata.version, "1.0.1");
    }

    #[test]
    fn test_workflow_is_newer_than() {
        let mut workflow1 = Workflow::new("Test".to_string());
        workflow1.metadata.version = "1.0.0".to_string();

        let mut workflow2 = Workflow::new("Test".to_string());
        workflow2.metadata.version = "2.0.0".to_string();

        assert!(workflow2.is_newer_than(&workflow1).unwrap());
        assert!(!workflow1.is_newer_than(&workflow2).unwrap());
    }

    #[test]
    fn test_workflow_version_tuple() {
        let mut workflow = Workflow::new("Test".to_string());
        workflow.metadata.version = "3.2.1".to_string();

        let (major, minor, patch) = workflow.version_tuple().unwrap();
        assert_eq!(major, 3);
        assert_eq!(minor, 2);
        assert_eq!(patch, 1);
    }

    #[test]
    fn test_workflow_schedule_new() {
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string());

        assert_eq!(schedule.cron, "0 0 * * *");
        assert_eq!(schedule.timezone, "UTC");
        assert!(schedule.enabled);
        assert!(schedule.max_concurrent_runs.is_none());
        assert!(!schedule.retry_on_failure);
        assert!(schedule.start_date.is_none());
        assert!(schedule.end_date.is_none());
    }

    #[test]
    fn test_workflow_schedule_with_timezone() {
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string())
            .with_timezone("America/New_York".to_string());

        assert_eq!(schedule.timezone, "America/New_York");
    }

    #[test]
    fn test_workflow_schedule_set_enabled() {
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string()).set_enabled(false);

        assert!(!schedule.enabled);
    }

    #[test]
    fn test_workflow_schedule_with_max_concurrent_runs() {
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string()).with_max_concurrent_runs(5);

        assert_eq!(schedule.max_concurrent_runs, Some(5));
    }

    #[test]
    fn test_workflow_schedule_with_date_range() {
        let now = Utc::now();
        let future = now + Duration::days(7);
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string()).with_date_range(now, future);

        assert!(schedule.start_date.is_some());
        assert!(schedule.end_date.is_some());
        assert_eq!(schedule.start_date.unwrap(), now);
        assert_eq!(schedule.end_date.unwrap(), future);
    }

    #[test]
    fn test_workflow_schedule_is_valid_now_enabled() {
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string());
        assert!(schedule.is_valid_now());
    }

    #[test]
    fn test_workflow_schedule_is_valid_now_disabled() {
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string()).set_enabled(false);
        assert!(!schedule.is_valid_now());
    }

    #[test]
    fn test_workflow_schedule_is_valid_now_before_start() {
        let future = Utc::now() + Duration::days(1);
        let end = future + Duration::days(7);
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string()).with_date_range(future, end);

        assert!(!schedule.is_valid_now());
    }

    #[test]
    fn test_workflow_schedule_is_valid_now_after_end() {
        let past_start = Utc::now() - Duration::days(7);
        let past_end = Utc::now() - Duration::days(1);
        let schedule =
            WorkflowSchedule::new("0 0 * * *".to_string()).with_date_range(past_start, past_end);

        assert!(!schedule.is_valid_now());
    }

    #[test]
    fn test_workflow_schedule_is_valid_now_within_range() {
        let past = Utc::now() - Duration::days(1);
        let future = Utc::now() + Duration::days(1);
        let schedule = WorkflowSchedule::new("0 0 * * *".to_string()).with_date_range(past, future);

        assert!(schedule.is_valid_now());
    }

    #[test]
    fn test_version_bump_enum() {
        assert_eq!(VersionBump::Major, VersionBump::Major);
        assert_ne!(VersionBump::Major, VersionBump::Minor);
        assert_ne!(VersionBump::Minor, VersionBump::Patch);
    }

    #[test]
    fn test_workflow_get_node_mut() {
        let mut workflow = Workflow::new("test".to_string());
        let node = Node::new("Start".to_string(), NodeKind::Start);
        let node_id = node.id;
        workflow.add_node(node);

        // Get mutable reference and modify
        let node_mut = workflow.get_node_mut(&node_id);
        assert!(node_mut.is_some());
        let node_mut = node_mut.unwrap();
        node_mut.name = "Modified".to_string();

        // Verify modification
        let node = workflow.get_node(&node_id).unwrap();
        assert_eq!(node.name, "Modified");
    }

    #[test]
    fn test_workflow_find_nodes_by_kind() {
        let mut workflow = Workflow::new("test".to_string());
        workflow.add_node(Node::new("Start".to_string(), NodeKind::Start));

        let llm_config = LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: None,
            max_tokens: None,
            tools: vec![],
            images: vec![],
            extra_params: serde_json::json!({}),
        };

        workflow.add_node(Node::new(
            "LLM1".to_string(),
            NodeKind::LLM(llm_config.clone()),
        ));
        workflow.add_node(Node::new(
            "LLM2".to_string(),
            NodeKind::LLM(llm_config.clone()),
        ));
        workflow.add_node(Node::new("End".to_string(), NodeKind::End));

        let llm_nodes = workflow.find_nodes_by_kind(&NodeKind::LLM(llm_config));
        assert_eq!(llm_nodes.len(), 2);

        let start_nodes = workflow.find_nodes_by_kind(&NodeKind::Start);
        assert_eq!(start_nodes.len(), 1);
    }

    #[test]
    fn test_workflow_get_start_node() {
        let mut workflow = Workflow::new("test".to_string());
        assert!(workflow.get_start_node().is_none());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        workflow.add_node(start);

        let start_node = workflow.get_start_node();
        assert!(start_node.is_some());
        assert!(matches!(start_node.unwrap().kind, NodeKind::Start));
    }

    #[test]
    fn test_workflow_get_end_nodes() {
        let mut workflow = Workflow::new("test".to_string());
        assert_eq!(workflow.get_end_nodes().len(), 0);

        workflow.add_node(Node::new("End1".to_string(), NodeKind::End));
        workflow.add_node(Node::new("End2".to_string(), NodeKind::End));

        let end_nodes = workflow.get_end_nodes();
        assert_eq!(end_nodes.len(), 2);
    }

    #[test]
    fn test_workflow_remove_node() {
        let mut workflow = Workflow::new("test".to_string());
        let node1 = Node::new("Start".to_string(), NodeKind::Start);
        let node2 = Node::new("End".to_string(), NodeKind::End);
        let id1 = node1.id;
        let id2 = node2.id;

        workflow.add_node(node1);
        workflow.add_node(node2);
        workflow.add_edge(Edge::new(id1, id2));

        assert_eq!(workflow.nodes.len(), 2);
        assert_eq!(workflow.edges.len(), 1);

        // Remove node1
        let removed = workflow.remove_node(&id1);
        assert!(removed);
        assert_eq!(workflow.nodes.len(), 1);
        assert_eq!(workflow.edges.len(), 0); // Edge should also be removed

        // Try removing non-existent node
        let removed = workflow.remove_node(&id1);
        assert!(!removed);
    }

    #[test]
    fn test_workflow_remove_edge() {
        let mut workflow = Workflow::new("test".to_string());
        let node1 = Node::new("Start".to_string(), NodeKind::Start);
        let node2 = Node::new("End".to_string(), NodeKind::End);
        let id1 = node1.id;
        let id2 = node2.id;

        workflow.add_node(node1);
        workflow.add_node(node2);
        workflow.add_edge(Edge::new(id1, id2));

        assert_eq!(workflow.edges.len(), 1);

        // Remove edge
        let removed = workflow.remove_edge(&id1, &id2);
        assert!(removed);
        assert_eq!(workflow.edges.len(), 0);

        // Try removing non-existent edge
        let removed = workflow.remove_edge(&id1, &id2);
        assert!(!removed);
    }

    #[test]
    fn test_workflow_node_count() {
        let mut workflow = Workflow::new("test".to_string());
        assert_eq!(workflow.node_count(), 0);

        workflow.add_node(Node::new("Start".to_string(), NodeKind::Start));
        assert_eq!(workflow.node_count(), 1);

        workflow.add_node(Node::new("End".to_string(), NodeKind::End));
        assert_eq!(workflow.node_count(), 2);
    }

    #[test]
    fn test_workflow_edge_count() {
        let mut workflow = Workflow::new("test".to_string());
        let node1 = Node::new("Start".to_string(), NodeKind::Start);
        let node2 = Node::new("End".to_string(), NodeKind::End);
        let id1 = node1.id;
        let id2 = node2.id;

        workflow.add_node(node1);
        workflow.add_node(node2);

        assert_eq!(workflow.edge_count(), 0);

        workflow.add_edge(Edge::new(id1, id2));
        assert_eq!(workflow.edge_count(), 1);
    }
}
