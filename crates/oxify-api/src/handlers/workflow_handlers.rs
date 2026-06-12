//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::types::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use oxify_engine::ExecutionConfig;
use oxify_model::{ExecutionContext, ExecutionState, WorkflowId};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use super::template_vector_handlers::AppState;

/// Create a new workflow
#[utoipa::path(
    post,
    path = "/api/v1/workflows",
    request_body = CreateWorkflowRequest,
    responses(
        (
            status = 201,
            description = "Workflow created successfully",
            body = CreateWorkflowResponse
        ),
        (status = 400, description = "Invalid workflow", body = ErrorResponse)
    )
)]
pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<CreateWorkflowResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Creating workflow: {}", req.workflow.metadata.name);
    if let Err(e) = req.workflow.validate() {
        error!("Workflow validation failed: {}", e);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: e,
            }),
        ));
    }
    let id = state
        .workflow_store
        .create(req.workflow)
        .await
        .map_err(|e| {
            error!("Workflow creation error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to create workflow: {}", e),
                }),
            )
        })?;
    Ok((
        StatusCode::CREATED,
        Json(CreateWorkflowResponse {
            id,
            message: "Workflow created successfully".to_string(),
        }),
    ))
}
/// Get a workflow by ID
#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}",
    params(("id" = String, Path, description = "Workflow ID")),
    responses(
        (status = 200, description = "Workflow found", body = GetWorkflowResponse),
        (status = 404, description = "Workflow not found", body = ErrorResponse)
    )
)]
pub async fn get_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<WorkflowId>,
) -> Result<Json<GetWorkflowResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting workflow: {}", id);
    match state.workflow_store.get(&id).await {
        Ok(Some(workflow)) => Ok(Json(GetWorkflowResponse { workflow })),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Workflow {} not found", id),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to get workflow: {}", e),
            }),
        )),
    }
}
/// List all workflows
#[utoipa::path(
    get,
    path = "/api/v1/workflows",
    responses(
        (status = 200, description = "List of workflows", body = ListWorkflowsResponse)
    )
)]
pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListWorkflowsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing workflows");
    let workflows = state.workflow_store.list().await.map_err(|e| {
        error!("Failed to list workflows: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to list workflows: {}", e),
            }),
        )
    })?;
    let total = workflows.len();
    Ok(Json(ListWorkflowsResponse { workflows, total }))
}
/// Update a workflow
#[utoipa::path(
    put,
    path = "/api/v1/workflows/{id}",
    params(("id" = String, Path, description = "Workflow ID")),
    request_body = UpdateWorkflowRequest,
    responses(
        (
            status = 200,
            description = "Workflow updated successfully",
            body = UpdateWorkflowResponse
        ),
        (status = 404, description = "Workflow not found", body = ErrorResponse),
        (status = 400, description = "Invalid workflow", body = ErrorResponse)
    )
)]
pub async fn update_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<WorkflowId>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> Result<Json<UpdateWorkflowResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Updating workflow: {}", id);
    if let Err(e) = req.workflow.validate() {
        error!("Workflow validation failed: {}", e);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: e,
            }),
        ));
    }
    match state.workflow_store.update(&id, req.workflow).await {
        Ok(Some(_)) => Ok(Json(UpdateWorkflowResponse {
            message: "Workflow updated successfully".to_string(),
        })),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Workflow {} not found", id),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to update workflow: {}", e),
            }),
        )),
    }
}
/// Delete a workflow
#[utoipa::path(
    delete,
    path = "/api/v1/workflows/{id}",
    params(("id" = String, Path, description = "Workflow ID")),
    responses(
        (
            status = 200,
            description = "Workflow deleted successfully",
            body = DeleteWorkflowResponse
        ),
        (status = 404, description = "Workflow not found", body = ErrorResponse)
    )
)]
pub async fn delete_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<WorkflowId>,
) -> Result<Json<DeleteWorkflowResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Deleting workflow: {}", id);
    match state.workflow_store.delete(&id).await {
        Ok(true) => Ok(Json(DeleteWorkflowResponse {
            message: "Workflow deleted successfully".to_string(),
        })),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Workflow {} not found", id),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to delete workflow: {}", e),
            }),
        )),
    }
}
/// Execute a workflow
#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/execute",
    params(("id" = String, Path, description = "Workflow ID")),
    request_body = ExecuteWorkflowRequest,
    responses(
        (
            status = 202,
            description = "Workflow execution started",
            body = ExecuteWorkflowResponse
        ),
        (status = 404, description = "Workflow not found", body = ErrorResponse),
        (status = 500, description = "Execution failed", body = ErrorResponse)
    )
)]
pub async fn execute_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<WorkflowId>,
    Json(req): Json<ExecuteWorkflowRequest>,
) -> Result<(StatusCode, Json<ExecuteWorkflowResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Executing workflow: {}", id);
    let workflow = match state.workflow_store.get(&id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Workflow {} not found", id),
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            ));
        }
    };
    let mut ctx = ExecutionContext::new(workflow.metadata.id);
    for (key, value) in req.variables {
        ctx.set_variable(key, value);
    }
    let execution_id = ctx.execution_id;
    state
        .execution_store
        .create(ctx.clone())
        .await
        .map_err(|e| {
            error!("Failed to create execution: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to create execution: {}", e),
                }),
            )
        })?;
    state.http_metrics.inc_active_execution();
    let engine = state.engine.clone();
    let execution_store = state.execution_store.clone();
    let metrics = state.http_metrics.clone();
    tokio::spawn(async move {
        let result = engine
            .execute_with_context(&workflow, ctx, ExecutionConfig::new().with_events())
            .await;
        metrics.dec_active_execution();
        match result {
            Ok(result_ctx) => match execution_store.update(&execution_id, result_ctx).await {
                Ok(Some(_)) => {
                    info!("Execution {} completed successfully", execution_id);
                }
                Ok(None) => {
                    error!(
                        "Failed to update execution {}: execution not found",
                        execution_id
                    );
                }
                Err(e) => {
                    error!("Failed to update execution {}: {}", execution_id, e);
                }
            },
            Err(e) => {
                error!("Workflow execution {} failed: {}", execution_id, e);
            }
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(ExecuteWorkflowResponse {
            execution_id,
            message: "Workflow execution started".to_string(),
        }),
    ))
}
/// Get execution status
#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}",
    params(("id" = String, Path, description = "Execution ID")),
    responses(
        (status = 200, description = "Execution found", body = GetExecutionResponse),
        (status = 404, description = "Execution not found", body = ErrorResponse)
    )
)]
pub async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<GetExecutionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting execution: {}", id);
    match state.execution_store.get(&id).await {
        Ok(Some(ctx)) => {
            let node_results = ctx
                .node_results
                .values()
                .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
                .collect();
            let variables: serde_json::Map<String, serde_json::Value> =
                ctx.variables.into_iter().collect();
            Ok(Json(GetExecutionResponse {
                execution_id: id,
                workflow_id: ctx.workflow_id,
                state: ctx.state,
                variables,
                node_results,
            }))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Execution {} not found", id),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to get execution: {}", e),
            }),
        )),
    }
}
/// List all executions
#[utoipa::path(
    get,
    path = "/api/v1/executions",
    responses(
        (status = 200, description = "List of executions", body = ListExecutionsResponse)
    )
)]
pub async fn list_executions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListExecutionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing executions");
    let executions = state.execution_store.list().await.map_err(|e| {
        error!("Failed to list executions: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to list executions: {}", e),
            }),
        )
    })?;
    let total = executions.len();
    let executions = executions
        .into_iter()
        .map(|(id, ctx)| ExecutionSummary {
            execution_id: id,
            workflow_id: ctx.workflow_id,
            state: ctx.state,
            started_at: Some(ctx.started_at.to_rfc3339()),
            completed_at: ctx.completed_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();
    Ok(Json(ListExecutionsResponse { executions, total }))
}
/// List executions for a specific workflow
#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/executions",
    params(("id" = String, Path, description = "Workflow ID")),
    responses(
        (status = 200, description = "List of executions", body = ListExecutionsResponse)
    )
)]
pub async fn list_workflow_executions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<WorkflowId>,
) -> Result<Json<ListExecutionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing executions for workflow: {}", id);
    let executions = state
        .execution_store
        .list_by_workflow(&id)
        .await
        .map_err(|e| {
            error!("Failed to list workflow executions: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to list executions: {}", e),
                }),
            )
        })?;
    let total = executions.len();
    let executions = executions
        .into_iter()
        .map(|(exec_id, ctx)| ExecutionSummary {
            execution_id: exec_id,
            workflow_id: ctx.workflow_id,
            state: ctx.state,
            started_at: Some(ctx.started_at.to_rfc3339()),
            completed_at: ctx.completed_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();
    Ok(Json(ListExecutionsResponse { executions, total }))
}
/// Cancel a running execution
#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/cancel",
    params(("id" = String, Path, description = "Execution ID")),
    responses(
        (status = 200, description = "Execution cancelled", body = serde_json::Value),
        (status = 404, description = "Execution not found", body = ErrorResponse),
        (
            status = 400,
            description = "Execution already completed/failed/cancelled",
            body = ErrorResponse
        )
    )
)]
pub async fn cancel_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    info!("Cancelling execution: {}", id);
    let execution = match state.execution_store.get(&id).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", id),
                }),
            ));
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
    match execution.state {
        ExecutionState::Running | ExecutionState::Paused => {
            let mut updated_ctx = execution;
            updated_ctx.cancel();
            state
                .execution_store
                .update(&id, updated_ctx)
                .await
                .map_err(|e| {
                    error!("Failed to update execution: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "StorageError".to_string(),
                            message: format!("Failed to update execution: {}", e),
                        }),
                    )
                })?;
            state.http_metrics.dec_active_execution();
            info!("Successfully cancelled execution: {}", id);
            Ok((
                StatusCode::OK,
                Json(serde_json::json!(
                    { "message" : "Execution cancelled successfully", "execution_id"
                    : id, "state" : "Cancelled" }
                )),
            ))
        }
        ExecutionState::Completed => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "InvalidState".to_string(),
                message: "Cannot cancel completed execution".to_string(),
            }),
        )),
        ExecutionState::Failed(_) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "InvalidState".to_string(),
                message: "Cannot cancel failed execution".to_string(),
            }),
        )),
        ExecutionState::Cancelled => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "InvalidState".to_string(),
                message: "Execution already cancelled".to_string(),
            }),
        )),
    }
}
