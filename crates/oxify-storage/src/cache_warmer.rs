//! Cache Warming Service
//!
//! Proactively populates caches on application startup to improve performance.
//!
//! ## Overview
//!
//! Cache warming solves the "cold start" problem where the first requests after
//! application startup experience high latency due to empty caches.
//!
//! ## Benefits
//!
//! - **Reduced latency**: First requests hit warm cache instead of database
//! - **Predictable performance**: Consistent response times from the start
//! - **Load reduction**: Fewer database queries during traffic spikes
//! - **Better UX**: Users don't experience slow initial page loads
//!
//! ## Strategies
//!
//! 1. **Most Popular**: Cache most frequently accessed items (workflows, users)
//! 2. **Recent**: Cache recently modified/accessed items
//! 3. **Critical Path**: Cache items required for critical user flows
//! 4. **Scheduled**: Periodically refresh caches to prevent expiration
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{CacheWarmer, CacheWarmerConfig, Cache};
//!
//! let cache = Cache::new(cache_config);
//! let config = CacheWarmerConfig {
//!     warm_popular_workflows: true,
//!     warm_recent_workflows: true,
//!     popular_limit: 100,
//!     recent_limit: 50,
//!     enable_background_refresh: true,
//!     refresh_interval: Duration::from_secs(300), // 5 minutes
//! };
//!
//! let warmer = CacheWarmer::new(pool, cache, config);
//!
//! // Warm caches on startup
//! warmer.warm_all().await?;
//!
//! // Start background refresh task
//! warmer.start_background_refresh().await;
//! ```

use crate::{Cache, DatabasePool, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Cache warming configuration
#[derive(Debug, Clone)]
pub struct CacheWarmerConfig {
    /// Warm most popular workflows (by execution count)
    pub warm_popular_workflows: bool,
    /// Warm recently modified workflows
    pub warm_recent_workflows: bool,
    /// Number of popular workflows to warm
    pub popular_limit: usize,
    /// Number of recent workflows to warm
    pub recent_limit: usize,
    /// Enable background cache refresh
    pub enable_background_refresh: bool,
    /// Interval for background refresh
    pub refresh_interval: Duration,
    /// Warm user quotas for active users
    pub warm_user_quotas: bool,
    /// Number of active users to warm quotas for
    pub active_users_limit: usize,
}

impl Default for CacheWarmerConfig {
    fn default() -> Self {
        Self {
            warm_popular_workflows: true,
            warm_recent_workflows: true,
            popular_limit: 100,
            recent_limit: 50,
            enable_background_refresh: true,
            refresh_interval: Duration::from_secs(300), // 5 minutes
            warm_user_quotas: true,
            active_users_limit: 100,
        }
    }
}

/// Cache warming statistics
#[derive(Debug, Clone, Default)]
pub struct CacheWarmingStats {
    /// Number of workflows warmed
    pub workflows_warmed: usize,
    /// Number of user quotas warmed
    pub user_quotas_warmed: usize,
    /// Number of workflow quotas warmed
    pub workflow_quotas_warmed: usize,
    /// Duration of warming operation (milliseconds)
    pub duration_ms: u64,
    /// Number of errors encountered
    pub errors: usize,
}

impl CacheWarmingStats {
    /// Get total items warmed
    pub fn total_items(&self) -> usize {
        self.workflows_warmed + self.user_quotas_warmed + self.workflow_quotas_warmed
    }

    /// Check if warming was successful
    pub fn is_success(&self) -> bool {
        self.errors == 0 && self.total_items() > 0
    }

    /// Get warming rate (items per second)
    pub fn warming_rate(&self) -> f64 {
        if self.duration_ms == 0 {
            return 0.0;
        }
        (self.total_items() as f64 / self.duration_ms as f64) * 1000.0
    }
}

/// Cache warming service
pub struct CacheWarmer {
    pool: DatabasePool,
    cache: Arc<Cache>,
    config: CacheWarmerConfig,
}

impl CacheWarmer {
    /// Create a new cache warmer
    pub fn new(pool: DatabasePool, cache: Cache, config: CacheWarmerConfig) -> Self {
        Self {
            pool,
            cache: Arc::new(cache),
            config,
        }
    }

    /// Warm all configured caches
    pub async fn warm_all(&self) -> Result<CacheWarmingStats> {
        let start = std::time::Instant::now();
        let mut stats = CacheWarmingStats::default();

        info!("Starting cache warming...");

        // Warm popular workflows
        if self.config.warm_popular_workflows {
            match self.warm_popular_workflows().await {
                Ok(count) => {
                    stats.workflows_warmed += count;
                    debug!("Warmed {} popular workflows", count);
                }
                Err(e) => {
                    error!("Failed to warm popular workflows: {}", e);
                    stats.errors += 1;
                }
            }
        }

        // Warm recent workflows
        if self.config.warm_recent_workflows {
            match self.warm_recent_workflows().await {
                Ok(count) => {
                    stats.workflows_warmed += count;
                    debug!("Warmed {} recent workflows", count);
                }
                Err(e) => {
                    error!("Failed to warm recent workflows: {}", e);
                    stats.errors += 1;
                }
            }
        }

        stats.duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "Cache warming completed: {} items in {}ms ({:.0} items/sec)",
            stats.total_items(),
            stats.duration_ms,
            stats.warming_rate()
        );

        Ok(stats)
    }

    /// Warm popular workflows (by execution count)
    async fn warm_popular_workflows(&self) -> Result<usize> {
        // Get popular workflows
        let workflows = sqlx::query_as::<_, crate::models::WorkflowRow>(
            "SELECT w.* FROM workflows w
             LEFT JOIN executions e ON w.id = e.workflow_id
             GROUP BY w.id
             ORDER BY COUNT(e.id) DESC
             LIMIT $1",
        )
        .bind(self.config.popular_limit as i64)
        .fetch_all(self.pool.pool())
        .await?;

        let count = workflows.len();

        // Populate cache
        for workflow in workflows {
            self.cache.put_workflow(workflow.id, workflow);
        }

        Ok(count)
    }

    /// Warm recently modified workflows
    async fn warm_recent_workflows(&self) -> Result<usize> {
        // Get recent workflows
        let workflows = sqlx::query_as::<_, crate::models::WorkflowRow>(
            "SELECT * FROM workflows
             ORDER BY updated_at DESC
             LIMIT $1",
        )
        .bind(self.config.recent_limit as i64)
        .fetch_all(self.pool.pool())
        .await?;

        let count = workflows.len();

        // Populate cache
        for workflow in workflows {
            self.cache.put_workflow(workflow.id, workflow);
        }

        Ok(count)
    }

    /// Start background cache refresh task
    ///
    /// This spawns a background task that periodically refreshes caches
    /// to prevent expiration of frequently-used items.
    pub async fn start_background_refresh(self: Arc<Self>) {
        if !self.config.enable_background_refresh {
            warn!("Background cache refresh is disabled");
            return;
        }

        info!(
            "Starting background cache refresh (interval: {}s)",
            self.config.refresh_interval.as_secs()
        );

        tokio::spawn(async move {
            let mut interval = interval(self.config.refresh_interval);

            loop {
                interval.tick().await;

                debug!("Running background cache refresh...");

                match self.warm_all().await {
                    Ok(stats) => {
                        debug!(
                            "Background refresh completed: {} items in {}ms",
                            stats.total_items(),
                            stats.duration_ms
                        );
                    }
                    Err(e) => {
                        error!("Background refresh failed: {}", e);
                    }
                }
            }
        });
    }

    /// Get cache warmer statistics
    pub fn config(&self) -> &CacheWarmerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CacheWarmerConfig::default();
        assert!(config.warm_popular_workflows);
        assert!(config.warm_recent_workflows);
        assert_eq!(config.popular_limit, 100);
        assert_eq!(config.recent_limit, 50);
        assert!(config.enable_background_refresh);
    }

    #[test]
    fn test_warming_stats() {
        let stats = CacheWarmingStats {
            workflows_warmed: 50,
            user_quotas_warmed: 30,
            workflow_quotas_warmed: 20,
            duration_ms: 1000,
            errors: 0,
        };

        assert_eq!(stats.total_items(), 100);
        assert!(stats.is_success());
        assert_eq!(stats.warming_rate(), 100.0); // 100 items / 1 second
    }

    #[test]
    fn test_warming_stats_with_errors() {
        let stats = CacheWarmingStats {
            workflows_warmed: 50,
            user_quotas_warmed: 30,
            workflow_quotas_warmed: 20,
            duration_ms: 1000,
            errors: 1,
        };

        assert!(!stats.is_success());
    }

    #[test]
    fn test_warming_rate_zero_duration() {
        let stats = CacheWarmingStats {
            workflows_warmed: 100,
            user_quotas_warmed: 0,
            workflow_quotas_warmed: 0,
            duration_ms: 0,
            errors: 0,
        };

        assert_eq!(stats.warming_rate(), 0.0);
    }
}
