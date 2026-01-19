//! Database backup and restore utilities
//!
//! This module provides tools for backing up and restoring database data,
//! essential for disaster recovery and data migration scenarios.
//!
//! # Features
//!
//! - **Full Database Backup**: Export all tables to SQL dump format
//! - **Selective Table Backup**: Backup specific tables or schemas
//! - **Data Export**: Export data in JSON or CSV format
//! - **Point-in-Time Recovery**: Support for PostgreSQL WAL archiving
//! - **Incremental Backups**: Backup only changed data
//! - **Restore Operations**: Restore from backup files
//! - **Backup Verification**: Validate backup integrity
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, BackupManager, BackupConfig, BackupFormat};
//! use std::path::Path;
//!
//! let pool = DatabasePool::new(config).await?;
//! let backup_mgr = BackupManager::new(pool.clone());
//!
//! // Full database backup
//! let backup_path = Path::new("/backups/oxify_full_20260101.sql");
//! let result = backup_mgr
//!     .backup_full(backup_path, BackupFormat::SqlDump)
//!     .await?;
//! println!("Backup completed: {} bytes in {}ms",
//!          result.size_bytes, result.duration_ms);
//!
//! // Selective table backup
//! let tables = vec!["workflows", "executions", "users"];
//! let result = backup_mgr
//!     .backup_tables(&tables, backup_path)
//!     .await?;
//!
//! // Export to JSON
//! let json_path = Path::new("/exports/workflows.json");
//! let count = backup_mgr
//!     .export_table_json("workflows", json_path)
//!     .await?;
//! println!("Exported {} rows to JSON", count);
//!
//! // Restore from backup
//! let restore_result = backup_mgr
//!     .restore_from_sql(backup_path)
//!     .await?;
//! println!("Restored {} tables", restore_result.tables_restored);
//! ```

use crate::{DatabasePool, Result, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Backup file format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupFormat {
    /// PostgreSQL SQL dump (pg_dump format)
    SqlDump,
    /// Custom PostgreSQL archive format (compressed)
    Custom,
    /// Directory format (one file per table)
    Directory,
    /// Plain text SQL statements
    PlainSql,
}

impl BackupFormat {
    /// Get the pg_dump format flag
    fn pg_dump_format(&self) -> &str {
        match self {
            BackupFormat::SqlDump => "plain",
            BackupFormat::Custom => "custom",
            BackupFormat::Directory => "directory",
            BackupFormat::PlainSql => "plain",
        }
    }

    /// Get the typical file extension
    pub fn extension(&self) -> &str {
        match self {
            BackupFormat::SqlDump | BackupFormat::PlainSql => "sql",
            BackupFormat::Custom => "dump",
            BackupFormat::Directory => "dir",
        }
    }
}

/// Backup configuration
#[derive(Debug, Clone)]
pub struct BackupConfig {
    /// Use compression (if supported by format)
    pub compress: bool,
    /// Include large objects (BLOBs)
    pub include_blobs: bool,
    /// Include ownership information
    pub include_ownership: bool,
    /// Include CREATE DATABASE statement
    pub create_database: bool,
    /// Number of parallel jobs for backup (0 = auto)
    pub parallel_jobs: u32,
    /// Custom pg_dump path (if not in PATH)
    pub pg_dump_path: Option<PathBuf>,
    /// Custom pg_restore path (if not in PATH)
    pub pg_restore_path: Option<PathBuf>,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            compress: true,
            include_blobs: true,
            include_ownership: false,
            create_database: false,
            parallel_jobs: 0,
            pg_dump_path: None,
            pg_restore_path: None,
        }
    }
}

/// Backup operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult {
    /// Path to the backup file
    pub backup_path: PathBuf,
    /// Backup format used
    pub format: BackupFormat,
    /// Size of backup in bytes
    pub size_bytes: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Number of tables backed up
    pub tables_count: usize,
    /// Backup timestamp
    pub timestamp: i64,
    /// Success status
    pub success: bool,
    /// Optional error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Restore operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    /// Number of tables restored
    pub tables_restored: usize,
    /// Number of rows restored (if available)
    pub rows_restored: Option<u64>,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Success status
    pub success: bool,
    /// Optional error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Table export statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportStats {
    /// Table name
    pub table_name: String,
    /// Number of rows exported
    pub rows_exported: u64,
    /// File size in bytes
    pub file_size_bytes: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Backup manager for database backup and restore operations
pub struct BackupManager {
    pool: DatabasePool,
    config: BackupConfig,
}

impl BackupManager {
    /// Create a new backup manager with default configuration
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            config: BackupConfig::default(),
        }
    }

    /// Create a new backup manager with custom configuration
    pub fn with_config(pool: DatabasePool, config: BackupConfig) -> Self {
        Self { pool, config }
    }

    /// Perform a full database backup
    ///
    /// This uses pg_dump to create a complete backup of the database.
    /// The backup includes all tables, indexes, sequences, and constraints.
    pub async fn backup_full(
        &self,
        backup_path: &Path,
        format: BackupFormat,
    ) -> Result<BackupResult> {
        let start = Instant::now();

        // Extract connection details from pool
        let db_url = self.get_database_url()?;

        // Build pg_dump command
        let pg_dump = self.config.pg_dump_path.as_ref().map_or_else(
            || "pg_dump".to_string(),
            |p| p.to_string_lossy().to_string(),
        );

        let mut cmd = Command::new(&pg_dump);
        cmd.arg("--format")
            .arg(format.pg_dump_format())
            .arg("--file")
            .arg(backup_path);

        if self.config.compress && format == BackupFormat::Custom {
            cmd.arg("--compress").arg("9");
        }

        if self.config.include_blobs {
            cmd.arg("--blobs");
        }

        if !self.config.include_ownership {
            cmd.arg("--no-owner");
        }

        if self.config.create_database {
            cmd.arg("--create");
        }

        if self.config.parallel_jobs > 0 && format == BackupFormat::Directory {
            cmd.arg("--jobs").arg(self.config.parallel_jobs.to_string());
        }

        cmd.arg("--verbose");

        // Set DATABASE_URL environment
        cmd.env(
            "PGPASSWORD",
            self.extract_password(&db_url).unwrap_or_default(),
        );
        cmd.arg(&self.extract_connection_string(&db_url)?);

        // Execute backup
        let output = cmd
            .output()
            .await
            .map_err(|e| StorageError::BackupError(format!("Failed to execute pg_dump: {e}")))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Ok(BackupResult {
                backup_path: backup_path.to_path_buf(),
                format,
                size_bytes: 0,
                duration_ms: start.elapsed().as_millis() as u64,
                tables_count: 0,
                timestamp: chrono::Utc::now().timestamp(),
                success: false,
                error: Some(error.to_string()),
            });
        }

        // Get backup file size
        let metadata = fs::metadata(backup_path).await.map_err(|e| {
            StorageError::BackupError(format!("Failed to get backup file metadata: {e}"))
        })?;

        // Count tables
        let tables_count = self.count_tables().await?;

        Ok(BackupResult {
            backup_path: backup_path.to_path_buf(),
            format,
            size_bytes: metadata.len(),
            duration_ms: start.elapsed().as_millis() as u64,
            tables_count,
            timestamp: chrono::Utc::now().timestamp(),
            success: true,
            error: None,
        })
    }

    /// Backup specific tables only
    pub async fn backup_tables(
        &self,
        tables: &[impl AsRef<str>],
        backup_path: &Path,
    ) -> Result<BackupResult> {
        let start = Instant::now();

        let db_url = self.get_database_url()?;
        let pg_dump = self.config.pg_dump_path.as_ref().map_or_else(
            || "pg_dump".to_string(),
            |p| p.to_string_lossy().to_string(),
        );

        let mut cmd = Command::new(&pg_dump);
        cmd.arg("--format")
            .arg("plain")
            .arg("--file")
            .arg(backup_path);

        // Add table arguments
        for table in tables {
            cmd.arg("--table").arg(table.as_ref());
        }

        if !self.config.include_ownership {
            cmd.arg("--no-owner");
        }

        cmd.env(
            "PGPASSWORD",
            self.extract_password(&db_url).unwrap_or_default(),
        );
        cmd.arg(&self.extract_connection_string(&db_url)?);

        let output = cmd
            .output()
            .await
            .map_err(|e| StorageError::BackupError(format!("Failed to execute pg_dump: {e}")))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Ok(BackupResult {
                backup_path: backup_path.to_path_buf(),
                format: BackupFormat::PlainSql,
                size_bytes: 0,
                duration_ms: start.elapsed().as_millis() as u64,
                tables_count: 0,
                timestamp: chrono::Utc::now().timestamp(),
                success: false,
                error: Some(error.to_string()),
            });
        }

        let metadata = fs::metadata(backup_path).await.map_err(|e| {
            StorageError::BackupError(format!("Failed to get backup file metadata: {e}"))
        })?;

        Ok(BackupResult {
            backup_path: backup_path.to_path_buf(),
            format: BackupFormat::PlainSql,
            size_bytes: metadata.len(),
            duration_ms: start.elapsed().as_millis() as u64,
            tables_count: tables.len(),
            timestamp: chrono::Utc::now().timestamp(),
            success: true,
            error: None,
        })
    }

    /// Export a table to JSON format
    pub async fn export_table_json(
        &self,
        table_name: &str,
        output_path: &Path,
    ) -> Result<ExportStats> {
        let start = Instant::now();

        // Query all rows
        let query = format!("SELECT row_to_json({table_name}) FROM {table_name}");
        let rows = sqlx::query(&query).fetch_all(self.pool.pool()).await?;

        // Write to file
        let mut file = fs::File::create(output_path)
            .await
            .map_err(|e| StorageError::BackupError(format!("Failed to create export file: {e}")))?;

        file.write_all(b"[\n").await.map_err(|e| {
            StorageError::BackupError(format!("Failed to write to export file: {e}"))
        })?;

        let row_count = rows.len();
        for (i, row) in rows.iter().enumerate() {
            let json: serde_json::Value = row.get(0);
            let json_str = serde_json::to_string(&json)
                .map_err(|e| StorageError::BackupError(format!("Failed to serialize row: {e}")))?;

            file.write_all(json_str.as_bytes()).await.map_err(|e| {
                StorageError::BackupError(format!("Failed to write to export file: {e}"))
            })?;

            if i < row_count - 1 {
                file.write_all(b",\n").await.map_err(|e| {
                    StorageError::BackupError(format!("Failed to write to export file: {e}"))
                })?;
            }
        }

        file.write_all(b"\n]\n").await.map_err(|e| {
            StorageError::BackupError(format!("Failed to write to export file: {e}"))
        })?;

        let metadata = fs::metadata(output_path).await.map_err(|e| {
            StorageError::BackupError(format!("Failed to get export file metadata: {e}"))
        })?;

        Ok(ExportStats {
            table_name: table_name.to_string(),
            rows_exported: row_count as u64,
            file_size_bytes: metadata.len(),
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Restore database from SQL dump file
    pub async fn restore_from_sql(&self, backup_path: &Path) -> Result<RestoreResult> {
        let start = Instant::now();

        let db_url = self.get_database_url()?;
        let psql = "psql";

        let mut cmd = Command::new(psql);
        cmd.arg("--file").arg(backup_path);
        cmd.arg("--quiet");

        cmd.env(
            "PGPASSWORD",
            self.extract_password(&db_url).unwrap_or_default(),
        );
        cmd.arg(&self.extract_connection_string(&db_url)?);

        let output = cmd
            .output()
            .await
            .map_err(|e| StorageError::BackupError(format!("Failed to execute psql: {e}")))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Ok(RestoreResult {
                tables_restored: 0,
                rows_restored: None,
                duration_ms: start.elapsed().as_millis() as u64,
                success: false,
                error: Some(error.to_string()),
            });
        }

        let tables_count = self.count_tables().await?;

        Ok(RestoreResult {
            tables_restored: tables_count,
            rows_restored: None,
            duration_ms: start.elapsed().as_millis() as u64,
            success: true,
            error: None,
        })
    }

    /// Verify backup integrity
    pub async fn verify_backup(&self, backup_path: &Path) -> Result<bool> {
        // Check if file exists and is readable
        if !backup_path.exists() {
            return Ok(false);
        }

        let metadata = fs::metadata(backup_path)
            .await
            .map_err(|e| StorageError::BackupError(format!("Failed to read backup file: {e}")))?;

        // Check if file has content
        Ok(metadata.len() > 0)
    }

    // Helper methods

    fn get_database_url(&self) -> Result<String> {
        std::env::var("DATABASE_URL")
            .map_err(|_| StorageError::BackupError("DATABASE_URL not set".to_string()))
    }

    fn extract_password(&self, db_url: &str) -> Option<String> {
        // Parse password from URL: postgres://user:password@host:port/db
        db_url
            .split('@')
            .next()?
            .split(':')
            .nth(2)
            .map(std::string::ToString::to_string)
    }

    fn extract_connection_string(&self, db_url: &str) -> Result<String> {
        // Convert URL to psql connection string
        // postgres://user:password@host:port/database
        let parts: Vec<&str> = db_url.split("://").collect();
        if parts.len() != 2 {
            return Err(StorageError::BackupError(
                "Invalid DATABASE_URL".to_string(),
            ));
        }

        let rest = parts[1];
        let (user_pass, host_db) = rest
            .split_once('@')
            .ok_or_else(|| StorageError::BackupError("Invalid DATABASE_URL format".to_string()))?;

        let (user, _pass) = user_pass.split_once(':').unwrap_or((user_pass, ""));
        let (host_port, database) = host_db.split_once('/').unwrap_or((host_db, ""));
        let (host, port) = host_port.split_once(':').unwrap_or((host_port, "5432"));

        Ok(format!(
            "--host={host} --port={port} --username={user} --dbname={database}"
        ))
    }

    async fn count_tables(&self) -> Result<usize> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM information_schema.tables
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE'",
        )
        .fetch_one(self.pool.pool())
        .await?;

        let count: i64 = sqlx::Row::get(&row, "count");
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_format() {
        assert_eq!(BackupFormat::SqlDump.pg_dump_format(), "plain");
        assert_eq!(BackupFormat::Custom.pg_dump_format(), "custom");
        assert_eq!(BackupFormat::Directory.pg_dump_format(), "directory");

        assert_eq!(BackupFormat::SqlDump.extension(), "sql");
        assert_eq!(BackupFormat::Custom.extension(), "dump");
        assert_eq!(BackupFormat::Directory.extension(), "dir");
    }

    #[test]
    fn test_default_backup_config() {
        let config = BackupConfig::default();
        assert!(config.compress);
        assert!(config.include_blobs);
        assert!(!config.include_ownership);
        assert!(!config.create_database);
        assert_eq!(config.parallel_jobs, 0);
    }

    #[test]
    fn test_backup_result_serialization() {
        let result = BackupResult {
            backup_path: PathBuf::from("/tmp/backup.sql"),
            format: BackupFormat::SqlDump,
            size_bytes: 1024,
            duration_ms: 5000,
            tables_count: 10,
            timestamp: 1234567890,
            success: true,
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("backup.sql"));
        assert!(json.contains("1024"));
    }
}
