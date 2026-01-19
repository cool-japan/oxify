-- Execution Metrics tables for analytics and performance tracking (SQLite)
-- Provides time-series data for workflow and execution analysis

-- Execution metrics aggregated by time bucket
CREATE TABLE IF NOT EXISTS execution_metrics (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,

    -- Time bucket (hourly aggregation)
    time_bucket TEXT NOT NULL,

    -- Execution counts
    total_executions INTEGER NOT NULL DEFAULT 0,
    successful_executions INTEGER NOT NULL DEFAULT 0,
    failed_executions INTEGER NOT NULL DEFAULT 0,
    cancelled_executions INTEGER NOT NULL DEFAULT 0,

    -- Duration statistics (in milliseconds)
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    min_duration_ms INTEGER,
    max_duration_ms INTEGER,
    avg_duration_ms INTEGER,
    p50_duration_ms INTEGER,
    p95_duration_ms INTEGER,
    p99_duration_ms INTEGER,

    -- Node statistics
    total_nodes_executed INTEGER NOT NULL DEFAULT 0,
    total_node_failures INTEGER NOT NULL DEFAULT 0,
    total_retries INTEGER NOT NULL DEFAULT 0,

    -- Token usage (for LLM workflows)
    total_input_tokens INTEGER NOT NULL DEFAULT 0,
    total_output_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd REAL,

    -- Created timestamp
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,

    -- Unique constraint per workflow per time bucket
    CONSTRAINT execution_metrics_workflow_time_unique UNIQUE (workflow_id, time_bucket)
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_execution_metrics_workflow ON execution_metrics(workflow_id, time_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_execution_metrics_time ON execution_metrics(time_bucket DESC);

-- Node-level metrics for performance analysis
CREATE TABLE IF NOT EXISTS node_metrics (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    node_type TEXT NOT NULL,

    -- Time bucket (hourly aggregation)
    time_bucket TEXT NOT NULL,

    -- Execution counts
    total_executions INTEGER NOT NULL DEFAULT 0,
    successful_executions INTEGER NOT NULL DEFAULT 0,
    failed_executions INTEGER NOT NULL DEFAULT 0,

    -- Duration statistics (in milliseconds)
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    min_duration_ms INTEGER,
    max_duration_ms INTEGER,
    avg_duration_ms INTEGER,

    -- Retry statistics
    total_retries INTEGER NOT NULL DEFAULT 0,
    max_retries_single_execution INTEGER,

    -- Token usage (for LLM nodes)
    total_input_tokens INTEGER NOT NULL DEFAULT 0,
    total_output_tokens INTEGER NOT NULL DEFAULT 0,

    -- Created timestamp
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,

    -- Unique constraint per workflow/node per time bucket
    CONSTRAINT node_metrics_workflow_node_time_unique UNIQUE (workflow_id, node_id, time_bucket)
);

-- Indexes for node metrics
CREATE INDEX IF NOT EXISTS idx_node_metrics_workflow ON node_metrics(workflow_id, time_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_node_metrics_node ON node_metrics(node_id, time_bucket DESC);
CREATE INDEX IF NOT EXISTS idx_node_metrics_type ON node_metrics(node_type, time_bucket DESC);

-- System-wide metrics snapshot
CREATE TABLE IF NOT EXISTS system_metrics (
    id TEXT PRIMARY KEY,

    -- Time of snapshot
    timestamp TEXT DEFAULT (datetime('now')) NOT NULL,

    -- Workflow counts
    total_workflows INTEGER NOT NULL DEFAULT 0,
    active_workflows INTEGER NOT NULL DEFAULT 0,

    -- Execution counts
    total_executions INTEGER NOT NULL DEFAULT 0,
    active_executions INTEGER NOT NULL DEFAULT 0,
    executions_last_hour INTEGER NOT NULL DEFAULT 0,
    executions_last_day INTEGER NOT NULL DEFAULT 0,

    -- User counts
    total_users INTEGER NOT NULL DEFAULT 0,
    active_users_last_hour INTEGER NOT NULL DEFAULT 0,
    active_users_last_day INTEGER NOT NULL DEFAULT 0,

    -- Resource usage
    total_api_keys INTEGER NOT NULL DEFAULT 0,
    total_secrets INTEGER NOT NULL DEFAULT 0,
    total_schedules INTEGER NOT NULL DEFAULT 0,
    total_webhooks INTEGER NOT NULL DEFAULT 0,

    -- Storage usage
    total_storage_bytes INTEGER,
    audit_logs_count INTEGER NOT NULL DEFAULT 0,

    -- Performance indicators
    avg_execution_duration_ms INTEGER,
    success_rate_percent REAL,

    -- Metadata
    metadata TEXT NOT NULL DEFAULT '{}'
);

-- Index for system metrics
CREATE INDEX IF NOT EXISTS idx_system_metrics_timestamp ON system_metrics(timestamp DESC);
