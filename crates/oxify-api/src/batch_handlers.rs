//! Batch operation handlers for workflows and executions
//!
//! This module provides batch API endpoints for efficient bulk operations
//! on workflows and executions.

use crate::handlers::AppState;
use crate::types::ErrorResponse;
use axum::{extract::State, http::StatusCode, Json};
use oxify_model::{ExecutionContext, Workflow, WorkflowId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

// ============================================================================
// Batch Types
// ============================================================================

/// Request to create multiple workflows in a single operation
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchCreateWorkflowsRequest {
    /// List of workflows to create
    pub workflows: Vec<Workflow>,
    /// If true, fail entire batch on any validation error
    #[serde(default)]
    pub atomic: bool,
}

/// Result for a single workflow in a batch operation
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchWorkflowResult {
    /// Index of the workflow in the request
    pub index: usize,
    /// Workflow ID if successful
    #[schema(value_type = Option<String>)]
    pub id: Option<WorkflowId>,
    /// Whether the operation succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Response for batch workflow create
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchCreateWorkflowsResponse {
    /// Results for each workflow
    pub results: Vec<BatchWorkflowResult>,
    /// Total workflows processed
    pub total: usize,
    /// Number of successful creates
    pub succeeded: usize,
    /// Number of failed creates
    pub failed: usize,
}

/// Request to delete multiple workflows
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchDeleteWorkflowsRequest {
    /// List of workflow IDs to delete
    #[schema(value_type = Vec<String>)]
    pub ids: Vec<WorkflowId>,
    /// If true, fail entire batch on any error
    #[serde(default)]
    pub atomic: bool,
}

/// Result for a single delete operation
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchDeleteResult {
    /// Workflow ID
    #[schema(value_type = String)]
    pub id: WorkflowId,
    /// Whether the deletion succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Response for batch workflow delete
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchDeleteWorkflowsResponse {
    /// Results for each deletion
    pub results: Vec<BatchDeleteResult>,
    /// Total workflows processed
    pub total: usize,
    /// Number of successful deletions
    pub succeeded: usize,
    /// Number of failed deletions
    pub failed: usize,
}

/// Single execution request within a batch
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchExecutionItem {
    /// Workflow ID to execute
    #[schema(value_type = String)]
    pub workflow_id: WorkflowId,
    /// Initial variables for the execution context
    #[serde(default)]
    pub variables: serde_json::Map<String, serde_json::Value>,
}

/// Request to execute multiple workflows
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchExecuteWorkflowsRequest {
    /// List of execution requests
    pub executions: Vec<BatchExecutionItem>,
    /// Maximum concurrent executions (default: 10)
    /// Future: used for controlled concurrency with semaphore
    #[serde(default = "default_max_concurrent")]
    #[allow(dead_code)]
    pub max_concurrent: usize,
}

fn default_max_concurrent() -> usize {
    10
}

/// Result for a single execution start
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchExecutionResult {
    /// Index in the request
    pub index: usize,
    /// Workflow ID
    #[schema(value_type = String)]
    pub workflow_id: WorkflowId,
    /// Execution ID if started successfully
    #[schema(value_type = Option<String>)]
    pub execution_id: Option<Uuid>,
    /// Whether the execution started successfully
    pub success: bool,
    /// Error message if failed to start
    pub error: Option<String>,
}

/// Response for batch execution
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchExecuteWorkflowsResponse {
    /// Results for each execution
    pub results: Vec<BatchExecutionResult>,
    /// Total executions requested
    pub total: usize,
    /// Number of successfully started executions
    pub succeeded: usize,
    /// Number of failed starts
    pub failed: usize,
}

/// Request to get multiple workflows by ID
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchGetWorkflowsRequest {
    /// List of workflow IDs to retrieve
    #[schema(value_type = Vec<String>)]
    pub ids: Vec<WorkflowId>,
}

/// Result for a single get operation
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchGetResult {
    /// Workflow ID
    #[schema(value_type = String)]
    pub id: WorkflowId,
    /// The workflow if found
    pub workflow: Option<Workflow>,
    /// Whether the workflow was found
    pub found: bool,
}

/// Response for batch workflow get
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchGetWorkflowsResponse {
    /// Results for each workflow
    pub results: Vec<BatchGetResult>,
    /// Total workflows requested
    pub total: usize,
    /// Number of workflows found
    pub found: usize,
}

// ============================================================================
// Batch Handlers
// ============================================================================

/// Batch create multiple workflows
#[utoipa::path(
    post,
    path = "/api/v1/workflows/batch",
    request_body = BatchCreateWorkflowsRequest,
    responses(
        (status = 200, description = "Batch operation completed", body = BatchCreateWorkflowsResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    ),
    tag = "Batch Operations"
)]
pub async fn batch_create_workflows(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCreateWorkflowsRequest>,
) -> Result<Json<BatchCreateWorkflowsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Batch creating {} workflows", req.workflows.len());

    if req.workflows.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "No workflows provided".to_string(),
            }),
        ));
    }

    if req.workflows.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Maximum 100 workflows per batch".to_string(),
            }),
        ));
    }

    // If atomic mode, validate all workflows first
    if req.atomic {
        for (i, workflow) in req.workflows.iter().enumerate() {
            if let Err(e) = workflow.validate() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "ValidationError".to_string(),
                        message: format!("Workflow at index {} failed validation: {}", i, e),
                    }),
                ));
            }
        }
    }

    let mut results = Vec::with_capacity(req.workflows.len());
    let mut succeeded = 0;
    let mut failed = 0;

    for (index, workflow) in req.workflows.into_iter().enumerate() {
        // Validate workflow
        if let Err(e) = workflow.validate() {
            warn!("Workflow {} validation failed: {}", index, e);
            results.push(BatchWorkflowResult {
                index,
                id: None,
                success: false,
                error: Some(format!("Validation error: {}", e)),
            });
            failed += 1;
            continue;
        }

        // Create workflow
        match state.workflow_store.create(workflow).await {
            Ok(id) => {
                results.push(BatchWorkflowResult {
                    index,
                    id: Some(id),
                    success: true,
                    error: None,
                });
                succeeded += 1;
            }
            Err(e) => {
                error!("Failed to create workflow {}: {}", index, e);
                results.push(BatchWorkflowResult {
                    index,
                    id: None,
                    success: false,
                    error: Some(format!("Storage error: {}", e)),
                });
                failed += 1;
            }
        }
    }

    let total = results.len();

    Ok(Json(BatchCreateWorkflowsResponse {
        results,
        total,
        succeeded,
        failed,
    }))
}

/// Batch delete multiple workflows
#[utoipa::path(
    delete,
    path = "/api/v1/workflows/batch",
    request_body = BatchDeleteWorkflowsRequest,
    responses(
        (status = 200, description = "Batch operation completed", body = BatchDeleteWorkflowsResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    ),
    tag = "Batch Operations"
)]
pub async fn batch_delete_workflows(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchDeleteWorkflowsRequest>,
) -> Result<Json<BatchDeleteWorkflowsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Batch deleting {} workflows", req.ids.len());

    if req.ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "No workflow IDs provided".to_string(),
            }),
        ));
    }

    if req.ids.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Maximum 100 workflows per batch".to_string(),
            }),
        ));
    }

    // If atomic mode, check all workflows exist first
    if req.atomic {
        for id in &req.ids {
            match state.workflow_store.get(id).await {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse {
                            error: "NotFound".to_string(),
                            message: format!("Workflow {} not found (atomic mode)", id),
                        }),
                    ));
                }
                Err(e) => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "StorageError".to_string(),
                            message: format!("Error checking workflow {}: {}", id, e),
                        }),
                    ));
                }
            }
        }
    }

    let mut results = Vec::with_capacity(req.ids.len());
    let mut succeeded = 0;
    let mut failed = 0;

    for id in req.ids {
        match state.workflow_store.delete(&id).await {
            Ok(true) => {
                results.push(BatchDeleteResult {
                    id,
                    success: true,
                    error: None,
                });
                succeeded += 1;
            }
            Ok(false) => {
                results.push(BatchDeleteResult {
                    id,
                    success: false,
                    error: Some("Workflow not found".to_string()),
                });
                failed += 1;
            }
            Err(e) => {
                error!("Failed to delete workflow {}: {}", id, e);
                results.push(BatchDeleteResult {
                    id,
                    success: false,
                    error: Some(format!("Storage error: {}", e)),
                });
                failed += 1;
            }
        }
    }

    let total = results.len();

    Ok(Json(BatchDeleteWorkflowsResponse {
        results,
        total,
        succeeded,
        failed,
    }))
}

/// Batch execute multiple workflows
#[utoipa::path(
    post,
    path = "/api/v1/executions/batch",
    request_body = BatchExecuteWorkflowsRequest,
    responses(
        (status = 200, description = "Batch operation completed", body = BatchExecuteWorkflowsResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    ),
    tag = "Batch Operations"
)]
pub async fn batch_execute_workflows(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchExecuteWorkflowsRequest>,
) -> Result<Json<BatchExecuteWorkflowsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Batch executing {} workflows", req.executions.len());

    if req.executions.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "No executions provided".to_string(),
            }),
        ));
    }

    if req.executions.len() > 50 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Maximum 50 executions per batch".to_string(),
            }),
        ));
    }

    let mut results = Vec::with_capacity(req.executions.len());
    let mut succeeded = 0;
    let mut failed = 0;

    // Process executions sequentially to avoid overwhelming the system
    // In future, could use semaphore for controlled concurrency
    for (index, exec_req) in req.executions.into_iter().enumerate() {
        // Get workflow
        let workflow = match state.workflow_store.get(&exec_req.workflow_id).await {
            Ok(Some(w)) => w,
            Ok(None) => {
                results.push(BatchExecutionResult {
                    index,
                    workflow_id: exec_req.workflow_id,
                    execution_id: None,
                    success: false,
                    error: Some("Workflow not found".to_string()),
                });
                failed += 1;
                continue;
            }
            Err(e) => {
                error!("Failed to get workflow {}: {}", exec_req.workflow_id, e);
                results.push(BatchExecutionResult {
                    index,
                    workflow_id: exec_req.workflow_id,
                    execution_id: None,
                    success: false,
                    error: Some(format!("Storage error: {}", e)),
                });
                failed += 1;
                continue;
            }
        };

        // Create execution context with initial variables
        let mut ctx = ExecutionContext::new(workflow.metadata.id);
        for (key, value) in exec_req.variables {
            ctx.set_variable(key, value);
        }

        // Generate execution ID
        let execution_id = Uuid::new_v4();

        // Create initial execution record
        if let Err(e) = state.execution_store.create(ctx.clone()).await {
            error!("Failed to create execution record: {}", e);
            results.push(BatchExecutionResult {
                index,
                workflow_id: exec_req.workflow_id,
                execution_id: None,
                success: false,
                error: Some(format!("Failed to create execution: {}", e)),
            });
            failed += 1;
            continue;
        }

        // Clone state references for the spawned task
        let engine = state.engine.clone();
        let execution_store = state.execution_store.clone();
        let workflow_id = exec_req.workflow_id;
        let exec_id_clone = execution_id;
        let metrics = state.http_metrics.clone();

        // Increment active executions counter
        state.http_metrics.inc_active_execution();

        // Spawn execution task
        tokio::spawn(async move {
            match engine.execute(&workflow).await {
                Ok(result_ctx) => {
                    // Decrement active executions on completion
                    metrics.dec_active_execution();

                    match execution_store.update(&exec_id_clone, result_ctx).await {
                        Ok(Some(_)) => {
                            info!("Batch execution {} completed successfully", exec_id_clone);
                        }
                        Ok(None) => {
                            error!(
                                "Failed to update execution {}: execution not found",
                                exec_id_clone
                            );
                        }
                        Err(e) => {
                            error!("Failed to update execution {}: {}", exec_id_clone, e);
                        }
                    }
                }
                Err(e) => {
                    // Decrement active executions on failure
                    metrics.dec_active_execution();
                    error!("Batch workflow execution {} failed: {}", exec_id_clone, e);
                }
            }
        });

        results.push(BatchExecutionResult {
            index,
            workflow_id,
            execution_id: Some(execution_id),
            success: true,
            error: None,
        });
        succeeded += 1;
    }

    let total = results.len();

    Ok(Json(BatchExecuteWorkflowsResponse {
        results,
        total,
        succeeded,
        failed,
    }))
}

/// Batch get multiple workflows by ID
#[utoipa::path(
    post,
    path = "/api/v1/workflows/batch/get",
    request_body = BatchGetWorkflowsRequest,
    responses(
        (status = 200, description = "Batch get completed", body = BatchGetWorkflowsResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    ),
    tag = "Batch Operations"
)]
pub async fn batch_get_workflows(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchGetWorkflowsRequest>,
) -> Result<Json<BatchGetWorkflowsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Batch getting {} workflows", req.ids.len());

    if req.ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "No workflow IDs provided".to_string(),
            }),
        ));
    }

    if req.ids.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Maximum 100 workflows per batch".to_string(),
            }),
        ));
    }

    let mut results = Vec::with_capacity(req.ids.len());
    let mut found = 0;

    for id in req.ids {
        match state.workflow_store.get(&id).await {
            Ok(Some(workflow)) => {
                results.push(BatchGetResult {
                    id,
                    workflow: Some(workflow),
                    found: true,
                });
                found += 1;
            }
            Ok(None) => {
                results.push(BatchGetResult {
                    id,
                    workflow: None,
                    found: false,
                });
            }
            Err(e) => {
                error!("Failed to get workflow {}: {}", id, e);
                results.push(BatchGetResult {
                    id,
                    workflow: None,
                    found: false,
                });
            }
        }
    }

    let total = results.len();

    Ok(Json(BatchGetWorkflowsResponse {
        results,
        total,
        found,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::WorkflowBuilder;

    #[allow(dead_code)]
    fn create_test_workflow(name: &str) -> Workflow {
        WorkflowBuilder::new(name)
            .description("Test workflow")
            .start("start")
            .end("end")
            .build()
    }

    #[test]
    fn test_batch_create_request_deserialization() {
        let json = r#"{
            "workflows": [],
            "atomic": true
        }"#;

        let req: BatchCreateWorkflowsRequest = serde_json::from_str(json).unwrap();
        assert!(req.atomic);
        assert!(req.workflows.is_empty());
    }

    #[test]
    fn test_batch_delete_request_deserialization() {
        let json = r#"{
            "ids": ["550e8400-e29b-41d4-a716-446655440000"],
            "atomic": false
        }"#;

        let req: BatchDeleteWorkflowsRequest = serde_json::from_str(json).unwrap();
        assert!(!req.atomic);
        assert_eq!(req.ids.len(), 1);
    }

    #[test]
    fn test_batch_execute_request_deserialization() {
        let json = r#"{
            "executions": [
                {
                    "workflow_id": "550e8400-e29b-41d4-a716-446655440000",
                    "variables": {"key": "value"}
                }
            ],
            "max_concurrent": 5
        }"#;

        let req: BatchExecuteWorkflowsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.max_concurrent, 5);
        assert_eq!(req.executions.len(), 1);
    }

    #[test]
    fn test_batch_get_request_deserialization() {
        let json = r#"{
            "ids": [
                "550e8400-e29b-41d4-a716-446655440000",
                "550e8400-e29b-41d4-a716-446655440001"
            ]
        }"#;

        let req: BatchGetWorkflowsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.ids.len(), 2);
    }

    #[test]
    fn test_batch_workflow_result_serialization() {
        let result = BatchWorkflowResult {
            index: 0,
            id: Some(Uuid::new_v4()),
            success: true,
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"index\":0"));
    }

    #[test]
    fn test_batch_create_response_serialization() {
        let response = BatchCreateWorkflowsResponse {
            results: vec![],
            total: 0,
            succeeded: 0,
            failed: 0,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"total\":0"));
    }
}
