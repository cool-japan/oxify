//! Metrics collection and reporting module.
//!
//! This module provides comprehensive metrics tracking for OCR operations:
//! - Processing latency (histograms)
//! - Cache hit/miss rates
//! - Error rates by type
//! - Provider usage statistics
//! - Throughput metrics
//!
//! Metrics are designed to be compatible with Prometheus format.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Metric type for categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricType {
    /// Counter metric (monotonically increasing)
    Counter,
    /// Gauge metric (can increase or decrease)
    Gauge,
    /// Histogram metric (distribution of values)
    Histogram,
}

/// A single metric value
#[derive(Debug, Clone)]
pub struct MetricValue {
    /// Metric type
    pub metric_type: MetricType,
    /// Current value
    pub value: f64,
    /// Optional help text
    pub help: String,
    /// Labels for this metric
    pub labels: HashMap<String, String>,
}

/// Counter metric (monotonically increasing)
#[derive(Debug, Default)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    /// Create a new counter
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    /// Increment the counter by 1
    pub fn inc(&self) {
        self.add(1);
    }

    /// Add a value to the counter
    pub fn add(&self, value: u64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    /// Get the current value
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Reset the counter to 0
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}

/// Gauge metric (can increase or decrease)
#[derive(Debug, Default)]
pub struct Gauge {
    value: AtomicU64,
}

impl Gauge {
    /// Create a new gauge
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    /// Set the gauge value
    pub fn set(&self, value: f64) {
        self.value.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Increment the gauge by 1
    pub fn inc(&self) {
        self.add(1.0);
    }

    /// Decrement the gauge by 1
    pub fn dec(&self) {
        self.sub(1.0);
    }

    /// Add to the gauge
    pub fn add(&self, value: f64) {
        let mut old = self.value.load(Ordering::Relaxed);
        loop {
            let old_f64 = f64::from_bits(old);
            let new = (old_f64 + value).to_bits();
            match self
                .value
                .compare_exchange_weak(old, new, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(x) => old = x,
            }
        }
    }

    /// Subtract from the gauge
    pub fn sub(&self, value: f64) {
        self.add(-value);
    }

    /// Get the current value
    pub fn get(&self) -> f64 {
        f64::from_bits(self.value.load(Ordering::Relaxed))
    }
}

/// Histogram bucket
#[derive(Debug, Clone)]
pub struct HistogramBucket {
    /// Upper bound for this bucket
    pub upper_bound: f64,
    /// Count of values in this bucket
    pub count: Arc<AtomicU64>,
}

/// Histogram metric (distribution of values)
#[derive(Debug, Clone)]
pub struct Histogram {
    buckets: Vec<HistogramBucket>,
    sum: Arc<AtomicU64>,
    count: Arc<AtomicU64>,
}

impl Histogram {
    /// Create a new histogram with default buckets
    pub fn new() -> Self {
        Self::with_buckets(vec![
            0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.5, 5.0, 10.0,
        ])
    }

    /// Create a histogram with custom buckets
    pub fn with_buckets(bounds: Vec<f64>) -> Self {
        let mut buckets = Vec::new();
        for bound in bounds {
            buckets.push(HistogramBucket {
                upper_bound: bound,
                count: Arc::new(AtomicU64::new(0)),
            });
        }
        // Add +Inf bucket
        buckets.push(HistogramBucket {
            upper_bound: f64::INFINITY,
            count: Arc::new(AtomicU64::new(0)),
        });

        Self {
            buckets,
            sum: Arc::new(AtomicU64::new(0)),
            count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Observe a value
    pub fn observe(&self, value: f64) {
        // Update sum
        let mut old_sum = self.sum.load(Ordering::Relaxed);
        loop {
            let old_sum_f64 = f64::from_bits(old_sum);
            let new_sum = (old_sum_f64 + value).to_bits();
            match self.sum.compare_exchange_weak(
                old_sum,
                new_sum,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => old_sum = x,
            }
        }

        // Increment count
        self.count.fetch_add(1, Ordering::Relaxed);

        // Update buckets
        for bucket in &self.buckets {
            if value <= bucket.upper_bound {
                bucket.count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Get the sum of all observed values
    pub fn sum(&self) -> f64 {
        f64::from_bits(self.sum.load(Ordering::Relaxed))
    }

    /// Get the count of observed values
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Get the buckets
    pub fn buckets(&self) -> &[HistogramBucket] {
        &self.buckets
    }

    /// Get the average value
    pub fn avg(&self) -> f64 {
        let count = self.count();
        if count == 0 {
            0.0
        } else {
            self.sum() / count as f64
        }
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer for measuring durations
pub struct Timer {
    histogram: Histogram,
    start: Instant,
}

impl Timer {
    /// Start a new timer
    pub fn start(histogram: Histogram) -> Self {
        Self {
            histogram,
            start: Instant::now(),
        }
    }

    /// Stop the timer and record the duration
    pub fn stop(self) {
        let duration = self.start.elapsed();
        self.histogram.observe(duration.as_secs_f64());
    }
}

/// Metrics collector for OCR operations
#[derive(Debug, Clone)]
pub struct OcrMetrics {
    // Processing metrics
    pub requests_total: Arc<Counter>,
    pub requests_success: Arc<Counter>,
    pub requests_failed: Arc<Counter>,
    pub processing_duration: Histogram,

    // Cache metrics
    pub cache_hits: Arc<Counter>,
    pub cache_misses: Arc<Counter>,
    pub cache_size: Arc<Gauge>,

    // Provider metrics
    provider_usage: Arc<RwLock<HashMap<String, u64>>>,

    // Error metrics
    error_counts: Arc<RwLock<HashMap<String, u64>>>,

    // Throughput metrics
    pub bytes_processed: Arc<Counter>,
    pub images_processed: Arc<Counter>,
}

impl OcrMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            requests_total: Arc::new(Counter::new()),
            requests_success: Arc::new(Counter::new()),
            requests_failed: Arc::new(Counter::new()),
            processing_duration: Histogram::new(),
            cache_hits: Arc::new(Counter::new()),
            cache_misses: Arc::new(Counter::new()),
            cache_size: Arc::new(Gauge::new()),
            provider_usage: Arc::new(RwLock::new(HashMap::new())),
            error_counts: Arc::new(RwLock::new(HashMap::new())),
            bytes_processed: Arc::new(Counter::new()),
            images_processed: Arc::new(Counter::new()),
        }
    }

    /// Start timing a request
    pub fn start_timer(&self) -> Timer {
        self.requests_total.inc();
        Timer::start(self.processing_duration.clone())
    }

    /// Record a successful request
    pub fn record_success(&self) {
        self.requests_success.inc();
    }

    /// Record a failed request
    pub fn record_failure(&self, error_type: &str) {
        self.requests_failed.inc();
        let mut errors = self.error_counts.write().unwrap();
        *errors.entry(error_type.to_string()).or_insert(0) += 1;
    }

    /// Record a cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits.inc();
    }

    /// Record a cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses.inc();
    }

    /// Update cache size
    pub fn update_cache_size(&self, size: usize) {
        self.cache_size.set(size as f64);
    }

    /// Record provider usage
    pub fn record_provider_usage(&self, provider: &str) {
        let mut usage = self.provider_usage.write().unwrap();
        *usage.entry(provider.to_string()).or_insert(0) += 1;
    }

    /// Record bytes processed
    pub fn record_bytes(&self, bytes: usize) {
        self.bytes_processed.add(bytes as u64);
    }

    /// Record an image processed
    pub fn record_image(&self) {
        self.images_processed.inc();
    }

    /// Get cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.get() as f64;
        let misses = self.cache_misses.get() as f64;
        let total = hits + misses;
        if total == 0.0 {
            0.0
        } else {
            hits / total
        }
    }

    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        let total = self.requests_total.get() as f64;
        if total == 0.0 {
            0.0
        } else {
            self.requests_failed.get() as f64 / total
        }
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.requests_total.get() as f64;
        if total == 0.0 {
            0.0
        } else {
            self.requests_success.get() as f64 / total
        }
    }

    /// Get average processing duration
    pub fn avg_duration(&self) -> Duration {
        Duration::from_secs_f64(self.processing_duration.avg())
    }

    /// Get provider usage statistics
    pub fn provider_stats(&self) -> HashMap<String, u64> {
        self.provider_usage.read().unwrap().clone()
    }

    /// Get error statistics
    pub fn error_stats(&self) -> HashMap<String, u64> {
        self.error_counts.read().unwrap().clone()
    }

    /// Get a summary of all metrics
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            total_requests: self.requests_total.get(),
            successful_requests: self.requests_success.get(),
            failed_requests: self.requests_failed.get(),
            cache_hits: self.cache_hits.get(),
            cache_misses: self.cache_misses.get(),
            cache_hit_rate: self.cache_hit_rate(),
            error_rate: self.error_rate(),
            success_rate: self.success_rate(),
            avg_duration: self.avg_duration(),
            bytes_processed: self.bytes_processed.get(),
            images_processed: self.images_processed.get(),
            provider_stats: self.provider_stats(),
            error_stats: self.error_stats(),
        }
    }

    /// Export metrics in Prometheus format
    pub fn prometheus_format(&self) -> String {
        let mut output = String::new();

        // Requests
        output.push_str("# HELP ocr_requests_total Total number of OCR requests\n");
        output.push_str("# TYPE ocr_requests_total counter\n");
        output.push_str(&format!(
            "ocr_requests_total {}\n",
            self.requests_total.get()
        ));

        output.push_str("# HELP ocr_requests_success Number of successful OCR requests\n");
        output.push_str("# TYPE ocr_requests_success counter\n");
        output.push_str(&format!(
            "ocr_requests_success {}\n",
            self.requests_success.get()
        ));

        output.push_str("# HELP ocr_requests_failed Number of failed OCR requests\n");
        output.push_str("# TYPE ocr_requests_failed counter\n");
        output.push_str(&format!(
            "ocr_requests_failed {}\n",
            self.requests_failed.get()
        ));

        // Cache
        output.push_str("# HELP ocr_cache_hits Number of cache hits\n");
        output.push_str("# TYPE ocr_cache_hits counter\n");
        output.push_str(&format!("ocr_cache_hits {}\n", self.cache_hits.get()));

        output.push_str("# HELP ocr_cache_misses Number of cache misses\n");
        output.push_str("# TYPE ocr_cache_misses counter\n");
        output.push_str(&format!("ocr_cache_misses {}\n", self.cache_misses.get()));

        output.push_str("# HELP ocr_cache_size Current cache size\n");
        output.push_str("# TYPE ocr_cache_size gauge\n");
        output.push_str(&format!("ocr_cache_size {}\n", self.cache_size.get()));

        // Processing duration histogram
        output.push_str("# HELP ocr_processing_duration_seconds OCR processing duration\n");
        output.push_str("# TYPE ocr_processing_duration_seconds histogram\n");
        for bucket in self.processing_duration.buckets() {
            output.push_str(&format!(
                "ocr_processing_duration_seconds_bucket{{le=\"{}\"}} {}\n",
                bucket.upper_bound,
                bucket.count.load(Ordering::Relaxed)
            ));
        }
        output.push_str(&format!(
            "ocr_processing_duration_seconds_sum {}\n",
            self.processing_duration.sum()
        ));
        output.push_str(&format!(
            "ocr_processing_duration_seconds_count {}\n",
            self.processing_duration.count()
        ));

        // Provider usage
        output.push_str("# HELP ocr_provider_usage Usage count by provider\n");
        output.push_str("# TYPE ocr_provider_usage counter\n");
        for (provider, count) in self.provider_stats() {
            output.push_str(&format!(
                "ocr_provider_usage{{provider=\"{}\"}} {}\n",
                provider, count
            ));
        }

        // Error counts
        output.push_str("# HELP ocr_errors_total Error count by type\n");
        output.push_str("# TYPE ocr_errors_total counter\n");
        for (error_type, count) in self.error_stats() {
            output.push_str(&format!(
                "ocr_errors_total{{type=\"{}\"}} {}\n",
                error_type, count
            ));
        }

        output
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.requests_total.reset();
        self.requests_success.reset();
        self.requests_failed.reset();
        self.cache_hits.reset();
        self.cache_misses.reset();
        self.bytes_processed.reset();
        self.images_processed.reset();
        self.provider_usage.write().unwrap().clear();
        self.error_counts.write().unwrap().clear();
    }
}

impl Default for OcrMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of metrics
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub error_rate: f64,
    pub success_rate: f64,
    pub avg_duration: Duration,
    pub bytes_processed: u64,
    pub images_processed: u64,
    pub provider_stats: HashMap<String, u64>,
    pub error_stats: HashMap<String, u64>,
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "OCR Metrics Summary")?;
        writeln!(f, "===================")?;
        writeln!(f, "Total Requests:      {}", self.total_requests)?;
        writeln!(f, "Successful:          {}", self.successful_requests)?;
        writeln!(f, "Failed:              {}", self.failed_requests)?;
        writeln!(f, "Success Rate:        {:.2}%", self.success_rate * 100.0)?;
        writeln!(f, "Error Rate:          {:.2}%", self.error_rate * 100.0)?;
        writeln!(f, "Cache Hits:          {}", self.cache_hits)?;
        writeln!(f, "Cache Misses:        {}", self.cache_misses)?;
        writeln!(
            f,
            "Cache Hit Rate:      {:.2}%",
            self.cache_hit_rate * 100.0
        )?;
        writeln!(f, "Avg Duration:        {:?}", self.avg_duration)?;
        writeln!(f, "Images Processed:    {}", self.images_processed)?;
        writeln!(f, "Bytes Processed:     {}", self.bytes_processed)?;

        if !self.provider_stats.is_empty() {
            writeln!(f, "\nProvider Usage:")?;
            for (provider, count) in &self.provider_stats {
                writeln!(f, "  {}: {}", provider, count)?;
            }
        }

        if !self.error_stats.is_empty() {
            writeln!(f, "\nError Statistics:")?;
            for (error_type, count) in &self.error_stats {
                writeln!(f, "  {}: {}", error_type, count)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new();
        assert_eq!(counter.get(), 0);

        counter.inc();
        assert_eq!(counter.get(), 1);

        counter.add(5);
        assert_eq!(counter.get(), 6);

        counter.reset();
        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new();
        assert_eq!(gauge.get(), 0.0);

        gauge.set(10.0);
        assert_eq!(gauge.get(), 10.0);

        gauge.inc();
        assert_eq!(gauge.get(), 11.0);

        gauge.dec();
        assert_eq!(gauge.get(), 10.0);

        gauge.add(5.5);
        assert_eq!(gauge.get(), 15.5);

        gauge.sub(2.5);
        assert_eq!(gauge.get(), 13.0);
    }

    #[test]
    fn test_histogram() {
        let hist = Histogram::new();
        assert_eq!(hist.count(), 0);
        assert_eq!(hist.sum(), 0.0);

        hist.observe(0.5);
        hist.observe(1.5);
        hist.observe(2.5);

        assert_eq!(hist.count(), 3);
        assert_eq!(hist.sum(), 4.5);
        assert_eq!(hist.avg(), 1.5);
    }

    #[test]
    fn test_ocr_metrics() {
        let metrics = OcrMetrics::new();

        // Test requests
        metrics.requests_total.inc();
        metrics.record_success();
        assert_eq!(metrics.requests_total.get(), 1);
        assert_eq!(metrics.requests_success.get(), 1);

        // Test failures
        metrics.record_failure("timeout");
        assert_eq!(metrics.requests_failed.get(), 1);

        // Test cache
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();
        assert_eq!(metrics.cache_hits.get(), 2);
        assert_eq!(metrics.cache_misses.get(), 1);
        assert_eq!(metrics.cache_hit_rate(), 2.0 / 3.0);

        // Test provider usage
        metrics.record_provider_usage("tesseract");
        metrics.record_provider_usage("tesseract");
        metrics.record_provider_usage("surya");
        let stats = metrics.provider_stats();
        assert_eq!(stats.get("tesseract"), Some(&2));
        assert_eq!(stats.get("surya"), Some(&1));
    }

    #[test]
    fn test_metrics_summary() {
        let metrics = OcrMetrics::new();
        metrics.requests_total.inc();
        metrics.record_success();
        metrics.record_cache_hit();

        let summary = metrics.summary();
        assert_eq!(summary.total_requests, 1);
        assert_eq!(summary.successful_requests, 1);
        assert_eq!(summary.cache_hits, 1);
    }

    #[test]
    fn test_prometheus_format() {
        let metrics = OcrMetrics::new();
        metrics.requests_total.inc();
        metrics.record_success();

        let output = metrics.prometheus_format();
        assert!(output.contains("ocr_requests_total"));
        assert!(output.contains("ocr_requests_success"));
    }

    #[test]
    fn test_timer() {
        let hist = Histogram::new();
        let timer = Timer::start(hist.clone());
        std::thread::sleep(Duration::from_millis(10));
        timer.stop();

        assert!(hist.count() > 0);
        assert!(hist.sum() > 0.0);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = OcrMetrics::new();
        metrics.requests_total.inc();
        metrics.record_success();
        metrics.record_cache_hit();

        metrics.reset();

        assert_eq!(metrics.requests_total.get(), 0);
        assert_eq!(metrics.requests_success.get(), 0);
        assert_eq!(metrics.cache_hits.get(), 0);
    }

    #[test]
    fn test_error_rate() {
        let metrics = OcrMetrics::new();
        metrics.requests_total.add(10);
        metrics.requests_success.add(7);
        metrics.requests_failed.add(3);

        assert_eq!(metrics.error_rate(), 0.3);
        assert_eq!(metrics.success_rate(), 0.7);
    }

    #[test]
    fn test_histogram_buckets() {
        let hist = Histogram::with_buckets(vec![1.0, 5.0, 10.0]);
        hist.observe(0.5);
        hist.observe(2.0);
        hist.observe(7.0);
        hist.observe(15.0);

        let buckets = hist.buckets();
        assert_eq!(buckets.len(), 4); // 3 + infinity
        assert_eq!(buckets[0].count.load(Ordering::Relaxed), 1);
        assert_eq!(buckets[1].count.load(Ordering::Relaxed), 2);
        assert_eq!(buckets[2].count.load(Ordering::Relaxed), 3);
        assert_eq!(buckets[3].count.load(Ordering::Relaxed), 4);
    }
}
