//! Connection pool auto-tuner for dynamic pool sizing
//!
//! This module provides utilities for automatically adjusting database connection
//! pool sizes based on workload metrics and performance characteristics.
//!
//! # How It Works
//!
//! 1. **Monitor**: Track pool utilization, wait times, and query latency
//! 2. **Analyze**: Detect patterns indicating pool is too small or too large
//! 3. **Adjust**: Increase or decrease pool size within configured bounds
//! 4. **Stabilize**: Avoid oscillation with dampening and cooldown periods
//!
//! # Usage
//!
//! ```ignore
//! use oxify_storage::pool_tuner::{PoolTuner, TuningConfig};
//!
//! let config = TuningConfig::default()
//!     .with_min_connections(2)
//!     .with_max_connections(50)
//!     .with_target_utilization(0.7);
//!
//! let tuner = PoolTuner::new(pool.clone(), config);
//!
//! // Run tuning loop in background
//! tokio::spawn(async move {
//!     tuner.run_tuning_loop().await;
//! });
//! ```

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{DatabasePool, PoolMetrics};

/// Configuration for pool auto-tuning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningConfig {
    /// Minimum number of connections
    pub min_connections: u32,
    /// Maximum number of connections
    pub max_connections: u32,
    /// Target pool utilization (0.0 - 1.0)
    pub target_utilization: f64,
    /// High utilization threshold for scaling up
    pub scale_up_threshold: f64,
    /// Low utilization threshold for scaling down
    pub scale_down_threshold: f64,
    /// How often to check metrics and adjust (seconds)
    pub check_interval: Duration,
    /// Cooldown period after adjustment (seconds)
    pub cooldown_period: Duration,
    /// Minimum sustained period before adjustment
    pub sustained_period: Duration,
    /// Maximum adjustment per tuning cycle
    pub max_adjustment: u32,
    /// Enable automatic tuning
    pub enabled: bool,
}

impl TuningConfig {
    /// Create a new tuning configuration with defaults
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::pool_tuner::TuningConfig;
    /// let config = TuningConfig::new();
    /// assert_eq!(config.min_connections, 5);
    /// assert_eq!(config.max_connections, 50);
    /// ```
    pub fn new() -> Self {
        Self {
            min_connections: 5,
            max_connections: 50,
            target_utilization: 0.70,
            scale_up_threshold: 0.80,
            scale_down_threshold: 0.30,
            check_interval: Duration::from_secs(60),
            cooldown_period: Duration::from_secs(300),
            sustained_period: Duration::from_secs(180),
            max_adjustment: 5,
            enabled: true,
        }
    }

    /// Set minimum connections
    pub fn with_min_connections(mut self, min: u32) -> Self {
        self.min_connections = min;
        self
    }

    /// Set maximum connections
    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Set target utilization (0.0 - 1.0)
    pub fn with_target_utilization(mut self, target: f64) -> Self {
        self.target_utilization = target.clamp(0.0, 1.0);
        self
    }

    /// Set check interval
    pub fn with_check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    /// Disable auto-tuning
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.min_connections > self.max_connections {
            return Err("min_connections must be <= max_connections".to_string());
        }

        if !(0.0..=1.0).contains(&self.target_utilization) {
            return Err("target_utilization must be between 0.0 and 1.0".to_string());
        }

        if self.scale_down_threshold >= self.scale_up_threshold {
            return Err("scale_down_threshold must be < scale_up_threshold".to_string());
        }

        Ok(())
    }
}

impl Default for TuningConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Pool tuning decision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuningDecision {
    /// Keep current pool size
    NoChange,
    /// Increase pool size
    ScaleUp(u32),
    /// Decrease pool size
    ScaleDown(u32),
}

/// Pool tuning statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningStats {
    /// Total number of tuning cycles
    pub total_cycles: u64,
    /// Number of scale-up decisions
    pub scale_up_count: u64,
    /// Number of scale-down decisions
    pub scale_down_count: u64,
    /// Number of no-change decisions
    pub no_change_count: u64,
    /// Current pool size
    pub current_size: u32,
    /// Current utilization
    pub current_utilization: f64,
    /// Last adjustment time
    pub last_adjustment: Option<DateTime<Utc>>,
    /// Last decision
    pub last_decision: TuningDecision,
}

impl TuningStats {
    /// Create new tuning statistics
    pub fn new() -> Self {
        Self {
            total_cycles: 0,
            scale_up_count: 0,
            scale_down_count: 0,
            no_change_count: 0,
            current_size: 0,
            current_utilization: 0.0,
            last_adjustment: None,
            last_decision: TuningDecision::NoChange,
        }
    }

    /// Record a tuning decision
    pub fn record_decision(
        &mut self,
        decision: TuningDecision,
        current_size: u32,
        utilization: f64,
    ) {
        self.total_cycles += 1;
        self.current_size = current_size;
        self.current_utilization = utilization;
        self.last_decision = decision;

        match decision {
            TuningDecision::ScaleUp(_) => {
                self.scale_up_count += 1;
                self.last_adjustment = Some(Utc::now());
            }
            TuningDecision::ScaleDown(_) => {
                self.scale_down_count += 1;
                self.last_adjustment = Some(Utc::now());
            }
            TuningDecision::NoChange => {
                self.no_change_count += 1;
            }
        }
    }
}

impl Default for TuningStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Connection pool auto-tuner
pub struct PoolTuner {
    pool: DatabasePool,
    config: TuningConfig,
    stats: TuningStats,
    sustained_high_util_since: Option<DateTime<Utc>>,
    sustained_low_util_since: Option<DateTime<Utc>>,
}

impl PoolTuner {
    /// Create a new pool tuner
    ///
    /// # Examples
    /// ```ignore
    /// # use oxify_storage::pool_tuner::{PoolTuner, TuningConfig};
    /// let tuner = PoolTuner::new(pool.clone(), TuningConfig::default());
    /// ```
    pub fn new(pool: DatabasePool, config: TuningConfig) -> Self {
        Self {
            pool,
            config,
            stats: TuningStats::new(),
            sustained_high_util_since: None,
            sustained_low_util_since: None,
        }
    }

    /// Get tuning statistics
    pub fn stats(&self) -> &TuningStats {
        &self.stats
    }

    /// Analyze current metrics and decide on pool adjustment
    pub fn analyze(&mut self, metrics: &PoolMetrics) -> TuningDecision {
        if !self.config.enabled {
            return TuningDecision::NoChange;
        }

        let utilization = metrics.stats.utilization();
        let current_size = metrics.stats.active_connections() as u32;

        // Check if we're in cooldown period
        if let Some(last_adjustment) = self.stats.last_adjustment {
            let elapsed = Utc::now().signed_duration_since(last_adjustment);
            if elapsed < chrono::Duration::from_std(self.config.cooldown_period).unwrap() {
                return TuningDecision::NoChange;
            }
        }

        // Detect sustained high utilization
        if utilization >= self.config.scale_up_threshold {
            if self.sustained_high_util_since.is_none() {
                self.sustained_high_util_since = Some(Utc::now());
            }
            self.sustained_low_util_since = None;

            // Check if sustained long enough
            if let Some(since) = self.sustained_high_util_since {
                let elapsed = Utc::now().signed_duration_since(since);
                if elapsed >= chrono::Duration::from_std(self.config.sustained_period).unwrap() {
                    // Scale up
                    let target_size = (current_size as f64 * 1.5) as u32;
                    let increase = (target_size - current_size).min(self.config.max_adjustment);
                    let new_size = (current_size + increase).min(self.config.max_connections);

                    if new_size > current_size {
                        return TuningDecision::ScaleUp(new_size - current_size);
                    }
                }
            }
        }
        // Detect sustained low utilization
        else if utilization <= self.config.scale_down_threshold {
            if self.sustained_low_util_since.is_none() {
                self.sustained_low_util_since = Some(Utc::now());
            }
            self.sustained_high_util_since = None;

            // Check if sustained long enough
            if let Some(since) = self.sustained_low_util_since {
                let elapsed = Utc::now().signed_duration_since(since);
                if elapsed >= chrono::Duration::from_std(self.config.sustained_period).unwrap() {
                    // Scale down
                    let target_size = (current_size as f64 * 0.7) as u32;
                    let decrease = (current_size - target_size).min(self.config.max_adjustment);
                    let new_size = current_size
                        .saturating_sub(decrease)
                        .max(self.config.min_connections);

                    if new_size < current_size {
                        return TuningDecision::ScaleDown(current_size - new_size);
                    }
                }
            }
        } else {
            // Utilization is in normal range
            self.sustained_high_util_since = None;
            self.sustained_low_util_since = None;
        }

        TuningDecision::NoChange
    }

    /// Apply a tuning decision
    ///
    /// Note: Actual pool resizing would require underlying pool support.
    /// This is a placeholder for the logic.
    #[allow(dead_code)]
    pub async fn apply_decision(&mut self, decision: TuningDecision) {
        match decision {
            TuningDecision::ScaleUp(amount) => {
                tracing::info!("Scaling up pool by {} connections", amount);
                // In a real implementation, this would call pool.set_max_connections()
                // or similar method to adjust the pool size
            }
            TuningDecision::ScaleDown(amount) => {
                tracing::info!("Scaling down pool by {} connections", amount);
                // In a real implementation, this would call pool.set_max_connections()
                // or similar method to adjust the pool size
            }
            TuningDecision::NoChange => {}
        }
    }

    /// Run the tuning loop continuously
    ///
    /// This should be spawned as a background task.
    #[allow(dead_code)]
    pub async fn run_tuning_loop(mut self) {
        let mut interval = tokio::time::interval(self.config.check_interval);

        loop {
            interval.tick().await;

            if !self.config.enabled {
                continue;
            }

            // Get current metrics
            let metrics = self.pool.metrics();
            let utilization = metrics.stats.utilization();
            let current_size = metrics.stats.active_connections() as u32;

            // Analyze and decide
            let decision = self.analyze(&metrics);

            // Record decision
            self.stats
                .record_decision(decision, current_size, utilization);

            // Apply decision
            if decision != TuningDecision::NoChange {
                self.apply_decision(decision).await;

                tracing::info!(
                    decision = ?decision,
                    utilization = %utilization,
                    current_size = current_size,
                    "Pool tuning decision made"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tuning_config_default() {
        let config = TuningConfig::default();
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.max_connections, 50);
        assert_eq!(config.target_utilization, 0.70);
        assert!(config.enabled);
    }

    #[test]
    fn test_tuning_config_builder() {
        let config = TuningConfig::default()
            .with_min_connections(10)
            .with_max_connections(100)
            .with_target_utilization(0.75);

        assert_eq!(config.min_connections, 10);
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.target_utilization, 0.75);
    }

    #[test]
    fn test_tuning_config_validation() {
        // Valid config
        let config = TuningConfig::default();
        assert!(config.validate().is_ok());

        // Invalid: min > max
        let config = TuningConfig::default()
            .with_min_connections(100)
            .with_max_connections(50);
        assert!(config.validate().is_err());

        // Invalid: target utilization out of range
        let config = TuningConfig::default().with_target_utilization(1.5);
        assert_eq!(config.target_utilization, 1.0); // Clamped

        // Invalid: scale_down >= scale_up
        let config = TuningConfig {
            scale_down_threshold: 0.9,
            scale_up_threshold: 0.8,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_tuning_stats_record_decision() {
        let mut stats = TuningStats::new();

        stats.record_decision(TuningDecision::ScaleUp(5), 20, 0.85);
        assert_eq!(stats.scale_up_count, 1);
        assert_eq!(stats.total_cycles, 1);
        assert_eq!(stats.current_size, 20);
        assert_eq!(stats.current_utilization, 0.85);
        assert!(stats.last_adjustment.is_some());

        stats.record_decision(TuningDecision::NoChange, 20, 0.70);
        assert_eq!(stats.no_change_count, 1);
        assert_eq!(stats.total_cycles, 2);

        stats.record_decision(TuningDecision::ScaleDown(3), 17, 0.25);
        assert_eq!(stats.scale_down_count, 1);
        assert_eq!(stats.total_cycles, 3);
    }

    #[test]
    fn test_tuning_decision_equality() {
        assert_eq!(TuningDecision::NoChange, TuningDecision::NoChange);
        assert_eq!(TuningDecision::ScaleUp(5), TuningDecision::ScaleUp(5));
        assert_ne!(TuningDecision::ScaleUp(5), TuningDecision::ScaleUp(10));
    }
}
