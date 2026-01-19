//! Materialized view management for PostgreSQL
//!
//! Provides utilities to create, refresh, and manage materialized views,
//! which are cached query results that can significantly improve read performance.
//!
//! # Features
//!
//! - Create and drop materialized views
//! - Concurrent and non-concurrent refresh
//! - List all materialized views with statistics
//! - Automatic refresh scheduling
//! - Index management on materialized views
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::materialized_view::MaterializedViewManager;
//!
//! let manager = MaterializedViewManager::new(pool);
//!
//! // Create a materialized view
//! manager
//!     .create("workflow_stats")
//!     .query(r#"
//!         SELECT workflow_id, COUNT(*) as execution_count,
//!                AVG(duration) as avg_duration
//!         FROM executions
//!         GROUP BY workflow_id
//!     "#)
//!     .with_data() // Populate immediately
//!     .execute()
//!     .await?;
//!
//! // Refresh the view
//! manager.refresh("workflow_stats").concurrent().execute().await?;
//! ```

use crate::{Result, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Information about a materialized view
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MaterializedViewInfo {
    /// Schema name
    pub schema_name: String,
    /// View name
    pub matview_name: String,
    /// View definition
    pub definition: String,
    /// Whether the view has data
    pub ispopulated: bool,
    /// Size in bytes
    pub size_bytes: i64,
    /// Number of rows
    pub row_count: Option<i64>,
}

impl MaterializedViewInfo {
    /// Get human-readable size
    pub fn size_human(&self) -> String {
        let kb = self.size_bytes as f64 / 1024.0;
        if kb < 1024.0 {
            format!("{:.1} KB", kb)
        } else {
            let mb = kb / 1024.0;
            if mb < 1024.0 {
                format!("{:.1} MB", mb)
            } else {
                format!("{:.1} GB", mb / 1024.0)
            }
        }
    }
}

/// Builder for creating materialized views
pub struct CreateMaterializedView<'a> {
    pool: &'a PgPool,
    name: String,
    query: Option<String>,
    with_data: bool,
    if_not_exists: bool,
}

impl<'a> CreateMaterializedView<'a> {
    fn new(pool: &'a PgPool, name: impl Into<String>) -> Self {
        Self {
            pool,
            name: name.into(),
            query: None,
            with_data: true,
            if_not_exists: false,
        }
    }

    /// Set the query for the materialized view
    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Create with data (default)
    pub fn with_data(mut self) -> Self {
        self.with_data = true;
        self
    }

    /// Create without populating data
    pub fn without_data(mut self) -> Self {
        self.with_data = false;
        self
    }

    /// Add IF NOT EXISTS clause
    pub fn if_not_exists(mut self) -> Self {
        self.if_not_exists = true;
        self
    }

    /// Execute the CREATE MATERIALIZED VIEW statement
    pub async fn execute(self) -> Result<()> {
        let query = self.query.ok_or_else(|| {
            StorageError::ValidationError("Query is required for materialized view".to_string())
        })?;

        let if_not_exists = if self.if_not_exists {
            "IF NOT EXISTS "
        } else {
            ""
        };

        let with_data = if self.with_data {
            "WITH DATA"
        } else {
            "WITH NO DATA"
        };

        let sql = format!(
            "CREATE MATERIALIZED VIEW {}{} AS {} {}",
            if_not_exists, self.name, query, with_data
        );

        sqlx::query(&sql).execute(self.pool).await?;

        Ok(())
    }
}

/// Builder for refreshing materialized views
pub struct RefreshMaterializedView<'a> {
    pool: &'a PgPool,
    name: String,
    concurrent: bool,
}

impl<'a> RefreshMaterializedView<'a> {
    fn new(pool: &'a PgPool, name: impl Into<String>) -> Self {
        Self {
            pool,
            name: name.into(),
            concurrent: false,
        }
    }

    /// Use CONCURRENTLY (allows reads during refresh)
    ///
    /// Note: Requires a unique index on the materialized view
    pub fn concurrent(mut self) -> Self {
        self.concurrent = true;
        self
    }

    /// Execute the REFRESH MATERIALIZED VIEW statement
    pub async fn execute(self) -> Result<()> {
        let concurrent = if self.concurrent { "CONCURRENTLY " } else { "" };

        let sql = format!("REFRESH MATERIALIZED VIEW {}{}", concurrent, self.name);

        sqlx::query(&sql).execute(self.pool).await?;

        Ok(())
    }
}

/// Materialized view manager
pub struct MaterializedViewManager {
    pool: PgPool,
}

impl MaterializedViewManager {
    /// Create a new materialized view manager
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Start creating a materialized view
    pub fn create(&self, name: impl Into<String>) -> CreateMaterializedView<'_> {
        CreateMaterializedView::new(&self.pool, name)
    }

    /// Start refreshing a materialized view
    pub fn refresh(&self, name: impl Into<String>) -> RefreshMaterializedView<'_> {
        RefreshMaterializedView::new(&self.pool, name)
    }

    /// Drop a materialized view
    pub async fn drop(&self, name: &str, if_exists: bool) -> Result<()> {
        let if_exists_clause = if if_exists { "IF EXISTS " } else { "" };

        let sql = format!("DROP MATERIALIZED VIEW {}{}", if_exists_clause, name);

        sqlx::query(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// List all materialized views
    #[tracing::instrument(skip(self))]
    pub async fn list_all(&self) -> Result<Vec<MaterializedViewInfo>> {
        let views = sqlx::query_as::<_, MaterializedViewInfo>(
            r#"
            SELECT
                n.nspname AS schema_name,
                c.relname AS matview_name,
                pg_get_viewdef(c.oid) AS definition,
                c.relispopulated AS ispopulated,
                pg_total_relation_size(c.oid) AS size_bytes,
                c.reltuples::bigint AS row_count
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind = 'm'
              AND n.nspname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY n.nspname, c.relname
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(views)
    }

    /// Get information about a specific materialized view
    #[tracing::instrument(skip(self))]
    pub async fn get_info(&self, name: &str) -> Result<Option<MaterializedViewInfo>> {
        let view = sqlx::query_as::<_, MaterializedViewInfo>(
            r#"
            SELECT
                n.nspname AS schema_name,
                c.relname AS matview_name,
                pg_get_viewdef(c.oid) AS definition,
                c.relispopulated AS ispopulated,
                pg_total_relation_size(c.oid) AS size_bytes,
                c.reltuples::bigint AS row_count
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind = 'm'
              AND c.relname = $1
              AND n.nspname = 'public'
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(view)
    }

    /// Create an index on a materialized view
    pub async fn create_index(
        &self,
        matview_name: &str,
        index_name: &str,
        columns: &[&str],
        unique: bool,
    ) -> Result<()> {
        let unique_clause = if unique { "UNIQUE " } else { "" };

        let sql = format!(
            "CREATE {}INDEX IF NOT EXISTS {} ON {} ({})",
            unique_clause,
            index_name,
            matview_name,
            columns.join(", ")
        );

        sqlx::query(&sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Check if a materialized view exists
    pub async fn exists(&self, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind = 'm'
              AND c.relname = $1
              AND n.nspname = 'public'
            "#,
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_materialized_view_info_size_human() {
        let view = MaterializedViewInfo {
            schema_name: "public".to_string(),
            matview_name: "test_view".to_string(),
            definition: "SELECT * FROM test".to_string(),
            ispopulated: true,
            size_bytes: 1024 * 1024, // 1 MB
            row_count: Some(1000),
        };

        assert_eq!(view.size_human(), "1.0 MB");
    }

    #[test]
    fn test_materialized_view_info_size_human_kb() {
        let view = MaterializedViewInfo {
            schema_name: "public".to_string(),
            matview_name: "test_view".to_string(),
            definition: "SELECT * FROM test".to_string(),
            ispopulated: true,
            size_bytes: 512, // 512 bytes
            row_count: Some(10),
        };

        assert_eq!(view.size_human(), "0.5 KB");
    }

    #[test]
    fn test_materialized_view_info_size_human_gb() {
        let view = MaterializedViewInfo {
            schema_name: "public".to_string(),
            matview_name: "test_view".to_string(),
            definition: "SELECT * FROM test".to_string(),
            ispopulated: true,
            size_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
            row_count: Some(1000000),
        };

        assert_eq!(view.size_human(), "2.0 GB");
    }
}
