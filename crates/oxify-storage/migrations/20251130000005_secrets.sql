-- Secrets table for secure credential storage (SQLite)
CREATE TABLE IF NOT EXISTS secrets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    encrypted_value BLOB NOT NULL,

    -- Encryption metadata
    encryption_algorithm TEXT NOT NULL,
    encryption_kdf TEXT NOT NULL,
    encryption_salt TEXT NOT NULL,
    encryption_iv TEXT NOT NULL,
    encryption_key_version INTEGER NOT NULL DEFAULT 1,

    -- Metadata
    tags TEXT NOT NULL DEFAULT '[]',
    owner_id TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    last_accessed_at TEXT,
    expires_at TEXT,

    -- Access control
    allowed_workflows TEXT NOT NULL DEFAULT '[]',
    allowed_users TEXT NOT NULL DEFAULT '[]',
    ip_whitelist TEXT NOT NULL DEFAULT '[]',
    require_mfa INTEGER NOT NULL DEFAULT 0,

    -- Ensure unique name per owner
    CONSTRAINT secrets_name_owner_unique UNIQUE (name, owner_id)
);

-- Index for faster lookups by owner
CREATE INDEX IF NOT EXISTS idx_secrets_owner ON secrets(owner_id);

-- Index for expiration queries
CREATE INDEX IF NOT EXISTS idx_secrets_expires ON secrets(expires_at);

-- Secret audit log table
CREATE TABLE IF NOT EXISTS secret_audit_logs (
    id TEXT PRIMARY KEY,
    secret_id TEXT NOT NULL,
    user_id TEXT,
    workflow_id TEXT,
    action TEXT NOT NULL,
    ip_address TEXT,
    success INTEGER NOT NULL,
    error_message TEXT,
    timestamp TEXT DEFAULT (datetime('now')) NOT NULL,

    FOREIGN KEY (secret_id) REFERENCES secrets(id) ON DELETE CASCADE
);

-- Index for audit log queries
CREATE INDEX IF NOT EXISTS idx_secret_audit_logs_secret ON secret_audit_logs(secret_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_secret_audit_logs_user ON secret_audit_logs(user_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_secret_audit_logs_workflow ON secret_audit_logs(workflow_id, timestamp DESC);
