//! Optimistic locking utilities for handling concurrent updates
//!
//! This module provides utilities for implementing optimistic locking,
//! a concurrency control method that assumes conflicts are rare and checks
//! for conflicts only at update time.
//!
//! # How Optimistic Locking Works
//!
//! 1. Each record has a version number or timestamp
//! 2. When reading, capture the current version
//! 3. When updating, check if version matches
//! 4. If version changed, another update occurred (conflict)
//! 5. Handle conflict by retrying or failing
//!
//! # Benefits
//!
//! - **No Locks**: Avoids lock contention and deadlocks
//! - **Better Performance**: No waiting for locks
//! - **Scalability**: Works well with distributed systems
//! - **Simple**: Easy to understand and implement
//!
//! # Usage Examples
//!
//! ## Basic Version-Based Locking
//! ```ignore
//! use oxify_storage::optimistic_lock::{OptimisticLock, VersionType};
//!
//! // Read current version
//! let (workflow, version) = get_workflow_with_version(id).await?;
//!
//! // Update with version check
//! let lock = OptimisticLock::version("workflows", "id", VersionType::Integer);
//! lock.update(
//!     &pool,
//!     &id,
//!     version,
//!     "SET name = $1",
//!     &[&new_name],
//! ).await?;
//! ```
//!
//! ## Timestamp-Based Locking
//! ```ignore
//! use oxify_storage::optimistic_lock::{OptimisticLock, VersionType};
//!
//! let lock = OptimisticLock::timestamp("workflows", "id", "updated_at");
//! lock.update_with_retry(
//!     &pool,
//!     &id,
//!     last_updated_at,
//!     "SET name = $1",
//!     &[&new_name],
//!     3, // max retries
//! ).await?;
//! ```

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{Result, StorageError};

/// Version type for optimistic locking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    /// Integer version number (incremented on each update)
    Integer,
    /// Timestamp-based versioning (updated_at column)
    Timestamp,
}

/// Optimistic lock configuration
#[derive(Debug, Clone)]
pub struct OptimisticLock {
    /// Table name
    table: String,
    /// Primary key column name
    id_column: String,
    /// Version column name
    version_column: String,
    /// Version type
    version_type: VersionType,
}

impl OptimisticLock {
    /// Create a new optimistic lock with integer version
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::optimistic_lock::{OptimisticLock, VersionType};
    /// let lock = OptimisticLock::version("workflows", "id", VersionType::Integer);
    /// ```
    pub fn version(table: &str, id_column: &str, version_type: VersionType) -> Self {
        let version_column = match version_type {
            VersionType::Integer => "version",
            VersionType::Timestamp => "updated_at",
        };

        Self {
            table: table.to_string(),
            id_column: id_column.to_string(),
            version_column: version_column.to_string(),
            version_type,
        }
    }

    /// Create a new optimistic lock with timestamp version
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::optimistic_lock::OptimisticLock;
    /// let lock = OptimisticLock::timestamp("workflows", "id", "updated_at");
    /// ```
    pub fn timestamp(table: &str, id_column: &str, updated_at_column: &str) -> Self {
        Self {
            table: table.to_string(),
            id_column: id_column.to_string(),
            version_column: updated_at_column.to_string(),
            version_type: VersionType::Timestamp,
        }
    }

    /// Create with custom version column name
    pub fn with_version_column(mut self, column: &str) -> Self {
        self.version_column = column.to_string();
        self
    }

    /// Build the UPDATE query with version check
    fn build_update_query(&self, update_clause: &str) -> String {
        let version_update = match self.version_type {
            VersionType::Integer => {
                format!("{} = {} + 1", self.version_column, self.version_column)
            }
            VersionType::Timestamp => format!("{} = NOW()", self.version_column),
        };

        format!(
            "UPDATE {} SET {}, {} WHERE {} = $1 AND {} = $2",
            self.table, update_clause, version_update, self.id_column, self.version_column
        )
    }

    /// Get the SQL query for update with version check
    ///
    /// Use this to build your own update query with version checking.
    ///
    /// # Examples
    /// ```ignore
    /// let lock = OptimisticLock::version("workflows", "id", VersionType::Integer);
    /// let query = lock.update_query("name = $3, description = $4");
    /// let result = sqlx::query(&query)
    ///     .bind(workflow_id)
    ///     .bind(current_version)
    ///     .bind(new_name)
    ///     .bind(new_description)
    ///     .execute(pool)
    ///     .await?;
    ///
    /// if result.rows_affected() == 0 {
    ///     return Err(StorageError::ConcurrentModification("...".to_string()));
    /// }
    /// ```
    pub fn update_query(&self, update_clause: &str) -> String {
        self.build_update_query(update_clause)
    }

    /// Execute an update with automatic retry on version mismatch
    ///
    /// This will retry the operation up to `max_retries` times if a concurrent
    /// modification is detected. The `update_fn` should reload the record,
    /// perform the update attempt, and return whether it succeeded.
    ///
    /// Note: This is a simplified version. For production use, consider using
    /// a more sophisticated retry mechanism with exponential backoff.
    #[allow(dead_code)]
    pub async fn update_with_retry<F, Fut>(
        &self,
        _pool: &PgPool,
        mut update_fn: F,
        max_retries: u32,
    ) -> Result<()>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<bool>>,
    {
        let mut attempts = 0;

        loop {
            attempts += 1;

            // Try to update
            let success = update_fn().await?;

            if success {
                return Ok(());
            }

            // Version mismatch - check if we should retry
            if attempts >= max_retries {
                return Err(StorageError::ConcurrentModification(format!(
                    "Failed to update after {attempts} attempts due to concurrent modifications"
                )));
            }

            // Brief delay before retry
            tokio::time::sleep(tokio::time::Duration::from_millis(10 * attempts as u64)).await;
        }
    }
}

/// Helper for checking if a record has been modified
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct VersionChecker {
    table: String,
    id_column: String,
    version_column: String,
    version_type: VersionType,
}

impl VersionChecker {
    /// Create a new version checker
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::optimistic_lock::{VersionChecker, VersionType};
    /// let checker = VersionChecker::new("workflows", "id", "version", VersionType::Integer);
    /// ```
    pub fn new(
        table: &str,
        id_column: &str,
        version_column: &str,
        version_type: VersionType,
    ) -> Self {
        Self {
            table: table.to_string(),
            id_column: id_column.to_string(),
            version_column: version_column.to_string(),
            version_type,
        }
    }

    /// Get the SQL query for checking version
    ///
    /// Use this to build your own version check query.
    ///
    /// # Examples
    /// ```ignore
    /// let checker = VersionChecker::new("workflows", "id", "version", VersionType::Integer);
    /// let query = checker.version_query();
    /// let row: (i32,) = sqlx::query_as(&query)
    ///     .bind(workflow_id)
    ///     .fetch_one(pool)
    ///     .await?;
    /// let has_changed = row.0 != expected_version;
    /// ```
    pub fn version_query(&self) -> String {
        format!(
            "SELECT {} FROM {} WHERE {} = $1",
            self.version_column, self.table, self.id_column
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimistic_lock_version_integer() {
        let lock = OptimisticLock::version("workflows", "id", VersionType::Integer);
        assert_eq!(lock.table, "workflows");
        assert_eq!(lock.id_column, "id");
        assert_eq!(lock.version_column, "version");
        assert_eq!(lock.version_type, VersionType::Integer);
    }

    #[test]
    fn test_optimistic_lock_version_timestamp() {
        let lock = OptimisticLock::version("workflows", "id", VersionType::Timestamp);
        assert_eq!(lock.version_column, "updated_at");
        assert_eq!(lock.version_type, VersionType::Timestamp);
    }

    #[test]
    fn test_optimistic_lock_timestamp() {
        let lock = OptimisticLock::timestamp("users", "user_id", "modified_at");
        assert_eq!(lock.table, "users");
        assert_eq!(lock.id_column, "user_id");
        assert_eq!(lock.version_column, "modified_at");
        assert_eq!(lock.version_type, VersionType::Timestamp);
    }

    #[test]
    fn test_optimistic_lock_custom_version_column() {
        let lock = OptimisticLock::version("workflows", "id", VersionType::Integer)
            .with_version_column("revision");
        assert_eq!(lock.version_column, "revision");
    }

    #[test]
    fn test_build_update_query_integer() {
        let lock = OptimisticLock::version("workflows", "id", VersionType::Integer);
        let query = lock.build_update_query("name = $3");

        assert!(query.contains("UPDATE workflows"));
        assert!(query.contains("SET name = $3"));
        assert!(query.contains("version = version + 1"));
        assert!(query.contains("WHERE id = $1"));
        assert!(query.contains("AND version = $2"));
    }

    #[test]
    fn test_build_update_query_timestamp() {
        let lock = OptimisticLock::timestamp("workflows", "id", "updated_at");
        let query = lock.build_update_query("name = $3");

        assert!(query.contains("UPDATE workflows"));
        assert!(query.contains("SET name = $3"));
        assert!(query.contains("updated_at = NOW()"));
        assert!(query.contains("WHERE id = $1"));
        assert!(query.contains("AND updated_at = $2"));
    }

    #[test]
    fn test_version_checker_new() {
        let checker = VersionChecker::new("workflows", "id", "version", VersionType::Integer);

        assert_eq!(checker.table, "workflows");
        assert_eq!(checker.id_column, "id");
        assert_eq!(checker.version_column, "version");
        assert_eq!(checker.version_type, VersionType::Integer);
    }
}
