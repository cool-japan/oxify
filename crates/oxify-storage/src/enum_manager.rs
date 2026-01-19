//! PostgreSQL Enum Type Manager
//!
//! This module provides utilities for managing PostgreSQL custom ENUM types.
//! ENUM types provide a way to define a fixed set of allowed values for a column,
//! similar to Rust enums but at the database level.
//!
//! # Benefits of PostgreSQL ENUMs
//!
//! - **Type Safety**: Database enforces valid values
//! - **Storage Efficiency**: More compact than storing strings
//! - **Query Performance**: Faster comparisons than strings
//! - **Self-Documenting**: Schema clearly shows allowed values
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::EnumManager;
//!
//! let enum_mgr = EnumManager::new(pool.clone());
//!
//! // Create a new enum type
//! enum_mgr.create_enum("execution_state", &[
//!     "pending", "running", "completed", "failed", "cancelled"
//! ]).await?;
//!
//! // Add a new value to existing enum
//! enum_mgr.add_enum_value("execution_state", "paused").await?;
//!
//! // List all enum types
//! let enums = enum_mgr.list_enums().await?;
//! for enum_info in enums {
//!     println!("{}: {:?}", enum_info.name, enum_info.values);
//! }
//! ```

use crate::{Result, StorageError};
use sqlx::PgPool;
use std::sync::Arc;

/// Information about a PostgreSQL ENUM type
#[derive(Debug, Clone)]
pub struct EnumInfo {
    /// Enum type name
    pub name: String,
    /// Schema where enum is defined
    pub schema: String,
    /// Ordered list of enum values
    pub values: Vec<String>,
}

/// PostgreSQL Enum Type Manager
///
/// Provides methods for creating, modifying, and querying ENUM types.
#[derive(Clone)]
pub struct EnumManager {
    pool: Arc<PgPool>,
}

impl EnumManager {
    /// Create a new enum manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // ========================================================================
    // Enum Creation and Removal
    // ========================================================================

    /// Create a new ENUM type
    ///
    /// # Example
    ///
    /// ```ignore
    /// enum_mgr.create_enum("priority", &["low", "medium", "high", "urgent"]).await?;
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn create_enum(&self, enum_name: &str, values: &[&str]) -> Result<()> {
        if values.is_empty() {
            return Err(StorageError::validation(
                "Enum must have at least one value",
            ));
        }

        let values_str = values
            .iter()
            .map(|v| format!("'{}'", v))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!("CREATE TYPE {} AS ENUM ({})", enum_name, values_str);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Create ENUM type if it doesn't already exist
    ///
    /// Note: PostgreSQL doesn't have "CREATE TYPE IF NOT EXISTS",
    /// so this checks for existence first.
    #[tracing::instrument(skip(self))]
    pub async fn create_enum_if_not_exists(
        &self,
        enum_name: &str,
        values: &[&str],
    ) -> Result<bool> {
        if self.enum_exists(enum_name).await? {
            return Ok(false);
        }

        self.create_enum(enum_name, values).await?;
        Ok(true)
    }

    /// Drop an ENUM type
    ///
    /// # Warning
    ///
    /// This will fail if any columns are using this enum type.
    /// Use `drop_enum_cascade` to remove the type and all dependent columns.
    #[tracing::instrument(skip(self))]
    pub async fn drop_enum(&self, enum_name: &str) -> Result<()> {
        let sql = format!("DROP TYPE {}", enum_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop ENUM type if it exists
    #[tracing::instrument(skip(self))]
    pub async fn drop_enum_if_exists(&self, enum_name: &str) -> Result<()> {
        let sql = format!("DROP TYPE IF EXISTS {}", enum_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop ENUM type with CASCADE to remove dependent objects
    #[tracing::instrument(skip(self))]
    pub async fn drop_enum_cascade(&self, enum_name: &str) -> Result<()> {
        let sql = format!("DROP TYPE {} CASCADE", enum_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Enum Modification
    // ========================================================================

    /// Add a new value to an existing ENUM type
    ///
    /// The new value is added at the end by default.
    ///
    /// # Example
    ///
    /// ```ignore
    /// enum_mgr.add_enum_value("execution_state", "paused").await?;
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn add_enum_value(&self, enum_name: &str, new_value: &str) -> Result<()> {
        let sql = format!("ALTER TYPE {} ADD VALUE '{}'", enum_name, new_value);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Add a new value to an ENUM type if it doesn't already exist
    #[tracing::instrument(skip(self))]
    pub async fn add_enum_value_if_not_exists(
        &self,
        enum_name: &str,
        new_value: &str,
    ) -> Result<bool> {
        let sql = format!(
            "ALTER TYPE {} ADD VALUE IF NOT EXISTS '{}'",
            enum_name, new_value
        );
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        // PostgreSQL doesn't return whether the value was added, so we return true
        Ok(true)
    }

    /// Add a new value before an existing value
    ///
    /// # Example
    ///
    /// ```ignore
    /// // If enum is ["low", "high"], add "medium" before "high"
    /// enum_mgr.add_enum_value_before("priority", "medium", "high").await?;
    /// // Result: ["low", "medium", "high"]
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn add_enum_value_before(
        &self,
        enum_name: &str,
        new_value: &str,
        before_value: &str,
    ) -> Result<()> {
        let sql = format!(
            "ALTER TYPE {} ADD VALUE '{}' BEFORE '{}'",
            enum_name, new_value, before_value
        );
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Add a new value after an existing value
    ///
    /// # Example
    ///
    /// ```ignore
    /// // If enum is ["low", "high"], add "medium" after "low"
    /// enum_mgr.add_enum_value_after("priority", "medium", "low").await?;
    /// // Result: ["low", "medium", "high"]
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn add_enum_value_after(
        &self,
        enum_name: &str,
        new_value: &str,
        after_value: &str,
    ) -> Result<()> {
        let sql = format!(
            "ALTER TYPE {} ADD VALUE '{}' AFTER '{}'",
            enum_name, new_value, after_value
        );
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Rename an ENUM type
    #[tracing::instrument(skip(self))]
    pub async fn rename_enum(&self, old_name: &str, new_name: &str) -> Result<()> {
        let sql = format!("ALTER TYPE {} RENAME TO {}", old_name, new_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Rename a value in an ENUM type
    ///
    /// # Note
    ///
    /// Renaming enum values requires PostgreSQL 10+. For older versions,
    /// you need to create a new enum type and migrate data.
    #[tracing::instrument(skip(self))]
    pub async fn rename_enum_value(
        &self,
        enum_name: &str,
        old_value: &str,
        new_value: &str,
    ) -> Result<()> {
        let sql = format!(
            "ALTER TYPE {} RENAME VALUE '{}' TO '{}'",
            enum_name, old_value, new_value
        );
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Enum Information
    // ========================================================================

    /// List all ENUM types in the database
    #[tracing::instrument(skip(self))]
    pub async fn list_enums(&self) -> Result<Vec<EnumInfo>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r"
            SELECT
                t.typname as enum_name,
                n.nspname as schema_name
            FROM pg_type t
            JOIN pg_namespace n ON t.typnamespace = n.oid
            WHERE t.typtype = 'e'
              AND n.nspname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY n.nspname, t.typname
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        let mut enums = Vec::new();
        for (enum_name, schema) in rows {
            let values = self.get_enum_values(&enum_name).await?;
            enums.push(EnumInfo {
                name: enum_name,
                schema,
                values,
            });
        }

        Ok(enums)
    }

    /// Get all values for a specific ENUM type
    #[tracing::instrument(skip(self))]
    pub async fn get_enum_values(&self, enum_name: &str) -> Result<Vec<String>> {
        let values: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT enumlabel
            FROM pg_enum
            WHERE enumtypid = $1::regtype
            ORDER BY enumsortorder
            ",
        )
        .bind(enum_name)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(values.into_iter().map(|v| v.0).collect())
    }

    /// Get detailed information about a specific ENUM type
    #[tracing::instrument(skip(self))]
    pub async fn get_enum_info(&self, enum_name: &str) -> Result<Option<EnumInfo>> {
        let row: Option<(String, String)> = sqlx::query_as(
            r"
            SELECT
                t.typname as enum_name,
                n.nspname as schema_name
            FROM pg_type t
            JOIN pg_namespace n ON t.typnamespace = n.oid
            WHERE t.typname = $1
              AND t.typtype = 'e'
            ",
        )
        .bind(enum_name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        if let Some((name, schema)) = row {
            let values = self.get_enum_values(enum_name).await?;
            Ok(Some(EnumInfo {
                name,
                schema,
                values,
            }))
        } else {
            Ok(None)
        }
    }

    /// Check if an ENUM type exists
    #[tracing::instrument(skip(self))]
    pub async fn enum_exists(&self, enum_name: &str) -> Result<bool> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM pg_type t
                JOIN pg_namespace n ON t.typnamespace = n.oid
                WHERE t.typname = $1 AND t.typtype = 'e'
            )",
        )
        .bind(enum_name)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Check if a value exists in an ENUM type
    #[tracing::instrument(skip(self))]
    pub async fn enum_has_value(&self, enum_name: &str, value: &str) -> Result<bool> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM pg_enum
                WHERE enumtypid = $1::regtype AND enumlabel = $2
            )",
        )
        .bind(enum_name)
        .bind(value)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// List all columns that use a specific ENUM type
    #[tracing::instrument(skip(self))]
    pub async fn list_enum_usage(&self, enum_name: &str) -> Result<Vec<(String, String)>> {
        let columns: Vec<(String, String)> = sqlx::query_as(
            r"
            SELECT
                c.table_name,
                c.column_name
            FROM information_schema.columns c
            WHERE c.udt_name = $1
              AND c.table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY c.table_name, c.column_name
            ",
        )
        .bind(enum_name)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enum_info_structure() {
        let info = EnumInfo {
            name: "execution_state".to_string(),
            schema: "public".to_string(),
            values: vec![
                "pending".to_string(),
                "running".to_string(),
                "completed".to_string(),
            ],
        };

        assert_eq!(info.name, "execution_state");
        assert_eq!(info.schema, "public");
        assert_eq!(info.values.len(), 3);
        assert_eq!(info.values[0], "pending");
    }

    #[test]
    fn test_enum_info_clone() {
        let info = EnumInfo {
            name: "status".to_string(),
            schema: "public".to_string(),
            values: vec!["active".to_string(), "inactive".to_string()],
        };

        let cloned = info.clone();
        assert_eq!(cloned.name, info.name);
        assert_eq!(cloned.values, info.values);
    }
}
