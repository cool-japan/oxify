//! API handlers for workflow versioning

use crate::handlers::AppState;
use crate::types::ErrorResponse;
use crate::user_types::ApiUser;
use crate::version_types::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use oxify_model::WorkflowId;
use std::sync::Arc;
use tracing::{error, info};

/// Save a new version of a workflow
#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/versions",
    params(
        ("id" = Uuid, Path, description = "Workflow ID")
    ),
    request_body = SaveVersionRequest,
    responses(
        (status = 201, description = "Version saved successfully", body = SaveVersionResponse),
        (status = 404, description = "Workflow not found", body = ErrorResponse),
        (status = 503, description = "Versioning not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn save_workflow_version(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<ApiUser>,
    Path(id): Path<WorkflowId>,
    Json(req): Json<SaveVersionRequest>,
) -> Result<(StatusCode, Json<SaveVersionResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Saving version for workflow: {} by user: {}", id, user.id);

    let version_store = state.version_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Workflow versioning is not enabled".to_string(),
            }),
        )
    })?;

    // Get the current workflow
    let workflow = state
        .workflow_store
        .get(&id)
        .await
        .map_err(|e| {
            error!("Failed to get workflow: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Workflow {} not found", id),
                }),
            )
        })?;

    // Save the version
    let version_id = version_store
        .save_version(&workflow, req.description, Some(user.id.to_string()))
        .await
        .map_err(|e| {
            error!("Failed to save version: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to save version: {}", e),
                }),
            )
        })?;

    // Get the version number
    let versions = version_store.get_versions(&id).await.map_err(|e| {
        error!("Failed to get versions: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to get version number: {}", e),
            }),
        )
    })?;

    let version = versions
        .iter()
        .find(|v| v.id == version_id)
        .map(|v| v.version)
        .unwrap_or(1);

    info!("Version {} saved for workflow: {}", version, id);
    Ok((
        StatusCode::CREATED,
        Json(SaveVersionResponse {
            id: version_id,
            version,
            message: format!("Version {} saved successfully", version),
        }),
    ))
}

/// Get version history for a workflow
#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/versions",
    params(
        ("id" = Uuid, Path, description = "Workflow ID")
    ),
    responses(
        (status = 200, description = "Version history retrieved", body = GetVersionHistoryResponse),
        (status = 503, description = "Versioning not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_workflow_versions(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<WorkflowId>,
) -> Result<Json<GetVersionHistoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting version history for workflow: {}", id);

    let version_store = state.version_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Workflow versioning is not enabled".to_string(),
            }),
        )
    })?;

    let versions = version_store.get_versions(&id).await.map_err(|e| {
        error!("Failed to get versions: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to get versions: {}", e),
            }),
        )
    })?;

    let summaries: Vec<WorkflowVersionSummary> = versions
        .into_iter()
        .map(|v| WorkflowVersionSummary {
            id: v.id,
            workflow_id: v.workflow_id,
            version: v.version,
            description: v.description,
            workflow_name: v.workflow.metadata.name.clone(),
            created_at: v.created_at,
            created_by: v.created_by,
        })
        .collect();

    let total = summaries.len();

    Ok(Json(GetVersionHistoryResponse {
        versions: summaries,
        total,
    }))
}

/// Get a specific version of a workflow
#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/versions/{version}",
    params(
        ("id" = Uuid, Path, description = "Workflow ID"),
        ("version" = i32, Path, description = "Version number")
    ),
    responses(
        (status = 200, description = "Version retrieved", body = GetVersionResponse),
        (status = 404, description = "Version not found", body = ErrorResponse),
        (status = 503, description = "Versioning not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_workflow_version(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path((id, version)): Path<(WorkflowId, i32)>,
) -> Result<Json<GetVersionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting version {} for workflow: {}", version, id);

    let version_store = state.version_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Workflow versioning is not enabled".to_string(),
            }),
        )
    })?;

    let version_record = version_store
        .get_version(&id, version)
        .await
        .map_err(|e| {
            error!("Failed to get version: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get version: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Version {} not found for workflow {}", version, id),
                }),
            )
        })?;

    Ok(Json(GetVersionResponse {
        id: version_record.id,
        workflow_id: version_record.workflow_id,
        version: version_record.version,
        description: version_record.description,
        workflow: version_record.workflow,
        created_at: version_record.created_at,
        created_by: version_record.created_by,
    }))
}

/// Compare two versions of a workflow
#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/versions/compare",
    params(
        ("id" = Uuid, Path, description = "Workflow ID"),
        ("v1" = i32, Query, description = "First version number"),
        ("v2" = i32, Query, description = "Second version number")
    ),
    responses(
        (status = 200, description = "Versions compared", body = CompareVersionsResponse),
        (status = 404, description = "One or both versions not found", body = ErrorResponse),
        (status = 503, description = "Versioning not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn compare_workflow_versions(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<WorkflowId>,
    axum::extract::Query(params): axum::extract::Query<CompareParams>,
) -> Result<Json<CompareVersionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        "Comparing versions {} and {} for workflow: {}",
        params.v1, params.v2, id
    );

    let version_store = state.version_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Workflow versioning is not enabled".to_string(),
            }),
        )
    })?;

    let comparison = version_store
        .compare_versions(&id, params.v1, params.v2)
        .await
        .map_err(|e| {
            error!("Failed to compare versions: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to compare versions: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!(
                        "One or both versions ({}, {}) not found for workflow {}",
                        params.v1, params.v2, id
                    ),
                }),
            )
        })?;

    Ok(Json(CompareVersionsResponse {
        version1: comparison.version1,
        version2: comparison.version2,
        nodes_added: comparison.nodes_added,
        nodes_removed: comparison.nodes_removed,
        edges_added: comparison.edges_added,
        edges_removed: comparison.edges_removed,
        name_changed: comparison.name_changed,
        description_changed: comparison.description_changed,
    }))
}

/// Restore a workflow to a specific version
#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/versions/{version}/restore",
    params(
        ("id" = Uuid, Path, description = "Workflow ID"),
        ("version" = i32, Path, description = "Version number to restore")
    ),
    responses(
        (status = 200, description = "Version restored", body = RestoreVersionResponse),
        (status = 404, description = "Version not found", body = ErrorResponse),
        (status = 503, description = "Versioning not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn restore_workflow_version(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<ApiUser>,
    Path((id, version)): Path<(WorkflowId, i32)>,
) -> Result<Json<RestoreVersionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        "Restoring workflow {} to version {} by user: {}",
        id, version, user.id
    );

    let version_store = state.version_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Workflow versioning is not enabled".to_string(),
            }),
        )
    })?;

    // Get the version to restore
    let version_record = version_store
        .get_version(&id, version)
        .await
        .map_err(|e| {
            error!("Failed to get version: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get version: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Version {} not found for workflow {}", version, id),
                }),
            )
        })?;

    // Update the workflow with the old version
    state
        .workflow_store
        .update(&id, version_record.workflow.clone())
        .await
        .map_err(|e| {
            error!("Failed to update workflow: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to restore workflow: {}", e),
                }),
            )
        })?;

    // Save a new version marking the restore
    let new_version_id = version_store
        .save_version(
            &version_record.workflow,
            Some(format!("Restored from version {}", version)),
            Some(user.id.to_string()),
        )
        .await
        .map_err(|e| {
            error!("Failed to save restore version: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to save restore version: {}", e),
                }),
            )
        })?;

    // Get the new version number
    let versions = version_store.get_versions(&id).await.map_err(|e| {
        error!("Failed to get versions: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to get version number: {}", e),
            }),
        )
    })?;

    let new_version = versions
        .iter()
        .find(|v| v.id == new_version_id)
        .map(|v| v.version)
        .unwrap_or(1);

    info!(
        "Workflow {} restored to version {} (new version: {})",
        id, version, new_version
    );

    Ok(Json(RestoreVersionResponse {
        message: format!(
            "Workflow restored to version {} (created new version {})",
            version, new_version
        ),
        new_version,
    }))
}

/// Query parameters for version comparison
#[derive(Debug, serde::Deserialize)]
pub struct CompareParams {
    pub v1: i32,
    pub v2: i32,
}
