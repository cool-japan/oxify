//! Authorization middleware for protecting endpoints

use crate::auth::AuthUser;
use crate::types::ErrorResponse;
use crate::user_types::ApiUser;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use oxify_authn::Permission;
use std::sync::Arc;
use tracing::warn;

/// Extract authenticated user from request
async fn get_authenticated_user(
    headers: &axum::http::HeaderMap,
    state: &Arc<crate::handlers::AppState>,
) -> Result<ApiUser, (StatusCode, Json<ErrorResponse>)> {
    // Verify token and get user
    let auth_user = AuthUser::from_headers(headers, &state.auth)?;

    // Get full user from store to verify they still exist
    state
        .user_store
        .get(&auth_user.user_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                    message: "User not found".to_string(),
                }),
            )
        })
}

/// Check if user has a specific permission
fn has_permission(user: &ApiUser, permission: &Permission) -> bool {
    user.permissions.contains(permission)
}

/// Require authentication (any authenticated user)
pub async fn require_auth(
    State(state): State<Arc<crate::handlers::AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let headers = req.headers();
    let user = get_authenticated_user(headers, &state).await?;

    // Store user in request extensions for downstream handlers
    req.extensions_mut().insert(user);

    Ok(next.run(req).await)
}

/// Require Read permission
pub async fn require_read(
    State(state): State<Arc<crate::handlers::AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let headers = req.headers();
    let user = get_authenticated_user(headers, &state).await?;

    if !has_permission(&user, &Permission::Read) {
        warn!(
            "User {} lacks Read permission for {}",
            user.email,
            req.uri().path()
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Forbidden".to_string(),
                message: "Read permission required".to_string(),
            }),
        ));
    }

    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

/// Require Write permission
pub async fn require_write(
    State(state): State<Arc<crate::handlers::AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let headers = req.headers();
    let user = get_authenticated_user(headers, &state).await?;

    if !has_permission(&user, &Permission::Write) {
        warn!(
            "User {} lacks Write permission for {}",
            user.email,
            req.uri().path()
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Forbidden".to_string(),
                message: "Write permission required".to_string(),
            }),
        ));
    }

    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

/// Require Admin permission
pub async fn require_admin(
    State(state): State<Arc<crate::handlers::AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let headers = req.headers();
    let user = get_authenticated_user(headers, &state).await?;

    if !has_permission(&user, &Permission::Admin) {
        warn!(
            "User {} lacks Admin permission for {}",
            user.email,
            req.uri().path()
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Forbidden".to_string(),
                message: "Admin permission required".to_string(),
            }),
        ));
    }

    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}
