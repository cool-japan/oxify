//! Chaos engineering utilities for testing system resilience.
//!
//! This module provides tools for injecting failures, latency, and resource exhaustion
//! into the system to test its resilience and recovery capabilities.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Chaos engineering configuration.
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Whether chaos engineering is enabled
    pub enabled: bool,
    /// Probability of injecting a failure (0.0 - 1.0)
    pub failure_rate: f64,
    /// Probability of injecting latency (0.0 - 1.0)
    pub latency_rate: f64,
    /// Minimum latency to inject (milliseconds)
    pub min_latency_ms: u64,
    /// Maximum latency to inject (milliseconds)
    pub max_latency_ms: u64,
    /// Whether to inject CPU load
    pub cpu_load_enabled: bool,
    /// Whether to inject memory pressure
    pub memory_pressure_enabled: bool,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            failure_rate: 0.0,
            latency_rate: 0.0,
            min_latency_ms: 100,
            max_latency_ms: 5000,
            cpu_load_enabled: false,
            memory_pressure_enabled: false,
        }
    }
}

impl ChaosConfig {
    /// Create a chaos config builder.
    pub fn builder() -> ChaosConfigBuilder {
        ChaosConfigBuilder::default()
    }

    /// Development chaos config (low impact).
    pub fn development() -> Self {
        Self {
            enabled: true,
            failure_rate: 0.01, // 1% failure rate
            latency_rate: 0.05, // 5% latency injection
            min_latency_ms: 100,
            max_latency_ms: 1000,
            cpu_load_enabled: false,
            memory_pressure_enabled: false,
        }
    }

    /// Aggressive chaos config (high impact for testing).
    pub fn aggressive() -> Self {
        Self {
            enabled: true,
            failure_rate: 0.1, // 10% failure rate
            latency_rate: 0.2, // 20% latency injection
            min_latency_ms: 500,
            max_latency_ms: 5000,
            cpu_load_enabled: true,
            memory_pressure_enabled: true,
        }
    }
}

/// Builder for chaos configuration.
#[derive(Debug, Default)]
pub struct ChaosConfigBuilder {
    config: ChaosConfig,
}

impl ChaosConfigBuilder {
    /// Enable chaos engineering.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    /// Set failure rate (0.0 - 1.0).
    pub fn failure_rate(mut self, rate: f64) -> Self {
        self.config.failure_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Set latency injection rate (0.0 - 1.0).
    pub fn latency_rate(mut self, rate: f64) -> Self {
        self.config.latency_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Set latency range (milliseconds).
    pub fn latency_range(mut self, min_ms: u64, max_ms: u64) -> Self {
        self.config.min_latency_ms = min_ms;
        self.config.max_latency_ms = max_ms;
        self
    }

    /// Enable CPU load injection.
    pub fn cpu_load(mut self, enabled: bool) -> Self {
        self.config.cpu_load_enabled = enabled;
        self
    }

    /// Enable memory pressure injection.
    pub fn memory_pressure(mut self, enabled: bool) -> Self {
        self.config.memory_pressure_enabled = enabled;
        self
    }

    /// Build the chaos config.
    pub fn build(self) -> ChaosConfig {
        self.config
    }
}

/// Chaos engineering middleware state.
#[derive(Debug, Clone)]
pub struct ChaosMiddleware {
    #[allow(dead_code)]
    config: Arc<ChaosConfig>,
    stats: Arc<ChaosStats>,
}

/// Statistics for chaos engineering.
#[derive(Debug, Default)]
pub struct ChaosStats {
    failures_injected: AtomicU64,
    latency_injected: AtomicU64,
    total_requests: AtomicU64,
}

impl ChaosMiddleware {
    /// Create a new chaos middleware.
    pub fn new(config: ChaosConfig) -> Self {
        Self {
            config: Arc::new(config),
            stats: Arc::new(ChaosStats::default()),
        }
    }

    /// Get chaos statistics.
    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.stats.failures_injected.load(Ordering::Relaxed),
            self.stats.latency_injected.load(Ordering::Relaxed),
            self.stats.total_requests.load(Ordering::Relaxed),
        )
    }

    /// Reset statistics.
    pub fn reset_stats(&self) {
        self.stats.failures_injected.store(0, Ordering::Relaxed);
        self.stats.latency_injected.store(0, Ordering::Relaxed);
        self.stats.total_requests.store(0, Ordering::Relaxed);
    }

    /// Check if should inject failure.
    #[allow(dead_code)]
    fn should_inject_failure(&self) -> bool {
        if !self.config.enabled || self.config.failure_rate == 0.0 {
            return false;
        }
        rand::random::<f64>() < self.config.failure_rate
    }

    /// Check if should inject latency.
    #[allow(dead_code)]
    fn should_inject_latency(&self) -> bool {
        if !self.config.enabled || self.config.latency_rate == 0.0 {
            return false;
        }
        rand::random::<f64>() < self.config.latency_rate
    }

    /// Get random latency duration.
    #[allow(dead_code)]
    fn random_latency(&self) -> Duration {
        let range = self.config.max_latency_ms - self.config.min_latency_ms;
        let ms = self.config.min_latency_ms + (rand::random::<u64>() % (range + 1));
        Duration::from_millis(ms)
    }
}

/// Chaos engineering middleware handler.
pub async fn chaos_middleware(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    // For now, just pass through
    // In production, this would be configured via ChaosMiddleware
    Ok(next.run(req).await)
}

/// Inject latency into the current task.
pub async fn inject_latency(duration: Duration) {
    sleep(duration).await;
}

/// Inject CPU load (blocking operation).
pub fn inject_cpu_load(duration: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        // CPU-intensive operation
        let mut sum: u64 = 0;
        for i in 0..1000 {
            sum = sum.wrapping_add(i);
        }
        // Prevent optimization
        std::hint::black_box(sum);
    }
}

/// Inject memory pressure by allocating memory.
pub fn inject_memory_pressure(size_mb: usize) -> Vec<Vec<u8>> {
    let mut allocations = Vec::new();
    for _ in 0..size_mb {
        allocations.push(vec![0u8; 1024 * 1024]); // 1 MB
    }
    allocations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chaos_config_default() {
        let config = ChaosConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.failure_rate, 0.0);
        assert_eq!(config.latency_rate, 0.0);
    }

    #[test]
    fn test_chaos_config_development() {
        let config = ChaosConfig::development();
        assert!(config.enabled);
        assert!(config.failure_rate > 0.0);
        assert!(config.latency_rate > 0.0);
    }

    #[test]
    fn test_chaos_config_aggressive() {
        let config = ChaosConfig::aggressive();
        assert!(config.enabled);
        assert!(config.failure_rate >= 0.1);
        assert!(config.latency_rate >= 0.2);
        assert!(config.cpu_load_enabled);
        assert!(config.memory_pressure_enabled);
    }

    #[test]
    fn test_chaos_config_builder() {
        let config = ChaosConfig::builder()
            .enabled(true)
            .failure_rate(0.5)
            .latency_rate(0.3)
            .latency_range(200, 2000)
            .cpu_load(true)
            .memory_pressure(true)
            .build();

        assert!(config.enabled);
        assert_eq!(config.failure_rate, 0.5);
        assert_eq!(config.latency_rate, 0.3);
        assert_eq!(config.min_latency_ms, 200);
        assert_eq!(config.max_latency_ms, 2000);
        assert!(config.cpu_load_enabled);
        assert!(config.memory_pressure_enabled);
    }

    #[test]
    fn test_failure_rate_clamping() {
        let config = ChaosConfig::builder()
            .failure_rate(2.0) // Should be clamped to 1.0
            .build();
        assert_eq!(config.failure_rate, 1.0);

        let config2 = ChaosConfig::builder()
            .failure_rate(-0.5) // Should be clamped to 0.0
            .build();
        assert_eq!(config2.failure_rate, 0.0);
    }

    #[test]
    fn test_chaos_middleware_creation() {
        let config = ChaosConfig::development();
        let middleware = ChaosMiddleware::new(config);
        let (failures, latency, total) = middleware.stats();
        assert_eq!(failures, 0);
        assert_eq!(latency, 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_chaos_stats_reset() {
        let config = ChaosConfig::development();
        let middleware = ChaosMiddleware::new(config);

        // Simulate some stats
        middleware
            .stats
            .failures_injected
            .store(10, Ordering::Relaxed);
        middleware
            .stats
            .latency_injected
            .store(20, Ordering::Relaxed);
        middleware
            .stats
            .total_requests
            .store(100, Ordering::Relaxed);

        middleware.reset_stats();

        let (failures, latency, total) = middleware.stats();
        assert_eq!(failures, 0);
        assert_eq!(latency, 0);
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn test_inject_latency() {
        let start = std::time::Instant::now();
        inject_latency(Duration::from_millis(100)).await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(100));
    }

    #[test]
    fn test_inject_cpu_load() {
        let start = std::time::Instant::now();
        inject_cpu_load(Duration::from_millis(50));
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(50));
    }

    #[test]
    fn test_inject_memory_pressure() {
        let allocations = inject_memory_pressure(10); // 10 MB
        assert_eq!(allocations.len(), 10);
        assert_eq!(allocations[0].len(), 1024 * 1024);
    }
}
