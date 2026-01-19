-- OxiFY Database Schema (SQLite)
-- Initial migration for workflow orchestration storage

-- Workflows table
CREATE TABLE IF NOT EXISTS workflows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    version INTEGER NOT NULL DEFAULT 1,
    -- Store the entire workflow as JSON for flexibility
    definition TEXT NOT NULL,
    -- Indexes for search (stored as JSON array)
    tags TEXT DEFAULT '[]'
);

-- Workflow executions table
CREATE TABLE IF NOT EXISTS executions (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    state TEXT NOT NULL CHECK (state IN ('Running', 'Completed', 'Failed', 'Cancelled', 'Paused')),
    -- Store execution context as JSON
    context TEXT NOT NULL,
    -- Store node results as JSON
    node_results TEXT NOT NULL DEFAULT '{}',
    -- Store variables as JSON
    variables TEXT NOT NULL DEFAULT '{}',
    error_message TEXT
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_workflows_name ON workflows(name);
CREATE INDEX IF NOT EXISTS idx_workflows_created_at ON workflows(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_executions_workflow_id ON executions(workflow_id);
CREATE INDEX IF NOT EXISTS idx_executions_started_at ON executions(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_executions_state ON executions(state);
CREATE INDEX IF NOT EXISTS idx_executions_completed_at ON executions(completed_at DESC);
