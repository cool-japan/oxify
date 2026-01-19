//! API client for communication with the OxiFY backend
//!
//! This module provides a typed HTTP client for interacting with the backend API.

use oxify_model::{ExecutionContext, ExecutionState, Workflow, WorkflowId};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API client for the OxiFY backend
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
}

/// Workflow summary for list views
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub node_count: u32,
    pub status: String,
    pub last_run: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Execution summary for list views
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub status: String,
    pub progress: u32,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration: Option<String>,
}

/// Dashboard statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub active_workflows: u32,
    pub running_executions: u32,
    pub completed_today: u32,
    pub failed_today: u32,
    pub recent_workflows: Vec<WorkflowSummary>,
    pub recent_executions: Vec<ExecutionSummary>,
}

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub has_more: bool,
}

/// Query parameters for workflow listing
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowListQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub sort: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// Query parameters for execution listing
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionListQuery {
    pub workflow_id: Option<Uuid>,
    pub status: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// Request to create a new workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/// Request to update a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub nodes: Option<serde_json::Value>,
    pub edges: Option<serde_json::Value>,
}

/// Request to start a workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartExecutionRequest {
    pub workflow_id: Uuid,
    pub variables: Option<serde_json::Value>,
}

/// API error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    pub error: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

/// API client errors
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Deserialization error: {0}")]
    Deserialize(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,
}

impl ApiClient {
    /// Create a new API client
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Create with custom client
    pub fn with_client(base_url: String, client: Client) -> Self {
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    // =========================================================================
    // Dashboard
    // =========================================================================

    /// Get dashboard statistics
    pub async fn get_dashboard_stats(&self) -> Result<DashboardStats, ApiError> {
        let url = format!("{}/api/v1/dashboard", self.base_url);
        let response = self.client.get(&url).send().await?;
        self.handle_response(response).await
    }

    // =========================================================================
    // Workflows
    // =========================================================================

    /// List workflows with optional filtering
    pub async fn list_workflows(
        &self,
        query: WorkflowListQuery,
    ) -> Result<PaginatedResponse<WorkflowSummary>, ApiError> {
        let url = format!("{}/api/v1/workflows", self.base_url);
        let response = self.client.get(&url).query(&query).send().await?;
        self.handle_response(response).await
    }

    /// Get a single workflow by ID
    pub async fn get_workflow(&self, id: WorkflowId) -> Result<Workflow, ApiError> {
        let url = format!("{}/api/v1/workflows/{}", self.base_url, id);
        let response = self.client.get(&url).send().await?;
        self.handle_response(response).await
    }

    /// Get workflow summary (metadata only)
    pub async fn get_workflow_summary(&self, id: WorkflowId) -> Result<WorkflowSummary, ApiError> {
        let url = format!("{}/api/v1/workflows/{}/summary", self.base_url, id);
        let response = self.client.get(&url).send().await?;
        self.handle_response(response).await
    }

    /// Create a new workflow
    pub async fn create_workflow(
        &self,
        request: CreateWorkflowRequest,
    ) -> Result<Workflow, ApiError> {
        let url = format!("{}/api/v1/workflows", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;
        self.handle_response(response).await
    }

    /// Update an existing workflow
    pub async fn update_workflow(
        &self,
        id: WorkflowId,
        request: UpdateWorkflowRequest,
    ) -> Result<Workflow, ApiError> {
        let url = format!("{}/api/v1/workflows/{}", self.base_url, id);
        let response = self.client.put(&url).json(&request).send().await?;
        self.handle_response(response).await
    }

    /// Delete a workflow
    pub async fn delete_workflow(&self, id: WorkflowId) -> Result<(), ApiError> {
        let url = format!("{}/api/v1/workflows/{}", self.base_url, id);
        let response = self.client.delete(&url).send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(self.extract_error(response).await)
        }
    }

    // =========================================================================
    // Executions
    // =========================================================================

    /// List executions with optional filtering
    pub async fn list_executions(
        &self,
        query: ExecutionListQuery,
    ) -> Result<PaginatedResponse<ExecutionSummary>, ApiError> {
        let url = format!("{}/api/v1/executions", self.base_url);
        let response = self.client.get(&url).query(&query).send().await?;
        self.handle_response(response).await
    }

    /// Get a single execution by ID
    pub async fn get_execution(&self, id: Uuid) -> Result<ExecutionContext, ApiError> {
        let url = format!("{}/api/v1/executions/{}", self.base_url, id);
        let response = self.client.get(&url).send().await?;
        self.handle_response(response).await
    }

    /// Get execution summary (metadata only)
    pub async fn get_execution_summary(&self, id: Uuid) -> Result<ExecutionSummary, ApiError> {
        let url = format!("{}/api/v1/executions/{}/summary", self.base_url, id);
        let response = self.client.get(&url).send().await?;
        self.handle_response(response).await
    }

    /// Start a new workflow execution
    pub async fn start_execution(
        &self,
        request: StartExecutionRequest,
    ) -> Result<ExecutionContext, ApiError> {
        let url = format!("{}/api/v1/executions", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;
        self.handle_response(response).await
    }

    /// Cancel a running execution
    pub async fn cancel_execution(&self, id: Uuid) -> Result<(), ApiError> {
        let url = format!("{}/api/v1/executions/{}/cancel", self.base_url, id);
        let response = self.client.post(&url).send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(self.extract_error(response).await)
        }
    }

    /// Pause a running execution
    pub async fn pause_execution(&self, id: Uuid) -> Result<(), ApiError> {
        let url = format!("{}/api/v1/executions/{}/pause", self.base_url, id);
        let response = self.client.post(&url).send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(self.extract_error(response).await)
        }
    }

    /// Resume a paused execution
    pub async fn resume_execution(&self, id: Uuid) -> Result<(), ApiError> {
        let url = format!("{}/api/v1/executions/{}/resume", self.base_url, id);
        let response = self.client.post(&url).send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(self.extract_error(response).await)
        }
    }

    /// Get execution logs
    pub async fn get_execution_logs(
        &self,
        id: Uuid,
        limit: Option<u32>,
    ) -> Result<Vec<LogEntry>, ApiError> {
        let url = format!("{}/api/v1/executions/{}/logs", self.base_url, id);
        let mut request = self.client.get(&url);
        if let Some(limit) = limit {
            request = request.query(&[("limit", limit)]);
        }
        let response = request.send().await?;
        self.handle_response(response).await
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ApiError> {
        let status = response.status();

        if status.is_success() {
            response
                .json::<T>()
                .await
                .map_err(|e| ApiError::Deserialize(e.to_string()))
        } else {
            Err(self.extract_error(response).await)
        }
    }

    async fn extract_error(&self, response: reqwest::Response) -> ApiError {
        let status = response.status().as_u16();

        match status {
            401 => ApiError::Unauthorized,
            403 => ApiError::Forbidden,
            404 => ApiError::NotFound("Resource not found".to_string()),
            _ => {
                let message = response
                    .json::<ApiErrorResponse>()
                    .await
                    .map(|e| e.message)
                    .unwrap_or_else(|_| "Unknown error".to_string());

                ApiError::Api { status, message }
            }
        }
    }
}

/// Log entry from execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub node_id: Option<Uuid>,
    pub details: Option<serde_json::Value>,
}

/// Helper to convert ExecutionState to string
pub fn execution_state_to_string(state: &ExecutionState) -> String {
    match state {
        ExecutionState::Running => "running".to_string(),
        ExecutionState::Completed => "completed".to_string(),
        ExecutionState::Failed(msg) => format!("failed: {}", msg),
        ExecutionState::Cancelled => "cancelled".to_string(),
        ExecutionState::Paused => "paused".to_string(),
    }
}

/// Helper to get status CSS class
pub fn execution_status_class(status: &str) -> &'static str {
    if status == "running" {
        "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200"
    } else if status == "completed" {
        "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
    } else if status == "failed" || status.starts_with("failed:") {
        "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200"
    } else if status == "cancelled" {
        "bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300"
    } else if status == "paused" {
        "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200"
    } else {
        "bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300"
    }
}

/// Helper to get workflow status CSS class
pub fn workflow_status_class(status: &str) -> &'static str {
    match status {
        "active" => "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200",
        "draft" => "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200",
        "archived" => "bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300",
        "disabled" => "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200",
        _ => "bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300",
    }
}
