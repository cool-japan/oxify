//! Route definitions for the UI

use axum::{
    http::StatusCode,
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

use crate::handlers;
use crate::state::AppState;

/// Health check routes for Kubernetes liveness/readiness probes
pub fn health_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(|| async { StatusCode::OK }))
        .route("/livez", get(|| async { StatusCode::OK }))
}

/// UI page routes (HTML responses)
pub fn ui_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Authentication
        .route("/login", get(handlers::auth::login_page))
        .route("/login", post(handlers::auth::login_handler))
        .route("/logout", post(handlers::auth::logout_handler))
        .route("/register", post(handlers::auth::register_handler))
        // Dashboard
        .route("/", get(handlers::pages::dashboard))
        // Workflows
        .route("/workflows", get(handlers::pages::workflow_list))
        .route("/workflows/new", get(handlers::pages::workflow_new))
        .route("/workflows/{id}", get(handlers::pages::workflow_detail))
        .route("/workflows/{id}/edit", get(handlers::pages::workflow_edit))
        // Executions
        .route("/executions", get(handlers::pages::execution_list))
        .route(
            "/executions/compare",
            get(handlers::pages::execution_compare),
        )
        .route("/executions/{id}", get(handlers::pages::execution_detail))
        // Templates gallery
        .route("/templates", get(handlers::pages::templates_gallery))
        // Search
        .route("/search", get(handlers::pages::search))
        // Settings
        .route("/settings", get(handlers::pages::settings))
        // Custom 404 fallback
        .fallback(handlers::pages::handler_404)
}

/// HTMX routes (HTML partial responses)
pub fn htmx_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Workflow HTMX endpoints
        .route(
            "/htmx/workflows",
            get(handlers::htmx::workflow_list_partial),
        )
        .route(
            "/htmx/workflows/search",
            get(handlers::htmx::workflow_search),
        )
        .route("/htmx/workflows/{id}", get(handlers::htmx::workflow_card))
        .route(
            "/htmx/workflows/{id}",
            delete(handlers::htmx::workflow_delete),
        )
        .route(
            "/htmx/workflows/{id}/preview",
            get(handlers::htmx::workflow_preview),
        )
        // Execution HTMX endpoints
        .route(
            "/htmx/executions",
            get(handlers::htmx::execution_list_partial),
        )
        .route("/htmx/executions/rows", get(handlers::htmx::execution_rows))
        .route(
            "/htmx/executions/{id}/status",
            get(handlers::htmx::execution_status),
        )
        .route(
            "/htmx/executions/{id}/logs",
            get(handlers::htmx::execution_logs),
        )
        .route(
            "/htmx/executions/{id}/graph",
            get(handlers::htmx::execution_graph),
        )
        // SSE for real-time updates
        .route("/sse/executions/{id}", get(handlers::sse::execution_stream))
        // Multiplexed SSE for monitoring multiple executions
        .route(
            "/sse/executions",
            get(handlers::sse::execution_multi_stream),
        )
        // Node editor endpoints
        .route(
            "/htmx/nodes/form/{node_type}",
            get(handlers::htmx::node_form),
        )
        .route("/htmx/nodes/validate", post(handlers::htmx::node_validate))
        // Template gallery endpoints
        .route(
            "/htmx/templates",
            get(handlers::htmx::template_list_partial),
        )
        .route(
            "/htmx/templates/{id}",
            get(handlers::htmx::template_detail_partial),
        )
        // Toast notifications
        .route("/htmx/toast", get(handlers::htmx::toast))
}

/// JSON API routes (for frontend AJAX/fetch requests)
pub fn json_api_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Import/export endpoints
        .route(
            "/api/workflows/export",
            get(handlers::export::export_workflows),
        )
        .route(
            "/api/workflows/import",
            post(handlers::export::import_workflows),
        )
        // Workflow endpoints
        .route(
            "/api/v1/workflows",
            post(handlers::json_api::create_workflow),
        )
        .route(
            "/api/v1/workflows/{id}",
            put(handlers::json_api::update_workflow),
        )
        .route(
            "/api/v1/workflows/{id}",
            delete(handlers::json_api::delete_workflow),
        )
        // Workflow import/export
        .route(
            "/api/v1/workflows/{id}/export/yaml",
            get(handlers::json_api::export_workflow_yaml),
        )
        .route(
            "/api/v1/workflows/import/yaml",
            post(handlers::json_api::import_workflow_yaml),
        )
        // Execution endpoints
        .route(
            "/api/v1/executions",
            post(handlers::json_api::start_execution),
        )
        .route(
            "/api/v1/executions/{id}/cancel",
            post(handlers::json_api::cancel_execution),
        )
        .route(
            "/api/v1/executions/{id}/pause",
            post(handlers::json_api::pause_execution),
        )
        .route(
            "/api/v1/executions/{id}/resume",
            post(handlers::json_api::resume_execution),
        )
        // Add caching middleware for API responses
        .layer(middleware::from_fn(crate::cache::cache_api_responses))
}

/// All API routes (legacy name for backward compatibility)
#[allow(dead_code)]
pub fn api_routes() -> Router<Arc<AppState>> {
    htmx_routes().merge(json_api_routes())
}
