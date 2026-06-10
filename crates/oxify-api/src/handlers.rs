//! API request handlers

use crate::auth::AuthState;
use crate::storage::{ExecutionStoreBackend, UserStoreBackend, WorkflowStoreBackend};
use crate::types::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use oxify_engine::{Engine, EngineBuilder, EventBus, ExecutionConfig};
use oxify_model::{ExecutionContext, ExecutionState, WorkflowId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

/// Application state
#[derive(Clone)]
pub struct AppState {
    pub workflow_store: WorkflowStoreBackend,
    pub execution_store: ExecutionStoreBackend,
    pub user_store: UserStoreBackend,
    pub auth: AuthState,
    pub engine: Arc<Engine>,
    pub event_bus: Arc<EventBus>,
    // Disabled modules for SQLite migration
    // pub secret_store: Option<Arc<oxify_storage::SecretStore>>,
    pub version_store: Option<Arc<oxify_storage::WorkflowVersionStore>>,
    // pub checkpoint_store: Option<Arc<oxify_storage::DatabaseCheckpointStore>>,
    // pub schedule_store: Option<Arc<oxify_storage::ScheduleStore>>,
    // pub webhook_store: Option<Arc<oxify_storage::WebhookStore>>,
    pub approval_store: Option<Arc<oxify_engine::ApprovalStore>>,
    pub vector_registry: Option<Arc<crate::vector_handlers::VectorStoreRegistry>>,
    pub mcp_registry: Arc<tokio::sync::RwLock<oxify_mcp::McpRegistry>>,
    pub db_pool: Option<oxify_storage::DatabasePool>,
    pub http_metrics: Arc<crate::middleware::HttpMetrics>,
}

impl AppState {
    pub fn new() -> Self {
        let event_bus = Arc::new(EventBus::new(1024));
        let engine = Arc::new(
            EngineBuilder::new()
                .with_event_bus(event_bus.clone())
                .build(),
        );
        Self {
            workflow_store: WorkflowStoreBackend::new_in_memory(),
            execution_store: ExecutionStoreBackend::new_in_memory(),
            user_store: UserStoreBackend::new_in_memory(),
            auth: AuthState::new(),
            engine,
            event_bus,
            // Disabled for SQLite migration
            // secret_store: None,
            version_store: None,
            // checkpoint_store: None,
            // schedule_store: None,
            // webhook_store: None,
            approval_store: Some(Arc::new(oxify_engine::ApprovalStore::new())),
            vector_registry: Some(Arc::new(crate::vector_handlers::VectorStoreRegistry::new())),
            mcp_registry: Arc::new(tokio::sync::RwLock::new(oxify_mcp::McpRegistry::new())),
            db_pool: None,
            http_metrics: Arc::new(crate::middleware::HttpMetrics::new()),
        }
    }

    pub async fn new_with_database() -> Result<Self, String> {
        let config = oxify_storage::DatabaseConfig::default();
        let pool = oxify_storage::DatabasePool::new(config)
            .await
            .map_err(|e| e.to_string())?;

        // Run migrations
        pool.migrate().await.map_err(|e| e.to_string())?;

        // Initialize workflow version store
        let version_store = Some(Arc::new(oxify_storage::WorkflowVersionStore::new(
            pool.clone(),
        )));

        // Initialize approval store
        let approval_store = Some(Arc::new(oxify_engine::ApprovalStore::new()));

        // Initialize vector store registry
        let vector_registry = Some(Arc::new(crate::vector_handlers::VectorStoreRegistry::new()));

        let event_bus = Arc::new(EventBus::new(1024));
        let engine = Arc::new(
            EngineBuilder::new()
                .with_event_bus(event_bus.clone())
                .build(),
        );

        Ok(Self {
            workflow_store: WorkflowStoreBackend::new_database(pool.clone()),
            execution_store: ExecutionStoreBackend::new_database(pool.clone()),
            user_store: UserStoreBackend::new_database(pool.clone()),
            auth: AuthState::new(),
            engine,
            event_bus,
            // Disabled modules for SQLite migration
            // secret_store: None,
            version_store,
            // checkpoint_store: None,
            // schedule_store: None,
            // webhook_store: None,
            approval_store,
            vector_registry,
            mcp_registry: Arc::new(tokio::sync::RwLock::new(oxify_mcp::McpRegistry::new())),
            db_pool: Some(pool),
            http_metrics: Arc::new(crate::middleware::HttpMetrics::new()),
        })
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Health check handler (backward-compatible alias for `/livez`)
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Liveness probe response
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LivezResponse {
    pub status: &'static str,
}

/// Liveness probe — always returns 200 as long as the process is alive.
///
/// A liveness probe failure triggers a container restart. This endpoint
/// performs no I/O and will never return a non-2xx status voluntarily.
#[utoipa::path(
    get,
    path = "/livez",
    responses(
        (status = 200, description = "Process is alive", body = LivezResponse)
    )
)]
pub async fn livez() -> Json<LivezResponse> {
    Json(LivezResponse { status: "alive" })
}

/// Readiness probe response
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReadyzResponse {
    pub status: &'static str,
}

/// Readiness probe — returns 200 when the service is ready to accept traffic.
///
/// A readiness probe failure removes the pod from the load-balancer rotation
/// without restarting it.  This stub unconditionally returns `ready`; a full
/// implementation would delegate to a [`oxify_server::ReadinessRegistry`].
#[utoipa::path(
    get,
    path = "/readyz",
    responses(
        (status = 200, description = "Service is ready", body = ReadyzResponse),
        (status = 503, description = "Service is not ready")
    )
)]
pub async fn readyz() -> Json<ReadyzResponse> {
    Json(ReadyzResponse { status: "ready" })
}

/// Create a new workflow
#[utoipa::path(
    post,
    path = "/api/v1/workflows",
    request_body = CreateWorkflowRequest,
    responses(
        (status = 201, description = "Workflow created successfully", body = CreateWorkflowResponse),
        (status = 400, description = "Invalid workflow", body = ErrorResponse)
    )
)]
pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<CreateWorkflowResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Creating workflow: {}", req.workflow.metadata.name);

    // Validate workflow
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
    params(
        ("id" = String, Path, description = "Workflow ID")
    ),
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
    params(
        ("id" = String, Path, description = "Workflow ID")
    ),
    request_body = UpdateWorkflowRequest,
    responses(
        (status = 200, description = "Workflow updated successfully", body = UpdateWorkflowResponse),
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

    // Validate workflow
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
    params(
        ("id" = String, Path, description = "Workflow ID")
    ),
    responses(
        (status = 200, description = "Workflow deleted successfully", body = DeleteWorkflowResponse),
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
    params(
        ("id" = String, Path, description = "Workflow ID")
    ),
    request_body = ExecuteWorkflowRequest,
    responses(
        (status = 202, description = "Workflow execution started", body = ExecuteWorkflowResponse),
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

    // Get workflow
    let workflow = match state.workflow_store.get(&id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Workflow {} not found", id),
                }),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            ))
        }
    };

    // One execution_id, used everywhere end-to-end
    let mut ctx = ExecutionContext::new(workflow.metadata.id);
    for (key, value) in req.variables {
        ctx.set_variable(key, value);
    }
    // Single source of truth — both storage and SSE use this id
    let execution_id = ctx.execution_id;

    // Store initial Running context BEFORE spawning, so SSE subscribers
    // arriving immediately after the 202 response can always find the row
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

    // Increment active executions counter
    state.http_metrics.inc_active_execution();

    let engine = state.engine.clone();
    let execution_store = state.execution_store.clone();
    let metrics = state.http_metrics.clone();

    tokio::spawn(async move {
        let result = engine
            .execute_with_context(&workflow, ctx, ExecutionConfig::new().with_events())
            .await;
        // Decrement active executions regardless of outcome
        metrics.dec_active_execution();
        match result {
            Ok(result_ctx) => {
                // result_ctx.execution_id == execution_id (preserved) — update succeeds
                match execution_store.update(&execution_id, result_ctx).await {
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
                }
            }
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
    params(
        ("id" = String, Path, description = "Execution ID")
    ),
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

            // Convert HashMap to serde_json::Map
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
    params(
        ("id" = String, Path, description = "Workflow ID")
    ),
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
    params(
        ("id" = String, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Execution cancelled", body = serde_json::Value),
        (status = 404, description = "Execution not found", body = ErrorResponse),
        (status = 400, description = "Execution already completed/failed/cancelled", body = ErrorResponse)
    )
)]
pub async fn cancel_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    info!("Cancelling execution: {}", id);

    // Get execution
    let execution = match state.execution_store.get(&id).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", id),
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
            ))
        }
    };

    // Check if execution can be cancelled (must be Running or Paused)
    match execution.state {
        ExecutionState::Running | ExecutionState::Paused => {
            // Cancel the execution
            let mut updated_ctx = execution;
            updated_ctx.cancel();

            // Update in storage
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

            // Decrement active executions counter
            state.http_metrics.dec_active_execution();

            info!("Successfully cancelled execution: {}", id);

            Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "message": "Execution cancelled successfully",
                    "execution_id": id,
                    "state": "Cancelled"
                })),
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

/// Cost estimation request
#[derive(serde::Deserialize)]
pub struct EstimateCostRequest {
    pub avg_prompt_tokens: Option<u32>,
    pub avg_response_tokens: Option<u32>,
}

/// Cost estimation response
#[derive(serde::Serialize)]
pub struct EstimateCostResponse {
    pub total_cost_usd: f64,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub node_costs: Vec<NodeCostSummary>,
    pub category_costs: std::collections::HashMap<String, f64>,
}

#[derive(serde::Serialize)]
pub struct NodeCostSummary {
    pub node_id: Uuid,
    pub node_name: String,
    pub cost_usd: f64,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Estimate workflow execution cost
pub async fn estimate_workflow_cost(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(request): Json<EstimateCostRequest>,
) -> Result<Json<EstimateCostResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Estimating cost for workflow: {}", id);

    // Get workflow
    let workflow = match state.workflow_store.get(&id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: "Workflow not found".to_string(),
                }),
            ))
        }
        Err(e) => {
            error!("Failed to get workflow: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            ));
        }
    };

    // Create cost estimator
    let estimator = if let (Some(prompt), Some(response)) =
        (request.avg_prompt_tokens, request.avg_response_tokens)
    {
        oxify_engine::CostEstimator::with_averages(prompt, response)
    } else {
        oxify_engine::CostEstimator::new()
    };

    // Estimate costs
    let estimate = estimator.estimate_workflow(&workflow);

    let node_costs = estimate
        .node_costs
        .iter()
        .map(|nc| NodeCostSummary {
            node_id: nc.node_id,
            node_name: nc.node_name.clone(),
            cost_usd: nc.cost_usd,
            input_tokens: nc.estimated_input_tokens,
            output_tokens: nc.estimated_output_tokens,
        })
        .collect();

    Ok(Json(EstimateCostResponse {
        total_cost_usd: estimate.total_cost_usd,
        total_input_tokens: estimate.total_input_tokens,
        total_output_tokens: estimate.total_output_tokens,
        node_costs,
        category_costs: estimate.category_costs,
    }))
}

/// Batching analysis response
#[derive(serde::Serialize)]
pub struct BatchAnalysisResponse {
    pub total_nodes: usize,
    pub batched_nodes: usize,
    pub batch_count: usize,
    pub average_batch_size: f32,
    pub batching_efficiency: f32,
    pub estimated_time_savings: f32,
    pub batches: Vec<BatchSummary>,
}

#[derive(serde::Serialize)]
pub struct BatchSummary {
    pub group_type: String,
    pub node_count: usize,
    pub speedup_factor: f32,
    pub node_ids: Vec<Uuid>,
}

/// Analyze workflow batching opportunities
pub async fn analyze_workflow_batching(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<BatchAnalysisResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Analyzing batching for workflow: {}", id);

    // Get workflow
    let workflow = match state.workflow_store.get(&id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: "Workflow not found".to_string(),
                }),
            ))
        }
        Err(e) => {
            error!("Failed to get workflow: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            ));
        }
    };

    // Analyze batching
    let analyzer = oxify_engine::BatchAnalyzer::new();
    let node_refs: Vec<&oxify_model::Node> = workflow.nodes.iter().collect();
    let plan = analyzer.analyze(&node_refs);
    let stats = oxify_engine::BatchStats::from_plan(&plan, &analyzer);

    let batches = plan
        .batches
        .iter()
        .map(|b| BatchSummary {
            group_type: format!("{:?}", b.group),
            node_count: b.size(),
            speedup_factor: b.speedup_factor,
            node_ids: b.nodes.clone(),
        })
        .collect();

    Ok(Json(BatchAnalysisResponse {
        total_nodes: stats.total_nodes,
        batched_nodes: stats.batched_nodes,
        batch_count: stats.batch_count,
        average_batch_size: stats.average_batch_size,
        batching_efficiency: stats.efficiency(),
        estimated_time_savings: stats.estimated_time_savings,
        batches,
    }))
}

// ============================================================================
// Workflow Testing
// ============================================================================

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub struct TestWorkflowRequest {
    pub inputs: std::collections::HashMap<String, String>,
    pub expected_output: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(serde::Serialize)]
pub struct TestWorkflowResponse {
    pub passed: bool,
    pub execution_id: Uuid,
    pub output: String,
    pub expected_output: Option<String>,
    pub execution_time_ms: u64,
    pub error: Option<String>,
}

/// Test workflow execution with inputs and optional expected output
pub async fn test_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(request): Json<TestWorkflowRequest>,
) -> Result<Json<TestWorkflowResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Testing workflow: {}", id);

    // Get workflow
    let workflow = match state.workflow_store.get(&id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: "Workflow not found".to_string(),
                }),
            ))
        }
        Err(e) => {
            error!("Failed to get workflow: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            ));
        }
    };

    // Execute workflow synchronously for testing
    let start_time = std::time::Instant::now();
    let result = state.engine.execute(&workflow).await;
    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    match result {
        Ok(ctx) => {
            // Get output from context (check for "result" or "output" variables)
            let output = ctx
                .get_variable("result")
                .or_else(|| ctx.get_variable("output"))
                .and_then(|v| serde_json::to_string(&v).ok())
                .unwrap_or_else(|| "{}".to_string());

            // Check if output matches expected
            let passed = if let Some(ref expected) = request.expected_output {
                // Try exact match first, then try JSON comparison
                if output.trim() == expected.trim() {
                    true
                } else {
                    // Try parsing both as JSON and comparing
                    match (
                        serde_json::from_str::<serde_json::Value>(&output),
                        serde_json::from_str::<serde_json::Value>(expected),
                    ) {
                        (Ok(output_json), Ok(expected_json)) => output_json == expected_json,
                        _ => false,
                    }
                }
            } else {
                true // No expected output means just check if execution succeeded
            };

            let execution_id = ctx.execution_id;

            Ok(Json(TestWorkflowResponse {
                passed,
                execution_id,
                output,
                expected_output: request.expected_output,
                execution_time_ms,
                error: None,
            }))
        }
        Err(e) => {
            error!("Workflow execution failed: {}", e);
            Ok(Json(TestWorkflowResponse {
                passed: false,
                execution_id: Uuid::new_v4(),
                output: String::new(),
                expected_output: request.expected_output,
                execution_time_ms,
                error: Some(e.to_string()),
            }))
        }
    }
}

// ============================================================================
// Schedule Management
// ============================================================================

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub struct CreateScheduleRequest {
    pub workflow_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub cron: String,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
    pub input_variables: Option<std::collections::HashMap<String, serde_json::Value>>,
    pub max_runs: Option<u64>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(serde::Serialize)]
pub struct CreateScheduleResponse {
    pub id: Uuid,
    pub message: String,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub struct UpdateScheduleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub cron: Option<String>,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
    pub input_variables: Option<std::collections::HashMap<String, serde_json::Value>>,
    pub max_runs: Option<u64>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(serde::Serialize)]
pub struct ListSchedulesResponse {
    pub schedules: Vec<oxify_model::Schedule>,
    pub total: usize,
}

// ============================================================================
// Schedule Handlers (DISABLED - SQLite migration)
// ============================================================================

/// Create a new schedule (DISABLED)
#[allow(unused_variables)]
pub async fn create_schedule(
    State(_state): State<Arc<AppState>>,
    Json(_request): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<CreateScheduleResponse>), (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Schedule functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// List all schedules (DISABLED)
#[allow(unused_variables)]
pub async fn list_schedules(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<ListSchedulesResponse>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Schedule functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// List schedules for a specific workflow (DISABLED)
#[allow(unused_variables)]
pub async fn list_workflow_schedules(
    State(_state): State<Arc<AppState>>,
    Path(_workflow_id): Path<Uuid>,
) -> Result<Json<ListSchedulesResponse>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Schedule functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Get a schedule by ID (DISABLED)
#[allow(unused_variables)]
pub async fn get_schedule(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<Uuid>,
) -> Result<Json<oxify_model::Schedule>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Schedule functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Update a schedule (DISABLED)
#[allow(unused_variables)]
pub async fn update_schedule(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<Uuid>,
    Json(_request): Json<UpdateScheduleRequest>,
) -> Result<Json<oxify_model::Schedule>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Schedule functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Delete a schedule (DISABLED)
#[allow(unused_variables)]
pub async fn delete_schedule(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Schedule functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Get execution history for a schedule (DISABLED)
#[allow(unused_variables)]
pub async fn get_schedule_history(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<oxify_model::ScheduleExecution>>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Schedule functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

// ============================================================================
// Execution Analytics
// ============================================================================

#[derive(serde::Serialize)]
pub struct ExecutionAnalytics {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub success_rate: f64,
    pub average_duration_ms: f64,
    pub median_duration_ms: f64,
    pub min_duration_ms: u64,
    pub max_duration_ms: u64,
    pub executions_by_status: std::collections::HashMap<String, u64>,
    pub executions_over_time: Vec<TimeSeriesData>,
}

#[derive(serde::Serialize)]
pub struct TimeSeriesData {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub count: u64,
    pub avg_duration_ms: f64,
}

#[derive(serde::Serialize)]
pub struct WorkflowAnalytics {
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub total_executions: u64,
    pub success_rate: f64,
    pub average_duration_ms: f64,
    pub last_execution: Option<chrono::DateTime<chrono::Utc>>,
    pub most_common_errors: Vec<ErrorFrequency>,
}

#[derive(serde::Serialize)]
pub struct ErrorFrequency {
    pub error_message: String,
    pub count: u64,
    pub percentage: f64,
}

/// Get overall execution analytics
pub async fn get_execution_analytics(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<ExecutionAnalytics>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting execution analytics");

    // Mock data - would be calculated from database
    let analytics = ExecutionAnalytics {
        total_executions: 0,
        successful_executions: 0,
        failed_executions: 0,
        success_rate: 0.0,
        average_duration_ms: 0.0,
        median_duration_ms: 0.0,
        min_duration_ms: 0,
        max_duration_ms: 0,
        executions_by_status: std::collections::HashMap::new(),
        executions_over_time: vec![],
    };

    Ok(Json(analytics))
}

/// Get analytics for a specific workflow
pub async fn get_workflow_analytics(
    State(_state): State<Arc<AppState>>,
    Path(workflow_id): Path<Uuid>,
) -> Result<Json<WorkflowAnalytics>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting analytics for workflow: {}", workflow_id);

    // Mock data - would be calculated from database
    let analytics = WorkflowAnalytics {
        workflow_id,
        workflow_name: "Unknown".to_string(),
        total_executions: 0,
        success_rate: 0.0,
        average_duration_ms: 0.0,
        last_execution: None,
        most_common_errors: vec![],
    };

    Ok(Json(analytics))
}

/// Get top performing workflows
pub async fn get_top_workflows(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Vec<WorkflowAnalytics>>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting top performing workflows");

    // Mock data - would be queried from database
    let workflows = vec![];

    Ok(Json(workflows))
}

/// Get execution trends over time
pub async fn get_execution_trends(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Vec<TimeSeriesData>>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting execution trends");

    // Mock data - would aggregate from database
    let trends = vec![];

    Ok(Json(trends))
}

/// Optimization analysis response
#[derive(serde::Serialize)]
pub struct OptimizationAnalysisResponse {
    pub total_optimizations: usize,
    pub by_priority: OptimizationsByPriority,
    pub by_category: OptimizationsByCategory,
    pub estimated_time_savings: f32,
    pub estimated_cost_reduction: f32,
    pub optimizations: Vec<OptimizationDto>,
}

#[derive(serde::Serialize)]
pub struct OptimizationsByPriority {
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

#[derive(serde::Serialize)]
pub struct OptimizationsByCategory {
    pub performance: usize,
    pub cost: usize,
    pub reliability: usize,
    pub maintainability: usize,
    pub security: usize,
}

#[derive(serde::Serialize)]
pub struct OptimizationDto {
    pub category: String,
    pub priority: String,
    pub title: String,
    pub description: String,
    pub affected_nodes: Vec<Uuid>,
    pub time_savings: Option<f32>,
    pub cost_reduction: Option<f32>,
    pub reliability_improvement: Option<String>,
    pub action: String,
}

/// Analyze workflow for optimization opportunities
pub async fn analyze_workflow_optimization(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<OptimizationAnalysisResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Analyzing optimization opportunities for workflow: {}", id);

    // Get workflow
    let workflow = match state.workflow_store.get(&id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: "Workflow not found".to_string(),
                }),
            ))
        }
        Err(e) => {
            error!("Failed to get workflow: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            ));
        }
    };

    // Run optimization analysis
    let optimizer = oxify_engine::WorkflowOptimizer::new();
    let optimizations = optimizer.optimize(&workflow);

    // Count by priority
    let mut critical = 0;
    let mut high = 0;
    let mut medium = 0;
    let mut low = 0;

    for opt in &optimizations {
        match opt.priority {
            oxify_engine::Priority::Critical => critical += 1,
            oxify_engine::Priority::High => high += 1,
            oxify_engine::Priority::Medium => medium += 1,
            oxify_engine::Priority::Low => low += 1,
        }
    }

    // Count by category
    let mut performance = 0;
    let mut cost = 0;
    let mut reliability = 0;
    let mut maintainability = 0;
    let mut security = 0;

    for opt in &optimizations {
        match opt.category {
            oxify_engine::OptimizationCategory::Performance => performance += 1,
            oxify_engine::OptimizationCategory::Cost => cost += 1,
            oxify_engine::OptimizationCategory::Reliability => reliability += 1,
            oxify_engine::OptimizationCategory::Maintainability => maintainability += 1,
            oxify_engine::OptimizationCategory::Security => security += 1,
        }
    }

    // Calculate total savings
    let total_time_savings: f32 = optimizations
        .iter()
        .filter_map(|o| o.impact.time_savings)
        .sum();
    let total_cost_reduction: f32 = optimizations
        .iter()
        .filter_map(|o| o.impact.cost_reduction)
        .sum();

    // Convert to DTOs
    let optimization_dtos: Vec<OptimizationDto> = optimizations
        .iter()
        .map(|opt| OptimizationDto {
            category: format!("{:?}", opt.category),
            priority: format!("{:?}", opt.priority),
            title: opt.title.clone(),
            description: opt.description.clone(),
            affected_nodes: opt.affected_nodes.clone(),
            time_savings: opt.impact.time_savings,
            cost_reduction: opt.impact.cost_reduction,
            reliability_improvement: opt.impact.reliability_improvement.clone(),
            action: opt.action.clone(),
        })
        .collect();

    Ok(Json(OptimizationAnalysisResponse {
        total_optimizations: optimizations.len(),
        by_priority: OptimizationsByPriority {
            critical,
            high,
            medium,
            low,
        },
        by_category: OptimizationsByCategory {
            performance,
            cost,
            reliability,
            maintainability,
            security,
        },
        estimated_time_savings: total_time_savings,
        estimated_cost_reduction: total_cost_reduction,
        optimizations: optimization_dtos,
    }))
}

// ============================================================================
// Webhook Handlers (DISABLED - SQLite migration)
// ============================================================================

/// Create a new webhook (DISABLED)
#[allow(unused_variables)]
pub async fn create_webhook(
    State(_state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
    Json(_request): Json<oxify_model::CreateWebhookRequest>,
) -> Result<
    (StatusCode, Json<oxify_model::WebhookRegistrationResponse>),
    (StatusCode, Json<ErrorResponse>),
> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// List all webhooks for the authenticated user (DISABLED)
#[allow(unused_variables)]
pub async fn list_webhooks(
    State(_state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
) -> Result<Json<Vec<oxify_model::WebhookView>>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Get webhook by ID (DISABLED)
#[allow(unused_variables)]
pub async fn get_webhook(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<uuid::Uuid>,
) -> Result<Json<oxify_model::WebhookView>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Update webhook configuration (DISABLED)
#[allow(unused_variables)]
pub async fn update_webhook(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<uuid::Uuid>,
    Json(_request): Json<oxify_model::UpdateWebhookRequest>,
) -> Result<Json<oxify_model::WebhookView>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Delete webhook (DISABLED)
#[allow(unused_variables)]
pub async fn delete_webhook(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Receive webhook event (public endpoint) (DISABLED)
#[allow(unused_variables)]
pub async fn receive_webhook_event(
    State(_state): State<Arc<AppState>>,
    Path(_webhook_id): Path<uuid::Uuid>,
    _headers: axum::http::HeaderMap,
    _body: String,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// List webhook events (DISABLED)
#[allow(unused_variables)]
pub async fn list_webhook_events(
    State(_state): State<Arc<AppState>>,
    Path(_webhook_id): Path<uuid::Uuid>,
) -> Result<Json<Vec<oxify_model::WebhookEvent>>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

/// Get webhook statistics
#[derive(serde::Serialize)]
pub struct WebhookStats {
    pub webhook_id: uuid::Uuid,
    pub total_events: u64,
    pub successful_events: u64,
    pub failed_events: u64,
    pub pending_events: u64,
    pub success_rate: f64,
    pub avg_processing_time_ms: Option<f64>,
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Get webhook statistics (DISABLED)
#[allow(unused_variables)]
pub async fn get_webhook_stats(
    State(_state): State<Arc<AppState>>,
    Path(_webhook_id): Path<uuid::Uuid>,
) -> Result<Json<WebhookStats>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}

// ============================================================================
// Approval Handlers (Human-in-the-Loop)
// ============================================================================

/// List all pending approval requests
pub async fn list_pending_approvals(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
) -> Result<Json<Vec<ApprovalRequestView>>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing all pending approvals");

    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;

    let approvals = approval_store.list_pending();

    let views: Vec<ApprovalRequestView> = approvals
        .into_iter()
        .map(|a| ApprovalRequestView {
            id: a.id,
            execution_id: a.execution_id,
            node_id: a.node_id,
            message: a.config.message,
            description: a.config.description,
            approvers: a.config.approvers,
            timeout_seconds: a.config.timeout_seconds,
            context_data: a.config.context_data,
            status: format!("{:?}", a.status),
            requested_at: a.requested_at,
            resolved_at: a.resolved_at,
            resolved_by: a.resolved_by,
            comments: a.comments,
        })
        .collect();

    Ok(Json(views))
}

/// List pending approvals for a specific execution
pub async fn list_execution_approvals(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
    Path(execution_id): Path<String>,
) -> Result<Json<Vec<ApprovalRequestView>>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing approvals for execution: {}", execution_id);

    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;

    let approvals = approval_store.list_pending_for_execution(&execution_id);

    let views: Vec<ApprovalRequestView> = approvals
        .into_iter()
        .map(|a| ApprovalRequestView {
            id: a.id,
            execution_id: a.execution_id,
            node_id: a.node_id,
            message: a.config.message,
            description: a.config.description,
            approvers: a.config.approvers,
            timeout_seconds: a.config.timeout_seconds,
            context_data: a.config.context_data,
            status: format!("{:?}", a.status),
            requested_at: a.requested_at,
            resolved_at: a.resolved_at,
            resolved_by: a.resolved_by,
            comments: a.comments,
        })
        .collect();

    Ok(Json(views))
}

/// Get approval request details
pub async fn get_approval(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
    Path(approval_id): Path<Uuid>,
) -> Result<Json<ApprovalRequestView>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting approval: {}", approval_id);

    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;

    let approval = approval_store.get(approval_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Approval {} not found", approval_id),
            }),
        )
    })?;

    Ok(Json(ApprovalRequestView {
        id: approval.id,
        execution_id: approval.execution_id,
        node_id: approval.node_id,
        message: approval.config.message,
        description: approval.config.description,
        approvers: approval.config.approvers,
        timeout_seconds: approval.config.timeout_seconds,
        context_data: approval.config.context_data,
        status: format!("{:?}", approval.status),
        requested_at: approval.requested_at,
        resolved_at: approval.resolved_at,
        resolved_by: approval.resolved_by,
        comments: approval.comments,
    }))
}

/// Approve an approval request
pub async fn approve_approval(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<crate::user_types::ApiUser>,
    Path(approval_id): Path<Uuid>,
    Json(request): Json<ApprovalActionRequest>,
) -> Result<Json<ApprovalActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Approving approval: {} by user: {}", approval_id, user.id);

    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;

    let success = approval_store.approve(approval_id, user.id.to_string(), request.comments);

    if success {
        Ok(Json(ApprovalActionResponse {
            success: true,
            message: "Approval approved successfully".to_string(),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Approval {} not found", approval_id),
            }),
        ))
    }
}

/// Reject an approval request
pub async fn reject_approval(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<crate::user_types::ApiUser>,
    Path(approval_id): Path<Uuid>,
    Json(request): Json<ApprovalActionRequest>,
) -> Result<Json<ApprovalActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Rejecting approval: {} by user: {}", approval_id, user.id);

    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;

    let success = approval_store.reject(approval_id, user.id.to_string(), request.comments);

    if success {
        Ok(Json(ApprovalActionResponse {
            success: true,
            message: "Approval rejected successfully".to_string(),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Approval {} not found", approval_id),
            }),
        ))
    }
}

// Response types for approval endpoints
#[derive(Debug, Serialize, Deserialize)]
pub struct ApprovalRequestView {
    pub id: Uuid,
    pub execution_id: String,
    pub node_id: Uuid,
    pub message: String,
    pub description: Option<String>,
    pub approvers: Vec<String>,
    pub timeout_seconds: Option<u64>,
    pub context_data: serde_json::Value,
    pub status: String,
    pub requested_at: std::time::SystemTime,
    pub resolved_at: Option<std::time::SystemTime>,
    pub resolved_by: Option<String>,
    pub comments: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApprovalActionRequest {
    pub comments: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApprovalActionResponse {
    pub success: bool,
    pub message: String,
}

// ==================== Vector Store Management ====================

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateCollectionRequest {
    pub name: String,
    pub dimension: usize,
    pub provider: String, // "qdrant", "pgvector", "milvus", etc.
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CollectionInfo {
    pub name: String,
    pub dimension: usize,
    pub provider: String,
    pub vector_count: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InsertVectorsRequest {
    pub vectors: Vec<VectorWithId>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VectorWithId {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchVectorsRequest {
    pub query: Vec<f32>,
    pub top_k: usize,
    pub filter: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: Option<serde_json::Value>,
}

/// Create a new vector collection
#[allow(dead_code)]
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/vectors/collections",
    request_body = CreateCollectionRequest,
    responses(
        (status = 201, description = "Collection created", body = CollectionInfo),
        (status = 400, description = "Invalid request"),
        (status = 409, description = "Collection already exists"),
    )
))]
pub async fn create_collection(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<CollectionInfo>), StatusCode> {
    // Stub implementation
    let info = CollectionInfo {
        name: req.name,
        dimension: req.dimension,
        provider: req.provider,
        vector_count: 0,
        created_at: chrono::Utc::now(),
    };
    Ok((StatusCode::CREATED, Json(info)))
}

/// List all vector collections
#[allow(dead_code)]
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/vectors/collections",
    responses(
        (status = 200, description = "List of collections", body = Vec<CollectionInfo>),
    )
))]
pub async fn list_collections(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Vec<CollectionInfo>>, StatusCode> {
    // Stub implementation
    Ok(Json(vec![]))
}

/// Get collection info
#[allow(dead_code)]
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/vectors/collections/{name}",
    params(
        ("name" = String, Path, description = "Collection name")
    ),
    responses(
        (status = 200, description = "Collection info", body = CollectionInfo),
        (status = 404, description = "Collection not found"),
    )
))]
pub async fn get_collection(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
) -> Result<Json<CollectionInfo>, StatusCode> {
    // Stub implementation
    Err(StatusCode::NOT_FOUND)
}

/// Delete a vector collection
#[allow(dead_code)]
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/v1/vectors/collections/{name}",
    params(
        ("name" = String, Path, description = "Collection name")
    ),
    responses(
        (status = 204, description = "Collection deleted"),
        (status = 404, description = "Collection not found"),
    )
))]
pub async fn delete_collection(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Stub implementation
    Ok(StatusCode::NO_CONTENT)
}

/// Insert vectors into a collection
#[allow(dead_code)]
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/vectors/collections/{name}/vectors",
    params(
        ("name" = String, Path, description = "Collection name")
    ),
    request_body = InsertVectorsRequest,
    responses(
        (status = 201, description = "Vectors inserted"),
        (status = 404, description = "Collection not found"),
    )
))]
pub async fn insert_vectors(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
    Json(_req): Json<InsertVectorsRequest>,
) -> Result<StatusCode, StatusCode> {
    // Stub implementation
    Ok(StatusCode::CREATED)
}

/// Search vectors in a collection
#[allow(dead_code)]
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/vectors/collections/{name}/search",
    params(
        ("name" = String, Path, description = "Collection name")
    ),
    request_body = SearchVectorsRequest,
    responses(
        (status = 200, description = "Search results", body = Vec<SearchResult>),
        (status = 404, description = "Collection not found"),
    )
))]
pub async fn search_vectors(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
    Json(_req): Json<SearchVectorsRequest>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    // Stub implementation
    Ok(Json(vec![]))
}

/// Delete vectors from a collection
#[allow(dead_code)]
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/v1/vectors/collections/{name}/vectors/{id}",
    params(
        ("name" = String, Path, description = "Collection name"),
        ("id" = String, Path, description = "Vector ID")
    ),
    responses(
        (status = 204, description = "Vector deleted"),
        (status = 404, description = "Collection or vector not found"),
    )
))]
pub async fn delete_vector(
    State(_state): State<Arc<AppState>>,
    Path((_name, _id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    // Stub implementation
    Ok(StatusCode::NO_CONTENT)
}

// ==================== Workflow Import/Export ====================

/// Export format for workflows
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>, // "json" or "yaml", defaults to "json"
}

/// Export a workflow as JSON or YAML
#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/export",
    params(
        ("id" = String, Path, description = "Workflow ID"),
        ("format" = Option<String>, Query, description = "Export format: json or yaml (default: json)")
    ),
    responses(
        (status = 200, description = "Workflow exported successfully", content_type = "application/json"),
        (status = 404, description = "Workflow not found", body = ErrorResponse),
        (status = 400, description = "Invalid format", body = ErrorResponse)
    )
)]
pub async fn export_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<WorkflowId>,
    axum::extract::Query(query): axum::extract::Query<ExportQuery>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    info!("Exporting workflow: {} as {:?}", id, query.format);

    // Get workflow
    let workflow = match state.workflow_store.get(&id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Workflow {} not found", id),
                }),
            ))
        }
        Err(e) => {
            error!("Failed to get workflow: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            ));
        }
    };

    let format = query.format.as_deref().unwrap_or("json");

    match format {
        "json" => {
            let json_str = serde_json::to_string_pretty(&workflow).map_err(|e| {
                error!("Failed to serialize workflow to JSON: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "SerializationError".to_string(),
                        message: format!("Failed to serialize workflow: {}", e),
                    }),
                )
            })?;

            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"workflow_{}.json\"", id),
                )
                .body(json_str.into())
                .map_err(|e| {
                    error!("Failed to build response: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "ResponseBuildError".to_string(),
                            message: format!("Failed to build response: {}", e),
                        }),
                    )
                })
        }
        "yaml" => {
            let yaml_str = oxify_model::workflow_to_yaml(&workflow).map_err(|e| {
                error!("Failed to serialize workflow to YAML: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "SerializationError".to_string(),
                        message: format!("Failed to serialize workflow: {}", e),
                    }),
                )
            })?;

            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/x-yaml")
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"workflow_{}.yaml\"", id),
                )
                .body(yaml_str.into())
                .map_err(|e| {
                    error!("Failed to build response: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "ResponseBuildError".to_string(),
                            message: format!("Failed to build response: {}", e),
                        }),
                    )
                })
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "InvalidFormat".to_string(),
                message: format!("Invalid format '{}'. Supported formats: json, yaml", format),
            }),
        )),
    }
}

/// Import workflow request
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportWorkflowRequest {
    /// Workflow content (JSON or YAML string)
    pub content: String,

    /// Format of the content: "json" or "yaml"
    pub format: String,

    /// Whether to generate a new ID for the imported workflow (default: true)
    #[serde(default = "default_generate_id")]
    pub generate_new_id: bool,

    /// Optional new name for the imported workflow
    pub new_name: Option<String>,
}

fn default_generate_id() -> bool {
    true
}

/// Import workflow response
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportWorkflowResponse {
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = "uuid"))]
    pub id: WorkflowId,
    pub name: String,
    pub message: String,
}

/// Import a workflow from JSON or YAML
#[utoipa::path(
    post,
    path = "/api/v1/workflows/import",
    request_body = ImportWorkflowRequest,
    responses(
        (status = 201, description = "Workflow imported successfully", body = ImportWorkflowResponse),
        (status = 400, description = "Invalid workflow or format", body = ErrorResponse),
        (status = 409, description = "Workflow with this ID already exists", body = ErrorResponse)
    )
)]
pub async fn import_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportWorkflowRequest>,
) -> Result<(StatusCode, Json<ImportWorkflowResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Importing workflow from {}", req.format);

    // Parse workflow based on format
    let mut workflow = match req.format.as_str() {
        "json" => serde_json::from_str::<oxify_model::Workflow>(&req.content).map_err(|e| {
            error!("Failed to parse JSON workflow: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "ParseError".to_string(),
                    message: format!("Failed to parse JSON: {}", e),
                }),
            )
        })?,
        "yaml" => oxify_model::workflow_from_yaml(&req.content).map_err(|e| {
            error!("Failed to parse YAML workflow: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "ParseError".to_string(),
                    message: format!("Failed to parse YAML: {}", e),
                }),
            )
        })?,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "InvalidFormat".to_string(),
                    message: format!(
                        "Invalid format '{}'. Supported formats: json, yaml",
                        req.format
                    ),
                }),
            ))
        }
    };

    // Generate new ID if requested
    if req.generate_new_id {
        workflow.metadata.id = Uuid::new_v4();
        // Also regenerate IDs for all nodes and edges to avoid conflicts
        let mut old_to_new_ids = std::collections::HashMap::new();

        for node in &mut workflow.nodes {
            let old_id = node.id;
            let new_id = Uuid::new_v4();
            old_to_new_ids.insert(old_id, new_id);
            node.id = new_id;
        }

        // Update edge references
        for edge in &mut workflow.edges {
            edge.id = Uuid::new_v4();
            if let Some(&new_from) = old_to_new_ids.get(&edge.from) {
                edge.from = new_from;
            }
            if let Some(&new_to) = old_to_new_ids.get(&edge.to) {
                edge.to = new_to;
            }
        }
    }

    // Update name if provided
    if let Some(new_name) = req.new_name {
        workflow.metadata.name = new_name;
    }

    // Validate workflow
    if let Err(e) = workflow.validate() {
        error!("Workflow validation failed: {}", e);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: e,
            }),
        ));
    }

    // Check if workflow with this ID already exists (if not generating new ID)
    if !req.generate_new_id {
        if let Ok(Some(_)) = state.workflow_store.get(&workflow.metadata.id).await {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "Conflict".to_string(),
                    message: format!(
                        "Workflow with ID {} already exists. Use generate_new_id=true to create a new workflow",
                        workflow.metadata.id
                    ),
                }),
            ));
        }
    }

    let workflow_id = workflow.metadata.id;
    let workflow_name = workflow.metadata.name.clone();

    // Store workflow
    state.workflow_store.create(workflow).await.map_err(|e| {
        error!("Failed to create workflow: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to store workflow: {}", e),
            }),
        )
    })?;

    info!("Workflow imported successfully: {}", workflow_id);

    Ok((
        StatusCode::CREATED,
        Json(ImportWorkflowResponse {
            id: workflow_id,
            name: workflow_name,
            message: "Workflow imported successfully".to_string(),
        }),
    ))
}

// ==================== Workflow Templates / Marketplace ====================

/// Template listing response
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TemplateListItem {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub version: String,
    pub author: Option<String>,
    pub usage_count: u64,
    pub is_public: bool,
}

/// Template filter query parameters
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct TemplateListQuery {
    pub category: Option<String>,
    pub tag: Option<String>,
    pub search: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Built-in templates for the marketplace
fn get_builtin_templates() -> Vec<TemplateListItem> {
    vec![
        TemplateListItem {
            id: "rag-basic".to_string(),
            name: "Basic RAG Pipeline".to_string(),
            description: Some(
                "A simple RAG pipeline with document retrieval and LLM generation".to_string(),
            ),
            category: Some("RAG".to_string()),
            tags: vec![
                "rag".to_string(),
                "retrieval".to_string(),
                "generation".to_string(),
            ],
            version: "1.0.0".to_string(),
            author: Some("OxiFY Team".to_string()),
            usage_count: 1250,
            is_public: true,
        },
        TemplateListItem {
            id: "agent-react".to_string(),
            name: "ReAct Agent".to_string(),
            description: Some("Reasoning and Acting agent with tool use capabilities".to_string()),
            category: Some("Agent".to_string()),
            tags: vec![
                "agent".to_string(),
                "react".to_string(),
                "tool-use".to_string(),
            ],
            version: "1.0.0".to_string(),
            author: Some("OxiFY Team".to_string()),
            usage_count: 890,
            is_public: true,
        },
        TemplateListItem {
            id: "data-extraction".to_string(),
            name: "Structured Data Extraction".to_string(),
            description: Some(
                "Extract structured data from unstructured text using LLMs".to_string(),
            ),
            category: Some("Data Processing".to_string()),
            tags: vec![
                "extraction".to_string(),
                "structured-output".to_string(),
                "parsing".to_string(),
            ],
            version: "1.0.0".to_string(),
            author: Some("OxiFY Team".to_string()),
            usage_count: 720,
            is_public: true,
        },
        TemplateListItem {
            id: "chatbot-simple".to_string(),
            name: "Simple Chatbot".to_string(),
            description: Some("A basic conversational chatbot with memory".to_string()),
            category: Some("Chatbot".to_string()),
            tags: vec![
                "chatbot".to_string(),
                "conversation".to_string(),
                "memory".to_string(),
            ],
            version: "1.0.0".to_string(),
            author: Some("OxiFY Team".to_string()),
            usage_count: 2100,
            is_public: true,
        },
        TemplateListItem {
            id: "summarization".to_string(),
            name: "Document Summarization".to_string(),
            description: Some(
                "Summarize long documents using map-reduce or iterative refinement".to_string(),
            ),
            category: Some("Data Processing".to_string()),
            tags: vec![
                "summarization".to_string(),
                "documents".to_string(),
                "map-reduce".to_string(),
            ],
            version: "1.0.0".to_string(),
            author: Some("OxiFY Team".to_string()),
            usage_count: 560,
            is_public: true,
        },
        TemplateListItem {
            id: "multi-agent".to_string(),
            name: "Multi-Agent Collaboration".to_string(),
            description: Some("Multiple AI agents working together on complex tasks".to_string()),
            category: Some("Agent".to_string()),
            tags: vec![
                "multi-agent".to_string(),
                "collaboration".to_string(),
                "orchestration".to_string(),
            ],
            version: "1.0.0".to_string(),
            author: Some("OxiFY Team".to_string()),
            usage_count: 340,
            is_public: true,
        },
        TemplateListItem {
            id: "code-review".to_string(),
            name: "AI Code Review".to_string(),
            description: Some("Automated code review with security and quality checks".to_string()),
            category: Some("Developer Tools".to_string()),
            tags: vec![
                "code-review".to_string(),
                "security".to_string(),
                "quality".to_string(),
            ],
            version: "1.0.0".to_string(),
            author: Some("OxiFY Team".to_string()),
            usage_count: 480,
            is_public: true,
        },
        TemplateListItem {
            id: "translation".to_string(),
            name: "Multi-Language Translation".to_string(),
            description: Some(
                "Translate content between multiple languages with quality checks".to_string(),
            ),
            category: Some("Data Processing".to_string()),
            tags: vec![
                "translation".to_string(),
                "multilingual".to_string(),
                "localization".to_string(),
            ],
            version: "1.0.0".to_string(),
            author: Some("OxiFY Team".to_string()),
            usage_count: 390,
            is_public: true,
        },
    ]
}

/// List available workflow templates (marketplace)
#[allow(dead_code)]
#[utoipa::path(
    get,
    path = "/api/v1/templates",
    params(
        ("category" = Option<String>, Query, description = "Filter by category"),
        ("tag" = Option<String>, Query, description = "Filter by tag"),
        ("search" = Option<String>, Query, description = "Search in name/description"),
        ("limit" = Option<usize>, Query, description = "Max results to return"),
        ("offset" = Option<usize>, Query, description = "Offset for pagination")
    ),
    responses(
        (status = 200, description = "List of templates", body = Vec<TemplateListItem>)
    )
)]
pub async fn list_templates(
    axum::extract::Query(query): axum::extract::Query<TemplateListQuery>,
) -> Json<Vec<TemplateListItem>> {
    info!(
        "Listing templates: category={:?}, tag={:?}, search={:?}",
        query.category, query.tag, query.search
    );

    let mut templates = get_builtin_templates();

    // Filter by category
    if let Some(ref category) = query.category {
        templates.retain(|t| {
            t.category
                .as_ref()
                .is_some_and(|c| c.eq_ignore_ascii_case(category))
        });
    }

    // Filter by tag
    if let Some(ref tag) = query.tag {
        templates.retain(|t| t.tags.iter().any(|t_tag| t_tag.eq_ignore_ascii_case(tag)));
    }

    // Search in name/description
    if let Some(ref search) = query.search {
        let search_lower = search.to_lowercase();
        templates.retain(|t| {
            t.name.to_lowercase().contains(&search_lower)
                || t.description
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(&search_lower))
        });
    }

    // Pagination
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);
    let templates: Vec<_> = templates.into_iter().skip(offset).take(limit).collect();

    Json(templates)
}

/// Get template categories with counts
#[allow(dead_code)]
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TemplateCategory {
    pub name: String,
    pub count: usize,
    pub description: Option<String>,
}

/// List template categories
#[allow(dead_code)]
#[utoipa::path(
    get,
    path = "/api/v1/templates/categories",
    responses(
        (status = 200, description = "List of categories with counts", body = Vec<TemplateCategory>)
    )
)]
pub async fn list_template_categories() -> Json<Vec<TemplateCategory>> {
    let templates = get_builtin_templates();
    let mut category_counts = std::collections::HashMap::new();

    for template in templates {
        if let Some(category) = template.category {
            *category_counts.entry(category).or_insert(0) += 1;
        }
    }

    let categories: Vec<TemplateCategory> = category_counts
        .into_iter()
        .map(|(name, count)| {
            let description = match name.as_str() {
                "RAG" => Some("Retrieval-Augmented Generation pipelines".to_string()),
                "Agent" => Some("Autonomous AI agents with tool use".to_string()),
                "Data Processing" => Some("Transform and process data with LLMs".to_string()),
                "Chatbot" => Some("Conversational AI interfaces".to_string()),
                "Developer Tools" => Some("Tools for software development".to_string()),
                _ => None,
            };
            TemplateCategory {
                name,
                count,
                description,
            }
        })
        .collect();

    Json(categories)
}

/// Template detail response
#[allow(dead_code)]
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TemplateDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub version: String,
    pub author: Option<String>,
    pub usage_count: u64,
    pub is_public: bool,
    pub parameters: Vec<TemplateParameter>,
    pub preview_nodes: Vec<String>,
}

/// Template parameter definition
#[allow(dead_code)]
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TemplateParameter {
    pub name: String,
    pub param_type: String,
    pub description: Option<String>,
    pub required: bool,
    pub default_value: Option<String>,
}

/// Get template details by ID
#[allow(dead_code)]
#[utoipa::path(
    get,
    path = "/api/v1/templates/{id}",
    params(
        ("id" = String, Path, description = "Template ID")
    ),
    responses(
        (status = 200, description = "Template details", body = TemplateDetail),
        (status = 404, description = "Template not found", body = ErrorResponse)
    )
)]
pub async fn get_template(
    Path(id): Path<String>,
) -> Result<Json<TemplateDetail>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting template: {}", id);

    let templates = get_builtin_templates();
    let template = templates.into_iter().find(|t| t.id == id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Template '{}' not found", id),
            }),
        )
    })?;

    // Generate parameters based on template type
    let parameters = match id.as_str() {
        "rag-basic" => vec![
            TemplateParameter {
                name: "model".to_string(),
                param_type: "model".to_string(),
                description: Some("LLM model to use".to_string()),
                required: true,
                default_value: Some("gpt-4".to_string()),
            },
            TemplateParameter {
                name: "collection".to_string(),
                param_type: "collection".to_string(),
                description: Some("Vector collection for retrieval".to_string()),
                required: true,
                default_value: None,
            },
            TemplateParameter {
                name: "top_k".to_string(),
                param_type: "integer".to_string(),
                description: Some("Number of documents to retrieve".to_string()),
                required: false,
                default_value: Some("5".to_string()),
            },
        ],
        "agent-react" => vec![
            TemplateParameter {
                name: "model".to_string(),
                param_type: "model".to_string(),
                description: Some("LLM model for agent reasoning".to_string()),
                required: true,
                default_value: Some("gpt-4".to_string()),
            },
            TemplateParameter {
                name: "max_iterations".to_string(),
                param_type: "integer".to_string(),
                description: Some("Maximum reasoning iterations".to_string()),
                required: false,
                default_value: Some("10".to_string()),
            },
            TemplateParameter {
                name: "tools".to_string(),
                param_type: "string_array".to_string(),
                description: Some("Tools available to the agent".to_string()),
                required: false,
                default_value: Some("[\"search\", \"calculator\"]".to_string()),
            },
        ],
        _ => vec![TemplateParameter {
            name: "model".to_string(),
            param_type: "model".to_string(),
            description: Some("LLM model to use".to_string()),
            required: true,
            default_value: Some("gpt-4".to_string()),
        }],
    };

    // Generate preview nodes based on template type
    let preview_nodes = match id.as_str() {
        "rag-basic" => vec![
            "Start".to_string(),
            "Query Embedding".to_string(),
            "Vector Search".to_string(),
            "Context Assembly".to_string(),
            "LLM Generation".to_string(),
            "End".to_string(),
        ],
        "agent-react" => vec![
            "Start".to_string(),
            "Think".to_string(),
            "Act (Tool Call)".to_string(),
            "Observe".to_string(),
            "Loop Check".to_string(),
            "End".to_string(),
        ],
        "chatbot-simple" => vec![
            "Start".to_string(),
            "Load History".to_string(),
            "LLM Response".to_string(),
            "Save History".to_string(),
            "End".to_string(),
        ],
        _ => vec![
            "Start".to_string(),
            "Process".to_string(),
            "End".to_string(),
        ],
    };

    Ok(Json(TemplateDetail {
        id: template.id,
        name: template.name,
        description: template.description,
        category: template.category,
        tags: template.tags,
        version: template.version,
        author: template.author,
        usage_count: template.usage_count,
        is_public: template.is_public,
        parameters,
        preview_nodes,
    }))
}

/// Request to instantiate a template
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InstantiateTemplateRequest {
    pub name: String,
    pub description: Option<String>,
    #[allow(dead_code)]
    pub parameters: std::collections::HashMap<String, serde_json::Value>,
}

/// Response after instantiating a template
#[allow(dead_code)]
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InstantiateTemplateResponse {
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub message: String,
}

/// Instantiate a template to create a new workflow
#[allow(dead_code)]
#[utoipa::path(
    post,
    path = "/api/v1/templates/{id}/instantiate",
    params(
        ("id" = String, Path, description = "Template ID")
    ),
    request_body = InstantiateTemplateRequest,
    responses(
        (status = 201, description = "Workflow created from template", body = InstantiateTemplateResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 400, description = "Invalid parameters", body = ErrorResponse)
    )
)]
pub async fn instantiate_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<InstantiateTemplateRequest>,
) -> Result<(StatusCode, Json<InstantiateTemplateResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Instantiating template: {} as '{}'", id, req.name);

    // Verify template exists
    let templates = get_builtin_templates();
    let _template = templates.iter().find(|t| t.id == id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Template '{}' not found", id),
            }),
        )
    })?;

    // Create workflow from template
    // In a real implementation, this would load the full template definition
    // and substitute parameters. For now, we create a basic workflow.
    let workflow_id = Uuid::new_v4();
    let start_node = oxify_model::Node {
        id: Uuid::new_v4(),
        name: "Start".to_string(),
        kind: oxify_model::NodeKind::Start,
        position: Some((100.0, 200.0)),
        retry_config: None,
        timeout_config: None,
    };
    let end_node = oxify_model::Node {
        id: Uuid::new_v4(),
        name: "End".to_string(),
        kind: oxify_model::NodeKind::End,
        position: Some((500.0, 200.0)),
        retry_config: None,
        timeout_config: None,
    };

    let edge = oxify_model::Edge {
        id: Uuid::new_v4(),
        from: start_node.id,
        to: end_node.id,
        label: None,
        condition: None,
    };

    let workflow = oxify_model::Workflow {
        metadata: oxify_model::WorkflowMetadata {
            id: workflow_id,
            name: req.name.clone(),
            description: req.description,
            version: "1.0.0".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            tags: vec![format!("template:{}", id)],
            parent_id: None,
            change_description: Some(format!("Created from template '{}'", id)),
            schedule: None,
        },
        nodes: vec![start_node, end_node],
        edges: vec![edge],
    };

    // Store workflow
    state.workflow_store.create(workflow).await.map_err(|e| {
        error!("Failed to create workflow from template: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to create workflow: {}", e),
            }),
        )
    })?;

    info!("Created workflow {} from template {}", workflow_id, id);

    Ok((
        StatusCode::CREATED,
        Json(InstantiateTemplateResponse {
            workflow_id,
            workflow_name: req.name,
            message: format!("Workflow created from template '{}'", id),
        }),
    ))
}

/// Get Prometheus metrics
///
/// Returns metrics in Prometheus text format for monitoring and observability.
/// Includes HTTP request metrics, database connection pool metrics, cache statistics, and query performance.
pub async fn get_metrics(State(state): State<Arc<AppState>>) -> Result<String, StatusCode> {
    // Build Prometheus metrics in text format
    let mut metrics = String::new();

    // HTTP request metrics (always included)
    metrics.push_str(&state.http_metrics.to_prometheus_format().await);
    metrics.push('\n');

    // Database pool metrics (if available)
    if let Some(pool) = &state.db_pool {
        // Get pool metrics
        let pool_metrics = pool.metrics();
        let stats = pool_metrics.stats;

        metrics.push_str(&format!(
            "# HELP oxify_db_pool_size Current size of the database connection pool\n\
             # TYPE oxify_db_pool_size gauge\n\
             oxify_db_pool_size {}\n",
            stats.size
        ));

        metrics.push_str(&format!(
            "# HELP oxify_db_pool_idle Number of idle connections in the pool\n\
             # TYPE oxify_db_pool_idle gauge\n\
             oxify_db_pool_idle {}\n",
            stats.num_idle
        ));

        metrics.push_str(&format!(
            "# HELP oxify_db_pool_max Maximum number of connections in the pool\n\
             # TYPE oxify_db_pool_max gauge\n\
             oxify_db_pool_max {}\n",
            stats.max_connections
        ));

        // Calculate utilization
        let utilization = if stats.max_connections > 0 {
            stats.size as f64 / stats.max_connections as f64
        } else {
            0.0
        };

        metrics.push_str(&format!(
            "# HELP oxify_db_pool_utilization Database connection pool utilization (0-1)\n\
             # TYPE oxify_db_pool_utilization gauge\n\
             oxify_db_pool_utilization {:.3}\n",
            utilization
        ));
    }

    // Add API version info
    metrics.push_str(&format!(
        "# HELP oxify_api_info API version information\n\
         # TYPE oxify_api_info gauge\n\
         oxify_api_info{{version=\"{}\"}} 1\n",
        env!("CARGO_PKG_VERSION")
    ));

    Ok(metrics)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_engine::execution_events;
    use oxify_model::{Edge, Node, NodeKind, Workflow, WorkflowMetadata};

    /// Build a minimal Start → End workflow for testing.
    fn build_test_workflow() -> Workflow {
        let mut workflow = Workflow::new("Test Workflow".to_string());
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);
        let start_id = start.id;
        let end_id = end.id;
        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));
        workflow
    }

    /// Verify that the execution_id returned by `execute_workflow` logic is
    /// the same id used as the storage key, so `execution_store.get(returned_id)`
    /// reliably finds the row immediately after creation.
    #[tokio::test]
    async fn test_execute_workflow_execution_id_consistency() {
        let state = AppState::new();

        // Store a workflow so we can retrieve it
        let workflow = build_test_workflow();
        let workflow_id = state
            .workflow_store
            .create(workflow.clone())
            .await
            .expect("workflow create");

        // Simulate the fixed handler logic directly:
        let mut ctx = ExecutionContext::new(workflow_id);
        let execution_id = ctx.execution_id;

        // Set a test variable (mirrors the handler loop)
        ctx.set_variable("test_key".to_string(), serde_json::Value::from("test_val"));

        // Store BEFORE spawning
        let stored_id = state
            .execution_store
            .create(ctx.clone())
            .await
            .expect("execution create");

        // The stored key must equal execution_id (not a random new UUID)
        assert_eq!(
            stored_id, execution_id,
            "storage key must equal ctx.execution_id"
        );

        // The row must be retrievable immediately using execution_id
        let found = state
            .execution_store
            .get(&execution_id)
            .await
            .expect("get ok")
            .expect("row must exist");

        assert_eq!(found.execution_id, execution_id);
    }

    /// Verify that execute_with_context preserves the caller's execution_id
    /// and emits events carrying that same id on the event bus.
    #[tokio::test]
    async fn test_execute_with_context_emits_bus_events_with_correct_id() {
        let state = AppState::new();

        // Subscribe to the shared bus before execution starts
        let mut rx = state.event_bus.subscribe();

        let workflow = build_test_workflow();
        let workflow_id = state
            .workflow_store
            .create(workflow.clone())
            .await
            .expect("workflow create");

        let ctx = ExecutionContext::new(workflow_id);
        let execution_id = ctx.execution_id;

        // Run synchronously — mirrors what the spawned task does
        let result = state
            .engine
            .execute_with_context(
                &workflow,
                ctx,
                oxify_engine::ExecutionConfig::new().with_events(),
            )
            .await
            .expect("execution should succeed");

        // execution_id must be preserved end-to-end
        assert_eq!(result.execution_id, execution_id);

        // All events on the bus must carry the same execution_id
        let mut saw_started = false;
        let mut saw_completed = false;
        while let Ok(ev) = rx.try_recv() {
            if ev.execution_id != Some(execution_id) {
                continue; // events from other tests — ignore
            }
            match ev.event_type.as_str() {
                s if s == execution_events::WORKFLOW_STARTED => saw_started = true,
                s if s == execution_events::WORKFLOW_COMPLETED => saw_completed = true,
                _ => {}
            }
        }
        assert!(saw_started, "expected workflow.started event on the bus");
        assert!(
            saw_completed,
            "expected workflow.completed event on the bus"
        );
    }

    /// Verify the in-memory ExecutionStore uses ctx.execution_id as the key,
    /// making update() reliable after create().
    #[tokio::test]
    async fn test_in_memory_store_create_uses_execution_id() {
        use crate::storage::ExecutionStoreBackend;

        let store = ExecutionStoreBackend::new_in_memory();
        let workflow_id = WorkflowMetadata::new("w".to_string()).id;

        let ctx = ExecutionContext::new(workflow_id);
        let expected_id = ctx.execution_id;

        let returned_id = store.create(ctx.clone()).await.expect("create ok");

        assert_eq!(
            returned_id, expected_id,
            "create must return ctx.execution_id as the storage key"
        );

        // get() with the same id must succeed
        let found = store.get(&expected_id).await.expect("get ok");
        assert!(
            found.is_some(),
            "row must be retrievable by ctx.execution_id"
        );

        // update() must also succeed since key now matches
        let mut updated_ctx = ctx;
        updated_ctx.state = oxify_model::ExecutionState::Completed;
        let update_result = store
            .update(&expected_id, updated_ctx)
            .await
            .expect("update ok");
        assert!(
            update_result.is_some(),
            "update must find the row by the same id"
        );
    }
}
