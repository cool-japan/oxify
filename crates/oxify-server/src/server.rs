//! HTTP server runtime with Axum and graceful shutdown
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY (simplified for LLM workflow focus)
//! License: MIT OR Apache-2.0 (compatible with OxiRS)

use crate::{
    middleware::{logging_middleware, request_id_middleware},
    shutdown::shutdown_signal_immediate,
    types::{Result, ServerConfig, ServerError},
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::json;
use tokio::net::TcpListener;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use tracing::info;

/// HTTP server runtime
pub struct ServerRuntime {
    config: ServerConfig,
    router: Option<Router>,
}

impl ServerRuntime {
    /// Create a new server runtime
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            router: None,
        }
    }

    /// Create with development configuration
    pub fn development() -> Self {
        Self::new(ServerConfig::development())
    }

    /// Create with production configuration
    pub fn production(config: ServerConfig) -> Self {
        Self::new(config)
    }

    /// Set custom router
    pub fn with_router(mut self, router: Router) -> Self {
        self.router = Some(router);
        self
    }

    /// Build the application router with middleware
    fn build_app(&self) -> Router {
        let app = self.router.clone().unwrap_or_else(|| self.default_router());

        // Build middleware stack by chaining layers
        let app = app
            .layer(axum::middleware::from_fn(request_id_middleware))
            .layer(TraceLayer::new_for_http());

        // Add request logging if enabled
        let app = if self.config.request_logging {
            app.layer(axum::middleware::from_fn(logging_middleware))
        } else {
            app
        };

        // Add compression if enabled
        #[cfg(feature = "compression")]
        let app = if self.config.compression {
            app.layer(CompressionLayer::new())
        } else {
            app
        };

        #[cfg(not(feature = "compression"))]
        let app = app;

        app
    }

    /// Default router with health check endpoints
    fn default_router(&self) -> Router {
        use crate::graphql::{create_schema, graphql_handler, graphql_playground};
        use crate::openapi::ApiDoc;
        use utoipa::OpenApi;
        use utoipa_swagger_ui::SwaggerUi;

        // Create GraphQL schema
        let graphql_schema = create_schema();

        Router::new()
            .route("/", get(root_handler))
            .route("/health", get(health_handler))
            .route("/ready", get(ready_handler))
            .route("/live", get(live_handler))
            .route("/graphql", get(graphql_playground).post(graphql_handler))
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .layer(axum::extract::Extension(graphql_schema))
    }

    /// Run the server
    pub async fn run(self) -> Result<()> {
        let addr = self.config.address;

        info!("Starting OxiFY server on {}", addr);
        info!("Configuration: {:#?}", self.config);

        // Build the application
        let app = self.build_app();

        // Bind to address
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::BindError(e.to_string()))?;

        info!("Server listening on {}", addr);

        // Run server with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal_immediate())
            .await
            .map_err(|e| ServerError::StartupFailed(e.to_string()))?;

        info!("Server shutdown complete");
        Ok(())
    }
}

/// Root handler
async fn root_handler() -> impl IntoResponse {
    Json(json!({
        "name": "OxiFY Server",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "running"
    }))
}

/// Health check handler
async fn health_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })),
    )
}

/// Readiness probe handler
async fn ready_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "ready",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })),
    )
}

/// Liveness probe handler
async fn live_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "alive",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_runtime_creation() {
        let runtime = ServerRuntime::development();
        assert_eq!(runtime.config.address.to_string(), "127.0.0.1:3000");
    }

    #[test]
    fn test_default_router() {
        let runtime = ServerRuntime::development();
        let _router = runtime.default_router();
        // Router creation should not panic
    }

    #[tokio::test]
    async fn test_health_handler() {
        let response = health_handler().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_ready_handler() {
        let response = ready_handler().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_live_handler() {
        let response = live_handler().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
