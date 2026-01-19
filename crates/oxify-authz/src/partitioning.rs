//! PostgreSQL Partitioning Support
//!
//! Implements table partitioning strategies for scaling to 100M+ tuples:
//! - **Tenant-based partitioning**: Logical isolation by tenant_id
//! - **Time-based partitioning**: Archival and retention policies
//! - **Hybrid partitioning**: Combined tenant + time for optimal performance
//!
//! ## Architecture
//!
//! ```text
//! relation_tuples (parent table)
//! ├── relation_tuples_tenant_1 (tenant partitions)
//! ├── relation_tuples_tenant_2
//! └── ...
//!
//! audit_log (parent table, time-based)
//! ├── audit_log_2026_01 (monthly partitions)
//! ├── audit_log_2026_02
//! └── ...
//! ```
//!
//! ## Benefits
//!
//! - **Isolation**: Perfect tenant data separation
//! - **Performance**: Partition pruning reduces scan cost
//! - **Scalability**: Easy to add/drop partitions
//! - **Archival**: Time-based cleanup for compliance
//!
//! ## Usage
//!
//! ```no_run
//! use oxify_authz::partitioning::*;
//! use sqlx::PgPool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let pool = PgPool::connect("postgres://localhost/db").await?;
//! let manager = PartitionManager::new(pool);
//!
//! // Create tenant partition
//! manager.create_tenant_partition("tenant_123").await?;
//!
//! // Create monthly audit partition
//! manager.create_time_partition("audit_log", "2026_01").await?;
//!
//! // List all partitions
//! let partitions = manager.list_partitions().await?;
//! # Ok(())
//! # }
//! ```

use crate::{AuthzError, Result};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;

/// Partitioning strategy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PartitionStrategy {
    /// Partition by tenant_id (hash or list)
    Tenant,
    /// Partition by time (range partitioning)
    Time,
    /// Hybrid: tenant + time (sub-partitioning)
    Hybrid,
}

/// Partition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    /// Partition name
    pub name: String,
    /// Parent table name
    pub parent_table: String,
    /// Partition strategy
    pub strategy: PartitionStrategy,
    /// Partition key (tenant_id or time range)
    pub key: String,
    /// Size in bytes
    pub size_bytes: i64,
    /// Number of rows
    pub row_count: i64,
    /// Created at
    pub created_at: DateTime<Utc>,
}

/// Configuration for partition management
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Automatically create partitions for new tenants
    pub auto_create_tenant: bool,
    /// Monthly time partitions (true) or daily (false)
    pub monthly_time_partitions: bool,
    /// Retention period for audit logs (days)
    pub audit_retention_days: u32,
    /// Maximum partition size before warning (GB)
    pub max_partition_size_gb: u32,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        Self {
            auto_create_tenant: true,
            monthly_time_partitions: true,
            audit_retention_days: 90,
            max_partition_size_gb: 100,
        }
    }
}

/// Partition manager for PostgreSQL
pub struct PartitionManager {
    pool: Arc<PgPool>,
    config: PartitionConfig,
}

impl PartitionManager {
    /// Create a new partition manager
    pub fn new(pool: PgPool) -> Self {
        Self::with_config(pool, PartitionConfig::default())
    }

    /// Create a partition manager with custom configuration
    pub fn with_config(pool: PgPool, config: PartitionConfig) -> Self {
        Self {
            pool: Arc::new(pool),
            config,
        }
    }

    /// Initialize partitioning for relation_tuples table
    #[allow(dead_code)]
    pub async fn init_relation_tuples_partitioning(&self) -> Result<()> {
        // Create parent table if not exists (should already exist)
        // Convert to partitioned table by tenant_id
        let sql = r#"
            -- Check if table is already partitioned
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_partitioned_table
                    WHERE partrelid = 'relation_tuples'::regclass
                ) THEN
                    -- Cannot convert existing table to partitioned without recreation
                    -- This should be done during initial migration
                    RAISE NOTICE 'relation_tuples is not partitioned. Run migration first.';
                END IF;
            END $$;
        "#;

        sqlx::query(sql).execute(&*self.pool).await.map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to check partitioning: {}", e))
        })?;

        Ok(())
    }

    /// Create a tenant partition
    #[allow(dead_code)]
    pub async fn create_tenant_partition(&self, tenant_id: &str) -> Result<()> {
        let partition_name = format!("relation_tuples_tenant_{}", sanitize_identifier(tenant_id));

        let sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} PARTITION OF relation_tuples
            FOR VALUES IN ('{}');

            -- Create indexes on partition
            CREATE INDEX IF NOT EXISTS {}_idx_namespace_object_relation
                ON {} (namespace, object_id, relation);

            CREATE INDEX IF NOT EXISTS {}_idx_subject
                ON {} (subject_type, subject_id);
            "#,
            partition_name,
            tenant_id,
            partition_name,
            partition_name,
            partition_name,
            partition_name
        );

        sqlx::query(&sql).execute(&*self.pool).await.map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to create tenant partition: {}", e))
        })?;

        tracing::info!("Created tenant partition: {}", partition_name);
        Ok(())
    }

    /// Create a time-based partition for audit logs
    pub async fn create_time_partition(&self, table_name: &str, period: &str) -> Result<()> {
        let partition_name = format!("{}_{}", table_name, period);

        // Parse period (e.g., "2026_01" for monthly)
        let (year, month) = parse_period(period)?;
        let start_date = format!("{:04}-{:02}-01", year, month);
        let end_date = if month == 12 {
            format!("{:04}-01-01", year + 1)
        } else {
            format!("{:04}-{:02}-01", year, month + 1)
        };

        let sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} PARTITION OF {}
            FOR VALUES FROM ('{}') TO ('{}');

            -- Create indexes on partition
            CREATE INDEX IF NOT EXISTS {}_idx_timestamp
                ON {} (timestamp);

            CREATE INDEX IF NOT EXISTS {}_idx_subject
                ON {} (subject);
            "#,
            partition_name,
            table_name,
            start_date,
            end_date,
            partition_name,
            partition_name,
            partition_name,
            partition_name
        );

        sqlx::query(&sql).execute(&*self.pool).await.map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to create time partition: {}", e))
        })?;

        tracing::info!("Created time partition: {}", partition_name);
        Ok(())
    }

    /// List all partitions for a table
    pub async fn list_partitions(&self) -> Result<Vec<PartitionInfo>> {
        let sql = r#"
            SELECT
                c.relname AS partition_name,
                p.relname AS parent_table,
                pg_get_expr(c.relpartbound, c.oid) AS partition_key,
                pg_total_relation_size(c.oid) AS size_bytes,
                c.reltuples::bigint AS row_count
            FROM pg_class c
            JOIN pg_inherits i ON c.oid = i.inhrelid
            JOIN pg_class p ON i.inhparent = p.oid
            WHERE c.relkind = 'r' AND p.relkind = 'p'
            ORDER BY p.relname, c.relname;
        "#;

        let rows = sqlx::query(sql)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to list partitions: {}", e)))?;

        let mut partitions = Vec::new();
        for row in rows {
            let name: String = row.try_get("partition_name").map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to get partition_name: {}", e))
            })?;
            let parent: String = row.try_get("parent_table").map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to get parent_table: {}", e))
            })?;
            let key: String = row.try_get("partition_key").map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to get partition_key: {}", e))
            })?;
            let size: i64 = row.try_get("size_bytes").map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to get size_bytes: {}", e))
            })?;
            let count: i64 = row.try_get("row_count").map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to get row_count: {}", e))
            })?;

            // Determine strategy from partition name
            let strategy = if name.contains("tenant") {
                PartitionStrategy::Tenant
            } else {
                PartitionStrategy::Time
            };

            partitions.push(PartitionInfo {
                name,
                parent_table: parent,
                strategy,
                key,
                size_bytes: size,
                row_count: count,
                created_at: Utc::now(), // PostgreSQL doesn't track partition creation time
            });
        }

        Ok(partitions)
    }

    /// Drop old audit log partitions based on retention policy
    pub async fn cleanup_old_partitions(&self, table_name: &str) -> Result<Vec<String>> {
        let cutoff_date = Utc::now()
            .checked_sub_signed(chrono::Duration::days(
                self.config.audit_retention_days as i64,
            ))
            .ok_or_else(|| AuthzError::DatabaseError("Invalid date calculation".to_string()))?;

        let cutoff_period = format!("{}_{:02}", cutoff_date.year(), cutoff_date.month());

        tracing::info!("Cleaning up audit partitions older than: {}", cutoff_period);

        let partitions = self.list_partitions().await?;
        let mut dropped = Vec::new();

        for partition in partitions {
            if partition.parent_table == table_name && partition.strategy == PartitionStrategy::Time
            {
                // Extract period from partition name (e.g., "audit_log_2026_01" -> "2026_01")
                if let Some(period) = partition.name.strip_prefix(&format!("{}_", table_name)) {
                    if period < cutoff_period.as_str() {
                        self.drop_partition(&partition.name).await?;
                        dropped.push(partition.name.clone());
                    }
                }
            }
        }

        Ok(dropped)
    }

    /// Drop a partition
    pub async fn drop_partition(&self, partition_name: &str) -> Result<()> {
        let sql = format!("DROP TABLE IF EXISTS {} CASCADE;", partition_name);

        sqlx::query(&sql)
            .execute(&*self.pool)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to drop partition: {}", e)))?;

        tracing::info!("Dropped partition: {}", partition_name);
        Ok(())
    }

    /// Create next month's partition proactively
    pub async fn create_next_month_partition(&self, table_name: &str) -> Result<()> {
        let next_month = Utc::now()
            .checked_add_signed(chrono::Duration::days(30))
            .ok_or_else(|| AuthzError::DatabaseError("Invalid date calculation".to_string()))?;

        let period = format!("{}_{:02}", next_month.year(), next_month.month());
        self.create_time_partition(table_name, &period).await
    }

    /// Get partition statistics
    pub async fn partition_stats(&self) -> Result<Vec<PartitionInfo>> {
        self.list_partitions().await
    }
}

/// Sanitize identifier for SQL (prevent injection)
fn sanitize_identifier(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// Parse period string (e.g., "2026_01") into year and month
fn parse_period(period: &str) -> Result<(i32, u32)> {
    let parts: Vec<&str> = period.split('_').collect();
    if parts.len() != 2 {
        return Err(AuthzError::DatabaseError(format!(
            "Invalid period format: {}",
            period
        )));
    }

    let year = parts[0]
        .parse::<i32>()
        .map_err(|_| AuthzError::DatabaseError(format!("Invalid year in period: {}", period)))?;

    let month = parts[1]
        .parse::<u32>()
        .map_err(|_| AuthzError::DatabaseError(format!("Invalid month in period: {}", period)))?;

    if !(1..=12).contains(&month) {
        return Err(AuthzError::DatabaseError(format!(
            "Month out of range: {}",
            month
        )));
    }

    Ok((year, month))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_identifier() {
        assert_eq!(sanitize_identifier("tenant_123"), "tenant_123");
        assert_eq!(sanitize_identifier("tenant-123"), "tenant123");
        assert_eq!(sanitize_identifier("tenant@123"), "tenant123");
        assert_eq!(sanitize_identifier("tenant_abc_def"), "tenant_abc_def");
    }

    #[test]
    fn test_parse_period() {
        assert_eq!(parse_period("2026_01").unwrap(), (2026, 1));
        assert_eq!(parse_period("2026_12").unwrap(), (2026, 12));
        assert!(parse_period("2026_13").is_err()); // Invalid month
        assert!(parse_period("2026-01").is_err()); // Wrong format
        assert!(parse_period("202601").is_err()); // No underscore
    }

    #[test]
    fn test_partition_config_default() {
        let config = PartitionConfig::default();
        assert!(config.auto_create_tenant);
        assert!(config.monthly_time_partitions);
        assert_eq!(config.audit_retention_days, 90);
        assert_eq!(config.max_partition_size_gb, 100);
    }
}
