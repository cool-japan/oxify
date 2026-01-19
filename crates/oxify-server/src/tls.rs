//! TLS/HTTPS support for the HTTP server.
//!
//! This module provides TLS configuration and certificate management:
//! - TLS 1.2 and 1.3 support
//! - Certificate loading from files
//! - Certificate expiration monitoring
//! - ACME protocol support (future)

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// TLS certificate metadata
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    /// Certificate file path
    pub cert_path: PathBuf,
    /// Private key file path
    pub key_path: PathBuf,
    /// Certificate expiration time (if available)
    pub expires_at: Option<SystemTime>,
    /// Whether the certificate is valid
    pub is_valid: bool,
}

impl CertificateInfo {
    /// Create a new certificate info
    pub fn new(cert_path: PathBuf, key_path: PathBuf) -> Self {
        Self {
            cert_path,
            key_path,
            expires_at: None,
            is_valid: false,
        }
    }

    /// Check if the certificate is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            SystemTime::now() > expires_at
        } else {
            false
        }
    }

    /// Get time until expiration
    pub fn time_until_expiration(&self) -> Option<Duration> {
        self.expires_at
            .and_then(|expires_at| expires_at.duration_since(SystemTime::now()).ok())
    }

    /// Check if certificate will expire soon (within the given duration)
    pub fn expires_soon(&self, within: Duration) -> bool {
        if let Some(time_left) = self.time_until_expiration() {
            time_left < within
        } else {
            false
        }
    }
}

/// TLS configuration
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Enable TLS
    pub enabled: bool,
    /// Certificate file path
    pub cert_path: PathBuf,
    /// Private key file path
    pub key_path: PathBuf,
    /// Minimum TLS version (1.2 or 1.3)
    pub min_tls_version: TlsVersion,
    /// Enable client certificate authentication
    pub client_auth: bool,
    /// CA certificate path for client authentication
    pub ca_cert_path: Option<PathBuf>,
    /// Certificate renewal threshold (renew when this much time is left)
    pub renewal_threshold: Duration,
    /// Enable automatic certificate renewal (ACME)
    pub auto_renewal: bool,
}

/// TLS version enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    /// TLS 1.2
    Tls12,
    /// TLS 1.3
    Tls13,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_path: PathBuf::from("cert.pem"),
            key_path: PathBuf::from("key.pem"),
            min_tls_version: TlsVersion::Tls12,
            client_auth: false,
            ca_cert_path: None,
            renewal_threshold: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            auto_renewal: false,
        }
    }
}

impl TlsConfig {
    /// Create a new TLS configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a TLS configuration with TLS 1.3 only
    pub fn tls13_only() -> Self {
        Self {
            enabled: true,
            min_tls_version: TlsVersion::Tls13,
            ..Self::default()
        }
    }

    /// Create a production TLS configuration
    pub fn production(cert_path: PathBuf, key_path: PathBuf) -> Self {
        Self {
            enabled: true,
            cert_path,
            key_path,
            min_tls_version: TlsVersion::Tls12,
            client_auth: false,
            ca_cert_path: None,
            renewal_threshold: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            auto_renewal: false,
        }
    }

    /// Builder pattern: enable or disable TLS
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builder pattern: set certificate path
    pub fn with_cert_path(mut self, path: PathBuf) -> Self {
        self.cert_path = path;
        self
    }

    /// Builder pattern: set private key path
    pub fn with_key_path(mut self, path: PathBuf) -> Self {
        self.key_path = path;
        self
    }

    /// Builder pattern: set minimum TLS version
    pub fn with_min_tls_version(mut self, version: TlsVersion) -> Self {
        self.min_tls_version = version;
        self
    }

    /// Builder pattern: enable client authentication
    pub fn with_client_auth(mut self, ca_cert_path: PathBuf) -> Self {
        self.client_auth = true;
        self.ca_cert_path = Some(ca_cert_path);
        self
    }

    /// Builder pattern: set renewal threshold
    pub fn with_renewal_threshold(mut self, threshold: Duration) -> Self {
        self.renewal_threshold = threshold;
        self
    }

    /// Builder pattern: enable automatic renewal
    pub fn with_auto_renewal(mut self, enable: bool) -> Self {
        self.auto_renewal = enable;
        self
    }

    /// Validate the TLS configuration
    pub fn validate(&self) -> Result<(), TlsError> {
        if !self.enabled {
            return Ok(());
        }

        // Check if certificate file exists
        if !self.cert_path.exists() {
            return Err(TlsError::CertificateNotFound(
                self.cert_path.display().to_string(),
            ));
        }

        // Check if key file exists
        if !self.key_path.exists() {
            return Err(TlsError::KeyNotFound(self.key_path.display().to_string()));
        }

        // Check CA cert if client auth is enabled
        if self.client_auth {
            if let Some(ca_path) = &self.ca_cert_path {
                if !ca_path.exists() {
                    return Err(TlsError::CaCertNotFound(ca_path.display().to_string()));
                }
            } else {
                return Err(TlsError::MissingCaCert);
            }
        }

        Ok(())
    }
}

/// TLS error types
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    /// Certificate file not found
    #[error("Certificate file not found: {0}")]
    CertificateNotFound(String),

    /// Private key file not found
    #[error("Private key file not found: {0}")]
    KeyNotFound(String),

    /// CA certificate file not found
    #[error("CA certificate file not found: {0}")]
    CaCertNotFound(String),

    /// Missing CA certificate for client authentication
    #[error("Client authentication enabled but CA certificate path not provided")]
    MissingCaCert,

    /// Certificate parsing error
    #[error("Failed to parse certificate: {0}")]
    CertificateParseError(String),

    /// Private key parsing error
    #[error("Failed to parse private key: {0}")]
    KeyParseError(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Certificate monitor for tracking certificate expiration
#[derive(Debug)]
pub struct CertificateMonitor {
    /// Certificate information
    cert_info: Arc<CertificateInfo>,
    /// Whether monitoring is enabled
    enabled: AtomicBool,
    /// Renewal threshold
    renewal_threshold: Duration,
}

impl CertificateMonitor {
    /// Create a new certificate monitor
    pub fn new(cert_info: CertificateInfo, renewal_threshold: Duration) -> Self {
        Self {
            cert_info: Arc::new(cert_info),
            enabled: AtomicBool::new(true),
            renewal_threshold,
        }
    }

    /// Check if the certificate needs renewal
    pub fn needs_renewal(&self) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }

        self.cert_info.expires_soon(self.renewal_threshold)
    }

    /// Get certificate information
    pub fn cert_info(&self) -> &CertificateInfo {
        &self.cert_info
    }

    /// Enable or disable monitoring
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Check if monitoring is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

/// Load certificate from file
pub fn load_certificate(path: &Path) -> Result<Vec<u8>, TlsError> {
    fs::read(path).map_err(TlsError::IoError)
}

/// Load private key from file
pub fn load_private_key(path: &Path) -> Result<Vec<u8>, TlsError> {
    fs::read(path).map_err(TlsError::IoError)
}

/// Validate certificate and key pair
pub fn validate_cert_key_pair(cert_path: &Path, key_path: &Path) -> Result<(), TlsError> {
    // Load certificate
    let _cert = load_certificate(cert_path)?;

    // Load private key
    let _key = load_private_key(key_path)?;

    // In a real implementation, we would validate that the key matches the certificate
    // For now, we just check that both files can be read

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn test_tls_config_default() {
        let config = TlsConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.min_tls_version, TlsVersion::Tls12);
        assert!(!config.client_auth);
    }

    #[test]
    fn test_tls_config_tls13_only() {
        let config = TlsConfig::tls13_only();
        assert!(config.enabled);
        assert_eq!(config.min_tls_version, TlsVersion::Tls13);
    }

    #[test]
    fn test_tls_config_builder() {
        let config = TlsConfig::new()
            .with_enabled(true)
            .with_cert_path(PathBuf::from("/path/to/cert.pem"))
            .with_key_path(PathBuf::from("/path/to/key.pem"))
            .with_min_tls_version(TlsVersion::Tls13)
            .with_auto_renewal(true);

        assert!(config.enabled);
        assert_eq!(config.cert_path, PathBuf::from("/path/to/cert.pem"));
        assert_eq!(config.key_path, PathBuf::from("/path/to/key.pem"));
        assert_eq!(config.min_tls_version, TlsVersion::Tls13);
        assert!(config.auto_renewal);
    }

    #[test]
    fn test_certificate_info_expired() {
        let mut cert_info =
            CertificateInfo::new(PathBuf::from("cert.pem"), PathBuf::from("key.pem"));

        // Set expiration to past
        cert_info.expires_at = Some(UNIX_EPOCH);
        assert!(cert_info.is_expired());

        // Set expiration to future
        cert_info.expires_at = Some(SystemTime::now() + Duration::from_secs(3600));
        assert!(!cert_info.is_expired());
    }

    #[test]
    fn test_certificate_info_expires_soon() {
        let mut cert_info =
            CertificateInfo::new(PathBuf::from("cert.pem"), PathBuf::from("key.pem"));

        // Set expiration to 10 days from now
        cert_info.expires_at = Some(SystemTime::now() + Duration::from_secs(10 * 24 * 60 * 60));

        // Check if expires within 30 days
        assert!(cert_info.expires_soon(Duration::from_secs(30 * 24 * 60 * 60)));

        // Check if expires within 5 days
        assert!(!cert_info.expires_soon(Duration::from_secs(5 * 24 * 60 * 60)));
    }

    #[test]
    fn test_certificate_monitor() {
        let cert_info = CertificateInfo::new(PathBuf::from("cert.pem"), PathBuf::from("key.pem"));
        let monitor = CertificateMonitor::new(cert_info, Duration::from_secs(30 * 24 * 60 * 60));

        assert!(monitor.is_enabled());

        monitor.set_enabled(false);
        assert!(!monitor.is_enabled());
    }

    #[test]
    fn test_tls_version_equality() {
        assert_eq!(TlsVersion::Tls12, TlsVersion::Tls12);
        assert_eq!(TlsVersion::Tls13, TlsVersion::Tls13);
        assert_ne!(TlsVersion::Tls12, TlsVersion::Tls13);
    }

    #[test]
    fn test_tls_config_validation_disabled() {
        let config = TlsConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tls_error_display() {
        let err = TlsError::CertificateNotFound("cert.pem".to_string());
        assert_eq!(err.to_string(), "Certificate file not found: cert.pem");

        let err = TlsError::KeyNotFound("key.pem".to_string());
        assert_eq!(err.to_string(), "Private key file not found: key.pem");

        let err = TlsError::MissingCaCert;
        assert_eq!(
            err.to_string(),
            "Client authentication enabled but CA certificate path not provided"
        );
    }
}
