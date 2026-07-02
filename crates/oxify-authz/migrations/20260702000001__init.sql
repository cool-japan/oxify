-- OxiFY Authorization Engine Schema (SQLite)
-- Google Zanzibar-style ReBAC implementation

-- Relation tuples table (core data structure)
CREATE TABLE IF NOT EXISTS authz_relation_tuples (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Object being protected
    namespace TEXT NOT NULL,
    object_id TEXT NOT NULL,
    relation TEXT NOT NULL,

    -- Subject (who has the relation)
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    subject_relation TEXT,

    -- Metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,

    -- Unique constraint: each tuple is unique
    CONSTRAINT unique_tuple UNIQUE (namespace, object_id, relation, subject_type, subject_id, subject_relation)
);

-- Indexes for fast lookups
CREATE INDEX IF NOT EXISTS idx_tuples_object
    ON authz_relation_tuples(namespace, object_id, relation);

CREATE INDEX IF NOT EXISTS idx_tuples_subject
    ON authz_relation_tuples(subject_type, subject_id);

CREATE INDEX IF NOT EXISTS idx_tuples_userset
    ON authz_relation_tuples(namespace, object_id);

CREATE INDEX IF NOT EXISTS idx_tuples_active
    ON authz_relation_tuples(namespace, relation);

-- Namespace configurations table
CREATE TABLE IF NOT EXISTS authz_namespaces (
    name TEXT PRIMARY KEY,
    config TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Audit log for all authorization decisions
CREATE TABLE IF NOT EXISTS authz_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,

    -- What was checked
    namespace TEXT NOT NULL,
    object_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,

    -- Decision
    allowed INTEGER NOT NULL,
    cached INTEGER NOT NULL,
    depth INTEGER NOT NULL,
    latency_ms INTEGER NOT NULL,

    -- Context
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    ip_address TEXT,
    user_agent TEXT,
    metadata TEXT
);

-- Index for audit log queries
CREATE INDEX IF NOT EXISTS idx_audit_timestamp
    ON authz_audit_log(timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_audit_subject
    ON authz_audit_log(subject_id, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_audit_object
    ON authz_audit_log(namespace, object_id, timestamp DESC);

-- Insert default namespaces
INSERT OR IGNORE INTO authz_namespaces (name, config) VALUES
('document', '{"relations": {"owner": {"inherits": []}, "editor": {"inherits": ["owner"]}, "viewer": {"inherits": ["owner", "editor"]}}}'),
('folder', '{"relations": {"parent": {"inherits": []}, "owner": {"inherits": []}, "viewer": {"inherits": ["owner"]}}}'),
('team', '{"relations": {"admin": {"inherits": []}, "member": {"inherits": ["admin"]}}}');
