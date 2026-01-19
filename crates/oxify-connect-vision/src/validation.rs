//! Input validation module for secure image processing.
//!
//! This module provides comprehensive input validation for images including:
//! - Format verification
//! - Size limits
//! - Dimension constraints
//! - Malicious content detection
//! - MIME type validation

use crate::errors::{Result, VisionError};
use image::{DynamicImage, ImageFormat};
use std::path::Path;
use tracing::{debug, warn};

/// Maximum allowed image file size (50 MB by default)
pub const DEFAULT_MAX_FILE_SIZE: usize = 50 * 1024 * 1024;

/// Maximum allowed image dimension (width or height)
pub const DEFAULT_MAX_DIMENSION: u32 = 10_000;

/// Minimum allowed image dimension (width or height)
pub const DEFAULT_MIN_DIMENSION: u32 = 10;

/// Configuration for image validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Maximum allowed file size in bytes
    pub max_file_size: usize,

    /// Maximum allowed width or height
    pub max_dimension: u32,

    /// Minimum allowed width or height
    pub min_dimension: u32,

    /// Allowed image formats
    pub allowed_formats: Vec<ImageFormat>,

    /// Whether to perform deep content inspection
    pub deep_inspection: bool,

    /// Whether to validate aspect ratio
    pub validate_aspect_ratio: bool,

    /// Maximum aspect ratio (width/height)
    pub max_aspect_ratio: f32,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_dimension: DEFAULT_MAX_DIMENSION,
            min_dimension: DEFAULT_MIN_DIMENSION,
            allowed_formats: vec![
                ImageFormat::Png,
                ImageFormat::Jpeg,
                ImageFormat::WebP,
                ImageFormat::Tiff,
                ImageFormat::Bmp,
            ],
            deep_inspection: true,
            validate_aspect_ratio: true,
            max_aspect_ratio: 100.0, // Allow up to 100:1 aspect ratios
        }
    }
}

impl ValidationConfig {
    /// Create a permissive configuration for testing
    pub fn permissive() -> Self {
        Self {
            max_file_size: usize::MAX,
            max_dimension: u32::MAX,
            min_dimension: 1,
            allowed_formats: ImageFormat::all().collect(),
            deep_inspection: false,
            validate_aspect_ratio: false,
            max_aspect_ratio: f32::MAX,
        }
    }

    /// Create a strict configuration for production
    pub fn strict() -> Self {
        Self {
            max_file_size: 10 * 1024 * 1024, // 10 MB
            max_dimension: 5_000,
            min_dimension: 50,
            allowed_formats: vec![ImageFormat::Png, ImageFormat::Jpeg],
            deep_inspection: true,
            validate_aspect_ratio: true,
            max_aspect_ratio: 20.0,
        }
    }
}

/// Image validator for security checks
pub struct ImageValidator {
    config: ValidationConfig,
}

impl ImageValidator {
    /// Create a new validator with default configuration
    pub fn new() -> Self {
        Self {
            config: ValidationConfig::default(),
        }
    }

    /// Create a new validator with custom configuration
    pub fn with_config(config: ValidationConfig) -> Self {
        Self { config }
    }

    /// Validate image bytes before processing
    pub fn validate_bytes(&self, data: &[u8]) -> Result<()> {
        debug!("Validating image bytes (size: {} bytes)", data.len());

        // Check file size
        if data.len() > self.config.max_file_size {
            warn!(
                "Image size {} exceeds maximum allowed size {}",
                data.len(),
                self.config.max_file_size
            );
            return Err(VisionError::InvalidFormat(format!(
                "Image size {} bytes exceeds maximum allowed {} bytes",
                data.len(),
                self.config.max_file_size
            )));
        }

        // Check if data is empty
        if data.is_empty() {
            return Err(VisionError::InvalidFormat(
                "Image data is empty".to_string(),
            ));
        }

        // Detect and validate format
        let format = self.detect_format(data)?;
        self.validate_format(&format)?;

        // Load image for dimension validation
        let image = image::load_from_memory(data)
            .map_err(|e| VisionError::ImageDecode(format!("Failed to decode image: {}", e)))?;

        // Validate dimensions
        self.validate_dimensions(&image)?;

        // Perform deep inspection if enabled
        if self.config.deep_inspection {
            self.deep_inspect(&image)?;
        }

        debug!("Image validation passed");
        Ok(())
    }

    /// Validate image file before processing
    pub fn validate_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        debug!("Validating image file: {}", path.display());

        // Check if file exists
        if !path.exists() {
            return Err(VisionError::InvalidFormat(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check file size
        let metadata = std::fs::metadata(path).map_err(|e| {
            VisionError::InvalidFormat(format!("Failed to read file metadata: {}", e))
        })?;

        if metadata.len() > self.config.max_file_size as u64 {
            return Err(VisionError::InvalidFormat(format!(
                "File size {} bytes exceeds maximum allowed {} bytes",
                metadata.len(),
                self.config.max_file_size
            )));
        }

        // Validate file extension
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            let valid_extensions = ["png", "jpg", "jpeg", "webp", "tiff", "tif", "bmp"];
            if !valid_extensions.contains(&ext_str.as_str()) {
                warn!("Suspicious file extension: {}", ext_str);
            }
        }

        // Read and validate file content
        let data = std::fs::read(path)
            .map_err(|e| VisionError::InvalidFormat(format!("Failed to read file: {}", e)))?;

        self.validate_bytes(&data)
    }

    /// Validate a DynamicImage
    pub fn validate_image(&self, image: &DynamicImage) -> Result<()> {
        debug!("Validating DynamicImage");
        self.validate_dimensions(image)?;

        if self.config.deep_inspection {
            self.deep_inspect(image)?;
        }

        Ok(())
    }

    /// Detect image format from bytes
    fn detect_format(&self, data: &[u8]) -> Result<ImageFormat> {
        image::guess_format(data).map_err(|e| {
            VisionError::InvalidFormat(format!("Failed to detect image format: {}", e))
        })
    }

    /// Validate that the image format is allowed
    fn validate_format(&self, format: &ImageFormat) -> Result<()> {
        if !self.config.allowed_formats.contains(format) {
            return Err(VisionError::InvalidFormat(format!(
                "Image format {:?} is not allowed. Allowed formats: {:?}",
                format, self.config.allowed_formats
            )));
        }
        Ok(())
    }

    /// Validate image dimensions
    fn validate_dimensions(&self, image: &DynamicImage) -> Result<()> {
        let (width, height) = (image.width(), image.height());

        debug!("Image dimensions: {}x{}", width, height);

        // Check maximum dimensions
        if width > self.config.max_dimension || height > self.config.max_dimension {
            return Err(VisionError::InvalidFormat(format!(
                "Image dimensions {}x{} exceed maximum allowed {}x{}",
                width, height, self.config.max_dimension, self.config.max_dimension
            )));
        }

        // Check minimum dimensions
        if width < self.config.min_dimension || height < self.config.min_dimension {
            return Err(VisionError::InvalidFormat(format!(
                "Image dimensions {}x{} are below minimum required {}x{}",
                width, height, self.config.min_dimension, self.config.min_dimension
            )));
        }

        // Validate aspect ratio
        if self.config.validate_aspect_ratio {
            let aspect_ratio = width.max(height) as f32 / width.min(height) as f32;
            if aspect_ratio > self.config.max_aspect_ratio {
                warn!(
                    "Image aspect ratio {:.2} exceeds maximum {:.2}",
                    aspect_ratio, self.config.max_aspect_ratio
                );
                return Err(VisionError::InvalidFormat(format!(
                    "Image aspect ratio {:.2} exceeds maximum allowed {:.2}",
                    aspect_ratio, self.config.max_aspect_ratio
                )));
            }
        }

        Ok(())
    }

    /// Perform deep content inspection
    fn deep_inspect(&self, image: &DynamicImage) -> Result<()> {
        debug!("Performing deep content inspection");

        // Check for suspicious patterns
        self.check_entropy(image)?;
        self.check_color_distribution(image)?;

        Ok(())
    }

    /// Check image entropy (randomness) to detect potential attacks
    fn check_entropy(&self, image: &DynamicImage) -> Result<()> {
        let pixels = image.to_rgba8();
        let (width, height) = pixels.dimensions();
        let pixel_count = (width * height) as usize;

        // Very basic entropy check: count unique colors
        // A completely random image might indicate an attack
        let mut color_samples = std::collections::HashSet::new();
        let sample_size = pixel_count.min(1000);

        for y in
            (0..height).step_by((height as usize / (sample_size as f32).sqrt() as usize).max(1))
        {
            for x in
                (0..width).step_by((width as usize / (sample_size as f32).sqrt() as usize).max(1))
            {
                let pixel = pixels.get_pixel(x, y);
                let color = (pixel[0], pixel[1], pixel[2], pixel[3]);
                color_samples.insert(color);
            }
        }

        let uniqueness_ratio = color_samples.len() as f32 / sample_size as f32;

        // If almost every sampled pixel is unique, might be suspicious
        if uniqueness_ratio > 0.95 {
            debug!(
                "High entropy detected: uniqueness ratio = {:.2}",
                uniqueness_ratio
            );
            // Note: This is just a warning, not a hard error
            // Many legitimate images can have high entropy
        }

        Ok(())
    }

    /// Check color distribution for anomalies
    fn check_color_distribution(&self, image: &DynamicImage) -> Result<()> {
        let pixels = image.to_rgba8();

        // Check if image is completely transparent
        let mut all_transparent = true;
        for chunk in pixels.chunks(4) {
            if chunk[3] > 0 {
                // Alpha channel
                all_transparent = false;
                break;
            }
        }

        if all_transparent {
            return Err(VisionError::InvalidFormat(
                "Image is completely transparent".to_string(),
            ));
        }

        Ok(())
    }

    /// Get the current configuration
    pub fn config(&self) -> &ValidationConfig {
        &self.config
    }
}

impl Default for ImageValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate image bytes with default configuration
pub fn validate_image_bytes(data: &[u8]) -> Result<()> {
    ImageValidator::new().validate_bytes(data)
}

/// Validate image file with default configuration
pub fn validate_image_file<P: AsRef<Path>>(path: P) -> Result<()> {
    ImageValidator::new().validate_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let img = ImageBuffer::from_fn(width, height, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        DynamicImage::ImageRgba8(img)
    }

    fn image_to_bytes(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut cursor = Cursor::new(&mut bytes);
        image.write_to(&mut cursor, format).unwrap();
        bytes
    }

    #[test]
    fn test_validator_creation() {
        let validator = ImageValidator::new();
        assert_eq!(validator.config.max_file_size, DEFAULT_MAX_FILE_SIZE);
    }

    #[test]
    fn test_validator_with_config() {
        let config = ValidationConfig::strict();
        let validator = ImageValidator::with_config(config.clone());
        assert_eq!(validator.config.max_file_size, config.max_file_size);
    }

    #[test]
    fn test_validate_dimensions_valid() {
        let validator = ImageValidator::new();
        let image = create_test_image(800, 600);
        assert!(validator.validate_dimensions(&image).is_ok());
    }

    #[test]
    fn test_validate_dimensions_too_large() {
        let validator = ImageValidator::new();
        let image = create_test_image(15000, 600);
        assert!(validator.validate_dimensions(&image).is_err());
    }

    #[test]
    fn test_validate_dimensions_too_small() {
        let validator = ImageValidator::new();
        let image = create_test_image(5, 5);
        assert!(validator.validate_dimensions(&image).is_err());
    }

    #[test]
    fn test_validate_aspect_ratio_valid() {
        let config = ValidationConfig {
            max_aspect_ratio: 10.0,
            ..Default::default()
        };
        let validator = ImageValidator::with_config(config);
        let image = create_test_image(1000, 100); // 10:1 aspect ratio
        assert!(validator.validate_dimensions(&image).is_ok());
    }

    #[test]
    fn test_validate_aspect_ratio_invalid() {
        let config = ValidationConfig {
            max_aspect_ratio: 5.0,
            ..Default::default()
        };
        let validator = ImageValidator::with_config(config);
        let image = create_test_image(1000, 100); // 10:1 aspect ratio
        assert!(validator.validate_dimensions(&image).is_err());
    }

    #[test]
    fn test_validate_bytes_valid_png() {
        let image = create_test_image(800, 600);
        let bytes = image_to_bytes(&image, ImageFormat::Png);
        let validator = ImageValidator::new();
        assert!(validator.validate_bytes(&bytes).is_ok());
    }

    #[test]
    fn test_validate_bytes_empty() {
        let validator = ImageValidator::new();
        let result = validator.validate_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_bytes_too_large() {
        let config = ValidationConfig {
            max_file_size: 1000,
            ..Default::default()
        };
        let validator = ImageValidator::with_config(config);
        let image = create_test_image(800, 600);
        let bytes = image_to_bytes(&image, ImageFormat::Png);
        assert!(validator.validate_bytes(&bytes).is_err());
    }

    #[test]
    fn test_validate_format_allowed() {
        let validator = ImageValidator::new();
        assert!(validator.validate_format(&ImageFormat::Png).is_ok());
        assert!(validator.validate_format(&ImageFormat::Jpeg).is_ok());
    }

    #[test]
    fn test_validate_format_not_allowed() {
        let config = ValidationConfig {
            allowed_formats: vec![ImageFormat::Png],
            ..Default::default()
        };
        let validator = ImageValidator::with_config(config);
        assert!(validator.validate_format(&ImageFormat::Jpeg).is_err());
    }

    #[test]
    fn test_permissive_config() {
        let config = ValidationConfig::permissive();
        assert_eq!(config.max_file_size, usize::MAX);
        assert_eq!(config.max_dimension, u32::MAX);
    }

    #[test]
    fn test_strict_config() {
        let config = ValidationConfig::strict();
        assert_eq!(config.max_file_size, 10 * 1024 * 1024);
        assert_eq!(config.max_dimension, 5_000);
    }

    #[test]
    fn test_check_entropy() {
        let validator = ImageValidator::new();
        let image = create_test_image(800, 600);
        assert!(validator.check_entropy(&image).is_ok());
    }

    #[test]
    fn test_check_color_distribution_valid() {
        let validator = ImageValidator::new();
        let image = create_test_image(800, 600);
        assert!(validator.check_color_distribution(&image).is_ok());
    }

    #[test]
    fn test_check_color_distribution_transparent() {
        let validator = ImageValidator::new();
        let img = ImageBuffer::from_pixel(100, 100, Rgba([0, 0, 0, 0]));
        let image = DynamicImage::ImageRgba8(img);
        assert!(validator.check_color_distribution(&image).is_err());
    }

    #[test]
    fn test_validate_image_bytes_helper() {
        let image = create_test_image(800, 600);
        let bytes = image_to_bytes(&image, ImageFormat::Png);
        assert!(validate_image_bytes(&bytes).is_ok());
    }

    #[test]
    fn test_deep_inspection() {
        let validator = ImageValidator::new();
        let image = create_test_image(800, 600);
        assert!(validator.deep_inspect(&image).is_ok());
    }

    #[test]
    fn test_validation_config_default() {
        let config = ValidationConfig::default();
        assert_eq!(config.max_file_size, DEFAULT_MAX_FILE_SIZE);
        assert_eq!(config.max_dimension, DEFAULT_MAX_DIMENSION);
        assert!(config.deep_inspection);
    }
}
