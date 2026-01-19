//! OpenAPI documentation module for Oxify Server.
//!
//! This module provides OpenAPI/Swagger documentation for all API endpoints.
//! Access the Swagger UI at: http://localhost:8080/swagger-ui/

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::OpenApi;

/// Main OpenAPI documentation structure.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Oxify Server API",
        version = "1.0.0",
        description = "Production-ready HTTP server for Oxify workflow engine",
        contact(
            name = "Oxify Team",
            email = "team@oxify.example.com",
            url = "https://github.com/oxify/oxify"
        ),
        license(
            name = "MIT OR Apache-2.0",
            url = "https://opensource.org/licenses/MIT"
        )
    ),
    paths(
        health_check,
        ready_check,
        live_check,
        metrics_endpoint,
    ),
    components(
        schemas(HealthResponse, ReadyResponse, LiveResponse, ErrorResponse)
    ),
    tags(
        (name = "Health", description = "Health check endpoints"),
        (name = "Metrics", description = "Prometheus metrics endpoints"),
        (name = "Workflow", description = "Workflow execution endpoints"),
        (name = "WebSocket", description = "WebSocket endpoints for real-time communication"),
        (name = "SSE", description = "Server-Sent Events for workflow updates")
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

/// Security scheme addon for JWT authentication.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .description(Some("JWT token for authentication"))
                        .build(),
                ),
            );
        }
    }
}

// ===== Health Check Endpoints =====

/// Health check response.
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({"status": "healthy", "timestamp": "2026-01-08T12:00:00Z"}))]
pub struct HealthResponse {
    /// Health status
    pub status: String,
    /// Current timestamp
    pub timestamp: String,
}

/// Readiness check response.
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({"ready": true, "checks": {"database": "ok", "redis": "ok"}}))]
pub struct ReadyResponse {
    /// Whether the service is ready
    pub ready: bool,
    /// Individual subsystem checks
    pub checks: std::collections::HashMap<String, String>,
}

/// Liveness check response.
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({"alive": true}))]
pub struct LiveResponse {
    /// Whether the service is alive
    pub alive: bool,
}

/// Error response (RFC 7807 Problem Details).
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({
    "type": "https://api.oxify.example.com/errors/validation",
    "title": "Validation Error",
    "status": 400,
    "detail": "Invalid request parameters"
}))]
pub struct ErrorResponse {
    /// Error type URI
    #[serde(rename = "type")]
    pub error_type: String,
    /// Short error title
    pub title: String,
    /// HTTP status code
    pub status: u16,
    /// Detailed error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Request instance URI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

// ===== OpenAPI Path Documentation =====

/// General health check endpoint.
///
/// Returns the current health status of the server.
#[utoipa::path(
    get,
    path = "/health",
    tags = ["Health"],
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
        (status = 503, description = "Service is unhealthy", body = ErrorResponse)
    )
)]
#[allow(dead_code)]
fn health_check() {}

/// Readiness check endpoint for Kubernetes.
///
/// Checks if the service is ready to accept traffic (database, cache, etc. are available).
#[utoipa::path(
    get,
    path = "/ready",
    tags = ["Health"],
    responses(
        (status = 200, description = "Service is ready", body = ReadyResponse),
        (status = 503, description = "Service is not ready", body = ErrorResponse)
    )
)]
#[allow(dead_code)]
fn ready_check() {}

/// Liveness check endpoint for Kubernetes.
///
/// Simple check to determine if the service is alive.
#[utoipa::path(
    get,
    path = "/live",
    tags = ["Health"],
    responses(
        (status = 200, description = "Service is alive", body = LiveResponse)
    )
)]
#[allow(dead_code)]
fn live_check() {}

/// Prometheus metrics endpoint.
///
/// Returns metrics in Prometheus text format for scraping.
#[utoipa::path(
    get,
    path = "/metrics",
    tags = ["Metrics"],
    responses(
        (status = 200, description = "Prometheus metrics", content_type = "text/plain")
    )
)]
#[allow(dead_code)]
fn metrics_endpoint() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_doc_generation() {
        let doc = ApiDoc::openapi();
        assert_eq!(doc.info.title, "Oxify Server API");
        assert_eq!(doc.info.version, "1.0.0");
    }

    #[test]
    fn test_health_response_schema() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            timestamp: "2026-01-08T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
    }

    #[test]
    fn test_ready_response_schema() {
        let mut checks = std::collections::HashMap::new();
        checks.insert("database".to_string(), "ok".to_string());
        let response = ReadyResponse {
            ready: true,
            checks,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("database"));
    }

    #[test]
    fn test_error_response_schema() {
        let error = ErrorResponse {
            error_type: "https://api.oxify.example.com/errors/validation".to_string(),
            title: "Validation Error".to_string(),
            status: 400,
            detail: Some("Invalid request parameters".to_string()),
            instance: None,
        };
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Validation Error"));
    }
}
