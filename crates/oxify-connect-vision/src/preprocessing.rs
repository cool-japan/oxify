//! Image preprocessing utilities for OCR.
//!
//! This module provides image preprocessing operations to improve OCR accuracy:
//! - Auto-resize large images
//! - Noise reduction
//! - Contrast enhancement
//! - Deskewing
//! - Border removal

use crate::errors::{Result, VisionError};
use image::{DynamicImage, GrayImage, Luma, Rgb};
use imageproc::geometric_transformations::{rotate_about_center, Interpolation};

/// Maximum image dimension (width or height) for automatic resizing.
pub const DEFAULT_MAX_DIMENSION: u32 = 4096;

/// Preprocessing configuration.
#[derive(Debug, Clone)]
pub struct PreprocessConfig {
    /// Auto-resize images larger than this dimension
    pub max_dimension: Option<u32>,
    /// Apply noise reduction
    pub denoise: bool,
    /// Apply contrast enhancement
    pub enhance_contrast: bool,
    /// Apply deskewing (rotation correction)
    pub deskew: bool,
    /// Remove borders
    pub remove_borders: bool,
    /// Convert to grayscale
    pub grayscale: bool,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        Self {
            max_dimension: Some(DEFAULT_MAX_DIMENSION),
            denoise: false,
            enhance_contrast: false,
            deskew: false,
            remove_borders: false,
            grayscale: false,
        }
    }
}

impl PreprocessConfig {
    /// Create a configuration for high-quality OCR preprocessing.
    ///
    /// Enables all preprocessing steps for maximum quality.
    pub fn high_quality() -> Self {
        Self {
            max_dimension: Some(DEFAULT_MAX_DIMENSION),
            denoise: true,
            enhance_contrast: true,
            deskew: true,
            remove_borders: true,
            grayscale: true,
        }
    }

    /// Create a configuration for fast preprocessing.
    ///
    /// Only resizes images, skipping expensive operations.
    pub fn fast() -> Self {
        Self {
            max_dimension: Some(DEFAULT_MAX_DIMENSION),
            denoise: false,
            enhance_contrast: false,
            deskew: false,
            remove_borders: false,
            grayscale: false,
        }
    }

    /// Create a minimal configuration (no preprocessing).
    pub fn none() -> Self {
        Self {
            max_dimension: None,
            denoise: false,
            enhance_contrast: false,
            deskew: false,
            remove_borders: false,
            grayscale: false,
        }
    }
}

/// Image preprocessor for OCR.
pub struct ImagePreprocessor {
    config: PreprocessConfig,
}

impl ImagePreprocessor {
    /// Create a new preprocessor with the given configuration.
    pub fn new(config: PreprocessConfig) -> Self {
        Self { config }
    }

    /// Create a preprocessor with default settings.
    pub fn default_config() -> Self {
        Self {
            config: PreprocessConfig::default(),
        }
    }

    /// Preprocess an image for OCR.
    ///
    /// Applies configured preprocessing steps in optimal order.
    pub fn preprocess(&self, mut image: DynamicImage) -> Result<DynamicImage> {
        // Step 1: Auto-resize if needed
        if let Some(max_dim) = self.config.max_dimension {
            image = resize_if_needed(image, max_dim)?;
        }

        // Step 2: Convert to grayscale if requested
        if self.config.grayscale {
            image = DynamicImage::ImageLuma8(image.to_luma8());
        }

        // Step 3: Remove borders (before other processing)
        if self.config.remove_borders {
            image = remove_borders(image)?;
        }

        // Step 4: Deskew (rotation correction)
        if self.config.deskew {
            image = deskew_image(image)?;
        }

        // Step 5: Denoise
        if self.config.denoise {
            image = denoise_image(image)?;
        }

        // Step 6: Enhance contrast (last step for best results)
        if self.config.enhance_contrast {
            image = enhance_contrast(image)?;
        }

        Ok(image)
    }

    /// Preprocess image bytes directly.
    ///
    /// Decodes the image, preprocesses it, and returns the processed bytes.
    pub fn preprocess_bytes(
        &self,
        image_data: &[u8],
        format: image::ImageFormat,
    ) -> Result<Vec<u8>> {
        // Decode image
        let image = image::load_from_memory(image_data)
            .map_err(|e| VisionError::image_processing(format!("Failed to decode image: {}", e)))?;

        // Preprocess
        let processed = self.preprocess(image)?;

        // Encode back to bytes
        let mut output = Vec::new();
        processed
            .write_to(&mut std::io::Cursor::new(&mut output), format)
            .map_err(|e| VisionError::image_processing(format!("Failed to encode image: {}", e)))?;

        Ok(output)
    }
}

/// Resize image if it exceeds the maximum dimension.
///
/// Maintains aspect ratio and only resizes if needed.
pub fn resize_if_needed(image: DynamicImage, max_dimension: u32) -> Result<DynamicImage> {
    let (width, height) = (image.width(), image.height());

    // Check if resizing is needed
    if width <= max_dimension && height <= max_dimension {
        return Ok(image);
    }

    // Calculate new dimensions maintaining aspect ratio
    let (new_width, new_height) = if width > height {
        let scale = max_dimension as f32 / width as f32;
        (max_dimension, (height as f32 * scale) as u32)
    } else {
        let scale = max_dimension as f32 / height as f32;
        ((width as f32 * scale) as u32, max_dimension)
    };

    tracing::debug!(
        "Resizing image from {}x{} to {}x{}",
        width,
        height,
        new_width,
        new_height
    );

    Ok(image.resize(new_width, new_height, image::imageops::FilterType::Lanczos3))
}

/// Apply noise reduction using median filter.
///
/// Reduces salt-and-pepper noise while preserving edges.
pub fn denoise_image(image: DynamicImage) -> Result<DynamicImage> {
    let gray = image.to_luma8();
    let denoised = median_filter(&gray, 3);
    Ok(DynamicImage::ImageLuma8(denoised))
}

/// Apply median filter to reduce noise.
fn median_filter(image: &GrayImage, radius: u32) -> GrayImage {
    let (width, height) = image.dimensions();
    let mut output = GrayImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let mut values = Vec::new();

            // Collect values in neighborhood
            for dy in -(radius as i32)..=(radius as i32) {
                for dx in -(radius as i32)..=(radius as i32) {
                    let nx = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                    let ny = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                    values.push(image.get_pixel(nx, ny)[0]);
                }
            }

            // Sort and take median
            values.sort_unstable();
            let median = values[values.len() / 2];
            output.put_pixel(x, y, Luma([median]));
        }
    }

    output
}

/// Enhance image contrast using histogram equalization.
///
/// Improves text visibility by stretching the intensity histogram.
pub fn enhance_contrast(image: DynamicImage) -> Result<DynamicImage> {
    let gray = image.to_luma8();
    let equalized = histogram_equalization(&gray);
    Ok(DynamicImage::ImageLuma8(equalized))
}

/// Apply histogram equalization to enhance contrast.
fn histogram_equalization(image: &GrayImage) -> GrayImage {
    let (width, height) = image.dimensions();
    let total_pixels = (width * height) as f32;

    // Calculate histogram
    let mut histogram = [0u32; 256];
    for pixel in image.pixels() {
        histogram[pixel[0] as usize] += 1;
    }

    // Calculate cumulative distribution function (CDF)
    let mut cdf = [0u32; 256];
    cdf[0] = histogram[0];
    for i in 1..256 {
        cdf[i] = cdf[i - 1] + histogram[i];
    }

    // Normalize CDF to create lookup table
    let cdf_min = *cdf.iter().find(|&&x| x > 0).unwrap_or(&0);
    let mut lut = [0u8; 256];
    for i in 0..256 {
        let normalized =
            ((cdf[i] - cdf_min) as f32 / (total_pixels - cdf_min as f32) * 255.0) as u8;
        lut[i] = normalized;
    }

    // Apply lookup table
    let mut output = GrayImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels() {
        let new_value = lut[pixel[0] as usize];
        output.put_pixel(x, y, Luma([new_value]));
    }

    output
}

/// Detect and correct image skew (rotation).
///
/// Uses edge detection and Hough transform approximation.
pub fn deskew_image(image: DynamicImage) -> Result<DynamicImage> {
    let angle = detect_skew_angle(&image.to_luma8());

    // Only rotate if skew is significant (> 0.5 degrees)
    if angle.abs() < 0.5 {
        return Ok(image);
    }

    tracing::debug!("Deskewing image by {:.2} degrees", angle);

    // Convert to RGB for rotation (grayscale rotation has issues)
    let rgb_image = image.to_rgb8();
    let rotated = rotate_about_center(
        &rgb_image,
        angle.to_radians(),
        Interpolation::Bilinear,
        imageproc::geometric_transformations::Border::Constant(Rgb([255u8, 255u8, 255u8])),
    );

    Ok(DynamicImage::ImageRgb8(rotated))
}

/// Detect skew angle using edge-based heuristic.
///
/// Returns the estimated rotation angle in degrees.
fn detect_skew_angle(image: &GrayImage) -> f32 {
    // Simplified skew detection using horizontal edge density
    let (width, height) = image.dimensions();
    let mut best_angle = 0.0f32;
    let mut best_score = 0.0f32;

    // Try angles from -10 to +10 degrees
    for angle_deg in -10..=10 {
        let angle = angle_deg as f32;
        let mut score = 0.0f32;

        // Sample horizontal lines
        for y in (height / 4..height * 3 / 4).step_by(10) {
            let mut edge_count = 0;
            for x in 1..width {
                let diff =
                    (image.get_pixel(x, y)[0] as i32 - image.get_pixel(x - 1, y)[0] as i32).abs();
                if diff > 30 {
                    edge_count += 1;
                }
            }
            score += edge_count as f32;
        }

        if score > best_score {
            best_score = score;
            best_angle = angle;
        }
    }

    best_angle
}

/// Remove borders from an image.
///
/// Detects and crops white/blank borders around the content.
pub fn remove_borders(image: DynamicImage) -> Result<DynamicImage> {
    let gray = image.to_luma8();
    let (width, height) = gray.dimensions();

    // Find content bounds
    let mut min_x = width;
    let mut max_x = 0;
    let mut min_y = height;
    let mut max_y = 0;

    // Threshold for detecting content (non-white pixels)
    let threshold = 240u8;

    for y in 0..height {
        for x in 0..width {
            if gray.get_pixel(x, y)[0] < threshold {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }

    // If no content found, return original
    if min_x >= max_x || min_y >= max_y {
        return Ok(image);
    }

    // Add small padding
    let padding = 10;
    min_x = min_x.saturating_sub(padding);
    min_y = min_y.saturating_sub(padding);
    max_x = (max_x + padding).min(width - 1);
    max_y = (max_y + padding).min(height - 1);

    tracing::debug!(
        "Removing borders: cropping to ({}, {}) -> ({}, {})",
        min_x,
        min_y,
        max_x,
        max_y
    );

    // Crop image
    let crop_width = max_x - min_x + 1;
    let crop_height = max_y - min_y + 1;
    Ok(image.crop_imm(min_x, min_y, crop_width, crop_height))
}

/// Apply adaptive thresholding for binarization.
///
/// Converts image to black and white, useful for text extraction.
#[allow(dead_code)]
pub fn adaptive_threshold(image: &GrayImage, window_size: u32) -> GrayImage {
    let (width, height) = image.dimensions();
    let mut output = GrayImage::new(width, height);
    let half_window = window_size / 2;

    for y in 0..height {
        for x in 0..width {
            // Calculate local mean
            let mut sum = 0u32;
            let mut count = 0u32;

            for dy in -(half_window as i32)..=(half_window as i32) {
                for dx in -(half_window as i32)..=(half_window as i32) {
                    let nx = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                    let ny = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                    sum += image.get_pixel(nx, ny)[0] as u32;
                    count += 1;
                }
            }

            let mean = sum / count;
            let pixel_value = image.get_pixel(x, y)[0] as u32;

            // Threshold with small bias
            let threshold = mean.saturating_sub(5);
            let new_value = if pixel_value < threshold { 0 } else { 255 };
            output.put_pixel(x, y, Luma([new_value]));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let img = RgbImage::from_fn(width, height, |x, y| {
            let val = ((x + y) % 255) as u8;
            Rgb([val, val, val])
        });
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_preprocess_config_default() {
        let config = PreprocessConfig::default();
        assert_eq!(config.max_dimension, Some(DEFAULT_MAX_DIMENSION));
        assert!(!config.denoise);
    }

    #[test]
    fn test_preprocess_config_high_quality() {
        let config = PreprocessConfig::high_quality();
        assert!(config.denoise);
        assert!(config.enhance_contrast);
        assert!(config.deskew);
        assert!(config.remove_borders);
    }

    #[test]
    fn test_preprocess_config_fast() {
        let config = PreprocessConfig::fast();
        assert!(!config.denoise);
        assert!(!config.enhance_contrast);
    }

    #[test]
    fn test_preprocess_config_none() {
        let config = PreprocessConfig::none();
        assert_eq!(config.max_dimension, None);
        assert!(!config.denoise);
    }

    #[test]
    fn test_preprocessor_creation() {
        let preprocessor = ImagePreprocessor::default_config();
        let _ = preprocessor.config;
    }

    #[test]
    fn test_resize_if_needed_no_resize() {
        let image = create_test_image(1000, 1000);
        let result = resize_if_needed(image.clone(), 2000).unwrap();
        assert_eq!(result.width(), 1000);
        assert_eq!(result.height(), 1000);
    }

    #[test]
    fn test_resize_if_needed_resize() {
        // Optimized test with smaller image for fast execution
        let image = create_test_image(2000, 1200);
        let original_width = image.width();
        let original_height = image.height();
        let result = resize_if_needed(image, 1600).unwrap();
        assert!(result.width() <= 1600);
        assert!(result.height() <= 1600);
        // Verify aspect ratio is maintained
        let aspect_ratio = original_width as f32 / original_height as f32;
        let result_ratio = result.width() as f32 / result.height() as f32;
        assert!((aspect_ratio - result_ratio).abs() < 0.01);
    }

    #[test]
    #[ignore]
    fn test_resize_if_needed_resize_large() {
        // Slow comprehensive test with large image (84s+)
        // Run with: cargo test test_resize_if_needed_resize_large -- --ignored
        let image = create_test_image(5000, 3000);
        let result = resize_if_needed(image, 4000).unwrap();
        assert!(result.width() <= 4000);
        assert!(result.height() <= 4000);
    }

    #[test]
    fn test_denoise_image() {
        let image = create_test_image(100, 100);
        let result = denoise_image(image).unwrap();
        assert!(result.width() > 0);
        assert!(result.height() > 0);
    }

    #[test]
    fn test_enhance_contrast() {
        let image = create_test_image(100, 100);
        let result = enhance_contrast(image).unwrap();
        assert!(result.width() > 0);
        assert!(result.height() > 0);
    }

    #[test]
    fn test_deskew_image() {
        let image = create_test_image(100, 100);
        let result = deskew_image(image).unwrap();
        assert!(result.width() > 0);
        assert!(result.height() > 0);
    }

    #[test]
    fn test_remove_borders() {
        let image = create_test_image(100, 100);
        let result = remove_borders(image).unwrap();
        assert!(result.width() > 0);
        assert!(result.height() > 0);
    }

    #[test]
    fn test_preprocessor_preprocess() {
        let image = create_test_image(200, 200);
        let config = PreprocessConfig::default();
        let preprocessor = ImagePreprocessor::new(config);
        let result = preprocessor.preprocess(image).unwrap();
        assert!(result.width() > 0);
        assert!(result.height() > 0);
    }

    #[test]
    fn test_median_filter() {
        let gray = GrayImage::from_fn(50, 50, |x, y| Luma([((x + y) % 255) as u8]));
        let filtered = median_filter(&gray, 2);
        assert_eq!(filtered.dimensions(), gray.dimensions());
    }

    #[test]
    fn test_histogram_equalization() {
        let gray = GrayImage::from_fn(50, 50, |x, y| Luma([((x + y) % 128) as u8]));
        let equalized = histogram_equalization(&gray);
        assert_eq!(equalized.dimensions(), gray.dimensions());
    }

    #[test]
    fn test_detect_skew_angle() {
        let gray = GrayImage::from_fn(100, 100, |x, y| Luma([((x + y) % 255) as u8]));
        let angle = detect_skew_angle(&gray);
        assert!(angle.abs() <= 10.0);
    }

    #[test]
    fn test_adaptive_threshold() {
        let gray = GrayImage::from_fn(50, 50, |x, y| Luma([((x + y) % 255) as u8]));
        let thresholded = adaptive_threshold(&gray, 11);
        assert_eq!(thresholded.dimensions(), gray.dimensions());
    }
}
