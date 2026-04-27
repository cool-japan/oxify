//! Health / Readiness Probe subsystem
//!
//! Provides a composable readiness-check framework following the Kubernetes-style
//! liveness / readiness probe model.
//!
//! ## Concepts
//!
//! * [`ReadinessChecker`] — an async, named check (e.g. database ping, Redis ping)
//! * [`ReadinessStatus`] — outcome of a single check: `Ready`, `Degraded`, or `NotReady`
//! * [`CheckerResult`] — wraps a status together with the checker name and elapsed time
//! * [`AggregateReadiness`] — result of running all registered checkers concurrently
//! * [`ReadinessRegistry`] — owns a set of checkers; drives the concurrent execution

use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ─── ReadinessChecker ────────────────────────────────────────────────────────

/// Implemented by anything that can report its own readiness.
///
/// # Contract
///
/// * The implementation **must not** panic.
/// * The call site wraps each invocation in a per-checker timeout, so the
///   implementation does not need to impose its own upper bound.
#[async_trait]
pub trait ReadinessChecker: Send + Sync {
    /// Human-readable identifier (used in aggregated JSON output).
    fn name(&self) -> &str;

    /// Perform the check and return the outcome.
    async fn check(&self) -> ReadinessStatus;
}

// ─── ReadinessStatus ─────────────────────────────────────────────────────────

/// Outcome of a single readiness check.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadinessStatus {
    /// The component is fully operational.
    Ready,
    /// The component is partially functional; the service can still serve traffic
    /// but with reduced capabilities.
    Degraded {
        /// Human-readable explanation of the degradation.
        reason: String,
    },
    /// The component is not operational; the service should not serve traffic.
    NotReady {
        /// Human-readable explanation of why the component is not ready.
        reason: String,
    },
}

impl ReadinessStatus {
    /// Returns `true` for `Ready` or `Degraded`, `false` for `NotReady`.
    #[inline]
    pub fn is_acceptable(&self) -> bool {
        !matches!(self, ReadinessStatus::NotReady { .. })
    }
}

// ─── CheckerResult ───────────────────────────────────────────────────────────

/// The result of running a single [`ReadinessChecker`].
#[derive(Debug, Clone)]
pub struct CheckerResult {
    /// Name of the checker (copied from [`ReadinessChecker::name`]).
    pub name: String,
    /// Outcome of the check.
    pub status: ReadinessStatus,
    /// Wall-clock time the check took, in milliseconds.
    pub duration_ms: u64,
}

// ─── AggregateReadiness ──────────────────────────────────────────────────────

/// Aggregated result of running every checker in a [`ReadinessRegistry`].
#[derive(Debug)]
pub struct AggregateReadiness {
    /// `true` when **all** checkers returned `Ready` or `Degraded`.
    /// `false` as soon as at least one returned `NotReady` (including a timeout).
    pub healthy: bool,
    /// Per-checker results in the order they were registered.
    pub results: Vec<CheckerResult>,
}

impl AggregateReadiness {
    /// Suggested HTTP status code: 200 when healthy, 503 otherwise.
    pub fn status_code(&self) -> u16 {
        if self.healthy {
            200
        } else {
            503
        }
    }
}

// ─── ReadinessRegistry ───────────────────────────────────────────────────────

/// Manages a collection of [`ReadinessChecker`] instances and runs them
/// concurrently with a per-checker timeout.
pub struct ReadinessRegistry {
    checkers: Vec<Arc<dyn ReadinessChecker>>,
    /// Per-checker timeout. If a check exceeds this limit it is treated as
    /// `NotReady`.
    timeout: Duration,
}

impl Default for ReadinessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadinessRegistry {
    /// Create a registry with the default 2-second per-checker timeout.
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(2))
    }

    /// Create a registry with a custom per-checker timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            checkers: Vec::new(),
            timeout,
        }
    }

    /// Register a checker. Checkers are run in the order they are registered.
    pub fn register(&mut self, checker: Arc<dyn ReadinessChecker>) {
        self.checkers.push(checker);
    }

    /// Return the configured per-checker timeout (useful for tests).
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Run all checkers concurrently and aggregate their results.
    ///
    /// Each checker is wrapped in [`tokio::time::timeout`]. If it exceeds the
    /// configured timeout the checker is recorded as `NotReady`.
    pub async fn aggregate(&self) -> AggregateReadiness {
        // Build one future per checker. We collect into a `Vec` so that
        // `futures::future::join_all` can drive them concurrently.
        let futures: Vec<_> = self
            .checkers
            .iter()
            .map(|checker| {
                let checker = Arc::clone(checker);
                let timeout = self.timeout;
                async move {
                    let name = checker.name().to_owned();
                    let start = Instant::now();
                    let status = match tokio::time::timeout(timeout, checker.check()).await {
                        Ok(s) => s,
                        Err(_elapsed) => ReadinessStatus::NotReady {
                            reason: format!("check timed out after {}ms", timeout.as_millis()),
                        },
                    };
                    let duration_ms = start.elapsed().as_millis() as u64;
                    CheckerResult {
                        name,
                        status,
                        duration_ms,
                    }
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        let healthy = results.iter().all(|r| r.status.is_acceptable());

        AggregateReadiness { healthy, results }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // ── helpers ────────────────────────────────────────────────────────────

    struct FixedChecker {
        name: &'static str,
        status: ReadinessStatus,
    }

    #[async_trait]
    impl ReadinessChecker for FixedChecker {
        fn name(&self) -> &str {
            self.name
        }

        async fn check(&self) -> ReadinessStatus {
            self.status.clone()
        }
    }

    struct SlowChecker {
        name: &'static str,
        delay: Duration,
    }

    #[async_trait]
    impl ReadinessChecker for SlowChecker {
        fn name(&self) -> &str {
            self.name
        }

        async fn check(&self) -> ReadinessStatus {
            tokio::time::sleep(self.delay).await;
            ReadinessStatus::Ready
        }
    }

    // ── tests ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_aggregate_all_ready() {
        let mut registry = ReadinessRegistry::new();
        for name in &["db", "redis", "storage"] {
            registry.register(Arc::new(FixedChecker {
                name,
                status: ReadinessStatus::Ready,
            }));
        }

        let result = registry.aggregate().await;

        assert!(
            result.healthy,
            "expected healthy when all checkers are Ready"
        );
        assert_eq!(result.status_code(), 200);
        assert_eq!(result.results.len(), 3);
        for r in &result.results {
            assert_eq!(r.status, ReadinessStatus::Ready);
        }
    }

    #[tokio::test]
    async fn test_aggregate_one_not_ready() {
        let mut registry = ReadinessRegistry::new();
        registry.register(Arc::new(FixedChecker {
            name: "db",
            status: ReadinessStatus::Ready,
        }));
        registry.register(Arc::new(FixedChecker {
            name: "redis",
            status: ReadinessStatus::Ready,
        }));
        registry.register(Arc::new(FixedChecker {
            name: "broken",
            status: ReadinessStatus::NotReady {
                reason: "connection refused".to_string(),
            },
        }));

        let result = registry.aggregate().await;

        assert!(
            !result.healthy,
            "expected unhealthy when one checker is NotReady"
        );
        assert_eq!(result.status_code(), 503);
        assert_eq!(result.results.len(), 3);

        let broken = result
            .results
            .iter()
            .find(|r| r.name == "broken")
            .expect("broken checker must appear in results");
        assert!(matches!(broken.status, ReadinessStatus::NotReady { .. }));
    }

    #[tokio::test]
    async fn test_aggregate_timeout_treated_as_not_ready() {
        // Timeout set to 50ms, checker sleeps for 500ms → must be treated as
        // NotReady without actually waiting the full 500ms.
        let mut registry = ReadinessRegistry::with_timeout(Duration::from_millis(50));
        registry.register(Arc::new(SlowChecker {
            name: "slow_db",
            delay: Duration::from_millis(500),
        }));

        let start = std::time::Instant::now();
        let result = registry.aggregate().await;
        let elapsed = start.elapsed();

        // The aggregate should complete well before 500ms (give up to 400ms leeway).
        assert!(
            elapsed < Duration::from_millis(400),
            "aggregate took too long: {elapsed:?}",
        );
        assert!(
            !result.healthy,
            "timed-out checker must make result unhealthy"
        );
        assert_eq!(result.status_code(), 503);

        let r = &result.results[0];
        assert!(
            matches!(&r.status, ReadinessStatus::NotReady { reason } if reason.contains("timed out")),
            "expected timed-out reason, got {:?}",
            r.status
        );
    }

    #[tokio::test]
    async fn test_readiness_registry_default_timeout() {
        let registry = ReadinessRegistry::new();
        assert_eq!(registry.timeout(), Duration::from_secs(2));
    }

    #[tokio::test]
    async fn test_aggregate_degraded_counts_as_healthy() {
        let mut registry = ReadinessRegistry::new();
        registry.register(Arc::new(FixedChecker {
            name: "db",
            status: ReadinessStatus::Ready,
        }));
        registry.register(Arc::new(FixedChecker {
            name: "cache",
            status: ReadinessStatus::Degraded {
                reason: "high latency".to_string(),
            },
        }));

        let result = registry.aggregate().await;
        assert!(
            result.healthy,
            "Degraded checker must still count as healthy"
        );
        assert_eq!(result.status_code(), 200);
    }
}
