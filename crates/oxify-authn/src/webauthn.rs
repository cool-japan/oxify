//! WebAuthn/FIDO2 passwordless authentication.
//!
//! This module provides `WebAuthn` (FIDO2) support for passwordless authentication,
//! enabling users to authenticate using biometric sensors, security keys, or platform authenticators.
//!
//! # Features
//! - Credential registration (for new devices/authenticators)
//! - Credential authentication (passwordless login)
//! - Platform authenticator support (Touch ID, Windows Hello, etc.)
//! - Cross-platform authenticator support (`YubiKey`, etc.)
//! - Multiple credentials per user
//! - Credential metadata tracking (device name, last used, etc.)
//!
//! # Example
//! ```no_run
//! use oxify_authn::webauthn::{WebAuthnAuthenticator, WebAuthnConfig};
//! use url::Url;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = WebAuthnConfig::builder()
//!         .rp_id("example.com")
//!         .rp_name("Example Corp")
//!         .origin(Url::parse("https://example.com")?)
//!         .build()?;
//!
//!     let authenticator = WebAuthnAuthenticator::new(&config)?;
//!
//!     // Registration: Start credential creation
//!     let (challenge, reg_state) = authenticator
//!         .start_registration("alice", "Alice Wonderland", false)
//!         .await?;
//!
//!     // Client sends challenge.public_key to browser
//!     // Browser calls navigator.credentials.create()
//!     // Client receives PublicKeyCredential and sends back
//!
//!     // Registration: Finish credential creation
//!     // let credential = authenticator
//!     //     .finish_registration(&public_key_credential, &reg_state)
//!     //     .await?;
//!
//!     // Authentication: Start login
//!     let (auth_challenge, auth_state) = authenticator
//!         .start_authentication("alice")
//!         .await?;
//!
//!     // Client sends auth_challenge.public_key to browser
//!     // Browser calls navigator.credentials.get()
//!     // Client receives PublicKeyCredential and sends back
//!
//!     // Authentication: Finish login
//!     // let result = authenticator
//!     //     .finish_authentication(&public_key_credential, &auth_state)
//!     //     .await?;
//!
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::*;
use webauthn_rs_proto::{AttestationConveyancePreference, UserVerificationPolicy};

/// WebAuthn-specific errors.
#[derive(Error, Debug)]
pub enum WebAuthnError {
    #[error("Invalid WebAuthn configuration: {0}")]
    InvalidConfig(String),

    #[error("WebAuthn initialization failed: {0}")]
    InitError(String),

    #[error("Registration failed: {0}")]
    RegistrationError(String),

    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Credential not found")]
    CredentialNotFound,

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

impl From<WebauthnError> for WebAuthnError {
    fn from(err: WebauthnError) -> Self {
        Self::RegistrationError(err.to_string())
    }
}

/// `WebAuthn` configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAuthnConfig {
    /// Relying Party ID (typically the domain, e.g., "example.com")
    pub rp_id: String,

    /// Relying Party name (human-readable, e.g., "Example Corp")
    pub rp_name: String,

    /// Origin URL (e.g., "<https://example.com>")
    pub origin: Url,

    /// Allowed origins (for multi-domain support)
    pub allowed_origins: Vec<Url>,

    /// Timeout for credential operations in milliseconds
    pub timeout_ms: u32,

    /// Require user verification (biometric, PIN, etc.)
    pub user_verification: UserVerificationPolicy,

    /// Attestation conveyance preference
    pub attestation: AttestationConveyancePreference,
}

impl WebAuthnConfig {
    /// Creates a new builder for `WebAuthnConfig`.
    #[must_use]
    pub fn builder() -> WebAuthnConfigBuilder {
        WebAuthnConfigBuilder::default()
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), WebAuthnError> {
        if self.rp_id.is_empty() {
            return Err(WebAuthnError::InvalidConfig("rp_id is required".into()));
        }
        if self.rp_name.is_empty() {
            return Err(WebAuthnError::InvalidConfig("rp_name is required".into()));
        }

        Ok(())
    }
}

/// Builder for `WebAuthnConfig`.
#[derive(Default)]
pub struct WebAuthnConfigBuilder {
    rp_id: Option<String>,
    rp_name: Option<String>,
    origin: Option<Url>,
    allowed_origins: Vec<Url>,
    timeout_ms: u32,
    user_verification: UserVerificationPolicy,
    attestation: AttestationConveyancePreference,
}

impl WebAuthnConfigBuilder {
    #[must_use]
    pub fn rp_id(mut self, rp_id: impl Into<String>) -> Self {
        self.rp_id = Some(rp_id.into());
        self
    }

    #[must_use]
    pub fn rp_name(mut self, rp_name: impl Into<String>) -> Self {
        self.rp_name = Some(rp_name.into());
        self
    }

    #[must_use]
    pub fn origin(mut self, origin: Url) -> Self {
        self.origin = Some(origin);
        self
    }

    #[must_use]
    pub fn allowed_origins(mut self, origins: Vec<Url>) -> Self {
        self.allowed_origins = origins;
        self
    }

    #[must_use]
    pub fn add_origin(mut self, origin: Url) -> Self {
        self.allowed_origins.push(origin);
        self
    }

    #[must_use]
    pub fn timeout_ms(mut self, timeout: u32) -> Self {
        self.timeout_ms = timeout;
        self
    }

    #[must_use]
    pub fn user_verification(mut self, policy: UserVerificationPolicy) -> Self {
        self.user_verification = policy;
        self
    }

    #[must_use]
    pub fn attestation(mut self, attestation: AttestationConveyancePreference) -> Self {
        self.attestation = attestation;
        self
    }

    pub fn build(self) -> Result<WebAuthnConfig, WebAuthnError> {
        let rp_id = self
            .rp_id
            .ok_or_else(|| WebAuthnError::InvalidConfig("rp_id is required".into()))?;
        let rp_name = self
            .rp_name
            .ok_or_else(|| WebAuthnError::InvalidConfig("rp_name is required".into()))?;
        let origin = self
            .origin
            .ok_or_else(|| WebAuthnError::InvalidConfig("origin is required".into()))?;

        let config = WebAuthnConfig {
            rp_id,
            rp_name,
            origin,
            allowed_origins: self.allowed_origins,
            timeout_ms: if self.timeout_ms > 0 {
                self.timeout_ms
            } else {
                60000 // Default 60 seconds
            },
            user_verification: self.user_verification,
            attestation: self.attestation,
        };

        config.validate()?;
        Ok(config)
    }
}

/// Stored credential information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredential {
    /// Credential ID
    pub credential_id: Vec<u8>,

    /// User ID (username)
    pub user_id: String,

    /// Public key
    pub public_key: Passkey,

    /// Counter for replay protection
    pub counter: u32,

    /// Device name (optional, user-provided)
    pub device_name: Option<String>,

    /// Credential created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Last used timestamp
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,

    /// Is this a platform authenticator (Touch ID, Windows Hello) or cross-platform (`YubiKey`)
    pub is_platform: bool,
}

/// Trait for storing `WebAuthn` credentials.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    /// Stores a new credential.
    async fn store_credential(&self, credential: StoredCredential) -> Result<(), WebAuthnError>;

    /// Retrieves all credentials for a user.
    async fn get_user_credentials(
        &self,
        user_id: &str,
    ) -> Result<Vec<StoredCredential>, WebAuthnError>;

    /// Updates a credential (for counter updates after successful authentication).
    async fn update_credential(&self, credential: StoredCredential) -> Result<(), WebAuthnError>;

    /// Deletes a credential.
    async fn delete_credential(
        &self,
        user_id: &str,
        credential_id: &[u8],
    ) -> Result<(), WebAuthnError>;

    /// Gets a specific credential by ID.
    async fn get_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<StoredCredential>, WebAuthnError>;
}

/// In-memory credential store (for testing/development).
pub struct InMemoryCredentialStore {
    credentials: Arc<RwLock<HashMap<String, Vec<StoredCredential>>>>,
}

impl InMemoryCredentialStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            credentials: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialStore for InMemoryCredentialStore {
    async fn store_credential(&self, credential: StoredCredential) -> Result<(), WebAuthnError> {
        let mut creds = self
            .credentials
            .write()
            .map_err(|e| WebAuthnError::StorageError(format!("Lock poisoned: {e}")))?;

        creds
            .entry(credential.user_id.clone())
            .or_insert_with(Vec::new)
            .push(credential);

        Ok(())
    }

    async fn get_user_credentials(
        &self,
        user_id: &str,
    ) -> Result<Vec<StoredCredential>, WebAuthnError> {
        let creds = self
            .credentials
            .read()
            .map_err(|e| WebAuthnError::StorageError(format!("Lock poisoned: {e}")))?;

        Ok(creds.get(user_id).cloned().unwrap_or_default())
    }

    async fn update_credential(&self, credential: StoredCredential) -> Result<(), WebAuthnError> {
        let mut creds = self
            .credentials
            .write()
            .map_err(|e| WebAuthnError::StorageError(format!("Lock poisoned: {e}")))?;

        if let Some(user_creds) = creds.get_mut(&credential.user_id) {
            if let Some(stored) = user_creds
                .iter_mut()
                .find(|c| c.credential_id == credential.credential_id)
            {
                *stored = credential;
                return Ok(());
            }
        }

        Err(WebAuthnError::CredentialNotFound)
    }

    async fn delete_credential(
        &self,
        user_id: &str,
        credential_id: &[u8],
    ) -> Result<(), WebAuthnError> {
        let mut creds = self
            .credentials
            .write()
            .map_err(|e| WebAuthnError::StorageError(format!("Lock poisoned: {e}")))?;

        if let Some(user_creds) = creds.get_mut(user_id) {
            user_creds.retain(|c| c.credential_id != credential_id);
            Ok(())
        } else {
            Err(WebAuthnError::UserNotFound(user_id.to_string()))
        }
    }

    async fn get_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<StoredCredential>, WebAuthnError> {
        let creds = self
            .credentials
            .read()
            .map_err(|e| WebAuthnError::StorageError(format!("Lock poisoned: {e}")))?;

        for user_creds in creds.values() {
            if let Some(cred) = user_creds.iter().find(|c| c.credential_id == credential_id) {
                return Ok(Some(cred.clone()));
            }
        }

        Ok(None)
    }
}

/// `WebAuthn` authenticator for credential registration and authentication.
pub struct WebAuthnAuthenticator<S: CredentialStore> {
    webauthn: Webauthn,
    store: Arc<S>,
}

impl<S: CredentialStore> WebAuthnAuthenticator<S> {
    /// Creates a new `WebAuthn` authenticator with the given configuration and store.
    pub fn new_with_store(config: &WebAuthnConfig, store: S) -> Result<Self, WebAuthnError> {
        let rp_id = config.rp_id.clone();
        let origin = config.origin.clone();

        let mut builder = WebauthnBuilder::new(&rp_id, &origin)
            .map_err(|e| WebAuthnError::InitError(format!("WebauthnBuilder failed: {e}")))?;

        builder = builder.rp_name(&config.rp_name);

        for origin in &config.allowed_origins {
            builder = builder.append_allowed_origin(origin);
        }

        let webauthn = builder
            .build()
            .map_err(|e| WebAuthnError::InitError(format!("Webauthn build failed: {e}")))?;

        Ok(Self {
            webauthn,
            store: Arc::new(store),
        })
    }

    /// Starts the registration process for a new credential.
    ///
    /// Returns the challenge to send to the client and the registration state to persist.
    pub async fn start_registration(
        &self,
        user_id: &str,
        user_display_name: &str,
        _require_resident_key: bool,
    ) -> Result<(CreationChallengeResponse, PasskeyRegistration), WebAuthnError> {
        // Get existing credentials for this user to exclude them
        let existing_creds = self.store.get_user_credentials(user_id).await?;
        let exclude_credentials: Vec<CredentialID> = existing_creds
            .iter()
            .map(|c| CredentialID::from(c.credential_id.clone()))
            .collect();

        let (challenge, reg_state) = self
            .webauthn
            .start_passkey_registration(
                Uuid::new_v4(),
                user_id,
                user_display_name,
                Some(exclude_credentials),
            )
            .map_err(|e| {
                WebAuthnError::RegistrationError(format!("Failed to start registration: {e}"))
            })?;

        Ok((challenge, reg_state))
    }

    /// Finishes the registration process.
    ///
    /// Validates the credential and stores it.
    pub async fn finish_registration(
        &self,
        credential: &RegisterPublicKeyCredential,
        reg_state: &PasskeyRegistration,
        user_id: &str,
    ) -> Result<StoredCredential, WebAuthnError> {
        let passkey = self
            .webauthn
            .finish_passkey_registration(credential, reg_state)
            .map_err(|e| {
                WebAuthnError::RegistrationError(format!("Failed to finish registration: {e}"))
            })?;

        let stored_credential = StoredCredential {
            credential_id: passkey.cred_id().clone().into(),
            user_id: user_id.to_string(),
            public_key: passkey.clone(),
            counter: 0,
            device_name: None,
            created_at: chrono::Utc::now(),
            last_used: None,
            is_platform: false, // Can be updated by client
        };

        self.store
            .store_credential(stored_credential.clone())
            .await?;

        Ok(stored_credential)
    }

    /// Starts the authentication process.
    ///
    /// Returns the challenge to send to the client and the authentication state.
    pub async fn start_authentication(
        &self,
        user_id: &str,
    ) -> Result<(RequestChallengeResponse, PasskeyAuthentication), WebAuthnError> {
        let user_creds = self.store.get_user_credentials(user_id).await?;

        if user_creds.is_empty() {
            return Err(WebAuthnError::UserNotFound(user_id.to_string()));
        }

        let allowed_credentials: Vec<Passkey> =
            user_creds.iter().map(|c| c.public_key.clone()).collect();

        let (challenge, auth_state) = self
            .webauthn
            .start_passkey_authentication(&allowed_credentials)
            .map_err(|e| {
                WebAuthnError::AuthenticationError(format!("Failed to start authentication: {e}"))
            })?;

        Ok((challenge, auth_state))
    }

    /// Finishes the authentication process.
    ///
    /// Validates the credential response and updates the counter.
    pub async fn finish_authentication(
        &self,
        credential: &PublicKeyCredential,
        auth_state: &PasskeyAuthentication,
    ) -> Result<StoredCredential, WebAuthnError> {
        let auth_result = self
            .webauthn
            .finish_passkey_authentication(credential, auth_state)
            .map_err(|e| {
                WebAuthnError::AuthenticationError(format!("Failed to finish authentication: {e}"))
            })?;

        // Update counter and last_used
        let credential_id = auth_result.cred_id();
        let mut stored_cred = self
            .store
            .get_credential(credential_id)
            .await?
            .ok_or(WebAuthnError::CredentialNotFound)?;

        // Update counter (incremented by authenticator)
        stored_cred.counter = stored_cred.counter.saturating_add(1);
        stored_cred.last_used = Some(chrono::Utc::now());

        self.store.update_credential(stored_cred.clone()).await?;

        Ok(stored_cred)
    }

    /// Gets all credentials for a user.
    pub async fn get_user_credentials(
        &self,
        user_id: &str,
    ) -> Result<Vec<StoredCredential>, WebAuthnError> {
        self.store.get_user_credentials(user_id).await
    }

    /// Deletes a credential.
    pub async fn delete_credential(
        &self,
        user_id: &str,
        credential_id: &[u8],
    ) -> Result<(), WebAuthnError> {
        self.store.delete_credential(user_id, credential_id).await
    }
}

impl WebAuthnAuthenticator<InMemoryCredentialStore> {
    /// Creates a new `WebAuthn` authenticator with in-memory storage.
    pub fn new(config: &WebAuthnConfig) -> Result<Self, WebAuthnError> {
        Self::new_with_store(config, InMemoryCredentialStore::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webauthn_config_builder() {
        let config = WebAuthnConfig::builder()
            .rp_id("example.com")
            .rp_name("Example Corp")
            .origin(Url::parse("https://example.com").unwrap())
            .timeout_ms(60000)
            .user_verification(UserVerificationPolicy::Preferred)
            .build()
            .unwrap();

        assert_eq!(config.rp_id, "example.com");
        assert_eq!(config.rp_name, "Example Corp");
        assert_eq!(config.timeout_ms, 60000);
    }

    #[test]
    fn test_webauthn_config_validation() {
        let result = WebAuthnConfig::builder()
            .rp_name("Example Corp")
            .origin(Url::parse("https://example.com").unwrap())
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_in_memory_store_creation() {
        let store = InMemoryCredentialStore::new();
        assert!(Arc::strong_count(&store.credentials) > 0);
    }

    #[test]
    fn test_webauthn_authenticator_creation() {
        let config = WebAuthnConfig::builder()
            .rp_id("example.com")
            .rp_name("Example Corp")
            .origin(Url::parse("https://example.com").unwrap())
            .build()
            .unwrap();

        let result = WebAuthnAuthenticator::new(&config);
        assert!(result.is_ok());
    }
}
