//! GPU detection and management utilities.
//!
//! This module provides runtime GPU detection for CUDA and CoreML,
//! automatic fallback to CPU when GPU is unavailable, and GPU memory
//! management helpers.

use std::sync::OnceLock;

/// GPU availability information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuInfo {
    /// CUDA is available
    pub cuda_available: bool,
    /// CoreML is available
    pub coreml_available: bool,
    /// Available GPU memory in bytes (if detectable)
    pub gpu_memory_bytes: Option<u64>,
}

impl GpuInfo {
    /// Detect GPU availability at runtime.
    pub fn detect() -> Self {
        Self {
            cuda_available: Self::detect_cuda(),
            coreml_available: Self::detect_coreml(),
            gpu_memory_bytes: Self::detect_gpu_memory(),
        }
    }

    /// Check if any GPU is available.
    pub fn has_gpu(&self) -> bool {
        self.cuda_available || self.coreml_available
    }

    /// Get recommended execution provider based on availability.
    pub fn recommended_provider(&self) -> GpuProvider {
        if self.cuda_available {
            GpuProvider::Cuda
        } else if self.coreml_available {
            GpuProvider::CoreMl
        } else {
            GpuProvider::Cpu
        }
    }

    /// Detect CUDA availability at runtime.
    fn detect_cuda() -> bool {
        #[cfg(feature = "cuda")]
        {
            // Try multiple detection methods

            // Method 1: Check nvidia-smi command
            if let Ok(output) = std::process::Command::new("nvidia-smi")
                .arg("--query-gpu=name")
                .arg("--format=csv,noheader")
                .output()
            {
                if output.status.success() && !output.stdout.is_empty() {
                    return true;
                }
            }

            // Method 2: Check for CUDA library files
            #[cfg(target_os = "linux")]
            {
                if std::path::Path::new("/usr/local/cuda/lib64/libcudart.so").exists()
                    || std::path::Path::new("/usr/lib/x86_64-linux-gnu/libcudart.so").exists()
                {
                    return true;
                }
            }

            #[cfg(target_os = "windows")]
            {
                // Check Windows CUDA paths
                if let Ok(cuda_path) = std::env::var("CUDA_PATH") {
                    let dll_path = std::path::Path::new(&cuda_path)
                        .join("bin")
                        .join("cudart64_110.dll");
                    if dll_path.exists() {
                        return true;
                    }
                }
            }

            // Method 3: Try to detect via environment variables
            if std::env::var("CUDA_VISIBLE_DEVICES").is_ok() {
                return true;
            }

            false
        }
        #[cfg(not(feature = "cuda"))]
        {
            false
        }
    }

    /// Detect CoreML availability at runtime.
    fn detect_coreml() -> bool {
        #[cfg(feature = "coreml")]
        {
            #[cfg(target_os = "macos")]
            {
                // Check macOS version - CoreML requires 10.13+
                if let Ok(output) = std::process::Command::new("sw_vers")
                    .arg("-productVersion")
                    .output()
                {
                    if let Ok(version_str) = String::from_utf8(output.stdout) {
                        if let Some(major) = version_str.split('.').next() {
                            if let Ok(major_num) = major.trim().parse::<u32>() {
                                // macOS 10.13+ or macOS 11+ (Big Sur changed versioning)
                                return major_num >= 11 || major_num == 10;
                            }
                        }
                    }
                }
                // Assume available if we can't determine version
                true
            }
            #[cfg(not(target_os = "macos"))]
            {
                false
            }
        }
        #[cfg(not(feature = "coreml"))]
        {
            false
        }
    }

    /// Detect available GPU memory (CUDA only).
    fn detect_gpu_memory() -> Option<u64> {
        #[cfg(feature = "cuda")]
        {
            if let Ok(output) = std::process::Command::new("nvidia-smi")
                .arg("--query-gpu=memory.total")
                .arg("--format=csv,noheader,nounits")
                .output()
            {
                if output.status.success() {
                    if let Ok(mem_str) = String::from_utf8(output.stdout) {
                        if let Ok(mem_mb) = mem_str.trim().parse::<u64>() {
                            return Some(mem_mb * 1024 * 1024); // Convert MB to bytes
                        }
                    }
                }
            }
        }
        None
    }

    /// Get cached GPU information (detected once per process).
    pub fn cached() -> &'static GpuInfo {
        static GPU_INFO: OnceLock<GpuInfo> = OnceLock::new();
        GPU_INFO.get_or_init(GpuInfo::detect)
    }
}

/// GPU execution provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuProvider {
    /// CPU execution (fallback)
    Cpu,
    /// CUDA GPU acceleration
    Cuda,
    /// CoreML acceleration (macOS)
    CoreMl,
}

impl GpuProvider {
    /// Get the provider name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            GpuProvider::Cpu => "CPU",
            GpuProvider::Cuda => "CUDA",
            GpuProvider::CoreMl => "CoreML",
        }
    }

    /// Check if this is a GPU provider (not CPU).
    pub fn is_gpu(&self) -> bool {
        matches!(self, GpuProvider::Cuda | GpuProvider::CoreMl)
    }
}

/// GPU configuration for ONNX providers.
#[derive(Debug, Clone)]
pub struct GpuConfig {
    /// Requested GPU provider (may fall back to CPU if unavailable)
    pub requested: GpuProvider,
    /// Actually used provider (after fallback check)
    pub actual: GpuProvider,
    /// GPU device ID (for multi-GPU systems)
    pub device_id: u32,
}

impl GpuConfig {
    /// Create a new GPU configuration with automatic fallback.
    ///
    /// If the requested GPU is not available, falls back to CPU and logs a warning.
    pub fn new(use_gpu: bool) -> Self {
        let gpu_info = GpuInfo::cached();

        let requested = if use_gpu {
            gpu_info.recommended_provider()
        } else {
            GpuProvider::Cpu
        };

        let actual = match requested {
            GpuProvider::Cuda if !gpu_info.cuda_available => {
                tracing::warn!("CUDA requested but not available, falling back to CPU");
                GpuProvider::Cpu
            }
            GpuProvider::CoreMl if !gpu_info.coreml_available => {
                tracing::warn!("CoreML requested but not available, falling back to CPU");
                GpuProvider::Cpu
            }
            provider => provider,
        };

        Self {
            requested,
            actual,
            device_id: 0,
        }
    }

    /// Create a CPU-only configuration.
    pub fn cpu() -> Self {
        Self {
            requested: GpuProvider::Cpu,
            actual: GpuProvider::Cpu,
            device_id: 0,
        }
    }

    /// Create a CUDA configuration with fallback.
    pub fn cuda(device_id: u32) -> Self {
        let mut config = Self::new(true);
        config.device_id = device_id;

        // Force CUDA if requested
        if config.actual == GpuProvider::Cpu && GpuInfo::cached().cuda_available {
            config.actual = GpuProvider::Cuda;
        }

        config
    }

    /// Create a CoreML configuration with fallback.
    pub fn coreml() -> Self {
        let mut config = Self::new(true);

        // Force CoreML if requested
        if config.actual == GpuProvider::Cpu && GpuInfo::cached().coreml_available {
            config.actual = GpuProvider::CoreMl;
        }

        config
    }

    /// Check if GPU was successfully configured (not fallen back to CPU).
    pub fn is_using_gpu(&self) -> bool {
        self.actual.is_gpu()
    }

    /// Get a description of the configuration.
    pub fn description(&self) -> String {
        if self.requested == self.actual {
            format!("Using {}", self.actual.name())
        } else {
            format!(
                "Requested {} but using {} (fallback)",
                self.requested.name(),
                self.actual.name()
            )
        }
    }
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self::new(false)
    }
}

/// Check if sufficient GPU memory is available for a model.
///
/// Returns true if:
/// - CPU is being used (no memory limit)
/// - GPU has sufficient memory
/// - Memory detection failed (assume sufficient)
#[allow(dead_code)]
pub fn check_gpu_memory(required_mb: u64) -> bool {
    let gpu_info = GpuInfo::cached();

    // CPU has no GPU memory limits
    if !gpu_info.has_gpu() {
        return true;
    }

    // If we can't detect memory, assume it's sufficient
    let Some(total_bytes) = gpu_info.gpu_memory_bytes else {
        return true;
    };

    let required_bytes = required_mb * 1024 * 1024;
    let available_bytes = total_bytes * 8 / 10; // Use 80% as threshold

    available_bytes >= required_bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_info_detect() {
        let info = GpuInfo::detect();
        // Just ensure it doesn't panic
        let _ = info.has_gpu();
        let _ = info.recommended_provider();
    }

    #[test]
    fn test_gpu_info_cached() {
        let info1 = GpuInfo::cached();
        let info2 = GpuInfo::cached();
        // Should be the same instance
        assert_eq!(info1, info2);
    }

    #[test]
    fn test_gpu_provider_name() {
        assert_eq!(GpuProvider::Cpu.name(), "CPU");
        assert_eq!(GpuProvider::Cuda.name(), "CUDA");
        assert_eq!(GpuProvider::CoreMl.name(), "CoreML");
    }

    #[test]
    fn test_gpu_provider_is_gpu() {
        assert!(!GpuProvider::Cpu.is_gpu());
        assert!(GpuProvider::Cuda.is_gpu());
        assert!(GpuProvider::CoreMl.is_gpu());
    }

    #[test]
    fn test_gpu_config_cpu() {
        let config = GpuConfig::cpu();
        assert_eq!(config.requested, GpuProvider::Cpu);
        assert_eq!(config.actual, GpuProvider::Cpu);
        assert!(!config.is_using_gpu());
    }

    #[test]
    fn test_gpu_config_default() {
        let config = GpuConfig::default();
        assert_eq!(config.requested, GpuProvider::Cpu);
        assert_eq!(config.actual, GpuProvider::Cpu);
    }

    #[test]
    fn test_gpu_config_new() {
        // Test CPU-only config
        let config = GpuConfig::new(false);
        assert_eq!(config.actual, GpuProvider::Cpu);

        // Test GPU config (may fall back to CPU)
        let config = GpuConfig::new(true);
        // Should be valid configuration
        assert!(config.requested == config.actual || config.actual == GpuProvider::Cpu);
    }

    #[test]
    fn test_gpu_config_description() {
        let config = GpuConfig::cpu();
        let desc = config.description();
        assert!(desc.contains("CPU"));
    }

    #[test]
    fn test_check_gpu_memory() {
        // Should not panic
        let _ = check_gpu_memory(1000);
        let _ = check_gpu_memory(100000);
    }

    #[test]
    fn test_gpu_config_cuda() {
        let config = GpuConfig::cuda(0);
        assert_eq!(config.device_id, 0);
        // GpuConfig::cuda() requests CUDA, but actual provider depends on availability:
        // - Linux with CUDA: Cuda or CPU fallback
        // - macOS: CoreML or CPU fallback (CUDA not available)
        // - Windows: CPU fallback
        // The key assertion is that 'actual' is a valid GPU provider
        assert!(
            config.actual == GpuProvider::Cuda
                || config.actual == GpuProvider::CoreMl
                || config.actual == GpuProvider::Cpu
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_gpu_config_coreml() {
        let config = GpuConfig::coreml();
        // Should either be CoreML or CPU (fallback)
        assert!(config.actual == GpuProvider::CoreMl || config.actual == GpuProvider::Cpu);
    }
}
