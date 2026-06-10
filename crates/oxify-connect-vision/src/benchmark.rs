//! Benchmark and comparison utilities.
//!
//! Provides tools for comparing provider performance,
//! generating benchmark reports, and profiling memory usage.

use crate::providers::VisionProvider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

/// Benchmark result for a single operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Provider name
    pub provider: String,
    /// Number of iterations
    pub iterations: usize,
    /// Minimum latency (milliseconds)
    pub min_ms: f64,
    /// Maximum latency (milliseconds)
    pub max_ms: f64,
    /// Mean latency (milliseconds)
    pub mean_ms: f64,
    /// Median latency (milliseconds)
    pub median_ms: f64,
    /// Standard deviation (milliseconds)
    pub std_dev_ms: f64,
    /// Percentile 95 (milliseconds)
    pub p95_ms: f64,
    /// Percentile 99 (milliseconds)
    pub p99_ms: f64,
    /// Total time (milliseconds)
    pub total_ms: f64,
    /// Operations per second
    pub ops_per_sec: f64,
}

impl BenchmarkResult {
    /// Create from timing measurements.
    pub fn from_timings(provider: String, mut timings: Vec<f64>) -> Self {
        if timings.is_empty() {
            return Self::empty(provider);
        }

        timings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let iterations = timings.len();
        let min_ms = timings[0];
        let max_ms = timings[timings.len() - 1];
        let total_ms: f64 = timings.iter().sum();
        let mean_ms = total_ms / iterations as f64;

        let median_ms = if iterations.is_multiple_of(2) {
            (timings[iterations / 2 - 1] + timings[iterations / 2]) / 2.0
        } else {
            timings[iterations / 2]
        };

        let variance: f64 = timings
            .iter()
            .map(|t| {
                let diff = t - mean_ms;
                diff * diff
            })
            .sum::<f64>()
            / iterations as f64;
        let std_dev_ms = variance.sqrt();

        let p95_idx = ((iterations as f64 * 0.95) as usize).min(iterations - 1);
        let p99_idx = ((iterations as f64 * 0.99) as usize).min(iterations - 1);

        let p95_ms = timings[p95_idx];
        let p99_ms = timings[p99_idx];

        let ops_per_sec = if mean_ms > 0.0 { 1000.0 / mean_ms } else { 0.0 };

        Self {
            provider,
            iterations,
            min_ms,
            max_ms,
            mean_ms,
            median_ms,
            std_dev_ms,
            p95_ms,
            p99_ms,
            total_ms,
            ops_per_sec,
        }
    }

    /// Create an empty result.
    fn empty(provider: String) -> Self {
        Self {
            provider,
            iterations: 0,
            min_ms: 0.0,
            max_ms: 0.0,
            mean_ms: 0.0,
            median_ms: 0.0,
            std_dev_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            total_ms: 0.0,
            ops_per_sec: 0.0,
        }
    }

    /// Format as a table row.
    pub fn format_row(&self) -> String {
        format!(
            "{:<12} {:>8} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2}",
            self.provider,
            self.iterations,
            self.mean_ms,
            self.median_ms,
            self.p95_ms,
            self.p99_ms,
            self.ops_per_sec
        )
    }
}

/// Comparison report for multiple providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// Benchmark results for each provider
    pub results: Vec<BenchmarkResult>,
    /// Test image size in bytes
    pub image_size_bytes: usize,
    /// Report timestamp
    pub timestamp: String,
}

impl ComparisonReport {
    /// Create a new comparison report.
    pub fn new(results: Vec<BenchmarkResult>, image_size_bytes: usize) -> Self {
        Self {
            results,
            image_size_bytes,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Format as a readable table.
    pub fn format_table(&self) -> String {
        let mut output = String::new();

        output.push('\n');
        output.push_str("═══════════════════════════════════════════════════════════════════\n");
        output.push_str("  PROVIDER PERFORMANCE COMPARISON\n");
        output.push_str("═══════════════════════════════════════════════════════════════════\n");
        output.push_str(&format!("Image Size: {} bytes\n", self.image_size_bytes));
        output.push_str(&format!("Timestamp:  {}\n", self.timestamp));
        output.push_str("───────────────────────────────────────────────────────────────────\n");
        output.push_str(&format!(
            "{:<12} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}\n",
            "Provider", "Iters", "Mean", "Median", "P95", "P99", "Ops/sec"
        ));
        output.push_str(&format!(
            "{:<12} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}\n",
            "", "", "(ms)", "(ms)", "(ms)", "(ms)", ""
        ));
        output.push_str("───────────────────────────────────────────────────────────────────\n");

        for result in &self.results {
            output.push_str(&result.format_row());
            output.push('\n');
        }

        output.push_str("═══════════════════════════════════════════════════════════════════\n");

        output
    }

    /// Get the fastest provider.
    pub fn fastest_provider(&self) -> Option<&BenchmarkResult> {
        self.results
            .iter()
            .min_by(|a, b| a.mean_ms.partial_cmp(&b.mean_ms).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Save report as JSON.
    pub fn save_json(&self, path: &str) -> crate::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            crate::VisionError::config(format!("Failed to serialize report: {}", e))
        })?;

        std::fs::write(path, json)
            .map_err(|e| crate::VisionError::config(format!("Failed to write report: {}", e)))?;

        Ok(())
    }
}

/// Memory profiling result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProfile {
    /// Provider name
    pub provider: String,
    /// Peak memory usage (bytes)
    pub peak_bytes: usize,
    /// Average memory usage (bytes)
    pub avg_bytes: usize,
    /// Memory usage per operation (bytes)
    pub bytes_per_op: usize,
}

impl MemoryProfile {
    /// Format as human-readable string.
    pub fn format(&self) -> String {
        format!(
            "Provider: {}\n  Peak: {} MB\n  Average: {} MB\n  Per Operation: {} KB",
            self.provider,
            self.peak_bytes / (1024 * 1024),
            self.avg_bytes / (1024 * 1024),
            self.bytes_per_op / 1024
        )
    }
}

/// Benchmark runner for comparing providers.
pub struct BenchmarkRunner {
    iterations: usize,
    warmup_iterations: usize,
}

impl Default for BenchmarkRunner {
    fn default() -> Self {
        Self {
            iterations: 10,
            warmup_iterations: 2,
        }
    }
}

impl BenchmarkRunner {
    /// Create a new benchmark runner.
    pub fn new(iterations: usize) -> Self {
        Self {
            iterations,
            warmup_iterations: iterations.min(2),
        }
    }

    /// Set warmup iterations.
    pub fn with_warmup(mut self, warmup: usize) -> Self {
        self.warmup_iterations = warmup;
        self
    }

    /// Run benchmark for a single provider.
    pub async fn benchmark_provider(
        &self,
        provider: Arc<dyn VisionProvider>,
        image_data: &[u8],
    ) -> crate::Result<BenchmarkResult> {
        let provider_name = provider.provider_name().to_string();

        // Warmup
        for _ in 0..self.warmup_iterations {
            let _ = provider.process_image(image_data).await;
        }

        // Benchmark
        let mut timings = Vec::with_capacity(self.iterations);

        for _ in 0..self.iterations {
            let start = Instant::now();
            let _ = provider.process_image(image_data).await?;
            let elapsed = start.elapsed();
            timings.push(elapsed.as_secs_f64() * 1000.0);
        }

        Ok(BenchmarkResult::from_timings(provider_name, timings))
    }

    /// Compare multiple providers.
    pub async fn compare_providers(
        &self,
        providers: Vec<Arc<dyn VisionProvider>>,
        image_data: &[u8],
    ) -> crate::Result<ComparisonReport> {
        let mut results = Vec::new();

        for provider in providers {
            let result = self.benchmark_provider(provider, image_data).await?;
            results.push(result);
        }

        Ok(ComparisonReport::new(results, image_data.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_result_from_timings() {
        let timings = vec![10.0, 20.0, 15.0, 25.0, 12.0];
        let result = BenchmarkResult::from_timings("test".to_string(), timings);

        assert_eq!(result.provider, "test");
        assert_eq!(result.iterations, 5);
        assert_eq!(result.min_ms, 10.0);
        assert_eq!(result.max_ms, 25.0);
        assert!((result.mean_ms - 16.4).abs() < 0.1);
    }

    #[test]
    fn test_benchmark_result_empty() {
        let result = BenchmarkResult::from_timings("test".to_string(), vec![]);
        assert_eq!(result.iterations, 0);
        assert_eq!(result.mean_ms, 0.0);
    }

    #[test]
    fn test_comparison_report() {
        let results = vec![
            BenchmarkResult::from_timings("provider1".to_string(), vec![10.0, 12.0, 11.0]),
            BenchmarkResult::from_timings("provider2".to_string(), vec![20.0, 22.0, 21.0]),
        ];

        let report = ComparisonReport::new(results, 1024);
        assert_eq!(report.results.len(), 2);
        assert_eq!(report.image_size_bytes, 1024);

        let fastest = report.fastest_provider().unwrap();
        assert_eq!(fastest.provider, "provider1");
    }

    #[test]
    fn test_benchmark_result_format_row() {
        let result = BenchmarkResult::from_timings("test".to_string(), vec![10.0, 20.0, 15.0]);
        let row = result.format_row();
        assert!(row.contains("test"));
        assert!(row.contains("3")); // iterations
    }

    #[test]
    fn test_comparison_report_format_table() {
        let results = vec![BenchmarkResult::from_timings(
            "mock".to_string(),
            vec![5.0, 6.0, 5.5],
        )];

        let report = ComparisonReport::new(results, 1024);
        let table = report.format_table();
        assert!(table.contains("PROVIDER PERFORMANCE COMPARISON"));
        assert!(table.contains("mock"));
    }

    #[test]
    fn test_benchmark_runner_creation() {
        let runner = BenchmarkRunner::new(20);
        assert_eq!(runner.iterations, 20);

        let runner = runner.with_warmup(5);
        assert_eq!(runner.warmup_iterations, 5);
    }

    #[test]
    fn test_memory_profile_format() {
        let profile = MemoryProfile {
            provider: "test".to_string(),
            peak_bytes: 100 * 1024 * 1024,
            avg_bytes: 80 * 1024 * 1024,
            bytes_per_op: 512 * 1024,
        };

        let formatted = profile.format();
        assert!(formatted.contains("test"));
        assert!(formatted.contains("100 MB"));
        assert!(formatted.contains("80 MB"));
        assert!(formatted.contains("512 KB"));
    }
}
