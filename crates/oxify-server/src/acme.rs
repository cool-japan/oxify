//! ACME protocol support for automatic certificate management.
//!
//! This module provides support for the ACME protocol (Automatic Certificate
//! Management Environment) used by Let's Encrypt and other certificate authorities.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// ACME configuration
#[derive(Debug, Clone)]
pub struct AcmeConfig {
    /// Enable ACME
    pub enabled: bool,
    /// ACME directory URL (e.g., Let's Encrypt production or staging)
    pub directory_url: String,
    /// Domain names to request certificates for
    pub domains: Vec<String>,
    /// Contact email for ACME account
    pub contact_email: String,
    /// Directory to store certificates and account keys
    pub cert_dir: PathBuf,
    /// Challenge type (http-01, dns-01, tls-alpn-01)
    pub challenge_type: ChallengeType,
    /// Automatic renewal threshold (renew when this much time is left)
    pub renewal_threshold: Duration,
    /// Enable test mode (use Let's Encrypt staging)
    pub test_mode: bool,
}

/// ACME challenge type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeType {
    /// HTTP-01 challenge (verification via HTTP)
    Http01,
    /// DNS-01 challenge (verification via DNS TXT record)
    Dns01,
    /// TLS-ALPN-01 challenge (verification via TLS)
    TlsAlpn01,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
            domains: Vec::new(),
            contact_email: String::new(),
            cert_dir: PathBuf::from("./certs"),
            challenge_type: ChallengeType::Http01,
            renewal_threshold: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            test_mode: false,
        }
    }
}

impl AcmeConfig {
    /// Create a new ACME configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a Let's Encrypt production configuration
    pub fn letsencrypt_production(domains: Vec<String>, email: String) -> Self {
        Self {
            enabled: true,
            directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
            domains,
            contact_email: email,
            test_mode: false,
            ..Self::default()
        }
    }

    /// Create a Let's Encrypt staging configuration (for testing)
    pub fn letsencrypt_staging(domains: Vec<String>, email: String) -> Self {
        Self {
            enabled: true,
            directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            domains,
            contact_email: email,
            test_mode: true,
            ..Self::default()
        }
    }

    /// Builder pattern: enable ACME
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builder pattern: set directory URL
    pub fn with_directory_url(mut self, url: String) -> Self {
        self.directory_url = url;
        self
    }

    /// Builder pattern: set domains
    pub fn with_domains(mut self, domains: Vec<String>) -> Self {
        self.domains = domains;
        self
    }

    /// Builder pattern: add a domain
    pub fn add_domain(mut self, domain: String) -> Self {
        self.domains.push(domain);
        self
    }

    /// Builder pattern: set contact email
    pub fn with_contact_email(mut self, email: String) -> Self {
        self.contact_email = email;
        self
    }

    /// Builder pattern: set certificate directory
    pub fn with_cert_dir(mut self, dir: PathBuf) -> Self {
        self.cert_dir = dir;
        self
    }

    /// Builder pattern: set challenge type
    pub fn with_challenge_type(mut self, challenge_type: ChallengeType) -> Self {
        self.challenge_type = challenge_type;
        self
    }

    /// Builder pattern: set renewal threshold
    pub fn with_renewal_threshold(mut self, threshold: Duration) -> Self {
        self.renewal_threshold = threshold;
        self
    }

    /// Builder pattern: set test mode
    pub fn with_test_mode(mut self, test_mode: bool) -> Self {
        self.test_mode = test_mode;
        self
    }

    /// Validate the ACME configuration
    pub fn validate(&self) -> Result<(), AcmeError> {
        if !self.enabled {
            return Ok(());
        }

        if self.domains.is_empty() {
            return Err(AcmeError::NoDomains);
        }

        if self.contact_email.is_empty() {
            return Err(AcmeError::NoContactEmail);
        }

        if !self.contact_email.contains('@') {
            return Err(AcmeError::InvalidEmail(self.contact_email.clone()));
        }

        Ok(())
    }
}

/// ACME error types
#[derive(Debug, thiserror::Error)]
pub enum AcmeError {
    /// No domains specified
    #[error("No domains specified for ACME certificate")]
    NoDomains,

    /// No contact email specified
    #[error("No contact email specified for ACME account")]
    NoContactEmail,

    /// Invalid email format
    #[error("Invalid email format: {0}")]
    InvalidEmail(String),

    /// Challenge failed
    #[error("ACME challenge failed: {0}")]
    ChallengeFailed(String),

    /// Certificate request failed
    #[error("Certificate request failed: {0}")]
    RequestFailed(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// ACME certificate manager
#[derive(Debug)]
pub struct AcmeManager {
    /// ACME configuration
    config: Arc<AcmeConfig>,
    /// Whether the manager is running
    running: AtomicBool,
    /// Last renewal attempt time
    last_renewal: Arc<std::sync::RwLock<Option<SystemTime>>>,
}

impl AcmeManager {
    /// Create a new ACME manager
    pub fn new(config: AcmeConfig) -> Self {
        Self {
            config: Arc::new(config),
            running: AtomicBool::new(false),
            last_renewal: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Start the ACME manager
    pub fn start(&self) -> Result<(), AcmeError> {
        self.config.validate()?;
        self.running.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Stop the ACME manager
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Check if the manager is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get the ACME configuration
    pub fn config(&self) -> &AcmeConfig {
        &self.config
    }

    /// Request a new certificate
    pub fn request_certificate(&self) -> Result<(), AcmeError> {
        if !self.is_running() {
            return Ok(());
        }

        // In a real implementation, this would:
        // 1. Connect to ACME directory
        // 2. Create or load account
        // 3. Request certificate for domains
        // 4. Complete challenges
        // 5. Download and save certificate

        // For now, just record the attempt
        let mut last_renewal = self.last_renewal.write().unwrap_or_else(|e| e.into_inner());
        *last_renewal = Some(SystemTime::now());

        Ok(())
    }

    /// Get last renewal attempt time
    pub fn last_renewal(&self) -> Option<SystemTime> {
        *self.last_renewal.read().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acme_config_default() {
        let config = AcmeConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.challenge_type, ChallengeType::Http01);
        assert!(config.domains.is_empty());
    }

    #[test]
    fn test_acme_config_letsencrypt_production() {
        let config = AcmeConfig::letsencrypt_production(
            vec!["example.com".to_string()],
            "admin@example.com".to_string(),
        );

        assert!(config.enabled);
        assert!(!config.test_mode);
        assert_eq!(config.domains, vec!["example.com"]);
        assert_eq!(config.contact_email, "admin@example.com");
    }

    #[test]
    fn test_acme_config_letsencrypt_staging() {
        let config = AcmeConfig::letsencrypt_staging(
            vec!["example.com".to_string()],
            "admin@example.com".to_string(),
        );

        assert!(config.enabled);
        assert!(config.test_mode);
        assert_eq!(config.domains, vec!["example.com"]);
    }

    #[test]
    fn test_acme_config_builder() {
        let config = AcmeConfig::new()
            .with_enabled(true)
            .add_domain("example.com".to_string())
            .add_domain("www.example.com".to_string())
            .with_contact_email("admin@example.com".to_string())
            .with_challenge_type(ChallengeType::Dns01)
            .with_test_mode(true);

        assert!(config.enabled);
        assert_eq!(config.domains.len(), 2);
        assert_eq!(config.challenge_type, ChallengeType::Dns01);
        assert!(config.test_mode);
    }

    #[test]
    fn test_acme_config_validation() {
        let config = AcmeConfig::default();
        assert!(config.validate().is_ok()); // Disabled config is valid

        let config = AcmeConfig::new().with_enabled(true);
        assert!(config.validate().is_err()); // No domains

        let config = AcmeConfig::new()
            .with_enabled(true)
            .add_domain("example.com".to_string());
        assert!(config.validate().is_err()); // No email

        let config = AcmeConfig::new()
            .with_enabled(true)
            .add_domain("example.com".to_string())
            .with_contact_email("invalid-email".to_string());
        assert!(config.validate().is_err()); // Invalid email

        let config = AcmeConfig::new()
            .with_enabled(true)
            .add_domain("example.com".to_string())
            .with_contact_email("admin@example.com".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_challenge_type_equality() {
        assert_eq!(ChallengeType::Http01, ChallengeType::Http01);
        assert_eq!(ChallengeType::Dns01, ChallengeType::Dns01);
        assert_ne!(ChallengeType::Http01, ChallengeType::Dns01);
    }

    #[test]
    fn test_acme_manager() {
        let config = AcmeConfig::letsencrypt_staging(
            vec!["example.com".to_string()],
            "admin@example.com".to_string(),
        );
        let manager = AcmeManager::new(config);

        assert!(!manager.is_running());

        manager.start().unwrap();
        assert!(manager.is_running());

        manager.stop();
        assert!(!manager.is_running());
    }

    #[test]
    fn test_acme_manager_request_certificate() {
        let config = AcmeConfig::letsencrypt_staging(
            vec!["example.com".to_string()],
            "admin@example.com".to_string(),
        );
        let manager = AcmeManager::new(config);

        manager.start().unwrap();
        assert!(manager.request_certificate().is_ok());
        assert!(manager.last_renewal().is_some());
    }

    #[test]
    fn test_acme_error_display() {
        let err = AcmeError::NoDomains;
        assert_eq!(err.to_string(), "No domains specified for ACME certificate");

        let err = AcmeError::NoContactEmail;
        assert_eq!(
            err.to_string(),
            "No contact email specified for ACME account"
        );

        let err = AcmeError::InvalidEmail("invalid".to_string());
        assert_eq!(err.to_string(), "Invalid email format: invalid");
    }
}
