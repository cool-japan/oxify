//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::auth::AuthState;
use crate::storage::{ExecutionStoreBackend, UserStoreBackend, WorkflowStoreBackend};
use axum::{
    extract::{Path, State},
    http::StatusCode, Json,
};
use oxify_engine::{Engine, EngineBuilder, EventBus};
use oxify_model::WorkflowId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;
use crate::types::*;


/// Application state
#[derive(Clone)]
pub struct AppState {
    pub workflow_store: WorkflowStoreBackend,
    pub execution_store: ExecutionStoreBackend,
    pub user_store: UserStoreBackend,
    pub auth: AuthState,
    pub engine: Arc<Engine>,
    pub event_bus: Arc<EventBus>,
    pub version_store: Option<Arc<oxify_storage::WorkflowVersionStore>>,
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
            EngineBuilder::new().with_event_bus(event_bus.clone()).build(),
        );
        Self {
            workflow_store: WorkflowStoreBackend::new_in_memory(),
            execution_store: ExecutionStoreBackend::new_in_memory(),
            user_store: UserStoreBackend::new_in_memory(),
            auth: AuthState::new(),
            engine,
            event_bus,
            version_store: None,
            approval_store: Some(Arc::new(oxify_engine::ApprovalStore::new())),
            vector_registry: Some(
                Arc::new(crate::vector_handlers::VectorStoreRegistry::new()),
            ),
            mcp_registry: Arc::new(
                tokio::sync::RwLock::new(oxify_mcp::McpRegistry::new()),
            ),
            db_pool: None,
            http_metrics: Arc::new(crate::middleware::HttpMetrics::new()),
        }
    }
    pub async fn new_with_database() -> Result<Self, String> {
        let config = oxify_storage::DatabaseConfig::default();
        let pool = oxify_storage::DatabasePool::new(config)
            .await
            .map_err(|e| e.to_string())?;
        pool.migrate().await.map_err(|e| e.to_string())?;
        let version_store = Some(
            Arc::new(oxify_storage::WorkflowVersionStore::new(pool.clone())),
        );
        let approval_store = Some(Arc::new(oxify_engine::ApprovalStore::new()));
        let vector_registry = Some(
            Arc::new(crate::vector_handlers::VectorStoreRegistry::new()),
        );
        let event_bus = Arc::new(EventBus::new(1024));
        let engine = Arc::new(
            EngineBuilder::new().with_event_bus(event_bus.clone()).build(),
        );
        Ok(Self {
            workflow_store: WorkflowStoreBackend::new_database(pool.clone()),
            execution_store: ExecutionStoreBackend::new_database(pool.clone()),
            user_store: UserStoreBackend::new_database(pool.clone()),
            auth: AuthState::new(),
            engine,
            event_bus,
            version_store,
            approval_store,
            vector_registry,
            mcp_registry: Arc::new(
                tokio::sync::RwLock::new(oxify_mcp::McpRegistry::new()),
            ),
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
pub struct CreateCollectionRequest {
    pub name: String,
    pub dimension: usize,
    pub provider: String,
    pub config: Option<serde_json::Value>,
}
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
#[derive(serde::Serialize)]
pub struct ErrorFrequency {
    pub error_message: String,
    pub count: u64,
    pub percentage: f64,
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
/// Export format for workflows
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>,
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
/// Import workflow response
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportWorkflowResponse {
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = "uuid"))]
    pub id: WorkflowId,
    pub name: String,
    pub message: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InsertVectorsRequest {
    pub vectors: Vec<VectorWithId>,
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
#[derive(serde::Serialize)]
pub struct ListSchedulesResponse {
    pub schedules: Vec<oxify_model::Schedule>,
    pub total: usize,
}
/// Liveness probe response
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LivezResponse {
    pub status: &'static str,
}
#[derive(serde::Serialize)]
pub struct NodeCostSummary {
    pub node_id: Uuid,
    pub node_name: String,
    pub cost_usd: f64,
    pub input_tokens: u32,
    pub output_tokens: u32,
}
/// Readiness probe response
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReadyzResponse {
    pub status: &'static str,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: Option<serde_json::Value>,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchVectorsRequest {
    pub query: Vec<f32>,
    pub top_k: usize,
    pub filter: Option<serde_json::Value>,
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
#[derive(serde::Serialize)]
pub struct TimeSeriesData {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub count: u64,
    pub avg_duration_ms: f64,
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
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VectorWithId {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
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
/// Health check handler (backward-compatible alias for `/livez`)
#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Service is healthy", body = HealthResponse))
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
/// Liveness probe — always returns 200 as long as the process is alive.
///
/// A liveness probe failure triggers a container restart. This endpoint
/// performs no I/O and will never return a non-2xx status voluntarily.
#[utoipa::path(
    get,
    path = "/livez",
    responses((status = 200, description = "Process is alive", body = LivezResponse))
)]
pub async fn livez() -> Json<LivezResponse> {
    Json(LivezResponse { status: "alive" })
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
/// Create a new vector collection
#[allow(dead_code)]
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/vectors/collections",
        request_body = CreateCollectionRequest,
        responses(
            (status = 201, description = "Collection created", body = CollectionInfo),
            (status = 400, description = "Invalid request"),
            (status = 409, description = "Collection already exists"),
        )
    )
)]
pub async fn create_collection(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<CollectionInfo>), StatusCode> {
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
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/vectors/collections",
        responses(
            (
                status = 200,
                description = "List of collections",
                body = Vec<CollectionInfo>
            ),
        )
    )
)]
pub async fn list_collections(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Vec<CollectionInfo>>, StatusCode> {
    Ok(Json(vec![]))
}
/// Get collection info
#[allow(dead_code)]
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/vectors/collections/{name}",
        params(("name" = String, Path, description = "Collection name")),
        responses(
            (status = 200, description = "Collection info", body = CollectionInfo),
            (status = 404, description = "Collection not found"),
        )
    )
)]
pub async fn get_collection(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
) -> Result<Json<CollectionInfo>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}
/// Delete a vector collection
#[allow(dead_code)]
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/v1/vectors/collections/{name}",
        params(("name" = String, Path, description = "Collection name")),
        responses(
            (status = 204, description = "Collection deleted"),
            (status = 404, description = "Collection not found"),
        )
    )
)]
pub async fn delete_collection(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}
/// Insert vectors into a collection
#[allow(dead_code)]
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/vectors/collections/{name}/vectors",
        params(("name" = String, Path, description = "Collection name")),
        request_body = InsertVectorsRequest,
        responses(
            (status = 201, description = "Vectors inserted"),
            (status = 404, description = "Collection not found"),
        )
    )
)]
pub async fn insert_vectors(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
    Json(_req): Json<InsertVectorsRequest>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::CREATED)
}
/// Search vectors in a collection
#[allow(dead_code)]
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/vectors/collections/{name}/search",
        params(("name" = String, Path, description = "Collection name")),
        request_body = SearchVectorsRequest,
        responses(
            (status = 200, description = "Search results", body = Vec<SearchResult>),
            (status = 404, description = "Collection not found"),
        )
    )
)]
pub async fn search_vectors(
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
    Json(_req): Json<SearchVectorsRequest>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    Ok(Json(vec![]))
}
/// Delete vectors from a collection
#[allow(dead_code)]
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
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
    )
)]
pub async fn delete_vector(
    State(_state): State<Arc<AppState>>,
    Path((_name, _id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}
/// Export a workflow as JSON or YAML
#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/export",
    params(
        ("id" = String, Path, description = "Workflow ID"),
        (
            "format" = Option<String>,
            Query,
            description = "Export format: json or yaml (default: json)"
        )
    ),
    responses(
        (
            status = 200,
            description = "Workflow exported successfully",
            content_type = "application/json"
        ),
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
            let json_str = serde_json::to_string_pretty(&workflow)
                .map_err(|e| {
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
            let yaml_str = oxify_model::workflow_to_yaml(&workflow)
                .map_err(|e| {
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
        _ => {
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "InvalidFormat".to_string(),
                    message: format!(
                        "Invalid format '{}'. Supported formats: json, yaml", format
                    ),
                }),
            ))
        }
    }
}
fn default_generate_id() -> bool {
    true
}
/// Import a workflow from JSON or YAML
#[utoipa::path(
    post,
    path = "/api/v1/workflows/import",
    request_body = ImportWorkflowRequest,
    responses(
        (
            status = 201,
            description = "Workflow imported successfully",
            body = ImportWorkflowResponse
        ),
        (status = 400, description = "Invalid workflow or format", body = ErrorResponse),
        (
            status = 409,
            description = "Workflow with this ID already exists",
            body = ErrorResponse
        )
    )
)]
pub async fn import_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportWorkflowRequest>,
) -> Result<
    (StatusCode, Json<ImportWorkflowResponse>),
    (StatusCode, Json<ErrorResponse>),
> {
    info!("Importing workflow from {}", req.format);
    let mut workflow = match req.format.as_str() {
        "json" => {
            serde_json::from_str::<oxify_model::Workflow>(&req.content)
                .map_err(|e| {
                    error!("Failed to parse JSON workflow: {}", e);
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "ParseError".to_string(),
                            message: format!("Failed to parse JSON: {}", e),
                        }),
                    )
                })?
        }
        "yaml" => {
            oxify_model::workflow_from_yaml(&req.content)
                .map_err(|e| {
                    error!("Failed to parse YAML workflow: {}", e);
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "ParseError".to_string(),
                            message: format!("Failed to parse YAML: {}", e),
                        }),
                    )
                })?
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "InvalidFormat".to_string(),
                    message: format!(
                        "Invalid format '{}'. Supported formats: json, yaml", req.format
                    ),
                }),
            ));
        }
    };
    if req.generate_new_id {
        workflow.metadata.id = Uuid::new_v4();
        let mut old_to_new_ids = std::collections::HashMap::new();
        for node in &mut workflow.nodes {
            let old_id = node.id;
            let new_id = Uuid::new_v4();
            old_to_new_ids.insert(old_id, new_id);
            node.id = new_id;
        }
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
    if let Some(new_name) = req.new_name {
        workflow.metadata.name = new_name;
    }
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
    state
        .workflow_store
        .create(workflow)
        .await
        .map_err(|e| {
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
/// Built-in templates for the marketplace
fn get_builtin_templates() -> Vec<TemplateListItem> {
    vec![
        TemplateListItem { id : "rag-basic".to_string(), name : "Basic RAG Pipeline"
        .to_string(), description :
        Some("A simple RAG pipeline with document retrieval and LLM generation"
        .to_string(),), category : Some("RAG".to_string()), tags : vec!["rag"
        .to_string(), "retrieval".to_string(), "generation".to_string(),], version :
        "1.0.0".to_string(), author : Some("OxiFY Team".to_string()), usage_count : 1250,
        is_public : true, }, TemplateListItem { id : "agent-react".to_string(), name :
        "ReAct Agent".to_string(), description :
        Some("Reasoning and Acting agent with tool use capabilities".to_string()),
        category : Some("Agent".to_string()), tags : vec!["agent".to_string(), "react"
        .to_string(), "tool-use".to_string(),], version : "1.0.0".to_string(), author :
        Some("OxiFY Team".to_string()), usage_count : 890, is_public : true, },
        TemplateListItem { id : "data-extraction".to_string(), name :
        "Structured Data Extraction".to_string(), description :
        Some("Extract structured data from unstructured text using LLMs".to_string(),),
        category : Some("Data Processing".to_string()), tags : vec!["extraction"
        .to_string(), "structured-output".to_string(), "parsing".to_string(),], version :
        "1.0.0".to_string(), author : Some("OxiFY Team".to_string()), usage_count : 720,
        is_public : true, }, TemplateListItem { id : "chatbot-simple".to_string(), name :
        "Simple Chatbot".to_string(), description :
        Some("A basic conversational chatbot with memory".to_string()), category :
        Some("Chatbot".to_string()), tags : vec!["chatbot".to_string(), "conversation"
        .to_string(), "memory".to_string(),], version : "1.0.0".to_string(), author :
        Some("OxiFY Team".to_string()), usage_count : 2100, is_public : true, },
        TemplateListItem { id : "summarization".to_string(), name :
        "Document Summarization".to_string(), description :
        Some("Summarize long documents using map-reduce or iterative refinement"
        .to_string(),), category : Some("Data Processing".to_string()), tags :
        vec!["summarization".to_string(), "documents".to_string(), "map-reduce"
        .to_string(),], version : "1.0.0".to_string(), author : Some("OxiFY Team"
        .to_string()), usage_count : 560, is_public : true, }, TemplateListItem { id :
        "multi-agent".to_string(), name : "Multi-Agent Collaboration".to_string(),
        description : Some("Multiple AI agents working together on complex tasks"
        .to_string()), category : Some("Agent".to_string()), tags : vec!["multi-agent"
        .to_string(), "collaboration".to_string(), "orchestration".to_string(),], version
        : "1.0.0".to_string(), author : Some("OxiFY Team".to_string()), usage_count :
        340, is_public : true, }, TemplateListItem { id : "code-review".to_string(), name
        : "AI Code Review".to_string(), description :
        Some("Automated code review with security and quality checks".to_string()),
        category : Some("Developer Tools".to_string()), tags : vec!["code-review"
        .to_string(), "security".to_string(), "quality".to_string(),], version : "1.0.0"
        .to_string(), author : Some("OxiFY Team".to_string()), usage_count : 480,
        is_public : true, }, TemplateListItem { id : "translation".to_string(), name :
        "Multi-Language Translation".to_string(), description :
        Some("Translate content between multiple languages with quality checks"
        .to_string(),), category : Some("Data Processing".to_string()), tags :
        vec!["translation".to_string(), "multilingual".to_string(), "localization"
        .to_string(),], version : "1.0.0".to_string(), author : Some("OxiFY Team"
        .to_string()), usage_count : 390, is_public : true, },
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
        "Listing templates: category={:?}, tag={:?}, search={:?}", query.category, query
        .tag, query.search
    );
    let mut templates = get_builtin_templates();
    if let Some(ref category) = query.category {
        templates
            .retain(|t| {
                t.category.as_ref().is_some_and(|c| c.eq_ignore_ascii_case(category))
            });
    }
    if let Some(ref tag) = query.tag {
        templates.retain(|t| t.tags.iter().any(|t_tag| t_tag.eq_ignore_ascii_case(tag)));
    }
    if let Some(ref search) = query.search {
        let search_lower = search.to_lowercase();
        templates
            .retain(|t| {
                t.name.to_lowercase().contains(&search_lower)
                    || t
                        .description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&search_lower))
            });
    }
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);
    let templates: Vec<_> = templates.into_iter().skip(offset).take(limit).collect();
    Json(templates)
}
/// List template categories
#[allow(dead_code)]
#[utoipa::path(
    get,
    path = "/api/v1/templates/categories",
    responses(
        (
            status = 200,
            description = "List of categories with counts",
            body = Vec<TemplateCategory>
        )
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
                "Data Processing" => {
                    Some("Transform and process data with LLMs".to_string())
                }
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
/// Get template details by ID
#[allow(dead_code)]
#[utoipa::path(
    get,
    path = "/api/v1/templates/{id}",
    params(("id" = String, Path, description = "Template ID")),
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
    let template = templates
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Template '{}' not found", id),
                }),
            )
        })?;
    let parameters = match id.as_str() {
        "rag-basic" => {
            vec![
                TemplateParameter { name : "model".to_string(), param_type : "model"
                .to_string(), description : Some("LLM model to use".to_string()),
                required : true, default_value : Some("gpt-4".to_string()), },
                TemplateParameter { name : "collection".to_string(), param_type :
                "collection".to_string(), description :
                Some("Vector collection for retrieval".to_string()), required : true,
                default_value : None, }, TemplateParameter { name : "top_k".to_string(),
                param_type : "integer".to_string(), description :
                Some("Number of documents to retrieve".to_string()), required : false,
                default_value : Some("5".to_string()), },
            ]
        }
        "agent-react" => {
            vec![
                TemplateParameter { name : "model".to_string(), param_type : "model"
                .to_string(), description : Some("LLM model for agent reasoning"
                .to_string()), required : true, default_value : Some("gpt-4"
                .to_string()), }, TemplateParameter { name : "max_iterations"
                .to_string(), param_type : "integer".to_string(), description :
                Some("Maximum reasoning iterations".to_string()), required : false,
                default_value : Some("10".to_string()), }, TemplateParameter { name :
                "tools".to_string(), param_type : "string_array".to_string(), description
                : Some("Tools available to the agent".to_string()), required : false,
                default_value : Some("[\"search\", \"calculator\"]".to_string()), },
            ]
        }
        _ => {
            vec![
                TemplateParameter { name : "model".to_string(), param_type : "model"
                .to_string(), description : Some("LLM model to use".to_string()),
                required : true, default_value : Some("gpt-4".to_string()), }
            ]
        }
    };
    let preview_nodes = match id.as_str() {
        "rag-basic" => {
            vec![
                "Start".to_string(), "Query Embedding".to_string(), "Vector Search"
                .to_string(), "Context Assembly".to_string(), "LLM Generation"
                .to_string(), "End".to_string(),
            ]
        }
        "agent-react" => {
            vec![
                "Start".to_string(), "Think".to_string(), "Act (Tool Call)".to_string(),
                "Observe".to_string(), "Loop Check".to_string(), "End".to_string(),
            ]
        }
        "chatbot-simple" => {
            vec![
                "Start".to_string(), "Load History".to_string(), "LLM Response"
                .to_string(), "Save History".to_string(), "End".to_string(),
            ]
        }
        _ => vec!["Start".to_string(), "Process".to_string(), "End".to_string(),],
    };
    Ok(
        Json(TemplateDetail {
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
        }),
    )
}
/// Instantiate a template to create a new workflow
#[allow(dead_code)]
#[utoipa::path(
    post,
    path = "/api/v1/templates/{id}/instantiate",
    params(("id" = String, Path, description = "Template ID")),
    request_body = InstantiateTemplateRequest,
    responses(
        (
            status = 201,
            description = "Workflow created from template",
            body = InstantiateTemplateResponse
        ),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 400, description = "Invalid parameters", body = ErrorResponse)
    )
)]
pub async fn instantiate_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<InstantiateTemplateRequest>,
) -> Result<
    (StatusCode, Json<InstantiateTemplateResponse>),
    (StatusCode, Json<ErrorResponse>),
> {
    info!("Instantiating template: {} as '{}'", id, req.name);
    let templates = get_builtin_templates();
    let _template = templates
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Template '{}' not found", id),
                }),
            )
        })?;
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
    state
        .workflow_store
        .create(workflow)
        .await
        .map_err(|e| {
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
pub async fn get_metrics(
    State(state): State<Arc<AppState>>,
) -> Result<String, StatusCode> {
    let mut metrics = String::new();
    metrics.push_str(&state.http_metrics.to_prometheus_format().await);
    metrics.push('\n');
    if let Some(pool) = &state.db_pool {
        let pool_metrics = pool.metrics();
        let stats = pool_metrics.stats;
        metrics
            .push_str(
                &format!(
                    "# HELP oxify_db_pool_size Current size of the database connection pool\n\
             # TYPE oxify_db_pool_size gauge\n\
             oxify_db_pool_size {}\n",
                    stats.size
                ),
            );
        metrics
            .push_str(
                &format!(
                    "# HELP oxify_db_pool_idle Number of idle connections in the pool\n\
             # TYPE oxify_db_pool_idle gauge\n\
             oxify_db_pool_idle {}\n",
                    stats.num_idle
                ),
            );
        metrics
            .push_str(
                &format!(
                    "# HELP oxify_db_pool_max Maximum number of connections in the pool\n\
             # TYPE oxify_db_pool_max gauge\n\
             oxify_db_pool_max {}\n",
                    stats.max_connections
                ),
            );
        let utilization = if stats.max_connections > 0 {
            stats.size as f64 / stats.max_connections as f64
        } else {
            0.0
        };
        metrics
            .push_str(
                &format!(
                    "# HELP oxify_db_pool_utilization Database connection pool utilization (0-1)\n\
             # TYPE oxify_db_pool_utilization gauge\n\
             oxify_db_pool_utilization {:.3}\n",
                    utilization
                ),
            );
    }
    metrics
        .push_str(
            &format!(
                "# HELP oxify_api_info API version information\n\
         # TYPE oxify_api_info gauge\n\
         oxify_api_info{{version=\"{}\"}} 1\n",
                env!("CARGO_PKG_VERSION")
            ),
        );
    Ok(metrics)
}
