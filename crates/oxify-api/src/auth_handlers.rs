//! Authentication endpoint handlers

use crate::auth::{AuthResponse, AuthUser, LoginRequest, RegisterRequest, UserInfo};
use crate::types::ErrorResponse;
use crate::user_types::ApiUser;
use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;
use tracing::{error, info};

/// Login handler
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = AuthResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse)
    )
)]
pub async fn login(
    State(state): State<Arc<crate::handlers::AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Login attempt for email: {}", req.email);

    // Get user from store
    let user = state
        .user_store
        .get_by_email(&req.email)
        .await
        .ok_or_else(|| {
            error!("User not found: {}", req.email);
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                    message: "Invalid email or password".to_string(),
                }),
            )
        })?;

    // Verify password
    let password_valid = state
        .auth
        .verify_password(&req.password, &user.password_hash)
        .map_err(|e| {
            error!("Password verification error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "InternalError".to_string(),
                    message: "Failed to verify password".to_string(),
                }),
            )
        })?;

    if !password_valid {
        error!("Invalid password for user: {}", req.email);
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
                message: "Invalid email or password".to_string(),
            }),
        ));
    }

    // Generate JWT token
    let token = state.auth.generate_token(&user).map_err(|e| {
        error!("Token generation error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "InternalError".to_string(),
                message: "Failed to generate token".to_string(),
            }),
        )
    })?;

    info!("Login successful for user: {}", user.email);

    Ok(Json(AuthResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        expires_in: 3600 * 24, // 24 hours
        user: UserInfo {
            id: user.id.to_string(),
            email: user.email.clone(),
            name: user.full_name.clone(),
            roles: user.roles.clone(),
        },
    }))
}

/// Register handler
#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Registration successful", body = AuthResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 409, description = "User already exists", body = ErrorResponse)
    )
)]
pub async fn register(
    State(state): State<Arc<crate::handlers::AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Registration attempt for email: {}", req.email);

    // Validate email format
    if !req.email.contains('@') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Invalid email format".to_string(),
            }),
        ));
    }

    // Validate password strength
    if req.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Password must be at least 8 characters".to_string(),
            }),
        ));
    }

    // Check if user already exists
    if state.user_store.get_by_email(&req.email).await.is_some() {
        error!("User already exists: {}", req.email);
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "ConflictError".to_string(),
                message: "User with this email already exists".to_string(),
            }),
        ));
    }

    // Hash password
    let password_hash = state.auth.hash_password(&req.password).map_err(|e| {
        error!("Password hashing error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "InternalError".to_string(),
                message: "Failed to hash password".to_string(),
            }),
        )
    })?;

    // Create user
    let user = ApiUser::new(req.email.clone(), req.email.clone(), password_hash);

    // Store user
    state.user_store.create(user.clone()).await.map_err(|e| {
        error!("User storage error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "InternalError".to_string(),
                message: "Failed to store user".to_string(),
            }),
        )
    })?;

    // Generate JWT token
    let token = state.auth.generate_token(&user).map_err(|e| {
        error!("Token generation error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "InternalError".to_string(),
                message: "Failed to generate token".to_string(),
            }),
        )
    })?;

    info!("Registration successful for user: {}", user.email);

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            access_token: token,
            token_type: "Bearer".to_string(),
            expires_in: 3600 * 24, // 24 hours
            user: UserInfo {
                id: user.id.to_string(),
                email: user.email.clone(),
                name: user.full_name.clone(),
                roles: user.roles.clone(),
            },
        }),
    ))
}

/// Get current user info (protected endpoint)
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    responses(
        (status = 200, description = "User information", body = UserInfo),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_current_user(
    State(state): State<Arc<crate::handlers::AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<UserInfo>, (StatusCode, Json<ErrorResponse>)> {
    // Extract and verify JWT token
    let auth_user = AuthUser::from_headers(&headers, &state.auth)?;

    // Get full user info from store
    let user = state
        .user_store
        .get(&auth_user.user_id)
        .await
        .ok_or_else(|| {
            error!("User not found in store: {}", auth_user.user_id);
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                    message: "User not found".to_string(),
                }),
            )
        })?;

    Ok(Json(UserInfo {
        id: user.id.to_string(),
        email: user.email.clone(),
        name: user.full_name.clone(),
        roles: user.roles.clone(),
    }))
}
