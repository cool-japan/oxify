//! Connection pooling utilities for HTTP, database, and Redis connections.
//!
//! This module provides connection pooling to minimize connection overhead
//! and improve performance by reusing connections.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Statistics for connection pool
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Total number of connections created
    pub connections_created: u64,
    /// Total number of connections reused
    pub connections_reused: u64,
    /// Total number of connections closed
    pub connections_closed: u64,
    /// Current active connections
    pub active_connections: u64,
    /// Current idle connections
    pub idle_connections: u64,
}

/// Connection pool statistics tracker
#[derive(Debug)]
pub struct PoolStatsTracker {
    connections_created: AtomicU64,
    connections_reused: AtomicU64,
    connections_closed: AtomicU64,
    active_connections: AtomicU64,
    idle_connections: AtomicU64,
}

impl PoolStatsTracker {
    /// Create a new pool stats tracker
    pub const fn new() -> Self {
        Self {
            connections_created: AtomicU64::new(0),
            connections_reused: AtomicU64::new(0),
            connections_closed: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            idle_connections: AtomicU64::new(0),
        }
    }

    /// Record a connection creation
    pub fn record_create(&self) {
        self.connections_created.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a connection reuse
    pub fn record_reuse(&self) {
        self.connections_reused.fetch_add(1, Ordering::Relaxed);
        self.idle_connections.fetch_sub(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a connection close
    pub fn record_close(&self) {
        self.connections_closed.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record a connection return to pool
    pub fn record_return(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
        self.idle_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            connections_created: self.connections_created.load(Ordering::Relaxed),
            connections_reused: self.connections_reused.load(Ordering::Relaxed),
            connections_closed: self.connections_closed.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            idle_connections: self.idle_connections.load(Ordering::Relaxed),
        }
    }

    /// Reset statistics
    pub fn reset(&self) {
        self.connections_created.store(0, Ordering::Relaxed);
        self.connections_reused.store(0, Ordering::Relaxed);
        self.connections_closed.store(0, Ordering::Relaxed);
        self.active_connections.store(0, Ordering::Relaxed);
        self.idle_connections.store(0, Ordering::Relaxed);
    }
}

impl Default for PoolStatsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP/2 connection pool configuration
#[derive(Debug, Clone)]
pub struct Http2PoolConfig {
    /// Maximum number of idle connections per host
    pub max_idle_per_host: usize,
    /// Maximum number of concurrent requests per connection
    pub max_concurrent_streams: usize,
    /// Idle connection timeout
    pub idle_timeout: Duration,
    /// Connection keep-alive interval
    pub keep_alive_interval: Duration,
    /// Enable HTTP/2 adaptive window
    pub adaptive_window: bool,
}

impl Default for Http2PoolConfig {
    fn default() -> Self {
        Self {
            max_idle_per_host: 32,
            max_concurrent_streams: 100,
            idle_timeout: Duration::from_secs(90),
            keep_alive_interval: Duration::from_secs(30),
            adaptive_window: true,
        }
    }
}

impl Http2PoolConfig {
    /// Create a new HTTP/2 pool configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder pattern: set max_idle_per_host
    pub fn with_max_idle_per_host(mut self, max: usize) -> Self {
        self.max_idle_per_host = max;
        self
    }

    /// Builder pattern: set max_concurrent_streams
    pub fn with_max_concurrent_streams(mut self, max: usize) -> Self {
        self.max_concurrent_streams = max;
        self
    }

    /// Builder pattern: set idle_timeout
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Builder pattern: set keep_alive_interval
    pub fn with_keep_alive_interval(mut self, interval: Duration) -> Self {
        self.keep_alive_interval = interval;
        self
    }

    /// Builder pattern: set adaptive_window
    pub fn with_adaptive_window(mut self, enable: bool) -> Self {
        self.adaptive_window = enable;
        self
    }
}

/// Database connection pool configuration
#[derive(Debug, Clone)]
pub struct DbPoolConfig {
    /// Maximum number of connections in the pool
    pub max_connections: u32,
    /// Minimum number of idle connections
    pub min_idle: u32,
    /// Maximum lifetime of a connection
    pub max_lifetime: Duration,
    /// Idle timeout for connections
    pub idle_timeout: Duration,
    /// Connection timeout
    pub connection_timeout: Duration,
}

impl Default for DbPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            min_idle: 10,
            max_lifetime: Duration::from_secs(1800), // 30 minutes
            idle_timeout: Duration::from_secs(600),  // 10 minutes
            connection_timeout: Duration::from_secs(30),
        }
    }
}

impl DbPoolConfig {
    /// Create a new database pool configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder pattern: set max_connections
    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Builder pattern: set min_idle
    pub fn with_min_idle(mut self, min: u32) -> Self {
        self.min_idle = min;
        self
    }

    /// Builder pattern: set max_lifetime
    pub fn with_max_lifetime(mut self, lifetime: Duration) -> Self {
        self.max_lifetime = lifetime;
        self
    }

    /// Builder pattern: set idle_timeout
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Builder pattern: set connection_timeout
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }
}

/// Redis connection pool configuration
#[derive(Debug, Clone)]
pub struct RedisPoolConfig {
    /// Maximum number of connections in the pool
    pub max_connections: usize,
    /// Minimum number of idle connections
    pub min_idle: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Read timeout
    pub read_timeout: Duration,
    /// Write timeout
    pub write_timeout: Duration,
}

impl Default for RedisPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 50,
            min_idle: 5,
            connection_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(3),
            write_timeout: Duration::from_secs(3),
        }
    }
}

impl RedisPoolConfig {
    /// Create a new Redis pool configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder pattern: set max_connections
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    /// Builder pattern: set min_idle
    pub fn with_min_idle(mut self, min: usize) -> Self {
        self.min_idle = min;
        self
    }

    /// Builder pattern: set connection_timeout
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Builder pattern: set read_timeout
    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    /// Builder pattern: set write_timeout
    pub fn with_write_timeout(mut self, timeout: Duration) -> Self {
        self.write_timeout = timeout;
        self
    }
}

/// Connection metadata
#[derive(Debug, Clone)]
pub struct ConnectionMetadata {
    /// Connection ID
    pub id: String,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last used timestamp
    pub last_used: Instant,
    /// Number of times this connection was reused
    pub reuse_count: u64,
}

impl ConnectionMetadata {
    /// Create new connection metadata
    pub fn new(id: String) -> Self {
        let now = Instant::now();
        Self {
            id,
            created_at: now,
            last_used: now,
            reuse_count: 0,
        }
    }

    /// Mark the connection as used
    pub fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.reuse_count += 1;
    }

    /// Get the age of the connection
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get the idle time of the connection
    pub fn idle_time(&self) -> Duration {
        self.last_used.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_stats_tracker() {
        let tracker = PoolStatsTracker::new();
        tracker.reset();

        tracker.record_create();
        tracker.record_create();
        tracker.record_return();
        tracker.record_reuse();

        let stats = tracker.stats();
        assert_eq!(stats.connections_created, 2);
        assert_eq!(stats.connections_reused, 1);
        assert_eq!(stats.active_connections, 2);
    }

    #[test]
    fn test_http2_pool_config_default() {
        let config = Http2PoolConfig::default();
        assert_eq!(config.max_idle_per_host, 32);
        assert_eq!(config.max_concurrent_streams, 100);
        assert!(config.adaptive_window);
    }

    #[test]
    fn test_http2_pool_config_builder() {
        let config = Http2PoolConfig::new()
            .with_max_idle_per_host(64)
            .with_max_concurrent_streams(200)
            .with_adaptive_window(false);

        assert_eq!(config.max_idle_per_host, 64);
        assert_eq!(config.max_concurrent_streams, 200);
        assert!(!config.adaptive_window);
    }

    #[test]
    fn test_db_pool_config_default() {
        let config = DbPoolConfig::default();
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.min_idle, 10);
    }

    #[test]
    fn test_db_pool_config_builder() {
        let config = DbPoolConfig::new()
            .with_max_connections(200)
            .with_min_idle(20);

        assert_eq!(config.max_connections, 200);
        assert_eq!(config.min_idle, 20);
    }

    #[test]
    fn test_redis_pool_config_default() {
        let config = RedisPoolConfig::default();
        assert_eq!(config.max_connections, 50);
        assert_eq!(config.min_idle, 5);
    }

    #[test]
    fn test_redis_pool_config_builder() {
        let config = RedisPoolConfig::new()
            .with_max_connections(100)
            .with_min_idle(10);

        assert_eq!(config.max_connections, 100);
        assert_eq!(config.min_idle, 10);
    }

    #[test]
    fn test_connection_metadata() {
        let mut metadata = ConnectionMetadata::new("conn-123".to_string());
        assert_eq!(metadata.id, "conn-123");
        assert_eq!(metadata.reuse_count, 0);

        std::thread::sleep(std::time::Duration::from_millis(10));
        metadata.mark_used();
        assert_eq!(metadata.reuse_count, 1);
        assert!(metadata.age() >= Duration::from_millis(10));
    }

    #[test]
    fn test_pool_stats_close() {
        let tracker = PoolStatsTracker::new();
        tracker.reset();

        tracker.record_create();
        tracker.record_close();

        let stats = tracker.stats();
        assert_eq!(stats.connections_created, 1);
        assert_eq!(stats.connections_closed, 1);
        assert_eq!(stats.active_connections, 0);
    }
}
