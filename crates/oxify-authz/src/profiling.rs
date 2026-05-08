//! Performance profiling utilities for authorization operations
//!
//! This module provides tools to measure and analyze the performance
//! of authorization checks, helping identify bottlenecks and optimization opportunities.
//!
//! # Example
//!
//! ```no_run
//! use oxify_authz::profiling::*;
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let profiler = AuthzProfiler::new();
//!
//! // Profile a check operation
//! let result = profiler.profile_async("check_document_viewer", async {
//!     // Your authorization check here
//!     Ok::<bool, Box<dyn std::error::Error>>(true)
//! }).await?;
//!
//! // Get profiling statistics
//! let stats = profiler.get_stats();
//! println!("Total operations: {}", stats.total_operations);
//! println!("Average latency: {:?}", stats.avg_latency());
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Performance profiler for authorization operations
#[derive(Clone)]
pub struct AuthzProfiler {
    metrics: Arc<Mutex<HashMap<String, OperationMetrics>>>,
}

/// Metrics for a specific operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationMetrics {
    /// Operation name
    pub name: String,
    /// Number of times this operation was called
    pub call_count: u64,
    /// Total time spent in this operation
    pub total_duration_ns: u64,
    /// Minimum observed latency
    pub min_latency_ns: u64,
    /// Maximum observed latency
    pub max_latency_ns: u64,
    /// P50 latency approximation (median)
    pub p50_latency_ns: u64,
    /// P95 latency approximation
    pub p95_latency_ns: u64,
    /// P99 latency approximation
    pub p99_latency_ns: u64,
    /// Recent latencies for percentile calculation (last 1000)
    #[serde(skip)]
    pub recent_latencies: Vec<u64>,
}

impl OperationMetrics {
    fn new(name: String) -> Self {
        Self {
            name,
            call_count: 0,
            total_duration_ns: 0,
            min_latency_ns: u64::MAX,
            max_latency_ns: 0,
            p50_latency_ns: 0,
            p95_latency_ns: 0,
            p99_latency_ns: 0,
            recent_latencies: Vec::with_capacity(1000),
        }
    }

    /// Record a new measurement
    fn record(&mut self, duration_ns: u64) {
        self.call_count += 1;
        self.total_duration_ns += duration_ns;
        self.min_latency_ns = self.min_latency_ns.min(duration_ns);
        self.max_latency_ns = self.max_latency_ns.max(duration_ns);

        // Keep last 1000 measurements for percentile calculation
        if self.recent_latencies.len() >= 1000 {
            self.recent_latencies.remove(0);
        }
        self.recent_latencies.push(duration_ns);

        // Update percentiles
        self.update_percentiles();
    }

    /// Update percentile calculations based on recent latencies
    fn update_percentiles(&mut self) {
        if self.recent_latencies.is_empty() {
            return;
        }

        let mut sorted = self.recent_latencies.clone();
        sorted.sort_unstable();

        let len = sorted.len();
        self.p50_latency_ns = sorted[len / 2];
        self.p95_latency_ns = sorted[(len as f64 * 0.95) as usize];
        self.p99_latency_ns = sorted[(len as f64 * 0.99) as usize];
    }

    /// Get average latency
    pub fn avg_latency_ns(&self) -> u64 {
        self.total_duration_ns
            .checked_div(self.call_count)
            .unwrap_or(0)
    }

    /// Get average latency as Duration
    pub fn avg_latency(&self) -> Duration {
        Duration::from_nanos(self.avg_latency_ns())
    }

    /// Check if operation meets performance target
    pub fn meets_target(&self, target_p99_ns: u64) -> bool {
        self.p99_latency_ns <= target_p99_ns
    }
}

impl AuthzProfiler {
    /// Create a new profiler
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Profile a synchronous operation
    pub fn profile<F, T>(&self, operation_name: &str, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();

        self.record_measurement(operation_name, duration);
        result
    }

    /// Profile an asynchronous operation
    pub async fn profile_async<F, T>(&self, operation_name: &str, fut: F) -> T
    where
        F: Future<Output = T>,
    {
        let start = Instant::now();
        let result = fut.await;
        let duration = start.elapsed();

        self.record_measurement(operation_name, duration);
        result
    }

    /// Record a measurement manually
    pub fn record_measurement(&self, operation_name: &str, duration: Duration) {
        let duration_ns = duration.as_nanos() as u64;

        let mut metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());
        let entry = metrics
            .entry(operation_name.to_string())
            .or_insert_with(|| OperationMetrics::new(operation_name.to_string()));

        entry.record(duration_ns);
    }

    /// Get metrics for a specific operation
    pub fn get_operation_metrics(&self, operation_name: &str) -> Option<OperationMetrics> {
        self.metrics.lock().unwrap_or_else(|e| e.into_inner()).get(operation_name).cloned()
    }

    /// Get all profiling statistics
    pub fn get_stats(&self) -> ProfilingStats {
        let metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());

        let total_operations: u64 = metrics.values().map(|m| m.call_count).sum();
        let total_duration_ns: u64 = metrics.values().map(|m| m.total_duration_ns).sum();

        let operations: Vec<OperationMetrics> = metrics.values().cloned().collect();

        ProfilingStats {
            total_operations,
            total_duration_ns,
            operations,
        }
    }

    /// Reset all profiling data
    pub fn reset(&self) {
        self.metrics.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Generate a performance report
    pub fn generate_report(&self) -> String {
        let stats = self.get_stats();
        let mut report = String::new();

        report.push_str("=== Authorization Performance Report ===\n\n");
        report.push_str(&format!("Total Operations: {}\n", stats.total_operations));
        report.push_str(&format!(
            "Total Time: {:.2}ms\n\n",
            stats.total_duration_ns as f64 / 1_000_000.0
        ));

        report.push_str("Operation Breakdown:\n");
        report.push_str(&format!(
            "{:<30} {:>10} {:>12} {:>12} {:>12} {:>12}\n",
            "Operation", "Calls", "Avg (μs)", "P50 (μs)", "P95 (μs)", "P99 (μs)"
        ));
        report.push_str(&"-".repeat(100));
        report.push('\n');

        let mut sorted_ops = stats.operations.clone();
        sorted_ops.sort_by_key(|m| std::cmp::Reverse(m.call_count));

        for metric in sorted_ops {
            report.push_str(&format!(
                "{:<30} {:>10} {:>12.2} {:>12.2} {:>12.2} {:>12.2}\n",
                metric.name,
                metric.call_count,
                metric.avg_latency_ns() as f64 / 1_000.0,
                metric.p50_latency_ns as f64 / 1_000.0,
                metric.p95_latency_ns as f64 / 1_000.0,
                metric.p99_latency_ns as f64 / 1_000.0,
            ));
        }

        report
    }

    /// Export metrics as JSON
    pub fn export_json(&self) -> serde_json::Result<String> {
        let stats = self.get_stats();
        serde_json::to_string_pretty(&stats)
    }
}

impl Default for AuthzProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Overall profiling statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilingStats {
    /// Total number of operations profiled
    pub total_operations: u64,
    /// Total time spent in all operations (nanoseconds)
    pub total_duration_ns: u64,
    /// Metrics for individual operations
    pub operations: Vec<OperationMetrics>,
}

impl ProfilingStats {
    /// Get average latency across all operations
    pub fn avg_latency(&self) -> Duration {
        Duration::from_nanos(
            self.total_duration_ns
                .checked_div(self.total_operations)
                .unwrap_or(0),
        )
    }

    /// Find slowest operation
    pub fn slowest_operation(&self) -> Option<&OperationMetrics> {
        self.operations.iter().max_by_key(|m| m.p99_latency_ns)
    }

    /// Find most called operation
    pub fn most_called_operation(&self) -> Option<&OperationMetrics> {
        self.operations.iter().max_by_key(|m| m.call_count)
    }
}

/// Lightweight performance counter for hot paths
#[derive(Clone)]
pub struct PerfCounter {
    count: Arc<AtomicU64>,
    total_ns: Arc<AtomicU64>,
}

impl PerfCounter {
    /// Create a new performance counter
    pub fn new() -> Self {
        Self {
            count: Arc::new(AtomicU64::new(0)),
            total_ns: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a measurement
    pub fn record(&self, duration: Duration) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    /// Get count
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Get average latency in nanoseconds
    pub fn avg_ns(&self) -> u64 {
        let count = self.count();
        self.total_ns
            .load(Ordering::Relaxed)
            .checked_div(count)
            .unwrap_or(0)
    }

    /// Get average latency as Duration
    pub fn avg_duration(&self) -> Duration {
        Duration::from_nanos(self.avg_ns())
    }

    /// Reset the counter
    pub fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
        self.total_ns.store(0, Ordering::Relaxed);
    }
}

impl Default for PerfCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_profiler_basic() {
        let profiler = AuthzProfiler::new();

        // Profile some operations
        for _ in 0..10 {
            profiler.profile("test_op", || {
                thread::sleep(Duration::from_millis(1));
            });
        }

        let stats = profiler.get_stats();
        assert_eq!(stats.total_operations, 10);

        let metrics = profiler.get_operation_metrics("test_op").unwrap();
        assert_eq!(metrics.call_count, 10);
        assert!(metrics.avg_latency().as_millis() >= 1);
    }

    #[tokio::test]
    async fn test_profiler_async() {
        let profiler = AuthzProfiler::new();

        // Profile async operations
        for _ in 0..5 {
            profiler
                .profile_async("async_op", async {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                })
                .await;
        }

        let metrics = profiler.get_operation_metrics("async_op").unwrap();
        assert_eq!(metrics.call_count, 5);
        assert!(metrics.avg_latency().as_millis() >= 2);
    }

    #[test]
    fn test_operation_metrics() {
        let mut metrics = OperationMetrics::new("test".to_string());

        // Record some measurements
        metrics.record(1_000_000); // 1ms
        metrics.record(2_000_000); // 2ms
        metrics.record(3_000_000); // 3ms
        metrics.record(10_000_000); // 10ms

        assert_eq!(metrics.call_count, 4);
        assert_eq!(metrics.min_latency_ns, 1_000_000);
        assert_eq!(metrics.max_latency_ns, 10_000_000);
        assert_eq!(metrics.avg_latency_ns(), 4_000_000);
    }

    #[test]
    fn test_perf_counter() {
        let counter = PerfCounter::new();

        for _ in 0..100 {
            counter.record(Duration::from_micros(100));
        }

        assert_eq!(counter.count(), 100);
        assert_eq!(counter.avg_ns(), 100_000);
        assert_eq!(counter.avg_duration(), Duration::from_micros(100));
    }

    #[test]
    fn test_report_generation() {
        let profiler = AuthzProfiler::new();

        profiler.profile("check_permission", || {
            thread::sleep(Duration::from_micros(100));
        });

        profiler.profile("write_tuple", || {
            thread::sleep(Duration::from_micros(200));
        });

        let report = profiler.generate_report();
        assert!(report.contains("Authorization Performance Report"));
        assert!(report.contains("check_permission"));
        assert!(report.contains("write_tuple"));
    }

    #[test]
    fn test_json_export() {
        let profiler = AuthzProfiler::new();

        profiler.profile("test_op", || {});

        let json = profiler.export_json().unwrap();
        assert!(json.contains("total_operations"));
        assert!(json.contains("test_op"));
    }

    #[test]
    fn test_percentile_calculation() {
        let mut metrics = OperationMetrics::new("test".to_string());

        // Record measurements with known distribution
        for i in 1..=100 {
            metrics.record(i * 1_000_000); // 1ms to 100ms
        }

        assert!(metrics.p50_latency_ns > 40_000_000); // ~50ms
        assert!(metrics.p95_latency_ns > 90_000_000); // ~95ms
        assert!(metrics.p99_latency_ns > 95_000_000); // ~99ms
    }

    #[test]
    fn test_profiler_reset() {
        let profiler = AuthzProfiler::new();

        profiler.profile("test", || {});
        assert_eq!(profiler.get_stats().total_operations, 1);

        profiler.reset();
        assert_eq!(profiler.get_stats().total_operations, 0);
    }

    #[test]
    fn test_meets_target() {
        let mut metrics = OperationMetrics::new("test".to_string());

        for _ in 0..100 {
            metrics.record(50_000); // 50μs
        }

        // Should meet 100μs target
        assert!(metrics.meets_target(100_000));

        // Should not meet 10μs target
        assert!(!metrics.meets_target(10_000));
    }

    #[test]
    fn test_stats_slowest_operation() {
        let profiler = AuthzProfiler::new();

        profiler.record_measurement("fast_op", Duration::from_micros(10));
        profiler.record_measurement("slow_op", Duration::from_millis(100));

        let stats = profiler.get_stats();
        let slowest = stats.slowest_operation().unwrap();
        assert_eq!(slowest.name, "slow_op");
    }

    #[test]
    fn test_stats_most_called() {
        let profiler = AuthzProfiler::new();

        for _ in 0..100 {
            profiler.record_measurement("frequent_op", Duration::from_micros(1));
        }
        profiler.record_measurement("rare_op", Duration::from_micros(1));

        let stats = profiler.get_stats();
        let most_called = stats.most_called_operation().unwrap();
        assert_eq!(most_called.name, "frequent_op");
        assert_eq!(most_called.call_count, 100);
    }
}
