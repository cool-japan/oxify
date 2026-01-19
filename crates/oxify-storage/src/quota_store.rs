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
//! quota_store.increment_user_execution_count(&user_id).await?;
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

use crate::{DatabasePool, Result};
use chrono::{DateTime, Duration, Timelike, Utc};
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
        now.date_naive()
            .and_hms_opt(now.hour(), 0, 0)
            .expect("Invalid time")
            .and_utc()
    }

    // ==================== User Quotas ====================

    /// Get or create user quota
    pub async fn get_or_create_user_quota(&self, user_id: &Uuid) -> Result<UserQuota> {
        // Try to get existing quota
        if let Some(quota) = self.get_user_quota(user_id).await? {
            return Ok(quota);
        }

        // Create default quota
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r"
            INSERT INTO user_quotas (id, user_id, last_hourly_reset, last_daily_reset, last_monthly_reset)
            VALUES ($1, $2, $3, $3, $3)
            ON CONFLICT (user_id) DO NOTHING
            ",
        )
        .bind(id)
        .bind(user_id)
        .bind(now)
        .execute(self.pool.pool())
        .await?;

        // Fetch the quota (either just created or existing due to race)
        self.get_user_quota(user_id).await?.ok_or_else(|| {
            crate::StorageError::not_found(crate::ResourceType::Quota, user_id.to_string())
        })
    }

    /// Get user quota
    pub async fn get_user_quota(&self, user_id: &Uuid) -> Result<Option<UserQuota>> {
        #[derive(sqlx::FromRow)]
        struct UserQuotaRow {
            id: Uuid,
            user_id: Uuid,
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
            last_hourly_reset: DateTime<Utc>,
            last_daily_reset: DateTime<Utc>,
            last_monthly_reset: DateTime<Utc>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }

        let row = sqlx::query_as::<_, UserQuotaRow>(
            r"
            SELECT * FROM user_quotas WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_optional(self.pool.pool())
        .await?;

        Ok(row.map(|r| UserQuota {
            id: r.id,
            user_id: r.user_id,
            max_executions_per_day: r.max_executions_per_day,
            max_executions_per_hour: r.max_executions_per_hour,
            max_concurrent_executions: r.max_concurrent_executions,
            max_tokens_per_day: r.max_tokens_per_day,
            max_tokens_per_month: r.max_tokens_per_month,
            max_cost_per_day_cents: r.max_cost_per_day_cents,
            max_cost_per_month_cents: r.max_cost_per_month_cents,
            max_workflows: r.max_workflows,
            max_secrets: r.max_secrets,
            max_api_keys: r.max_api_keys,
            executions_today: r.executions_today,
            executions_this_hour: r.executions_this_hour,
            tokens_today: r.tokens_today,
            tokens_this_month: r.tokens_this_month,
            cost_today_cents: r.cost_today_cents,
            cost_this_month_cents: r.cost_this_month_cents,
            last_hourly_reset: r.last_hourly_reset,
            last_daily_reset: r.last_daily_reset,
            last_monthly_reset: r.last_monthly_reset,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
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

        let result = sqlx::query(
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
        )
        .bind(user_id)
        .bind(max_executions_per_day)
        .bind(max_executions_per_hour)
        .bind(max_tokens_per_day)
        .bind(max_tokens_per_month)
        .bind(max_cost_per_day_cents)
        .bind(max_cost_per_month_cents)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
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
        sqlx::query(
            r"
            UPDATE user_quotas SET
                executions_today = executions_today + 1,
                executions_this_hour = executions_this_hour + 1
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(self.pool.pool())
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
        sqlx::query(
            r"
            UPDATE user_quotas SET
                tokens_today = tokens_today + $2,
                tokens_this_month = tokens_this_month + $2,
                cost_today_cents = cost_today_cents + $3,
                cost_this_month_cents = cost_this_month_cents + $3
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .bind(tokens)
        .bind(cost_cents)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Reset user counters if periods have elapsed
    async fn reset_user_counters_if_needed(&self, user_id: &Uuid, quota: &UserQuota) -> Result<()> {
        let now = Utc::now();

        // Check if hourly reset needed
        if now - quota.last_hourly_reset >= Duration::hours(1) {
            sqlx::query(
                r"
                UPDATE user_quotas SET
                    executions_this_hour = 0,
                    last_hourly_reset = $2
                WHERE user_id = $1
                ",
            )
            .bind(user_id)
            .bind(now)
            .execute(self.pool.pool())
            .await?;
        }

        // Check if daily reset needed
        if now - quota.last_daily_reset >= Duration::days(1) {
            sqlx::query(
                r"
                UPDATE user_quotas SET
                    executions_today = 0,
                    tokens_today = 0,
                    cost_today_cents = 0,
                    last_daily_reset = $2
                WHERE user_id = $1
                ",
            )
            .bind(user_id)
            .bind(now)
            .execute(self.pool.pool())
            .await?;
        }

        // Check if monthly reset needed
        if now - quota.last_monthly_reset >= Duration::days(30) {
            sqlx::query(
                r"
                UPDATE user_quotas SET
                    tokens_this_month = 0,
                    cost_this_month_cents = 0,
                    last_monthly_reset = $2
                WHERE user_id = $1
                ",
            )
            .bind(user_id)
            .bind(now)
            .execute(self.pool.pool())
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

        // Create default quota
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r"
            INSERT INTO workflow_quotas (id, workflow_id, last_hourly_reset, last_daily_reset)
            VALUES ($1, $2, $3, $3)
            ON CONFLICT (workflow_id) DO NOTHING
            ",
        )
        .bind(id)
        .bind(workflow_id)
        .bind(now)
        .execute(self.pool.pool())
        .await?;

        // Fetch the quota
        self.get_workflow_quota(workflow_id).await?.ok_or_else(|| {
            crate::StorageError::not_found(crate::ResourceType::Workflow, *workflow_id)
        })
    }

    /// Get workflow quota
    pub async fn get_workflow_quota(&self, workflow_id: &Uuid) -> Result<Option<WorkflowQuota>> {
        #[derive(sqlx::FromRow)]
        struct WorkflowQuotaRow {
            id: Uuid,
            workflow_id: Uuid,
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
            last_hourly_reset: DateTime<Utc>,
            last_daily_reset: DateTime<Utc>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }

        let row = sqlx::query_as::<_, WorkflowQuotaRow>(
            r"
            SELECT * FROM workflow_quotas WHERE workflow_id = $1
            ",
        )
        .bind(workflow_id)
        .fetch_optional(self.pool.pool())
        .await?;

        Ok(row.map(|r| WorkflowQuota {
            id: r.id,
            workflow_id: r.workflow_id,
            max_executions_per_day: r.max_executions_per_day,
            max_executions_per_hour: r.max_executions_per_hour,
            max_execution_duration_ms: r.max_execution_duration_ms,
            max_retries: r.max_retries,
            max_tokens_per_execution: r.max_tokens_per_execution,
            max_cost_per_execution_cents: r.max_cost_per_execution_cents,
            max_nodes: r.max_nodes,
            max_parallel_nodes: r.max_parallel_nodes,
            executions_today: r.executions_today,
            executions_this_hour: r.executions_this_hour,
            last_hourly_reset: r.last_hourly_reset,
            last_daily_reset: r.last_daily_reset,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
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

        let result = sqlx::query(
            r"
            UPDATE workflow_quotas SET
                max_executions_per_day = COALESCE($2, max_executions_per_day),
                max_executions_per_hour = COALESCE($3, max_executions_per_hour),
                max_execution_duration_ms = COALESCE($4, max_execution_duration_ms),
                max_tokens_per_execution = COALESCE($5, max_tokens_per_execution),
                max_cost_per_execution_cents = COALESCE($6, max_cost_per_execution_cents)
            WHERE workflow_id = $1
            ",
        )
        .bind(workflow_id)
        .bind(max_executions_per_day)
        .bind(max_executions_per_hour)
        .bind(max_execution_duration_ms)
        .bind(max_tokens_per_execution)
        .bind(max_cost_per_execution_cents)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
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
        sqlx::query(
            r"
            UPDATE workflow_quotas SET
                executions_today = executions_today + 1,
                executions_this_hour = executions_this_hour + 1
            WHERE workflow_id = $1
            ",
        )
        .bind(workflow_id)
        .execute(self.pool.pool())
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

        // Check if hourly reset needed
        if now - quota.last_hourly_reset >= Duration::hours(1) {
            sqlx::query(
                r"
                UPDATE workflow_quotas SET
                    executions_this_hour = 0,
                    last_hourly_reset = $2
                WHERE workflow_id = $1
                ",
            )
            .bind(workflow_id)
            .bind(now)
            .execute(self.pool.pool())
            .await?;
        }

        // Check if daily reset needed
        if now - quota.last_daily_reset >= Duration::days(1) {
            sqlx::query(
                r"
                UPDATE workflow_quotas SET
                    executions_today = 0,
                    last_daily_reset = $2
                WHERE workflow_id = $1
                ",
            )
            .bind(workflow_id)
            .bind(now)
            .execute(self.pool.pool())
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
        let time_bucket = Self::current_time_bucket();

        sqlx::query(
            r"
            INSERT INTO quota_usage_history (
                id, user_id, workflow_id, time_bucket,
                executions_count, tokens_used, cost_cents,
                executions_blocked, tokens_blocked, cost_blocked
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT DO NOTHING
            ",
        )
        .bind(id)
        .bind(user_id)
        .bind(workflow_id)
        .bind(time_bucket)
        .bind(executions)
        .bind(tokens)
        .bind(cost_cents)
        .bind(blocked_executions)
        .bind(blocked_tokens)
        .bind(blocked_cost)
        .execute(self.pool.pool())
        .await?;

        Ok(id)
    }

    /// Get usage history for a user
    pub async fn get_user_usage_history(
        &self,
        user_id: &Uuid,
        limit: i64,
    ) -> Result<Vec<QuotaUsageRecord>> {
        #[derive(sqlx::FromRow)]
        struct UsageRow {
            id: Uuid,
            user_id: Option<Uuid>,
            workflow_id: Option<Uuid>,
            time_bucket: DateTime<Utc>,
            executions_count: i32,
            tokens_used: i64,
            cost_cents: i32,
            executions_blocked: i32,
            tokens_blocked: i64,
            cost_blocked: i32,
            created_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, UsageRow>(
            r"
            SELECT * FROM quota_usage_history
            WHERE user_id = $1
            ORDER BY time_bucket DESC
            LIMIT $2
            ",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| QuotaUsageRecord {
                id: r.id,
                user_id: r.user_id,
                workflow_id: r.workflow_id,
                time_bucket: r.time_bucket,
                executions_count: r.executions_count,
                tokens_used: r.tokens_used,
                cost_cents: r.cost_cents,
                executions_blocked: r.executions_blocked,
                tokens_blocked: r.tokens_blocked,
                cost_blocked: r.cost_blocked,
                created_at: r.created_at,
            })
            .collect())
    }

    /// Delete user quota
    pub async fn delete_user_quota(&self, user_id: &Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM user_quotas WHERE user_id = $1")
            .bind(user_id)
            .execute(self.pool.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete workflow quota
    pub async fn delete_workflow_quota(&self, workflow_id: &Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM workflow_quotas WHERE workflow_id = $1")
            .bind(workflow_id)
            .execute(self.pool.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Cleanup old usage history
    pub async fn cleanup_old_history(&self, older_than_days: i64) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(older_than_days);

        let result = sqlx::query("DELETE FROM quota_usage_history WHERE time_bucket < $1")
            .bind(cutoff)
            .execute(self.pool.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Reset all daily counters for all users
    /// Useful for scheduled maintenance or testing
    pub async fn reset_all_daily_counters(&self) -> Result<u64> {
        let now = Utc::now();

        let result = sqlx::query(
            r"
            UPDATE user_quotas SET
                executions_today = 0,
                tokens_today = 0,
                cost_today_cents = 0,
                last_daily_reset = $1
            ",
        )
        .bind(now)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Bulk update user quota limits
    /// Updates limits for multiple users in a single transaction
    pub async fn bulk_update_user_limits(&self, updates: &[UserQuotaLimitUpdate]) -> Result<u64> {
        let mut tx = self.pool.pool().begin().await?;
        let mut total_updated = 0u64;

        for (user_id, max_executions_per_day, max_executions_per_hour, max_tokens_per_day) in
            updates
        {
            // Validate limits
            if let Some(limit) = max_executions_per_day {
                if *limit <= 0 {
                    return Err(crate::StorageError::ValidationError(
                        "max_executions_per_day must be positive".to_string(),
                    ));
                }
            }
            if let Some(limit) = max_executions_per_hour {
                if *limit <= 0 {
                    return Err(crate::StorageError::ValidationError(
                        "max_executions_per_hour must be positive".to_string(),
                    ));
                }
            }
            if let Some(limit) = max_tokens_per_day {
                if *limit <= 0 {
                    return Err(crate::StorageError::ValidationError(
                        "max_tokens_per_day must be positive".to_string(),
                    ));
                }
            }

            let result = sqlx::query(
                r"
                UPDATE user_quotas SET
                    max_executions_per_day = COALESCE($2, max_executions_per_day),
                    max_executions_per_hour = COALESCE($3, max_executions_per_hour),
                    max_tokens_per_day = COALESCE($4, max_tokens_per_day)
                WHERE user_id = $1
                ",
            )
            .bind(user_id)
            .bind(max_executions_per_day)
            .bind(max_executions_per_hour)
            .bind(max_tokens_per_day)
            .execute(&mut *tx)
            .await?;

            total_updated += result.rows_affected();
        }

        tx.commit().await?;
        Ok(total_updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
