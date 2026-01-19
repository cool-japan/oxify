//! Database constraint management utilities
//!
//! Provides utilities to manage PostgreSQL constraints including check constraints,
//! unique constraints, foreign keys, and primary keys.
//!
//! # Features
//!
//! - List all constraints by type
//! - Add/drop constraints
//! - Validate constraint definitions
//! - Analyze constraint violations
//! - Temporarily disable/enable constraints
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::constraint_manager::{ConstraintManager, ConstraintType};
//!
//! let manager = ConstraintManager::new(pool);
//!
//! // List all foreign keys
//! let fkeys = manager.list_by_type(ConstraintType::ForeignKey).await?;
//!
//! // Add a check constraint
//! manager
//!     .add_check_constraint("users", "age_positive", "age > 0")
//!     .await?;
//!
//! // Temporarily disable a constraint
//! manager.disable_trigger("users", "age_positive_trigger").await?;
//! ```

use crate::{Result, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Type of database constraint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    /// Primary key constraint
    PrimaryKey,
    /// Foreign key constraint
    ForeignKey,
    /// Unique constraint
    Unique,
    /// Check constraint
    Check,
    /// Exclusion constraint
    Exclusion,
}

impl ConstraintType {
    #[allow(dead_code)]
    fn as_pg_char(&self) -> char {
        match self {
            ConstraintType::PrimaryKey => 'p',
            ConstraintType::ForeignKey => 'f',
            ConstraintType::Unique => 'u',
            ConstraintType::Check => 'c',
            ConstraintType::Exclusion => 'x',
        }
    }

    fn from_pg_char(c: char) -> Option<Self> {
        match c {
            'p' => Some(ConstraintType::PrimaryKey),
            'f' => Some(ConstraintType::ForeignKey),
            'u' => Some(ConstraintType::Unique),
            'c' => Some(ConstraintType::Check),
            'x' => Some(ConstraintType::Exclusion),
            _ => None,
        }
    }
}

impl std::fmt::Display for ConstraintType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConstraintType::PrimaryKey => write!(f, "PRIMARY KEY"),
            ConstraintType::ForeignKey => write!(f, "FOREIGN KEY"),
            ConstraintType::Unique => write!(f, "UNIQUE"),
            ConstraintType::Check => write!(f, "CHECK"),
            ConstraintType::Exclusion => write!(f, "EXCLUSION"),
        }
    }
}

/// Information about a database constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintInfo {
    /// Schema name
    pub schema_name: String,
    /// Table name
    pub table_name: String,
    /// Constraint name
    pub constraint_name: String,
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Constraint definition
    pub definition: String,
    /// Is the constraint deferrable
    pub is_deferrable: bool,
    /// Is the constraint initially deferred
    pub is_deferred: bool,
    /// Foreign table (for foreign keys)
    pub foreign_table: Option<String>,
    /// Foreign columns (for foreign keys)
    pub foreign_columns: Option<Vec<String>>,
}

/// Constraint manager
pub struct ConstraintManager {
    pool: PgPool,
}

impl ConstraintManager {
    /// Create a new constraint manager
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List all constraints
    #[tracing::instrument(skip(self))]
    pub async fn list_all(&self) -> Result<Vec<ConstraintInfo>> {
        let rows = sqlx::query_as::<_, ConstraintRow>(
            r#"
            SELECT
                n.nspname AS schema_name,
                c.relname AS table_name,
                con.conname AS constraint_name,
                con.contype,
                pg_get_constraintdef(con.oid) AS definition,
                con.condeferrable AS is_deferrable,
                con.condeferred AS is_deferred,
                fn.nspname AS foreign_schema,
                fc.relname AS foreign_table
            FROM pg_constraint con
            JOIN pg_class c ON c.oid = con.conrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            LEFT JOIN pg_class fc ON fc.oid = con.confrelid
            LEFT JOIN pg_namespace fn ON fn.oid = fc.relnamespace
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY n.nspname, c.relname, con.conname
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| row.try_into().ok())
            .collect())
    }

    /// List constraints of a specific type
    #[tracing::instrument(skip(self))]
    pub async fn list_by_type(
        &self,
        constraint_type: ConstraintType,
    ) -> Result<Vec<ConstraintInfo>> {
        let all_constraints = self.list_all().await?;
        Ok(all_constraints
            .into_iter()
            .filter(|c| c.constraint_type == constraint_type)
            .collect())
    }

    /// List constraints for a specific table
    #[tracing::instrument(skip(self))]
    pub async fn list_for_table(&self, table_name: &str) -> Result<Vec<ConstraintInfo>> {
        let all_constraints = self.list_all().await?;
        Ok(all_constraints
            .into_iter()
            .filter(|c| c.table_name == table_name)
            .collect())
    }

    /// Add a CHECK constraint
    pub async fn add_check_constraint(
        &self,
        table_name: &str,
        constraint_name: &str,
        check_expression: &str,
    ) -> Result<()> {
        let sql = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({})",
            table_name, constraint_name, check_expression
        );

        sqlx::query(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Add a UNIQUE constraint
    pub async fn add_unique_constraint(
        &self,
        table_name: &str,
        constraint_name: &str,
        columns: &[&str],
    ) -> Result<()> {
        let sql = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE ({})",
            table_name,
            constraint_name,
            columns.join(", ")
        );

        sqlx::query(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Add a FOREIGN KEY constraint
    #[allow(clippy::too_many_arguments)]
    pub async fn add_foreign_key_constraint(
        &self,
        table_name: &str,
        constraint_name: &str,
        columns: &[&str],
        foreign_table: &str,
        foreign_columns: &[&str],
        on_delete: Option<&str>,
        on_update: Option<&str>,
    ) -> Result<()> {
        let mut sql = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            table_name,
            constraint_name,
            columns.join(", "),
            foreign_table,
            foreign_columns.join(", ")
        );

        if let Some(on_delete) = on_delete {
            sql.push_str(&format!(" ON DELETE {}", on_delete));
        }

        if let Some(on_update) = on_update {
            sql.push_str(&format!(" ON UPDATE {}", on_update));
        }

        sqlx::query(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Drop a constraint
    pub async fn drop_constraint(
        &self,
        table_name: &str,
        constraint_name: &str,
        if_exists: bool,
    ) -> Result<()> {
        let if_exists_clause = if if_exists { "IF EXISTS " } else { "" };

        let sql = format!(
            "ALTER TABLE {} DROP CONSTRAINT {}{}",
            table_name, if_exists_clause, constraint_name
        );

        sqlx::query(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Validate a constraint (check if it would fail on current data)
    pub async fn validate_constraint(
        &self,
        table_name: &str,
        constraint_name: &str,
    ) -> Result<bool> {
        let sql = format!(
            "ALTER TABLE {} VALIDATE CONSTRAINT {}",
            table_name, constraint_name
        );

        match sqlx::query(&sql).execute(&self.pool).await {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.to_string().contains("violates check constraint") {
                    Ok(false)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    /// Disable a trigger (useful for bulk operations)
    pub async fn disable_trigger(&self, table_name: &str, trigger_name: &str) -> Result<()> {
        let sql = format!(
            "ALTER TABLE {} DISABLE TRIGGER {}",
            table_name, trigger_name
        );

        sqlx::query(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Enable a trigger
    pub async fn enable_trigger(&self, table_name: &str, trigger_name: &str) -> Result<()> {
        let sql = format!("ALTER TABLE {} ENABLE TRIGGER {}", table_name, trigger_name);

        sqlx::query(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Check if a constraint exists
    pub async fn constraint_exists(&self, table_name: &str, constraint_name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM pg_constraint con
            JOIN pg_class c ON c.oid = con.conrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relname = $1 AND con.conname = $2
              AND n.nspname = 'public'
            "#,
        )
        .bind(table_name)
        .bind(constraint_name)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }
}

/// Internal struct for SQL query result
#[derive(Debug, sqlx::FromRow)]
struct ConstraintRow {
    schema_name: String,
    table_name: String,
    constraint_name: String,
    contype: String,
    definition: String,
    is_deferrable: bool,
    is_deferred: bool,
    _foreign_schema: Option<String>,
    foreign_table: Option<String>,
}

impl TryFrom<ConstraintRow> for ConstraintInfo {
    type Error = StorageError;

    fn try_from(row: ConstraintRow) -> Result<Self> {
        let constraint_type = row
            .contype
            .chars()
            .next()
            .and_then(ConstraintType::from_pg_char)
            .ok_or_else(|| {
                StorageError::ValidationError(format!("Unknown constraint type: {}", row.contype))
            })?;

        Ok(ConstraintInfo {
            schema_name: row.schema_name,
            table_name: row.table_name,
            constraint_name: row.constraint_name,
            constraint_type,
            definition: row.definition,
            is_deferrable: row.is_deferrable,
            is_deferred: row.is_deferred,
            foreign_table: row.foreign_table,
            foreign_columns: None, // Would need additional parsing
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_type_as_pg_char() {
        assert_eq!(ConstraintType::PrimaryKey.as_pg_char(), 'p');
        assert_eq!(ConstraintType::ForeignKey.as_pg_char(), 'f');
        assert_eq!(ConstraintType::Unique.as_pg_char(), 'u');
        assert_eq!(ConstraintType::Check.as_pg_char(), 'c');
    }

    #[test]
    fn test_constraint_type_from_pg_char() {
        assert_eq!(
            ConstraintType::from_pg_char('p'),
            Some(ConstraintType::PrimaryKey)
        );
        assert_eq!(
            ConstraintType::from_pg_char('f'),
            Some(ConstraintType::ForeignKey)
        );
        assert_eq!(
            ConstraintType::from_pg_char('u'),
            Some(ConstraintType::Unique)
        );
        assert_eq!(
            ConstraintType::from_pg_char('c'),
            Some(ConstraintType::Check)
        );
        assert_eq!(
            ConstraintType::from_pg_char('x'),
            Some(ConstraintType::Exclusion)
        );
        assert_eq!(ConstraintType::from_pg_char('z'), None);
    }

    #[test]
    fn test_constraint_type_display() {
        assert_eq!(ConstraintType::PrimaryKey.to_string(), "PRIMARY KEY");
        assert_eq!(ConstraintType::ForeignKey.to_string(), "FOREIGN KEY");
        assert_eq!(ConstraintType::Unique.to_string(), "UNIQUE");
        assert_eq!(ConstraintType::Check.to_string(), "CHECK");
    }
}
