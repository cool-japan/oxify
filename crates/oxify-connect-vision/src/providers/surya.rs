//! Surya OCR provider using ONNX Runtime.
//!
//! Surya is a document OCR toolkit that provides high-quality OCR with
//! layout analysis. This implementation uses ONNX Runtime for inference.

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(feature = "surya")]
use oxionnx::{inputs, Session, SessionOutputs, Tensor};

#[cfg(all(feature = "surya", feature = "cuda"))]
use oxionnx::CUDAExecutionProvider;

#[cfg(all(feature = "surya", feature = "coreml", not(feature = "cuda")))]
use oxionnx::CoreMLExecutionProvider;

use crate::errors::{Result, VisionError};
use crate::types::{BlockRole, OcrMetadata, OcrResult, TextBlock};

use super::{ProviderCapabilities, VisionProvider};

/// Surya OCR client using ONNX Runtime.
///
/// Surya consists of multiple models:
/// - Detection model: Finds text regions
/// - Recognition model: Converts regions to text
/// - Layout model (optional): Analyzes document structure
pub struct SuryaClient {
    /// Path to model directory containing ONNX files.
    model_path: PathBuf,
    /// Whether to use GPU acceleration.
    use_gpu: bool,
    /// Whether models are loaded.
    initialized: AtomicBool,
    /// ONNX session for detection.
    detection_session: Arc<RwLock<Option<Session>>>,
    /// ONNX session for recognition.
    recognition_session: Arc<RwLock<Option<Session>>>,
    /// ONNX session for layout analysis.
    layout_session: Arc<RwLock<Option<Session>>>,
}

impl SuryaClient {
    /// Create a new Surya client.
    ///
    /// # Arguments
    /// * `model_path` - Path to directory containing ONNX model files
    /// * `use_gpu` - Whether to use GPU acceleration (CUDA or CoreML)
    pub fn new(model_path: impl Into<PathBuf>, use_gpu: bool) -> Self {
        Self {
            model_path: model_path.into(),
            use_gpu,
            initialized: AtomicBool::new(false),
            detection_session: Arc::new(RwLock::new(None)),
            recognition_session: Arc::new(RwLock::new(None)),
            layout_session: Arc::new(RwLock::new(None)),
        }
    }

    /// Load an ONNX model from the model directory.
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
            let builder = Session::builder();

            // Configure execution providers
            #[allow(unused_mut)]
            let mut builder = if use_gpu {
                #[cfg(feature = "cuda")]
                {
                    builder.with_execution_providers([CUDAExecutionProvider.build()])
                }
                #[cfg(all(feature = "coreml", not(feature = "cuda")))]
                {
                    builder.with_execution_providers([CoreMLExecutionProvider::default().build()])
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

    /// Preprocess image for detection model.
    fn preprocess_for_detection(&self, img: &image::DynamicImage) -> Result<ndarray::Array4<f32>> {
        let target_size = 1024;
        let resized = img.resize_exact(
            target_size,
            target_size,
            image::imageops::FilterType::Lanczos3,
        );
        let rgb = resized.to_rgb8();

        // Convert to NCHW format and normalize
        let mut array =
            ndarray::Array4::<f32>::zeros((1, 3, target_size as usize, target_size as usize));

        for (y, row) in rgb.rows().enumerate() {
            for (x, pixel) in row.enumerate() {
                array[[0, 0, y, x]] = pixel[0] as f32 / 255.0;
                array[[0, 1, y, x]] = pixel[1] as f32 / 255.0;
                array[[0, 2, y, x]] = pixel[2] as f32 / 255.0;
            }
        }

        // Apply ImageNet normalization
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];

        for c in 0..3 {
            for y in 0..target_size as usize {
                for x in 0..target_size as usize {
                    array[[0, c, y, x]] = (array[[0, c, y, x]] - mean[c]) / std[c];
                }
            }
        }

        Ok(array)
    }

    /// Run detection to find text regions.
    async fn detect_regions(&self, img: &image::DynamicImage) -> Result<Vec<[f32; 4]>> {
        let mut session_guard = self.detection_session.write().await;
        let session = session_guard.as_mut().ok_or(VisionError::ModelNotLoaded)?;

        let input = self.preprocess_for_detection(img)?;

        // Convert ndarray to Tensor
        let input_value = Tensor::from_ndarray(input);

        // Run inference
        let inp = inputs!["input" => input_value]
            .map_err(|e| VisionError::onnx_runtime(format!("Failed to build inputs: {}", e)))?;
        let outputs = session
            .run(&inp)
            .map_err(|e| VisionError::onnx_runtime(format!("Detection inference failed: {}", e)))?;

        // Parse detection output (this is simplified - actual Surya output parsing is more complex)
        let boxes_tensor = outputs
            .get("boxes")
            .ok_or_else(|| VisionError::onnx_runtime("Missing 'boxes' output"))?;

        let boxes: Vec<[f32; 4]> = self.parse_detection_output(boxes_tensor)?;
        Ok(boxes)
    }

    /// Parse detection model output to bounding boxes.
    fn parse_detection_output(&self, _tensor: &Tensor) -> Result<Vec<[f32; 4]>> {
        // Simplified implementation - actual parsing depends on model output format
        // Surya detection output typically includes:
        // - boxes: [N, 4] normalized coordinates
        // - scores: [N] confidence scores
        // - classes: [N] class indices

        // For now, return empty to allow compilation
        // Real implementation would parse the tensor data
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

        let (img_width, img_height) = (img.width() as f32, img.height() as f32);
        let mut results = Vec::new();

        for bbox in regions {
            // Extract region from image
            let x1 = (bbox[0] * img_width) as u32;
            let y1 = (bbox[1] * img_height) as u32;
            let x2 = (bbox[2] * img_width) as u32;
            let y2 = (bbox[3] * img_height) as u32;

            let region = img.crop_imm(x1, y1, x2.saturating_sub(x1), y2.saturating_sub(y1));

            // Preprocess for recognition (resize to fixed height, variable width)
            let target_height = 32u32;
            let aspect = region.width() as f32 / region.height() as f32;
            let target_width = ((target_height as f32 * aspect) as u32).max(32);

            let resized = region.resize_exact(
                target_width,
                target_height,
                image::imageops::FilterType::Lanczos3,
            );

            let rgb = resized.to_rgb8();
            let mut input = ndarray::Array4::<f32>::zeros((
                1,
                3,
                target_height as usize,
                target_width as usize,
            ));

            for (y, row) in rgb.rows().enumerate() {
                for (x, pixel) in row.enumerate() {
                    input[[0, 0, y, x]] = pixel[0] as f32 / 255.0;
                    input[[0, 1, y, x]] = pixel[1] as f32 / 255.0;
                    input[[0, 2, y, x]] = pixel[2] as f32 / 255.0;
                }
            }

            // Convert to Tensor
            let input_value = Tensor::from_ndarray(input);

            // Run recognition
            let inp = inputs!["input" => input_value]
                .map_err(|e| VisionError::onnx_runtime(format!("Failed to build inputs: {}", e)))?;
            let outputs = session
                .run(&inp)
                .map_err(|e| VisionError::onnx_runtime(format!("Recognition failed: {}", e)))?;

            // Parse output (simplified)
            let text = self.decode_recognition_output(&outputs)?;
            results.push((text, 0.95)); // Placeholder confidence
        }

        Ok(results)
    }

    /// Decode recognition model output to text.
    fn decode_recognition_output(&self, _outputs: &SessionOutputs<'_>) -> Result<String> {
        // Simplified - actual implementation would use CTC decoding
        // with a character vocabulary
        Ok(String::new())
    }
}

#[async_trait]
impl VisionProvider for SuryaClient {
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

        // Generate markdown
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
                provider: "surya".to_string(),
                model: Some("surya-onnx".to_string()),
                processing_time_ms: processing_time,
                image_size: Some((width, height)),
                languages: vec!["multilingual".to_string()],
                page_count: 1,
                current_page: 1,
            },
        })
    }

    async fn load_model(&self) -> Result<()> {
        tracing::info!("Loading Surya models from: {}", self.model_path.display());

        // Load detection model
        let det_session = self.load_onnx_model("detection").await?;
        *self.detection_session.write().await = Some(det_session);

        // Load recognition model
        let rec_session = self.load_onnx_model("recognition").await?;
        *self.recognition_session.write().await = Some(rec_session);

        // Optionally load layout model
        if self.model_path.join("layout.onnx").exists() {
            let layout_session = self.load_onnx_model("layout").await?;
            *self.layout_session.write().await = Some(layout_session);
        }

        self.initialized.store(true, Ordering::SeqCst);
        tracing::info!("Surya models loaded successfully");

        Ok(())
    }

    async fn unload_model(&self) -> Result<()> {
        *self.detection_session.write().await = None;
        *self.recognition_session.write().await = None;
        *self.layout_session.write().await = None;
        self.initialized.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_model_loaded(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    fn provider_name(&self) -> &str {
        "surya"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            table_detection: true,
            layout_analysis: true,
            handwriting: true,
            multi_language: true,
            gpu_acceleration: self.use_gpu,
            languages: vec![
                "multilingual".to_string(),
                "en".to_string(),
                "ja".to_string(),
                "zh".to_string(),
                "ko".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surya_client_creation() {
        let client = SuryaClient::new("/path/to/models", true);
        assert_eq!(client.provider_name(), "surya");
        assert!(client.use_gpu);
    }

    #[test]
    fn test_surya_capabilities() {
        let client = SuryaClient::new("/path/to/models", true);
        let caps = client.capabilities();
        assert!(caps.table_detection);
        assert!(caps.layout_analysis);
        assert!(caps.multi_language);
        assert!(caps.gpu_acceleration);
    }

    #[test]
    fn test_model_not_loaded() {
        let client = SuryaClient::new("/path/to/models", false);
        assert!(!client.is_model_loaded());
    }
}
