//! Audit logging module for compliance and security tracking.
//!
//! This module provides:
//! - Comprehensive audit trail of all OCR operations
//! - User activity tracking
//! - Compliance reporting
//! - Data retention policies
//! - Export capabilities for audit logs

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Audit event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    /// OCR processing started
    ProcessingStarted,
    /// OCR processing completed
    ProcessingCompleted,
    /// OCR processing failed
    ProcessingFailed,
    /// API key created
    ApiKeyCreated,
    /// API key revoked
    ApiKeyRevoked,
    /// API key deleted
    ApiKeyDeleted,
    /// Access denied
    AccessDenied,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Quota exceeded
    QuotaExceeded,
    /// Configuration changed
    ConfigurationChanged,
    /// Data exported
    DataExported,
    /// Audit log accessed
    AuditLogAccessed,
}

impl std::fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProcessingStarted => write!(f, "PROCESSING_STARTED"),
            Self::ProcessingCompleted => write!(f, "PROCESSING_COMPLETED"),
            Self::ProcessingFailed => write!(f, "PROCESSING_FAILED"),
            Self::ApiKeyCreated => write!(f, "API_KEY_CREATED"),
            Self::ApiKeyRevoked => write!(f, "API_KEY_REVOKED"),
            Self::ApiKeyDeleted => write!(f, "API_KEY_DELETED"),
            Self::AccessDenied => write!(f, "ACCESS_DENIED"),
            Self::RateLimitExceeded => write!(f, "RATE_LIMIT_EXCEEDED"),
            Self::QuotaExceeded => write!(f, "QUOTA_EXCEEDED"),
            Self::ConfigurationChanged => write!(f, "CONFIGURATION_CHANGED"),
            Self::DataExported => write!(f, "DATA_EXPORTED"),
            Self::AuditLogAccessed => write!(f, "AUDIT_LOG_ACCESSED"),
        }
    }
}

/// Audit event severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AuditSeverity {
    /// Informational events
    Info,
    /// Warning events
    Warning,
    /// Error events
    Error,
    /// Critical security events
    Critical,
}

impl std::fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Audit event entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event ID
    pub id: String,

    /// Event type
    pub event_type: AuditEventType,

    /// Event severity
    pub severity: AuditSeverity,

    /// Timestamp (Unix timestamp in milliseconds)
    pub timestamp: u64,

    /// User ID (optional)
    pub user_id: Option<String>,

    /// API key used (optional)
    pub api_key: Option<String>,

    /// IP address (optional)
    pub ip_address: Option<String>,

    /// Request ID for correlation (optional)
    pub request_id: Option<String>,

    /// Resource affected (e.g., image path, key name)
    pub resource: Option<String>,

    /// Action performed
    pub action: String,

    /// Result of the action (success/failure)
    pub result: AuditResult,

    /// Additional details
    pub details: std::collections::HashMap<String, String>,

    /// Error message if applicable
    pub error: Option<String>,
}

/// Result of an audited action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditResult {
    /// Action succeeded
    Success,
    /// Action failed
    Failure,
    /// Action was denied
    Denied,
}

impl std::fmt::Display for AuditResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "SUCCESS"),
            Self::Failure => write!(f, "FAILURE"),
            Self::Denied => write!(f, "DENIED"),
        }
    }
}

impl AuditEvent {
    /// Create a new audit event
    pub fn new(event_type: AuditEventType, action: impl Into<String>) -> Self {
        let severity = match event_type {
            AuditEventType::ProcessingStarted
            | AuditEventType::ProcessingCompleted
            | AuditEventType::DataExported
            | AuditEventType::AuditLogAccessed => AuditSeverity::Info,

            AuditEventType::ConfigurationChanged
            | AuditEventType::ApiKeyCreated
            | AuditEventType::ApiKeyRevoked => AuditSeverity::Warning,

            AuditEventType::ProcessingFailed | AuditEventType::ApiKeyDeleted => {
                AuditSeverity::Error
            }

            AuditEventType::AccessDenied
            | AuditEventType::RateLimitExceeded
            | AuditEventType::QuotaExceeded => AuditSeverity::Critical,
        };

        Self {
            id: generate_event_id(),
            event_type,
            severity,
            timestamp: current_timestamp_ms(),
            user_id: None,
            api_key: None,
            ip_address: None,
            request_id: None,
            resource: None,
            action: action.into(),
            result: AuditResult::Success,
            details: std::collections::HashMap::new(),
            error: None,
        }
    }

    /// Set user ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set API key
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set IP address
    pub fn with_ip_address(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    /// Set request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Set resource
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// Set result
    pub fn with_result(mut self, result: AuditResult) -> Self {
        self.result = result;
        self
    }

    /// Add detail
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    /// Set error
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| "{\"error\":\"failed to serialize audit event\"}".to_string())
    }

    /// Convert to CSV row
    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{}",
            self.id,
            self.timestamp,
            self.event_type,
            self.severity,
            self.user_id.as_deref().unwrap_or(""),
            self.api_key.as_deref().unwrap_or(""),
            self.ip_address.as_deref().unwrap_or(""),
            self.resource.as_deref().unwrap_or(""),
            self.action,
            self.result,
            self.error.as_deref().unwrap_or("")
        )
    }
}

/// Generate a unique event ID
fn generate_event_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let mut hasher = RandomState::new().build_hasher();
    let timestamp = current_timestamp_ms();
    hasher.write_u64(timestamp);

    let hash = hasher.finish();
    format!("audit_{:016x}", hash)
}

/// Get current timestamp in milliseconds
fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Data retention policy
#[derive(Debug, Clone, Copy)]
pub struct RetentionPolicy {
    /// Maximum number of events to retain
    pub max_events: usize,

    /// Maximum age of events in seconds (None = infinite)
    pub max_age_seconds: Option<u64>,

    /// Whether to auto-export before deletion
    pub auto_export: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_events: 100_000,
            max_age_seconds: Some(90 * 24 * 3600), // 90 days
            auto_export: false,
        }
    }
}

impl RetentionPolicy {
    /// Create a policy for compliance (7 years)
    pub fn compliance() -> Self {
        Self {
            max_events: 10_000_000,
            max_age_seconds: Some(7 * 365 * 24 * 3600), // 7 years
            auto_export: true,
        }
    }

    /// Create a policy for short-term retention (30 days)
    pub fn short_term() -> Self {
        Self {
            max_events: 10_000,
            max_age_seconds: Some(30 * 24 * 3600), // 30 days
            auto_export: false,
        }
    }

    /// Create an unlimited policy
    pub fn unlimited() -> Self {
        Self {
            max_events: usize::MAX,
            max_age_seconds: None,
            auto_export: false,
        }
    }
}

/// Audit logger
pub struct AuditLogger {
    events: Arc<RwLock<VecDeque<AuditEvent>>>,
    policy: RetentionPolicy,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(policy: RetentionPolicy) -> Self {
        Self {
            events: Arc::new(RwLock::new(VecDeque::new())),
            policy,
        }
    }

    /// Log an audit event
    pub fn log(&self, event: AuditEvent) {
        let mut events = self.events.write().unwrap_or_else(|e| e.into_inner());

        // Apply retention policy
        self.apply_retention_policy(&mut events);

        // Add the new event
        events.push_back(event);
    }

    /// Apply retention policy
    fn apply_retention_policy(&self, events: &mut VecDeque<AuditEvent>) {
        let now = current_timestamp_ms();

        // Remove events exceeding max count
        while events.len() >= self.policy.max_events {
            events.pop_front();
        }

        // Remove events exceeding max age
        if let Some(max_age_ms) = self.policy.max_age_seconds.map(|s| s * 1000) {
            while let Some(event) = events.front() {
                if now - event.timestamp > max_age_ms {
                    events.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    /// Get all events
    pub fn get_events(&self) -> Vec<AuditEvent> {
        self.events.read().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
    }

    /// Get events by type
    pub fn get_events_by_type(&self, event_type: AuditEventType) -> Vec<AuditEvent> {
        self.events
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|e| e.event_type == event_type)
            .cloned()
            .collect()
    }

    /// Get events by user
    pub fn get_events_by_user(&self, user_id: &str) -> Vec<AuditEvent> {
        self.events
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|e| e.user_id.as_deref() == Some(user_id))
            .cloned()
            .collect()
    }

    /// Get events by severity
    pub fn get_events_by_severity(&self, min_severity: AuditSeverity) -> Vec<AuditEvent> {
        self.events
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|e| e.severity >= min_severity)
            .cloned()
            .collect()
    }

    /// Get events in time range
    pub fn get_events_in_range(&self, start_ms: u64, end_ms: u64) -> Vec<AuditEvent> {
        self.events
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|e| e.timestamp >= start_ms && e.timestamp <= end_ms)
            .cloned()
            .collect()
    }

    /// Export events to JSON
    pub fn export_json(&self) -> String {
        let events = self.get_events();
        serde_json::to_string_pretty(&events).unwrap_or_else(|_| "[]".to_string())
    }

    /// Export events to CSV
    pub fn export_csv(&self) -> String {
        let mut csv = String::from("id,timestamp,event_type,severity,user_id,api_key,ip_address,resource,action,result,error\n");

        for event in self.get_events() {
            csv.push_str(&event.to_csv_row());
            csv.push('\n');
        }

        csv
    }

    /// Get event count
    pub fn count(&self) -> usize {
        self.events.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Clear all events
    pub fn clear(&self) {
        self.events.write().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Get statistics
    pub fn stats(&self) -> AuditStats {
        let events = self.events.read().unwrap_or_else(|e| e.into_inner());

        let mut stats = AuditStats {
            total_events: events.len(),
            ..Default::default()
        };

        for event in events.iter() {
            match event.event_type {
                AuditEventType::ProcessingStarted => stats.processing_events += 1,
                AuditEventType::ApiKeyCreated | AuditEventType::ApiKeyRevoked => {
                    stats.key_management_events += 1
                }
                AuditEventType::AccessDenied
                | AuditEventType::RateLimitExceeded
                | AuditEventType::QuotaExceeded => stats.security_events += 1,
                _ => {}
            }

            match event.severity {
                AuditSeverity::Info => stats.info_events += 1,
                AuditSeverity::Warning => stats.warning_events += 1,
                AuditSeverity::Error => stats.error_events += 1,
                AuditSeverity::Critical => stats.critical_events += 1,
            }
        }

        stats
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new(RetentionPolicy::default())
    }
}

/// Audit statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditStats {
    pub total_events: usize,
    pub processing_events: usize,
    pub key_management_events: usize,
    pub security_events: usize,
    pub info_events: usize,
    pub warning_events: usize,
    pub error_events: usize,
    pub critical_events: usize,
}

impl std::fmt::Display for AuditStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Audit Statistics:")?;
        writeln!(f, "  Total Events:        {}", self.total_events)?;
        writeln!(f, "  Processing Events:   {}", self.processing_events)?;
        writeln!(f, "  Key Management:      {}", self.key_management_events)?;
        writeln!(f, "  Security Events:     {}", self.security_events)?;
        writeln!(f, "  Info:                {}", self.info_events)?;
        writeln!(f, "  Warning:             {}", self.warning_events)?;
        writeln!(f, "  Error:               {}", self.error_events)?;
        writeln!(f, "  Critical:            {}", self.critical_events)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_type_display() {
        assert_eq!(
            format!("{}", AuditEventType::ProcessingStarted),
            "PROCESSING_STARTED"
        );
        assert_eq!(format!("{}", AuditEventType::AccessDenied), "ACCESS_DENIED");
    }

    #[test]
    fn test_audit_severity_ordering() {
        assert!(AuditSeverity::Critical > AuditSeverity::Error);
        assert!(AuditSeverity::Error > AuditSeverity::Warning);
        assert!(AuditSeverity::Warning > AuditSeverity::Info);
    }

    #[test]
    fn test_audit_event_creation() {
        let event = AuditEvent::new(AuditEventType::ProcessingStarted, "OCR processing");
        assert_eq!(event.event_type, AuditEventType::ProcessingStarted);
        assert_eq!(event.severity, AuditSeverity::Info);
        assert_eq!(event.action, "OCR processing");
    }

    #[test]
    fn test_audit_event_with_fields() {
        let event = AuditEvent::new(AuditEventType::ProcessingCompleted, "Complete")
            .with_user_id("user123")
            .with_api_key("key456")
            .with_resource("image.png")
            .with_detail("duration_ms", "1500");

        assert_eq!(event.user_id, Some("user123".to_string()));
        assert_eq!(event.api_key, Some("key456".to_string()));
        assert_eq!(event.resource, Some("image.png".to_string()));
        assert_eq!(event.details.get("duration_ms"), Some(&"1500".to_string()));
    }

    #[test]
    fn test_audit_event_to_json() {
        let event = AuditEvent::new(AuditEventType::ApiKeyCreated, "Create key");
        let json = event.to_json();
        assert!(json.contains("\"event_type\":\"ApiKeyCreated\""));
    }

    #[test]
    fn test_audit_result_display() {
        assert_eq!(format!("{}", AuditResult::Success), "SUCCESS");
        assert_eq!(format!("{}", AuditResult::Failure), "FAILURE");
        assert_eq!(format!("{}", AuditResult::Denied), "DENIED");
    }

    #[test]
    fn test_retention_policy_default() {
        let policy = RetentionPolicy::default();
        assert_eq!(policy.max_events, 100_000);
        assert!(policy.max_age_seconds.is_some());
    }

    #[test]
    fn test_retention_policy_presets() {
        let compliance = RetentionPolicy::compliance();
        assert!(compliance.auto_export);
        assert!(compliance.max_age_seconds.unwrap() > 365 * 24 * 3600);

        let short = RetentionPolicy::short_term();
        assert!(!short.auto_export);

        let unlimited = RetentionPolicy::unlimited();
        assert_eq!(unlimited.max_events, usize::MAX);
        assert!(unlimited.max_age_seconds.is_none());
    }

    #[test]
    fn test_audit_logger_creation() {
        let logger = AuditLogger::new(RetentionPolicy::default());
        assert_eq!(logger.count(), 0);
    }

    #[test]
    fn test_audit_logger_log_event() {
        let logger = AuditLogger::default();
        let event = AuditEvent::new(AuditEventType::ProcessingStarted, "Start");

        logger.log(event);
        assert_eq!(logger.count(), 1);
    }

    #[test]
    fn test_audit_logger_get_events() {
        let logger = AuditLogger::default();
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "1"));
        logger.log(AuditEvent::new(AuditEventType::ProcessingCompleted, "2"));

        let events = logger.get_events();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_audit_logger_get_events_by_type() {
        let logger = AuditLogger::default();
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "1"));
        logger.log(AuditEvent::new(AuditEventType::ProcessingCompleted, "2"));
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "3"));

        let events = logger.get_events_by_type(AuditEventType::ProcessingStarted);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_audit_logger_get_events_by_user() {
        let logger = AuditLogger::default();
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "1").with_user_id("user1"));
        logger.log(AuditEvent::new(AuditEventType::ProcessingCompleted, "2").with_user_id("user2"));
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "3").with_user_id("user1"));

        let events = logger.get_events_by_user("user1");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_audit_logger_get_events_by_severity() {
        let logger = AuditLogger::default();
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "1")); // Info
        logger.log(AuditEvent::new(AuditEventType::AccessDenied, "2")); // Critical

        let events = logger.get_events_by_severity(AuditSeverity::Critical);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_audit_logger_export_json() {
        let logger = AuditLogger::default();
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "Test"));

        let json = logger.export_json();
        assert!(json.contains("ProcessingStarted"));
    }

    #[test]
    fn test_audit_logger_export_csv() {
        let logger = AuditLogger::default();
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "Test"));

        let csv = logger.export_csv();
        assert!(csv.contains("id,timestamp"));
        assert!(csv.contains("PROCESSING_STARTED"));
    }

    #[test]
    fn test_audit_logger_clear() {
        let logger = AuditLogger::default();
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "Test"));
        assert_eq!(logger.count(), 1);

        logger.clear();
        assert_eq!(logger.count(), 0);
    }

    #[test]
    fn test_audit_logger_stats() {
        let logger = AuditLogger::default();
        logger.log(AuditEvent::new(AuditEventType::ProcessingStarted, "1"));
        logger.log(AuditEvent::new(AuditEventType::ApiKeyCreated, "2"));
        logger.log(AuditEvent::new(AuditEventType::AccessDenied, "3"));

        let stats = logger.stats();
        assert_eq!(stats.total_events, 3);
        assert_eq!(stats.processing_events, 1);
        assert_eq!(stats.key_management_events, 1);
        assert_eq!(stats.security_events, 1);
    }

    #[test]
    fn test_generate_event_id_format() {
        let id = generate_event_id();
        assert!(id.starts_with("audit_"));
    }
}
