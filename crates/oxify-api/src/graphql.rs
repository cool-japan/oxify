//! GraphQL API implementation for OxiFY
//!
//! Provides a flexible GraphQL interface for workflow and execution management.
//!
//! # Features
//!
//! Enable the `graphql` feature to use this module:
//!
//! ```toml
//! oxify-api = { version = "0.1", features = ["graphql"] }
//! ```
//!
//! # Endpoints
//!
//! - `GET /graphql` - GraphQL Playground
//! - `POST /graphql` - GraphQL queries and mutations

use crate::handlers::AppState;
use async_graphql::{
    Context, EmptySubscription, Enum, InputObject, Object, Result, Schema, SimpleObject, ID,
};
use chrono::{DateTime, Utc};
use oxify_model::{ExecutionState, Workflow};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use uuid::Uuid;

/// GraphQL schema type alias
pub type OxifySchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

/// Create the GraphQL schema with the application state
pub fn create_schema(state: Arc<AppState>) -> OxifySchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(state)
        .finish()
}

// =============================================================================
// GraphQL Types
// =============================================================================

/// GraphQL representation of a Workflow
#[derive(SimpleObject)]
pub struct GqlWorkflow {
    /// Unique workflow identifier
    pub id: ID,
    /// Workflow name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Workflow version
    pub version: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Number of nodes in the workflow
    pub node_count: i32,
    /// Number of edges in the workflow
    pub edge_count: i32,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Raw workflow JSON for full details
    pub raw_json: String,
}

impl From<&Workflow> for GqlWorkflow {
    fn from(w: &Workflow) -> Self {
        Self {
            id: ID(w.metadata.id.to_string()),
            name: w.metadata.name.clone(),
            description: w.metadata.description.clone(),
            version: w.metadata.version.clone(),
            tags: w.metadata.tags.clone(),
            node_count: w.nodes.len() as i32,
            edge_count: w.edges.len() as i32,
            created_at: w.metadata.created_at,
            updated_at: w.metadata.updated_at,
            raw_json: serde_json::to_string(w).unwrap_or_default(),
        }
    }
}

/// GraphQL execution state enum
#[derive(Debug, Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlExecutionState {
    /// Execution is currently running
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed with an error
    Failed,
    /// Execution was cancelled
    Cancelled,
    /// Execution is paused
    Paused,
}

impl From<ExecutionState> for GqlExecutionState {
    fn from(state: ExecutionState) -> Self {
        match state {
            ExecutionState::Running => GqlExecutionState::Running,
            ExecutionState::Completed => GqlExecutionState::Completed,
            ExecutionState::Failed(_) => GqlExecutionState::Failed,
            ExecutionState::Cancelled => GqlExecutionState::Cancelled,
            ExecutionState::Paused => GqlExecutionState::Paused,
        }
    }
}

/// GraphQL representation of an Execution
#[derive(SimpleObject)]
pub struct GqlExecution {
    /// Unique execution identifier
    pub id: ID,
    /// Associated workflow ID
    pub workflow_id: ID,
    /// Current execution state
    pub state: GqlExecutionState,
    /// Execution variables as JSON string
    pub variables: String,
    /// Node results as JSON string
    pub node_results: String,
    /// When the execution started
    pub started_at: Option<String>,
    /// When the execution completed
    pub completed_at: Option<String>,
}

/// GraphQL representation of an Execution Summary (for list views)
#[derive(SimpleObject)]
pub struct GqlExecutionSummary {
    /// Unique execution identifier
    pub id: ID,
    /// Associated workflow ID
    pub workflow_id: ID,
    /// Current execution state
    pub state: GqlExecutionState,
    /// When the execution started
    pub started_at: Option<String>,
    /// When the execution completed
    pub completed_at: Option<String>,
}

// =============================================================================
// Query Root
// =============================================================================

/// GraphQL Query Root
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get a single workflow by ID
    async fn workflow(&self, ctx: &Context<'_>, id: ID) -> Result<Option<GqlWorkflow>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let workflow_id = Uuid::parse_str(&id).map_err(|_| "Invalid workflow ID")?;

        let workflow = state
            .workflow_store
            .get(&workflow_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(workflow.map(|w| GqlWorkflow::from(&w)))
    }

    /// List all workflows with optional filtering
    async fn workflows(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by name (partial match)")] name: Option<String>,
        #[graphql(desc = "Filter by tags")] tags: Option<Vec<String>>,
        #[graphql(default = 0, desc = "Skip first N results")] offset: i32,
        #[graphql(default = 100, desc = "Maximum results to return")] limit: i32,
    ) -> Result<Vec<GqlWorkflow>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let all_workflows = state
            .workflow_store
            .list()
            .await
            .map_err(|e| e.to_string())?;

        let filtered: Vec<GqlWorkflow> = all_workflows
            .iter()
            .filter(|w| {
                // Filter by name if provided
                if let Some(ref name_filter) = name {
                    if !w
                        .metadata
                        .name
                        .to_lowercase()
                        .contains(&name_filter.to_lowercase())
                    {
                        return false;
                    }
                }
                // Filter by tags if provided
                if let Some(ref tag_filter) = tags {
                    if !tag_filter.iter().any(|t| w.metadata.tags.contains(t)) {
                        return false;
                    }
                }
                true
            })
            .skip(offset as usize)
            .take(limit as usize)
            .map(GqlWorkflow::from)
            .collect();

        Ok(filtered)
    }

    /// Count total workflows
    async fn workflow_count(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by tags")] tags: Option<Vec<String>>,
    ) -> Result<i32> {
        let state = ctx.data::<Arc<AppState>>()?;
        let all_workflows = state
            .workflow_store
            .list()
            .await
            .map_err(|e| e.to_string())?;

        let count = all_workflows
            .iter()
            .filter(|w| {
                if let Some(ref tag_filter) = tags {
                    tag_filter.iter().any(|t| w.metadata.tags.contains(t))
                } else {
                    true
                }
            })
            .count();

        Ok(count as i32)
    }

    /// Get a single execution by ID
    async fn execution(&self, ctx: &Context<'_>, id: ID) -> Result<Option<GqlExecution>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let execution_id = Uuid::parse_str(&id).map_err(|_| "Invalid execution ID")?;

        let execution = state
            .execution_store
            .get(&execution_id)
            .await
            .map_err(|e| e.to_string())?;

        Ok(execution.map(|e| GqlExecution {
            id: ID(e.execution_id.to_string()),
            workflow_id: ID(e.workflow_id.to_string()),
            state: GqlExecutionState::from(e.state.clone()),
            variables: serde_json::to_string(&e.variables).unwrap_or_default(),
            node_results: serde_json::to_string(&e.node_results).unwrap_or_default(),
            started_at: Some(e.started_at.to_rfc3339()),
            completed_at: e.completed_at.map(|t| t.to_rfc3339()),
        }))
    }

    /// List executions with filtering
    async fn executions(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by workflow ID")] workflow_id: Option<ID>,
        #[graphql(desc = "Filter by execution state")] state_filter: Option<GqlExecutionState>,
        #[graphql(default = 0)] offset: i32,
        #[graphql(default = 100)] limit: i32,
    ) -> Result<Vec<GqlExecutionSummary>> {
        let app_state = ctx.data::<Arc<AppState>>()?;
        let executions = app_state
            .execution_store
            .list()
            .await
            .map_err(|e| e.to_string())?;

        let filtered: Vec<GqlExecutionSummary> = executions
            .iter()
            .filter(|(_, e)| {
                // Filter by workflow_id if provided
                if let Some(ref wf_id) = workflow_id {
                    if e.workflow_id.to_string() != wf_id.as_str() {
                        return false;
                    }
                }
                // Filter by state if provided
                if let Some(ref sf) = state_filter {
                    let exec_state = GqlExecutionState::from(e.state.clone());
                    if exec_state != *sf {
                        return false;
                    }
                }
                true
            })
            .skip(offset as usize)
            .take(limit as usize)
            .map(|(id, e)| GqlExecutionSummary {
                id: ID(id.to_string()),
                workflow_id: ID(e.workflow_id.to_string()),
                state: GqlExecutionState::from(e.state.clone()),
                started_at: Some(e.started_at.to_rfc3339()),
                completed_at: e.completed_at.map(|t| t.to_rfc3339()),
            })
            .collect();

        Ok(filtered)
    }

    /// Count total executions
    async fn execution_count(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by workflow ID")] workflow_id: Option<ID>,
        #[graphql(desc = "Filter by execution state")] state_filter: Option<GqlExecutionState>,
    ) -> Result<i32> {
        let app_state = ctx.data::<Arc<AppState>>()?;
        let executions = app_state
            .execution_store
            .list()
            .await
            .map_err(|e| e.to_string())?;

        let count = executions
            .iter()
            .filter(|(_, e)| {
                if let Some(ref wf_id) = workflow_id {
                    if e.workflow_id.to_string() != wf_id.as_str() {
                        return false;
                    }
                }
                if let Some(ref sf) = state_filter {
                    let exec_state = GqlExecutionState::from(e.state.clone());
                    if exec_state != *sf {
                        return false;
                    }
                }
                true
            })
            .count();

        Ok(count as i32)
    }

    /// API health check
    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            status: "healthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
}

/// Health status type
#[derive(SimpleObject)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
}

// =============================================================================
// Mutation Root
// =============================================================================

/// GraphQL Mutation Root
pub struct MutationRoot;

/// Input for creating a workflow
#[derive(InputObject)]
pub struct CreateWorkflowInput {
    /// Workflow name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Optional tags
    pub tags: Option<Vec<String>>,
    /// Raw workflow JSON (includes nodes, edges, etc.)
    pub workflow_json: String,
}

/// Input for executing a workflow
#[derive(InputObject)]
pub struct ExecuteWorkflowInput {
    /// Workflow ID to execute
    pub workflow_id: ID,
    /// Optional initial variables as JSON
    pub variables_json: Option<String>,
}

/// Result of creating a workflow
#[derive(SimpleObject)]
pub struct CreateWorkflowResult {
    /// Created workflow ID
    pub id: ID,
    /// Success message
    pub message: String,
}

/// Result of executing a workflow
#[derive(SimpleObject)]
pub struct ExecuteWorkflowResult {
    /// Execution ID
    pub execution_id: ID,
    /// Success message
    pub message: String,
}

/// Result of deleting a workflow
#[derive(SimpleObject)]
pub struct DeleteWorkflowResult {
    /// Success status
    pub success: bool,
    /// Message
    pub message: String,
}

/// Result of cancelling an execution
#[derive(SimpleObject)]
pub struct CancelExecutionResult {
    /// Success status
    pub success: bool,
    /// Message
    pub message: String,
}

#[Object]
impl MutationRoot {
    /// Create a new workflow
    async fn create_workflow(
        &self,
        ctx: &Context<'_>,
        input: CreateWorkflowInput,
    ) -> Result<CreateWorkflowResult> {
        let state = ctx.data::<Arc<AppState>>()?;

        // Parse the workflow JSON
        let mut workflow: Workflow = serde_json::from_str(&input.workflow_json)
            .map_err(|e| format!("Invalid workflow JSON: {}", e))?;

        // Update metadata with input values
        workflow.metadata.name = input.name;
        if let Some(desc) = input.description {
            workflow.metadata.description = Some(desc);
        }
        if let Some(tags) = input.tags {
            workflow.metadata.tags = tags;
        }

        // Validate workflow
        workflow
            .validate()
            .map_err(|e| format!("Workflow validation failed: {}", e))?;

        // Store workflow
        let id = state
            .workflow_store
            .create(workflow.clone())
            .await
            .map_err(|e| e.to_string())?;

        Ok(CreateWorkflowResult {
            id: ID(id.to_string()),
            message: "Workflow created successfully".to_string(),
        })
    }

    /// Update an existing workflow
    async fn update_workflow(
        &self,
        ctx: &Context<'_>,
        id: ID,
        workflow_json: String,
    ) -> Result<CreateWorkflowResult> {
        let state = ctx.data::<Arc<AppState>>()?;
        let workflow_id = Uuid::parse_str(&id).map_err(|_| "Invalid workflow ID")?;

        // Check if workflow exists
        if state
            .workflow_store
            .get(&workflow_id)
            .await
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err("Workflow not found".into());
        }

        // Parse the workflow JSON
        let mut workflow: Workflow = serde_json::from_str(&workflow_json)
            .map_err(|e| format!("Invalid workflow JSON: {}", e))?;

        // Ensure the ID matches
        workflow.metadata.id = workflow_id;
        workflow.metadata.updated_at = Utc::now();

        // Validate workflow
        workflow
            .validate()
            .map_err(|e| format!("Workflow validation failed: {}", e))?;

        // Update workflow
        state
            .workflow_store
            .update(&workflow_id, workflow)
            .await
            .map_err(|e| e.to_string())?;

        Ok(CreateWorkflowResult {
            id: ID(workflow_id.to_string()),
            message: "Workflow updated successfully".to_string(),
        })
    }

    /// Delete a workflow
    async fn delete_workflow(&self, ctx: &Context<'_>, id: ID) -> Result<DeleteWorkflowResult> {
        let state = ctx.data::<Arc<AppState>>()?;
        let workflow_id = Uuid::parse_str(&id).map_err(|_| "Invalid workflow ID")?;

        // Check if workflow exists
        if state
            .workflow_store
            .get(&workflow_id)
            .await
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err("Workflow not found".into());
        }

        // Delete workflow
        state
            .workflow_store
            .delete(&workflow_id)
            .await
            .map_err(|e| e.to_string())?;

        Ok(DeleteWorkflowResult {
            success: true,
            message: "Workflow deleted successfully".to_string(),
        })
    }

    /// Execute a workflow
    async fn execute_workflow(
        &self,
        ctx: &Context<'_>,
        input: ExecuteWorkflowInput,
    ) -> Result<ExecuteWorkflowResult> {
        let state = ctx.data::<Arc<AppState>>()?;
        let workflow_id = Uuid::parse_str(&input.workflow_id).map_err(|_| "Invalid workflow ID")?;

        // Get workflow
        let workflow = state
            .workflow_store
            .get(&workflow_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Workflow not found")?;

        // Parse variables if provided
        let variables: serde_json::Map<String, JsonValue> =
            if let Some(vars_json) = input.variables_json {
                serde_json::from_str(&vars_json)
                    .map_err(|e| format!("Invalid variables JSON: {}", e))?
            } else {
                serde_json::Map::new()
            };

        // Create execution context
        let mut execution_ctx = oxify_model::ExecutionContext::new(workflow_id);
        for (key, value) in variables {
            execution_ctx.variables.insert(key, value);
        }

        // Store execution record
        let execution_id = state
            .execution_store
            .create(execution_ctx.clone())
            .await
            .map_err(|e| e.to_string())?;

        // Increment active executions counter
        state.http_metrics.inc_active_execution();

        // Execute workflow asynchronously
        let state_clone = state.clone();
        let engine_clone = state.engine.clone();
        let metrics = state.http_metrics.clone();
        tokio::spawn(async move {
            let result = engine_clone.execute(&workflow).await;

            // Decrement active executions on completion/failure
            metrics.dec_active_execution();

            // Update execution record with result
            let mut updated_ctx = execution_ctx.clone();
            updated_ctx.state = match result {
                Ok(_) => ExecutionState::Completed,
                Err(e) => ExecutionState::Failed(e.to_string()),
            };
            updated_ctx.completed_at = Some(Utc::now());

            let _ = state_clone
                .execution_store
                .update(&execution_id, updated_ctx)
                .await;
        });

        Ok(ExecuteWorkflowResult {
            execution_id: ID(execution_id.to_string()),
            message: "Execution started".to_string(),
        })
    }

    /// Cancel a running execution
    async fn cancel_execution(&self, ctx: &Context<'_>, id: ID) -> Result<CancelExecutionResult> {
        let state = ctx.data::<Arc<AppState>>()?;
        let execution_id = Uuid::parse_str(&id).map_err(|_| "Invalid execution ID")?;

        // Get execution
        let mut execution = state
            .execution_store
            .get(&execution_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Execution not found")?;

        // Check if execution can be cancelled
        match execution.state {
            ExecutionState::Running | ExecutionState::Paused => {
                execution.state = ExecutionState::Cancelled;
                execution.completed_at = Some(Utc::now());
                state
                    .execution_store
                    .update(&execution_id, execution)
                    .await
                    .map_err(|e| e.to_string())?;

                // Decrement active executions counter
                state.http_metrics.dec_active_execution();

                Ok(CancelExecutionResult {
                    success: true,
                    message: "Execution cancelled".to_string(),
                })
            }
            _ => Err(format!("Cannot cancel execution in state: {:?}", execution.state).into()),
        }
    }
}

// =============================================================================
// Axum Integration
// =============================================================================

use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::Extension,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};

/// Handle GraphQL requests
pub async fn graphql_handler(
    Extension(schema): Extension<OxifySchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

/// GraphQL Playground HTML
pub async fn graphql_playground() -> impl IntoResponse {
    Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql"),
    ))
}

/// Create a Router with GraphQL endpoints
pub fn graphql_router(schema: OxifySchema) -> Router {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/graphql", get(graphql_playground))
        .layer(Extension(schema))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::WorkflowBuilder;

    fn create_test_workflow() -> Workflow {
        WorkflowBuilder::new("Test Workflow")
            .description("A test workflow")
            .tag("test")
            .start("Start")
            .end("End")
            .build()
    }

    #[test]
    fn test_gql_workflow_from_workflow() {
        let workflow = create_test_workflow();
        let gql_workflow = GqlWorkflow::from(&workflow);

        assert_eq!(gql_workflow.name, "Test Workflow");
        assert_eq!(
            gql_workflow.description,
            Some("A test workflow".to_string())
        );
        assert_eq!(gql_workflow.node_count, 2);
        assert_eq!(gql_workflow.edge_count, 1);
        assert!(gql_workflow.tags.contains(&"test".to_string()));
    }

    #[test]
    fn test_gql_execution_state_from() {
        assert_eq!(
            GqlExecutionState::from(ExecutionState::Running),
            GqlExecutionState::Running
        );
        assert_eq!(
            GqlExecutionState::from(ExecutionState::Completed),
            GqlExecutionState::Completed
        );
        assert_eq!(
            GqlExecutionState::from(ExecutionState::Failed("error".to_string())),
            GqlExecutionState::Failed
        );
        assert_eq!(
            GqlExecutionState::from(ExecutionState::Cancelled),
            GqlExecutionState::Cancelled
        );
        assert_eq!(
            GqlExecutionState::from(ExecutionState::Paused),
            GqlExecutionState::Paused
        );
    }

    #[test]
    fn test_create_workflow_input() {
        let input = CreateWorkflowInput {
            name: "Test".to_string(),
            description: Some("Description".to_string()),
            tags: Some(vec!["tag1".to_string(), "tag2".to_string()]),
            workflow_json: "{}".to_string(),
        };

        assert_eq!(input.name, "Test");
        assert_eq!(input.description, Some("Description".to_string()));
        assert_eq!(input.tags.unwrap().len(), 2);
    }

    #[test]
    fn test_execute_workflow_input() {
        let input = ExecuteWorkflowInput {
            workflow_id: ID("123".to_string()),
            variables_json: Some(r#"{"key": "value"}"#.to_string()),
        };

        assert_eq!(input.workflow_id.as_str(), "123");
        assert!(input.variables_json.is_some());
    }

    #[test]
    fn test_health_status() {
        let status = HealthStatus {
            status: "healthy".to_string(),
            version: "0.1.0".to_string(),
        };

        assert_eq!(status.status, "healthy");
        assert_eq!(status.version, "0.1.0");
    }

    #[test]
    fn test_create_workflow_result() {
        let result = CreateWorkflowResult {
            id: ID("abc123".to_string()),
            message: "Created".to_string(),
        };

        assert_eq!(result.id.as_str(), "abc123");
        assert_eq!(result.message, "Created");
    }
}
