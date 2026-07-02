-- Workflow version history table (SQLite)
CREATE TABLE IF NOT EXISTS workflow_versions (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    description TEXT,
    definition TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    created_by TEXT,

    -- Constraints
    CONSTRAINT workflow_versions_workflow_version_unique UNIQUE (workflow_id, version),
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);

-- Index for faster version lookups
CREATE INDEX IF NOT EXISTS idx_workflow_versions_workflow_id
    ON workflow_versions(workflow_id);

-- Index for version ordering
CREATE INDEX IF NOT EXISTS idx_workflow_versions_created_at
    ON workflow_versions(created_at DESC);
