-- Webhooks table for event-driven workflow triggering (SQLite)
CREATE TABLE IF NOT EXISTS webhooks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    workflow_id TEXT NOT NULL,
    secret TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    event_types TEXT NOT NULL DEFAULT '[]',
    required_headers TEXT NOT NULL DEFAULT '{}',
    ip_whitelist TEXT NOT NULL DEFAULT '[]',
    max_body_size INTEGER NOT NULL DEFAULT 1048576,
    timeout_seconds INTEGER NOT NULL DEFAULT 300,
    owner_id TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    last_triggered_at TEXT,
    trigger_count INTEGER NOT NULL DEFAULT 0,
    failed_count INTEGER NOT NULL DEFAULT 0,

    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);

-- Index for faster lookups
CREATE INDEX IF NOT EXISTS idx_webhooks_workflow ON webhooks(workflow_id);
CREATE INDEX IF NOT EXISTS idx_webhooks_owner ON webhooks(owner_id);
CREATE INDEX IF NOT EXISTS idx_webhooks_enabled ON webhooks(enabled);

-- Webhook events table for tracking incoming events
CREATE TABLE IF NOT EXISTS webhook_events (
    id TEXT PRIMARY KEY,
    webhook_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    headers TEXT NOT NULL,
    source_ip TEXT NOT NULL,
    received_at TEXT DEFAULT (datetime('now')) NOT NULL,
    processed_at TEXT,
    status TEXT NOT NULL DEFAULT 'PENDING',
    execution_id TEXT,
    error_message TEXT,

    FOREIGN KEY (webhook_id) REFERENCES webhooks(id) ON DELETE CASCADE
);

-- Indexes for event queries
CREATE INDEX IF NOT EXISTS idx_webhook_events_webhook ON webhook_events(webhook_id, received_at DESC);
CREATE INDEX IF NOT EXISTS idx_webhook_events_status ON webhook_events(status, received_at DESC);
CREATE INDEX IF NOT EXISTS idx_webhook_events_execution ON webhook_events(execution_id);
