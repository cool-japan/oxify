//! Authentication handlers for login, logout, and registration

use crate::state::AppState;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use oxify_authn::{PasswordManager, Permission, User};
use serde::Deserialize;
use std::sync::Arc;

/// Login form data
#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub remember_me: Option<bool>,
}

/// Login handler - authenticates user and returns JWT token
pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    Form(form): Form<LoginForm>,
) -> Result<Response, AuthHandlerError> {
    // In production, validate credentials against database
    // For now, we'll accept any non-empty username/password
    if form.username.is_empty() || form.password.is_empty() {
        return Err(AuthHandlerError::InvalidCredentials);
    }

    // For demo: validate password (in production, check against database)
    let password_manager = PasswordManager::default();
    // Here we would check against stored hash, for now just verify it's valid
    let _strength = password_manager.check_password_strength(&form.password);

    // Create user object
    let user = User {
        username: form.username.clone(),
        roles: vec!["user".to_string()],
        email: Some(format!("{}@example.com", form.username)),
        full_name: Some(form.username.clone()),
        last_login: Some(chrono::Utc::now()),
        permissions: vec![Permission::Read, Permission::Write],
    };

    // Generate JWT token
    let token = state
        .jwt_manager
        .generate_token(&user)
        .map_err(|_| AuthHandlerError::TokenGenerationFailed)?;

    // Set cookie and redirect to dashboard
    let cookie = format!(
        "auth_token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        token,
        if form.remember_me.unwrap_or(false) {
            86400 * 30 // 30 days
        } else {
            3600 * 8 // 8 hours
        }
    );

    Ok((
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, cookie),
            (header::LOCATION, "/".to_string()),
        ],
    )
        .into_response())
}

/// Logout handler - clears authentication token
pub async fn logout_handler() -> impl IntoResponse {
    let cookie = "auth_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0";

    (
        StatusCode::SEE_OTHER,
        [(header::SET_COOKIE, cookie), (header::LOCATION, "/login")],
    )
}

/// Show login page
pub async fn login_page() -> impl IntoResponse {
    // In a real app, this would render a login template
    // For now, return a simple redirect to the main page
    Redirect::to("/")
}

/// Registration form data
#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub password: String,
    pub email: String,
    pub full_name: String,
}

/// Register handler - creates new user account
pub async fn register_handler(
    State(state): State<Arc<AppState>>,
    Form(form): Form<RegisterForm>,
) -> Result<Response, AuthHandlerError> {
    // Validate input
    if form.username.is_empty()
        || form.password.is_empty()
        || form.email.is_empty()
        || form.full_name.is_empty()
    {
        return Err(AuthHandlerError::InvalidInput);
    }

    // Check password strength
    let password_manager = PasswordManager::default();
    let strength = password_manager.check_password_strength(&form.password);
    if !matches!(
        strength,
        oxify_authn::PasswordStrength::Strong | oxify_authn::PasswordStrength::VeryStrong
    ) {
        return Err(AuthHandlerError::WeakPassword);
    }

    // Hash password
    let _password_hash = password_manager
        .hash_password(&form.password)
        .map_err(|_| AuthHandlerError::HashingFailed)?;

    // In production: Save user to database with hashed password
    // For now, just create user and generate token

    let user = User {
        username: form.username.clone(),
        roles: vec!["user".to_string()],
        email: Some(form.email),
        full_name: Some(form.full_name),
        last_login: Some(chrono::Utc::now()),
        permissions: vec![Permission::Read, Permission::Write],
    };

    // Generate JWT token
    let token = state
        .jwt_manager
        .generate_token(&user)
        .map_err(|_| AuthHandlerError::TokenGenerationFailed)?;

    // Set cookie and redirect to dashboard
    let cookie = format!(
        "auth_token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=28800",
        token
    );

    Ok((
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, cookie),
            (header::LOCATION, "/".to_string()),
        ],
    )
        .into_response())
}

/// Authentication handler errors
#[derive(Debug, thiserror::Error)]
pub enum AuthHandlerError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Invalid input")]
    InvalidInput,
    #[error("Weak password")]
    WeakPassword,
    #[error("Token generation failed")]
    TokenGenerationFailed,
    #[error("Password hashing failed")]
    HashingFailed,
}

impl IntoResponse for AuthHandlerError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthHandlerError::InvalidCredentials => {
                (StatusCode::UNAUTHORIZED, "Invalid username or password")
            }
            AuthHandlerError::InvalidInput => (StatusCode::BAD_REQUEST, "Invalid input"),
            AuthHandlerError::WeakPassword => (
                StatusCode::BAD_REQUEST,
                "Password is too weak. Use a stronger password.",
            ),
            AuthHandlerError::TokenGenerationFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to generate token",
            ),
            AuthHandlerError::HashingFailed => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password")
            }
        };

        (status, message).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_form_deserialization() {
        let form = LoginForm {
            username: "alice".to_string(),
            password: "secret123".to_string(),
            remember_me: Some(true),
        };

        assert_eq!(form.username, "alice");
        assert_eq!(form.password, "secret123");
        assert_eq!(form.remember_me, Some(true));
    }
}
