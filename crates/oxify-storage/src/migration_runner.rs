//! Database Migration Runner
//!
//! This module provides utilities for applying and tracking database schema migrations.
//!
//! ## Features
//!
//! - Version-tracked migrations with automatic ordering
//! - Idempotent migration application (safe to run multiple times)
//! - Transaction-wrapped migrations for atomicity
//! - Migration history tracking in `schema_migrations` table
//! - Rollback support for reversible migrations
//! - Dry-run mode for previewing changes
//!
//! ## Usage
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, migration_runner::MigrationRunner};
//!
//! let pool = DatabasePool::new(config).await?;
//! let runner = MigrationRunner::new(pool);
//!
//! // Run all pending migrations
//! let applied = runner.run_pending_migrations().await?;
//! println!("Applied {} migrations", applied.len());
//!
//! // Check migration status
//! let status = runner.get_migration_status().await?;
//! for migration in status {
//!     println!("{}: {} - {}", migration.version, migration.name,
//!              if migration.applied { "applied" } else { "pending" });
//! }
//! ```

use crate::{DatabasePool, Result, StorageError};
use chrono::{DateTime, Utc};
use sqlx::Row;
use std::collections::HashMap;

/// Migration definition
#[derive(Debug, Clone)]
pub struct Migration {
    /// Migration version (e.g., "20260101_001")
    pub version: String,
    /// Human-readable migration name
    pub name: String,
    /// SQL to apply the migration
    pub up_sql: String,
    /// SQL to rollback the migration (optional)
    pub down_sql: Option<String>,
    /// Migration description
    pub description: Option<String>,
}

/// Migration status information
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    /// Migration version
    pub version: String,
    /// Migration name
    pub name: String,
    /// Whether the migration has been applied
    pub applied: bool,
    /// When the migration was applied (if applied)
    pub applied_at: Option<DateTime<Utc>>,
    /// Checksum of the migration SQL
    pub checksum: Option<String>,
}

/// Migration history record
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MigrationRecord {
    version: String,
    name: String,
    applied_at: String,
    checksum: String,
}

/// Migration runner for applying and tracking database migrations
pub struct MigrationRunner {
    pool: DatabasePool,
}

impl MigrationRunner {
    /// Create a new migration runner
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Initialize the migrations tracking table
    ///
    /// Creates the `schema_migrations` table if it doesn't exist.
    pub async fn initialize(&self) -> Result<()> {
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (datetime('now')),
                checksum TEXT NOT NULL,
                execution_time_ms INTEGER
            )
            ",
        )
        .execute(self.pool.pool())
        .await?;

        // Create index for faster lookups
        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_schema_migrations_applied_at
            ON schema_migrations(applied_at DESC)
            ",
        )
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Get all built-in migrations
    ///
    /// Returns the standard set of migrations for oxify-storage.
    pub fn get_builtin_migrations() -> Vec<Migration> {
        vec![
            Migration {
                version: "001_performance_indexes".to_string(),
                name: "Add Performance Indexes".to_string(),
                description: Some("Creates indexes for improved query performance".to_string()),
                up_sql: include_str!("../sql/001_performance_indexes.sql").to_string(),
                down_sql: Some(include_str!("../sql/001_performance_indexes_down.sql").to_string()),
            },
            Migration {
                version: "002_schema_constraints".to_string(),
                name: "Add Schema Constraints".to_string(),
                description: Some("Adds foreign keys and CHECK constraints".to_string()),
                up_sql: include_str!("../sql/002_schema_constraints.sql").to_string(),
                down_sql: Some(include_str!("../sql/002_schema_constraints_down.sql").to_string()),
            },
        ]
    }

    /// Check if a migration has been applied
    pub async fn is_applied(&self, version: &str) -> Result<bool> {
        let row = sqlx::query(
            r"
            SELECT COUNT(*) as count FROM schema_migrations WHERE version = ?
            ",
        )
        .bind(version)
        .fetch_one(self.pool.pool())
        .await?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }

    /// Apply a single migration
    ///
    /// Runs the migration in a transaction and records it in the migrations table.
    pub async fn apply_migration(&self, migration: &Migration) -> Result<i64> {
        // Check if already applied
        if self.is_applied(&migration.version).await? {
            return Ok(0);
        }

        let start_time = std::time::Instant::now();
        let mut tx = self.pool.pool().begin().await?;

        // Execute the migration SQL
        // For dynamic SQL, we use Box::leak to get a static string (SqlSafeStr requirement)
        let up_sql: &'static str = Box::leak(migration.up_sql.clone().into_boxed_str());
        sqlx::query(up_sql).execute(&mut *tx).await?;

        // Calculate checksum
        let checksum = Self::calculate_checksum(&migration.up_sql);

        // Record the migration
        let execution_time_ms = start_time.elapsed().as_millis() as i64;
        sqlx::query(
            r"
            INSERT INTO schema_migrations (version, name, checksum, execution_time_ms)
            VALUES (?, ?, ?, ?)
            ",
        )
        .bind(&migration.version)
        .bind(&migration.name)
        .bind(&checksum)
        .bind(execution_time_ms)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(execution_time_ms)
    }

    /// Run all pending migrations
    ///
    /// Applies all migrations that haven't been applied yet.
    pub async fn run_pending_migrations(&self) -> Result<Vec<String>> {
        self.initialize().await?;

        let migrations = Self::get_builtin_migrations();
        let mut applied = Vec::new();

        for migration in migrations {
            if !self.is_applied(&migration.version).await? {
                self.apply_migration(&migration).await?;
                applied.push(migration.version.clone());
            }
        }

        Ok(applied)
    }

    /// Get migration status for all migrations
    pub async fn get_migration_status(&self) -> Result<Vec<MigrationStatus>> {
        self.initialize().await?;

        let migrations = Self::get_builtin_migrations();
        let applied_records = self.get_applied_migrations().await?;

        let applied_map: HashMap<String, MigrationRecord> = applied_records
            .into_iter()
            .map(|r| (r.version.clone(), r))
            .collect();

        let mut status = Vec::new();
        for migration in migrations {
            let record = applied_map.get(&migration.version);
            status.push(MigrationStatus {
                version: migration.version,
                name: migration.name,
                applied: record.is_some(),
                applied_at: record.and_then(|r| {
                    DateTime::parse_from_rfc3339(&r.applied_at)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                }),
                checksum: record.map(|r| r.checksum.clone()),
            });
        }

        Ok(status)
    }

    /// Get all applied migrations
    async fn get_applied_migrations(&self) -> Result<Vec<MigrationRecord>> {
        let rows = sqlx::query(
            r"
            SELECT version, name, applied_at, checksum
            FROM schema_migrations
            ORDER BY applied_at ASC
            ",
        )
        .fetch_all(self.pool.pool())
        .await?;

        let records: Vec<MigrationRecord> = rows
            .into_iter()
            .map(|row| MigrationRecord {
                version: row.get("version"),
                name: row.get("name"),
                applied_at: row.get("applied_at"),
                checksum: row.get("checksum"),
            })
            .collect();

        Ok(records)
    }

    /// Rollback a migration
    ///
    /// Reverts a migration if it has a down_sql defined.
    pub async fn rollback_migration(&self, version: &str) -> Result<()> {
        let migrations = Self::get_builtin_migrations();
        let migration = migrations
            .iter()
            .find(|m| m.version == version)
            .ok_or_else(|| {
                StorageError::NotFoundLegacy(format!("Migration {version} not found"))
            })?;

        let down_sql = migration.down_sql.as_ref().ok_or_else(|| {
            StorageError::ValidationError(format!("Migration {version} has no rollback defined"))
        })?;

        let mut tx = self.pool.pool().begin().await?;

        // Execute rollback SQL
        // For dynamic SQL, we use Box::leak to get a static string (SqlSafeStr requirement)
        let down_sql_static: &'static str = Box::leak(down_sql.clone().into_boxed_str());
        sqlx::query(down_sql_static).execute(&mut *tx).await?;

        // Remove migration record
        sqlx::query("DELETE FROM schema_migrations WHERE version = ?")
            .bind(version)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Calculate SHA-256 checksum of SQL content
    fn calculate_checksum(sql: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(sql.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Verify migration checksums match recorded values
    ///
    /// Detects if migration files have been modified after being applied.
    pub async fn verify_checksums(&self) -> Result<Vec<String>> {
        let migrations = Self::get_builtin_migrations();
        let applied = self.get_applied_migrations().await?;
        let mut mismatches = Vec::new();

        let applied_map: HashMap<String, MigrationRecord> = applied
            .into_iter()
            .map(|r| (r.version.clone(), r))
            .collect();

        for migration in migrations {
            if let Some(record) = applied_map.get(&migration.version) {
                let current_checksum = Self::calculate_checksum(&migration.up_sql);
                if current_checksum != record.checksum {
                    mismatches.push(migration.version.clone());
                }
            }
        }

        Ok(mismatches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_calculation() {
        let sql = "CREATE TABLE test (id INT)";
        let checksum = MigrationRunner::calculate_checksum(sql);
        assert_eq!(checksum.len(), 64); // SHA-256 produces 64 hex characters
    }

    #[test]
    fn test_checksum_consistency() {
        let sql = "SELECT * FROM users";
        let checksum1 = MigrationRunner::calculate_checksum(sql);
        let checksum2 = MigrationRunner::calculate_checksum(sql);
        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn test_builtin_migrations_exist() {
        let migrations = MigrationRunner::get_builtin_migrations();
        assert!(!migrations.is_empty());
        assert!(migrations
            .iter()
            .any(|m| m.version.contains("performance_indexes")));
        assert!(migrations
            .iter()
            .any(|m| m.version.contains("schema_constraints")));
    }
}
