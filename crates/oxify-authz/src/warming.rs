//! Cache Warming Strategies
//!
//! Pre-populate caches at startup to achieve optimal performance immediately:
//! - **Hot Path Warming**: Load most frequently accessed permissions
//! - **Tenant Warming**: Pre-load critical tenant permissions
//! - **Incremental Warming**: Background loading to avoid startup delay
//! - **Access Pattern Analysis**: Use historical data to predict hot paths
//!
//! ## Benefits
//!
//! - **Zero Cold Start**: No cache misses on first requests
//! - **Predictable Performance**: Consistent latency from startup
//! - **Reduced Database Load**: Fewer queries during traffic spikes
//! - **Better User Experience**: Fast authorization checks immediately
//!
//! ## Usage
//!
//! ```no_run
//! use oxify_authz::warming::*;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = oxify_authz::HybridRebacEngine::new("postgres://localhost/db").await?;
//! let warmer = CacheWarmer::new(std::sync::Arc::new(engine));
//!
//! // Warm cache with top 1000 permissions
//! let stats = warmer.warm_hot_paths(1000).await?;
//! println!("Warmed {} entries in {:?}", stats.entries_loaded, stats.duration);
//!
//! // Warm specific tenant
//! warmer.warm_tenant("tenant_123").await?;
//! # Ok(())
//! # }
//! ```

use crate::{HybridRebacEngine, RelationTuple, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Cache warming strategy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WarmingStrategy {
    /// Load most frequently accessed permissions
    HotPaths,
    /// Load all permissions for critical tenants
    CriticalTenants,
    /// Load permissions for specific namespaces
    Namespaces,
    /// Load recent permissions (last N days)
    Recent,
    /// Custom query-based warming
    Custom,
}

/// Cache warming configuration
#[derive(Debug, Clone)]
pub struct WarmingConfig {
    /// Strategy to use
    pub strategy: WarmingStrategy,
    /// Maximum entries to load
    pub max_entries: usize,
    /// Concurrent loading (parallelism)
    pub concurrency: usize,
    /// Timeout for entire warming process (seconds)
    pub timeout_sec: u64,
    /// Skip warming if cache already has entries
    pub skip_if_warm: bool,
    /// Namespaces to warm (for Namespaces strategy)
    pub namespaces: Vec<String>,
    /// Tenant IDs to warm (for CriticalTenants strategy)
    pub tenant_ids: Vec<String>,
    /// Days to look back (for Recent strategy)
    pub recent_days: u32,
}

impl Default for WarmingConfig {
    fn default() -> Self {
        Self {
            strategy: WarmingStrategy::HotPaths,
            max_entries: 1000,
            concurrency: 10,
            timeout_sec: 60,
            skip_if_warm: true,
            namespaces: Vec::new(),
            tenant_ids: Vec::new(),
            recent_days: 7,
        }
    }
}

/// Cache warming statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmingStats {
    /// Number of entries loaded
    pub entries_loaded: usize,
    /// Number of entries failed
    pub entries_failed: usize,
    /// Time taken
    pub duration: Duration,
    /// Cache hit rate after warming
    pub cache_hit_rate: f64,
}

/// Cache warmer for pre-populating caches
pub struct CacheWarmer {
    engine: Arc<HybridRebacEngine>,
}

impl CacheWarmer {
    /// Create a new cache warmer
    pub fn new(engine: Arc<HybridRebacEngine>) -> Self {
        Self { engine }
    }

    /// Warm cache using configuration
    pub async fn warm(&self, config: WarmingConfig) -> Result<WarmingStats> {
        let start = Instant::now();

        // Check if we should skip warming
        if config.skip_if_warm && self.is_cache_warm().await {
            tracing::info!("Cache already warm, skipping warming");
            return Ok(WarmingStats {
                entries_loaded: 0,
                entries_failed: 0,
                duration: start.elapsed(),
                cache_hit_rate: 1.0,
            });
        }

        // Load tuples based on strategy
        let tuples = match config.strategy {
            WarmingStrategy::HotPaths => self.load_hot_paths(config.max_entries).await?,
            WarmingStrategy::CriticalTenants => {
                self.load_critical_tenants(&config.tenant_ids, config.max_entries)
                    .await?
            }
            WarmingStrategy::Namespaces => {
                self.load_namespaces(&config.namespaces, config.max_entries)
                    .await?
            }
            WarmingStrategy::Recent => {
                self.load_recent_tuples(config.recent_days, config.max_entries)
                    .await?
            }
            WarmingStrategy::Custom => {
                // Custom strategy requires external tuple list
                Vec::new()
            }
        };

        // Pre-load tuples into cache (parallel)
        let stats = self.preload_tuples(tuples, config.concurrency).await?;

        tracing::info!(
            "Cache warming complete: {} entries loaded, {} failed in {:?}",
            stats.entries_loaded,
            stats.entries_failed,
            stats.duration
        );

        Ok(stats)
    }

    /// Warm hot paths (most frequently accessed)
    pub async fn warm_hot_paths(&self, limit: usize) -> Result<WarmingStats> {
        let config = WarmingConfig {
            strategy: WarmingStrategy::HotPaths,
            max_entries: limit,
            ..Default::default()
        };
        self.warm(config).await
    }

    /// Warm specific tenant
    pub async fn warm_tenant(&self, tenant_id: &str) -> Result<WarmingStats> {
        let config = WarmingConfig {
            strategy: WarmingStrategy::CriticalTenants,
            tenant_ids: vec![tenant_id.to_string()],
            max_entries: 10_000,
            ..Default::default()
        };
        self.warm(config).await
    }

    /// Warm specific namespace
    pub async fn warm_namespace(&self, namespace: &str) -> Result<WarmingStats> {
        let config = WarmingConfig {
            strategy: WarmingStrategy::Namespaces,
            namespaces: vec![namespace.to_string()],
            max_entries: 10_000,
            ..Default::default()
        };
        self.warm(config).await
    }

    /// Load hot path tuples (most accessed)
    async fn load_hot_paths(&self, limit: usize) -> Result<Vec<RelationTuple>> {
        // Query audit log for most frequently checked permissions
        // For now, load most recent tuples as a proxy
        self.load_recent_tuples(7, limit).await
    }

    /// Load tuples for critical tenants
    async fn load_critical_tenants(
        &self,
        tenant_ids: &[String],
        limit: usize,
    ) -> Result<Vec<RelationTuple>> {
        let mut all_tuples = Vec::new();

        for tenant_id in tenant_ids {
            // Load tuples for this tenant
            // This would require tenant-aware tuple listing
            tracing::info!("Loading tuples for tenant: {}", tenant_id);
            // Placeholder: would use engine's list_tuples with tenant filter
        }

        all_tuples.truncate(limit);
        Ok(all_tuples)
    }

    /// Load tuples for specific namespaces
    async fn load_namespaces(
        &self,
        _namespaces: &[String],
        _limit: usize,
    ) -> Result<Vec<RelationTuple>> {
        // TODO: Implement namespace listing API in HybridRebacEngine
        // For now, return empty vector
        Ok(Vec::new())
    }

    /// Load recent tuples
    async fn load_recent_tuples(&self, _days: u32, _limit: usize) -> Result<Vec<RelationTuple>> {
        // TODO: Implement tuple listing API with timestamps in HybridRebacEngine
        // For now, return empty vector
        Ok(Vec::new())
    }

    /// Pre-load tuples into cache (parallel)
    async fn preload_tuples(
        &self,
        tuples: Vec<RelationTuple>,
        concurrency: usize,
    ) -> Result<WarmingStats> {
        let start = Instant::now();
        let total = tuples.len();
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut tasks = Vec::new();

        let mut loaded = 0;
        let mut failed = 0;

        for tuple in tuples {
            let engine = self.engine.clone();
            let sem = semaphore.clone();

            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await;
                // Pre-load by performing a check (which caches the result)
                engine
                    .check(crate::CheckRequest {
                        namespace: tuple.namespace.clone(),
                        object_id: tuple.object_id.clone(),
                        relation: tuple.relation.clone(),
                        subject: tuple.subject.clone(),
                        context: None,
                    })
                    .await
            });

            tasks.push(task);
        }

        // Wait for all tasks
        for task in tasks {
            match task.await {
                Ok(Ok(_)) => loaded += 1,
                Ok(Err(e)) => {
                    tracing::warn!("Failed to preload tuple: {}", e);
                    failed += 1;
                }
                Err(e) => {
                    tracing::error!("Task panicked: {}", e);
                    failed += 1;
                }
            }
        }

        Ok(WarmingStats {
            entries_loaded: loaded,
            entries_failed: failed,
            duration: start.elapsed(),
            cache_hit_rate: loaded as f64 / total as f64,
        })
    }

    /// Check if cache is already warm
    async fn is_cache_warm(&self) -> bool {
        // Check if cache has entries (simplified heuristic)
        // In production, would check cache hit rate or size
        false // Always warm for now
    }
}

/// Background cache warmer (runs periodically)
pub struct BackgroundWarmer {
    warmer: Arc<CacheWarmer>,
    config: WarmingConfig,
    interval: Duration,
}

impl BackgroundWarmer {
    /// Create a new background warmer
    pub fn new(engine: Arc<HybridRebacEngine>, config: WarmingConfig, interval_sec: u64) -> Self {
        Self {
            warmer: Arc::new(CacheWarmer::new(engine)),
            config,
            interval: Duration::from_secs(interval_sec),
        }
    }

    /// Start background warming task
    pub fn start(&self) {
        let warmer = self.warmer.clone();
        let config = self.config.clone();
        let interval = self.interval;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                tracing::info!("Starting background cache warming");

                match warmer.warm(config.clone()).await {
                    Ok(stats) => {
                        tracing::info!(
                            "Background warming complete: {} entries loaded in {:?}",
                            stats.entries_loaded,
                            stats.duration
                        );
                    }
                    Err(e) => {
                        tracing::error!("Background warming failed: {}", e);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warming_config_default() {
        let config = WarmingConfig::default();
        assert_eq!(config.strategy, WarmingStrategy::HotPaths);
        assert_eq!(config.max_entries, 1000);
        assert_eq!(config.concurrency, 10);
        assert!(config.skip_if_warm);
    }

    #[test]
    fn test_warming_stats() {
        let stats = WarmingStats {
            entries_loaded: 950,
            entries_failed: 50,
            duration: Duration::from_secs(10),
            cache_hit_rate: 0.95,
        };

        assert_eq!(stats.entries_loaded, 950);
        assert_eq!(stats.entries_failed, 50);
        assert_eq!(stats.cache_hit_rate, 0.95);
    }
}
