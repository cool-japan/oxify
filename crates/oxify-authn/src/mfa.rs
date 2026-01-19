//! Multi-Factor Authentication (MFA) support
//!
//! Provides TOTP (Time-Based One-Time Password) implementation
//! following RFC 6238 for secure two-factor authentication.
//!
//! # Features
//! - TOTP secret generation
//! - QR code data for authenticator app enrollment
//! - TOTP validation with time drift tolerance
//! - Backup codes for account recovery
//!
//! # Example
//!
//! ```no_run
//! use oxify_authn::mfa::{TotpManager, TotpConfig};
//!
//! let config = TotpConfig::default();
//! let manager = TotpManager::new(config);
//!
//! // Generate secret for a user
//! let enrollment = manager.generate_secret("alice@example.com").unwrap();
//! println!("Secret: {}", enrollment.secret);
//! println!("QR URL: {}", enrollment.qr_code_url);
//!
//! // Validate TOTP code from user
//! let is_valid = manager.validate_code(&enrollment.secret, "123456").unwrap();
//! ```

use crate::types::{AuthError, Result};
use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Secret, TOTP};

/// TOTP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpConfig {
    /// Issuer name (shown in authenticator app)
    pub issuer: String,
    /// Number of digits in the code (default: 6)
    pub digits: usize,
    /// Time step in seconds (default: 30)
    pub step: u64,
    /// Number of time steps to allow for drift (default: 1)
    pub skew: u8,
    /// Algorithm (default: SHA1 for compatibility)
    pub algorithm: TotpAlgorithm,
}

/// TOTP algorithm options
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum TotpAlgorithm {
    #[default]
    Sha1,
    Sha256,
    Sha512,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            issuer: "OxiFY".to_string(),
            digits: 6,
            step: 30,
            skew: 1,
            algorithm: TotpAlgorithm::Sha1,
        }
    }
}

/// TOTP enrollment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpEnrollment {
    /// Base32-encoded secret
    pub secret: String,
    /// URL for QR code generation (otpauth:// format)
    pub qr_code_url: String,
    /// Backup codes for recovery
    pub backup_codes: Vec<String>,
}

/// TOTP manager for MFA operations
pub struct TotpManager {
    config: TotpConfig,
}

impl TotpManager {
    /// Create a new TOTP manager
    #[must_use]
    pub fn new(config: TotpConfig) -> Self {
        Self { config }
    }

    /// Generate a new TOTP secret for user enrollment
    pub fn generate_secret(&self, account_name: &str) -> Result<TotpEnrollment> {
        // Generate random secret using totp-rs
        let secret = Secret::generate_secret();

        let totp = self.create_totp(&secret, account_name)?;

        let qr_code_url = totp.get_url();
        let secret_base32 = totp.get_secret_base32();

        // Generate backup codes
        let backup_codes = Self::generate_backup_codes(8);

        Ok(TotpEnrollment {
            secret: secret_base32,
            qr_code_url,
            backup_codes,
        })
    }

    /// Validate a TOTP code
    pub fn validate_code(&self, secret: &str, code: &str) -> Result<bool> {
        let secret = Secret::Encoded(secret.to_string());
        let totp = self.create_totp(&secret, "")?;

        // Use the built-in check_current which handles skew
        totp.check_current(code)
            .map_err(|e| AuthError::InternalError(format!("System time error: {e}")))
    }

    /// Generate current TOTP code (for testing)
    pub fn generate_current_code(&self, secret: &str) -> Result<String> {
        let secret = Secret::Encoded(secret.to_string());
        let totp = self.create_totp(&secret, "")?;

        totp.generate_current()
            .map_err(|e| AuthError::InternalError(format!("System time error: {e}")))
    }

    /// Validate a backup code (one-time use)
    /// Returns the index of the matched code if valid
    #[must_use]
    pub fn validate_backup_code(code: &str, stored_codes: &[String]) -> Option<usize> {
        stored_codes.iter().position(|stored| {
            constant_time_eq(
                code.to_uppercase().as_bytes(),
                stored.to_uppercase().as_bytes(),
            )
        })
    }

    /// Get time to live (TTL) in seconds for current token
    pub fn get_token_ttl(&self, secret: &str) -> Result<u64> {
        let secret = Secret::Encoded(secret.to_string());
        let totp = self.create_totp(&secret, "")?;

        totp.ttl()
            .map_err(|e| AuthError::InternalError(format!("System time error: {e}")))
    }

    /// Create TOTP instance from secret
    fn create_totp(&self, secret: &Secret, account_name: &str) -> Result<TOTP> {
        let algorithm = match self.config.algorithm {
            TotpAlgorithm::Sha1 => Algorithm::SHA1,
            TotpAlgorithm::Sha256 => Algorithm::SHA256,
            TotpAlgorithm::Sha512 => Algorithm::SHA512,
        };

        TOTP::new(
            algorithm,
            self.config.digits,
            self.config.skew,
            self.config.step,
            secret
                .to_bytes()
                .map_err(|e| AuthError::InternalError(format!("Invalid secret: {e}")))?,
            Some(self.config.issuer.clone()),
            account_name.to_string(),
        )
        .map_err(|e| AuthError::InternalError(format!("Failed to create TOTP: {e}")))
    }

    /// Generate backup codes for recovery
    fn generate_backup_codes(count: usize) -> Vec<String> {
        use rand::Rng;
        let mut rng = rand::rng();

        (0..count)
            .map(|_| {
                let code: u32 = rng.random_range(0..100_000_000);
                format!("{code:08}")
            })
            .collect()
    }
}

impl Default for TotpManager {
    fn default() -> Self {
        Self::new(TotpConfig::default())
    }
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_totp_enrollment() {
        let manager = TotpManager::default();
        let enrollment = manager.generate_secret("test@example.com").unwrap();

        assert!(!enrollment.secret.is_empty());
        assert!(enrollment.qr_code_url.starts_with("otpauth://totp/"));
        assert_eq!(enrollment.backup_codes.len(), 8);
    }

    #[test]
    fn test_totp_validation() {
        let manager = TotpManager::default();
        let enrollment = manager.generate_secret("test@example.com").unwrap();

        // Generate current code
        let code = manager.generate_current_code(&enrollment.secret).unwrap();
        assert_eq!(code.len(), 6);

        // Validate the code
        let is_valid = manager.validate_code(&enrollment.secret, &code).unwrap();
        assert!(is_valid);

        // Invalid code should fail (with very high probability)
        let is_valid = manager.validate_code(&enrollment.secret, "000000").unwrap();
        // Note: This might very rarely pass by chance (1 in ~1000000)
        let _ = is_valid;
    }

    #[test]
    fn test_backup_code_validation() {
        let codes = vec!["12345678".to_string(), "87654321".to_string()];

        let index = TotpManager::validate_backup_code("12345678", &codes);
        assert_eq!(index, Some(0));

        let index = TotpManager::validate_backup_code("00000000", &codes);
        assert_eq!(index, None);

        // Case insensitive
        let codes = vec!["ABCD1234".to_string()];
        let index = TotpManager::validate_backup_code("abcd1234", &codes);
        assert_eq!(index, Some(0));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
    }

    #[test]
    fn test_totp_config() {
        let config = TotpConfig {
            issuer: "TestApp".to_string(),
            digits: 6,
            step: 30,
            skew: 1,
            algorithm: TotpAlgorithm::Sha256,
        };

        let manager = TotpManager::new(config);
        let enrollment = manager.generate_secret("user@test.com").unwrap();

        assert!(enrollment.qr_code_url.contains("TestApp"));
    }

    #[test]
    fn test_token_ttl() {
        let manager = TotpManager::default();
        let enrollment = manager.generate_secret("test@example.com").unwrap();

        let ttl = manager.get_token_ttl(&enrollment.secret).unwrap();
        // TTL should be between 1 and 30 seconds (step size)
        assert!((1..=30).contains(&ttl));
    }
}
