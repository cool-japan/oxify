//! # oxify-connect-vision
//!
//! Vision/OCR connector for OxiFY workflows.
//!
//! This crate provides OCR (Optical Character Recognition) capabilities
//! for extracting text from images and documents.
//!
//! ## Features
//!
//! - **Mock provider**: For testing and development
//! - **Tesseract**: Traditional OCR engine (requires system installation)
//! - **Surya**: Deep learning OCR with layout analysis (ONNX)
//! - **PaddleOCR**: High-quality multilingual OCR (ONNX)
//! - **Google Cloud Vision**: Cloud-based OCR with Google's Vision API
//!
//! ## Usage
//!
//! ```rust,ignore
//! use oxify_connect_vision::{providers, VisionProvider};
//!
//! // Create a mock provider for testing
//! let provider = providers::MockVisionProvider::new();
//!
//! // Process an image
//! let result = provider.process_image(&image_bytes).await?;
//!
//! println!("Extracted text: {}", result.text);
//! println!("Markdown: {}", result.markdown);
//! ```
//!
//! ## Feature Flags
//!
//! - `mock` (default): Enable mock provider
//! - `tesseract`: Enable Tesseract OCR support
//! - `surya`: Enable Surya OCR (requires ONNX models)
//! - `paddle`: Enable PaddleOCR (requires ONNX models)
//! - `google-vision`: Enable Google Cloud Vision API support
//! - `cuda`: Enable CUDA GPU acceleration
//! - `coreml`: Enable CoreML GPU acceleration (macOS)
//! - `all-providers`: Enable all OCR providers

pub mod access_control;
pub mod audit;
pub mod batch;
pub mod benchmark;
pub mod cache;
pub mod config;
pub mod diagnostics;
pub mod downloader;
pub mod encryption;
pub mod errors;
pub mod form_detection;
pub mod gpu;
pub mod logging;
pub mod metrics;
pub mod model_loading;
pub mod otel;
pub mod pdf_processing;
pub mod persistent_cache;
pub mod preprocessing;
pub mod profiling;
pub mod providers;
pub mod quantization;
pub mod simd;
pub mod streaming;
pub mod table_extraction;
pub mod types;
pub mod validation;

// Re-exports for convenience
pub use access_control::{
    AccessController, AccessError, ApiKey, Permission, UsageSnapshot, UsageStats,
};
pub use audit::{
    AuditEvent, AuditEventType, AuditLogger, AuditResult, AuditSeverity, AuditStats,
    RetentionPolicy,
};
pub use batch::{
    process_batch_simple, BatchConfig, BatchItemResult, BatchProcessor, BatchProgress, BatchResult,
};
pub use benchmark::{BenchmarkResult, BenchmarkRunner, ComparisonReport, MemoryProfile};
pub use cache::{CacheKey, CacheStats, VisionCache};
pub use config::{
    BatchConfig as ConfigBatch, CacheConfig as ConfigCache, ConfigWatcher,
    DownloaderConfig as ConfigDownloader, PreprocessingConfig as ConfigPreprocessing,
    ProviderConfig as ConfigProvider, VisionConfig,
};
pub use diagnostics::{ErrorCategory, ErrorDiagnostic, SystemDiagnostics};
pub use downloader::{
    compute_checksum, default_cache_dir, DownloadProgress, DownloaderConfig, ModelDownloader,
    ModelInfo,
};
pub use encryption::{
    EncryptedData, EncryptionAlgorithm, EncryptionConfig, EncryptionError, EncryptionProvider,
    EncryptionStats, KeyDerivationFunction,
};
pub use errors::{Result, VisionError};
pub use form_detection::{
    Checkbox, FieldType, FormDetectionConfig, FormDetectionResult, FormDetector, FormField,
    RadioButton, RadioGroup, Signature,
};
pub use gpu::{GpuConfig, GpuInfo, GpuProvider};
pub use logging::{LogEntry, LogLevel, LogSampler, SamplingConfig, StructuredLogger};
pub use metrics::{Counter, Gauge, Histogram, MetricsSummary, OcrMetrics, Timer};
pub use model_loading::{
    LoadingStrategy, MemoryStats, ModelHandle, ModelLoader, ModelLoadingConfig,
};
pub use otel::{
    OtelConfig, OtelError, Span, SpanAttributes, SpanContext, SpanData, SpanEvent, SpanStatus,
    TracingProvider, TracingStats,
};
pub use pdf_processing::{
    PdfDocumentResult, PdfMetadata, PdfPage, PdfProcessingConfig, PdfProcessor, SearchResult,
    TocEntry,
};
pub use persistent_cache::{
    CacheBackend, CacheStats as PersistentCacheStats, EvictionPolicy, PersistentCache, RedisConfig,
    SqliteConfig,
};
pub use preprocessing::{ImagePreprocessor, PreprocessConfig};
pub use profiling::{
    BottleneckInfo, CallTreeNode, MemorySnapshot, ProfileEntry, Profiler, ProfilerConfig,
    ProfilingError, ProfilingReport, ReportStats,
};
pub use providers::{create_provider, ProviderCapabilities, VisionProvider, VisionProviderConfig};

#[cfg(feature = "google-vision")]
pub use providers::{CostStats, GoogleVisionConfig, GoogleVisionProvider};
pub use quantization::{
    ModelQuantizer, QuantizationBenefits, QuantizationConfig, QuantizationMethod,
    QuantizationPrecision, QuantizedModelInfo,
};
pub use simd::{SimdConfig, SimdError, SimdInstructionSet, SimdProcessor, SimdStats};
pub use streaming::{
    AsyncFrameStream, FrameMetadata, SamplingStrategy, StreamConfig, StreamError, StreamProcessor,
    StreamStats,
};
pub use table_extraction::{Table, TableCell, TableExtractionConfig, TableExtractor};
pub use types::{BlockRole, ImageInput, OcrMetadata, OcrResult, OutputFormat, TextBlock};
pub use validation::{validate_image_bytes, validate_image_file, ImageValidator, ValidationConfig};

/// Prelude module for common imports.
pub mod prelude {
    pub use crate::errors::{Result, VisionError};
    pub use crate::providers::{VisionProvider, VisionProviderConfig};
    pub use crate::types::{BlockRole, OcrResult, TextBlock};
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

    #[tokio::test]
    async fn test_mock_provider_process() {
        let provider = providers::MockVisionProvider::new();
        let result = provider.process_image(b"fake image data").await.unwrap();

        assert!(!result.text.is_empty());
        assert!(!result.markdown.is_empty());
        assert_eq!(result.metadata.provider, "mock");
    }

    #[test]
    fn test_ocr_result_serialization() {
        let result = OcrResult::from_text("Hello, World!");
        let json = serde_json::to_string(&result).unwrap();
        let parsed: OcrResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, result.text);
    }

    #[test]
    fn test_text_block_builder() {
        let block = TextBlock::new("Test")
            .with_bbox([0.1, 0.2, 0.3, 0.4])
            .with_confidence(0.95)
            .with_role(BlockRole::Header)
            .with_order(1);

        assert_eq!(block.text, "Test");
        assert_eq!(block.confidence, 0.95);
        assert_eq!(block.role, BlockRole::Header);
    }

    #[test]
    fn test_cache_operations() {
        let cache = VisionCache::new();
        let key = CacheKey::new(b"test", "mock", "markdown", None);
        let result = OcrResult::from_text("Cached result");

        cache.set(key.clone(), result);

        let cached = cache.get(&key).unwrap();
        assert_eq!(cached.text, "Cached result");
    }
}
