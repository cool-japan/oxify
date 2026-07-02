-- Create execution_checkpoints table for pause/resume functionality (SQLite)
CREATE TABLE IF NOT EXISTS execution_checkpoints (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    execution_id TEXT NOT NULL,
    context TEXT NOT NULL,
    completed_nodes TEXT NOT NULL DEFAULT '[]',
    node_results TEXT NOT NULL DEFAULT '{}',
    current_level INTEGER NOT NULL DEFAULT 0,
    paused INTEGER NOT NULL DEFAULT 0,
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (execution_id) REFERENCES executions(id) ON DELETE CASCADE
);

-- Create indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_checkpoints_execution_id ON execution_checkpoints(execution_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_workflow_id ON execution_checkpoints(workflow_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_created_at ON execution_checkpoints(created_at DESC);
