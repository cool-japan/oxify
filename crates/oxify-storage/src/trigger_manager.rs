//! PostgreSQL Trigger Manager
//!
//! This module provides utilities for managing PostgreSQL triggers, which are database
//! callbacks that automatically execute functions when specific events occur on tables.
//!
//! # Use Cases
//!
//! - **Audit Logging**: Automatically track changes to tables
//! - **Data Validation**: Enforce complex business rules
//! - **Derived Data**: Automatically update related tables
//! - **Notifications**: Send events when data changes
//! - **Timestamps**: Automatically set updated_at columns
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{TriggerManager, TriggerTiming, TriggerEvent};
//!
//! let trig_mgr = TriggerManager::new(pool.clone());
//!
//! // Create a trigger to set updated_at timestamp
//! trig_mgr.create_trigger("set_updated_at")
//!     .on_table("workflows")
//!     .timing(TriggerTiming::Before)
//!     .events(vec![TriggerEvent::Update])
//!     .for_each_row()
//!     .execute_function("update_timestamp_function")
//!     .execute(&pool).await?;
//!
//! // List all triggers
//! let triggers = trig_mgr.list_triggers().await?;
//! ```

use crate::{Result, StorageError};
use sqlx::PgPool;
use std::sync::Arc;

/// Trigger timing (when the trigger fires)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerTiming {
    /// Before the event
    Before,
    /// After the event
    After,
    /// Instead of the event (for views)
    InsteadOf,
}

impl TriggerTiming {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Before => "BEFORE",
            Self::After => "AFTER",
            Self::InsteadOf => "INSTEAD OF",
        }
    }
}

/// Trigger event (what causes the trigger to fire)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEvent {
    /// INSERT operation
    Insert,
    /// UPDATE operation
    Update,
    /// DELETE operation
    Delete,
    /// TRUNCATE operation
    Truncate,
}

impl TriggerEvent {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::Truncate => "TRUNCATE",
        }
    }
}

/// Trigger granularity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerLevel {
    /// For each row affected
    Row,
    /// Once per statement
    Statement,
}

/// Information about a PostgreSQL trigger
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TriggerInfo {
    /// Schema name
    pub schema_name: String,
    /// Trigger name
    pub trigger_name: String,
    /// Table name
    pub table_name: String,
    /// Event (INSERT, UPDATE, DELETE, TRUNCATE)
    pub event: String,
    /// Timing (BEFORE, AFTER, INSTEAD OF)
    pub timing: String,
    /// Level (ROW or STATEMENT)
    pub level: String,
    /// Function name that trigger executes
    pub function_name: String,
    /// Is trigger enabled
    pub enabled: bool,
}

/// Builder for creating triggers
#[derive(Debug, Clone)]
pub struct TriggerBuilder {
    name: String,
    table_name: Option<String>,
    timing: Option<TriggerTiming>,
    events: Vec<TriggerEvent>,
    level: TriggerLevel,
    function_name: Option<String>,
    when_condition: Option<String>,
    update_columns: Vec<String>,
}

impl TriggerBuilder {
    /// Create a new trigger builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            table_name: None,
            timing: None,
            events: Vec::new(),
            level: TriggerLevel::Row,
            function_name: None,
            when_condition: None,
            update_columns: Vec::new(),
        }
    }

    /// Set the table this trigger applies to
    pub fn on_table(mut self, table_name: impl Into<String>) -> Self {
        self.table_name = Some(table_name.into());
        self
    }

    /// Set when the trigger fires (BEFORE, AFTER, INSTEAD OF)
    pub fn timing(mut self, timing: TriggerTiming) -> Self {
        self.timing = Some(timing);
        self
    }

    /// Set the events that trigger this (INSERT, UPDATE, DELETE, TRUNCATE)
    pub fn events(mut self, events: Vec<TriggerEvent>) -> Self {
        self.events = events;
        self
    }

    /// Add a single event
    pub fn event(mut self, event: TriggerEvent) -> Self {
        self.events.push(event);
        self
    }

    /// Fire trigger for each affected row (default)
    pub fn for_each_row(mut self) -> Self {
        self.level = TriggerLevel::Row;
        self
    }

    /// Fire trigger once per statement
    pub fn for_each_statement(mut self) -> Self {
        self.level = TriggerLevel::Statement;
        self
    }

    /// Set the function to execute when trigger fires
    pub fn execute_function(mut self, function_name: impl Into<String>) -> Self {
        self.function_name = Some(function_name.into());
        self
    }

    /// Add a WHEN condition (only fire trigger if condition is true)
    pub fn when_condition(mut self, condition: impl Into<String>) -> Self {
        self.when_condition = Some(condition.into());
        self
    }

    /// Specify columns for UPDATE OF trigger (only fire on specific column updates)
    pub fn update_of_columns(mut self, columns: Vec<String>) -> Self {
        self.update_columns = columns;
        self
    }

    /// Build the CREATE TRIGGER SQL statement
    pub fn build_sql(&self) -> Result<String> {
        if self.table_name.is_none() {
            return Err(StorageError::validation("Table name is required"));
        }

        if self.timing.is_none() {
            return Err(StorageError::validation("Trigger timing is required"));
        }

        if self.events.is_empty() {
            return Err(StorageError::validation(
                "At least one trigger event is required",
            ));
        }

        if self.function_name.is_none() {
            return Err(StorageError::validation("Function name is required"));
        }

        let mut sql = format!("CREATE TRIGGER {}", self.name);

        sql.push_str(&format!(" {}", self.timing.expect("invariant: timing checked non-None above").as_str()));

        let events_str = self
            .events
            .iter()
            .map(|e| {
                if *e == TriggerEvent::Update && !self.update_columns.is_empty() {
                    format!("UPDATE OF {}", self.update_columns.join(", "))
                } else {
                    e.as_str().to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" OR ");

        sql.push_str(&format!(
            " {} ON {}",
            events_str,
            self.table_name.as_ref().expect("invariant: table_name checked non-None above")
        ));

        match self.level {
            TriggerLevel::Row => sql.push_str(" FOR EACH ROW"),
            TriggerLevel::Statement => sql.push_str(" FOR EACH STATEMENT"),
        }

        if let Some(ref when_cond) = self.when_condition {
            sql.push_str(&format!(" WHEN ({})", when_cond));
        }

        sql.push_str(&format!(
            " EXECUTE FUNCTION {}()",
            self.function_name.as_ref().expect("invariant: function_name checked non-None above")
        ));

        Ok(sql)
    }

    /// Execute the CREATE TRIGGER command
    pub async fn execute(&self, pool: &PgPool) -> Result<()> {
        let sql = self.build_sql()?;
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }
}

/// PostgreSQL Trigger Manager
///
/// Provides methods for creating, dropping, and managing triggers.
#[derive(Clone)]
pub struct TriggerManager {
    pool: Arc<PgPool>,
}

impl TriggerManager {
    /// Create a new trigger manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // ========================================================================
    // Trigger Creation and Removal
    // ========================================================================

    /// Create a trigger with a builder for custom configuration
    pub fn create_trigger(&self, trigger_name: impl Into<String>) -> TriggerBuilder {
        TriggerBuilder::new(trigger_name)
    }

    /// Drop a trigger
    #[tracing::instrument(skip(self))]
    pub async fn drop_trigger(&self, trigger_name: &str, table_name: &str) -> Result<()> {
        let sql = format!("DROP TRIGGER {} ON {}", trigger_name, table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop trigger if it exists
    #[tracing::instrument(skip(self))]
    pub async fn drop_trigger_if_exists(&self, trigger_name: &str, table_name: &str) -> Result<()> {
        let sql = format!("DROP TRIGGER IF EXISTS {} ON {}", trigger_name, table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop trigger with CASCADE to remove dependent objects
    #[tracing::instrument(skip(self))]
    pub async fn drop_trigger_cascade(&self, trigger_name: &str, table_name: &str) -> Result<()> {
        let sql = format!("DROP TRIGGER {} ON {} CASCADE", trigger_name, table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Trigger Control
    // ========================================================================

    /// Enable a trigger
    #[tracing::instrument(skip(self))]
    pub async fn enable_trigger(&self, trigger_name: &str, table_name: &str) -> Result<()> {
        let sql = format!("ALTER TABLE {} ENABLE TRIGGER {}", table_name, trigger_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Disable a trigger
    #[tracing::instrument(skip(self))]
    pub async fn disable_trigger(&self, trigger_name: &str, table_name: &str) -> Result<()> {
        let sql = format!(
            "ALTER TABLE {} DISABLE TRIGGER {}",
            table_name, trigger_name
        );
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Enable all triggers on a table
    #[tracing::instrument(skip(self))]
    pub async fn enable_all_triggers(&self, table_name: &str) -> Result<()> {
        let sql = format!("ALTER TABLE {} ENABLE TRIGGER ALL", table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Disable all triggers on a table
    #[tracing::instrument(skip(self))]
    pub async fn disable_all_triggers(&self, table_name: &str) -> Result<()> {
        let sql = format!("ALTER TABLE {} DISABLE TRIGGER ALL", table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Trigger Information
    // ========================================================================

    /// List all triggers in the database
    #[tracing::instrument(skip(self))]
    pub async fn list_triggers(&self) -> Result<Vec<TriggerInfo>> {
        let triggers: Vec<TriggerInfo> = sqlx::query_as(
            r"
            SELECT
                n.nspname as schema_name,
                t.tgname as trigger_name,
                c.relname as table_name,
                CASE
                    WHEN t.tgtype & 4 != 0 THEN 'INSERT'
                    WHEN t.tgtype & 8 != 0 THEN 'DELETE'
                    WHEN t.tgtype & 16 != 0 THEN 'UPDATE'
                    WHEN t.tgtype & 32 != 0 THEN 'TRUNCATE'
                    ELSE 'UNKNOWN'
                END as event,
                CASE
                    WHEN t.tgtype & 2 != 0 THEN 'BEFORE'
                    WHEN t.tgtype & 64 != 0 THEN 'INSTEAD OF'
                    ELSE 'AFTER'
                END as timing,
                CASE WHEN t.tgtype & 1 != 0 THEN 'ROW' ELSE 'STATEMENT' END as level,
                p.proname as function_name,
                t.tgenabled = 'O' as enabled
            FROM pg_trigger t
            JOIN pg_class c ON t.tgrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            JOIN pg_proc p ON t.tgfoid = p.oid
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
              AND NOT t.tgisinternal
            ORDER BY n.nspname, c.relname, t.tgname
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(triggers)
    }

    /// List triggers for a specific table
    #[tracing::instrument(skip(self))]
    pub async fn list_table_triggers(&self, table_name: &str) -> Result<Vec<TriggerInfo>> {
        let triggers: Vec<TriggerInfo> = sqlx::query_as(
            r"
            SELECT
                n.nspname as schema_name,
                t.tgname as trigger_name,
                c.relname as table_name,
                CASE
                    WHEN t.tgtype & 4 != 0 THEN 'INSERT'
                    WHEN t.tgtype & 8 != 0 THEN 'DELETE'
                    WHEN t.tgtype & 16 != 0 THEN 'UPDATE'
                    WHEN t.tgtype & 32 != 0 THEN 'TRUNCATE'
                    ELSE 'UNKNOWN'
                END as event,
                CASE
                    WHEN t.tgtype & 2 != 0 THEN 'BEFORE'
                    WHEN t.tgtype & 64 != 0 THEN 'INSTEAD OF'
                    ELSE 'AFTER'
                END as timing,
                CASE WHEN t.tgtype & 1 != 0 THEN 'ROW' ELSE 'STATEMENT' END as level,
                p.proname as function_name,
                t.tgenabled = 'O' as enabled
            FROM pg_trigger t
            JOIN pg_class c ON t.tgrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            JOIN pg_proc p ON t.tgfoid = p.oid
            WHERE c.relname = $1
              AND n.nspname = 'public'
              AND NOT t.tgisinternal
            ORDER BY t.tgname
            ",
        )
        .bind(table_name)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(triggers)
    }

    /// Check if a trigger exists
    #[tracing::instrument(skip(self))]
    pub async fn trigger_exists(&self, trigger_name: &str, table_name: &str) -> Result<bool> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM pg_trigger t
                JOIN pg_class c ON t.tgrelid = c.oid
                JOIN pg_namespace n ON c.relnamespace = n.oid
                WHERE t.tgname = $1
                  AND c.relname = $2
                  AND n.nspname = 'public'
                  AND NOT t.tgisinternal
            )",
        )
        .bind(trigger_name)
        .bind(table_name)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Rename a trigger
    #[tracing::instrument(skip(self))]
    pub async fn rename_trigger(
        &self,
        old_name: &str,
        new_name: &str,
        table_name: &str,
    ) -> Result<()> {
        let sql = format!(
            "ALTER TRIGGER {} ON {} RENAME TO {}",
            old_name, table_name, new_name
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
    fn test_trigger_timing_as_str() {
        assert_eq!(TriggerTiming::Before.as_str(), "BEFORE");
        assert_eq!(TriggerTiming::After.as_str(), "AFTER");
        assert_eq!(TriggerTiming::InsteadOf.as_str(), "INSTEAD OF");
    }

    #[test]
    fn test_trigger_event_as_str() {
        assert_eq!(TriggerEvent::Insert.as_str(), "INSERT");
        assert_eq!(TriggerEvent::Update.as_str(), "UPDATE");
        assert_eq!(TriggerEvent::Delete.as_str(), "DELETE");
        assert_eq!(TriggerEvent::Truncate.as_str(), "TRUNCATE");
    }

    #[test]
    fn test_trigger_builder_basic() {
        let builder = TriggerBuilder::new("test_trigger")
            .on_table("users")
            .timing(TriggerTiming::Before)
            .events(vec![TriggerEvent::Insert])
            .execute_function("test_function");

        let sql = builder.build_sql().unwrap();
        assert!(sql.contains("CREATE TRIGGER test_trigger"));
        assert!(sql.contains("BEFORE"));
        assert!(sql.contains("INSERT"));
        assert!(sql.contains("ON users"));
        assert!(sql.contains("EXECUTE FUNCTION test_function()"));
    }

    #[test]
    fn test_trigger_builder_multiple_events() {
        let builder = TriggerBuilder::new("multi_trigger")
            .on_table("workflows")
            .timing(TriggerTiming::After)
            .events(vec![TriggerEvent::Insert, TriggerEvent::Update])
            .execute_function("audit_function");

        let sql = builder.build_sql().unwrap();
        assert!(sql.contains("INSERT OR UPDATE"));
    }

    #[test]
    fn test_trigger_builder_with_when() {
        let builder = TriggerBuilder::new("conditional_trigger")
            .on_table("orders")
            .timing(TriggerTiming::Before)
            .event(TriggerEvent::Update)
            .when_condition("NEW.status != OLD.status")
            .execute_function("status_change_handler");

        let sql = builder.build_sql().unwrap();
        assert!(sql.contains("WHEN (NEW.status != OLD.status)"));
    }

    #[test]
    fn test_trigger_builder_statement_level() {
        let builder = TriggerBuilder::new("stmt_trigger")
            .on_table("logs")
            .timing(TriggerTiming::After)
            .event(TriggerEvent::Truncate)
            .for_each_statement()
            .execute_function("truncate_handler");

        let sql = builder.build_sql().unwrap();
        assert!(sql.contains("FOR EACH STATEMENT"));
    }

    #[test]
    fn test_trigger_builder_update_of_columns() {
        let builder = TriggerBuilder::new("column_trigger")
            .on_table("products")
            .timing(TriggerTiming::Before)
            .events(vec![TriggerEvent::Update])
            .update_of_columns(vec!["price".to_string(), "stock".to_string()])
            .execute_function("price_stock_handler");

        let sql = builder.build_sql().unwrap();
        assert!(sql.contains("UPDATE OF price, stock"));
    }
}
