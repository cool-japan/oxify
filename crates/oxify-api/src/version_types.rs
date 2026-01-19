//! API types for workflow versioning

use oxify_model::Workflow;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Request to save a new workflow version
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SaveVersionRequest {
    /// Version description/change message
    pub description: Option<String>,
}

/// Response after saving a version
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SaveVersionResponse {
    /// Version record ID
    pub id: Uuid,
    /// Version number
    pub version: i32,
    /// Success message
    pub message: String,
}

/// Response containing version history
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetVersionHistoryResponse {
    /// List of workflow versions
    pub versions: Vec<WorkflowVersionSummary>,
    /// Total number of versions
    pub total: usize,
}

/// Summary of a workflow version (without full workflow definition)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkflowVersionSummary {
    /// Version record ID
    pub id: Uuid,
    /// Workflow ID
    pub workflow_id: Uuid,
    /// Version number
    pub version: i32,
    /// Version description
    pub description: Option<String>,
    /// Workflow name at this version
    pub workflow_name: String,
    /// When created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Who created
    pub created_by: Option<String>,
}

/// Response containing a specific version
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetVersionResponse {
    /// Version record ID
    pub id: Uuid,
    /// Workflow ID
    pub workflow_id: Uuid,
    /// Version number
    pub version: i32,
    /// Version description
    pub description: Option<String>,
    /// Full workflow definition at this version
    pub workflow: Workflow,
    /// When created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Who created
    pub created_by: Option<String>,
}

/// Response containing version comparison
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CompareVersionsResponse {
    /// First version number
    pub version1: i32,
    /// Second version number
    pub version2: i32,
    /// Number of nodes added
    pub nodes_added: i32,
    /// Number of nodes removed
    pub nodes_removed: i32,
    /// Number of edges added
    pub edges_added: i32,
    /// Number of edges removed
    pub edges_removed: i32,
    /// Whether name changed
    pub name_changed: bool,
    /// Whether description changed
    pub description_changed: bool,
}

/// Response after restoring a version
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RestoreVersionResponse {
    /// Success message
    pub message: String,
    /// New version number created from restore
    pub new_version: i32,
}
