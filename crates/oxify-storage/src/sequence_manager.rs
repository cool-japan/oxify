//! PostgreSQL Sequence Manager
//!
//! This module provides utilities for managing PostgreSQL sequences, which are used
//! to generate unique sequential numeric identifiers (like auto-incrementing IDs).
//!
//! # Use Cases
//!
//! - Custom auto-increment columns
//! - Order numbers, invoice numbers
//! - Guaranteed unique identifiers without gaps
//! - Counter management
//! - Distributed ID generation
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::SequenceManager;
//!
//! let seq_mgr = SequenceManager::new(pool.clone());
//!
//! // Create a sequence for order numbers
//! seq_mgr.create_sequence("order_numbers")
//!     .start_with(1000)
//!     .increment_by(1)
//!     .execute().await?;
//!
//! // Get next value
//! let order_num = seq_mgr.nextval("order_numbers").await?;
//!
//! // Reset sequence
//! seq_mgr.setval("order_numbers", 1000).await?;
//! ```

use crate::{Result, StorageError};
use sqlx::PgPool;
use std::sync::Arc;

/// Information about a PostgreSQL sequence
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SequenceInfo {
    /// Schema name
    pub schema_name: String,
    /// Sequence name
    pub sequence_name: String,
    /// Data type (bigint, integer, smallint)
    pub data_type: String,
    /// Start value
    pub start_value: String,
    /// Minimum value
    pub minimum_value: String,
    /// Maximum value
    pub maximum_value: String,
    /// Increment by
    pub increment: String,
    /// Whether sequence cycles on overflow
    pub cycle_option: bool,
}

/// Builder for creating sequences with custom configuration
#[derive(Debug, Clone)]
pub struct SequenceBuilder {
    name: String,
    start: Option<i64>,
    increment: Option<i64>,
    minvalue: Option<i64>,
    maxvalue: Option<i64>,
    cache: Option<i64>,
    cycle: bool,
    owned_by: Option<String>,
}

impl SequenceBuilder {
    /// Create a new sequence builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: None,
            increment: None,
            minvalue: None,
            maxvalue: None,
            cache: None,
            cycle: false,
            owned_by: None,
        }
    }

    /// Set the starting value
    pub fn start_with(mut self, value: i64) -> Self {
        self.start = Some(value);
        self
    }

    /// Set the increment value (can be negative)
    pub fn increment_by(mut self, value: i64) -> Self {
        self.increment = Some(value);
        self
    }

    /// Set the minimum value
    pub fn min_value(mut self, value: i64) -> Self {
        self.minvalue = Some(value);
        self
    }

    /// Set the maximum value
    pub fn max_value(mut self, value: i64) -> Self {
        self.maxvalue = Some(value);
        self
    }

    /// Set cache size for performance (number of sequence values to preallocate)
    pub fn cache(mut self, size: i64) -> Self {
        self.cache = Some(size);
        self
    }

    /// Enable cycling (wrap around when reaching min/max)
    pub fn cycle(mut self) -> Self {
        self.cycle = true;
        self
    }

    /// Do not cycle (fail when reaching min/max)
    pub fn no_cycle(mut self) -> Self {
        self.cycle = false;
        self
    }

    /// Set the owning column (sequence is dropped if column is dropped)
    pub fn owned_by(mut self, table_column: impl Into<String>) -> Self {
        self.owned_by = Some(table_column.into());
        self
    }

    /// Build the CREATE SEQUENCE SQL statement
    pub fn build_sql(&self) -> String {
        let mut sql = format!("CREATE SEQUENCE {}", self.name);

        if let Some(start) = self.start {
            sql.push_str(&format!(" START WITH {}", start));
        }

        if let Some(increment) = self.increment {
            sql.push_str(&format!(" INCREMENT BY {}", increment));
        }

        if let Some(minvalue) = self.minvalue {
            sql.push_str(&format!(" MINVALUE {}", minvalue));
        }

        if let Some(maxvalue) = self.maxvalue {
            sql.push_str(&format!(" MAXVALUE {}", maxvalue));
        }

        if let Some(cache) = self.cache {
            sql.push_str(&format!(" CACHE {}", cache));
        }

        if self.cycle {
            sql.push_str(" CYCLE");
        }

        if let Some(ref owned_by) = self.owned_by {
            sql.push_str(&format!(" OWNED BY {}", owned_by));
        }

        sql
    }

    /// Execute the CREATE SEQUENCE command
    pub async fn execute(&self, pool: &PgPool) -> Result<()> {
        let sql = self.build_sql();
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }
}

/// PostgreSQL Sequence Manager
///
/// Provides methods for creating, dropping, and manipulating sequences.
#[derive(Clone)]
pub struct SequenceManager {
    pool: Arc<PgPool>,
}

impl SequenceManager {
    /// Create a new sequence manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // ========================================================================
    // Sequence Creation and Removal
    // ========================================================================

    /// Create a simple sequence with default settings
    ///
    /// # Example
    ///
    /// ```ignore
    /// seq_mgr.create_simple_sequence("my_sequence").await?;
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn create_simple_sequence(&self, sequence_name: &str) -> Result<()> {
        let sql = format!("CREATE SEQUENCE {}", sequence_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Create a sequence with a builder for custom configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// let builder = seq_mgr.create_sequence("order_ids")
    ///     .start_with(1000)
    ///     .increment_by(1)
    ///     .cache(20);
    ///
    /// builder.execute(&pool).await?;
    /// ```
    pub fn create_sequence(&self, sequence_name: impl Into<String>) -> SequenceBuilder {
        SequenceBuilder::new(sequence_name)
    }

    /// Create sequence if it doesn't exist
    #[tracing::instrument(skip(self))]
    pub async fn create_sequence_if_not_exists(&self, sequence_name: &str) -> Result<bool> {
        if self.sequence_exists(sequence_name).await? {
            return Ok(false);
        }

        self.create_simple_sequence(sequence_name).await?;
        Ok(true)
    }

    /// Drop a sequence
    #[tracing::instrument(skip(self))]
    pub async fn drop_sequence(&self, sequence_name: &str) -> Result<()> {
        let sql = format!("DROP SEQUENCE {}", sequence_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop sequence if it exists
    #[tracing::instrument(skip(self))]
    pub async fn drop_sequence_if_exists(&self, sequence_name: &str) -> Result<()> {
        let sql = format!("DROP SEQUENCE IF EXISTS {}", sequence_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop sequence with CASCADE to remove dependent objects
    #[tracing::instrument(skip(self))]
    pub async fn drop_sequence_cascade(&self, sequence_name: &str) -> Result<()> {
        let sql = format!("DROP SEQUENCE {} CASCADE", sequence_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Sequence Operations
    // ========================================================================

    /// Get the next value from a sequence
    ///
    /// This advances the sequence and returns the new value.
    #[tracing::instrument(skip(self))]
    pub async fn nextval(&self, sequence_name: &str) -> Result<i64> {
        let sql = format!("SELECT nextval('{}')", sequence_name);
        let result: (i64,) = sqlx::query_as(&sql)
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Get the current value of a sequence without advancing it
    ///
    /// Returns an error if nextval has never been called in this session.
    #[tracing::instrument(skip(self))]
    pub async fn currval(&self, sequence_name: &str) -> Result<i64> {
        let sql = format!("SELECT currval('{}')", sequence_name);
        let result: (i64,) = sqlx::query_as(&sql)
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Get the last value returned by nextval (in any session)
    ///
    /// This is safe to call even if nextval has never been called.
    #[tracing::instrument(skip(self))]
    pub async fn last_value(&self, sequence_name: &str) -> Result<i64> {
        let sql = format!("SELECT last_value FROM {}", sequence_name);
        let result: (i64,) = sqlx::query_as(&sql)
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Set the sequence value
    ///
    /// The next call to nextval will return value + increment.
    #[tracing::instrument(skip(self))]
    pub async fn setval(&self, sequence_name: &str, value: i64) -> Result<()> {
        let sql = format!("SELECT setval('{}', {})", sequence_name, value);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Set the sequence value with is_called flag
    ///
    /// If is_called is true, the next nextval will return value + increment.
    /// If is_called is false, the next nextval will return value.
    #[tracing::instrument(skip(self))]
    pub async fn setval_with_is_called(
        &self,
        sequence_name: &str,
        value: i64,
        is_called: bool,
    ) -> Result<()> {
        let sql = format!(
            "SELECT setval('{}', {}, {})",
            sequence_name, value, is_called
        );
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Reset a sequence to its start value
    #[tracing::instrument(skip(self))]
    pub async fn reset_sequence(&self, sequence_name: &str) -> Result<()> {
        // First get the start value
        let info = self.get_sequence_info(sequence_name).await?;
        if let Some(info) = info {
            let start_value: i64 = info.start_value.parse().unwrap_or(1);
            self.setval_with_is_called(sequence_name, start_value, false)
                .await?;
        }

        Ok(())
    }

    // ========================================================================
    // Sequence Information
    // ========================================================================

    /// List all sequences in the database
    #[tracing::instrument(skip(self))]
    pub async fn list_sequences(&self) -> Result<Vec<SequenceInfo>> {
        let sequences: Vec<SequenceInfo> = sqlx::query_as(
            r"
            SELECT
                n.nspname as schema_name,
                c.relname as sequence_name,
                format_type(s.seqtypid, NULL) as data_type,
                s.seqstart::text as start_value,
                s.seqmin::text as minimum_value,
                s.seqmax::text as maximum_value,
                s.seqincrement::text as increment,
                s.seqcycle as cycle_option
            FROM pg_sequence s
            JOIN pg_class c ON s.seqrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY n.nspname, c.relname
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(sequences)
    }

    /// Get information about a specific sequence
    #[tracing::instrument(skip(self))]
    pub async fn get_sequence_info(&self, sequence_name: &str) -> Result<Option<SequenceInfo>> {
        let sequence: Option<SequenceInfo> = sqlx::query_as(
            r"
            SELECT
                n.nspname as schema_name,
                c.relname as sequence_name,
                format_type(s.seqtypid, NULL) as data_type,
                s.seqstart::text as start_value,
                s.seqmin::text as minimum_value,
                s.seqmax::text as maximum_value,
                s.seqincrement::text as increment,
                s.seqcycle as cycle_option
            FROM pg_sequence s
            JOIN pg_class c ON s.seqrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE c.relname = $1
              AND n.nspname = 'public'
            ",
        )
        .bind(sequence_name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(sequence)
    }

    /// Check if a sequence exists
    #[tracing::instrument(skip(self))]
    pub async fn sequence_exists(&self, sequence_name: &str) -> Result<bool> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM pg_class c
                JOIN pg_namespace n ON c.relnamespace = n.oid
                WHERE c.relname = $1
                  AND c.relkind = 'S'
                  AND n.nspname = 'public'
            )",
        )
        .bind(sequence_name)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Rename a sequence
    #[tracing::instrument(skip(self))]
    pub async fn rename_sequence(&self, old_name: &str, new_name: &str) -> Result<()> {
        let sql = format!("ALTER SEQUENCE {} RENAME TO {}", old_name, new_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Set the owner of a sequence (ties it to a table column)
    #[tracing::instrument(skip(self))]
    pub async fn set_sequence_owner(
        &self,
        sequence_name: &str,
        table_name: &str,
        column_name: &str,
    ) -> Result<()> {
        let sql = format!(
            "ALTER SEQUENCE {} OWNED BY {}.{}",
            sequence_name, table_name, column_name
        );
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_builder_basic() {
        let builder = SequenceBuilder::new("test_seq")
            .start_with(100)
            .increment_by(1);

        let sql = builder.build_sql();
        assert!(sql.contains("CREATE SEQUENCE test_seq"));
        assert!(sql.contains("START WITH 100"));
        assert!(sql.contains("INCREMENT BY 1"));
    }

    #[test]
    fn test_sequence_builder_full() {
        let builder = SequenceBuilder::new("full_seq")
            .start_with(1)
            .increment_by(2)
            .min_value(1)
            .max_value(1000)
            .cache(20)
            .cycle();

        let sql = builder.build_sql();
        assert!(sql.contains("CREATE SEQUENCE full_seq"));
        assert!(sql.contains("START WITH 1"));
        assert!(sql.contains("INCREMENT BY 2"));
        assert!(sql.contains("MINVALUE 1"));
        assert!(sql.contains("MAXVALUE 1000"));
        assert!(sql.contains("CACHE 20"));
        assert!(sql.contains("CYCLE"));
    }

    #[test]
    fn test_sequence_builder_owned_by() {
        let builder = SequenceBuilder::new("owned_seq").owned_by("users.id");

        let sql = builder.build_sql();
        assert!(sql.contains("CREATE SEQUENCE owned_seq"));
        assert!(sql.contains("OWNED BY users.id"));
    }

    #[test]
    fn test_sequence_info_structure() {
        let info = SequenceInfo {
            schema_name: "public".to_string(),
            sequence_name: "test_seq".to_string(),
            data_type: "bigint".to_string(),
            start_value: "1".to_string(),
            minimum_value: "1".to_string(),
            maximum_value: "9223372036854775807".to_string(),
            increment: "1".to_string(),
            cycle_option: false,
        };

        assert_eq!(info.sequence_name, "test_seq");
        assert_eq!(info.data_type, "bigint");
        assert!(!info.cycle_option);
    }
}
