//! Database Maintenance Utilities for SQLite
//!
//! Provides utilities for SQLite maintenance operations.
//!
//! ## Overview
//!
//! SQLite maintenance operations:
//! - **VACUUM**: Rebuilds database file, reclaims space
//! - **ANALYZE**: Updates statistics for query planner
//! - **Statistics**: Provides insights into database health
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{MaintenanceService, MaintenanceConfig};
//!
//! let config = MaintenanceConfig::default();
//! let maintenance = MaintenanceService::new(pool, config);
//!
//! // Run maintenance
//! let results = maintenance.run_maintenance().await?;
//! println!("Maintenance completed");
//!
//! // Get database size
//! let size = maintenance.get_database_size_mb().await?;
//! println!("Database size: {} MB", size);
//! ```

use crate::{DatabasePool, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::info;

/// Maintenance configuration
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    /// Enable automatic vacuum
    pub auto_vacuum: bool,
    /// Enable automatic analyze
    pub auto_analyze: bool,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            auto_vacuum: true,
            auto_analyze: true,
        }
    }
}

/// Table statistics (simplified for SQLite)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStats {
    pub table_name: String,
    pub row_count: i64,
}

/// Maintenance results
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaintenanceResults {
    pub vacuumed: bool,
    pub analyzed: bool,
    pub errors: Vec<String>,
}

/// Index bloat information (simplified for SQLite)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexBloatInfo {
    pub table_name: String,
    pub index_name: String,
}

/// Maintenance service for database upkeep
pub struct MaintenanceService {
    pool: DatabasePool,
    config: MaintenanceConfig,
}

impl MaintenanceService {
    /// Create a new maintenance service
    pub fn new(pool: DatabasePool, config: MaintenanceConfig) -> Self {
        Self { pool, config }
    }

    /// Run all maintenance operations
    pub async fn run_maintenance(&self) -> Result<MaintenanceResults> {
        let mut results = MaintenanceResults::default();

        // Run VACUUM if enabled
        if self.config.auto_vacuum {
            match self.vacuum().await {
                Ok(()) => results.vacuumed = true,
                Err(e) => results.errors.push(format!("VACUUM failed: {e}")),
            }
        }

        // Run ANALYZE if enabled
        if self.config.auto_analyze {
            match self.analyze().await {
                Ok(()) => results.analyzed = true,
                Err(e) => results.errors.push(format!("ANALYZE failed: {e}")),
            }
        }

        info!(?results, "Maintenance completed");
        Ok(results)
    }

    /// VACUUM the database
    pub async fn vacuum(&self) -> Result<()> {
        sqlx::query("VACUUM").execute(self.pool.pool()).await?;
        Ok(())
    }

    /// ANALYZE the database
    pub async fn analyze(&self) -> Result<()> {
        sqlx::query("ANALYZE").execute(self.pool.pool()).await?;
        Ok(())
    }

    /// VACUUM a specific table
    pub async fn vacuum_table(&self, _table_name: &str) -> Result<()> {
        // SQLite VACUUM cannot target individual tables
        // Run full VACUUM instead
        self.vacuum().await
    }

    /// ANALYZE a specific table
    pub async fn analyze_table(&self, table_name: &str) -> Result<()> {
        // For dynamic table names, we use Box::leak to get a static string
        // This is safe because table names are short-lived
        let query: &'static str = Box::leak(format!("ANALYZE {table_name}").into_boxed_str());
        sqlx::query(query).execute(self.pool.pool()).await?;
        Ok(())
    }

    /// Get statistics for a specific table
    pub async fn get_table_stats(&self, table_name: &str) -> Result<TableStats> {
        // For dynamic table names, we use Box::leak to get a static string
        let query: &'static str =
            Box::leak(format!("SELECT COUNT(*) as count FROM {table_name}").into_boxed_str());
        let row = sqlx::query(query).fetch_one(self.pool.pool()).await?;

        let row_count: i64 = row.get("count");

        Ok(TableStats {
            table_name: table_name.to_string(),
            row_count,
        })
    }

    /// List all user tables
    pub async fn list_tables(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(self.pool.pool())
        .await?;

        let tables: Vec<String> = rows.into_iter().map(|r| r.get("name")).collect();

        Ok(tables)
    }

    /// Get database size in bytes
    ///
    /// Note: For SQLite, this returns the page count * page size
    pub async fn get_database_size(&self) -> Result<i64> {
        let row = sqlx::query(
            "SELECT page_count * page_size as size FROM pragma_page_count(), pragma_page_size()",
        )
        .fetch_one(self.pool.pool())
        .await?;

        let size: i64 = row.get("size");
        Ok(size)
    }

    /// Get database size in megabytes
    pub async fn get_database_size_mb(&self) -> Result<f64> {
        let size = self.get_database_size().await?;
        Ok(size as f64 / 1024.0 / 1024.0)
    }

    /// Get index information
    pub async fn get_index_info(&self) -> Result<Vec<IndexBloatInfo>> {
        let rows = sqlx::query(
            "SELECT tbl_name as table_name, name as index_name
             FROM sqlite_master
             WHERE type='index' AND name NOT LIKE 'sqlite_%'
             ORDER BY tbl_name, name",
        )
        .fetch_all(self.pool.pool())
        .await?;

        let indexes: Vec<IndexBloatInfo> = rows
            .into_iter()
            .map(|r| IndexBloatInfo {
                table_name: r.get("table_name"),
                index_name: r.get("index_name"),
            })
            .collect();

        Ok(indexes)
    }

    /// Check database integrity
    pub async fn integrity_check(&self) -> Result<bool> {
        let row = sqlx::query("SELECT integrity_check FROM pragma_integrity_check()")
            .fetch_one(self.pool.pool())
            .await?;

        let result: String = row.get("integrity_check");
        Ok(result == "ok")
    }

    /// Optimize database (runs VACUUM and ANALYZE)
    pub async fn optimize(&self) -> Result<()> {
        self.vacuum().await?;
        self.analyze().await?;
        Ok(())
    }

    /// Get freelist count (unused pages)
    pub async fn get_freelist_count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT freelist_count FROM pragma_freelist_count()")
            .fetch_one(self.pool.pool())
            .await?;

        let count: i64 = row.get("freelist_count");
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maintenance_config_default() {
        let config = MaintenanceConfig::default();
        assert!(config.auto_vacuum);
        assert!(config.auto_analyze);
    }

    #[test]
    fn test_maintenance_results_default() {
        let results = MaintenanceResults::default();
        assert!(!results.vacuumed);
        assert!(!results.analyzed);
        assert!(results.errors.is_empty());
    }
}
