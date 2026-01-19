//! Authentication middleware and handlers for the UI

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension,
};
use oxify_authn::{JwtConfig, JwtManager, User};
use std::sync::Arc;

/// Extract JWT token from Authorization header or cookie
pub fn extract_token(req: &Request) -> Option<String> {
    // Try Authorization header first
    if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(stripped) = auth_str.strip_prefix("Bearer ") {
                return Some(stripped.to_string());
            }
        }
    }

    // Try cookie
    if let Some(cookie_header) = req.headers().get(header::COOKIE) {
        if let Ok(cookie_str) = cookie_header.to_str() {
            for cookie in cookie_str.split(';') {
                let cookie = cookie.trim();
                if let Some(stripped) = cookie.strip_prefix("auth_token=") {
                    return Some(stripped.to_string());
                }
            }
        }
    }

    None
}

/// Authentication middleware that validates JWT tokens
pub async fn auth_middleware(
    State(jwt_manager): State<Arc<JwtManager>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = extract_token(&req).ok_or(AuthError::MissingToken)?;

    let validation = jwt_manager
        .validate_token(&token)
        .map_err(|_| AuthError::InvalidToken)?;

    // Add user to request extensions
    req.extensions_mut().insert(validation.user);

    Ok(next.run(req).await)
}

/// Optional authentication middleware (allows unauthenticated requests)
pub async fn optional_auth_middleware(
    State(jwt_manager): State<Arc<JwtManager>>,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(token) = extract_token(&req) {
        if let Ok(validation) = jwt_manager.validate_token(&token) {
            req.extensions_mut().insert(validation.user);
        }
    }

    next.run(req).await
}

/// Extract authenticated user from request extensions
pub fn get_user(Extension(user): Extension<User>) -> User {
    user
}

/// Authentication errors
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Missing authentication token")]
    MissingToken,
    #[error("Invalid authentication token")]
    InvalidToken,
    #[error("Insufficient permissions")]
    InsufficientPermissions,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authentication token"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid authentication token"),
            AuthError::InsufficientPermissions => {
                (StatusCode::FORBIDDEN, "Insufficient permissions")
            }
        };

        (status, message).into_response()
    }
}

/// Create default JWT configuration for UI
pub fn create_jwt_config() -> JwtConfig {
    JwtConfig::development()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};

    #[test]
    fn test_extract_token_from_bearer() {
        let req = Request::builder()
            .header(header::AUTHORIZATION, "Bearer test_token_123")
            .body(Body::empty())
            .unwrap();

        assert_eq!(extract_token(&req), Some("test_token_123".to_string()));
    }

    #[test]
    fn test_extract_token_from_cookie() {
        let req = Request::builder()
            .header(header::COOKIE, "auth_token=cookie_token_456; other=value")
            .body(Body::empty())
            .unwrap();

        assert_eq!(extract_token(&req), Some("cookie_token_456".to_string()));
    }

    #[test]
    fn test_extract_token_missing() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert_eq!(extract_token(&req), None);
    }
}
