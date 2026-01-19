//! Observability and metrics for vector operations
//!
//! This module provides instrumentation for tracking vector database operations,
//! including latencies, success/failure rates, and operation counts.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Metrics collector for vector operations
#[derive(Debug, Clone)]
pub struct VectorMetrics {
    inner: Arc<VectorMetricsInner>,
}

#[derive(Debug)]
struct VectorMetricsInner {
    // Search metrics
    search_count: AtomicU64,
    search_errors: AtomicU64,
    search_total_duration_ms: AtomicU64,

    // Insert metrics
    insert_count: AtomicU64,
    insert_errors: AtomicU64,
    insert_total_duration_ms: AtomicU64,

    // Delete metrics
    delete_count: AtomicU64,
    delete_errors: AtomicU64,
    delete_total_duration_ms: AtomicU64,

    // Batch insert metrics
    batch_insert_count: AtomicU64,
    batch_insert_errors: AtomicU64,
    batch_insert_total_duration_ms: AtomicU64,
    batch_insert_total_vectors: AtomicU64,

    // Update metrics
    update_count: AtomicU64,
    update_errors: AtomicU64,
    update_total_duration_ms: AtomicU64,
}

impl Default for VectorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            inner: Arc::new(VectorMetricsInner {
                search_count: AtomicU64::new(0),
                search_errors: AtomicU64::new(0),
                search_total_duration_ms: AtomicU64::new(0),
                insert_count: AtomicU64::new(0),
                insert_errors: AtomicU64::new(0),
                insert_total_duration_ms: AtomicU64::new(0),
                delete_count: AtomicU64::new(0),
                delete_errors: AtomicU64::new(0),
                delete_total_duration_ms: AtomicU64::new(0),
                batch_insert_count: AtomicU64::new(0),
                batch_insert_errors: AtomicU64::new(0),
                batch_insert_total_duration_ms: AtomicU64::new(0),
                batch_insert_total_vectors: AtomicU64::new(0),
                update_count: AtomicU64::new(0),
                update_errors: AtomicU64::new(0),
                update_total_duration_ms: AtomicU64::new(0),
            }),
        }
    }

    /// Record a search operation
    pub fn record_search(&self, duration: Duration, success: bool) {
        self.inner.search_count.fetch_add(1, Ordering::Relaxed);
        self.inner
            .search_total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        if !success {
            self.inner.search_errors.fetch_add(1, Ordering::Relaxed);
        }

        tracing::debug!(
            operation = "search",
            duration_ms = duration.as_millis(),
            success = success,
            "Vector search operation completed"
        );
    }

    /// Record an insert operation
    pub fn record_insert(&self, duration: Duration, success: bool) {
        self.inner.insert_count.fetch_add(1, Ordering::Relaxed);
        self.inner
            .insert_total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        if !success {
            self.inner.insert_errors.fetch_add(1, Ordering::Relaxed);
        }

        tracing::debug!(
            operation = "insert",
            duration_ms = duration.as_millis(),
            success = success,
            "Vector insert operation completed"
        );
    }

    /// Record a delete operation
    pub fn record_delete(&self, duration: Duration, success: bool, deleted_count: usize) {
        self.inner.delete_count.fetch_add(1, Ordering::Relaxed);
        self.inner
            .delete_total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        if !success {
            self.inner.delete_errors.fetch_add(1, Ordering::Relaxed);
        }

        tracing::debug!(
            operation = "delete",
            duration_ms = duration.as_millis(),
            success = success,
            deleted_count = deleted_count,
            "Vector delete operation completed"
        );
    }

    /// Record a batch insert operation
    pub fn record_batch_insert(&self, duration: Duration, success: bool, vector_count: usize) {
        self.inner
            .batch_insert_count
            .fetch_add(1, Ordering::Relaxed);
        self.inner
            .batch_insert_total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.inner
            .batch_insert_total_vectors
            .fetch_add(vector_count as u64, Ordering::Relaxed);
        if !success {
            self.inner
                .batch_insert_errors
                .fetch_add(1, Ordering::Relaxed);
        }

        tracing::debug!(
            operation = "batch_insert",
            duration_ms = duration.as_millis(),
            success = success,
            vector_count = vector_count,
            "Vector batch insert operation completed"
        );
    }

    /// Record an update operation
    pub fn record_update(&self, duration: Duration, success: bool) {
        self.inner.update_count.fetch_add(1, Ordering::Relaxed);
        self.inner
            .update_total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        if !success {
            self.inner.update_errors.fetch_add(1, Ordering::Relaxed);
        }

        tracing::debug!(
            operation = "update",
            duration_ms = duration.as_millis(),
            success = success,
            "Vector update operation completed"
        );
    }

    /// Get search metrics
    pub fn search_stats(&self) -> OperationStats {
        let count = self.inner.search_count.load(Ordering::Relaxed);
        let errors = self.inner.search_errors.load(Ordering::Relaxed);
        let total_ms = self.inner.search_total_duration_ms.load(Ordering::Relaxed);

        OperationStats {
            count,
            errors,
            avg_duration_ms: if count > 0 {
                (total_ms as f64) / (count as f64)
            } else {
                0.0
            },
            error_rate: if count > 0 {
                (errors as f64) / (count as f64)
            } else {
                0.0
            },
        }
    }

    /// Get insert metrics
    pub fn insert_stats(&self) -> OperationStats {
        let count = self.inner.insert_count.load(Ordering::Relaxed);
        let errors = self.inner.insert_errors.load(Ordering::Relaxed);
        let total_ms = self.inner.insert_total_duration_ms.load(Ordering::Relaxed);

        OperationStats {
            count,
            errors,
            avg_duration_ms: if count > 0 {
                (total_ms as f64) / (count as f64)
            } else {
                0.0
            },
            error_rate: if count > 0 {
                (errors as f64) / (count as f64)
            } else {
                0.0
            },
        }
    }

    /// Get delete metrics
    pub fn delete_stats(&self) -> OperationStats {
        let count = self.inner.delete_count.load(Ordering::Relaxed);
        let errors = self.inner.delete_errors.load(Ordering::Relaxed);
        let total_ms = self.inner.delete_total_duration_ms.load(Ordering::Relaxed);

        OperationStats {
            count,
            errors,
            avg_duration_ms: if count > 0 {
                (total_ms as f64) / (count as f64)
            } else {
                0.0
            },
            error_rate: if count > 0 {
                (errors as f64) / (count as f64)
            } else {
                0.0
            },
        }
    }

    /// Get batch insert metrics
    pub fn batch_insert_stats(&self) -> BatchOperationStats {
        let count = self.inner.batch_insert_count.load(Ordering::Relaxed);
        let errors = self.inner.batch_insert_errors.load(Ordering::Relaxed);
        let total_ms = self
            .inner
            .batch_insert_total_duration_ms
            .load(Ordering::Relaxed);
        let total_vectors = self
            .inner
            .batch_insert_total_vectors
            .load(Ordering::Relaxed);

        BatchOperationStats {
            batch_count: count,
            errors,
            avg_duration_ms: if count > 0 {
                (total_ms as f64) / (count as f64)
            } else {
                0.0
            },
            error_rate: if count > 0 {
                (errors as f64) / (count as f64)
            } else {
                0.0
            },
            total_vectors,
            avg_vectors_per_batch: if count > 0 {
                (total_vectors as f64) / (count as f64)
            } else {
                0.0
            },
        }
    }

    /// Get update metrics
    pub fn update_stats(&self) -> OperationStats {
        let count = self.inner.update_count.load(Ordering::Relaxed);
        let errors = self.inner.update_errors.load(Ordering::Relaxed);
        let total_ms = self.inner.update_total_duration_ms.load(Ordering::Relaxed);

        OperationStats {
            count,
            errors,
            avg_duration_ms: if count > 0 {
                (total_ms as f64) / (count as f64)
            } else {
                0.0
            },
            error_rate: if count > 0 {
                (errors as f64) / (count as f64)
            } else {
                0.0
            },
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.inner.search_count.store(0, Ordering::Relaxed);
        self.inner.search_errors.store(0, Ordering::Relaxed);
        self.inner
            .search_total_duration_ms
            .store(0, Ordering::Relaxed);
        self.inner.insert_count.store(0, Ordering::Relaxed);
        self.inner.insert_errors.store(0, Ordering::Relaxed);
        self.inner
            .insert_total_duration_ms
            .store(0, Ordering::Relaxed);
        self.inner.delete_count.store(0, Ordering::Relaxed);
        self.inner.delete_errors.store(0, Ordering::Relaxed);
        self.inner
            .delete_total_duration_ms
            .store(0, Ordering::Relaxed);
        self.inner.batch_insert_count.store(0, Ordering::Relaxed);
        self.inner.batch_insert_errors.store(0, Ordering::Relaxed);
        self.inner
            .batch_insert_total_duration_ms
            .store(0, Ordering::Relaxed);
        self.inner
            .batch_insert_total_vectors
            .store(0, Ordering::Relaxed);
        self.inner.update_count.store(0, Ordering::Relaxed);
        self.inner.update_errors.store(0, Ordering::Relaxed);
        self.inner
            .update_total_duration_ms
            .store(0, Ordering::Relaxed);
    }
}

/// Statistics for a single operation type
#[derive(Debug, Clone)]
pub struct OperationStats {
    pub count: u64,
    pub errors: u64,
    pub avg_duration_ms: f64,
    pub error_rate: f64,
}

/// Statistics for batch operations
#[derive(Debug, Clone)]
pub struct BatchOperationStats {
    pub batch_count: u64,
    pub errors: u64,
    pub avg_duration_ms: f64,
    pub error_rate: f64,
    pub total_vectors: u64,
    pub avg_vectors_per_batch: f64,
}

/// Timer helper for recording operation durations
pub struct OperationTimer {
    start: Instant,
}

impl OperationTimer {
    /// Start a new timer
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Get elapsed duration
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

// ===== Metrics Provider Wrapper =====

use crate::{
    BatchInsertRequest, CollectionInfo, DeleteRequest, InsertRequest, Result, SearchRequest,
    SearchResult, UpdateRequest, VectorProvider,
};
use async_trait::async_trait;

/// Wrapper that adds automatic metrics collection to any VectorProvider
///
/// This wrapper tracks all vector operations and provides detailed statistics
/// about operation counts, latencies, and error rates.
///
/// # Example
/// ```no_run
/// use oxify_connect_vector::{MockVectorProvider, MetricsProvider};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = MockVectorProvider::new();
/// let metrics_provider = MetricsProvider::new(provider);
///
/// // Use provider normally
/// // ...
///
/// // Get metrics
/// let stats = metrics_provider.metrics().search_stats();
/// println!("Searches: {}, Avg latency: {}ms", stats.count, stats.avg_duration_ms);
/// # Ok(())
/// # }
/// ```
pub struct MetricsProvider<P>
where
    P: VectorProvider,
{
    provider: P,
    metrics: VectorMetrics,
}

impl<P> MetricsProvider<P>
where
    P: VectorProvider,
{
    /// Create a new metrics provider wrapping the given provider
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            metrics: VectorMetrics::new(),
        }
    }

    /// Get reference to the metrics collector
    pub fn metrics(&self) -> &VectorMetrics {
        &self.metrics
    }

    /// Get reference to the underlying provider
    pub fn inner(&self) -> &P {
        &self.provider
    }

    /// Consume this wrapper and return the underlying provider
    pub fn into_inner(self) -> P {
        self.provider
    }
}

#[async_trait]
impl<P> VectorProvider for MetricsProvider<P>
where
    P: VectorProvider,
{
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let timer = OperationTimer::start();
        let result = self.provider.search(request).await;
        self.metrics.record_search(timer.elapsed(), result.is_ok());
        result
    }

    async fn insert(&self, request: InsertRequest) -> Result<()> {
        let timer = OperationTimer::start();
        let result = self.provider.insert(request).await;
        self.metrics.record_insert(timer.elapsed(), result.is_ok());
        result
    }

    async fn delete(&self, request: DeleteRequest) -> Result<usize> {
        let timer = OperationTimer::start();
        let result = self.provider.delete(request).await;
        let count = result.as_ref().map(|c| *c).unwrap_or(0);
        self.metrics
            .record_delete(timer.elapsed(), result.is_ok(), count);
        result
    }

    async fn create_collection(&self, name: &str, dimension: usize) -> Result<()> {
        self.provider.create_collection(name, dimension).await
    }

    async fn collection_exists(&self, name: &str) -> Result<bool> {
        self.provider.collection_exists(name).await
    }

    async fn batch_insert(&self, request: BatchInsertRequest) -> Result<usize> {
        let vector_count = request.vectors.len();
        let timer = OperationTimer::start();
        let result = self.provider.batch_insert(request).await;
        self.metrics
            .record_batch_insert(timer.elapsed(), result.is_ok(), vector_count);
        result
    }

    async fn update(&self, request: UpdateRequest) -> Result<()> {
        let timer = OperationTimer::start();
        let result = self.provider.update(request).await;
        self.metrics.record_update(timer.elapsed(), result.is_ok());
        result
    }

    async fn collection_info(&self, name: &str) -> Result<CollectionInfo> {
        self.provider.collection_info(name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collection() {
        let metrics = VectorMetrics::new();

        // Record some operations
        metrics.record_search(Duration::from_millis(100), true);
        metrics.record_search(Duration::from_millis(200), true);
        metrics.record_search(Duration::from_millis(150), false);

        let stats = metrics.search_stats();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.errors, 1);
        assert!((stats.avg_duration_ms - 150.0).abs() < 1.0);
        assert!((stats.error_rate - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_batch_metrics() {
        let metrics = VectorMetrics::new();

        metrics.record_batch_insert(Duration::from_millis(500), true, 100);
        metrics.record_batch_insert(Duration::from_millis(600), true, 200);

        let stats = metrics.batch_insert_stats();
        assert_eq!(stats.batch_count, 2);
        assert_eq!(stats.total_vectors, 300);
        assert_eq!(stats.avg_vectors_per_batch, 150.0);
    }

    #[test]
    fn test_reset() {
        let metrics = VectorMetrics::new();

        metrics.record_insert(Duration::from_millis(100), true);
        assert_eq!(metrics.insert_stats().count, 1);

        metrics.reset();
        assert_eq!(metrics.insert_stats().count, 0);
    }

    #[test]
    fn test_timer() {
        let timer = OperationTimer::start();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = timer.elapsed();
        assert!(elapsed.as_millis() >= 10);
    }

    #[tokio::test]
    async fn test_metrics_provider_wrapper() {
        use crate::mock::MockVectorProvider;
        use crate::{InsertRequest, SearchRequest};

        let provider = MockVectorProvider::new();
        let metrics_provider = MetricsProvider::new(provider);

        // Create collection
        metrics_provider
            .create_collection("test", 128)
            .await
            .unwrap();

        // Insert vector
        metrics_provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: vec![0.1; 128],
                payload: serde_json::json!({"key": "value"}),
            })
            .await
            .unwrap();

        // Search
        metrics_provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![0.1; 128],
                top_k: 5,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        // Verify metrics were collected
        let search_stats = metrics_provider.metrics().search_stats();
        assert_eq!(search_stats.count, 1);
        assert_eq!(search_stats.errors, 0);

        let insert_stats = metrics_provider.metrics().insert_stats();
        assert_eq!(insert_stats.count, 1);
        assert_eq!(insert_stats.errors, 0);
    }
}
