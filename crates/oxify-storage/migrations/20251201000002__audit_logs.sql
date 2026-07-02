-- General Audit Log table for system-wide activity tracking (SQLite)
-- Captures workflow operations, execution events, user actions, and security events

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,

    -- Event classification
    event_type TEXT NOT NULL,
    event_category TEXT NOT NULL,

    -- Actor information
    actor_id TEXT,
    actor_type TEXT NOT NULL DEFAULT 'user',
    actor_ip TEXT,

    -- Target resource
    resource_type TEXT,
    resource_id TEXT,

    -- Event details
    action TEXT NOT NULL,
    description TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',

    -- Request context
    request_id TEXT,
    session_id TEXT,

    -- Result
    success INTEGER NOT NULL DEFAULT 1,
    error_code TEXT,
    error_message TEXT,

    -- Timing
    timestamp TEXT DEFAULT (datetime('now')) NOT NULL,
    duration_ms INTEGER,

    -- Retention policy
    retention_days INTEGER DEFAULT 90
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type ON audit_logs(event_type, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_event_category ON audit_logs(event_category, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_actor ON audit_logs(actor_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_resource ON audit_logs(resource_type, resource_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_request ON audit_logs(request_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_success ON audit_logs(success, timestamp DESC);
