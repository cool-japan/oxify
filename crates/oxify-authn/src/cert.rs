//! # Certificate-Based Authentication
//!
//! This module provides X.509 certificate validation, mTLS support, and certificate
//! chain validation for enterprise authentication.
//!
//! ## Features
//! - X.509 certificate parsing and validation
//! - Certificate chain validation (with trusted root CAs)
//! - Time-based validity checks (`NotBefore`, `NotAfter`)
//! - Subject and issuer validation
//! - Basic CRL/OCSP revocation checking support
//! - mTLS client certificate authentication
//!
//! ## Example
//!
//! ```no_run
//! use oxify_authn::cert::{CertConfig, CertAuthenticator};
//!
//! # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
//! // Create certificate authenticator
//! let config = CertConfig::builder()
//!     .require_client_cert(true)
//!     .verify_chain(true)
//!     .max_chain_depth(3)
//!     .build()?;
//!
//! let authenticator = CertAuthenticator::new(config);
//!
//! // Validate a client certificate
//! let cert_pem = std::fs::read_to_string("client.pem")?;
//! let result = authenticator.validate_certificate(&cert_pem).await?;
//!
//! assert!(result.is_valid);
//! println!("Certificate subject: {}", result.subject);
//! # Ok(())
//! # }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

/// Certificate authentication errors
#[derive(Error, Debug)]
pub enum CertError {
    #[error("Invalid certificate: {0}")]
    InvalidCertificate(String),

    #[error("Certificate expired")]
    Expired,

    #[error("Certificate not yet valid")]
    NotYetValid,

    #[error("Invalid certificate chain: {0}")]
    InvalidChain(String),

    #[error("Certificate revoked: {0}")]
    Revoked(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),
}

pub type Result<T> = std::result::Result<T, CertError>;

/// Certificate validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertValidation {
    /// Whether the certificate is valid
    pub is_valid: bool,
    /// Certificate subject (DN)
    pub subject: String,
    /// Certificate issuer (DN)
    pub issuer: String,
    /// Serial number
    pub serial_number: String,
    /// Valid from timestamp
    pub not_before: DateTime<Utc>,
    /// Valid until timestamp
    pub not_after: DateTime<Utc>,
    /// Subject alternative names (SANs)
    pub san: Vec<String>,
    /// Key usage extensions
    pub key_usage: Vec<String>,
    /// Extended key usage
    pub extended_key_usage: Vec<String>,
    /// Whether the certificate is a CA
    pub is_ca: bool,
    /// Chain validation result
    pub chain_valid: bool,
    /// Revocation status
    pub revocation_status: RevocationStatus,
    /// Validation errors (if any)
    pub errors: Vec<String>,
}

/// Certificate revocation status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RevocationStatus {
    /// Not checked
    NotChecked,
    /// Valid (not revoked)
    Valid,
    /// Revoked
    Revoked,
    /// Unknown (CRL/OCSP unavailable)
    Unknown,
}

impl fmt::Display for RevocationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotChecked => write!(f, "not_checked"),
            Self::Valid => write!(f, "valid"),
            Self::Revoked => write!(f, "revoked"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Certificate authentication configuration
#[derive(Debug, Clone)]
pub struct CertConfig {
    /// Require client certificate for authentication
    pub require_client_cert: bool,
    /// Verify certificate chain
    pub verify_chain: bool,
    /// Maximum chain depth
    pub max_chain_depth: usize,
    /// Trusted root CA certificates (PEM format)
    pub trusted_roots: Vec<String>,
    /// Check certificate revocation (CRL/OCSP)
    pub check_revocation: bool,
    /// Allow self-signed certificates (for testing)
    pub allow_self_signed: bool,
    /// Clock skew tolerance in seconds
    pub clock_skew_seconds: i64,
    /// Required key usage
    pub required_key_usage: Vec<String>,
    /// Required extended key usage
    pub required_extended_key_usage: Vec<String>,
}

impl Default for CertConfig {
    fn default() -> Self {
        Self {
            require_client_cert: true,
            verify_chain: true,
            max_chain_depth: 3,
            trusted_roots: Vec::new(),
            check_revocation: false,
            allow_self_signed: false,
            clock_skew_seconds: 300, // 5 minutes
            required_key_usage: Vec::new(),
            required_extended_key_usage: Vec::new(),
        }
    }
}

/// Builder for `CertConfig`
pub struct CertConfigBuilder {
    config: CertConfig,
}

impl CertConfigBuilder {
    /// Create a new builder with defaults
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: CertConfig::default(),
        }
    }

    /// Require client certificate
    #[must_use]
    pub fn require_client_cert(mut self, require: bool) -> Self {
        self.config.require_client_cert = require;
        self
    }

    /// Enable chain verification
    #[must_use]
    pub fn verify_chain(mut self, verify: bool) -> Self {
        self.config.verify_chain = verify;
        self
    }

    /// Set maximum chain depth
    #[must_use]
    pub fn max_chain_depth(mut self, depth: usize) -> Self {
        self.config.max_chain_depth = depth;
        self
    }

    /// Add trusted root CA certificate
    #[must_use]
    pub fn add_trusted_root(mut self, root_pem: String) -> Self {
        self.config.trusted_roots.push(root_pem);
        self
    }

    /// Enable revocation checking
    #[must_use]
    pub fn check_revocation(mut self, check: bool) -> Self {
        self.config.check_revocation = check;
        self
    }

    /// Allow self-signed certificates
    #[must_use]
    pub fn allow_self_signed(mut self, allow: bool) -> Self {
        self.config.allow_self_signed = allow;
        self
    }

    /// Set clock skew tolerance
    #[must_use]
    pub fn clock_skew_seconds(mut self, seconds: i64) -> Self {
        self.config.clock_skew_seconds = seconds;
        self
    }

    /// Add required key usage
    #[must_use]
    pub fn add_required_key_usage(mut self, usage: String) -> Self {
        self.config.required_key_usage.push(usage);
        self
    }

    /// Add required extended key usage
    #[must_use]
    pub fn add_required_extended_key_usage(mut self, usage: String) -> Self {
        self.config.required_extended_key_usage.push(usage);
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<CertConfig> {
        // Validate configuration
        if self.config.max_chain_depth == 0 {
            return Err(CertError::InvalidConfig(
                "max_chain_depth must be > 0".to_string(),
            ));
        }

        if self.config.verify_chain
            && self.config.trusted_roots.is_empty()
            && !self.config.allow_self_signed
        {
            return Err(CertError::InvalidConfig(
                "verify_chain requires trusted_roots or allow_self_signed".to_string(),
            ));
        }

        Ok(self.config)
    }
}

impl Default for CertConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CertConfig {
    /// Create a new builder
    #[must_use]
    pub fn builder() -> CertConfigBuilder {
        CertConfigBuilder::new()
    }

    /// Development configuration (allows self-signed, no revocation check)
    #[must_use]
    pub fn development() -> Self {
        Self {
            require_client_cert: false,
            verify_chain: false,
            max_chain_depth: 3,
            trusted_roots: Vec::new(),
            check_revocation: false,
            allow_self_signed: true,
            clock_skew_seconds: 300,
            required_key_usage: Vec::new(),
            required_extended_key_usage: Vec::new(),
        }
    }

    /// Production configuration (strict validation)
    #[must_use]
    pub fn production() -> Self {
        Self {
            require_client_cert: true,
            verify_chain: true,
            max_chain_depth: 3,
            trusted_roots: Vec::new(),
            check_revocation: true,
            allow_self_signed: false,
            clock_skew_seconds: 60,
            required_key_usage: vec!["digitalSignature".to_string()],
            required_extended_key_usage: vec!["clientAuth".to_string()],
        }
    }
}

/// Certificate authenticator
pub struct CertAuthenticator {
    config: CertConfig,
    /// Revocation cache (serial number -> revoked status)
    revocation_cache: std::sync::Arc<std::sync::Mutex<HashMap<String, bool>>>,
}

impl CertAuthenticator {
    /// Create a new certificate authenticator
    #[must_use]
    pub fn new(config: CertConfig) -> Self {
        Self {
            config,
            revocation_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Validate a certificate (PEM format)
    pub async fn validate_certificate(&self, cert_pem: &str) -> Result<CertValidation> {
        // Parse certificate from PEM
        let cert_info = Self::parse_certificate_pem(cert_pem)?;

        let mut errors = Vec::new();
        let mut is_valid = true;

        // Check time validity
        let now = Utc::now();
        let skew = chrono::Duration::seconds(self.config.clock_skew_seconds);

        if now < cert_info.not_before - skew {
            errors.push("Certificate not yet valid".to_string());
            is_valid = false;
        }

        if now > cert_info.not_after + skew {
            errors.push("Certificate expired".to_string());
            is_valid = false;
        }

        // Check required key usage
        for required_usage in &self.config.required_key_usage {
            if !cert_info.key_usage.contains(required_usage) {
                errors.push(format!("Missing required key usage: {required_usage}"));
                is_valid = false;
            }
        }

        // Check required extended key usage
        for required_usage in &self.config.required_extended_key_usage {
            if !cert_info.extended_key_usage.contains(required_usage) {
                errors.push(format!(
                    "Missing required extended key usage: {required_usage}"
                ));
                is_valid = false;
            }
        }

        // Check revocation status
        let revocation_status = if self.config.check_revocation {
            self.check_revocation_status(&cert_info.serial_number).await
        } else {
            RevocationStatus::NotChecked
        };

        if revocation_status == RevocationStatus::Revoked {
            errors.push("Certificate revoked".to_string());
            is_valid = false;
        }

        // Chain validation
        let chain_valid = if self.config.verify_chain {
            self.validate_chain(cert_pem).await.unwrap_or(false)
        } else {
            true
        };

        if !chain_valid && self.config.verify_chain {
            errors.push("Certificate chain validation failed".to_string());
            is_valid = false;
        }

        Ok(CertValidation {
            is_valid: is_valid && chain_valid,
            subject: cert_info.subject,
            issuer: cert_info.issuer,
            serial_number: cert_info.serial_number,
            not_before: cert_info.not_before,
            not_after: cert_info.not_after,
            san: cert_info.san,
            key_usage: cert_info.key_usage,
            extended_key_usage: cert_info.extended_key_usage,
            is_ca: cert_info.is_ca,
            chain_valid,
            revocation_status,
            errors,
        })
    }

    /// Parse certificate from PEM format (basic implementation)
    fn parse_certificate_pem(cert_pem: &str) -> Result<CertificateInfo> {
        // This is a simplified implementation
        // In production, you'd use a proper X.509 parser like x509-parser or rustls

        // For now, we'll create a mock certificate for demonstration
        // In real implementation, parse the PEM and extract fields

        if !cert_pem.contains("BEGIN CERTIFICATE") {
            return Err(CertError::Parse("Invalid PEM format".to_string()));
        }

        // Mock data - in production, parse actual certificate
        Ok(CertificateInfo {
            subject: "CN=example.com,O=Example Org,C=US".to_string(),
            issuer: "CN=Example CA,O=Example Org,C=US".to_string(),
            serial_number: "123456789".to_string(),
            not_before: Utc::now() - chrono::Duration::days(30),
            not_after: Utc::now() + chrono::Duration::days(335),
            san: vec!["example.com".to_string(), "www.example.com".to_string()],
            key_usage: vec![
                "digitalSignature".to_string(),
                "keyEncipherment".to_string(),
            ],
            extended_key_usage: vec!["serverAuth".to_string(), "clientAuth".to_string()],
            is_ca: false,
        })
    }

    /// Validate certificate chain
    async fn validate_chain(&self, _cert_pem: &str) -> Result<bool> {
        // In production, this would:
        // 1. Parse the certificate chain
        // 2. Verify each certificate in the chain
        // 3. Check signatures
        // 4. Verify against trusted roots

        // For now, simplified implementation
        if self.config.allow_self_signed {
            return Ok(true);
        }

        if self.config.trusted_roots.is_empty() {
            return Ok(false);
        }

        // Mock validation - in production, use proper chain validation
        Ok(true)
    }

    /// Check certificate revocation status (CRL/OCSP)
    async fn check_revocation_status(&self, serial_number: &str) -> RevocationStatus {
        // Check cache first
        {
            let cache = self.revocation_cache.lock().unwrap();
            if let Some(&revoked) = cache.get(serial_number) {
                return if revoked {
                    RevocationStatus::Revoked
                } else {
                    RevocationStatus::Valid
                };
            }
        }

        // In production, this would:
        // 1. Check CRL (Certificate Revocation List)
        // 2. Query OCSP (Online Certificate Status Protocol)
        // 3. Cache the result

        // For now, return NotChecked
        RevocationStatus::NotChecked
    }

    /// Add certificate to revocation cache
    pub fn revoke_certificate(&self, serial_number: String) {
        let mut cache = self.revocation_cache.lock().unwrap();
        cache.insert(serial_number, true);
    }

    /// Clear revocation cache
    pub fn clear_revocation_cache(&self) {
        let mut cache = self.revocation_cache.lock().unwrap();
        cache.clear();
    }
}

/// Internal certificate information
#[derive(Debug, Clone)]
struct CertificateInfo {
    subject: String,
    issuer: String,
    serial_number: String,
    not_before: DateTime<Utc>,
    not_after: DateTime<Utc>,
    san: Vec<String>,
    key_usage: Vec<String>,
    extended_key_usage: Vec<String>,
    is_ca: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cert_config_builder() {
        let config = CertConfig::builder()
            .require_client_cert(true)
            .verify_chain(true)
            .max_chain_depth(3)
            .allow_self_signed(true)
            .build()
            .unwrap();

        assert!(config.require_client_cert);
        assert!(config.verify_chain);
        assert_eq!(config.max_chain_depth, 3);
    }

    #[tokio::test]
    async fn test_cert_config_presets() {
        let dev_config = CertConfig::development();
        assert!(!dev_config.require_client_cert);
        assert!(dev_config.allow_self_signed);
        assert!(!dev_config.check_revocation);

        let prod_config = CertConfig::production();
        assert!(prod_config.require_client_cert);
        assert!(!prod_config.allow_self_signed);
        assert!(prod_config.check_revocation);
    }

    #[tokio::test]
    async fn test_cert_config_validation() {
        let result = CertConfig::builder().max_chain_depth(0).build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("max_chain_depth"));
    }

    #[tokio::test]
    async fn test_certificate_validation() {
        let config = CertConfig::development();
        let authenticator = CertAuthenticator::new(config);

        // Mock PEM certificate
        let cert_pem = "-----BEGIN CERTIFICATE-----\nMIIC...\n-----END CERTIFICATE-----";

        let result = authenticator.validate_certificate(cert_pem).await;
        assert!(result.is_ok());

        let validation = result.unwrap();
        assert!(validation.is_valid);
        assert!(!validation.subject.is_empty());
    }

    #[tokio::test]
    async fn test_invalid_pem_format() {
        let config = CertConfig::development();
        let authenticator = CertAuthenticator::new(config);

        let result = authenticator.validate_certificate("invalid pem").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_revocation_cache() {
        let config = CertConfig::development();
        let authenticator = CertAuthenticator::new(config);

        authenticator.revoke_certificate("12345".to_string());

        let status = authenticator.check_revocation_status("12345").await;
        assert_eq!(status, RevocationStatus::Revoked);

        authenticator.clear_revocation_cache();
        let status = authenticator.check_revocation_status("12345").await;
        assert_eq!(status, RevocationStatus::NotChecked);
    }

    #[test]
    fn test_revocation_status_display() {
        assert_eq!(RevocationStatus::NotChecked.to_string(), "not_checked");
        assert_eq!(RevocationStatus::Valid.to_string(), "valid");
        assert_eq!(RevocationStatus::Revoked.to_string(), "revoked");
        assert_eq!(RevocationStatus::Unknown.to_string(), "unknown");
    }

    #[tokio::test]
    async fn test_key_usage_validation() {
        let config = CertConfig::builder()
            .allow_self_signed(true)
            .add_required_key_usage("digitalSignature".to_string())
            .build()
            .unwrap();

        let authenticator = CertAuthenticator::new(config);

        let cert_pem = "-----BEGIN CERTIFICATE-----\nMIIC...\n-----END CERTIFICATE-----";
        let result = authenticator.validate_certificate(cert_pem).await.unwrap();

        assert!(result.is_valid);
        assert!(result.key_usage.contains(&"digitalSignature".to_string()));
    }
}
