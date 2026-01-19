//! Webhook storage implementation

use crate::{DatabasePool, Result};
use chrono::{DateTime, Utc};
use oxify_model::{Webhook, WebhookEvent, WebhookEventStatus, WebhookId};
use uuid::Uuid;

/// Webhook storage layer
#[derive(Clone)]
pub struct WebhookStore {
    pool: DatabasePool,
}

impl WebhookStore {
    /// Create a new webhook store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Create a new webhook
    pub async fn create(&self, webhook: &Webhook) -> Result<WebhookId> {
        let required_headers = serde_json::to_value(&webhook.required_headers)?;

        sqlx::query(
            r"
            INSERT INTO webhooks
            (id, name, description, workflow_id, secret, enabled, event_types,
             required_headers, ip_whitelist, max_body_size, timeout_seconds,
             owner_id, created_at, updated_at, last_triggered_at, trigger_count, failed_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            ",
        )
        .bind(webhook.id)
        .bind(&webhook.name)
        .bind(&webhook.description)
        .bind(webhook.workflow_id)
        .bind(&webhook.secret)
        .bind(webhook.enabled)
        .bind(&webhook.event_types)
        .bind(required_headers)
        .bind(&webhook.ip_whitelist)
        .bind(webhook.max_body_size as i32)
        .bind(webhook.timeout_seconds as i32)
        .bind(webhook.owner_id)
        .bind(webhook.created_at)
        .bind(webhook.updated_at)
        .bind(webhook.last_triggered_at)
        .bind(webhook.trigger_count as i64)
        .bind(webhook.failed_count as i64)
        .execute(self.pool.pool())
        .await?;

        Ok(webhook.id)
    }

    /// Get a webhook by ID
    pub async fn get(&self, id: &WebhookId) -> Result<Option<Webhook>> {
        #[derive(sqlx::FromRow)]
        struct WebhookRow {
            id: Uuid,
            name: String,
            description: Option<String>,
            workflow_id: Uuid,
            secret: String,
            enabled: bool,
            event_types: Vec<String>,
            required_headers: serde_json::Value,
            ip_whitelist: Vec<String>,
            max_body_size: i32,
            timeout_seconds: i32,
            owner_id: Uuid,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            last_triggered_at: Option<DateTime<Utc>>,
            trigger_count: i64,
            failed_count: i64,
        }

        let row = sqlx::query_as::<_, WebhookRow>(
            r"
            SELECT id, name, description, workflow_id, secret, enabled, event_types,
                   required_headers, ip_whitelist, max_body_size, timeout_seconds,
                   owner_id, created_at, updated_at, last_triggered_at, trigger_count, failed_count
            FROM webhooks
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => {
                let required_headers = serde_json::from_value(row.required_headers)?;

                Ok(Some(Webhook {
                    id: row.id,
                    name: row.name,
                    description: row.description,
                    workflow_id: row.workflow_id,
                    secret: row.secret,
                    enabled: row.enabled,
                    event_types: row.event_types,
                    required_headers,
                    ip_whitelist: row.ip_whitelist,
                    max_body_size: row.max_body_size as usize,
                    timeout_seconds: row.timeout_seconds as u32,
                    owner_id: row.owner_id,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    last_triggered_at: row.last_triggered_at,
                    trigger_count: row.trigger_count as u64,
                    failed_count: row.failed_count as u64,
                }))
            }
            None => Ok(None),
        }
    }

    /// List all webhooks for an owner
    pub async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<Webhook>> {
        #[derive(sqlx::FromRow)]
        struct WebhookRow {
            id: Uuid,
            name: String,
            description: Option<String>,
            workflow_id: Uuid,
            secret: String,
            enabled: bool,
            event_types: Vec<String>,
            required_headers: serde_json::Value,
            ip_whitelist: Vec<String>,
            max_body_size: i32,
            timeout_seconds: i32,
            owner_id: Uuid,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            last_triggered_at: Option<DateTime<Utc>>,
            trigger_count: i64,
            failed_count: i64,
        }

        let rows = sqlx::query_as::<_, WebhookRow>(
            r"
            SELECT id, name, description, workflow_id, secret, enabled, event_types,
                   required_headers, ip_whitelist, max_body_size, timeout_seconds,
                   owner_id, created_at, updated_at, last_triggered_at, trigger_count, failed_count
            FROM webhooks
            WHERE owner_id = $1
            ORDER BY created_at DESC
            ",
        )
        .bind(owner_id)
        .fetch_all(self.pool.pool())
        .await?;

        let webhooks = rows
            .into_iter()
            .filter_map(|row| {
                let required_headers = serde_json::from_value(row.required_headers).ok()?;

                Some(Webhook {
                    id: row.id,
                    name: row.name,
                    description: row.description,
                    workflow_id: row.workflow_id,
                    secret: row.secret,
                    enabled: row.enabled,
                    event_types: row.event_types,
                    required_headers,
                    ip_whitelist: row.ip_whitelist,
                    max_body_size: row.max_body_size as usize,
                    timeout_seconds: row.timeout_seconds as u32,
                    owner_id: row.owner_id,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    last_triggered_at: row.last_triggered_at,
                    trigger_count: row.trigger_count as u64,
                    failed_count: row.failed_count as u64,
                })
            })
            .collect();

        Ok(webhooks)
    }

    /// Update a webhook
    pub async fn update(&self, webhook: &Webhook) -> Result<()> {
        let required_headers = serde_json::to_value(&webhook.required_headers)?;

        sqlx::query(
            r"
            UPDATE webhooks
            SET name = $2, description = $3, enabled = $4, event_types = $5,
                required_headers = $6, ip_whitelist = $7, max_body_size = $8,
                timeout_seconds = $9, updated_at = $10, last_triggered_at = $11,
                trigger_count = $12, failed_count = $13
            WHERE id = $1
            ",
        )
        .bind(webhook.id)
        .bind(&webhook.name)
        .bind(&webhook.description)
        .bind(webhook.enabled)
        .bind(&webhook.event_types)
        .bind(required_headers)
        .bind(&webhook.ip_whitelist)
        .bind(webhook.max_body_size as i32)
        .bind(webhook.timeout_seconds as i32)
        .bind(webhook.updated_at)
        .bind(webhook.last_triggered_at)
        .bind(webhook.trigger_count as i64)
        .bind(webhook.failed_count as i64)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Delete a webhook
    pub async fn delete(&self, id: &WebhookId) -> Result<bool> {
        let result = sqlx::query(
            r"
            DELETE FROM webhooks
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Record a webhook event
    pub async fn record_event(&self, event: &WebhookEvent) -> Result<Uuid> {
        let headers = serde_json::to_value(&event.headers)?;

        sqlx::query(
            r"
            INSERT INTO webhook_events
            (id, webhook_id, event_type, payload, headers, source_ip,
             received_at, processed_at, status, execution_id, error_message)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ",
        )
        .bind(event.id)
        .bind(event.webhook_id)
        .bind(&event.event_type)
        .bind(&event.payload)
        .bind(headers)
        .bind(&event.source_ip)
        .bind(event.received_at)
        .bind(event.processed_at)
        .bind(event.status.to_string())
        .bind(event.execution_id)
        .bind(&event.error_message)
        .execute(self.pool.pool())
        .await?;

        Ok(event.id)
    }

    /// List events for a webhook
    pub async fn list_events(
        &self,
        webhook_id: &WebhookId,
        limit: i64,
    ) -> Result<Vec<WebhookEvent>> {
        #[derive(sqlx::FromRow)]
        struct WebhookEventRow {
            id: Uuid,
            webhook_id: Uuid,
            event_type: String,
            payload: serde_json::Value,
            headers: serde_json::Value,
            source_ip: String,
            received_at: DateTime<Utc>,
            processed_at: Option<DateTime<Utc>>,
            status: String,
            execution_id: Option<Uuid>,
            error_message: Option<String>,
        }

        let rows = sqlx::query_as::<_, WebhookEventRow>(
            r"
            SELECT id, webhook_id, event_type, payload, headers, source_ip,
                   received_at, processed_at, status, execution_id, error_message
            FROM webhook_events
            WHERE webhook_id = $1
            ORDER BY received_at DESC
            LIMIT $2
            ",
        )
        .bind(webhook_id)
        .bind(limit)
        .fetch_all(self.pool.pool())
        .await?;

        let events = rows
            .into_iter()
            .filter_map(|row| {
                let headers = serde_json::from_value(row.headers).ok()?;
                let status = match row.status.as_str() {
                    "PENDING" => WebhookEventStatus::Pending,
                    "PROCESSING" => WebhookEventStatus::Processing,
                    "COMPLETED" => WebhookEventStatus::Completed,
                    "FAILED" => WebhookEventStatus::Failed,
                    "REJECTED" => WebhookEventStatus::Rejected,
                    _ => return None,
                };

                Some(WebhookEvent {
                    id: row.id,
                    webhook_id: row.webhook_id,
                    event_type: row.event_type,
                    payload: row.payload,
                    headers,
                    source_ip: row.source_ip,
                    received_at: row.received_at,
                    processed_at: row.processed_at,
                    status,
                    execution_id: row.execution_id,
                    error_message: row.error_message,
                })
            })
            .collect();

        Ok(events)
    }

    /// Get webhook statistics
    pub async fn get_stats(&self, webhook_id: &WebhookId) -> Result<Option<WebhookStats>> {
        #[derive(sqlx::FromRow)]
        struct StatsRow {
            total_events: i64,
            successful_events: i64,
            failed_events: i64,
            pending_events: i64,
            avg_processing_time_ms: Option<f64>,
            last_event_at: Option<DateTime<Utc>>,
        }

        let row = sqlx::query_as::<_, StatsRow>(
            r"
            SELECT
                COUNT(*) as total_events,
                SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) as successful_events,
                SUM(CASE WHEN status = 'FAILED' OR status = 'REJECTED' THEN 1 ELSE 0 END) as failed_events,
                SUM(CASE WHEN status = 'PENDING' OR status = 'PROCESSING' THEN 1 ELSE 0 END) as pending_events,
                AVG(EXTRACT(EPOCH FROM (processed_at - received_at)) * 1000) FILTER (WHERE processed_at IS NOT NULL) as avg_processing_time_ms,
                MAX(received_at) as last_event_at
            FROM webhook_events
            WHERE webhook_id = $1
            ",
        )
        .bind(webhook_id)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(WebhookStats {
                webhook_id: *webhook_id,
                total_events: row.total_events as u64,
                successful_events: row.successful_events as u64,
                failed_events: row.failed_events as u64,
                pending_events: row.pending_events as u64,
                avg_processing_time_ms: row.avg_processing_time_ms,
                last_event_at: row.last_event_at,
            })),
            None => Ok(None),
        }
    }
}

/// Webhook statistics
#[derive(Debug, Clone)]
pub struct WebhookStats {
    pub webhook_id: WebhookId,
    pub total_events: u64,
    pub successful_events: u64,
    pub failed_events: u64,
    pub pending_events: u64,
    pub avg_processing_time_ms: Option<f64>,
    pub last_event_at: Option<DateTime<Utc>>,
}
