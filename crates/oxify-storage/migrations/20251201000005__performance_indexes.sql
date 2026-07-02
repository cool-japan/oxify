-- Migration: performance_indexes
-- Description: Add performance indexes for improved query performance.
-- Folded in from the retired hand-rolled migration_runner (sql/001_performance_indexes.sql)
-- during the sqlx -> oxisql migration. Pure SQLite CREATE INDEX statements only;
-- the Postgres-only sql/002_schema_constraints.sql (DO $$ ... $$ / ALTER TABLE
-- ADD CONSTRAINT) was dropped because SQLite cannot express it.
--
-- The source sql/001_performance_indexes.sql was written against the Postgres
-- schema, whose column/table names differ from the SQLite schema defined by the
-- earlier migrations in this directory. Under sqlx (compile-time-embedded, never
-- actually run against this SQLite schema) the mismatch went unnoticed; under
-- oxisql-migrate the statements execute against SQLite at runtime, so the
-- following corrections were required to keep the whole migration run from
-- aborting with "no such column"/"no such table":
--   * `executions` has no `created_at`; its creation timestamp column is
--     `started_at` (see 20251130000001__init.sql) -- indexes and their names
--     corrected accordingly.
--   * `schedule_executions` has no `executed_at`; its trigger timestamp column
--     is `triggered_at` (see 20251130000004__schedules.sql) -- index and name
--     corrected accordingly.
--   * the Postgres-era `execution_durations` table and the `workflows.user_id`
--     column have no SQLite counterpart in this schema, so their two indexes are
--     intentionally omitted (there is no schema object to index; redirecting
--     them would fabricate a meaning the SQLite schema does not model).

-- Index for executions table - workflow_id + started_at (Postgres source used created_at)
CREATE INDEX IF NOT EXISTS idx_executions_workflow_id_started_at
ON executions(workflow_id, started_at DESC);

-- Index for executions table - state + started_at (Postgres source used created_at)
CREATE INDEX IF NOT EXISTS idx_executions_state_started_at
ON executions(state, started_at DESC);

-- Index for audit_logs table - event_type + timestamp
CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type_timestamp
ON audit_logs(event_type, timestamp DESC);

-- Index for quota_usage_history table - user_id + time_bucket
CREATE INDEX IF NOT EXISTS idx_quota_usage_history_user_id_time_bucket
ON quota_usage_history(user_id, time_bucket DESC);

-- Index for secret_audit_logs table - timestamp
CREATE INDEX IF NOT EXISTS idx_secret_audit_logs_timestamp
ON secret_audit_logs(timestamp DESC);

-- Index for api_key_usage_logs table - key_id + timestamp
CREATE INDEX IF NOT EXISTS idx_api_key_usage_logs_key_id_timestamp
ON api_key_usage_logs(api_key_id, timestamp DESC);

-- Index for schedule_executions table - schedule_id + triggered_at (Postgres source used executed_at)
CREATE INDEX IF NOT EXISTS idx_schedule_executions_schedule_id_triggered_at
ON schedule_executions(schedule_id, triggered_at DESC);

-- Index for workflow_versions table - workflow_id + version
CREATE INDEX IF NOT EXISTS idx_workflow_versions_workflow_id_version
ON workflow_versions(workflow_id, version DESC);
