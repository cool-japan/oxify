//! DDoS protection mechanisms for the HTTP server.
//!
//! This module provides protection against various DDoS attack vectors:
//! - Connection limits (prevent resource exhaustion)
//! - Slowloris protection (detect slow HTTP attacks)
//! - Request timeouts (prevent long-running requests)
//! - IP-based rate limiting integration

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// DDoS protection statistics
#[derive(Debug, Clone)]
pub struct DdosStats {
    /// Total number of connections blocked
    pub connections_blocked: u64,
    /// Total number of slow requests detected
    pub slow_requests_detected: u64,
    /// Total number of timeouts
    pub timeouts: u64,
    /// Current active connections
    pub active_connections: u64,
}

/// DDoS protection statistics tracker
#[derive(Debug)]
pub struct DdosStatsTracker {
    connections_blocked: AtomicU64,
    slow_requests_detected: AtomicU64,
    timeouts: AtomicU64,
    active_connections: AtomicU64,
}

impl DdosStatsTracker {
    /// Create a new DDoS stats tracker
    pub const fn new() -> Self {
        Self {
            connections_blocked: AtomicU64::new(0),
            slow_requests_detected: AtomicU64::new(0),
            timeouts: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
        }
    }

    /// Record a blocked connection
    pub fn record_block(&self) {
        self.connections_blocked.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a slow request detection
    pub fn record_slow_request(&self) {
        self.slow_requests_detected.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a timeout
    pub fn record_timeout(&self) {
        self.timeouts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record connection start
    pub fn record_connection_start(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record connection end
    pub fn record_connection_end(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get current statistics
    pub fn stats(&self) -> DdosStats {
        DdosStats {
            connections_blocked: self.connections_blocked.load(Ordering::Relaxed),
            slow_requests_detected: self.slow_requests_detected.load(Ordering::Relaxed),
            timeouts: self.timeouts.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
        }
    }

    /// Reset statistics
    pub fn reset(&self) {
        self.connections_blocked.store(0, Ordering::Relaxed);
        self.slow_requests_detected.store(0, Ordering::Relaxed);
        self.timeouts.store(0, Ordering::Relaxed);
        self.active_connections.store(0, Ordering::Relaxed);
    }
}

impl Default for DdosStatsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Connection limit configuration
#[derive(Debug, Clone)]
pub struct ConnectionLimitConfig {
    /// Maximum concurrent connections globally
    pub max_concurrent_connections: usize,
    /// Maximum concurrent connections per IP
    pub max_connections_per_ip: usize,
    /// Maximum requests per connection
    pub max_requests_per_connection: usize,
    /// Idle connection timeout
    pub idle_timeout: Duration,
}

impl Default for ConnectionLimitConfig {
    fn default() -> Self {
        Self {
            max_concurrent_connections: 10000,
            max_connections_per_ip: 100,
            max_requests_per_connection: 1000,
            idle_timeout: Duration::from_secs(60),
        }
    }
}

impl ConnectionLimitConfig {
    /// Create a new connection limit configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder pattern: set max_concurrent_connections
    pub fn with_max_concurrent_connections(mut self, max: usize) -> Self {
        self.max_concurrent_connections = max;
        self
    }

    /// Builder pattern: set max_connections_per_ip
    pub fn with_max_connections_per_ip(mut self, max: usize) -> Self {
        self.max_connections_per_ip = max;
        self
    }

    /// Builder pattern: set max_requests_per_connection
    pub fn with_max_requests_per_connection(mut self, max: usize) -> Self {
        self.max_requests_per_connection = max;
        self
    }

    /// Builder pattern: set idle_timeout
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }
}

/// Slowloris protection configuration
#[derive(Debug, Clone)]
pub struct SlowlorisConfig {
    /// Request header timeout (time to receive all headers)
    pub header_timeout: Duration,
    /// Request body timeout (time to receive entire body)
    pub body_timeout: Duration,
    /// Response timeout (time to send entire response)
    pub response_timeout: Duration,
    /// Minimum data rate (bytes per second)
    pub min_data_rate: usize,
}

impl Default for SlowlorisConfig {
    fn default() -> Self {
        Self {
            header_timeout: Duration::from_secs(10),
            body_timeout: Duration::from_secs(30),
            response_timeout: Duration::from_secs(60),
            min_data_rate: 1024, // 1 KB/s minimum
        }
    }
}

impl SlowlorisConfig {
    /// Create a new slowloris protection configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder pattern: set header_timeout
    pub fn with_header_timeout(mut self, timeout: Duration) -> Self {
        self.header_timeout = timeout;
        self
    }

    /// Builder pattern: set body_timeout
    pub fn with_body_timeout(mut self, timeout: Duration) -> Self {
        self.body_timeout = timeout;
        self
    }

    /// Builder pattern: set response_timeout
    pub fn with_response_timeout(mut self, timeout: Duration) -> Self {
        self.response_timeout = timeout;
        self
    }

    /// Builder pattern: set min_data_rate
    pub fn with_min_data_rate(mut self, rate: usize) -> Self {
        self.min_data_rate = rate;
        self
    }
}

/// DDoS protection configuration
#[derive(Debug, Clone)]
pub struct DdosConfig {
    /// Enable DDoS protection
    pub enabled: bool,
    /// Connection limit configuration
    pub connection_limits: ConnectionLimitConfig,
    /// Slowloris protection configuration
    pub slowloris_protection: SlowlorisConfig,
}

impl Default for DdosConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            connection_limits: ConnectionLimitConfig::default(),
            slowloris_protection: SlowlorisConfig::default(),
        }
    }
}

impl DdosConfig {
    /// Create a new DDoS protection configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a strict DDoS configuration (more aggressive)
    pub fn strict() -> Self {
        Self {
            enabled: true,
            connection_limits: ConnectionLimitConfig {
                max_concurrent_connections: 5000,
                max_connections_per_ip: 50,
                max_requests_per_connection: 500,
                idle_timeout: Duration::from_secs(30),
            },
            slowloris_protection: SlowlorisConfig {
                header_timeout: Duration::from_secs(5),
                body_timeout: Duration::from_secs(15),
                response_timeout: Duration::from_secs(30),
                min_data_rate: 2048, // 2 KB/s minimum
            },
        }
    }

    /// Create a relaxed DDoS configuration (less aggressive)
    pub fn relaxed() -> Self {
        Self {
            enabled: true,
            connection_limits: ConnectionLimitConfig {
                max_concurrent_connections: 20000,
                max_connections_per_ip: 200,
                max_requests_per_connection: 2000,
                idle_timeout: Duration::from_secs(120),
            },
            slowloris_protection: SlowlorisConfig {
                header_timeout: Duration::from_secs(20),
                body_timeout: Duration::from_secs(60),
                response_timeout: Duration::from_secs(120),
                min_data_rate: 512, // 512 B/s minimum
            },
        }
    }

    /// Builder pattern: enable or disable DDoS protection
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builder pattern: set connection limits
    pub fn with_connection_limits(mut self, limits: ConnectionLimitConfig) -> Self {
        self.connection_limits = limits;
        self
    }

    /// Builder pattern: set slowloris protection
    pub fn with_slowloris_protection(mut self, protection: SlowlorisConfig) -> Self {
        self.slowloris_protection = protection;
        self
    }
}

/// Connection tracker for DDoS protection
#[derive(Debug)]
pub struct ConnectionTracker {
    /// Per-IP connection count
    ip_connections: Arc<RwLock<HashMap<IpAddr, usize>>>,
    /// Global connection count
    global_connections: AtomicU64,
    /// DDoS statistics
    stats: Arc<DdosStatsTracker>,
}

impl ConnectionTracker {
    /// Create a new connection tracker
    pub fn new(stats: Arc<DdosStatsTracker>) -> Self {
        Self {
            ip_connections: Arc::new(RwLock::new(HashMap::new())),
            global_connections: AtomicU64::new(0),
            stats,
        }
    }

    /// Check if a connection from the given IP should be allowed
    pub fn allow_connection(&self, ip: IpAddr, config: &ConnectionLimitConfig) -> bool {
        // Check global limit
        let global = self.global_connections.load(Ordering::Relaxed) as usize;
        if global >= config.max_concurrent_connections {
            self.stats.record_block();
            return false;
        }

        // Check per-IP limit
        let connections = self.ip_connections.read().unwrap();
        let count = connections.get(&ip).copied().unwrap_or(0);
        if count >= config.max_connections_per_ip {
            self.stats.record_block();
            return false;
        }

        true
    }

    /// Register a new connection
    pub fn register_connection(&self, ip: IpAddr) {
        self.global_connections.fetch_add(1, Ordering::Relaxed);
        self.stats.record_connection_start();

        let mut connections = self.ip_connections.write().unwrap();
        *connections.entry(ip).or_insert(0) += 1;
    }

    /// Unregister a connection
    pub fn unregister_connection(&self, ip: IpAddr) {
        self.global_connections.fetch_sub(1, Ordering::Relaxed);
        self.stats.record_connection_end();

        let mut connections = self.ip_connections.write().unwrap();
        if let Some(count) = connections.get_mut(&ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                connections.remove(&ip);
            }
        }
    }

    /// Get current connection count for an IP
    pub fn get_ip_connections(&self, ip: IpAddr) -> usize {
        self.ip_connections
            .read()
            .unwrap()
            .get(&ip)
            .copied()
            .unwrap_or(0)
    }

    /// Get total connection count
    pub fn get_total_connections(&self) -> usize {
        self.global_connections.load(Ordering::Relaxed) as usize
    }
}

/// Request timing tracker for slowloris detection
#[derive(Debug)]
pub struct RequestTimingTracker {
    /// Request start time
    start_time: Instant,
    /// Bytes received/sent
    bytes_transferred: usize,
}

impl RequestTimingTracker {
    /// Create a new request timing tracker
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            bytes_transferred: 0,
        }
    }

    /// Record bytes transferred
    pub fn record_bytes(&mut self, bytes: usize) {
        self.bytes_transferred += bytes;
    }

    /// Check if the request is too slow
    pub fn is_too_slow(&self, config: &SlowlorisConfig) -> bool {
        let elapsed = self.start_time.elapsed();
        if elapsed.as_secs() == 0 {
            return false;
        }

        let rate = self.bytes_transferred / elapsed.as_secs() as usize;
        rate < config.min_data_rate
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Default for RequestTimingTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_ddos_stats_tracker() {
        let tracker = DdosStatsTracker::new();
        tracker.reset();

        tracker.record_block();
        tracker.record_slow_request();
        tracker.record_timeout();
        tracker.record_connection_start();

        let stats = tracker.stats();
        assert_eq!(stats.connections_blocked, 1);
        assert_eq!(stats.slow_requests_detected, 1);
        assert_eq!(stats.timeouts, 1);
        assert_eq!(stats.active_connections, 1);
    }

    #[test]
    fn test_connection_limit_config_default() {
        let config = ConnectionLimitConfig::default();
        assert_eq!(config.max_concurrent_connections, 10000);
        assert_eq!(config.max_connections_per_ip, 100);
    }

    #[test]
    fn test_connection_limit_config_builder() {
        let config = ConnectionLimitConfig::new()
            .with_max_concurrent_connections(5000)
            .with_max_connections_per_ip(50);

        assert_eq!(config.max_concurrent_connections, 5000);
        assert_eq!(config.max_connections_per_ip, 50);
    }

    #[test]
    fn test_slowloris_config_default() {
        let config = SlowlorisConfig::default();
        assert_eq!(config.header_timeout, Duration::from_secs(10));
        assert_eq!(config.min_data_rate, 1024);
    }

    #[test]
    fn test_slowloris_config_builder() {
        let config = SlowlorisConfig::new()
            .with_header_timeout(Duration::from_secs(5))
            .with_min_data_rate(2048);

        assert_eq!(config.header_timeout, Duration::from_secs(5));
        assert_eq!(config.min_data_rate, 2048);
    }

    #[test]
    fn test_ddos_config_default() {
        let config = DdosConfig::default();
        assert!(config.enabled);
    }

    #[test]
    fn test_ddos_config_strict() {
        let config = DdosConfig::strict();
        assert!(config.enabled);
        assert_eq!(config.connection_limits.max_concurrent_connections, 5000);
        assert_eq!(config.slowloris_protection.min_data_rate, 2048);
    }

    #[test]
    fn test_ddos_config_relaxed() {
        let config = DdosConfig::relaxed();
        assert!(config.enabled);
        assert_eq!(config.connection_limits.max_concurrent_connections, 20000);
        assert_eq!(config.slowloris_protection.min_data_rate, 512);
    }

    #[test]
    fn test_connection_tracker() {
        let stats = Arc::new(DdosStatsTracker::new());
        let tracker = ConnectionTracker::new(stats);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let config = ConnectionLimitConfig::default();

        assert!(tracker.allow_connection(ip, &config));
        tracker.register_connection(ip);
        assert_eq!(tracker.get_ip_connections(ip), 1);
        assert_eq!(tracker.get_total_connections(), 1);

        tracker.unregister_connection(ip);
        assert_eq!(tracker.get_ip_connections(ip), 0);
        assert_eq!(tracker.get_total_connections(), 0);
    }

    #[test]
    fn test_connection_tracker_limit() {
        let stats = Arc::new(DdosStatsTracker::new());
        let tracker = ConnectionTracker::new(stats.clone());
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let config = ConnectionLimitConfig::new().with_max_connections_per_ip(2);

        tracker.register_connection(ip);
        tracker.register_connection(ip);
        assert!(!tracker.allow_connection(ip, &config));

        let ddos_stats = stats.stats();
        assert_eq!(ddos_stats.connections_blocked, 1);
    }

    #[test]
    fn test_request_timing_tracker() {
        let mut tracker = RequestTimingTracker::new();
        tracker.record_bytes(1024);
        assert_eq!(tracker.bytes_transferred, 1024);

        let config = SlowlorisConfig::default();
        // Fresh tracker should not be too slow
        assert!(!tracker.is_too_slow(&config));
    }

    #[test]
    fn test_request_timing_elapsed() {
        let tracker = RequestTimingTracker::new();
        std::thread::sleep(Duration::from_millis(10));
        assert!(tracker.elapsed() >= Duration::from_millis(10));
    }
}
