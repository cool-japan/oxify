//! Security Scanner Module
//!
//! This module provides comprehensive security analysis for workflows,
//! including threat detection, risk assessment, compliance checking,
//! and security audit report generation.
//!
//! # Features
//!
//! - **Threat Detection:** Identify security vulnerabilities (injection, XSS, etc.)
//! - **Risk Assessment:** Calculate risk scores and impact analysis
//! - **Compliance Checking:** Validate against security standards (GDPR, etc.)
//! - **Secret Scanning:** Advanced secret and credential detection
//! - **Audit Reports:** Generate comprehensive security audit reports
//!
//! # Example
//!
//! ```rust
//! use oxify_model::security::{SecurityScanner, SecurityConfig, RiskLevel};
//! use oxify_model::WorkflowBuilder;
//!
//! let workflow = WorkflowBuilder::new("test")
//!     .start("Start")
//!     .end("End")
//!     .build();
//!
//! let scanner = SecurityScanner::new(SecurityConfig::default());
//! let report = scanner.scan(&workflow);
//!
//! println!("Security Score: {}", report.security_score);
//! println!("Critical Issues: {}", report.findings_by_severity(RiskLevel::Critical).len());
//! ```

use crate::{NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Risk level for security findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Informational finding
    Info,
    /// Low risk
    Low,
    /// Medium risk
    Medium,
    /// High risk
    High,
    /// Critical risk requiring immediate attention
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Info => write!(f, "INFO"),
            RiskLevel::Low => write!(f, "LOW"),
            RiskLevel::Medium => write!(f, "MEDIUM"),
            RiskLevel::High => write!(f, "HIGH"),
            RiskLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Category of security threat
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatCategory {
    /// Injection attacks (SQL, command, prompt injection)
    Injection,
    /// Cross-site scripting vulnerabilities
    Xss,
    /// Sensitive data exposure
    DataExposure,
    /// Authentication/authorization issues
    AuthN,
    /// Broken access control
    AccessControl,
    /// Security misconfiguration
    Misconfiguration,
    /// Insecure deserialization
    Deserialization,
    /// Using components with known vulnerabilities
    KnownVulnerabilities,
    /// Insufficient logging and monitoring
    InsufficientLogging,
    /// Data privacy violations (GDPR, etc.)
    DataPrivacy,
}

impl std::fmt::Display for ThreatCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThreatCategory::Injection => write!(f, "Injection"),
            ThreatCategory::Xss => write!(f, "XSS"),
            ThreatCategory::DataExposure => write!(f, "Data Exposure"),
            ThreatCategory::AuthN => write!(f, "Authentication"),
            ThreatCategory::AccessControl => write!(f, "Access Control"),
            ThreatCategory::Misconfiguration => write!(f, "Misconfiguration"),
            ThreatCategory::Deserialization => write!(f, "Deserialization"),
            ThreatCategory::KnownVulnerabilities => write!(f, "Known Vulnerabilities"),
            ThreatCategory::InsufficientLogging => write!(f, "Insufficient Logging"),
            ThreatCategory::DataPrivacy => write!(f, "Data Privacy"),
        }
    }
}

/// Compliance standard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComplianceStandard {
    /// General Data Protection Regulation
    Gdpr,
    /// Health Insurance Portability and Accountability Act
    Hipaa,
    /// Payment Card Industry Data Security Standard
    PciDss,
    /// Sarbanes-Oxley Act
    Sox,
    /// OWASP Top 10
    OwaspTop10,
}

impl std::fmt::Display for ComplianceStandard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComplianceStandard::Gdpr => write!(f, "GDPR"),
            ComplianceStandard::Hipaa => write!(f, "HIPAA"),
            ComplianceStandard::PciDss => write!(f, "PCI-DSS"),
            ComplianceStandard::Sox => write!(f, "SOX"),
            ComplianceStandard::OwaspTop10 => write!(f, "OWASP Top 10"),
        }
    }
}

/// Security finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    /// Finding identifier
    pub id: String,
    /// Risk level
    pub risk_level: RiskLevel,
    /// Threat category
    pub category: ThreatCategory,
    /// Title of the finding
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Node ID where the issue was found
    pub node_id: Option<String>,
    /// Node name for reference
    pub node_name: Option<String>,
    /// Affected workflow components
    pub affected_components: Vec<String>,
    /// Remediation recommendation
    pub remediation: String,
    /// OWASP category reference
    pub owasp_category: Option<String>,
    /// CWE (Common Weakness Enumeration) ID
    pub cwe_id: Option<u32>,
    /// Compliance standards violated
    pub compliance_violations: Vec<ComplianceStandard>,
}

impl SecurityFinding {
    /// Create a new security finding
    pub fn new(
        id: impl Into<String>,
        risk_level: RiskLevel,
        category: ThreatCategory,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            risk_level,
            category,
            title: title.into(),
            description: description.into(),
            node_id: None,
            node_name: None,
            affected_components: Vec::new(),
            remediation: String::new(),
            owasp_category: None,
            cwe_id: None,
            compliance_violations: Vec::new(),
        }
    }

    /// Set node information
    pub fn with_node(mut self, node_id: String, node_name: String) -> Self {
        self.node_id = Some(node_id);
        self.node_name = Some(node_name);
        self
    }

    /// Add remediation recommendation
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = remediation.into();
        self
    }

    /// Add OWASP category
    pub fn with_owasp(mut self, category: impl Into<String>) -> Self {
        self.owasp_category = Some(category.into());
        self
    }

    /// Add CWE ID
    pub fn with_cwe(mut self, cwe_id: u32) -> Self {
        self.cwe_id = Some(cwe_id);
        self
    }

    /// Add compliance violation
    pub fn with_compliance(mut self, standard: ComplianceStandard) -> Self {
        self.compliance_violations.push(standard);
        self
    }

    /// Add affected component
    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.affected_components.push(component.into());
        self
    }
}

/// Security audit report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditReport {
    /// Workflow ID being audited
    pub workflow_id: String,
    /// Workflow name
    pub workflow_name: String,
    /// All security findings
    pub findings: Vec<SecurityFinding>,
    /// Overall security score (0-100, higher is better)
    pub security_score: f64,
    /// Compliance status
    pub compliance_status: HashMap<ComplianceStandard, bool>,
    /// Risk summary
    pub risk_summary: RiskSummary,
    /// Scan timestamp
    pub scanned_at: String,
    /// Recommendations
    pub recommendations: Vec<String>,
}

impl SecurityAuditReport {
    /// Get findings by risk level
    pub fn findings_by_severity(&self, level: RiskLevel) -> Vec<&SecurityFinding> {
        self.findings
            .iter()
            .filter(|f| f.risk_level == level)
            .collect()
    }

    /// Get findings by category
    pub fn findings_by_category(&self, category: ThreatCategory) -> Vec<&SecurityFinding> {
        self.findings
            .iter()
            .filter(|f| f.category == category)
            .collect()
    }

    /// Check if audit passed (no critical or high findings)
    pub fn passed(&self) -> bool {
        self.findings_by_severity(RiskLevel::Critical).is_empty()
            && self.findings_by_severity(RiskLevel::High).is_empty()
    }

    /// Get total finding count
    pub fn total_findings(&self) -> usize {
        self.findings.len()
    }
}

/// Risk summary statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskSummary {
    /// Number of critical findings
    pub critical: usize,
    /// Number of high findings
    pub high: usize,
    /// Number of medium findings
    pub medium: usize,
    /// Number of low findings
    pub low: usize,
    /// Number of informational findings
    pub info: usize,
}

impl RiskSummary {
    /// Calculate from findings
    pub fn from_findings(findings: &[SecurityFinding]) -> Self {
        let mut summary = Self::default();
        for finding in findings {
            match finding.risk_level {
                RiskLevel::Critical => summary.critical += 1,
                RiskLevel::High => summary.high += 1,
                RiskLevel::Medium => summary.medium += 1,
                RiskLevel::Low => summary.low += 1,
                RiskLevel::Info => summary.info += 1,
            }
        }
        summary
    }

    /// Calculate total issues
    pub fn total(&self) -> usize {
        self.critical + self.high + self.medium + self.low + self.info
    }

    /// Calculate risk score (0-100, higher is worse)
    pub fn risk_score(&self) -> f64 {
        let weighted = (self.critical as f64 * 10.0)
            + (self.high as f64 * 5.0)
            + (self.medium as f64 * 2.0)
            + (self.low as f64 * 0.5);

        // Cap at 100
        weighted.min(100.0)
    }
}

/// Security scanner configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable prompt injection detection
    pub check_prompt_injection: bool,
    /// Enable SQL injection detection
    pub check_sql_injection: bool,
    /// Enable command injection detection
    pub check_command_injection: bool,
    /// Enable XSS detection
    pub check_xss: bool,
    /// Enable secret scanning
    pub check_secrets: bool,
    /// Enable data privacy checks
    pub check_data_privacy: bool,
    /// Enable compliance checks
    pub check_compliance: bool,
    /// Required compliance standards
    pub compliance_standards: Vec<ComplianceStandard>,
    /// Custom secret patterns (regex)
    pub custom_secret_patterns: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            check_prompt_injection: true,
            check_sql_injection: true,
            check_command_injection: true,
            check_xss: true,
            check_secrets: true,
            check_data_privacy: true,
            check_compliance: true,
            compliance_standards: vec![ComplianceStandard::OwaspTop10],
            custom_secret_patterns: Vec::new(),
        }
    }
}

/// Security scanner for workflows
pub struct SecurityScanner {
    config: SecurityConfig,
}

impl SecurityScanner {
    /// Create a new security scanner
    pub fn new(config: SecurityConfig) -> Self {
        Self { config }
    }

    /// Scan a workflow for security issues
    pub fn scan(&self, workflow: &Workflow) -> SecurityAuditReport {
        let mut findings = Vec::new();

        // Run all security checks
        if self.config.check_prompt_injection {
            findings.extend(self.check_prompt_injection(workflow));
        }

        if self.config.check_sql_injection {
            findings.extend(self.check_sql_injection(workflow));
        }

        if self.config.check_command_injection {
            findings.extend(self.check_command_injection(workflow));
        }

        if self.config.check_xss {
            findings.extend(self.check_xss(workflow));
        }

        if self.config.check_secrets {
            findings.extend(self.check_secrets(workflow));
        }

        if self.config.check_data_privacy {
            findings.extend(self.check_data_privacy(workflow));
        }

        // Calculate risk summary and security score
        let risk_summary = RiskSummary::from_findings(&findings);
        let security_score = 100.0 - risk_summary.risk_score();

        // Check compliance
        let compliance_status = self.check_compliance(workflow, &findings);

        // Generate recommendations
        let recommendations = self.generate_recommendations(&findings, &risk_summary);

        SecurityAuditReport {
            workflow_id: workflow.metadata.id.to_string(),
            workflow_name: workflow.metadata.name.clone(),
            findings,
            security_score,
            compliance_status,
            risk_summary,
            scanned_at: chrono::Utc::now().to_rfc3339(),
            recommendations,
        }
    }

    /// Check for prompt injection vulnerabilities
    fn check_prompt_injection(&self, workflow: &Workflow) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        let injection_patterns = [
            "ignore previous instructions",
            "ignore all previous",
            "disregard all prior",
            "forget previous",
            "new instructions:",
            "system:",
            "assistant:",
            "{{",
            "}}",
        ];

        for node in &workflow.nodes {
            if let NodeKind::LLM(llm_config) = &node.kind {
                let prompt = &llm_config.prompt_template;

                // Check for user input placeholders that might be vulnerable
                if prompt.contains("{{user_input}}") || prompt.contains("${user_input}") {
                    findings.push(
                        SecurityFinding::new(
                            "PROMPT_INJECTION_001",
                            RiskLevel::High,
                            ThreatCategory::Injection,
                            "Potential Prompt Injection Vulnerability",
                            "LLM prompt contains unsanitized user input that could be exploited for prompt injection attacks",
                        )
                        .with_node(node.id.to_string(), node.name.clone())
                        .with_remediation(
                            "Implement input validation and sanitization. Use prompt templates that clearly separate instructions from user input. Consider using a dedicated prompt injection prevention library."
                        )
                        .with_owasp("A03:2021 - Injection")
                        .with_cwe(94)
                        .with_compliance(ComplianceStandard::OwaspTop10)
                    );
                }

                // Check if prompt contains injection patterns (might be testing or malicious)
                for pattern in &injection_patterns {
                    if prompt.to_lowercase().contains(&pattern.to_lowercase()) {
                        findings.push(
                            SecurityFinding::new(
                                "PROMPT_INJECTION_002",
                                RiskLevel::Medium,
                                ThreatCategory::Injection,
                                "Suspicious Prompt Content Detected",
                                format!("Prompt contains pattern '{}' that may indicate prompt injection", pattern),
                            )
                            .with_node(node.id.to_string(), node.name.clone())
                            .with_remediation("Review prompt content and ensure it's not vulnerable to manipulation")
                            .with_cwe(94)
                        );
                    }
                }
            }
        }

        findings
    }

    /// Check for SQL injection vulnerabilities
    fn check_sql_injection(&self, workflow: &Workflow) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        let sql_patterns = [
            "SELECT", "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER",
        ];

        for node in &workflow.nodes {
            match &node.kind {
                NodeKind::Code(script_config) => {
                    let code = &script_config.code;

                    // Check for SQL patterns with user input
                    for pattern in &sql_patterns {
                        if code.contains(pattern) && (code.contains("{{") || code.contains("${")) {
                            findings.push(
                                SecurityFinding::new(
                                    "SQL_INJECTION_001",
                                    RiskLevel::Critical,
                                    ThreatCategory::Injection,
                                    "Potential SQL Injection Vulnerability",
                                    format!("Code contains SQL statement '{}' with dynamic user input", pattern),
                                )
                                .with_node(node.id.to_string(), node.name.clone())
                                .with_remediation(
                                    "Use parameterized queries or prepared statements. Never concatenate user input directly into SQL queries."
                                )
                                .with_owasp("A03:2021 - Injection")
                                .with_cwe(89)
                                .with_compliance(ComplianceStandard::OwaspTop10)
                                .with_compliance(ComplianceStandard::PciDss)
                            );
                            break;
                        }
                    }
                }
                NodeKind::LLM(llm_config) => {
                    // Check if LLM might be generating SQL queries
                    let prompt = &llm_config.prompt_template.to_lowercase();
                    if (prompt.contains("sql")
                        || prompt.contains("database")
                        || prompt.contains("query"))
                        && (prompt.contains("{{") || prompt.contains("${"))
                    {
                        findings.push(
                                SecurityFinding::new(
                                    "SQL_INJECTION_002",
                                    RiskLevel::High,
                                    ThreatCategory::Injection,
                                    "LLM Generating SQL with User Input",
                                    "LLM is being used to generate SQL queries with user input, which may be vulnerable to injection",
                                )
                                .with_node(node.id.to_string(), node.name.clone())
                                .with_remediation(
                                    "Validate and sanitize all user inputs. Use an allowlist for table/column names. Consider using an ORM instead of raw SQL."
                                )
                                .with_cwe(89)
                            );
                    }
                }
                NodeKind::Start
                | NodeKind::End
                | NodeKind::Retriever(_)
                | NodeKind::IfElse(_)
                | NodeKind::Tool(_)
                | NodeKind::Loop(_)
                | NodeKind::TryCatch(_)
                | NodeKind::SubWorkflow(_)
                | NodeKind::Switch(_)
                | NodeKind::Parallel(_)
                | NodeKind::Approval(_)
                | NodeKind::Form(_)
                | NodeKind::Vision(_)
                | NodeKind::Custom(_) => {}
            }
        }

        findings
    }

    /// Check for command injection vulnerabilities
    fn check_command_injection(&self, workflow: &Workflow) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        let shell_patterns = [
            "exec",
            "eval",
            "system",
            "popen",
            "subprocess",
            "sh",
            "bash",
            "cmd",
        ];

        for node in &workflow.nodes {
            if let NodeKind::Code(script_config) = &node.kind {
                let code = &script_config.code;

                for pattern in &shell_patterns {
                    if code.contains(pattern) && (code.contains("{{") || code.contains("${")) {
                        findings.push(
                            SecurityFinding::new(
                                "CMD_INJECTION_001",
                                RiskLevel::Critical,
                                ThreatCategory::Injection,
                                "Potential Command Injection Vulnerability",
                                format!("Code uses '{}' with dynamic user input", pattern),
                            )
                            .with_node(node.id.to_string(), node.name.clone())
                            .with_remediation(
                                "Avoid executing shell commands with user input. Use safe APIs instead. If necessary, use strict input validation and allowlisting."
                            )
                            .with_owasp("A03:2021 - Injection")
                            .with_cwe(78)
                            .with_compliance(ComplianceStandard::OwaspTop10)
                        );
                        break;
                    }
                }
            }
        }

        findings
    }

    /// Check for XSS vulnerabilities
    fn check_xss(&self, workflow: &Workflow) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        let html_patterns = ["<script>", "innerHTML", "document.write", "eval("];

        for node in &workflow.nodes {
            if let NodeKind::Code(script_config) = &node.kind {
                let code = &script_config.code;

                for pattern in &html_patterns {
                    if code.contains(pattern) && (code.contains("{{") || code.contains("${")) {
                        findings.push(
                                SecurityFinding::new(
                                    "XSS_001",
                                    RiskLevel::High,
                                    ThreatCategory::Xss,
                                    "Potential Cross-Site Scripting (XSS) Vulnerability",
                                    format!("Code uses '{}' with dynamic content", pattern),
                                )
                                .with_node(node.id.to_string(), node.name.clone())
                                .with_remediation(
                                    "Always sanitize and escape user input before rendering in HTML. Use Content Security Policy (CSP) headers."
                                )
                                .with_owasp("A03:2021 - Injection")
                                .with_cwe(79)
                                .with_compliance(ComplianceStandard::OwaspTop10)
                            );
                        break;
                    }
                }
            }
        }

        findings
    }

    /// Check for hardcoded secrets and credentials
    fn check_secrets(&self, workflow: &Workflow) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        let secret_patterns = vec![
            ("api_key", "API Key"),
            ("apikey", "API Key"),
            ("password", "Password"),
            ("passwd", "Password"),
            ("secret", "Secret"),
            ("token", "Token"),
            ("bearer", "Bearer Token"),
            ("aws_access_key", "AWS Access Key"),
            ("private_key", "Private Key"),
            ("credentials", "Credentials"),
        ];

        for node in &workflow.nodes {
            let search_text = match &node.kind {
                NodeKind::LLM(cfg) => &cfg.prompt_template,
                NodeKind::Code(cfg) => &cfg.code,
                NodeKind::Start
                | NodeKind::End
                | NodeKind::Retriever(_)
                | NodeKind::IfElse(_)
                | NodeKind::Tool(_)
                | NodeKind::Loop(_)
                | NodeKind::TryCatch(_)
                | NodeKind::SubWorkflow(_)
                | NodeKind::Switch(_)
                | NodeKind::Parallel(_)
                | NodeKind::Approval(_)
                | NodeKind::Form(_)
                | NodeKind::Vision(_)
                | NodeKind::Custom(_) => continue,
            };

            for (pattern, secret_type) in &secret_patterns {
                if search_text.to_lowercase().contains(pattern) {
                    // Check if it looks like a hardcoded value (contains =)
                    if search_text.contains(&format!("{} =", pattern))
                        || search_text.contains(&format!("{}=", pattern))
                        || search_text.contains(&format!("{}: ", pattern))
                    {
                        findings.push(
                            SecurityFinding::new(
                                "SECRET_001",
                                RiskLevel::Critical,
                                ThreatCategory::DataExposure,
                                format!("Potential Hardcoded {}", secret_type),
                                format!("Node may contain hardcoded {} which should be stored securely", secret_type),
                            )
                            .with_node(node.id.to_string(), node.name.clone())
                            .with_remediation(
                                "Use environment variables or a secure secret management system (e.g., AWS Secrets Manager, HashiCorp Vault). Never hardcode secrets in workflows."
                            )
                            .with_owasp("A02:2021 - Cryptographic Failures")
                            .with_cwe(798)
                            .with_compliance(ComplianceStandard::OwaspTop10)
                            .with_compliance(ComplianceStandard::PciDss)
                        );
                    }
                }
            }

            // Check custom patterns
            for pattern in &self.config.custom_secret_patterns {
                if search_text.contains(pattern) {
                    findings.push(
                        SecurityFinding::new(
                            "SECRET_002",
                            RiskLevel::High,
                            ThreatCategory::DataExposure,
                            "Custom Secret Pattern Detected",
                            format!("Node contains custom secret pattern: {}", pattern),
                        )
                        .with_node(node.id.to_string(), node.name.clone())
                        .with_remediation("Review and remove any hardcoded secrets"),
                    );
                }
            }
        }

        findings
    }

    /// Check for data privacy violations
    fn check_data_privacy(&self, workflow: &Workflow) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        let pii_patterns = [
            "ssn",
            "social security",
            "credit card",
            "card number",
            "email",
            "phone",
            "address",
            "date of birth",
            "dob",
            "passport",
            "driver license",
            "medical",
            "health",
        ];

        for node in &workflow.nodes {
            if let NodeKind::LLM(llm_config) = &node.kind {
                let prompt = &llm_config.prompt_template.to_lowercase();

                for pattern in &pii_patterns {
                    if prompt.contains(pattern) {
                        findings.push(
                            SecurityFinding::new(
                                "PRIVACY_001",
                                RiskLevel::High,
                                ThreatCategory::DataPrivacy,
                                "Potential PII Processing Detected",
                                format!("Workflow may process sensitive personal data: {}", pattern),
                            )
                            .with_node(node.id.to_string(), node.name.clone())
                            .with_remediation(
                                "Ensure PII is processed in compliance with GDPR, HIPAA, or other applicable regulations. Implement data minimization and anonymization where possible."
                            )
                            .with_compliance(ComplianceStandard::Gdpr)
                            .with_compliance(ComplianceStandard::Hipaa)
                        );
                        break;
                    }
                }
            }
        }

        findings
    }

    /// Check compliance status
    fn check_compliance(
        &self,
        _workflow: &Workflow,
        findings: &[SecurityFinding],
    ) -> HashMap<ComplianceStandard, bool> {
        let mut status = HashMap::new();

        for standard in &self.config.compliance_standards {
            let violations = findings
                .iter()
                .any(|f| f.compliance_violations.contains(standard));
            status.insert(*standard, !violations);
        }

        status
    }

    /// Generate recommendations based on findings
    fn generate_recommendations(
        &self,
        findings: &[SecurityFinding],
        risk_summary: &RiskSummary,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        if risk_summary.critical > 0 {
            recommendations.push(
                "URGENT: Address all critical security findings immediately before deploying this workflow".to_string()
            );
        }

        if risk_summary.high > 0 {
            recommendations.push(
                "Address all high-severity security findings as soon as possible".to_string(),
            );
        }

        // Category-specific recommendations
        let has_injection = findings
            .iter()
            .any(|f| f.category == ThreatCategory::Injection);
        if has_injection {
            recommendations.push(
                "Implement input validation and sanitization for all user inputs".to_string(),
            );
        }

        let has_secrets = findings
            .iter()
            .any(|f| f.category == ThreatCategory::DataExposure);
        if has_secrets {
            recommendations.push(
                "Use a secure secret management system (e.g., AWS Secrets Manager, HashiCorp Vault)".to_string()
            );
        }

        let has_privacy = findings
            .iter()
            .any(|f| f.category == ThreatCategory::DataPrivacy);
        if has_privacy {
            recommendations.push(
                "Ensure data privacy compliance (GDPR, HIPAA) for all PII processing".to_string(),
            );
        }

        if recommendations.is_empty() {
            recommendations.push(
                "No major security issues found. Continue following security best practices."
                    .to_string(),
            );
        }

        recommendations
    }
}

impl Default for SecurityScanner {
    fn default() -> Self {
        Self::new(SecurityConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmConfig, ScriptConfig, WorkflowBuilder};

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
        assert!(RiskLevel::Low > RiskLevel::Info);
    }

    #[test]
    fn test_security_finding_builder() {
        let finding = SecurityFinding::new(
            "TEST_001",
            RiskLevel::High,
            ThreatCategory::Injection,
            "Test Finding",
            "Test description",
        )
        .with_node("node1".to_string(), "Test Node".to_string())
        .with_remediation("Fix this issue")
        .with_owasp("A03:2021")
        .with_cwe(89)
        .with_compliance(ComplianceStandard::OwaspTop10);

        assert_eq!(finding.id, "TEST_001");
        assert_eq!(finding.risk_level, RiskLevel::High);
        assert_eq!(finding.node_id, Some("node1".to_string()));
        assert_eq!(finding.cwe_id, Some(89));
        assert!(finding
            .compliance_violations
            .contains(&ComplianceStandard::OwaspTop10));
    }

    #[test]
    fn test_risk_summary_calculation() {
        let findings = vec![
            SecurityFinding::new(
                "1",
                RiskLevel::Critical,
                ThreatCategory::Injection,
                "T1",
                "D1",
            ),
            SecurityFinding::new("2", RiskLevel::High, ThreatCategory::Injection, "T2", "D2"),
            SecurityFinding::new("3", RiskLevel::Medium, ThreatCategory::Xss, "T3", "D3"),
            SecurityFinding::new(
                "4",
                RiskLevel::Low,
                ThreatCategory::DataExposure,
                "T4",
                "D4",
            ),
        ];

        let summary = RiskSummary::from_findings(&findings);
        assert_eq!(summary.critical, 1);
        assert_eq!(summary.high, 1);
        assert_eq!(summary.medium, 1);
        assert_eq!(summary.low, 1);
        assert_eq!(summary.total(), 4);
    }

    #[test]
    fn test_risk_score_calculation() {
        let summary = RiskSummary {
            critical: 1,
            high: 2,
            medium: 3,
            low: 0,
            info: 0,
        };

        let score = summary.risk_score();
        // 1*10 + 2*5 + 3*2 = 10 + 10 + 6 = 26
        assert_eq!(score, 26.0);
    }

    #[test]
    fn test_prompt_injection_detection() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm(
                "LLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    prompt_template: "Process this: {{user_input}}".to_string(),
                    temperature: Some(0.7),
                    max_tokens: None,
                    system_prompt: None,
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let scanner = SecurityScanner::default();
        let report = scanner.scan(&workflow);

        let injection_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.category == ThreatCategory::Injection)
            .collect();

        assert!(!injection_findings.is_empty());
        assert_eq!(injection_findings[0].risk_level, RiskLevel::High);
    }

    #[test]
    fn test_sql_injection_detection() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .code(
                "SQL",
                ScriptConfig {
                    runtime: "python".to_string(),
                    code: "SELECT * FROM users WHERE id = {{user_id}}".to_string(),
                    inputs: Vec::new(),
                    output: "result".to_string(),
                },
            )
            .end("End")
            .build();

        let scanner = SecurityScanner::default();
        let report = scanner.scan(&workflow);

        let sql_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.id.starts_with("SQL_INJECTION"))
            .collect();

        assert!(!sql_findings.is_empty());
        assert_eq!(sql_findings[0].risk_level, RiskLevel::Critical);
    }

    #[test]
    fn test_command_injection_detection() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .code(
                "Shell",
                ScriptConfig {
                    runtime: "bash".to_string(),
                    code: "exec {{command}}".to_string(),
                    inputs: Vec::new(),
                    output: "result".to_string(),
                },
            )
            .end("End")
            .build();

        let scanner = SecurityScanner::default();
        let report = scanner.scan(&workflow);

        let cmd_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.id.starts_with("CMD_INJECTION"))
            .collect();

        assert!(!cmd_findings.is_empty());
        assert_eq!(cmd_findings[0].risk_level, RiskLevel::Critical);
    }

    #[test]
    fn test_secret_detection() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm(
                "LLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    prompt_template: "Use api_key = sk-1234567890".to_string(),
                    temperature: Some(0.7),
                    max_tokens: None,
                    system_prompt: None,
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let scanner = SecurityScanner::default();
        let report = scanner.scan(&workflow);

        let secret_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.id.starts_with("SECRET"))
            .collect();

        assert!(!secret_findings.is_empty());
        assert_eq!(secret_findings[0].risk_level, RiskLevel::Critical);
    }

    #[test]
    fn test_pii_detection() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm(
                "LLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    prompt_template: "Process the user's email and social security number"
                        .to_string(),
                    temperature: Some(0.7),
                    max_tokens: None,
                    system_prompt: None,
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let scanner = SecurityScanner::default();
        let report = scanner.scan(&workflow);

        let privacy_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.category == ThreatCategory::DataPrivacy)
            .collect();

        assert!(!privacy_findings.is_empty());
    }

    #[test]
    fn test_security_score_calculation() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .end("End")
            .build();

        let scanner = SecurityScanner::default();
        let report = scanner.scan(&workflow);

        // Clean workflow should have high security score
        assert!(report.security_score >= 90.0);
        assert!(report.passed());
    }

    #[test]
    fn test_audit_report_filtering() {
        let findings = vec![
            SecurityFinding::new(
                "1",
                RiskLevel::Critical,
                ThreatCategory::Injection,
                "T1",
                "D1",
            ),
            SecurityFinding::new("2", RiskLevel::High, ThreatCategory::Injection, "T2", "D2"),
            SecurityFinding::new("3", RiskLevel::Medium, ThreatCategory::Xss, "T3", "D3"),
        ];

        let report = SecurityAuditReport {
            workflow_id: "test".to_string(),
            workflow_name: "Test".to_string(),
            findings,
            security_score: 75.0,
            compliance_status: HashMap::new(),
            risk_summary: RiskSummary::default(),
            scanned_at: "2026-01-31T00:00:00Z".to_string(),
            recommendations: Vec::new(),
        };

        assert_eq!(report.findings_by_severity(RiskLevel::Critical).len(), 1);
        assert_eq!(report.findings_by_severity(RiskLevel::High).len(), 1);
        assert_eq!(
            report.findings_by_category(ThreatCategory::Injection).len(),
            2
        );
        assert_eq!(report.findings_by_category(ThreatCategory::Xss).len(), 1);
    }

    #[test]
    fn test_compliance_checking() {
        let config = SecurityConfig {
            compliance_standards: vec![ComplianceStandard::OwaspTop10, ComplianceStandard::Gdpr],
            ..Default::default()
        };

        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .end("End")
            .build();

        let scanner = SecurityScanner::new(config);
        let report = scanner.scan(&workflow);

        // Clean workflow should pass all compliance checks
        assert_eq!(
            report
                .compliance_status
                .get(&ComplianceStandard::OwaspTop10),
            Some(&true)
        );
        assert_eq!(
            report.compliance_status.get(&ComplianceStandard::Gdpr),
            Some(&true)
        );
    }

    #[test]
    fn test_recommendations_generation() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .code(
                "SQL",
                ScriptConfig {
                    runtime: "python".to_string(),
                    code: "SELECT * FROM users WHERE id = {{user_id}}".to_string(),
                    inputs: Vec::new(),
                    output: "result".to_string(),
                },
            )
            .end("End")
            .build();

        let scanner = SecurityScanner::default();
        let report = scanner.scan(&workflow);

        assert!(!report.recommendations.is_empty());
        assert!(report
            .recommendations
            .iter()
            .any(|r| r.contains("URGENT") || r.contains("critical")));
    }

    #[test]
    fn test_security_config_customization() {
        let config = SecurityConfig {
            check_prompt_injection: false,
            custom_secret_patterns: vec!["CUSTOM_SECRET".to_string()],
            ..Default::default()
        };

        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm(
                "LLM",
                LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    prompt_template: "{{user_input}} CUSTOM_SECRET=abc123".to_string(),
                    temperature: Some(0.7),
                    max_tokens: None,
                    system_prompt: None,
                    tools: Vec::new(),
                    images: Vec::new(),
                    extra_params: serde_json::Value::Null,
                },
            )
            .end("End")
            .build();

        let scanner = SecurityScanner::new(config);
        let report = scanner.scan(&workflow);

        // Should not detect prompt injection (disabled)
        let injection_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.id.starts_with("PROMPT_INJECTION"))
            .collect();
        assert!(injection_findings.is_empty());

        // Should detect custom secret pattern
        let custom_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.id == "SECRET_002")
            .collect();
        assert!(!custom_findings.is_empty());
    }

    #[test]
    fn test_xss_detection() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .code(
                "JS",
                ScriptConfig {
                    runtime: "javascript".to_string(),
                    code: "document.write({{user_content}})".to_string(),
                    inputs: Vec::new(),
                    output: "result".to_string(),
                },
            )
            .end("End")
            .build();

        let scanner = SecurityScanner::default();
        let report = scanner.scan(&workflow);

        let xss_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.category == ThreatCategory::Xss)
            .collect();

        assert!(!xss_findings.is_empty());
        assert_eq!(xss_findings[0].risk_level, RiskLevel::High);
    }
}
