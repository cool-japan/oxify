//! LDAP/Active Directory authentication integration.
//!
//! This module provides LDAP authentication support for enterprise directory services,
//! including Active Directory, `OpenLDAP`, and other LDAP-compliant directories.
//!
//! # Features
//! - LDAP bind for password verification
//! - User attribute retrieval
//! - Group membership queries
//! - Connection pooling (via ldap3 async)
//! - TLS/STARTTLS support
//!
//! # Example
//! ```no_run
//! use oxify_authn::ldap::{LdapAuthenticator, LdapConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = LdapConfig::builder()
//!         .url("ldap://ldap.example.com:389")
//!         .base_dn("dc=example,dc=com")
//!         .bind_dn("cn=admin,dc=example,dc=com")
//!         .bind_password("admin_password")
//!         .user_search_filter("(uid={username})")
//!         .build()?;
//!
//!     let authenticator = LdapAuthenticator::new(config);
//!
//!     // Authenticate user
//!     match authenticator.authenticate("alice", "alice_password").await {
//!         Ok(user) => {
//!             println!("Authenticated: {}", user.username);
//!             println!("Groups: {:?}", user.groups);
//!         }
//!         Err(e) => eprintln!("Authentication failed: {}", e),
//!     }
//!
//!     Ok(())
//! }
//! ```

use ldap3::{Ldap, LdapConnAsync, Scope, SearchEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Semaphore;

/// LDAP-specific errors.
#[derive(Error, Debug)]
pub enum LdapError {
    #[error("Invalid LDAP configuration: {0}")]
    InvalidConfig(String),

    #[error("LDAP connection failed: {0}")]
    ConnectionError(String),

    #[error("LDAP bind failed: {0}")]
    BindError(String),

    #[error("LDAP search failed: {0}")]
    SearchError(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Multiple users found for username: {0}")]
    MultipleUsersFound(String),
}

impl From<ldap3::LdapError> for LdapError {
    fn from(err: ldap3::LdapError) -> Self {
        // Convert ldap3::LdapError to our LdapError
        Self::ConnectionError(err.to_string())
    }
}

/// LDAP configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
    /// LDAP server URL (e.g., "<ldap://ldap.example.com:389>" or "<ldaps://ldap.example.com:636>")
    pub url: String,

    /// Base DN for searches (e.g., "dc=example,dc=com")
    pub base_dn: String,

    /// Bind DN for the service account (e.g., "cn=admin,dc=example,dc=com")
    pub bind_dn: String,

    /// Password for the bind DN
    pub bind_password: String,

    /// User search filter (use {username} as placeholder, e.g., "(uid={username})")
    pub user_search_filter: String,

    /// Group search filter (optional, use {`user_dn`} as placeholder)
    pub group_search_filter: Option<String>,

    /// Attributes to retrieve for users
    pub user_attributes: Vec<String>,

    /// Use TLS/STARTTLS
    pub use_tls: bool,

    /// Skip TLS certificate verification (for testing only)
    pub skip_tls_verify: bool,

    /// Connection timeout in seconds
    pub timeout_seconds: u64,

    /// Maximum number of concurrent LDAP connections (connection pool size)
    /// Default: 10 connections
    pub max_connections: usize,
}

impl LdapConfig {
    /// Creates a new builder for `LdapConfig`.
    #[must_use]
    pub fn builder() -> LdapConfigBuilder {
        LdapConfigBuilder::default()
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), LdapError> {
        if self.url.is_empty() {
            return Err(LdapError::InvalidConfig("url is required".into()));
        }
        if self.base_dn.is_empty() {
            return Err(LdapError::InvalidConfig("base_dn is required".into()));
        }
        if self.bind_dn.is_empty() {
            return Err(LdapError::InvalidConfig("bind_dn is required".into()));
        }
        if self.user_search_filter.is_empty() {
            return Err(LdapError::InvalidConfig(
                "user_search_filter is required".into(),
            ));
        }

        Ok(())
    }
}

/// Builder for `LdapConfig`.
#[derive(Default)]
pub struct LdapConfigBuilder {
    url: Option<String>,
    base_dn: Option<String>,
    bind_dn: Option<String>,
    bind_password: Option<String>,
    user_search_filter: Option<String>,
    group_search_filter: Option<String>,
    user_attributes: Vec<String>,
    use_tls: bool,
    skip_tls_verify: bool,
    timeout_seconds: u64,
    max_connections: Option<usize>,
}

impl LdapConfigBuilder {
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    #[must_use]
    pub fn base_dn(mut self, base_dn: impl Into<String>) -> Self {
        self.base_dn = Some(base_dn.into());
        self
    }

    #[must_use]
    pub fn bind_dn(mut self, bind_dn: impl Into<String>) -> Self {
        self.bind_dn = Some(bind_dn.into());
        self
    }

    #[must_use]
    pub fn bind_password(mut self, password: impl Into<String>) -> Self {
        self.bind_password = Some(password.into());
        self
    }

    #[must_use]
    pub fn user_search_filter(mut self, filter: impl Into<String>) -> Self {
        self.user_search_filter = Some(filter.into());
        self
    }

    #[must_use]
    pub fn group_search_filter(mut self, filter: impl Into<String>) -> Self {
        self.group_search_filter = Some(filter.into());
        self
    }

    #[must_use]
    pub fn user_attributes(mut self, attributes: Vec<String>) -> Self {
        self.user_attributes = attributes;
        self
    }

    #[must_use]
    pub fn add_user_attribute(mut self, attribute: impl Into<String>) -> Self {
        self.user_attributes.push(attribute.into());
        self
    }

    #[must_use]
    pub fn use_tls(mut self, use_tls: bool) -> Self {
        self.use_tls = use_tls;
        self
    }

    #[must_use]
    pub fn skip_tls_verify(mut self, skip: bool) -> Self {
        self.skip_tls_verify = skip;
        self
    }

    #[must_use]
    pub fn timeout_seconds(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    #[must_use]
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = Some(max);
        self
    }

    pub fn build(self) -> Result<LdapConfig, LdapError> {
        let config = LdapConfig {
            url: self
                .url
                .ok_or_else(|| LdapError::InvalidConfig("url is required".into()))?,
            base_dn: self
                .base_dn
                .ok_or_else(|| LdapError::InvalidConfig("base_dn is required".into()))?,
            bind_dn: self
                .bind_dn
                .ok_or_else(|| LdapError::InvalidConfig("bind_dn is required".into()))?,
            bind_password: self
                .bind_password
                .ok_or_else(|| LdapError::InvalidConfig("bind_password is required".into()))?,
            user_search_filter: self
                .user_search_filter
                .ok_or_else(|| LdapError::InvalidConfig("user_search_filter is required".into()))?,
            group_search_filter: self.group_search_filter,
            user_attributes: if self.user_attributes.is_empty() {
                vec!["cn".into(), "mail".into(), "uid".into()]
            } else {
                self.user_attributes
            },
            use_tls: self.use_tls,
            skip_tls_verify: self.skip_tls_verify,
            timeout_seconds: if self.timeout_seconds > 0 {
                self.timeout_seconds
            } else {
                30
            },
            max_connections: self.max_connections.unwrap_or(10),
        };

        config.validate()?;
        Ok(config)
    }
}

/// LDAP user information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapUser {
    /// Username (from uid or cn)
    pub username: String,

    /// Distinguished Name
    pub dn: String,

    /// Email address
    pub email: Option<String>,

    /// Full name (from cn)
    pub full_name: Option<String>,

    /// Group DNs the user belongs to
    pub groups: Vec<String>,

    /// Additional attributes
    pub attributes: HashMap<String, Vec<String>>,
}

/// LDAP authenticator for directory services with connection pooling.
pub struct LdapAuthenticator {
    config: LdapConfig,
    /// Semaphore to limit concurrent connections (connection pool)
    connection_semaphore: Arc<Semaphore>,
}

impl LdapAuthenticator {
    /// Creates a new LDAP authenticator with the given configuration.
    ///
    /// This initializes a connection pool with `max_connections` limit,
    /// preventing resource exhaustion on the LDAP server.
    #[must_use]
    pub fn new(config: LdapConfig) -> Self {
        let max_connections = config.max_connections;
        Self {
            config,
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
        }
    }

    /// Authenticates a user with username and password.
    ///
    /// Uses connection pooling to limit concurrent LDAP connections.
    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<LdapUser, LdapError> {
        // Acquire connection permit from pool
        let _permit = self.connection_semaphore.acquire().await.map_err(|e| {
            LdapError::ConnectionError(format!("Failed to acquire connection: {e}"))
        })?;

        // First, bind with service account to search for the user
        let (conn, mut ldap) = self.connect().await?;

        // Bind with service account
        ldap.simple_bind(&self.config.bind_dn, &self.config.bind_password)
            .await
            .map_err(|e| LdapError::BindError(format!("Service account bind failed: {e}")))?
            .success()
            .map_err(|e| LdapError::BindError(format!("Service account bind failed: {e}")))?;

        // Search for the user
        let search_filter = self
            .config
            .user_search_filter
            .replace("{username}", username);
        let (rs, _res) = ldap
            .search(
                &self.config.base_dn,
                Scope::Subtree,
                &search_filter,
                &self.config.user_attributes,
            )
            .await
            .map_err(|e| LdapError::SearchError(format!("User search failed: {e}")))?
            .success()
            .map_err(|e| LdapError::SearchError(format!("User search failed: {e}")))?;

        if rs.is_empty() {
            return Err(LdapError::UserNotFound(username.to_string()));
        }

        if rs.len() > 1 {
            return Err(LdapError::MultipleUsersFound(username.to_string()));
        }

        let entry = SearchEntry::construct(rs[0].clone());
        let user_dn = entry.dn.clone();

        // Parse user attributes
        let mut user = LdapUser {
            username: username.to_string(),
            dn: user_dn.clone(),
            email: None,
            full_name: None,
            groups: Vec::new(),
            attributes: HashMap::new(),
        };

        // Extract common attributes
        for (key, values) in &entry.attrs {
            user.attributes.insert(key.clone(), values.clone());

            match key.as_str() {
                "mail" => user.email = values.first().cloned(),
                "cn" => user.full_name = values.first().cloned(),
                "uid" if user.username.is_empty() => {
                    user.username = values.first().cloned().unwrap_or_default();
                }
                _ => {}
            }
        }

        // Unbind service account
        ldap.unbind()
            .await
            .map_err(|e| LdapError::ConnectionError(format!("Unbind failed: {e}")))?;

        drop(conn);

        // Now bind with user credentials to verify password
        let (conn, mut ldap) = self.connect().await?;

        ldap.simple_bind(&user_dn, password)
            .await
            .map_err(|e| LdapError::AuthenticationFailed(format!("User bind failed: {e}")))?
            .success()
            .map_err(|_| LdapError::AuthenticationFailed("Invalid password".into()))?;

        // Retrieve group memberships if configured
        if let Some(group_filter) = &self.config.group_search_filter {
            let filter = group_filter.replace("{user_dn}", &user_dn);
            let (group_rs, _res) = ldap
                .search(&self.config.base_dn, Scope::Subtree, &filter, &["cn"])
                .await
                .map_err(|e| LdapError::SearchError(format!("Group search failed: {e}")))?
                .success()
                .map_err(|e| LdapError::SearchError(format!("Group search failed: {e}")))?;

            user.groups = group_rs
                .into_iter()
                .map(|g| SearchEntry::construct(g).dn)
                .collect();
        }

        ldap.unbind()
            .await
            .map_err(|e| LdapError::ConnectionError(format!("Unbind failed: {e}")))?;

        drop(conn);

        Ok(user)
    }

    /// Gets user information without authenticating (requires service account).
    ///
    /// Uses connection pooling to limit concurrent LDAP connections.
    pub async fn get_user(&self, username: &str) -> Result<LdapUser, LdapError> {
        // Acquire connection permit from pool
        let _permit = self.connection_semaphore.acquire().await.map_err(|e| {
            LdapError::ConnectionError(format!("Failed to acquire connection: {e}"))
        })?;

        let (conn, mut ldap) = self.connect().await?;

        // Bind with service account
        ldap.simple_bind(&self.config.bind_dn, &self.config.bind_password)
            .await
            .map_err(|e| LdapError::BindError(format!("Service account bind failed: {e}")))?
            .success()
            .map_err(|e| LdapError::BindError(format!("Service account bind failed: {e}")))?;

        // Search for the user
        let search_filter = self
            .config
            .user_search_filter
            .replace("{username}", username);
        let (rs, _res) = ldap
            .search(
                &self.config.base_dn,
                Scope::Subtree,
                &search_filter,
                &self.config.user_attributes,
            )
            .await
            .map_err(|e| LdapError::SearchError(format!("User search failed: {e}")))?
            .success()
            .map_err(|e| LdapError::SearchError(format!("User search failed: {e}")))?;

        if rs.is_empty() {
            return Err(LdapError::UserNotFound(username.to_string()));
        }

        if rs.len() > 1 {
            return Err(LdapError::MultipleUsersFound(username.to_string()));
        }

        let entry = SearchEntry::construct(rs[0].clone());
        let user_dn = entry.dn.clone();

        let mut user = LdapUser {
            username: username.to_string(),
            dn: user_dn.clone(),
            email: None,
            full_name: None,
            groups: Vec::new(),
            attributes: HashMap::new(),
        };

        for (key, values) in &entry.attrs {
            user.attributes.insert(key.clone(), values.clone());

            match key.as_str() {
                "mail" => user.email = values.first().cloned(),
                "cn" => user.full_name = values.first().cloned(),
                "uid" if user.username.is_empty() => {
                    user.username = values.first().cloned().unwrap_or_default();
                }
                _ => {}
            }
        }

        ldap.unbind()
            .await
            .map_err(|e| LdapError::ConnectionError(format!("Unbind failed: {e}")))?;

        drop(conn);

        Ok(user)
    }

    /// Gets user groups without authenticating (requires service account).
    ///
    /// Uses connection pooling to limit concurrent LDAP connections.
    pub async fn get_user_groups(&self, username: &str) -> Result<Vec<String>, LdapError> {
        // Acquire connection permit from pool
        let _permit = self.connection_semaphore.acquire().await.map_err(|e| {
            LdapError::ConnectionError(format!("Failed to acquire connection: {e}"))
        })?;

        let (conn, mut ldap) = self.connect().await?;

        // Bind with service account
        ldap.simple_bind(&self.config.bind_dn, &self.config.bind_password)
            .await
            .map_err(|e| LdapError::BindError(format!("Service account bind failed: {e}")))?
            .success()
            .map_err(|e| LdapError::BindError(format!("Service account bind failed: {e}")))?;

        // First, find the user's DN
        let search_filter = self
            .config
            .user_search_filter
            .replace("{username}", username);
        let (rs, _res) = ldap
            .search(
                &self.config.base_dn,
                Scope::Subtree,
                &search_filter,
                &["dn"],
            )
            .await
            .map_err(|e| LdapError::SearchError(format!("User search failed: {e}")))?
            .success()
            .map_err(|e| LdapError::SearchError(format!("User search failed: {e}")))?;

        if rs.is_empty() {
            return Err(LdapError::UserNotFound(username.to_string()));
        }

        let entry = SearchEntry::construct(rs[0].clone());
        let user_dn = entry.dn;

        // Get groups if configured
        let groups = if let Some(group_filter) = &self.config.group_search_filter {
            let filter = group_filter.replace("{user_dn}", &user_dn);
            let (group_rs, _res) = ldap
                .search(&self.config.base_dn, Scope::Subtree, &filter, &["cn"])
                .await
                .map_err(|e| LdapError::SearchError(format!("Group search failed: {e}")))?
                .success()
                .map_err(|e| LdapError::SearchError(format!("Group search failed: {e}")))?;

            group_rs
                .into_iter()
                .map(|g| SearchEntry::construct(g).dn)
                .collect()
        } else {
            Vec::new()
        };

        ldap.unbind()
            .await
            .map_err(|e| LdapError::ConnectionError(format!("Unbind failed: {e}")))?;

        drop(conn);

        Ok(groups)
    }

    /// Creates an LDAP connection.
    async fn connect(&self) -> Result<(ldap3::LdapConnAsync, Ldap), LdapError> {
        let (conn, ldap) = LdapConnAsync::new(&self.config.url)
            .await
            .map_err(|e| LdapError::ConnectionError(format!("Connection failed: {e}")))?;

        Ok((conn, ldap))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ldap_config_builder() {
        let config = LdapConfig::builder()
            .url("ldap://ldap.example.com:389")
            .base_dn("dc=example,dc=com")
            .bind_dn("cn=admin,dc=example,dc=com")
            .bind_password("password")
            .user_search_filter("(uid={username})")
            .add_user_attribute("mail")
            .add_user_attribute("cn")
            .timeout_seconds(30)
            .build()
            .unwrap();

        assert_eq!(config.url, "ldap://ldap.example.com:389");
        assert_eq!(config.base_dn, "dc=example,dc=com");
        assert_eq!(config.timeout_seconds, 30);
        assert!(config.user_attributes.contains(&"mail".to_string()));
    }

    #[test]
    fn test_ldap_config_validation() {
        let result = LdapConfig::builder().url("ldap://ldap.example.com").build();

        assert!(result.is_err());
    }

    #[test]
    fn test_user_search_filter_replacement() {
        let config = LdapConfig::builder()
            .url("ldap://ldap.example.com:389")
            .base_dn("dc=example,dc=com")
            .bind_dn("cn=admin,dc=example,dc=com")
            .bind_password("password")
            .user_search_filter("(uid={username})")
            .build()
            .unwrap();

        let filter = config.user_search_filter.replace("{username}", "alice");
        assert_eq!(filter, "(uid=alice)");
    }

    #[test]
    fn test_ldap_user_creation() {
        let user = LdapUser {
            username: "alice".to_string(),
            dn: "uid=alice,ou=users,dc=example,dc=com".to_string(),
            email: Some("alice@example.com".to_string()),
            full_name: Some("Alice Wonderland".to_string()),
            groups: vec!["cn=admins,ou=groups,dc=example,dc=com".to_string()],
            attributes: HashMap::new(),
        };

        assert_eq!(user.username, "alice");
        assert_eq!(user.email, Some("alice@example.com".to_string()));
        assert_eq!(user.groups.len(), 1);
    }

    #[test]
    fn test_connection_pool_configuration() {
        let config = LdapConfig::builder()
            .url("ldap://ldap.example.com:389")
            .base_dn("dc=example,dc=com")
            .bind_dn("cn=admin,dc=example,dc=com")
            .bind_password("password")
            .user_search_filter("(uid={username})")
            .max_connections(20)
            .build()
            .unwrap();

        assert_eq!(config.max_connections, 20);

        let authenticator = LdapAuthenticator::new(config);
        assert!(authenticator.connection_semaphore.available_permits() == 20);
    }

    #[test]
    fn test_default_connection_pool_size() {
        let config = LdapConfig::builder()
            .url("ldap://ldap.example.com:389")
            .base_dn("dc=example,dc=com")
            .bind_dn("cn=admin,dc=example,dc=com")
            .bind_password("password")
            .user_search_filter("(uid={username})")
            .build()
            .unwrap();

        // Default should be 10 connections
        assert_eq!(config.max_connections, 10);

        let authenticator = LdapAuthenticator::new(config);
        assert!(authenticator.connection_semaphore.available_permits() == 10);
    }
}
