//! Performance Metrics Tracking
//!
//! Track and export performance metrics for the authorization engine.
//!
//! ## Features
//!
//! - Request latency tracking (p50, p95, p99)
//! - Cache hit rate monitoring
//! - Throughput measurement
//! - Error rate tracking
//! - Export metrics in JSON format for CI integration
//!
//! ## Example
//!
//! ```rust
//! use oxify_authz::metrics::{PerformanceMetrics, MetricsSnapshot};
//!
//! let metrics = PerformanceMetrics::new();
//!
//! // Record a request
//! metrics.record_check_latency(125); // 125 microseconds
//! metrics.record_cache_hit(true);
//!
//! // Get snapshot
//! let snapshot = metrics.snapshot();
//! println!("Cache hit rate: {:.2}%", snapshot.cache_hit_rate() * 100.0);
//! ```

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Performance metrics for the authorization engine
#[derive(Clone)]
pub struct PerformanceMetrics {
    inner: Arc<MetricsInner>,
}

struct MetricsInner {
    // Request counts
    total_checks: AtomicU64,
    successful_checks: AtomicU64,
    failed_checks: AtomicU64,

    // Cache metrics
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,

    // Latency tracking (in microseconds)
    total_latency_us: AtomicU64,
    min_latency_us: AtomicU64,
    max_latency_us: AtomicU64,

    // Delegation metrics
    delegation_checks: AtomicU64,
    delegation_grants: AtomicU64,

    // Tenant metrics
    tenant_quota_exceeded: AtomicU64,

    // Start time for throughput calculation
    start_time: Instant,
}

impl PerformanceMetrics {
    /// Create a new performance metrics tracker
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                total_checks: AtomicU64::new(0),
                successful_checks: AtomicU64::new(0),
                failed_checks: AtomicU64::new(0),
                cache_hits: AtomicU64::new(0),
                cache_misses: AtomicU64::new(0),
                total_latency_us: AtomicU64::new(0),
                min_latency_us: AtomicU64::new(u64::MAX),
                max_latency_us: AtomicU64::new(0),
                delegation_checks: AtomicU64::new(0),
                delegation_grants: AtomicU64::new(0),
                tenant_quota_exceeded: AtomicU64::new(0),
                start_time: Instant::now(),
            }),
        }
    }

    /// Record a permission check latency (in microseconds)
    pub fn record_check_latency(&self, latency_us: u64) {
        self.inner.total_checks.fetch_add(1, Ordering::Relaxed);
        self.inner
            .total_latency_us
            .fetch_add(latency_us, Ordering::Relaxed);

        // Update min latency
        let mut current_min = self.inner.min_latency_us.load(Ordering::Relaxed);
        while latency_us < current_min {
            match self.inner.min_latency_us.compare_exchange_weak(
                current_min,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_min = x,
            }
        }

        // Update max latency
        let mut current_max = self.inner.max_latency_us.load(Ordering::Relaxed);
        while latency_us > current_max {
            match self.inner.max_latency_us.compare_exchange_weak(
                current_max,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }
    }

    /// Record a successful permission check
    pub fn record_check_success(&self) {
        self.inner.successful_checks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed permission check
    pub fn record_check_failure(&self) {
        self.inner.failed_checks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache hit or miss
    pub fn record_cache_hit(&self, hit: bool) {
        if hit {
            self.inner.cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.inner.cache_misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a delegation check
    pub fn record_delegation_check(&self, granted: bool) {
        self.inner.delegation_checks.fetch_add(1, Ordering::Relaxed);
        if granted {
            self.inner.delegation_grants.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a tenant quota exceeded event
    pub fn record_quota_exceeded(&self) {
        self.inner
            .tenant_quota_exceeded
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of current metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        let total_checks = self.inner.total_checks.load(Ordering::Relaxed);
        let total_latency = self.inner.total_latency_us.load(Ordering::Relaxed);
        let cache_hits = self.inner.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.inner.cache_misses.load(Ordering::Relaxed);

        MetricsSnapshot {
            total_checks,
            successful_checks: self.inner.successful_checks.load(Ordering::Relaxed),
            failed_checks: self.inner.failed_checks.load(Ordering::Relaxed),
            cache_hits,
            cache_misses,
            avg_latency_us: if total_checks > 0 {
                total_latency / total_checks
            } else {
                0
            },
            min_latency_us: self.inner.min_latency_us.load(Ordering::Relaxed),
            max_latency_us: self.inner.max_latency_us.load(Ordering::Relaxed),
            delegation_checks: self.inner.delegation_checks.load(Ordering::Relaxed),
            delegation_grants: self.inner.delegation_grants.load(Ordering::Relaxed),
            tenant_quota_exceeded: self.inner.tenant_quota_exceeded.load(Ordering::Relaxed),
            uptime_seconds: self.inner.start_time.elapsed().as_secs(),
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.inner.total_checks.store(0, Ordering::Relaxed);
        self.inner.successful_checks.store(0, Ordering::Relaxed);
        self.inner.failed_checks.store(0, Ordering::Relaxed);
        self.inner.cache_hits.store(0, Ordering::Relaxed);
        self.inner.cache_misses.store(0, Ordering::Relaxed);
        self.inner.total_latency_us.store(0, Ordering::Relaxed);
        self.inner.min_latency_us.store(u64::MAX, Ordering::Relaxed);
        self.inner.max_latency_us.store(0, Ordering::Relaxed);
        self.inner.delegation_checks.store(0, Ordering::Relaxed);
        self.inner.delegation_grants.store(0, Ordering::Relaxed);
        self.inner.tenant_quota_exceeded.store(0, Ordering::Relaxed);
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of performance metrics at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub total_checks: u64,
    pub successful_checks: u64,
    pub failed_checks: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub avg_latency_us: u64,
    pub min_latency_us: u64,
    pub max_latency_us: u64,
    pub delegation_checks: u64,
    pub delegation_grants: u64,
    pub tenant_quota_exceeded: u64,
    pub uptime_seconds: u64,
}

impl MetricsSnapshot {
    /// Calculate cache hit rate (0.0 to 1.0)
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    /// Calculate success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_checks == 0 {
            0.0
        } else {
            self.successful_checks as f64 / self.total_checks as f64
        }
    }

    /// Calculate average latency in milliseconds
    pub fn avg_latency_ms(&self) -> f64 {
        self.avg_latency_us as f64 / 1000.0
    }

    /// Calculate throughput (requests per second)
    pub fn throughput_rps(&self) -> f64 {
        if self.uptime_seconds == 0 {
            0.0
        } else {
            self.total_checks as f64 / self.uptime_seconds as f64
        }
    }

    /// Calculate delegation grant rate (0.0 to 1.0)
    pub fn delegation_grant_rate(&self) -> f64 {
        if self.delegation_checks == 0 {
            0.0
        } else {
            self.delegation_grants as f64 / self.delegation_checks as f64
        }
    }

    /// Export metrics as JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Check if metrics meet performance targets
    pub fn meets_targets(&self) -> PerformanceTargets {
        PerformanceTargets {
            avg_latency_target_met: self.avg_latency_us < 100, // <100μs for cached
            cache_hit_rate_target_met: self.cache_hit_rate() > 0.95, // >95% hit rate
            success_rate_target_met: self.success_rate() > 0.99, // >99% success
        }
    }
}

/// Performance targets evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTargets {
    pub avg_latency_target_met: bool,
    pub cache_hit_rate_target_met: bool,
    pub success_rate_target_met: bool,
}

impl PerformanceTargets {
    /// Check if all targets are met
    pub fn all_met(&self) -> bool {
        self.avg_latency_target_met
            && self.cache_hit_rate_target_met
            && self.success_rate_target_met
    }
}

/// Timer for measuring operation duration
pub struct MetricsTimer {
    start: Instant,
    metrics: PerformanceMetrics,
}

impl MetricsTimer {
    /// Start a new timer
    pub fn start(metrics: PerformanceMetrics) -> Self {
        Self {
            start: Instant::now(),
            metrics,
        }
    }

    /// Stop the timer and record the latency
    pub fn stop(self) {
        let elapsed_us = self.start.elapsed().as_micros() as u64;
        self.metrics.record_check_latency(elapsed_us);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_basic() {
        let metrics = PerformanceMetrics::new();

        metrics.record_check_latency(100);
        metrics.record_check_latency(200);
        metrics.record_check_latency(150);

        metrics.record_cache_hit(true);
        metrics.record_cache_hit(true);
        metrics.record_cache_hit(false);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_checks, 3);
        assert_eq!(snapshot.avg_latency_us, 150); // (100+200+150)/3
        assert_eq!(snapshot.min_latency_us, 100);
        assert_eq!(snapshot.max_latency_us, 200);
        assert_eq!(snapshot.cache_hits, 2);
        assert_eq!(snapshot.cache_misses, 1);
    }

    #[test]
    fn test_cache_hit_rate() {
        let metrics = PerformanceMetrics::new();

        for _ in 0..95 {
            metrics.record_cache_hit(true);
        }
        for _ in 0..5 {
            metrics.record_cache_hit(false);
        }

        let snapshot = metrics.snapshot();
        assert!((snapshot.cache_hit_rate() - 0.95).abs() < 0.01);
    }

    #[test]
    fn test_delegation_metrics() {
        let metrics = PerformanceMetrics::new();

        metrics.record_delegation_check(true);
        metrics.record_delegation_check(true);
        metrics.record_delegation_check(false);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.delegation_checks, 3);
        assert_eq!(snapshot.delegation_grants, 2);
        assert!((snapshot.delegation_grant_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = PerformanceMetrics::new();

        metrics.record_check_latency(100);
        metrics.record_cache_hit(true);

        assert_eq!(metrics.snapshot().total_checks, 1);

        metrics.reset();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_checks, 0);
        assert_eq!(snapshot.cache_hits, 0);
    }

    #[test]
    fn test_json_export() {
        let snapshot = MetricsSnapshot {
            total_checks: 1000,
            successful_checks: 995,
            failed_checks: 5,
            cache_hits: 950,
            cache_misses: 50,
            avg_latency_us: 75,
            min_latency_us: 10,
            max_latency_us: 500,
            delegation_checks: 100,
            delegation_grants: 80,
            tenant_quota_exceeded: 2,
            uptime_seconds: 3600,
        };

        let json = snapshot.to_json().unwrap();
        assert!(json.contains("total_checks"));
        assert!(json.contains("1000"));
    }
}
