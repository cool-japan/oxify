//! Authentication support for MCP communication
//!
//! This module provides authentication mechanisms for secure MCP server communication.
//!
//! # Supported Authentication Methods
//!
//! - **API Key**: Pass API keys via headers or query parameters
//! - **Basic Auth**: HTTP Basic authentication (username:password)
//! - **Bearer Token**: OAuth2-style bearer token authentication
//!
//! # Example
//!
//! ```ignore
//! use oxify_mcp::auth::{AuthConfig, ApiKeyAuth, AuthenticatedHttpTransport};
//!
//! // API Key authentication
//! let auth = AuthConfig::api_key("X-API-Key", "your-api-key");
//! let transport = AuthenticatedHttpTransport::new("http://localhost:3000", auth);
//!
//! // Bearer token authentication
//! let auth = AuthConfig::bearer_token("your-jwt-token");
//! let transport = AuthenticatedHttpTransport::new("http://localhost:3000", auth);
//! ```

use crate::{McpError, McpTransport, Result};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;

/// Authentication method for MCP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    /// No authentication
    None,
    /// API key authentication
    ApiKey(ApiKeyAuth),
    /// HTTP Basic authentication
    Basic(BasicAuth),
    /// Bearer token authentication
    Bearer(BearerAuth),
    /// Custom header authentication
    CustomHeader(CustomHeaderAuth),
    /// OAuth2 authentication with token refresh
    OAuth2(OAuth2Auth),
}

/// API Key authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyAuth {
    /// Header name to use (e.g., "X-API-Key", "Authorization")
    pub header_name: String,
    /// The API key value
    pub api_key: String,
    /// Optional prefix (e.g., "Bearer", "Api-Key")
    pub prefix: Option<String>,
}

impl ApiKeyAuth {
    /// Create new API key authentication
    pub fn new(header_name: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            header_name: header_name.into(),
            api_key: api_key.into(),
            prefix: None,
        }
    }

    /// Add a prefix to the API key value
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Get the formatted header value
    pub fn header_value(&self) -> String {
        match &self.prefix {
            Some(prefix) => format!("{} {}", prefix, self.api_key),
            None => self.api_key.clone(),
        }
    }
}

/// HTTP Basic authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicAuth {
    /// Username
    pub username: String,
    /// Password
    pub password: String,
}

impl BasicAuth {
    /// Create new Basic authentication
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }

    /// Get the encoded credentials
    pub fn encoded_credentials(&self) -> String {
        use base64::Engine;
        let credentials = format!("{}:{}", self.username, self.password);
        base64::engine::general_purpose::STANDARD.encode(credentials)
    }

    /// Get the Authorization header value
    pub fn header_value(&self) -> String {
        format!("Basic {}", self.encoded_credentials())
    }
}

/// Bearer token authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BearerAuth {
    /// The bearer token (JWT or opaque token)
    pub token: String,
}

impl BearerAuth {
    /// Create new Bearer authentication
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    /// Get the Authorization header value
    pub fn header_value(&self) -> String {
        format!("Bearer {}", self.token)
    }
}

/// Custom header authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomHeaderAuth {
    /// Custom headers to add
    pub headers: HashMap<String, String>,
}

impl CustomHeaderAuth {
    /// Create new custom header authentication
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
        }
    }

    /// Add a header
    pub fn add_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }
}

impl Default for CustomHeaderAuth {
    fn default() -> Self {
        Self::new()
    }
}

/// OAuth2 grant type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OAuth2GrantType {
    /// Client credentials flow (machine-to-machine)
    ClientCredentials,
    /// Authorization code flow (with optional PKCE)
    AuthorizationCode,
    /// Refresh token flow
    RefreshToken,
}

/// OAuth2 authentication configuration with token refresh support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Auth {
    /// OAuth2 token endpoint URL
    pub token_url: String,
    /// Client ID
    pub client_id: String,
    /// Client secret (optional for PKCE)
    pub client_secret: Option<String>,
    /// Grant type
    pub grant_type: OAuth2GrantType,
    /// Access token
    pub access_token: Option<String>,
    /// Refresh token
    pub refresh_token: Option<String>,
    /// Token expiry time (Unix timestamp)
    pub expires_at: Option<i64>,
    /// Requested scopes
    pub scopes: Vec<String>,
    /// PKCE code verifier (for authorization code flow)
    pub code_verifier: Option<String>,
    /// Authorization code (for authorization code flow)
    pub authorization_code: Option<String>,
}

impl OAuth2Auth {
    /// Create new OAuth2 authentication with client credentials
    pub fn client_credentials(
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret: Some(client_secret.into()),
            grant_type: OAuth2GrantType::ClientCredentials,
            access_token: None,
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
            code_verifier: None,
            authorization_code: None,
        }
    }

    /// Create new OAuth2 authentication with authorization code
    pub fn authorization_code(
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: Option<String>,
        authorization_code: impl Into<String>,
    ) -> Self {
        Self {
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret,
            grant_type: OAuth2GrantType::AuthorizationCode,
            access_token: None,
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
            code_verifier: None,
            authorization_code: Some(authorization_code.into()),
        }
    }

    /// Create OAuth2 authentication with existing access and refresh tokens
    pub fn with_tokens(
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: Option<String>,
        access_token: impl Into<String>,
        refresh_token: impl Into<String>,
    ) -> Self {
        Self {
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret,
            grant_type: OAuth2GrantType::RefreshToken,
            access_token: Some(access_token.into()),
            refresh_token: Some(refresh_token.into()),
            expires_at: None,
            scopes: Vec::new(),
            code_verifier: None,
            authorization_code: None,
        }
    }

    /// Add scopes to the OAuth2 request
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Set PKCE code verifier (for authorization code flow)
    pub fn with_pkce(mut self, code_verifier: impl Into<String>) -> Self {
        self.code_verifier = Some(code_verifier.into());
        self
    }

    /// Check if the token is expired
    pub fn is_token_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            // Consider token expired 60 seconds before actual expiry (buffer)
            now >= expires_at - 60
        } else {
            // If no expiry time set, assume not expired
            false
        }
    }

    /// Get the Authorization header value
    pub fn header_value(&self) -> Option<String> {
        self.access_token
            .as_ref()
            .map(|token| format!("Bearer {}", token))
    }

    /// Request a new access token from the OAuth2 server
    pub async fn request_token(&mut self) -> Result<()> {
        let client = reqwest::Client::new();
        let mut params: Vec<(String, String)> = Vec::new();

        match self.grant_type {
            OAuth2GrantType::ClientCredentials => {
                params.push(("grant_type".to_string(), "client_credentials".to_string()));
                params.push(("client_id".to_string(), self.client_id.clone()));
                if let Some(ref secret) = self.client_secret {
                    params.push(("client_secret".to_string(), secret.clone()));
                }
                if !self.scopes.is_empty() {
                    params.push(("scope".to_string(), self.scopes.join(" ")));
                }
            }
            OAuth2GrantType::AuthorizationCode => {
                params.push(("grant_type".to_string(), "authorization_code".to_string()));
                params.push(("client_id".to_string(), self.client_id.clone()));
                if let Some(ref secret) = self.client_secret {
                    params.push(("client_secret".to_string(), secret.clone()));
                }
                if let Some(ref code) = self.authorization_code {
                    params.push(("code".to_string(), code.clone()));
                }
                if let Some(ref verifier) = self.code_verifier {
                    params.push(("code_verifier".to_string(), verifier.clone()));
                }
            }
            OAuth2GrantType::RefreshToken => {
                params.push(("grant_type".to_string(), "refresh_token".to_string()));
                params.push(("client_id".to_string(), self.client_id.clone()));
                if let Some(ref secret) = self.client_secret {
                    params.push(("client_secret".to_string(), secret.clone()));
                }
                if let Some(ref refresh) = self.refresh_token {
                    params.push(("refresh_token".to_string(), refresh.clone()));
                }
            }
        }

        let response = client
            .post(&self.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| McpError::ServerError(format!("OAuth2 token request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ServerError(format!(
                "OAuth2 token request failed with status: {}",
                response.status()
            )));
        }

        let token_response: Value = response.json().await.map_err(|e| {
            McpError::ProtocolError(format!("Failed to parse OAuth2 token response: {}", e))
        })?;

        // Extract access token
        if let Some(access_token) = token_response.get("access_token").and_then(|v| v.as_str()) {
            self.access_token = Some(access_token.to_string());
        } else {
            return Err(McpError::ProtocolError(
                "OAuth2 response missing access_token".to_string(),
            ));
        }

        // Extract refresh token (if provided)
        if let Some(refresh_token) = token_response.get("refresh_token").and_then(|v| v.as_str()) {
            self.refresh_token = Some(refresh_token.to_string());
        }

        // Extract expiry time
        if let Some(expires_in) = token_response.get("expires_in").and_then(|v| v.as_i64()) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            self.expires_at = Some(now + expires_in);
        }

        Ok(())
    }

    /// Refresh the access token if expired
    pub async fn refresh_if_needed(&mut self) -> Result<bool> {
        if self.is_token_expired() && self.refresh_token.is_some() {
            // Temporarily switch to refresh token grant type
            let original_grant = self.grant_type.clone();
            self.grant_type = OAuth2GrantType::RefreshToken;

            let result = self.request_token().await;

            // Restore original grant type
            self.grant_type = original_grant;

            result?;
            Ok(true) // Token was refreshed
        } else {
            Ok(false) // No refresh needed
        }
    }
}

/// Authentication configuration wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// The authentication method
    pub method: AuthMethod,
    /// Optional scopes or permissions
    pub scopes: Vec<String>,
}

impl AuthConfig {
    /// Create authentication configuration with no auth
    pub fn none() -> Self {
        Self {
            method: AuthMethod::None,
            scopes: Vec::new(),
        }
    }

    /// Create API key authentication
    pub fn api_key(header_name: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            method: AuthMethod::ApiKey(ApiKeyAuth::new(header_name, api_key)),
            scopes: Vec::new(),
        }
    }

    /// Create API key authentication with prefix
    pub fn api_key_with_prefix(
        header_name: impl Into<String>,
        api_key: impl Into<String>,
        prefix: impl Into<String>,
    ) -> Self {
        Self {
            method: AuthMethod::ApiKey(ApiKeyAuth::new(header_name, api_key).with_prefix(prefix)),
            scopes: Vec::new(),
        }
    }

    /// Create Basic authentication
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            method: AuthMethod::Basic(BasicAuth::new(username, password)),
            scopes: Vec::new(),
        }
    }

    /// Create Bearer token authentication
    pub fn bearer_token(token: impl Into<String>) -> Self {
        Self {
            method: AuthMethod::Bearer(BearerAuth::new(token)),
            scopes: Vec::new(),
        }
    }

    /// Create custom header authentication
    pub fn custom_headers(headers: HashMap<String, String>) -> Self {
        Self {
            method: AuthMethod::CustomHeader(CustomHeaderAuth { headers }),
            scopes: Vec::new(),
        }
    }

    /// Create OAuth2 authentication with client credentials
    pub fn oauth2_client_credentials(
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            method: AuthMethod::OAuth2(OAuth2Auth::client_credentials(
                token_url,
                client_id,
                client_secret,
            )),
            scopes: Vec::new(),
        }
    }

    /// Create OAuth2 authentication with authorization code
    pub fn oauth2_authorization_code(
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: Option<String>,
        authorization_code: impl Into<String>,
    ) -> Self {
        Self {
            method: AuthMethod::OAuth2(OAuth2Auth::authorization_code(
                token_url,
                client_id,
                client_secret,
                authorization_code,
            )),
            scopes: Vec::new(),
        }
    }

    /// Create OAuth2 authentication with existing tokens
    pub fn oauth2_with_tokens(
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: Option<String>,
        access_token: impl Into<String>,
        refresh_token: impl Into<String>,
    ) -> Self {
        Self {
            method: AuthMethod::OAuth2(OAuth2Auth::with_tokens(
                token_url,
                client_id,
                client_secret,
                access_token,
                refresh_token,
            )),
            scopes: Vec::new(),
        }
    }

    /// Add scopes to authentication
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Build request headers from authentication config
    pub fn build_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();

        match &self.method {
            AuthMethod::None => {}
            AuthMethod::ApiKey(auth) => {
                let header_name = HeaderName::from_str(&auth.header_name)
                    .map_err(|e| McpError::InvalidRequest(format!("Invalid header name: {}", e)))?;
                let header_value = HeaderValue::from_str(&auth.header_value()).map_err(|e| {
                    McpError::InvalidRequest(format!("Invalid header value: {}", e))
                })?;
                headers.insert(header_name, header_value);
            }
            AuthMethod::Basic(auth) => {
                let header_value = HeaderValue::from_str(&auth.header_value()).map_err(|e| {
                    McpError::InvalidRequest(format!("Invalid header value: {}", e))
                })?;
                headers.insert(reqwest::header::AUTHORIZATION, header_value);
            }
            AuthMethod::Bearer(auth) => {
                let header_value = HeaderValue::from_str(&auth.header_value()).map_err(|e| {
                    McpError::InvalidRequest(format!("Invalid header value: {}", e))
                })?;
                headers.insert(reqwest::header::AUTHORIZATION, header_value);
            }
            AuthMethod::CustomHeader(auth) => {
                for (name, value) in &auth.headers {
                    let header_name = HeaderName::from_str(name).map_err(|e| {
                        McpError::InvalidRequest(format!("Invalid header name: {}", e))
                    })?;
                    let header_value = HeaderValue::from_str(value).map_err(|e| {
                        McpError::InvalidRequest(format!("Invalid header value: {}", e))
                    })?;
                    headers.insert(header_name, header_value);
                }
            }
            AuthMethod::OAuth2(auth) => {
                if let Some(header_value_str) = auth.header_value() {
                    let header_value = HeaderValue::from_str(&header_value_str).map_err(|e| {
                        McpError::InvalidRequest(format!("Invalid header value: {}", e))
                    })?;
                    headers.insert(reqwest::header::AUTHORIZATION, header_value);
                }
            }
        }

        Ok(headers)
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::none()
    }
}

/// HTTP transport with authentication support
pub struct AuthenticatedHttpTransport {
    client: reqwest::Client,
    base_url: String,
    auth: AuthConfig,
    request_id: u64,
    max_response_size: usize,
}

impl AuthenticatedHttpTransport {
    /// Default maximum response size (10MB)
    const DEFAULT_MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

    /// Create a new authenticated HTTP transport
    pub fn new(base_url: impl Into<String>, auth: AuthConfig) -> Result<Self> {
        let headers = auth.build_headers()?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .default_headers(headers)
            .build()
            .map_err(|e| McpError::ServerError(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            client,
            base_url: base_url.into(),
            auth,
            request_id: 1,
            max_response_size: Self::DEFAULT_MAX_RESPONSE_SIZE,
        })
    }

    /// Set maximum response size
    pub fn with_max_response_size(mut self, size: usize) -> Self {
        self.max_response_size = size;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Result<Self> {
        let headers = self.auth.build_headers()?;
        self.client = reqwest::Client::builder()
            .timeout(timeout)
            .default_headers(headers)
            .build()
            .map_err(|e| McpError::ServerError(format!("Failed to rebuild HTTP client: {}", e)))?;
        Ok(self)
    }

    /// Get the current authentication config
    pub fn auth_config(&self) -> &AuthConfig {
        &self.auth
    }
}

#[async_trait]
impl McpTransport for AuthenticatedHttpTransport {
    async fn send_request(&mut self, mut request: Value) -> Result<Value> {
        // Add JSON-RPC fields
        if let Value::Object(ref mut obj) = request {
            obj.insert("jsonrpc".to_string(), Value::String("2.0".to_string()));
            obj.insert("id".to_string(), Value::Number(self.request_id.into()));
            self.request_id += 1;
        }

        let response = self
            .client
            .post(&self.base_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| McpError::ServerError(format!("HTTP request failed: {}", e)))?;

        // Check for auth errors
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(McpError::ServerError(
                "Authentication failed: Invalid or missing credentials".to_string(),
            ));
        }

        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(McpError::ServerError(
                "Authorization failed: Insufficient permissions".to_string(),
            ));
        }

        let response_json: Value = response
            .json()
            .await
            .map_err(|e| McpError::ProtocolError(format!("Failed to parse response: {}", e)))?;

        Ok(response_json)
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Credential store for managing multiple API keys
#[derive(Debug, Clone, Default)]
pub struct CredentialStore {
    credentials: HashMap<String, AuthConfig>,
}

impl CredentialStore {
    /// Create a new credential store
    pub fn new() -> Self {
        Self {
            credentials: HashMap::new(),
        }
    }

    /// Add credentials for a server
    pub fn add(&mut self, server_id: impl Into<String>, auth: AuthConfig) {
        self.credentials.insert(server_id.into(), auth);
    }

    /// Get credentials for a server
    pub fn get(&self, server_id: &str) -> Option<&AuthConfig> {
        self.credentials.get(server_id)
    }

    /// Remove credentials for a server
    pub fn remove(&mut self, server_id: &str) -> Option<AuthConfig> {
        self.credentials.remove(server_id)
    }

    /// Check if credentials exist for a server
    pub fn has(&self, server_id: &str) -> bool {
        self.credentials.contains_key(server_id)
    }

    /// List all server IDs with stored credentials
    pub fn server_ids(&self) -> Vec<&String> {
        self.credentials.keys().collect()
    }

    /// Get the number of stored credentials
    pub fn len(&self) -> usize {
        self.credentials.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.credentials.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_auth() {
        let auth = ApiKeyAuth::new("X-API-Key", "secret123");
        assert_eq!(auth.header_name, "X-API-Key");
        assert_eq!(auth.api_key, "secret123");
        assert_eq!(auth.header_value(), "secret123");
    }

    #[test]
    fn test_api_key_auth_with_prefix() {
        let auth = ApiKeyAuth::new("Authorization", "secret123").with_prefix("Api-Key");
        assert_eq!(auth.header_value(), "Api-Key secret123");
    }

    #[test]
    fn test_basic_auth() {
        let auth = BasicAuth::new("user", "pass");
        assert_eq!(auth.username, "user");
        assert_eq!(auth.password, "pass");
        // Base64 of "user:pass" is "dXNlcjpwYXNz"
        assert_eq!(auth.header_value(), "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn test_bearer_auth() {
        let auth = BearerAuth::new("jwt-token-here");
        assert_eq!(auth.token, "jwt-token-here");
        assert_eq!(auth.header_value(), "Bearer jwt-token-here");
    }

    #[test]
    fn test_auth_config_none() {
        let config = AuthConfig::none();
        matches!(config.method, AuthMethod::None);
    }

    #[test]
    fn test_auth_config_api_key() {
        let config = AuthConfig::api_key("X-API-Key", "secret");
        if let AuthMethod::ApiKey(auth) = config.method {
            assert_eq!(auth.header_name, "X-API-Key");
            assert_eq!(auth.api_key, "secret");
        } else {
            panic!("Expected ApiKey auth method");
        }
    }

    #[test]
    fn test_auth_config_bearer() {
        let config = AuthConfig::bearer_token("token123");
        if let AuthMethod::Bearer(auth) = config.method {
            assert_eq!(auth.token, "token123");
        } else {
            panic!("Expected Bearer auth method");
        }
    }

    #[test]
    fn test_auth_config_basic() {
        let config = AuthConfig::basic("user", "pass");
        if let AuthMethod::Basic(auth) = config.method {
            assert_eq!(auth.username, "user");
            assert_eq!(auth.password, "pass");
        } else {
            panic!("Expected Basic auth method");
        }
    }

    #[test]
    fn test_auth_config_with_scopes() {
        let config = AuthConfig::bearer_token("token")
            .with_scopes(vec!["read".to_string(), "write".to_string()]);
        assert_eq!(config.scopes.len(), 2);
        assert!(config.scopes.contains(&"read".to_string()));
        assert!(config.scopes.contains(&"write".to_string()));
    }

    #[test]
    fn test_build_headers_api_key() {
        let config = AuthConfig::api_key("X-API-Key", "secret");
        let headers = config.build_headers().unwrap();
        assert!(headers.contains_key("x-api-key"));
        assert_eq!(headers.get("x-api-key").unwrap(), "secret");
    }

    #[test]
    fn test_build_headers_bearer() {
        let config = AuthConfig::bearer_token("token123");
        let headers = config.build_headers().unwrap();
        assert!(headers.contains_key("authorization"));
        assert_eq!(headers.get("authorization").unwrap(), "Bearer token123");
    }

    #[test]
    fn test_credential_store() {
        let mut store = CredentialStore::new();
        assert!(store.is_empty());

        store.add("server1", AuthConfig::api_key("X-API-Key", "key1"));
        store.add("server2", AuthConfig::bearer_token("token2"));

        assert_eq!(store.len(), 2);
        assert!(store.has("server1"));
        assert!(!store.has("server3"));

        let auth = store.get("server1").unwrap();
        matches!(auth.method, AuthMethod::ApiKey(_));

        store.remove("server1");
        assert!(!store.has("server1"));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_custom_header_auth() {
        let auth = CustomHeaderAuth::new()
            .add_header("X-Custom-Header", "value1")
            .add_header("X-Another-Header", "value2");

        assert_eq!(auth.headers.len(), 2);
        assert_eq!(
            auth.headers.get("X-Custom-Header"),
            Some(&"value1".to_string())
        );
    }

    #[test]
    fn test_build_headers_custom() {
        let mut headers = HashMap::new();
        headers.insert("X-Custom-1".to_string(), "value1".to_string());
        headers.insert("X-Custom-2".to_string(), "value2".to_string());

        let config = AuthConfig::custom_headers(headers);
        let built = config.build_headers().unwrap();

        assert!(built.contains_key("x-custom-1"));
        assert!(built.contains_key("x-custom-2"));
    }

    #[test]
    fn test_oauth2_client_credentials() {
        let auth = OAuth2Auth::client_credentials(
            "https://auth.example.com/token",
            "client_id",
            "client_secret",
        );
        assert_eq!(auth.token_url, "https://auth.example.com/token");
        assert_eq!(auth.client_id, "client_id");
        assert_eq!(auth.client_secret, Some("client_secret".to_string()));
        assert_eq!(auth.grant_type, OAuth2GrantType::ClientCredentials);
        assert!(auth.access_token.is_none());
    }

    #[test]
    fn test_oauth2_authorization_code() {
        let auth = OAuth2Auth::authorization_code(
            "https://auth.example.com/token",
            "client_id",
            Some("client_secret".to_string()),
            "auth_code_123",
        );
        assert_eq!(auth.grant_type, OAuth2GrantType::AuthorizationCode);
        assert_eq!(auth.authorization_code, Some("auth_code_123".to_string()));
    }

    #[test]
    fn test_oauth2_with_tokens() {
        let auth = OAuth2Auth::with_tokens(
            "https://auth.example.com/token",
            "client_id",
            Some("client_secret".to_string()),
            "access_token_123",
            "refresh_token_456",
        );
        assert_eq!(auth.grant_type, OAuth2GrantType::RefreshToken);
        assert_eq!(auth.access_token, Some("access_token_123".to_string()));
        assert_eq!(auth.refresh_token, Some("refresh_token_456".to_string()));
    }

    #[test]
    fn test_oauth2_with_scopes() {
        let auth = OAuth2Auth::client_credentials(
            "https://auth.example.com/token",
            "client_id",
            "client_secret",
        )
        .with_scopes(vec!["read".to_string(), "write".to_string()]);

        assert_eq!(auth.scopes.len(), 2);
        assert!(auth.scopes.contains(&"read".to_string()));
    }

    #[test]
    fn test_oauth2_with_pkce() {
        let auth = OAuth2Auth::authorization_code(
            "https://auth.example.com/token",
            "client_id",
            None,
            "auth_code",
        )
        .with_pkce("code_verifier_123");

        assert_eq!(auth.code_verifier, Some("code_verifier_123".to_string()));
    }

    #[test]
    fn test_oauth2_header_value() {
        let mut auth = OAuth2Auth::client_credentials(
            "https://auth.example.com/token",
            "client_id",
            "client_secret",
        );
        assert!(auth.header_value().is_none());

        auth.access_token = Some("test_token".to_string());
        assert_eq!(auth.header_value(), Some("Bearer test_token".to_string()));
    }

    #[test]
    fn test_oauth2_is_token_expired() {
        let mut auth = OAuth2Auth::client_credentials(
            "https://auth.example.com/token",
            "client_id",
            "client_secret",
        );

        // No expiry set
        assert!(!auth.is_token_expired());

        // Set expiry in the past
        auth.expires_at = Some(1000);
        assert!(auth.is_token_expired());

        // Set expiry far in the future
        let future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600;
        auth.expires_at = Some(future);
        assert!(!auth.is_token_expired());
    }

    #[test]
    fn test_auth_config_oauth2_client_credentials() {
        let config = AuthConfig::oauth2_client_credentials(
            "https://auth.example.com/token",
            "client_id",
            "client_secret",
        );
        if let AuthMethod::OAuth2(auth) = config.method {
            assert_eq!(auth.grant_type, OAuth2GrantType::ClientCredentials);
        } else {
            panic!("Expected OAuth2 auth method");
        }
    }

    #[test]
    fn test_auth_config_oauth2_with_tokens() {
        let config = AuthConfig::oauth2_with_tokens(
            "https://auth.example.com/token",
            "client_id",
            Some("client_secret".to_string()),
            "access_token",
            "refresh_token",
        );
        if let AuthMethod::OAuth2(auth) = config.method {
            assert_eq!(auth.access_token, Some("access_token".to_string()));
            assert_eq!(auth.refresh_token, Some("refresh_token".to_string()));
        } else {
            panic!("Expected OAuth2 auth method");
        }
    }

    #[test]
    fn test_build_headers_oauth2() {
        let mut auth = OAuth2Auth::client_credentials(
            "https://auth.example.com/token",
            "client_id",
            "client_secret",
        );
        auth.access_token = Some("test_access_token".to_string());

        let config = AuthConfig {
            method: AuthMethod::OAuth2(auth),
            scopes: Vec::new(),
        };

        let headers = config.build_headers().unwrap();
        assert!(headers.contains_key("authorization"));
        assert_eq!(
            headers.get("authorization").unwrap(),
            "Bearer test_access_token"
        );
    }

    #[test]
    fn test_oauth2_grant_type_equality() {
        assert_eq!(
            OAuth2GrantType::ClientCredentials,
            OAuth2GrantType::ClientCredentials
        );
        assert_ne!(
            OAuth2GrantType::ClientCredentials,
            OAuth2GrantType::AuthorizationCode
        );
    }
}
