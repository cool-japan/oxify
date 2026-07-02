//! Database MCP server - provides SQL database operations
//!
//! This module implements a Model Context Protocol server for database operations,
//! supporting SQLite queries, commands, and transactions.
//!
//! # Features
//!
//! Enable the `database` feature to use OxiSQL-backed database operations (the
//! Pure-Rust `oxisql-core` / `oxisql-sqlite-compat` / `oxisql-pool` stack, Limbo
//! engine):
//!
//! ```toml
//! oxify-mcp = { version = "0.1", features = ["database"] }
//! ```
//!
//! # Placeholder syntax
//!
//! Callers may write parameterised SQL using either the SQLite-style bare `?`
//! placeholder or OxiSQL's native `$1`, `$2`, … numbered form. Every caller-
//! supplied `?` is renumbered to `$N` at runtime by
//! [`rewrite_question_placeholders`] before the statement reaches the engine,
//! because `oxisql-sqlite-compat` only recognises `$N` placeholders — a bare `?`
//! would otherwise be passed through and silently bind no value.

use crate::{McpServer, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[cfg(feature = "database")]
use oxisql_core::{Connection, ToSqlValue, Value as OxiValue};
#[cfg(feature = "database")]
use oxisql_pool::sqlite::{new_sqlite_compat_pool_with_config, SqliteCompatPool};
#[cfg(feature = "database")]
use oxisql_pool::PoolConfig;

/// Database type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum DatabaseType {
    /// PostgreSQL database (not supported)
    Postgres,
    /// MySQL database (not supported)
    Mysql,
    /// SQLite database
    #[default]
    Sqlite,
}

/// Configuration for the database server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database connection string
    pub connection_string: String,
    /// Database type
    pub db_type: DatabaseType,
    /// Maximum number of connections in the pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Whether to enable read-only mode (disables mutations)
    #[serde(default)]
    pub read_only: bool,
    /// Maximum rows to return from queries
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
}

fn default_max_connections() -> u32 {
    5
}

fn default_timeout() -> u64 {
    30
}

fn default_max_rows() -> usize {
    1000
}

impl DatabaseConfig {
    /// Create a new SQLite configuration
    pub fn sqlite(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
            db_type: DatabaseType::Sqlite,
            max_connections: default_max_connections(),
            timeout_secs: default_timeout(),
            read_only: false,
            max_rows: default_max_rows(),
        }
    }

    /// Create a new PostgreSQL configuration (deprecated, SQLite is now default)
    #[deprecated(note = "PostgreSQL is no longer supported. Use sqlite() instead.")]
    pub fn postgres(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
            db_type: DatabaseType::Postgres,
            max_connections: default_max_connections(),
            timeout_secs: default_timeout(),
            read_only: false,
            max_rows: default_max_rows(),
        }
    }

    /// Set the maximum number of connections
    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Set read-only mode
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Set maximum rows to return
    pub fn with_max_rows(mut self, max_rows: usize) -> Self {
        self.max_rows = max_rows;
        self
    }
}

/// Query result from database operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Column names
    pub columns: Vec<String>,
    /// Rows as JSON arrays
    pub rows: Vec<Vec<Value>>,
    /// Number of rows returned
    pub row_count: usize,
    /// Whether results were truncated due to max_rows limit
    pub truncated: bool,
}

/// Execute result from database commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// Number of rows affected
    pub rows_affected: u64,
}

/// Transaction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    /// Results from each statement
    pub statement_results: Vec<StatementResult>,
    /// Whether the transaction was committed
    pub committed: bool,
}

/// Result from a single statement in a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatementResult {
    /// Statement index (0-based)
    pub index: usize,
    /// Number of rows affected (for execute) or returned (for query)
    pub rows_affected: u64,
    /// Error message if statement failed
    pub error: Option<String>,
}

/// Built-in MCP server for database operations
#[cfg(feature = "database")]
pub struct DatabaseServer {
    /// Database configuration
    config: DatabaseConfig,
    /// Connection pool (Pure-Rust OxiSQL SQLite pool, Limbo backend)
    pool: SqliteCompatPool,
}

#[cfg(not(feature = "database"))]
pub struct DatabaseServer {
    /// Database configuration
    #[allow(dead_code)]
    config: DatabaseConfig,
}

impl DatabaseServer {
    /// Create a new database server (async, requires database feature)
    #[cfg(feature = "database")]
    pub async fn new(config: DatabaseConfig) -> Result<Self> {
        match config.db_type {
            DatabaseType::Sqlite => {
                // OxiSQL's SQLite backend opens a plain filesystem path (or
                // `":memory:"`) rather than a `sqlite:` URL, so normalise the
                // configured connection string first.
                let path = connection_string_to_path(&config.connection_string);

                // `connect_timeout_ms` preserves the original acquire-timeout
                // intent; `min_idle` / `idle_timeout_ms` are left at their
                // defaults since the MCP server does not expose them.
                let pool_config = PoolConfig {
                    max_size: config.max_connections as usize,
                    min_idle: None,
                    connect_timeout_ms: Some(config.timeout_secs.saturating_mul(1000)),
                    idle_timeout_ms: None,
                };

                let pool = new_sqlite_compat_pool_with_config(path.as_str(), pool_config)
                    .await
                    .map_err(|e| {
                        crate::McpError::ToolExecutionError(format!(
                            "Failed to connect to database: {e}"
                        ))
                    })?;

                Ok(Self { config, pool })
            }
            DatabaseType::Mysql | DatabaseType::Postgres => {
                Err(crate::McpError::ToolExecutionError(format!(
                    "{:?} is not yet supported. Only SQLite is currently implemented.",
                    config.db_type
                )))
            }
        }
    }

    /// Create a new database server (stub without database feature)
    #[cfg(not(feature = "database"))]
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

    /// Create from an existing pool (useful for testing)
    #[cfg(feature = "database")]
    pub fn from_pool(pool: SqliteCompatPool, config: DatabaseConfig) -> Self {
        Self { config, pool }
    }

    /// Check if the server is in read-only mode
    #[allow(dead_code)]
    fn is_read_only(&self) -> bool {
        self.config.read_only
    }

    /// Check if a SQL statement is a mutation (INSERT, UPDATE, DELETE, etc.)
    #[cfg_attr(not(feature = "database"), allow(dead_code))]
    fn is_mutation(sql: &str) -> bool {
        let sql_upper = sql.trim().to_uppercase();
        sql_upper.starts_with("INSERT")
            || sql_upper.starts_with("UPDATE")
            || sql_upper.starts_with("DELETE")
            || sql_upper.starts_with("DROP")
            || sql_upper.starts_with("CREATE")
            || sql_upper.starts_with("ALTER")
            || sql_upper.starts_with("TRUNCATE")
    }
}

#[cfg(feature = "database")]
#[async_trait]
impl McpServer for DatabaseServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "db_query" => {
                let sql_str = arguments
                    .get("sql")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        crate::McpError::InvalidArgument("sql is required".to_string())
                    })?;

                // Check for mutations in read-only mode
                if self.config.read_only && Self::is_mutation(sql_str) {
                    return Err(crate::McpError::ToolExecutionError(
                        "Mutation queries are not allowed in read-only mode".to_string(),
                    ));
                }

                // Renumber caller-supplied `?` placeholders to OxiSQL's `$N`
                // form, then bind any provided positional params.
                let sql = rewrite_question_placeholders(sql_str);
                let params = extract_params(&arguments);
                let param_refs: Vec<&dyn ToSqlValue> =
                    params.iter().map(|v| v as &dyn ToSqlValue).collect();

                let conn = self.pool.get().await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to acquire connection: {e}"
                    ))
                })?;

                // Execute the query
                let rows = conn.query(&sql, &param_refs).await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!("Query failed: {e}"))
                })?;

                // Extract column names from the first row (if any)
                let columns: Vec<String> = rows
                    .first()
                    .map(|row| row.columns().to_vec())
                    .unwrap_or_default();

                // Convert rows to JSON, honouring the max_rows truncation limit.
                let truncated = rows.len() > self.config.max_rows;
                let mut result_rows = Vec::new();
                for row in rows.iter().take(self.config.max_rows) {
                    result_rows.push(row_to_json(row));
                }

                let result = QueryResult {
                    columns,
                    row_count: result_rows.len(),
                    rows: result_rows,
                    truncated,
                };

                serde_json::to_value(result).map_err(|e| {
                    crate::McpError::ToolExecutionError(format!("Failed to serialize result: {e}"))
                })
            }

            "db_execute" => {
                if self.config.read_only {
                    return Err(crate::McpError::ToolExecutionError(
                        "Execute commands are not allowed in read-only mode".to_string(),
                    ));
                }

                let sql_str = arguments
                    .get("sql")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        crate::McpError::InvalidArgument("sql is required".to_string())
                    })?;

                let sql = rewrite_question_placeholders(sql_str);
                let params = extract_params(&arguments);
                let param_refs: Vec<&dyn ToSqlValue> =
                    params.iter().map(|v| v as &dyn ToSqlValue).collect();

                let conn = self.pool.get().await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to acquire connection: {e}"
                    ))
                })?;

                let rows_affected = conn.execute(&sql, &param_refs).await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!("Execute failed: {e}"))
                })?;

                let exec_result = ExecuteResult { rows_affected };

                serde_json::to_value(exec_result).map_err(|e| {
                    crate::McpError::ToolExecutionError(format!("Failed to serialize result: {e}"))
                })
            }

            "db_transaction" => {
                if self.config.read_only {
                    return Err(crate::McpError::ToolExecutionError(
                        "Transactions are not allowed in read-only mode".to_string(),
                    ));
                }

                let statements = arguments
                    .get("statements")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        crate::McpError::InvalidArgument(
                            "statements is required and must be an array".to_string(),
                        )
                    })?;

                // OxiSQL transactions are borrowed from a checked-out
                // connection: hold the connection for the duration of the
                // transaction so the borrow stays valid through commit/rollback.
                let conn = self.pool.get().await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to acquire connection: {e}"
                    ))
                })?;

                let mut tx = conn.transaction().await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!("Failed to start transaction: {e}"))
                })?;

                let mut statement_results = Vec::new();

                for (index, stmt) in statements.iter().enumerate() {
                    let sql_str = stmt.get("sql").and_then(|v| v.as_str()).ok_or_else(|| {
                        crate::McpError::InvalidArgument(format!(
                            "Statement {index} is missing sql field"
                        ))
                    })?;

                    // Each statement is caller-supplied SQL: renumber `?`
                    // placeholders and bind any per-statement params.
                    let sql = rewrite_question_placeholders(sql_str);
                    let params = extract_params(stmt);
                    let param_refs: Vec<&dyn ToSqlValue> =
                        params.iter().map(|v| v as &dyn ToSqlValue).collect();

                    match tx.execute(&sql, &param_refs).await {
                        Ok(rows_affected) => {
                            statement_results.push(StatementResult {
                                index,
                                rows_affected,
                                error: None,
                            });
                        }
                        Err(e) => {
                            // Rollback on error
                            let _ = tx.rollback().await;

                            statement_results.push(StatementResult {
                                index,
                                rows_affected: 0,
                                error: Some(e.to_string()),
                            });

                            let result = TransactionResult {
                                statement_results,
                                committed: false,
                            };

                            return serde_json::to_value(result).map_err(|e| {
                                crate::McpError::ToolExecutionError(format!(
                                    "Failed to serialize result: {e}"
                                ))
                            });
                        }
                    }
                }

                // Commit transaction
                tx.commit().await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to commit transaction: {e}"
                    ))
                })?;

                let result = TransactionResult {
                    statement_results,
                    committed: true,
                };

                serde_json::to_value(result).map_err(|e| {
                    crate::McpError::ToolExecutionError(format!("Failed to serialize result: {e}"))
                })
            }

            "db_describe" => {
                let table = arguments
                    .get("table")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        crate::McpError::InvalidArgument("table is required".to_string())
                    })?;

                // Get column information using SQLite's PRAGMA. Rows are read
                // positionally (0=cid, 1=name, 2=type, 3=notnull, 4=dflt_value,
                // 5=pk) because the SQLite backend does not guarantee column
                // names on PRAGMA result sets.
                let sql = format!("PRAGMA table_info({table})");

                let conn = self.pool.get().await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to acquire connection: {e}"
                    ))
                })?;

                let rows = conn.query(&sql, &[]).await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!("Describe failed: {e}"))
                })?;

                let mut columns: Vec<Value> = Vec::with_capacity(rows.len());
                for row in &rows {
                    let name = match row.get_by_index(1) {
                        Some(OxiValue::Text(s)) => s.clone(),
                        _ => String::new(),
                    };
                    let data_type = match row.get_by_index(2) {
                        Some(OxiValue::Text(s)) => s.clone(),
                        _ => String::new(),
                    };
                    let not_null = matches!(row.get_by_index(3), Some(OxiValue::I64(n)) if *n != 0);
                    let default = match row.get_by_index(4) {
                        Some(OxiValue::Text(s)) => Some(s.clone()),
                        _ => None,
                    };
                    columns.push(json!({
                        "column_name": name,
                        "data_type": data_type,
                        "is_nullable": if not_null { "NO" } else { "YES" },
                        "column_default": default,
                    }));
                }

                Ok(json!({
                    "table": table,
                    "columns": columns
                }))
            }

            "db_tables" => {
                let _schema = arguments
                    .get("schema")
                    .and_then(|v| v.as_str())
                    .unwrap_or("main");

                // SQLite uses sqlite_master instead of information_schema
                let sql = "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name";

                let conn = self.pool.get().await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to acquire connection: {e}"
                    ))
                })?;

                let rows = conn.query(sql, &[]).await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!("List tables failed: {e}"))
                })?;

                let tables: Vec<String> = rows
                    .iter()
                    .filter_map(|row| match row.get_by_index(0) {
                        Some(OxiValue::Text(s)) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();

                Ok(json!({
                    "schema": "main",
                    "tables": tables
                }))
            }

            _ => Err(crate::McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "db_query",
                "description": "Execute a SQL SELECT query and return results as JSON",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sql": {
                            "type": "string",
                            "description": "SQL query to execute (SELECT statements). Use `?` or `$N` placeholders for bound parameters."
                        },
                        "params": {
                            "type": "array",
                            "description": "Optional positional parameters bound to the query's placeholders, in order",
                            "items": {}
                        }
                    },
                    "required": ["sql"]
                }
            }),
            json!({
                "name": "db_execute",
                "description": "Execute a SQL command (INSERT, UPDATE, DELETE) and return rows affected",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sql": {
                            "type": "string",
                            "description": "SQL command to execute. Use `?` or `$N` placeholders for bound parameters."
                        },
                        "params": {
                            "type": "array",
                            "description": "Optional positional parameters bound to the command's placeholders, in order",
                            "items": {}
                        }
                    },
                    "required": ["sql"]
                }
            }),
            json!({
                "name": "db_transaction",
                "description": "Execute multiple SQL statements in a transaction (atomic)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "statements": {
                            "type": "array",
                            "description": "SQL statements to execute in the transaction",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "sql": { "type": "string" },
                                    "params": {
                                        "type": "array",
                                        "description": "Optional positional parameters for this statement",
                                        "items": {}
                                    }
                                },
                                "required": ["sql"]
                            }
                        }
                    },
                    "required": ["statements"]
                }
            }),
            json!({
                "name": "db_describe",
                "description": "Get schema information for a table (columns, types)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "table": {
                            "type": "string",
                            "description": "Name of the table to describe"
                        }
                    },
                    "required": ["table"]
                }
            }),
            json!({
                "name": "db_tables",
                "description": "List all tables in a schema",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "schema": {
                            "type": "string",
                            "description": "Schema name (default: public)",
                            "default": "public"
                        }
                    }
                }
            }),
        ])
    }
}

#[cfg(not(feature = "database"))]
#[async_trait]
impl McpServer for DatabaseServer {
    async fn call_tool(&self, name: &str, _arguments: Value) -> Result<Value> {
        match name {
            "db_query" | "db_execute" | "db_transaction" | "db_describe" | "db_tables" => {
                Err(crate::McpError::ToolExecutionError(
                    "Database operations require the 'database' feature. \
                     Enable it with: oxify-mcp = { version = \"0.1\", features = [\"database\"] }"
                        .to_string(),
                ))
            }
            _ => Err(crate::McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "db_query",
                "description": "Execute SQL query (requires 'database' feature)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sql": {
                            "type": "string",
                            "description": "SQL query to execute"
                        }
                    },
                    "required": ["sql"]
                }
            }),
            json!({
                "name": "db_execute",
                "description": "Execute SQL command (requires 'database' feature)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sql": {
                            "type": "string",
                            "description": "SQL command to execute"
                        }
                    },
                    "required": ["sql"]
                }
            }),
            json!({
                "name": "db_transaction",
                "description": "Execute transaction (requires 'database' feature)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "statements": {
                            "type": "array",
                            "description": "SQL statements in transaction",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "sql": { "type": "string" }
                                }
                            }
                        }
                    },
                    "required": ["statements"]
                }
            }),
            json!({
                "name": "db_describe",
                "description": "Describe table schema (requires 'database' feature)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "table": {
                            "type": "string",
                            "description": "Table name"
                        }
                    },
                    "required": ["table"]
                }
            }),
            json!({
                "name": "db_tables",
                "description": "List tables (requires 'database' feature)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "schema": {
                            "type": "string",
                            "description": "Schema name"
                        }
                    }
                }
            }),
        ])
    }
}

/// Convert an OxiSQL result row into the JSON array of column values that the
/// `db_query` tool contract expects.
///
/// OxiSQL rows are already a typed [`oxisql_core::Value`] enum, so this walks the
/// row's columns zipped with their values and maps each value to a
/// `serde_json::Value` — there is no separate `Column` / `TypeInfo` / `ValueRef`
/// layer to consult as there was with sqlx.
#[cfg(feature = "database")]
fn row_to_json(row: &oxisql_core::Row) -> Vec<Value> {
    let mut values = Vec::with_capacity(row.values().len());
    for (_col_name, value) in row.columns().iter().zip(row.values().iter()) {
        values.push(oxi_value_to_json(value));
    }
    values
}

/// Map a single [`oxisql_core::Value`] to a `serde_json::Value`, preserving the
/// output shapes the previous sqlx-based `extract_column_value` produced:
///
/// - integers → JSON numbers; reals → JSON numbers (non-finite → `null`),
/// - text that looks like JSON is parsed, otherwise emitted as a JSON string,
/// - blobs → base64-encoded JSON strings.
///
/// The SQLite-atypical extended variants (timestamp / date / time / uuid /
/// decimal / array) have no dedicated shape in the original contract, so they
/// fall back to their canonical `Display` string — mirroring the old
/// "unknown type → string" arm. `Json` values are parsed back into structured
/// JSON (falling back to a string on malformed input), matching the text path's
/// JSON detection.
#[cfg(feature = "database")]
fn oxi_value_to_json(value: &OxiValue) -> Value {
    match value {
        OxiValue::Null => Value::Null,
        OxiValue::Bool(b) => Value::Bool(*b),
        OxiValue::I64(n) => Value::Number((*n).into()),
        OxiValue::F64(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        OxiValue::Text(s) => text_to_json(s),
        OxiValue::Blob(b) => {
            Value::String(base64::Engine::encode(&base64::prelude::BASE64_STANDARD, b))
        }
        OxiValue::Json(s) => {
            serde_json::from_str::<Value>(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        OxiValue::Timestamp(_)
        | OxiValue::Date(_)
        | OxiValue::Time(_)
        | OxiValue::Uuid(_)
        | OxiValue::Decimal(_)
        | OxiValue::Array(_)
        | OxiValue::TypedArray { .. } => Value::String(value.to_string()),
    }
}

/// Reproduce the original TEXT handling: if the string looks like a JSON object
/// or array and parses cleanly, surface the parsed JSON; otherwise a plain
/// string.
#[cfg(feature = "database")]
fn text_to_json(s: &str) -> Value {
    if s.starts_with('{') || s.starts_with('[') {
        if let Ok(json_val) = serde_json::from_str::<Value>(s) {
            return json_val;
        }
    }
    Value::String(s.to_string())
}

/// Rewrite bare `?` positional placeholders in caller-supplied SQL to OxiSQL's
/// canonical `$1`, `$2`, … numbered form, in order of appearance.
///
/// `oxisql-sqlite-compat` recognises only `$N` placeholders; a bare `?` is passed
/// straight through to the engine where it silently binds no value, corrupting
/// results. Because the SQL is caller-supplied (not known ahead of time) a static
/// renumbering is impossible, so this runs at request time. `?` characters inside
/// single-quoted string literals, double-quoted identifiers, and `--` / `/* */`
/// comments are left untouched.
#[cfg(feature = "database")]
fn rewrite_question_placeholders(sql: &str) -> String {
    // Operate on `char`s (not bytes) so multi-byte UTF-8 inside string literals
    // is copied through intact.
    let chars: Vec<char> = sql.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut i = 0usize;
    let mut next_index = 1usize;

    while i < n {
        match chars[i] {
            // Single-quoted string literal — copy verbatim, honouring the SQL
            // `''` escape sequence.
            '\'' => {
                out.push('\'');
                i += 1;
                while i < n {
                    let c = chars[i];
                    out.push(c);
                    i += 1;
                    if c == '\'' {
                        if i < n && chars[i] == '\'' {
                            out.push('\'');
                            i += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
            // Double-quoted identifier — copy verbatim.
            '"' => {
                out.push('"');
                i += 1;
                while i < n {
                    let c = chars[i];
                    out.push(c);
                    i += 1;
                    if c == '"' {
                        break;
                    }
                }
            }
            // `--` line comment — copy to end of line.
            '-' if i + 1 < n && chars[i + 1] == '-' => {
                out.push('-');
                out.push('-');
                i += 2;
                while i < n && chars[i] != '\n' {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            // `/* ... */` block comment — copy to the closing `*/`.
            '/' if i + 1 < n && chars[i + 1] == '*' => {
                out.push('/');
                out.push('*');
                i += 2;
                while i < n {
                    if chars[i] == '*' && i + 1 < n && chars[i + 1] == '/' {
                        out.push('*');
                        out.push('/');
                        i += 2;
                        break;
                    }
                    out.push(chars[i]);
                    i += 1;
                }
            }
            // A live placeholder — renumber to `$N`.
            '?' => {
                out.push('$');
                out.push_str(&next_index.to_string());
                next_index += 1;
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }

    out
}

/// Extract optional positional parameters from an MCP tool argument object.
///
/// Reads the `params` array (if present) and converts each JSON element to an
/// [`oxisql_core::Value`] via [`json_to_sql_value`]. Absent or non-array `params`
/// yields an empty parameter list.
#[cfg(feature = "database")]
fn extract_params(arguments: &Value) -> Vec<OxiValue> {
    arguments
        .get("params")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(json_to_sql_value).collect())
        .unwrap_or_default()
}

/// Convert a JSON parameter value to an [`oxisql_core::Value`] for binding.
///
/// Integers bind as `I64`, other numbers as `F64`, strings as `Text`, booleans
/// as `Bool`, and JSON `null` as `Null`. Structured arrays/objects are bound as
/// their serialised JSON text (suitable for JSON/TEXT columns).
#[cfg(feature = "database")]
fn json_to_sql_value(value: &Value) -> OxiValue {
    match value {
        Value::Null => OxiValue::Null,
        Value::Bool(b) => OxiValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                OxiValue::I64(i)
            } else if let Some(f) = n.as_f64() {
                OxiValue::F64(f)
            } else {
                // A u64 too large for i64 — preserve it losslessly as text.
                OxiValue::Text(n.to_string())
            }
        }
        Value::String(s) => OxiValue::Text(s.clone()),
        Value::Array(_) | Value::Object(_) => OxiValue::Text(value.to_string()),
    }
}

/// Normalise a database connection string into a path accepted by the OxiSQL
/// SQLite backend.
///
/// sqlx accepted `sqlite:`-scheme URLs with query parameters (e.g.
/// `sqlite:app.db?mode=rwc`), but `oxisql-sqlite-compat` opens a plain filesystem
/// path (or `":memory:"`). This strips the `sqlite:` / `sqlite://` scheme and any
/// `?query` suffix, mapping empty / `:memory:` forms to `":memory:"`.
#[cfg(feature = "database")]
fn connection_string_to_path(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .unwrap_or(url);
    let path = without_scheme.split('?').next().unwrap_or(without_scheme);
    if path.is_empty() || path == ":memory:" || path == "memory:" {
        ":memory:".to_string()
    } else {
        path.to_string()
    }
}

#[cfg(all(test, feature = "database"))]
mod tests {
    use super::*;

    #[test]
    fn test_database_config() {
        let config = DatabaseConfig::sqlite("sqlite:test.db")
            .with_max_connections(10)
            .with_read_only(true)
            .with_max_rows(500);

        assert_eq!(config.max_connections, 10);
        assert!(config.read_only);
        assert_eq!(config.max_rows, 500);
        assert_eq!(config.db_type, DatabaseType::Sqlite);
    }

    #[test]
    fn test_is_mutation() {
        assert!(DatabaseServer::is_mutation(
            "INSERT INTO users (name) VALUES ('test')"
        ));
        assert!(DatabaseServer::is_mutation(
            "UPDATE users SET name = 'test'"
        ));
        assert!(DatabaseServer::is_mutation(
            "DELETE FROM users WHERE id = 1"
        ));
        assert!(DatabaseServer::is_mutation("DROP TABLE users"));
        assert!(DatabaseServer::is_mutation("CREATE TABLE users (id INT)"));
        assert!(DatabaseServer::is_mutation(
            "ALTER TABLE users ADD COLUMN age INT"
        ));
        assert!(DatabaseServer::is_mutation("TRUNCATE TABLE users"));

        assert!(!DatabaseServer::is_mutation("SELECT * FROM users"));
        assert!(!DatabaseServer::is_mutation("  SELECT id FROM users"));
    }

    #[test]
    fn test_query_result_serialization() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                vec![Value::Number(1.into()), Value::String("Alice".to_string())],
                vec![Value::Number(2.into()), Value::String("Bob".to_string())],
            ],
            row_count: 2,
            truncated: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: QueryResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.columns, result.columns);
        assert_eq!(parsed.row_count, 2);
        assert!(!parsed.truncated);
    }

    #[test]
    fn test_execute_result_serialization() {
        let result = ExecuteResult { rows_affected: 5 };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ExecuteResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.rows_affected, 5);
    }

    #[test]
    fn test_transaction_result_serialization() {
        let result = TransactionResult {
            statement_results: vec![
                StatementResult {
                    index: 0,
                    rows_affected: 1,
                    error: None,
                },
                StatementResult {
                    index: 1,
                    rows_affected: 2,
                    error: None,
                },
            ],
            committed: true,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: TransactionResult = serde_json::from_str(&json).unwrap();

        assert!(parsed.committed);
        assert_eq!(parsed.statement_results.len(), 2);
    }

    #[test]
    fn test_database_type_default() {
        let db_type = DatabaseType::default();
        assert_eq!(db_type, DatabaseType::Sqlite);
    }

    // ── Runtime `?` → `$N` placeholder preprocessing ──────────────────────────

    #[test]
    fn test_rewrite_question_placeholders_none() {
        assert_eq!(
            rewrite_question_placeholders("SELECT * FROM t WHERE a = 1"),
            "SELECT * FROM t WHERE a = 1"
        );
    }

    #[test]
    fn test_rewrite_question_placeholders_multiple() {
        assert_eq!(
            rewrite_question_placeholders("INSERT INTO t (a, b, c) VALUES (?, ?, ?)"),
            "INSERT INTO t (a, b, c) VALUES ($1, $2, $3)"
        );
    }

    #[test]
    fn test_rewrite_question_placeholders_ignores_single_quoted_literal() {
        // The `?` inside the string literal must be preserved; only the real
        // placeholder outside the literal is renumbered.
        assert_eq!(
            rewrite_question_placeholders("SELECT * FROM t WHERE label = 'is it ok?' AND id = ?"),
            "SELECT * FROM t WHERE label = 'is it ok?' AND id = $1"
        );
    }

    #[test]
    fn test_rewrite_question_placeholders_escaped_quote_in_literal() {
        // A `''` escape must not terminate the string literal early, so the `?`
        // remains inside the literal and only the trailing placeholder is
        // renumbered.
        assert_eq!(
            rewrite_question_placeholders("SELECT 'it''s a ?' , ? FROM t"),
            "SELECT 'it''s a ?' , $1 FROM t"
        );
    }

    #[test]
    fn test_rewrite_question_placeholders_ignores_double_quoted_identifier() {
        assert_eq!(
            rewrite_question_placeholders("SELECT \"weird?col\" FROM t WHERE id = ?"),
            "SELECT \"weird?col\" FROM t WHERE id = $1"
        );
    }

    #[test]
    fn test_rewrite_question_placeholders_ignores_comments() {
        assert_eq!(
            rewrite_question_placeholders("SELECT ? -- trailing ? comment\nFROM t"),
            "SELECT $1 -- trailing ? comment\nFROM t"
        );
        assert_eq!(
            rewrite_question_placeholders("SELECT /* ? */ ? FROM t"),
            "SELECT /* ? */ $1 FROM t"
        );
    }

    // ── Pure value/param mapping ──────────────────────────────────────────────

    #[test]
    fn test_json_to_sql_value_mapping() {
        assert_eq!(json_to_sql_value(&json!(null)), OxiValue::Null);
        assert_eq!(json_to_sql_value(&json!(true)), OxiValue::Bool(true));
        assert_eq!(json_to_sql_value(&json!(7)), OxiValue::I64(7));
        assert_eq!(json_to_sql_value(&json!(2.5)), OxiValue::F64(2.5));
        assert_eq!(
            json_to_sql_value(&json!("hi")),
            OxiValue::Text("hi".to_string())
        );
        assert_eq!(
            json_to_sql_value(&json!({"k": 1})),
            OxiValue::Text("{\"k\":1}".to_string())
        );
    }

    #[test]
    fn test_oxi_value_to_json_mapping() {
        assert_eq!(oxi_value_to_json(&OxiValue::Null), Value::Null);
        assert_eq!(oxi_value_to_json(&OxiValue::Bool(true)), json!(true));
        assert_eq!(oxi_value_to_json(&OxiValue::I64(9)), json!(9));
        assert_eq!(oxi_value_to_json(&OxiValue::F64(1.5)), json!(1.5));
        assert_eq!(
            oxi_value_to_json(&OxiValue::Text("plain".into())),
            json!("plain")
        );
        // Text that looks like JSON is parsed into structured JSON.
        assert_eq!(
            oxi_value_to_json(&OxiValue::Text("[1,2,3]".into())),
            json!([1, 2, 3])
        );
        // Blob -> base64-encoded string.
        let expected_blob =
            base64::Engine::encode(&base64::prelude::BASE64_STANDARD, [1u8, 2, 3, 4]);
        assert_eq!(
            oxi_value_to_json(&OxiValue::Blob(vec![1, 2, 3, 4])).as_str(),
            Some(expected_blob.as_str())
        );
        // Non-finite float -> null (serde_json cannot represent NaN).
        assert_eq!(oxi_value_to_json(&OxiValue::F64(f64::NAN)), Value::Null);
        // Extended variants fall back to their Display string.
        assert!(oxi_value_to_json(&OxiValue::Uuid(1)).as_str().is_some());
    }

    #[test]
    fn test_connection_string_to_path() {
        assert_eq!(
            connection_string_to_path("sqlite:app.db?mode=rwc"),
            "app.db"
        );
        assert_eq!(
            connection_string_to_path("sqlite://data/app.db"),
            "data/app.db"
        );
        assert_eq!(connection_string_to_path("sqlite::memory:"), ":memory:");
        assert_eq!(connection_string_to_path("app.db"), "app.db");
        assert_eq!(connection_string_to_path("sqlite:"), ":memory:");
    }

    // ── End-to-end tests against a real OxiSQL SQLite pool ─────────────────────

    /// Build a unique temporary database path so concurrently-running tests do
    /// not collide. Uses `std::env::temp_dir()` per the project test policy.
    fn temp_db_path(tag: &str) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "oxify_mcp_db_{}_{}_{}.sqlite3",
            std::process::id(),
            tag,
            nanos
        ))
    }

    /// Open a fresh single-connection server backed by a temporary file so that
    /// sequential tool calls observe the same underlying database.
    async fn open_test_server(tag: &str) -> (DatabaseServer, std::path::PathBuf) {
        let path = temp_db_path(tag);
        let _ = std::fs::remove_file(&path);
        let config =
            DatabaseConfig::sqlite(path.to_string_lossy().to_string()).with_max_connections(1);
        let server = DatabaseServer::new(config)
            .await
            .expect("failed to open OxiSQL SQLite pool");
        (server, path)
    }

    #[tokio::test]
    async fn test_db_execute_and_query_roundtrip() {
        let (server, path) = open_test_server("roundtrip").await;

        server
            .call_tool(
                "db_execute",
                json!({ "sql": "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)" }),
            )
            .await
            .expect("create table");

        // db_execute round trip: INSERT with `?` placeholders + bound params.
        let inserted = server
            .call_tool(
                "db_execute",
                json!({
                    "sql": "INSERT INTO users (id, name) VALUES (?, ?)",
                    "params": [1, "Alice"]
                }),
            )
            .await
            .expect("insert row");
        assert_eq!(
            inserted.get("rows_affected").and_then(|v| v.as_u64()),
            Some(1),
            "one row should be affected: {inserted}"
        );

        // db_query round trip: SELECT with a `?` placeholder, proving the runtime
        // placeholder rewriter binds the param correctly end-to-end.
        let queried = server
            .call_tool(
                "db_query",
                json!({
                    "sql": "SELECT id, name FROM users WHERE id = ?",
                    "params": [1]
                }),
            )
            .await
            .expect("query row");

        let columns = queried
            .get("columns")
            .and_then(|v| v.as_array())
            .expect("columns array");
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].as_str(), Some("id"));
        assert_eq!(columns[1].as_str(), Some("name"));

        let rows = queried
            .get("rows")
            .and_then(|v| v.as_array())
            .expect("rows array");
        assert_eq!(rows.len(), 1, "exactly one row expected: {queried}");
        let row0 = rows[0].as_array().expect("row is an array");
        assert_eq!(row0[0].as_i64(), Some(1));
        assert_eq!(row0[1].as_str(), Some("Alice"));
        assert_eq!(queried.get("row_count").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            queried.get("truncated").and_then(|v| v.as_bool()),
            Some(false)
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_in_memory_pool_roundtrip() {
        // A literal `sqlite::memory:` connection string, normalised to
        // `:memory:` by `new()`. Pin to one connection so the create/insert/
        // query all observe the same in-memory database.
        let config = DatabaseConfig::sqlite("sqlite::memory:").with_max_connections(1);
        let server = DatabaseServer::new(config)
            .await
            .expect("open in-memory oxisql pool");

        server
            .call_tool(
                "db_execute",
                json!({ "sql": "CREATE TABLE kv (k TEXT, v INTEGER)" }),
            )
            .await
            .expect("create table");

        let inserted = server
            .call_tool(
                "db_execute",
                json!({
                    "sql": "INSERT INTO kv (k, v) VALUES (?, ?)",
                    "params": ["answer", 42]
                }),
            )
            .await
            .expect("insert row");
        assert_eq!(
            inserted.get("rows_affected").and_then(|v| v.as_u64()),
            Some(1)
        );

        let queried = server
            .call_tool(
                "db_query",
                json!({ "sql": "SELECT k, v FROM kv WHERE k = ?", "params": ["answer"] }),
            )
            .await
            .expect("query row");
        let rows = queried
            .get("rows")
            .and_then(|v| v.as_array())
            .expect("rows array");
        assert_eq!(rows.len(), 1);
        let row0 = rows[0].as_array().expect("row is an array");
        assert_eq!(row0[0].as_str(), Some("answer"));
        assert_eq!(row0[1].as_i64(), Some(42));
    }

    #[tokio::test]
    async fn test_db_query_value_introspection_variants() {
        let (server, path) = open_test_server("introspect").await;

        server
            .call_tool(
                "db_execute",
                json!({
                    "sql": "CREATE TABLE t (i INTEGER, r REAL, s TEXT, b BLOB, n TEXT, j TEXT)"
                }),
            )
            .await
            .expect("create table");

        // Insert one row using SQL literals so we control the exact storage
        // classes returned (integer, real, text, blob, null, JSON-looking text).
        server
            .call_tool(
                "db_execute",
                json!({
                    "sql": "INSERT INTO t (i, r, s, b, n, j) \
                            VALUES (42, 3.5, 'hello', X'01020304', NULL, '{\"k\":1}')"
                }),
            )
            .await
            .expect("insert row");

        let queried = server
            .call_tool(
                "db_query",
                json!({ "sql": "SELECT i, r, s, b, n, j FROM t" }),
            )
            .await
            .expect("query row");

        let rows = queried
            .get("rows")
            .and_then(|v| v.as_array())
            .expect("rows array");
        assert_eq!(rows.len(), 1);
        let row0 = rows[0].as_array().expect("row is an array");
        assert_eq!(row0.len(), 6, "row: {:?}", row0);

        // I64 -> JSON number.
        assert_eq!(row0[0].as_i64(), Some(42));
        // F64 -> JSON number.
        assert_eq!(row0[1].as_f64(), Some(3.5));
        // Text -> JSON string.
        assert_eq!(row0[2].as_str(), Some("hello"));
        // Blob -> base64 JSON string.
        let expected_blob =
            base64::Engine::encode(&base64::prelude::BASE64_STANDARD, [1u8, 2, 3, 4]);
        assert_eq!(row0[3].as_str(), Some(expected_blob.as_str()));
        // NULL -> JSON null.
        assert!(row0[4].is_null());
        // JSON-looking text -> parsed JSON object.
        assert_eq!(row0[5].get("k").and_then(|v| v.as_i64()), Some(1));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_db_transaction_commit_and_rollback() {
        let (server, path) = open_test_server("txn").await;

        server
            .call_tool(
                "db_execute",
                json!({ "sql": "CREATE TABLE items (id INTEGER PRIMARY KEY, label TEXT)" }),
            )
            .await
            .expect("create table");

        // Successful transaction: two inserts commit atomically.
        let committed = server
            .call_tool(
                "db_transaction",
                json!({
                    "statements": [
                        { "sql": "INSERT INTO items (id, label) VALUES (?, ?)", "params": [1, "one"] },
                        { "sql": "INSERT INTO items (id, label) VALUES (?, ?)", "params": [2, "two"] }
                    ]
                }),
            )
            .await
            .expect("commit transaction");
        assert_eq!(
            committed.get("committed").and_then(|v| v.as_bool()),
            Some(true),
            "transaction should commit: {committed}"
        );
        assert_eq!(count_rows(&server, "items").await, 2);

        // Failing transaction: the second statement targets a missing table, so
        // the whole transaction (including the id=3 insert) must roll back.
        let rolled_back = server
            .call_tool(
                "db_transaction",
                json!({
                    "statements": [
                        { "sql": "INSERT INTO items (id, label) VALUES (?, ?)", "params": [3, "three"] },
                        { "sql": "INSERT INTO no_such_table (x) VALUES (1)" }
                    ]
                }),
            )
            .await
            .expect("transaction call returns Ok with committed=false");
        assert_eq!(
            rolled_back.get("committed").and_then(|v| v.as_bool()),
            Some(false),
            "transaction should roll back: {rolled_back}"
        );
        // The failing statement's error must be reported.
        let stmt_results = rolled_back
            .get("statement_results")
            .and_then(|v| v.as_array())
            .expect("statement_results array");
        assert!(
            stmt_results
                .iter()
                .any(|s| s.get("error").map(|e| !e.is_null()).unwrap_or(false)),
            "a failing statement error should be recorded: {rolled_back}"
        );
        // Atomicity: the id=3 insert from the rolled-back transaction must not persist.
        assert_eq!(count_rows(&server, "items").await, 2);

        let _ = std::fs::remove_file(&path);
    }

    /// Helper: run `SELECT COUNT(*) FROM <table>` through `db_query` and return
    /// the count.
    async fn count_rows(server: &DatabaseServer, table: &str) -> i64 {
        let result = server
            .call_tool(
                "db_query",
                json!({ "sql": format!("SELECT COUNT(*) FROM {table}") }),
            )
            .await
            .expect("count query");
        result
            .get("rows")
            .and_then(|v| v.as_array())
            .and_then(|rows| rows.first())
            .and_then(|row| row.as_array())
            .and_then(|cols| cols.first())
            .and_then(|v| v.as_i64())
            .expect("count value")
    }

    #[tokio::test]
    async fn test_read_only_rejects_mutations() {
        let path = temp_db_path("readonly");
        let _ = std::fs::remove_file(&path);
        let server = DatabaseServer::new(
            DatabaseConfig::sqlite(path.to_string_lossy().to_string())
                .with_max_connections(1)
                .with_read_only(true),
        )
        .await
        .expect("open read-only pool");

        // db_execute is unconditionally rejected in read-only mode.
        assert!(server
            .call_tool(
                "db_execute",
                json!({ "sql": "INSERT INTO k (v) VALUES (1)" })
            )
            .await
            .is_err());

        // A mutating db_query (DELETE) is rejected by the is_mutation guard
        // before touching the database.
        assert!(server
            .call_tool("db_query", json!({ "sql": "DELETE FROM k" }))
            .await
            .is_err());

        // db_transaction is rejected in read-only mode.
        assert!(server
            .call_tool(
                "db_transaction",
                json!({ "statements": [ { "sql": "INSERT INTO k VALUES (1)" } ] }),
            )
            .await
            .is_err());

        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(all(test, not(feature = "database")))]
mod tests_no_feature {
    use super::*;

    #[test]
    fn test_database_config() {
        let config = DatabaseConfig::sqlite("sqlite:test.db")
            .with_max_connections(10)
            .with_read_only(true)
            .with_max_rows(500);

        assert_eq!(config.max_connections, 10);
        assert!(config.read_only);
        assert_eq!(config.max_rows, 500);
        assert_eq!(config.db_type, DatabaseType::Sqlite);
    }

    #[test]
    fn test_is_mutation() {
        assert!(DatabaseServer::is_mutation(
            "INSERT INTO users (name) VALUES ('test')"
        ));
        assert!(DatabaseServer::is_mutation(
            "UPDATE users SET name = 'test'"
        ));
        assert!(!DatabaseServer::is_mutation("SELECT * FROM users"));
    }

    #[tokio::test]
    async fn test_stub_returns_feature_error() {
        let config = DatabaseConfig::sqlite("sqlite::memory:");
        let server = DatabaseServer::new(config);

        let result = server
            .call_tool("db_query", json!({"sql": "SELECT 1"}))
            .await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("database"));
    }

    #[tokio::test]
    async fn test_list_tools_without_feature() {
        let config = DatabaseConfig::sqlite("sqlite::memory:");
        let server = DatabaseServer::new(config);

        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 5);

        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();

        assert!(names.contains(&"db_query"));
        assert!(names.contains(&"db_execute"));
        assert!(names.contains(&"db_transaction"));
        assert!(names.contains(&"db_describe"));
        assert!(names.contains(&"db_tables"));
    }
}
