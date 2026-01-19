-- API Keys table for LLM provider credential storage (SQLite)
-- Supports OpenAI, Anthropic, Ollama, and other providers

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,

    -- Provider information
    provider TEXT NOT NULL CHECK (provider IN ('openai', 'anthropic', 'ollama', 'azure_openai', 'cohere', 'huggingface', 'custom')),
    provider_url TEXT,

    -- Encrypted API key
    encrypted_key BLOB NOT NULL,
    encryption_algorithm TEXT NOT NULL,
    encryption_kdf TEXT NOT NULL,
    encryption_salt TEXT NOT NULL,
    encryption_iv TEXT NOT NULL,
    encryption_key_version INTEGER NOT NULL DEFAULT 1,

    -- Metadata
    owner_id TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    last_used_at TEXT,
    expires_at TEXT,

    -- Usage tracking
    usage_count INTEGER NOT NULL DEFAULT 0,
    usage_limit INTEGER,

    -- Status
    is_active INTEGER NOT NULL DEFAULT 1,
    is_default INTEGER NOT NULL DEFAULT 0,

    -- Access control
    allowed_workflows TEXT NOT NULL DEFAULT '[]',
    allowed_users TEXT NOT NULL DEFAULT '[]',

    -- Rate limiting
    rate_limit_per_minute INTEGER,

    -- Ensure unique name per owner
    CONSTRAINT api_keys_name_owner_unique UNIQUE (name, owner_id)
);

-- Index for faster lookups
CREATE INDEX IF NOT EXISTS idx_api_keys_owner ON api_keys(owner_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_provider ON api_keys(provider);
CREATE INDEX IF NOT EXISTS idx_api_keys_active ON api_keys(is_active);
CREATE INDEX IF NOT EXISTS idx_api_keys_default ON api_keys(owner_id, provider, is_default);
CREATE INDEX IF NOT EXISTS idx_api_keys_expires ON api_keys(expires_at);

-- API key usage log table for audit trail
CREATE TABLE IF NOT EXISTS api_key_usage_logs (
    id TEXT PRIMARY KEY,
    api_key_id TEXT NOT NULL,
    workflow_id TEXT,
    execution_id TEXT,

    -- Usage details
    model TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    total_tokens INTEGER,
    estimated_cost REAL,

    -- Request metadata
    request_type TEXT,
    success INTEGER NOT NULL,
    error_message TEXT,
    latency_ms INTEGER,

    timestamp TEXT DEFAULT (datetime('now')) NOT NULL,

    FOREIGN KEY (api_key_id) REFERENCES api_keys(id) ON DELETE CASCADE
);

-- Indexes for usage log queries
CREATE INDEX IF NOT EXISTS idx_api_key_usage_logs_key ON api_key_usage_logs(api_key_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_api_key_usage_logs_workflow ON api_key_usage_logs(workflow_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_api_key_usage_logs_execution ON api_key_usage_logs(execution_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_api_key_usage_logs_timestamp ON api_key_usage_logs(timestamp DESC);
