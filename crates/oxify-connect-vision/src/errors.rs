//! Error types for vision/OCR operations.

use crate::diagnostics::ErrorDiagnostic;
use thiserror::Error;

/// Errors that can occur during vision/OCR processing.
#[derive(Debug, Error)]
pub enum VisionError {
    /// Failed to load the model.
    #[error("Failed to load model: {0}")]
    ModelLoad(String),

    /// Model is not loaded.
    #[error("Model is not loaded. Call load_model() first.")]
    ModelNotLoaded,

    /// Failed to process the image.
    #[error("Image processing error: {0}")]
    ImageProcessing(String),

    /// Invalid image format.
    #[error("Invalid image format: {0}")]
    InvalidFormat(String),

    /// Image decoding error.
    #[error("Failed to decode image: {0}")]
    ImageDecode(String),

    /// OCR engine error.
    #[error("OCR engine error: {0}")]
    OcrEngine(String),

    /// ONNX runtime error.
    #[error("ONNX runtime error: {0}")]
    OnnxRuntime(String),

    /// Tesseract error.
    #[error("Tesseract error: {0}")]
    Tesseract(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Unsupported provider.
    #[error("Unsupported provider: {0}")]
    UnsupportedProvider(String),

    /// Timeout error.
    #[error("Processing timeout after {0}ms")]
    Timeout(u64),

    /// Resource exhaustion.
    #[error("Resource exhaustion: {0}")]
    ResourceExhaustion(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP client/transport error (oxihttp).
    #[error("HTTP error: {0}")]
    Http(#[from] oxihttp::OxiHttpError),

    /// Other error.
    #[error("{0}")]
    Other(String),
}

impl VisionError {
    /// Create a model load error.
    pub fn model_load(msg: impl Into<String>) -> Self {
        VisionError::ModelLoad(msg.into())
    }

    /// Create an image processing error.
    pub fn image_processing(msg: impl Into<String>) -> Self {
        VisionError::ImageProcessing(msg.into())
    }

    /// Create an OCR engine error.
    pub fn ocr_engine(msg: impl Into<String>) -> Self {
        VisionError::OcrEngine(msg.into())
    }

    /// Create an ONNX runtime error.
    pub fn onnx_runtime(msg: impl Into<String>) -> Self {
        VisionError::OnnxRuntime(msg.into())
    }

    /// Create a tesseract error.
    pub fn tesseract(msg: impl Into<String>) -> Self {
        VisionError::Tesseract(msg.into())
    }

    /// Create a configuration error.
    pub fn config(msg: impl Into<String>) -> Self {
        VisionError::Config(msg.into())
    }

    /// Create an unsupported provider error.
    pub fn unsupported_provider(provider: impl Into<String>) -> Self {
        VisionError::UnsupportedProvider(provider.into())
    }

    /// Get diagnostic information for this error.
    ///
    /// Returns helpful suggestions, documentation links, and system diagnostics
    /// to help troubleshoot the error.
    pub fn diagnostic(&self) -> ErrorDiagnostic {
        match self {
            VisionError::ModelLoad(msg) => {
                // Extract path from error message if possible
                let path = msg.split(':').next_back().unwrap_or(msg).trim();
                ErrorDiagnostic::model_load(path)
            }
            VisionError::ModelNotLoaded => ErrorDiagnostic::model_not_loaded(),
            VisionError::ImageProcessing(msg) | VisionError::ImageDecode(msg) => {
                ErrorDiagnostic::image_format(msg)
            }
            VisionError::InvalidFormat(msg) => ErrorDiagnostic::image_format(msg),
            VisionError::OnnxRuntime(msg) => ErrorDiagnostic::onnx_runtime(msg),
            VisionError::Tesseract(msg) => ErrorDiagnostic::tesseract(msg),
            VisionError::Config(msg) => ErrorDiagnostic::configuration(msg),
            VisionError::ResourceExhaustion(msg) => ErrorDiagnostic::resource_exhaustion(msg),
            VisionError::OcrEngine(msg) => {
                // Determine which provider based on message content
                if msg.contains("Tesseract") || msg.contains("tesseract") {
                    ErrorDiagnostic::tesseract(msg)
                } else if msg.contains("ONNX") || msg.contains("onnx") {
                    ErrorDiagnostic::onnx_runtime(msg)
                } else {
                    ErrorDiagnostic::configuration(msg)
                }
            }
            _ => ErrorDiagnostic::configuration(&self.to_string()),
        }
    }

    /// Get a user-friendly error message with diagnostic information.
    ///
    /// This includes the error message plus suggestions and system diagnostics.
    pub fn with_diagnostics(&self) -> String {
        format!("{}\n{}", self, self.diagnostic().format())
    }
}

/// Result type for vision operations.
pub type Result<T> = std::result::Result<T, VisionError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = VisionError::model_load("Model file not found");
        assert_eq!(
            err.to_string(),
            "Failed to load model: Model file not found"
        );
    }

    #[test]
    fn test_error_variants() {
        let errors = vec![
            VisionError::ModelNotLoaded,
            VisionError::image_processing("test"),
            VisionError::ocr_engine("test"),
            VisionError::config("test"),
            VisionError::unsupported_provider("test"),
            VisionError::Timeout(1000),
        ];

        for err in errors {
            // Just ensure Display works
            let _ = err.to_string();
        }
    }

    #[test]
    fn test_error_diagnostics() {
        let err = VisionError::ModelNotLoaded;
        let diag = err.diagnostic();
        assert!(!diag.suggestions.is_empty());

        let err = VisionError::model_load("/nonexistent/model.onnx");
        let diag = err.diagnostic();
        assert!(!diag.suggestions.is_empty());
        assert!(diag.system_info.is_some());
    }

    #[test]
    fn test_error_with_diagnostics() {
        let err = VisionError::ModelNotLoaded;
        let msg = err.with_diagnostics();
        assert!(msg.contains("Call provider.load_model()"));
        assert!(msg.contains("Suggestions"));
    }

    #[test]
    fn test_onnx_runtime_diagnostics() {
        let err = VisionError::onnx_runtime("CUDA memory allocation failed");
        let diag = err.diagnostic();
        assert!(!diag.suggestions.is_empty());
        assert!(diag.system_info.is_some());
    }

    #[test]
    fn test_tesseract_diagnostics() {
        let err = VisionError::tesseract("Language data not found");
        let diag = err.diagnostic();
        assert!(!diag.suggestions.is_empty());
    }

    #[test]
    fn test_image_format_diagnostics() {
        let err = VisionError::InvalidFormat("decode error".to_string());
        let diag = err.diagnostic();
        assert!(!diag.suggestions.is_empty());
    }
}
