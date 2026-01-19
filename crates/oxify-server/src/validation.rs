//! Request validation middleware
//!
//! Provides input sanitization, size limits, and timeout handling.

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::time::Duration;

/// Request validation configuration
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Maximum request body size in bytes (default: 10MB)
    pub max_body_size: usize,
    /// Request timeout duration (default: 30s)
    pub request_timeout: Duration,
    /// Maximum URI length (default: 8KB)
    pub max_uri_length: usize,
    /// Maximum number of headers (default: 100)
    pub max_headers: usize,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_body_size: 10 * 1024 * 1024, // 10MB
            request_timeout: Duration::from_secs(30),
            max_uri_length: 8 * 1024, // 8KB
            max_headers: 100,
        }
    }
}

impl ValidationConfig {
    /// Create a new validation configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum body size in bytes
    pub fn with_max_body_size(mut self, size: usize) -> Self {
        self.max_body_size = size;
        self
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set maximum URI length
    pub fn with_max_uri_length(mut self, length: usize) -> Self {
        self.max_uri_length = length;
        self
    }
}

/// Request validation middleware
///
/// Validates request size limits and rejects oversized requests.
pub async fn validation_middleware(
    config: ValidationConfig,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    // Validate URI length
    if request.uri().to_string().len() > config.max_uri_length {
        return Err((
            StatusCode::URI_TOO_LONG,
            "Request URI exceeds maximum allowed length".to_string(),
        ));
    }

    // Validate header count
    if request.headers().len() > config.max_headers {
        return Err((
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            "Too many request headers".to_string(),
        ));
    }

    // Validate Content-Length header if present
    if let Some(content_length) = request.headers().get(header::CONTENT_LENGTH) {
        if let Ok(length_str) = content_length.to_str() {
            if let Ok(length) = length_str.parse::<usize>() {
                if length > config.max_body_size {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!(
                            "Request body exceeds maximum size of {} bytes",
                            config.max_body_size
                        ),
                    ));
                }
            }
        }
    }

    // Process request with timeout
    match tokio::time::timeout(config.request_timeout, next.run(request)).await {
        Ok(response) => Ok(response),
        Err(_) => Err((
            StatusCode::REQUEST_TIMEOUT,
            "Request processing exceeded timeout".to_string(),
        )),
    }
}

/// Body size limit layer
///
/// Can be used as a tower layer to enforce body size limits.
pub fn body_limit_layer(max_size: usize) -> tower_http::limit::RequestBodyLimitLayer {
    tower_http::limit::RequestBodyLimitLayer::new(max_size)
}

/// Sanitize string input to prevent injection attacks
pub fn sanitize_string(input: &str) -> String {
    input
        .chars()
        .filter(|c| {
            // Allow alphanumeric, spaces, and common punctuation
            c.is_alphanumeric()
                || c.is_whitespace()
                || matches!(c, '-' | '_' | '.' | ',' | ':' | ';' | '!' | '?' | '@')
        })
        .collect()
}

/// Validate JSON structure size
pub fn validate_json_depth(value: &serde_json::Value, max_depth: usize) -> bool {
    fn check_depth(value: &serde_json::Value, current_depth: usize, max_depth: usize) -> bool {
        if current_depth > max_depth {
            return false;
        }

        match value {
            serde_json::Value::Array(arr) => arr
                .iter()
                .all(|v| check_depth(v, current_depth + 1, max_depth)),
            serde_json::Value::Object(obj) => obj
                .values()
                .all(|v| check_depth(v, current_depth + 1, max_depth)),
            _ => true,
        }
    }

    check_depth(value, 0, max_depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validation_config_default() {
        let config = ValidationConfig::default();
        assert_eq!(config.max_body_size, 10 * 1024 * 1024);
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.max_uri_length, 8 * 1024);
        assert_eq!(config.max_headers, 100);
    }

    #[test]
    fn test_validation_config_builder() {
        let config = ValidationConfig::new()
            .with_max_body_size(1024)
            .with_timeout(Duration::from_secs(10))
            .with_max_uri_length(4096);

        assert_eq!(config.max_body_size, 1024);
        assert_eq!(config.request_timeout, Duration::from_secs(10));
        assert_eq!(config.max_uri_length, 4096);
    }

    #[test]
    fn test_sanitize_string() {
        assert_eq!(sanitize_string("hello world"), "hello world");
        assert_eq!(sanitize_string("hello<script>"), "helloscript");
        assert_eq!(sanitize_string("user@example.com"), "user@example.com");
        assert_eq!(sanitize_string("a-b_c.d"), "a-b_c.d");
    }

    #[test]
    fn test_validate_json_depth() {
        let shallow = json!({"a": 1, "b": 2});
        assert!(validate_json_depth(&shallow, 10));

        let deep = json!({
            "a": {
                "b": {
                    "c": {
                        "d": {
                            "e": "too deep"
                        }
                    }
                }
            }
        });
        assert!(!validate_json_depth(&deep, 3));
        assert!(validate_json_depth(&deep, 10));
    }

    #[test]
    fn test_validate_json_array_depth() {
        let nested_array = json!([[[[1, 2, 3]]]]);
        assert!(!validate_json_depth(&nested_array, 2));
        assert!(validate_json_depth(&nested_array, 5));
    }
}
