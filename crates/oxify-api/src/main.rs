//! OxiFY API Server
//!
//! REST API for LLM workflow orchestration with real-time updates via SSE

use axum::{
    middleware::from_fn_with_state,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod auth;
mod auth_handlers;
mod authz_middleware;
mod batch_handlers;
// Disabled for SQLite migration (requires DatabaseCheckpointStore)
// mod checkpoint_handlers;
// mod checkpoint_types;
mod handlers;
mod mcp_handlers;
mod mcp_types;
mod middleware;
mod otel;
mod rollback_handlers;
// Disabled for SQLite migration (requires SecretStore)
// mod secret_handlers;
// mod secret_types;
mod sse;
mod storage;
mod types;
mod user_types;
mod vector_handlers;
mod version_handlers;
mod version_types;
mod websocket;

use handlers::*;

/// OpenAPI documentation
#[allow(dead_code)]
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health,
        handlers::create_workflow,
        handlers::get_workflow,
        handlers::list_workflows,
        handlers::update_workflow,
        handlers::delete_workflow,
        handlers::execute_workflow,
        handlers::get_execution,
        handlers::list_executions,
        handlers::list_workflow_executions,
        handlers::cancel_execution,
        handlers::export_workflow,
        handlers::import_workflow,
        batch_handlers::batch_create_workflows,
        batch_handlers::batch_delete_workflows,
        batch_handlers::batch_execute_workflows,
        batch_handlers::batch_get_workflows,
        auth_handlers::login,
        auth_handlers::register,
        auth_handlers::get_current_user,
        // Disabled for SQLite migration (requires SecretStore)
        // secret_handlers::create_secret,
        // secret_handlers::get_secret,
        // secret_handlers::list_secrets,
        // secret_handlers::update_secret,
        // secret_handlers::delete_secret,
        // secret_handlers::get_secret_audit_logs,
        version_handlers::save_workflow_version,
        version_handlers::get_workflow_versions,
        version_handlers::get_workflow_version,
        version_handlers::compare_workflow_versions,
        version_handlers::restore_workflow_version,
        // Disabled for SQLite migration (requires DatabaseCheckpointStore)
        // checkpoint_handlers::pause_execution,
        // checkpoint_handlers::resume_execution,
        // checkpoint_handlers::list_execution_checkpoints,
        // checkpoint_handlers::delete_execution_checkpoints,
        rollback_handlers::create_snapshot,
        rollback_handlers::list_snapshots,
        rollback_handlers::rollback_execution,
        rollback_handlers::get_rollback_summary,
        rollback_handlers::clear_snapshots,
        vector_handlers::create_collection,
        vector_handlers::list_collections,
        vector_handlers::get_collection,
        vector_handlers::delete_collection,
        vector_handlers::insert_vectors,
        vector_handlers::search_vectors,
        vector_handlers::delete_vector,
        sse::stream_execution,
    ),
    components(
        schemas(
            types::HealthResponse,
            types::CreateWorkflowRequest,
            types::CreateWorkflowResponse,
            types::UpdateWorkflowRequest,
            types::UpdateWorkflowResponse,
            types::GetWorkflowResponse,
            types::ListWorkflowsResponse,
            types::DeleteWorkflowResponse,
            types::ExecuteWorkflowRequest,
            types::ExecuteWorkflowResponse,
            types::GetExecutionResponse,
            types::ListExecutionsResponse,
            types::ExecutionSummary,
            types::ErrorResponse,
            auth::LoginRequest,
            auth::RegisterRequest,
            auth::AuthResponse,
            auth::UserInfo,
            // Disabled for SQLite migration (requires SecretStore)
            // secret_types::CreateSecretRequest,
            // secret_types::CreateSecretResponse,
            // secret_types::UpdateSecretRequest,
            // secret_types::UpdateSecretResponse,
            // secret_types::GetSecretResponse,
            // secret_types::ListSecretsResponse,
            // secret_types::DeleteSecretResponse,
            // secret_types::GetSecretAuditLogsResponse,
            version_types::SaveVersionRequest,
            version_types::SaveVersionResponse,
            version_types::GetVersionHistoryResponse,
            version_types::WorkflowVersionSummary,
            version_types::GetVersionResponse,
            version_types::CompareVersionsResponse,
            version_types::RestoreVersionResponse,
            // Disabled for SQLite migration (requires DatabaseCheckpointStore)
            // checkpoint_types::PauseExecutionRequest,
            // checkpoint_types::PauseExecutionResponse,
            // checkpoint_types::ResumeExecutionResponse,
            // checkpoint_types::CheckpointSummary,
            // checkpoint_types::ListCheckpointsResponse,
            // checkpoint_types::DeleteCheckpointsResponse,
            handlers::ImportWorkflowRequest,
            handlers::ImportWorkflowResponse,
            batch_handlers::BatchCreateWorkflowsRequest,
            batch_handlers::BatchWorkflowResult,
            batch_handlers::BatchCreateWorkflowsResponse,
            batch_handlers::BatchDeleteWorkflowsRequest,
            batch_handlers::BatchDeleteResult,
            batch_handlers::BatchDeleteWorkflowsResponse,
            batch_handlers::BatchExecutionItem,
            batch_handlers::BatchExecuteWorkflowsRequest,
            batch_handlers::BatchExecutionResult,
            batch_handlers::BatchExecuteWorkflowsResponse,
            batch_handlers::BatchGetWorkflowsRequest,
            batch_handlers::BatchGetResult,
            batch_handlers::BatchGetWorkflowsResponse,
            rollback_handlers::CreateSnapshotRequest,
            rollback_handlers::CreateSnapshotResponse,
            rollback_handlers::ListSnapshotsResponse,
            rollback_handlers::SnapshotSummary,
            rollback_handlers::RollbackRequest,
            rollback_handlers::RollbackResponse,
            rollback_handlers::GetRollbackSummaryResponse,
            vector_handlers::CreateCollectionRequest,
            vector_handlers::CollectionInfo,
            vector_handlers::InsertVectorsRequest,
            vector_handlers::VectorWithId,
            vector_handlers::SearchVectorsRequest,
            vector_handlers::SearchResult,
        )
    ),
    tags(
        (name = "oxify-api", description = "OxiFY Workflow Orchestration API")
    ),
    info(
        title = "OxiFY API",
        version = "0.1.0",
        description = "REST API for LLM workflow orchestration",
        license(name = "MIT OR Apache-2.0"),
    ),
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

#[allow(dead_code)]
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            )
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables first (needed for OTel config)
    dotenv::dotenv().ok();

    // Initialize tracing (with OpenTelemetry if enabled)
    let otel_config = otel::OtelConfig::default();
    otel::init_tracing(otel_config)?;

    // Create application state with database support
    info!("Initializing database connection...");
    let state = Arc::new(
        AppState::new_with_database()
            .await
            .expect("Failed to initialize database"),
    );
    info!("Database initialized successfully");

    // Clone http_metrics for use in middleware before state is moved
    let http_metrics = state.http_metrics.clone();

    // Public routes (no authentication required)
    let public_router = Router::new()
        .route("/health", get(health))
        // Kubernetes-style liveness / readiness probes
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
        .route("/metrics", get(get_metrics))
        .route("/api/v1/auth/login", post(auth_handlers::login))
        .route("/api/v1/auth/register", post(auth_handlers::register))
        // Public webhook event receiver
        .route(
            "/api/v1/webhooks/events/{webhook_id}",
            post(receive_webhook_event),
        );

    // Auth-only routes (just need to be authenticated)
    let auth_router = Router::new()
        .route("/api/v1/auth/me", get(auth_handlers::get_current_user))
        // WebSocket endpoint for real-time updates
        .route("/api/v1/ws", get(websocket::ws_handler))
        .route_layer(from_fn_with_state(
            state.clone(),
            authz_middleware::require_auth,
        ));

    // Read-only routes (require Read permission)
    let read_router = Router::new()
        .route("/api/v1/workflows", get(list_workflows))
        .route("/api/v1/workflows/{id}", get(get_workflow))
        .route("/api/v1/workflows/{id}/export", get(export_workflow))
        .route("/api/v1/workflows/{id}/cost", post(estimate_workflow_cost))
        .route(
            "/api/v1/workflows/{id}/analyze/batching",
            get(analyze_workflow_batching),
        )
        .route(
            "/api/v1/workflows/{id}/analyze/optimize",
            get(analyze_workflow_optimization),
        )
        .route(
            "/api/v1/workflows/{id}/schedules",
            get(list_workflow_schedules),
        )
        .route("/api/v1/executions", get(list_executions))
        .route("/api/v1/executions/{id}", get(get_execution))
        // Secret read routes (DISABLED for SQLite migration)
        // .route("/api/v1/secrets", get(secret_handlers::list_secrets))
        // .route("/api/v1/secrets/{id}", get(secret_handlers::get_secret))
        // .route(
        //     "/api/v1/secrets/{id}/audit",
        //     get(secret_handlers::get_secret_audit_logs),
        // )
        // Version read routes
        .route(
            "/api/v1/workflows/{id}/versions",
            get(version_handlers::get_workflow_versions),
        )
        .route(
            "/api/v1/workflows/{id}/versions/{version}",
            get(version_handlers::get_workflow_version),
        )
        .route(
            "/api/v1/workflows/{id}/versions/compare",
            get(version_handlers::compare_workflow_versions),
        )
        .route("/api/v1/executions/{id}/stream", get(sse::stream_execution))
        .route(
            "/api/v1/workflows/{id}/executions",
            get(list_workflow_executions),
        )
        .route("/api/v1/schedules", get(list_schedules))
        .route("/api/v1/schedules/{id}", get(get_schedule))
        .route("/api/v1/schedules/{id}/history", get(get_schedule_history))
        .route("/api/v1/analytics/executions", get(get_execution_analytics))
        .route("/api/v1/analytics/workflows", get(get_top_workflows))
        .route(
            "/api/v1/analytics/workflows/{id}",
            get(get_workflow_analytics),
        )
        .route("/api/v1/analytics/trends", get(get_execution_trends))
        // Webhook read routes
        .route("/api/v1/webhooks", get(list_webhooks))
        .route("/api/v1/webhooks/{id}", get(get_webhook))
        .route("/api/v1/webhooks/{id}/events", get(list_webhook_events))
        .route("/api/v1/webhooks/{id}/stats", get(get_webhook_stats))
        // Checkpoint read routes (DISABLED for SQLite migration)
        // .route(
        //     "/api/v1/executions/{id}/checkpoints",
        //     get(checkpoint_handlers::list_execution_checkpoints),
        // )
        // Rollback read routes
        .route(
            "/api/v1/executions/{id}/snapshots",
            get(rollback_handlers::list_snapshots),
        )
        .route(
            "/api/v1/executions/{id}/rollback/summary",
            get(rollback_handlers::get_rollback_summary),
        )
        // Approval read routes
        .route("/api/v1/approvals", get(list_pending_approvals))
        .route("/api/v1/approvals/{id}", get(get_approval))
        .route(
            "/api/v1/executions/{id}/approvals",
            get(list_execution_approvals),
        )
        // Vector store read routes
        .route(
            "/api/v1/vectors/collections",
            get(vector_handlers::list_collections),
        )
        .route(
            "/api/v1/vectors/collections/{name}",
            get(vector_handlers::get_collection),
        )
        // MCP read routes
        .route("/api/v1/mcp/servers", get(mcp_handlers::list_mcp_servers))
        .route("/api/v1/mcp/tools", post(mcp_handlers::list_mcp_tools))
        .route("/api/v1/mcp/stats", get(mcp_handlers::get_registry_stats))
        .route_layer(from_fn_with_state(
            state.clone(),
            authz_middleware::require_read,
        ));

    // Write routes (require Write permission)
    let write_router = Router::new()
        .route("/api/v1/workflows", post(create_workflow))
        .route("/api/v1/workflows/import", post(import_workflow))
        .route(
            "/api/v1/workflows/batch",
            post(batch_handlers::batch_create_workflows),
        )
        .route(
            "/api/v1/workflows/batch/get",
            post(batch_handlers::batch_get_workflows),
        )
        .route("/api/v1/workflows/{id}", put(update_workflow))
        .route("/api/v1/workflows/{id}/execute", post(execute_workflow))
        .route("/api/v1/workflows/{id}/test", post(test_workflow))
        .route("/api/v1/executions/{id}/cancel", post(cancel_execution))
        .route(
            "/api/v1/executions/batch",
            post(batch_handlers::batch_execute_workflows),
        )
        .route("/api/v1/schedules", post(create_schedule))
        .route("/api/v1/schedules/{id}", put(update_schedule))
        // Webhook write routes
        .route("/api/v1/webhooks", post(create_webhook))
        .route("/api/v1/webhooks/{id}", put(update_webhook))
        // Secret write routes (DISABLED for SQLite migration)
        // .route("/api/v1/secrets", post(secret_handlers::create_secret))
        // .route("/api/v1/secrets/{id}", put(secret_handlers::update_secret))
        // Version write routes
        .route(
            "/api/v1/workflows/{id}/versions",
            post(version_handlers::save_workflow_version),
        )
        .route(
            "/api/v1/workflows/{id}/versions/{version}/restore",
            post(version_handlers::restore_workflow_version),
        )
        // Checkpoint write routes (DISABLED for SQLite migration)
        // .route(
        //     "/api/v1/executions/{id}/pause",
        //     post(checkpoint_handlers::pause_execution),
        // )
        // .route(
        //     "/api/v1/executions/{id}/resume",
        //     post(checkpoint_handlers::resume_execution),
        // )
        // Approval write routes
        .route("/api/v1/approvals/{id}/approve", post(approve_approval))
        .route("/api/v1/approvals/{id}/reject", post(reject_approval))
        // Rollback write routes
        .route(
            "/api/v1/executions/{id}/snapshots",
            post(rollback_handlers::create_snapshot),
        )
        .route(
            "/api/v1/executions/{id}/rollback",
            post(rollback_handlers::rollback_execution),
        )
        // Vector store write routes
        .route(
            "/api/v1/vectors/collections",
            post(vector_handlers::create_collection),
        )
        .route(
            "/api/v1/vectors/collections/{name}/vectors",
            post(vector_handlers::insert_vectors),
        )
        .route(
            "/api/v1/vectors/collections/{name}/search",
            post(vector_handlers::search_vectors),
        )
        // MCP write routes
        .route(
            "/api/v1/mcp/servers/register",
            post(mcp_handlers::register_mcp_server),
        )
        .route(
            "/api/v1/mcp/tools/invoke",
            post(mcp_handlers::invoke_mcp_tool),
        )
        .route_layer(from_fn_with_state(
            state.clone(),
            authz_middleware::require_write,
        ));

    // Delete routes (require Admin permission)
    let admin_router = Router::new()
        .route("/api/v1/workflows/{id}", delete(delete_workflow))
        .route(
            "/api/v1/workflows/batch",
            delete(batch_handlers::batch_delete_workflows),
        )
        .route("/api/v1/schedules/{id}", delete(delete_schedule))
        .route("/api/v1/webhooks/{id}", delete(delete_webhook))
        // Secret delete routes (DISABLED for SQLite migration)
        // .route(
        //     "/api/v1/secrets/{id}",
        //     delete(secret_handlers::delete_secret),
        // )
        // Checkpoint delete routes (DISABLED for SQLite migration)
        // .route(
        //     "/api/v1/executions/{id}/checkpoints",
        //     delete(checkpoint_handlers::delete_execution_checkpoints),
        // )
        // Rollback delete routes
        .route(
            "/api/v1/executions/{id}/snapshots",
            delete(rollback_handlers::clear_snapshots),
        )
        // Vector store delete routes
        .route(
            "/api/v1/vectors/collections/{name}",
            delete(vector_handlers::delete_collection),
        )
        .route(
            "/api/v1/vectors/collections/{name}/vectors/{id}",
            delete(vector_handlers::delete_vector),
        )
        // MCP admin routes
        .route(
            "/api/v1/mcp/servers/unregister",
            delete(mcp_handlers::unregister_mcp_server),
        )
        .route_layer(from_fn_with_state(
            state.clone(),
            authz_middleware::require_admin,
        ));

    // OpenAPI spec endpoint (serves JSON)
    let openapi_router = Router::new()
        .route(
            "/api-docs/openapi.json",
            get(|| async { axum::Json(ApiDoc::openapi()) }),
        )
        // Swagger UI for interactive API documentation
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()));

    // Combine all routers
    let api_router = public_router
        .merge(auth_router)
        .merge(read_router)
        .merge(write_router)
        .merge(admin_router)
        .merge(openapi_router)
        .with_state(state);

    let app = api_router
        // Add middleware
        .layer(middleware::HttpMetricsLayer::new(http_metrics))
        .layer(middleware::RateLimitLayer::new(
            middleware::RateLimitConfig::medium(), // 500 req/min per IP
        ))
        .layer(middleware::ApiVersionLayer::new(
            middleware::ApiVersion::v1(),
        ))
        .layer(CompressionLayer::new())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    let addr = listener.local_addr()?;

    info!("🚀 OxiFY API Server starting");
    info!("📍 Listening on http://{}", addr);
    info!("📚 OpenAPI Spec: http://{}/api-docs/openapi.json", addr);
    info!("📖 Swagger UI: http://{}/swagger-ui", addr);
    info!("🔍 Health check: http://{}/health", addr);
    info!("📊 Prometheus metrics: http://{}/metrics", addr);

    axum::serve(listener, app).await?;

    // Gracefully shutdown tracing (flush pending traces)
    otel::shutdown_tracing();

    Ok(())
}
