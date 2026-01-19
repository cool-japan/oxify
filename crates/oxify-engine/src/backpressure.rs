//! Backpressure handling for workflow execution
//!
//! This module provides mechanisms to prevent system overload during workflow execution
//! by controlling the flow of node execution based on queue sizes and resource availability.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Strategy for handling backpressure when limits are exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackpressureStrategy {
    /// Block execution until queue space is available
    Block,
    /// Drop new tasks when queue is full (fails fast)
    Drop,
    /// Throttle execution by adding delays
    Throttle,
    /// Allow execution to continue (no backpressure)
    None,
}

/// Configuration for backpressure handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackpressureConfig {
    /// Strategy to use when backpressure is triggered
    pub strategy: BackpressureStrategy,
    /// Maximum number of queued nodes before backpressure kicks in
    pub max_queued_nodes: usize,
    /// Maximum number of active (executing) nodes
    pub max_active_nodes: usize,
    /// Throttle delay when using Throttle strategy
    pub throttle_delay: Duration,
    /// High water mark (percentage of max) to start applying backpressure
    pub high_water_mark: f64,
    /// Low water mark (percentage of max) to stop applying backpressure
    pub low_water_mark: f64,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            strategy: BackpressureStrategy::Throttle,
            max_queued_nodes: 1000,
            max_active_nodes: 100,
            throttle_delay: Duration::from_millis(100),
            high_water_mark: 0.8,
            low_water_mark: 0.6,
        }
    }
}

impl BackpressureConfig {
    /// Create a new backpressure configuration with custom settings
    pub fn new(
        strategy: BackpressureStrategy,
        max_queued_nodes: usize,
        max_active_nodes: usize,
    ) -> Self {
        Self {
            strategy,
            max_queued_nodes,
            max_active_nodes,
            ..Default::default()
        }
    }

    /// Create a configuration with no backpressure (unlimited)
    pub fn unlimited() -> Self {
        Self {
            strategy: BackpressureStrategy::None,
            max_queued_nodes: usize::MAX,
            max_active_nodes: usize::MAX,
            ..Default::default()
        }
    }

    /// Create a configuration with strict limits
    pub fn strict(max_queued: usize, max_active: usize) -> Self {
        Self {
            strategy: BackpressureStrategy::Block,
            max_queued_nodes: max_queued,
            max_active_nodes: max_active,
            high_water_mark: 0.9,
            low_water_mark: 0.7,
            ..Default::default()
        }
    }

    /// Calculate the high water mark threshold
    pub fn high_water_threshold(&self) -> usize {
        (self.max_queued_nodes as f64 * self.high_water_mark) as usize
    }

    /// Calculate the low water mark threshold
    pub fn low_water_threshold(&self) -> usize {
        (self.max_queued_nodes as f64 * self.low_water_mark) as usize
    }
}

/// Current backpressure state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackpressureState {
    /// Normal operation, no backpressure
    Normal,
    /// Warning level - approaching limits
    Warning,
    /// Backpressure active - limits exceeded
    Active,
}

/// Monitor for tracking and applying backpressure
pub struct BackpressureMonitor {
    config: BackpressureConfig,
    queued_count: Arc<AtomicUsize>,
    active_count: Arc<AtomicUsize>,
    total_blocked: Arc<AtomicUsize>,
    total_dropped: Arc<AtomicUsize>,
    total_throttled: Arc<AtomicUsize>,
}

impl BackpressureMonitor {
    /// Create a new backpressure monitor with the given configuration
    pub fn new(config: BackpressureConfig) -> Self {
        Self {
            config,
            queued_count: Arc::new(AtomicUsize::new(0)),
            active_count: Arc::new(AtomicUsize::new(0)),
            total_blocked: Arc::new(AtomicUsize::new(0)),
            total_dropped: Arc::new(AtomicUsize::new(0)),
            total_throttled: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get the current backpressure state
    pub fn state(&self) -> BackpressureState {
        let queued = self.queued_count.load(Ordering::Relaxed);
        let active = self.active_count.load(Ordering::Relaxed);

        if queued >= self.config.max_queued_nodes || active >= self.config.max_active_nodes {
            BackpressureState::Active
        } else if queued >= self.config.high_water_threshold() {
            BackpressureState::Warning
        } else {
            BackpressureState::Normal
        }
    }

    /// Check if backpressure should be applied
    pub fn should_apply_backpressure(&self) -> bool {
        matches!(self.state(), BackpressureState::Active)
    }

    /// Record a node being queued
    pub fn record_queued(&self) {
        self.queued_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a node being dequeued (starting execution)
    pub fn record_dequeued(&self) {
        self.queued_count.fetch_sub(1, Ordering::Relaxed);
        self.active_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a node completing execution
    pub fn record_completed(&self) {
        self.active_count.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record a blocked operation (when using Block strategy)
    pub fn record_blocked(&self) {
        self.total_blocked.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a dropped task (when using Drop strategy)
    pub fn record_dropped(&self) {
        self.total_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a throttled operation (when using Throttle strategy)
    pub fn record_throttled(&self) {
        self.total_throttled.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the current number of queued nodes
    pub fn queued_count(&self) -> usize {
        self.queued_count.load(Ordering::Relaxed)
    }

    /// Get the current number of active nodes
    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::Relaxed)
    }

    /// Get statistics about backpressure events
    pub fn stats(&self) -> BackpressureStats {
        BackpressureStats {
            queued_count: self.queued_count(),
            active_count: self.active_count(),
            total_blocked: self.total_blocked.load(Ordering::Relaxed),
            total_dropped: self.total_dropped.load(Ordering::Relaxed),
            total_throttled: self.total_throttled.load(Ordering::Relaxed),
            state: self.state(),
            config: self.config.clone(),
        }
    }

    /// Reset all counters
    pub fn reset(&self) {
        self.queued_count.store(0, Ordering::Relaxed);
        self.active_count.store(0, Ordering::Relaxed);
        self.total_blocked.store(0, Ordering::Relaxed);
        self.total_dropped.store(0, Ordering::Relaxed);
        self.total_throttled.store(0, Ordering::Relaxed);
    }

    /// Get the configured throttle delay
    pub fn throttle_delay(&self) -> Duration {
        self.config.throttle_delay
    }

    /// Get the backpressure strategy
    pub fn strategy(&self) -> BackpressureStrategy {
        self.config.strategy
    }
}

/// Statistics about backpressure events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackpressureStats {
    /// Current number of queued nodes
    pub queued_count: usize,
    /// Current number of active (executing) nodes
    pub active_count: usize,
    /// Total number of blocked operations
    pub total_blocked: usize,
    /// Total number of dropped tasks
    pub total_dropped: usize,
    /// Total number of throttled operations
    pub total_throttled: usize,
    /// Current backpressure state
    pub state: BackpressureState,
    /// Configuration used
    pub config: BackpressureConfig,
}

impl BackpressureStats {
    /// Calculate the utilization percentage (0.0 to 1.0)
    pub fn utilization(&self) -> f64 {
        let max_total = self.config.max_queued_nodes + self.config.max_active_nodes;
        let current_total = self.queued_count + self.active_count;
        (current_total as f64) / (max_total as f64)
    }

    /// Check if any backpressure events occurred
    pub fn has_backpressure_events(&self) -> bool {
        self.total_blocked > 0 || self.total_dropped > 0 || self.total_throttled > 0
    }

    /// Get total backpressure events
    pub fn total_backpressure_events(&self) -> usize {
        self.total_blocked + self.total_dropped + self.total_throttled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backpressure_config_default() {
        let config = BackpressureConfig::default();
        assert_eq!(config.strategy, BackpressureStrategy::Throttle);
        assert_eq!(config.max_queued_nodes, 1000);
        assert_eq!(config.max_active_nodes, 100);
    }

    #[test]
    fn test_backpressure_config_unlimited() {
        let config = BackpressureConfig::unlimited();
        assert_eq!(config.strategy, BackpressureStrategy::None);
        assert_eq!(config.max_queued_nodes, usize::MAX);
        assert_eq!(config.max_active_nodes, usize::MAX);
    }

    #[test]
    fn test_backpressure_config_strict() {
        let config = BackpressureConfig::strict(100, 10);
        assert_eq!(config.strategy, BackpressureStrategy::Block);
        assert_eq!(config.max_queued_nodes, 100);
        assert_eq!(config.max_active_nodes, 10);
    }

    #[test]
    fn test_water_marks() {
        let config = BackpressureConfig {
            max_queued_nodes: 100,
            high_water_mark: 0.8,
            low_water_mark: 0.6,
            ..Default::default()
        };

        assert_eq!(config.high_water_threshold(), 80);
        assert_eq!(config.low_water_threshold(), 60);
    }

    #[test]
    fn test_monitor_state_normal() {
        let config = BackpressureConfig::default();
        let monitor = BackpressureMonitor::new(config);

        assert_eq!(monitor.state(), BackpressureState::Normal);
        assert!(!monitor.should_apply_backpressure());
    }

    #[test]
    fn test_monitor_state_warning() {
        let config = BackpressureConfig {
            max_queued_nodes: 100,
            high_water_mark: 0.8,
            ..Default::default()
        };
        let monitor = BackpressureMonitor::new(config);

        // Add 81 queued nodes (above high water mark of 80)
        for _ in 0..81 {
            monitor.record_queued();
        }

        assert_eq!(monitor.state(), BackpressureState::Warning);
        assert!(!monitor.should_apply_backpressure()); // Warning doesn't trigger backpressure
    }

    #[test]
    fn test_monitor_state_active() {
        let config = BackpressureConfig {
            max_queued_nodes: 100,
            ..Default::default()
        };
        let monitor = BackpressureMonitor::new(config);

        // Add 100 queued nodes (at max)
        for _ in 0..100 {
            monitor.record_queued();
        }

        assert_eq!(monitor.state(), BackpressureState::Active);
        assert!(monitor.should_apply_backpressure());
    }

    #[test]
    fn test_monitor_queued_and_active() {
        let config = BackpressureConfig::default();
        let monitor = BackpressureMonitor::new(config);

        assert_eq!(monitor.queued_count(), 0);
        assert_eq!(monitor.active_count(), 0);

        monitor.record_queued();
        assert_eq!(monitor.queued_count(), 1);
        assert_eq!(monitor.active_count(), 0);

        monitor.record_dequeued();
        assert_eq!(monitor.queued_count(), 0);
        assert_eq!(monitor.active_count(), 1);

        monitor.record_completed();
        assert_eq!(monitor.queued_count(), 0);
        assert_eq!(monitor.active_count(), 0);
    }

    #[test]
    fn test_monitor_backpressure_events() {
        let config = BackpressureConfig::default();
        let monitor = BackpressureMonitor::new(config);

        monitor.record_blocked();
        monitor.record_dropped();
        monitor.record_throttled();

        let stats = monitor.stats();
        assert_eq!(stats.total_blocked, 1);
        assert_eq!(stats.total_dropped, 1);
        assert_eq!(stats.total_throttled, 1);
        assert!(stats.has_backpressure_events());
        assert_eq!(stats.total_backpressure_events(), 3);
    }

    #[test]
    fn test_monitor_reset() {
        let config = BackpressureConfig::default();
        let monitor = BackpressureMonitor::new(config);

        monitor.record_queued();
        monitor.record_blocked();
        monitor.record_dropped();

        assert_eq!(monitor.queued_count(), 1);
        assert_eq!(monitor.stats().total_blocked, 1);

        monitor.reset();

        assert_eq!(monitor.queued_count(), 0);
        assert_eq!(monitor.stats().total_blocked, 0);
        assert_eq!(monitor.stats().total_dropped, 0);
    }

    #[test]
    fn test_stats_utilization() {
        let config = BackpressureConfig {
            max_queued_nodes: 100,
            max_active_nodes: 10,
            ..Default::default()
        };
        let monitor = BackpressureMonitor::new(config);

        // Add 50 queued and 5 active (50% utilization)
        for _ in 0..50 {
            monitor.record_queued();
        }
        for _ in 0..5 {
            monitor.record_dequeued();
        }

        let stats = monitor.stats();
        assert_eq!(stats.queued_count, 45); // 50 - 5
        assert_eq!(stats.active_count, 5);

        // Total = 50, Max = 110, Utilization = 50/110 ≈ 0.45
        let utilization = stats.utilization();
        assert!(utilization > 0.44 && utilization < 0.46);
    }
}
