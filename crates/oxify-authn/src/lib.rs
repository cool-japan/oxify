//! # `OxiFY` Authentication Module
//!
//! Ported from `OxiRS` (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) `OxiRS` Contributors
//! Adapted for `OxiFY`
//! License: MIT OR Apache-2.0 (compatible with `OxiRS`)
//!
//! This crate provides enterprise-grade authentication for `OxiFY`, including:
//! - JWT token management
//! - OAuth2/OIDC integration
//! - SAML authentication
//! - LDAP/Active Directory
//! - Password hashing and validation
//! - Multi-factor authentication (MFA)
//!
//! ## Example
//!
//! ```no_run
//! use oxify_authn::*;
//!
//! # fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
//! // Create JWT configuration
//! let config = JwtConfig::development();
//!
//! // Create JWT manager
//! let jwt_manager = JwtManager::new(&config)?;
//!
//! // Create a user
//! let user = User {
//!     username: "alice".to_string(),
//!     roles: vec!["admin".to_string()],
//!     email: Some("alice@example.com".to_string()),
//!     full_name: Some("Alice Wonderland".to_string()),
//!     last_login: None,
//!     permissions: vec![Permission::Admin],
//! };
//!
//! // Generate JWT token
//! let token = jwt_manager.generate_token(&user)?;
//!
//! // Validate token
//! let validation = jwt_manager.validate_token(&token)?;
//! assert_eq!(validation.user.username, "alice");
//! # Ok(())
//! # }
//! ```

// Clippy pedantic lint configuration
// These lints are allowed as "common allows" to maintain code clarity and API stability
#![allow(clippy::missing_errors_doc)] // Would require 90+ doc additions without significant value
#![allow(clippy::missing_panics_doc)] // Would require 36+ doc additions, panics are in error paths
#![allow(clippy::unused_async)] // Async trait methods must remain async for API consistency
#![allow(clippy::cast_possible_truncation)] // Intentional casts for time conversions (u64 to i64)
#![allow(clippy::cast_precision_loss)] // Acceptable precision loss for metrics (usize/u64 to f64)
#![allow(clippy::cast_sign_loss)] // Intentional for non-negative time calculations
#![allow(clippy::cast_possible_wrap)] // Time calculations are within valid i64 range
#![allow(clippy::struct_excessive_bools)] // Complex config structs need boolean flags
#![allow(clippy::too_many_lines)] // Some parsing functions are necessarily long

pub mod types;

#[cfg(feature = "jwt")]
pub mod jwt;

#[cfg(feature = "password")]
pub mod password;

#[cfg(feature = "oauth")]
pub mod oauth;

#[cfg(feature = "mfa")]
pub mod mfa;

#[cfg(feature = "session")]
pub mod session;

#[cfg(feature = "revocation")]
pub mod revocation;

#[cfg(feature = "ratelimit")]
pub mod ratelimit;

#[cfg(feature = "saml")]
pub mod saml;

#[cfg(feature = "ldap")]
pub mod ldap;

#[cfg(feature = "webauthn")]
pub mod webauthn;

#[cfg(feature = "risk")]
pub mod risk;

#[cfg(feature = "rotation")]
pub mod rotation;

#[cfg(feature = "apikey")]
pub mod apikey;

#[cfg(feature = "metrics")]
pub mod metrics;

#[cfg(feature = "cert")]
pub mod cert;

#[cfg(feature = "idp")]
pub mod idp;

#[cfg(feature = "ai")]
pub mod ai;

// Re-export commonly used types
pub use types::*;

#[cfg(feature = "jwt")]
pub use jwt::{ClaimsBuilder, JwtManager};

#[cfg(feature = "password")]
pub use password::{
    PasswordManager, PasswordPolicy, PasswordStrength, PolicyValidationResult, PolicyViolation,
};

#[cfg(feature = "oauth")]
pub use oauth::{OAuth2Service, OAuth2State, OAuth2Token, OIDCUserInfo};

#[cfg(feature = "mfa")]
pub use mfa::{TotpConfig, TotpEnrollment, TotpManager};

#[cfg(feature = "session")]
pub use session::{
    InMemorySessionStore, Session, SessionConfig, SessionConfigBuilder, SessionInfo,
    SessionManager, SessionStore,
};

#[cfg(feature = "revocation")]
pub use revocation::{
    InMemoryRevocationStore, RevocationConfig, RevocationConfigBuilder, RevocationEntry,
    RevocationManager, RevocationReason, RevocationStats, RevocationStore,
};

#[cfg(feature = "ratelimit")]
pub use ratelimit::{
    RateLimitConfig, RateLimitConfigBuilder, RateLimitResult, RateLimitStatus, RateLimiter,
    UserRateLimitStatus,
};

#[cfg(feature = "saml")]
pub use saml::{Assertion, AuthnRequest, SamlError, ServiceProvider, SpConfig, SpConfigBuilder};

#[cfg(feature = "ldap")]
pub use ldap::{LdapAuthenticator, LdapConfig, LdapError, LdapUser};

#[cfg(feature = "webauthn")]
pub use webauthn::{
    CredentialStore, InMemoryCredentialStore, StoredCredential, WebAuthnAuthenticator,
    WebAuthnConfig, WebAuthnConfigBuilder, WebAuthnError,
};

#[cfg(feature = "risk")]
pub use risk::{
    DeviceFingerprint, GeoLocation, LoginContext, LoginContextBuilder, RiskAnalyzer,
    RiskAssessment, RiskConfig, RiskLevel, RiskReason,
};

#[cfg(feature = "rotation")]
pub use rotation::{
    RefreshTokenMetadata, RotationConfig, RotationConfigBuilder, RotationManager, RotationStats,
};

#[cfg(feature = "apikey")]
pub use apikey::{
    ApiKeyConfig, ApiKeyManager, ApiKeyMetadata, ApiKeyScope, ApiKeyStats, ApiKeyValidation,
    GeneratedApiKey,
};

#[cfg(feature = "metrics")]
pub use metrics::{
    AuthEvent, AuthEventRecord, AuthMetrics, GeoDistribution, MetricsCollector, TimeSeriesPoint,
};

#[cfg(feature = "cert")]
pub use cert::{
    CertAuthenticator, CertConfig, CertConfigBuilder, CertError, CertValidation, RevocationStatus,
};

#[cfg(feature = "idp")]
pub use idp::{
    discover_oidc, IdpClient, IdpConfig, IdpConfigBuilder, IdpError, IdpProvider, IdpUserInfo,
    OidcDiscovery,
};

#[cfg(feature = "ai")]
pub use ai::{
    AiSecurityConfig, AiSecurityEngine, AiSecurityError, AiSecurityStats, AnomalyDetection,
    BehaviorProfile, LoginEvent, TrustLevel,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_types() {
        let user = User {
            username: "test".to_string(),
            roles: vec!["user".to_string()],
            email: None,
            full_name: None,
            last_login: None,
            permissions: vec![],
        };

        assert_eq!(user.username, "test");
    }
}
