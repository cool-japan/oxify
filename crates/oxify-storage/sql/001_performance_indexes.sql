-- Migration: 001_performance_indexes
-- Description: Add performance indexes for improved query performance

-- Index for executions table - workflow_id + created_at
CREATE INDEX IF NOT EXISTS idx_executions_workflow_id_created_at
ON executions(workflow_id, created_at DESC);

-- Index for executions table - state + created_at
CREATE INDEX IF NOT EXISTS idx_executions_state_created_at
ON executions(state, created_at DESC);

-- Index for audit_logs table - event_type + timestamp
CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type_timestamp
ON audit_logs(event_type, timestamp DESC);

-- Index for quota_usage_history table - user_id + time_bucket
CREATE INDEX IF NOT EXISTS idx_quota_usage_history_user_id_time_bucket
ON quota_usage_history(user_id, time_bucket DESC);

-- Index for secret_audit_logs table - timestamp
CREATE INDEX IF NOT EXISTS idx_secret_audit_logs_timestamp
ON secret_audit_logs(timestamp DESC);

-- Index for execution_durations table - for percentile calculations
CREATE INDEX IF NOT EXISTS idx_execution_durations_workflow_time
ON execution_durations(workflow_id, time_bucket, duration_ms);

-- Index for api_key_usage_logs table - key_id + timestamp
CREATE INDEX IF NOT EXISTS idx_api_key_usage_logs_key_id_timestamp
ON api_key_usage_logs(api_key_id, timestamp DESC);

-- Index for schedule_executions table - schedule_id + executed_at
CREATE INDEX IF NOT EXISTS idx_schedule_executions_schedule_id_executed_at
ON schedule_executions(schedule_id, executed_at DESC);

-- Index for workflows table - user_id + updated_at
CREATE INDEX IF NOT EXISTS idx_workflows_user_id_updated_at
ON workflows(user_id, updated_at DESC);

-- Index for workflow_versions table - workflow_id + version
CREATE INDEX IF NOT EXISTS idx_workflow_versions_workflow_id_version
ON workflow_versions(workflow_id, version DESC);
