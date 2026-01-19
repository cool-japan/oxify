//! Data Retention and Cleanup Utilities
//!
//! Provides centralized cleanup functionality for maintaining database hygiene
//! and enforcing data retention policies.
//!
//! ## Overview
//!
//! The cleanup service manages data retention across all storage tables:
//! - Completed executions
//! - Audit logs
//! - Metrics data
//! - Webhook events
//! - API key usage logs
//! - Execution checkpoints
//! - Schedule execution history
//!
//! ## Retention Configuration
//!
//! Default retention periods:
//! - **Executions**: 30 days
//! - **Audit Logs**: 90 days
//! - **Metrics**: 90 days
//! - **Webhook Events**: 7 days
//! - **API Key Logs**: 30 days
//! - **Checkpoints**: 7 days
//! - **Schedule History**: 30 days
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{CleanupService, RetentionConfig};
//!
//! // Create with default retention config
//! let cleanup = CleanupService::new(pool);
//!
//! // Or customize retention periods
//! let config = RetentionConfig {
//!     executions_days: 60,      // Keep executions for 60 days
//!     audit_logs_days: 180,     // Keep audit logs for 6 months
//!     metrics_days: 365,        // Keep metrics for 1 year
//!     webhook_events_days: 14,  // Keep webhook events for 2 weeks
//!     ..Default::default()
//! };
//! let cleanup = CleanupService::with_config(pool, config);
//!
//! // Run cleanup for all tables
//! let results = cleanup.run_all().await?;
//! println!("Deleted {} total records in {}ms",
//!     results.total_deleted(),
//!     results.duration_ms.unwrap_or(0)
//! );
//!
//! // Or run cleanup for specific tables
//! let deleted = cleanup.cleanup_executions().await?;
//! println!("Deleted {} old executions", deleted);
//! ```

use crate::{DatabasePool, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Data retention configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    /// Days to retain completed executions (default: 90)
    pub executions_retention_days: i64,
    /// Days to retain audit logs (default: 90)
    pub audit_logs_retention_days: i64,
    /// Days to retain metrics data (default: 365)
    pub metrics_retention_days: i64,
    /// Days to retain webhook events (default: 30)
    pub webhook_events_retention_days: i64,
    /// Days to retain API key usage logs (default: 90)
    pub api_key_logs_retention_days: i64,
    /// Days to retain execution checkpoints (default: 7)
    pub checkpoints_retention_days: i64,
    /// Days to retain schedule execution history (default: 90)
    pub schedule_history_retention_days: i64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            executions_retention_days: 90,
            audit_logs_retention_days: 90,
            metrics_retention_days: 365,
            webhook_events_retention_days: 30,
            api_key_logs_retention_days: 90,
            checkpoints_retention_days: 7,
            schedule_history_retention_days: 90,
        }
    }
}

/// Cleanup results summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupResults {
    pub executions_deleted: u64,
    pub audit_logs_deleted: u64,
    pub metrics_deleted: u64,
    pub webhook_events_deleted: u64,
    pub api_key_logs_deleted: u64,
    pub checkpoints_deleted: u64,
    pub schedule_history_deleted: u64,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub errors: Vec<String>,
}

impl CleanupResults {
    fn new() -> Self {
        Self {
            started_at: Utc::now(),
            ..Default::default()
        }
    }

    fn complete(&mut self) {
        let completed_at = Utc::now();
        self.completed_at = Some(completed_at);
        self.duration_ms = Some((completed_at - self.started_at).num_milliseconds());
    }

    /// Total records deleted across all tables
    pub fn total_deleted(&self) -> u64 {
        self.executions_deleted
            + self.audit_logs_deleted
            + self.metrics_deleted
            + self.webhook_events_deleted
            + self.api_key_logs_deleted
            + self.checkpoints_deleted
            + self.schedule_history_deleted
    }
}

/// Cleanup service for managing data retention
pub struct CleanupService {
    pool: DatabasePool,
    config: RetentionConfig,
}

impl CleanupService {
    /// Create a new cleanup service with default retention config
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            config: RetentionConfig::default(),
        }
    }

    /// Create a new cleanup service with custom retention config
    pub fn with_config(pool: DatabasePool, config: RetentionConfig) -> Self {
        Self { pool, config }
    }

    /// Run all cleanup tasks based on retention configuration
    pub async fn run_all(&self) -> Result<CleanupResults> {
        let mut results = CleanupResults::new();

        info!("Starting cleanup with retention config: {:?}", self.config);

        // Clean up completed executions
        match self.cleanup_executions().await {
            Ok(count) => {
                results.executions_deleted = count;
                info!("Cleaned up {} old executions", count);
            }
            Err(e) => {
                results
                    .errors
                    .push(format!("Executions cleanup failed: {e}"));
            }
        }

        // Clean up audit logs
        match self.cleanup_audit_logs().await {
            Ok(count) => {
                results.audit_logs_deleted = count;
                info!("Cleaned up {} old audit logs", count);
            }
            Err(e) => {
                results
                    .errors
                    .push(format!("Audit logs cleanup failed: {e}"));
            }
        }

        // Clean up metrics
        match self.cleanup_metrics().await {
            Ok(count) => {
                results.metrics_deleted = count;
                info!("Cleaned up {} old metrics records", count);
            }
            Err(e) => {
                results.errors.push(format!("Metrics cleanup failed: {e}"));
            }
        }

        // Clean up webhook events
        match self.cleanup_webhook_events().await {
            Ok(count) => {
                results.webhook_events_deleted = count;
                info!("Cleaned up {} old webhook events", count);
            }
            Err(e) => {
                results
                    .errors
                    .push(format!("Webhook events cleanup failed: {e}"));
            }
        }

        // Clean up API key usage logs
        match self.cleanup_api_key_logs().await {
            Ok(count) => {
                results.api_key_logs_deleted = count;
                info!("Cleaned up {} old API key logs", count);
            }
            Err(e) => {
                results
                    .errors
                    .push(format!("API key logs cleanup failed: {e}"));
            }
        }

        // Clean up checkpoints
        match self.cleanup_checkpoints().await {
            Ok(count) => {
                results.checkpoints_deleted = count;
                info!("Cleaned up {} old checkpoints", count);
            }
            Err(e) => {
                results
                    .errors
                    .push(format!("Checkpoints cleanup failed: {e}"));
            }
        }

        // Clean up schedule execution history
        match self.cleanup_schedule_history().await {
            Ok(count) => {
                results.schedule_history_deleted = count;
                info!("Cleaned up {} old schedule executions", count);
            }
            Err(e) => {
                results
                    .errors
                    .push(format!("Schedule history cleanup failed: {e}"));
            }
        }

        results.complete();
        info!(
            "Cleanup completed: {} total records deleted in {}ms",
            results.total_deleted(),
            results.duration_ms.unwrap_or(0)
        );

        Ok(results)
    }

    /// Clean up old completed/failed executions
    pub async fn cleanup_executions(&self) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(self.config.executions_retention_days);

        let result = sqlx::query(
            r"
            DELETE FROM executions
            WHERE completed_at IS NOT NULL
              AND completed_at < $1
              AND state IN ('Completed', 'Failed', 'Cancelled')
            ",
        )
        .bind(cutoff)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Clean up old audit logs based on retention period
    pub async fn cleanup_audit_logs(&self) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(self.config.audit_logs_retention_days);

        // First try the stored procedure if available
        let result = sqlx::query(
            r"
            DELETE FROM audit_logs
            WHERE timestamp < $1
            ",
        )
        .bind(cutoff)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Clean up old metrics data
    pub async fn cleanup_metrics(&self) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(self.config.metrics_retention_days);

        let result1 = sqlx::query("DELETE FROM execution_metrics WHERE time_bucket < $1")
            .bind(cutoff)
            .execute(self.pool.pool())
            .await?;

        let result2 = sqlx::query("DELETE FROM node_metrics WHERE time_bucket < $1")
            .bind(cutoff)
            .execute(self.pool.pool())
            .await?;

        let result3 = sqlx::query("DELETE FROM system_metrics WHERE timestamp < $1")
            .bind(cutoff)
            .execute(self.pool.pool())
            .await?;

        Ok(result1.rows_affected() + result2.rows_affected() + result3.rows_affected())
    }

    /// Clean up old webhook events
    pub async fn cleanup_webhook_events(&self) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(self.config.webhook_events_retention_days);

        let result = sqlx::query(
            r"
            DELETE FROM webhook_events
            WHERE received_at < $1
            ",
        )
        .bind(cutoff)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Clean up old API key usage logs
    pub async fn cleanup_api_key_logs(&self) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(self.config.api_key_logs_retention_days);

        let result = sqlx::query(
            r"
            DELETE FROM api_key_usage_logs
            WHERE timestamp < $1
            ",
        )
        .bind(cutoff)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Clean up old execution checkpoints
    pub async fn cleanup_checkpoints(&self) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(self.config.checkpoints_retention_days);

        let result = sqlx::query(
            r"
            DELETE FROM execution_checkpoints
            WHERE created_at < $1
            ",
        )
        .bind(cutoff)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Clean up old schedule execution history
    pub async fn cleanup_schedule_history(&self) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(self.config.schedule_history_retention_days);

        let result = sqlx::query(
            r"
            DELETE FROM schedule_executions
            WHERE triggered_at < $1
            ",
        )
        .bind(cutoff)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Clean up expired secrets
    pub async fn cleanup_expired_secrets(&self) -> Result<u64> {
        let now = Utc::now();

        let result = sqlx::query(
            r"
            DELETE FROM secrets
            WHERE expires_at IS NOT NULL AND expires_at < $1
            ",
        )
        .bind(now)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Clean up expired API keys
    pub async fn cleanup_expired_api_keys(&self) -> Result<u64> {
        let now = Utc::now();

        let result = sqlx::query(
            r"
            DELETE FROM api_keys
            WHERE expires_at IS NOT NULL AND expires_at < $1
            ",
        )
        .bind(now)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Get storage usage statistics
    pub async fn get_storage_stats(&self) -> Result<StorageStats> {
        #[derive(sqlx::FromRow)]
        struct TableCount {
            count: i64,
        }

        let workflows = sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM workflows")
            .fetch_one(self.pool.pool())
            .await?;

        let executions =
            sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM executions")
                .fetch_one(self.pool.pool())
                .await?;

        let audit_logs =
            sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM audit_logs")
                .fetch_one(self.pool.pool())
                .await?;

        let api_keys = sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM api_keys")
            .fetch_one(self.pool.pool())
            .await?;

        let secrets = sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM secrets")
            .fetch_one(self.pool.pool())
            .await?;

        let webhooks = sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM webhooks")
            .fetch_one(self.pool.pool())
            .await?;

        let webhook_events =
            sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM webhook_events")
                .fetch_one(self.pool.pool())
                .await?;

        let checkpoints =
            sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM execution_checkpoints")
                .fetch_one(self.pool.pool())
                .await?;

        let schedules = sqlx::query_as::<_, TableCount>("SELECT COUNT(*) as count FROM schedules")
            .fetch_one(self.pool.pool())
            .await?;

        Ok(StorageStats {
            workflows_count: workflows.count as u64,
            executions_count: executions.count as u64,
            audit_logs_count: audit_logs.count as u64,
            api_keys_count: api_keys.count as u64,
            secrets_count: secrets.count as u64,
            webhooks_count: webhooks.count as u64,
            webhook_events_count: webhook_events.count as u64,
            checkpoints_count: checkpoints.count as u64,
            schedules_count: schedules.count as u64,
            timestamp: Utc::now(),
        })
    }
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub workflows_count: u64,
    pub executions_count: u64,
    pub audit_logs_count: u64,
    pub api_keys_count: u64,
    pub secrets_count: u64,
    pub webhooks_count: u64,
    pub webhook_events_count: u64,
    pub checkpoints_count: u64,
    pub schedules_count: u64,
    pub timestamp: DateTime<Utc>,
}

impl StorageStats {
    /// Total record count across all tables
    pub fn total_records(&self) -> u64 {
        self.workflows_count
            + self.executions_count
            + self.audit_logs_count
            + self.api_keys_count
            + self.secrets_count
            + self.webhooks_count
            + self.webhook_events_count
            + self.checkpoints_count
            + self.schedules_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_retention_config() {
        let config = RetentionConfig::default();
        assert_eq!(config.executions_retention_days, 90);
        assert_eq!(config.audit_logs_retention_days, 90);
        assert_eq!(config.metrics_retention_days, 365);
    }

    #[test]
    fn test_cleanup_results() {
        let mut results = CleanupResults::new();
        results.executions_deleted = 10;
        results.audit_logs_deleted = 20;
        results.metrics_deleted = 30;

        assert_eq!(results.total_deleted(), 60);
    }
}
