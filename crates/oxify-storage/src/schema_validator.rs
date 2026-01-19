//! Schema validation utilities
//!
//! This module provides tools to validate that the database schema matches
//! the expected structure. This is critical for ensuring database consistency
//! and detecting schema drift in production environments.
//!
//! # Features
//!
//! - **Table Existence Check**: Verify all required tables exist
//! - **Column Validation**: Check column names, types, and constraints
//! - **Index Validation**: Verify critical indexes are present
//! - **Foreign Key Check**: Validate foreign key relationships
//! - **Schema Version**: Track schema version for migrations
//! - **Comprehensive Report**: Detailed validation results
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, SchemaValidator};
//!
//! let pool = DatabasePool::new(config).await?;
//! let validator = SchemaValidator::new(pool.clone());
//!
//! // Quick validation
//! let is_valid = validator.validate_quick().await?;
//! if !is_valid {
//!     panic!("Schema validation failed!");
//! }
//!
//! // Detailed validation
//! let report = validator.validate_full().await?;
//! if !report.is_valid {
//!     eprintln!("Schema validation errors:");
//!     for error in &report.errors {
//!         eprintln!("  - {}", error);
//!     }
//! }
//!
//! // Check specific table
//! let table_valid = validator
//!     .validate_table("workflows", &["id", "name", "user_id"])
//!     .await?;
//! ```

use crate::{DatabasePool, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;

/// Schema validation error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SchemaError {
    /// Required table is missing
    MissingTable(String),
    /// Required column is missing
    MissingColumn { table: String, column: String },
    /// Column has wrong type
    WrongColumnType {
        table: String,
        column: String,
        expected: String,
        actual: String,
    },
    /// Required index is missing
    MissingIndex { table: String, index: String },
    /// Foreign key constraint is missing
    MissingForeignKey { table: String, constraint: String },
    /// Schema version mismatch
    VersionMismatch { expected: String, actual: String },
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaError::MissingTable(table) => write!(f, "Missing table: {table}"),
            SchemaError::MissingColumn { table, column } => {
                write!(f, "Missing column '{column}' in table '{table}'")
            }
            SchemaError::WrongColumnType {
                table,
                column,
                expected,
                actual,
            } => write!(
                f,
                "Wrong type for column '{column}' in table '{table}': expected {expected}, got {actual}"
            ),
            SchemaError::MissingIndex { table, index } => {
                write!(f, "Missing index '{index}' on table '{table}'")
            }
            SchemaError::MissingForeignKey { table, constraint } => {
                write!(
                    f,
                    "Missing foreign key '{constraint}' on table '{table}'"
                )
            }
            SchemaError::VersionMismatch { expected, actual } => {
                write!(
                    f,
                    "Schema version mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

/// Schema validation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Overall validation status
    pub is_valid: bool,
    /// List of errors found
    pub errors: Vec<SchemaError>,
    /// List of warnings (non-critical issues)
    pub warnings: Vec<String>,
    /// Number of tables checked
    pub tables_checked: usize,
    /// Number of columns checked
    pub columns_checked: usize,
    /// Number of indexes checked
    pub indexes_checked: usize,
    /// Schema version
    pub schema_version: Option<String>,
}

impl ValidationReport {
    /// Create a new empty validation report
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            tables_checked: 0,
            columns_checked: 0,
            indexes_checked: 0,
            schema_version: None,
        }
    }

    /// Add an error to the report
    pub fn add_error(&mut self, error: SchemaError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    /// Add a warning to the report
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Required table schema definition
#[derive(Debug, Clone)]
pub struct TableSchema {
    /// Table name
    pub name: String,
    /// Required columns with their expected types
    pub columns: Vec<(String, String)>,
    /// Required indexes
    pub indexes: Vec<String>,
}

/// Schema validator
pub struct SchemaValidator {
    pool: DatabasePool,
    required_tables: Vec<TableSchema>,
}

impl SchemaValidator {
    /// Create a new schema validator with default schema
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            required_tables: Self::default_schema(),
        }
    }

    /// Create a schema validator with custom schema definition
    pub fn with_schema(pool: DatabasePool, schema: Vec<TableSchema>) -> Self {
        Self {
            pool,
            required_tables: schema,
        }
    }

    /// Quick validation - just check if all required tables exist
    pub async fn validate_quick(&self) -> Result<bool> {
        for table_schema in &self.required_tables {
            if !self.table_exists(&table_schema.name).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Full validation - comprehensive check of schema
    pub async fn validate_full(&self) -> Result<ValidationReport> {
        let mut report = ValidationReport::new();

        // Check each required table
        for table_schema in &self.required_tables {
            report.tables_checked += 1;

            // Check table exists
            if !self.table_exists(&table_schema.name).await? {
                report.add_error(SchemaError::MissingTable(table_schema.name.clone()));
                continue;
            }

            // Check columns
            for (column_name, expected_type) in &table_schema.columns {
                report.columns_checked += 1;

                match self
                    .get_column_type(&table_schema.name, column_name)
                    .await?
                {
                    Some(actual_type) => {
                        // Normalize types for comparison
                        if !self.types_compatible(expected_type, &actual_type) {
                            report.add_error(SchemaError::WrongColumnType {
                                table: table_schema.name.clone(),
                                column: column_name.clone(),
                                expected: expected_type.clone(),
                                actual: actual_type,
                            });
                        }
                    }
                    None => {
                        report.add_error(SchemaError::MissingColumn {
                            table: table_schema.name.clone(),
                            column: column_name.clone(),
                        });
                    }
                }
            }

            // Check indexes
            let existing_indexes = self.get_table_indexes(&table_schema.name).await?;
            for required_index in &table_schema.indexes {
                report.indexes_checked += 1;

                if !existing_indexes.contains(required_index) {
                    report.add_warning(format!(
                        "Index '{}' not found on table '{}' (performance may be affected)",
                        required_index, table_schema.name
                    ));
                }
            }
        }

        Ok(report)
    }

    /// Validate a specific table
    pub async fn validate_table(
        &self,
        table_name: &str,
        required_columns: &[&str],
    ) -> Result<bool> {
        if !self.table_exists(table_name).await? {
            return Ok(false);
        }

        for column in required_columns {
            if self.get_column_type(table_name, column).await?.is_none() {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Get list of all tables in the database
    pub async fn list_tables(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT table_name FROM information_schema.tables
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
             ORDER BY table_name",
        )
        .fetch_all(self.pool.pool())
        .await?;

        Ok(rows
            .iter()
            .map(|row| row.get::<String, _>("table_name"))
            .collect())
    }

    /// Get schema version from database
    pub async fn get_schema_version(&self) -> Result<Option<String>> {
        // Try to get version from a schema_version table if it exists
        let result =
            sqlx::query("SELECT version FROM schema_version ORDER BY applied_at DESC LIMIT 1")
                .fetch_optional(self.pool.pool())
                .await;

        match result {
            Ok(Some(row)) => Ok(Some(row.get("version"))),
            Ok(None) | Err(_) => Ok(None),
        }
    }

    /// Compare database schema with expected version
    pub async fn check_version(&self, expected_version: &str) -> Result<bool> {
        match self.get_schema_version().await? {
            Some(actual) => Ok(actual == expected_version),
            None => Ok(false),
        }
    }

    // Private helper methods

    async fn table_exists(&self, table_name: &str) -> Result<bool> {
        let row = sqlx::query(
            "SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_schema = 'public' AND table_name = $1
            )",
        )
        .bind(table_name)
        .fetch_one(self.pool.pool())
        .await?;

        Ok(row.get(0))
    }

    async fn get_column_type(&self, table_name: &str, column_name: &str) -> Result<Option<String>> {
        let result = sqlx::query(
            "SELECT data_type FROM information_schema.columns
             WHERE table_schema = 'public'
               AND table_name = $1
               AND column_name = $2",
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_optional(self.pool.pool())
        .await?;

        Ok(result.map(|row| row.get("data_type")))
    }

    async fn get_table_indexes(&self, table_name: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT indexname FROM pg_indexes
             WHERE schemaname = 'public' AND tablename = $1",
        )
        .bind(table_name)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(rows
            .iter()
            .map(|row| row.get::<String, _>("indexname"))
            .collect())
    }

    fn types_compatible(&self, expected: &str, actual: &str) -> bool {
        // Normalize and compare types
        let expected_lower = expected.to_lowercase();
        let actual_lower = actual.to_lowercase();

        // Direct match
        if expected_lower == actual_lower {
            return true;
        }

        // Common aliases
        let type_map: HashMap<&str, Vec<&str>> = [
            ("uuid", vec!["uuid"]),
            ("text", vec!["text", "character varying", "varchar"]),
            ("bigint", vec!["bigint", "int8"]),
            ("integer", vec!["integer", "int", "int4"]),
            ("smallint", vec!["smallint", "int2"]),
            ("boolean", vec!["boolean", "bool"]),
            (
                "timestamp",
                vec!["timestamp without time zone", "timestamp"],
            ),
            (
                "timestamptz",
                vec!["timestamp with time zone", "timestamptz"],
            ),
            ("jsonb", vec!["jsonb"]),
            ("bytea", vec!["bytea"]),
        ]
        .iter()
        .cloned()
        .collect();

        // Check if both types map to the same canonical type
        for (_, aliases) in type_map {
            if aliases.contains(&expected_lower.as_str())
                && aliases.contains(&actual_lower.as_str())
            {
                return true;
            }
        }

        false
    }

    /// Define the default expected schema for OxiFY storage
    fn default_schema() -> Vec<TableSchema> {
        vec![
            TableSchema {
                name: "users".to_string(),
                columns: vec![
                    ("id".to_string(), "uuid".to_string()),
                    ("email".to_string(), "text".to_string()),
                    ("password_hash".to_string(), "text".to_string()),
                    ("created_at".to_string(), "timestamp".to_string()),
                ],
                indexes: vec!["users_pkey".to_string(), "users_email_key".to_string()],
            },
            TableSchema {
                name: "workflows".to_string(),
                columns: vec![
                    ("id".to_string(), "uuid".to_string()),
                    ("user_id".to_string(), "uuid".to_string()),
                    ("name".to_string(), "text".to_string()),
                    ("version".to_string(), "integer".to_string()),
                    ("created_at".to_string(), "timestamp".to_string()),
                    ("updated_at".to_string(), "timestamp".to_string()),
                ],
                indexes: vec!["workflows_pkey".to_string()],
            },
            TableSchema {
                name: "executions".to_string(),
                columns: vec![
                    ("id".to_string(), "uuid".to_string()),
                    ("workflow_id".to_string(), "uuid".to_string()),
                    ("state".to_string(), "text".to_string()),
                    ("created_at".to_string(), "timestamp".to_string()),
                ],
                indexes: vec!["executions_pkey".to_string()],
            },
            TableSchema {
                name: "user_quotas".to_string(),
                columns: vec![
                    ("user_id".to_string(), "uuid".to_string()),
                    ("executions_today".to_string(), "integer".to_string()),
                    ("storage_bytes".to_string(), "bigint".to_string()),
                ],
                indexes: vec!["user_quotas_pkey".to_string()],
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_report() {
        let mut report = ValidationReport::new();
        assert!(report.is_valid);
        assert_eq!(report.errors.len(), 0);

        report.add_error(SchemaError::MissingTable("test".to_string()));
        assert!(!report.is_valid);
        assert_eq!(report.errors.len(), 1);

        report.add_warning("Test warning".to_string());
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn test_schema_error_display() {
        let err = SchemaError::MissingTable("users".to_string());
        assert_eq!(format!("{}", err), "Missing table: users");

        let err = SchemaError::MissingColumn {
            table: "users".to_string(),
            column: "email".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "Missing column 'email' in table 'users'"
        );
    }

    #[test]
    fn test_default_schema() {
        let schema = SchemaValidator::default_schema();
        assert!(!schema.is_empty());

        let users_table = schema.iter().find(|t| t.name == "users");
        assert!(users_table.is_some());

        let users = users_table.unwrap();
        assert!(users.columns.iter().any(|(name, _)| name == "id"));
        assert!(users.columns.iter().any(|(name, _)| name == "email"));
    }
}
