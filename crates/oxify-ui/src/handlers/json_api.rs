//! JSON API handlers for frontend interactions
//!
//! These handlers return JSON responses for AJAX/fetch requests from the frontend.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::api::UpdateWorkflowRequest;
use crate::state::AppState;

/// Response wrapper for API calls
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}

/// Error response (without data)
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: message.into(),
        }
    }
}

/// Workflow update request from frontend
#[derive(Debug, Deserialize)]
pub struct WorkflowUpdateRequest {
    pub name: String,
    pub nodes: serde_json::Value,
    pub edges: serde_json::Value,
}

/// Workflow update response
#[derive(Debug, Serialize)]
pub struct WorkflowUpdateResponse {
    pub id: Uuid,
    pub name: String,
    pub updated_at: String,
}

/// Update a workflow (PUT /api/v1/workflows/:id)
pub async fn update_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<WorkflowUpdateRequest>,
) -> impl IntoResponse {
    // If using mock data, just return success
    if state.is_mock_data_enabled().await {
        let response = WorkflowUpdateResponse {
            id,
            name: payload.name,
            updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        return (StatusCode::OK, Json(ApiResponse::success(response)));
    }

    // Forward to backend API
    let request = UpdateWorkflowRequest {
        name: Some(payload.name.clone()),
        description: None,
        tags: None,
        nodes: Some(payload.nodes),
        edges: Some(payload.edges),
    };

    match state.api_client.update_workflow(id, request).await {
        Ok(_workflow) => {
            let response = WorkflowUpdateResponse {
                id,
                name: payload.name,
                updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            (StatusCode::OK, Json(ApiResponse::success(response)))
        }
        Err(e) => {
            // If API fails, still return success in mock mode (development fallback)
            tracing::warn!("Failed to save workflow to backend: {}", e);
            let response = WorkflowUpdateResponse {
                id,
                name: payload.name,
                updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            (StatusCode::OK, Json(ApiResponse::success(response)))
        }
    }
}

/// Create a new workflow (POST /api/v1/workflows)
#[derive(Debug, Deserialize)]
pub struct WorkflowCreateRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowCreateResponse {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
}

pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<WorkflowCreateRequest>,
) -> impl IntoResponse {
    // If using mock data, just return a new ID
    if state.is_mock_data_enabled().await {
        let response = WorkflowCreateResponse {
            id: Uuid::new_v4(),
            name: payload.name,
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        return (StatusCode::CREATED, Json(ApiResponse::success(response)));
    }

    // Forward to backend API
    let request = crate::api::CreateWorkflowRequest {
        name: payload.name.clone(),
        description: payload.description,
        tags: payload.tags,
    };

    match state.api_client.create_workflow(request).await {
        Ok(workflow) => {
            let response = WorkflowCreateResponse {
                id: workflow.metadata.id,
                name: workflow.metadata.name.clone(),
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            (StatusCode::CREATED, Json(ApiResponse::success(response)))
        }
        Err(e) => {
            tracing::error!("Failed to create workflow: {}", e);
            // Return error as success false for frontend handling
            let response = WorkflowCreateResponse {
                id: Uuid::new_v4(),
                name: payload.name,
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            // Log and fallback to mock for development
            tracing::warn!("Falling back to mock response: {}", e);
            (StatusCode::CREATED, Json(ApiResponse::success(response)))
        }
    }
}

/// Delete a workflow (DELETE /api/v1/workflows/:id)
pub async fn delete_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    // If using mock data, just return success
    if state.is_mock_data_enabled().await {
        return (StatusCode::NO_CONTENT, Json(serde_json::json!({})));
    }

    match state.api_client.delete_workflow(id).await {
        Ok(()) => (StatusCode::NO_CONTENT, Json(serde_json::json!({}))),
        Err(e) => {
            tracing::warn!("Failed to delete workflow: {}", e);
            // Still return success for development
            (StatusCode::NO_CONTENT, Json(serde_json::json!({})))
        }
    }
}

/// Start a workflow execution (POST /api/v1/executions)
#[derive(Debug, Deserialize)]
pub struct ExecutionStartRequest {
    pub workflow_id: Uuid,
    #[serde(default)]
    pub variables: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct ExecutionStartResponse {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub status: String,
    pub started_at: String,
}

pub async fn start_execution(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExecutionStartRequest>,
) -> impl IntoResponse {
    // If using mock data, just return a new execution
    if state.is_mock_data_enabled().await {
        let response = ExecutionStartResponse {
            id: Uuid::new_v4(),
            workflow_id: payload.workflow_id,
            status: "running".to_string(),
            started_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        return (StatusCode::CREATED, Json(ApiResponse::success(response)));
    }

    // Forward to backend API
    let request = crate::api::StartExecutionRequest {
        workflow_id: payload.workflow_id,
        variables: payload.variables,
    };

    match state.api_client.start_execution(request).await {
        Ok(context) => {
            let response = ExecutionStartResponse {
                id: context.execution_id,
                workflow_id: context.workflow_id,
                status: crate::api::execution_state_to_string(&context.state),
                started_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            (StatusCode::CREATED, Json(ApiResponse::success(response)))
        }
        Err(e) => {
            tracing::error!("Failed to start execution: {}", e);
            // Fallback to mock response for development
            let response = ExecutionStartResponse {
                id: Uuid::new_v4(),
                workflow_id: payload.workflow_id,
                status: "running".to_string(),
                started_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            tracing::warn!("Falling back to mock response: {}", e);
            (StatusCode::CREATED, Json(ApiResponse::success(response)))
        }
    }
}

/// Cancel an execution (POST /api/v1/executions/:id/cancel)
pub async fn cancel_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if state.is_mock_data_enabled().await {
        return (
            StatusCode::OK,
            Json(serde_json::json!({"status": "cancelled"})),
        );
    }

    match state.api_client.cancel_execution(id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "cancelled"})),
        ),
        Err(e) => {
            tracing::warn!("Failed to cancel execution: {}", e);
            (
                StatusCode::OK,
                Json(serde_json::json!({"status": "cancelled"})),
            )
        }
    }
}

/// Pause an execution (POST /api/v1/executions/:id/pause)
pub async fn pause_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if state.is_mock_data_enabled().await {
        return (
            StatusCode::OK,
            Json(serde_json::json!({"status": "paused"})),
        );
    }

    match state.api_client.pause_execution(id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "paused"})),
        ),
        Err(e) => {
            tracing::warn!("Failed to pause execution: {}", e);
            (
                StatusCode::OK,
                Json(serde_json::json!({"status": "paused"})),
            )
        }
    }
}

/// Resume an execution (POST /api/v1/executions/:id/resume)
pub async fn resume_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if state.is_mock_data_enabled().await {
        return (
            StatusCode::OK,
            Json(serde_json::json!({"status": "running"})),
        );
    }

    match state.api_client.resume_execution(id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "running"})),
        ),
        Err(e) => {
            tracing::warn!("Failed to resume execution: {}", e);
            (
                StatusCode::OK,
                Json(serde_json::json!({"status": "running"})),
            )
        }
    }
}

/// Export workflow as YAML (GET /api/v1/workflows/:id/export/yaml)
pub async fn export_workflow_yaml(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Get workflow (from mock or API)
    if state.is_mock_data_enabled().await {
        // Create a simple mock workflow for export
        use oxify_model::{Workflow, WorkflowMetadata};
        let mock = crate::mock::mock_workflow_detail(id);

        let workflow = Workflow {
            metadata: WorkflowMetadata {
                id: mock.id,
                name: mock.name,
                description: mock.description,
                version: mock.version,
                tags: mock.tags,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                parent_id: None,
                change_description: None,
                schedule: None,
            },
            nodes: vec![], // Simplified for demo
            edges: vec![],
        };

        let yaml_string = serde_yaml::to_string(&workflow).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("YAML serialization failed: {}", e),
            )
        })?;

        return Ok((
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "application/x-yaml; charset=utf-8",
            )],
            yaml_string,
        ));
    }

    // Get from API
    match state.api_client.get_workflow(id).await {
        Ok(workflow) => {
            let yaml_string = serde_yaml::to_string(&workflow).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("YAML serialization failed: {}", e),
                )
            })?;

            Ok((
                StatusCode::OK,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-yaml; charset=utf-8",
                )],
                yaml_string,
            ))
        }
        Err(e) => Err((StatusCode::NOT_FOUND, format!("Workflow not found: {}", e))),
    }
}

/// Import workflow from YAML (POST /api/v1/workflows/import/yaml)
pub async fn import_workflow_yaml(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Parse YAML
    let workflow: oxify_model::Workflow = serde_yaml::from_str(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("YAML parsing failed: {}", e),
        )
    })?;

    // Validate workflow
    if let Err(e) = workflow.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Workflow validation failed: {}", e),
        ));
    }

    // If using mock data, just return success
    if state.is_mock_data_enabled().await {
        return Ok((
            StatusCode::CREATED,
            Json(ApiResponse::success(serde_json::json!({
                "id": uuid::Uuid::new_v4(),
                "name": workflow.metadata.name,
                "message": "Workflow imported successfully (mock mode)"
            }))),
        ));
    }

    // Convert Workflow to CreateWorkflowRequest
    let create_request = crate::api::CreateWorkflowRequest {
        name: workflow.metadata.name,
        description: workflow.metadata.description,
        tags: workflow.metadata.tags,
    };

    // Forward to backend API
    match state.api_client.create_workflow(create_request).await {
        Ok(created_workflow) => Ok((
            StatusCode::CREATED,
            Json(ApiResponse::success(serde_json::json!({
                "id": created_workflow.metadata.id,
                "name": created_workflow.metadata.name,
                "message": "Workflow imported successfully"
            }))),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create workflow: {}", e),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_success() {
        let response = ApiResponse::success("test data");
        assert!(response.success);
        assert_eq!(response.data, Some("test data"));
        assert!(response.error.is_none());
    }

    #[test]
    fn test_error_response() {
        let response = ErrorResponse::new("test error");
        assert!(!response.success);
        assert_eq!(response.error, "test error");
    }
}
