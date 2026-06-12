//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::types::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use super::template_vector_handlers::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct ApprovalActionRequest {
    pub comments: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ApprovalActionResponse {
    pub success: bool,
    pub message: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ApprovalRequestView {
    pub id: Uuid,
    pub execution_id: String,
    pub node_id: Uuid,
    pub message: String,
    pub description: Option<String>,
    pub approvers: Vec<String>,
    pub timeout_seconds: Option<u64>,
    pub context_data: serde_json::Value,
    pub status: String,
    pub requested_at: std::time::SystemTime,
    pub resolved_at: Option<std::time::SystemTime>,
    pub resolved_by: Option<String>,
    pub comments: Option<String>,
}
/// Get webhook statistics
#[derive(serde::Serialize)]
pub struct WebhookStats {
    pub webhook_id: uuid::Uuid,
    pub total_events: u64,
    pub successful_events: u64,
    pub failed_events: u64,
    pub pending_events: u64,
    pub success_rate: f64,
    pub avg_processing_time_ms: Option<f64>,
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
}
/// Create a new webhook (DISABLED)
#[allow(unused_variables)]
pub async fn create_webhook(
    State(_state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
    Json(_request): Json<oxify_model::CreateWebhookRequest>,
) -> Result<
    (StatusCode, Json<oxify_model::WebhookRegistrationResponse>),
    (StatusCode, Json<ErrorResponse>),
> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}
/// List all webhooks for the authenticated user (DISABLED)
#[allow(unused_variables)]
pub async fn list_webhooks(
    State(_state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
) -> Result<Json<Vec<oxify_model::WebhookView>>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}
/// Get webhook by ID (DISABLED)
#[allow(unused_variables)]
pub async fn get_webhook(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<uuid::Uuid>,
) -> Result<Json<oxify_model::WebhookView>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}
/// Update webhook configuration (DISABLED)
#[allow(unused_variables)]
pub async fn update_webhook(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<uuid::Uuid>,
    Json(_request): Json<oxify_model::UpdateWebhookRequest>,
) -> Result<Json<oxify_model::WebhookView>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}
/// Delete webhook (DISABLED)
#[allow(unused_variables)]
pub async fn delete_webhook(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}
/// Receive webhook event (public endpoint) (DISABLED)
#[allow(unused_variables)]
pub async fn receive_webhook_event(
    State(_state): State<Arc<AppState>>,
    Path(_webhook_id): Path<uuid::Uuid>,
    _headers: axum::http::HeaderMap,
    _body: String,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}
/// List webhook events (DISABLED)
#[allow(unused_variables)]
pub async fn list_webhook_events(
    State(_state): State<Arc<AppState>>,
    Path(_webhook_id): Path<uuid::Uuid>,
) -> Result<Json<Vec<oxify_model::WebhookEvent>>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}
/// Get webhook statistics (DISABLED)
#[allow(unused_variables)]
pub async fn get_webhook_stats(
    State(_state): State<Arc<AppState>>,
    Path(_webhook_id): Path<uuid::Uuid>,
) -> Result<Json<WebhookStats>, (StatusCode, Json<ErrorResponse>)> {
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "ServiceUnavailable".to_string(),
            message: "Webhook functionality is disabled (SQLite migration)".to_string(),
        }),
    ))
}
/// List all pending approval requests
pub async fn list_pending_approvals(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
) -> Result<Json<Vec<ApprovalRequestView>>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing all pending approvals");
    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;
    let approvals = approval_store.list_pending();
    let views: Vec<ApprovalRequestView> = approvals
        .into_iter()
        .map(|a| ApprovalRequestView {
            id: a.id,
            execution_id: a.execution_id,
            node_id: a.node_id,
            message: a.config.message,
            description: a.config.description,
            approvers: a.config.approvers,
            timeout_seconds: a.config.timeout_seconds,
            context_data: a.config.context_data,
            status: format!("{:?}", a.status),
            requested_at: a.requested_at,
            resolved_at: a.resolved_at,
            resolved_by: a.resolved_by,
            comments: a.comments,
        })
        .collect();
    Ok(Json(views))
}
/// List pending approvals for a specific execution
pub async fn list_execution_approvals(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
    Path(execution_id): Path<String>,
) -> Result<Json<Vec<ApprovalRequestView>>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing approvals for execution: {}", execution_id);
    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;
    let approvals = approval_store.list_pending_for_execution(&execution_id);
    let views: Vec<ApprovalRequestView> = approvals
        .into_iter()
        .map(|a| ApprovalRequestView {
            id: a.id,
            execution_id: a.execution_id,
            node_id: a.node_id,
            message: a.config.message,
            description: a.config.description,
            approvers: a.config.approvers,
            timeout_seconds: a.config.timeout_seconds,
            context_data: a.config.context_data,
            status: format!("{:?}", a.status),
            requested_at: a.requested_at,
            resolved_at: a.resolved_at,
            resolved_by: a.resolved_by,
            comments: a.comments,
        })
        .collect();
    Ok(Json(views))
}
/// Get approval request details
pub async fn get_approval(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<crate::user_types::ApiUser>,
    Path(approval_id): Path<Uuid>,
) -> Result<Json<ApprovalRequestView>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting approval: {}", approval_id);
    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;
    let approval = approval_store.get(approval_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Approval {} not found", approval_id),
            }),
        )
    })?;
    Ok(Json(ApprovalRequestView {
        id: approval.id,
        execution_id: approval.execution_id,
        node_id: approval.node_id,
        message: approval.config.message,
        description: approval.config.description,
        approvers: approval.config.approvers,
        timeout_seconds: approval.config.timeout_seconds,
        context_data: approval.config.context_data,
        status: format!("{:?}", approval.status),
        requested_at: approval.requested_at,
        resolved_at: approval.resolved_at,
        resolved_by: approval.resolved_by,
        comments: approval.comments,
    }))
}
/// Approve an approval request
pub async fn approve_approval(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<crate::user_types::ApiUser>,
    Path(approval_id): Path<Uuid>,
    Json(request): Json<ApprovalActionRequest>,
) -> Result<Json<ApprovalActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Approving approval: {} by user: {}", approval_id, user.id);
    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;
    let success = approval_store.approve(approval_id, user.id.to_string(), request.comments);
    if success {
        Ok(Json(ApprovalActionResponse {
            success: true,
            message: "Approval approved successfully".to_string(),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Approval {} not found", approval_id),
            }),
        ))
    }
}
/// Reject an approval request
pub async fn reject_approval(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<crate::user_types::ApiUser>,
    Path(approval_id): Path<Uuid>,
    Json(request): Json<ApprovalActionRequest>,
) -> Result<Json<ApprovalActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Rejecting approval: {} by user: {}", approval_id, user.id);
    let approval_store = state.approval_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Approval functionality is not enabled".to_string(),
            }),
        )
    })?;
    let success = approval_store.reject(approval_id, user.id.to_string(), request.comments);
    if success {
        Ok(Json(ApprovalActionResponse {
            success: true,
            message: "Approval rejected successfully".to_string(),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: format!("Approval {} not found", approval_id),
            }),
        ))
    }
}
