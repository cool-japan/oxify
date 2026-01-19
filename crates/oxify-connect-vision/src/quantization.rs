//! Model quantization support for performance optimization.
//!
//! This module provides support for quantized ONNX models:
//! - INT8 quantization for CPU inference
//! - FP16 quantization for GPU inference
//! - Dynamic quantization
//! - Quantization configuration and validation
//!
//! Quantized models can significantly reduce:
//! - Model size (2-4x smaller)
//! - Memory usage (2-4x less)
//! - Inference latency (1.5-3x faster)
//!
//! With minimal accuracy loss (typically <1%).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Quantization precision level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuantizationPrecision {
    /// Full precision (FP32) - no quantization
    #[default]
    FP32,
    /// Half precision (FP16) - good for GPU
    FP16,
    /// 8-bit integer (INT8) - good for CPU
    INT8,
    /// Mixed precision (automatic selection)
    Mixed,
}

impl QuantizationPrecision {
    /// Get the size reduction factor compared to FP32
    pub fn size_reduction_factor(&self) -> f32 {
        match self {
            Self::FP32 => 1.0,
            Self::FP16 => 2.0,
            Self::INT8 => 4.0,
            Self::Mixed => 2.5, // Approximate
        }
    }

    /// Get the expected speedup factor compared to FP32
    pub fn speedup_factor(&self) -> f32 {
        match self {
            Self::FP32 => 1.0,
            Self::FP16 => 1.5,
            Self::INT8 => 2.5,
            Self::Mixed => 2.0, // Approximate
        }
    }

    /// Get the typical accuracy loss percentage
    pub fn accuracy_loss(&self) -> f32 {
        match self {
            Self::FP32 => 0.0,
            Self::FP16 => 0.1,  // ~0.1% loss
            Self::INT8 => 0.5,  // ~0.5% loss
            Self::Mixed => 0.3, // ~0.3% loss
        }
    }

    /// Check if this precision is suitable for GPU
    pub fn is_gpu_suitable(&self) -> bool {
        matches!(self, Self::FP16 | Self::Mixed)
    }

    /// Check if this precision is suitable for CPU
    pub fn is_cpu_suitable(&self) -> bool {
        matches!(self, Self::INT8 | Self::Mixed | Self::FP32)
    }
}

impl std::fmt::Display for QuantizationPrecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FP32 => write!(f, "FP32"),
            Self::FP16 => write!(f, "FP16"),
            Self::INT8 => write!(f, "INT8"),
            Self::Mixed => write!(f, "Mixed"),
        }
    }
}

/// Quantization method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuantizationMethod {
    /// Static quantization (requires calibration data)
    Static,
    /// Dynamic quantization (no calibration needed)
    #[default]
    Dynamic,
    /// Quantization-aware training (QAT)
    QAT,
}

/// Configuration for model quantization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationConfig {
    /// Quantization precision
    pub precision: QuantizationPrecision,

    /// Quantization method
    pub method: QuantizationMethod,

    /// Whether to use symmetric quantization
    pub symmetric: bool,

    /// Whether to quantize per-channel
    pub per_channel: bool,

    /// Layers to exclude from quantization (e.g., first/last layers)
    pub exclude_layers: Vec<String>,

    /// Calibration data size (for static quantization)
    pub calibration_size: Option<usize>,

    /// Target accuracy threshold (fail if accuracy drops more than this)
    pub min_accuracy: Option<f32>,
}

impl QuantizationConfig {
    /// Create a new quantization config with defaults
    pub fn new(precision: QuantizationPrecision) -> Self {
        Self {
            precision,
            method: QuantizationMethod::Dynamic,
            symmetric: true,
            per_channel: true,
            exclude_layers: vec![],
            calibration_size: None,
            min_accuracy: None,
        }
    }

    /// Create a config for INT8 CPU inference
    pub fn int8_cpu() -> Self {
        Self {
            precision: QuantizationPrecision::INT8,
            method: QuantizationMethod::Dynamic,
            symmetric: true,
            per_channel: true,
            exclude_layers: vec![],
            calibration_size: None,
            min_accuracy: Some(0.99), // Allow 1% accuracy loss
        }
    }

    /// Create a config for FP16 GPU inference
    pub fn fp16_gpu() -> Self {
        Self {
            precision: QuantizationPrecision::FP16,
            method: QuantizationMethod::Dynamic,
            symmetric: false,
            per_channel: false,
            exclude_layers: vec![],
            calibration_size: None,
            min_accuracy: Some(0.999), // Allow 0.1% accuracy loss
        }
    }

    /// Create a config for static quantization
    pub fn static_quantization(precision: QuantizationPrecision, calibration_size: usize) -> Self {
        Self {
            precision,
            method: QuantizationMethod::Static,
            symmetric: true,
            per_channel: true,
            exclude_layers: vec![],
            calibration_size: Some(calibration_size),
            min_accuracy: Some(0.98),
        }
    }

    /// Exclude specific layers from quantization
    pub fn exclude_layer(mut self, layer_name: impl Into<String>) -> Self {
        self.exclude_layers.push(layer_name.into());
        self
    }

    /// Set minimum accuracy threshold
    pub fn with_min_accuracy(mut self, accuracy: f32) -> Self {
        self.min_accuracy = Some(accuracy);
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Check if calibration is required but not provided
        if self.method == QuantizationMethod::Static && self.calibration_size.is_none() {
            return Err("Static quantization requires calibration_size".to_string());
        }

        // Check minimum accuracy
        if let Some(min_acc) = self.min_accuracy {
            if !(0.0..=1.0).contains(&min_acc) {
                return Err("min_accuracy must be between 0.0 and 1.0".to_string());
            }
        }

        Ok(())
    }
}

impl Default for QuantizationConfig {
    fn default() -> Self {
        Self::new(QuantizationPrecision::FP32)
    }
}

/// Information about a quantized model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedModelInfo {
    /// Original model path
    pub original_path: PathBuf,

    /// Quantized model path
    pub quantized_path: PathBuf,

    /// Quantization configuration used
    pub config: QuantizationConfig,

    /// Original model size in bytes
    pub original_size: u64,

    /// Quantized model size in bytes
    pub quantized_size: u64,

    /// Size reduction ratio
    pub size_reduction: f32,

    /// Measured speedup (if available)
    pub speedup: Option<f32>,

    /// Accuracy on validation set (if available)
    pub accuracy: Option<f32>,
}

impl QuantizedModelInfo {
    /// Create a new quantized model info
    pub fn new(
        original_path: PathBuf,
        quantized_path: PathBuf,
        config: QuantizationConfig,
    ) -> Self {
        Self {
            original_path,
            quantized_path,
            config,
            original_size: 0,
            quantized_size: 0,
            size_reduction: 0.0,
            speedup: None,
            accuracy: None,
        }
    }

    /// Set the model sizes
    pub fn with_sizes(mut self, original: u64, quantized: u64) -> Self {
        self.original_size = original;
        self.quantized_size = quantized;
        self.size_reduction = original as f32 / quantized as f32;
        self
    }

    /// Set the speedup factor
    pub fn with_speedup(mut self, speedup: f32) -> Self {
        self.speedup = Some(speedup);
        self
    }

    /// Set the accuracy
    pub fn with_accuracy(mut self, accuracy: f32) -> Self {
        self.accuracy = Some(accuracy);
        self
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        format!(
            "Quantization: {} -> {:.2}x smaller",
            self.config.precision, self.size_reduction
        )
    }
}

/// Model quantizer (placeholder for actual quantization logic)
pub struct ModelQuantizer {
    config: QuantizationConfig,
}

impl ModelQuantizer {
    /// Create a new quantizer with the given configuration
    pub fn new(config: QuantizationConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Create a quantizer for INT8 CPU inference
    pub fn int8_cpu() -> Result<Self, String> {
        Self::new(QuantizationConfig::int8_cpu())
    }

    /// Create a quantizer for FP16 GPU inference
    pub fn fp16_gpu() -> Result<Self, String> {
        Self::new(QuantizationConfig::fp16_gpu())
    }

    /// Quantize a model
    ///
    /// Note: This is a placeholder. Actual quantization would require:
    /// 1. Loading the ONNX model
    /// 2. Converting weights to lower precision
    /// 3. Optionally calibrating with sample data
    /// 4. Saving the quantized model
    pub fn quantize<P: AsRef<Path>>(
        &self,
        model_path: P,
        output_path: P,
    ) -> Result<QuantizedModelInfo, String> {
        let model_path = model_path.as_ref();
        let output_path = output_path.as_ref();

        info!(
            "Quantizing model {} to {} precision",
            model_path.display(),
            self.config.precision
        );

        // Validate input
        if !model_path.exists() {
            return Err(format!("Model not found: {}", model_path.display()));
        }

        // Get original model size
        let original_size = std::fs::metadata(model_path)
            .map_err(|e| format!("Failed to get model size: {}", e))?
            .len();

        debug!("Original model size: {} bytes", original_size);

        // In a real implementation, this would:
        // 1. Load the ONNX model with ort
        // 2. Apply quantization transformations
        // 3. Save the quantized model
        //
        // For now, we just log a warning
        warn!(
            "Model quantization is a placeholder. \
             Actual quantization requires ONNX Runtime quantization tools."
        );

        // Estimate quantized size based on precision
        let estimated_size =
            (original_size as f32 / self.config.precision.size_reduction_factor()) as u64;

        let info = QuantizedModelInfo::new(
            model_path.to_path_buf(),
            output_path.to_path_buf(),
            self.config.clone(),
        )
        .with_sizes(original_size, estimated_size);

        Ok(info)
    }

    /// Get the configuration
    pub fn config(&self) -> &QuantizationConfig {
        &self.config
    }

    /// Check if a model is already quantized
    pub fn is_quantized<P: AsRef<Path>>(model_path: P) -> bool {
        let path = model_path.as_ref();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Check for common quantization indicators in filename
        file_name.contains("int8")
            || file_name.contains("fp16")
            || file_name.contains("quantized")
            || file_name.contains("quant")
    }

    /// Estimate the benefits of quantization
    pub fn estimate_benefits(&self, model_size_bytes: u64) -> QuantizationBenefits {
        let size_reduction = self.config.precision.size_reduction_factor();
        let speedup = self.config.precision.speedup_factor();
        let accuracy_loss = self.config.precision.accuracy_loss();

        QuantizationBenefits {
            original_size_mb: model_size_bytes as f32 / (1024.0 * 1024.0),
            quantized_size_mb: model_size_bytes as f32 / (1024.0 * 1024.0) / size_reduction,
            size_reduction_factor: size_reduction,
            expected_speedup: speedup,
            expected_accuracy_loss: accuracy_loss,
        }
    }
}

/// Estimated benefits of quantization
#[derive(Debug, Clone)]
pub struct QuantizationBenefits {
    pub original_size_mb: f32,
    pub quantized_size_mb: f32,
    pub size_reduction_factor: f32,
    pub expected_speedup: f32,
    pub expected_accuracy_loss: f32,
}

impl std::fmt::Display for QuantizationBenefits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Quantization Benefits Estimate:")?;
        writeln!(f, "  Original Size:     {:.2} MB", self.original_size_mb)?;
        writeln!(f, "  Quantized Size:    {:.2} MB", self.quantized_size_mb)?;
        writeln!(f, "  Size Reduction:    {:.2}x", self.size_reduction_factor)?;
        writeln!(f, "  Expected Speedup:  {:.2}x", self.expected_speedup)?;
        writeln!(
            f,
            "  Accuracy Loss:     ~{:.1}%",
            self.expected_accuracy_loss
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_quantization_precision() {
        assert_eq!(QuantizationPrecision::FP32.size_reduction_factor(), 1.0);
        assert_eq!(QuantizationPrecision::FP16.size_reduction_factor(), 2.0);
        assert_eq!(QuantizationPrecision::INT8.size_reduction_factor(), 4.0);
    }

    #[test]
    fn test_quantization_precision_gpu_suitable() {
        assert!(QuantizationPrecision::FP16.is_gpu_suitable());
        assert!(!QuantizationPrecision::INT8.is_gpu_suitable());
    }

    #[test]
    fn test_quantization_precision_cpu_suitable() {
        assert!(QuantizationPrecision::INT8.is_cpu_suitable());
        assert!(QuantizationPrecision::FP32.is_cpu_suitable());
    }

    #[test]
    fn test_quantization_config_int8() {
        let config = QuantizationConfig::int8_cpu();
        assert_eq!(config.precision, QuantizationPrecision::INT8);
        assert_eq!(config.method, QuantizationMethod::Dynamic);
        assert!(config.symmetric);
    }

    #[test]
    fn test_quantization_config_fp16() {
        let config = QuantizationConfig::fp16_gpu();
        assert_eq!(config.precision, QuantizationPrecision::FP16);
        assert_eq!(config.method, QuantizationMethod::Dynamic);
    }

    #[test]
    fn test_quantization_config_exclude_layer() {
        let config = QuantizationConfig::int8_cpu().exclude_layer("input");
        assert_eq!(config.exclude_layers, vec!["input"]);
    }

    #[test]
    fn test_quantization_config_validation() {
        let config = QuantizationConfig::int8_cpu();
        assert!(config.validate().is_ok());

        let invalid = QuantizationConfig {
            method: QuantizationMethod::Static,
            calibration_size: None,
            ..QuantizationConfig::int8_cpu()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_model_quantizer_creation() {
        let config = QuantizationConfig::int8_cpu();
        let quantizer = ModelQuantizer::new(config);
        assert!(quantizer.is_ok());
    }

    #[test]
    fn test_model_quantizer_int8() {
        let quantizer = ModelQuantizer::int8_cpu();
        assert!(quantizer.is_ok());
        assert_eq!(
            quantizer.unwrap().config().precision,
            QuantizationPrecision::INT8
        );
    }

    #[test]
    fn test_model_quantizer_fp16() {
        let quantizer = ModelQuantizer::fp16_gpu();
        assert!(quantizer.is_ok());
        assert_eq!(
            quantizer.unwrap().config().precision,
            QuantizationPrecision::FP16
        );
    }

    #[test]
    fn test_is_quantized() {
        assert!(ModelQuantizer::is_quantized("model_int8.onnx"));
        assert!(ModelQuantizer::is_quantized("model_fp16.onnx"));
        assert!(ModelQuantizer::is_quantized("model_quantized.onnx"));
        assert!(!ModelQuantizer::is_quantized("model.onnx"));
    }

    #[test]
    fn test_quantize_model_not_found() {
        let quantizer = ModelQuantizer::int8_cpu().unwrap();
        let result = quantizer.quantize("nonexistent.onnx", "output.onnx");
        assert!(result.is_err());
    }

    #[test]
    fn test_quantize_model() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"fake model data").unwrap();

        let quantizer = ModelQuantizer::int8_cpu().unwrap();
        let output_path = PathBuf::from("output.onnx");
        let result = quantizer.quantize(temp_file.path(), &output_path);

        assert!(result.is_ok());
        let info = result.unwrap();
        assert!(info.size_reduction > 0.0);
    }

    #[test]
    fn test_quantized_model_info() {
        let info = QuantizedModelInfo::new(
            PathBuf::from("input.onnx"),
            PathBuf::from("output.onnx"),
            QuantizationConfig::int8_cpu(),
        )
        .with_sizes(1000, 250);

        assert_eq!(info.size_reduction, 4.0);
        assert!(info.summary().contains("INT8"));
    }

    #[test]
    fn test_estimate_benefits() {
        let quantizer = ModelQuantizer::int8_cpu().unwrap();
        let benefits = quantizer.estimate_benefits(100 * 1024 * 1024); // 100 MB

        assert!(benefits.original_size_mb > 99.0);
        assert!(benefits.quantized_size_mb < benefits.original_size_mb);
        assert!(benefits.size_reduction_factor > 1.0);
    }

    #[test]
    fn test_quantization_precision_display() {
        assert_eq!(format!("{}", QuantizationPrecision::FP32), "FP32");
        assert_eq!(format!("{}", QuantizationPrecision::FP16), "FP16");
        assert_eq!(format!("{}", QuantizationPrecision::INT8), "INT8");
    }

    #[test]
    fn test_quantization_method_default() {
        let method = QuantizationMethod::default();
        assert_eq!(method, QuantizationMethod::Dynamic);
    }

    #[test]
    fn test_benefits_display() {
        let benefits = QuantizationBenefits {
            original_size_mb: 100.0,
            quantized_size_mb: 25.0,
            size_reduction_factor: 4.0,
            expected_speedup: 2.5,
            expected_accuracy_loss: 0.5,
        };

        let display = format!("{}", benefits);
        assert!(display.contains("100.00 MB"));
        assert!(display.contains("25.00 MB"));
    }
}
