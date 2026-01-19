//! Query performance profiling and slow query tracking
//!
//! This module provides tools for monitoring and profiling database query performance.
//! It helps identify slow queries, track query patterns, and optimize database access.
//!
//! # Features
//!
//! - **Query Timing**: Track execution time for all queries
//! - **Slow Query Detection**: Automatically flag queries exceeding thresholds
//! - **Query Statistics**: Aggregate stats (count, avg, min, max, p95)
//! - **Query Fingerprinting**: Group similar queries for analysis
//! - **Top N Queries**: Identify most time-consuming queries
//! - **Query Logging**: Log slow queries for debugging
//! - **Integration**: Easy integration with existing code
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, QueryProfiler, QueryProfilerConfig};
//! use std::time::Duration;
//!
//! let pool = DatabasePool::new(config).await?;
//! let profiler_config = QueryProfilerConfig {
//!     slow_query_threshold: Duration::from_millis(100),
//!     enable_logging: true,
//!     ..Default::default()
//! };
//! let profiler = QueryProfiler::new(profiler_config);
//!
//! // Profile a query
//! let start = std::time::Instant::now();
//! let result = sqlx::query("SELECT * FROM workflows WHERE user_id = $1")
//!     .bind(&user_id)
//!     .fetch_all(pool.pool())
//!     .await?;
//! profiler.record_query(
//!     "SELECT * FROM workflows WHERE user_id = ?",
//!     start.elapsed(),
//! ).await;
//!
//! // Get statistics
//! let stats = profiler.get_stats().await;
//! println!("Total queries: {}", stats.total_queries);
//! println!("Slow queries: {}", stats.slow_queries);
//! println!("Average duration: {:?}", stats.avg_duration);
//!
//! // Get slow queries
//! let slow = profiler.get_slow_queries(10).await;
//! for query in slow {
//!     println!("Slow query: {} ({:?})", query.fingerprint, query.avg_duration);
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Query profiler configuration
#[derive(Debug, Clone)]
pub struct QueryProfilerConfig {
    /// Threshold for considering a query slow
    pub slow_query_threshold: Duration,
    /// Enable logging of slow queries
    pub enable_logging: bool,
    /// Maximum number of query records to keep in memory
    pub max_records: usize,
    /// Enable query fingerprinting
    pub enable_fingerprinting: bool,
}

impl Default for QueryProfilerConfig {
    fn default() -> Self {
        Self {
            slow_query_threshold: Duration::from_millis(100),
            enable_logging: true,
            max_records: 10_000,
            enable_fingerprinting: true,
        }
    }
}

/// Query execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRecord {
    /// Query fingerprint (normalized query)
    pub fingerprint: String,
    /// Execution duration
    pub duration: Duration,
    /// Timestamp of execution
    pub timestamp: i64,
    /// Whether this query exceeded slow threshold
    pub is_slow: bool,
}

/// Query statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStats {
    /// Total number of queries executed
    pub total_queries: u64,
    /// Number of slow queries
    pub slow_queries: u64,
    /// Total execution time across all queries
    pub total_duration: Duration,
    /// Average query duration
    pub avg_duration: Duration,
    /// Minimum query duration
    pub min_duration: Duration,
    /// Maximum query duration
    pub max_duration: Duration,
    /// 95th percentile duration
    pub p95_duration: Duration,
}

impl Default for QueryStats {
    fn default() -> Self {
        Self {
            total_queries: 0,
            slow_queries: 0,
            total_duration: Duration::ZERO,
            avg_duration: Duration::ZERO,
            min_duration: Duration::MAX,
            max_duration: Duration::ZERO,
            p95_duration: Duration::ZERO,
        }
    }
}

/// Query aggregation data
#[derive(Debug, Clone)]
struct QueryAggregation {
    count: u64,
    total_duration: Duration,
    min_duration: Duration,
    max_duration: Duration,
    durations: Vec<u64>, // For percentile calculation
}

impl QueryAggregation {
    fn new() -> Self {
        Self {
            count: 0,
            total_duration: Duration::ZERO,
            min_duration: Duration::MAX,
            max_duration: Duration::ZERO,
            durations: Vec::new(),
        }
    }

    fn add_duration(&mut self, duration: Duration) {
        self.count += 1;
        self.total_duration += duration;
        self.min_duration = self.min_duration.min(duration);
        self.max_duration = self.max_duration.max(duration);
        self.durations.push(duration.as_micros() as u64);
    }

    fn avg_duration(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total_duration / self.count as u32
        }
    }

    fn p95_duration(&self) -> Duration {
        if self.durations.is_empty() {
            return Duration::ZERO;
        }

        let mut sorted = self.durations.clone();
        sorted.sort_unstable();

        let index = (sorted.len() as f64 * 0.95) as usize;
        let p95_micros = sorted
            .get(index.min(sorted.len() - 1))
            .copied()
            .unwrap_or(0);
        Duration::from_micros(p95_micros)
    }
}

/// Query information for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryInfo {
    /// Query fingerprint
    pub fingerprint: String,
    /// Number of executions
    pub count: u64,
    /// Total duration across all executions
    pub total_duration: Duration,
    /// Average duration
    pub avg_duration: Duration,
    /// Minimum duration
    pub min_duration: Duration,
    /// Maximum duration
    pub max_duration: Duration,
    /// 95th percentile duration
    pub p95_duration: Duration,
}

/// Query profiler
pub struct QueryProfiler {
    config: QueryProfilerConfig,
    records: Arc<RwLock<Vec<QueryRecord>>>,
    aggregations: Arc<RwLock<HashMap<String, QueryAggregation>>>,
}

impl QueryProfiler {
    /// Create a new query profiler
    pub fn new(config: QueryProfilerConfig) -> Self {
        Self {
            config,
            records: Arc::new(RwLock::new(Vec::new())),
            aggregations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a query execution
    pub async fn record_query(&self, query: &str, duration: Duration) {
        let fingerprint = if self.config.enable_fingerprinting {
            self.fingerprint_query(query)
        } else {
            query.to_string()
        };

        let is_slow = duration >= self.config.slow_query_threshold;

        if is_slow && self.config.enable_logging {
            tracing::warn!(
                query = fingerprint,
                duration_ms = duration.as_millis(),
                "Slow query detected"
            );
        }

        let record = QueryRecord {
            fingerprint: fingerprint.clone(),
            duration,
            timestamp: chrono::Utc::now().timestamp(),
            is_slow,
        };

        // Add to records
        let mut records = self.records.write().await;
        records.push(record);

        // Limit memory usage
        if records.len() > self.config.max_records {
            let drain_count = records.len() / 2;
            records.drain(0..drain_count); // Remove oldest half
        }
        drop(records);

        // Update aggregations
        let mut aggregations = self.aggregations.write().await;
        aggregations
            .entry(fingerprint)
            .or_insert_with(QueryAggregation::new)
            .add_duration(duration);
    }

    /// Get overall query statistics
    pub async fn get_stats(&self) -> QueryStats {
        let records = self.records.read().await;

        if records.is_empty() {
            return QueryStats::default();
        }

        let total_queries = records.len() as u64;
        let slow_queries = records.iter().filter(|r| r.is_slow).count() as u64;

        let mut durations: Vec<Duration> = records.iter().map(|r| r.duration).collect();
        durations.sort();

        let total_duration: Duration = durations.iter().sum();
        let avg_duration = total_duration / total_queries as u32;

        let min_duration = durations.first().copied().unwrap_or(Duration::ZERO);
        let max_duration = durations.last().copied().unwrap_or(Duration::ZERO);

        let p95_index = ((durations.len() as f64) * 0.95) as usize;
        let p95_duration = durations
            .get(p95_index.min(durations.len() - 1))
            .copied()
            .unwrap_or(Duration::ZERO);

        QueryStats {
            total_queries,
            slow_queries,
            total_duration,
            avg_duration,
            min_duration,
            max_duration,
            p95_duration,
        }
    }

    /// Get slow queries (queries exceeding threshold)
    pub async fn get_slow_queries(&self, limit: usize) -> Vec<QueryInfo> {
        let aggregations = self.aggregations.read().await;

        let mut queries: Vec<QueryInfo> = aggregations
            .iter()
            .map(|(fingerprint, agg)| QueryInfo {
                fingerprint: fingerprint.clone(),
                count: agg.count,
                total_duration: agg.total_duration,
                avg_duration: agg.avg_duration(),
                min_duration: agg.min_duration,
                max_duration: agg.max_duration,
                p95_duration: agg.p95_duration(),
            })
            .filter(|q| q.avg_duration >= self.config.slow_query_threshold)
            .collect();

        // Sort by average duration (slowest first)
        queries.sort_by(|a, b| b.avg_duration.cmp(&a.avg_duration));

        queries.into_iter().take(limit).collect()
    }

    /// Get top N queries by total execution time
    pub async fn get_top_queries(&self, limit: usize) -> Vec<QueryInfo> {
        let aggregations = self.aggregations.read().await;

        let mut queries: Vec<QueryInfo> = aggregations
            .iter()
            .map(|(fingerprint, agg)| QueryInfo {
                fingerprint: fingerprint.clone(),
                count: agg.count,
                total_duration: agg.total_duration,
                avg_duration: agg.avg_duration(),
                min_duration: agg.min_duration,
                max_duration: agg.max_duration,
                p95_duration: agg.p95_duration(),
            })
            .collect();

        // Sort by total duration (most time-consuming first)
        queries.sort_by(|a, b| b.total_duration.cmp(&a.total_duration));

        queries.into_iter().take(limit).collect()
    }

    /// Get query statistics for a specific query fingerprint
    pub async fn get_query_stats(&self, fingerprint: &str) -> Option<QueryInfo> {
        let aggregations = self.aggregations.read().await;

        aggregations.get(fingerprint).map(|agg| QueryInfo {
            fingerprint: fingerprint.to_string(),
            count: agg.count,
            total_duration: agg.total_duration,
            avg_duration: agg.avg_duration(),
            min_duration: agg.min_duration,
            max_duration: agg.max_duration,
            p95_duration: agg.p95_duration(),
        })
    }

    /// Clear all profiling data
    pub async fn clear(&self) {
        self.records.write().await.clear();
        self.aggregations.write().await.clear();
    }

    /// Export profiling data for monitoring systems
    pub async fn export_metrics(&self) -> HashMap<String, f64> {
        let stats = self.get_stats().await;
        let mut metrics = HashMap::new();

        metrics.insert("total_queries".to_string(), stats.total_queries as f64);
        metrics.insert("slow_queries".to_string(), stats.slow_queries as f64);
        metrics.insert(
            "slow_query_ratio".to_string(),
            if stats.total_queries > 0 {
                stats.slow_queries as f64 / stats.total_queries as f64
            } else {
                0.0
            },
        );
        metrics.insert(
            "avg_duration_ms".to_string(),
            stats.avg_duration.as_secs_f64() * 1000.0,
        );
        metrics.insert(
            "p95_duration_ms".to_string(),
            stats.p95_duration.as_secs_f64() * 1000.0,
        );
        metrics.insert(
            "max_duration_ms".to_string(),
            stats.max_duration.as_secs_f64() * 1000.0,
        );

        metrics
    }

    // Helper methods

    fn fingerprint_query(&self, query: &str) -> String {
        // Normalize query by replacing values with placeholders
        let mut fingerprint = query.to_lowercase();

        // Replace string literals
        fingerprint = regex::Regex::new(r"'[^']*'")
            .unwrap()
            .replace_all(&fingerprint, "'?'")
            .to_string();

        // Replace numeric literals
        fingerprint = regex::Regex::new(r"\b\d+\b")
            .unwrap()
            .replace_all(&fingerprint, "?")
            .to_string();

        // Replace UUIDs
        fingerprint =
            regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                .unwrap()
                .replace_all(&fingerprint, "?")
                .to_string();

        // Normalize whitespace
        fingerprint = regex::Regex::new(r"\s+")
            .unwrap()
            .replace_all(&fingerprint, " ")
            .trim()
            .to_string();

        fingerprint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = QueryProfilerConfig::default();
        assert_eq!(config.slow_query_threshold, Duration::from_millis(100));
        assert!(config.enable_logging);
        assert_eq!(config.max_records, 10_000);
    }

    #[tokio::test]
    async fn test_record_query() {
        let profiler = QueryProfiler::new(QueryProfilerConfig::default());

        profiler
            .record_query("SELECT * FROM users", Duration::from_millis(50))
            .await;

        let stats = profiler.get_stats().await;
        assert_eq!(stats.total_queries, 1);
        assert_eq!(stats.slow_queries, 0);
    }

    #[tokio::test]
    async fn test_slow_query_detection() {
        let config = QueryProfilerConfig {
            slow_query_threshold: Duration::from_millis(100),
            ..Default::default()
        };
        let profiler = QueryProfiler::new(config);

        profiler
            .record_query("SELECT * FROM users", Duration::from_millis(150))
            .await;

        let stats = profiler.get_stats().await;
        assert_eq!(stats.total_queries, 1);
        assert_eq!(stats.slow_queries, 1);
    }

    #[tokio::test]
    async fn test_query_aggregation() {
        let profiler = QueryProfiler::new(QueryProfilerConfig::default());

        profiler
            .record_query("SELECT * FROM users", Duration::from_millis(50))
            .await;
        profiler
            .record_query("SELECT * FROM users", Duration::from_millis(70))
            .await;

        let top = profiler.get_top_queries(10).await;
        assert!(!top.is_empty());
        assert_eq!(top[0].count, 2);
    }

    #[test]
    fn test_query_fingerprinting() {
        let profiler = QueryProfiler::new(QueryProfilerConfig::default());

        let fp1 = profiler.fingerprint_query("SELECT * FROM users WHERE id = 123");
        let fp2 = profiler.fingerprint_query("SELECT * FROM users WHERE id = 456");
        assert_eq!(fp1, fp2); // Should be the same after normalization

        let fp3 = profiler.fingerprint_query("SELECT * FROM users WHERE name = 'John'");
        let fp4 = profiler.fingerprint_query("SELECT * FROM users WHERE name = 'Jane'");
        assert_eq!(fp3, fp4); // Should be the same after normalization
    }
}
