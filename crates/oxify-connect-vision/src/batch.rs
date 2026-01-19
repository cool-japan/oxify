//! Batch processing API for OCR operations.
//!
//! This module provides efficient batch processing of multiple images
//! with parallel execution, progress tracking, and result aggregation.

use crate::errors::{Result, VisionError};
use crate::providers::VisionProvider;
use crate::types::OcrResult;
use futures::stream::{self, StreamExt};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for batch processing.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of concurrent operations
    pub max_concurrency: usize,
    /// Continue processing on error (don't stop entire batch)
    pub continue_on_error: bool,
    /// Enable progress reporting
    pub report_progress: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_concurrency: num_cpus::get(),
            continue_on_error: true,
            report_progress: false,
        }
    }
}

impl BatchConfig {
    /// Create a configuration optimized for fast processing.
    ///
    /// Uses maximum available CPUs.
    pub fn fast() -> Self {
        Self {
            max_concurrency: num_cpus::get() * 2,
            continue_on_error: true,
            report_progress: false,
        }
    }

    /// Create a configuration optimized for GPU processing.
    ///
    /// Lower concurrency to avoid GPU memory exhaustion.
    pub fn gpu() -> Self {
        Self {
            max_concurrency: 4,
            continue_on_error: true,
            report_progress: true,
        }
    }

    /// Create a configuration with progress reporting enabled.
    pub fn with_progress() -> Self {
        Self {
            max_concurrency: num_cpus::get(),
            continue_on_error: true,
            report_progress: true,
        }
    }

    /// Set maximum concurrency.
    pub fn with_max_concurrency(mut self, max: usize) -> Self {
        self.max_concurrency = max.max(1);
        self
    }

    /// Set whether to continue on error.
    pub fn with_continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }

    /// Set whether to report progress.
    pub fn with_report_progress(mut self, report: bool) -> Self {
        self.report_progress = report;
        self
    }
}

/// Progress information for batch processing.
#[derive(Debug, Clone)]
pub struct BatchProgress {
    /// Total number of items to process
    pub total: usize,
    /// Number of items completed
    pub completed: usize,
    /// Number of items that succeeded
    pub succeeded: usize,
    /// Number of items that failed
    pub failed: usize,
}

impl BatchProgress {
    /// Create a new progress tracker.
    pub fn new(total: usize) -> Self {
        Self {
            total,
            completed: 0,
            succeeded: 0,
            failed: 0,
        }
    }

    /// Get completion percentage (0.0 to 1.0).
    pub fn percentage(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            self.completed as f32 / self.total as f32
        }
    }

    /// Check if processing is complete.
    pub fn is_complete(&self) -> bool {
        self.completed >= self.total
    }

    /// Get a formatted progress string.
    pub fn format(&self) -> String {
        format!(
            "{}/{} ({:.1}%) - {} succeeded, {} failed",
            self.completed,
            self.total,
            self.percentage() * 100.0,
            self.succeeded,
            self.failed
        )
    }
}

/// Result of a single batch item.
#[derive(Debug, Clone)]
pub struct BatchItemResult {
    /// Index of the item in the batch
    pub index: usize,
    /// OCR result (if successful)
    pub result: Option<OcrResult>,
    /// Error (if failed)
    pub error: Option<String>,
}

impl BatchItemResult {
    /// Check if this item succeeded.
    pub fn is_success(&self) -> bool {
        self.result.is_some()
    }

    /// Check if this item failed.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// Result of batch processing operation.
#[derive(Debug, Clone)]
pub struct BatchResult {
    /// Individual item results
    pub items: Vec<BatchItemResult>,
    /// Final progress statistics
    pub progress: BatchProgress,
    /// Total processing time (milliseconds)
    pub total_time_ms: u64,
}

impl BatchResult {
    /// Get all successful results.
    pub fn successful_results(&self) -> Vec<&OcrResult> {
        self.items
            .iter()
            .filter_map(|item| item.result.as_ref())
            .collect()
    }

    /// Get all errors.
    pub fn errors(&self) -> Vec<(usize, &str)> {
        self.items
            .iter()
            .filter_map(|item| item.error.as_ref().map(|e| (item.index, e.as_str())))
            .collect()
    }

    /// Get success rate (0.0 to 1.0).
    pub fn success_rate(&self) -> f32 {
        if self.items.is_empty() {
            0.0
        } else {
            self.progress.succeeded as f32 / self.items.len() as f32
        }
    }

    /// Get formatted summary.
    pub fn summary(&self) -> String {
        format!(
            "Batch processing complete: {} items in {}ms\n\
             Success: {} ({:.1}%), Failed: {}",
            self.items.len(),
            self.total_time_ms,
            self.progress.succeeded,
            self.success_rate() * 100.0,
            self.progress.failed
        )
    }
}

/// Progress callback for batch processing.
pub type ProgressCallback = Arc<dyn Fn(&BatchProgress) + Send + Sync>;

/// Batch processor for OCR operations.
pub struct BatchProcessor {
    config: BatchConfig,
    progress_callback: Option<ProgressCallback>,
}

impl BatchProcessor {
    /// Create a new batch processor with the given configuration.
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            progress_callback: None,
        }
    }

    /// Create a batch processor with default configuration.
    pub fn default_config() -> Self {
        Self::new(BatchConfig::default())
    }

    /// Set progress callback.
    ///
    /// The callback will be invoked periodically with progress updates.
    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&BatchProgress) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Arc::new(callback));
        self
    }

    /// Process multiple images in parallel.
    ///
    /// # Arguments
    ///
    /// * `provider` - The vision provider to use
    /// * `images` - Vector of image data (as byte slices)
    ///
    /// # Returns
    ///
    /// Batch result with individual item results and statistics.
    pub async fn process_batch(
        &self,
        provider: Arc<dyn VisionProvider>,
        images: Vec<Vec<u8>>,
    ) -> Result<BatchResult> {
        let start_time = std::time::Instant::now();
        let total = images.len();

        if total == 0 {
            return Ok(BatchResult {
                items: vec![],
                progress: BatchProgress::new(0),
                total_time_ms: 0,
            });
        }

        // Create progress tracker
        let progress = Arc::new(RwLock::new(BatchProgress::new(total)));
        let completed_count = Arc::new(AtomicUsize::new(0));
        let succeeded_count = Arc::new(AtomicUsize::new(0));
        let failed_count = Arc::new(AtomicUsize::new(0));

        // Process items in parallel using stream
        let items = stream::iter(images.into_iter().enumerate())
            .map(|(index, image_data)| {
                let provider = Arc::clone(&provider);
                let progress = Arc::clone(&progress);
                let completed = Arc::clone(&completed_count);
                let succeeded = Arc::clone(&succeeded_count);
                let failed = Arc::clone(&failed_count);
                let progress_callback = self.progress_callback.clone();
                let continue_on_error = self.config.continue_on_error;

                async move {
                    // Process image
                    let result = provider.process_image(&image_data).await;

                    // Update progress
                    let is_success = result.is_ok();
                    completed.fetch_add(1, Ordering::SeqCst);

                    if is_success {
                        succeeded.fetch_add(1, Ordering::SeqCst);
                    } else {
                        failed.fetch_add(1, Ordering::SeqCst);
                    }

                    // Update progress tracker
                    {
                        let mut p = progress.write().await;
                        p.completed = completed.load(Ordering::SeqCst);
                        p.succeeded = succeeded.load(Ordering::SeqCst);
                        p.failed = failed.load(Ordering::SeqCst);

                        // Invoke callback if set
                        if let Some(ref callback) = progress_callback {
                            callback(&p);
                        }
                    }

                    // Create item result
                    match result {
                        Ok(ocr_result) => Ok(BatchItemResult {
                            index,
                            result: Some(ocr_result),
                            error: None,
                        }),
                        Err(e) => {
                            if !continue_on_error {
                                // Return error to stop processing
                                return Err(e);
                            }
                            Ok(BatchItemResult {
                                index,
                                result: None,
                                error: Some(e.to_string()),
                            })
                        }
                    }
                }
            })
            .buffer_unordered(self.config.max_concurrency);

        // Collect results
        let results: Vec<Result<BatchItemResult>> = items.collect().await;

        // Check for fatal errors (if continue_on_error is false)
        let mut item_results = Vec::new();
        for result in results {
            match result {
                Ok(item) => item_results.push(item),
                Err(e) => {
                    // Fatal error occurred
                    return Err(e);
                }
            }
        }

        // Sort by index to maintain order
        item_results.sort_by_key(|item| item.index);

        let elapsed = start_time.elapsed();
        let final_progress = progress.read().await.clone();

        Ok(BatchResult {
            items: item_results,
            progress: final_progress,
            total_time_ms: elapsed.as_millis() as u64,
        })
    }

    /// Process multiple images with custom result processing.
    ///
    /// This method allows you to process results as they complete.
    pub async fn process_batch_with_callback<F>(
        &self,
        provider: Arc<dyn VisionProvider>,
        images: Vec<Vec<u8>>,
        callback: F,
    ) -> Result<()>
    where
        F: Fn(usize, Result<OcrResult>) + Send + Sync,
    {
        let total = images.len();

        if total == 0 {
            return Ok(());
        }

        // Process items in parallel
        let mut stream = stream::iter(images.into_iter().enumerate())
            .map(|(index, image_data)| {
                let provider = Arc::clone(&provider);
                async move {
                    let result = provider.process_image(&image_data).await;
                    (index, result)
                }
            })
            .buffer_unordered(self.config.max_concurrency);

        // Process results as they complete
        while let Some((index, result)) = stream.next().await {
            if !self.config.continue_on_error && result.is_err() {
                return result.map(|_| ());
            }
            callback(index, result);
        }

        Ok(())
    }
}

/// Helper function to process a batch of images with default settings.
///
/// This is a convenience function for simple batch processing.
pub async fn process_batch_simple(
    provider: Arc<dyn VisionProvider>,
    images: Vec<Vec<u8>>,
) -> Result<Vec<Result<OcrResult>>> {
    let processor = BatchProcessor::default_config();
    let batch_result = processor.process_batch(provider, images).await?;

    Ok(batch_result
        .items
        .into_iter()
        .map(|item| {
            if let Some(result) = item.result {
                Ok(result)
            } else {
                Err(VisionError::image_processing(
                    item.error.unwrap_or_else(|| "Unknown error".to_string()),
                ))
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MockVisionProvider;

    #[test]
    fn test_batch_config_default() {
        let config = BatchConfig::default();
        assert!(config.max_concurrency > 0);
        assert!(config.continue_on_error);
    }

    #[test]
    fn test_batch_config_fast() {
        let config = BatchConfig::fast();
        assert!(config.max_concurrency >= num_cpus::get());
    }

    #[test]
    fn test_batch_config_gpu() {
        let config = BatchConfig::gpu();
        assert_eq!(config.max_concurrency, 4);
        assert!(config.report_progress);
    }

    #[test]
    fn test_batch_config_builders() {
        let config = BatchConfig::default()
            .with_max_concurrency(8)
            .with_continue_on_error(false)
            .with_report_progress(true);

        assert_eq!(config.max_concurrency, 8);
        assert!(!config.continue_on_error);
        assert!(config.report_progress);
    }

    #[test]
    fn test_batch_progress() {
        let mut progress = BatchProgress::new(100);
        assert_eq!(progress.percentage(), 0.0);
        assert!(!progress.is_complete());

        progress.completed = 50;
        assert_eq!(progress.percentage(), 0.5);
        assert!(!progress.is_complete());

        progress.completed = 100;
        assert_eq!(progress.percentage(), 1.0);
        assert!(progress.is_complete());
    }

    #[test]
    fn test_batch_progress_format() {
        let progress = BatchProgress {
            total: 100,
            completed: 50,
            succeeded: 45,
            failed: 5,
        };

        let formatted = progress.format();
        assert!(formatted.contains("50/100"));
        assert!(formatted.contains("45 succeeded"));
        assert!(formatted.contains("5 failed"));
    }

    #[test]
    fn test_batch_item_result() {
        let success_item = BatchItemResult {
            index: 0,
            result: Some(OcrResult::from_text("test")),
            error: None,
        };
        assert!(success_item.is_success());
        assert!(!success_item.is_error());

        let error_item = BatchItemResult {
            index: 1,
            result: None,
            error: Some("error".to_string()),
        };
        assert!(!error_item.is_success());
        assert!(error_item.is_error());
    }

    #[tokio::test]
    async fn test_batch_processor_creation() {
        let processor = BatchProcessor::default_config();
        assert!(processor.config.max_concurrency > 0);
    }

    #[tokio::test]
    async fn test_process_batch_empty() {
        let provider = Arc::new(MockVisionProvider::new()) as Arc<dyn VisionProvider>;
        let processor = BatchProcessor::default_config();
        let result = processor.process_batch(provider, vec![]).await.unwrap();

        assert_eq!(result.items.len(), 0);
        assert_eq!(result.progress.total, 0);
    }

    #[tokio::test]
    async fn test_process_batch_single() {
        let provider = Arc::new(MockVisionProvider::new()) as Arc<dyn VisionProvider>;
        provider.load_model().await.unwrap();

        let processor = BatchProcessor::default_config();
        let images = vec![b"test image".to_vec()];
        let result = processor.process_batch(provider, images).await.unwrap();

        assert_eq!(result.items.len(), 1);
        assert_eq!(result.progress.total, 1);
        assert_eq!(result.progress.completed, 1);
        assert_eq!(result.progress.succeeded, 1);
    }

    #[tokio::test]
    async fn test_process_batch_multiple() {
        let provider = Arc::new(MockVisionProvider::new()) as Arc<dyn VisionProvider>;
        provider.load_model().await.unwrap();

        let processor = BatchProcessor::default_config();
        let images = vec![b"image1".to_vec(), b"image2".to_vec(), b"image3".to_vec()];
        let result = processor.process_batch(provider, images).await.unwrap();

        assert_eq!(result.items.len(), 3);
        assert_eq!(result.progress.succeeded, 3);
        assert_eq!(result.success_rate(), 1.0);
    }

    #[tokio::test]
    async fn test_batch_result_methods() {
        let batch_result = BatchResult {
            items: vec![
                BatchItemResult {
                    index: 0,
                    result: Some(OcrResult::from_text("success")),
                    error: None,
                },
                BatchItemResult {
                    index: 1,
                    result: None,
                    error: Some("failed".to_string()),
                },
            ],
            progress: BatchProgress {
                total: 2,
                completed: 2,
                succeeded: 1,
                failed: 1,
            },
            total_time_ms: 100,
        };

        assert_eq!(batch_result.successful_results().len(), 1);
        assert_eq!(batch_result.errors().len(), 1);
        assert_eq!(batch_result.success_rate(), 0.5);

        let summary = batch_result.summary();
        assert!(summary.contains("2 items"));
        assert!(summary.contains("100ms"));
    }

    #[tokio::test]
    async fn test_progress_callback() {
        let provider = Arc::new(MockVisionProvider::new()) as Arc<dyn VisionProvider>;
        provider.load_model().await.unwrap();

        let progress_updates = Arc::new(RwLock::new(Vec::new()));
        let updates_clone = Arc::clone(&progress_updates);

        let processor = BatchProcessor::new(BatchConfig::with_progress()).with_progress_callback(
            move |progress| {
                let updates = updates_clone.clone();
                let completed = progress.completed; // Clone the data, not the reference
                tokio::spawn(async move {
                    updates.write().await.push(completed);
                });
            },
        );

        let images = vec![b"img1".to_vec(), b"img2".to_vec()];
        let _result = processor.process_batch(provider, images).await.unwrap();

        // Give callbacks time to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let updates = progress_updates.read().await;
        assert!(!updates.is_empty());
    }

    #[tokio::test]
    async fn test_process_batch_simple() {
        let provider = Arc::new(MockVisionProvider::new()) as Arc<dyn VisionProvider>;
        provider.load_model().await.unwrap();

        let images = vec![b"test".to_vec()];
        let results = process_batch_simple(provider, images).await.unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());
    }
}
