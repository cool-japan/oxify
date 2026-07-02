//! Plugin Marketplace and Registry Integration
//!
//! Provides integration with plugin registries for discovering,
//! downloading, and publishing plugins.
//!
//! # Features
//!
//! - Plugin search and discovery from remote registries
//! - Plugin download and installation
//! - Plugin publishing (upload to registry)
//! - Version management and updates
//! - Multiple registry support

use crate::plugin_manifest::PluginManifest;
use oxify_model::http_util::append_query_params;
use oxihttp::{Client, HttpsClient};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

/// Marketplace errors
#[derive(Error, Debug)]
pub enum MarketplaceError {
    #[error("Registry error: {0}")]
    RegistryError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    #[error("Download error: {0}")]
    DownloadError(String),

    #[error("Invalid plugin package: {0}")]
    InvalidPackage(String),

    #[error("Publication error: {0}")]
    PublicationError(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("IO error: {0}")]
    IoError(String),
}

/// Plugin search criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchCriteria {
    /// Search query (plugin name, keywords, description)
    pub query: Option<String>,
    /// Filter by category
    pub category: Option<String>,
    /// Filter by author
    pub author: Option<String>,
    /// Filter by keyword
    pub keywords: Vec<String>,
    /// Minimum version
    pub min_version: Option<String>,
    /// Maximum results
    pub limit: usize,
    /// Offset for pagination
    pub offset: usize,
}

impl Default for SearchCriteria {
    fn default() -> Self {
        Self {
            query: None,
            category: None,
            author: None,
            keywords: vec![],
            min_version: None,
            limit: 20,
            offset: 0,
        }
    }
}

impl SearchCriteria {
    /// Create a new search criteria
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the search query
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Set the category filter
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the author filter
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Add a keyword filter
    pub fn with_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.keywords.push(keyword.into());
        self
    }

    /// Set the result limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set the offset
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }
}

/// Plugin search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// Download URL
    pub download_url: String,
    /// Download count
    pub downloads: u64,
    /// Rating (0-5)
    pub rating: f32,
    /// Last updated timestamp
    pub updated_at: String,
}

/// Plugin registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Registry URL
    pub url: String,
    /// API key for authentication
    pub api_key: Option<String>,
    /// Request timeout
    pub timeout: Duration,
    /// Enable SSL verification
    pub verify_ssl: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://plugins.oxify.io".to_string(),
            api_key: None,
            timeout: Duration::from_secs(30),
            verify_ssl: true,
        }
    }
}

impl RegistryConfig {
    /// Create a new registry config
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    /// Set the API key
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Disable SSL verification (for testing)
    pub fn without_ssl_verification(mut self) -> Self {
        self.verify_ssl = false;
        self
    }
}

/// Plugin registry client
pub struct RegistryClient {
    /// Configuration
    config: RegistryConfig,
    /// HTTP client
    client: HttpsClient,
}

impl RegistryClient {
    /// Create a new registry client
    pub fn new(config: RegistryConfig) -> Result<Self, MarketplaceError> {
        let builder = Client::builder()
            .connect_timeout(config.timeout)
            .read_timeout(config.timeout);
        // oxihttp's `with_danger_accept_invalid_certs` takes no boolean, so the
        // builder chain is branched on the SSL verification flag instead.
        let builder = if config.verify_ssl {
            builder.with_tls()
        } else {
            builder.with_tls().with_danger_accept_invalid_certs()
        };
        let client = builder
            .build_https()
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        Ok(Self { config, client })
    }

    /// Search for plugins in the registry
    pub async fn search(
        &self,
        criteria: SearchCriteria,
    ) -> Result<Vec<SearchResult>, MarketplaceError> {
        let base_url = format!("{}/api/v1/plugins/search", self.config.url);

        // oxihttp has no builder-level query API; collect all parameters and
        // append them to the URL before constructing the request.
        let mut query_pairs: Vec<(String, String)> = Vec::new();
        if let Some(query) = &criteria.query {
            query_pairs.push(("q".to_string(), query.clone()));
        }
        if let Some(category) = &criteria.category {
            query_pairs.push(("category".to_string(), category.clone()));
        }
        if let Some(author) = &criteria.author {
            query_pairs.push(("author".to_string(), author.clone()));
        }
        if !criteria.keywords.is_empty() {
            query_pairs.push(("keywords".to_string(), criteria.keywords.join(",")));
        }
        query_pairs.push(("limit".to_string(), criteria.limit.to_string()));
        query_pairs.push(("offset".to_string(), criteria.offset.to_string()));

        let url = append_query_params(&base_url, &query_pairs);

        let mut request = self
            .client
            .get(&url)
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        // Add authentication
        if let Some(api_key) = &self.config.api_key {
            request = request
                .header("Authorization", &format!("Bearer {}", api_key))
                .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;
        }

        let response = request
            .send()
            .await
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MarketplaceError::RegistryError(format!(
                "Registry returned status: {}",
                response.status()
            )));
        }

        let results: Vec<SearchResult> = response
            .body_json()
            .await
            .map_err(|e| MarketplaceError::RegistryError(e.to_string()))?;

        Ok(results)
    }

    /// Get plugin details by name
    pub async fn get_plugin(&self, name: &str) -> Result<SearchResult, MarketplaceError> {
        let url = format!("{}/api/v1/plugins/{}", self.config.url, name);

        let mut request = self
            .client
            .get(&url)
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        // Add authentication
        if let Some(api_key) = &self.config.api_key {
            request = request
                .header("Authorization", &format!("Bearer {}", api_key))
                .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;
        }

        let response = request
            .send()
            .await
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        if response.status().as_u16() == 404 {
            return Err(MarketplaceError::PluginNotFound(name.to_string()));
        }

        if !response.status().is_success() {
            return Err(MarketplaceError::RegistryError(format!(
                "Registry returned status: {}",
                response.status()
            )));
        }

        let result: SearchResult = response
            .body_json()
            .await
            .map_err(|e| MarketplaceError::RegistryError(e.to_string()))?;

        Ok(result)
    }

    /// Download a plugin
    pub async fn download(
        &self,
        name: &str,
        destination: &Path,
    ) -> Result<PathBuf, MarketplaceError> {
        // Get plugin details
        let plugin_info = self.get_plugin(name).await?;

        // Download the plugin package
        let mut request = self
            .client
            .get(&plugin_info.download_url)
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        // Add authentication
        if let Some(api_key) = &self.config.api_key {
            request = request
                .header("Authorization", &format!("Bearer {}", api_key))
                .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;
        }

        let response = request
            .send()
            .await
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MarketplaceError::DownloadError(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        // Save to file
        let plugin_path = destination.join(format!("{}.tar.gz", name));
        let bytes = response
            .body_bytes()
            .await
            .map_err(|e| MarketplaceError::DownloadError(e.to_string()))?;

        std::fs::create_dir_all(destination)
            .map_err(|e| MarketplaceError::IoError(e.to_string()))?;

        std::fs::write(&plugin_path, bytes)
            .map_err(|e| MarketplaceError::IoError(e.to_string()))?;

        Ok(plugin_path)
    }

    /// Publish a plugin to the registry
    pub async fn publish(
        &self,
        manifest_path: &Path,
        package_path: &Path,
    ) -> Result<(), MarketplaceError> {
        if self.config.api_key.is_none() {
            return Err(MarketplaceError::AuthError(
                "API key is required for publishing".to_string(),
            ));
        }

        // Read the manifest
        let manifest = PluginManifest::from_file(manifest_path)
            .map_err(|e| MarketplaceError::InvalidPackage(e.to_string()))?;

        // Validate the manifest
        manifest
            .validate()
            .map_err(|e| MarketplaceError::InvalidPackage(e.to_string()))?;

        // Step 1: Create the plugin entry with manifest
        let create_url = format!("{}/api/v1/plugins", self.config.url);
        let manifest_json = serde_json::to_string(&manifest)
            .map_err(|e| MarketplaceError::InvalidPackage(e.to_string()))?;

        let create_request = self
            .client
            .post(&create_url)
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?
            .header(
                "Authorization",
                &format!(
                    "Bearer {}",
                    self.config
                        .api_key
                        .as_ref()
                        .expect("api_key required for marketplace operations")
                ),
            )
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?
            .header("Content-Type", "application/json")
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?
            .body(manifest_json);

        let create_response = create_request
            .send()
            .await
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        if !create_response.status().is_success() {
            let error_text = create_response
                .body_text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(MarketplaceError::PublicationError(format!(
                "Failed to create plugin entry: {}",
                error_text
            )));
        }

        // Step 2: Upload the package
        let upload_url = format!(
            "{}/api/v1/plugins/{}/upload",
            self.config.url, manifest.plugin.name
        );

        let package_bytes =
            std::fs::read(package_path).map_err(|e| MarketplaceError::IoError(e.to_string()))?;

        let upload_request = self
            .client
            .put(&upload_url)
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?
            .header(
                "Authorization",
                &format!(
                    "Bearer {}",
                    self.config
                        .api_key
                        .as_ref()
                        .expect("api_key required for marketplace operations")
                ),
            )
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?
            .header("Content-Type", "application/gzip")
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?
            .body(package_bytes);

        let upload_response = upload_request
            .send()
            .await
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        if !upload_response.status().is_success() {
            let error_text = upload_response
                .body_text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(MarketplaceError::PublicationError(format!(
                "Package upload failed: {}",
                error_text
            )));
        }

        Ok(())
    }

    /// Get available versions for a plugin
    pub async fn get_versions(&self, name: &str) -> Result<Vec<String>, MarketplaceError> {
        let url = format!("{}/api/v1/plugins/{}/versions", self.config.url, name);

        let mut request = self
            .client
            .get(&url)
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        // Add authentication
        if let Some(api_key) = &self.config.api_key {
            request = request
                .header("Authorization", &format!("Bearer {}", api_key))
                .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;
        }

        let response = request
            .send()
            .await
            .map_err(|e| MarketplaceError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MarketplaceError::RegistryError(format!(
                "Registry returned status: {}",
                response.status()
            )));
        }

        let versions: Vec<String> = response
            .body_json()
            .await
            .map_err(|e| MarketplaceError::RegistryError(e.to_string()))?;

        Ok(versions)
    }
}

/// Plugin marketplace manager
pub struct MarketplaceManager {
    /// Registry clients (name -> client)
    registries: HashMap<String, RegistryClient>,
    /// Default registry name
    default_registry: String,
}

impl MarketplaceManager {
    /// Create a new marketplace manager
    pub fn new() -> Self {
        Self {
            registries: HashMap::new(),
            default_registry: "default".to_string(),
        }
    }

    /// Add a registry
    pub fn add_registry(
        &mut self,
        name: impl Into<String>,
        config: RegistryConfig,
    ) -> Result<(), MarketplaceError> {
        let name = name.into();
        let client = RegistryClient::new(config)?;
        self.registries.insert(name, client);
        Ok(())
    }

    /// Set the default registry
    pub fn set_default_registry(&mut self, name: impl Into<String>) {
        self.default_registry = name.into();
    }

    /// Get a registry client
    pub fn get_registry(&self, name: &str) -> Option<&RegistryClient> {
        self.registries.get(name)
    }

    /// Get the default registry client
    pub fn default_registry(&self) -> Option<&RegistryClient> {
        self.registries.get(&self.default_registry)
    }

    /// Search all registries
    pub async fn search_all(
        &self,
        criteria: SearchCriteria,
    ) -> Result<Vec<SearchResult>, MarketplaceError> {
        let mut all_results = Vec::new();

        for client in self.registries.values() {
            match client.search(criteria.clone()).await {
                Ok(mut results) => all_results.append(&mut results),
                Err(e) => {
                    tracing::warn!("Failed to search registry: {}", e);
                }
            }
        }

        Ok(all_results)
    }

    /// Search the default registry
    pub async fn search(
        &self,
        criteria: SearchCriteria,
    ) -> Result<Vec<SearchResult>, MarketplaceError> {
        let client = self.default_registry().ok_or_else(|| {
            MarketplaceError::RegistryError("No default registry configured".to_string())
        })?;

        client.search(criteria).await
    }

    /// Download a plugin from the default registry
    pub async fn download(
        &self,
        name: &str,
        destination: &Path,
    ) -> Result<PathBuf, MarketplaceError> {
        let client = self.default_registry().ok_or_else(|| {
            MarketplaceError::RegistryError("No default registry configured".to_string())
        })?;

        client.download(name, destination).await
    }

    /// Publish a plugin to the default registry
    pub async fn publish(
        &self,
        manifest_path: &Path,
        package_path: &Path,
    ) -> Result<(), MarketplaceError> {
        let client = self.default_registry().ok_or_else(|| {
            MarketplaceError::RegistryError("No default registry configured".to_string())
        })?;

        client.publish(manifest_path, package_path).await
    }
}

impl Default for MarketplaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_criteria_default() {
        let criteria = SearchCriteria::default();
        assert_eq!(criteria.limit, 20);
        assert_eq!(criteria.offset, 0);
        assert!(criteria.query.is_none());
    }

    #[test]
    fn test_search_criteria_builder() {
        let criteria = SearchCriteria::new()
            .with_query("test")
            .with_category("transform")
            .with_author("john")
            .with_keyword("ml")
            .with_limit(50)
            .with_offset(10);

        assert_eq!(criteria.query, Some("test".to_string()));
        assert_eq!(criteria.category, Some("transform".to_string()));
        assert_eq!(criteria.author, Some("john".to_string()));
        assert_eq!(criteria.keywords, vec!["ml"]);
        assert_eq!(criteria.limit, 50);
        assert_eq!(criteria.offset, 10);
    }

    #[test]
    fn test_registry_config_default() {
        let config = RegistryConfig::default();
        assert_eq!(config.url, "https://plugins.oxify.io");
        assert!(config.api_key.is_none());
        assert!(config.verify_ssl);
    }

    #[test]
    fn test_registry_config_builder() {
        let config = RegistryConfig::new("https://custom.registry.io")
            .with_api_key("secret-key")
            .with_timeout(Duration::from_secs(60))
            .without_ssl_verification();

        assert_eq!(config.url, "https://custom.registry.io");
        assert_eq!(config.api_key, Some("secret-key".to_string()));
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert!(!config.verify_ssl);
    }

    #[test]
    fn test_marketplace_manager_creation() {
        let manager = MarketplaceManager::new();
        assert_eq!(manager.default_registry, "default");
        assert_eq!(manager.registries.len(), 0);
    }

    #[test]
    fn test_marketplace_manager_add_registry() {
        let mut manager = MarketplaceManager::new();
        let config = RegistryConfig::default();

        let result = manager.add_registry("test-registry", config);
        assert!(result.is_ok());
        assert_eq!(manager.registries.len(), 1);
    }

    #[test]
    fn test_marketplace_manager_set_default() {
        let mut manager = MarketplaceManager::new();
        manager.set_default_registry("custom");
        assert_eq!(manager.default_registry, "custom");
    }

    #[test]
    fn test_marketplace_manager_get_registry() {
        let mut manager = MarketplaceManager::new();
        let config = RegistryConfig::default();
        manager.add_registry("test", config).unwrap();

        let registry = manager.get_registry("test");
        assert!(registry.is_some());

        let missing = manager.get_registry("missing");
        assert!(missing.is_none());
    }
}
