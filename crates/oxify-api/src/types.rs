//! API request and response types

use oxify_model::{ExecutionState, Workflow, WorkflowId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Health check response
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Create workflow request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkflowRequest {
    pub workflow: Workflow,
}

/// Create workflow response
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateWorkflowResponse {
    #[schema(value_type = String)]
    pub id: WorkflowId,
    pub message: String,
}

/// Update workflow request
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateWorkflowRequest {
    pub workflow: Workflow,
}

/// Update workflow response
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateWorkflowResponse {
    pub message: String,
}

/// Get workflow response
#[derive(Debug, Serialize, ToSchema)]
pub struct GetWorkflowResponse {
    pub workflow: Workflow,
}

/// List workflows response
#[derive(Debug, Serialize, ToSchema)]
pub struct ListWorkflowsResponse {
    pub workflows: Vec<Workflow>,
    pub total: usize,
}

/// Delete workflow response
#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteWorkflowResponse {
    pub message: String,
}

/// Execute workflow request
#[derive(Debug, Deserialize, ToSchema)]
pub struct ExecuteWorkflowRequest {
    /// Initial variables for the execution context
    #[serde(default)]
    pub variables: serde_json::Map<String, serde_json::Value>,
}

/// Execute workflow response
#[derive(Debug, Serialize, ToSchema)]
pub struct ExecuteWorkflowResponse {
    #[schema(value_type = String)]
    pub execution_id: Uuid,
    pub message: String,
}

/// Get execution status response
#[derive(Debug, Serialize, ToSchema)]
pub struct GetExecutionResponse {
    #[schema(value_type = String)]
    pub execution_id: Uuid,
    #[schema(value_type = String)]
    pub workflow_id: WorkflowId,
    pub state: ExecutionState,
    pub variables: serde_json::Map<String, serde_json::Value>,
    pub node_results: Vec<serde_json::Value>,
}

/// List executions response
#[derive(Debug, Serialize, ToSchema)]
pub struct ListExecutionsResponse {
    pub executions: Vec<ExecutionSummary>,
    pub total: usize,
}

/// Execution summary
#[derive(Debug, Serialize, ToSchema)]
pub struct ExecutionSummary {
    #[schema(value_type = String)]
    pub execution_id: Uuid,
    #[schema(value_type = String)]
    pub workflow_id: WorkflowId,
    pub state: ExecutionState,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Error response
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
