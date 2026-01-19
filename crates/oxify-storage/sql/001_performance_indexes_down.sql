-- Rollback: 001_performance_indexes
-- Description: Remove performance indexes

DROP INDEX IF EXISTS idx_executions_workflow_id_created_at;
DROP INDEX IF EXISTS idx_executions_state_created_at;
DROP INDEX IF EXISTS idx_audit_logs_event_type_timestamp;
DROP INDEX IF EXISTS idx_quota_usage_history_user_id_time_bucket;
DROP INDEX IF EXISTS idx_secret_audit_logs_timestamp;
DROP INDEX IF EXISTS idx_execution_durations_workflow_time;
DROP INDEX IF EXISTS idx_api_key_usage_logs_key_id_timestamp;
DROP INDEX IF EXISTS idx_schedule_executions_schedule_id_executed_at;
DROP INDEX IF EXISTS idx_workflows_user_id_updated_at;
DROP INDEX IF EXISTS idx_workflow_versions_workflow_id_version;
