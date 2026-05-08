//! Tesseract OCR provider.
//!
//! This provider uses the Tesseract OCR engine via the leptess crate.
//! Requires Tesseract to be installed on the system.

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::errors::{Result, VisionError};
use crate::types::{BlockRole, OcrMetadata, OcrResult, TextBlock};

use super::{ProviderCapabilities, VisionProvider};

/// Tesseract OCR provider.
///
/// # Example
/// ```ignore
/// use oxify_connect_vision::providers::TesseractProvider;
///
/// let provider = TesseractProvider::new(Some("eng+jpn"));
/// let result = provider.process_image(&image_bytes).await?;
/// ```
pub struct TesseractProvider {
    /// Language code(s) for OCR (e.g., "eng", "jpn", "eng+jpn").
    language: String,
    /// Whether the engine is initialized.
    initialized: AtomicBool,
    /// Data path for Tesseract models.
    data_path: Option<String>,
}

impl TesseractProvider {
    /// Create a new Tesseract provider.
    ///
    /// # Arguments
    /// * `language` - Language code(s) for OCR. If None, defaults to "eng".
    pub fn new(language: Option<&str>) -> Self {
        Self {
            language: language.unwrap_or("eng").to_string(),
            initialized: AtomicBool::new(false),
            data_path: None,
        }
    }

    /// Set the Tesseract data path.
    pub fn with_data_path(mut self, path: &str) -> Self {
        self.data_path = Some(path.to_string());
        self
    }
}

#[async_trait]
impl VisionProvider for TesseractProvider {
    async fn process_image(&self, image_data: &[u8]) -> Result<OcrResult> {
        use std::time::Instant;
        let start = Instant::now();

        // Load image to get dimensions
        let img = image::load_from_memory(image_data)
            .map_err(|e| VisionError::image_processing(e.to_string()))?;
        let (width, height) = (img.width(), img.height());

        // Convert to grayscale for better OCR
        let gray_img = img.to_luma8();

        // Run Tesseract OCR in a blocking task
        let language = self.language.clone();
        let data_path = self.data_path.clone();
        let img_bytes = gray_img.into_raw();
        let w = width;
        let h = height;

        let text = tokio::task::spawn_blocking(move || -> Result<String> {
            // Create a temporary file for the image (leptess requires file path)
            let temp_path = std::env::temp_dir().join(format!("ocr_{}.png", uuid::Uuid::new_v4()));

            // Save grayscale image
            image::save_buffer(&temp_path, &img_bytes, w, h, image::ColorType::L8).map_err(
                |e| VisionError::image_processing(format!("Failed to save temp image: {}", e)),
            )?;

            // Initialize Tesseract
            let mut api = leptess::LepTess::new(data_path.as_deref(), &language)
                .map_err(|e| VisionError::tesseract(format!("Failed to init Tesseract: {}", e)))?;

            // Set the image
            let temp_path_str = temp_path.to_str().ok_or_else(|| {
                VisionError::tesseract(format!(
                    "Temp image path is not valid UTF-8: {:?}",
                    temp_path
                ))
            })?;
            api.set_image(temp_path_str)
                .map_err(|e| VisionError::tesseract(format!("Failed to set image: {}", e)))?;

            // Get text
            let text = api
                .get_utf8_text()
                .map_err(|e| VisionError::tesseract(format!("Failed to get text: {}", e)))?;

            // Clean up temp file
            let _ = std::fs::remove_file(&temp_path);

            Ok(text)
        })
        .await
        .map_err(|e| VisionError::tesseract(format!("Task join error: {}", e)))??;

        let processing_time = start.elapsed().as_millis() as u64;

        // Create simple blocks from lines
        let blocks: Vec<TextBlock> = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(i, line)| {
                let total = text.lines().count().max(1) as f32;
                TextBlock::new(line.trim())
                    .with_bbox([0.0, i as f32 / total, 1.0, (i + 1) as f32 / total])
                    .with_confidence(0.9)
                    .with_role(BlockRole::Text)
                    .with_order(i)
            })
            .collect();

        // Generate markdown (simple line-by-line)
        let markdown = blocks
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(OcrResult {
            text: text.trim().to_string(),
            markdown,
            blocks,
            metadata: OcrMetadata {
                provider: "tesseract".to_string(),
                model: Some(format!("tesseract-{}", self.language)),
                processing_time_ms: processing_time,
                image_size: Some((width, height)),
                languages: self.language.split('+').map(|s| s.to_string()).collect(),
                page_count: 1,
                current_page: 1,
            },
        })
    }

    async fn load_model(&self) -> Result<()> {
        // Tesseract loads models lazily, just mark as initialized
        self.initialized.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn is_model_loaded(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    fn provider_name(&self) -> &str {
        "tesseract"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            table_detection: false,
            layout_analysis: true,
            handwriting: false, // Tesseract has limited handwriting support
            multi_language: true,
            gpu_acceleration: false,
            languages: vec![
                "eng".to_string(),
                "jpn".to_string(),
                "chi_sim".to_string(),
                "chi_tra".to_string(),
                "kor".to_string(),
                "fra".to_string(),
                "deu".to_string(),
                "spa".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tesseract_provider_creation() {
        let provider = TesseractProvider::new(Some("eng+jpn"));
        assert_eq!(provider.language, "eng+jpn");
        assert_eq!(provider.provider_name(), "tesseract");
    }

    #[test]
    fn test_tesseract_with_data_path() {
        let provider =
            TesseractProvider::new(None).with_data_path("/usr/share/tesseract-ocr/4.00/tessdata");
        assert!(provider.data_path.is_some());
    }

    #[test]
    fn test_capabilities() {
        let provider = TesseractProvider::new(None);
        let caps = provider.capabilities();
        assert!(caps.multi_language);
        assert!(caps.layout_analysis);
        assert!(!caps.gpu_acceleration);
    }
}
