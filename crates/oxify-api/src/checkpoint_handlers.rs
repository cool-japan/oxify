//! API handlers for checkpoint/pause/resume operations

use crate::checkpoint_types::*;
use crate::handlers::AppState;
use crate::types::ErrorResponse;
use crate::user_types::ApiUser;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use oxify_storage::ExecutionCheckpoint;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

/// Pause an execution and create a checkpoint
#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/pause",
    params(
        ("id" = Uuid, Path, description = "Execution ID")
    ),
    request_body = PauseExecutionRequest,
    responses(
        (status = 200, description = "Execution paused", body = PauseExecutionResponse),
        (status = 404, description = "Execution not found", body = ErrorResponse),
        (status = 503, description = "Checkpointing not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn pause_execution(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<PauseExecutionRequest>,
) -> Result<Json<PauseExecutionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Pausing execution: {}", id);

    let checkpoint_store = state.checkpoint_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Checkpoint/pause functionality is not enabled".to_string(),
            }),
        )
    })?;

    // Get the execution
    let execution = state
        .execution_store
        .get(&id)
        .await
        .map_err(|e| {
            error!("Failed to get execution: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get execution: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", id),
                }),
            )
        })?;

    // Create checkpoint
    let reason = req.reason.unwrap_or_else(|| "manual_pause".to_string());

    let mut checkpoint = ExecutionCheckpoint::new(
        execution.workflow_id,
        id,
        execution.clone(),
        vec![], // Would need to track completed nodes in execution
        0,      // Would need to track current level
        reason,
    );
    checkpoint.paused = true;

    let checkpoint_id = checkpoint_store.save(&checkpoint).await.map_err(|e| {
        error!("Failed to save checkpoint: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to save checkpoint: {}", e),
            }),
        )
    })?;

    info!("Execution {} paused with checkpoint {}", id, checkpoint_id);

    Ok(Json(PauseExecutionResponse {
        checkpoint_id,
        message: "Execution paused successfully".to_string(),
    }))
}

/// Resume a paused execution from latest checkpoint
#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/resume",
    params(
        ("id" = Uuid, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Execution resumed", body = ResumeExecutionResponse),
        (status = 404, description = "Execution or checkpoint not found", body = ErrorResponse),
        (status = 503, description = "Checkpointing not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn resume_execution(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<ResumeExecutionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Resuming execution: {}", id);

    let checkpoint_store = state.checkpoint_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Checkpoint/resume functionality is not enabled".to_string(),
            }),
        )
    })?;

    // Load latest checkpoint
    let _checkpoint = checkpoint_store
        .load_latest(id)
        .await
        .map_err(|e| {
            error!("Failed to load checkpoint: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to load checkpoint: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("No checkpoint found for execution {}", id),
                }),
            )
        })?;

    // TODO: Actually resume the execution using the engine
    // This would involve:
    // 1. Restoring the ExecutionContext from the checkpoint
    // 2. Marking completed nodes as done
    // 3. Re-running the workflow from the checkpoint state

    info!("Execution {} resumed", id);

    Ok(Json(ResumeExecutionResponse {
        execution_id: id,
        message: "Execution resume initiated (implementation pending)".to_string(),
    }))
}

/// List checkpoints for an execution
#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/checkpoints",
    params(
        ("id" = Uuid, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Checkpoints retrieved", body = ListCheckpointsResponse),
        (status = 503, description = "Checkpointing not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_execution_checkpoints(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<ListCheckpointsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing checkpoints for execution: {}", id);

    let checkpoint_store = state.checkpoint_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Checkpoint functionality is not enabled".to_string(),
            }),
        )
    })?;

    let checkpoints = checkpoint_store.list_by_execution(id).await.map_err(|e| {
        error!("Failed to list checkpoints: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to list checkpoints: {}", e),
            }),
        )
    })?;

    let summaries: Vec<CheckpointSummary> = checkpoints
        .into_iter()
        .map(|c| CheckpointSummary {
            id: c.id,
            workflow_id: c.workflow_id,
            execution_id: c.execution_id,
            completed_nodes_count: c.completed_nodes.len(),
            current_level: c.current_level,
            paused: c.paused,
            reason: c.reason,
            created_at: c.created_at,
        })
        .collect();

    let total = summaries.len();

    Ok(Json(ListCheckpointsResponse {
        checkpoints: summaries,
        total,
    }))
}

/// Delete all checkpoints for an execution
#[utoipa::path(
    delete,
    path = "/api/v1/executions/{id}/checkpoints",
    params(
        ("id" = Uuid, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Checkpoints deleted", body = DeleteCheckpointsResponse),
        (status = 503, description = "Checkpointing not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_execution_checkpoints(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteCheckpointsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Deleting checkpoints for execution: {}", id);

    let checkpoint_store = state.checkpoint_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Checkpoint functionality is not enabled".to_string(),
            }),
        )
    })?;

    let deleted_count = checkpoint_store
        .delete_by_execution(id)
        .await
        .map_err(|e| {
            error!("Failed to delete checkpoints: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to delete checkpoints: {}", e),
                }),
            )
        })?;

    info!("Deleted {} checkpoints for execution {}", deleted_count, id);

    Ok(Json(DeleteCheckpointsResponse {
        deleted_count,
        message: format!("Deleted {} checkpoints", deleted_count),
    }))
}
