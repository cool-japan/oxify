//! Async optimization utilities for minimizing context switches and improving performance.
//!
//! This module provides utilities for optimizing async code:
//! - CPU-intensive task spawning
//! - Context switch minimization
//! - Performance tracking
//! - Hot path optimization

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

/// Statistics for async task execution
#[derive(Debug, Clone)]
pub struct AsyncStats {
    /// Total number of tasks spawned
    pub tasks_spawned: u64,
    /// Total number of tasks completed
    pub tasks_completed: u64,
    /// Average task execution time in microseconds
    pub avg_execution_time_us: u64,
}

/// Global async statistics tracker
pub struct AsyncStatsTracker {
    tasks_spawned: AtomicU64,
    tasks_completed: AtomicU64,
    total_execution_time_us: AtomicU64,
}

impl AsyncStatsTracker {
    /// Create a new async stats tracker
    pub const fn new() -> Self {
        Self {
            tasks_spawned: AtomicU64::new(0),
            tasks_completed: AtomicU64::new(0),
            total_execution_time_us: AtomicU64::new(0),
        }
    }

    /// Record a task spawn
    pub fn record_spawn(&self) {
        self.tasks_spawned.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a task completion with execution time
    pub fn record_completion(&self, execution_time: Duration) {
        self.tasks_completed.fetch_add(1, Ordering::Relaxed);
        self.total_execution_time_us
            .fetch_add(execution_time.as_micros() as u64, Ordering::Relaxed);
    }

    /// Get current statistics
    pub fn stats(&self) -> AsyncStats {
        let spawned = self.tasks_spawned.load(Ordering::Relaxed);
        let completed = self.tasks_completed.load(Ordering::Relaxed);
        let total_time = self.total_execution_time_us.load(Ordering::Relaxed);

        let avg_time = if completed > 0 {
            total_time / completed
        } else {
            0
        };

        AsyncStats {
            tasks_spawned: spawned,
            tasks_completed: completed,
            avg_execution_time_us: avg_time,
        }
    }

    /// Reset statistics
    pub fn reset(&self) {
        self.tasks_spawned.store(0, Ordering::Relaxed);
        self.tasks_completed.store(0, Ordering::Relaxed);
        self.total_execution_time_us.store(0, Ordering::Relaxed);
    }
}

impl Default for AsyncStatsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Global async stats tracker instance
pub static ASYNC_STATS: AsyncStatsTracker = AsyncStatsTracker::new();

/// Spawn a CPU-intensive task on a blocking thread pool
///
/// This avoids blocking the async runtime for CPU-bound work.
///
/// # Example
///
/// ```no_run
/// use oxify_server::async_optimization::spawn_cpu_task;
///
/// # #[tokio::main]
/// # async fn main() {
/// let result = spawn_cpu_task(|| {
///     // CPU-intensive computation
///     (0..1000000).sum::<u64>()
/// }).await.unwrap();
/// # }
/// ```
pub fn spawn_cpu_task<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    ASYNC_STATS.record_spawn();
    let start = Instant::now();

    tokio::task::spawn_blocking(move || {
        let result = f();
        ASYNC_STATS.record_completion(start.elapsed());
        result
    })
}

/// Execute a future with performance tracking
///
/// # Example
///
/// ```no_run
/// use oxify_server::async_optimization::track_async;
///
/// # #[tokio::main]
/// # async fn main() {
/// let result = track_async(async {
///     tokio::time::sleep(std::time::Duration::from_millis(10)).await;
///     42
/// }).await;
/// # }
/// ```
pub async fn track_async<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    ASYNC_STATS.record_spawn();
    let start = Instant::now();
    let result = future.await;
    ASYNC_STATS.record_completion(start.elapsed());
    result
}

/// Batch multiple futures and execute them concurrently
///
/// This minimizes context switches by grouping related work.
///
/// # Example
///
/// ```
/// use oxify_server::async_optimization::batch_futures;
///
/// # #[tokio::main]
/// # async fn main() {
/// async fn make_future(val: i32) -> i32 { val }
///
/// let futures = vec![make_future(1), make_future(2), make_future(3)];
/// let results = batch_futures(futures).await;
/// assert_eq!(results, vec![1, 2, 3]);
/// # }
/// ```
pub async fn batch_futures<F, T>(futures: Vec<F>) -> Vec<T>
where
    F: Future<Output = T>,
{
    futures::future::join_all(futures).await
}

/// Spawn a task on the current runtime with performance tracking
///
/// # Example
///
/// ```no_run
/// use oxify_server::async_optimization::spawn_tracked;
///
/// # #[tokio::main]
/// # async fn main() {
/// let handle = spawn_tracked(async {
///     42
/// });
/// let result = handle.await.unwrap();
/// # }
/// ```
pub fn spawn_tracked<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    ASYNC_STATS.record_spawn();
    let start = Instant::now();

    tokio::spawn(async move {
        let result = future.await;
        ASYNC_STATS.record_completion(start.elapsed());
        result
    })
}

/// Configuration for async optimization
#[derive(Debug, Clone)]
pub struct AsyncConfig {
    /// Enable performance tracking
    pub enable_tracking: bool,
    /// Maximum number of concurrent tasks
    pub max_concurrent_tasks: usize,
    /// CPU task thread pool size
    pub cpu_pool_size: usize,
}

impl Default for AsyncConfig {
    fn default() -> Self {
        Self {
            enable_tracking: true,
            max_concurrent_tasks: 1000,
            cpu_pool_size: num_cpus::get(),
        }
    }
}

impl AsyncConfig {
    /// Create a new async configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder pattern: set enable_tracking
    pub fn with_tracking(mut self, enable: bool) -> Self {
        self.enable_tracking = enable;
        self
    }

    /// Builder pattern: set max_concurrent_tasks
    pub fn with_max_concurrent_tasks(mut self, max: usize) -> Self {
        self.max_concurrent_tasks = max;
        self
    }

    /// Builder pattern: set cpu_pool_size
    pub fn with_cpu_pool_size(mut self, size: usize) -> Self {
        self.cpu_pool_size = size;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_config_default() {
        let config = AsyncConfig::default();
        assert!(config.enable_tracking);
        assert_eq!(config.max_concurrent_tasks, 1000);
        assert_eq!(config.cpu_pool_size, num_cpus::get());
    }

    #[test]
    fn test_async_config_builder() {
        let config = AsyncConfig::new()
            .with_tracking(false)
            .with_max_concurrent_tasks(500)
            .with_cpu_pool_size(4);

        assert!(!config.enable_tracking);
        assert_eq!(config.max_concurrent_tasks, 500);
        assert_eq!(config.cpu_pool_size, 4);
    }

    #[test]
    fn test_async_stats_tracker() {
        let tracker = AsyncStatsTracker::new();
        tracker.reset();

        tracker.record_spawn();
        tracker.record_spawn();
        tracker.record_completion(Duration::from_micros(100));

        let stats = tracker.stats();
        assert_eq!(stats.tasks_spawned, 2);
        assert_eq!(stats.tasks_completed, 1);
        assert_eq!(stats.avg_execution_time_us, 100);
    }

    #[tokio::test]
    async fn test_spawn_cpu_task() {
        let result = spawn_cpu_task(|| {
            let mut sum = 0u64;
            for i in 0..1000 {
                sum += i;
            }
            sum
        })
        .await
        .unwrap();

        assert_eq!(result, (0..1000).sum::<u64>());
    }

    #[tokio::test]
    async fn test_track_async() {
        let result = track_async(async { 42 }).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_batch_futures() {
        async fn make_future(val: i32) -> i32 {
            val
        }

        let futures = vec![make_future(1), make_future(2), make_future(3)];
        let results = batch_futures(futures).await;
        assert_eq!(results, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_spawn_tracked() {
        let handle = spawn_tracked(async { 42 });
        let result = handle.await.unwrap();
        assert_eq!(result, 42);
    }
}
