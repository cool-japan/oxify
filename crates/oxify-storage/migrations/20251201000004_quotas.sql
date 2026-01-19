-- Quota and Resource Limits tables (SQLite)
-- For tracking and enforcing execution quotas, token budgets, and rate limits

-- User quotas table
CREATE TABLE IF NOT EXISTS user_quotas (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,

    -- Execution limits
    max_executions_per_day INTEGER,
    max_executions_per_hour INTEGER,
    max_concurrent_executions INTEGER DEFAULT 5,

    -- Token budget limits
    max_tokens_per_day INTEGER,
    max_tokens_per_month INTEGER,

    -- Cost limits (in USD cents to avoid floating point)
    max_cost_per_day_cents INTEGER,
    max_cost_per_month_cents INTEGER,

    -- Storage limits
    max_workflows INTEGER DEFAULT 100,
    max_secrets INTEGER DEFAULT 50,
    max_api_keys INTEGER DEFAULT 10,

    -- Current usage tracking (reset periodically)
    executions_today INTEGER NOT NULL DEFAULT 0,
    executions_this_hour INTEGER NOT NULL DEFAULT 0,
    tokens_today INTEGER NOT NULL DEFAULT 0,
    tokens_this_month INTEGER NOT NULL DEFAULT 0,
    cost_today_cents INTEGER NOT NULL DEFAULT 0,
    cost_this_month_cents INTEGER NOT NULL DEFAULT 0,

    -- Reset timestamps
    last_hourly_reset TEXT DEFAULT (datetime('now')) NOT NULL,
    last_daily_reset TEXT DEFAULT (datetime('now')) NOT NULL,
    last_monthly_reset TEXT DEFAULT (datetime('now')) NOT NULL,

    -- Metadata
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL
);

-- Workflow quotas table
CREATE TABLE IF NOT EXISTS workflow_quotas (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL UNIQUE,

    -- Execution limits
    max_executions_per_day INTEGER,
    max_executions_per_hour INTEGER,
    max_execution_duration_ms INTEGER DEFAULT 300000,
    max_retries INTEGER DEFAULT 3,

    -- Token budget per execution
    max_tokens_per_execution INTEGER,

    -- Cost limit per execution (in cents)
    max_cost_per_execution_cents INTEGER,

    -- Node limits
    max_nodes INTEGER DEFAULT 100,
    max_parallel_nodes INTEGER DEFAULT 10,

    -- Current usage tracking
    executions_today INTEGER NOT NULL DEFAULT 0,
    executions_this_hour INTEGER NOT NULL DEFAULT 0,

    -- Reset timestamps
    last_hourly_reset TEXT DEFAULT (datetime('now')) NOT NULL,
    last_daily_reset TEXT DEFAULT (datetime('now')) NOT NULL,

    -- Metadata
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL
);

-- Quota usage history for analytics
CREATE TABLE IF NOT EXISTS quota_usage_history (
    id TEXT PRIMARY KEY,
    user_id TEXT,
    workflow_id TEXT,

    -- Time bucket (hourly)
    time_bucket TEXT NOT NULL,

    -- Usage counts
    executions_count INTEGER NOT NULL DEFAULT 0,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    cost_cents INTEGER NOT NULL DEFAULT 0,

    -- Limit violations
    executions_blocked INTEGER NOT NULL DEFAULT 0,
    tokens_blocked INTEGER NOT NULL DEFAULT 0,
    cost_blocked INTEGER NOT NULL DEFAULT 0,

    created_at TEXT DEFAULT (datetime('now')) NOT NULL
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_user_quotas_user ON user_quotas(user_id);
CREATE INDEX IF NOT EXISTS idx_workflow_quotas_workflow ON workflow_quotas(workflow_id);
CREATE INDEX IF NOT EXISTS idx_quota_usage_history_user ON quota_usage_history(user_id, time_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_quota_usage_history_workflow ON quota_usage_history(workflow_id, time_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_quota_usage_history_time ON quota_usage_history(time_bucket DESC);
