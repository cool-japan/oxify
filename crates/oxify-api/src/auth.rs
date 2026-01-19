//! Authentication module
//!
//! JWT-based authentication with middleware and endpoints

use crate::user_types::ApiUser;
use axum::{
    http::{header::AUTHORIZATION, StatusCode},
    Json,
};
use oxify_authn::{JwtConfig, JwtManager, PasswordManager};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// JWT claims extracted from validated token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,   // User ID
    pub email: String, // User email
    pub roles: Vec<String>,
    pub exp: i64, // Expiration timestamp
}

/// Authenticated user from JWT token
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    #[allow(dead_code)]
    pub email: String,
    #[allow(dead_code)]
    pub roles: Vec<String>,
}

/// Login request
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Register request
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    #[allow(dead_code)]
    pub name: Option<String>,
}

/// Authentication response
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserInfo,
}

/// User information
#[derive(Debug, Serialize, ToSchema)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub roles: Vec<String>,
}

/// Authentication state
#[derive(Clone)]
pub struct AuthState {
    pub jwt_manager: Arc<JwtManager>,
    pub password_manager: Arc<PasswordManager>,
}

impl AuthState {
    /// Create new authentication state
    pub fn new() -> Self {
        let jwt_config = JwtConfig::new(
            std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string()),
            "oxify-api",
            "oxify",
            3600 * 24, // 24 hours
        );

        Self {
            jwt_manager: Arc::new(
                JwtManager::new(&jwt_config).expect("Failed to create JwtManager"),
            ),
            password_manager: Arc::new(PasswordManager::new()),
        }
    }

    /// Generate JWT token for user
    pub fn generate_token(&self, user: &ApiUser) -> Result<String, String> {
        // Create a modified authn user with user ID as username (for sub claim)
        let authn_user = oxify_authn::User {
            username: user.id.to_string(), // Use ID as subject
            roles: user.roles.clone(),
            email: Some(user.email.clone()),
            full_name: user.full_name.clone(),
            last_login: Some(chrono::Utc::now()),
            permissions: user.permissions.clone(),
        };

        self.jwt_manager
            .generate_token(&authn_user)
            .map_err(|e| e.to_string())
    }

    /// Verify JWT token and extract claims
    pub fn verify_token(&self, token: &str) -> Result<Claims, String> {
        let validation = self
            .jwt_manager
            .validate_token(token)
            .map_err(|e| e.to_string())?;

        Ok(Claims {
            sub: validation.user.username.clone(), // This is the user ID as string
            email: validation.user.email.unwrap_or_default(),
            roles: validation.user.roles,
            exp: validation.expires_at.timestamp(),
        })
    }

    /// Hash password
    pub fn hash_password(&self, password: &str) -> Result<String, String> {
        self.password_manager
            .hash_password(password)
            .map_err(|e| e.to_string())
    }

    /// Verify password against hash
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool, String> {
        self.password_manager
            .verify_password(password, hash)
            .map_err(|e| e.to_string())
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthUser {
    /// Extract and verify JWT token from request headers
    pub fn from_headers(
        headers: &axum::http::HeaderMap,
        auth_state: &AuthState,
    ) -> Result<Self, (StatusCode, Json<crate::types::ErrorResponse>)> {
        // Extract Authorization header
        let auth_header = headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(crate::types::ErrorResponse {
                        error: "Unauthorized".to_string(),
                        message: "Missing Authorization header".to_string(),
                    }),
                )
            })?;

        // Extract token from "Bearer <token>"
        let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(crate::types::ErrorResponse {
                    error: "Unauthorized".to_string(),
                    message: "Invalid Authorization header format".to_string(),
                }),
            )
        })?;

        // Verify token
        let claims = auth_state.verify_token(token).map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(crate::types::ErrorResponse {
                    error: "Unauthorized".to_string(),
                    message: format!("Invalid token: {}", e),
                }),
            )
        })?;

        // Parse user ID
        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(crate::types::ErrorResponse {
                    error: "Unauthorized".to_string(),
                    message: "Invalid user ID in token".to_string(),
                }),
            )
        })?;

        Ok(AuthUser {
            user_id,
            email: claims.email,
            roles: claims.roles,
        })
    }
}
