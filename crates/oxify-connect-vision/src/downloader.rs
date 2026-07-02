//! Model downloader with progress reporting and verification.
//!
//! This module provides automatic downloading of ONNX models with:
//! - Progress reporting with indicators
//! - SHA256 checksum verification
//! - Cached downloaded models
//! - Mirror URL support for reliability

use crate::errors::{Result, VisionError};
use futures::StreamExt;
use oxicrypto_hash::Sha256;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

/// Default cache directory for downloaded models.
pub fn default_cache_dir() -> PathBuf {
    if let Some(cache_dir) = dirs::cache_dir() {
        cache_dir.join("oxify-vision-models")
    } else {
        PathBuf::from(".oxify-vision-models")
    }
}

/// Model information for downloading.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model name
    pub name: String,
    /// Primary download URL
    pub url: String,
    /// Mirror URLs (fallback)
    pub mirrors: Vec<String>,
    /// Expected SHA256 checksum
    pub sha256: String,
    /// Expected file size in bytes (optional)
    pub size_bytes: Option<u64>,
}

impl ModelInfo {
    /// Create a new model info.
    pub fn new(name: impl Into<String>, url: impl Into<String>, sha256: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            mirrors: vec![],
            sha256: sha256.into(),
            size_bytes: None,
        }
    }

    /// Add a mirror URL.
    pub fn with_mirror(mut self, mirror: impl Into<String>) -> Self {
        self.mirrors.push(mirror.into());
        self
    }

    /// Set expected file size.
    pub fn with_size(mut self, size_bytes: u64) -> Self {
        self.size_bytes = Some(size_bytes);
        self
    }

    /// Get all URLs (primary + mirrors).
    pub fn all_urls(&self) -> Vec<&str> {
        std::iter::once(self.url.as_str())
            .chain(self.mirrors.iter().map(|s| s.as_str()))
            .collect()
    }
}

/// Download progress information.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Model name
    pub model_name: String,
    /// Total bytes to download (if known)
    pub total_bytes: Option<u64>,
    /// Bytes downloaded so far
    pub downloaded_bytes: u64,
    /// Download speed (bytes per second)
    pub speed_bps: f64,
}

impl DownloadProgress {
    /// Get completion percentage (0.0 to 1.0).
    pub fn percentage(&self) -> Option<f32> {
        self.total_bytes.map(|total| {
            if total == 0 {
                1.0
            } else {
                (self.downloaded_bytes as f32 / total as f32).min(1.0)
            }
        })
    }

    /// Format as a human-readable string.
    pub fn format(&self) -> String {
        let downloaded = format_bytes(self.downloaded_bytes);
        let speed = format_bytes(self.speed_bps as u64);

        if let Some(total) = self.total_bytes {
            let total_str = format_bytes(total);
            let pct = self.percentage().unwrap_or(0.0) * 100.0;
            format!(
                "{}: {}/{} ({:.1}%) @ {}/s",
                self.model_name, downloaded, total_str, pct, speed
            )
        } else {
            format!("{}: {} @ {}/s", self.model_name, downloaded, speed)
        }
    }
}

/// Format bytes as human-readable string.
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

/// Progress callback for downloads.
pub type ProgressCallback = Arc<dyn Fn(&DownloadProgress) + Send + Sync>;

/// Configuration for model downloader.
#[derive(Debug, Clone)]
pub struct DownloaderConfig {
    /// Directory to cache downloaded models
    pub cache_dir: PathBuf,
    /// Verify checksums after download
    pub verify_checksums: bool,
    /// Timeout for downloads (seconds)
    pub timeout_secs: u64,
    /// Enable progress reporting
    pub report_progress: bool,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            cache_dir: default_cache_dir(),
            verify_checksums: true,
            timeout_secs: 600, // 10 minutes
            report_progress: true,
        }
    }
}

impl DownloaderConfig {
    /// Create a new configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set cache directory.
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        self.cache_dir = dir;
        self
    }

    /// Set whether to verify checksums.
    pub fn with_verify_checksums(mut self, verify: bool) -> Self {
        self.verify_checksums = verify;
        self
    }

    /// Set download timeout.
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set whether to report progress.
    pub fn with_report_progress(mut self, report: bool) -> Self {
        self.report_progress = report;
        self
    }
}

/// Model downloader.
pub struct ModelDownloader {
    config: DownloaderConfig,
    progress_callback: Option<ProgressCallback>,
    client: oxihttp::HttpsClient,
}

impl ModelDownloader {
    /// Create a new model downloader.
    pub fn new(config: DownloaderConfig) -> Result<Self> {
        let timeout = std::time::Duration::from_secs(config.timeout_secs);
        let client = oxihttp::Client::builder()
            .with_tls()
            .connect_timeout(timeout)
            .read_timeout(timeout)
            .build_https()
            .map_err(|e| VisionError::config(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            config,
            progress_callback: None,
            client,
        })
    }

    /// Create a downloader with default configuration.
    pub fn default_config() -> Result<Self> {
        Self::new(DownloaderConfig::default())
    }

    /// Set progress callback.
    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&DownloadProgress) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Arc::new(callback));
        self
    }

    /// Check if a model is already cached.
    pub async fn is_cached(&self, model: &ModelInfo) -> bool {
        let cache_path = self.get_cache_path(&model.name);

        if !cache_path.exists() {
            return false;
        }

        // Verify checksum if enabled
        if self.config.verify_checksums {
            verify_checksum(&cache_path, &model.sha256)
                .await
                .unwrap_or_default()
        } else {
            true
        }
    }

    /// Get the cache path for a model.
    pub fn get_cache_path(&self, model_name: &str) -> PathBuf {
        self.config.cache_dir.join(model_name)
    }

    /// Download a model if not cached.
    ///
    /// Returns the path to the model file (cached or downloaded).
    pub async fn ensure_model(&self, model: &ModelInfo) -> Result<PathBuf> {
        let cache_path = self.get_cache_path(&model.name);

        // Check if already cached
        if self.is_cached(model).await {
            tracing::debug!("Model '{}' already cached at {:?}", model.name, cache_path);
            return Ok(cache_path);
        }

        tracing::info!("Downloading model '{}'...", model.name);
        self.download_model(model).await?;

        Ok(cache_path)
    }

    /// Download a model to cache.
    async fn download_model(&self, model: &ModelInfo) -> Result<()> {
        // Create cache directory
        tokio::fs::create_dir_all(&self.config.cache_dir)
            .await
            .map_err(|e| VisionError::config(format!("Failed to create cache directory: {}", e)))?;

        let cache_path = self.get_cache_path(&model.name);
        let temp_path = cache_path.with_extension("tmp");

        // Try primary URL and mirrors
        let mut last_error = None;
        for (idx, url) in model.all_urls().iter().enumerate() {
            if idx > 0 {
                tracing::warn!("Trying mirror URL {}/{}", idx, model.all_urls().len() - 1);
            }

            match self.download_from_url(url, &temp_path, model).await {
                Ok(()) => {
                    // Verify checksum
                    if self.config.verify_checksums {
                        match verify_checksum(&temp_path, &model.sha256).await {
                            Ok(true) => {
                                // Checksum valid, move to final location
                                tokio::fs::rename(&temp_path, &cache_path)
                                    .await
                                    .map_err(|e| {
                                        VisionError::config(format!(
                                            "Failed to move model file: {}",
                                            e
                                        ))
                                    })?;
                                tracing::info!("Model '{}' downloaded successfully", model.name);
                                return Ok(());
                            }
                            Ok(false) => {
                                let err = VisionError::config(format!(
                                    "Checksum verification failed for '{}'",
                                    model.name
                                ));
                                last_error = Some(err);
                                // Try next mirror
                                continue;
                            }
                            Err(e) => {
                                last_error = Some(e);
                                continue;
                            }
                        }
                    } else {
                        // No verification, move to final location
                        tokio::fs::rename(&temp_path, &cache_path)
                            .await
                            .map_err(|e| {
                                VisionError::config(format!("Failed to move model file: {}", e))
                            })?;
                        tracing::info!("Model '{}' downloaded successfully", model.name);
                        return Ok(());
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_path).await;

        // All URLs failed
        Err(last_error.unwrap_or_else(|| {
            VisionError::config(format!("Failed to download model '{}'", model.name))
        }))
    }

    /// Download from a specific URL.
    async fn download_from_url(
        &self,
        url: &str,
        dest_path: &Path,
        model: &ModelInfo,
    ) -> Result<()> {
        tracing::debug!("Downloading from {}", url);

        let response = self
            .client
            .get(url)
            .map_err(|e| VisionError::config(format!("Download request failed: {}", e)))?
            .send()
            .await
            .map_err(|e| VisionError::config(format!("Download request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(VisionError::config(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        let total_bytes = response.content_length();
        let mut stream = response.body_stream();
        let mut file = tokio::fs::File::create(dest_path)
            .await
            .map_err(|e| VisionError::config(format!("Failed to create file: {}", e)))?;

        let mut downloaded: u64 = 0;
        let start_time = std::time::Instant::now();
        let mut last_report = start_time;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                VisionError::config(format!("Failed to read download chunk: {}", e))
            })?;

            file.write_all(&chunk)
                .await
                .map_err(|e| VisionError::config(format!("Failed to write to file: {}", e)))?;

            downloaded += chunk.len() as u64;

            // Report progress every 500ms
            let now = std::time::Instant::now();
            if self.config.report_progress && now.duration_since(last_report).as_millis() >= 500 {
                let elapsed = now.duration_since(start_time).as_secs_f64();
                let speed = if elapsed > 0.0 {
                    downloaded as f64 / elapsed
                } else {
                    0.0
                };

                let progress = DownloadProgress {
                    model_name: model.name.clone(),
                    total_bytes: total_bytes.or(model.size_bytes),
                    downloaded_bytes: downloaded,
                    speed_bps: speed,
                };

                if let Some(ref callback) = self.progress_callback {
                    callback(&progress);
                }

                last_report = now;
            }
        }

        file.flush()
            .await
            .map_err(|e| VisionError::config(format!("Failed to flush file: {}", e)))?;

        Ok(())
    }

    /// Download multiple models in parallel.
    pub async fn ensure_models(&self, models: Vec<ModelInfo>) -> Result<Vec<PathBuf>> {
        let mut results = Vec::new();

        for model in models {
            let path = self.ensure_model(&model).await?;
            results.push(path);
        }

        Ok(results)
    }
}

/// Verify SHA256 checksum of a file.
async fn verify_checksum(path: &Path, expected: &str) -> Result<bool> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| VisionError::config(format!("Failed to read file for checksum: {}", e)))?;

    let hash = Sha256.hash_fixed(&data);
    let hash_hex = hex::encode(hash);

    Ok(hash_hex.eq_ignore_ascii_case(expected))
}

/// Compute SHA256 checksum of a file.
#[allow(dead_code)]
pub async fn compute_checksum(path: &Path) -> Result<String> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| VisionError::config(format!("Failed to read file: {}", e)))?;

    let hash = Sha256.hash_fixed(&data);

    Ok(hex::encode(hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_model_info() {
        let model = ModelInfo::new("test.onnx", "https://example.com/model.onnx", "abc123")
            .with_mirror("https://mirror1.com/model.onnx")
            .with_mirror("https://mirror2.com/model.onnx")
            .with_size(1024 * 1024);

        assert_eq!(model.name, "test.onnx");
        assert_eq!(model.url, "https://example.com/model.onnx");
        assert_eq!(model.mirrors.len(), 2);
        assert_eq!(model.sha256, "abc123");
        assert_eq!(model.size_bytes, Some(1024 * 1024));
        assert_eq!(model.all_urls().len(), 3);
    }

    #[test]
    fn test_download_progress() {
        let progress = DownloadProgress {
            model_name: "test.onnx".to_string(),
            total_bytes: Some(1000),
            downloaded_bytes: 500,
            speed_bps: 1024.0 * 1024.0,
        };

        assert_eq!(progress.percentage(), Some(0.5));

        let formatted = progress.format();
        assert!(formatted.contains("test.onnx"));
        assert!(formatted.contains("50.0%"));
    }

    #[test]
    fn test_downloader_config() {
        let config = DownloaderConfig::new()
            .with_cache_dir(PathBuf::from("/tmp/cache"))
            .with_verify_checksums(false)
            .with_timeout(300)
            .with_report_progress(false);

        assert_eq!(config.cache_dir, PathBuf::from("/tmp/cache"));
        assert!(!config.verify_checksums);
        assert_eq!(config.timeout_secs, 300);
        assert!(!config.report_progress);
    }

    #[test]
    fn test_default_cache_dir() {
        let cache_dir = default_cache_dir();
        assert!(cache_dir.to_string_lossy().contains("oxify-vision-models"));
    }

    #[tokio::test]
    async fn test_downloader_creation() {
        let config = DownloaderConfig::default();
        let downloader = ModelDownloader::new(config);
        assert!(downloader.is_ok());
    }

    #[tokio::test]
    async fn test_get_cache_path() {
        let downloader = ModelDownloader::default_config().unwrap();
        let path = downloader.get_cache_path("test.onnx");
        assert!(path.to_string_lossy().contains("test.onnx"));
    }

    #[test]
    fn test_progress_format_without_total() {
        let progress = DownloadProgress {
            model_name: "model.onnx".to_string(),
            total_bytes: None,
            downloaded_bytes: 2048,
            speed_bps: 1024.0,
        };

        let formatted = progress.format();
        assert!(formatted.contains("model.onnx"));
        assert!(formatted.contains("2.00 KB"));
        assert!(!formatted.contains("%"));
    }
}
