//! Configuration management for vision/OCR system.
//!
//! This module provides:
//! - YAML/TOML configuration file support
//! - Environment variable overrides
//! - Configuration validation
//! - Hot-reload capability
//! - Default configurations for common scenarios

use crate::errors::{Result, VisionError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Main vision configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VisionConfig {
    /// Provider configuration
    #[serde(default)]
    pub provider: ProviderConfig,

    /// Cache configuration
    #[serde(default)]
    pub cache: CacheConfig,

    /// Preprocessing configuration
    #[serde(default)]
    pub preprocessing: PreprocessingConfig,

    /// Batch processing configuration
    #[serde(default)]
    pub batch: BatchConfig,

    /// Model download configuration
    #[serde(default)]
    pub downloader: DownloaderConfig,
}

impl VisionConfig {
    /// Load configuration from a YAML file.
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| VisionError::config(format!("Failed to read config file: {}", e)))?;

        Self::from_yaml_str(&content)
    }

    /// Load configuration from a YAML string.
    pub fn from_yaml_str(content: &str) -> Result<Self> {
        serde_yaml::from_str(content)
            .map_err(|e| VisionError::config(format!("Failed to parse YAML config: {}", e)))
    }

    /// Load configuration from a TOML file.
    pub fn from_toml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| VisionError::config(format!("Failed to read config file: {}", e)))?;

        Self::from_toml_str(&content)
    }

    /// Load configuration from a TOML string.
    pub fn from_toml_str(content: &str) -> Result<Self> {
        toml::from_str(content)
            .map_err(|e| VisionError::config(format!("Failed to parse TOML config: {}", e)))
    }

    /// Load configuration with environment variable overrides.
    pub fn with_env_overrides(mut self) -> Self {
        // Provider overrides
        if let Ok(provider) = std::env::var("OXIFY_VISION_PROVIDER") {
            self.provider.name = provider;
        }
        if let Ok(model_path) = std::env::var("OXIFY_VISION_MODEL_PATH") {
            self.provider.model_path = Some(PathBuf::from(model_path));
        }
        if let Ok(use_gpu) = std::env::var("OXIFY_VISION_USE_GPU") {
            self.provider.use_gpu = use_gpu.parse().unwrap_or(false);
        }

        // Cache overrides
        if let Ok(enabled) = std::env::var("OXIFY_VISION_CACHE_ENABLED") {
            self.cache.enabled = enabled.parse().unwrap_or(true);
        }
        if let Ok(max_entries) = std::env::var("OXIFY_VISION_CACHE_MAX_ENTRIES") {
            if let Ok(n) = max_entries.parse() {
                self.cache.max_entries = n;
            }
        }

        // Preprocessing overrides
        if let Ok(enabled) = std::env::var("OXIFY_VISION_PREPROCESSING_ENABLED") {
            self.preprocessing.enabled = enabled.parse().unwrap_or(false);
        }

        self
    }

    /// Validate configuration.
    pub fn validate(&self) -> Result<()> {
        // Validate provider
        self.provider.validate()?;

        // Validate cache
        self.cache.validate()?;

        // Validate batch
        self.batch.validate()?;

        Ok(())
    }

    /// Save configuration to YAML file.
    pub fn save_yaml<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_yaml::to_string(self)
            .map_err(|e| VisionError::config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(path.as_ref(), content)
            .map_err(|e| VisionError::config(format!("Failed to write config file: {}", e)))
    }

    /// Save configuration to TOML file.
    pub fn save_toml<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| VisionError::config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(path.as_ref(), content)
            .map_err(|e| VisionError::config(format!("Failed to write config file: {}", e)))
    }
}

/// Provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider name (mock, tesseract, surya, paddle)
    pub name: String,

    /// Model path for ONNX providers
    pub model_path: Option<PathBuf>,

    /// Language code (e.g., "en", "ja", "zh")
    pub language: Option<String>,

    /// Use GPU acceleration
    #[serde(default)]
    pub use_gpu: bool,

    /// GPU device ID
    #[serde(default)]
    pub gpu_device_id: u32,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: "mock".to_string(),
            model_path: None,
            language: None,
            use_gpu: false,
            gpu_device_id: 0,
        }
    }
}

impl ProviderConfig {
    /// Validate provider configuration.
    pub fn validate(&self) -> Result<()> {
        let valid_providers = ["mock", "tesseract", "surya", "paddle"];
        if !valid_providers.contains(&self.name.as_str()) {
            return Err(VisionError::config(format!(
                "Invalid provider: {}. Must be one of: {}",
                self.name,
                valid_providers.join(", ")
            )));
        }

        // ONNX providers require model_path
        if matches!(self.name.as_str(), "surya" | "paddle") && self.model_path.is_none() {
            return Err(VisionError::config(format!(
                "Provider '{}' requires model_path to be set",
                self.name
            )));
        }

        Ok(())
    }
}

/// Cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable caching
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Cache type (memory, redis, sqlite)
    #[serde(default = "default_cache_type")]
    pub cache_type: String,

    /// Maximum cache entries (for memory cache)
    #[serde(default = "default_cache_max_entries")]
    pub max_entries: usize,

    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl_secs")]
    pub ttl_seconds: u64,

    /// Redis URL (if using redis cache)
    pub redis_url: Option<String>,

    /// SQLite database path (if using sqlite cache)
    pub sqlite_path: Option<PathBuf>,
}

fn default_true() -> bool {
    true
}

fn default_cache_type() -> String {
    "memory".to_string()
}

fn default_cache_max_entries() -> usize {
    1000
}

fn default_cache_ttl_secs() -> u64 {
    3600
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_type: "memory".to_string(),
            max_entries: 1000,
            ttl_seconds: 3600,
            redis_url: None,
            sqlite_path: None,
        }
    }
}

impl CacheConfig {
    /// Validate cache configuration.
    pub fn validate(&self) -> Result<()> {
        let valid_types = ["memory", "redis", "sqlite"];
        if !valid_types.contains(&self.cache_type.as_str()) {
            return Err(VisionError::config(format!(
                "Invalid cache type: {}. Must be one of: {}",
                self.cache_type,
                valid_types.join(", ")
            )));
        }

        if self.cache_type == "redis" && self.redis_url.is_none() {
            return Err(VisionError::config(
                "Redis cache requires redis_url to be set",
            ));
        }

        if self.cache_type == "sqlite" && self.sqlite_path.is_none() {
            return Err(VisionError::config(
                "SQLite cache requires sqlite_path to be set",
            ));
        }

        Ok(())
    }

    /// Get cache TTL as Duration.
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.ttl_seconds)
    }
}

/// Preprocessing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessingConfig {
    /// Enable preprocessing
    #[serde(default)]
    pub enabled: bool,

    /// Maximum image dimension
    pub max_dimension: Option<u32>,

    /// Apply noise reduction
    #[serde(default)]
    pub denoise: bool,

    /// Enhance contrast
    #[serde(default)]
    pub enhance_contrast: bool,

    /// Apply deskewing
    #[serde(default)]
    pub deskew: bool,

    /// Remove borders
    #[serde(default)]
    pub remove_borders: bool,

    /// Convert to grayscale
    #[serde(default)]
    pub grayscale: bool,
}

impl Default for PreprocessingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_dimension: Some(4096),
            denoise: false,
            enhance_contrast: false,
            deskew: false,
            remove_borders: false,
            grayscale: false,
        }
    }
}

impl PreprocessingConfig {
    /// Create a high-quality preprocessing config.
    pub fn high_quality() -> Self {
        Self {
            enabled: true,
            max_dimension: Some(4096),
            denoise: true,
            enhance_contrast: true,
            deskew: true,
            remove_borders: true,
            grayscale: true,
        }
    }
}

/// Batch processing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Maximum concurrent operations
    #[serde(default = "default_batch_concurrency")]
    pub max_concurrency: usize,

    /// Continue on error
    #[serde(default = "default_true")]
    pub continue_on_error: bool,

    /// Report progress
    #[serde(default)]
    pub report_progress: bool,
}

fn default_batch_concurrency() -> usize {
    num_cpus::get()
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_concurrency: num_cpus::get(),
            continue_on_error: true,
            report_progress: false,
        }
    }
}

impl BatchConfig {
    /// Validate batch configuration.
    pub fn validate(&self) -> Result<()> {
        if self.max_concurrency == 0 {
            return Err(VisionError::config(
                "Batch max_concurrency must be at least 1",
            ));
        }
        Ok(())
    }
}

/// Model downloader configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderConfig {
    /// Cache directory for models
    pub cache_dir: Option<PathBuf>,

    /// Verify checksums
    #[serde(default = "default_true")]
    pub verify_checksums: bool,

    /// Download timeout in seconds
    #[serde(default = "default_download_timeout")]
    pub timeout_seconds: u64,

    /// Report progress
    #[serde(default = "default_true")]
    pub report_progress: bool,
}

fn default_download_timeout() -> u64 {
    600
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            cache_dir: None,
            verify_checksums: true,
            timeout_seconds: 600,
            report_progress: true,
        }
    }
}

/// Configuration file watcher for hot-reload.
pub struct ConfigWatcher {
    config_path: PathBuf,
    last_modified: Option<std::time::SystemTime>,
}

impl ConfigWatcher {
    /// Create a new configuration watcher.
    pub fn new<P: AsRef<Path>>(config_path: P) -> Self {
        Self {
            config_path: config_path.as_ref().to_path_buf(),
            last_modified: None,
        }
    }

    /// Check if configuration file has been modified.
    pub fn has_changed(&mut self) -> Result<bool> {
        let metadata = std::fs::metadata(&self.config_path)
            .map_err(|e| VisionError::config(format!("Failed to read config metadata: {}", e)))?;

        let modified = metadata
            .modified()
            .map_err(|e| VisionError::config(format!("Failed to get modification time: {}", e)))?;

        if let Some(last_mod) = self.last_modified {
            if modified > last_mod {
                self.last_modified = Some(modified);
                return Ok(true);
            }
        } else {
            self.last_modified = Some(modified);
        }

        Ok(false)
    }

    /// Reload configuration if changed.
    pub fn reload_if_changed(&mut self) -> Result<Option<VisionConfig>> {
        if self.has_changed()? {
            let ext = self
                .config_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            let config = match ext {
                "yaml" | "yml" => VisionConfig::from_yaml_file(&self.config_path)?,
                "toml" => VisionConfig::from_toml_file(&self.config_path)?,
                _ => {
                    return Err(VisionError::config(format!(
                        "Unsupported config file extension: {}",
                        ext
                    )))
                }
            };

            config.validate()?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = VisionConfig::default();
        assert_eq!(config.provider.name, "mock");
        assert!(config.cache.enabled);
        assert!(!config.preprocessing.enabled);
    }

    #[test]
    fn test_yaml_serialization() {
        let config = VisionConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("provider"));
        assert!(yaml.contains("cache"));

        let parsed: VisionConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.provider.name, "mock");
    }

    #[test]
    fn test_toml_serialization() {
        let config = VisionConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("provider"));
        assert!(toml_str.contains("cache"));

        let parsed: VisionConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.provider.name, "mock");
    }

    #[test]
    fn test_provider_validation() {
        let mut config = ProviderConfig::default();
        assert!(config.validate().is_ok());

        config.name = "invalid".to_string();
        assert!(config.validate().is_err());

        config.name = "surya".to_string();
        assert!(config.validate().is_err()); // Missing model_path

        config.model_path = Some(PathBuf::from("/path/to/models"));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cache_validation() {
        let mut config = CacheConfig::default();
        assert!(config.validate().is_ok());

        config.cache_type = "invalid".to_string();
        assert!(config.validate().is_err());

        config.cache_type = "redis".to_string();
        assert!(config.validate().is_err()); // Missing redis_url

        config.redis_url = Some("redis://localhost".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_batch_validation() {
        let mut config = BatchConfig::default();
        assert!(config.validate().is_ok());

        config.max_concurrency = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_cache_ttl() {
        let config = CacheConfig::default();
        let ttl = config.ttl();
        assert_eq!(ttl.as_secs(), 3600);
    }

    #[test]
    fn test_preprocessing_high_quality() {
        let config = PreprocessingConfig::high_quality();
        assert!(config.enabled);
        assert!(config.denoise);
        assert!(config.enhance_contrast);
        assert!(config.deskew);
    }

    #[test]
    fn test_env_overrides() {
        std::env::set_var("OXIFY_VISION_PROVIDER", "tesseract");
        std::env::set_var("OXIFY_VISION_USE_GPU", "true");

        let config = VisionConfig::default().with_env_overrides();
        assert_eq!(config.provider.name, "tesseract");
        assert!(config.provider.use_gpu);

        std::env::remove_var("OXIFY_VISION_PROVIDER");
        std::env::remove_var("OXIFY_VISION_USE_GPU");
    }

    #[test]
    fn test_config_watcher() {
        let mut watcher = ConfigWatcher::new("/tmp/nonexistent.yaml");
        assert!(watcher.has_changed().is_err());
    }
}
