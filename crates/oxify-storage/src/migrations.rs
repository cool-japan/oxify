//! Database migration utilities
//!
//! This module provides functions to apply database schema changes,
//! including creating missing indexes for performance optimization.

use crate::row_ext::row_to;
use crate::{DatabasePool, Result};
use oxisql_core::Connection;

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
/// use oxify_storage::{DatabaseConfig, DatabasePool, migrations};
///
/// let pool = DatabasePool::new(DatabaseConfig::default()).await?;
/// migrations::apply_performance_indexes(&pool).await?;
/// ```
pub async fn apply_performance_indexes(pool: &DatabasePool) -> Result<()> {
    let conn = pool.acquire().await?;

    // Index for executions table - workflow_id + created_at
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_executions_workflow_id_created_at
        ON executions(workflow_id, created_at DESC)
        ",
        &[],
    )
    .await?;

    // Index for executions table - state + created_at
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_executions_state_created_at
        ON executions(state, created_at DESC)
        ",
        &[],
    )
    .await?;

    // Index for audit_logs table - event_type + timestamp
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type_timestamp
        ON audit_logs(event_type, timestamp DESC)
        ",
        &[],
    )
    .await?;

    // Index for quota_usage_history table - user_id + time_bucket
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_quota_usage_history_user_id_time_bucket
        ON quota_usage_history(user_id, time_bucket DESC)
        ",
        &[],
    )
    .await?;

    // Index for secret_audit_logs table - timestamp
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_secret_audit_logs_timestamp
        ON secret_audit_logs(timestamp DESC)
        ",
        &[],
    )
    .await?;

    // Index for execution_durations table - for percentile calculations
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_execution_durations_workflow_time
        ON execution_durations(workflow_id, time_bucket, duration_ms)
        ",
        &[],
    )
    .await?;

    // Index for api_key_usage_logs table - key_id + timestamp
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_api_key_usage_logs_key_id_timestamp
        ON api_key_usage_logs(api_key_id, timestamp DESC)
        ",
        &[],
    )
    .await?;

    // Index for schedule_executions table - schedule_id + executed_at
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_schedule_executions_schedule_id_executed_at
        ON schedule_executions(schedule_id, executed_at DESC)
        ",
        &[],
    )
    .await?;

    // Index for workflows table - user_id + updated_at
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_workflows_user_id_updated_at
        ON workflows(user_id, updated_at DESC)
        ",
        &[],
    )
    .await?;

    // Index for workflow_versions table - workflow_id + version
    conn.execute(
        r"
        CREATE INDEX IF NOT EXISTS idx_workflow_versions_workflow_id_version
        ON workflow_versions(workflow_id, version DESC)
        ",
        &[],
    )
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
/// use oxify_storage::{DatabaseConfig, DatabasePool, migrations};
///
/// let pool = DatabasePool::new(DatabaseConfig::default()).await?;
/// migrations::apply_schema_enhancements(&pool).await?;
/// ```
///
/// ## Note
///
/// These statements use Postgres-only syntax (`DO $$ ... $$` anonymous
/// blocks against `information_schema`, plus `ALTER TABLE ... ADD
/// CONSTRAINT`, which SQLite does not support at all). This function has no
/// callers anywhere in the workspace; the constraint checks it was meant to
/// add were intentionally *not* ported to the SQLite-backed schema (see
/// `migrations/20251201000005__performance_indexes.sql`, which only carries
/// forward the pure-SQL `CREATE INDEX` statements from
/// [`apply_performance_indexes`]). The API surface is preserved here as a
/// mechanical sqlx -> oxisql conversion, but invoking it against the SQLite
/// backend will fail at execution time.
pub async fn apply_schema_enhancements(pool: &DatabasePool) -> Result<()> {
    let conn = pool.acquire().await?;

    // Add foreign key from schedule_executions to executions
    // Note: This might fail if there are orphaned records
    conn.execute(
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
        &[],
    )
    .await?;

    // Add CHECK constraint for cron expressions (5 or 6 fields separated by spaces)
    conn.execute(
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
        &[],
    )
    .await?;

    // Add CHECK constraints for positive quota values
    conn.execute(
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
        &[],
    )
    .await?;

    conn.execute(
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
        &[],
    )
    .await?;

    // Add CHECK constraint for non-negative counters
    conn.execute(
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
        &[],
    )
    .await?;

    conn.execute(
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
        &[],
    )
    .await?;

    Ok(())
}

/// Row shape for the `sqlite_master` index-existence probe in
/// [`check_missing_indexes`]. Replaces the old
/// `#[derive(sqlx::FromRow)] struct IndexExists { exists: bool }`, which
/// modelled a Postgres `SELECT EXISTS(...)` boolean projection; SQLite has no
/// native boolean type, and the untyped integer a boolean expression would
/// produce here decodes as [`oxisql_core::Value::I64`], not
/// `oxisql_core::Value::Bool`, so the row shape is redefined around the
/// `name` column actually selected from `sqlite_master` instead.
struct IndexNameRow {
    name: String,
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
    let conn = pool.acquire().await?;

    for index_name in required_indexes {
        // Postgres-only `pg_indexes` catalog replaced with the SQLite
        // equivalent `sqlite_master` catalog table.
        let rows = conn
            .query(
                r"
                SELECT name FROM sqlite_master WHERE type='index' AND name = $1
                ",
                &[&index_name],
            )
            .await?;

        let matches = rows
            .iter()
            .map(row_to!(IndexNameRow { name: "name" }))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let exists = matches.iter().any(|row| row.name == index_name);

        if !exists {
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
