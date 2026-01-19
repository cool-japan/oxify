//! Database MCP server - provides SQL database operations
//!
//! This module implements a Model Context Protocol server for database operations,
//! supporting SQLite queries, commands, and transactions.
//!
//! # Features
//!
//! Enable the `database` feature to use sqlx-backed database operations:
//!
//! ```toml
//! oxify-mcp = { version = "0.1", features = ["database"] }
//! ```

use crate::{McpServer, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[cfg(feature = "database")]
use sqlx::{sqlite::SqlitePool, Column, Row, TypeInfo};

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
    /// Connection pool
    pool: SqlitePool,
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
                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(config.max_connections)
                    .acquire_timeout(std::time::Duration::from_secs(config.timeout_secs))
                    .connect(&config.connection_string)
                    .await
                    .map_err(|e| {
                        crate::McpError::ToolExecutionError(format!(
                            "Failed to connect to database: {}",
                            e
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
    pub fn from_pool(pool: SqlitePool, config: DatabaseConfig) -> Self {
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

                let sql: &'static str = Box::leak(sql_str.to_string().into_boxed_str());

                // Execute the query
                let rows: Vec<sqlx::sqlite::SqliteRow> =
                    sqlx::query(sql).fetch_all(&self.pool).await.map_err(|e| {
                        crate::McpError::ToolExecutionError(format!("Query failed: {}", e))
                    })?;

                // Extract column names from the first row (if any)
                let columns: Vec<String> = if let Some(row) = rows.first() {
                    row.columns().iter().map(|c| c.name().to_string()).collect()
                } else {
                    vec![]
                };

                // Convert rows to JSON
                let mut result_rows = Vec::new();
                let truncated = rows.len() > self.config.max_rows;

                for row in rows.iter().take(self.config.max_rows) {
                    let mut row_values = Vec::new();
                    for col in row.columns() {
                        let value = extract_column_value(row, col)?;
                        row_values.push(value);
                    }
                    result_rows.push(row_values);
                }

                let result = QueryResult {
                    columns,
                    rows: result_rows.clone(),
                    row_count: result_rows.len(),
                    truncated,
                };

                Ok(serde_json::to_value(result).map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to serialize result: {}",
                        e
                    ))
                })?)
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

                let sql: &'static str = Box::leak(sql_str.to_string().into_boxed_str());

                let result: sqlx::sqlite::SqliteQueryResult =
                    sqlx::query(sql).execute(&self.pool).await.map_err(|e| {
                        crate::McpError::ToolExecutionError(format!("Execute failed: {}", e))
                    })?;

                let exec_result = ExecuteResult {
                    rows_affected: result.rows_affected(),
                };

                Ok(serde_json::to_value(exec_result).map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to serialize result: {}",
                        e
                    ))
                })?)
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

                let mut tx: sqlx::Transaction<'_, sqlx::Sqlite> =
                    self.pool.begin().await.map_err(|e| {
                        crate::McpError::ToolExecutionError(format!(
                            "Failed to start transaction: {}",
                            e
                        ))
                    })?;

                let mut statement_results = Vec::new();

                for (index, stmt) in statements.iter().enumerate() {
                    let sql_str = stmt.get("sql").and_then(|v| v.as_str()).ok_or_else(|| {
                        crate::McpError::InvalidArgument(format!(
                            "Statement {} is missing sql field",
                            index
                        ))
                    })?;

                    let sql: &'static str = Box::leak(sql_str.to_string().into_boxed_str());

                    match sqlx::query(sql).execute(&mut *tx).await {
                        Ok(result) => {
                            statement_results.push(StatementResult {
                                index,
                                rows_affected: result.rows_affected(),
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
                                    "Failed to serialize result: {}",
                                    e
                                ))
                            });
                        }
                    }
                }

                // Commit transaction
                tx.commit().await.map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to commit transaction: {}",
                        e
                    ))
                })?;

                let result = TransactionResult {
                    statement_results,
                    committed: true,
                };

                Ok(serde_json::to_value(result).map_err(|e| {
                    crate::McpError::ToolExecutionError(format!(
                        "Failed to serialize result: {}",
                        e
                    ))
                })?)
            }

            "db_describe" => {
                let table = arguments
                    .get("table")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        crate::McpError::InvalidArgument("table is required".to_string())
                    })?;

                // Get column information using SQLite's PRAGMA
                let sql: &'static str =
                    Box::leak(format!("PRAGMA table_info({})", table).into_boxed_str());

                let rows: Vec<sqlx::sqlite::SqliteRow> =
                    sqlx::query(sql).fetch_all(&self.pool).await.map_err(|e| {
                        crate::McpError::ToolExecutionError(format!("Describe failed: {}", e))
                    })?;

                let columns: Vec<Value> = rows
                    .iter()
                    .map(|row: &sqlx::sqlite::SqliteRow| {
                        json!({
                            "column_name": row.get::<String, _>("name"),
                            "data_type": row.get::<String, _>("type"),
                            "is_nullable": if row.get::<i32, _>("notnull") == 0 { "YES" } else { "NO" },
                            "column_default": row.get::<Option<String>, _>("dflt_value")
                        })
                    })
                    .collect();

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

                let rows: Vec<sqlx::sqlite::SqliteRow> =
                    sqlx::query(sql).fetch_all(&self.pool).await.map_err(|e| {
                        crate::McpError::ToolExecutionError(format!("List tables failed: {}", e))
                    })?;

                let tables: Vec<String> = rows
                    .iter()
                    .map(|row: &sqlx::sqlite::SqliteRow| row.get::<String, _>("name"))
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
                            "description": "SQL query to execute (SELECT statements)"
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
                            "description": "SQL command to execute"
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
                                    "sql": { "type": "string" }
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

/// Extract a column value from a row as JSON
#[cfg(feature = "database")]
fn extract_column_value(
    row: &sqlx::sqlite::SqliteRow,
    col: &sqlx::sqlite::SqliteColumn,
) -> Result<Value> {
    use sqlx::ValueRef;

    // Check if the value is null first
    if row
        .try_get_raw(col.ordinal())
        .map(|v: sqlx::sqlite::SqliteValueRef<'_>| v.is_null())
        .unwrap_or(true)
    {
        return Ok(Value::Null);
    }

    let type_info = col.type_info();
    let type_name = type_info.name();

    // SQLite has simpler type system: NULL, INTEGER, REAL, TEXT, BLOB
    match type_name {
        "BOOLEAN" | "BOOL" => {
            let v: bool = row.try_get(col.ordinal()).map_err(|e| {
                crate::McpError::ToolExecutionError(format!("Failed to get bool: {}", e))
            })?;
            Ok(Value::Bool(v))
        }
        "INTEGER" | "INT" | "BIGINT" | "SMALLINT" => {
            let v: i64 = row.try_get(col.ordinal()).map_err(|e| {
                crate::McpError::ToolExecutionError(format!("Failed to get i64: {}", e))
            })?;
            Ok(Value::Number(v.into()))
        }
        "REAL" | "DOUBLE" | "FLOAT" => {
            let v: f64 = row.try_get(col.ordinal()).map_err(|e| {
                crate::McpError::ToolExecutionError(format!("Failed to get f64: {}", e))
            })?;
            Ok(serde_json::Number::from_f64(v)
                .map(Value::Number)
                .unwrap_or(Value::Null))
        }
        "TEXT" | "VARCHAR" | "CHAR" => {
            let v: String = row.try_get(col.ordinal()).map_err(|e| {
                crate::McpError::ToolExecutionError(format!("Failed to get string: {}", e))
            })?;
            // Try to parse as JSON if it looks like JSON
            if v.starts_with('{') || v.starts_with('[') {
                if let Ok(json_val) = serde_json::from_str::<Value>(&v) {
                    return Ok(json_val);
                }
            }
            Ok(Value::String(v))
        }
        "BLOB" => {
            let v: Vec<u8> = row.try_get(col.ordinal()).map_err(|e| {
                crate::McpError::ToolExecutionError(format!("Failed to get bytes: {}", e))
            })?;
            Ok(Value::String(base64::Engine::encode(
                &base64::prelude::BASE64_STANDARD,
                &v,
            )))
        }
        _ => {
            // Try to get as string for unknown types
            let v: String = row
                .try_get(col.ordinal())
                .unwrap_or_else(|_| format!("<unsupported type: {}>", type_name));
            Ok(Value::String(v))
        }
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
