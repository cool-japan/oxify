//! Diagnostic utilities for vision/OCR troubleshooting.
//!
//! This module provides system diagnostics, error suggestions,
//! and troubleshooting helpers for common vision/OCR issues.

use std::path::Path;

/// System diagnostic information.
#[derive(Debug, Clone)]
pub struct SystemDiagnostics {
    /// Operating system name
    pub os: String,
    /// CPU architecture
    pub arch: String,
    /// Available memory (MB)
    pub memory_mb: Option<u64>,
    /// CUDA availability
    pub cuda_available: bool,
    /// CoreML availability
    pub coreml_available: bool,
    /// Tesseract installation detected
    pub tesseract_installed: bool,
}

impl SystemDiagnostics {
    /// Collect system diagnostics.
    pub fn collect() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            memory_mb: Self::get_available_memory(),
            cuda_available: Self::check_cuda(),
            coreml_available: Self::check_coreml(),
            tesseract_installed: Self::check_tesseract(),
        }
    }

    /// Get available system memory in MB.
    fn get_available_memory() -> Option<u64> {
        // Basic memory check - could be enhanced with sysinfo crate
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|content| {
                    content
                        .lines()
                        .find(|line| line.starts_with("MemAvailable:"))
                        .and_then(|line| {
                            line.split_whitespace()
                                .nth(1)
                                .and_then(|s| s.parse::<u64>().ok())
                                .map(|kb| kb / 1024)
                        })
                })
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Check if CUDA is available.
    fn check_cuda() -> bool {
        #[cfg(feature = "cuda")]
        {
            // Check for CUDA library
            std::process::Command::new("nvidia-smi")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }
        #[cfg(not(feature = "cuda"))]
        {
            false
        }
    }

    /// Check if CoreML is available.
    fn check_coreml() -> bool {
        #[cfg(all(target_os = "macos", feature = "coreml"))]
        {
            true
        }
        #[cfg(not(all(target_os = "macos", feature = "coreml")))]
        {
            false
        }
    }

    /// Check if Tesseract is installed.
    fn check_tesseract() -> bool {
        #[cfg(feature = "tesseract")]
        {
            std::process::Command::new("tesseract")
                .arg("--version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }
        #[cfg(not(feature = "tesseract"))]
        {
            false
        }
    }
}

/// Error diagnostic information with suggestions.
#[derive(Debug, Clone)]
pub struct ErrorDiagnostic {
    /// Error category
    pub category: ErrorCategory,
    /// Suggested fixes
    pub suggestions: Vec<String>,
    /// Documentation links
    pub docs_links: Vec<String>,
    /// System diagnostics (optional)
    pub system_info: Option<SystemDiagnostics>,
}

/// Categories of errors for diagnostic purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Model loading issues
    ModelLoad,
    /// Model not loaded
    ModelNotLoaded,
    /// Image format/decoding issues
    ImageFormat,
    /// ONNX Runtime issues
    OnnxRuntime,
    /// Tesseract issues
    Tesseract,
    /// Configuration issues
    Configuration,
    /// Resource exhaustion
    Resources,
    /// GPU/Hardware issues
    Hardware,
    /// Other/unknown
    Other,
}

impl ErrorDiagnostic {
    /// Create diagnostic for model loading error.
    pub fn model_load(path: &str) -> Self {
        let mut suggestions = vec![
            "Verify the model file exists at the specified path".to_string(),
            "Check file permissions (read access required)".to_string(),
            "Ensure the model file is in ONNX format".to_string(),
            format!("Path provided: {}", path),
        ];

        // Check if path exists
        if !Path::new(path).exists() {
            suggestions.push("ERROR: Path does not exist".to_string());
            suggestions.push("Download models from provider documentation".to_string());
        } else if !Path::new(path).is_file() {
            suggestions.push("ERROR: Path is a directory, not a file".to_string());
            suggestions.push("Point to the .onnx model file directly".to_string());
        }

        Self {
            category: ErrorCategory::ModelLoad,
            suggestions,
            docs_links: vec![
                "https://github.com/cool-japan/oxify/blob/main/crates/oxify-connect-vision/README.md#provider-setup".to_string(),
            ],
            system_info: Some(SystemDiagnostics::collect()),
        }
    }

    /// Create diagnostic for model not loaded error.
    pub fn model_not_loaded() -> Self {
        Self {
            category: ErrorCategory::ModelNotLoaded,
            suggestions: vec![
                "Call provider.load_model().await before processing images".to_string(),
                "Ensure load_model() completed successfully (check for errors)".to_string(),
                "Example: let provider = create_provider(&config)?; provider.load_model().await?;".to_string(),
            ],
            docs_links: vec![
                "https://github.com/cool-japan/oxify/blob/main/crates/oxify-connect-vision/README.md#quick-start".to_string(),
            ],
            system_info: None,
        }
    }

    /// Create diagnostic for image format error.
    pub fn image_format(error_msg: &str) -> Self {
        let mut suggestions = vec![
            "Supported formats: PNG, JPEG, WebP, TIFF, BMP".to_string(),
            "Check image file is not corrupted".to_string(),
            "Verify image dimensions are reasonable (1-10000 pixels)".to_string(),
        ];

        if error_msg.contains("decode") {
            suggestions
                .push("Try opening the image in an image viewer to verify it's valid".to_string());
            suggestions
                .push("Consider converting to PNG format for better compatibility".to_string());
        }

        Self {
            category: ErrorCategory::ImageFormat,
            suggestions,
            docs_links: vec![
                "https://github.com/cool-japan/oxify/blob/main/crates/oxify-connect-vision/README.md#troubleshooting".to_string(),
            ],
            system_info: None,
        }
    }

    /// Create diagnostic for ONNX Runtime error.
    pub fn onnx_runtime(error_msg: &str) -> Self {
        let system_info = SystemDiagnostics::collect();
        let mut suggestions = vec![];

        // GPU-related errors
        if error_msg.contains("CUDA") || error_msg.contains("cuda") {
            suggestions.push("CUDA error detected".to_string());
            if system_info.cuda_available {
                suggestions.push("CUDA runtime detected but model execution failed".to_string());
                suggestions.push("Check NVIDIA driver version: nvidia-smi".to_string());
                suggestions.push("Verify GPU has sufficient memory".to_string());
            } else {
                suggestions.push("CUDA not available on this system".to_string());
                suggestions.push("Install NVIDIA drivers and CUDA toolkit".to_string());
                suggestions.push("Or disable GPU: set use_gpu=false in config".to_string());
            }
        }

        // CoreML-related errors
        if error_msg.contains("CoreML") || error_msg.contains("coreml") {
            suggestions.push("CoreML error detected".to_string());
            if !system_info.coreml_available {
                suggestions.push("CoreML is only available on macOS".to_string());
                suggestions.push("Disable GPU or use CUDA on other platforms".to_string());
            } else {
                suggestions.push("CoreML detected but execution failed".to_string());
                suggestions.push("Check macOS version (CoreML requires 10.13+)".to_string());
            }
        }

        // Memory errors
        if error_msg.contains("memory") || error_msg.contains("Memory") {
            suggestions.push("Memory allocation failed".to_string());
            if let Some(mem_mb) = system_info.memory_mb {
                suggestions.push(format!("Available memory: {} MB", mem_mb));
                if mem_mb < 2048 {
                    suggestions.push("WARNING: Low memory detected (< 2GB available)".to_string());
                    suggestions.push("Close other applications to free memory".to_string());
                    suggestions.push("Consider using mock provider for testing".to_string());
                }
            }
            suggestions.push("Try processing smaller images".to_string());
            suggestions.push("Reduce cache size if enabled".to_string());
        }

        // Model compatibility
        if error_msg.contains("opset") || error_msg.contains("Opset") {
            suggestions.push("ONNX opset version mismatch".to_string());
            suggestions.push("Update ONNX Runtime: cargo update ort".to_string());
            suggestions.push("Or download compatible model version".to_string());
        }

        // Generic fallback
        if suggestions.is_empty() {
            suggestions.push("ONNX Runtime execution failed".to_string());
            suggestions.push("Check model file integrity".to_string());
            suggestions.push("Try re-downloading the model files".to_string());
            suggestions.push(format!("System: {} {}", system_info.os, system_info.arch));
        }

        Self {
            category: ErrorCategory::OnnxRuntime,
            suggestions,
            docs_links: vec![
                "https://github.com/cool-japan/oxify/blob/main/crates/oxify-connect-vision/README.md#onnx-runtime-errors".to_string(),
            ],
            system_info: Some(system_info),
        }
    }

    /// Create diagnostic for Tesseract error.
    pub fn tesseract(error_msg: &str) -> Self {
        let system_info = SystemDiagnostics::collect();
        let mut suggestions = vec![];

        if !system_info.tesseract_installed {
            suggestions.push("Tesseract OCR not detected on system".to_string());
            suggestions.push("Install Tesseract:".to_string());

            match system_info.os.as_str() {
                "linux" => {
                    suggestions.push("  Ubuntu/Debian: sudo apt install tesseract-ocr".to_string());
                    suggestions.push("  Fedora: sudo dnf install tesseract".to_string());
                }
                "macos" => {
                    suggestions.push("  macOS: brew install tesseract".to_string());
                }
                "windows" => {
                    suggestions.push(
                        "  Windows: Download from https://github.com/UB-Mannheim/tesseract/wiki"
                            .to_string(),
                    );
                }
                _ => {
                    suggestions
                        .push("  See: https://github.com/tesseract-ocr/tesseract/wiki".to_string());
                }
            }
        } else if error_msg.contains("language") || error_msg.contains("lang") {
            suggestions.push("Language data file missing".to_string());
            suggestions.push("Install language packs:".to_string());
            suggestions.push("  Ubuntu/Debian: sudo apt install tesseract-ocr-<lang>".to_string());
            suggestions.push("  Example: sudo apt install tesseract-ocr-jpn".to_string());
            suggestions.push("Check installed languages: tesseract --list-langs".to_string());
        } else {
            suggestions.push("Tesseract execution failed".to_string());
            suggestions.push("Verify Tesseract is working: tesseract --version".to_string());
            suggestions.push("Check image quality (DPI, contrast, noise)".to_string());
        }

        Self {
            category: ErrorCategory::Tesseract,
            suggestions,
            docs_links: vec![
                "https://github.com/cool-japan/oxify/blob/main/crates/oxify-connect-vision/README.md#tesseract-installation".to_string(),
            ],
            system_info: Some(system_info),
        }
    }

    /// Create diagnostic for configuration error.
    pub fn configuration(config_issue: &str) -> Self {
        let mut suggestions = vec![
            format!("Configuration issue: {}", config_issue),
            "Review provider configuration parameters".to_string(),
        ];

        if config_issue.contains("model_path") {
            suggestions
                .push("model_path is required for ONNX providers (Surya, PaddleOCR)".to_string());
            suggestions.push(
                "Example: VisionProviderConfig::surya(\"/path/to/models\", false)".to_string(),
            );
        }

        if config_issue.contains("language") {
            suggestions.push("Check language code format".to_string());
            suggestions.push("Examples: 'en', 'ja', 'zh', 'ko', 'de', 'fr'".to_string());
        }

        Self {
            category: ErrorCategory::Configuration,
            suggestions,
            docs_links: vec![
                "https://github.com/cool-japan/oxify/blob/main/crates/oxify-connect-vision/README.md#quick-start".to_string(),
            ],
            system_info: None,
        }
    }

    /// Create diagnostic for resource exhaustion.
    pub fn resource_exhaustion(resource: &str) -> Self {
        let system_info = SystemDiagnostics::collect();
        let mut suggestions = vec![format!("Resource exhausted: {}", resource)];

        if resource.contains("memory") {
            if let Some(mem_mb) = system_info.memory_mb {
                suggestions.push(format!("Available memory: {} MB", mem_mb));
            }
            suggestions.push("Close unnecessary applications".to_string());
            suggestions.push("Process images in smaller batches".to_string());
            suggestions.push("Reduce cache size".to_string());
            suggestions.push("Consider using CPU instead of GPU".to_string());
        }

        if resource.contains("cache") {
            suggestions.push("Clear cache: cache.clear()".to_string());
            suggestions.push("Reduce cache size: cache.set_max_entries(100)".to_string());
        }

        Self {
            category: ErrorCategory::Resources,
            suggestions,
            docs_links: vec![
                "https://github.com/cool-japan/oxify/blob/main/crates/oxify-connect-vision/README.md#memory-issues".to_string(),
            ],
            system_info: Some(system_info),
        }
    }

    /// Format diagnostic as a user-friendly string.
    pub fn format(&self) -> String {
        let mut output = String::new();

        output.push_str("\n╔══════════════════════════════════════════════════════════════╗\n");
        output.push_str("║  DIAGNOSTIC INFORMATION                                      ║\n");
        output.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");

        // Suggestions
        if !self.suggestions.is_empty() {
            output.push_str("💡 Suggestions:\n");
            for (i, suggestion) in self.suggestions.iter().enumerate() {
                output.push_str(&format!("   {}. {}\n", i + 1, suggestion));
            }
            output.push('\n');
        }

        // Documentation links
        if !self.docs_links.is_empty() {
            output.push_str("📚 Documentation:\n");
            for link in &self.docs_links {
                output.push_str(&format!("   {}\n", link));
            }
            output.push('\n');
        }

        // System information
        if let Some(ref sys_info) = self.system_info {
            output.push_str("🖥️  System Information:\n");
            output.push_str(&format!("   OS: {} ({})\n", sys_info.os, sys_info.arch));
            if let Some(mem_mb) = sys_info.memory_mb {
                output.push_str(&format!("   Available Memory: {} MB\n", mem_mb));
            }
            output.push_str(&format!("   CUDA Available: {}\n", sys_info.cuda_available));
            output.push_str(&format!(
                "   CoreML Available: {}\n",
                sys_info.coreml_available
            ));
            output.push_str(&format!(
                "   Tesseract Installed: {}\n",
                sys_info.tesseract_installed
            ));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_diagnostics_collect() {
        let diag = SystemDiagnostics::collect();
        assert!(!diag.os.is_empty());
        assert!(!diag.arch.is_empty());
    }

    #[test]
    fn test_error_diagnostic_model_load() {
        let diag = ErrorDiagnostic::model_load("/nonexistent/path/model.onnx");
        assert_eq!(diag.category, ErrorCategory::ModelLoad);
        assert!(!diag.suggestions.is_empty());
        assert!(!diag.docs_links.is_empty());
    }

    #[test]
    fn test_error_diagnostic_model_not_loaded() {
        let diag = ErrorDiagnostic::model_not_loaded();
        assert_eq!(diag.category, ErrorCategory::ModelNotLoaded);
        assert!(!diag.suggestions.is_empty());
    }

    #[test]
    fn test_error_diagnostic_image_format() {
        let diag = ErrorDiagnostic::image_format("decode error");
        assert_eq!(diag.category, ErrorCategory::ImageFormat);
        assert!(!diag.suggestions.is_empty());
    }

    #[test]
    fn test_error_diagnostic_onnx_runtime() {
        let diag = ErrorDiagnostic::onnx_runtime("CUDA memory allocation failed");
        assert_eq!(diag.category, ErrorCategory::OnnxRuntime);
        assert!(!diag.suggestions.is_empty());
        assert!(diag.system_info.is_some());
    }

    #[test]
    fn test_error_diagnostic_tesseract() {
        let diag = ErrorDiagnostic::tesseract("language not found");
        assert_eq!(diag.category, ErrorCategory::Tesseract);
        assert!(!diag.suggestions.is_empty());
    }

    #[test]
    fn test_error_diagnostic_format() {
        let diag = ErrorDiagnostic::model_not_loaded();
        let formatted = diag.format();
        assert!(formatted.contains("Suggestions"));
        assert!(formatted.contains("Documentation"));
    }

    #[test]
    fn test_error_categories() {
        let categories = vec![
            ErrorCategory::ModelLoad,
            ErrorCategory::ModelNotLoaded,
            ErrorCategory::ImageFormat,
            ErrorCategory::OnnxRuntime,
            ErrorCategory::Tesseract,
            ErrorCategory::Configuration,
            ErrorCategory::Resources,
            ErrorCategory::Hardware,
            ErrorCategory::Other,
        ];

        for category in categories {
            // Ensure Debug works
            let _ = format!("{:?}", category);
        }
    }
}
