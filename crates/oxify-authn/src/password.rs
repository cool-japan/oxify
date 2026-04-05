//! Password hashing and validation with configurable policy
//!
//! Ported from `OxiRS` (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) `OxiRS` Contributors
//! Adapted for `OxiFY`
//! License: MIT OR Apache-2.0 (compatible with `OxiRS`)
//!
//! # Features
//! - Argon2 password hashing (memory-hard, GPU-resistant)
//! - Configurable password policy enforcement
//! - Password strength analysis
//! - Common password checking

use crate::types::{AuthError, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use serde::{Deserialize, Serialize};

/// Password policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordPolicy {
    /// Minimum password length
    pub min_length: usize,
    /// Maximum password length (0 = unlimited)
    pub max_length: usize,
    /// Require at least one uppercase letter
    pub require_uppercase: bool,
    /// Require at least one lowercase letter
    pub require_lowercase: bool,
    /// Require at least one digit
    pub require_digit: bool,
    /// Require at least one special character
    pub require_special: bool,
    /// Minimum number of character classes required (1-4)
    pub min_character_classes: usize,
    /// Disallow common passwords
    pub disallow_common: bool,
    /// Disallow passwords containing username
    pub disallow_username: bool,
    /// Minimum strength level required
    pub min_strength: PasswordStrength,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: 8,
            max_length: 128,
            require_uppercase: false,
            require_lowercase: false,
            require_digit: false,
            require_special: false,
            min_character_classes: 3,
            disallow_common: true,
            disallow_username: true,
            min_strength: PasswordStrength::Medium,
        }
    }
}

impl PasswordPolicy {
    /// Create a strict password policy
    #[must_use]
    pub fn strict() -> Self {
        Self {
            min_length: 12,
            max_length: 128,
            require_uppercase: true,
            require_lowercase: true,
            require_digit: true,
            require_special: true,
            min_character_classes: 4,
            disallow_common: true,
            disallow_username: true,
            min_strength: PasswordStrength::Strong,
        }
    }

    /// Create a relaxed password policy
    #[must_use]
    pub fn relaxed() -> Self {
        Self {
            min_length: 6,
            max_length: 128,
            require_uppercase: false,
            require_lowercase: false,
            require_digit: false,
            require_special: false,
            min_character_classes: 1,
            disallow_common: false,
            disallow_username: false,
            min_strength: PasswordStrength::VeryWeak,
        }
    }

    /// Create a NIST-compliant password policy
    /// Based on NIST SP 800-63B guidelines
    #[must_use]
    pub fn nist_compliant() -> Self {
        Self {
            min_length: 8,
            max_length: 64,
            require_uppercase: false,
            require_lowercase: false,
            require_digit: false,
            require_special: false,
            min_character_classes: 1,
            disallow_common: true,
            disallow_username: true,
            min_strength: PasswordStrength::Weak,
        }
    }
}

/// Result of password policy validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyValidationResult {
    /// Whether the password passes the policy
    pub is_valid: bool,
    /// List of policy violations
    pub violations: Vec<PolicyViolation>,
    /// Password strength
    pub strength: PasswordStrength,
    /// Suggestions for improvement
    pub suggestions: Vec<String>,
}

/// Password policy violation types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyViolation {
    /// Password is too short
    TooShort { min: usize, actual: usize },
    /// Password is too long
    TooLong { max: usize, actual: usize },
    /// Missing uppercase letter
    MissingUppercase,
    /// Missing lowercase letter
    MissingLowercase,
    /// Missing digit
    MissingDigit,
    /// Missing special character
    MissingSpecial,
    /// Not enough character classes
    InsufficientCharacterClasses { required: usize, actual: usize },
    /// Password is too common
    CommonPassword,
    /// Password contains username
    ContainsUsername,
    /// Password is too weak
    TooWeak {
        required: PasswordStrength,
        actual: PasswordStrength,
    },
}

/// Password manager for hashing and verifying passwords
pub struct PasswordManager {
    argon2: Argon2<'static>,
    policy: PasswordPolicy,
}

impl PasswordManager {
    /// Create a new password manager with default policy
    #[must_use]
    pub fn new() -> Self {
        Self {
            argon2: Argon2::default(),
            policy: PasswordPolicy::default(),
        }
    }

    /// Create a password manager with custom policy
    #[must_use]
    pub fn with_policy(policy: PasswordPolicy) -> Self {
        Self {
            argon2: Argon2::default(),
            policy,
        }
    }

    /// Get the current policy
    #[must_use]
    pub fn policy(&self) -> &PasswordPolicy {
        &self.policy
    }

    /// Hash a password using Argon2
    pub fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self
            .argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AuthError::InternalError(format!("Failed to hash password: {e}")))?;

        Ok(password_hash.to_string())
    }

    /// Verify a password against a hash
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AuthError::InternalError(format!("Invalid password hash: {e}")))?;

        match self
            .argon2
            .verify_password(password.as_bytes(), &parsed_hash)
        {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Validate password against policy
    pub fn validate_password(
        &self,
        password: &str,
        username: Option<&str>,
    ) -> PolicyValidationResult {
        let mut violations = Vec::new();
        let mut suggestions = Vec::new();

        let length = password.len();
        let has_uppercase = password.chars().any(char::is_uppercase);
        let has_lowercase = password.chars().any(char::is_lowercase);
        let has_digit = password.chars().any(char::is_numeric);
        let has_special = password.chars().any(|c| !c.is_alphanumeric());

        let character_classes = usize::from(has_uppercase)
            + usize::from(has_lowercase)
            + usize::from(has_digit)
            + usize::from(has_special);

        // Check minimum length
        if length < self.policy.min_length {
            violations.push(PolicyViolation::TooShort {
                min: self.policy.min_length,
                actual: length,
            });
            suggestions.push(format!(
                "Password must be at least {} characters long",
                self.policy.min_length
            ));
        }

        // Check maximum length
        if self.policy.max_length > 0 && length > self.policy.max_length {
            violations.push(PolicyViolation::TooLong {
                max: self.policy.max_length,
                actual: length,
            });
            suggestions.push(format!(
                "Password must be at most {} characters long",
                self.policy.max_length
            ));
        }

        // Check required character types
        if self.policy.require_uppercase && !has_uppercase {
            violations.push(PolicyViolation::MissingUppercase);
            suggestions.push("Add at least one uppercase letter".to_string());
        }

        if self.policy.require_lowercase && !has_lowercase {
            violations.push(PolicyViolation::MissingLowercase);
            suggestions.push("Add at least one lowercase letter".to_string());
        }

        if self.policy.require_digit && !has_digit {
            violations.push(PolicyViolation::MissingDigit);
            suggestions.push("Add at least one digit".to_string());
        }

        if self.policy.require_special && !has_special {
            violations.push(PolicyViolation::MissingSpecial);
            suggestions.push("Add at least one special character (!@#$%^&* etc.)".to_string());
        }

        // Check minimum character classes
        if character_classes < self.policy.min_character_classes {
            violations.push(PolicyViolation::InsufficientCharacterClasses {
                required: self.policy.min_character_classes,
                actual: character_classes,
            });
            suggestions.push(format!(
                "Use at least {} of: uppercase, lowercase, digits, special characters",
                self.policy.min_character_classes
            ));
        }

        // Check common passwords
        if self.policy.disallow_common && Self::is_common_password(password) {
            violations.push(PolicyViolation::CommonPassword);
            suggestions.push("Choose a less common password".to_string());
        }

        // Check username in password
        if self.policy.disallow_username {
            if let Some(user) = username {
                if !user.is_empty() && password.to_lowercase().contains(&user.to_lowercase()) {
                    violations.push(PolicyViolation::ContainsUsername);
                    suggestions.push("Password should not contain your username".to_string());
                }
            }
        }

        // Check strength
        let strength = self.check_password_strength(password);
        if strength < self.policy.min_strength {
            violations.push(PolicyViolation::TooWeak {
                required: self.policy.min_strength,
                actual: strength,
            });
            suggestions.push(format!(
                "Password strength must be at least {:?}",
                self.policy.min_strength
            ));
        }

        PolicyValidationResult {
            is_valid: violations.is_empty(),
            violations,
            strength,
            suggestions,
        }
    }

    /// Check password strength
    pub fn check_password_strength(&self, password: &str) -> PasswordStrength {
        let length = password.len();
        let has_uppercase = password.chars().any(char::is_uppercase);
        let has_lowercase = password.chars().any(char::is_lowercase);
        let has_digit = password.chars().any(char::is_numeric);
        let has_special = password.chars().any(|c| !c.is_alphanumeric());

        let score = u8::from(length >= 8)
            + u8::from(length >= 12)
            + u8::from(length >= 16)
            + u8::from(has_uppercase)
            + u8::from(has_lowercase)
            + u8::from(has_digit)
            + u8::from(has_special);

        match score {
            0..=2 => PasswordStrength::VeryWeak,
            3..=4 => PasswordStrength::Weak,
            5 => PasswordStrength::Medium,
            6 => PasswordStrength::Strong,
            _ => PasswordStrength::VeryStrong,
        }
    }

    /// Check if password is in common passwords list
    fn is_common_password(password: &str) -> bool {
        const COMMON_PASSWORDS: &[&str] = &[
            "password",
            "123456",
            "12345678",
            "qwerty",
            "abc123",
            "monkey",
            "1234567",
            "letmein",
            "trustno1",
            "dragon",
            "baseball",
            "iloveyou",
            "master",
            "sunshine",
            "ashley",
            "bailey",
            "shadow",
            "123123",
            "654321",
            "superman",
            "qazwsx",
            "michael",
            "football",
            "password1",
            "password123",
            "welcome",
            "jesus",
            "ninja",
            "mustang",
            "password!",
            "admin",
            "root",
            "toor",
            "pass",
            "test",
            "guest",
            "master",
            "changeme",
            "default",
            "hello",
        ];

        let lower = password.to_lowercase();
        COMMON_PASSWORDS.contains(&lower.as_str())
    }

    /// Generate a random password meeting policy requirements
    #[must_use]
    pub fn generate_password(&self) -> String {
        use rand::RngExt;
        let mut rng = rand::rng();

        let length = std::cmp::max(self.policy.min_length, 16);

        let uppercase: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let lowercase: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
        let digits: &[u8] = b"0123456789";
        let special: &[u8] = b"!@#$%^&*()_+-=[]{}|;:,.<>?";

        let mut password = Vec::with_capacity(length);

        // Ensure we have at least one of each required type
        if self.policy.require_uppercase || self.policy.min_character_classes >= 1 {
            password.push(uppercase[rng.random_range(0..uppercase.len())]);
        }
        if self.policy.require_lowercase || self.policy.min_character_classes >= 2 {
            password.push(lowercase[rng.random_range(0..lowercase.len())]);
        }
        if self.policy.require_digit || self.policy.min_character_classes >= 3 {
            password.push(digits[rng.random_range(0..digits.len())]);
        }
        if self.policy.require_special || self.policy.min_character_classes >= 4 {
            password.push(special[rng.random_range(0..special.len())]);
        }

        // Fill the rest with random characters from all sets
        let all_chars: Vec<u8> = [uppercase, lowercase, digits, special].concat();
        while password.len() < length {
            password.push(all_chars[rng.random_range(0..all_chars.len())]);
        }

        // Shuffle the password
        for i in (1..password.len()).rev() {
            let j = rng.random_range(0..=i);
            password.swap(i, j);
        }

        String::from_utf8(password).unwrap_or_else(|_| self.generate_password())
    }
}

impl Default for PasswordManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Password strength levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PasswordStrength {
    VeryWeak,
    Weak,
    Medium,
    Strong,
    VeryStrong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing() {
        let manager = PasswordManager::new();
        let password = "my-secure-password-123";

        let hash = manager.hash_password(password).unwrap();
        assert!(!hash.is_empty());

        // Verify correct password
        let is_valid = manager.verify_password(password, &hash).unwrap();
        assert!(is_valid);

        // Verify incorrect password
        let is_valid = manager.verify_password("wrong-password", &hash).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_password_strength() {
        let manager = PasswordManager::new();

        assert_eq!(
            manager.check_password_strength("123"),
            PasswordStrength::VeryWeak
        );
        assert_eq!(
            manager.check_password_strength("password"),
            PasswordStrength::VeryWeak
        ); // Only lowercase, length 8
        assert_eq!(
            manager.check_password_strength("Password123"),
            PasswordStrength::Weak // Length 11, uppercase, lowercase, digit = score 4
        );
        assert_eq!(
            manager.check_password_strength("Password123!"),
            PasswordStrength::Strong // Length 12, uppercase, lowercase, digit, special = score 6
        );
        assert_eq!(
            manager.check_password_strength("MyVerySecureP@ssw0rd2024!"),
            PasswordStrength::VeryStrong // Length 24, all checks = score 7+
        );
    }

    #[test]
    fn test_different_hashes() {
        let manager = PasswordManager::new();
        let password = "same-password";

        let hash1 = manager.hash_password(password).unwrap();
        let hash2 = manager.hash_password(password).unwrap();

        // Hashes should be different due to random salt
        assert_ne!(hash1, hash2);

        // But both should verify correctly
        assert!(manager.verify_password(password, &hash1).unwrap());
        assert!(manager.verify_password(password, &hash2).unwrap());
    }

    #[test]
    fn test_password_policy_default() {
        let manager = PasswordManager::new();

        // Too short
        let result = manager.validate_password("Short1!", None);
        assert!(!result.is_valid);
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v, PolicyViolation::TooShort { .. })));

        // Good password
        let result = manager.validate_password("MySecureP@ss123", None);
        assert!(result.is_valid);
    }

    #[test]
    fn test_strict_policy() {
        let manager = PasswordManager::with_policy(PasswordPolicy::strict());

        // Missing requirements
        let result = manager.validate_password("password", None);
        assert!(!result.is_valid);

        // Meets all requirements
        let result = manager.validate_password("MySecureP@ssw0rd!", None);
        assert!(result.is_valid);
    }

    #[test]
    fn test_common_password_detection() {
        let manager = PasswordManager::new();

        let result = manager.validate_password("password123", None);
        // Common passwords are blocked
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v, PolicyViolation::CommonPassword)));
    }

    #[test]
    fn test_username_in_password() {
        let manager = PasswordManager::new();

        let result = manager.validate_password("MyUsernameIsHere123!", Some("username"));
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v, PolicyViolation::ContainsUsername)));
    }

    #[test]
    fn test_password_generation() {
        let manager = PasswordManager::with_policy(PasswordPolicy::strict());

        let password = manager.generate_password();

        // Should meet policy requirements
        let result = manager.validate_password(&password, None);
        assert!(
            result.is_valid,
            "Generated password should meet policy: {:?}",
            result.violations
        );
    }

    #[test]
    fn test_policy_presets() {
        let strict = PasswordPolicy::strict();
        assert_eq!(strict.min_length, 12);
        assert!(strict.require_uppercase);

        let relaxed = PasswordPolicy::relaxed();
        assert_eq!(relaxed.min_length, 6);
        assert!(!relaxed.require_uppercase);

        let nist = PasswordPolicy::nist_compliant();
        assert_eq!(nist.min_length, 8);
        assert!(nist.disallow_common);
    }

    #[test]
    fn test_strength_comparison() {
        assert!(PasswordStrength::Strong > PasswordStrength::Medium);
        assert!(PasswordStrength::VeryStrong > PasswordStrength::Strong);
        assert!(PasswordStrength::Weak < PasswordStrength::Medium);
    }
}
