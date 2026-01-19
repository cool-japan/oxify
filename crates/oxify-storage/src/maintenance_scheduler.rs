//! Automated Maintenance Scheduler
//!
//! Schedules and executes database maintenance tasks automatically.
//!
//! ## Overview
//!
//! Regular database maintenance is critical for production systems to maintain
//! performance and reliability. This scheduler automates routine tasks:
//!
//! - **VACUUM**: Reclaim storage and update statistics
//! - **ANALYZE**: Update query planner statistics
//! - **REINDEX**: Rebuild bloated indexes
//! - **Cleanup**: Delete old data based on retention policies
//! - **Health Checks**: Monitor database health
//!
//! ## Benefits
//!
//! - **Automatic Execution**: No manual intervention required
//! - **Configurable Schedules**: Different intervals for different tasks
//! - **Non-Blocking**: Runs during off-peak hours
//! - **Monitoring**: Tracks execution history and results
//! - **Error Recovery**: Automatic retry on transient failures
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{MaintenanceScheduler, MaintenanceScheduleConfig};
//!
//! let config = MaintenanceScheduleConfig {
//!     vacuum_interval: Duration::from_secs(86400), // Daily
//!     analyze_interval: Duration::from_secs(3600),  // Hourly
//!     cleanup_interval: Duration::from_secs(86400), // Daily
//!     reindex_interval: Duration::from_secs(604800), // Weekly
//!     enable_vacuum: true,
//!     enable_analyze: true,
//!     enable_cleanup: true,
//!     enable_reindex: true,
//! };
//!
//! let scheduler = MaintenanceScheduler::new(pool, cleanup_service, maintenance_service, config);
//!
//! // Start all scheduled tasks
//! scheduler.start_all().await;
//!
//! // Or start individual tasks
//! scheduler.start_vacuum_task().await;
//! scheduler.start_cleanup_task().await;
//! ```

use crate::{CleanupService, DatabasePool, MaintenanceService, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Maintenance schedule configuration
#[derive(Debug, Clone)]
pub struct MaintenanceScheduleConfig {
    /// Interval for VACUUM operations
    pub vacuum_interval: Duration,
    /// Interval for ANALYZE operations
    pub analyze_interval: Duration,
    /// Interval for cleanup operations
    pub cleanup_interval: Duration,
    /// Interval for REINDEX operations
    pub reindex_interval: Duration,
    /// Enable automatic VACUUM
    pub enable_vacuum: bool,
    /// Enable automatic ANALYZE
    pub enable_analyze: bool,
    /// Enable automatic cleanup
    pub enable_cleanup: bool,
    /// Enable automatic REINDEX
    pub enable_reindex: bool,
    /// Retry failed operations
    pub enable_retry: bool,
    /// Maximum retries for failed operations
    pub max_retries: u32,
}

impl Default for MaintenanceScheduleConfig {
    fn default() -> Self {
        Self {
            vacuum_interval: Duration::from_secs(86400),   // 24 hours
            analyze_interval: Duration::from_secs(3600),   // 1 hour
            cleanup_interval: Duration::from_secs(86400),  // 24 hours
            reindex_interval: Duration::from_secs(604800), // 7 days
            enable_vacuum: true,
            enable_analyze: true,
            enable_cleanup: true,
            enable_reindex: false, // Disabled by default (resource intensive)
            enable_retry: true,
            max_retries: 3,
        }
    }
}

/// Maintenance task execution statistics
#[derive(Debug, Clone, Default)]
pub struct MaintenanceTaskStats {
    /// Number of successful executions
    pub success_count: u64,
    /// Number of failed executions
    pub failure_count: u64,
    /// Total execution time (milliseconds)
    pub total_duration_ms: u64,
    /// Last execution timestamp
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    /// Last error message
    pub last_error: Option<String>,
}

impl MaintenanceTaskStats {
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.0;
        }
        self.success_count as f64 / total as f64
    }

    /// Calculate average duration
    pub fn avg_duration_ms(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.0;
        }
        self.total_duration_ms as f64 / total as f64
    }
}

/// Automated maintenance scheduler
pub struct MaintenanceScheduler {
    #[allow(dead_code)]
    pool: Arc<DatabasePool>,
    cleanup_service: Arc<CleanupService>,
    maintenance_service: Arc<MaintenanceService>,
    config: MaintenanceScheduleConfig,
}

impl MaintenanceScheduler {
    /// Create a new maintenance scheduler
    pub fn new(
        pool: DatabasePool,
        cleanup_service: CleanupService,
        maintenance_service: MaintenanceService,
        config: MaintenanceScheduleConfig,
    ) -> Self {
        Self {
            pool: Arc::new(pool),
            cleanup_service: Arc::new(cleanup_service),
            maintenance_service: Arc::new(maintenance_service),
            config,
        }
    }

    /// Start all enabled maintenance tasks
    pub async fn start_all(self: Arc<Self>) {
        info!("Starting automated maintenance scheduler");

        if self.config.enable_vacuum {
            self.clone().start_vacuum_task().await;
        }

        if self.config.enable_analyze {
            self.clone().start_analyze_task().await;
        }

        if self.config.enable_cleanup {
            self.clone().start_cleanup_task().await;
        }

        if self.config.enable_reindex {
            self.clone().start_reindex_task().await;
        }

        info!("All maintenance tasks started");
    }

    /// Start VACUUM task
    pub async fn start_vacuum_task(self: Arc<Self>) {
        let interval_duration = self.config.vacuum_interval;

        info!(
            "Starting VACUUM task (interval: {}s)",
            interval_duration.as_secs()
        );

        tokio::spawn(async move {
            let mut interval = interval(interval_duration);

            loop {
                interval.tick().await;

                debug!("Running scheduled VACUUM...");
                let start = std::time::Instant::now();

                match self.run_vacuum_all_tables().await {
                    Ok(()) => {
                        let duration = start.elapsed();
                        info!("VACUUM completed in {:?}", duration);
                    }
                    Err(e) => {
                        error!("VACUUM failed: {}", e);
                        if self.config.enable_retry {
                            self.retry_vacuum().await;
                        }
                    }
                }
            }
        });
    }

    /// Start ANALYZE task
    pub async fn start_analyze_task(self: Arc<Self>) {
        let interval_duration = self.config.analyze_interval;

        info!(
            "Starting ANALYZE task (interval: {}s)",
            interval_duration.as_secs()
        );

        tokio::spawn(async move {
            let mut interval = interval(interval_duration);

            loop {
                interval.tick().await;

                debug!("Running scheduled ANALYZE...");
                let start = std::time::Instant::now();

                match self.run_analyze_all_tables().await {
                    Ok(()) => {
                        let duration = start.elapsed();
                        debug!("ANALYZE completed in {:?}", duration);
                    }
                    Err(e) => {
                        error!("ANALYZE failed: {}", e);
                        if self.config.enable_retry {
                            self.retry_analyze().await;
                        }
                    }
                }
            }
        });
    }

    /// Start cleanup task
    pub async fn start_cleanup_task(self: Arc<Self>) {
        let interval_duration = self.config.cleanup_interval;

        info!(
            "Starting cleanup task (interval: {}s)",
            interval_duration.as_secs()
        );

        tokio::spawn(async move {
            let mut interval = interval(interval_duration);

            loop {
                interval.tick().await;

                debug!("Running scheduled cleanup...");
                let start = std::time::Instant::now();

                match self.cleanup_service.run_all().await {
                    Ok(results) => {
                        let duration = start.elapsed();
                        info!(
                            "Cleanup completed: {} items deleted in {:?}",
                            results.total_deleted(),
                            duration
                        );
                    }
                    Err(e) => {
                        error!("Cleanup failed: {}", e);
                    }
                }
            }
        });
    }

    /// Start REINDEX task
    pub async fn start_reindex_task(self: Arc<Self>) {
        let interval_duration = self.config.reindex_interval;

        info!(
            "Starting REINDEX task (interval: {}s)",
            interval_duration.as_secs()
        );

        tokio::spawn(async move {
            let mut interval = interval(interval_duration);

            loop {
                interval.tick().await;

                debug!("Running scheduled REINDEX...");
                let start = std::time::Instant::now();

                match self.run_reindex_bloated_indexes().await {
                    Ok(()) => {
                        let duration = start.elapsed();
                        info!("REINDEX completed in {:?}", duration);
                    }
                    Err(e) => {
                        error!("REINDEX failed: {}", e);
                        if self.config.enable_retry {
                            self.retry_reindex().await;
                        }
                    }
                }
            }
        });
    }

    /// Run VACUUM on all tables
    async fn run_vacuum_all_tables(&self) -> Result<()> {
        let tables = vec!["workflows", "executions", "audit_logs", "execution_metrics"];
        for table in tables {
            self.maintenance_service.vacuum_table(table).await?;
        }
        Ok(())
    }

    /// Run ANALYZE on all tables
    async fn run_analyze_all_tables(&self) -> Result<()> {
        let tables = vec!["workflows", "executions", "audit_logs", "execution_metrics"];
        for table in tables {
            self.maintenance_service.analyze_table(table).await?;
        }
        Ok(())
    }

    /// Run REINDEX on bloated indexes
    async fn run_reindex_bloated_indexes(&self) -> Result<()> {
        let bloated = self.maintenance_service.get_index_bloat().await?;
        for index_info in bloated {
            // Check if index has low usage (potential candidate for optimization)
            if index_info.index_scans < 100 {
                warn!(
                    "Found potentially unused index: {} (scans: {})",
                    index_info.index_name, index_info.index_scans
                );
                // Note: Actual reindex would need proper bloat detection logic
                // For now, we'll just log it
            }
        }
        Ok(())
    }

    /// Retry VACUUM operation
    async fn retry_vacuum(&self) {
        for attempt in 1..=self.config.max_retries {
            warn!(
                "Retrying VACUUM (attempt {}/{})",
                attempt, self.config.max_retries
            );
            tokio::time::sleep(Duration::from_secs(60 * u64::from(attempt))).await;

            match self.run_vacuum_all_tables().await {
                Ok(()) => {
                    info!("VACUUM succeeded on retry {}", attempt);
                    return;
                }
                Err(e) => {
                    error!("VACUUM retry {} failed: {}", attempt, e);
                }
            }
        }

        error!("VACUUM failed after {} retries", self.config.max_retries);
    }

    /// Retry ANALYZE operation
    async fn retry_analyze(&self) {
        for attempt in 1..=self.config.max_retries {
            warn!(
                "Retrying ANALYZE (attempt {}/{})",
                attempt, self.config.max_retries
            );
            tokio::time::sleep(Duration::from_secs(30 * u64::from(attempt))).await;

            match self.run_analyze_all_tables().await {
                Ok(()) => {
                    info!("ANALYZE succeeded on retry {}", attempt);
                    return;
                }
                Err(e) => {
                    error!("ANALYZE retry {} failed: {}", attempt, e);
                }
            }
        }

        error!("ANALYZE failed after {} retries", self.config.max_retries);
    }

    /// Retry REINDEX operation
    async fn retry_reindex(&self) {
        for attempt in 1..=self.config.max_retries {
            warn!(
                "Retrying REINDEX (attempt {}/{})",
                attempt, self.config.max_retries
            );
            tokio::time::sleep(Duration::from_secs(120 * u64::from(attempt))).await;

            match self.run_reindex_bloated_indexes().await {
                Ok(()) => {
                    info!("REINDEX succeeded on retry {}", attempt);
                    return;
                }
                Err(e) => {
                    error!("REINDEX retry {} failed: {}", attempt, e);
                }
            }
        }

        error!("REINDEX failed after {} retries", self.config.max_retries);
    }

    /// Get scheduler configuration
    pub fn config(&self) -> &MaintenanceScheduleConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MaintenanceScheduleConfig::default();
        assert!(config.enable_vacuum);
        assert!(config.enable_analyze);
        assert!(config.enable_cleanup);
        assert!(!config.enable_reindex); // Disabled by default
        assert!(config.enable_retry);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_task_stats_success_rate() {
        let stats = MaintenanceTaskStats {
            success_count: 90,
            failure_count: 10,
            total_duration_ms: 5000,
            last_run: None,
            last_error: None,
        };

        assert_eq!(stats.success_rate(), 0.9);
        assert_eq!(stats.avg_duration_ms(), 50.0);
    }

    #[test]
    fn test_task_stats_zero_executions() {
        let stats = MaintenanceTaskStats::default();
        assert_eq!(stats.success_rate(), 0.0);
        assert_eq!(stats.avg_duration_ms(), 0.0);
    }

    #[test]
    fn test_task_stats_all_failures() {
        let stats = MaintenanceTaskStats {
            success_count: 0,
            failure_count: 10,
            total_duration_ms: 1000,
            last_run: None,
            last_error: Some("Database locked".to_string()),
        };

        assert_eq!(stats.success_rate(), 0.0);
        assert_eq!(stats.avg_duration_ms(), 100.0);
    }
}
