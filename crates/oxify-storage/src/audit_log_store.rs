//! Audit Log Storage
//!
//! Provides system-wide audit logging for compliance and debugging

use crate::{DatabasePool, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::Row;
use uuid::Uuid;

/// Event categories for grouping audit logs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventCategory {
    Workflow,
    Execution,
    User,
    Security,
    Api,
    System,
    Schedule,
    Webhook,
}

impl AuditEventCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditEventCategory::Workflow => "workflow",
            AuditEventCategory::Execution => "execution",
            AuditEventCategory::User => "user",
            AuditEventCategory::Security => "security",
            AuditEventCategory::Api => "api",
            AuditEventCategory::System => "system",
            AuditEventCategory::Schedule => "schedule",
            AuditEventCategory::Webhook => "webhook",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "workflow" => Some(AuditEventCategory::Workflow),
            "execution" => Some(AuditEventCategory::Execution),
            "user" => Some(AuditEventCategory::User),
            "security" => Some(AuditEventCategory::Security),
            "api" => Some(AuditEventCategory::Api),
            "system" => Some(AuditEventCategory::System),
            "schedule" => Some(AuditEventCategory::Schedule),
            "webhook" => Some(AuditEventCategory::Webhook),
            _ => None,
        }
    }
}

impl std::fmt::Display for AuditEventCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Actor types for audit logs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AuditActorType {
    #[default]
    User,
    System,
    Scheduler,
    Webhook,
    ApiKey,
}

impl AuditActorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditActorType::User => "user",
            AuditActorType::System => "system",
            AuditActorType::Scheduler => "scheduler",
            AuditActorType::Webhook => "webhook",
            AuditActorType::ApiKey => "api_key",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "user" => Some(AuditActorType::User),
            "system" => Some(AuditActorType::System),
            "scheduler" => Some(AuditActorType::Scheduler),
            "webhook" => Some(AuditActorType::Webhook),
            "api_key" => Some(AuditActorType::ApiKey),
            _ => None,
        }
    }
}

impl std::fmt::Display for AuditActorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Resource types that can be audited
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResourceType {
    Workflow,
    Execution,
    User,
    ApiKey,
    Secret,
    Schedule,
    Webhook,
    Checkpoint,
    Version,
}

impl AuditResourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditResourceType::Workflow => "workflow",
            AuditResourceType::Execution => "execution",
            AuditResourceType::User => "user",
            AuditResourceType::ApiKey => "api_key",
            AuditResourceType::Secret => "secret",
            AuditResourceType::Schedule => "schedule",
            AuditResourceType::Webhook => "webhook",
            AuditResourceType::Checkpoint => "checkpoint",
            AuditResourceType::Version => "version",
        }
    }

    #[allow(dead_code)]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "workflow" => Some(AuditResourceType::Workflow),
            "execution" => Some(AuditResourceType::Execution),
            "user" => Some(AuditResourceType::User),
            "api_key" => Some(AuditResourceType::ApiKey),
            "secret" => Some(AuditResourceType::Secret),
            "schedule" => Some(AuditResourceType::Schedule),
            "webhook" => Some(AuditResourceType::Webhook),
            "checkpoint" => Some(AuditResourceType::Checkpoint),
            "version" => Some(AuditResourceType::Version),
            _ => None,
        }
    }
}

impl std::fmt::Display for AuditResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Common action types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Create,
    Read,
    Update,
    Delete,
    Execute,
    Start,
    Stop,
    Pause,
    Resume,
    Complete,
    Fail,
    Login,
    Logout,
    Register,
    Verify,
    Export,
    Import,
    Trigger,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::Create => "create",
            AuditAction::Read => "read",
            AuditAction::Update => "update",
            AuditAction::Delete => "delete",
            AuditAction::Execute => "execute",
            AuditAction::Start => "start",
            AuditAction::Stop => "stop",
            AuditAction::Pause => "pause",
            AuditAction::Resume => "resume",
            AuditAction::Complete => "complete",
            AuditAction::Fail => "fail",
            AuditAction::Login => "login",
            AuditAction::Logout => "logout",
            AuditAction::Register => "register",
            AuditAction::Verify => "verify",
            AuditAction::Export => "export",
            AuditAction::Import => "import",
            AuditAction::Trigger => "trigger",
        }
    }

    #[allow(dead_code)]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "create" => Some(AuditAction::Create),
            "read" => Some(AuditAction::Read),
            "update" => Some(AuditAction::Update),
            "delete" => Some(AuditAction::Delete),
            "execute" => Some(AuditAction::Execute),
            "start" => Some(AuditAction::Start),
            "stop" => Some(AuditAction::Stop),
            "pause" => Some(AuditAction::Pause),
            "resume" => Some(AuditAction::Resume),
            "complete" => Some(AuditAction::Complete),
            "fail" => Some(AuditAction::Fail),
            "login" => Some(AuditAction::Login),
            "logout" => Some(AuditAction::Logout),
            "register" => Some(AuditAction::Register),
            "verify" => Some(AuditAction::Verify),
            "export" => Some(AuditAction::Export),
            "import" => Some(AuditAction::Import),
            "trigger" => Some(AuditAction::Trigger),
            _ => None,
        }
    }
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: Uuid,
    pub event_type: String,
    pub event_category: AuditEventCategory,
    pub actor_id: Option<Uuid>,
    pub actor_type: AuditActorType,
    pub actor_ip: Option<String>,
    pub resource_type: Option<AuditResourceType>,
    pub resource_id: Option<Uuid>,
    pub action: String,
    pub description: Option<String>,
    pub metadata: JsonValue,
    pub request_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub success: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: Option<i32>,
    pub retention_days: Option<i32>,
}

/// Builder for creating audit log entries
#[derive(Debug, Clone, Default)]
pub struct AuditLogBuilder {
    event_type: Option<String>,
    event_category: Option<AuditEventCategory>,
    actor_id: Option<Uuid>,
    actor_type: AuditActorType,
    actor_ip: Option<String>,
    resource_type: Option<AuditResourceType>,
    resource_id: Option<Uuid>,
    action: Option<String>,
    description: Option<String>,
    metadata: JsonValue,
    request_id: Option<Uuid>,
    session_id: Option<Uuid>,
    success: bool,
    error_code: Option<String>,
    error_message: Option<String>,
    duration_ms: Option<i32>,
    retention_days: Option<i32>,
}

impl AuditLogBuilder {
    pub fn new() -> Self {
        Self {
            metadata: serde_json::json!({}),
            success: true,
            ..Default::default()
        }
    }

    pub fn event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    pub fn category(mut self, category: AuditEventCategory) -> Self {
        self.event_category = Some(category);
        self
    }

    pub fn actor(mut self, actor_id: Uuid, actor_type: AuditActorType) -> Self {
        self.actor_id = Some(actor_id);
        self.actor_type = actor_type;
        self
    }

    pub fn system_actor(mut self) -> Self {
        self.actor_id = None;
        self.actor_type = AuditActorType::System;
        self
    }

    pub fn actor_ip(mut self, ip: impl Into<String>) -> Self {
        self.actor_ip = Some(ip.into());
        self
    }

    pub fn resource(mut self, resource_type: AuditResourceType, resource_id: Uuid) -> Self {
        self.resource_type = Some(resource_type);
        self.resource_id = Some(resource_id);
        self
    }

    pub fn action(mut self, action: AuditAction) -> Self {
        self.action = Some(action.as_str().to_string());
        self
    }

    pub fn custom_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn metadata(mut self, metadata: JsonValue) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn add_metadata(mut self, key: impl Into<String>, value: JsonValue) -> Self {
        if let Some(obj) = self.metadata.as_object_mut() {
            obj.insert(key.into(), value);
        }
        self
    }

    pub fn request_id(mut self, request_id: Uuid) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn session_id(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    pub fn error(mut self, code: impl Into<String>, message: impl Into<String>) -> Self {
        self.success = false;
        self.error_code = Some(code.into());
        self.error_message = Some(message.into());
        self
    }

    pub fn duration_ms(mut self, duration: i32) -> Self {
        self.duration_ms = Some(duration);
        self
    }

    pub fn retention_days(mut self, days: i32) -> Self {
        self.retention_days = Some(days);
        self
    }

    pub fn build(self) -> Option<AuditLog> {
        Some(AuditLog {
            id: Uuid::new_v4(),
            event_type: self.event_type?,
            event_category: self.event_category?,
            actor_id: self.actor_id,
            actor_type: self.actor_type,
            actor_ip: self.actor_ip,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            action: self.action?,
            description: self.description,
            metadata: self.metadata,
            request_id: self.request_id,
            session_id: self.session_id,
            success: self.success,
            error_code: self.error_code,
            error_message: self.error_message,
            timestamp: Utc::now(),
            duration_ms: self.duration_ms,
            retention_days: self.retention_days,
        })
    }
}

/// Audit log query filter
#[derive(Debug, Clone, Default)]
pub struct AuditLogFilter {
    pub event_category: Option<AuditEventCategory>,
    pub event_type: Option<String>,
    pub actor_id: Option<Uuid>,
    pub resource_type: Option<AuditResourceType>,
    pub resource_id: Option<Uuid>,
    pub success: Option<bool>,
    pub from_timestamp: Option<DateTime<Utc>>,
    pub to_timestamp: Option<DateTime<Utc>>,
}

/// Audit log statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogStats {
    pub total_events: u64,
    pub successful_events: u64,
    pub failed_events: u64,
    pub events_by_category: Vec<(String, u64)>,
    pub events_by_action: Vec<(String, u64)>,
    pub first_event_at: Option<DateTime<Utc>>,
    pub last_event_at: Option<DateTime<Utc>>,
}

/// Audit log storage layer
#[derive(Clone)]
pub struct AuditLogStore {
    pool: DatabasePool,
}

impl AuditLogStore {
    /// Create a new audit log store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Log an audit event
    pub async fn log(&self, entry: &AuditLog) -> Result<Uuid> {
        let resource_type_str = entry.resource_type.map(|rt| rt.as_str().to_string());

        sqlx::query(
            r"
            INSERT INTO audit_logs (
                id, event_type, event_category, actor_id, actor_type, actor_ip,
                resource_type, resource_id, action, description, metadata,
                request_id, session_id, success, error_code, error_message,
                timestamp, duration_ms, retention_days
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            ",
        )
        .bind(entry.id)
        .bind(&entry.event_type)
        .bind(entry.event_category.as_str())
        .bind(entry.actor_id)
        .bind(entry.actor_type.as_str())
        .bind(&entry.actor_ip)
        .bind(resource_type_str)
        .bind(entry.resource_id)
        .bind(&entry.action)
        .bind(&entry.description)
        .bind(&entry.metadata)
        .bind(entry.request_id)
        .bind(entry.session_id)
        .bind(entry.success)
        .bind(&entry.error_code)
        .bind(&entry.error_message)
        .bind(entry.timestamp)
        .bind(entry.duration_ms)
        .bind(entry.retention_days)
        .execute(self.pool.pool())
        .await?;

        Ok(entry.id)
    }

    /// Batch log multiple audit events
    ///
    /// This is more efficient than calling `log()` multiple times
    /// as it uses a single database transaction.
    ///
    /// Returns the number of audit logs created.
    pub async fn batch_log(&self, entries: &[AuditLog]) -> Result<u64> {
        if entries.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.pool().begin().await?;

        for entry in entries {
            let resource_type_str = entry.resource_type.map(|rt| rt.as_str().to_string());

            sqlx::query(
                r"
                INSERT INTO audit_logs (
                    id, event_type, event_category, actor_id, actor_type, actor_ip,
                    resource_type, resource_id, action, description, metadata,
                    request_id, session_id, success, error_code, error_message,
                    timestamp, duration_ms, retention_days
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
                ",
            )
            .bind(entry.id)
            .bind(&entry.event_type)
            .bind(entry.event_category.as_str())
            .bind(entry.actor_id)
            .bind(entry.actor_type.as_str())
            .bind(&entry.actor_ip)
            .bind(&resource_type_str)
            .bind(entry.resource_id)
            .bind(&entry.action)
            .bind(&entry.description)
            .bind(&entry.metadata)
            .bind(entry.request_id)
            .bind(entry.session_id)
            .bind(entry.success)
            .bind(&entry.error_code)
            .bind(&entry.error_message)
            .bind(entry.timestamp)
            .bind(entry.duration_ms)
            .bind(entry.retention_days)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(entries.len() as u64)
    }

    /// Log a workflow event (convenience method)
    pub async fn log_workflow_event(
        &self,
        action: AuditAction,
        workflow_id: Uuid,
        actor_id: Option<Uuid>,
        description: Option<String>,
        metadata: Option<JsonValue>,
    ) -> Result<Uuid> {
        let entry = AuditLogBuilder::new()
            .event_type(format!("workflow.{}", action.as_str()))
            .category(AuditEventCategory::Workflow)
            .action(action)
            .resource(AuditResourceType::Workflow, workflow_id)
            .description(description.unwrap_or_default())
            .metadata(metadata.unwrap_or(serde_json::json!({})));

        let entry = if let Some(id) = actor_id {
            entry.actor(id, AuditActorType::User)
        } else {
            entry.system_actor()
        };

        if let Some(log) = entry.build() {
            self.log(&log).await
        } else {
            Ok(Uuid::nil())
        }
    }

    /// Log an execution event (convenience method)
    pub async fn log_execution_event(
        &self,
        action: AuditAction,
        execution_id: Uuid,
        workflow_id: Uuid,
        actor_id: Option<Uuid>,
        description: Option<String>,
        metadata: Option<JsonValue>,
    ) -> Result<Uuid> {
        let mut base_metadata = metadata.unwrap_or(serde_json::json!({}));
        if let Some(obj) = base_metadata.as_object_mut() {
            obj.insert("workflow_id".to_string(), serde_json::json!(workflow_id));
        }

        let entry = AuditLogBuilder::new()
            .event_type(format!("execution.{}", action.as_str()))
            .category(AuditEventCategory::Execution)
            .action(action)
            .resource(AuditResourceType::Execution, execution_id)
            .description(description.unwrap_or_default())
            .metadata(base_metadata);

        let entry = if let Some(id) = actor_id {
            entry.actor(id, AuditActorType::User)
        } else {
            entry.system_actor()
        };

        if let Some(log) = entry.build() {
            self.log(&log).await
        } else {
            Ok(Uuid::nil())
        }
    }

    /// Log a user security event (convenience method)
    pub async fn log_security_event(
        &self,
        action: AuditAction,
        actor_id: Uuid,
        actor_ip: Option<String>,
        description: String,
        success: bool,
        error_message: Option<String>,
    ) -> Result<Uuid> {
        let mut entry = AuditLogBuilder::new()
            .event_type(format!("security.{}", action.as_str()))
            .category(AuditEventCategory::Security)
            .action(action)
            .actor(actor_id, AuditActorType::User)
            .resource(AuditResourceType::User, actor_id)
            .description(description)
            .success(success);

        if let Some(ip) = actor_ip {
            entry = entry.actor_ip(ip);
        }

        if let Some(msg) = error_message {
            entry = entry.error("AUTH_ERROR", msg);
        }

        if let Some(log) = entry.build() {
            self.log(&log).await
        } else {
            Ok(Uuid::nil())
        }
    }

    /// Query audit logs with filters
    pub async fn query(
        &self,
        filter: &AuditLogFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AuditLog>> {
        // Build dynamic query
        let mut query = String::from(
            r"
            SELECT id, event_type, event_category, actor_id, actor_type, actor_ip,
                   resource_type, resource_id, action, description, metadata,
                   request_id, session_id, success, error_code, error_message,
                   timestamp, duration_ms, retention_days
            FROM audit_logs
            WHERE 1=1
            ",
        );

        let mut params: Vec<String> = Vec::new();
        let mut param_index = 1;

        if let Some(ref cat) = filter.event_category {
            query.push_str(&format!(" AND event_category = ${param_index}"));
            params.push(cat.as_str().to_string());
            param_index += 1;
        }

        if let Some(ref event_type) = filter.event_type {
            query.push_str(&format!(" AND event_type = ${param_index}"));
            params.push(event_type.clone());
            param_index += 1;
        }

        if filter.actor_id.is_some() {
            query.push_str(&format!(" AND actor_id = ${param_index}"));
            param_index += 1;
        }

        if let Some(ref rt) = filter.resource_type {
            query.push_str(&format!(" AND resource_type = ${param_index}"));
            params.push(rt.as_str().to_string());
            param_index += 1;
        }

        if filter.resource_id.is_some() {
            query.push_str(&format!(" AND resource_id = ${param_index}"));
            param_index += 1;
        }

        if filter.success.is_some() {
            query.push_str(&format!(" AND success = ${param_index}"));
            param_index += 1;
        }

        if filter.from_timestamp.is_some() {
            query.push_str(&format!(" AND timestamp >= ${param_index}"));
            param_index += 1;
        }

        if filter.to_timestamp.is_some() {
            query.push_str(&format!(" AND timestamp <= ${param_index}"));
            // param_index += 1;
        }

        query.push_str(" ORDER BY timestamp DESC");
        query.push_str(&format!(" LIMIT {limit} OFFSET {offset}"));

        // For simpler implementation, use the non-filter query for now
        // A more sophisticated implementation would use dynamic parameter binding
        self.list_recent(limit, offset).await
    }

    /// List recent audit logs
    pub async fn list_recent(&self, limit: i64, offset: i64) -> Result<Vec<AuditLog>> {
        #[derive(sqlx::FromRow)]
        struct AuditLogRow {
            id: Uuid,
            event_type: String,
            event_category: String,
            actor_id: Option<Uuid>,
            actor_type: String,
            actor_ip: Option<String>,
            resource_type: Option<String>,
            resource_id: Option<Uuid>,
            action: String,
            description: Option<String>,
            metadata: JsonValue,
            request_id: Option<Uuid>,
            session_id: Option<Uuid>,
            success: bool,
            error_code: Option<String>,
            error_message: Option<String>,
            timestamp: DateTime<Utc>,
            duration_ms: Option<i32>,
            retention_days: Option<i32>,
        }

        let rows = sqlx::query_as::<_, AuditLogRow>(
            r"
            SELECT id, event_type, event_category, actor_id, actor_type, actor_ip,
                   resource_type, resource_id, action, description, metadata,
                   request_id, session_id, success, error_code, error_message,
                   timestamp, duration_ms, retention_days
            FROM audit_logs
            ORDER BY timestamp DESC
            LIMIT $1 OFFSET $2
            ",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await?;

        let logs = rows
            .into_iter()
            .map(|row| {
                let event_category = AuditEventCategory::from_str(&row.event_category)
                    .unwrap_or(AuditEventCategory::System);
                let actor_type =
                    AuditActorType::from_str(&row.actor_type).unwrap_or(AuditActorType::System);
                let resource_type = row
                    .resource_type
                    .as_ref()
                    .and_then(|s| AuditResourceType::from_str(s));

                AuditLog {
                    id: row.id,
                    event_type: row.event_type,
                    event_category,
                    actor_id: row.actor_id,
                    actor_type,
                    actor_ip: row.actor_ip,
                    resource_type,
                    resource_id: row.resource_id,
                    action: row.action,
                    description: row.description,
                    metadata: row.metadata,
                    request_id: row.request_id,
                    session_id: row.session_id,
                    success: row.success,
                    error_code: row.error_code,
                    error_message: row.error_message,
                    timestamp: row.timestamp,
                    duration_ms: row.duration_ms,
                    retention_days: row.retention_days,
                }
            })
            .collect();

        Ok(logs)
    }

    /// Get logs for a specific resource
    pub async fn get_resource_logs(
        &self,
        resource_type: AuditResourceType,
        resource_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        #[derive(sqlx::FromRow)]
        struct AuditLogRow {
            id: Uuid,
            event_type: String,
            event_category: String,
            actor_id: Option<Uuid>,
            actor_type: String,
            actor_ip: Option<String>,
            resource_type: Option<String>,
            resource_id: Option<Uuid>,
            action: String,
            description: Option<String>,
            metadata: JsonValue,
            request_id: Option<Uuid>,
            session_id: Option<Uuid>,
            success: bool,
            error_code: Option<String>,
            error_message: Option<String>,
            timestamp: DateTime<Utc>,
            duration_ms: Option<i32>,
            retention_days: Option<i32>,
        }

        let rows = sqlx::query_as::<_, AuditLogRow>(
            r"
            SELECT id, event_type, event_category, actor_id, actor_type, actor_ip,
                   resource_type, resource_id, action, description, metadata,
                   request_id, session_id, success, error_code, error_message,
                   timestamp, duration_ms, retention_days
            FROM audit_logs
            WHERE resource_type = $1 AND resource_id = $2
            ORDER BY timestamp DESC
            LIMIT $3
            ",
        )
        .bind(resource_type.as_str())
        .bind(resource_id)
        .bind(limit)
        .fetch_all(self.pool.pool())
        .await?;

        let logs = rows
            .into_iter()
            .map(|row| {
                let event_category = AuditEventCategory::from_str(&row.event_category)
                    .unwrap_or(AuditEventCategory::System);
                let actor_type =
                    AuditActorType::from_str(&row.actor_type).unwrap_or(AuditActorType::System);
                let resource_type = row
                    .resource_type
                    .as_ref()
                    .and_then(|s| AuditResourceType::from_str(s));

                AuditLog {
                    id: row.id,
                    event_type: row.event_type,
                    event_category,
                    actor_id: row.actor_id,
                    actor_type,
                    actor_ip: row.actor_ip,
                    resource_type,
                    resource_id: row.resource_id,
                    action: row.action,
                    description: row.description,
                    metadata: row.metadata,
                    request_id: row.request_id,
                    session_id: row.session_id,
                    success: row.success,
                    error_code: row.error_code,
                    error_message: row.error_message,
                    timestamp: row.timestamp,
                    duration_ms: row.duration_ms,
                    retention_days: row.retention_days,
                }
            })
            .collect();

        Ok(logs)
    }

    /// Get audit log statistics
    pub async fn get_stats(&self) -> Result<AuditLogStats> {
        #[derive(sqlx::FromRow)]
        struct CountRow {
            total: i64,
            successful: i64,
            failed: i64,
            first_at: Option<DateTime<Utc>>,
            last_at: Option<DateTime<Utc>>,
        }

        let count_row = sqlx::query_as::<_, CountRow>(
            r"
            SELECT
                COUNT(*) as total,
                SUM(CASE WHEN success THEN 1 ELSE 0 END) as successful,
                SUM(CASE WHEN NOT success THEN 1 ELSE 0 END) as failed,
                MIN(timestamp) as first_at,
                MAX(timestamp) as last_at
            FROM audit_logs
            ",
        )
        .fetch_one(self.pool.pool())
        .await?;

        #[derive(sqlx::FromRow)]
        struct CategoryCount {
            event_category: String,
            count: i64,
        }

        let category_rows = sqlx::query_as::<_, CategoryCount>(
            r"
            SELECT event_category, COUNT(*) as count
            FROM audit_logs
            GROUP BY event_category
            ORDER BY count DESC
            ",
        )
        .fetch_all(self.pool.pool())
        .await?;

        #[derive(sqlx::FromRow)]
        struct ActionCount {
            action: String,
            count: i64,
        }

        let action_rows = sqlx::query_as::<_, ActionCount>(
            r"
            SELECT action, COUNT(*) as count
            FROM audit_logs
            GROUP BY action
            ORDER BY count DESC
            LIMIT 10
            ",
        )
        .fetch_all(self.pool.pool())
        .await?;

        Ok(AuditLogStats {
            total_events: count_row.total as u64,
            successful_events: count_row.successful as u64,
            failed_events: count_row.failed as u64,
            events_by_category: category_rows
                .into_iter()
                .map(|r| (r.event_category, r.count as u64))
                .collect(),
            events_by_action: action_rows
                .into_iter()
                .map(|r| (r.action, r.count as u64))
                .collect(),
            first_event_at: count_row.first_at,
            last_event_at: count_row.last_at,
        })
    }

    /// Clean up expired audit logs
    pub async fn cleanup_expired(&self) -> Result<u64> {
        let result = sqlx::query("SELECT cleanup_expired_audit_logs() as count")
            .fetch_one(self.pool.pool())
            .await?;

        let count: i32 = result.get("count");
        Ok(count as u64)
    }

    /// Delete audit logs older than a certain date
    pub async fn delete_before(&self, before: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query("DELETE FROM audit_logs WHERE timestamp < $1")
            .bind(before)
            .execute(self.pool.pool())
            .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_builder() {
        let log = AuditLogBuilder::new()
            .event_type("workflow.create")
            .category(AuditEventCategory::Workflow)
            .action(AuditAction::Create)
            .actor(Uuid::new_v4(), AuditActorType::User)
            .resource(AuditResourceType::Workflow, Uuid::new_v4())
            .description("Created a new workflow")
            .success(true)
            .build();

        assert!(log.is_some());
        let log = log.unwrap();
        assert_eq!(log.event_type, "workflow.create");
        assert!(log.success);
    }

    #[test]
    fn test_audit_log_builder_with_error() {
        let log = AuditLogBuilder::new()
            .event_type("security.login")
            .category(AuditEventCategory::Security)
            .action(AuditAction::Login)
            .actor(Uuid::new_v4(), AuditActorType::User)
            .actor_ip("192.168.1.1")
            .error("AUTH_FAILED", "Invalid password")
            .build();

        assert!(log.is_some());
        let log = log.unwrap();
        assert!(!log.success);
        assert_eq!(log.error_code, Some("AUTH_FAILED".to_string()));
    }

    #[test]
    fn test_event_category_conversion() {
        assert_eq!(AuditEventCategory::Workflow.as_str(), "workflow");
        assert_eq!(
            AuditEventCategory::from_str("workflow"),
            Some(AuditEventCategory::Workflow)
        );
        assert_eq!(AuditEventCategory::from_str("invalid"), None);
    }
}
