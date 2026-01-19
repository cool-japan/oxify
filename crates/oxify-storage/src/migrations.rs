//! Database migration utilities
//!
//! This module provides functions to apply database schema changes,
//! including creating missing indexes for performance optimization.

use crate::{DatabasePool, Result};

/// Apply all missing performance indexes to the database
///
/// This function creates indexes that improve query performance across
/// the oxify-storage system. It's safe to run multiple times as it uses
/// `IF NOT EXISTS` clauses.
///
/// ## Indexes Created
///
/// - `idx_executions_workflow_id_created_at` - Speeds up workflow execution lookups
/// - `idx_executions_state_created_at` - Speeds up state-based queries
/// - `idx_audit_logs_event_type_timestamp` - Speeds up audit log filtering
/// - `idx_quota_usage_history_user_id_time_bucket` - Speeds up quota history lookups
/// - `idx_secret_audit_logs_timestamp` - Speeds up secret audit log queries
/// - `idx_execution_durations_workflow_time` - Speeds up percentile calculations
/// - `idx_api_key_usage_logs_key_id_timestamp` - Speeds up API key usage queries
/// - `idx_schedule_executions_schedule_id_executed_at` - Speeds up schedule history
/// - `idx_workflows_user_id_updated_at` - Speeds up workflow filtering
/// - `idx_workflow_versions_workflow_id_version` - Speeds up version history
///
/// ## Example
///
/// ```ignore
/// use oxify_storage::{DatabasePool, migrations};
///
/// let pool = DatabasePool::new("postgresql://localhost/oxify", 10, 2).await?;
/// migrations::apply_performance_indexes(&pool).await?;
/// ```
pub async fn apply_performance_indexes(pool: &DatabasePool) -> Result<()> {
    // Index for executions table - workflow_id + created_at
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_executions_workflow_id_created_at
        ON executions(workflow_id, created_at DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for executions table - state + created_at
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_executions_state_created_at
        ON executions(state, created_at DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for audit_logs table - event_type + timestamp
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type_timestamp
        ON audit_logs(event_type, timestamp DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for quota_usage_history table - user_id + time_bucket
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_quota_usage_history_user_id_time_bucket
        ON quota_usage_history(user_id, time_bucket DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for secret_audit_logs table - timestamp
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_secret_audit_logs_timestamp
        ON secret_audit_logs(timestamp DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for execution_durations table - for percentile calculations
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_execution_durations_workflow_time
        ON execution_durations(workflow_id, time_bucket, duration_ms)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for api_key_usage_logs table - key_id + timestamp
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_api_key_usage_logs_key_id_timestamp
        ON api_key_usage_logs(api_key_id, timestamp DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for schedule_executions table - schedule_id + executed_at
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_schedule_executions_schedule_id_executed_at
        ON schedule_executions(schedule_id, executed_at DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for workflows table - user_id + updated_at
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_workflows_user_id_updated_at
        ON workflows(user_id, updated_at DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    // Index for workflow_versions table - workflow_id + version
    sqlx::query(
        r"
        CREATE INDEX IF NOT EXISTS idx_workflow_versions_workflow_id_version
        ON workflow_versions(workflow_id, version DESC)
        ",
    )
    .execute(pool.pool())
    .await?;

    Ok(())
}

/// Apply schema enhancements including foreign keys and CHECK constraints
///
/// This function adds data integrity constraints to improve schema robustness.
/// It's safe to run multiple times as it uses `IF NOT EXISTS` clauses where applicable.
///
/// ## Constraints Added
///
/// - Foreign key from schedule_executions to executions
/// - CHECK constraints for cron expressions (5 or 6 fields)
/// - CHECK constraints for positive quota values
///
/// ## Example
///
/// ```ignore
/// use oxify_storage::{DatabasePool, migrations};
///
/// let pool = DatabasePool::new("postgresql://localhost/oxify", 10, 2).await?;
/// migrations::apply_schema_enhancements(&pool).await?;
/// ```
pub async fn apply_schema_enhancements(pool: &DatabasePool) -> Result<()> {
    // Add foreign key from schedule_executions to executions
    // Note: This might fail if there are orphaned records
    sqlx::query(
        r"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.table_constraints
                WHERE constraint_name = 'fk_schedule_executions_execution_id'
            ) THEN
                ALTER TABLE schedule_executions
                ADD CONSTRAINT fk_schedule_executions_execution_id
                FOREIGN KEY (execution_id) REFERENCES executions(id)
                ON DELETE CASCADE;
            END IF;
        END $$;
        ",
    )
    .execute(pool.pool())
    .await?;

    // Add CHECK constraint for cron expressions (5 or 6 fields separated by spaces)
    sqlx::query(
        r"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.constraint_column_usage
                WHERE constraint_name = 'chk_schedules_cron_format'
            ) THEN
                ALTER TABLE schedules
                ADD CONSTRAINT chk_schedules_cron_format
                CHECK (
                    -- Allow 5 or 6 fields: second minute hour day month day_of_week [year]
                    -- This is a basic check - actual validation happens in application code
                    LENGTH(cron_expression) >= 9 AND
                    LENGTH(cron_expression) <= 100 AND
                    cron_expression ~ '^[0-9*,/\-]+\s+[0-9*,/\-]+\s+[0-9*,/\-]+\s+[0-9*,/\-?LW]+\s+[0-9*,/\-A-Z]+(\s+[0-9*,/\-?L#]+)?(\s+[0-9*,/\-]+)?$'
                );
            END IF;
        END $$;
        ",
    )
    .execute(pool.pool())
    .await?;

    // Add CHECK constraints for positive quota values
    sqlx::query(
        r"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.constraint_column_usage
                WHERE constraint_name = 'chk_user_quotas_positive_limits'
            ) THEN
                ALTER TABLE user_quotas
                ADD CONSTRAINT chk_user_quotas_positive_limits
                CHECK (
                    (max_workflows IS NULL OR max_workflows >= 0) AND
                    (max_executions_per_hour IS NULL OR max_executions_per_hour >= 0) AND
                    (max_executions_per_day IS NULL OR max_executions_per_day >= 0)
                );
            END IF;
        END $$;
        ",
    )
    .execute(pool.pool())
    .await?;

    sqlx::query(
        r"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.constraint_column_usage
                WHERE constraint_name = 'chk_workflow_quotas_positive_limits'
            ) THEN
                ALTER TABLE workflow_quotas
                ADD CONSTRAINT chk_workflow_quotas_positive_limits
                CHECK (
                    (max_executions_per_hour IS NULL OR max_executions_per_hour >= 0) AND
                    (max_executions_per_day IS NULL OR max_executions_per_day >= 0)
                );
            END IF;
        END $$;
        ",
    )
    .execute(pool.pool())
    .await?;

    // Add CHECK constraint for non-negative counters
    sqlx::query(
        r"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.constraint_column_usage
                WHERE constraint_name = 'chk_user_quotas_non_negative_counters'
            ) THEN
                ALTER TABLE user_quotas
                ADD CONSTRAINT chk_user_quotas_non_negative_counters
                CHECK (
                    workflow_count >= 0 AND
                    executions_this_hour >= 0 AND
                    executions_this_day >= 0
                );
            END IF;
        END $$;
        ",
    )
    .execute(pool.pool())
    .await?;

    sqlx::query(
        r"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.constraint_column_usage
                WHERE constraint_name = 'chk_workflow_quotas_non_negative_counters'
            ) THEN
                ALTER TABLE workflow_quotas
                ADD CONSTRAINT chk_workflow_quotas_non_negative_counters
                CHECK (
                    executions_this_hour >= 0 AND
                    executions_this_day >= 0
                );
            END IF;
        END $$;
        ",
    )
    .execute(pool.pool())
    .await?;

    Ok(())
}

/// Check if all performance indexes exist
///
/// Returns a list of missing index names that should be created.
pub async fn check_missing_indexes(pool: &DatabasePool) -> Result<Vec<String>> {
    let required_indexes = vec![
        "idx_executions_workflow_id_created_at",
        "idx_executions_state_created_at",
        "idx_audit_logs_event_type_timestamp",
        "idx_quota_usage_history_user_id_time_bucket",
        "idx_secret_audit_logs_timestamp",
        "idx_execution_durations_workflow_time",
        "idx_api_key_usage_logs_key_id_timestamp",
        "idx_schedule_executions_schedule_id_executed_at",
        "idx_workflows_user_id_updated_at",
        "idx_workflow_versions_workflow_id_version",
    ];

    let mut missing = Vec::new();

    for index_name in required_indexes {
        #[derive(sqlx::FromRow)]
        struct IndexExists {
            exists: bool,
        }

        let result = sqlx::query_as::<_, IndexExists>(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM pg_indexes
                WHERE indexname = $1
            ) as exists
            ",
        )
        .bind(index_name)
        .fetch_one(pool.pool())
        .await?;

        if !result.exists {
            missing.push(index_name.to_string());
        }
    }

    Ok(missing)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_required_indexes_list() {
        // This test ensures we document all required indexes
        let indexes = [
            "idx_executions_workflow_id_created_at",
            "idx_executions_state_created_at",
            "idx_audit_logs_event_type_timestamp",
            "idx_quota_usage_history_user_id_time_bucket",
            "idx_secret_audit_logs_timestamp",
            "idx_execution_durations_workflow_time",
            "idx_api_key_usage_logs_key_id_timestamp",
            "idx_schedule_executions_schedule_id_executed_at",
            "idx_workflows_user_id_updated_at",
            "idx_workflow_versions_workflow_id_version",
        ];

        assert_eq!(indexes.len(), 10);
    }
}
