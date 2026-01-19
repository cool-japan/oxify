//! Authentication types and data structures
//!
//! Ported from `OxiRS` (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) `OxiRS` Contributors
//! Adapted for `OxiFY`
//! License: MIT OR Apache-2.0 (compatible with `OxiRS`)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Authentication result
#[derive(Debug, Clone)]
pub enum AuthResult {
    Authenticated(User),
    Unauthenticated,
    Forbidden,
    Expired,
    Invalid,
    Locked,
    MfaRequired,
}

/// Authenticated user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub roles: Vec<String>,
    pub email: Option<String>,
    pub full_name: Option<String>,
    pub last_login: Option<DateTime<Utc>>,
    pub permissions: Vec<Permission>,
}

/// Permission types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Read,
    Write,
    Admin,
    GlobalAdmin,
    GlobalRead,
    GlobalWrite,
    DatasetCreate,
    DatasetDelete,
    DatasetManage,
    DatasetRead(String),
    DatasetWrite(String),
    DatasetAdmin(String),
    UserManage,
    SystemConfig,
    SystemMetrics,
    QueryExecute,
    UpdateExecute,
    Monitor,
    Audit,
}

/// JWT token claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user identifier)
    pub sub: String,
    /// User roles
    pub roles: Vec<String>,
    /// User permissions
    pub permissions: Vec<Permission>,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// Not before (Unix timestamp)
    pub nbf: i64,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// JWT ID (unique identifier for revocation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    /// User email (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Token type (access, refresh, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
}

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub mfa_token: Option<String>,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User,
    pub mfa_required: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub message: String,
}

/// Token validation result
#[derive(Debug)]
pub struct TokenValidation {
    pub user: User,
    pub expires_at: DateTime<Utc>,
}

/// Authentication errors
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Token revoked")]
    TokenRevoked,

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("MFA required")]
    MfaRequired,

    #[error("Invalid MFA token")]
    InvalidMfaToken,

    #[error("User not found")]
    UserNotFound,

    #[error("User disabled")]
    UserDisabled,

    #[error("User locked")]
    UserLocked,

    #[error("Permission denied")]
    PermissionDenied,

    #[error("SAML error: {0}")]
    SamlError(String),

    #[error("LDAP error: {0}")]
    LdapError(String),

    #[error("OAuth error: {0}")]
    OAuthError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type Result<T> = std::result::Result<T, AuthError>;

/// JWT algorithm types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum JwtAlgorithm {
    /// HMAC with SHA-256 (symmetric)
    #[default]
    HS256,
    /// HMAC with SHA-384 (symmetric)
    HS384,
    /// HMAC with SHA-512 (symmetric)
    HS512,
    /// RSA with SHA-256 (asymmetric)
    RS256,
    /// RSA with SHA-384 (asymmetric)
    RS384,
    /// RSA with SHA-512 (asymmetric)
    RS512,
    /// ECDSA with SHA-256 (asymmetric)
    ES256,
    /// ECDSA with SHA-384 (asymmetric)
    ES384,
}

/// JWT key configuration
#[derive(Debug, Clone)]
pub enum JwtKeyConfig {
    /// Symmetric secret (for HS256/384/512)
    Secret(String),
    /// RSA key pair (PEM format)
    Rsa {
        private_key: Option<String>,
        public_key: String,
    },
    /// EC key pair (PEM format)
    Ec {
        private_key: Option<String>,
        public_key: String,
    },
}

/// JWT configuration
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// Secret or key material
    pub secret: String,
    /// Token issuer
    pub issuer: String,
    /// Token audience
    pub audience: String,
    /// Access token expiration in seconds
    pub expiration_secs: u64,
    /// Refresh token expiration in seconds
    pub refresh_expiration_secs: u64,
    /// Algorithm to use
    pub algorithm: JwtAlgorithm,
    /// Key configuration (for asymmetric algorithms)
    pub key_config: Option<JwtKeyConfig>,
    /// Include JWT ID (jti) claim
    pub include_jti: bool,
}

impl JwtConfig {
    /// Create a new JWT configuration with symmetric secret
    pub fn new(
        secret: impl Into<String>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
        expiration_secs: u64,
    ) -> Self {
        Self {
            secret: secret.into(),
            issuer: issuer.into(),
            audience: audience.into(),
            expiration_secs,
            refresh_expiration_secs: 86400 * 30, // 30 days
            algorithm: JwtAlgorithm::HS256,
            key_config: None,
            include_jti: false,
        }
    }

    /// Default configuration for development
    #[must_use]
    pub fn development() -> Self {
        Self {
            secret: "dev-secret-change-in-production".to_string(),
            issuer: "oxify-dev".to_string(),
            audience: "oxify-api".to_string(),
            expiration_secs: 3600,               // 1 hour
            refresh_expiration_secs: 86400 * 30, // 30 days
            algorithm: JwtAlgorithm::HS256,
            key_config: None,
            include_jti: true,
        }
    }

    /// Create configuration with RS256
    pub fn with_rsa(
        private_key: impl Into<String>,
        public_key: impl Into<String>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
        expiration_secs: u64,
    ) -> Self {
        Self {
            secret: String::new(),
            issuer: issuer.into(),
            audience: audience.into(),
            expiration_secs,
            refresh_expiration_secs: 86400 * 30,
            algorithm: JwtAlgorithm::RS256,
            key_config: Some(JwtKeyConfig::Rsa {
                private_key: Some(private_key.into()),
                public_key: public_key.into(),
            }),
            include_jti: true,
        }
    }

    /// Create configuration for verification only (public key only)
    pub fn verification_only_rsa(
        public_key: impl Into<String>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        Self {
            secret: String::new(),
            issuer: issuer.into(),
            audience: audience.into(),
            expiration_secs: 3600,
            refresh_expiration_secs: 86400 * 30,
            algorithm: JwtAlgorithm::RS256,
            key_config: Some(JwtKeyConfig::Rsa {
                private_key: None,
                public_key: public_key.into(),
            }),
            include_jti: false,
        }
    }

    /// Set algorithm
    #[must_use]
    pub fn with_algorithm(mut self, algorithm: JwtAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Enable JWT ID generation
    #[must_use]
    pub fn with_jti(mut self, include_jti: bool) -> Self {
        self.include_jti = include_jti;
        self
    }

    /// Set refresh token expiration
    #[must_use]
    pub fn with_refresh_expiration(mut self, secs: u64) -> Self {
        self.refresh_expiration_secs = secs;
        self
    }
}

/// `OAuth2` configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Config {
    pub provider: String,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub user_info_url: String,
    pub scopes: Vec<String>,
}

impl OAuth2Config {
    /// Create a new `OAuth2` configuration
    pub fn new(
        provider: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        auth_url: impl Into<String>,
        token_url: impl Into<String>,
        user_info_url: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            auth_url: auth_url.into(),
            token_url: token_url.into(),
            user_info_url: user_info_url.into(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
        }
    }

    /// Create GitHub OAuth configuration
    pub fn github(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self::new(
            "github",
            client_id,
            client_secret,
            "https://github.com/login/oauth/authorize",
            "https://github.com/login/oauth/access_token",
            "https://api.github.com/user",
        )
    }

    /// Create Google OAuth configuration
    pub fn google(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self::new(
            "google",
            client_id,
            client_secret,
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://oauth2.googleapis.com/token",
            "https://www.googleapis.com/oauth2/v2/userinfo",
        )
    }
}

// ============================================================================
// Audit Events
// ============================================================================

/// Authentication audit event types for logging and compliance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    // Login events
    /// Successful login
    LoginSuccess,
    /// Failed login attempt
    LoginFailure,
    /// User logout
    Logout,
    /// Session expired
    SessionExpired,

    // MFA events
    /// MFA enrollment started
    MfaEnrollmentStarted,
    /// MFA enrollment completed
    MfaEnrollmentCompleted,
    /// MFA challenge sent
    MfaChallengeSent,
    /// MFA verification succeeded
    MfaVerificationSuccess,
    /// MFA verification failed
    MfaVerificationFailure,
    /// Backup code used
    MfaBackupCodeUsed,

    // Password events
    /// Password changed
    PasswordChanged,
    /// Password reset requested
    PasswordResetRequested,
    /// Password reset completed
    PasswordResetCompleted,

    // Token events
    /// Access token issued
    TokenIssued,
    /// Token refreshed
    TokenRefreshed,
    /// Token revoked
    TokenRevoked,
    /// Token validation failed
    TokenValidationFailed,

    // Account events
    /// Account created
    AccountCreated,
    /// Account updated
    AccountUpdated,
    /// Account disabled
    AccountDisabled,
    /// Account enabled
    AccountEnabled,
    /// Account deleted
    AccountDeleted,
    /// Account locked due to failed attempts
    AccountLocked,
    /// Account unlocked
    AccountUnlocked,

    // OAuth events
    /// OAuth authorization started
    OAuthAuthorizationStarted,
    /// OAuth callback received
    OAuthCallbackReceived,
    /// OAuth token exchange completed
    OAuthTokenExchanged,

    // Security events
    /// Suspicious activity detected
    SuspiciousActivityDetected,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Permission denied
    PermissionDenied,
    /// IP blocked
    IpBlocked,

    // Session events
    /// Session created
    SessionCreated,
    /// Session invalidated
    SessionInvalidated,
    /// All sessions invalidated (force logout)
    AllSessionsInvalidated,

    // Admin events
    /// Admin action performed
    AdminAction(String),
}

/// Audit event with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Event type
    pub event_type: AuditEventType,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// User ID (if applicable)
    pub user_id: Option<String>,
    /// IP address
    pub ip_address: Option<String>,
    /// User agent
    pub user_agent: Option<String>,
    /// Session ID (if applicable)
    pub session_id: Option<String>,
    /// Request ID for tracing
    pub request_id: Option<String>,
    /// Success indicator
    pub success: bool,
    /// Failure reason (if failed)
    pub failure_reason: Option<String>,
    /// Additional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl AuditEvent {
    /// Create a new audit event
    #[must_use]
    pub fn new(event_type: AuditEventType) -> Self {
        Self {
            event_type,
            timestamp: Utc::now(),
            user_id: None,
            ip_address: None,
            user_agent: None,
            session_id: None,
            request_id: None,
            success: true,
            failure_reason: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Set user ID
    #[must_use]
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set IP address
    #[must_use]
    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    /// Set user agent
    #[must_use]
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Set session ID
    #[must_use]
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set request ID
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Mark as failed
    #[must_use]
    pub fn failed(mut self, reason: impl Into<String>) -> Self {
        self.success = false;
        self.failure_reason = Some(reason.into());
        self
    }

    /// Add metadata
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Create a login success event
    pub fn login_success(user_id: impl Into<String>) -> Self {
        Self::new(AuditEventType::LoginSuccess).with_user(user_id)
    }

    /// Create a login failure event
    pub fn login_failure(username: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(AuditEventType::LoginFailure)
            .with_metadata("username", username)
            .failed(reason)
    }

    /// Create a logout event
    pub fn logout(user_id: impl Into<String>) -> Self {
        Self::new(AuditEventType::Logout).with_user(user_id)
    }

    /// Create a token issued event
    pub fn token_issued(user_id: impl Into<String>, token_type: impl Into<String>) -> Self {
        Self::new(AuditEventType::TokenIssued)
            .with_user(user_id)
            .with_metadata("token_type", token_type)
    }

    /// Create a password changed event
    pub fn password_changed(user_id: impl Into<String>) -> Self {
        Self::new(AuditEventType::PasswordChanged).with_user(user_id)
    }

    /// Create an MFA verification event
    pub fn mfa_verification(user_id: impl Into<String>, success: bool) -> Self {
        let event_type = if success {
            AuditEventType::MfaVerificationSuccess
        } else {
            AuditEventType::MfaVerificationFailure
        };
        let event = Self::new(event_type).with_user(user_id);
        if success {
            event
        } else {
            event.failed("MFA verification failed")
        }
    }

    /// Create a rate limit exceeded event
    pub fn rate_limit_exceeded(ip: impl Into<String>) -> Self {
        Self::new(AuditEventType::RateLimitExceeded)
            .with_ip(ip)
            .failed("Rate limit exceeded")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User {
            username: "alice".to_string(),
            roles: vec!["admin".to_string()],
            email: Some("alice@example.com".to_string()),
            full_name: Some("Alice Wonderland".to_string()),
            last_login: None,
            permissions: vec![Permission::Admin, Permission::Read],
        };

        assert_eq!(user.username, "alice");
        assert_eq!(user.roles.len(), 1);
        assert_eq!(user.permissions.len(), 2);
    }

    #[test]
    fn test_jwt_config_development() {
        let config = JwtConfig::development();
        assert_eq!(config.issuer, "oxify-dev");
        assert_eq!(config.expiration_secs, 3600);
    }

    #[test]
    fn test_permission_ordering() {
        let mut perms = vec![Permission::Write, Permission::Admin, Permission::Read];
        perms.sort();
        // Permissions sort by variant order, not alphabetically
        assert_eq!(
            perms,
            vec![Permission::Read, Permission::Write, Permission::Admin]
        );
    }

    #[test]
    fn test_audit_event_login_success() {
        let event = AuditEvent::login_success("user123")
            .with_ip("192.168.1.1")
            .with_user_agent("Mozilla/5.0");

        assert!(matches!(event.event_type, AuditEventType::LoginSuccess));
        assert_eq!(event.user_id, Some("user123".to_string()));
        assert_eq!(event.ip_address, Some("192.168.1.1".to_string()));
        assert!(event.success);
    }

    #[test]
    fn test_audit_event_login_failure() {
        let event = AuditEvent::login_failure("baduser", "Invalid credentials");

        assert!(matches!(event.event_type, AuditEventType::LoginFailure));
        assert!(!event.success);
        assert_eq!(
            event.failure_reason,
            Some("Invalid credentials".to_string())
        );
        assert_eq!(event.metadata.get("username"), Some(&"baduser".to_string()));
    }

    #[test]
    fn test_audit_event_builder() {
        let event = AuditEvent::new(AuditEventType::TokenIssued)
            .with_user("user123")
            .with_ip("10.0.0.1")
            .with_session("session-abc")
            .with_request_id("req-123")
            .with_metadata("token_type", "access");

        assert_eq!(event.user_id, Some("user123".to_string()));
        assert_eq!(event.ip_address, Some("10.0.0.1".to_string()));
        assert_eq!(event.session_id, Some("session-abc".to_string()));
        assert_eq!(event.request_id, Some("req-123".to_string()));
        assert_eq!(
            event.metadata.get("token_type"),
            Some(&"access".to_string())
        );
    }
}
