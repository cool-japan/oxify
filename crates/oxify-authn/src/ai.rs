//! # AI-Powered Security
//!
//! This module provides intelligent security features using statistical analysis
//! and behavioral profiling for advanced threat detection.
//!
//! ## Features
//! - Statistical anomaly detection (z-score, moving averages)
//! - Behavioral profiling and analysis
//! - Threat intelligence integration
//! - User behavior pattern learning
//! - Adaptive risk scoring
//! - Time-series analysis for login patterns
//!
//! ## Example
//!
//! ```
//! use oxify_authn::ai::{AiSecurityEngine, BehaviorProfile, LoginEvent};
//! use chrono::Utc;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create AI security engine
//! let mut engine = AiSecurityEngine::new();
//!
//! // Record login events
//! let event = LoginEvent {
//!     user_id: "alice".to_string(),
//!     timestamp: Utc::now(),
//!     ip_address: "192.168.1.100".to_string(),
//!     user_agent: "Mozilla/5.0".to_string(),
//!     location: Some("US".to_string()),
//!     success: true,
//!     session_duration_seconds: Some(3600),
//! };
//!
//! engine.record_event(event);
//!
//! // Analyze behavior
//! let profile = engine.get_behavior_profile("alice")?;
//! println!("Anomaly score: {}", profile.anomaly_score);
//! println!("Trust level: {}", profile.trust_level);
//! # Ok(())
//! # }
//! ```

use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// AI security errors
#[derive(Error, Debug)]
pub enum AiSecurityError {
    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Insufficient data for analysis: {0}")]
    InsufficientData(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

pub type Result<T> = std::result::Result<T, AiSecurityError>;

/// Login event for behavioral analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginEvent {
    /// User ID
    pub user_id: String,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// IP address
    pub ip_address: String,
    /// User agent
    pub user_agent: String,
    /// Geographic location (country code)
    pub location: Option<String>,
    /// Whether login was successful
    pub success: bool,
    /// Session duration in seconds (if available)
    pub session_duration_seconds: Option<u64>,
}

/// Trust level based on behavioral analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    /// Very high trust (0.9-1.0)
    VeryHigh,
    /// High trust (0.7-0.9)
    High,
    /// Medium trust (0.5-0.7)
    Medium,
    /// Low trust (0.3-0.5)
    Low,
    /// Very low trust (0.0-0.3)
    VeryLow,
}

impl TrustLevel {
    /// Create trust level from score (0.0 to 1.0)
    #[must_use]
    pub fn from_score(score: f64) -> Self {
        if score >= 0.9 {
            Self::VeryHigh
        } else if score >= 0.7 {
            Self::High
        } else if score >= 0.5 {
            Self::Medium
        } else if score >= 0.3 {
            Self::Low
        } else {
            Self::VeryLow
        }
    }

    /// Get numeric score
    #[must_use]
    pub fn score(&self) -> f64 {
        match self {
            Self::VeryHigh => 0.95,
            Self::High => 0.8,
            Self::Medium => 0.6,
            Self::Low => 0.4,
            Self::VeryLow => 0.15,
        }
    }
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VeryHigh => write!(f, "very_high"),
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
            Self::VeryLow => write!(f, "very_low"),
        }
    }
}

/// Anomaly detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetection {
    /// Whether an anomaly was detected
    pub is_anomaly: bool,
    /// Anomaly score (0.0 = normal, 1.0 = highly anomalous)
    pub score: f64,
    /// Reasons for anomaly detection
    pub reasons: Vec<String>,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f64,
}

/// Behavioral profile for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorProfile {
    /// User ID
    pub user_id: String,
    /// Trust level
    pub trust_level: TrustLevel,
    /// Anomaly score (0.0 to 1.0)
    pub anomaly_score: f64,
    /// Total login events analyzed
    pub total_events: usize,
    /// Successful login count
    pub successful_logins: usize,
    /// Failed login count
    pub failed_logins: usize,
    /// Common IP addresses
    pub common_ips: Vec<String>,
    /// Common locations
    pub common_locations: Vec<String>,
    /// Average session duration (seconds)
    pub avg_session_duration: f64,
    /// Typical login hours (0-23)
    pub typical_login_hours: Vec<u32>,
    /// Last seen timestamp
    pub last_seen: DateTime<Utc>,
    /// Profile created timestamp
    pub created_at: DateTime<Utc>,
    /// Profile updated timestamp
    pub updated_at: DateTime<Utc>,
}

/// Time series data point
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct TimeSeriesPoint {
    timestamp: DateTime<Utc>,
    value: f64,
}

/// User behavior data (internal)
#[derive(Debug, Clone)]
struct UserBehavior {
    #[allow(dead_code)]
    user_id: String,
    events: VecDeque<LoginEvent>,
    ip_frequency: HashMap<String, usize>,
    location_frequency: HashMap<String, usize>,
    hour_frequency: HashMap<u32, usize>,
    session_durations: Vec<u64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl UserBehavior {
    fn new(user_id: String) -> Self {
        let now = Utc::now();
        Self {
            user_id,
            events: VecDeque::new(),
            ip_frequency: HashMap::new(),
            location_frequency: HashMap::new(),
            hour_frequency: HashMap::new(),
            session_durations: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    fn add_event(&mut self, event: LoginEvent, max_history: usize) {
        // Update frequencies
        *self
            .ip_frequency
            .entry(event.ip_address.clone())
            .or_insert(0) += 1;

        if let Some(ref location) = event.location {
            *self.location_frequency.entry(location.clone()).or_insert(0) += 1;
        }

        let hour = event.timestamp.hour();
        *self.hour_frequency.entry(hour).or_insert(0) += 1;

        if let Some(duration) = event.session_duration_seconds {
            self.session_durations.push(duration);
            if self.session_durations.len() > max_history {
                self.session_durations.remove(0);
            }
        }

        // Add to event history
        self.events.push_back(event);
        if self.events.len() > max_history {
            self.events.pop_front();
        }

        self.updated_at = Utc::now();
    }
}

/// AI security engine configuration
#[derive(Debug, Clone)]
pub struct AiSecurityConfig {
    /// Maximum events to keep in history per user
    pub max_history: usize,
    /// Minimum events required for anomaly detection
    pub min_events_for_analysis: usize,
    /// Z-score threshold for anomaly detection
    pub anomaly_z_score_threshold: f64,
    /// Window size for moving average (in events)
    pub moving_average_window: usize,
    /// Time window for threat intelligence (hours)
    pub threat_window_hours: i64,
    /// Trust decay rate per day of inactivity
    pub trust_decay_per_day: f64,
}

impl Default for AiSecurityConfig {
    fn default() -> Self {
        Self {
            max_history: 1000,
            min_events_for_analysis: 10,
            anomaly_z_score_threshold: 2.5,
            moving_average_window: 20,
            threat_window_hours: 24,
            trust_decay_per_day: 0.05,
        }
    }
}

/// AI-powered security engine
pub struct AiSecurityEngine {
    config: AiSecurityConfig,
    user_behaviors: Arc<Mutex<HashMap<String, UserBehavior>>>,
    threat_ips: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}

impl AiSecurityEngine {
    /// Create a new AI security engine with default config
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AiSecurityConfig::default())
    }

    /// Create with custom configuration
    #[must_use]
    pub fn with_config(config: AiSecurityConfig) -> Self {
        Self {
            config,
            user_behaviors: Arc::new(Mutex::new(HashMap::new())),
            threat_ips: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record a login event
    pub fn record_event(&mut self, event: LoginEvent) {
        let mut behaviors = self
            .user_behaviors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let behavior = behaviors
            .entry(event.user_id.clone())
            .or_insert_with(|| UserBehavior::new(event.user_id.clone()));

        behavior.add_event(event, self.config.max_history);
    }

    /// Get behavioral profile for a user
    pub fn get_behavior_profile(&self, user_id: &str) -> Result<BehaviorProfile> {
        let behaviors = self
            .user_behaviors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let behavior = behaviors
            .get(user_id)
            .ok_or_else(|| AiSecurityError::UserNotFound(user_id.to_string()))?;

        if behavior.events.len() < self.config.min_events_for_analysis {
            return Err(AiSecurityError::InsufficientData(format!(
                "Only {} events, need at least {}",
                behavior.events.len(),
                self.config.min_events_for_analysis
            )));
        }

        let successful_logins = behavior.events.iter().filter(|e| e.success).count();
        let failed_logins = behavior.events.len() - successful_logins;

        // Calculate common IPs (top 5)
        let mut ip_vec: Vec<_> = behavior.ip_frequency.iter().collect();
        ip_vec.sort_by(|a, b| b.1.cmp(a.1));
        let common_ips: Vec<String> = ip_vec.iter().take(5).map(|(ip, _)| (*ip).clone()).collect();

        // Calculate common locations (top 3)
        let mut loc_vec: Vec<_> = behavior.location_frequency.iter().collect();
        loc_vec.sort_by(|a, b| b.1.cmp(a.1));
        let common_locations: Vec<String> = loc_vec
            .iter()
            .take(3)
            .map(|(loc, _)| (*loc).clone())
            .collect();

        // Calculate average session duration
        let avg_session_duration = if behavior.session_durations.is_empty() {
            0.0
        } else {
            behavior.session_durations.iter().sum::<u64>() as f64
                / behavior.session_durations.len() as f64
        };

        // Find typical login hours (hours with > 10% of logins)
        let total_logins = behavior.hour_frequency.values().sum::<usize>();
        let threshold = (total_logins as f64 * 0.1).max(1.0) as usize;
        let mut typical_login_hours: Vec<u32> = behavior
            .hour_frequency
            .iter()
            .filter(|(_, &count)| count >= threshold)
            .map(|(&hour, _)| hour)
            .collect();
        typical_login_hours.sort_unstable();

        // Calculate anomaly score
        let anomaly_score = Self::calculate_anomaly_score(behavior);

        // Calculate trust level with decay
        let base_trust = 1.0 - anomaly_score;
        let days_since_last_seen = (Utc::now() - behavior.updated_at).num_days();
        let trust_decay = (days_since_last_seen as f64 * self.config.trust_decay_per_day).min(0.5);
        let adjusted_trust = (base_trust - trust_decay).max(0.0);

        let trust_level = TrustLevel::from_score(adjusted_trust);

        let last_event = behavior
            .events
            .back()
            .expect("invariant: events non-empty after processing");

        Ok(BehaviorProfile {
            user_id: user_id.to_string(),
            trust_level,
            anomaly_score,
            total_events: behavior.events.len(),
            successful_logins,
            failed_logins,
            common_ips,
            common_locations,
            avg_session_duration,
            typical_login_hours,
            last_seen: last_event.timestamp,
            created_at: behavior.created_at,
            updated_at: behavior.updated_at,
        })
    }

    /// Detect anomalies in recent behavior
    pub fn detect_anomalies(&self, user_id: &str, event: &LoginEvent) -> Result<AnomalyDetection> {
        let behaviors = self
            .user_behaviors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let behavior = behaviors
            .get(user_id)
            .ok_or_else(|| AiSecurityError::UserNotFound(user_id.to_string()))?;

        if behavior.events.len() < self.config.min_events_for_analysis {
            return Ok(AnomalyDetection {
                is_anomaly: false,
                score: 0.0,
                reasons: vec!["Insufficient historical data".to_string()],
                confidence: 0.1,
            });
        }

        let mut reasons = Vec::new();
        let mut anomaly_scores = Vec::new();

        // Check IP anomaly
        let ip_seen_before = behavior.ip_frequency.contains_key(&event.ip_address);
        if !ip_seen_before {
            reasons.push("New IP address".to_string());
            anomaly_scores.push(0.6);
        }

        // Check location anomaly
        if let Some(ref location) = event.location {
            let location_seen_before = behavior.location_frequency.contains_key(location);
            if !location_seen_before {
                reasons.push(format!("New location: {location}"));
                anomaly_scores.push(0.5);
            }
        }

        // Check time anomaly (unusual hour)
        let hour = event.timestamp.hour();
        let hour_count = behavior.hour_frequency.get(&hour).copied().unwrap_or(0);
        let total_logins = behavior.hour_frequency.values().sum::<usize>();
        let hour_probability = hour_count as f64 / total_logins as f64;
        if hour_probability < 0.05 && hour_count < 2 {
            reasons.push(format!("Unusual login hour: {hour}:00"));
            anomaly_scores.push(0.4);
        }

        // Check session duration anomaly (if available)
        if let Some(duration) = event.session_duration_seconds {
            if !behavior.session_durations.is_empty() {
                let (mean, std_dev) = calculate_mean_std(&behavior.session_durations);
                let z_score = ((duration as f64 - mean) / std_dev).abs();
                if z_score > self.config.anomaly_z_score_threshold {
                    reasons.push(format!(
                        "Unusual session duration: {duration} seconds (z-score: {z_score:.2})"
                    ));
                    anomaly_scores.push((z_score / 5.0).min(0.8));
                }
            }
        }

        // Check threat intelligence
        let threat_ips = self.threat_ips.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&threat_time) = threat_ips.get(&event.ip_address) {
            let hours_since_threat = (Utc::now() - threat_time).num_hours();
            if hours_since_threat <= self.config.threat_window_hours {
                reasons.push("IP address flagged as threat".to_string());
                anomaly_scores.push(0.9);
            }
        }

        // Calculate overall anomaly score
        let score = if anomaly_scores.is_empty() {
            0.0
        } else {
            anomaly_scores.iter().sum::<f64>() / anomaly_scores.len() as f64
        };

        let is_anomaly = score > 0.5 || !reasons.is_empty();
        let confidence = (behavior.events.len() as f64 / self.config.max_history as f64).min(1.0);

        Ok(AnomalyDetection {
            is_anomaly,
            score,
            reasons,
            confidence,
        })
    }

    /// Calculate overall anomaly score for a user
    fn calculate_anomaly_score(behavior: &UserBehavior) -> f64 {
        let mut score = 0.0;
        let mut factors = 0;

        // Factor 1: Failed login ratio
        let failed_ratio = behavior.events.iter().filter(|e| !e.success).count() as f64
            / behavior.events.len() as f64;
        score += failed_ratio * 0.5;
        factors += 1;

        // Factor 2: IP diversity (more IPs = higher anomaly)
        let ip_diversity = behavior.ip_frequency.len() as f64 / behavior.events.len() as f64;
        score += ip_diversity.min(0.3);
        factors += 1;

        // Factor 3: Location diversity
        if !behavior.location_frequency.is_empty() {
            let loc_diversity =
                behavior.location_frequency.len() as f64 / behavior.events.len() as f64;
            score += loc_diversity.min(0.2);
            factors += 1;
        }

        (score / f64::from(factors)).min(1.0)
    }

    /// Flag an IP address as threat
    pub fn flag_threat_ip(&mut self, ip_address: String) {
        let mut threat_ips = self.threat_ips.lock().unwrap_or_else(|e| e.into_inner());
        threat_ips.insert(ip_address, Utc::now());
    }

    /// Clear threat IP flag
    pub fn clear_threat_ip(&mut self, ip_address: &str) {
        let mut threat_ips = self.threat_ips.lock().unwrap_or_else(|e| e.into_inner());
        threat_ips.remove(ip_address);
    }

    /// Clean up old threat intelligence data
    pub fn cleanup_old_threats(&mut self) {
        let mut threat_ips = self.threat_ips.lock().unwrap_or_else(|e| e.into_inner());
        let cutoff = Utc::now() - Duration::hours(self.config.threat_window_hours);
        threat_ips.retain(|_, &mut timestamp| timestamp > cutoff);
    }

    /// Get statistics
    #[must_use]
    pub fn get_stats(&self) -> AiSecurityStats {
        let behaviors = self
            .user_behaviors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let threat_ips = self.threat_ips.lock().unwrap_or_else(|e| e.into_inner());

        let total_events: usize = behaviors.values().map(|b| b.events.len()).sum();
        let total_users = behaviors.len();

        AiSecurityStats {
            total_users,
            total_events,
            active_threats: threat_ips.len(),
        }
    }

    /// Clear all user behavior data
    pub fn clear_all_data(&mut self) {
        let mut behaviors = self
            .user_behaviors
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        behaviors.clear();
        let mut threat_ips = self.threat_ips.lock().unwrap_or_else(|e| e.into_inner());
        threat_ips.clear();
    }
}

impl Default for AiSecurityEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// AI security statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSecurityStats {
    /// Total users being tracked
    pub total_users: usize,
    /// Total events analyzed
    pub total_events: usize,
    /// Active threat IPs
    pub active_threats: usize,
}

/// Calculate mean and standard deviation
fn calculate_mean_std(values: &[u64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    let mean = values.iter().sum::<u64>() as f64 / values.len() as f64;

    let variance = values
        .iter()
        .map(|&v| {
            let diff = v as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / values.len() as f64;

    let std_dev = variance.sqrt();
    (mean, std_dev)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event(user_id: &str, ip: &str, success: bool) -> LoginEvent {
        LoginEvent {
            user_id: user_id.to_string(),
            timestamp: Utc::now(),
            ip_address: ip.to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            location: Some("US".to_string()),
            success,
            session_duration_seconds: Some(3600),
        }
    }

    #[test]
    fn test_ai_engine_creation() {
        let engine = AiSecurityEngine::new();
        let stats = engine.get_stats();
        assert_eq!(stats.total_users, 0);
        assert_eq!(stats.total_events, 0);
    }

    #[test]
    fn test_record_event() {
        let mut engine = AiSecurityEngine::new();
        let event = create_test_event("alice", "192.168.1.1", true);

        engine.record_event(event);

        let stats = engine.get_stats();
        assert_eq!(stats.total_users, 1);
        assert_eq!(stats.total_events, 1);
    }

    #[test]
    fn test_behavior_profile() {
        let mut engine = AiSecurityEngine::new();

        // Record enough events for analysis
        for _i in 0..15 {
            let event = create_test_event("alice", "192.168.1.1", true);
            engine.record_event(event);
        }

        let profile = engine.get_behavior_profile("alice").unwrap();
        assert_eq!(profile.user_id, "alice");
        assert_eq!(profile.total_events, 15);
        assert_eq!(profile.successful_logins, 15);
        assert_eq!(profile.failed_logins, 0);
    }

    #[test]
    fn test_insufficient_data() {
        let mut engine = AiSecurityEngine::new();

        // Record only a few events
        for _i in 0..5 {
            let event = create_test_event("alice", "192.168.1.1", true);
            engine.record_event(event);
        }

        let result = engine.get_behavior_profile("alice");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Insufficient data"));
    }

    #[test]
    fn test_trust_level_from_score() {
        assert_eq!(TrustLevel::from_score(0.95), TrustLevel::VeryHigh);
        assert_eq!(TrustLevel::from_score(0.8), TrustLevel::High);
        assert_eq!(TrustLevel::from_score(0.6), TrustLevel::Medium);
        assert_eq!(TrustLevel::from_score(0.4), TrustLevel::Low);
        assert_eq!(TrustLevel::from_score(0.1), TrustLevel::VeryLow);
    }

    #[test]
    fn test_trust_level_display() {
        assert_eq!(TrustLevel::VeryHigh.to_string(), "very_high");
        assert_eq!(TrustLevel::High.to_string(), "high");
        assert_eq!(TrustLevel::Medium.to_string(), "medium");
        assert_eq!(TrustLevel::Low.to_string(), "low");
        assert_eq!(TrustLevel::VeryLow.to_string(), "very_low");
    }

    #[test]
    fn test_anomaly_detection_new_ip() {
        let mut engine = AiSecurityEngine::new();

        // Record events with one IP
        for _i in 0..15 {
            let event = create_test_event("alice", "192.168.1.1", true);
            engine.record_event(event);
        }

        // Test with new IP
        let new_event = create_test_event("alice", "10.0.0.1", true);
        let anomaly = engine.detect_anomalies("alice", &new_event).unwrap();

        assert!(anomaly.is_anomaly);
        assert!(anomaly.score > 0.0);
        assert!(!anomaly.reasons.is_empty());
    }

    #[test]
    fn test_threat_ip_flagging() {
        let mut engine = AiSecurityEngine::new();

        engine.flag_threat_ip("192.168.1.100".to_string());

        let stats = engine.get_stats();
        assert_eq!(stats.active_threats, 1);

        engine.clear_threat_ip("192.168.1.100");

        let stats = engine.get_stats();
        assert_eq!(stats.active_threats, 0);
    }

    #[test]
    fn test_threat_detection() {
        let mut engine = AiSecurityEngine::new();

        // Record events
        for _i in 0..15 {
            let event = create_test_event("alice", "192.168.1.1", true);
            engine.record_event(event);
        }

        // Flag IP as threat
        engine.flag_threat_ip("10.0.0.1".to_string());

        // Test with threat IP
        let threat_event = create_test_event("alice", "10.0.0.1", true);
        let anomaly = engine.detect_anomalies("alice", &threat_event).unwrap();

        assert!(anomaly.is_anomaly);
        assert!(anomaly.score > 0.5);
    }

    #[test]
    fn test_calculate_mean_std() {
        let values = vec![100, 200, 300, 400, 500];
        let (mean, std_dev) = calculate_mean_std(&values);

        assert!((mean - 300.0).abs() < 0.1);
        assert!(std_dev > 0.0);
    }

    #[test]
    fn test_clear_all_data() {
        let mut engine = AiSecurityEngine::new();

        for _i in 0..10 {
            let event = create_test_event("alice", "192.168.1.1", true);
            engine.record_event(event);
        }

        engine.flag_threat_ip("192.168.1.100".to_string());

        engine.clear_all_data();

        let stats = engine.get_stats();
        assert_eq!(stats.total_users, 0);
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.active_threats, 0);
    }

    #[test]
    fn test_failed_login_ratio() {
        let mut engine = AiSecurityEngine::new();

        // Record mix of successful and failed logins
        for _i in 0..10 {
            let event = create_test_event("bob", "192.168.1.1", true);
            engine.record_event(event);
        }
        for _i in 0..5 {
            let event = create_test_event("bob", "192.168.1.1", false);
            engine.record_event(event);
        }

        let profile = engine.get_behavior_profile("bob").unwrap();
        assert_eq!(profile.successful_logins, 10);
        assert_eq!(profile.failed_logins, 5);
        assert!(profile.anomaly_score > 0.0);
    }

    #[test]
    fn test_custom_config() {
        let config = AiSecurityConfig {
            max_history: 500,
            min_events_for_analysis: 5,
            anomaly_z_score_threshold: 3.0,
            moving_average_window: 10,
            threat_window_hours: 48,
            trust_decay_per_day: 0.1,
        };

        let engine = AiSecurityEngine::with_config(config);
        let stats = engine.get_stats();
        assert_eq!(stats.total_users, 0);
    }
}
