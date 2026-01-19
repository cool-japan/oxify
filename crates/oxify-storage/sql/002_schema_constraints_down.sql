-- Rollback: 002_schema_constraints
-- Description: Remove foreign keys and CHECK constraints

-- Remove foreign key
ALTER TABLE schedule_executions DROP CONSTRAINT IF EXISTS fk_schedule_executions_execution_id;

-- Remove CHECK constraints
ALTER TABLE schedules DROP CONSTRAINT IF EXISTS chk_schedules_cron_format;
ALTER TABLE user_quotas DROP CONSTRAINT IF EXISTS chk_user_quotas_positive_limits;
ALTER TABLE workflow_quotas DROP CONSTRAINT IF EXISTS chk_workflow_quotas_positive_limits;
ALTER TABLE user_quotas DROP CONSTRAINT IF EXISTS chk_user_quotas_non_negative_counters;
ALTER TABLE workflow_quotas DROP CONSTRAINT IF EXISTS chk_workflow_quotas_non_negative_counters;
