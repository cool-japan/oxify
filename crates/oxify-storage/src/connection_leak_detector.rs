//! Connection leak detection and monitoring
//!
//! Provides utilities to detect and monitor potential connection leaks in the database pool.
//! This is critical for production debugging when connections are not being properly returned.
//!
//! # Features
//!
//! - Track connection acquisition and release
//! - Detect long-lived connections that may indicate leaks
//! - Report connection usage statistics
//! - Integration with tracing for observability
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::connection_leak_detector::{LeakDetector, LeakDetectorConfig};
//!
//! let config = LeakDetectorConfig {
//!     leak_threshold_seconds: 300, // 5 minutes
//!     check_interval_seconds: 60,  // Check every minute
//!     max_tracked_connections: 1000,
//! };
//!
//! let detector = LeakDetector::new(config);
//!
//! // Track connection acquisition
//! let token = detector.track_acquisition("user_query");
//!
//! // Perform database operations...
//!
//! // Release tracking when done
//! detector.track_release(token);
//!
//! // Get leak report
//! let report = detector.get_leak_report();
//! if !report.suspected_leaks.is_empty() {
//!     warn!("Detected {} potential connection leaks", report.suspected_leaks.len());
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Configuration for leak detection
#[derive(Debug, Clone)]
pub struct LeakDetectorConfig {
    /// Threshold in seconds after which a connection is considered potentially leaked
    pub leak_threshold_seconds: u64,
    /// Interval in seconds between leak checks
    pub check_interval_seconds: u64,
    /// Maximum number of connections to track (prevents memory bloat)
    pub max_tracked_connections: usize,
}

impl Default for LeakDetectorConfig {
    fn default() -> Self {
        Self {
            leak_threshold_seconds: 300, // 5 minutes
            check_interval_seconds: 60,  // 1 minute
            max_tracked_connections: 1000,
        }
    }
}

/// Token for tracking a connection acquisition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionToken(u64);

/// Information about a tracked connection
#[derive(Debug, Clone)]
struct ConnectionInfo {
    /// When the connection was acquired
    acquired_at: Instant,
    /// Where the connection was acquired (e.g., function name, query type)
    context: String,
    /// Whether this connection has been flagged as a potential leak
    flagged_as_leak: bool,
}

/// Connection leak detector
#[derive(Clone)]
pub struct LeakDetector {
    config: LeakDetectorConfig,
    state: Arc<RwLock<DetectorState>>,
}

struct DetectorState {
    /// Currently tracked connections
    tracked: HashMap<ConnectionToken, ConnectionInfo>,
    /// Next token ID to assign
    next_token: u64,
    /// Statistics
    stats: LeakStats,
    /// Last time a leak check was performed
    last_check: Instant,
}

/// Statistics about connection usage and leaks
#[derive(Debug, Clone, Default)]
pub struct LeakStats {
    /// Total connections tracked since detector creation
    pub total_tracked: u64,
    /// Total connections released
    pub total_released: u64,
    /// Total suspected leaks detected
    pub total_suspected_leaks: u64,
    /// Current number of active connections being tracked
    pub active_connections: usize,
    /// Longest connection duration ever observed (in seconds)
    pub longest_connection_duration_secs: u64,
}

/// Information about a suspected connection leak
#[derive(Debug, Clone)]
pub struct SuspectedLeak {
    /// Token identifying the connection
    pub token: ConnectionToken,
    /// How long the connection has been held (in seconds)
    pub duration_secs: u64,
    /// Context where the connection was acquired
    pub context: String,
    /// When the connection was acquired
    pub acquired_at: Instant,
}

/// Report of detected leaks and statistics
#[derive(Debug, Clone)]
pub struct LeakReport {
    /// List of suspected leaks
    pub suspected_leaks: Vec<SuspectedLeak>,
    /// Overall statistics
    pub stats: LeakStats,
    /// When the report was generated
    pub generated_at: Instant,
}

impl LeakDetector {
    /// Create a new leak detector with the given configuration
    pub fn new(config: LeakDetectorConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(DetectorState {
                tracked: HashMap::new(),
                next_token: 1,
                stats: LeakStats::default(),
                last_check: Instant::now(),
            })),
        }
    }

    /// Create a new leak detector with default configuration
    pub fn with_defaults() -> Self {
        Self::new(LeakDetectorConfig::default())
    }

    /// Track a connection acquisition
    ///
    /// Returns a token that should be passed to `track_release()` when the connection is returned.
    ///
    /// # Arguments
    ///
    /// * `context` - Description of where/why the connection was acquired (e.g., "user_query", "workflow_update")
    pub fn track_acquisition(&self, context: impl Into<String>) -> ConnectionToken {
        let mut state = self
            .state
            .write()
            .expect("Leak detector state lock poisoned");

        // Check if we've exceeded max tracked connections
        if state.tracked.len() >= self.config.max_tracked_connections {
            tracing::warn!(
                "Leak detector at maximum capacity ({}), not tracking new connection",
                self.config.max_tracked_connections
            );
            return ConnectionToken(0); // Invalid token
        }

        let token = ConnectionToken(state.next_token);
        state.next_token += 1;

        let info = ConnectionInfo {
            acquired_at: Instant::now(),
            context: context.into(),
            flagged_as_leak: false,
        };

        state.tracked.insert(token, info);
        state.stats.total_tracked += 1;
        state.stats.active_connections = state.tracked.len();

        token
    }

    /// Track a connection release
    ///
    /// Should be called when a connection is returned to the pool.
    ///
    /// # Arguments
    ///
    /// * `token` - The token returned from `track_acquisition()`
    pub fn track_release(&self, token: ConnectionToken) {
        if token.0 == 0 {
            return; // Invalid token
        }

        let mut state = self
            .state
            .write()
            .expect("Leak detector state lock poisoned");

        if let Some(info) = state.tracked.remove(&token) {
            let duration = info.acquired_at.elapsed();
            let duration_secs = duration.as_secs();

            state.stats.total_released += 1;
            state.stats.active_connections = state.tracked.len();

            if duration_secs > state.stats.longest_connection_duration_secs {
                state.stats.longest_connection_duration_secs = duration_secs;
            }

            if duration_secs > self.config.leak_threshold_seconds {
                tracing::info!(
                    context = %info.context,
                    duration_secs,
                    "Long-lived connection released (exceeded threshold)"
                );
            }
        } else {
            tracing::warn!(
                token = token.0,
                "Attempted to release connection that was not tracked"
            );
        }
    }

    /// Perform a leak check and return a report
    ///
    /// This scans all tracked connections and identifies those that have been held
    /// longer than the configured threshold.
    pub fn get_leak_report(&self) -> LeakReport {
        let mut state = self
            .state
            .write()
            .expect("Leak detector state lock poisoned");

        let now = Instant::now();
        let threshold = Duration::from_secs(self.config.leak_threshold_seconds);

        let mut suspected_leaks = Vec::new();
        let mut new_leaks_count = 0;

        for (token, info) in &mut state.tracked {
            let duration = now.duration_since(info.acquired_at);

            if duration > threshold {
                if !info.flagged_as_leak {
                    info.flagged_as_leak = true;
                    new_leaks_count += 1;

                    tracing::warn!(
                        token = token.0,
                        context = %info.context,
                        duration_secs = duration.as_secs(),
                        "Suspected connection leak detected"
                    );
                }

                suspected_leaks.push(SuspectedLeak {
                    token: *token,
                    duration_secs: duration.as_secs(),
                    context: info.context.clone(),
                    acquired_at: info.acquired_at,
                });
            }
        }

        state.stats.total_suspected_leaks += new_leaks_count;
        state.last_check = now;

        // Sort by duration (longest first)
        suspected_leaks.sort_by(|a, b| b.duration_secs.cmp(&a.duration_secs));

        LeakReport {
            suspected_leaks,
            stats: state.stats.clone(),
            generated_at: now,
        }
    }

    /// Check for leaks and log warnings if any are found
    ///
    /// This is a convenience method that gets a leak report and logs warnings
    /// for each suspected leak.
    pub fn check_and_log_leaks(&self) {
        let report = self.get_leak_report();

        if !report.suspected_leaks.is_empty() {
            tracing::warn!(
                "Detected {} suspected connection leaks",
                report.suspected_leaks.len()
            );

            for leak in &report.suspected_leaks {
                tracing::warn!(
                    token = leak.token.0,
                    context = %leak.context,
                    duration_secs = leak.duration_secs,
                    "Connection held for {} seconds",
                    leak.duration_secs
                );
            }
        }
    }

    /// Get current statistics
    pub fn get_stats(&self) -> LeakStats {
        let state = self
            .state
            .read()
            .expect("Leak detector state lock poisoned");
        state.stats.clone()
    }

    /// Clear all tracked connections
    ///
    /// This is useful for testing or resetting the detector state.
    /// Use with caution in production.
    pub fn clear(&self) {
        let mut state = self
            .state
            .write()
            .expect("Leak detector state lock poisoned");
        state.tracked.clear();
        state.stats.active_connections = 0;
    }

    /// Get the number of currently tracked connections
    pub fn active_count(&self) -> usize {
        let state = self
            .state
            .read()
            .expect("Leak detector state lock poisoned");
        state.tracked.len()
    }
}

impl LeakReport {
    /// Check if there are any suspected leaks
    pub fn has_leaks(&self) -> bool {
        !self.suspected_leaks.is_empty()
    }

    /// Get the number of suspected leaks
    pub fn leak_count(&self) -> usize {
        self.suspected_leaks.len()
    }

    /// Get contexts where leaks occurred (unique)
    pub fn leak_contexts(&self) -> Vec<String> {
        let mut contexts: Vec<String> = self
            .suspected_leaks
            .iter()
            .map(|leak| leak.context.clone())
            .collect();
        contexts.sort();
        contexts.dedup();
        contexts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_track_acquisition_and_release() {
        let detector = LeakDetector::with_defaults();

        let token = detector.track_acquisition("test_query");
        assert_eq!(detector.active_count(), 1);

        detector.track_release(token);
        assert_eq!(detector.active_count(), 0);

        let stats = detector.get_stats();
        assert_eq!(stats.total_tracked, 1);
        assert_eq!(stats.total_released, 1);
    }

    #[test]
    fn test_multiple_connections() {
        let detector = LeakDetector::with_defaults();

        let token1 = detector.track_acquisition("query1");
        let token2 = detector.track_acquisition("query2");
        let token3 = detector.track_acquisition("query3");

        assert_eq!(detector.active_count(), 3);

        detector.track_release(token2);
        assert_eq!(detector.active_count(), 2);

        detector.track_release(token1);
        detector.track_release(token3);
        assert_eq!(detector.active_count(), 0);
    }

    #[test]
    fn test_leak_detection() {
        let config = LeakDetectorConfig {
            leak_threshold_seconds: 0, // Immediate leak detection for testing
            check_interval_seconds: 1,
            max_tracked_connections: 100,
        };

        let detector = LeakDetector::new(config);

        let _token = detector.track_acquisition("slow_query");

        // Wait a bit to exceed threshold
        thread::sleep(Duration::from_millis(100));

        let report = detector.get_leak_report();
        assert!(report.has_leaks());
        assert_eq!(report.leak_count(), 1);
        assert_eq!(report.leak_contexts(), vec!["slow_query".to_string()]);
    }

    #[test]
    fn test_max_tracked_connections() {
        let config = LeakDetectorConfig {
            leak_threshold_seconds: 300,
            check_interval_seconds: 60,
            max_tracked_connections: 3,
        };

        let detector = LeakDetector::new(config);

        let token1 = detector.track_acquisition("query1");
        let token2 = detector.track_acquisition("query2");
        let token3 = detector.track_acquisition("query3");
        let token4 = detector.track_acquisition("query4"); // Should not be tracked

        assert_eq!(detector.active_count(), 3);
        assert_eq!(token4.0, 0); // Invalid token

        detector.track_release(token1);
        detector.track_release(token2);
        detector.track_release(token3);
        detector.track_release(token4); // Should be a no-op

        assert_eq!(detector.active_count(), 0);
    }

    #[test]
    fn test_clear() {
        let detector = LeakDetector::with_defaults();

        detector.track_acquisition("query1");
        detector.track_acquisition("query2");
        assert_eq!(detector.active_count(), 2);

        detector.clear();
        assert_eq!(detector.active_count(), 0);
    }

    #[test]
    fn test_release_untracked_connection() {
        let detector = LeakDetector::with_defaults();

        // Try to release a connection that was never tracked
        detector.track_release(ConnectionToken(9999));

        // Should not panic, just log a warning
        assert_eq!(detector.active_count(), 0);
    }

    #[test]
    fn test_stats_tracking() {
        let detector = LeakDetector::with_defaults();

        let token1 = detector.track_acquisition("query1");
        let token2 = detector.track_acquisition("query2");

        let stats = detector.get_stats();
        assert_eq!(stats.total_tracked, 2);
        assert_eq!(stats.active_connections, 2);
        assert_eq!(stats.total_released, 0);

        detector.track_release(token1);

        let stats = detector.get_stats();
        assert_eq!(stats.total_released, 1);
        assert_eq!(stats.active_connections, 1);

        detector.track_release(token2);

        let stats = detector.get_stats();
        assert_eq!(stats.total_released, 2);
        assert_eq!(stats.active_connections, 0);
    }
}
