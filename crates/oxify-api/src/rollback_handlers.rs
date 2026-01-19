//! Rollback API handlers for execution state management
//!
//! This module provides API endpoints for managing execution snapshots
//! and performing rollback operations.

use crate::handlers::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use oxify_model::rollback::{ExecutionSnapshot, RollbackManager, RollbackResult, RollbackSummary};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use utoipa::ToSchema;
use uuid::Uuid;

// ============================================================================
// Types
// ============================================================================

/// Request to create a snapshot
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSnapshotRequest {
    /// Optional label for the snapshot
    pub label: Option<String>,
    /// Optional reason for the snapshot
    pub reason: Option<String>,
}

/// Response after creating a snapshot
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateSnapshotResponse {
    /// ID of the created snapshot
    #[schema(value_type = String)]
    pub snapshot_id: Uuid,
    /// Message
    pub message: String,
}

/// List snapshots response
#[derive(Debug, Serialize, ToSchema)]
pub struct ListSnapshotsResponse {
    /// List of snapshot summaries
    pub snapshots: Vec<SnapshotSummary>,
    /// Total count
    pub total: usize,
}

/// Summary of a snapshot
#[derive(Debug, Serialize, ToSchema)]
pub struct SnapshotSummary {
    /// Snapshot ID
    #[schema(value_type = String)]
    pub id: Uuid,
    /// Creation timestamp
    pub created_at: String,
    /// Label
    pub label: Option<String>,
    /// Number of variables
    pub variable_count: usize,
    /// Number of completed nodes
    pub node_count: usize,
    /// Whether this is automatic
    pub is_auto: bool,
}

impl From<&ExecutionSnapshot> for SnapshotSummary {
    fn from(snapshot: &ExecutionSnapshot) -> Self {
        Self {
            id: snapshot.id,
            created_at: snapshot.created_at.to_rfc3339(),
            label: snapshot.label.clone(),
            variable_count: snapshot.variable_count(),
            node_count: snapshot.completed_node_count(),
            is_auto: snapshot.metadata.is_auto,
        }
    }
}

/// Request to rollback execution
#[derive(Debug, Deserialize, ToSchema)]
pub struct RollbackRequest {
    /// Snapshot ID to rollback to (if not provided, rollbacks to most recent)
    #[schema(value_type = Option<String>)]
    pub snapshot_id: Option<Uuid>,
    /// Number of steps to rollback (alternative to snapshot_id)
    pub steps: Option<usize>,
}

/// Response after rollback
#[derive(Debug, Serialize, ToSchema)]
pub struct RollbackResponse {
    /// Whether rollback was successful
    pub success: bool,
    /// Snapshot that was applied
    #[schema(value_type = Option<String>)]
    pub applied_snapshot_id: Option<Uuid>,
    /// Nodes removed
    pub nodes_removed: usize,
    /// Variables changed
    pub variables_changed: usize,
    /// Error message if failed
    pub error: Option<String>,
}

impl From<RollbackResult> for RollbackResponse {
    fn from(result: RollbackResult) -> Self {
        Self {
            success: result.success,
            applied_snapshot_id: result.applied_snapshot_id,
            nodes_removed: result.nodes_removed,
            variables_changed: result.variables_changed,
            error: result.error,
        }
    }
}

/// Rollback summary response
#[derive(Debug, Serialize, ToSchema)]
pub struct GetRollbackSummaryResponse {
    /// Total snapshots
    pub total_snapshots: usize,
    /// Max snapshots allowed
    pub max_snapshots: usize,
    /// Auto snapshot enabled
    pub auto_snapshot_enabled: bool,
    /// Auto snapshot interval
    pub auto_snapshot_interval: usize,
    /// Oldest snapshot time
    pub oldest_snapshot: Option<String>,
    /// Newest snapshot time
    pub newest_snapshot: Option<String>,
    /// Nodes processed
    pub nodes_processed: usize,
}

impl From<RollbackSummary> for GetRollbackSummaryResponse {
    fn from(summary: RollbackSummary) -> Self {
        Self {
            total_snapshots: summary.total_snapshots,
            max_snapshots: summary.max_snapshots,
            auto_snapshot_enabled: summary.auto_snapshot_enabled,
            auto_snapshot_interval: summary.auto_snapshot_interval,
            oldest_snapshot: summary.oldest_snapshot.map(|t| t.to_rfc3339()),
            newest_snapshot: summary.newest_snapshot.map(|t| t.to_rfc3339()),
            nodes_processed: summary.nodes_processed,
        }
    }
}

// ============================================================================
// State
// ============================================================================

/// Rollback state manager - stores rollback managers per execution
pub struct RollbackState {
    managers: RwLock<HashMap<Uuid, RollbackManager>>,
    default_max_snapshots: usize,
}

impl RollbackState {
    /// Create a new rollback state
    pub fn new(default_max_snapshots: usize) -> Self {
        Self {
            managers: RwLock::new(HashMap::new()),
            default_max_snapshots,
        }
    }

    /// Get or create a rollback manager for an execution
    pub async fn get_or_create(&self, execution_id: Uuid) -> RollbackManager {
        let mut managers = self.managers.write().await;
        managers
            .entry(execution_id)
            .or_insert_with(|| RollbackManager::new(self.default_max_snapshots))
            .clone()
    }

    /// Update the manager for an execution
    pub async fn update(&self, execution_id: Uuid, manager: RollbackManager) {
        let mut managers = self.managers.write().await;
        managers.insert(execution_id, manager);
    }

    /// Remove a manager
    #[allow(dead_code)]
    pub async fn remove(&self, execution_id: &Uuid) -> Option<RollbackManager> {
        let mut managers = self.managers.write().await;
        managers.remove(execution_id)
    }

    /// Check if manager exists
    #[allow(dead_code)]
    pub async fn exists(&self, execution_id: &Uuid) -> bool {
        let managers = self.managers.read().await;
        managers.contains_key(execution_id)
    }
}

impl Default for RollbackState {
    fn default() -> Self {
        Self::new(20) // Keep 20 snapshots by default
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// Create a snapshot for an execution
#[utoipa::path(
    post,
    path = "/api/v1/executions/{execution_id}/snapshots",
    request_body = CreateSnapshotRequest,
    params(
        ("execution_id" = String, Path, description = "Execution ID")
    ),
    responses(
        (status = 201, description = "Snapshot created", body = CreateSnapshotResponse),
        (status = 404, description = "Execution not found", body = ErrorResponse)
    ),
    tag = "Rollback"
)]
pub async fn create_snapshot(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<Uuid>,
    Json(req): Json<CreateSnapshotRequest>,
) -> Result<(StatusCode, Json<CreateSnapshotResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Creating snapshot for execution: {}", execution_id);

    // Get execution context
    let ctx = match state.execution_store.get(&execution_id).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", execution_id),
                }),
            ))
        }
        Err(e) => {
            error!("Failed to get execution: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get execution: {}", e),
                }),
            ));
        }
    };

    // Get or create rollback manager
    let rollback_state = get_rollback_state();
    let mut manager = rollback_state.get_or_create(execution_id).await;

    // Create snapshot
    let mut snapshot = ExecutionSnapshot::from_context(&ctx);
    if let Some(label) = req.label {
        snapshot = snapshot.with_label(label);
    }
    if let Some(reason) = req.reason {
        snapshot = snapshot.with_reason(reason);
    }

    let snapshot_id = snapshot.id;
    manager.push_snapshot(snapshot);

    // Save manager
    rollback_state.update(execution_id, manager).await;

    Ok((
        StatusCode::CREATED,
        Json(CreateSnapshotResponse {
            snapshot_id,
            message: "Snapshot created successfully".to_string(),
        }),
    ))
}

/// List snapshots for an execution
#[utoipa::path(
    get,
    path = "/api/v1/executions/{execution_id}/snapshots",
    params(
        ("execution_id" = String, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "List of snapshots", body = ListSnapshotsResponse),
        (status = 404, description = "Execution not found", body = ErrorResponse)
    ),
    tag = "Rollback"
)]
pub async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ListSnapshotsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing snapshots for execution: {}", execution_id);

    // Verify execution exists
    match state.execution_store.get(&execution_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", execution_id),
                }),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get execution: {}", e),
                }),
            ));
        }
    };

    let rollback_state = get_rollback_state();
    let manager = rollback_state.get_or_create(execution_id).await;

    let snapshots: Vec<SnapshotSummary> = manager
        .list_snapshots()
        .into_iter()
        .map(SnapshotSummary::from)
        .collect();
    let total = snapshots.len();

    Ok(Json(ListSnapshotsResponse { snapshots, total }))
}

/// Rollback an execution
#[utoipa::path(
    post,
    path = "/api/v1/executions/{execution_id}/rollback",
    request_body = RollbackRequest,
    params(
        ("execution_id" = String, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Rollback result", body = RollbackResponse),
        (status = 404, description = "Execution not found", body = ErrorResponse),
        (status = 400, description = "Invalid rollback request", body = ErrorResponse)
    ),
    tag = "Rollback"
)]
pub async fn rollback_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<Uuid>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<RollbackResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Rolling back execution: {}", execution_id);

    // Get execution context
    let mut ctx = match state.execution_store.get(&execution_id).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", execution_id),
                }),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get execution: {}", e),
                }),
            ));
        }
    };

    let rollback_state = get_rollback_state();
    let mut manager = rollback_state.get_or_create(execution_id).await;

    // Perform rollback
    let result = if let Some(snapshot_id) = req.snapshot_id {
        manager.rollback_to(&mut ctx, snapshot_id)
    } else if let Some(steps) = req.steps {
        manager.rollback_n(&mut ctx, steps)
    } else {
        manager.rollback(&mut ctx)
    };

    if result.success {
        // Update execution context
        if let Err(e) = state.execution_store.update(&execution_id, ctx).await {
            error!("Failed to update execution after rollback: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to update execution: {}", e),
                }),
            ));
        }
    }

    // Save manager
    rollback_state.update(execution_id, manager).await;

    Ok(Json(RollbackResponse::from(result)))
}

/// Get rollback summary for an execution
#[utoipa::path(
    get,
    path = "/api/v1/executions/{execution_id}/rollback/summary",
    params(
        ("execution_id" = String, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Rollback summary", body = GetRollbackSummaryResponse),
        (status = 404, description = "Execution not found", body = ErrorResponse)
    ),
    tag = "Rollback"
)]
pub async fn get_rollback_summary(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<GetRollbackSummaryResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting rollback summary for execution: {}", execution_id);

    // Verify execution exists
    match state.execution_store.get(&execution_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", execution_id),
                }),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get execution: {}", e),
                }),
            ));
        }
    };

    let rollback_state = get_rollback_state();
    let manager = rollback_state.get_or_create(execution_id).await;
    let summary = manager.summary();

    Ok(Json(GetRollbackSummaryResponse::from(summary)))
}

/// Clear all snapshots for an execution
#[utoipa::path(
    delete,
    path = "/api/v1/executions/{execution_id}/snapshots",
    params(
        ("execution_id" = String, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Snapshots cleared"),
        (status = 404, description = "Execution not found", body = ErrorResponse)
    ),
    tag = "Rollback"
)]
pub async fn clear_snapshots(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    info!("Clearing snapshots for execution: {}", execution_id);

    // Verify execution exists
    match state.execution_store.get(&execution_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", execution_id),
                }),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get execution: {}", e),
                }),
            ));
        }
    };

    let rollback_state = get_rollback_state();
    let mut manager = rollback_state.get_or_create(execution_id).await;
    manager.clear();
    rollback_state.update(execution_id, manager).await;

    Ok(StatusCode::OK)
}

// ============================================================================
// Global State
// ============================================================================

// Global rollback state using LazyLock for thread-safe initialization
static ROLLBACK_STATE: std::sync::LazyLock<RollbackState> =
    std::sync::LazyLock::new(RollbackState::default);

fn get_rollback_state() -> &'static RollbackState {
    &ROLLBACK_STATE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_summary_from() {
        let ctx = oxify_model::ExecutionContext::new(Uuid::new_v4());
        let snapshot = ExecutionSnapshot::from_context(&ctx).with_label("test");
        let summary = SnapshotSummary::from(&snapshot);

        assert_eq!(summary.id, snapshot.id);
        assert_eq!(summary.label, Some("test".to_string()));
    }

    #[test]
    fn test_rollback_response_from() {
        let result = RollbackResult::success(Uuid::new_v4(), 3, 2);
        let response = RollbackResponse::from(result);

        assert!(response.success);
        assert_eq!(response.nodes_removed, 3);
        assert_eq!(response.variables_changed, 2);
    }

    #[tokio::test]
    async fn test_rollback_state_create() {
        let state = RollbackState::new(10);
        let manager = state.get_or_create(Uuid::new_v4()).await;
        assert_eq!(manager.snapshot_count(), 0);
    }

    #[tokio::test]
    async fn test_rollback_state_update() {
        let state = RollbackState::new(10);
        let exec_id = Uuid::new_v4();

        let mut manager = state.get_or_create(exec_id).await;
        let ctx = oxify_model::ExecutionContext::new(Uuid::new_v4());
        manager.create_snapshot(&ctx);
        state.update(exec_id, manager).await;

        let retrieved = state.get_or_create(exec_id).await;
        assert_eq!(retrieved.snapshot_count(), 1);
    }
}
