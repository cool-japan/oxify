-- Migration: 002_schema_constraints
-- Description: Add foreign keys and CHECK constraints for data integrity

-- Add foreign key from schedule_executions to executions
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

-- Add CHECK constraint for cron expressions (5 or 6 fields separated by spaces)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.constraint_column_usage
        WHERE constraint_name = 'chk_schedules_cron_format'
    ) THEN
        ALTER TABLE schedules
        ADD CONSTRAINT chk_schedules_cron_format
        CHECK (
            LENGTH(cron_expression) >= 9 AND
            LENGTH(cron_expression) <= 100 AND
            cron_expression ~ '^[0-9*,/\-]+\s+[0-9*,/\-]+\s+[0-9*,/\-]+\s+[0-9*,/\-?LW]+\s+[0-9*,/\-A-Z]+(\s+[0-9*,/\-?L#]+)?(\s+[0-9*,/\-]+)?$'
        );
    END IF;
END $$;

-- Add CHECK constraints for positive quota values
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

-- Add CHECK constraint for non-negative counters
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
