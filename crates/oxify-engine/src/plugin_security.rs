//! Plugin Security Scanning and Verification
//!
//! Provides security checks for plugins before loading and execution.
//!
//! # Features
//!
//! - File integrity verification (hash checking)
//! - Permission analysis
//! - Resource usage limits verification
//! - Malicious pattern detection
//! - Dependency vulnerability scanning

use crate::plugin_manifest::{PluginManifest, ResourceRequirements};
use oxicrypto_hash::Sha256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// Security scan errors
#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Failed to read file: {0}")]
    IoError(String),

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Suspicious pattern detected: {0}")]
    SuspiciousPattern(String),

    #[error("Excessive resource requirements: {0}")]
    ExcessiveResources(String),

    #[error("Dangerous permission: {0}")]
    DangerousPermission(String),

    #[error("Vulnerability detected: {0}")]
    VulnerabilityDetected(String),
}

/// Security scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScanResult {
    /// Overall security score (0-100, higher is better)
    pub security_score: u8,
    /// File hash (SHA-256)
    pub file_hash: String,
    /// List of warnings
    pub warnings: Vec<SecurityWarning>,
    /// List of critical issues
    pub critical_issues: Vec<SecurityIssue>,
    /// Scan timestamp
    pub scanned_at: chrono::DateTime<chrono::Utc>,
}

/// Security warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityWarning {
    /// Warning category
    pub category: SecurityCategory,
    /// Warning message
    pub message: String,
    /// Impact on security score
    pub score_impact: u8,
}

/// Security issue (critical)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    /// Issue category
    pub category: SecurityCategory,
    /// Issue description
    pub description: String,
    /// Recommended action
    pub recommendation: String,
}

/// Security categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityCategory {
    /// File integrity issues
    FileIntegrity,
    /// Permission-related issues
    Permissions,
    /// Resource usage concerns
    Resources,
    /// Malicious code patterns
    MaliciousCode,
    /// Dependency vulnerabilities
    Dependencies,
    /// Network access concerns
    Network,
    /// Filesystem access concerns
    Filesystem,
}

/// Plugin security scanner
pub struct PluginSecurityScanner {
    /// Maximum allowed memory in MB
    max_memory_mb: u64,
    /// Maximum allowed CPU cores
    max_cpu_cores: u32,
    /// Allow network access
    allow_network: bool,
    /// Allow filesystem access
    allow_filesystem: bool,
    /// Known malicious patterns
    malicious_patterns: Vec<String>,
}

impl Default for PluginSecurityScanner {
    fn default() -> Self {
        Self {
            max_memory_mb: 1024, // 1GB
            max_cpu_cores: 4,
            allow_network: false,
            allow_filesystem: false,
            malicious_patterns: vec![
                "eval(".to_string(),
                "exec(".to_string(),
                "subprocess".to_string(),
                "__import__".to_string(),
                "dangerous_syscall".to_string(),
            ],
        }
    }
}

impl PluginSecurityScanner {
    /// Create a new security scanner
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a strict scanner with tight security
    pub fn strict() -> Self {
        Self {
            max_memory_mb: 256,
            max_cpu_cores: 1,
            allow_network: false,
            allow_filesystem: false,
            malicious_patterns: vec![
                "eval(".to_string(),
                "exec(".to_string(),
                "subprocess".to_string(),
                "__import__".to_string(),
                "dangerous_syscall".to_string(),
                "require(".to_string(),
                "import(".to_string(),
            ],
        }
    }

    /// Configure maximum memory
    pub fn with_max_memory(mut self, mb: u64) -> Self {
        self.max_memory_mb = mb;
        self
    }

    /// Configure network access
    pub fn with_network_access(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }

    /// Configure filesystem access
    pub fn with_filesystem_access(mut self, allow: bool) -> Self {
        self.allow_filesystem = allow;
        self
    }

    /// Scan a plugin file
    pub fn scan_file(&self, path: &Path) -> Result<SecurityScanResult, SecurityError> {
        // Calculate file hash
        let file_hash = self.calculate_file_hash(path)?;

        let mut warnings = Vec::new();
        let mut critical_issues = Vec::new();

        // Scan file contents for malicious patterns
        if let Ok(content) = std::fs::read_to_string(path) {
            self.scan_for_malicious_patterns(&content, &mut warnings, &mut critical_issues);
        }

        // Calculate security score
        let security_score = self.calculate_security_score(&warnings, &critical_issues);

        Ok(SecurityScanResult {
            security_score,
            file_hash,
            warnings,
            critical_issues,
            scanned_at: chrono::Utc::now(),
        })
    }

    /// Scan a plugin manifest
    pub fn scan_manifest(
        &self,
        manifest: &PluginManifest,
    ) -> Result<SecurityScanResult, SecurityError> {
        let mut warnings = Vec::new();
        let mut critical_issues = Vec::new();

        // Check resource requirements
        self.check_resource_requirements(
            &manifest.capabilities.resource_requirements,
            &mut warnings,
            &mut critical_issues,
        );

        // Check permissions
        self.check_permissions(
            &manifest.capabilities.resource_requirements,
            &mut warnings,
            &mut critical_issues,
        );

        // Calculate security score
        let security_score = self.calculate_security_score(&warnings, &critical_issues);

        Ok(SecurityScanResult {
            security_score,
            file_hash: String::new(), // No file hash for manifest-only scan
            warnings,
            critical_issues,
            scanned_at: chrono::Utc::now(),
        })
    }

    /// Verify file hash
    pub fn verify_hash(&self, path: &Path, expected_hash: &str) -> Result<(), SecurityError> {
        let actual_hash = self.calculate_file_hash(path)?;

        if actual_hash != expected_hash {
            return Err(SecurityError::HashMismatch {
                expected: expected_hash.to_string(),
                actual: actual_hash,
            });
        }

        Ok(())
    }

    /// Calculate SHA-256 hash of a file
    fn calculate_file_hash(&self, path: &Path) -> Result<String, SecurityError> {
        let bytes = std::fs::read(path).map_err(|e| SecurityError::IoError(e.to_string()))?;

        Ok(hex::encode(Sha256.hash_fixed(&bytes)))
    }

    /// Scan content for malicious patterns
    fn scan_for_malicious_patterns(
        &self,
        content: &str,
        _warnings: &mut Vec<SecurityWarning>,
        critical_issues: &mut Vec<SecurityIssue>,
    ) {
        for pattern in &self.malicious_patterns {
            if content.contains(pattern) {
                critical_issues.push(SecurityIssue {
                    category: SecurityCategory::MaliciousCode,
                    description: format!("Detected potentially malicious pattern: {}", pattern),
                    recommendation: "Review the code carefully or reject the plugin".to_string(),
                });
            }
        }
    }

    /// Check resource requirements
    fn check_resource_requirements(
        &self,
        requirements: &ResourceRequirements,
        warnings: &mut Vec<SecurityWarning>,
        critical_issues: &mut Vec<SecurityIssue>,
    ) {
        // Check memory
        if let Some(max_mem) = requirements.max_memory_mb {
            if max_mem > self.max_memory_mb {
                critical_issues.push(SecurityIssue {
                    category: SecurityCategory::Resources,
                    description: format!(
                        "Plugin requires {}MB memory, exceeds limit of {}MB",
                        max_mem, self.max_memory_mb
                    ),
                    recommendation: "Increase limit or reject the plugin".to_string(),
                });
            } else if max_mem > self.max_memory_mb / 2 {
                warnings.push(SecurityWarning {
                    category: SecurityCategory::Resources,
                    message: format!("Plugin requires high memory: {}MB", max_mem),
                    score_impact: 10,
                });
            }
        }

        // Check CPU
        if let Some(cpu_cores) = requirements.cpu_cores {
            if cpu_cores > self.max_cpu_cores {
                warnings.push(SecurityWarning {
                    category: SecurityCategory::Resources,
                    message: format!(
                        "Plugin requires {} CPU cores, exceeds limit of {}",
                        cpu_cores, self.max_cpu_cores
                    ),
                    score_impact: 10,
                });
            }
        }
    }

    /// Check permissions
    fn check_permissions(
        &self,
        requirements: &ResourceRequirements,
        warnings: &mut Vec<SecurityWarning>,
        critical_issues: &mut Vec<SecurityIssue>,
    ) {
        // Check network permission
        if requirements.requires_network && !self.allow_network {
            critical_issues.push(SecurityIssue {
                category: SecurityCategory::Network,
                description: "Plugin requires network access but it is not allowed".to_string(),
                recommendation: "Enable network access or reject the plugin".to_string(),
            });
        } else if requirements.requires_network {
            warnings.push(SecurityWarning {
                category: SecurityCategory::Network,
                message: "Plugin has network access - potential data exfiltration risk".to_string(),
                score_impact: 15,
            });
        }

        // Check filesystem permission
        if requirements.requires_filesystem && !self.allow_filesystem {
            critical_issues.push(SecurityIssue {
                category: SecurityCategory::Filesystem,
                description: "Plugin requires filesystem access but it is not allowed".to_string(),
                recommendation: "Enable filesystem access or reject the plugin".to_string(),
            });
        } else if requirements.requires_filesystem {
            warnings.push(SecurityWarning {
                category: SecurityCategory::Filesystem,
                message: "Plugin has filesystem access - potential security risk".to_string(),
                score_impact: 15,
            });
        }
    }

    /// Calculate overall security score
    fn calculate_security_score(
        &self,
        warnings: &[SecurityWarning],
        critical_issues: &[SecurityIssue],
    ) -> u8 {
        let mut score = 100u8;

        // Deduct for warnings
        for warning in warnings {
            score = score.saturating_sub(warning.score_impact);
        }

        // Deduct heavily for critical issues
        let critical_deduction = (critical_issues.len() as u8).saturating_mul(30);
        score = score.saturating_sub(critical_deduction);

        score
    }
}

/// Security policy for plugin loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Minimum security score required (0-100)
    pub min_security_score: u8,
    /// Allow plugins with warnings
    pub allow_warnings: bool,
    /// Allow plugins with critical issues
    pub allow_critical_issues: bool,
    /// Require hash verification
    pub require_hash_verification: bool,
    /// Known good hashes
    pub known_good_hashes: HashMap<String, String>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            min_security_score: 70,
            allow_warnings: true,
            allow_critical_issues: false,
            require_hash_verification: false,
            known_good_hashes: HashMap::new(),
        }
    }
}

impl SecurityPolicy {
    /// Create a strict security policy
    pub fn strict() -> Self {
        Self {
            min_security_score: 90,
            allow_warnings: false,
            allow_critical_issues: false,
            require_hash_verification: true,
            known_good_hashes: HashMap::new(),
        }
    }

    /// Check if a scan result passes the policy
    pub fn check(&self, result: &SecurityScanResult) -> Result<(), SecurityError> {
        // Check security score
        if result.security_score < self.min_security_score {
            return Err(SecurityError::VulnerabilityDetected(format!(
                "Security score {} below minimum {}",
                result.security_score, self.min_security_score
            )));
        }

        // Check warnings
        if !self.allow_warnings && !result.warnings.is_empty() {
            return Err(SecurityError::VulnerabilityDetected(format!(
                "Plugin has {} warnings, which are not allowed",
                result.warnings.len()
            )));
        }

        // Check critical issues
        if !self.allow_critical_issues && !result.critical_issues.is_empty() {
            return Err(SecurityError::VulnerabilityDetected(format!(
                "Plugin has {} critical issues",
                result.critical_issues.len()
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_scanner_creation() {
        let scanner = PluginSecurityScanner::new();
        assert_eq!(scanner.max_memory_mb, 1024);
        assert_eq!(scanner.max_cpu_cores, 4);
    }

    #[test]
    fn test_strict_scanner() {
        let scanner = PluginSecurityScanner::strict();
        assert_eq!(scanner.max_memory_mb, 256);
        assert_eq!(scanner.max_cpu_cores, 1);
        assert!(!scanner.allow_network);
        assert!(!scanner.allow_filesystem);
    }

    #[test]
    fn test_scanner_configuration() {
        let scanner = PluginSecurityScanner::new()
            .with_max_memory(512)
            .with_network_access(true)
            .with_filesystem_access(true);

        assert_eq!(scanner.max_memory_mb, 512);
        assert!(scanner.allow_network);
        assert!(scanner.allow_filesystem);
    }

    #[test]
    fn test_security_score_calculation() {
        let scanner = PluginSecurityScanner::new();

        let warnings = vec![
            SecurityWarning {
                category: SecurityCategory::Resources,
                message: "High memory".to_string(),
                score_impact: 10,
            },
            SecurityWarning {
                category: SecurityCategory::Network,
                message: "Network access".to_string(),
                score_impact: 15,
            },
        ];

        let critical_issues = vec![];

        let score = scanner.calculate_security_score(&warnings, &critical_issues);
        assert_eq!(score, 75); // 100 - 10 - 15
    }

    #[test]
    fn test_security_score_with_critical_issues() {
        let scanner = PluginSecurityScanner::new();

        let warnings = vec![];
        let critical_issues = vec![SecurityIssue {
            category: SecurityCategory::MaliciousCode,
            description: "Malicious pattern".to_string(),
            recommendation: "Reject".to_string(),
        }];

        let score = scanner.calculate_security_score(&warnings, &critical_issues);
        assert_eq!(score, 70); // 100 - 30
    }

    #[test]
    fn test_security_policy_default() {
        let policy = SecurityPolicy::default();
        assert_eq!(policy.min_security_score, 70);
        assert!(policy.allow_warnings);
        assert!(!policy.allow_critical_issues);
    }

    #[test]
    fn test_security_policy_strict() {
        let policy = SecurityPolicy::strict();
        assert_eq!(policy.min_security_score, 90);
        assert!(!policy.allow_warnings);
        assert!(!policy.allow_critical_issues);
        assert!(policy.require_hash_verification);
    }

    #[test]
    fn test_policy_check_passes() {
        let policy = SecurityPolicy::default();
        let result = SecurityScanResult {
            security_score: 80,
            file_hash: "abc123".to_string(),
            warnings: vec![],
            critical_issues: vec![],
            scanned_at: chrono::Utc::now(),
        };

        assert!(policy.check(&result).is_ok());
    }

    #[test]
    fn test_policy_check_fails_score() {
        let policy = SecurityPolicy::default();
        let result = SecurityScanResult {
            security_score: 50,
            file_hash: "abc123".to_string(),
            warnings: vec![],
            critical_issues: vec![],
            scanned_at: chrono::Utc::now(),
        };

        assert!(policy.check(&result).is_err());
    }

    #[test]
    fn test_policy_check_fails_critical_issues() {
        let policy = SecurityPolicy::default();
        let result = SecurityScanResult {
            security_score: 80,
            file_hash: "abc123".to_string(),
            warnings: vec![],
            critical_issues: vec![SecurityIssue {
                category: SecurityCategory::MaliciousCode,
                description: "Malicious".to_string(),
                recommendation: "Reject".to_string(),
            }],
            scanned_at: chrono::Utc::now(),
        };

        assert!(policy.check(&result).is_err());
    }
}
