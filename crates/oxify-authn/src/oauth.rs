//! OAuth2/OIDC authentication support
//!
//! Ported from `OxiRS` (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) `OxiRS` Contributors
//! Adapted for `OxiFY` (simplified for maintainability)
//! License: MIT OR Apache-2.0 (compatible with `OxiRS`)

use crate::types::{AuthError, AuthResult, OAuth2Config, Permission, Result, User};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// `OAuth2` authentication service
#[derive(Clone)]
pub struct OAuth2Service {
    config: Arc<OAuth2Config>,
    active_states: Arc<RwLock<HashMap<String, OAuth2State>>>,
    client: reqwest::Client,
}

/// `OAuth2` state for authorization flow
#[derive(Debug, Clone)]
pub struct OAuth2State {
    pub state: String,
    pub code_verifier: Option<String>, // For PKCE
    pub redirect_uri: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// `OAuth2` access token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Token {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
    pub scope: String,
    pub id_token: Option<String>,
    pub issued_at: DateTime<Utc>,
}

/// OIDC user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OIDCUserInfo {
    pub sub: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub groups: Option<Vec<String>>,
    pub roles: Option<Vec<String>>,
}

/// `OAuth2` token response from provider
#[derive(Debug, Deserialize)]
struct OAuth2TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    id_token: Option<String>,
}

impl OAuth2Service {
    /// Create new `OAuth2` service
    #[must_use]
    pub fn new(config: OAuth2Config) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            config: Arc::new(config),
            active_states: Arc::new(RwLock::new(HashMap::new())),
            client,
        }
    }

    /// Generate authorization URL for `OAuth2` flow
    pub async fn generate_authorization_url(
        &self,
        redirect_uri: &str,
        use_pkce: bool,
    ) -> Result<(String, String)> {
        let state = uuid::Uuid::new_v4().to_string();
        let scope_string = self.config.scopes.join(" ");

        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&state={}&scope={}",
            self.config.auth_url,
            url_encode(&self.config.client_id),
            url_encode(redirect_uri),
            url_encode(&state),
            url_encode(&scope_string)
        );

        let mut oauth_state = OAuth2State {
            state: state.clone(),
            code_verifier: None,
            redirect_uri: redirect_uri.to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::minutes(10),
        };

        // Add PKCE if requested
        if use_pkce {
            let code_verifier = generate_code_verifier();
            let code_challenge = generate_code_challenge(&code_verifier);

            url.push_str("&code_challenge=");
            url.push_str(&url_encode(&code_challenge));
            url.push_str("&code_challenge_method=S256");

            oauth_state.code_verifier = Some(code_verifier);
        }

        // Store state
        let mut states = self.active_states.write().await;
        states.insert(state.clone(), oauth_state);

        Ok((url, state))
    }

    /// Exchange authorization code for access token
    pub async fn exchange_code_for_token(
        &self,
        code: &str,
        state: &str,
        redirect_uri: &str,
    ) -> Result<OAuth2Token> {
        // Validate and remove state
        let oauth_state = {
            let mut states = self.active_states.write().await;
            states.remove(state)
        };

        let oauth_state = oauth_state.ok_or(AuthError::OAuthError(
            "Invalid or expired state".to_string(),
        ))?;

        if oauth_state.redirect_uri != redirect_uri {
            return Err(AuthError::OAuthError("Redirect URI mismatch".to_string()));
        }

        if Utc::now() > oauth_state.expires_at {
            return Err(AuthError::OAuthError("State expired".to_string()));
        }

        // Prepare token request
        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", redirect_uri.to_string()),
            ("client_id", self.config.client_id.clone()),
            ("client_secret", self.config.client_secret.clone()),
        ];

        // Add PKCE if used
        if let Some(code_verifier) = oauth_state.code_verifier {
            params.push(("code_verifier", code_verifier));
        }

        let response = self
            .client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError(format!("Token exchange failed: {e}")))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AuthError::OAuthError(format!(
                "Token exchange failed: {error_text}"
            )));
        }

        let token_response: OAuth2TokenResponse = response
            .json()
            .await
            .map_err(|e| AuthError::OAuthError(format!("Failed to parse token: {e}")))?;

        Ok(OAuth2Token {
            access_token: token_response.access_token,
            token_type: token_response.token_type,
            expires_in: token_response.expires_in.unwrap_or(3600),
            refresh_token: token_response.refresh_token,
            scope: token_response.scope.unwrap_or_default(),
            id_token: token_response.id_token,
            issued_at: Utc::now(),
        })
    }

    /// Get user information from OIDC userinfo endpoint
    pub async fn get_user_info(&self, access_token: &str) -> Result<OIDCUserInfo> {
        let response = self
            .client
            .get(&self.config.user_info_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError(format!("UserInfo request failed: {e}")))?;

        if !response.status().is_success() {
            return Err(AuthError::OAuthError(format!(
                "UserInfo failed with status: {}",
                response.status()
            )));
        }

        response
            .json()
            .await
            .map_err(|e| AuthError::OAuthError(format!("Failed to parse user info: {e}")))
    }

    /// Authenticate user using OAuth2/OIDC
    pub async fn authenticate(&self, access_token: &str) -> Result<AuthResult> {
        let user_info = self.get_user_info(access_token).await?;
        let user = Self::map_oidc_user(user_info);
        Ok(AuthResult::Authenticated(user))
    }

    /// Refresh access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OAuth2Token> {
        let params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
        ];

        let response = self
            .client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError(format!("Token refresh failed: {e}")))?;

        if !response.status().is_success() {
            return Err(AuthError::OAuthError(format!(
                "Token refresh failed: {}",
                response.status()
            )));
        }

        let token_response: OAuth2TokenResponse = response
            .json()
            .await
            .map_err(|e| AuthError::OAuthError(format!("Failed to parse refresh: {e}")))?;

        Ok(OAuth2Token {
            access_token: token_response.access_token,
            token_type: token_response.token_type,
            expires_in: token_response.expires_in.unwrap_or(3600),
            refresh_token: token_response
                .refresh_token
                .or(Some(refresh_token.to_string())),
            scope: token_response.scope.unwrap_or_default(),
            id_token: token_response.id_token,
            issued_at: Utc::now(),
        })
    }

    /// Map OIDC user info to internal user
    fn map_oidc_user(user_info: OIDCUserInfo) -> User {
        let username = user_info.email.as_ref().unwrap_or(&user_info.sub).clone();

        let mut roles = Vec::new();
        if let Some(oidc_roles) = &user_info.roles {
            roles.extend(oidc_roles.clone());
        }
        if let Some(groups) = &user_info.groups {
            for group in groups {
                roles.push(map_group_to_role(group));
            }
        }
        if roles.is_empty() {
            roles.push("user".to_string());
        }

        let permissions = compute_permissions(&roles);

        User {
            username,
            roles,
            email: user_info.email,
            full_name: user_info.name,
            last_login: Some(Utc::now()),
            permissions,
        }
    }

    /// Cleanup expired states
    pub async fn cleanup_expired(&self) {
        let now = Utc::now();
        let mut states = self.active_states.write().await;
        states.retain(|_, state| state.expires_at > now);
    }
}

/// Generate code verifier for PKCE
fn generate_code_verifier() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    use rand::RngExt;
    let mut rng = rand::rng();

    (0..128)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate code challenge for PKCE (S256 method)
fn generate_code_challenge(code_verifier: &str) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// URL encoding helper
fn url_encode(input: &str) -> String {
    percent_encoding::utf8_percent_encode(input, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// Map OIDC group to internal role
fn map_group_to_role(group: &str) -> String {
    match group.to_lowercase().as_str() {
        "admin" | "administrators" => "admin".to_string(),
        "writers" | "editors" => "writer".to_string(),
        "readers" | "viewers" => "reader".to_string(),
        _ => "user".to_string(),
    }
}

/// Compute permissions from roles
fn compute_permissions(roles: &[String]) -> Vec<Permission> {
    let mut permissions = Vec::new();

    for role in roles {
        match role.as_str() {
            "admin" => {
                permissions.extend(vec![
                    Permission::GlobalAdmin,
                    Permission::GlobalRead,
                    Permission::GlobalWrite,
                    Permission::Admin,
                ]);
            }
            "writer" => {
                permissions.extend(vec![
                    Permission::GlobalRead,
                    Permission::GlobalWrite,
                    Permission::Write,
                ]);
            }
            "reader" | "user" => {
                permissions.extend(vec![Permission::GlobalRead, Permission::Read]);
            }
            _ => {}
        }
    }

    permissions.sort();
    permissions.dedup();
    permissions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> OAuth2Config {
        OAuth2Config {
            provider: "test".to_string(),
            client_id: "test_client_id".to_string(),
            client_secret: "test_secret".to_string(),
            auth_url: "https://provider.example.com/auth".to_string(),
            token_url: "https://provider.example.com/token".to_string(),
            user_info_url: "https://provider.example.com/userinfo".to_string(),
            scopes: vec!["openid".to_string(), "profile".to_string()],
        }
    }

    #[tokio::test]
    async fn test_oauth2_service_creation() {
        let config = create_test_config();
        let service = OAuth2Service::new(config);
        assert_eq!(service.config.provider, "test");
    }

    #[tokio::test]
    async fn test_authorization_url() {
        let config = create_test_config();
        let service = OAuth2Service::new(config);

        let (url, state) = service
            .generate_authorization_url("http://localhost/callback", false)
            .await
            .unwrap();

        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id"));
        assert!(!state.is_empty());
    }

    #[tokio::test]
    async fn test_pkce_generation() {
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);

        assert_eq!(verifier.len(), 128);
        assert!(!challenge.is_empty());
        assert_ne!(verifier, challenge);
    }

    #[test]
    fn test_group_mapping() {
        assert_eq!(map_group_to_role("admin"), "admin");
        assert_eq!(map_group_to_role("administrators"), "admin");
        assert_eq!(map_group_to_role("writers"), "writer");
        assert_eq!(map_group_to_role("unknown"), "user");
    }

    #[test]
    fn test_permission_computation() {
        let perms = compute_permissions(&["admin".to_string()]);
        assert!(perms.contains(&Permission::GlobalAdmin));

        let perms = compute_permissions(&["reader".to_string()]);
        assert!(perms.contains(&Permission::Read));
    }
}
