//! Structured error responses following RFC 7807 Problem Details
//!
//! Provides consistent error formatting for API responses.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

/// RFC 7807 Problem Details
///
/// Standard format for HTTP API error responses.
/// Spec: <https://tools.ietf.org/html/rfc7807>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemDetails {
    /// A URI reference that identifies the problem type
    #[serde(rename = "type")]
    pub problem_type: String,

    /// A short, human-readable summary of the problem type
    pub title: String,

    /// The HTTP status code
    pub status: u16,

    /// A human-readable explanation specific to this occurrence
    pub detail: Option<String>,

    /// A URI reference that identifies the specific occurrence
    pub instance: Option<String>,

    /// Additional extension members
    #[serde(flatten)]
    pub extensions: Option<serde_json::Value>,
}

impl ProblemDetails {
    /// Create a new problem details response
    pub fn new(status: StatusCode, title: impl Into<String>) -> Self {
        Self {
            problem_type: format!("https://httpstatuses.com/{}", status.as_u16()),
            title: title.into(),
            status: status.as_u16(),
            detail: None,
            instance: None,
            extensions: None,
        }
    }

    /// Set the detail message
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Set the instance URI
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// Set extension fields
    pub fn with_extensions(mut self, extensions: serde_json::Value) -> Self {
        self.extensions = Some(extensions);
        self
    }

    /// Set a custom problem type URI
    pub fn with_type(mut self, problem_type: impl Into<String>) -> Self {
        self.problem_type = problem_type.into();
        self
    }

    /// Create a bad request error
    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "Bad Request").with_detail(detail)
    }

    /// Create an unauthorized error
    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Unauthorized").with_detail(detail)
    }

    /// Create a forbidden error
    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "Forbidden").with_detail(detail)
    }

    /// Create a not found error
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not Found").with_detail(detail)
    }

    /// Create a rate limit error
    pub fn rate_limited(retry_after: u64) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "Too Many Requests")
            .with_detail("Rate limit exceeded")
            .with_extensions(serde_json::json!({
                "retry_after": retry_after
            }))
    }

    /// Create an internal server error
    pub fn internal_error(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").with_detail(detail)
    }

    /// Create a service unavailable error
    pub fn service_unavailable(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable").with_detail(detail)
    }

    /// Create a payload too large error
    pub fn payload_too_large(max_size: usize) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "Payload Too Large").with_detail(format!(
            "Request body exceeds maximum size of {} bytes",
            max_size
        ))
    }

    /// Get the HTTP status code
    pub fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl IntoResponse for ProblemDetails {
    fn into_response(self) -> Response {
        let status = self.status_code();
        (status, Json(self)).into_response()
    }
}

/// Application error type that can be converted to ProblemDetails
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Authorization error: {0}")]
    Authorization(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limit exceeded")]
    RateLimit { retry_after: u64 },

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("External service error: {0}")]
    ExternalService(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let problem = match self {
            AppError::Validation(msg) => ProblemDetails::bad_request(msg),
            AppError::Authentication(msg) => ProblemDetails::unauthorized(msg),
            AppError::Authorization(msg) => ProblemDetails::forbidden(msg),
            AppError::NotFound(msg) => ProblemDetails::not_found(msg),
            AppError::RateLimit { retry_after } => ProblemDetails::rate_limited(retry_after),
            AppError::Internal(msg) => ProblemDetails::internal_error(msg),
            AppError::Database(msg) => {
                // Don't expose internal database errors in production
                if cfg!(debug_assertions) {
                    ProblemDetails::internal_error(msg)
                } else {
                    ProblemDetails::internal_error("A database error occurred")
                }
            }
            AppError::ExternalService(msg) => ProblemDetails::service_unavailable(msg),
        };

        problem.into_response()
    }
}

/// Sanitize error messages for production
///
/// Removes sensitive information from error messages.
pub fn sanitize_error_message(error: &str, is_production: bool) -> String {
    if is_production {
        // In production, return generic messages for internal errors
        if error.contains("database")
            || error.contains("SQL")
            || error.contains("connection")
            || error.contains("panic")
        {
            return "An internal error occurred".to_string();
        }
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_problem_details_new() {
        let problem = ProblemDetails::new(StatusCode::BAD_REQUEST, "Bad Request");
        assert_eq!(problem.status, 400);
        assert_eq!(problem.title, "Bad Request");
        assert_eq!(problem.problem_type, "https://httpstatuses.com/400");
    }

    #[test]
    fn test_problem_details_with_detail() {
        let problem = ProblemDetails::bad_request("Invalid input");
        assert_eq!(problem.status, 400);
        assert_eq!(problem.detail, Some("Invalid input".to_string()));
    }

    #[test]
    fn test_problem_details_with_instance() {
        let problem =
            ProblemDetails::not_found("Resource not found").with_instance("/api/users/123");
        assert_eq!(problem.instance, Some("/api/users/123".to_string()));
    }

    #[test]
    fn test_problem_details_with_extensions() {
        let problem = ProblemDetails::rate_limited(60);
        assert!(problem.extensions.is_some());
        if let Some(ext) = problem.extensions {
            assert_eq!(ext["retry_after"], 60);
        }
    }

    #[test]
    fn test_problem_details_serialization() {
        let problem = ProblemDetails::bad_request("Invalid email format");
        let json = serde_json::to_string(&problem).unwrap();
        assert!(json.contains("Bad Request"));
        assert!(json.contains("Invalid email format"));
        assert!(json.contains("400"));
    }

    #[test]
    fn test_app_error_variants() {
        let err = AppError::Validation("Invalid input".to_string());
        assert!(err.to_string().contains("Validation error"));

        let err = AppError::NotFound("User not found".to_string());
        assert!(err.to_string().contains("Not found"));

        let err = AppError::RateLimit { retry_after: 60 };
        assert!(err.to_string().contains("Rate limit exceeded"));
    }

    #[test]
    fn test_sanitize_error_message() {
        let msg = "Database connection failed";
        assert_eq!(
            sanitize_error_message(msg, true),
            "An internal error occurred"
        );
        assert_eq!(
            sanitize_error_message(msg, false),
            "Database connection failed"
        );

        let msg = "Invalid user input";
        assert_eq!(sanitize_error_message(msg, true), "Invalid user input");
    }

    #[test]
    fn test_problem_details_status_code() {
        let problem = ProblemDetails::not_found("Resource not found");
        assert_eq!(problem.status_code(), StatusCode::NOT_FOUND);
    }
}
