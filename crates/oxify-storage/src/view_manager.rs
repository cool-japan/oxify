//! PostgreSQL View Manager
//!
//! This module provides utilities for managing PostgreSQL views (regular, non-materialized views).
//! Views are virtual tables created by stored queries that simplify complex queries and provide
//! abstraction layers.
//!
//! # Benefits
//!
//! - **Simplify Complex Queries**: Encapsulate complex JOINs and aggregations
//! - **Security**: Restrict access to specific columns
//! - **Abstraction**: Hide implementation details from users
//! - **Consistency**: Ensure queries are executed consistently
//! - **No Storage Overhead**: Views don't store data (unlike materialized views)
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::ViewManager;
//!
//! let view_mgr = ViewManager::new(pool.clone());
//!
//! // Create a view
//! view_mgr.create_view(
//!     "active_workflows",
//!     "SELECT id, name, state FROM workflows WHERE state IN ('pending', 'running')"
//! ).await?;
//!
//! // List all views
//! let views = view_mgr.list_views().await?;
//! for view in views {
//!     println!("{}: {}", view.view_name, view.definition);
//! }
//!
//! // Drop view
//! view_mgr.drop_view("active_workflows").await?;
//! ```

use crate::{Result, StorageError};
use sqlx::PgPool;
use std::sync::Arc;

/// Information about a PostgreSQL view
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ViewInfo {
    /// Schema name
    pub schema_name: String,
    /// View name
    pub view_name: String,
    /// View definition (SELECT query)
    pub definition: String,
    /// Whether the view is updatable
    pub is_updatable: String,
}

/// Builder for creating views with options
#[derive(Debug, Clone)]
pub struct ViewBuilder {
    name: String,
    query: String,
    or_replace: bool,
    recursive: bool,
    columns: Vec<String>,
    check_option: Option<CheckOption>,
}

/// Check option for updatable views
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOption {
    /// WITH CHECK OPTION - Rows must satisfy view condition after update
    Local,
    /// WITH CASCADED CHECK OPTION - Rows must satisfy all view conditions
    Cascaded,
}

impl CheckOption {
    fn as_str(&self) -> &'static str {
        match self {
            CheckOption::Local => "WITH LOCAL CHECK OPTION",
            CheckOption::Cascaded => "WITH CASCADED CHECK OPTION",
        }
    }
}

impl ViewBuilder {
    /// Create a new view builder
    pub fn new(name: impl Into<String>, query: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            query: query.into(),
            or_replace: false,
            recursive: false,
            columns: Vec::new(),
            check_option: None,
        }
    }

    /// Create or replace the view if it exists
    pub fn or_replace(mut self) -> Self {
        self.or_replace = true;
        self
    }

    /// Create a recursive view (for hierarchical queries)
    pub fn recursive(mut self) -> Self {
        self.recursive = true;
        self
    }

    /// Specify column names (optional, can override query column names)
    pub fn columns(mut self, cols: Vec<String>) -> Self {
        self.columns = cols;
        self
    }

    /// Add check option for updatable views
    pub fn with_check_option(mut self, option: CheckOption) -> Self {
        self.check_option = Some(option);
        self
    }

    /// Build the CREATE VIEW SQL statement
    pub fn build_sql(&self) -> String {
        let mut sql = String::from("CREATE");

        if self.or_replace {
            sql.push_str(" OR REPLACE");
        }

        if self.recursive {
            sql.push_str(" RECURSIVE VIEW");
        } else {
            sql.push_str(" VIEW");
        }

        sql.push_str(&format!(" {}", self.name));

        if !self.columns.is_empty() {
            sql.push_str(&format!(" ({})", self.columns.join(", ")));
        }

        sql.push_str(&format!(" AS {}", self.query));

        if let Some(check_opt) = self.check_option {
            sql.push(' ');
            sql.push_str(check_opt.as_str());
        }

        sql
    }

    /// Execute the CREATE VIEW command
    pub async fn execute(&self, pool: &PgPool) -> Result<()> {
        let sql = self.build_sql();
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }
}

/// PostgreSQL View Manager
///
/// Provides methods for creating, dropping, and querying views.
#[derive(Clone)]
pub struct ViewManager {
    pool: Arc<PgPool>,
}

impl ViewManager {
    /// Create a new view manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // ========================================================================
    // View Creation and Removal
    // ========================================================================

    /// Create a simple view
    ///
    /// # Example
    ///
    /// ```ignore
    /// view_mgr.create_view(
    ///     "pending_executions",
    ///     "SELECT * FROM executions WHERE state = 'pending'"
    /// ).await?;
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn create_view(&self, view_name: &str, query: &str) -> Result<()> {
        let sql = format!("CREATE VIEW {} AS {}", view_name, query);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Create or replace a view
    #[tracing::instrument(skip(self))]
    pub async fn create_or_replace_view(&self, view_name: &str, query: &str) -> Result<()> {
        let sql = format!("CREATE OR REPLACE VIEW {} AS {}", view_name, query);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Create a view with a builder for custom configuration
    pub fn build_view(
        &self,
        view_name: impl Into<String>,
        query: impl Into<String>,
    ) -> ViewBuilder {
        ViewBuilder::new(view_name, query)
    }

    /// Drop a view
    #[tracing::instrument(skip(self))]
    pub async fn drop_view(&self, view_name: &str) -> Result<()> {
        let sql = format!("DROP VIEW {}", view_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop view if it exists
    #[tracing::instrument(skip(self))]
    pub async fn drop_view_if_exists(&self, view_name: &str) -> Result<()> {
        let sql = format!("DROP VIEW IF EXISTS {}", view_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop view with CASCADE to remove dependent objects
    #[tracing::instrument(skip(self))]
    pub async fn drop_view_cascade(&self, view_name: &str) -> Result<()> {
        let sql = format!("DROP VIEW {} CASCADE", view_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // View Information
    // ========================================================================

    /// List all views in the database
    #[tracing::instrument(skip(self))]
    pub async fn list_views(&self) -> Result<Vec<ViewInfo>> {
        let views: Vec<ViewInfo> = sqlx::query_as(
            r"
            SELECT
                schemaname as schema_name,
                viewname as view_name,
                definition,
                'NO' as is_updatable
            FROM pg_views
            WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY schemaname, viewname
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(views)
    }

    /// Get information about a specific view
    #[tracing::instrument(skip(self))]
    pub async fn get_view_info(&self, view_name: &str) -> Result<Option<ViewInfo>> {
        let view: Option<ViewInfo> = sqlx::query_as(
            r"
            SELECT
                schemaname as schema_name,
                viewname as view_name,
                definition,
                'NO' as is_updatable
            FROM pg_views
            WHERE viewname = $1
              AND schemaname = 'public'
            ",
        )
        .bind(view_name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(view)
    }

    /// Get the definition (query) of a view
    #[tracing::instrument(skip(self))]
    pub async fn get_view_definition(&self, view_name: &str) -> Result<Option<String>> {
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT definition FROM pg_views WHERE viewname = $1 AND schemaname = 'public'",
        )
        .bind(view_name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.map(|r| r.0))
    }

    /// Check if a view exists
    #[tracing::instrument(skip(self))]
    pub async fn view_exists(&self, view_name: &str) -> Result<bool> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM pg_views
                WHERE viewname = $1
                  AND schemaname = 'public'
            )",
        )
        .bind(view_name)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Rename a view
    #[tracing::instrument(skip(self))]
    pub async fn rename_view(&self, old_name: &str, new_name: &str) -> Result<()> {
        let sql = format!("ALTER VIEW {} RENAME TO {}", old_name, new_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Change the owner of a view
    #[tracing::instrument(skip(self))]
    pub async fn set_view_owner(&self, view_name: &str, owner: &str) -> Result<()> {
        let sql = format!("ALTER VIEW {} OWNER TO {}", view_name, owner);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Set view schema
    #[tracing::instrument(skip(self))]
    pub async fn set_view_schema(&self, view_name: &str, schema_name: &str) -> Result<()> {
        let sql = format!("ALTER VIEW {} SET SCHEMA {}", view_name, schema_name);
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
    fn test_view_builder_basic() {
        let builder = ViewBuilder::new("test_view", "SELECT * FROM users");

        let sql = builder.build_sql();
        assert!(sql.contains("CREATE VIEW test_view"));
        assert!(sql.contains("SELECT * FROM users"));
    }

    #[test]
    fn test_view_builder_or_replace() {
        let builder = ViewBuilder::new("test_view", "SELECT * FROM users").or_replace();

        let sql = builder.build_sql();
        assert!(sql.contains("CREATE OR REPLACE VIEW test_view"));
    }

    #[test]
    fn test_view_builder_recursive() {
        let builder = ViewBuilder::new("recursive_view", "SELECT * FROM tree").recursive();

        let sql = builder.build_sql();
        assert!(sql.contains("CREATE RECURSIVE VIEW recursive_view"));
    }

    #[test]
    fn test_view_builder_with_columns() {
        let builder = ViewBuilder::new("test_view", "SELECT id, name FROM users")
            .columns(vec!["user_id".to_string(), "user_name".to_string()]);

        let sql = builder.build_sql();
        assert!(sql.contains("CREATE VIEW test_view (user_id, user_name)"));
    }

    #[test]
    fn test_view_builder_with_check_option() {
        let builder = ViewBuilder::new("test_view", "SELECT * FROM users WHERE active = true")
            .with_check_option(CheckOption::Local);

        let sql = builder.build_sql();
        assert!(sql.contains("WITH LOCAL CHECK OPTION"));
    }

    #[test]
    fn test_check_option_as_str() {
        assert_eq!(CheckOption::Local.as_str(), "WITH LOCAL CHECK OPTION");
        assert_eq!(CheckOption::Cascaded.as_str(), "WITH CASCADED CHECK OPTION");
    }
}
