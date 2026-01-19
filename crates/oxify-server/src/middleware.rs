//! HTTP middleware for authentication, logging, and request tracking
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY (simplified for LLM workflow focus)
//! License: MIT OR Apache-2.0 (compatible with OxiRS)

use axum::{
    extract::Request,
    http::{header, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use oxify_authn::JwtManager;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Authentication middleware
///
/// Validates JWT tokens from Authorization header.
/// Skips authentication for public routes (health checks, etc.).
pub async fn auth_middleware(
    jwt_manager: Arc<JwtManager>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path();

    // Skip auth for public routes
    if is_public_route(path) {
        return Ok(next.run(request).await);
    }

    // Extract token from Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(header_value) => {
            JwtManager::extract_token_from_header(header_value).ok_or(StatusCode::UNAUTHORIZED)?
        }
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // Validate token
    let validation = jwt_manager
        .validate_token(token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Store user in request extensions for downstream handlers
    request.extensions_mut().insert(validation.user.clone());

    Ok(next.run(request).await)
}

/// Request logging middleware
///
/// Logs all incoming requests with method, path, status, and duration.
pub async fn logging_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_string();

    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed();

    let status = response.status();

    // Log based on status code level
    if status.is_success() {
        tracing::info!(
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request completed"
        );
    } else if status.is_client_error() {
        tracing::warn!(
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request completed"
        );
    } else {
        tracing::error!(
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request completed"
        );
    }

    response
}

/// Request ID middleware
///
/// Adds a unique request ID to each request for tracing.
/// Uses X-Request-ID header if present, otherwise generates a new UUID.
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    // Try to get existing request ID from header
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Store request ID in extensions
    request.extensions_mut().insert(request_id.clone());

    let mut response = next.run(request).await;

    // Add request ID to response headers
    if let Ok(header_value) = request_id.parse() {
        response.headers_mut().insert("x-request-id", header_value);
    }

    response
}

/// CORS middleware
///
/// Handles Cross-Origin Resource Sharing for web clients.
#[cfg(feature = "cors")]
pub async fn cors_middleware(request: Request, next: Next) -> Response {
    use axum::http::HeaderValue;

    let method = request.method().clone();
    let mut response = next.run(request).await;

    // Add CORS headers to response
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("3600"),
    );

    // Handle preflight requests
    if method == Method::OPTIONS {
        *response.status_mut() = StatusCode::OK;
    }

    response
}

/// Check if a route is public (doesn't require authentication)
fn is_public_route(path: &str) -> bool {
    matches!(path, "/health" | "/metrics" | "/ready" | "/live" | "/")
}

/// OpenTelemetry tracing middleware
///
/// Automatically creates spans for HTTP requests with W3C Trace Context propagation.
/// Records request method, path, status code, and duration.
pub async fn otel_tracing_middleware(request: Request, next: Next) -> Response {
    use opentelemetry::trace::{SpanKind, Status, TraceContextExt, Tracer};
    use opentelemetry::{global, Context, KeyValue};

    let start = Instant::now();
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    // Create a new span for this request
    let tracer = global::tracer("http");
    let span = tracer
        .span_builder(format!("{} {}", method, path))
        .with_kind(SpanKind::Server)
        .with_attributes(vec![
            KeyValue::new("http.method", method.clone()),
            KeyValue::new("http.route", path.clone()),
            KeyValue::new(
                "http.scheme",
                request.uri().scheme_str().unwrap_or("http").to_string(),
            ),
        ])
        .start(&tracer);

    let cx = Context::current().with_span(span);
    let _guard = cx.clone().attach();

    // Process request
    let response = next.run(request).await;

    // Record response details
    let status_code = response.status().as_u16();
    let current_span = cx.span();
    current_span.set_attribute(KeyValue::new("http.status_code", status_code as i64));

    // Set span status based on HTTP status code
    if status_code >= 500 {
        current_span.set_status(Status::error("Server error"));
    } else if status_code >= 400 {
        current_span.set_status(Status::error("Client error"));
    }

    // Record duration
    let duration = start.elapsed();
    current_span.add_event(
        "request_completed",
        vec![KeyValue::new("duration_ms", duration.as_millis() as i64)],
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_public_route() {
        assert!(is_public_route("/health"));
        assert!(is_public_route("/metrics"));
        assert!(is_public_route("/ready"));
        assert!(is_public_route("/live"));
        assert!(is_public_route("/"));
        assert!(!is_public_route("/api/workflow"));
        assert!(!is_public_route("/api/execute"));
    }
}
