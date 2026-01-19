//! # Data Archival Strategy
//!
//! This module provides utilities for archiving old execution data to manage
//! database growth and maintain performance. Archived data is moved to separate
//! archive tables that can be stored on cheaper storage tiers or exported to
//! external storage systems.
//!
//! ## Archival Strategy
//!
//! The archival process involves:
//! 1. Identifying executions older than retention threshold
//! 2. Copying executions to archive table
//! 3. Deleting archived executions from main table
//! 4. Optionally exporting to external storage (S3, file system, etc.)
//!
//! ## Archive Tables
//!
//! Archive tables mirror the structure of main tables:
//! - `executions_archive` - Archived execution records
//! - `execution_variables_archive` - Archived execution variables
//! - `execution_durations_archive` - Archived duration metrics
//!
//! ## Data Retention Policy
//!
//! Recommended retention periods:
//! - Hot data (main tables): 90 days
//! - Warm data (archive tables): 1 year
//! - Cold data (external storage): Indefinite
//!
//! ## Example
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, ArchivalManager, ArchivalConfig};
//! use chrono::{Utc, Duration};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let pool = DatabasePool::new("postgres://localhost/test").await?;
//! let manager = ArchivalManager::new(pool.clone());
//!
//! // Configure archival
//! let config = ArchivalConfig {
//!     retention_days: 90,
//!     batch_size: 1000,
//! };
//!
//! // Archive old executions
//! let stats = manager.archive_old_executions(&config).await?;
//! println!("Archived {} executions, freed {} bytes",
//!     stats.archived_count, stats.space_freed);
//!
//! // List archived executions
//! let archived = manager.list_archived_executions(
//!     Some(workflow_id),
//!     None,
//!     100,
//!     0
//! ).await?;
//!
//! // Restore specific execution
//! manager.restore_execution(execution_id).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, StorageError};
use crate::pool::DatabasePool;
use chrono::{DateTime, Duration, Utc};
use oxify_model::ExecutionState;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

/// Configuration for archival operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivalConfig {
    /// Number of days to retain in main tables
    pub retention_days: i32,
    /// Batch size for archival operations
    pub batch_size: i32,
}

impl Default for ArchivalConfig {
    fn default() -> Self {
        Self {
            retention_days: 90, // 3 months in hot storage
            batch_size: 1000,   // Archive 1000 records at a time
        }
    }
}

/// Statistics about archival operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivalStats {
    /// Number of executions archived
    pub archived_count: i64,
    /// Number of variables archived
    pub variables_archived: i64,
    /// Number of duration records archived
    pub durations_archived: i64,
    /// Estimated space freed in bytes
    pub space_freed: i64,
    /// Duration of archival operation
    pub duration_ms: i64,
}

/// Archived execution summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedExecution {
    /// Execution ID
    pub id: Uuid,
    /// Workflow ID
    pub workflow_id: Uuid,
    /// Execution state
    pub state: ExecutionState,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Completion timestamp
    pub completed_at: Option<DateTime<Utc>>,
    /// Archive timestamp
    pub archived_at: DateTime<Utc>,
}

/// Archival management service
pub struct ArchivalManager {
    pool: DatabasePool,
}

impl ArchivalManager {
    /// Create a new archival manager
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Initialize archive tables
    ///
    /// Creates archive tables if they don't exist. Safe to call multiple times.
    pub async fn initialize_archive_tables(&self) -> Result<()> {
        // Create executions_archive table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS executions_archive (
                LIKE executions INCLUDING ALL,
                archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(self.pool.pool())
        .await
        .map_err(StorageError::Database)?;

        // Create index on archived_at
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_executions_archive_archived_at
            ON executions_archive(archived_at DESC)",
        )
        .execute(self.pool.pool())
        .await
        .map_err(StorageError::Database)?;

        // Create index on workflow_id
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_executions_archive_workflow_id
            ON executions_archive(workflow_id)",
        )
        .execute(self.pool.pool())
        .await
        .map_err(StorageError::Database)?;

        // Create execution_variables_archive table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS execution_variables_archive (
                LIKE execution_variables INCLUDING ALL,
                archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(self.pool.pool())
        .await
        .map_err(StorageError::Database)?;

        // Create execution_durations_archive table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS execution_durations_archive (
                LIKE execution_durations INCLUDING ALL,
                archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(self.pool.pool())
        .await
        .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Archive old executions based on retention policy
    ///
    /// Returns statistics about the archival operation
    pub async fn archive_old_executions(&self, config: &ArchivalConfig) -> Result<ArchivalStats> {
        let start_time = Utc::now();

        // Calculate cutoff date
        let cutoff = Utc::now() - Duration::days(i64::from(config.retention_days));

        let mut tx = self
            .pool
            .pool()
            .begin()
            .await
            .map_err(StorageError::Database)?;

        // Get count of executions to archive
        let count_row = sqlx::query(
            "SELECT COUNT(*) FROM executions
            WHERE created_at < $1
            AND state IN ('completed', 'failed', 'cancelled')",
        )
        .bind(cutoff)
        .fetch_one(&mut *tx)
        .await
        .map_err(StorageError::Database)?;

        let total_count: i64 = count_row.get(0);

        if total_count == 0 {
            tx.commit().await?;
            return Ok(ArchivalStats {
                archived_count: 0,
                variables_archived: 0,
                durations_archived: 0,
                space_freed: 0,
                duration_ms: 0,
            });
        }

        // Archive executions in batches
        let mut archived_count = 0;
        let mut variables_archived = 0;
        let mut durations_archived = 0;

        while archived_count < total_count {
            // Get batch of execution IDs
            let rows = sqlx::query(
                "SELECT id FROM executions
                WHERE created_at < $1
                AND state IN ('completed', 'failed', 'cancelled')
                ORDER BY created_at
                LIMIT $2",
            )
            .bind(cutoff)
            .bind(config.batch_size)
            .fetch_all(&mut *tx)
            .await
            .map_err(StorageError::Database)?;

            if rows.is_empty() {
                break;
            }

            let execution_ids: Vec<Uuid> = rows.iter().map(|r| r.get("id")).collect();

            // Copy executions to archive
            let result = sqlx::query(
                "INSERT INTO executions_archive
                SELECT *, NOW() as archived_at FROM executions
                WHERE id = ANY($1)",
            )
            .bind(&execution_ids)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::Database)?;

            archived_count += result.rows_affected() as i64;

            // Copy variables to archive
            let vars_result = sqlx::query(
                "INSERT INTO execution_variables_archive
                SELECT *, NOW() as archived_at FROM execution_variables
                WHERE execution_id = ANY($1)",
            )
            .bind(&execution_ids)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::Database)?;

            variables_archived += vars_result.rows_affected() as i64;

            // Copy durations to archive
            let durations_result = sqlx::query(
                "INSERT INTO execution_durations_archive
                SELECT *, NOW() as archived_at FROM execution_durations
                WHERE execution_id = ANY($1)",
            )
            .bind(&execution_ids)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::Database)?;

            durations_archived += durations_result.rows_affected() as i64;

            // Delete from main tables (cascading deletes handle related records)
            sqlx::query("DELETE FROM executions WHERE id = ANY($1)")
                .bind(&execution_ids)
                .execute(&mut *tx)
                .await
                .map_err(StorageError::Database)?;
        }

        // Estimate space freed (approximate)
        let space_freed = archived_count * 1024; // Rough estimate: 1KB per execution

        tx.commit().await?;

        let duration_ms = (Utc::now() - start_time).num_milliseconds();

        tracing::info!(
            "Archived {} executions, {} variables, {} durations in {}ms",
            archived_count,
            variables_archived,
            durations_archived,
            duration_ms
        );

        Ok(ArchivalStats {
            archived_count,
            variables_archived,
            durations_archived,
            space_freed,
            duration_ms,
        })
    }

    /// List archived executions with optional filtering
    pub async fn list_archived_executions(
        &self,
        workflow_id: Option<Uuid>,
        _state: Option<ExecutionState>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ArchivedExecution>> {
        // Simplified version without state filtering for now
        // State filtering can be added when ExecutionState has proper Display/FromStr traits
        let rows = if let Some(wf_id) = workflow_id {
            sqlx::query_as::<
                _,
                (
                    Uuid,
                    Uuid,
                    String,
                    DateTime<Utc>,
                    Option<DateTime<Utc>>,
                    DateTime<Utc>,
                ),
            >(
                "SELECT id, workflow_id, state, created_at, completed_at, archived_at
                FROM executions_archive
                WHERE workflow_id = $1
                ORDER BY archived_at DESC
                LIMIT $2 OFFSET $3",
            )
            .bind(wf_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool.pool())
            .await?
        } else {
            sqlx::query_as::<
                _,
                (
                    Uuid,
                    Uuid,
                    String,
                    DateTime<Utc>,
                    Option<DateTime<Utc>>,
                    DateTime<Utc>,
                ),
            >(
                "SELECT id, workflow_id, state, created_at, completed_at, archived_at
                FROM executions_archive
                ORDER BY archived_at DESC
                LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool.pool())
            .await?
        };

        let executions = rows
            .into_iter()
            .map(
                |(id, workflow_id, state_str, created_at, completed_at, archived_at)| {
                    let state = serde_json::from_str::<ExecutionState>(&format!("\"{state_str}\""))
                        .unwrap_or(ExecutionState::Running);

                    ArchivedExecution {
                        id,
                        workflow_id,
                        state,
                        created_at,
                        completed_at,
                        archived_at,
                    }
                },
            )
            .collect();

        Ok(executions)
    }

    /// Restore a specific execution from archive
    pub async fn restore_execution(&self, execution_id: Uuid) -> Result<()> {
        let mut tx = self
            .pool
            .pool()
            .begin()
            .await
            .map_err(StorageError::Database)?;

        // Check if execution exists in archive
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM executions_archive WHERE id = $1)")
                .bind(execution_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(StorageError::Database)?;

        if !exists {
            return Err(StorageError::NotFoundLegacy(format!(
                "Execution {execution_id} not found in archive"
            )));
        }

        // Copy execution back to main table
        sqlx::query(
            "INSERT INTO executions
            SELECT id, workflow_id, state, input_data, output_data, error_message,
                   started_at, completed_at, retry_count, created_at, updated_at
            FROM executions_archive WHERE id = $1",
        )
        .bind(execution_id)
        .execute(&mut *tx)
        .await
        .map_err(StorageError::Database)?;

        // Copy variables back
        sqlx::query(
            "INSERT INTO execution_variables
            SELECT execution_id, key, value
            FROM execution_variables_archive WHERE execution_id = $1",
        )
        .bind(execution_id)
        .execute(&mut *tx)
        .await
        .map_err(StorageError::Database)?;

        // Copy durations back
        sqlx::query(
            "INSERT INTO execution_durations
            SELECT execution_id, workflow_id, time_bucket, duration_ms
            FROM execution_durations_archive WHERE execution_id = $1",
        )
        .bind(execution_id)
        .execute(&mut *tx)
        .await
        .map_err(StorageError::Database)?;

        // Delete from archive
        sqlx::query("DELETE FROM executions_archive WHERE id = $1")
            .bind(execution_id)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::Database)?;

        tx.commit().await?;

        tracing::info!("Restored execution {} from archive", execution_id);

        Ok(())
    }

    /// Delete archived executions older than specified days
    pub async fn delete_old_archived_executions(&self, retention_days: i32) -> Result<i64> {
        let cutoff = Utc::now() - Duration::days(i64::from(retention_days));

        let result = sqlx::query("DELETE FROM executions_archive WHERE archived_at < $1")
            .bind(cutoff)
            .execute(self.pool.pool())
            .await
            .map_err(StorageError::Database)?;

        let deleted = result.rows_affected() as i64;
        tracing::info!(
            "Deleted {} archived executions older than {} days",
            deleted,
            retention_days
        );

        Ok(deleted)
    }

    /// Get archive statistics
    pub async fn get_archive_stats(&self) -> Result<ArchiveStats> {
        let row = sqlx::query(
            "SELECT
                COUNT(*) as total_executions,
                COUNT(DISTINCT workflow_id) as unique_workflows,
                pg_total_relation_size('executions_archive') as size_bytes,
                MIN(created_at) as oldest_execution,
                MAX(created_at) as newest_execution,
                MIN(archived_at) as first_archived,
                MAX(archived_at) as last_archived
            FROM executions_archive",
        )
        .fetch_one(self.pool.pool())
        .await
        .map_err(StorageError::Database)?;

        Ok(ArchiveStats {
            total_executions: row.get("total_executions"),
            unique_workflows: row.get("unique_workflows"),
            size_bytes: row.get("size_bytes"),
            oldest_execution: row.get("oldest_execution"),
            newest_execution: row.get("newest_execution"),
            first_archived: row.get("first_archived"),
            last_archived: row.get("last_archived"),
        })
    }

    /// Export archived executions to JSON
    ///
    /// Returns JSON array of archived executions for the specified workflow
    pub async fn export_archived_to_json(&self, workflow_id: Option<Uuid>) -> Result<String> {
        let executions = self
            .list_archived_executions(workflow_id, None, 10000, 0)
            .await?;
        serde_json::to_string_pretty(&executions).map_err(StorageError::Serialization)
    }
}

/// Archive statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveStats {
    /// Total number of archived executions
    pub total_executions: i64,
    /// Number of unique workflows
    pub unique_workflows: i64,
    /// Total size of archive in bytes
    pub size_bytes: i64,
    /// Oldest execution in archive
    pub oldest_execution: Option<DateTime<Utc>>,
    /// Newest execution in archive
    pub newest_execution: Option<DateTime<Utc>>,
    /// First archival timestamp
    pub first_archived: Option<DateTime<Utc>>,
    /// Last archival timestamp
    pub last_archived: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_archival_config() {
        let config = ArchivalConfig::default();
        assert_eq!(config.retention_days, 90);
        assert_eq!(config.batch_size, 1000);
    }

    #[test]
    fn test_archival_stats_serialization() {
        let stats = ArchivalStats {
            archived_count: 1000,
            variables_archived: 5000,
            durations_archived: 1000,
            space_freed: 1024000,
            duration_ms: 5000,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: ArchivalStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.archived_count, stats.archived_count);
    }

    #[test]
    fn test_archived_execution() {
        let now = Utc::now();
        let exec = ArchivedExecution {
            id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            state: ExecutionState::Completed,
            created_at: now,
            completed_at: Some(now),
            archived_at: now,
        };

        assert_eq!(exec.state, ExecutionState::Completed);
    }
}
