//! Security Metrics and Analytics
//!
//! Provides comprehensive authentication metrics, KPIs, and security analytics
//! for monitoring and improving authentication security posture.
//!
//! # Features
//!
//! - **Authentication Metrics**: Login success/failure rates, MFA adoption
//! - **Security KPIs**: Breach indicators, anomaly detection rates
//! - **Performance Metrics**: Response times, throughput
//! - **User Behavior**: Session duration, geographic distribution
//! - **Trend Analysis**: Time-series data for security trends
//!
//! # Example
//!
//! ```
//! use oxify_authn::metrics::{MetricsCollector, AuthEvent};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let collector = MetricsCollector::new();
//!
//! // Record an authentication event
//! collector.record_auth_success("user123", "192.168.1.1").await;
//!
//! // Get current metrics
//! let metrics = collector.get_metrics().await;
//! println!("Success rate: {:.2}%", metrics.success_rate());
//! # Ok(())
//! # }
//! ```

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Authentication event type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthEvent {
    /// Successful login
    LoginSuccess,
    /// Failed login
    LoginFailure,
    /// MFA challenge issued
    MfaChallenge,
    /// MFA success
    MfaSuccess,
    /// MFA failure
    MfaFailure,
    /// Password reset requested
    PasswordReset,
    /// Account locked
    AccountLocked,
    /// Suspicious activity detected
    SuspiciousActivity,
    /// Token issued
    TokenIssued,
    /// Token revoked
    TokenRevoked,
}

/// Authentication event record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthEventRecord {
    /// Event type
    event: AuthEvent,
    /// User ID
    user_id: String,
    /// IP address
    ip_address: String,
    /// Timestamp
    timestamp: DateTime<Utc>,
    /// Response time in milliseconds
    response_time_ms: Option<u64>,
    /// Additional metadata
    metadata: HashMap<String, String>,
}

impl AuthEventRecord {
    /// Create a new auth event record
    pub fn new(
        event: AuthEvent,
        user_id: impl Into<String>,
        ip_address: impl Into<String>,
    ) -> Self {
        Self {
            event,
            user_id: user_id.into(),
            ip_address: ip_address.into(),
            timestamp: Utc::now(),
            response_time_ms: None,
            metadata: HashMap::new(),
        }
    }

    /// Set response time
    #[must_use]
    pub fn with_response_time(mut self, ms: u64) -> Self {
        self.response_time_ms = Some(ms);
        self
    }

    /// Add metadata
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Get the event type
    #[must_use]
    pub fn event(&self) -> &AuthEvent {
        &self.event
    }

    /// Get the user ID
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get the IP address
    #[must_use]
    pub fn ip_address(&self) -> &str {
        &self.ip_address
    }

    /// Get the timestamp
    #[must_use]
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Authentication metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMetrics {
    /// Total authentication attempts
    pub total_attempts: u64,
    /// Successful authentications
    pub successful_auth: u64,
    /// Failed authentications
    pub failed_auth: u64,
    /// MFA challenges issued
    pub mfa_challenges: u64,
    /// MFA successes
    pub mfa_successes: u64,
    /// MFA failures
    pub mfa_failures: u64,
    /// Password resets
    pub password_resets: u64,
    /// Account lockouts
    pub account_lockouts: u64,
    /// Suspicious activities detected
    pub suspicious_activities: u64,
    /// Tokens issued
    pub tokens_issued: u64,
    /// Tokens revoked
    pub tokens_revoked: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Unique users
    pub unique_users: u64,
    /// Unique IP addresses
    pub unique_ips: u64,
}

impl AuthMetrics {
    /// Calculate success rate
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            return 0.0;
        }
        (self.successful_auth as f64 / self.total_attempts as f64) * 100.0
    }

    /// Calculate MFA adoption rate
    #[must_use]
    pub fn mfa_adoption_rate(&self) -> f64 {
        if self.successful_auth == 0 {
            return 0.0;
        }
        (self.mfa_successes as f64 / self.successful_auth as f64) * 100.0
    }

    /// Calculate failure rate
    #[must_use]
    pub fn failure_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            return 0.0;
        }
        (self.failed_auth as f64 / self.total_attempts as f64) * 100.0
    }

    /// Calculate security score (0-100)
    #[must_use]
    pub fn security_score(&self) -> f64 {
        let mut score: f64 = 100.0;

        // Deduct points for high failure rate
        if self.failure_rate() > 20.0 {
            score -= 20.0;
        } else if self.failure_rate() > 10.0 {
            score -= 10.0;
        }

        // Deduct points for low MFA adoption
        if self.mfa_adoption_rate() < 30.0 {
            score -= 20.0;
        } else if self.mfa_adoption_rate() < 60.0 {
            score -= 10.0;
        }

        // Deduct points for suspicious activities
        let suspicious_rate = self.suspicious_activities as f64 / self.total_attempts.max(1) as f64;
        if suspicious_rate > 0.05 {
            score -= 30.0;
        } else if suspicious_rate > 0.02 {
            score -= 15.0;
        }

        // Deduct points for account lockouts
        let lockout_rate = self.account_lockouts as f64 / self.total_attempts.max(1) as f64;
        if lockout_rate > 0.03 {
            score -= 20.0;
        } else if lockout_rate > 0.01 {
            score -= 10.0;
        }

        score.max(0.0)
    }
}

impl Default for AuthMetrics {
    fn default() -> Self {
        Self {
            total_attempts: 0,
            successful_auth: 0,
            failed_auth: 0,
            mfa_challenges: 0,
            mfa_successes: 0,
            mfa_failures: 0,
            password_resets: 0,
            account_lockouts: 0,
            suspicious_activities: 0,
            tokens_issued: 0,
            tokens_revoked: 0,
            avg_response_time_ms: 0.0,
            unique_users: 0,
            unique_ips: 0,
        }
    }
}

/// Time-series metrics point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Value
    pub value: f64,
}

/// Geographic distribution entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoDistribution {
    /// Country code
    pub country: String,
    /// Request count
    pub count: u64,
}

/// Metrics collector
pub struct MetricsCollector {
    /// Event store
    events: Arc<Mutex<Vec<AuthEventRecord>>>,
    /// Unique users
    unique_users: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Unique IPs
    unique_ips: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Maximum events to keep in memory
    max_events: usize,
}

impl MetricsCollector {
    /// Create a new metrics collector
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            unique_users: Arc::new(Mutex::new(std::collections::HashSet::new())),
            unique_ips: Arc::new(Mutex::new(std::collections::HashSet::new())),
            max_events: 10000, // Keep last 10k events
        }
    }

    /// Record an authentication success
    pub async fn record_auth_success(&self, user_id: &str, ip: &str) {
        let event = AuthEventRecord::new(AuthEvent::LoginSuccess, user_id, ip);
        self.record_event(event).await;
    }

    /// Record an authentication failure
    pub async fn record_auth_failure(&self, user_id: &str, ip: &str) {
        let event = AuthEventRecord::new(AuthEvent::LoginFailure, user_id, ip);
        self.record_event(event).await;
    }

    /// Record an MFA challenge
    pub async fn record_mfa_challenge(&self, user_id: &str, ip: &str) {
        let event = AuthEventRecord::new(AuthEvent::MfaChallenge, user_id, ip);
        self.record_event(event).await;
    }

    /// Record an MFA success
    pub async fn record_mfa_success(&self, user_id: &str, ip: &str) {
        let event = AuthEventRecord::new(AuthEvent::MfaSuccess, user_id, ip);
        self.record_event(event).await;
    }

    /// Record suspicious activity
    pub async fn record_suspicious_activity(&self, user_id: &str, ip: &str) {
        let event = AuthEventRecord::new(AuthEvent::SuspiciousActivity, user_id, ip);
        self.record_event(event).await;
    }

    /// Record a generic event
    pub async fn record_event(&self, event: AuthEventRecord) {
        let mut events = self.events.lock().unwrap();
        let mut users = self.unique_users.lock().unwrap();
        let mut ips = self.unique_ips.lock().unwrap();

        users.insert(event.user_id.clone());
        ips.insert(event.ip_address.clone());

        events.push(event);

        // Trim events if exceeding max
        if events.len() > self.max_events {
            let excess = events.len() - self.max_events;
            events.drain(0..excess);
        }
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> AuthMetrics {
        let events = self.events.lock().unwrap();
        let users = self.unique_users.lock().unwrap();
        let ips = self.unique_ips.lock().unwrap();

        let total_attempts = events
            .iter()
            .filter(|e| matches!(e.event, AuthEvent::LoginSuccess | AuthEvent::LoginFailure))
            .count() as u64;

        let successful_auth = events
            .iter()
            .filter(|e| e.event == AuthEvent::LoginSuccess)
            .count() as u64;
        let failed_auth = events
            .iter()
            .filter(|e| e.event == AuthEvent::LoginFailure)
            .count() as u64;
        let mfa_challenges = events
            .iter()
            .filter(|e| e.event == AuthEvent::MfaChallenge)
            .count() as u64;
        let mfa_successes = events
            .iter()
            .filter(|e| e.event == AuthEvent::MfaSuccess)
            .count() as u64;
        let mfa_failures = events
            .iter()
            .filter(|e| e.event == AuthEvent::MfaFailure)
            .count() as u64;
        let password_resets = events
            .iter()
            .filter(|e| e.event == AuthEvent::PasswordReset)
            .count() as u64;
        let account_lockouts = events
            .iter()
            .filter(|e| e.event == AuthEvent::AccountLocked)
            .count() as u64;
        let suspicious_activities = events
            .iter()
            .filter(|e| e.event == AuthEvent::SuspiciousActivity)
            .count() as u64;
        let tokens_issued = events
            .iter()
            .filter(|e| e.event == AuthEvent::TokenIssued)
            .count() as u64;
        let tokens_revoked = events
            .iter()
            .filter(|e| e.event == AuthEvent::TokenRevoked)
            .count() as u64;

        let response_times: Vec<u64> = events.iter().filter_map(|e| e.response_time_ms).collect();
        let avg_response_time_ms = if response_times.is_empty() {
            0.0
        } else {
            response_times.iter().sum::<u64>() as f64 / response_times.len() as f64
        };

        AuthMetrics {
            total_attempts,
            successful_auth,
            failed_auth,
            mfa_challenges,
            mfa_successes,
            mfa_failures,
            password_resets,
            account_lockouts,
            suspicious_activities,
            tokens_issued,
            tokens_revoked,
            avg_response_time_ms,
            unique_users: users.len() as u64,
            unique_ips: ips.len() as u64,
        }
    }

    /// Get metrics for a specific time period
    pub async fn get_metrics_for_period(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> AuthMetrics {
        let events = self.events.lock().unwrap();
        let filtered_events: Vec<_> = events
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect();

        let mut unique_users = std::collections::HashSet::new();
        let mut unique_ips = std::collections::HashSet::new();

        for event in &filtered_events {
            unique_users.insert(event.user_id.clone());
            unique_ips.insert(event.ip_address.clone());
        }

        let total_attempts = filtered_events
            .iter()
            .filter(|e| matches!(e.event, AuthEvent::LoginSuccess | AuthEvent::LoginFailure))
            .count() as u64;

        let successful_auth = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::LoginSuccess)
            .count() as u64;
        let failed_auth = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::LoginFailure)
            .count() as u64;
        let mfa_challenges = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::MfaChallenge)
            .count() as u64;
        let mfa_successes = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::MfaSuccess)
            .count() as u64;
        let mfa_failures = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::MfaFailure)
            .count() as u64;
        let password_resets = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::PasswordReset)
            .count() as u64;
        let account_lockouts = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::AccountLocked)
            .count() as u64;
        let suspicious_activities = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::SuspiciousActivity)
            .count() as u64;
        let tokens_issued = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::TokenIssued)
            .count() as u64;
        let tokens_revoked = filtered_events
            .iter()
            .filter(|e| e.event == AuthEvent::TokenRevoked)
            .count() as u64;

        let response_times: Vec<u64> = filtered_events
            .iter()
            .filter_map(|e| e.response_time_ms)
            .collect();
        let avg_response_time_ms = if response_times.is_empty() {
            0.0
        } else {
            response_times.iter().sum::<u64>() as f64 / response_times.len() as f64
        };

        AuthMetrics {
            total_attempts,
            successful_auth,
            failed_auth,
            mfa_challenges,
            mfa_successes,
            mfa_failures,
            password_resets,
            account_lockouts,
            suspicious_activities,
            tokens_issued,
            tokens_revoked,
            avg_response_time_ms,
            unique_users: unique_users.len() as u64,
            unique_ips: unique_ips.len() as u64,
        }
    }

    /// Get time series data for success rate
    pub async fn get_success_rate_timeseries(&self, interval: Duration) -> Vec<TimeSeriesPoint> {
        let events = self.events.lock().unwrap();
        let now = Utc::now();
        let mut series = Vec::new();

        // Group events by interval
        let mut interval_start = events.first().map_or(now, |e| e.timestamp);
        let end_time = now;

        while interval_start < end_time {
            let interval_end = interval_start + interval;

            let interval_events: Vec<_> = events
                .iter()
                .filter(|e| e.timestamp >= interval_start && e.timestamp < interval_end)
                .collect();

            let total = interval_events
                .iter()
                .filter(|e| matches!(e.event, AuthEvent::LoginSuccess | AuthEvent::LoginFailure))
                .count() as f64;

            let successes = interval_events
                .iter()
                .filter(|e| e.event == AuthEvent::LoginSuccess)
                .count() as f64;

            let success_rate = if total > 0.0 {
                (successes / total) * 100.0
            } else {
                0.0
            };

            series.push(TimeSeriesPoint {
                timestamp: interval_start,
                value: success_rate,
            });

            interval_start = interval_end;
        }

        series
    }

    /// Clear all metrics
    pub async fn clear(&self) {
        let mut events = self.events.lock().unwrap();
        let mut users = self.unique_users.lock().unwrap();
        let mut ips = self.unique_ips.lock().unwrap();

        events.clear();
        users.clear();
        ips.clear();
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_auth_success() {
        let collector = MetricsCollector::new();
        collector
            .record_auth_success("user123", "192.168.1.1")
            .await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.successful_auth, 1);
        assert_eq!(metrics.total_attempts, 1);
    }

    #[tokio::test]
    async fn test_record_auth_failure() {
        let collector = MetricsCollector::new();
        collector
            .record_auth_failure("user123", "192.168.1.1")
            .await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.failed_auth, 1);
        assert_eq!(metrics.total_attempts, 1);
    }

    #[tokio::test]
    async fn test_success_rate_calculation() {
        let collector = MetricsCollector::new();
        collector.record_auth_success("user1", "192.168.1.1").await;
        collector.record_auth_success("user2", "192.168.1.2").await;
        collector.record_auth_failure("user3", "192.168.1.3").await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.total_attempts, 3);
        assert_eq!(metrics.successful_auth, 2);
        assert_eq!(metrics.failed_auth, 1);
        assert!((metrics.success_rate() - 66.67).abs() < 0.1);
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)]
    async fn test_mfa_metrics() {
        let collector = MetricsCollector::new();
        collector.record_auth_success("user1", "192.168.1.1").await;
        collector.record_mfa_challenge("user1", "192.168.1.1").await;
        collector.record_mfa_success("user1", "192.168.1.1").await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.mfa_challenges, 1);
        assert_eq!(metrics.mfa_successes, 1);
        assert_eq!(metrics.mfa_adoption_rate(), 100.0);
    }

    #[tokio::test]
    async fn test_unique_users_and_ips() {
        let collector = MetricsCollector::new();
        collector.record_auth_success("user1", "192.168.1.1").await;
        collector.record_auth_success("user1", "192.168.1.2").await;
        collector.record_auth_success("user2", "192.168.1.1").await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.unique_users, 2);
        assert_eq!(metrics.unique_ips, 2);
    }

    #[tokio::test]
    async fn test_security_score() {
        let collector = MetricsCollector::new();

        // Good scenario: high success rate, good MFA adoption
        for _ in 0..8 {
            collector.record_auth_success("user1", "192.168.1.1").await;
            collector.record_mfa_success("user1", "192.168.1.1").await;
        }
        for _ in 0..2 {
            collector.record_auth_failure("user2", "192.168.1.2").await;
        }

        let metrics = collector.get_metrics().await;
        let score = metrics.security_score();
        assert!(score > 80.0); // Good security score
    }

    #[tokio::test]
    async fn test_security_score_poor() {
        let collector = MetricsCollector::new();

        // Poor scenario: high failure rate, no MFA, suspicious activities
        for _ in 0..3 {
            collector.record_auth_success("user1", "192.168.1.1").await;
        }
        for _ in 0..7 {
            collector.record_auth_failure("user2", "192.168.1.2").await;
        }
        collector
            .record_suspicious_activity("user3", "192.168.1.3")
            .await;

        let metrics = collector.get_metrics().await;
        let score = metrics.security_score();
        assert!(score < 60.0); // Poor security score
    }

    #[tokio::test]
    async fn test_metrics_for_period() {
        let collector = MetricsCollector::new();

        let now = Utc::now();
        let past = now - Duration::hours(2);

        // Create an event in the past (manually)
        let old_event = AuthEventRecord {
            event: AuthEvent::LoginSuccess,
            user_id: "user1".to_string(),
            ip_address: "192.168.1.1".to_string(),
            timestamp: past,
            response_time_ms: None,
            metadata: HashMap::new(),
        };
        collector.record_event(old_event).await;

        // Create recent events
        collector.record_auth_success("user2", "192.168.1.2").await;
        collector.record_auth_failure("user3", "192.168.1.3").await;

        // Get metrics for last hour (use a future end time to account for clock drift)
        let recent_start = now - Duration::hours(1);
        let metrics = collector
            .get_metrics_for_period(recent_start, Utc::now() + Duration::seconds(10))
            .await;

        // Should only count the recent events
        assert_eq!(metrics.total_attempts, 2);
    }

    #[tokio::test]
    async fn test_clear_metrics() {
        let collector = MetricsCollector::new();
        collector.record_auth_success("user1", "192.168.1.1").await;
        collector.record_auth_failure("user2", "192.168.1.2").await;

        let metrics_before = collector.get_metrics().await;
        assert!(metrics_before.total_attempts > 0);

        collector.clear().await;

        let metrics_after = collector.get_metrics().await;
        assert_eq!(metrics_after.total_attempts, 0);
    }

    #[tokio::test]
    async fn test_time_series() {
        let collector = MetricsCollector::new();

        // Record some events
        for _ in 0..5 {
            collector.record_auth_success("user1", "192.168.1.1").await;
        }
        for _ in 0..2 {
            collector.record_auth_failure("user2", "192.168.1.2").await;
        }

        let series = collector
            .get_success_rate_timeseries(Duration::minutes(1))
            .await;
        assert!(!series.is_empty());
    }
}
