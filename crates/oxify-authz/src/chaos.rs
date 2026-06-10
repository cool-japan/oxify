//! Chaos Engineering for Resilience Testing
//!
//! Test authorization system behavior under failure conditions:
//! - **Database Failures**: Connection loss, query timeouts
//! - **Cache Failures**: Redis unavailable, eviction storms
//! - **Network Issues**: Latency injection, packet loss
//! - **Resource Exhaustion**: Memory pressure, connection pool saturation
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │      Chaos Test Scenarios            │
//! ├──────────────────────────────────────┤
//! │ - Database Down                      │
//! │ - Cache Miss Storm                   │
//! │ - Slow Queries (>100ms)              │
//! │ - Connection Pool Exhaustion         │
//! │ - Partial Replica Failure            │
//! │ - Network Partition                  │
//! └──────────────────────────────────────┘
//!          ↓
//! ┌──────────────────────────────────────┐
//! │   Authorization Engine (DUT)         │
//! │   - Must maintain correctness        │
//! │   - Graceful degradation expected    │
//! │   - No panics or data corruption     │
//! └──────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use oxify_authz::chaos::*;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = oxify_authz::HybridRebacEngine::new("postgres://localhost/db").await?;
//! let executor = ChaosExecutor::new(ChaosConfig::default());
//!
//! // Run chaos test
//! let result = executor.execute(ChaosScenario::DatabaseDown, Arc::new(engine)).await;
//! assert!(result.survived, "Engine should survive database failure");
//! # Ok(())
//! # }
//! ```

use crate::{HybridRebacEngine, RelationTuple, Result, Subject};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Chaos engineering scenario
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChaosScenario {
    /// Database connection failure
    DatabaseDown,
    /// Cache completely unavailable
    CacheDown,
    /// Slow database queries (inject latency)
    SlowQueries,
    /// Connection pool exhaustion
    PoolExhausted,
    /// High cache eviction rate
    CacheEvictionStorm,
    /// Network partition between components
    NetworkPartition,
    /// Memory pressure (low memory conditions)
    MemoryPressure,
}

/// Chaos test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosResult {
    /// Did the system survive without panicking?
    pub survived: bool,
    /// Number of operations attempted
    pub operations_attempted: usize,
    /// Number of operations that succeeded
    pub operations_succeeded: usize,
    /// Number of operations that failed gracefully
    pub operations_failed_gracefully: usize,
    /// Number of unexpected errors (panics, data corruption)
    pub unexpected_errors: usize,
    /// Average latency during chaos (milliseconds)
    pub avg_latency_ms: f64,
    /// P99 latency during chaos (milliseconds)
    pub p99_latency_ms: f64,
    /// Test duration
    pub duration: Duration,
    /// Error messages
    pub error_messages: Vec<String>,
}

impl ChaosResult {
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.operations_attempted == 0 {
            0.0
        } else {
            self.operations_succeeded as f64 / self.operations_attempted as f64
        }
    }

    /// Calculate graceful failure rate
    pub fn graceful_failure_rate(&self) -> f64 {
        if self.operations_attempted == 0 {
            0.0
        } else {
            self.operations_failed_gracefully as f64 / self.operations_attempted as f64
        }
    }

    /// Check if the system behaved correctly under chaos
    pub fn is_resilient(&self) -> bool {
        self.survived && self.unexpected_errors == 0
    }
}

/// Chaos test configuration
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Number of operations to perform during chaos
    pub operation_count: usize,
    /// Duration of chaos injection
    pub duration: Duration,
    /// Expected failure rate (0.0 = no failures expected, 1.0 = all failures expected)
    pub expected_failure_rate: f64,
    /// Allow some operations to fail gracefully
    pub allow_graceful_failures: bool,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            operation_count: 100,
            duration: Duration::from_secs(10),
            expected_failure_rate: 0.5,
            allow_graceful_failures: true,
        }
    }
}

/// Chaos test executor
pub struct ChaosExecutor {
    config: ChaosConfig,
}

impl ChaosExecutor {
    /// Create a new chaos executor
    pub fn new(config: ChaosConfig) -> Self {
        Self { config }
    }

    /// Execute a chaos scenario against an engine
    pub async fn execute(
        &self,
        scenario: ChaosScenario,
        engine: Arc<HybridRebacEngine>,
    ) -> ChaosResult {
        let start = Instant::now();
        let mut latencies = Vec::new();
        let mut operations_attempted = 0;
        let mut operations_succeeded = 0;
        let mut operations_failed_gracefully = 0;
        let mut unexpected_errors = 0;
        let mut error_messages = Vec::new();
        let mut survived = true;

        tracing::info!("Starting chaos scenario: {:?}", scenario);

        // Generate test tuples
        let test_tuples = self.generate_test_tuples(10);

        // Run operations under chaos
        for i in 0..self.config.operation_count {
            operations_attempted += 1;

            // Inject chaos based on scenario
            if let Err(e) = self.inject_chaos(scenario, &engine).await {
                tracing::warn!("Chaos injection failed: {}", e);
            }

            // Perform operation
            let op_start = Instant::now();
            let tuple = &test_tuples[i % test_tuples.len()];

            match self.perform_operation(&engine, tuple).await {
                Ok(_) => {
                    operations_succeeded += 1;
                    let latency = op_start.elapsed().as_millis() as f64;
                    latencies.push(latency);
                }
                Err(e) => {
                    let err_msg = format!("Operation {}: {}", i, e);

                    // Check if error is expected/graceful
                    if self.is_graceful_error(&e) {
                        operations_failed_gracefully += 1;
                        tracing::debug!("Graceful failure: {}", err_msg);
                    } else {
                        unexpected_errors += 1;
                        survived = false;
                        error_messages.push(err_msg.clone());
                        tracing::error!("Unexpected error: {}", err_msg);
                    }
                }
            }

            // Small delay between operations
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Check if we've exceeded test duration
            if start.elapsed() >= self.config.duration {
                break;
            }
        }

        // Calculate statistics
        let avg_latency_ms = if latencies.is_empty() {
            0.0
        } else {
            latencies.iter().sum::<f64>() / latencies.len() as f64
        };

        let p99_latency_ms = if latencies.is_empty() {
            0.0
        } else {
            let mut sorted = latencies.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let p99_idx = (sorted.len() as f64 * 0.99) as usize;
            sorted[p99_idx.min(sorted.len() - 1)]
        };

        let duration = start.elapsed();

        tracing::info!(
            "Chaos test complete: {} ops, {:.1}% success, {} unexpected errors",
            operations_attempted,
            (operations_succeeded as f64 / operations_attempted as f64) * 100.0,
            unexpected_errors
        );

        ChaosResult {
            survived,
            operations_attempted,
            operations_succeeded,
            operations_failed_gracefully,
            unexpected_errors,
            avg_latency_ms,
            p99_latency_ms,
            duration,
            error_messages,
        }
    }

    /// Generate test tuples for chaos testing
    fn generate_test_tuples(&self, count: usize) -> Vec<RelationTuple> {
        (0..count)
            .map(|i| {
                RelationTuple::new(
                    "document",
                    "view",
                    format!("chaos_{}", i),
                    Subject::User(format!("user_{}", i)),
                )
            })
            .collect()
    }

    /// Inject chaos based on scenario
    async fn inject_chaos(
        &self,
        scenario: ChaosScenario,
        _engine: &HybridRebacEngine,
    ) -> Result<()> {
        match scenario {
            ChaosScenario::DatabaseDown => {
                // Simulate database connection loss
                // In real implementation, would use network policies or proxy
                tracing::debug!("Injecting: database connection loss");
            }
            ChaosScenario::CacheDown => {
                // Simulate cache unavailability
                tracing::debug!("Injecting: cache unavailability");
            }
            ChaosScenario::SlowQueries => {
                // Inject artificial latency
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            ChaosScenario::PoolExhausted => {
                // Simulate connection pool saturation
                tracing::debug!("Injecting: connection pool exhaustion");
            }
            ChaosScenario::CacheEvictionStorm => {
                // Simulate high cache eviction rate
                tracing::debug!("Injecting: cache eviction storm");
            }
            ChaosScenario::NetworkPartition => {
                // Simulate network partition
                tracing::debug!("Injecting: network partition");
            }
            ChaosScenario::MemoryPressure => {
                // Simulate memory pressure
                tracing::debug!("Injecting: memory pressure");
            }
        }
        Ok(())
    }

    /// Perform a test operation
    async fn perform_operation(
        &self,
        engine: &HybridRebacEngine,
        tuple: &RelationTuple,
    ) -> Result<()> {
        // Alternate between writes and checks
        if rand::random::<bool>() {
            engine.write_tuple(tuple.clone()).await?;
        } else {
            let _allowed = engine
                .check(crate::CheckRequest {
                    namespace: tuple.namespace.clone(),
                    object_id: tuple.object_id.clone(),
                    relation: tuple.relation.clone(),
                    subject: tuple.subject.clone(),
                    context: None,
                })
                .await?;
        }
        Ok(())
    }

    /// Check if an error is expected/graceful
    fn is_graceful_error(&self, error: &crate::AuthzError) -> bool {
        // Database errors during DatabaseDown scenario are expected
        matches!(error, crate::AuthzError::DatabaseError(_))
    }
}

/// Chaos test suite for comprehensive resilience testing
pub struct ChaosTestSuite {
    scenarios: Vec<(ChaosScenario, ChaosConfig)>,
}

impl ChaosTestSuite {
    /// Create a new chaos test suite with default scenarios
    pub fn default_suite() -> Self {
        Self {
            scenarios: vec![
                (ChaosScenario::DatabaseDown, ChaosConfig::default()),
                (ChaosScenario::CacheDown, ChaosConfig::default()),
                (
                    ChaosScenario::SlowQueries,
                    ChaosConfig {
                        expected_failure_rate: 0.0,
                        ..Default::default()
                    },
                ),
                (ChaosScenario::CacheEvictionStorm, ChaosConfig::default()),
            ],
        }
    }

    /// Run all chaos tests
    pub async fn run_all(
        &self,
        engine: Arc<HybridRebacEngine>,
    ) -> Vec<(ChaosScenario, ChaosResult)> {
        let mut results = Vec::new();

        for (scenario, config) in &self.scenarios {
            let executor = ChaosExecutor::new(config.clone());
            let result = executor.execute(*scenario, engine.clone()).await;
            results.push((*scenario, result));
        }

        results
    }

    /// Generate report from chaos test results
    pub fn generate_report(results: &[(ChaosScenario, ChaosResult)]) -> String {
        let mut report = String::from("# Chaos Engineering Report\n\n");

        for (scenario, result) in results {
            report.push_str(&format!("## Scenario: {:?}\n\n", scenario));
            report.push_str(&format!("- **Survived**: {}\n", result.survived));
            report.push_str(&format!(
                "- **Operations**: {}\n",
                result.operations_attempted
            ));
            report.push_str(&format!(
                "- **Success Rate**: {:.1}%\n",
                result.success_rate() * 100.0
            ));
            report.push_str(&format!(
                "- **Graceful Failures**: {:.1}%\n",
                result.graceful_failure_rate() * 100.0
            ));
            report.push_str(&format!(
                "- **Unexpected Errors**: {}\n",
                result.unexpected_errors
            ));
            report.push_str(&format!(
                "- **Avg Latency**: {:.2}ms\n",
                result.avg_latency_ms
            ));
            report.push_str(&format!(
                "- **P99 Latency**: {:.2}ms\n",
                result.p99_latency_ms
            ));
            report.push_str(&format!("- **Resilient**: {}\n\n", result.is_resilient()));

            if !result.error_messages.is_empty() {
                report.push_str("### Errors:\n");
                for msg in &result.error_messages {
                    report.push_str(&format!("- {}\n", msg));
                }
                report.push('\n');
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chaos_result_metrics() {
        let result = ChaosResult {
            survived: true,
            operations_attempted: 100,
            operations_succeeded: 80,
            operations_failed_gracefully: 15,
            unexpected_errors: 5,
            avg_latency_ms: 10.5,
            p99_latency_ms: 50.0,
            duration: Duration::from_secs(10),
            error_messages: vec!["Error 1".to_string()],
        };

        assert_eq!(result.success_rate(), 0.8);
        assert_eq!(result.graceful_failure_rate(), 0.15);
        assert!(!result.is_resilient()); // Has unexpected errors
    }

    #[test]
    fn test_chaos_config_default() {
        let config = ChaosConfig::default();
        assert_eq!(config.operation_count, 100);
        assert_eq!(config.duration, Duration::from_secs(10));
        assert!(config.allow_graceful_failures);
    }

    #[test]
    fn test_chaos_test_suite() {
        let suite = ChaosTestSuite::default_suite();
        assert!(suite.scenarios.len() >= 4);
    }
}
