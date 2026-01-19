//! Risk-Based Authentication
//!
//! Provides anomaly detection, device fingerprinting, and step-up authentication
//! to enhance security by identifying suspicious login attempts.
//!
//! # Features
//!
//! - **Anomaly Detection**: Detect unusual login patterns (location, device, time)
//! - **Device Fingerprinting**: Track and identify devices
//! - **Step-up Authentication**: Require additional verification for high-risk actions
//!
//! # Example
//!
//! ```
//! use oxify_authn::risk::{RiskAnalyzer, LoginContext, RiskLevel};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let analyzer = RiskAnalyzer::new();
//!
//! let context = LoginContext::builder()
//!     .user_id("user123")
//!     .ip_address("192.168.1.1")
//!     .user_agent("Mozilla/5.0...")
//!     .build();
//!
//! let risk = analyzer.analyze(&context).await?;
//! if risk.level() >= RiskLevel::High {
//!     // Require step-up authentication
//!     println!("High risk detected: {:?}", risk.reasons());
//! }
//! # Ok(())
//! # }
//! ```

use crate::types::AuthError;
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// Risk level for a login attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// No risk detected
    None,
    /// Low risk (minor anomaly)
    Low,
    /// Medium risk (notable anomaly)
    Medium,
    /// High risk (significant anomaly, recommend MFA)
    High,
    /// Critical risk (block access)
    Critical,
}

impl RiskLevel {
    /// Get the numeric score for this risk level
    #[must_use]
    pub fn score(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Low => 25,
            Self::Medium => 50,
            Self::High => 75,
            Self::Critical => 100,
        }
    }

    /// Convert a numeric score to a risk level
    #[must_use]
    pub fn from_score(score: u8) -> Self {
        match score {
            0..=10 => Self::None,
            11..=35 => Self::Low,
            36..=60 => Self::Medium,
            61..=85 => Self::High,
            _ => Self::Critical,
        }
    }
}

/// Reasons why a login might be flagged as risky
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskReason {
    /// Login from a new location
    NewLocation,
    /// Login from a new device
    NewDevice,
    /// Login at an unusual time
    UnusualTime,
    /// Rapid successive logins from different locations (impossible travel)
    ImpossibleTravel,
    /// Too many recent failed login attempts
    HighFailureRate,
    /// Login from a known malicious IP
    MaliciousIp,
    /// User agent mismatch with previous sessions
    UserAgentChange,
    /// Login after password change (should re-verify)
    RecentPasswordChange,
    /// Custom reason with description
    Custom(String),
}

/// Risk assessment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// Overall risk level
    level: RiskLevel,
    /// Risk score (0-100)
    score: u8,
    /// Reasons for the risk assessment
    reasons: Vec<RiskReason>,
    /// Whether step-up authentication is recommended
    requires_step_up: bool,
    /// Timestamp of the assessment
    timestamp: DateTime<Utc>,
}

impl RiskAssessment {
    /// Create a new risk assessment
    #[must_use]
    pub fn new(level: RiskLevel, reasons: Vec<RiskReason>) -> Self {
        Self {
            score: level.score(),
            level,
            reasons,
            requires_step_up: level >= RiskLevel::High,
            timestamp: Utc::now(),
        }
    }

    /// Get the risk level
    #[must_use]
    pub fn level(&self) -> RiskLevel {
        self.level
    }

    /// Get the risk score
    #[must_use]
    pub fn score(&self) -> u8 {
        self.score
    }

    /// Get the reasons for this risk level
    #[must_use]
    pub fn reasons(&self) -> &[RiskReason] {
        &self.reasons
    }

    /// Check if step-up authentication is required
    #[must_use]
    pub fn requires_step_up(&self) -> bool {
        self.requires_step_up
    }

    /// Get the timestamp of the assessment
    #[must_use]
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Context for a login attempt
#[derive(Debug, Clone)]
pub struct LoginContext {
    /// User ID attempting to log in
    user_id: String,
    /// IP address of the login attempt
    ip_address: String,
    /// User agent string
    user_agent: String,
    /// Timestamp of the login attempt
    timestamp: DateTime<Utc>,
    /// Geographic location (optional, derived from IP)
    location: Option<GeoLocation>,
    /// Device fingerprint (optional)
    device_fingerprint: Option<String>,
}

impl LoginContext {
    /// Create a new login context builder
    #[must_use]
    pub fn builder() -> LoginContextBuilder {
        LoginContextBuilder::default()
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

    /// Get the user agent
    #[must_use]
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Get the timestamp
    #[must_use]
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get the location
    #[must_use]
    pub fn location(&self) -> Option<&GeoLocation> {
        self.location.as_ref()
    }

    /// Get the device fingerprint
    #[must_use]
    pub fn device_fingerprint(&self) -> Option<&str> {
        self.device_fingerprint.as_deref()
    }
}

/// Builder for `LoginContext`
#[derive(Debug, Default)]
pub struct LoginContextBuilder {
    user_id: Option<String>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    location: Option<GeoLocation>,
    device_fingerprint: Option<String>,
}

impl LoginContextBuilder {
    /// Set the user ID
    #[must_use]
    pub fn user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set the IP address
    #[must_use]
    pub fn ip_address(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    /// Set the user agent
    #[must_use]
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Set the timestamp
    #[must_use]
    pub fn timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Set the location
    #[must_use]
    pub fn location(mut self, location: GeoLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the device fingerprint
    #[must_use]
    pub fn device_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.device_fingerprint = Some(fingerprint.into());
        self
    }

    /// Build the `LoginContext`
    pub fn build(self) -> LoginContext {
        LoginContext {
            user_id: self.user_id.unwrap_or_default(),
            ip_address: self.ip_address.unwrap_or_default(),
            user_agent: self.user_agent.unwrap_or_default(),
            timestamp: self.timestamp.unwrap_or_else(Utc::now),
            location: self.location,
            device_fingerprint: self.device_fingerprint,
        }
    }
}

/// Geographic location
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoLocation {
    /// Country code (ISO 3166-1 alpha-2)
    pub country: String,
    /// City name
    pub city: Option<String>,
    /// Latitude
    pub latitude: Option<f64>,
    /// Longitude
    pub longitude: Option<f64>,
}

impl GeoLocation {
    /// Create a new geographic location
    pub fn new(country: impl Into<String>) -> Self {
        Self {
            country: country.into(),
            city: None,
            latitude: None,
            longitude: None,
        }
    }

    /// Set the city
    #[must_use]
    pub fn with_city(mut self, city: impl Into<String>) -> Self {
        self.city = Some(city.into());
        self
    }

    /// Set the coordinates
    #[must_use]
    pub fn with_coordinates(mut self, lat: f64, lon: f64) -> Self {
        self.latitude = Some(lat);
        self.longitude = Some(lon);
        self
    }

    /// Calculate distance to another location (in kilometers)
    #[must_use]
    pub fn distance_to(&self, other: &Self) -> Option<f64> {
        let (lat1, lon1) = (self.latitude?, self.longitude?);
        let (lat2, lon2) = (other.latitude?, other.longitude?);

        // Haversine formula
        let r = 6371.0; // Earth's radius in km
        let d_lat = (lat2 - lat1).to_radians();
        let d_lon = (lon2 - lon1).to_radians();

        let a = (d_lat / 2.0).sin().mul_add(
            (d_lat / 2.0).sin(),
            lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2),
        );

        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        Some(r * c)
    }
}

/// Historical login data for a user
#[derive(Debug, Clone, Default)]
struct UserLoginHistory {
    /// Recent login locations
    locations: Vec<GeoLocation>,
    /// Recent IP addresses
    ip_addresses: Vec<String>,
    /// Recent user agents
    user_agents: Vec<String>,
    /// Recent device fingerprints
    device_fingerprints: Vec<String>,
    /// Recent login times (for pattern analysis)
    login_times: Vec<DateTime<Utc>>,
    /// Recent failed login attempts
    failed_attempts: Vec<DateTime<Utc>>,
    /// Last password change timestamp
    last_password_change: Option<DateTime<Utc>>,
}

/// Risk analyzer for authentication
pub struct RiskAnalyzer {
    /// User login history (in-memory store for now)
    history: Arc<Mutex<HashMap<String, UserLoginHistory>>>,
    /// Configuration for risk analysis
    config: RiskConfig,
}

/// Configuration for risk analysis
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// Maximum distance for location change before flagging (km)
    pub max_location_distance_km: f64,
    /// Maximum time between logins from different locations (hours)
    pub impossible_travel_hours: i64,
    /// Number of recent logins to keep in history
    pub history_size: usize,
    /// Number of failed attempts before flagging
    pub max_failed_attempts: usize,
    /// Time window for failed attempts (hours)
    pub failed_attempt_window_hours: i64,
    /// Hours after password change to flag logins
    pub password_change_grace_hours: i64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_location_distance_km: 500.0, // 500 km
            impossible_travel_hours: 2,      // 2 hours
            history_size: 10,                // Keep last 10 logins
            max_failed_attempts: 5,          // 5 failed attempts
            failed_attempt_window_hours: 24, // Within 24 hours
            password_change_grace_hours: 24, // 24 hours after password change
        }
    }
}

impl RiskAnalyzer {
    /// Create a new risk analyzer with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(RiskConfig::default())
    }

    /// Create a new risk analyzer with custom configuration
    #[must_use]
    pub fn with_config(config: RiskConfig) -> Self {
        Self {
            history: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Analyze a login attempt and return a risk assessment
    pub async fn analyze(&self, context: &LoginContext) -> Result<RiskAssessment, AuthError> {
        let mut reasons = Vec::new();
        let mut risk_score = 0u8;

        let history = self.get_user_history(&context.user_id);

        // Check for new location
        if let Some(location) = &context.location {
            if !history
                .locations
                .iter()
                .any(|loc| loc.country == location.country)
            {
                reasons.push(RiskReason::NewLocation);
                risk_score += 20;
            }
        }

        // Check for new device
        if let Some(fingerprint) = &context.device_fingerprint {
            if !history.device_fingerprints.contains(fingerprint) {
                reasons.push(RiskReason::NewDevice);
                risk_score += 15;
            }
        }

        // Check for new user agent
        if !history.user_agents.contains(&context.user_agent) {
            reasons.push(RiskReason::UserAgentChange);
            risk_score += 10;
        }

        // Check for impossible travel
        if let Some(location) = &context.location {
            if let Some(last_location) = history.locations.last() {
                if let Some(distance) = location.distance_to(last_location) {
                    if distance > self.config.max_location_distance_km {
                        if let Some(last_login) = history.login_times.last() {
                            let time_diff = context.timestamp.signed_duration_since(*last_login);
                            if time_diff < Duration::hours(self.config.impossible_travel_hours) {
                                reasons.push(RiskReason::ImpossibleTravel);
                                risk_score += 45; // High severity - impossible travel is a serious security concern
                            }
                        }
                    }
                }
            }
        }

        // Check for high failure rate
        let cutoff = Utc::now() - Duration::hours(self.config.failed_attempt_window_hours);
        let recent_failures = history
            .failed_attempts
            .iter()
            .filter(|&&t| t > cutoff)
            .count();

        if recent_failures >= self.config.max_failed_attempts {
            reasons.push(RiskReason::HighFailureRate);
            risk_score += 30;
        }

        // Check for recent password change
        if let Some(pwd_change) = history.last_password_change {
            let time_since_change = context.timestamp.signed_duration_since(pwd_change);
            if time_since_change < Duration::hours(self.config.password_change_grace_hours) {
                reasons.push(RiskReason::RecentPasswordChange);
                risk_score += 15;
            }
        }

        // Check for unusual login time (simple heuristic: outside 6 AM - 11 PM)
        let hour = context.timestamp.time().hour();
        if !(6..=23).contains(&hour) {
            reasons.push(RiskReason::UnusualTime);
            risk_score += 10;
        }

        let level = RiskLevel::from_score(risk_score);
        Ok(RiskAssessment::new(level, reasons))
    }

    /// Record a successful login
    pub fn record_success(&self, context: &LoginContext) {
        let mut history_map = self.history.lock().unwrap();
        let history = history_map.entry(context.user_id.clone()).or_default();

        // Add to history with size limit
        if let Some(location) = &context.location {
            history.locations.push(location.clone());
            if history.locations.len() > self.config.history_size {
                history.locations.remove(0);
            }
        }

        if !history.ip_addresses.contains(&context.ip_address) {
            history.ip_addresses.push(context.ip_address.clone());
            if history.ip_addresses.len() > self.config.history_size {
                history.ip_addresses.remove(0);
            }
        }

        if !history.user_agents.contains(&context.user_agent) {
            history.user_agents.push(context.user_agent.clone());
            if history.user_agents.len() > self.config.history_size {
                history.user_agents.remove(0);
            }
        }

        if let Some(fingerprint) = &context.device_fingerprint {
            if !history.device_fingerprints.contains(fingerprint) {
                history.device_fingerprints.push(fingerprint.clone());
                if history.device_fingerprints.len() > self.config.history_size {
                    history.device_fingerprints.remove(0);
                }
            }
        }

        history.login_times.push(context.timestamp);
        if history.login_times.len() > self.config.history_size {
            history.login_times.remove(0);
        }
    }

    /// Record a failed login attempt
    pub fn record_failure(&self, user_id: &str) {
        let mut history_map = self.history.lock().unwrap();
        let history = history_map.entry(user_id.to_string()).or_default();

        history.failed_attempts.push(Utc::now());

        // Clean up old failed attempts
        let cutoff = Utc::now() - Duration::hours(self.config.failed_attempt_window_hours);
        history.failed_attempts.retain(|&t| t > cutoff);
    }

    /// Record a password change
    pub fn record_password_change(&self, user_id: &str) {
        let mut history_map = self.history.lock().unwrap();
        let history = history_map.entry(user_id.to_string()).or_default();

        history.last_password_change = Some(Utc::now());
    }

    /// Get user login history
    fn get_user_history(&self, user_id: &str) -> UserLoginHistory {
        let history_map = self.history.lock().unwrap();
        history_map.get(user_id).cloned().unwrap_or_default()
    }

    /// Clear history for a user (e.g., after account deletion)
    pub fn clear_history(&self, user_id: &str) {
        let mut history_map = self.history.lock().unwrap();
        history_map.remove(user_id);
    }
}

impl Default for RiskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Device fingerprint generator
pub struct DeviceFingerprint;

impl DeviceFingerprint {
    /// Generate a device fingerprint from user agent and IP
    #[must_use]
    pub fn generate(ip: &str, user_agent: &str) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(ip.as_bytes());
        hasher.update(user_agent.as_bytes());
        let result = hasher.finalize();

        format!("{result:x}")
    }

    /// Parse IP address for validation
    pub fn parse_ip(ip: &str) -> Result<IpAddr, AuthError> {
        IpAddr::from_str(ip).map_err(|_| AuthError::InvalidInput("Invalid IP address".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_risk_level_ordering() {
        assert!(RiskLevel::None < RiskLevel::Low);
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[tokio::test]
    async fn test_risk_level_score_conversion() {
        assert_eq!(RiskLevel::from_score(0), RiskLevel::None);
        assert_eq!(RiskLevel::from_score(25), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(50), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(75), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(100), RiskLevel::Critical);
    }

    #[tokio::test]
    async fn test_login_context_builder() {
        let context = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.1")
            .user_agent("Mozilla/5.0")
            .build();

        assert_eq!(context.user_id(), "user123");
        assert_eq!(context.ip_address(), "192.168.1.1");
        assert_eq!(context.user_agent(), "Mozilla/5.0");
    }

    #[tokio::test]
    async fn test_geo_location_distance() {
        let tokyo = GeoLocation::new("JP").with_coordinates(35.6762, 139.6503);
        let new_york = GeoLocation::new("US").with_coordinates(40.7128, -74.0060);

        let distance = tokyo.distance_to(&new_york).unwrap();
        assert!(distance > 10000.0); // ~10,850 km
        assert!(distance < 11000.0);
    }

    #[tokio::test]
    async fn test_risk_analyzer_new_location() {
        let analyzer = RiskAnalyzer::new();

        let context1 = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.1")
            .user_agent("Mozilla/5.0")
            .location(GeoLocation::new("US"))
            .build();

        // First login - should be flagged as new location
        let risk = analyzer.analyze(&context1).await.unwrap();
        assert!(risk.reasons().contains(&RiskReason::NewLocation));
        assert!(risk.level() >= RiskLevel::Low);

        // Record the successful login
        analyzer.record_success(&context1);

        // Second login from same location - should be fine
        let context2 = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.1")
            .user_agent("Mozilla/5.0")
            .location(GeoLocation::new("US"))
            .build();

        let risk = analyzer.analyze(&context2).await.unwrap();
        assert!(!risk.reasons().contains(&RiskReason::NewLocation));
    }

    #[tokio::test]
    async fn test_risk_analyzer_new_device() {
        let analyzer = RiskAnalyzer::new();

        let context1 = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.1")
            .user_agent("Mozilla/5.0")
            .device_fingerprint("device1")
            .build();

        let risk = analyzer.analyze(&context1).await.unwrap();
        assert!(risk.reasons().contains(&RiskReason::NewDevice));

        analyzer.record_success(&context1);

        // Same device - no risk
        let context2 = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.2")
            .user_agent("Mozilla/5.0")
            .device_fingerprint("device1")
            .build();

        let risk = analyzer.analyze(&context2).await.unwrap();
        assert!(!risk.reasons().contains(&RiskReason::NewDevice));
    }

    #[tokio::test]
    async fn test_risk_analyzer_high_failure_rate() {
        let analyzer = RiskAnalyzer::new();

        // Record 6 failed attempts
        for _ in 0..6 {
            analyzer.record_failure("user123");
        }

        let context = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.1")
            .user_agent("Mozilla/5.0")
            .build();

        let risk = analyzer.analyze(&context).await.unwrap();
        assert!(risk.reasons().contains(&RiskReason::HighFailureRate));
        assert!(risk.level() >= RiskLevel::Medium);
    }

    #[tokio::test]
    async fn test_risk_analyzer_impossible_travel() {
        let config = RiskConfig {
            max_location_distance_km: 500.0,
            impossible_travel_hours: 2,
            ..Default::default()
        };
        let analyzer = RiskAnalyzer::with_config(config);

        let tokyo = GeoLocation::new("JP").with_coordinates(35.6762, 139.6503);
        let new_york = GeoLocation::new("US").with_coordinates(40.7128, -74.0060);

        let base_time = Utc::now();

        // First login from Tokyo
        let context1 = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.1")
            .user_agent("Mozilla/5.0")
            .location(tokyo.clone())
            .timestamp(base_time)
            .build();

        analyzer.record_success(&context1);

        // Second login from New York 1 hour later (impossible travel)
        let context2 = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.2")
            .user_agent("Mozilla/5.0")
            .location(new_york)
            .timestamp(base_time + Duration::hours(1))
            .build();

        let risk = analyzer.analyze(&context2).await.unwrap();
        assert!(risk.reasons().contains(&RiskReason::ImpossibleTravel));
        assert!(risk.level() >= RiskLevel::High);
    }

    #[tokio::test]
    async fn test_device_fingerprint_generation() {
        let fp1 = DeviceFingerprint::generate("192.168.1.1", "Mozilla/5.0");
        let fp2 = DeviceFingerprint::generate("192.168.1.1", "Mozilla/5.0");
        let fp3 = DeviceFingerprint::generate("192.168.1.2", "Mozilla/5.0");

        assert_eq!(fp1, fp2); // Same input = same fingerprint
        assert_ne!(fp1, fp3); // Different input = different fingerprint
    }

    #[tokio::test]
    async fn test_device_fingerprint_ip_parsing() {
        assert!(DeviceFingerprint::parse_ip("192.168.1.1").is_ok());
        assert!(DeviceFingerprint::parse_ip("2001:0db8:85a3::8a2e:0370:7334").is_ok());
        assert!(DeviceFingerprint::parse_ip("invalid").is_err());
    }

    #[tokio::test]
    async fn test_risk_assessment_step_up() {
        let assessment = RiskAssessment::new(RiskLevel::High, vec![RiskReason::ImpossibleTravel]);
        assert!(assessment.requires_step_up());

        let assessment = RiskAssessment::new(RiskLevel::Low, vec![RiskReason::NewLocation]);
        assert!(!assessment.requires_step_up());
    }

    #[tokio::test]
    async fn test_clear_history() {
        let analyzer = RiskAnalyzer::new();

        let context = LoginContext::builder()
            .user_id("user123")
            .ip_address("192.168.1.1")
            .user_agent("Mozilla/5.0")
            .location(GeoLocation::new("US"))
            .build();

        analyzer.record_success(&context);

        // Verify history is recorded
        let risk_before = analyzer.analyze(&context).await.unwrap();
        assert_eq!(risk_before.level(), RiskLevel::None); // Same context, no risk

        // Clear history
        analyzer.clear_history("user123");

        // Next login with location should be flagged as new location
        let risk = analyzer.analyze(&context).await.unwrap();
        assert!(risk.level() >= RiskLevel::Low);
        assert!(risk.reasons().contains(&RiskReason::NewLocation));
    }
}
