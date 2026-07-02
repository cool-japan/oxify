//! Quota and Resource Limits Storage
//!
//! Provides storage and enforcement for execution quotas, token budgets, and rate limits.
//!
//! ## Overview
//!
//! The quota system manages resource limits for both users and workflows, tracking:
//! - Execution quotas (hourly, daily, concurrent)
//! - Token usage limits (daily, monthly, per-execution)
//! - Cost limits in cents (daily, monthly, per-execution)
//! - Storage limits (workflows, secrets, API keys)
//!
//! ## Quota Reset Behavior
//!
//! Quotas are automatically reset at specific intervals:
//! - **Hourly**: On the hour boundary (e.g., 10:00, 11:00, 12:00)
//! - **Daily**: At midnight UTC
//! - **Monthly**: On the first day of the month at 00:00 UTC
//!
//! The reset is performed lazily when a quota is checked, ensuring accurate tracking
//! without requiring scheduled background jobs.
//!
//! ## Storage Backend Notes
//!
//! This module targets the Pure-Rust OxiSQL SQLite stack. Two conventions are
//! enforced throughout because `oxisql_core` provides neither a `FromValue for
//! Uuid` nor a bare-SQLite-datetime parser:
//!
//! - **UUIDs** are stored/read as their canonical hyphenated `String` form.
//!   Row structs keep `id`/`user_id`/`workflow_id` as `String` and parse to
//!   [`uuid::Uuid`] only in the public-facing conversion.
//! - **Timestamps** are always written by this code as explicit RFC 3339 strings
//!   (`chrono::Utc::now().to_rfc3339()`), never left to the column's
//!   `DEFAULT (datetime('now'))`, which produces a bare `YYYY-MM-DD HH:MM:SS`
//!   value that RFC 3339 parsing rejects. On read they are parsed back with
//!   [`chrono::DateTime::parse_from_rfc3339`].
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{QuotaStore, DatabasePool};
//!
//! let quota_store = QuotaStore::new(pool);
//!
//! // Check if user can execute
//! let check = quota_store.check_user_execution_quota(&user_id).await?;
//! if !check.allowed {
//!     return Err(format!("Quota exceeded: {}", check.reason.unwrap()));
//! }
//!
//! // Record execution usage
//! quota_store.increment_user_execution(&user_id).await?;
//!
//! // Update quota limits
//! quota_store.update_user_quota_limits(
//!     &user_id,
//!     Some(1000),  // max_executions_per_day
//!     Some(100),   // max_executions_per_hour
//!     Some(1_000_000), // max_tokens_per_day
//!     None,        // max_tokens_per_month (keep current)
//!     None,        // max_cost_per_day_cents
//!     None,        // max_cost_per_month_cents
//! ).await?;
//! ```

use crate::row_ext::row_to;
use crate::{DatabasePool, Result};
use chrono::{DateTime, Duration, Timelike, Utc};
use oxisql_core::{Connection, OxiSqlError, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User quota configuration and current usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserQuota {
    pub id: Uuid,
    pub user_id: Uuid,

    // Execution limits
    pub max_executions_per_day: Option<i32>,
    pub max_executions_per_hour: Option<i32>,
    pub max_concurrent_executions: i32,

    // Token budget limits
    pub max_tokens_per_day: Option<i64>,
    pub max_tokens_per_month: Option<i64>,

    // Cost limits (in cents)
    pub max_cost_per_day_cents: Option<i32>,
    pub max_cost_per_month_cents: Option<i32>,

    // Storage limits
    pub max_workflows: i32,
    pub max_secrets: i32,
    pub max_api_keys: i32,

    // Current usage
    pub executions_today: i32,
    pub executions_this_hour: i32,
    pub tokens_today: i64,
    pub tokens_this_month: i64,
    pub cost_today_cents: i32,
    pub cost_this_month_cents: i32,

    // Reset timestamps
    pub last_hourly_reset: DateTime<Utc>,
    pub last_daily_reset: DateTime<Utc>,
    pub last_monthly_reset: DateTime<Utc>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Workflow quota configuration and current usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowQuota {
    pub id: Uuid,
    pub workflow_id: Uuid,

    // Execution limits
    pub max_executions_per_day: Option<i32>,
    pub max_executions_per_hour: Option<i32>,
    pub max_execution_duration_ms: i32,
    pub max_retries: i32,

    // Token budget per execution
    pub max_tokens_per_execution: Option<i64>,

    // Cost limit per execution (in cents)
    pub max_cost_per_execution_cents: Option<i32>,

    // Node limits
    pub max_nodes: i32,
    pub max_parallel_nodes: i32,

    // Current usage
    pub executions_today: i32,
    pub executions_this_hour: i32,

    // Reset timestamps
    pub last_hourly_reset: DateTime<Utc>,
    pub last_daily_reset: DateTime<Utc>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Quota check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub current_usage: i64,
    pub limit: Option<i64>,
    pub remaining: Option<i64>,
}

/// Type alias for bulk user quota limit updates
/// Format: (user_id, max_executions_per_day, max_executions_per_hour, max_tokens_per_day)
pub type UserQuotaLimitUpdate = (Uuid, Option<i32>, Option<i32>, Option<i64>);

impl QuotaCheckResult {
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            reason: None,
            current_usage: 0,
            limit: None,
            remaining: None,
        }
    }

    pub fn denied(reason: impl Into<String>, current: i64, limit: i64) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
            current_usage: current,
            limit: Some(limit),
            remaining: Some(0),
        }
    }

    pub fn with_usage(mut self, current: i64, limit: Option<i64>) -> Self {
        self.current_usage = current;
        self.limit = limit;
        self.remaining = limit.map(|l| (l - current).max(0));
        self
    }
}

/// Quota usage record for history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaUsageRecord {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub time_bucket: DateTime<Utc>,
    pub executions_count: i32,
    pub tokens_used: i64,
    pub cost_cents: i32,
    pub executions_blocked: i32,
    pub tokens_blocked: i64,
    pub cost_blocked: i32,
    pub created_at: DateTime<Utc>,
}

// ==================== Raw row structs ====================
//
// These mirror the on-disk column shapes, keeping UUID columns as `String`
// (oxisql-core has no `FromValue for Uuid`) and timestamp columns as `String`
// (they are parsed as RFC 3339 rather than through the automatic
// `FromValue for DateTime<Utc>` text-arm, which rejects bare SQLite datetimes).
// They are mapped into the public-facing types by the `build_*` helpers below.

/// Raw `user_quotas` row shape.
struct UserQuotaRow {
    id: String,
    user_id: String,
    max_executions_per_day: Option<i32>,
    max_executions_per_hour: Option<i32>,
    max_concurrent_executions: i32,
    max_tokens_per_day: Option<i64>,
    max_tokens_per_month: Option<i64>,
    max_cost_per_day_cents: Option<i32>,
    max_cost_per_month_cents: Option<i32>,
    max_workflows: i32,
    max_secrets: i32,
    max_api_keys: i32,
    executions_today: i32,
    executions_this_hour: i32,
    tokens_today: i64,
    tokens_this_month: i64,
    cost_today_cents: i32,
    cost_this_month_cents: i32,
    last_hourly_reset: String,
    last_daily_reset: String,
    last_monthly_reset: String,
    created_at: String,
    updated_at: String,
}

/// Raw `workflow_quotas` row shape.
struct WorkflowQuotaRow {
    id: String,
    workflow_id: String,
    max_executions_per_day: Option<i32>,
    max_executions_per_hour: Option<i32>,
    max_execution_duration_ms: i32,
    max_retries: i32,
    max_tokens_per_execution: Option<i64>,
    max_cost_per_execution_cents: Option<i32>,
    max_nodes: i32,
    max_parallel_nodes: i32,
    executions_today: i32,
    executions_this_hour: i32,
    last_hourly_reset: String,
    last_daily_reset: String,
    created_at: String,
    updated_at: String,
}

/// Raw `quota_usage_history` row shape.
struct UsageRow {
    id: String,
    user_id: Option<String>,
    workflow_id: Option<String>,
    time_bucket: String,
    executions_count: i32,
    tokens_used: i64,
    cost_cents: i32,
    executions_blocked: i32,
    tokens_blocked: i64,
    cost_blocked: i32,
    created_at: String,
}

/// Parse a required UUID column value.
fn parse_uuid(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value)
        .map_err(|e| crate::StorageError::validation(format!("invalid UUID '{value}': {e}")))
}

/// Parse an optional UUID column value (`None` for a SQL `NULL`).
fn parse_optional_uuid(value: Option<&str>) -> Result<Option<Uuid>> {
    match value {
        Some(s) => Ok(Some(parse_uuid(s)?)),
        None => Ok(None),
    }
}

/// Parse an RFC 3339 timestamp column value into a UTC datetime.
fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            crate::StorageError::validation(format!("invalid RFC 3339 timestamp '{value}': {e}"))
        })
}

/// Map a `user_quotas` row onto [`UserQuotaRow`].
fn map_user_quota_row(row: &Row) -> std::result::Result<UserQuotaRow, OxiSqlError> {
    row_to!(UserQuotaRow {
        id: "id",
        user_id: "user_id",
        max_executions_per_day: "max_executions_per_day",
        max_executions_per_hour: "max_executions_per_hour",
        max_concurrent_executions: "max_concurrent_executions",
        max_tokens_per_day: "max_tokens_per_day",
        max_tokens_per_month: "max_tokens_per_month",
        max_cost_per_day_cents: "max_cost_per_day_cents",
        max_cost_per_month_cents: "max_cost_per_month_cents",
        max_workflows: "max_workflows",
        max_secrets: "max_secrets",
        max_api_keys: "max_api_keys",
        executions_today: "executions_today",
        executions_this_hour: "executions_this_hour",
        tokens_today: "tokens_today",
        tokens_this_month: "tokens_this_month",
        cost_today_cents: "cost_today_cents",
        cost_this_month_cents: "cost_this_month_cents",
        last_hourly_reset: "last_hourly_reset",
        last_daily_reset: "last_daily_reset",
        last_monthly_reset: "last_monthly_reset",
        created_at: "created_at",
        updated_at: "updated_at",
    })(row)
}

/// Map a `workflow_quotas` row onto [`WorkflowQuotaRow`].
fn map_workflow_quota_row(row: &Row) -> std::result::Result<WorkflowQuotaRow, OxiSqlError> {
    row_to!(WorkflowQuotaRow {
        id: "id",
        workflow_id: "workflow_id",
        max_executions_per_day: "max_executions_per_day",
        max_executions_per_hour: "max_executions_per_hour",
        max_execution_duration_ms: "max_execution_duration_ms",
        max_retries: "max_retries",
        max_tokens_per_execution: "max_tokens_per_execution",
        max_cost_per_execution_cents: "max_cost_per_execution_cents",
        max_nodes: "max_nodes",
        max_parallel_nodes: "max_parallel_nodes",
        executions_today: "executions_today",
        executions_this_hour: "executions_this_hour",
        last_hourly_reset: "last_hourly_reset",
        last_daily_reset: "last_daily_reset",
        created_at: "created_at",
        updated_at: "updated_at",
    })(row)
}

/// Map a `quota_usage_history` row onto [`UsageRow`].
fn map_usage_row(row: &Row) -> std::result::Result<UsageRow, OxiSqlError> {
    row_to!(UsageRow {
        id: "id",
        user_id: "user_id",
        workflow_id: "workflow_id",
        time_bucket: "time_bucket",
        executions_count: "executions_count",
        tokens_used: "tokens_used",
        cost_cents: "cost_cents",
        executions_blocked: "executions_blocked",
        tokens_blocked: "tokens_blocked",
        cost_blocked: "cost_blocked",
        created_at: "created_at",
    })(row)
}

/// Convert a raw [`UserQuotaRow`] into the public [`UserQuota`].
fn build_user_quota(raw: UserQuotaRow) -> Result<UserQuota> {
    Ok(UserQuota {
        id: parse_uuid(&raw.id)?,
        user_id: parse_uuid(&raw.user_id)?,
        max_executions_per_day: raw.max_executions_per_day,
        max_executions_per_hour: raw.max_executions_per_hour,
        max_concurrent_executions: raw.max_concurrent_executions,
        max_tokens_per_day: raw.max_tokens_per_day,
        max_tokens_per_month: raw.max_tokens_per_month,
        max_cost_per_day_cents: raw.max_cost_per_day_cents,
        max_cost_per_month_cents: raw.max_cost_per_month_cents,
        max_workflows: raw.max_workflows,
        max_secrets: raw.max_secrets,
        max_api_keys: raw.max_api_keys,
        executions_today: raw.executions_today,
        executions_this_hour: raw.executions_this_hour,
        tokens_today: raw.tokens_today,
        tokens_this_month: raw.tokens_this_month,
        cost_today_cents: raw.cost_today_cents,
        cost_this_month_cents: raw.cost_this_month_cents,
        last_hourly_reset: parse_timestamp(&raw.last_hourly_reset)?,
        last_daily_reset: parse_timestamp(&raw.last_daily_reset)?,
        last_monthly_reset: parse_timestamp(&raw.last_monthly_reset)?,
        created_at: parse_timestamp(&raw.created_at)?,
        updated_at: parse_timestamp(&raw.updated_at)?,
    })
}

/// Convert a raw [`WorkflowQuotaRow`] into the public [`WorkflowQuota`].
fn build_workflow_quota(raw: WorkflowQuotaRow) -> Result<WorkflowQuota> {
    Ok(WorkflowQuota {
        id: parse_uuid(&raw.id)?,
        workflow_id: parse_uuid(&raw.workflow_id)?,
        max_executions_per_day: raw.max_executions_per_day,
        max_executions_per_hour: raw.max_executions_per_hour,
        max_execution_duration_ms: raw.max_execution_duration_ms,
        max_retries: raw.max_retries,
        max_tokens_per_execution: raw.max_tokens_per_execution,
        max_cost_per_execution_cents: raw.max_cost_per_execution_cents,
        max_nodes: raw.max_nodes,
        max_parallel_nodes: raw.max_parallel_nodes,
        executions_today: raw.executions_today,
        executions_this_hour: raw.executions_this_hour,
        last_hourly_reset: parse_timestamp(&raw.last_hourly_reset)?,
        last_daily_reset: parse_timestamp(&raw.last_daily_reset)?,
        created_at: parse_timestamp(&raw.created_at)?,
        updated_at: parse_timestamp(&raw.updated_at)?,
    })
}

/// Convert a raw [`UsageRow`] into the public [`QuotaUsageRecord`].
fn build_usage_record(raw: UsageRow) -> Result<QuotaUsageRecord> {
    Ok(QuotaUsageRecord {
        id: parse_uuid(&raw.id)?,
        user_id: parse_optional_uuid(raw.user_id.as_deref())?,
        workflow_id: parse_optional_uuid(raw.workflow_id.as_deref())?,
        time_bucket: parse_timestamp(&raw.time_bucket)?,
        executions_count: raw.executions_count,
        tokens_used: raw.tokens_used,
        cost_cents: raw.cost_cents,
        executions_blocked: raw.executions_blocked,
        tokens_blocked: raw.tokens_blocked,
        cost_blocked: raw.cost_blocked,
        created_at: parse_timestamp(&raw.created_at)?,
    })
}

/// Quota storage layer
#[derive(Clone)]
pub struct QuotaStore {
    pool: DatabasePool,
}

impl QuotaStore {
    /// Create a new quota store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Get the start of the current hour (for time bucketing)
    fn current_time_bucket() -> DateTime<Utc> {
        let now = Utc::now();
        // `and_hms_opt(hour, 0, 0)` is infallible for any `hour` in `0..=23`
        // (which `now.hour()` always is), so the `None` arm is unreachable; we
        // fall back to `now` rather than panicking to keep this module
        // panic-free.
        match now.date_naive().and_hms_opt(now.hour(), 0, 0) {
            Some(naive) => naive.and_utc(),
            None => now,
        }
    }

    // ==================== User Quotas ====================

    /// Get or create user quota
    pub async fn get_or_create_user_quota(&self, user_id: &Uuid) -> Result<UserQuota> {
        // Try to get existing quota
        if let Some(quota) = self.get_user_quota(user_id).await? {
            return Ok(quota);
        }

        // Create default quota. `created_at`/`updated_at`/reset timestamps are
        // written explicitly as RFC 3339 rather than relying on the column
        // `DEFAULT (datetime('now'))`, which stores a bare SQLite datetime that
        // `parse_timestamp` (RFC 3339) would reject on read.
        let id = Uuid::new_v4();
        let id_str = id.to_string();
        let user_id_str = user_id.to_string();
        let now = Utc::now().to_rfc3339();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            INSERT OR IGNORE INTO user_quotas
                (id, user_id, last_hourly_reset, last_daily_reset, last_monthly_reset,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $3, $3, $3, $3)
            ",
            &[&id_str, &user_id_str, &now],
        )
        .await?;
        // Release the connection before the nested `get_user_quota` acquire so a
        // single-connection pool (used in tests) does not deadlock.
        drop(conn);

        // Fetch the quota (either just created or existing due to race)
        self.get_user_quota(user_id).await?.ok_or_else(|| {
            crate::StorageError::not_found(crate::ResourceType::Quota, user_id.to_string())
        })
    }

    /// Get user quota
    pub async fn get_user_quota(&self, user_id: &Uuid) -> Result<Option<UserQuota>> {
        let user_id_str = user_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT id, user_id, max_executions_per_day, max_executions_per_hour,
                       max_concurrent_executions, max_tokens_per_day, max_tokens_per_month,
                       max_cost_per_day_cents, max_cost_per_month_cents, max_workflows,
                       max_secrets, max_api_keys, executions_today, executions_this_hour,
                       tokens_today, tokens_this_month, cost_today_cents, cost_this_month_cents,
                       last_hourly_reset, last_daily_reset, last_monthly_reset,
                       created_at, updated_at
                FROM user_quotas
                WHERE user_id = $1
                ",
                &[&user_id_str],
            )
            .await?;

        match rows.first() {
            Some(row) => Ok(Some(build_user_quota(map_user_quota_row(row)?)?)),
            None => Ok(None),
        }
    }

    /// Update user quota limits
    #[allow(clippy::too_many_arguments)]
    pub async fn update_user_quota_limits(
        &self,
        user_id: &Uuid,
        max_executions_per_day: Option<i32>,
        max_executions_per_hour: Option<i32>,
        max_tokens_per_day: Option<i64>,
        max_tokens_per_month: Option<i64>,
        max_cost_per_day_cents: Option<i32>,
        max_cost_per_month_cents: Option<i32>,
    ) -> Result<bool> {
        // Validate all limits are positive if provided
        if let Some(limit) = max_executions_per_day {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_executions_per_day must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_executions_per_hour {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_executions_per_hour must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_tokens_per_day {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_tokens_per_day must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_tokens_per_month {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_tokens_per_month must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_cost_per_day_cents {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_cost_per_day_cents must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_cost_per_month_cents {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_cost_per_month_cents must be positive".to_string(),
                ));
            }
        }

        let user_id_str = user_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r"
                UPDATE user_quotas SET
                    max_executions_per_day = COALESCE($2, max_executions_per_day),
                    max_executions_per_hour = COALESCE($3, max_executions_per_hour),
                    max_tokens_per_day = COALESCE($4, max_tokens_per_day),
                    max_tokens_per_month = COALESCE($5, max_tokens_per_month),
                    max_cost_per_day_cents = COALESCE($6, max_cost_per_day_cents),
                    max_cost_per_month_cents = COALESCE($7, max_cost_per_month_cents)
                WHERE user_id = $1
                ",
                &[
                    &user_id_str,
                    &max_executions_per_day,
                    &max_executions_per_hour,
                    &max_tokens_per_day,
                    &max_tokens_per_month,
                    &max_cost_per_day_cents,
                    &max_cost_per_month_cents,
                ],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Check if user can execute (execution quota)
    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    pub async fn check_user_execution_quota(&self, user_id: &Uuid) -> Result<QuotaCheckResult> {
        let quota = self.get_or_create_user_quota(user_id).await?;

        // Reset counters if needed
        self.reset_user_counters_if_needed(user_id, &quota).await?;

        // Refresh quota after potential reset
        let quota = self.get_user_quota(user_id).await?.ok_or_else(|| {
            crate::StorageError::not_found(crate::ResourceType::Quota, user_id.to_string())
        })?;

        // Check hourly limit
        if let Some(limit) = quota.max_executions_per_hour {
            if quota.executions_this_hour >= limit {
                return Ok(QuotaCheckResult::denied(
                    "Hourly execution limit exceeded",
                    i64::from(quota.executions_this_hour),
                    i64::from(limit),
                ));
            }
        }

        // Check daily limit
        if let Some(limit) = quota.max_executions_per_day {
            if quota.executions_today >= limit {
                return Ok(QuotaCheckResult::denied(
                    "Daily execution limit exceeded",
                    i64::from(quota.executions_today),
                    i64::from(limit),
                ));
            }
        }

        Ok(QuotaCheckResult::allowed().with_usage(
            i64::from(quota.executions_today),
            quota.max_executions_per_day.map(i64::from),
        ))
    }

    /// Check if user has token budget
    #[tracing::instrument(skip(self), fields(user_id = %user_id, tokens_needed))]
    pub async fn check_user_token_quota(
        &self,
        user_id: &Uuid,
        tokens_needed: i64,
    ) -> Result<QuotaCheckResult> {
        let quota = self.get_or_create_user_quota(user_id).await?;

        // Check daily token limit
        if let Some(limit) = quota.max_tokens_per_day {
            if quota.tokens_today + tokens_needed > limit {
                return Ok(QuotaCheckResult::denied(
                    "Daily token limit exceeded",
                    quota.tokens_today,
                    limit,
                ));
            }
        }

        // Check monthly token limit
        if let Some(limit) = quota.max_tokens_per_month {
            if quota.tokens_this_month + tokens_needed > limit {
                return Ok(QuotaCheckResult::denied(
                    "Monthly token limit exceeded",
                    quota.tokens_this_month,
                    limit,
                ));
            }
        }

        Ok(QuotaCheckResult::allowed().with_usage(quota.tokens_today, quota.max_tokens_per_day))
    }

    /// Increment user execution count
    pub async fn increment_user_execution(&self, user_id: &Uuid) -> Result<()> {
        let user_id_str = user_id.to_string();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            UPDATE user_quotas SET
                executions_today = executions_today + 1,
                executions_this_hour = executions_this_hour + 1
            WHERE user_id = $1
            ",
            &[&user_id_str],
        )
        .await?;

        Ok(())
    }

    /// Add token usage for user
    pub async fn add_user_token_usage(
        &self,
        user_id: &Uuid,
        tokens: i64,
        cost_cents: i32,
    ) -> Result<()> {
        let user_id_str = user_id.to_string();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            UPDATE user_quotas SET
                tokens_today = tokens_today + $2,
                tokens_this_month = tokens_this_month + $2,
                cost_today_cents = cost_today_cents + $3,
                cost_this_month_cents = cost_this_month_cents + $3
            WHERE user_id = $1
            ",
            &[&user_id_str, &tokens, &cost_cents],
        )
        .await?;

        Ok(())
    }

    /// Reset user counters if periods have elapsed
    async fn reset_user_counters_if_needed(&self, user_id: &Uuid, quota: &UserQuota) -> Result<()> {
        let now = Utc::now();
        let user_id_str = user_id.to_string();
        let now_str = now.to_rfc3339();

        let conn = self.pool.acquire().await?;

        // Check if hourly reset needed
        if now - quota.last_hourly_reset >= Duration::hours(1) {
            conn.execute(
                r"
                UPDATE user_quotas SET
                    executions_this_hour = 0,
                    last_hourly_reset = $2
                WHERE user_id = $1
                ",
                &[&user_id_str, &now_str],
            )
            .await?;
        }

        // Check if daily reset needed
        if now - quota.last_daily_reset >= Duration::days(1) {
            conn.execute(
                r"
                UPDATE user_quotas SET
                    executions_today = 0,
                    tokens_today = 0,
                    cost_today_cents = 0,
                    last_daily_reset = $2
                WHERE user_id = $1
                ",
                &[&user_id_str, &now_str],
            )
            .await?;
        }

        // Check if monthly reset needed
        if now - quota.last_monthly_reset >= Duration::days(30) {
            conn.execute(
                r"
                UPDATE user_quotas SET
                    tokens_this_month = 0,
                    cost_this_month_cents = 0,
                    last_monthly_reset = $2
                WHERE user_id = $1
                ",
                &[&user_id_str, &now_str],
            )
            .await?;
        }

        Ok(())
    }

    // ==================== Workflow Quotas ====================

    /// Get or create workflow quota
    pub async fn get_or_create_workflow_quota(&self, workflow_id: &Uuid) -> Result<WorkflowQuota> {
        // Try to get existing quota
        if let Some(quota) = self.get_workflow_quota(workflow_id).await? {
            return Ok(quota);
        }

        // Create default quota (see `get_or_create_user_quota` for the explicit
        // RFC 3339 timestamp rationale).
        let id = Uuid::new_v4();
        let id_str = id.to_string();
        let workflow_id_str = workflow_id.to_string();
        let now = Utc::now().to_rfc3339();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            INSERT OR IGNORE INTO workflow_quotas
                (id, workflow_id, last_hourly_reset, last_daily_reset, created_at, updated_at)
            VALUES ($1, $2, $3, $3, $3, $3)
            ",
            &[&id_str, &workflow_id_str, &now],
        )
        .await?;
        // Release before the nested acquire (single-connection pool safety).
        drop(conn);

        // Fetch the quota
        self.get_workflow_quota(workflow_id).await?.ok_or_else(|| {
            crate::StorageError::not_found(crate::ResourceType::Workflow, *workflow_id)
        })
    }

    /// Get workflow quota
    pub async fn get_workflow_quota(&self, workflow_id: &Uuid) -> Result<Option<WorkflowQuota>> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT id, workflow_id, max_executions_per_day, max_executions_per_hour,
                       max_execution_duration_ms, max_retries, max_tokens_per_execution,
                       max_cost_per_execution_cents, max_nodes, max_parallel_nodes,
                       executions_today, executions_this_hour,
                       last_hourly_reset, last_daily_reset, created_at, updated_at
                FROM workflow_quotas
                WHERE workflow_id = $1
                ",
                &[&workflow_id_str],
            )
            .await?;

        match rows.first() {
            Some(row) => Ok(Some(build_workflow_quota(map_workflow_quota_row(row)?)?)),
            None => Ok(None),
        }
    }

    /// Update workflow quota limits
    #[allow(clippy::too_many_arguments)]
    pub async fn update_workflow_quota_limits(
        &self,
        workflow_id: &Uuid,
        max_executions_per_day: Option<i32>,
        max_executions_per_hour: Option<i32>,
        max_execution_duration_ms: Option<i32>,
        max_tokens_per_execution: Option<i64>,
        max_cost_per_execution_cents: Option<i32>,
    ) -> Result<bool> {
        // Validate all limits are positive if provided
        if let Some(limit) = max_executions_per_day {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_executions_per_day must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_executions_per_hour {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_executions_per_hour must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_execution_duration_ms {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_execution_duration_ms must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_tokens_per_execution {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_tokens_per_execution must be positive".to_string(),
                ));
            }
        }
        if let Some(limit) = max_cost_per_execution_cents {
            if limit <= 0 {
                return Err(crate::StorageError::ValidationError(
                    "max_cost_per_execution_cents must be positive".to_string(),
                ));
            }
        }

        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r"
                UPDATE workflow_quotas SET
                    max_executions_per_day = COALESCE($2, max_executions_per_day),
                    max_executions_per_hour = COALESCE($3, max_executions_per_hour),
                    max_execution_duration_ms = COALESCE($4, max_execution_duration_ms),
                    max_tokens_per_execution = COALESCE($5, max_tokens_per_execution),
                    max_cost_per_execution_cents = COALESCE($6, max_cost_per_execution_cents)
                WHERE workflow_id = $1
                ",
                &[
                    &workflow_id_str,
                    &max_executions_per_day,
                    &max_executions_per_hour,
                    &max_execution_duration_ms,
                    &max_tokens_per_execution,
                    &max_cost_per_execution_cents,
                ],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Check if workflow can execute
    #[tracing::instrument(skip(self), fields(workflow_id = %workflow_id))]
    pub async fn check_workflow_execution_quota(
        &self,
        workflow_id: &Uuid,
    ) -> Result<QuotaCheckResult> {
        let quota = self.get_or_create_workflow_quota(workflow_id).await?;

        // Reset counters if needed
        self.reset_workflow_counters_if_needed(workflow_id, &quota)
            .await?;

        // Refresh quota
        let quota = self.get_workflow_quota(workflow_id).await?.ok_or_else(|| {
            crate::StorageError::not_found(crate::ResourceType::Workflow, *workflow_id)
        })?;

        // Check hourly limit
        if let Some(limit) = quota.max_executions_per_hour {
            if quota.executions_this_hour >= limit {
                return Ok(QuotaCheckResult::denied(
                    "Workflow hourly execution limit exceeded",
                    i64::from(quota.executions_this_hour),
                    i64::from(limit),
                ));
            }
        }

        // Check daily limit
        if let Some(limit) = quota.max_executions_per_day {
            if quota.executions_today >= limit {
                return Ok(QuotaCheckResult::denied(
                    "Workflow daily execution limit exceeded",
                    i64::from(quota.executions_today),
                    i64::from(limit),
                ));
            }
        }

        Ok(QuotaCheckResult::allowed().with_usage(
            i64::from(quota.executions_today),
            quota.max_executions_per_day.map(i64::from),
        ))
    }

    /// Increment workflow execution count
    pub async fn increment_workflow_execution(&self, workflow_id: &Uuid) -> Result<()> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            UPDATE workflow_quotas SET
                executions_today = executions_today + 1,
                executions_this_hour = executions_this_hour + 1
            WHERE workflow_id = $1
            ",
            &[&workflow_id_str],
        )
        .await?;

        Ok(())
    }

    /// Reset workflow counters if periods have elapsed
    async fn reset_workflow_counters_if_needed(
        &self,
        workflow_id: &Uuid,
        quota: &WorkflowQuota,
    ) -> Result<()> {
        let now = Utc::now();
        let workflow_id_str = workflow_id.to_string();
        let now_str = now.to_rfc3339();

        let conn = self.pool.acquire().await?;

        // Check if hourly reset needed
        if now - quota.last_hourly_reset >= Duration::hours(1) {
            conn.execute(
                r"
                UPDATE workflow_quotas SET
                    executions_this_hour = 0,
                    last_hourly_reset = $2
                WHERE workflow_id = $1
                ",
                &[&workflow_id_str, &now_str],
            )
            .await?;
        }

        // Check if daily reset needed
        if now - quota.last_daily_reset >= Duration::days(1) {
            conn.execute(
                r"
                UPDATE workflow_quotas SET
                    executions_today = 0,
                    last_daily_reset = $2
                WHERE workflow_id = $1
                ",
                &[&workflow_id_str, &now_str],
            )
            .await?;
        }

        Ok(())
    }

    // ==================== Usage History ====================

    /// Record quota usage for analytics
    #[allow(clippy::too_many_arguments)]
    pub async fn record_usage(
        &self,
        user_id: Option<Uuid>,
        workflow_id: Option<Uuid>,
        executions: i32,
        tokens: i64,
        cost_cents: i32,
        blocked_executions: i32,
        blocked_tokens: i64,
        blocked_cost: i32,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let id_str = id.to_string();
        let user_id_str = user_id.map(|u| u.to_string());
        let workflow_id_str = workflow_id.map(|w| w.to_string());
        let time_bucket = Self::current_time_bucket().to_rfc3339();
        let created_at = Utc::now().to_rfc3339();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            INSERT OR IGNORE INTO quota_usage_history (
                id, user_id, workflow_id, time_bucket,
                executions_count, tokens_used, cost_cents,
                executions_blocked, tokens_blocked, cost_blocked,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ",
            &[
                &id_str,
                &user_id_str,
                &workflow_id_str,
                &time_bucket,
                &executions,
                &tokens,
                &cost_cents,
                &blocked_executions,
                &blocked_tokens,
                &blocked_cost,
                &created_at,
            ],
        )
        .await?;

        Ok(id)
    }

    /// Get usage history for a user
    pub async fn get_user_usage_history(
        &self,
        user_id: &Uuid,
        limit: i64,
    ) -> Result<Vec<QuotaUsageRecord>> {
        let user_id_str = user_id.to_string();

        // The `LIMIT` value is interpolated into the SQL text rather than bound
        // as a `$N` parameter: the Limbo backend mis-handles a parameterized
        // `LIMIT ?` (it yields zero rows, or panics inside the VDBE
        // `DecrJumpZero` limit-counter opcode once rows match). `limit` is a
        // trusted `i64`, whose decimal rendering is injection-safe.
        let sql = format!(
            r"
            SELECT id, user_id, workflow_id, time_bucket,
                   executions_count, tokens_used, cost_cents,
                   executions_blocked, tokens_blocked, cost_blocked, created_at
            FROM quota_usage_history
            WHERE user_id = $1
            ORDER BY time_bucket DESC
            LIMIT {limit}
            "
        );

        let conn = self.pool.acquire().await?;
        let rows = conn.query(&sql, &[&user_id_str]).await?;

        let records = rows
            .iter()
            .map(|row| build_usage_record(map_usage_row(row)?))
            .collect::<Result<Vec<QuotaUsageRecord>>>()?;

        Ok(records)
    }

    /// Delete user quota
    pub async fn delete_user_quota(&self, user_id: &Uuid) -> Result<bool> {
        let user_id_str = user_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                "DELETE FROM user_quotas WHERE user_id = $1",
                &[&user_id_str],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Delete workflow quota
    pub async fn delete_workflow_quota(&self, workflow_id: &Uuid) -> Result<bool> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                "DELETE FROM workflow_quotas WHERE workflow_id = $1",
                &[&workflow_id_str],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Cleanup old usage history
    pub async fn cleanup_old_history(&self, older_than_days: i64) -> Result<u64> {
        // `time_bucket` is stored as RFC 3339; comparing against an RFC 3339
        // cutoff string is lexicographically equivalent to chronological order
        // (fixed-width `YYYY-MM-DDTHH:MM:SS` prefix, identical `+00:00` offset).
        let cutoff = (Utc::now() - Duration::days(older_than_days)).to_rfc3339();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                "DELETE FROM quota_usage_history WHERE time_bucket < $1",
                &[&cutoff],
            )
            .await?;

        Ok(rows_affected)
    }

    /// Reset all daily counters for all users
    /// Useful for scheduled maintenance or testing
    pub async fn reset_all_daily_counters(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r"
                UPDATE user_quotas SET
                    executions_today = 0,
                    tokens_today = 0,
                    cost_today_cents = 0,
                    last_daily_reset = $1
                ",
                &[&now],
            )
            .await?;

        Ok(rows_affected)
    }

    /// Bulk update user quota limits
    /// Updates limits for multiple users in a single transaction
    pub async fn bulk_update_user_limits(&self, updates: &[UserQuotaLimitUpdate]) -> Result<u64> {
        let conn = self.pool.acquire().await?;
        let mut tx = conn.transaction().await?;
        let mut total_updated = 0u64;

        for (user_id, max_executions_per_day, max_executions_per_hour, max_tokens_per_day) in
            updates
        {
            // Validate limits. On failure, roll back everything applied so far
            // (an implicit rollback would occur on drop, but rolling back
            // explicitly keeps the intent obvious).
            if let Some(limit) = max_executions_per_day {
                if *limit <= 0 {
                    tx.rollback().await?;
                    return Err(crate::StorageError::ValidationError(
                        "max_executions_per_day must be positive".to_string(),
                    ));
                }
            }
            if let Some(limit) = max_executions_per_hour {
                if *limit <= 0 {
                    tx.rollback().await?;
                    return Err(crate::StorageError::ValidationError(
                        "max_executions_per_hour must be positive".to_string(),
                    ));
                }
            }
            if let Some(limit) = max_tokens_per_day {
                if *limit <= 0 {
                    tx.rollback().await?;
                    return Err(crate::StorageError::ValidationError(
                        "max_tokens_per_day must be positive".to_string(),
                    ));
                }
            }

            let user_id_str = user_id.to_string();
            let update_result = tx
                .execute(
                    r"
                    UPDATE user_quotas SET
                        max_executions_per_day = COALESCE($2, max_executions_per_day),
                        max_executions_per_hour = COALESCE($3, max_executions_per_hour),
                        max_tokens_per_day = COALESCE($4, max_tokens_per_day)
                    WHERE user_id = $1
                    ",
                    &[
                        &user_id_str,
                        max_executions_per_day,
                        max_executions_per_hour,
                        max_tokens_per_day,
                    ],
                )
                .await;

            match update_result {
                Ok(affected) => total_updated += affected,
                Err(e) => {
                    tx.rollback().await?;
                    return Err(crate::StorageError::Database(e));
                }
            }
        }

        tx.commit().await?;
        Ok(total_updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an in-memory pool pinned to a single connection.
    ///
    /// oxisql opens an independent `:memory:` database per pool slot, so the
    /// pool is pinned to one connection to guarantee that the schema applied by
    /// `migrate()` is visible to every subsequent `pool.acquire()` performed by
    /// the store under test.
    async fn setup_test_pool() -> Result<DatabasePool> {
        let config = crate::DatabaseConfig {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite::memory:".to_string()),
            max_connections: 1,
            min_connections: 1,
        };
        DatabasePool::new(config).await
    }

    /// Backdate a user quota's reset timestamps so the lazy-reset logic fires.
    async fn backdate_user_reset_timestamps(
        pool: &DatabasePool,
        user_id: &Uuid,
        to: DateTime<Utc>,
    ) -> Result<()> {
        let user_id_str = user_id.to_string();
        let to_str = to.to_rfc3339();
        let conn = pool.acquire().await?;
        conn.execute(
            r"
            UPDATE user_quotas SET
                last_hourly_reset = $2,
                last_daily_reset = $2,
                last_monthly_reset = $2
            WHERE user_id = $1
            ",
            &[&user_id_str, &to_str],
        )
        .await?;
        Ok(())
    }

    #[test]
    fn test_quota_check_result_allowed() {
        let result = QuotaCheckResult::allowed();
        assert!(result.allowed);
        assert!(result.reason.is_none());
    }

    #[test]
    fn test_quota_check_result_denied() {
        let result = QuotaCheckResult::denied("Test limit exceeded", 100, 100);
        assert!(!result.allowed);
        assert_eq!(result.reason, Some("Test limit exceeded".to_string()));
        assert_eq!(result.current_usage, 100);
        assert_eq!(result.limit, Some(100));
        assert_eq!(result.remaining, Some(0));
    }

    #[test]
    fn test_quota_check_with_usage() {
        let result = QuotaCheckResult::allowed().with_usage(50, Some(100));
        assert!(result.allowed);
        assert_eq!(result.current_usage, 50);
        assert_eq!(result.limit, Some(100));
        assert_eq!(result.remaining, Some(50));
    }

    #[tokio::test]
    async fn test_user_quota_roundtrip() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let user_id = Uuid::new_v4();

        // Nothing there yet.
        assert!(store.get_user_quota(&user_id).await?.is_none());

        // Create with schema defaults.
        let created = store.get_or_create_user_quota(&user_id).await?;
        assert_eq!(created.user_id, user_id);
        assert_eq!(created.max_concurrent_executions, 5);
        assert_eq!(created.max_workflows, 100);
        assert_eq!(created.max_secrets, 50);
        assert_eq!(created.max_api_keys, 10);
        assert_eq!(created.executions_today, 0);
        assert_eq!(created.executions_this_hour, 0);
        assert!(created.max_executions_per_day.is_none());
        assert!(created.max_tokens_per_month.is_none());

        // Read back and confirm the (RFC 3339) timestamps parse and the id is
        // stable.
        let fetched = store
            .get_user_quota(&user_id)
            .await?
            .expect("quota should exist after creation");
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.user_id, user_id);
        assert!(fetched.created_at <= Utc::now());
        assert!(fetched.last_hourly_reset <= Utc::now());

        // Idempotent: a second get_or_create returns the same row.
        let again = store.get_or_create_user_quota(&user_id).await?;
        assert_eq!(again.id, created.id);

        // Delete.
        assert!(store.delete_user_quota(&user_id).await?);
        assert!(store.get_user_quota(&user_id).await?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_workflow_quota_roundtrip() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let workflow_id = Uuid::new_v4();
        assert!(store.get_workflow_quota(&workflow_id).await?.is_none());

        let created = store.get_or_create_workflow_quota(&workflow_id).await?;
        assert_eq!(created.workflow_id, workflow_id);
        assert_eq!(created.max_execution_duration_ms, 300_000);
        assert_eq!(created.max_retries, 3);
        assert_eq!(created.max_nodes, 100);
        assert_eq!(created.max_parallel_nodes, 10);
        assert_eq!(created.executions_today, 0);
        assert!(created.max_executions_per_hour.is_none());

        let fetched = store
            .get_workflow_quota(&workflow_id)
            .await?
            .expect("workflow quota should exist after creation");
        assert_eq!(fetched.id, created.id);
        assert!(fetched.updated_at <= Utc::now());

        // Update limits and confirm COALESCE applied only the provided fields.
        assert!(
            store
                .update_workflow_quota_limits(&workflow_id, Some(500), Some(50), None, None, None,)
                .await?
        );
        let updated = store
            .get_workflow_quota(&workflow_id)
            .await?
            .expect("workflow quota should exist");
        assert_eq!(updated.max_executions_per_day, Some(500));
        assert_eq!(updated.max_executions_per_hour, Some(50));
        // Untouched: retains schema default.
        assert_eq!(updated.max_execution_duration_ms, 300_000);

        assert!(store.delete_workflow_quota(&workflow_id).await?);
        assert!(store.get_workflow_quota(&workflow_id).await?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_increment_user_execution_and_tokens() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let user_id = Uuid::new_v4();
        store.get_or_create_user_quota(&user_id).await?;

        store.increment_user_execution(&user_id).await?;
        store.increment_user_execution(&user_id).await?;
        store.add_user_token_usage(&user_id, 250, 75).await?;

        let quota = store
            .get_user_quota(&user_id)
            .await?
            .expect("user quota should exist");
        assert_eq!(quota.executions_today, 2);
        assert_eq!(quota.executions_this_hour, 2);
        assert_eq!(quota.tokens_today, 250);
        assert_eq!(quota.tokens_this_month, 250);
        assert_eq!(quota.cost_today_cents, 75);
        assert_eq!(quota.cost_this_month_cents, 75);

        Ok(())
    }

    #[tokio::test]
    async fn test_increment_workflow_execution() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let workflow_id = Uuid::new_v4();
        store.get_or_create_workflow_quota(&workflow_id).await?;

        store.increment_workflow_execution(&workflow_id).await?;
        store.increment_workflow_execution(&workflow_id).await?;
        store.increment_workflow_execution(&workflow_id).await?;

        let quota = store
            .get_workflow_quota(&workflow_id)
            .await?
            .expect("workflow quota should exist");
        assert_eq!(quota.executions_today, 3);
        assert_eq!(quota.executions_this_hour, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_user_counter_resets() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool.clone());

        let user_id = Uuid::new_v4();
        store.get_or_create_user_quota(&user_id).await?;

        // Accumulate hourly/daily/monthly usage.
        store.increment_user_execution(&user_id).await?;
        store.increment_user_execution(&user_id).await?;
        store.add_user_token_usage(&user_id, 500, 120).await?;

        // Backdate every reset marker well past all three windows.
        backdate_user_reset_timestamps(&pool, &user_id, Utc::now() - Duration::days(40)).await?;

        // Fetch the backdated quota and run the lazy reset against it.
        let backdated = store
            .get_user_quota(&user_id)
            .await?
            .expect("user quota should exist");
        assert_eq!(backdated.executions_today, 2);
        store
            .reset_user_counters_if_needed(&user_id, &backdated)
            .await?;

        let after = store
            .get_user_quota(&user_id)
            .await?
            .expect("user quota should exist");
        // Hourly reset.
        assert_eq!(after.executions_this_hour, 0);
        // Daily reset.
        assert_eq!(after.executions_today, 0);
        assert_eq!(after.tokens_today, 0);
        assert_eq!(after.cost_today_cents, 0);
        // Monthly reset.
        assert_eq!(after.tokens_this_month, 0);
        assert_eq!(after.cost_this_month_cents, 0);
        // The reset markers were advanced back to (roughly) now.
        assert!(after.last_daily_reset > Utc::now() - Duration::hours(1));

        Ok(())
    }

    #[tokio::test]
    async fn test_user_execution_quota_denied() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let user_id = Uuid::new_v4();
        store.get_or_create_user_quota(&user_id).await?;

        // Allow a generous daily budget but only one execution per hour.
        assert!(
            store
                .update_user_quota_limits(&user_id, Some(100), Some(1), None, None, None, None)
                .await?
        );

        // First check: nothing used yet -> allowed.
        let allowed = store.check_user_execution_quota(&user_id).await?;
        assert!(allowed.allowed);

        // Consume the single hourly slot.
        store.increment_user_execution(&user_id).await?;

        // Second check: hourly limit reached -> denied.
        let denied = store.check_user_execution_quota(&user_id).await?;
        assert!(!denied.allowed);
        assert_eq!(
            denied.reason.as_deref(),
            Some("Hourly execution limit exceeded")
        );
        assert_eq!(denied.current_usage, 1);
        assert_eq!(denied.limit, Some(1));
        assert_eq!(denied.remaining, Some(0));

        Ok(())
    }

    #[tokio::test]
    async fn test_user_token_quota_denied() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let user_id = Uuid::new_v4();
        store.get_or_create_user_quota(&user_id).await?;

        // Cap daily tokens at 100.
        assert!(
            store
                .update_user_quota_limits(&user_id, None, None, Some(100), None, None, None)
                .await?
        );

        // Requesting within budget is allowed.
        let allowed = store.check_user_token_quota(&user_id, 50).await?;
        assert!(allowed.allowed);

        // Requesting beyond budget is denied.
        let denied = store.check_user_token_quota(&user_id, 200).await?;
        assert!(!denied.allowed);
        assert_eq!(denied.reason.as_deref(), Some("Daily token limit exceeded"));
        assert_eq!(denied.limit, Some(100));

        Ok(())
    }

    #[tokio::test]
    async fn test_workflow_execution_quota_denied() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let workflow_id = Uuid::new_v4();
        store.get_or_create_workflow_quota(&workflow_id).await?;
        assert!(
            store
                .update_workflow_quota_limits(&workflow_id, Some(1), None, None, None, None)
                .await?
        );

        // First execution allowed.
        assert!(
            store
                .check_workflow_execution_quota(&workflow_id)
                .await?
                .allowed
        );
        store.increment_workflow_execution(&workflow_id).await?;

        // Daily limit of 1 now reached.
        let denied = store.check_workflow_execution_quota(&workflow_id).await?;
        assert!(!denied.allowed);
        assert_eq!(
            denied.reason.as_deref(),
            Some("Workflow daily execution limit exceeded")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_usage_history_roundtrip() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let user_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let record_id = store
            .record_usage(Some(user_id), Some(workflow_id), 5, 1_000, 200, 1, 10, 3)
            .await?;
        assert_ne!(record_id, Uuid::nil());

        let history = store.get_user_usage_history(&user_id, 10).await?;
        assert_eq!(history.len(), 1);
        let record = &history[0];
        assert_eq!(record.id, record_id);
        assert_eq!(record.user_id, Some(user_id));
        assert_eq!(record.workflow_id, Some(workflow_id));
        assert_eq!(record.executions_count, 5);
        assert_eq!(record.tokens_used, 1_000);
        assert_eq!(record.cost_cents, 200);
        assert_eq!(record.executions_blocked, 1);
        assert_eq!(record.tokens_blocked, 10);
        assert_eq!(record.cost_blocked, 3);
        // Timestamps round-tripped through RFC 3339.
        assert!(record.time_bucket <= Utc::now());
        assert!(record.created_at <= Utc::now());

        // A different user has no history.
        let other = Uuid::new_v4();
        assert!(store.get_user_usage_history(&other, 10).await?.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_bulk_update_user_limits() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let user_a = Uuid::new_v4();
        let user_b = Uuid::new_v4();
        store.get_or_create_user_quota(&user_a).await?;
        store.get_or_create_user_quota(&user_b).await?;

        let updated = store
            .bulk_update_user_limits(&[
                (user_a, Some(10), Some(2), Some(1_000)),
                (user_b, Some(20), None, None),
            ])
            .await?;
        assert_eq!(updated, 2);

        let a = store
            .get_user_quota(&user_a)
            .await?
            .expect("user a quota should exist");
        assert_eq!(a.max_executions_per_day, Some(10));
        assert_eq!(a.max_executions_per_hour, Some(2));
        assert_eq!(a.max_tokens_per_day, Some(1_000));

        let b = store
            .get_user_quota(&user_b)
            .await?
            .expect("user b quota should exist");
        assert_eq!(b.max_executions_per_day, Some(20));

        // An invalid limit rejects the whole batch.
        let err = store
            .bulk_update_user_limits(&[(user_a, Some(-1), None, None)])
            .await;
        assert!(err.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_reset_all_daily_counters_and_cleanup() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;
        let store = QuotaStore::new(pool);

        let user_id = Uuid::new_v4();
        store.get_or_create_user_quota(&user_id).await?;
        store.increment_user_execution(&user_id).await?;
        store.add_user_token_usage(&user_id, 300, 90).await?;

        let reset = store.reset_all_daily_counters().await?;
        assert!(reset >= 1);

        let quota = store
            .get_user_quota(&user_id)
            .await?
            .expect("user quota should exist");
        assert_eq!(quota.executions_today, 0);
        assert_eq!(quota.tokens_today, 0);
        assert_eq!(quota.cost_today_cents, 0);

        // Record a usage row and prove cleanup keeps recent history but drops
        // nothing when the retention window is wide.
        store
            .record_usage(Some(user_id), None, 1, 10, 5, 0, 0, 0)
            .await?;
        let removed = store.cleanup_old_history(3650).await?;
        assert_eq!(removed, 0);
        assert_eq!(store.get_user_usage_history(&user_id, 10).await?.len(), 1);

        Ok(())
    }
}
