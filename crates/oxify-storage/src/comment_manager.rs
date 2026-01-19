//! PostgreSQL Database Comments Manager
//!
//! This module provides utilities for managing PostgreSQL comments on database objects.
//! Comments are a powerful way to document your schema directly in the database,
//! making it self-documenting and easier to understand for developers and DBAs.
//!
//! # Supported Objects
//!
//! - Tables
//! - Columns
//! - Indexes
//! - Constraints
//! - Sequences
//! - Views
//! - Functions
//! - Schemas
//!
//! # Benefits
//!
//! - **Self-Documenting Schema**: Documentation lives with the data
//! - **Tool Integration**: Many database tools display comments
//! - **Version Control**: Comments are part of schema migrations
//! - **Team Collaboration**: Shared understanding of schema design
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::CommentManager;
//!
//! let comment_mgr = CommentManager::new(pool.clone());
//!
//! // Add table comment
//! comment_mgr.set_table_comment(
//!     "workflows",
//!     "Stores workflow definitions and metadata"
//! ).await?;
//!
//! // Add column comment
//! comment_mgr.set_column_comment(
//!     "workflows",
//!     "state",
//!     "Current execution state: pending, running, completed, failed"
//! ).await?;
//!
//! // Get all comments for a table
//! let comments = comment_mgr.get_table_comments("workflows").await?;
//! for comment in comments {
//!     println!("{}.{}: {}", comment.table_name, comment.column_name, comment.comment);
//! }
//! ```

use crate::{Result, StorageError};
use sqlx::PgPool;
use std::sync::Arc;

/// Comment information for a database object
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ObjectComment {
    /// Schema name
    pub schema_name: String,
    /// Object name (table, column, etc.)
    pub object_name: String,
    /// Comment text
    pub comment: Option<String>,
}

/// Column comment information
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ColumnComment {
    /// Schema name
    pub schema_name: String,
    /// Table name
    pub table_name: String,
    /// Column name
    pub column_name: String,
    /// Comment text
    pub comment: Option<String>,
}

/// PostgreSQL Comments Manager
///
/// Provides methods for setting and retrieving comments on database objects.
#[derive(Clone)]
pub struct CommentManager {
    pool: Arc<PgPool>,
}

impl CommentManager {
    /// Create a new comment manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // ========================================================================
    // Table Comments
    // ========================================================================

    /// Set a comment on a table
    ///
    /// # Example
    ///
    /// ```ignore
    /// comment_mgr.set_table_comment(
    ///     "users",
    ///     "Application users with authentication credentials"
    /// ).await?;
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn set_table_comment(&self, table_name: &str, comment: &str) -> Result<()> {
        let sql = format!("COMMENT ON TABLE {} IS $1", table_name);
        sqlx::query(&sql)
            .bind(comment)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Remove a comment from a table
    #[tracing::instrument(skip(self))]
    pub async fn remove_table_comment(&self, table_name: &str) -> Result<()> {
        let sql = format!("COMMENT ON TABLE {} IS NULL", table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Get a comment for a specific table
    #[tracing::instrument(skip(self))]
    pub async fn get_table_comment(&self, table_name: &str) -> Result<Option<String>> {
        let result: Option<(Option<String>,)> = sqlx::query_as(
            r"
            SELECT obj_description(c.oid, 'pg_class') as comment
            FROM pg_class c
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE c.relname = $1
              AND n.nspname = 'public'
              AND c.relkind = 'r'
            ",
        )
        .bind(table_name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.and_then(|r| r.0))
    }

    /// List all tables with comments
    #[tracing::instrument(skip(self))]
    pub async fn list_table_comments(&self) -> Result<Vec<ObjectComment>> {
        let comments: Vec<ObjectComment> = sqlx::query_as(
            r"
            SELECT
                n.nspname as schema_name,
                c.relname as object_name,
                obj_description(c.oid, 'pg_class') as comment
            FROM pg_class c
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
              AND c.relkind = 'r'
              AND obj_description(c.oid, 'pg_class') IS NOT NULL
            ORDER BY n.nspname, c.relname
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(comments)
    }

    // ========================================================================
    // Column Comments
    // ========================================================================

    /// Set a comment on a column
    ///
    /// # Example
    ///
    /// ```ignore
    /// comment_mgr.set_column_comment(
    ///     "users",
    ///     "email",
    ///     "User email address (unique, used for authentication)"
    /// ).await?;
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn set_column_comment(
        &self,
        table_name: &str,
        column_name: &str,
        comment: &str,
    ) -> Result<()> {
        let sql = format!("COMMENT ON COLUMN {}.{} IS $1", table_name, column_name);
        sqlx::query(&sql)
            .bind(comment)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Remove a comment from a column
    #[tracing::instrument(skip(self))]
    pub async fn remove_column_comment(&self, table_name: &str, column_name: &str) -> Result<()> {
        let sql = format!("COMMENT ON COLUMN {}.{} IS NULL", table_name, column_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Get a comment for a specific column
    #[tracing::instrument(skip(self))]
    pub async fn get_column_comment(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<Option<String>> {
        let result: Option<(Option<String>,)> = sqlx::query_as(
            r"
            SELECT col_description(c.oid, a.attnum) as comment
            FROM pg_class c
            JOIN pg_namespace n ON c.relnamespace = n.oid
            JOIN pg_attribute a ON a.attrelid = c.oid
            WHERE c.relname = $1
              AND a.attname = $2
              AND n.nspname = 'public'
              AND a.attnum > 0
              AND NOT a.attisdropped
            ",
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.and_then(|r| r.0))
    }

    /// Get all column comments for a table
    #[tracing::instrument(skip(self))]
    pub async fn get_table_column_comments(&self, table_name: &str) -> Result<Vec<ColumnComment>> {
        let comments: Vec<ColumnComment> = sqlx::query_as(
            r"
            SELECT
                n.nspname as schema_name,
                c.relname as table_name,
                a.attname as column_name,
                col_description(c.oid, a.attnum) as comment
            FROM pg_class c
            JOIN pg_namespace n ON c.relnamespace = n.oid
            JOIN pg_attribute a ON a.attrelid = c.oid
            WHERE c.relname = $1
              AND n.nspname = 'public'
              AND a.attnum > 0
              AND NOT a.attisdropped
              AND col_description(c.oid, a.attnum) IS NOT NULL
            ORDER BY a.attnum
            ",
        )
        .bind(table_name)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(comments)
    }

    /// List all columns with comments across all tables
    #[tracing::instrument(skip(self))]
    pub async fn list_all_column_comments(&self) -> Result<Vec<ColumnComment>> {
        let comments: Vec<ColumnComment> = sqlx::query_as(
            r"
            SELECT
                n.nspname as schema_name,
                c.relname as table_name,
                a.attname as column_name,
                col_description(c.oid, a.attnum) as comment
            FROM pg_class c
            JOIN pg_namespace n ON c.relnamespace = n.oid
            JOIN pg_attribute a ON a.attrelid = c.oid
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
              AND c.relkind = 'r'
              AND a.attnum > 0
              AND NOT a.attisdropped
              AND col_description(c.oid, a.attnum) IS NOT NULL
            ORDER BY n.nspname, c.relname, a.attnum
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(comments)
    }

    // ========================================================================
    // Index Comments
    // ========================================================================

    /// Set a comment on an index
    #[tracing::instrument(skip(self))]
    pub async fn set_index_comment(&self, index_name: &str, comment: &str) -> Result<()> {
        let sql = format!("COMMENT ON INDEX {} IS $1", index_name);
        sqlx::query(&sql)
            .bind(comment)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Remove a comment from an index
    #[tracing::instrument(skip(self))]
    pub async fn remove_index_comment(&self, index_name: &str) -> Result<()> {
        let sql = format!("COMMENT ON INDEX {} IS NULL", index_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// List all indexes with comments
    #[tracing::instrument(skip(self))]
    pub async fn list_index_comments(&self) -> Result<Vec<ObjectComment>> {
        let comments: Vec<ObjectComment> = sqlx::query_as(
            r"
            SELECT
                n.nspname as schema_name,
                c.relname as object_name,
                obj_description(c.oid, 'pg_class') as comment
            FROM pg_class c
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
              AND c.relkind = 'i'
              AND obj_description(c.oid, 'pg_class') IS NOT NULL
            ORDER BY n.nspname, c.relname
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(comments)
    }

    // ========================================================================
    // Constraint Comments
    // ========================================================================

    /// Set a comment on a constraint
    #[tracing::instrument(skip(self))]
    pub async fn set_constraint_comment(&self, constraint_name: &str, comment: &str) -> Result<()> {
        let sql = format!(
            "COMMENT ON CONSTRAINT {} ON public.{} IS $1",
            constraint_name, constraint_name
        );
        sqlx::query(&sql)
            .bind(comment)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Set a comment on a table constraint
    #[tracing::instrument(skip(self))]
    pub async fn set_table_constraint_comment(
        &self,
        table_name: &str,
        constraint_name: &str,
        comment: &str,
    ) -> Result<()> {
        let sql = format!(
            "COMMENT ON CONSTRAINT {} ON {} IS $1",
            constraint_name, table_name
        );
        sqlx::query(&sql)
            .bind(comment)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Sequence Comments
    // ========================================================================

    /// Set a comment on a sequence
    #[tracing::instrument(skip(self))]
    pub async fn set_sequence_comment(&self, sequence_name: &str, comment: &str) -> Result<()> {
        let sql = format!("COMMENT ON SEQUENCE {} IS $1", sequence_name);
        sqlx::query(&sql)
            .bind(comment)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Remove a comment from a sequence
    #[tracing::instrument(skip(self))]
    pub async fn remove_sequence_comment(&self, sequence_name: &str) -> Result<()> {
        let sql = format!("COMMENT ON SEQUENCE {} IS NULL", sequence_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // View Comments
    // ========================================================================

    /// Set a comment on a view
    #[tracing::instrument(skip(self))]
    pub async fn set_view_comment(&self, view_name: &str, comment: &str) -> Result<()> {
        let sql = format!("COMMENT ON VIEW {} IS $1", view_name);
        sqlx::query(&sql)
            .bind(comment)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Remove a comment from a view
    #[tracing::instrument(skip(self))]
    pub async fn remove_view_comment(&self, view_name: &str) -> Result<()> {
        let sql = format!("COMMENT ON VIEW {} IS NULL", view_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Schema Comments
    // ========================================================================

    /// Set a comment on a schema
    #[tracing::instrument(skip(self))]
    pub async fn set_schema_comment(&self, schema_name: &str, comment: &str) -> Result<()> {
        let sql = format!("COMMENT ON SCHEMA {} IS $1", schema_name);
        sqlx::query(&sql)
            .bind(comment)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Remove a comment from a schema
    #[tracing::instrument(skip(self))]
    pub async fn remove_schema_comment(&self, schema_name: &str) -> Result<()> {
        let sql = format!("COMMENT ON SCHEMA {} IS NULL", schema_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Bulk Operations
    // ========================================================================

    /// Set comments on multiple columns at once
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::collections::HashMap;
    ///
    /// let mut columns = HashMap::new();
    /// columns.insert("id", "Primary key UUID");
    /// columns.insert("name", "Workflow display name");
    /// columns.insert("created_at", "Timestamp when workflow was created");
    ///
    /// comment_mgr.set_bulk_column_comments("workflows", columns).await?;
    /// ```
    #[tracing::instrument(skip(self, comments))]
    pub async fn set_bulk_column_comments(
        &self,
        table_name: &str,
        comments: std::collections::HashMap<&str, &str>,
    ) -> Result<usize> {
        let mut count = 0;
        for (column_name, comment) in comments {
            self.set_column_comment(table_name, column_name, comment)
                .await?;
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_comment_structure() {
        let comment = ObjectComment {
            schema_name: "public".to_string(),
            object_name: "users".to_string(),
            comment: Some("User accounts table".to_string()),
        };

        assert_eq!(comment.schema_name, "public");
        assert_eq!(comment.object_name, "users");
        assert!(comment.comment.is_some());
    }

    #[test]
    fn test_column_comment_structure() {
        let comment = ColumnComment {
            schema_name: "public".to_string(),
            table_name: "workflows".to_string(),
            column_name: "state".to_string(),
            comment: Some("Execution state".to_string()),
        };

        assert_eq!(comment.table_name, "workflows");
        assert_eq!(comment.column_name, "state");
        assert!(comment.comment.is_some());
    }
}
