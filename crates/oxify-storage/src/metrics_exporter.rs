//! Prometheus Metrics Exporter
//!
//! Exports storage layer metrics in Prometheus format for monitoring and observability.
//!
//! ## Overview
//!
//! This module provides a unified interface for exporting metrics from all storage
//! components in Prometheus-compatible format.
//!
//! ## Metrics Categories
//!
//! 1. **Connection Pool**: Active connections, idle connections, utilization
//! 2. **Cache Performance**: Hit rates, evictions, size for all cache types
//! 3. **Query Performance**: Query durations, slow queries, errors
//! 4. **Database Health**: Table sizes, index bloat, replication lag
//! 5. **Batch Operations**: Throughput, errors, duration
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{MetricsExporter, MetricsFormat};
//!
//! let exporter = MetricsExporter::new(pool, cache, vector_cache, jwks_cache);
//!
//! // Export in Prometheus format
//! let metrics = exporter.export(MetricsFormat::Prometheus);
//! println!("{}", metrics);
//!
//! // Or get structured metrics
//! let structured = exporter.export_structured();
//! for (name, value, labels) in structured {
//!     println!("{}{{{}}}: {}", name, labels, value);
//! }
//! ```
//!
//! ## Integration
//!
//! ### With Axum
//! ```ignore
//! async fn metrics_handler(
//!     State(exporter): State<Arc<MetricsExporter>>,
//! ) -> impl IntoResponse {
//!     let metrics = exporter.export(MetricsFormat::Prometheus);
//!     (StatusCode::OK, metrics)
//! }
//! ```
//!
//! ### With Prometheus Client
//! ```ignore
//! // Set gauges from storage metrics
//! for (name, value) in exporter.export_flat() {
//!     prometheus::gauge(format!("oxify_storage_{}", name), value);
//! }
//! ```

use crate::{Cache, CacheStats, DatabasePool, PoolHealth};

// Stub types for disabled modules
pub struct VectorCache;
pub struct JwksCache;

// Stub stats/metrics types
#[derive(Debug, Clone, Default)]
pub struct VectorCacheStats {
    data: std::collections::HashMap<String, f64>,
}

impl VectorCacheStats {
    pub fn get(&self, key: &str) -> Option<&f64> {
        self.data.get(key)
    }
}

#[derive(Debug, Clone, Default)]
pub struct VectorCacheMetrics {
    pub hits: u64,
    pub misses: u64,
}

impl VectorCacheMetrics {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct JwksCacheStats {
    data: std::collections::HashMap<String, f64>,
}

impl JwksCacheStats {
    pub fn get(&self, key: &str) -> Option<&f64> {
        self.data.get(key)
    }
}

#[derive(Debug, Clone, Default)]
pub struct JwksCacheMetrics {
    pub key_hits: u64,
    pub key_misses: u64,
}

impl JwksCacheMetrics {
    pub fn key_hit_rate(&self) -> f64 {
        let total = self.key_hits + self.key_misses;
        if total == 0 {
            0.0
        } else {
            self.key_hits as f64 / total as f64
        }
    }
}

impl VectorCache {
    pub fn stats(&self) -> VectorCacheStats {
        VectorCacheStats::default()
    }
    pub fn metrics(&self) -> VectorCacheMetrics {
        VectorCacheMetrics::default()
    }
}

impl JwksCache {
    pub fn stats(&self) -> JwksCacheStats {
        JwksCacheStats::default()
    }
    pub fn metrics(&self) -> JwksCacheMetrics {
        JwksCacheMetrics::default()
    }
}
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::sync::Arc;

/// Metrics export format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricsFormat {
    /// Prometheus text format
    Prometheus,
    /// JSON format
    Json,
    /// Flat key-value pairs
    Flat,
}

/// Metric type in Prometheus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    Gauge,
    Counter,
    Histogram,
}

impl MetricType {
    fn as_str(&self) -> &'static str {
        match self {
            MetricType::Gauge => "gauge",
            MetricType::Counter => "counter",
            MetricType::Histogram => "histogram",
        }
    }
}

/// A single metric with metadata
#[derive(Debug, Clone)]
pub struct Metric {
    /// Metric name (e.g., "oxify_storage_pool_size")
    pub name: String,
    /// Metric type
    pub metric_type: MetricType,
    /// Help text
    pub help: String,
    /// Metric value
    pub value: f64,
    /// Labels (e.g., {"cache_type": "workflow"})
    pub labels: HashMap<String, String>,
}

impl Metric {
    fn new(
        name: impl Into<String>,
        metric_type: MetricType,
        help: impl Into<String>,
        value: f64,
    ) -> Self {
        Self {
            name: name.into(),
            metric_type,
            help: help.into(),
            value,
            labels: HashMap::new(),
        }
    }

    fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Format as Prometheus text
    fn to_prometheus(&self) -> String {
        let mut output = String::new();

        // Type and help
        writeln!(&mut output, "# HELP {} {}", self.name, self.help).unwrap();
        writeln!(
            &mut output,
            "# TYPE {} {}",
            self.name,
            self.metric_type.as_str()
        )
        .unwrap();

        // Metric value with labels
        if self.labels.is_empty() {
            writeln!(&mut output, "{} {}", self.name, self.value).unwrap();
        } else {
            let labels: Vec<String> = self
                .labels
                .iter()
                .map(|(k, v)| format!("{k}=\"{v}\""))
                .collect();
            writeln!(
                &mut output,
                "{}{{{}}} {}",
                self.name,
                labels.join(","),
                self.value
            )
            .unwrap();
        }

        output
    }
}

/// Prometheus metrics exporter
pub struct MetricsExporter {
    pool: DatabasePool,
    cache: Option<Arc<Cache>>,
    vector_cache: Option<Arc<VectorCache>>,
    jwks_cache: Option<Arc<JwksCache>>,
}

impl MetricsExporter {
    /// Create a new metrics exporter
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            cache: None,
            vector_cache: None,
            jwks_cache: None,
        }
    }

    /// Add cache for metrics collection
    pub fn with_cache(mut self, cache: Arc<Cache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Add vector cache for metrics collection
    pub fn with_vector_cache(mut self, vector_cache: Arc<VectorCache>) -> Self {
        self.vector_cache = Some(vector_cache);
        self
    }

    /// Add JWKS cache for metrics collection
    pub fn with_jwks_cache(mut self, jwks_cache: Arc<JwksCache>) -> Self {
        self.jwks_cache = Some(jwks_cache);
        self
    }

    /// Collect all metrics
    pub fn collect_metrics(&self) -> Vec<Metric> {
        let mut metrics = Vec::new();

        // Connection pool metrics
        metrics.extend(self.collect_pool_metrics());

        // Cache metrics
        if let Some(ref cache) = self.cache {
            metrics.extend(self.collect_cache_metrics(cache));
        }

        // Vector cache metrics
        if let Some(ref vector_cache) = self.vector_cache {
            metrics.extend(self.collect_vector_cache_metrics(vector_cache));
        }

        // JWKS cache metrics
        if let Some(ref jwks_cache) = self.jwks_cache {
            metrics.extend(self.collect_jwks_cache_metrics(jwks_cache));
        }

        metrics
    }

    /// Collect connection pool metrics
    fn collect_pool_metrics(&self) -> Vec<Metric> {
        let stats = self.pool.stats();
        let health = self.pool.health_status();

        vec![
            Metric::new(
                "oxify_storage_pool_size",
                MetricType::Gauge,
                "Current size of the connection pool",
                f64::from(stats.size),
            ),
            Metric::new(
                "oxify_storage_pool_idle",
                MetricType::Gauge,
                "Number of idle connections",
                stats.num_idle as f64,
            ),
            Metric::new(
                "oxify_storage_pool_active",
                MetricType::Gauge,
                "Number of active connections",
                stats.active_connections() as f64,
            ),
            Metric::new(
                "oxify_storage_pool_max",
                MetricType::Gauge,
                "Maximum connections allowed",
                f64::from(stats.max_connections),
            ),
            Metric::new(
                "oxify_storage_pool_utilization",
                MetricType::Gauge,
                "Pool utilization (0.0 to 1.0)",
                stats.utilization(),
            ),
            Metric::new(
                "oxify_storage_pool_health",
                MetricType::Gauge,
                "Pool health status (0=critical, 1=degraded, 2=healthy)",
                match health {
                    PoolHealth::Critical => 0.0,
                    PoolHealth::Degraded => 1.0,
                    PoolHealth::Healthy => 2.0,
                },
            ),
        ]
    }

    /// Collect cache metrics
    fn collect_cache_metrics(&self, cache: &Cache) -> Vec<Metric> {
        let stats = cache.stats();
        let metrics_data = cache.metrics();

        let workflow_stats = stats.get("workflows").cloned().unwrap_or(CacheStats {
            size: 0,
            capacity: 0,
            valid_entries: 0,
            expired_entries: 0,
            total_accesses: 0,
        });

        vec![
            Metric::new(
                "oxify_storage_cache_size",
                MetricType::Gauge,
                "Current cache size",
                workflow_stats.size as f64,
            )
            .with_label("cache_type", "workflow"),
            Metric::new(
                "oxify_storage_cache_hits",
                MetricType::Counter,
                "Cache hit count",
                metrics_data.workflow_hits as f64,
            )
            .with_label("cache_type", "workflow"),
            Metric::new(
                "oxify_storage_cache_misses",
                MetricType::Counter,
                "Cache miss count",
                metrics_data.workflow_misses as f64,
            )
            .with_label("cache_type", "workflow"),
            Metric::new(
                "oxify_storage_cache_hit_rate",
                MetricType::Gauge,
                "Cache hit rate (0.0 to 1.0)",
                metrics_data.overall_hit_rate(),
            )
            .with_label("cache_type", "workflow"),
            Metric::new(
                "oxify_storage_cache_evictions",
                MetricType::Counter,
                "Cache eviction count",
                metrics_data.evictions as f64,
            )
            .with_label("cache_type", "workflow"),
        ]
    }

    /// Collect vector cache metrics
    fn collect_vector_cache_metrics(&self, vector_cache: &VectorCache) -> Vec<Metric> {
        let stats = vector_cache.stats();
        let metrics_data = vector_cache.metrics();

        vec![
            Metric::new(
                "oxify_storage_cache_size",
                MetricType::Gauge,
                "Current cache size",
                stats.get("size").copied().unwrap_or(0.0),
            )
            .with_label("cache_type", "vector"),
            Metric::new(
                "oxify_storage_cache_hits",
                MetricType::Counter,
                "Cache hit count",
                metrics_data.hits as f64,
            )
            .with_label("cache_type", "vector"),
            Metric::new(
                "oxify_storage_cache_misses",
                MetricType::Counter,
                "Cache miss count",
                metrics_data.misses as f64,
            )
            .with_label("cache_type", "vector"),
            Metric::new(
                "oxify_storage_cache_hit_rate",
                MetricType::Gauge,
                "Cache hit rate (0.0 to 1.0)",
                metrics_data.hit_rate(),
            )
            .with_label("cache_type", "vector"),
        ]
    }

    /// Collect JWKS cache metrics
    fn collect_jwks_cache_metrics(&self, jwks_cache: &JwksCache) -> Vec<Metric> {
        let stats = jwks_cache.stats();
        let metrics_data = jwks_cache.metrics();

        vec![
            Metric::new(
                "oxify_storage_cache_size",
                MetricType::Gauge,
                "Current cache size",
                stats.get("issuers").copied().unwrap_or(0.0),
            )
            .with_label("cache_type", "jwks"),
            Metric::new(
                "oxify_storage_cache_hits",
                MetricType::Counter,
                "Cache hit count",
                metrics_data.key_hits as f64,
            )
            .with_label("cache_type", "jwks"),
            Metric::new(
                "oxify_storage_cache_misses",
                MetricType::Counter,
                "Cache miss count",
                metrics_data.key_misses as f64,
            )
            .with_label("cache_type", "jwks"),
            Metric::new(
                "oxify_storage_cache_hit_rate",
                MetricType::Gauge,
                "Cache hit rate (0.0 to 1.0)",
                metrics_data.key_hit_rate(),
            )
            .with_label("cache_type", "jwks"),
        ]
    }

    /// Export metrics in specified format
    pub fn export(&self, format: MetricsFormat) -> String {
        let metrics = self.collect_metrics();

        match format {
            MetricsFormat::Prometheus => {
                let mut output = String::new();
                for metric in metrics {
                    output.push_str(&metric.to_prometheus());
                }
                output
            }
            MetricsFormat::Json => {
                let json: Vec<serde_json::Value> = metrics
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "name": m.name,
                            "type": m.metric_type.as_str(),
                            "help": m.help,
                            "value": m.value,
                            "labels": m.labels,
                        })
                    })
                    .collect();
                serde_json::to_string_pretty(&json).unwrap()
            }
            MetricsFormat::Flat => {
                let mut output = String::new();
                for metric in metrics {
                    writeln!(&mut output, "{}: {}", metric.name, metric.value).unwrap();
                }
                output
            }
        }
    }

    /// Export metrics as flat key-value pairs
    pub fn export_flat(&self) -> HashMap<String, f64> {
        let metrics = self.collect_metrics();
        let mut flat = HashMap::new();

        for metric in metrics {
            if metric.labels.is_empty() {
                flat.insert(metric.name, metric.value);
            } else {
                let labels: Vec<String> = metric
                    .labels
                    .iter()
                    .map(|(k, v)| format!("{k}_{v}"))
                    .collect();
                let key = format!("{}_{}", metric.name, labels.join("_"));
                flat.insert(key, metric.value);
            }
        }

        flat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_creation() {
        let metric = Metric::new("test_metric", MetricType::Gauge, "Test metric", 42.0)
            .with_label("env", "test")
            .with_label("version", "1.0");

        assert_eq!(metric.name, "test_metric");
        assert_eq!(metric.value, 42.0);
        assert_eq!(metric.labels.len(), 2);
    }

    #[test]
    fn test_prometheus_format_no_labels() {
        let metric = Metric::new("test_gauge", MetricType::Gauge, "A test gauge", 123.45);

        let output = metric.to_prometheus();
        assert!(output.contains("# HELP test_gauge A test gauge"));
        assert!(output.contains("# TYPE test_gauge gauge"));
        assert!(output.contains("test_gauge 123.45"));
    }

    #[test]
    fn test_prometheus_format_with_labels() {
        let metric = Metric::new("test_counter", MetricType::Counter, "A test counter", 456.0)
            .with_label("env", "prod")
            .with_label("region", "us-east");

        let output = metric.to_prometheus();
        assert!(output.contains("# HELP test_counter A test counter"));
        assert!(output.contains("# TYPE test_counter counter"));
        assert!(output.contains("test_counter{"));
        assert!(output.contains("456"));
    }

    #[test]
    fn test_metric_type_as_str() {
        assert_eq!(MetricType::Gauge.as_str(), "gauge");
        assert_eq!(MetricType::Counter.as_str(), "counter");
        assert_eq!(MetricType::Histogram.as_str(), "histogram");
    }
}
