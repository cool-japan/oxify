//! Vision/OCR provider implementations.

mod mock;

#[cfg(feature = "tesseract")]
mod tesseract;

#[cfg(feature = "surya")]
mod surya;

#[cfg(feature = "paddle")]
mod paddle;

#[cfg(feature = "google-vision")]
mod google_vision;

#[cfg(feature = "azure-vision")]
mod azure_vision;

#[cfg(feature = "aws-textract")]
pub mod aws_sigv4;
#[cfg(feature = "aws-textract")]
pub mod textract;

// Re-exports
pub use mock::MockVisionProvider;

#[cfg(feature = "tesseract")]
pub use tesseract::TesseractProvider;

#[cfg(feature = "surya")]
pub use surya::SuryaClient;

#[cfg(feature = "paddle")]
pub use paddle::PaddleOcrClient;

#[cfg(feature = "google-vision")]
pub use google_vision::{CostStats, GoogleVisionConfig, GoogleVisionProvider};

#[cfg(feature = "azure-vision")]
pub use azure_vision::{AzureVisionConfig, AzureVisionProvider, CostStats as AzureVisionCostStats};

#[cfg(feature = "aws-textract")]
pub use aws_sigv4::AwsCredentials;
#[cfg(feature = "aws-textract")]
pub use textract::{AnalyzeFeature, TextractProvider};

use crate::errors::{Result, VisionError};
use crate::types::OcrResult;
use async_trait::async_trait;

/// Configuration for vision providers.
#[derive(Debug, Clone)]
pub struct VisionProviderConfig {
    /// Provider name.
    pub provider: String,
    /// Path to model files (for ONNX-based providers).
    pub model_path: Option<String>,
    /// Output format preference.
    pub output_format: String,
    /// Whether to use GPU acceleration.
    pub use_gpu: bool,
    /// Target language(s) for OCR.
    pub language: Option<String>,
    /// Additional provider-specific options.
    pub options: std::collections::HashMap<String, String>,
}

impl Default for VisionProviderConfig {
    fn default() -> Self {
        Self {
            provider: "mock".to_string(),
            model_path: None,
            output_format: "markdown".to_string(),
            use_gpu: false,
            language: None,
            options: std::collections::HashMap::new(),
        }
    }
}

impl VisionProviderConfig {
    /// Create a new configuration for the mock provider.
    pub fn mock() -> Self {
        Self::default()
    }

    /// Create a new configuration for Tesseract.
    pub fn tesseract(language: Option<&str>) -> Self {
        Self {
            provider: "tesseract".to_string(),
            language: language.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    /// Create a new configuration for Surya.
    pub fn surya(model_path: &str, use_gpu: bool) -> Self {
        Self {
            provider: "surya".to_string(),
            model_path: Some(model_path.to_string()),
            use_gpu,
            ..Default::default()
        }
    }

    /// Create a new configuration for PaddleOCR.
    pub fn paddle(model_path: &str, use_gpu: bool) -> Self {
        Self {
            provider: "paddle".to_string(),
            model_path: Some(model_path.to_string()),
            use_gpu,
            ..Default::default()
        }
    }

    /// Create a new configuration for Google Cloud Vision.
    pub fn google_vision() -> Self {
        Self {
            provider: "google_vision".to_string(),
            ..Default::default()
        }
    }

    /// Create a new configuration for Azure Computer Vision.
    pub fn azure_vision() -> Self {
        Self {
            provider: "azure_vision".to_string(),
            ..Default::default()
        }
    }
}

/// Core trait for vision/OCR providers.
#[async_trait]
pub trait VisionProvider: Send + Sync {
    /// Process an image and extract text/layout information.
    async fn process_image(&self, image_data: &[u8]) -> Result<OcrResult>;

    /// Load the model into memory.
    /// For some providers (like Mock), this is a no-op.
    async fn load_model(&self) -> Result<()>;

    /// Unload the model from memory.
    async fn unload_model(&self) -> Result<()> {
        Ok(()) // Default no-op
    }

    /// Check if the model is loaded.
    fn is_model_loaded(&self) -> bool {
        true // Default: always ready
    }

    /// Get the provider name.
    fn provider_name(&self) -> &str;

    /// Get provider capabilities.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }
}

/// Provider capabilities.
#[derive(Debug, Clone, Default)]
pub struct ProviderCapabilities {
    /// Supports table detection and extraction.
    pub table_detection: bool,
    /// Supports layout analysis.
    pub layout_analysis: bool,
    /// Supports handwriting recognition.
    pub handwriting: bool,
    /// Supports multi-language detection.
    pub multi_language: bool,
    /// Supports GPU acceleration.
    pub gpu_acceleration: bool,
    /// Supported languages.
    pub languages: Vec<String>,
}

/// Factory function to create a provider from configuration.
pub fn create_provider(config: &VisionProviderConfig) -> Result<Box<dyn VisionProvider>> {
    match config.provider.as_str() {
        "mock" => Ok(Box::new(MockVisionProvider::new())),

        #[cfg(feature = "tesseract")]
        "tesseract" => Ok(Box::new(TesseractProvider::new(config.language.as_deref()))),

        #[cfg(feature = "surya")]
        "surya" => {
            let model_path = config
                .model_path
                .as_ref()
                .ok_or_else(|| VisionError::config("Surya requires model_path"))?;
            Ok(Box::new(SuryaClient::new(model_path, config.use_gpu)))
        }

        #[cfg(feature = "paddle")]
        "paddle" => {
            let model_path = config
                .model_path
                .as_ref()
                .ok_or_else(|| VisionError::config("PaddleOCR requires model_path"))?;
            Ok(Box::new(PaddleOcrClient::new(model_path, config.use_gpu)))
        }

        #[cfg(feature = "google-vision")]
        "google_vision" => {
            let gcp_config = google_vision::GoogleVisionConfig::default();
            Ok(Box::new(google_vision::GoogleVisionProvider::new(
                gcp_config,
            )))
        }

        #[cfg(feature = "azure-vision")]
        "azure_vision" => {
            let az_config = azure_vision::AzureVisionConfig::default();
            Ok(Box::new(azure_vision::AzureVisionProvider::new(az_config)))
        }

        provider => Err(VisionError::unsupported_provider(provider)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_mock_provider() {
        let config = VisionProviderConfig::mock();
        let provider = create_provider(&config).unwrap();
        assert_eq!(provider.provider_name(), "mock");
    }

    #[test]
    fn test_unsupported_provider() {
        let config = VisionProviderConfig {
            provider: "unsupported".to_string(),
            ..Default::default()
        };
        let result = create_provider(&config);
        assert!(result.is_err());
    }
}
