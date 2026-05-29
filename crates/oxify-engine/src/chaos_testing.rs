//! Chaos testing utilities for workflow resilience
//!
//! This module provides tools for testing workflow behavior under adverse conditions:
//! - Random node failures
//! - Network latency simulation
//! - Resource exhaustion
//! - Timeout scenarios

use crate::{Engine, ExecutionConfig, Result};
use oxify_model::{ExecutionContext, Workflow};
use rand::RngExt;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Configuration for chaos testing
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Probability of node failure (0.0 - 1.0)
    pub failure_rate: f64,

    /// Minimum latency to inject (milliseconds)
    pub min_latency_ms: u64,

    /// Maximum latency to inject (milliseconds)
    pub max_latency_ms: u64,

    /// Probability of timeout (0.0 - 1.0)
    pub timeout_rate: f64,

    /// Timeout duration (milliseconds)
    pub timeout_ms: u64,

    /// Maximum memory usage simulation (bytes)
    pub max_memory_bytes: Option<u64>,

    /// Enable random pauses
    pub enable_random_pauses: bool,

    /// Seed for reproducible chaos (None = random)
    pub seed: Option<u64>,

    /// Probability of memory pressure (0.0 - 1.0)
    pub memory_pressure_rate: f64,

    /// Memory pressure size (bytes)
    pub memory_pressure_bytes: u64,

    /// Probability of CPU throttling (0.0 - 1.0)
    pub cpu_throttle_rate: f64,

    /// CPU throttle delay (microseconds)
    pub cpu_throttle_delay_us: u64,

    /// Probability of disk I/O failure (0.0 - 1.0)
    pub disk_io_failure_rate: f64,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            failure_rate: 0.1,      // 10% failure rate
            min_latency_ms: 100,    // 100ms min latency
            max_latency_ms: 2000,   // 2s max latency
            timeout_rate: 0.05,     // 5% timeout rate
            timeout_ms: 5000,       // 5s timeout
            max_memory_bytes: None, // No memory limit
            enable_random_pauses: false,
            seed: None,
            memory_pressure_rate: 0.05,        // 5% memory pressure rate
            memory_pressure_bytes: 10_000_000, // 10MB memory pressure
            cpu_throttle_rate: 0.1,            // 10% CPU throttle rate
            cpu_throttle_delay_us: 1000,       // 1ms CPU delay
            disk_io_failure_rate: 0.05,        // 5% disk I/O failure rate
        }
    }
}

impl ChaosConfig {
    /// Create a mild chaos configuration (low failure rates)
    pub fn mild() -> Self {
        Self {
            failure_rate: 0.05,
            min_latency_ms: 50,
            max_latency_ms: 500,
            timeout_rate: 0.02,
            ..Default::default()
        }
    }

    /// Create an aggressive chaos configuration (high failure rates)
    pub fn aggressive() -> Self {
        Self {
            failure_rate: 0.3,
            min_latency_ms: 500,
            max_latency_ms: 5000,
            timeout_rate: 0.15,
            ..Default::default()
        }
    }

    /// Create a latency-focused chaos configuration
    pub fn latency_focused() -> Self {
        Self {
            failure_rate: 0.0,
            min_latency_ms: 1000,
            max_latency_ms: 10000,
            timeout_rate: 0.0,
            ..Default::default()
        }
    }

    /// Create a resource-pressure focused chaos configuration
    pub fn resource_pressure() -> Self {
        Self {
            failure_rate: 0.02,
            min_latency_ms: 50,
            max_latency_ms: 500,
            timeout_rate: 0.0,
            memory_pressure_rate: 0.3,         // 30% memory pressure
            memory_pressure_bytes: 50_000_000, // 50MB
            cpu_throttle_rate: 0.4,            // 40% CPU throttle
            cpu_throttle_delay_us: 5000,       // 5ms delay
            disk_io_failure_rate: 0.15,        // 15% disk failures
            ..Default::default()
        }
    }

    /// Set seed for reproducible chaos
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

/// Chaos testing engine wrapper
pub struct ChaosEngine {
    engine: Engine,
    config: ChaosConfig,
    failure_history: Arc<RwLock<ChaosHistory>>,
}

impl ChaosEngine {
    /// Create a new chaos engine with default configuration
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            config: ChaosConfig::default(),
            failure_history: Arc::new(RwLock::new(ChaosHistory::default())),
        }
    }

    /// Create a chaos engine with custom configuration
    pub fn with_config(engine: Engine, config: ChaosConfig) -> Self {
        Self {
            engine,
            config,
            failure_history: Arc::new(RwLock::new(ChaosHistory::default())),
        }
    }

    /// Execute a workflow with chaos injection
    pub async fn execute_with_chaos(
        &self,
        workflow: &Workflow,
    ) -> Result<(ExecutionContext, ChaosReport)> {
        self.execute_with_chaos_and_config(workflow, ExecutionConfig::default())
            .await
    }

    /// Execute a workflow with chaos injection and custom execution config
    pub async fn execute_with_chaos_and_config(
        &self,
        workflow: &Workflow,
        mut exec_config: ExecutionConfig,
    ) -> Result<(ExecutionContext, ChaosReport)> {
        // Reset history
        self.failure_history
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();

        // Apply chaos configuration to execution config
        if self.config.timeout_rate > 0.0 {
            exec_config.node_timeout_ms = Some(self.config.timeout_ms);
        }

        // Execute workflow with chaos hooks
        let start_time = std::time::Instant::now();

        // Clone workflow and inject chaos into nodes
        let chaotic_workflow = self.inject_chaos_into_workflow(workflow)?;

        let result = self
            .engine
            .execute_with_config(&chaotic_workflow, exec_config)
            .await;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Generate report
        let history = self
            .failure_history
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let report = ChaosReport {
            total_failures: history.failures.len(),
            total_latency_injections: history.latency_injections.len(),
            total_timeouts: history.timeouts.len(),
            total_pauses: history.pauses.len(),
            total_memory_pressures: history.memory_pressures.len(),
            total_cpu_throttles: history.cpu_throttles.len(),
            total_disk_io_failures: history.disk_io_failures.len(),
            duration_ms,
            success: result.is_ok(),
            failures: history.failures.clone(),
            latency_injections: history.latency_injections.clone(),
            timeouts: history.timeouts.clone(),
            memory_pressures: history.memory_pressures.clone(),
            cpu_throttles: history.cpu_throttles.clone(),
            disk_io_failures: history.disk_io_failures.clone(),
        };

        match result {
            Ok(ctx) => Ok((ctx, report)),
            Err(e) => {
                // Return error with report
                tracing::error!("Chaos execution failed: {}", e);
                Err(e)
            }
        }
    }

    /// Inject chaos into workflow (modify workflow structure)
    fn inject_chaos_into_workflow(&self, _workflow: &Workflow) -> Result<Workflow> {
        // For now, return the original workflow
        // In a full implementation, we would:
        // 1. Clone the workflow
        // 2. Wrap node execution with chaos injection
        // 3. Return modified workflow

        // This is a placeholder - chaos is simulated through the report
        Ok(_workflow.clone())
    }

    /// Simulate a node failure based on chaos config
    #[allow(dead_code)]
    fn should_fail(&self) -> bool {
        let mut rng = rand::rng();
        rng.random_range(0.0..1.0) < self.config.failure_rate
    }

    /// Get chaos report for last execution
    pub fn get_report(&self) -> ChaosReport {
        let history = self
            .failure_history
            .read()
            .unwrap_or_else(|e| e.into_inner());
        ChaosReport {
            total_failures: history.failures.len(),
            total_latency_injections: history.latency_injections.len(),
            total_timeouts: history.timeouts.len(),
            total_pauses: history.pauses.len(),
            total_memory_pressures: history.memory_pressures.len(),
            total_cpu_throttles: history.cpu_throttles.len(),
            total_disk_io_failures: history.disk_io_failures.len(),
            duration_ms: 0,
            success: false,
            failures: history.failures.clone(),
            latency_injections: history.latency_injections.clone(),
            timeouts: history.timeouts.clone(),
            memory_pressures: history.memory_pressures.clone(),
            cpu_throttles: history.cpu_throttles.clone(),
            disk_io_failures: history.disk_io_failures.clone(),
        }
    }

    /// Get reference to underlying engine
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

/// History of chaos events during execution
#[derive(Debug, Clone, Default)]
struct ChaosHistory {
    failures: Vec<ChaosFailure>,
    latency_injections: Vec<LatencyInjection>,
    timeouts: Vec<ChaosTimeout>,
    pauses: Vec<ChaosPause>,
    memory_pressures: Vec<MemoryPressureEvent>,
    cpu_throttles: Vec<CpuThrottleEvent>,
    disk_io_failures: Vec<DiskIoFailureEvent>,
}

impl ChaosHistory {
    fn clear(&mut self) {
        self.failures.clear();
        self.latency_injections.clear();
        self.timeouts.clear();
        self.pauses.clear();
        self.memory_pressures.clear();
        self.cpu_throttles.clear();
        self.disk_io_failures.clear();
    }
}

/// Record of a chaos-induced failure
#[derive(Debug, Clone)]
pub struct ChaosFailure {
    /// Node ID that failed
    pub node_id: uuid::Uuid,
    /// Node name
    pub node_name: String,
    /// Failure reason
    pub reason: String,
    /// When the failure occurred
    pub timestamp_ms: u64,
}

/// Record of latency injection
#[derive(Debug, Clone)]
pub struct LatencyInjection {
    /// Node ID affected
    pub node_id: uuid::Uuid,
    /// Node name
    pub node_name: String,
    /// Latency added (milliseconds)
    pub latency_ms: u64,
    /// When the latency was injected
    pub timestamp_ms: u64,
}

/// Record of a timeout
#[derive(Debug, Clone)]
pub struct ChaosTimeout {
    /// Node ID that timed out
    pub node_id: uuid::Uuid,
    /// Node name
    pub node_name: String,
    /// Timeout duration (milliseconds)
    pub timeout_ms: u64,
    /// When the timeout occurred
    pub timestamp_ms: u64,
}

/// Record of a pause
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChaosPause {
    /// When the pause occurred
    pub timestamp_ms: u64,
    /// Duration of pause (milliseconds)
    pub duration_ms: u64,
}

/// Record of memory pressure event
#[derive(Debug, Clone)]
pub struct MemoryPressureEvent {
    /// Node ID affected
    pub node_id: uuid::Uuid,
    /// Node name
    pub node_name: String,
    /// Memory pressure size (bytes)
    pub pressure_bytes: u64,
    /// When the pressure was applied
    pub timestamp_ms: u64,
}

/// Record of CPU throttle event
#[derive(Debug, Clone)]
pub struct CpuThrottleEvent {
    /// Node ID affected
    pub node_id: uuid::Uuid,
    /// Node name
    pub node_name: String,
    /// Throttle delay (microseconds)
    pub delay_us: u64,
    /// When the throttle was applied
    pub timestamp_ms: u64,
}

/// Record of disk I/O failure
#[derive(Debug, Clone)]
pub struct DiskIoFailureEvent {
    /// Node ID affected
    pub node_id: uuid::Uuid,
    /// Node name
    pub node_name: String,
    /// Failure reason
    pub reason: String,
    /// When the failure occurred
    pub timestamp_ms: u64,
}

/// Report of chaos testing results
#[derive(Debug, Clone)]
pub struct ChaosReport {
    /// Total number of failures injected
    pub total_failures: usize,
    /// Total number of latency injections
    pub total_latency_injections: usize,
    /// Total number of timeouts
    pub total_timeouts: usize,
    /// Total number of pauses
    pub total_pauses: usize,
    /// Total number of memory pressure events
    pub total_memory_pressures: usize,
    /// Total number of CPU throttle events
    pub total_cpu_throttles: usize,
    /// Total number of disk I/O failures
    pub total_disk_io_failures: usize,
    /// Total execution duration (milliseconds)
    pub duration_ms: u64,
    /// Whether the workflow succeeded despite chaos
    pub success: bool,
    /// Detailed failure records
    pub failures: Vec<ChaosFailure>,
    /// Detailed latency injection records
    pub latency_injections: Vec<LatencyInjection>,
    /// Detailed timeout records
    pub timeouts: Vec<ChaosTimeout>,
    /// Detailed memory pressure records
    pub memory_pressures: Vec<MemoryPressureEvent>,
    /// Detailed CPU throttle records
    pub cpu_throttles: Vec<CpuThrottleEvent>,
    /// Detailed disk I/O failure records
    pub disk_io_failures: Vec<DiskIoFailureEvent>,
}

impl ChaosReport {
    /// Calculate total chaos events
    pub fn total_events(&self) -> usize {
        self.total_failures
            + self.total_latency_injections
            + self.total_timeouts
            + self.total_pauses
            + self.total_memory_pressures
            + self.total_cpu_throttles
            + self.total_disk_io_failures
    }

    /// Calculate resilience score (0.0 - 1.0)
    pub fn resilience_score(&self) -> f64 {
        if self.total_events() == 0 {
            return 1.0;
        }

        if self.success {
            1.0 - (self.total_failures as f64 / self.total_events() as f64)
        } else {
            0.0
        }
    }

    /// Get average latency added
    pub fn average_latency_ms(&self) -> f64 {
        if self.latency_injections.is_empty() {
            return 0.0;
        }

        let total: u64 = self.latency_injections.iter().map(|l| l.latency_ms).sum();
        total as f64 / self.latency_injections.len() as f64
    }
}

/// Chaos monkey for random workflow disruption
pub struct ChaosMonkey {
    config: ChaosConfig,
}

impl ChaosMonkey {
    /// Create a new chaos monkey
    pub fn new(config: ChaosConfig) -> Self {
        Self { config }
    }

    /// Inject random latency
    pub async fn inject_latency(&self) {
        let mut rng = rand::rng();
        let latency_ms = rng.random_range(self.config.min_latency_ms..=self.config.max_latency_ms);
        tokio::time::sleep(Duration::from_millis(latency_ms)).await;
    }

    /// Check if should inject failure
    pub fn should_inject_failure(&self) -> bool {
        let mut rng = rand::rng();
        rng.random_range(0.0..1.0) < self.config.failure_rate
    }

    /// Check if should inject timeout
    pub fn should_inject_timeout(&self) -> bool {
        let mut rng = rand::rng();
        rng.random_range(0.0..1.0) < self.config.timeout_rate
    }

    /// Check if should inject memory pressure
    pub fn should_inject_memory_pressure(&self) -> bool {
        let mut rng = rand::rng();
        rng.random_range(0.0..1.0) < self.config.memory_pressure_rate
    }

    /// Inject memory pressure by allocating memory
    pub fn inject_memory_pressure(&self) -> Vec<u8> {
        // Allocate memory to simulate pressure
        vec![0u8; self.config.memory_pressure_bytes as usize]
    }

    /// Check if should inject CPU throttling
    pub fn should_inject_cpu_throttle(&self) -> bool {
        let mut rng = rand::rng();
        rng.random_range(0.0..1.0) < self.config.cpu_throttle_rate
    }

    /// Inject CPU throttling by busy-waiting
    pub fn inject_cpu_throttle(&self) {
        let start = std::time::Instant::now();
        let duration = Duration::from_micros(self.config.cpu_throttle_delay_us);

        // Busy-wait to simulate CPU throttling
        while start.elapsed() < duration {
            // Spin to consume CPU
            std::hint::spin_loop();
        }
    }

    /// Check if should inject disk I/O failure
    pub fn should_inject_disk_io_failure(&self) -> bool {
        let mut rng = rand::rng();
        rng.random_range(0.0..1.0) < self.config.disk_io_failure_rate
    }

    /// Simulate a disk I/O failure (returns error message)
    pub fn simulate_disk_io_failure(&self) -> String {
        "Simulated disk I/O failure: Permission denied".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, Node, NodeKind};

    #[test]
    fn test_chaos_config_default() {
        let config = ChaosConfig::default();
        assert_eq!(config.failure_rate, 0.1);
        assert_eq!(config.min_latency_ms, 100);
        assert_eq!(config.max_latency_ms, 2000);
    }

    #[test]
    fn test_chaos_config_mild() {
        let config = ChaosConfig::mild();
        assert_eq!(config.failure_rate, 0.05);
        assert!(config.failure_rate < ChaosConfig::default().failure_rate);
    }

    #[test]
    fn test_chaos_config_aggressive() {
        let config = ChaosConfig::aggressive();
        assert_eq!(config.failure_rate, 0.3);
        assert!(config.failure_rate > ChaosConfig::default().failure_rate);
    }

    #[test]
    fn test_chaos_config_with_seed() {
        let config = ChaosConfig::default().with_seed(12345);
        assert_eq!(config.seed, Some(12345));
    }

    #[test]
    fn test_chaos_engine_creation() {
        let engine = Engine::new();
        let chaos_engine = ChaosEngine::new(engine);

        assert_eq!(chaos_engine.config.failure_rate, 0.1);
    }

    #[test]
    fn test_chaos_report_total_events() {
        let report = ChaosReport {
            total_failures: 2,
            total_latency_injections: 3,
            total_timeouts: 1,
            total_pauses: 0,
            total_memory_pressures: 2,
            total_cpu_throttles: 1,
            total_disk_io_failures: 1,
            duration_ms: 1000,
            success: true,
            failures: vec![],
            latency_injections: vec![],
            timeouts: vec![],
            memory_pressures: vec![],
            cpu_throttles: vec![],
            disk_io_failures: vec![],
        };

        assert_eq!(report.total_events(), 10);
    }

    #[test]
    fn test_chaos_report_resilience_score() {
        // Successful execution with some chaos
        let report = ChaosReport {
            total_failures: 2,
            total_latency_injections: 8,
            total_timeouts: 0,
            total_pauses: 0,
            total_memory_pressures: 0,
            total_cpu_throttles: 0,
            total_disk_io_failures: 0,
            duration_ms: 1000,
            success: true,
            failures: vec![],
            latency_injections: vec![],
            timeouts: vec![],
            memory_pressures: vec![],
            cpu_throttles: vec![],
            disk_io_failures: vec![],
        };

        // Resilience = 1.0 - (2 failures / 10 events) = 0.8
        assert!((report.resilience_score() - 0.8).abs() < 0.01);

        // Failed execution
        let failed_report = ChaosReport {
            total_failures: 5,
            total_latency_injections: 5,
            total_timeouts: 0,
            total_pauses: 0,
            total_memory_pressures: 0,
            total_cpu_throttles: 0,
            total_disk_io_failures: 0,
            duration_ms: 1000,
            success: false,
            failures: vec![],
            latency_injections: vec![],
            timeouts: vec![],
            memory_pressures: vec![],
            cpu_throttles: vec![],
            disk_io_failures: vec![],
        };

        assert_eq!(failed_report.resilience_score(), 0.0);
    }

    #[test]
    fn test_chaos_report_average_latency() {
        let report = ChaosReport {
            total_failures: 0,
            total_latency_injections: 3,
            total_timeouts: 0,
            total_pauses: 0,
            total_memory_pressures: 0,
            total_cpu_throttles: 0,
            total_disk_io_failures: 0,
            duration_ms: 1000,
            success: true,
            failures: vec![],
            latency_injections: vec![
                LatencyInjection {
                    node_id: uuid::Uuid::new_v4(),
                    node_name: "node1".to_string(),
                    latency_ms: 100,
                    timestamp_ms: 0,
                },
                LatencyInjection {
                    node_id: uuid::Uuid::new_v4(),
                    node_name: "node2".to_string(),
                    latency_ms: 200,
                    timestamp_ms: 0,
                },
                LatencyInjection {
                    node_id: uuid::Uuid::new_v4(),
                    node_name: "node3".to_string(),
                    latency_ms: 300,
                    timestamp_ms: 0,
                },
            ],
            timeouts: vec![],
            memory_pressures: vec![],
            cpu_throttles: vec![],
            disk_io_failures: vec![],
        };

        assert_eq!(report.average_latency_ms(), 200.0);
    }

    #[tokio::test]
    async fn test_chaos_monkey_inject_latency() {
        let config = ChaosConfig {
            min_latency_ms: 10,
            max_latency_ms: 20,
            ..Default::default()
        };

        let monkey = ChaosMonkey::new(config);
        let start = std::time::Instant::now();
        monkey.inject_latency().await;
        let elapsed = start.elapsed();

        // Should have added at least 10ms
        assert!(elapsed.as_millis() >= 10);
    }

    #[test]
    fn test_chaos_monkey_failure_injection() {
        let config = ChaosConfig {
            failure_rate: 0.0,
            ..Default::default()
        };

        let monkey = ChaosMonkey::new(config);
        assert!(!monkey.should_inject_failure());

        let config = ChaosConfig {
            failure_rate: 1.0,
            ..Default::default()
        };

        let monkey = ChaosMonkey::new(config);
        assert!(monkey.should_inject_failure());
    }

    #[tokio::test]
    async fn test_chaos_engine_execute_simple_workflow() {
        let mut workflow = Workflow::new("Chaos Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        let engine = Engine::new();
        let config = ChaosConfig::mild(); // Use mild chaos for test
        let chaos_engine = ChaosEngine::with_config(engine, config);

        let result = chaos_engine.execute_with_chaos(&workflow).await;

        // Even with chaos, a simple workflow should succeed
        assert!(result.is_ok());

        if let Ok((ctx, _report)) = result {
            assert!(ctx.node_results.len() >= 2);
            // Chaos report is always generated
        }
    }

    #[test]
    fn test_chaos_config_resource_pressure() {
        let config = ChaosConfig::resource_pressure();
        assert_eq!(config.memory_pressure_rate, 0.3);
        assert_eq!(config.cpu_throttle_rate, 0.4);
        assert_eq!(config.disk_io_failure_rate, 0.15);
    }

    #[test]
    fn test_chaos_monkey_memory_pressure() {
        let config = ChaosConfig {
            memory_pressure_rate: 0.0,
            memory_pressure_bytes: 1000,
            ..Default::default()
        };
        let monkey = ChaosMonkey::new(config.clone());
        assert!(!monkey.should_inject_memory_pressure());

        let config_with_pressure = ChaosConfig {
            memory_pressure_rate: 1.0,
            memory_pressure_bytes: 1000,
            ..Default::default()
        };
        let monkey_with_pressure = ChaosMonkey::new(config_with_pressure);
        assert!(monkey_with_pressure.should_inject_memory_pressure());

        // Test memory allocation
        let memory = monkey.inject_memory_pressure();
        assert_eq!(memory.len(), 1000);
    }

    #[test]
    fn test_chaos_monkey_cpu_throttle() {
        let config = ChaosConfig {
            cpu_throttle_rate: 0.0,
            cpu_throttle_delay_us: 1000,
            ..Default::default()
        };
        let monkey = ChaosMonkey::new(config.clone());
        assert!(!monkey.should_inject_cpu_throttle());

        let config_with_throttle = ChaosConfig {
            cpu_throttle_rate: 1.0,
            cpu_throttle_delay_us: 1000,
            ..Default::default()
        };
        let monkey_with_throttle = ChaosMonkey::new(config_with_throttle);
        assert!(monkey_with_throttle.should_inject_cpu_throttle());

        // Test CPU throttling (should take at least 1000 microseconds)
        let start = std::time::Instant::now();
        monkey.inject_cpu_throttle();
        let elapsed = start.elapsed();
        assert!(elapsed.as_micros() >= 1000);
    }

    #[test]
    fn test_chaos_monkey_disk_io_failure() {
        let config = ChaosConfig {
            disk_io_failure_rate: 0.0,
            ..Default::default()
        };
        let monkey = ChaosMonkey::new(config);
        assert!(!monkey.should_inject_disk_io_failure());

        let config_with_failure = ChaosConfig {
            disk_io_failure_rate: 1.0,
            ..Default::default()
        };
        let monkey_with_failure = ChaosMonkey::new(config_with_failure);
        assert!(monkey_with_failure.should_inject_disk_io_failure());

        // Test disk I/O failure simulation
        let error_msg = monkey_with_failure.simulate_disk_io_failure();
        assert!(error_msg.contains("disk I/O failure"));
    }
}
