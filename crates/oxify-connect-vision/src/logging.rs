//! Enhanced logging module for structured and secure logging.
//!
//! This module provides:
//! - Structured logging with contextual metadata
//! - Log sampling for high-volume operations
//! - Sensitive data redaction
//! - Log level filtering
//! - Performance metrics logging

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

/// Log level enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    /// Trace level (most verbose)
    Trace,
    /// Debug level
    Debug,
    /// Info level
    Info,
    /// Warning level
    Warn,
    /// Error level
    Error,
}

impl LogLevel {
    /// Check if this level should be logged given a minimum level
    pub fn should_log(&self, min_level: LogLevel) -> bool {
        *self >= min_level
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// Structured log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp (Unix timestamp in milliseconds)
    pub timestamp: u64,

    /// Log level
    pub level: LogLevel,

    /// Log message
    pub message: String,

    /// Structured fields
    pub fields: HashMap<String, String>,

    /// Optional request ID for correlation
    pub request_id: Option<String>,

    /// Optional user ID
    pub user_id: Option<String>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: current_timestamp_ms(),
            level,
            message: message.into(),
            fields: HashMap::new(),
            request_id: None,
            user_id: None,
        }
    }

    /// Add a field to the log entry
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Set request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Set user ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| "{\"error\":\"failed to serialize log entry\"}".to_string())
    }
}

/// Get current timestamp in milliseconds
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Log sampling configuration
#[derive(Debug, Clone)]
pub struct SamplingConfig {
    /// Sample rate (0.0 to 1.0)
    pub rate: f64,

    /// Always sample errors
    pub always_sample_errors: bool,

    /// Maximum samples per second
    pub max_samples_per_second: Option<u64>,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            rate: 1.0, // Sample everything by default
            always_sample_errors: true,
            max_samples_per_second: None,
        }
    }
}

impl SamplingConfig {
    /// Create a sampling config with a specific rate
    pub fn with_rate(rate: f64) -> Self {
        Self {
            rate: rate.clamp(0.0, 1.0),
            always_sample_errors: true,
            max_samples_per_second: None,
        }
    }

    /// Sample 1% of logs
    pub fn low() -> Self {
        Self::with_rate(0.01)
    }

    /// Sample 10% of logs
    pub fn medium() -> Self {
        Self::with_rate(0.10)
    }

    /// Sample 50% of logs
    pub fn high() -> Self {
        Self::with_rate(0.50)
    }

    /// Sample all logs
    pub fn all() -> Self {
        Self::with_rate(1.0)
    }
}

/// Log sampler for rate-limiting logs
pub struct LogSampler {
    config: SamplingConfig,
    sample_count: Arc<AtomicU64>,
    last_reset: Arc<AtomicU64>,
}

impl LogSampler {
    /// Create a new log sampler
    pub fn new(config: SamplingConfig) -> Self {
        Self {
            config,
            sample_count: Arc::new(AtomicU64::new(0)),
            last_reset: Arc::new(AtomicU64::new(current_timestamp_ms())),
        }
    }

    /// Check if a log should be sampled
    pub fn should_sample(&self, level: LogLevel) -> bool {
        // Always sample errors if configured
        if self.config.always_sample_errors && level >= LogLevel::Error {
            return true;
        }

        // Check rate-based sampling
        if self.config.rate >= 1.0 {
            return true;
        }

        if self.config.rate <= 0.0 {
            return false;
        }

        // Check max samples per second
        if let Some(max_samples) = self.config.max_samples_per_second {
            let now = current_timestamp_ms();
            let last = self.last_reset.load(Ordering::Relaxed);

            // Reset counter every second
            if now - last >= 1000 {
                self.sample_count.store(0, Ordering::Relaxed);
                self.last_reset.store(now, Ordering::Relaxed);
            }

            let count = self.sample_count.fetch_add(1, Ordering::Relaxed);
            if count >= max_samples {
                return false;
            }
        }

        // Random sampling based on rate
        use std::collections::hash_map::RandomState;
        use std::hash::BuildHasher;

        let hash = RandomState::new().hash_one(current_timestamp_ms());

        (hash as f64 / u64::MAX as f64) < self.config.rate
    }

    /// Get sample statistics
    pub fn stats(&self) -> SamplingStats {
        SamplingStats {
            sample_count: self.sample_count.load(Ordering::Relaxed),
            sample_rate: self.config.rate,
        }
    }
}

/// Sampling statistics
#[derive(Debug, Clone)]
pub struct SamplingStats {
    pub sample_count: u64,
    pub sample_rate: f64,
}

/// Patterns for sensitive data to redact
#[allow(dead_code)]
static SENSITIVE_PATTERNS: &[(&str, &str)] = &[
    // Credit card patterns
    (r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b", "[CARD]"),
    // Email addresses
    (
        r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b",
        "[EMAIL]",
    ),
    // Phone numbers
    (r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b", "[PHONE]"),
    // Social security numbers
    (r"\b\d{3}-\d{2}-\d{4}\b", "[SSN]"),
    // IP addresses
    (r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b", "[IP]"),
    // API keys (common patterns)
    (r"\b[A-Za-z0-9]{32,}\b", "[KEY]"),
];

/// Redact sensitive data from a string
pub fn redact_sensitive_data(input: &str) -> String {
    // Simple pattern matching (in production, use regex crate)
    // For now, we'll do basic replacements

    let words: Vec<&str> = input.split_whitespace().collect();
    let mut redacted_words = Vec::new();

    for word in words {
        if word.contains('@') && word.contains('.') {
            // Looks like an email
            redacted_words.push("[EMAIL]");
        } else if word.len() >= 32 && word.chars().all(|c| c.is_alphanumeric()) {
            // Looks like an API key
            redacted_words.push("[KEY]");
        } else {
            redacted_words.push(word);
        }
    }

    redacted_words.join(" ")
}

/// Structured logger with enhanced features
pub struct StructuredLogger {
    min_level: LogLevel,
    sampler: Option<LogSampler>,
    redact_sensitive: bool,
    default_fields: HashMap<String, String>,
}

impl StructuredLogger {
    /// Create a new structured logger
    pub fn new(min_level: LogLevel) -> Self {
        Self {
            min_level,
            sampler: None,
            redact_sensitive: true,
            default_fields: HashMap::new(),
        }
    }

    /// Enable log sampling
    pub fn with_sampling(mut self, config: SamplingConfig) -> Self {
        self.sampler = Some(LogSampler::new(config));
        self
    }

    /// Enable/disable sensitive data redaction
    pub fn with_redaction(mut self, enabled: bool) -> Self {
        self.redact_sensitive = enabled;
        self
    }

    /// Add a default field to all log entries
    pub fn with_default_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_fields.insert(key.into(), value.into());
        self
    }

    /// Log a message
    pub fn log(&self, mut entry: LogEntry) {
        // Check log level
        if !entry.level.should_log(self.min_level) {
            return;
        }

        // Check sampling
        if let Some(sampler) = &self.sampler {
            if !sampler.should_sample(entry.level) {
                return;
            }
        }

        // Redact sensitive data
        if self.redact_sensitive {
            entry.message = redact_sensitive_data(&entry.message);
        }

        // Add default fields
        for (key, value) in &self.default_fields {
            entry
                .fields
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }

        // Log using tracing
        let json = entry.to_json();
        match entry.level {
            LogLevel::Trace => trace!("{}", json),
            LogLevel::Debug => debug!("{}", json),
            LogLevel::Info => info!("{}", json),
            LogLevel::Warn => warn!("{}", json),
            LogLevel::Error => error!("{}", json),
        }
    }

    /// Log at trace level
    pub fn trace(&self, message: impl Into<String>) {
        self.log(LogEntry::new(LogLevel::Trace, message));
    }

    /// Log at debug level
    pub fn debug(&self, message: impl Into<String>) {
        self.log(LogEntry::new(LogLevel::Debug, message));
    }

    /// Log at info level
    pub fn info(&self, message: impl Into<String>) {
        self.log(LogEntry::new(LogLevel::Info, message));
    }

    /// Log at warn level
    pub fn warn(&self, message: impl Into<String>) {
        self.log(LogEntry::new(LogLevel::Warn, message));
    }

    /// Log at error level
    pub fn error(&self, message: impl Into<String>) {
        self.log(LogEntry::new(LogLevel::Error, message));
    }
}

impl Default for StructuredLogger {
    fn default() -> Self {
        Self::new(LogLevel::Info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Warn > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
        assert!(LogLevel::Debug > LogLevel::Trace);
    }

    #[test]
    fn test_log_level_should_log() {
        assert!(LogLevel::Error.should_log(LogLevel::Info));
        assert!(LogLevel::Warn.should_log(LogLevel::Info));
        assert!(!LogLevel::Debug.should_log(LogLevel::Info));
        assert!(!LogLevel::Trace.should_log(LogLevel::Info));
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(LogLevel::Info, "test message");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "test message");
        assert!(entry.fields.is_empty());
    }

    #[test]
    fn test_log_entry_with_fields() {
        let entry = LogEntry::new(LogLevel::Info, "test")
            .with_field("key1", "value1")
            .with_field("key2", "value2");

        assert_eq!(entry.fields.get("key1"), Some(&"value1".to_string()));
        assert_eq!(entry.fields.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_log_entry_with_request_id() {
        let entry = LogEntry::new(LogLevel::Info, "test").with_request_id("req-123");

        assert_eq!(entry.request_id, Some("req-123".to_string()));
    }

    #[test]
    fn test_log_entry_to_json() {
        let entry = LogEntry::new(LogLevel::Info, "test").with_field("key", "value");

        let json = entry.to_json();
        assert!(json.contains("\"message\":\"test\""));
        assert!(json.contains("\"level\":\"Info\""));
    }

    #[test]
    fn test_sampling_config_default() {
        let config = SamplingConfig::default();
        assert_eq!(config.rate, 1.0);
        assert!(config.always_sample_errors);
    }

    #[test]
    fn test_sampling_config_presets() {
        assert_eq!(SamplingConfig::low().rate, 0.01);
        assert_eq!(SamplingConfig::medium().rate, 0.10);
        assert_eq!(SamplingConfig::high().rate, 0.50);
        assert_eq!(SamplingConfig::all().rate, 1.0);
    }

    #[test]
    fn test_log_sampler_always_sample_errors() {
        let config = SamplingConfig::with_rate(0.0); // Never sample
        let sampler = LogSampler::new(config);

        assert!(sampler.should_sample(LogLevel::Error));
    }

    #[test]
    fn test_log_sampler_rate_zero() {
        let mut config = SamplingConfig::with_rate(0.0);
        config.always_sample_errors = false;
        let sampler = LogSampler::new(config);

        assert!(!sampler.should_sample(LogLevel::Info));
    }

    #[test]
    fn test_log_sampler_rate_one() {
        let config = SamplingConfig::with_rate(1.0);
        let sampler = LogSampler::new(config);

        assert!(sampler.should_sample(LogLevel::Info));
        assert!(sampler.should_sample(LogLevel::Debug));
    }

    #[test]
    fn test_redact_sensitive_data_email() {
        let input = "Contact me at user@example.com for details";
        let redacted = redact_sensitive_data(input);
        assert!(redacted.contains("[EMAIL]"));
        assert!(!redacted.contains("user@example.com"));
    }

    #[test]
    fn test_redact_sensitive_data_api_key() {
        let input = "API key: abcdef1234567890abcdef1234567890abcdef12";
        let redacted = redact_sensitive_data(input);
        assert!(redacted.contains("[KEY]"));
    }

    #[test]
    fn test_redact_sensitive_data_no_sensitive() {
        let input = "This is a normal message";
        let redacted = redact_sensitive_data(input);
        assert_eq!(redacted, input);
    }

    #[test]
    fn test_structured_logger_creation() {
        let logger = StructuredLogger::new(LogLevel::Info);
        assert_eq!(logger.min_level, LogLevel::Info);
        assert!(logger.redact_sensitive);
    }

    #[test]
    fn test_structured_logger_with_sampling() {
        let logger = StructuredLogger::new(LogLevel::Info).with_sampling(SamplingConfig::low());
        assert!(logger.sampler.is_some());
    }

    #[test]
    fn test_structured_logger_with_redaction() {
        let logger = StructuredLogger::new(LogLevel::Info).with_redaction(false);
        assert!(!logger.redact_sensitive);
    }

    #[test]
    fn test_structured_logger_with_default_field() {
        let logger = StructuredLogger::new(LogLevel::Info).with_default_field("service", "ocr");
        assert_eq!(
            logger.default_fields.get("service"),
            Some(&"ocr".to_string())
        );
    }

    #[test]
    fn test_sampling_stats() {
        let sampler = LogSampler::new(SamplingConfig::high());
        let stats = sampler.stats();
        assert_eq!(stats.sample_rate, 0.50);
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(format!("{}", LogLevel::Trace), "TRACE");
        assert_eq!(format!("{}", LogLevel::Debug), "DEBUG");
        assert_eq!(format!("{}", LogLevel::Info), "INFO");
        assert_eq!(format!("{}", LogLevel::Warn), "WARN");
        assert_eq!(format!("{}", LogLevel::Error), "ERROR");
    }
}
