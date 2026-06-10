//! Anomaly Detection for Authorization
//!
//! This module provides ML-powered anomaly detection to identify suspicious access patterns
//! such as unusual permission checks, privilege escalation attempts, and abnormal behavior.
//!
//! # Features
//! - Track access patterns (frequency, time, resources)
//! - Build baseline models of normal behavior
//! - Detect deviations using statistical methods
//! - Generate alerts for suspicious patterns
//!
//! # Example
//! ```rust,ignore
//! use oxify_authz::anomaly::{AnomalyDetector, AnomalyConfig, AccessEvent};
//! use std::time::Duration;
//!
//! let config = AnomalyConfig::default();
//! let mut detector = AnomalyDetector::new(config);
//!
//! // Record access events
//! let event = AccessEvent {
//!     subject_id: "user:alice".to_string(),
//!     resource_id: "doc:sensitive".to_string(),
//!     relation: "read".to_string(),
//!     granted: true,
//!     timestamp: std::time::SystemTime::now(),
//! };
//!
//! // Check for anomalies
//! if let Some(anomaly) = detector.check_anomaly(&event) {
//!     println!("Anomaly detected: {:?}", anomaly);
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Configuration for anomaly detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyConfig {
    /// Minimum number of events before anomaly detection kicks in
    pub min_baseline_events: usize,

    /// Z-score threshold for statistical anomalies (typically 2.0-3.0)
    pub zscore_threshold: f64,

    /// Time window for frequency analysis
    pub frequency_window: Duration,

    /// Maximum allowed access rate (events per minute)
    pub max_access_rate: f64,

    /// Enable temporal anomaly detection (unusual hours)
    pub enable_temporal_detection: bool,

    /// Enable privilege escalation detection
    pub enable_privilege_escalation: bool,

    /// Retention period for historical data
    pub retention_period: Duration,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            min_baseline_events: 100,
            zscore_threshold: 2.5,
            frequency_window: Duration::from_secs(3600), // 1 hour
            max_access_rate: 100.0,                      // 100 requests per minute
            enable_temporal_detection: true,
            enable_privilege_escalation: true,
            retention_period: Duration::from_secs(30 * 24 * 3600), // 30 days
        }
    }
}

/// An access event to be analyzed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessEvent {
    pub subject_id: String,
    pub resource_id: String,
    pub relation: String,
    pub granted: bool,
    pub timestamp: SystemTime,
}

/// Type of anomaly detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnomalyType {
    /// Unusual access frequency (statistical outlier)
    UnusualFrequency,

    /// Access at unusual time (e.g., 3 AM when user normally works 9-5)
    UnusualTime,

    /// Accessing resource user rarely or never accessed before
    UnusualResource,

    /// Too many denied permission checks (potential privilege escalation)
    PrivilegeEscalation,

    /// Burst of requests exceeding rate limit
    RateLimitExceeded,

    /// Multiple anomaly indicators combined
    Combined(Vec<AnomalyType>),
}

/// Details about a detected anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    pub anomaly_type: AnomalyType,
    pub subject_id: String,
    pub resource_id: String,
    pub severity: f64, // 0.0 to 1.0
    pub description: String,
    pub timestamp: SystemTime,
}

/// Statistics for a subject's access pattern
#[derive(Debug, Clone)]
struct SubjectStats {
    total_events: usize,
    denied_events: usize,
    resource_access_count: HashMap<String, usize>,
    hourly_distribution: [usize; 24],
    recent_events: Vec<SystemTime>,
}

impl SubjectStats {
    fn new() -> Self {
        Self {
            total_events: 0,
            denied_events: 0,
            resource_access_count: HashMap::new(),
            hourly_distribution: [0; 24],
            recent_events: Vec::new(),
        }
    }
}

/// Main anomaly detector
pub struct AnomalyDetector {
    config: AnomalyConfig,
    subject_stats: HashMap<String, SubjectStats>,
}

impl AnomalyDetector {
    /// Create a new anomaly detector with the given configuration
    pub fn new(config: AnomalyConfig) -> Self {
        Self {
            config,
            subject_stats: HashMap::new(),
        }
    }

    /// Record an access event and check for anomalies
    pub fn check_anomaly(&mut self, event: &AccessEvent) -> Option<Anomaly> {
        // Update statistics
        self.update_stats(event);

        // Get or create stats for this subject
        let stats = self.subject_stats.get(&event.subject_id)?;

        // Collect detected anomalies
        let mut detected = Vec::new();

        // Skip anomaly detection if we don't have enough baseline data
        if stats.total_events < self.config.min_baseline_events {
            return None;
        }

        // 1. Check for unusual frequency
        if let Some(freq_anomaly) = self.check_frequency_anomaly(event, stats) {
            detected.push(freq_anomaly);
        }

        // 2. Check for unusual time
        if self.config.enable_temporal_detection {
            if let Some(time_anomaly) = self.check_temporal_anomaly(event, stats) {
                detected.push(time_anomaly);
            }
        }

        // 3. Check for unusual resource access
        if let Some(resource_anomaly) = self.check_resource_anomaly(event, stats) {
            detected.push(resource_anomaly);
        }

        // 4. Check for privilege escalation
        if self.config.enable_privilege_escalation {
            if let Some(privesc_anomaly) = self.check_privilege_escalation(event, stats) {
                detected.push(privesc_anomaly);
            }
        }

        // 5. Check for rate limit exceeded
        if let Some(rate_anomaly) = self.check_rate_limit(event, stats) {
            detected.push(rate_anomaly);
        }

        // Return combined anomaly if any detected
        if !detected.is_empty() {
            let severity = detected
                .iter()
                .map(|a| a.severity)
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0);

            let anomaly_type = if detected.len() == 1 {
                detected[0].anomaly_type.clone()
            } else {
                AnomalyType::Combined(detected.iter().map(|a| a.anomaly_type.clone()).collect())
            };

            Some(Anomaly {
                anomaly_type,
                subject_id: event.subject_id.clone(),
                resource_id: event.resource_id.clone(),
                severity,
                description: format!("Multiple anomalies detected: {} indicators", detected.len()),
                timestamp: event.timestamp,
            })
        } else {
            None
        }
    }

    fn update_stats(&mut self, event: &AccessEvent) {
        let stats = self
            .subject_stats
            .entry(event.subject_id.clone())
            .or_insert_with(SubjectStats::new);

        stats.total_events += 1;
        if !event.granted {
            stats.denied_events += 1;
        }

        *stats
            .resource_access_count
            .entry(event.resource_id.clone())
            .or_insert(0) += 1;

        // Update hourly distribution
        if let Ok(duration) = event.timestamp.duration_since(SystemTime::UNIX_EPOCH) {
            let hour = ((duration.as_secs() / 3600) % 24) as usize;
            stats.hourly_distribution[hour] += 1;
        }

        // Track recent events for rate limiting
        stats.recent_events.push(event.timestamp);

        // Clean up old events
        let cutoff = event
            .timestamp
            .checked_sub(self.config.retention_period)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        stats.recent_events.retain(|&t| t >= cutoff);
    }

    fn check_frequency_anomaly(
        &self,
        event: &AccessEvent,
        stats: &SubjectStats,
    ) -> Option<Anomaly> {
        // Calculate access frequency for this resource
        let resource_count = *stats
            .resource_access_count
            .get(&event.resource_id)
            .unwrap_or(&0);

        // Calculate mean and std dev of all resource access counts
        let counts: Vec<usize> = stats.resource_access_count.values().copied().collect();
        if counts.is_empty() {
            return None;
        }

        let mean = counts.iter().sum::<usize>() as f64 / counts.len() as f64;
        let variance = counts
            .iter()
            .map(|&c| {
                let diff = c as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / counts.len() as f64;
        let std_dev = variance.sqrt();

        // Calculate z-score
        if std_dev == 0.0 {
            return None;
        }

        let zscore = (resource_count as f64 - mean) / std_dev;

        if zscore.abs() > self.config.zscore_threshold {
            let severity = (zscore.abs() / self.config.zscore_threshold).min(1.0);
            Some(Anomaly {
                anomaly_type: AnomalyType::UnusualFrequency,
                subject_id: event.subject_id.clone(),
                resource_id: event.resource_id.clone(),
                severity,
                description: format!("Unusual access frequency (z-score: {:.2})", zscore),
                timestamp: event.timestamp,
            })
        } else {
            None
        }
    }

    fn check_temporal_anomaly(&self, event: &AccessEvent, stats: &SubjectStats) -> Option<Anomaly> {
        // Extract hour from timestamp
        let hour = if let Ok(duration) = event.timestamp.duration_since(SystemTime::UNIX_EPOCH) {
            ((duration.as_secs() / 3600) % 24) as usize
        } else {
            return None;
        };

        // Check if this hour is unusual for this user
        let total_accesses: usize = stats.hourly_distribution.iter().sum();
        if total_accesses == 0 {
            return None;
        }

        let expected_proportion = stats.hourly_distribution[hour] as f64 / total_accesses as f64;

        // If user has never or rarely accessed at this hour (< 5% of accesses)
        if expected_proportion < 0.05 {
            Some(Anomaly {
                anomaly_type: AnomalyType::UnusualTime,
                subject_id: event.subject_id.clone(),
                resource_id: event.resource_id.clone(),
                severity: 1.0 - expected_proportion,
                description: format!(
                    "Access at unusual hour: {}:00 (only {:.1}% of normal activity)",
                    hour,
                    expected_proportion * 100.0
                ),
                timestamp: event.timestamp,
            })
        } else {
            None
        }
    }

    fn check_resource_anomaly(&self, event: &AccessEvent, stats: &SubjectStats) -> Option<Anomaly> {
        let resource_count = *stats
            .resource_access_count
            .get(&event.resource_id)
            .unwrap_or(&0);

        // First-time access to a new resource
        if resource_count <= 1 && stats.total_events > self.config.min_baseline_events {
            Some(Anomaly {
                anomaly_type: AnomalyType::UnusualResource,
                subject_id: event.subject_id.clone(),
                resource_id: event.resource_id.clone(),
                severity: 0.6,
                description: "First-time access to this resource".to_string(),
                timestamp: event.timestamp,
            })
        } else {
            None
        }
    }

    fn check_privilege_escalation(
        &self,
        event: &AccessEvent,
        stats: &SubjectStats,
    ) -> Option<Anomaly> {
        if !event.granted && stats.total_events > 0 {
            let denial_rate = stats.denied_events as f64 / stats.total_events as f64;

            // High denial rate indicates potential privilege escalation attempts
            if denial_rate > 0.3 {
                Some(Anomaly {
                    anomaly_type: AnomalyType::PrivilegeEscalation,
                    subject_id: event.subject_id.clone(),
                    resource_id: event.resource_id.clone(),
                    severity: denial_rate.min(1.0),
                    description: format!(
                        "High denial rate: {:.1}% of checks denied",
                        denial_rate * 100.0
                    ),
                    timestamp: event.timestamp,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    fn check_rate_limit(&self, event: &AccessEvent, stats: &SubjectStats) -> Option<Anomaly> {
        // Count events in the last minute
        let one_minute_ago = event
            .timestamp
            .checked_sub(Duration::from_secs(60))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let recent_count = stats
            .recent_events
            .iter()
            .filter(|&&t| t >= one_minute_ago)
            .count();

        if recent_count as f64 > self.config.max_access_rate {
            Some(Anomaly {
                anomaly_type: AnomalyType::RateLimitExceeded,
                subject_id: event.subject_id.clone(),
                resource_id: event.resource_id.clone(),
                severity: ((recent_count as f64 / self.config.max_access_rate) - 1.0).min(1.0),
                description: format!(
                    "Rate limit exceeded: {} requests in last minute",
                    recent_count
                ),
                timestamp: event.timestamp,
            })
        } else {
            None
        }
    }

    /// Get statistics for a specific subject
    pub fn get_subject_stats(&self, subject_id: &str) -> Option<AnomalyStats> {
        let stats = self.subject_stats.get(subject_id)?;

        Some(AnomalyStats {
            total_events: stats.total_events,
            denied_events: stats.denied_events,
            unique_resources: stats.resource_access_count.len(),
            denial_rate: if stats.total_events > 0 {
                stats.denied_events as f64 / stats.total_events as f64
            } else {
                0.0
            },
        })
    }

    /// Clear old statistics to free memory
    pub fn cleanup(&mut self, cutoff: SystemTime) {
        for stats in self.subject_stats.values_mut() {
            stats.recent_events.retain(|&t| t >= cutoff);
        }

        // Remove subjects with no recent activity
        self.subject_stats
            .retain(|_, stats| !stats.recent_events.is_empty());
    }
}

/// Aggregated statistics for a subject
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyStats {
    pub total_events: usize,
    pub denied_events: usize,
    pub unique_resources: usize,
    pub denial_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anomaly_detector_creation() {
        let config = AnomalyConfig::default();
        let detector = AnomalyDetector::new(config);
        assert_eq!(detector.subject_stats.len(), 0);
    }

    #[test]
    fn test_baseline_building() {
        let config = AnomalyConfig {
            min_baseline_events: 10,
            ..Default::default()
        };
        let mut detector = AnomalyDetector::new(config);

        // Build baseline with normal events
        for i in 0..20 {
            let event = AccessEvent {
                subject_id: "user:alice".to_string(),
                resource_id: format!("doc:{}", i % 5),
                relation: "read".to_string(),
                granted: true,
                timestamp: SystemTime::now(),
            };
            let _result = detector.check_anomaly(&event);
        }

        let stats = detector.get_subject_stats("user:alice").unwrap();
        assert_eq!(stats.total_events, 20);
        assert_eq!(stats.unique_resources, 5);
    }

    #[test]
    fn test_unusual_resource_detection() {
        let config = AnomalyConfig {
            min_baseline_events: 10,
            ..Default::default()
        };
        let mut detector = AnomalyDetector::new(config);

        // Build baseline with normal events
        for _i in 0..15 {
            let event = AccessEvent {
                subject_id: "user:bob".to_string(),
                resource_id: "doc:normal".to_string(),
                relation: "read".to_string(),
                granted: true,
                timestamp: SystemTime::now(),
            };
            let _result = detector.check_anomaly(&event);
            std::thread::sleep(Duration::from_millis(10));
        }

        // Access unusual resource
        let unusual_event = AccessEvent {
            subject_id: "user:bob".to_string(),
            resource_id: "doc:sensitive".to_string(),
            relation: "read".to_string(),
            granted: true,
            timestamp: SystemTime::now(),
        };

        let anomaly = detector.check_anomaly(&unusual_event);
        assert!(anomaly.is_some());
        let anomaly = anomaly.unwrap();
        assert_eq!(anomaly.anomaly_type, AnomalyType::UnusualResource);
    }

    #[test]
    fn test_privilege_escalation_detection() {
        let config = AnomalyConfig {
            min_baseline_events: 10,
            enable_privilege_escalation: true,
            ..Default::default()
        };
        let mut detector = AnomalyDetector::new(config);

        // Build baseline with many denied events
        for _i in 0..15 {
            let event = AccessEvent {
                subject_id: "user:eve".to_string(),
                resource_id: format!("doc:{}", _i),
                relation: "admin".to_string(),
                granted: _i % 2 == 0, // 50% denial rate
                timestamp: SystemTime::now(),
            };
            let _result = detector.check_anomaly(&event);
        }

        // Another denied event should trigger anomaly
        let denied_event = AccessEvent {
            subject_id: "user:eve".to_string(),
            resource_id: "doc:admin".to_string(),
            relation: "admin".to_string(),
            granted: false,
            timestamp: SystemTime::now(),
        };

        let anomaly = detector.check_anomaly(&denied_event);
        assert!(anomaly.is_some());
        let anomaly = anomaly.unwrap();
        match anomaly.anomaly_type {
            AnomalyType::PrivilegeEscalation => {}
            AnomalyType::Combined(types) => {
                assert!(types.contains(&AnomalyType::PrivilegeEscalation));
            }
            _ => panic!("Expected PrivilegeEscalation anomaly"),
        }
    }

    #[test]
    fn test_rate_limit_detection() {
        let config = AnomalyConfig {
            min_baseline_events: 10,
            max_access_rate: 5.0,
            ..Default::default()
        };
        let mut detector = AnomalyDetector::new(config);

        // Build baseline
        for _ in 0..15 {
            let event = AccessEvent {
                subject_id: "user:charlie".to_string(),
                resource_id: "doc:test".to_string(),
                relation: "read".to_string(),
                granted: true,
                timestamp: SystemTime::now(),
            };
            detector.check_anomaly(&event);
            std::thread::sleep(Duration::from_millis(200)); // Spread out over time
        }

        // Send burst of requests
        for _ in 0..10 {
            let event = AccessEvent {
                subject_id: "user:charlie".to_string(),
                resource_id: "doc:test".to_string(),
                relation: "read".to_string(),
                granted: true,
                timestamp: SystemTime::now(),
            };
            let anomaly = detector.check_anomaly(&event);
            if let Some(anomaly) = anomaly {
                match anomaly.anomaly_type {
                    AnomalyType::RateLimitExceeded => return,
                    AnomalyType::Combined(ref types)
                        if types.contains(&AnomalyType::RateLimitExceeded) =>
                    {
                        return
                    }
                    _ => {}
                }
            }
        }

        panic!("Expected RateLimitExceeded anomaly");
    }

    #[test]
    fn test_cleanup() {
        let config = AnomalyConfig::default();
        let mut detector = AnomalyDetector::new(config);

        // Add some events
        for i in 0..10 {
            let event = AccessEvent {
                subject_id: format!("user:{}", i),
                resource_id: "doc:test".to_string(),
                relation: "read".to_string(),
                granted: true,
                timestamp: SystemTime::now(),
            };
            detector.check_anomaly(&event);
        }

        assert_eq!(detector.subject_stats.len(), 10);

        // Cleanup old events
        let future = SystemTime::now() + Duration::from_secs(3600);
        detector.cleanup(future);

        assert_eq!(detector.subject_stats.len(), 0);
    }
}
