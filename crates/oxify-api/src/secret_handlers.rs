//! API handlers for secrets management

use crate::handlers::AppState;
use crate::secret_types::*;
use crate::types::ErrorResponse;
use crate::user_types::ApiUser;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

/// Query parameters for listing audit logs
#[derive(Debug, Deserialize)]
pub struct AuditLogsQuery {
    /// Maximum number of logs to return (default: 100)
    #[serde(default = "default_audit_limit")]
    pub limit: i64,
}

fn default_audit_limit() -> i64 {
    100
}

/// Create a new secret
#[utoipa::path(
    post,
    path = "/api/v1/secrets",
    request_body = CreateSecretRequest,
    responses(
        (status = 201, description = "Secret created successfully", body = CreateSecretResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 503, description = "Secret management not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_secret(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<ApiUser>,
    Json(req): Json<CreateSecretRequest>,
) -> Result<(StatusCode, Json<CreateSecretResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Creating secret: {} for user: {}", req.name, user.id);

    let secret_store = state.secret_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message:
                    "Secret management is not enabled. Set ENCRYPTION_KEY environment variable."
                        .to_string(),
            }),
        )
    })?;

    // Validate input
    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Secret name cannot be empty".to_string(),
            }),
        ));
    }

    if req.value.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Secret value cannot be empty".to_string(),
            }),
        ));
    }

    let secret = secret_store
        .create(req.name.clone(), req.value, user.id)
        .await
        .map_err(|e| {
            error!("Failed to create secret: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to create secret: {}", e),
                }),
            )
        })?;

    info!("Secret created successfully: {}", secret.id);
    Ok((
        StatusCode::CREATED,
        Json(CreateSecretResponse {
            id: secret.id,
            message: "Secret created successfully".to_string(),
        }),
    ))
}

/// Get a secret by ID (returns decrypted value)
#[utoipa::path(
    get,
    path = "/api/v1/secrets/{id}",
    params(
        ("id" = Uuid, Path, description = "Secret ID")
    ),
    responses(
        (status = 200, description = "Secret found", body = GetSecretResponse),
        (status = 404, description = "Secret not found", body = ErrorResponse),
        (status = 503, description = "Secret management not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_secret(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<GetSecretResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting secret: {} for user: {}", id, user.id);

    let secret_store = state.secret_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Secret management is not enabled".to_string(),
            }),
        )
    })?;

    match secret_store.get(&id, &user.id).await {
        Ok(Some((secret, value))) => {
            let secret_view = oxify_model::SecretView {
                id: secret.id,
                name: secret.name,
                description: secret.description,
                tags: secret.tags,
                owner_id: secret.owner_id,
                created_at: secret.created_at,
                updated_at: secret.updated_at,
                last_accessed_at: secret.last_accessed_at,
                expires_at: secret.expires_at,
                is_expired: secret
                    .expires_at
                    .map(|exp| chrono::Utc::now() > exp)
                    .unwrap_or(false),
            };

            Ok(Json(GetSecretResponse {
                secret: secret_view,
                value,
            }))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Secret {} not found", id),
            }),
        )),
        Err(e) => {
            error!("Failed to get secret: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get secret: {}", e),
                }),
            ))
        }
    }
}

/// List all secrets for the current user (without decrypted values)
#[utoipa::path(
    get,
    path = "/api/v1/secrets",
    responses(
        (status = 200, description = "List of secrets", body = ListSecretsResponse),
        (status = 503, description = "Secret management not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_secrets(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<ApiUser>,
) -> Result<Json<ListSecretsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing secrets for user: {}", user.id);

    let secret_store = state.secret_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Secret management is not enabled".to_string(),
            }),
        )
    })?;

    let secrets = secret_store.list_by_owner(&user.id).await.map_err(|e| {
        error!("Failed to list secrets: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to list secrets: {}", e),
            }),
        )
    })?;

    let total = secrets.len();

    Ok(Json(ListSecretsResponse { secrets, total }))
}

/// Update a secret's value
#[utoipa::path(
    put,
    path = "/api/v1/secrets/{id}",
    params(
        ("id" = Uuid, Path, description = "Secret ID")
    ),
    request_body = UpdateSecretRequest,
    responses(
        (status = 200, description = "Secret updated successfully", body = UpdateSecretResponse),
        (status = 404, description = "Secret not found", body = ErrorResponse),
        (status = 503, description = "Secret management not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_secret(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSecretRequest>,
) -> Result<Json<UpdateSecretResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Updating secret: {} for user: {}", id, user.id);

    let secret_store = state.secret_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Secret management is not enabled".to_string(),
            }),
        )
    })?;

    if req.value.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ValidationError".to_string(),
                message: "Secret value cannot be empty".to_string(),
            }),
        ));
    }

    let updated = secret_store
        .update(&id, req.value, &user.id)
        .await
        .map_err(|e| {
            error!("Failed to update secret: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to update secret: {}", e),
                }),
            )
        })?;

    if updated {
        Ok(Json(UpdateSecretResponse {
            message: "Secret updated successfully".to_string(),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Secret {} not found", id),
            }),
        ))
    }
}

/// Delete a secret
#[utoipa::path(
    delete,
    path = "/api/v1/secrets/{id}",
    params(
        ("id" = Uuid, Path, description = "Secret ID")
    ),
    responses(
        (status = 200, description = "Secret deleted successfully", body = DeleteSecretResponse),
        (status = 404, description = "Secret not found", body = ErrorResponse),
        (status = 503, description = "Secret management not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_secret(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteSecretResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Deleting secret: {} for user: {}", id, user.id);

    let secret_store = state.secret_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Secret management is not enabled".to_string(),
            }),
        )
    })?;

    let deleted = secret_store.delete(&id, &user.id).await.map_err(|e| {
        error!("Failed to delete secret: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to delete secret: {}", e),
            }),
        )
    })?;

    if deleted {
        Ok(Json(DeleteSecretResponse {
            message: "Secret deleted successfully".to_string(),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Secret {} not found", id),
            }),
        ))
    }
}

/// Get audit logs for a secret
#[utoipa::path(
    get,
    path = "/api/v1/secrets/{id}/audit",
    params(
        ("id" = Uuid, Path, description = "Secret ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of logs to return (default: 100)")
    ),
    responses(
        (status = 200, description = "Audit logs retrieved", body = GetSecretAuditLogsResponse),
        (status = 503, description = "Secret management not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_secret_audit_logs(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
    Query(query): Query<AuditLogsQuery>,
) -> Result<Json<GetSecretAuditLogsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        "Getting audit logs for secret: {} for user: {}",
        id, user.id
    );

    let secret_store = state.secret_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Secret management is not enabled".to_string(),
            }),
        )
    })?;

    let logs = secret_store
        .get_audit_logs(&id, query.limit)
        .await
        .map_err(|e| {
            error!("Failed to get audit logs: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get audit logs: {}", e),
                }),
            )
        })?;

    let total = logs.len();

    Ok(Json(GetSecretAuditLogsResponse { logs, total }))
}
