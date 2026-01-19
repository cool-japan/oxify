//! # Table Partitioning for Time-Series Data
//!
//! This module provides utilities for PostgreSQL table partitioning to efficiently
//! manage large time-series datasets. Partitioning improves query performance and
//! simplifies data retention management by allowing old partitions to be dropped
//! without expensive DELETE operations.
//!
//! ## Supported Tables
//!
//! The following tables benefit from partitioning:
//! - `executions` - Partitioned by `created_at` (monthly)
//! - `audit_logs` - Partitioned by `timestamp` (monthly)
//! - `metrics` - Partitioned by `time_bucket` (monthly)
//! - `execution_durations` - Partitioned by `time_bucket` (monthly)
//! - `api_key_usage_logs` - Partitioned by `timestamp` (monthly)
//! - `schedule_executions` - Partitioned by `executed_at` (monthly)
//!
//! ## Partitioning Strategy
//!
//! We use PostgreSQL's declarative partitioning with RANGE partitioning by date:
//! - Monthly partitions for most tables
//! - Automatic partition creation for future months
//! - Configurable retention policy for dropping old partitions
//!
//! ## Migration Strategy
//!
//! To convert an existing table to partitioned:
//! 1. Create new partitioned table with `_partitioned` suffix
//! 2. Copy data from old table to new partitioned table
//! 3. Rename old table to `_old` suffix
//! 4. Rename partitioned table to original name
//! 5. Drop old table after verification
//!
//! ## Example
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, PartitionManager, PartitionConfig};
//! use chrono::{Utc, Duration};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let pool = DatabasePool::new("postgres://localhost/test").await?;
//! let manager = PartitionManager::new(pool.clone());
//!
//! // Configure partitioning
//! let config = PartitionConfig {
//!     retention_months: 12,
//!     future_partitions: 3,
//! };
//!
//! // Create partitions for executions table
//! manager.ensure_partitions("executions", "created_at", &config).await?;
//!
//! // List all partitions
//! let partitions = manager.list_partitions("executions").await?;
//! for partition in partitions {
//!     println!("Partition: {}, rows: {}, size: {}",
//!         partition.name, partition.row_count, partition.size_bytes);
//! }
//!
//! // Drop old partitions based on retention policy
//! let dropped = manager.drop_old_partitions("executions", &config).await?;
//! println!("Dropped {} old partitions", dropped);
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, StorageError};
use crate::pool::DatabasePool;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Configuration for partition management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionConfig {
    /// Number of months to retain partitions
    pub retention_months: i32,
    /// Number of future partitions to pre-create
    pub future_partitions: i32,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        Self {
            retention_months: 12, // 1 year retention
            future_partitions: 3, // 3 months ahead
        }
    }
}

/// Information about a partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    /// Partition name
    pub name: String,
    /// Parent table name
    pub parent_table: String,
    /// Start date of partition range (inclusive)
    pub start_date: NaiveDate,
    /// End date of partition range (exclusive)
    pub end_date: NaiveDate,
    /// Number of rows in partition
    pub row_count: i64,
    /// Size in bytes
    pub size_bytes: i64,
}

/// Partition management service
pub struct PartitionManager {
    pool: DatabasePool,
}

impl PartitionManager {
    /// Create a new partition manager
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Ensure partitions exist for the specified table
    ///
    /// This will create partitions for the retention period plus future partitions.
    pub async fn ensure_partitions(
        &self,
        table_name: &str,
        date_column: &str,
        config: &PartitionConfig,
    ) -> Result<Vec<String>> {
        let mut created = Vec::new();

        // Calculate date range
        let now = Utc::now();
        let start_date = now - Duration::days(i64::from(config.retention_months * 31));
        let end_date = now + Duration::days(i64::from(config.future_partitions * 31));

        // Generate monthly partitions
        let mut current = NaiveDate::from_ymd_opt(start_date.year(), start_date.month(), 1)
            .ok_or_else(|| StorageError::ValidationError("Invalid start date".into()))?;

        let end = NaiveDate::from_ymd_opt(end_date.year(), end_date.month(), 1)
            .ok_or_else(|| StorageError::ValidationError("Invalid end date".into()))?;

        while current <= end {
            let partition_name = format!(
                "{}_{:04}_{:02}",
                table_name,
                current.year(),
                current.month()
            );

            // Check if partition already exists
            if !self.partition_exists(&partition_name).await? {
                // Calculate next month for range end
                let next_month = if current.month() == 12 {
                    NaiveDate::from_ymd_opt(current.year() + 1, 1, 1)
                } else {
                    NaiveDate::from_ymd_opt(current.year(), current.month() + 1, 1)
                }
                .ok_or_else(|| StorageError::ValidationError("Invalid next month".into()))?;

                // Create partition
                self.create_partition(
                    table_name,
                    &partition_name,
                    date_column,
                    current,
                    next_month,
                )
                .await?;

                created.push(partition_name);
            }

            // Move to next month
            current = if current.month() == 12 {
                NaiveDate::from_ymd_opt(current.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(current.year(), current.month() + 1, 1)
            }
            .ok_or_else(|| StorageError::ValidationError("Invalid next month".into()))?;
        }

        Ok(created)
    }

    /// Check if a partition exists
    async fn partition_exists(&self, partition_name: &str) -> Result<bool> {
        let row = sqlx::query(
            "SELECT EXISTS(
                SELECT 1 FROM pg_tables
                WHERE tablename = $1
            )",
        )
        .bind(partition_name)
        .fetch_one(self.pool.pool())
        .await
        .map_err(StorageError::Database)?;

        Ok(row.get(0))
    }

    /// Create a new partition
    async fn create_partition(
        &self,
        parent_table: &str,
        partition_name: &str,
        _date_column: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<()> {
        // Note: This assumes the parent table is already partitioned
        // For initial setup, use convert_to_partitioned() first
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} PARTITION OF {}
            FOR VALUES FROM ('{}') TO ('{}')",
            partition_name,
            parent_table,
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d")
        );

        sqlx::query(&sql)
            .execute(self.pool.pool())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// List all partitions for a table
    pub async fn list_partitions(&self, parent_table: &str) -> Result<Vec<PartitionInfo>> {
        let rows = sqlx::query(
            "SELECT
                child.relname AS partition_name,
                pg_get_expr(child.relpartbound, child.oid) AS partition_expr,
                pg_total_relation_size(child.oid) AS size_bytes,
                COALESCE(s.n_live_tup, 0) AS row_count
            FROM pg_inherits
            JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
            JOIN pg_class child ON pg_inherits.inhrelid = child.oid
            LEFT JOIN pg_stat_user_tables s ON s.relname = child.relname
            WHERE parent.relname = $1
            ORDER BY child.relname",
        )
        .bind(parent_table)
        .fetch_all(self.pool.pool())
        .await
        .map_err(StorageError::Database)?;

        let mut partitions = Vec::new();
        for row in rows {
            let name: String = row.get("partition_name");
            let size_bytes: i64 = row.get("size_bytes");
            let row_count: i64 = row.get("row_count");

            // Parse partition name to extract date range
            // Format: tablename_YYYY_MM
            let parts: Vec<&str> = name.rsplitn(3, '_').collect();
            if parts.len() >= 2 {
                if let (Ok(month), Ok(year)) = (parts[0].parse::<u32>(), parts[1].parse::<i32>()) {
                    if let Some(start_date) = NaiveDate::from_ymd_opt(year, month, 1) {
                        let end_date = if month == 12 {
                            NaiveDate::from_ymd_opt(year + 1, 1, 1)
                        } else {
                            NaiveDate::from_ymd_opt(year, month + 1, 1)
                        };

                        if let Some(end_date) = end_date {
                            partitions.push(PartitionInfo {
                                name,
                                parent_table: parent_table.to_string(),
                                start_date,
                                end_date,
                                row_count,
                                size_bytes,
                            });
                        }
                    }
                }
            }
        }

        Ok(partitions)
    }

    /// Drop old partitions based on retention policy
    ///
    /// Returns the number of partitions dropped
    pub async fn drop_old_partitions(
        &self,
        parent_table: &str,
        config: &PartitionConfig,
    ) -> Result<usize> {
        let partitions = self.list_partitions(parent_table).await?;
        let cutoff = Utc::now() - Duration::days(i64::from(config.retention_months * 31));
        let cutoff_date = NaiveDate::from_ymd_opt(cutoff.year(), cutoff.month(), 1)
            .ok_or_else(|| StorageError::ValidationError("Invalid cutoff date".into()))?;

        let mut dropped = 0;
        for partition in partitions {
            if partition.end_date <= cutoff_date {
                self.drop_partition(&partition.name).await?;
                dropped += 1;
                tracing::info!(
                    "Dropped old partition {} (ended {})",
                    partition.name,
                    partition.end_date
                );
            }
        }

        Ok(dropped)
    }

    /// Drop a specific partition
    async fn drop_partition(&self, partition_name: &str) -> Result<()> {
        let sql = format!("DROP TABLE IF EXISTS {partition_name}");
        sqlx::query(&sql)
            .execute(self.pool.pool())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Convert an existing table to partitioned table
    ///
    /// WARNING: This is a destructive operation that requires downtime.
    /// It creates a new partitioned table and copies data from the old table.
    ///
    /// Steps:
    /// 1. Creates new partitioned table with `_partitioned` suffix
    /// 2. Creates necessary partitions
    /// 3. Copies data from old table
    /// 4. Renames old table to `_old` suffix
    /// 5. Renames partitioned table to original name
    ///
    /// You must manually drop the `_old` table after verification.
    pub async fn convert_to_partitioned(
        &self,
        table_name: &str,
        date_column: &str,
        config: &PartitionConfig,
    ) -> Result<()> {
        let partitioned_name = format!("{table_name}_partitioned");
        let old_name = format!("{table_name}_old");

        // Step 1: Create partitioned table
        let create_sql = format!(
            "CREATE TABLE {partitioned_name} (LIKE {table_name} INCLUDING ALL)
            PARTITION BY RANGE ({date_column})"
        );
        sqlx::query(&create_sql)
            .execute(self.pool.pool())
            .await
            .map_err(StorageError::Database)?;

        // Step 2: Create partitions
        let now = Utc::now();
        let start_date = now - Duration::days(i64::from(config.retention_months * 31));
        let end_date = now + Duration::days(i64::from(config.future_partitions * 31));

        let mut current = NaiveDate::from_ymd_opt(start_date.year(), start_date.month(), 1)
            .ok_or_else(|| StorageError::ValidationError("Invalid start date".into()))?;

        let end = NaiveDate::from_ymd_opt(end_date.year(), end_date.month(), 1)
            .ok_or_else(|| StorageError::ValidationError("Invalid end date".into()))?;

        while current <= end {
            let partition_name = format!(
                "{}_{:04}_{:02}",
                partitioned_name,
                current.year(),
                current.month()
            );

            let next_month = if current.month() == 12 {
                NaiveDate::from_ymd_opt(current.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(current.year(), current.month() + 1, 1)
            }
            .ok_or_else(|| StorageError::ValidationError("Invalid next month".into()))?;

            let partition_sql = format!(
                "CREATE TABLE {} PARTITION OF {}
                FOR VALUES FROM ('{}') TO ('{}')",
                partition_name,
                partitioned_name,
                current.format("%Y-%m-%d"),
                next_month.format("%Y-%m-%d")
            );
            sqlx::query(&partition_sql)
                .execute(self.pool.pool())
                .await
                .map_err(StorageError::Database)?;

            current = next_month;
        }

        // Step 3: Copy data
        let copy_sql = format!("INSERT INTO {partitioned_name} SELECT * FROM {table_name}");
        sqlx::query(&copy_sql)
            .execute(self.pool.pool())
            .await
            .map_err(StorageError::Database)?;

        // Step 4: Rename tables
        let rename_old_sql = format!("ALTER TABLE {table_name} RENAME TO {old_name}");
        sqlx::query(&rename_old_sql)
            .execute(self.pool.pool())
            .await
            .map_err(StorageError::Database)?;

        let rename_new_sql = format!("ALTER TABLE {partitioned_name} RENAME TO {table_name}");
        sqlx::query(&rename_new_sql)
            .execute(self.pool.pool())
            .await
            .map_err(StorageError::Database)?;

        tracing::info!(
            "Converted {} to partitioned table. Old table renamed to {}",
            table_name,
            old_name
        );
        tracing::warn!("Remember to manually drop {} after verification", old_name);

        Ok(())
    }

    /// Get total size of all partitions for a table
    pub async fn get_table_total_size(&self, parent_table: &str) -> Result<i64> {
        let partitions = self.list_partitions(parent_table).await?;
        Ok(partitions.iter().map(|p| p.size_bytes).sum())
    }

    /// Get partition statistics summary
    pub async fn get_partition_stats(&self, parent_table: &str) -> Result<PartitionStats> {
        let partitions = self.list_partitions(parent_table).await?;

        let total_partitions = partitions.len();
        let total_size = partitions.iter().map(|p| p.size_bytes).sum();
        let total_rows = partitions.iter().map(|p| p.row_count).sum();

        let oldest = partitions.first().map(|p| p.start_date);
        let newest = partitions.last().map(|p| p.end_date);

        Ok(PartitionStats {
            parent_table: parent_table.to_string(),
            total_partitions,
            total_size_bytes: total_size,
            total_rows,
            oldest_partition: oldest,
            newest_partition: newest,
        })
    }
}

/// Statistics summary for partitioned table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionStats {
    /// Parent table name
    pub parent_table: String,
    /// Total number of partitions
    pub total_partitions: usize,
    /// Total size in bytes
    pub total_size_bytes: i64,
    /// Total number of rows
    pub total_rows: i64,
    /// Oldest partition start date
    pub oldest_partition: Option<NaiveDate>,
    /// Newest partition end date
    pub newest_partition: Option<NaiveDate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_partition_config() {
        let config = PartitionConfig::default();
        assert_eq!(config.retention_months, 12);
        assert_eq!(config.future_partitions, 3);
    }

    #[test]
    fn test_partition_info_serialization() {
        let info = PartitionInfo {
            name: "executions_2026_01".to_string(),
            parent_table: "executions".to_string(),
            start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            row_count: 1000,
            size_bytes: 1024000,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: PartitionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, info.name);
        assert_eq!(deserialized.row_count, info.row_count);
    }

    #[test]
    fn test_partition_stats() {
        let stats = PartitionStats {
            parent_table: "executions".to_string(),
            total_partitions: 12,
            total_size_bytes: 1024000000,
            total_rows: 1000000,
            oldest_partition: Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            newest_partition: Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()),
        };

        assert_eq!(stats.total_partitions, 12);
        assert!(stats.total_size_bytes > 0);
    }
}
