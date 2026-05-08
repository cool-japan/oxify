//! Resource-aware scheduling for workflow execution
//!
//! This module provides system resource monitoring (CPU, memory) and
//! dynamic concurrency adjustment based on available resources.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Resource utilization thresholds for scheduling decisions
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResourceThresholds {
    /// CPU usage percentage to trigger throttling (0.0 - 1.0)
    pub cpu_high: f64,
    /// CPU usage percentage to resume normal operation (0.0 - 1.0)
    pub cpu_low: f64,
    /// Memory usage percentage to trigger throttling (0.0 - 1.0)
    pub memory_high: f64,
    /// Memory usage percentage to resume normal operation (0.0 - 1.0)
    pub memory_low: f64,
}

impl Default for ResourceThresholds {
    fn default() -> Self {
        Self {
            cpu_high: 0.8,     // 80% CPU
            cpu_low: 0.6,      // 60% CPU
            memory_high: 0.85, // 85% memory
            memory_low: 0.7,   // 70% memory
        }
    }
}

impl ResourceThresholds {
    /// Create conservative thresholds (lower limits)
    pub fn conservative() -> Self {
        Self {
            cpu_high: 0.6,
            cpu_low: 0.4,
            memory_high: 0.7,
            memory_low: 0.5,
        }
    }

    /// Create aggressive thresholds (higher limits)
    pub fn aggressive() -> Self {
        Self {
            cpu_high: 0.9,
            cpu_low: 0.75,
            memory_high: 0.95,
            memory_low: 0.85,
        }
    }
}

/// Scheduling policy for resource-aware execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SchedulingPolicy {
    /// Fixed concurrency (no resource awareness)
    Fixed,
    /// Adjust concurrency based on CPU usage
    CpuBased,
    /// Adjust concurrency based on memory usage
    MemoryBased,
    /// Adjust based on both CPU and memory (most conservative)
    #[default]
    Balanced,
}

/// Configuration for resource-aware scheduling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    /// Scheduling policy to use
    pub policy: SchedulingPolicy,
    /// Resource thresholds
    pub thresholds: ResourceThresholds,
    /// Minimum concurrency (never go below this)
    pub min_concurrency: usize,
    /// Maximum concurrency (never exceed this)
    pub max_concurrency: usize,
    /// How often to check resources (in milliseconds)
    pub check_interval_ms: u64,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            policy: SchedulingPolicy::Balanced,
            thresholds: ResourceThresholds::default(),
            min_concurrency: 1,
            max_concurrency: 100,
            check_interval_ms: 1000, // Check every second
        }
    }
}

impl ResourceConfig {
    /// Create a fixed concurrency configuration (no resource monitoring)
    pub fn fixed(concurrency: usize) -> Self {
        Self {
            policy: SchedulingPolicy::Fixed,
            thresholds: ResourceThresholds::default(),
            min_concurrency: concurrency,
            max_concurrency: concurrency,
            check_interval_ms: 1000,
        }
    }

    /// Create a CPU-based configuration
    pub fn cpu_based(min: usize, max: usize) -> Self {
        Self {
            policy: SchedulingPolicy::CpuBased,
            thresholds: ResourceThresholds::default(),
            min_concurrency: min,
            max_concurrency: max,
            check_interval_ms: 500, // Check more frequently for CPU
        }
    }

    /// Create a balanced configuration (default)
    pub fn balanced(min: usize, max: usize) -> Self {
        Self {
            policy: SchedulingPolicy::Balanced,
            thresholds: ResourceThresholds::default(),
            min_concurrency: min,
            max_concurrency: max,
            check_interval_ms: 1000,
        }
    }
}

/// Current resource usage snapshot
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// CPU usage (0.0 - 1.0)
    pub cpu_usage: f64,
    /// Memory usage (0.0 - 1.0)
    pub memory_usage: f64,
    /// Available memory in bytes
    pub available_memory: u64,
    /// Total memory in bytes
    pub total_memory: u64,
    /// Timestamp of measurement
    pub timestamp: u64,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            available_memory: 0,
            total_memory: 0,
            timestamp: 0,
        }
    }
}

/// Resource monitor for tracking system resources
pub struct ResourceMonitor {
    config: ResourceConfig,
    last_check: Arc<std::sync::RwLock<Instant>>,
    current_usage: Arc<std::sync::RwLock<ResourceUsage>>,
    recommended_concurrency: Arc<AtomicUsize>,
    total_adjustments: Arc<AtomicU64>,
}

impl ResourceMonitor {
    /// Create a new resource monitor
    pub fn new(config: ResourceConfig) -> Self {
        let initial_concurrency = (config.min_concurrency + config.max_concurrency) / 2;

        Self {
            config,
            last_check: Arc::new(std::sync::RwLock::new(Instant::now())),
            current_usage: Arc::new(std::sync::RwLock::new(ResourceUsage::default())),
            recommended_concurrency: Arc::new(AtomicUsize::new(initial_concurrency)),
            total_adjustments: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Check if it's time to update resource measurements
    fn should_check(&self) -> bool {
        let last = self.last_check.read().unwrap_or_else(|e| e.into_inner());
        last.elapsed() >= Duration::from_millis(self.config.check_interval_ms)
    }

    /// Update resource usage measurements
    pub fn update(&self) {
        if !self.should_check() {
            return;
        }

        let usage = self.measure_resources();

        // Update stored usage
        {
            let mut current = self.current_usage.write().unwrap_or_else(|e| e.into_inner());
            *current = usage;
        }

        // Update last check time
        {
            let mut last = self.last_check.write().unwrap_or_else(|e| e.into_inner());
            *last = Instant::now();
        }

        // Calculate and update recommended concurrency
        let new_concurrency = self.calculate_concurrency(&usage);
        let old_concurrency = self
            .recommended_concurrency
            .swap(new_concurrency, Ordering::Relaxed);

        if new_concurrency != old_concurrency {
            self.total_adjustments.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Measure current system resources
    fn measure_resources(&self) -> ResourceUsage {
        // For now, return simulated values
        // In production, this would use system APIs like sysinfo crate
        ResourceUsage {
            cpu_usage: 0.5,                           // 50% CPU (placeholder)
            memory_usage: 0.6,                        // 60% memory (placeholder)
            available_memory: 8 * 1024 * 1024 * 1024, // 8GB (placeholder)
            total_memory: 16 * 1024 * 1024 * 1024,    // 16GB (placeholder)
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Calculate recommended concurrency based on current usage
    fn calculate_concurrency(&self, usage: &ResourceUsage) -> usize {
        match self.config.policy {
            SchedulingPolicy::Fixed => self.config.max_concurrency,
            SchedulingPolicy::CpuBased => self.calculate_cpu_based_concurrency(usage.cpu_usage),
            SchedulingPolicy::MemoryBased => {
                self.calculate_memory_based_concurrency(usage.memory_usage)
            }
            SchedulingPolicy::Balanced => {
                let cpu_concurrency = self.calculate_cpu_based_concurrency(usage.cpu_usage);
                let mem_concurrency = self.calculate_memory_based_concurrency(usage.memory_usage);
                cpu_concurrency.min(mem_concurrency) // Most conservative
            }
        }
    }

    /// Calculate concurrency based on CPU usage
    fn calculate_cpu_based_concurrency(&self, cpu_usage: f64) -> usize {
        let thresholds = &self.config.thresholds;
        let range = self.config.max_concurrency - self.config.min_concurrency;

        if cpu_usage >= thresholds.cpu_high {
            // High CPU - reduce concurrency
            self.config.min_concurrency
        } else if cpu_usage <= thresholds.cpu_low {
            // Low CPU - increase concurrency
            self.config.max_concurrency
        } else {
            // Linear interpolation between low and high
            let ratio =
                (thresholds.cpu_high - cpu_usage) / (thresholds.cpu_high - thresholds.cpu_low);
            let additional = (range as f64 * ratio) as usize;
            self.config.min_concurrency + additional
        }
    }

    /// Calculate concurrency based on memory usage
    fn calculate_memory_based_concurrency(&self, memory_usage: f64) -> usize {
        let thresholds = &self.config.thresholds;
        let range = self.config.max_concurrency - self.config.min_concurrency;

        if memory_usage >= thresholds.memory_high {
            // High memory - reduce concurrency
            self.config.min_concurrency
        } else if memory_usage <= thresholds.memory_low {
            // Low memory - increase concurrency
            self.config.max_concurrency
        } else {
            // Linear interpolation between low and high
            let ratio = (thresholds.memory_high - memory_usage)
                / (thresholds.memory_high - thresholds.memory_low);
            let additional = (range as f64 * ratio) as usize;
            self.config.min_concurrency + additional
        }
    }

    /// Get the current recommended concurrency level
    pub fn recommended_concurrency(&self) -> usize {
        self.update(); // Update if needed
        self.recommended_concurrency.load(Ordering::Relaxed)
    }

    /// Get current resource usage
    pub fn current_usage(&self) -> ResourceUsage {
        self.update(); // Update if needed
        *self.current_usage.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Get statistics about resource monitoring
    pub fn stats(&self) -> ResourceStats {
        ResourceStats {
            current_usage: self.current_usage(),
            recommended_concurrency: self.recommended_concurrency.load(Ordering::Relaxed),
            total_adjustments: self.total_adjustments.load(Ordering::Relaxed),
            config: self.config.clone(),
        }
    }

    /// Reset statistics
    pub fn reset(&self) {
        self.total_adjustments.store(0, Ordering::Relaxed);
    }
}

/// Statistics about resource monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStats {
    /// Current resource usage
    pub current_usage: ResourceUsage,
    /// Current recommended concurrency
    pub recommended_concurrency: usize,
    /// Total number of concurrency adjustments made
    pub total_adjustments: u64,
    /// Configuration used
    pub config: ResourceConfig,
}

impl ResourceStats {
    /// Check if system is under high load
    pub fn is_high_load(&self) -> bool {
        self.current_usage.cpu_usage >= self.config.thresholds.cpu_high
            || self.current_usage.memory_usage >= self.config.thresholds.memory_high
    }

    /// Check if system is under low load
    pub fn is_low_load(&self) -> bool {
        self.current_usage.cpu_usage <= self.config.thresholds.cpu_low
            && self.current_usage.memory_usage <= self.config.thresholds.memory_low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_thresholds_default() {
        let thresholds = ResourceThresholds::default();
        assert_eq!(thresholds.cpu_high, 0.8);
        assert_eq!(thresholds.cpu_low, 0.6);
        assert_eq!(thresholds.memory_high, 0.85);
        assert_eq!(thresholds.memory_low, 0.7);
    }

    #[test]
    fn test_resource_thresholds_conservative() {
        let thresholds = ResourceThresholds::conservative();
        assert_eq!(thresholds.cpu_high, 0.6);
        assert_eq!(thresholds.cpu_low, 0.4);
    }

    #[test]
    fn test_resource_thresholds_aggressive() {
        let thresholds = ResourceThresholds::aggressive();
        assert_eq!(thresholds.cpu_high, 0.9);
        assert_eq!(thresholds.cpu_low, 0.75);
    }

    #[test]
    fn test_resource_config_fixed() {
        let config = ResourceConfig::fixed(10);
        assert_eq!(config.policy, SchedulingPolicy::Fixed);
        assert_eq!(config.min_concurrency, 10);
        assert_eq!(config.max_concurrency, 10);
    }

    #[test]
    fn test_resource_config_cpu_based() {
        let config = ResourceConfig::cpu_based(5, 20);
        assert_eq!(config.policy, SchedulingPolicy::CpuBased);
        assert_eq!(config.min_concurrency, 5);
        assert_eq!(config.max_concurrency, 20);
    }

    #[test]
    fn test_resource_config_balanced() {
        let config = ResourceConfig::balanced(10, 50);
        assert_eq!(config.policy, SchedulingPolicy::Balanced);
        assert_eq!(config.min_concurrency, 10);
        assert_eq!(config.max_concurrency, 50);
    }

    #[test]
    fn test_resource_monitor_creation() {
        let config = ResourceConfig::default();
        let monitor = ResourceMonitor::new(config.clone());

        let concurrency = monitor.recommended_concurrency();
        assert!(concurrency >= config.min_concurrency);
        assert!(concurrency <= config.max_concurrency);
    }

    #[test]
    fn test_cpu_based_concurrency_high_usage() {
        let config = ResourceConfig::cpu_based(5, 20);
        let monitor = ResourceMonitor::new(config);

        // Simulate high CPU usage (90%)
        let concurrency = monitor.calculate_cpu_based_concurrency(0.9);
        assert_eq!(concurrency, 5); // Should be at minimum
    }

    #[test]
    fn test_cpu_based_concurrency_low_usage() {
        let config = ResourceConfig::cpu_based(5, 20);
        let monitor = ResourceMonitor::new(config);

        // Simulate low CPU usage (30%)
        let concurrency = monitor.calculate_cpu_based_concurrency(0.3);
        assert_eq!(concurrency, 20); // Should be at maximum
    }

    #[test]
    fn test_memory_based_concurrency_high_usage() {
        let config = ResourceConfig {
            policy: SchedulingPolicy::MemoryBased,
            min_concurrency: 5,
            max_concurrency: 20,
            ..Default::default()
        };
        let monitor = ResourceMonitor::new(config);

        // Simulate high memory usage (90%)
        let concurrency = monitor.calculate_memory_based_concurrency(0.9);
        assert_eq!(concurrency, 5); // Should be at minimum
    }

    #[test]
    fn test_resource_stats_high_load() {
        let usage = ResourceUsage {
            cpu_usage: 0.85,
            memory_usage: 0.9,
            ..Default::default()
        };

        let stats = ResourceStats {
            current_usage: usage,
            recommended_concurrency: 5,
            total_adjustments: 10,
            config: ResourceConfig::default(),
        };

        assert!(stats.is_high_load());
        assert!(!stats.is_low_load());
    }

    #[test]
    fn test_resource_stats_low_load() {
        let usage = ResourceUsage {
            cpu_usage: 0.3,
            memory_usage: 0.4,
            ..Default::default()
        };

        let stats = ResourceStats {
            current_usage: usage,
            recommended_concurrency: 20,
            total_adjustments: 5,
            config: ResourceConfig::default(),
        };

        assert!(!stats.is_high_load());
        assert!(stats.is_low_load());
    }

    #[test]
    fn test_resource_monitor_stats() {
        let config = ResourceConfig::default();
        let monitor = ResourceMonitor::new(config);

        let stats = monitor.stats();
        assert!(stats.recommended_concurrency > 0);
        assert_eq!(stats.total_adjustments, 0); // No adjustments yet
    }

    #[test]
    fn test_resource_monitor_reset() {
        let config = ResourceConfig::default();
        let monitor = ResourceMonitor::new(config);

        // Simulate some adjustments
        monitor.total_adjustments.store(10, Ordering::Relaxed);
        assert_eq!(monitor.stats().total_adjustments, 10);

        monitor.reset();
        assert_eq!(monitor.stats().total_adjustments, 0);
    }
}
