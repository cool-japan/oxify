//! PaddleOCR provider using ONNX Runtime.
//!
//! PaddleOCR is a practical OCR toolkit developed by Baidu.
//! This implementation uses ONNX-converted models for cross-platform inference.

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(feature = "paddle")]
use ort::{
    session::{Session, SessionOutputs},
    value::Value,
};

#[cfg(all(feature = "paddle", feature = "cuda"))]
use ort::execution_providers::CUDAExecutionProvider;

#[cfg(all(feature = "paddle", feature = "coreml", not(feature = "cuda")))]
use ort::execution_providers::CoreMLExecutionProvider;

use crate::errors::{Result, VisionError};
use crate::types::{BlockRole, OcrMetadata, OcrResult, TextBlock};

use super::{ProviderCapabilities, VisionProvider};

/// PaddleOCR client using ONNX Runtime.
///
/// PaddleOCR uses a pipeline of three models:
/// - Detection (DBNet): Finds text regions
/// - Direction classifier (optional): Rotates text to horizontal
/// - Recognition (CRNN): Converts regions to text
pub struct PaddleOcrClient {
    /// Path to model directory containing ONNX files.
    model_path: PathBuf,
    /// Whether to use GPU acceleration.
    use_gpu: bool,
    /// Whether models are loaded.
    initialized: AtomicBool,
    /// Language for OCR.
    language: String,
    /// ONNX session for detection.
    detection_session: Arc<RwLock<Option<Session>>>,
    /// ONNX session for recognition.
    recognition_session: Arc<RwLock<Option<Session>>>,
    /// ONNX session for text direction classification.
    classifier_session: Arc<RwLock<Option<Session>>>,
    /// Character dictionary for decoding.
    char_dict: Arc<RwLock<Vec<char>>>,
}

impl PaddleOcrClient {
    /// Create a new PaddleOCR client.
    ///
    /// # Arguments
    /// * `model_path` - Path to directory containing ONNX model files
    /// * `use_gpu` - Whether to use GPU acceleration
    pub fn new(model_path: impl Into<PathBuf>, use_gpu: bool) -> Self {
        Self {
            model_path: model_path.into(),
            use_gpu,
            initialized: AtomicBool::new(false),
            language: "ch".to_string(), // Default to Chinese + English
            detection_session: Arc::new(RwLock::new(None)),
            recognition_session: Arc::new(RwLock::new(None)),
            classifier_session: Arc::new(RwLock::new(None)),
            char_dict: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Set the language for OCR.
    pub fn with_language(mut self, lang: &str) -> Self {
        self.language = lang.to_string();
        self
    }

    /// Load an ONNX model.
    async fn load_onnx_model(&self, model_name: &str) -> Result<Session> {
        let model_file = self.model_path.join(format!("{}.onnx", model_name));

        if !model_file.exists() {
            return Err(VisionError::model_load(format!(
                "Model file not found: {}",
                model_file.display()
            )));
        }

        let use_gpu = self.use_gpu;
        let model_path = model_file.clone();

        tokio::task::spawn_blocking(move || {
            let builder = Session::builder().map_err(|e| {
                VisionError::onnx_runtime(format!("Failed to create builder: {}", e))
            })?;

            // Configure execution providers
            #[allow(unused_mut)]
            let mut builder = if use_gpu {
                #[cfg(feature = "cuda")]
                {
                    builder
                        .with_execution_providers([CUDAExecutionProvider::default().build()])
                        .map_err(|e| {
                            VisionError::onnx_runtime(format!("CUDA setup failed: {}", e))
                        })?
                }
                #[cfg(all(feature = "coreml", not(feature = "cuda")))]
                {
                    builder
                        .with_execution_providers([CoreMLExecutionProvider::default().build()])
                        .map_err(|e| {
                            VisionError::onnx_runtime(format!("CoreML setup failed: {}", e))
                        })?
                }
                #[cfg(not(any(feature = "cuda", feature = "coreml")))]
                {
                    builder
                }
            } else {
                builder
            };

            builder
                .commit_from_file(&model_path)
                .map_err(|e| VisionError::onnx_runtime(format!("Failed to load model: {}", e)))
        })
        .await
        .map_err(|e| VisionError::onnx_runtime(format!("Task join error: {}", e)))?
    }

    /// Load character dictionary for recognition decoding.
    async fn load_char_dict(&self) -> Result<Vec<char>> {
        let dict_file = self.model_path.join("ppocr_keys_v1.txt");

        if !dict_file.exists() {
            // Return a basic ASCII dictionary if file doesn't exist
            return Ok((32u8..127).map(|c| c as char).collect());
        }

        let content = tokio::fs::read_to_string(&dict_file)
            .await
            .map_err(|e| VisionError::model_load(format!("Failed to read dictionary: {}", e)))?;

        let chars: Vec<char> = content.lines().filter_map(|l| l.chars().next()).collect();

        Ok(chars)
    }

    /// Preprocess image for detection.
    fn preprocess_for_detection(
        &self,
        img: &image::DynamicImage,
    ) -> Result<(ndarray::Array4<f32>, f32, f32)> {
        // PaddleOCR detection expects images resized to have shorter side = 960
        // while maintaining aspect ratio
        let (orig_w, orig_h) = (img.width(), img.height());
        let target_size = 960;

        let scale = if orig_w < orig_h {
            target_size as f32 / orig_w as f32
        } else {
            target_size as f32 / orig_h as f32
        };

        let new_w = ((orig_w as f32 * scale) as u32 / 32) * 32;
        let new_h = ((orig_h as f32 * scale) as u32 / 32) * 32;

        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        let rgb = resized.to_rgb8();

        // Convert to NCHW format
        let mut array = ndarray::Array4::<f32>::zeros((1, 3, new_h as usize, new_w as usize));

        // PaddleOCR normalization: (pixel - 127.5) / 127.5
        for (y, row) in rgb.rows().enumerate() {
            for (x, pixel) in row.enumerate() {
                array[[0, 0, y, x]] = (pixel[0] as f32 - 127.5) / 127.5;
                array[[0, 1, y, x]] = (pixel[1] as f32 - 127.5) / 127.5;
                array[[0, 2, y, x]] = (pixel[2] as f32 - 127.5) / 127.5;
            }
        }

        Ok((array, scale, scale))
    }

    /// Run detection model.
    async fn detect_regions(&self, img: &image::DynamicImage) -> Result<Vec<[f32; 4]>> {
        let mut session_guard = self.detection_session.write().await;
        let session = session_guard.as_mut().ok_or(VisionError::ModelNotLoaded)?;

        let (input, scale_x, scale_y) = self.preprocess_for_detection(img)?;

        // Convert to ort::Value
        let input_value = Value::from_array(input)
            .map_err(|e| VisionError::onnx_runtime(format!("Failed to create input: {}", e)))?;

        // Run inference
        let outputs = session
            .run(ort::inputs!["x" => input_value])
            .map_err(|e| VisionError::onnx_runtime(format!("Detection failed: {}", e)))?;

        // Parse DBNet output (probability map)
        let _prob_map = outputs
            .get("sigmoid_0.tmp_0")
            .or_else(|| outputs.get("save_infer_model/scale_0.tmp_1"))
            .ok_or_else(|| VisionError::onnx_runtime("Missing detection output"))?;

        // Process probability map to get bounding boxes
        // (Simplified - actual implementation uses DB post-processing)
        let boxes = self.post_process_detection(img.width(), img.height(), scale_x, scale_y)?;

        Ok(boxes)
    }

    /// Post-process detection output to get bounding boxes.
    fn post_process_detection(
        &self,
        _img_width: u32,
        _img_height: u32,
        _scale_x: f32,
        _scale_y: f32,
    ) -> Result<Vec<[f32; 4]>> {
        // Simplified - actual implementation uses:
        // 1. Threshold the probability map
        // 2. Find contours
        // 3. Compute bounding boxes from contours
        // 4. Apply NMS
        Ok(vec![])
    }

    /// Run recognition on detected regions.
    async fn recognize_regions(
        &self,
        img: &image::DynamicImage,
        regions: &[[f32; 4]],
    ) -> Result<Vec<(String, f32)>> {
        let mut session_guard = self.recognition_session.write().await;
        let session = session_guard.as_mut().ok_or(VisionError::ModelNotLoaded)?;

        let char_dict = self.char_dict.read().await;
        let (img_width, img_height) = (img.width() as f32, img.height() as f32);

        let mut results = Vec::new();

        for bbox in regions {
            // Extract and preprocess region
            let x1 = (bbox[0] * img_width) as u32;
            let y1 = (bbox[1] * img_height) as u32;
            let x2 = (bbox[2] * img_width) as u32;
            let y2 = (bbox[3] * img_height) as u32;

            let region = img.crop_imm(x1, y1, x2.saturating_sub(x1), y2.saturating_sub(y1));

            // Resize to fixed height (32 for CRNN)
            let target_height = 32u32;
            let aspect = region.width() as f32 / region.height() as f32;
            let target_width = ((target_height as f32 * aspect) as u32).clamp(32, 320);

            let resized = region.resize_exact(
                target_width,
                target_height,
                image::imageops::FilterType::Lanczos3,
            );

            let rgb = resized.to_rgb8();

            // Convert to NCHW
            let mut input = ndarray::Array4::<f32>::zeros((
                1,
                3,
                target_height as usize,
                target_width as usize,
            ));
            for (y, row) in rgb.rows().enumerate() {
                for (x, pixel) in row.enumerate() {
                    input[[0, 0, y, x]] = (pixel[0] as f32 - 127.5) / 127.5;
                    input[[0, 1, y, x]] = (pixel[1] as f32 - 127.5) / 127.5;
                    input[[0, 2, y, x]] = (pixel[2] as f32 - 127.5) / 127.5;
                }
            }

            // Convert to ort::Value
            let input_value = Value::from_array(input)
                .map_err(|e| VisionError::onnx_runtime(format!("Failed to create input: {}", e)))?;

            // Run recognition
            let outputs = session
                .run(ort::inputs!["x" => input_value])
                .map_err(|e| VisionError::onnx_runtime(format!("Recognition failed: {}", e)))?;

            // Decode output using CTC
            let text = self.ctc_decode(&outputs, &char_dict)?;
            let confidence = 0.95; // Placeholder

            results.push((text, confidence));
        }

        Ok(results)
    }

    /// CTC decode recognition output.
    fn ctc_decode(&self, _outputs: &SessionOutputs, _char_dict: &[char]) -> Result<String> {
        // Simplified CTC decoding
        // Actual implementation:
        // 1. Get argmax indices from output logits
        // 2. Remove duplicates and blanks
        // 3. Map indices to characters using dictionary
        Ok(String::new())
    }
}

#[async_trait]
impl VisionProvider for PaddleOcrClient {
    async fn process_image(&self, image_data: &[u8]) -> Result<OcrResult> {
        use std::time::Instant;
        let start = Instant::now();

        if !self.initialized.load(Ordering::SeqCst) {
            return Err(VisionError::ModelNotLoaded);
        }

        // Load image
        let img = image::load_from_memory(image_data)
            .map_err(|e| VisionError::image_processing(e.to_string()))?;
        let (width, height) = (img.width(), img.height());

        // Run detection
        let regions = self.detect_regions(&img).await?;

        // Run recognition
        let recognized = self.recognize_regions(&img, &regions).await?;

        // Build result
        let mut blocks = Vec::new();
        let mut full_text = String::new();

        for (i, ((text, confidence), bbox)) in recognized.iter().zip(regions.iter()).enumerate() {
            if !text.is_empty() {
                blocks.push(
                    TextBlock::new(text.clone())
                        .with_bbox(*bbox)
                        .with_confidence(*confidence)
                        .with_role(BlockRole::Text)
                        .with_order(i),
                );
                full_text.push_str(text);
                full_text.push('\n');
            }
        }

        let processing_time = start.elapsed().as_millis() as u64;

        let markdown = blocks
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(OcrResult {
            text: full_text.trim().to_string(),
            markdown,
            blocks,
            metadata: OcrMetadata {
                provider: "paddle".to_string(),
                model: Some(format!("paddleocr-{}", self.language)),
                processing_time_ms: processing_time,
                image_size: Some((width, height)),
                languages: vec![self.language.clone()],
                page_count: 1,
                current_page: 1,
            },
        })
    }

    async fn load_model(&self) -> Result<()> {
        tracing::info!(
            "Loading PaddleOCR models from: {}",
            self.model_path.display()
        );

        // Load detection model
        let det_session = self.load_onnx_model("det").await?;
        *self.detection_session.write().await = Some(det_session);

        // Load recognition model
        let rec_session = self.load_onnx_model("rec").await?;
        *self.recognition_session.write().await = Some(rec_session);

        // Optionally load classifier
        let cls_path = self.model_path.join("cls.onnx");
        if cls_path.exists() {
            let cls_session = self.load_onnx_model("cls").await?;
            *self.classifier_session.write().await = Some(cls_session);
        }

        // Load character dictionary
        let dict = self.load_char_dict().await?;
        *self.char_dict.write().await = dict;

        self.initialized.store(true, Ordering::SeqCst);
        tracing::info!("PaddleOCR models loaded successfully");

        Ok(())
    }

    async fn unload_model(&self) -> Result<()> {
        *self.detection_session.write().await = None;
        *self.recognition_session.write().await = None;
        *self.classifier_session.write().await = None;
        self.initialized.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_model_loaded(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    fn provider_name(&self) -> &str {
        "paddle"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            table_detection: true,
            layout_analysis: true,
            handwriting: false,
            multi_language: true,
            gpu_acceleration: self.use_gpu,
            languages: vec![
                "ch".to_string(), // Chinese + English
                "en".to_string(), // English only
                "japan".to_string(),
                "korean".to_string(),
                "french".to_string(),
                "german".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paddle_client_creation() {
        let client = PaddleOcrClient::new("/path/to/models", true);
        assert_eq!(client.provider_name(), "paddle");
        assert!(client.use_gpu);
    }

    #[test]
    fn test_paddle_with_language() {
        let client = PaddleOcrClient::new("/path/to/models", false).with_language("japan");
        assert_eq!(client.language, "japan");
    }

    #[test]
    fn test_paddle_capabilities() {
        let client = PaddleOcrClient::new("/path/to/models", true);
        let caps = client.capabilities();
        assert!(caps.table_detection);
        assert!(caps.multi_language);
        assert!(caps.gpu_acceleration);
    }

    #[test]
    fn test_model_not_loaded() {
        let client = PaddleOcrClient::new("/path/to/models", false);
        assert!(!client.is_model_loaded());
    }
}
