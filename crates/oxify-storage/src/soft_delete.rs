//! Soft delete utilities for marking records as deleted without physical removal
//!
//! This module provides standardized patterns for implementing soft deletes,
//! which mark records as deleted without removing them from the database.
//!
//! # Benefits of Soft Deletes
//!
//! - **Data Recovery**: Accidentally deleted records can be restored
//! - **Audit Trail**: Maintain complete history of all operations
//! - **Referential Integrity**: Foreign key constraints remain valid
//! - **Performance**: Faster than cascading deletes
//!
//! # Usage Patterns
//!
//! ## Basic Soft Delete
//! ```ignore
//! use oxify_storage::soft_delete::SoftDeleteBuilder;
//!
//! SoftDeleteBuilder::new("workflows")
//!     .where_clause("id = $1")
//!     .mark_deleted(&pool, &[&workflow_id])
//!     .await?;
//! ```
//!
//! ## Soft Delete with Metadata
//! ```ignore
//! use oxify_storage::soft_delete::SoftDeleteMetadata;
//!
//! let metadata = SoftDeleteMetadata::new(user_id, Some("User requested deletion"));
//! SoftDeleteBuilder::new("workflows")
//!     .where_clause("id = $1")
//!     .with_metadata(metadata)
//!     .mark_deleted(&pool, &[&workflow_id])
//!     .await?;
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Result, StorageError};

/// Metadata for soft delete operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftDeleteMetadata {
    /// User who performed the deletion
    pub deleted_by: Option<Uuid>,
    /// Timestamp of deletion
    pub deleted_at: DateTime<Utc>,
    /// Reason for deletion
    pub deletion_reason: Option<String>,
}

impl SoftDeleteMetadata {
    /// Create new soft delete metadata
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::soft_delete::SoftDeleteMetadata;
    /// # use uuid::Uuid;
    /// let user_id = Uuid::new_v4();
    /// let metadata = SoftDeleteMetadata::new(user_id, Some("User requested"));
    /// ```
    pub fn new(deleted_by: Uuid, deletion_reason: Option<&str>) -> Self {
        Self {
            deleted_by: Some(deleted_by),
            deleted_at: Utc::now(),
            deletion_reason: deletion_reason.map(|s| s.to_string()),
        }
    }

    /// Create soft delete metadata without user tracking
    pub fn anonymous(deletion_reason: Option<&str>) -> Self {
        Self {
            deleted_by: None,
            deleted_at: Utc::now(),
            deletion_reason: deletion_reason.map(|s| s.to_string()),
        }
    }
}

/// Builder for constructing soft delete queries
///
/// This builder helps construct safe, parameterized soft delete queries.
pub struct SoftDeleteBuilder {
    table: String,
    where_clause: Option<String>,
    deleted_at_column: String,
    deleted_by_column: Option<String>,
    deletion_reason_column: Option<String>,
    metadata: Option<SoftDeleteMetadata>,
}

impl SoftDeleteBuilder {
    /// Create a new soft delete builder
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::soft_delete::SoftDeleteBuilder;
    /// let builder = SoftDeleteBuilder::new("workflows");
    /// ```
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            where_clause: None,
            deleted_at_column: "deleted_at".to_string(),
            deleted_by_column: None,
            deletion_reason_column: None,
            metadata: None,
        }
    }

    /// Set the WHERE clause for the soft delete
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::soft_delete::SoftDeleteBuilder;
    /// let builder = SoftDeleteBuilder::new("workflows")
    ///     .where_clause("id = $1");
    /// ```
    pub fn where_clause(mut self, clause: &str) -> Self {
        self.where_clause = Some(clause.to_string());
        self
    }

    /// Set custom column name for deleted_at timestamp
    pub fn deleted_at_column(mut self, column: &str) -> Self {
        self.deleted_at_column = column.to_string();
        self
    }

    /// Enable tracking of who deleted the record
    pub fn with_deleted_by_column(mut self, column: &str) -> Self {
        self.deleted_by_column = Some(column.to_string());
        self
    }

    /// Enable tracking of deletion reason
    pub fn with_deletion_reason_column(mut self, column: &str) -> Self {
        self.deletion_reason_column = Some(column.to_string());
        self
    }

    /// Set metadata for the soft delete operation
    pub fn with_metadata(mut self, metadata: SoftDeleteMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Build the UPDATE query SQL
    fn build_query(&self) -> Result<String> {
        if self.where_clause.is_none() {
            return Err(StorageError::ValidationError(
                "WHERE clause is required for soft delete".to_string(),
            ));
        }

        let mut updates = vec![format!("{} = NOW()", self.deleted_at_column)];

        if let Some(ref metadata) = self.metadata {
            if let Some(ref col) = self.deleted_by_column {
                if let Some(user_id) = metadata.deleted_by {
                    updates.push(format!("{col} = '{user_id}'"));
                }
            }

            if let Some(ref col) = self.deletion_reason_column {
                if let Some(ref reason) = metadata.deletion_reason {
                    // Note: This should use parameterized queries in production
                    let escaped_reason = reason.replace('\'', "''");
                    updates.push(format!("{col} = '{escaped_reason}'"));
                }
            }
        }

        let update_clause = updates.join(", ");
        let where_clause = self
            .where_clause
            .as_ref()
            .expect("invariant: where_clause checked non-None above");

        Ok(format!(
            "UPDATE {} SET {} WHERE {} AND {} IS NULL",
            self.table, update_clause, where_clause, self.deleted_at_column
        ))
    }

    /// Get the SQL query for soft delete
    ///
    /// Use this to execute the soft delete with your own parameters.
    ///
    /// # Examples
    /// ```ignore
    /// let builder = SoftDeleteBuilder::new("workflows").where_clause("id = $1");
    /// let query = builder.soft_delete_query()?;
    /// let result = sqlx::query(&query)
    ///     .bind(workflow_id)
    ///     .execute(pool)
    ///     .await?;
    /// ```
    pub fn soft_delete_query(&self) -> Result<String> {
        self.build_query()
    }
}

/// Helper for querying only non-deleted records
pub struct SoftDeleteFilter {
    deleted_at_column: String,
}

impl SoftDeleteFilter {
    /// Create a new soft delete filter
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::soft_delete::SoftDeleteFilter;
    /// let filter = SoftDeleteFilter::new("deleted_at");
    /// assert_eq!(filter.not_deleted_clause(), "deleted_at IS NULL");
    /// ```
    pub fn new(deleted_at_column: &str) -> Self {
        Self {
            deleted_at_column: deleted_at_column.to_string(),
        }
    }

    /// Get SQL clause for filtering non-deleted records
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::soft_delete::SoftDeleteFilter;
    /// let filter = SoftDeleteFilter::default();
    /// let clause = filter.not_deleted_clause();
    /// assert_eq!(clause, "deleted_at IS NULL");
    /// ```
    pub fn not_deleted_clause(&self) -> String {
        format!("{} IS NULL", self.deleted_at_column)
    }

    /// Get SQL clause for filtering deleted records
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::soft_delete::SoftDeleteFilter;
    /// let filter = SoftDeleteFilter::default();
    /// let clause = filter.deleted_clause();
    /// assert_eq!(clause, "deleted_at IS NOT NULL");
    /// ```
    pub fn deleted_clause(&self) -> String {
        format!("{} IS NOT NULL", self.deleted_at_column)
    }

    /// Get SQL clause for filtering by deletion time range
    pub fn deleted_between_clause(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> String {
        format!(
            "{} BETWEEN '{}' AND '{}'",
            self.deleted_at_column,
            start.to_rfc3339(),
            end.to_rfc3339()
        )
    }
}

impl Default for SoftDeleteFilter {
    fn default() -> Self {
        Self::new("deleted_at")
    }
}

/// Helper for restoring soft-deleted records
pub struct SoftDeleteRestorer {
    table: String,
    where_clause: Option<String>,
    deleted_at_column: String,
    deleted_by_column: Option<String>,
    deletion_reason_column: Option<String>,
}

impl SoftDeleteRestorer {
    /// Create a new soft delete restorer
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::soft_delete::SoftDeleteRestorer;
    /// let restorer = SoftDeleteRestorer::new("workflows");
    /// ```
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            where_clause: None,
            deleted_at_column: "deleted_at".to_string(),
            deleted_by_column: None,
            deletion_reason_column: None,
        }
    }

    /// Set the WHERE clause for the restore
    pub fn where_clause(mut self, clause: &str) -> Self {
        self.where_clause = Some(clause.to_string());
        self
    }

    /// Set custom column name for deleted_at timestamp
    pub fn deleted_at_column(mut self, column: &str) -> Self {
        self.deleted_at_column = column.to_string();
        self
    }

    /// Clear deleted_by column on restore
    pub fn clear_deleted_by_column(mut self, column: &str) -> Self {
        self.deleted_by_column = Some(column.to_string());
        self
    }

    /// Clear deletion_reason column on restore
    pub fn clear_deletion_reason_column(mut self, column: &str) -> Self {
        self.deletion_reason_column = Some(column.to_string());
        self
    }

    /// Build the restore query SQL
    fn build_query(&self) -> Result<String> {
        if self.where_clause.is_none() {
            return Err(StorageError::ValidationError(
                "WHERE clause is required for restore".to_string(),
            ));
        }

        let mut updates = vec![format!("{} = NULL", self.deleted_at_column)];

        if let Some(ref col) = self.deleted_by_column {
            updates.push(format!("{col} = NULL"));
        }

        if let Some(ref col) = self.deletion_reason_column {
            updates.push(format!("{col} = NULL"));
        }

        let update_clause = updates.join(", ");
        let where_clause = self
            .where_clause
            .as_ref()
            .expect("invariant: where_clause checked non-None above");

        Ok(format!(
            "UPDATE {} SET {} WHERE {} AND {} IS NOT NULL",
            self.table, update_clause, where_clause, self.deleted_at_column
        ))
    }

    /// Get the SQL query for restore
    ///
    /// Use this to execute the restore with your own parameters.
    ///
    /// # Examples
    /// ```ignore
    /// let restorer = SoftDeleteRestorer::new("workflows").where_clause("id = $1");
    /// let query = restorer.restore_query()?;
    /// let result = sqlx::query(&query)
    ///     .bind(workflow_id)
    ///     .execute(pool)
    ///     .await?;
    /// ```
    pub fn restore_query(&self) -> Result<String> {
        self.build_query()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_delete_metadata_new() {
        let user_id = Uuid::new_v4();
        let metadata = SoftDeleteMetadata::new(user_id, Some("Test reason"));

        assert_eq!(metadata.deleted_by, Some(user_id));
        assert_eq!(metadata.deletion_reason, Some("Test reason".to_string()));
    }

    #[test]
    fn test_soft_delete_metadata_anonymous() {
        let metadata = SoftDeleteMetadata::anonymous(Some("System cleanup"));

        assert_eq!(metadata.deleted_by, None);
        assert_eq!(metadata.deletion_reason, Some("System cleanup".to_string()));
    }

    #[test]
    fn test_soft_delete_builder_basic_query() {
        let builder = SoftDeleteBuilder::new("workflows").where_clause("id = $1");

        let query = builder.build_query().unwrap();
        assert!(query.contains("UPDATE workflows"));
        assert!(query.contains("SET deleted_at = NOW()"));
        assert!(query.contains("WHERE id = $1"));
        assert!(query.contains("AND deleted_at IS NULL"));
    }

    #[test]
    fn test_soft_delete_builder_no_where_clause() {
        let builder = SoftDeleteBuilder::new("workflows");

        let result = builder.build_query();
        assert!(result.is_err());
        match result {
            Err(StorageError::ValidationError(msg)) => {
                assert!(msg.contains("WHERE clause is required"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_soft_delete_builder_custom_columns() {
        let builder = SoftDeleteBuilder::new("workflows")
            .where_clause("id = $1")
            .deleted_at_column("removed_at")
            .with_deleted_by_column("removed_by")
            .with_deletion_reason_column("removal_reason");

        let query = builder.build_query().unwrap();
        assert!(query.contains("removed_at = NOW()"));
        assert!(query.contains("AND removed_at IS NULL"));
    }

    #[test]
    fn test_soft_delete_filter_default() {
        let filter = SoftDeleteFilter::default();
        assert_eq!(filter.not_deleted_clause(), "deleted_at IS NULL");
        assert_eq!(filter.deleted_clause(), "deleted_at IS NOT NULL");
    }

    #[test]
    fn test_soft_delete_filter_custom_column() {
        let filter = SoftDeleteFilter::new("removed_at");
        assert_eq!(filter.not_deleted_clause(), "removed_at IS NULL");
        assert_eq!(filter.deleted_clause(), "removed_at IS NOT NULL");
    }

    #[test]
    fn test_soft_delete_filter_time_range() {
        let filter = SoftDeleteFilter::default();
        let start = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-12-31T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc);

        let clause = filter.deleted_between_clause(start, end);
        assert!(clause.contains("deleted_at BETWEEN"));
        assert!(clause.contains("2026-01-01"));
        assert!(clause.contains("2026-12-31"));
    }

    #[test]
    fn test_soft_delete_restorer_basic_query() {
        let restorer = SoftDeleteRestorer::new("workflows").where_clause("id = $1");

        let query = restorer.build_query().unwrap();
        assert!(query.contains("UPDATE workflows"));
        assert!(query.contains("SET deleted_at = NULL"));
        assert!(query.contains("WHERE id = $1"));
        assert!(query.contains("AND deleted_at IS NOT NULL"));
    }

    #[test]
    fn test_soft_delete_restorer_with_metadata_columns() {
        let restorer = SoftDeleteRestorer::new("workflows")
            .where_clause("id = $1")
            .clear_deleted_by_column("deleted_by")
            .clear_deletion_reason_column("deletion_reason");

        let query = restorer.build_query().unwrap();
        assert!(query.contains("deleted_at = NULL"));
        assert!(query.contains("deleted_by = NULL"));
        assert!(query.contains("deletion_reason = NULL"));
    }

    #[test]
    fn test_soft_delete_restorer_no_where_clause() {
        let restorer = SoftDeleteRestorer::new("workflows");

        let result = restorer.build_query();
        assert!(result.is_err());
        match result {
            Err(StorageError::ValidationError(msg)) => {
                assert!(msg.contains("WHERE clause is required"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }
}
