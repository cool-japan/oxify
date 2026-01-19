//! Observability and Metrics
//!
//! This module provides metrics collection and tracking for vector search operations.
//!
//! ## Features
//!
//! - **Search Latency**: Track p50, p95, p99 latencies
//! - **Queries Per Second (QPS)**: Track query throughput
//! - **Index Health**: Monitor index size and build time
//! - **Thread-safe**: Metrics can be collected from multiple threads
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::metrics::Metrics;
//! use std::time::Duration;
//!
//! # fn example() {
//! let metrics = Metrics::new();
//!
//! // Record search latency
//! metrics.record_search_latency(Duration::from_micros(150));
//! metrics.record_search_latency(Duration::from_micros(200));
//!
//! // Get statistics
//! let stats = metrics.get_search_stats();
//! println!("p50 latency: {:?}", stats.p50_latency);
//! println!("QPS: {:.2}", stats.qps);
//! # }
//! ```

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Search metrics statistics
#[derive(Debug, Clone)]
pub struct SearchStats {
    /// Total number of queries
    pub total_queries: u64,
    /// Queries per second
    pub qps: f64,
    /// p50 (median) latency
    pub p50_latency: Duration,
    /// p95 latency
    pub p95_latency: Duration,
    /// p99 latency
    pub p99_latency: Duration,
    /// Average latency
    pub avg_latency: Duration,
    /// Minimum latency
    pub min_latency: Duration,
    /// Maximum latency
    pub max_latency: Duration,
}

impl Default for SearchStats {
    fn default() -> Self {
        Self {
            total_queries: 0,
            qps: 0.0,
            p50_latency: Duration::ZERO,
            p95_latency: Duration::ZERO,
            p99_latency: Duration::ZERO,
            avg_latency: Duration::ZERO,
            min_latency: Duration::MAX,
            max_latency: Duration::ZERO,
        }
    }
}

/// Index metrics statistics
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Number of vectors in index
    pub num_vectors: usize,
    /// Vector dimension
    pub dimensions: usize,
    /// Index build time
    pub build_time: Duration,
    /// Memory usage estimate (bytes)
    pub memory_bytes: usize,
}

impl Default for IndexStats {
    fn default() -> Self {
        Self {
            num_vectors: 0,
            dimensions: 0,
            build_time: Duration::ZERO,
            memory_bytes: 0,
        }
    }
}

/// Thread-safe metrics collector
#[derive(Clone)]
pub struct Metrics {
    search_metrics: Arc<Mutex<SearchMetrics>>,
    index_stats: Arc<Mutex<IndexStats>>,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            search_metrics: Arc::new(Mutex::new(SearchMetrics::new())),
            index_stats: Arc::new(Mutex::new(IndexStats::default())),
        }
    }

    /// Record a search latency
    pub fn record_search_latency(&self, latency: Duration) {
        let mut metrics = self.search_metrics.lock().unwrap();
        metrics.record_latency(latency);
    }

    /// Get search statistics
    pub fn get_search_stats(&self) -> SearchStats {
        let metrics = self.search_metrics.lock().unwrap();
        metrics.compute_stats()
    }

    /// Set index statistics
    pub fn set_index_stats(&self, stats: IndexStats) {
        let mut index_stats = self.index_stats.lock().unwrap();
        *index_stats = stats;
    }

    /// Get index statistics
    pub fn get_index_stats(&self) -> IndexStats {
        let index_stats = self.index_stats.lock().unwrap();
        index_stats.clone()
    }

    /// Reset all metrics
    pub fn reset(&self) {
        let mut search_metrics = self.search_metrics.lock().unwrap();
        *search_metrics = SearchMetrics::new();

        let mut index_stats = self.index_stats.lock().unwrap();
        *index_stats = IndexStats::default();
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal search metrics collector
struct SearchMetrics {
    latencies: Vec<Duration>,
    start_time: Instant,
}

impl SearchMetrics {
    fn new() -> Self {
        Self {
            latencies: Vec::new(),
            start_time: Instant::now(),
        }
    }

    fn record_latency(&mut self, latency: Duration) {
        self.latencies.push(latency);
    }

    fn compute_stats(&self) -> SearchStats {
        if self.latencies.is_empty() {
            return SearchStats::default();
        }

        let mut sorted_latencies = self.latencies.clone();
        sorted_latencies.sort();

        let total_queries = sorted_latencies.len() as u64;

        // Calculate percentiles (using linear interpolation at percentile boundary)
        let p50_idx = ((total_queries as f64 * 0.50).ceil() as usize).saturating_sub(1);
        let p95_idx = ((total_queries as f64 * 0.95).ceil() as usize).saturating_sub(1);
        let p99_idx = ((total_queries as f64 * 0.99).ceil() as usize).saturating_sub(1);

        let p50_latency = sorted_latencies
            .get(p50_idx)
            .copied()
            .unwrap_or(Duration::ZERO);
        let p95_latency = sorted_latencies
            .get(p95_idx)
            .copied()
            .unwrap_or(Duration::ZERO);
        let p99_latency = sorted_latencies
            .get(p99_idx)
            .copied()
            .unwrap_or(Duration::ZERO);

        // Calculate average
        let total_micros: u128 = sorted_latencies.iter().map(|d| d.as_micros()).sum();
        let avg_micros = total_micros / total_queries as u128;
        let avg_latency = Duration::from_micros(avg_micros as u64);

        // Calculate min and max
        let min_latency = *sorted_latencies.first().unwrap();
        let max_latency = *sorted_latencies.last().unwrap();

        // Calculate QPS
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let qps = if elapsed > 0.0 {
            total_queries as f64 / elapsed
        } else {
            0.0
        };

        SearchStats {
            total_queries,
            qps,
            p50_latency,
            p95_latency,
            p99_latency,
            avg_latency,
            min_latency,
            max_latency,
        }
    }
}

/// Helper to measure search latency
pub struct LatencyTimer {
    start: Instant,
    metrics: Option<Metrics>,
}

impl LatencyTimer {
    /// Create a new latency timer
    pub fn new(metrics: Option<Metrics>) -> Self {
        Self {
            start: Instant::now(),
            metrics,
        }
    }

    /// Finish timing and record the latency
    pub fn finish(self) -> Duration {
        let latency = self.start.elapsed();
        if let Some(metrics) = self.metrics {
            metrics.record_search_latency(latency);
        }
        latency
    }

    /// Get elapsed time without finishing
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        let stats = metrics.get_search_stats();
        assert_eq!(stats.total_queries, 0);
        assert_eq!(stats.qps, 0.0);
    }

    #[test]
    fn test_record_search_latency() {
        let metrics = Metrics::new();

        metrics.record_search_latency(Duration::from_micros(100));
        metrics.record_search_latency(Duration::from_micros(200));
        metrics.record_search_latency(Duration::from_micros(300));

        let stats = metrics.get_search_stats();
        assert_eq!(stats.total_queries, 3);
        assert_eq!(stats.min_latency, Duration::from_micros(100));
        assert_eq!(stats.max_latency, Duration::from_micros(300));
        assert_eq!(stats.p50_latency, Duration::from_micros(200));
    }

    #[test]
    fn test_percentiles() {
        let metrics = Metrics::new();

        // Record 100 samples: 1μs, 2μs, ..., 100μs
        for i in 1..=100 {
            metrics.record_search_latency(Duration::from_micros(i));
        }

        let stats = metrics.get_search_stats();
        assert_eq!(stats.total_queries, 100);

        // p50 should be around 50μs
        assert!(stats.p50_latency >= Duration::from_micros(49));
        assert!(stats.p50_latency <= Duration::from_micros(51));

        // p95 should be around 95μs
        assert!(stats.p95_latency >= Duration::from_micros(94));
        assert!(stats.p95_latency <= Duration::from_micros(96));

        // p99 should be around 99μs
        assert!(stats.p99_latency >= Duration::from_micros(98));
        assert!(stats.p99_latency <= Duration::from_micros(100));
    }

    #[test]
    fn test_average_latency() {
        let metrics = Metrics::new();

        metrics.record_search_latency(Duration::from_micros(100));
        metrics.record_search_latency(Duration::from_micros(200));
        metrics.record_search_latency(Duration::from_micros(300));

        let stats = metrics.get_search_stats();
        assert_eq!(stats.avg_latency, Duration::from_micros(200));
    }

    #[test]
    fn test_qps_calculation() {
        let metrics = Metrics::new();

        // Record some queries
        for _ in 0..10 {
            metrics.record_search_latency(Duration::from_micros(100));
        }

        // Wait a bit
        thread::sleep(Duration::from_millis(100));

        let stats = metrics.get_search_stats();
        assert!(stats.qps > 0.0);
        assert_eq!(stats.total_queries, 10);
    }

    #[test]
    fn test_index_stats() {
        let metrics = Metrics::new();

        let index_stats = IndexStats {
            num_vectors: 1000,
            dimensions: 768,
            build_time: Duration::from_secs(5),
            memory_bytes: 1024 * 1024, // 1MB
        };

        metrics.set_index_stats(index_stats.clone());

        let retrieved = metrics.get_index_stats();
        assert_eq!(retrieved.num_vectors, 1000);
        assert_eq!(retrieved.dimensions, 768);
        assert_eq!(retrieved.build_time, Duration::from_secs(5));
        assert_eq!(retrieved.memory_bytes, 1024 * 1024);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = Metrics::new();

        // Add some data
        metrics.record_search_latency(Duration::from_micros(100));
        metrics.record_search_latency(Duration::from_micros(200));

        let stats = metrics.get_search_stats();
        assert_eq!(stats.total_queries, 2);

        // Reset
        metrics.reset();

        let stats = metrics.get_search_stats();
        assert_eq!(stats.total_queries, 0);
    }

    #[test]
    fn test_latency_timer() {
        let metrics = Metrics::new();

        let timer = LatencyTimer::new(Some(metrics.clone()));
        thread::sleep(Duration::from_millis(10));
        let latency = timer.finish();

        assert!(latency >= Duration::from_millis(10));

        let stats = metrics.get_search_stats();
        assert_eq!(stats.total_queries, 1);
        assert!(stats.min_latency >= Duration::from_millis(10));
    }

    #[test]
    fn test_latency_timer_without_metrics() {
        let timer = LatencyTimer::new(None);
        thread::sleep(Duration::from_millis(5));
        let latency = timer.finish();

        assert!(latency >= Duration::from_millis(5));
    }

    #[test]
    fn test_latency_timer_elapsed() {
        let timer = LatencyTimer::new(None);
        thread::sleep(Duration::from_millis(5));
        let elapsed = timer.elapsed();

        assert!(elapsed >= Duration::from_millis(5));
    }

    #[test]
    fn test_thread_safety() {
        let metrics = Metrics::new();
        let metrics_clone = metrics.clone();

        let handle = thread::spawn(move || {
            for _ in 0..100 {
                metrics_clone.record_search_latency(Duration::from_micros(100));
            }
        });

        for _ in 0..100 {
            metrics.record_search_latency(Duration::from_micros(200));
        }

        handle.join().unwrap();

        let stats = metrics.get_search_stats();
        assert_eq!(stats.total_queries, 200);
    }
}
