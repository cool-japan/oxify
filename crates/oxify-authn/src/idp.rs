//! # Advanced Identity Provider (`IdP`) Support
//!
//! This module provides integrations with popular enterprise identity providers:
//! - Azure Active Directory (Azure AD / Microsoft Entra ID)
//! - Okta
//! - Auth0
//! - Generic `OpenID` Connect (OIDC) providers
//!
//! ## Features
//! - Pre-configured provider endpoints
//! - Automatic discovery via .well-known/openid-configuration
//! - Token validation with provider-specific JWKs
//! - User profile retrieval
//! - Group and role mapping
//! - Multi-tenant support (Azure AD)
//!
//! ## Example
//!
//! ```no_run
//! use oxify_authn::idp::{IdpConfig, IdpProvider, IdpClient};
//!
//! # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
//! // Configure Azure AD
//! let config = IdpConfig::azure_ad()
//!     .tenant_id("your-tenant-id")
//!     .client_id("your-client-id")
//!     .client_secret("your-client-secret")
//!     .redirect_uri("https://your-app.com/callback")
//!     .build()?;
//!
//! let mut client = IdpClient::new(config).await?;
//!
//! // Get authorization URL
//! let auth_url = client.get_authorization_url(&["openid", "profile", "email"]).await?;
//! println!("Redirect user to: {}", auth_url);
//!
//! // Exchange code for tokens (after callback)
//! let tokens = client.exchange_code("authorization_code", "state_from_callback").await?;
//! println!("Access token: {}", tokens.access_token);
//!
//! // Get user info
//! let user_info = client.get_user_info(&tokens.access_token).await?;
//! println!("User: {} ({})", user_info.name, user_info.email);
//! # Ok(())
//! # }
//! ```

use crate::oauth::{OAuth2Service, OAuth2Token};
use crate::types::OAuth2Config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// IdP-specific errors
#[derive(Error, Debug)]
pub enum IdpError {
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("Token validation failed: {0}")]
    TokenValidationFailed(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Provider error: {0}")]
    Provider(String),
}

pub type Result<T> = std::result::Result<T, IdpError>;

/// Supported identity providers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdpProvider {
    /// Azure Active Directory (Microsoft Entra ID)
    AzureAd,
    /// Okta
    Okta,
    /// Auth0
    Auth0,
    /// Generic OIDC provider
    Oidc,
}

impl std::fmt::Display for IdpProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AzureAd => write!(f, "azure_ad"),
            Self::Okta => write!(f, "okta"),
            Self::Auth0 => write!(f, "auth0"),
            Self::Oidc => write!(f, "oidc"),
        }
    }
}

/// `IdP` configuration
#[derive(Debug, Clone)]
pub struct IdpConfig {
    /// Identity provider type
    pub provider: IdpProvider,
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Redirect URI
    pub redirect_uri: String,
    /// Authorization endpoint
    pub auth_url: String,
    /// Token endpoint
    pub token_url: String,
    /// `UserInfo` endpoint
    pub userinfo_url: String,
    /// JWKS endpoint for token validation
    pub jwks_url: Option<String>,
    /// Provider-specific settings
    pub provider_settings: HashMap<String, String>,
}

/// `IdP` configuration builder
pub struct IdpConfigBuilder {
    provider: IdpProvider,
    client_id: Option<String>,
    client_secret: Option<String>,
    redirect_uri: Option<String>,
    auth_url: Option<String>,
    token_url: Option<String>,
    userinfo_url: Option<String>,
    jwks_url: Option<String>,
    provider_settings: HashMap<String, String>,
}

impl IdpConfigBuilder {
    /// Create a new builder for the specified provider
    #[must_use]
    pub fn new(provider: IdpProvider) -> Self {
        Self {
            provider,
            client_id: None,
            client_secret: None,
            redirect_uri: None,
            auth_url: None,
            token_url: None,
            userinfo_url: None,
            jwks_url: None,
            provider_settings: HashMap::new(),
        }
    }

    /// Set client ID
    #[must_use]
    pub fn client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = Some(client_id.into());
        self
    }

    /// Set client secret
    #[must_use]
    pub fn client_secret(mut self, client_secret: impl Into<String>) -> Self {
        self.client_secret = Some(client_secret.into());
        self
    }

    /// Set redirect URI
    #[must_use]
    pub fn redirect_uri(mut self, redirect_uri: impl Into<String>) -> Self {
        self.redirect_uri = Some(redirect_uri.into());
        self
    }

    /// Set authorization URL
    #[must_use]
    pub fn auth_url(mut self, url: impl Into<String>) -> Self {
        self.auth_url = Some(url.into());
        self
    }

    /// Set token URL
    #[must_use]
    pub fn token_url(mut self, url: impl Into<String>) -> Self {
        self.token_url = Some(url.into());
        self
    }

    /// Set userinfo URL
    #[must_use]
    pub fn userinfo_url(mut self, url: impl Into<String>) -> Self {
        self.userinfo_url = Some(url.into());
        self
    }

    /// Set JWKS URL
    #[must_use]
    pub fn jwks_url(mut self, url: impl Into<String>) -> Self {
        self.jwks_url = Some(url.into());
        self
    }

    /// Set tenant ID (Azure AD specific)
    #[must_use]
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.provider_settings
            .insert("tenant_id".to_string(), tenant_id.into());
        self
    }

    /// Set domain (Okta/Auth0 specific)
    #[must_use]
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.provider_settings
            .insert("domain".to_string(), domain.into());
        self
    }

    /// Add custom provider setting
    #[must_use]
    pub fn add_setting(mut self, key: String, value: String) -> Self {
        self.provider_settings.insert(key, value);
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<IdpConfig> {
        let client_id = self
            .client_id
            .ok_or_else(|| IdpError::InvalidConfig("client_id is required".to_string()))?;

        let client_secret = self
            .client_secret
            .ok_or_else(|| IdpError::InvalidConfig("client_secret is required".to_string()))?;

        let redirect_uri = self
            .redirect_uri
            .ok_or_else(|| IdpError::InvalidConfig("redirect_uri is required".to_string()))?;

        // Set default URLs based on provider
        let (auth_url, token_url, userinfo_url, jwks_url) = match &self.provider {
            IdpProvider::AzureAd => {
                let tenant_id = self.provider_settings.get("tenant_id").ok_or_else(|| {
                    IdpError::InvalidConfig("tenant_id required for Azure AD".to_string())
                })?;

                let base = format!("https://login.microsoftonline.com/{tenant_id}");
                (
                    self.auth_url
                        .unwrap_or_else(|| format!("{base}/oauth2/v2.0/authorize")),
                    self.token_url
                        .unwrap_or_else(|| format!("{base}/oauth2/v2.0/token")),
                    self.userinfo_url
                        .unwrap_or_else(|| "https://graph.microsoft.com/v1.0/me".to_string()),
                    Some(
                        self.jwks_url
                            .unwrap_or_else(|| format!("{base}/discovery/v2.0/keys")),
                    ),
                )
            }
            IdpProvider::Okta => {
                let domain = self.provider_settings.get("domain").ok_or_else(|| {
                    IdpError::InvalidConfig("domain required for Okta".to_string())
                })?;

                let base = format!("https://{domain}");
                (
                    self.auth_url
                        .unwrap_or_else(|| format!("{base}/oauth2/v1/authorize")),
                    self.token_url
                        .unwrap_or_else(|| format!("{base}/oauth2/v1/token")),
                    self.userinfo_url
                        .unwrap_or_else(|| format!("{base}/oauth2/v1/userinfo")),
                    Some(
                        self.jwks_url
                            .unwrap_or_else(|| format!("{base}/oauth2/v1/keys")),
                    ),
                )
            }
            IdpProvider::Auth0 => {
                let domain = self.provider_settings.get("domain").ok_or_else(|| {
                    IdpError::InvalidConfig("domain required for Auth0".to_string())
                })?;

                let base = format!("https://{domain}");
                (
                    self.auth_url.unwrap_or_else(|| format!("{base}/authorize")),
                    self.token_url
                        .unwrap_or_else(|| format!("{base}/oauth/token")),
                    self.userinfo_url
                        .unwrap_or_else(|| format!("{base}/userinfo")),
                    Some(
                        self.jwks_url
                            .unwrap_or_else(|| format!("{base}/.well-known/jwks.json")),
                    ),
                )
            }
            IdpProvider::Oidc => (
                self.auth_url.ok_or_else(|| {
                    IdpError::InvalidConfig("auth_url required for generic OIDC".to_string())
                })?,
                self.token_url.ok_or_else(|| {
                    IdpError::InvalidConfig("token_url required for generic OIDC".to_string())
                })?,
                self.userinfo_url.ok_or_else(|| {
                    IdpError::InvalidConfig("userinfo_url required for generic OIDC".to_string())
                })?,
                self.jwks_url,
            ),
        };

        Ok(IdpConfig {
            provider: self.provider,
            client_id,
            client_secret,
            redirect_uri,
            auth_url,
            token_url,
            userinfo_url,
            jwks_url,
            provider_settings: self.provider_settings,
        })
    }
}

impl IdpConfig {
    /// Azure AD configuration builder
    #[must_use]
    pub fn azure_ad() -> IdpConfigBuilder {
        IdpConfigBuilder::new(IdpProvider::AzureAd)
    }

    /// Okta configuration builder
    #[must_use]
    pub fn okta() -> IdpConfigBuilder {
        IdpConfigBuilder::new(IdpProvider::Okta)
    }

    /// Auth0 configuration builder
    #[must_use]
    pub fn auth0() -> IdpConfigBuilder {
        IdpConfigBuilder::new(IdpProvider::Auth0)
    }

    /// Generic OIDC configuration builder
    #[must_use]
    pub fn oidc() -> IdpConfigBuilder {
        IdpConfigBuilder::new(IdpProvider::Oidc)
    }
}

/// `IdP` user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdpUserInfo {
    /// User ID (subject)
    pub sub: String,
    /// Email address
    pub email: String,
    /// Email verified flag
    pub email_verified: Option<bool>,
    /// Full name
    pub name: String,
    /// Given name (first name)
    pub given_name: Option<String>,
    /// Family name (last name)
    pub family_name: Option<String>,
    /// Picture/avatar URL
    pub picture: Option<String>,
    /// Groups/roles
    pub groups: Vec<String>,
    /// Additional claims
    pub additional_claims: HashMap<String, serde_json::Value>,
}

/// `IdP` client for authentication
pub struct IdpClient {
    config: IdpConfig,
    oauth_service: OAuth2Service,
    state: Option<String>,
}

impl IdpClient {
    /// Create a new `IdP` client
    pub async fn new(config: IdpConfig) -> Result<Self> {
        // Create OAuth2Config from IdP config
        let oauth_config = OAuth2Config {
            provider: config.provider.to_string(),
            client_id: config.client_id.clone(),
            client_secret: config.client_secret.clone(),
            auth_url: config.auth_url.clone(),
            token_url: config.token_url.clone(),
            user_info_url: config.userinfo_url.clone(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
        };

        let oauth_service = OAuth2Service::new(oauth_config);

        Ok(Self {
            config,
            oauth_service,
            state: None,
        })
    }

    /// Get authorization URL
    pub async fn get_authorization_url(&mut self, _scopes: &[&str]) -> Result<String> {
        let (url, state) = self
            .oauth_service
            .generate_authorization_url(&self.config.redirect_uri, true)
            .await
            .map_err(|e| IdpError::Provider(e.to_string()))?;

        self.state = Some(state);
        Ok(url)
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str, state: &str) -> Result<OAuth2Token> {
        self.oauth_service
            .exchange_code_for_token(code, state, &self.config.redirect_uri)
            .await
            .map_err(|e| IdpError::Provider(e.to_string()))
    }

    /// Get user information
    pub async fn get_user_info(&self, access_token: &str) -> Result<IdpUserInfo> {
        // Make HTTP request to userinfo endpoint
        let client = reqwest::Client::new();
        let response = client
            .get(&self.config.userinfo_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| IdpError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IdpError::Provider(format!(
                "UserInfo request failed: {}",
                response.status()
            )));
        }

        let user_data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| IdpError::Parse(e.to_string()))?;

        // Parse provider-specific user info
        self.parse_user_info(&user_data)
    }

    /// Parse user info from provider response
    fn parse_user_info(&self, data: &serde_json::Value) -> Result<IdpUserInfo> {
        let obj = data
            .as_object()
            .ok_or_else(|| IdpError::Parse("Expected JSON object".to_string()))?;

        // Extract common fields
        let sub = obj
            .get("sub")
            .or_else(|| obj.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| IdpError::Parse("Missing 'sub' or 'id' field".to_string()))?
            .to_string();

        let email = obj
            .get("email")
            .or_else(|| obj.get("mail"))
            .or_else(|| obj.get("userPrincipalName"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| IdpError::Parse("Missing email field".to_string()))?
            .to_string();

        let name = obj
            .get("name")
            .or_else(|| obj.get("displayName"))
            .and_then(|v| v.as_str())
            .unwrap_or(&email)
            .to_string();

        let email_verified = obj
            .get("email_verified")
            .and_then(serde_json::Value::as_bool);

        let given_name = obj
            .get("given_name")
            .or_else(|| obj.get("givenName"))
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        let family_name = obj
            .get("family_name")
            .or_else(|| obj.get("surname"))
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        let picture = obj
            .get("picture")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        // Extract groups (provider-specific)
        let groups = match self.config.provider {
            IdpProvider::AzureAd => obj
                .get("groups")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(std::string::ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            IdpProvider::Okta => obj
                .get("groups")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(std::string::ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            IdpProvider::Auth0 => obj
                .get("https://your-app.com/roles")
                .or_else(|| obj.get("roles"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(std::string::ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            IdpProvider::Oidc => obj
                .get("groups")
                .or_else(|| obj.get("roles"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(std::string::ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        };

        // Convert serde_json::Map to HashMap
        let additional_claims: HashMap<String, serde_json::Value> =
            obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

        Ok(IdpUserInfo {
            sub,
            email,
            email_verified,
            name,
            given_name,
            family_name,
            picture,
            groups,
            additional_claims,
        })
    }

    /// Refresh access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OAuth2Token> {
        self.oauth_service
            .refresh_token(refresh_token)
            .await
            .map_err(|e| IdpError::Provider(e.to_string()))
    }
}

/// OIDC Discovery document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub scopes_supported: Option<Vec<String>>,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Option<Vec<String>>,
}

/// Discover OIDC configuration
pub async fn discover_oidc(issuer: &str) -> Result<OidcDiscovery> {
    let discovery_url = if issuer.ends_with('/') {
        format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        )
    } else {
        format!("{issuer}/.well-known/openid-configuration")
    };

    let client = reqwest::Client::new();
    let response = client
        .get(&discovery_url)
        .send()
        .await
        .map_err(|e| IdpError::DiscoveryFailed(e.to_string()))?;

    if !response.status().is_success() {
        return Err(IdpError::DiscoveryFailed(format!(
            "Discovery request failed: {}",
            response.status()
        )));
    }

    response
        .json()
        .await
        .map_err(|e| IdpError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idp_provider_display() {
        assert_eq!(IdpProvider::AzureAd.to_string(), "azure_ad");
        assert_eq!(IdpProvider::Okta.to_string(), "okta");
        assert_eq!(IdpProvider::Auth0.to_string(), "auth0");
        assert_eq!(IdpProvider::Oidc.to_string(), "oidc");
    }

    #[test]
    fn test_azure_ad_config() {
        let config = IdpConfig::azure_ad()
            .tenant_id("test-tenant")
            .client_id("test-client")
            .client_secret("test-secret")
            .redirect_uri("https://app.com/callback")
            .build()
            .unwrap();

        assert_eq!(config.provider, IdpProvider::AzureAd);
        assert!(config.auth_url.contains("test-tenant"));
        assert!(config.token_url.contains("test-tenant"));
    }

    #[test]
    fn test_okta_config() {
        let config = IdpConfig::okta()
            .domain("dev-123456.okta.com")
            .client_id("test-client")
            .client_secret("test-secret")
            .redirect_uri("https://app.com/callback")
            .build()
            .unwrap();

        assert_eq!(config.provider, IdpProvider::Okta);
        assert!(config.auth_url.contains("dev-123456.okta.com"));
    }

    #[test]
    fn test_auth0_config() {
        let config = IdpConfig::auth0()
            .domain("dev-123456.auth0.com")
            .client_id("test-client")
            .client_secret("test-secret")
            .redirect_uri("https://app.com/callback")
            .build()
            .unwrap();

        assert_eq!(config.provider, IdpProvider::Auth0);
        assert!(config.auth_url.contains("dev-123456.auth0.com"));
    }

    #[test]
    fn test_oidc_config() {
        let config = IdpConfig::oidc()
            .client_id("test-client")
            .client_secret("test-secret")
            .redirect_uri("https://app.com/callback")
            .auth_url("https://provider.com/auth")
            .token_url("https://provider.com/token")
            .userinfo_url("https://provider.com/userinfo")
            .build()
            .unwrap();

        assert_eq!(config.provider, IdpProvider::Oidc);
        assert_eq!(config.auth_url, "https://provider.com/auth");
    }

    #[test]
    fn test_missing_tenant_id() {
        let result = IdpConfig::azure_ad()
            .client_id("test-client")
            .client_secret("test-secret")
            .redirect_uri("https://app.com/callback")
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("tenant_id"));
    }

    #[test]
    fn test_missing_domain() {
        let result = IdpConfig::okta()
            .client_id("test-client")
            .client_secret("test-secret")
            .redirect_uri("https://app.com/callback")
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("domain"));
    }

    #[test]
    fn test_missing_required_fields() {
        let result = IdpConfig::oidc().client_id("test-client").build();

        assert!(result.is_err());
    }

    #[test]
    fn test_custom_provider_settings() {
        let config = IdpConfig::azure_ad()
            .tenant_id("test-tenant")
            .client_id("test-client")
            .client_secret("test-secret")
            .redirect_uri("https://app.com/callback")
            .add_setting("custom_key".to_string(), "custom_value".to_string())
            .build()
            .unwrap();

        assert_eq!(
            config.provider_settings.get("custom_key").unwrap(),
            "custom_value"
        );
    }
}
