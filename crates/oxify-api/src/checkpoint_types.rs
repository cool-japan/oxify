//! API types for checkpoint/pause/resume operations

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Request to pause an execution
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PauseExecutionRequest {
    /// Reason for pause (optional)
    pub reason: Option<String>,
}

/// Response after pausing an execution
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PauseExecutionResponse {
    /// Checkpoint ID created
    pub checkpoint_id: Uuid,
    /// Success message
    pub message: String,
}

/// Response after resuming an execution
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ResumeExecutionResponse {
    /// Execution ID
    pub execution_id: Uuid,
    /// Success message
    pub message: String,
}

/// Checkpoint summary
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CheckpointSummary {
    /// Checkpoint ID
    pub id: Uuid,
    /// Workflow ID
    pub workflow_id: Uuid,
    /// Execution ID
    pub execution_id: Uuid,
    /// Number of completed nodes
    pub completed_nodes_count: usize,
    /// Current execution level
    pub current_level: usize,
    /// Whether paused
    pub paused: bool,
    /// Reason for checkpoint
    pub reason: String,
    /// When created
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Response containing list of checkpoints
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListCheckpointsResponse {
    /// List of checkpoints
    pub checkpoints: Vec<CheckpointSummary>,
    /// Total count
    pub total: usize,
}

/// Response after deleting checkpoints
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeleteCheckpointsResponse {
    /// Number of checkpoints deleted
    pub deleted_count: u64,
    /// Success message
    pub message: String,
}
