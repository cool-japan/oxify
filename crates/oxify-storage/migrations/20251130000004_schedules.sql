-- Workflow schedules table (SQLite)
CREATE TABLE IF NOT EXISTS schedules (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    cron TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    enabled INTEGER NOT NULL DEFAULT 1,
    input_variables TEXT NOT NULL DEFAULT '{}',
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    last_run TEXT,
    next_run TEXT,
    run_count INTEGER NOT NULL DEFAULT 0,
    max_runs INTEGER,
    expires_at TEXT,

    -- Foreign key constraint
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);

-- Schedule execution history table
CREATE TABLE IF NOT EXISTS schedule_executions (
    id TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL,
    execution_id TEXT NOT NULL,
    triggered_at TEXT DEFAULT (datetime('now')) NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    duration_ms INTEGER,

    -- Foreign key constraints
    FOREIGN KEY (schedule_id) REFERENCES schedules(id) ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_schedules_workflow_id
    ON schedules(workflow_id);

CREATE INDEX IF NOT EXISTS idx_schedules_enabled
    ON schedules(enabled);

CREATE INDEX IF NOT EXISTS idx_schedules_next_run
    ON schedules(next_run);

CREATE INDEX IF NOT EXISTS idx_schedule_executions_schedule_id
    ON schedule_executions(schedule_id);

CREATE INDEX IF NOT EXISTS idx_schedule_executions_triggered_at
    ON schedule_executions(triggered_at DESC);
