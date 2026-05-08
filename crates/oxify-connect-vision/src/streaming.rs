//! Streaming Processing for Video Frames
//!
//! This module provides real-time OCR processing capabilities for video streams
//! and frame sequences. It supports frame buffering, rate control, and async
//! processing for efficient video analysis.
//!
//! # Features
//!
//! - Real-time video frame processing
//! - Configurable frame buffering and rate limiting
//! - Async stream processing with backpressure
//! - Frame skipping and sampling strategies
//! - Temporal smoothing for text stabilization
//! - Change detection to avoid redundant processing
//! - Performance metrics and statistics
//!
//! # Example
//!
//! ```rust,ignore
//! use oxify_connect_vision::streaming::{StreamProcessor, StreamConfig};
//!
//! let config = StreamConfig::default()
//!     .with_max_fps(30.0)
//!     .with_buffer_size(10);
//!
//! let mut processor = StreamProcessor::new(provider, config);
//!
//! // Process frames from a video stream
//! for frame in video_frames {
//!     if let Some(result) = processor.process_frame(&frame).await? {
//!         println!("Text: {}", result.text);
//!     }
//! }
//! ```

use crate::errors::Result;
use crate::providers::VisionProvider;
use crate::types::OcrResult;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Streaming errors
#[derive(Debug, Error)]
pub enum StreamError {
    #[error("Buffer overflow: {0}")]
    BufferOverflow(String),

    #[error("Invalid frame rate: {0}")]
    InvalidFrameRate(String),

    #[error("Processing failed: {0}")]
    ProcessingFailed(String),

    #[error("Stream closed")]
    StreamClosed,
}

/// Frame sampling strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SamplingStrategy {
    /// Process every frame
    All,

    /// Process every Nth frame
    EveryNth(u32),

    /// Process based on time interval (milliseconds)
    TimeInterval(u64),

    /// Process only when significant changes detected
    ChangeDetection,

    /// Adaptive based on processing speed
    Adaptive,
}

impl Default for SamplingStrategy {
    fn default() -> Self {
        Self::TimeInterval(100) // 10 FPS default
    }
}

/// Stream processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Maximum frames per second to process
    pub max_fps: f64,

    /// Frame buffer size
    pub buffer_size: usize,

    /// Sampling strategy
    pub sampling_strategy: SamplingStrategy,

    /// Enable temporal smoothing
    pub enable_smoothing: bool,

    /// Smoothing window size (frames)
    pub smoothing_window: usize,

    /// Change detection threshold (0.0 to 1.0)
    pub change_threshold: f64,

    /// Enable frame preprocessing
    pub enable_preprocessing: bool,

    /// Timeout for processing a single frame (milliseconds)
    pub frame_timeout_ms: u64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_fps: 10.0,
            buffer_size: 30,
            sampling_strategy: SamplingStrategy::default(),
            enable_smoothing: true,
            smoothing_window: 3,
            change_threshold: 0.15,
            enable_preprocessing: true,
            frame_timeout_ms: 5000,
        }
    }
}

impl StreamConfig {
    /// Create a new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum FPS
    pub fn with_max_fps(mut self, fps: f64) -> Self {
        self.max_fps = fps.max(0.1);
        self
    }

    /// Set buffer size
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size.max(1);
        self
    }

    /// Set sampling strategy
    pub fn with_sampling_strategy(mut self, strategy: SamplingStrategy) -> Self {
        self.sampling_strategy = strategy;
        self
    }

    /// Enable/disable smoothing
    pub fn with_smoothing(mut self, enabled: bool) -> Self {
        self.enable_smoothing = enabled;
        self
    }

    /// Set change detection threshold
    pub fn with_change_threshold(mut self, threshold: f64) -> Self {
        self.change_threshold = threshold.clamp(0.0, 1.0);
        self
    }
}

/// Frame metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMetadata {
    /// Frame number in sequence
    pub frame_number: u64,

    /// Timestamp when frame was received
    pub timestamp: std::time::SystemTime,

    /// Frame size in bytes
    pub size: usize,

    /// Frame dimensions (width, height)
    pub dimensions: Option<(u32, u32)>,

    /// Whether this frame was processed or skipped
    pub processed: bool,

    /// Processing duration (if processed)
    pub processing_time: Option<Duration>,
}

/// Buffered frame
#[derive(Debug, Clone)]
struct BufferedFrame {
    /// Frame data
    data: Vec<u8>,

    /// Frame metadata
    metadata: FrameMetadata,

    /// Hash for change detection
    hash: u64,
}

impl BufferedFrame {
    /// Create a new buffered frame
    fn new(data: Vec<u8>, frame_number: u64) -> Self {
        let hash = Self::compute_hash(&data);
        let size = data.len();

        Self {
            data,
            metadata: FrameMetadata {
                frame_number,
                timestamp: std::time::SystemTime::now(),
                size,
                dimensions: None,
                processed: false,
                processing_time: None,
            },
            hash,
        }
    }

    /// Compute a simple hash for change detection
    fn compute_hash(data: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Sample bytes for faster hashing
        let step = (data.len() / 100).max(1);
        for i in (0..data.len()).step_by(step) {
            data[i].hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Check if frame is significantly different from another
    fn is_different_from(&self, other: &BufferedFrame, threshold: f64) -> bool {
        if self.data.len() != other.data.len() {
            return true;
        }

        // Simple hash-based comparison
        let hash_diff = (self.hash as i64 - other.hash as i64).abs() as f64;
        let max_hash = u64::MAX as f64;

        (hash_diff / max_hash) > threshold
    }
}

/// Stream processing statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamStats {
    /// Total frames received
    pub frames_received: u64,

    /// Frames processed
    pub frames_processed: u64,

    /// Frames skipped
    pub frames_skipped: u64,

    /// Total processing time
    pub total_processing_time: Duration,

    /// Average processing time per frame
    pub avg_processing_time: Duration,

    /// Current FPS
    pub current_fps: f64,

    /// Buffer overflows
    pub buffer_overflows: u64,

    /// Processing errors
    pub processing_errors: u64,
}

impl StreamStats {
    /// Update average processing time
    fn update_avg_processing_time(&mut self, new_time: Duration) {
        let total_ms = self.total_processing_time.as_millis() as u64 + new_time.as_millis() as u64;
        self.total_processing_time = Duration::from_millis(total_ms);

        if self.frames_processed > 0 {
            self.avg_processing_time = self.total_processing_time / self.frames_processed as u32;
        }
    }
}

/// Stream processor for real-time OCR
pub struct StreamProcessor<P: VisionProvider> {
    /// OCR provider
    provider: Arc<P>,

    /// Configuration
    config: StreamConfig,

    /// Frame buffer
    buffer: Arc<Mutex<VecDeque<BufferedFrame>>>,

    /// Processing statistics
    stats: Arc<Mutex<StreamStats>>,

    /// Last processed frame
    last_processed: Arc<Mutex<Option<BufferedFrame>>>,

    /// Last processing time
    last_process_time: Arc<Mutex<Option<Instant>>>,

    /// Result smoothing buffer
    smoothing_buffer: Arc<Mutex<VecDeque<OcrResult>>>,

    /// Frame counter
    frame_counter: Arc<Mutex<u64>>,
}

impl<P: VisionProvider> StreamProcessor<P> {
    /// Create a new stream processor
    pub fn new(provider: P, config: StreamConfig) -> Self {
        Self {
            provider: Arc::new(provider),
            config,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            stats: Arc::new(Mutex::new(StreamStats::default())),
            last_processed: Arc::new(Mutex::new(None)),
            last_process_time: Arc::new(Mutex::new(None)),
            smoothing_buffer: Arc::new(Mutex::new(VecDeque::new())),
            frame_counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Process a single frame
    pub async fn process_frame(&self, frame_data: &[u8]) -> Result<Option<OcrResult>> {
        // Update frame counter
        let frame_number = {
            let mut counter = self.frame_counter.lock().unwrap_or_else(|e| e.into_inner());
            *counter += 1;
            *counter
        };

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.frames_received += 1;
        }

        // Create buffered frame
        let frame = BufferedFrame::new(frame_data.to_vec(), frame_number);

        // Check if we should process this frame
        if !self.should_process_frame(&frame)? {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.frames_skipped += 1;
            return Ok(None);
        }

        // Add to buffer
        self.add_to_buffer(frame.clone())?;

        // Process the frame
        let start_time = Instant::now();

        match self.provider.process_image(&frame.data).await {
            Ok(mut result) => {
                let processing_time = start_time.elapsed();

                // Update stats
                {
                    let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
                    stats.frames_processed += 1;
                    stats.update_avg_processing_time(processing_time);

                    // Calculate current FPS
                    if let Some(last_time) = *self.last_process_time.lock().unwrap_or_else(|e| e.into_inner()) {
                        let elapsed = start_time.duration_since(last_time);
                        if elapsed.as_secs_f64() > 0.0 {
                            stats.current_fps = 1.0 / elapsed.as_secs_f64();
                        }
                    }
                }

                // Update last process time
                *self.last_process_time.lock().unwrap_or_else(|e| e.into_inner()) = Some(start_time);

                // Update last processed frame
                *self.last_processed.lock().unwrap_or_else(|e| e.into_inner()) = Some(frame);

                // Apply smoothing if enabled
                if self.config.enable_smoothing {
                    result = self.apply_smoothing(result)?;
                }

                Ok(Some(result))
            }
            Err(e) => {
                let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
                stats.processing_errors += 1;
                Err(e)
            }
        }
    }

    /// Check if frame should be processed based on sampling strategy
    fn should_process_frame(&self, frame: &BufferedFrame) -> Result<bool> {
        match self.config.sampling_strategy {
            SamplingStrategy::All => Ok(true),

            SamplingStrategy::EveryNth(n) => {
                Ok(frame.metadata.frame_number.is_multiple_of(n as u64))
            }

            SamplingStrategy::TimeInterval(interval_ms) => {
                if let Some(last_time) = *self.last_process_time.lock().unwrap_or_else(|e| e.into_inner()) {
                    let elapsed = last_time.elapsed();
                    Ok(elapsed.as_millis() >= interval_ms as u128)
                } else {
                    Ok(true) // Process first frame
                }
            }

            SamplingStrategy::ChangeDetection => {
                if let Some(last_frame) = self.last_processed.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    Ok(frame.is_different_from(last_frame, self.config.change_threshold))
                } else {
                    Ok(true) // Process first frame
                }
            }

            SamplingStrategy::Adaptive => {
                // Adapt based on current processing speed
                let stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());

                if stats.avg_processing_time.as_secs_f64() > 0.0 {
                    let target_interval = 1.0 / self.config.max_fps;
                    let can_process = stats.avg_processing_time.as_secs_f64() <= target_interval;
                    Ok(can_process)
                } else {
                    Ok(true)
                }
            }
        }
    }

    /// Add frame to buffer
    fn add_to_buffer(&self, frame: BufferedFrame) -> Result<()> {
        let mut buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());

        if buffer.len() >= self.config.buffer_size {
            // Buffer is full, remove oldest frame
            buffer.pop_front();

            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.buffer_overflows += 1;
        }

        buffer.push_back(frame);
        Ok(())
    }

    /// Apply temporal smoothing to results
    fn apply_smoothing(&self, result: OcrResult) -> Result<OcrResult> {
        let mut smoothing_buffer = self.smoothing_buffer.lock().unwrap_or_else(|e| e.into_inner());

        smoothing_buffer.push_back(result.clone());

        if smoothing_buffer.len() > self.config.smoothing_window {
            smoothing_buffer.pop_front();
        }

        // For now, just return the latest result
        // In a more advanced implementation, we could:
        // - Merge text from multiple frames
        // - Average confidence scores
        // - Use majority voting for text detection
        Ok(result)
    }

    /// Get current statistics
    pub fn get_stats(&self) -> StreamStats {
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        *stats = StreamStats::default();
    }

    /// Get buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Clear buffer
    pub fn clear_buffer(&self) {
        self.buffer.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Get configuration
    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Get provider reference
    pub fn provider(&self) -> &P {
        &self.provider
    }
}

/// Async frame stream processor
pub struct AsyncFrameStream<P: VisionProvider> {
    /// Stream processor
    processor: Arc<StreamProcessor<P>>,
}

impl<P: VisionProvider> AsyncFrameStream<P> {
    /// Create a new async frame stream
    pub fn new(provider: P, config: StreamConfig) -> Self {
        Self {
            processor: Arc::new(StreamProcessor::new(provider, config)),
        }
    }

    /// Process a batch of frames
    pub async fn process_batch(&self, frames: Vec<Vec<u8>>) -> Result<Vec<Option<OcrResult>>> {
        let mut results = Vec::new();

        for frame in frames {
            let result = self.processor.process_frame(&frame).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Get processor statistics
    pub fn stats(&self) -> StreamStats {
        self.processor.get_stats()
    }

    /// Get processor
    pub fn processor(&self) -> &StreamProcessor<P> {
        &self.processor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MockVisionProvider;

    #[test]
    fn test_stream_config_default() {
        let config = StreamConfig::default();
        assert_eq!(config.max_fps, 10.0);
        assert_eq!(config.buffer_size, 30);
        assert!(config.enable_smoothing);
    }

    #[test]
    fn test_stream_config_builder() {
        let config = StreamConfig::new()
            .with_max_fps(30.0)
            .with_buffer_size(50)
            .with_smoothing(false)
            .with_change_threshold(0.2);

        assert_eq!(config.max_fps, 30.0);
        assert_eq!(config.buffer_size, 50);
        assert!(!config.enable_smoothing);
        assert_eq!(config.change_threshold, 0.2);
    }

    #[test]
    fn test_sampling_strategy() {
        assert_eq!(
            SamplingStrategy::default(),
            SamplingStrategy::TimeInterval(100)
        );
    }

    #[tokio::test]
    async fn test_stream_processor_creation() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default();
        let processor = StreamProcessor::new(provider, config);

        assert_eq!(processor.buffer_size(), 0);
        let stats = processor.get_stats();
        assert_eq!(stats.frames_received, 0);
    }

    #[tokio::test]
    async fn test_process_single_frame() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default();
        let processor = StreamProcessor::new(provider, config);

        let frame = vec![0u8; 100];
        let result = processor.process_frame(&frame).await;

        assert!(result.is_ok());

        let stats = processor.get_stats();
        assert_eq!(stats.frames_received, 1);
    }

    #[tokio::test]
    async fn test_process_multiple_frames() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default();
        let processor = StreamProcessor::new(provider, config);

        for i in 0..10 {
            let frame = vec![i as u8; 100];
            let _result = processor.process_frame(&frame).await;
        }

        let stats = processor.get_stats();
        assert_eq!(stats.frames_received, 10);
    }

    #[tokio::test]
    async fn test_buffer_overflow() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default()
            .with_buffer_size(5)
            .with_sampling_strategy(SamplingStrategy::All); // Process all frames
        let processor = StreamProcessor::new(provider, config);

        // Process more frames than buffer size
        for i in 0..10 {
            let frame = vec![i as u8; 100];
            let _result = processor.process_frame(&frame).await;
        }

        let stats = processor.get_stats();
        assert!(stats.buffer_overflows > 0);
        assert_eq!(processor.buffer_size(), 5);
    }

    #[tokio::test]
    async fn test_sampling_every_nth() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default().with_sampling_strategy(SamplingStrategy::EveryNth(2));
        let processor = StreamProcessor::new(provider, config);

        for i in 0..10 {
            let frame = vec![i as u8; 100];
            let _result = processor.process_frame(&frame).await;
        }

        let stats = processor.get_stats();
        assert_eq!(stats.frames_received, 10);
        assert!(stats.frames_processed <= 5); // Should process ~half
    }

    #[tokio::test]
    async fn test_sampling_all() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default().with_sampling_strategy(SamplingStrategy::All);
        let processor = StreamProcessor::new(provider, config);

        for i in 0..5 {
            let frame = vec![i as u8; 100];
            let _result = processor.process_frame(&frame).await;
        }

        let stats = processor.get_stats();
        assert_eq!(stats.frames_received, 5);
        assert_eq!(stats.frames_processed, 5);
    }

    #[tokio::test]
    async fn test_change_detection() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default()
            .with_sampling_strategy(SamplingStrategy::ChangeDetection)
            .with_change_threshold(0.1);
        let processor = StreamProcessor::new(provider, config);

        // Same frame multiple times
        let frame = vec![42u8; 100];
        for _ in 0..5 {
            let _result = processor.process_frame(&frame).await;
        }

        let stats = processor.get_stats();
        // Should skip similar frames after the first one
        assert!(stats.frames_skipped > 0);
    }

    #[tokio::test]
    async fn test_stats_reset() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default();
        let processor = StreamProcessor::new(provider, config);

        let frame = vec![0u8; 100];
        let _result = processor.process_frame(&frame).await;

        processor.reset_stats();
        let stats = processor.get_stats();
        assert_eq!(stats.frames_received, 0);
    }

    #[tokio::test]
    async fn test_clear_buffer() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default();
        let processor = StreamProcessor::new(provider, config);

        for i in 0..5 {
            let frame = vec![i as u8; 100];
            let _result = processor.process_frame(&frame).await;
        }

        assert!(processor.buffer_size() > 0);
        processor.clear_buffer();
        assert_eq!(processor.buffer_size(), 0);
    }

    #[tokio::test]
    async fn test_async_frame_stream() {
        let provider = MockVisionProvider::new();
        let config = StreamConfig::default();
        let stream = AsyncFrameStream::new(provider, config);

        let frames = vec![vec![1u8; 100], vec![2u8; 100], vec![3u8; 100]];

        let results = stream.process_batch(frames).await.unwrap();
        assert_eq!(results.len(), 3);

        let stats = stream.stats();
        assert_eq!(stats.frames_received, 3);
    }

    #[tokio::test]
    async fn test_buffered_frame_hash() {
        let frame1 = BufferedFrame::new(vec![1, 2, 3, 4, 5], 1);
        let frame2 = BufferedFrame::new(vec![1, 2, 3, 4, 5], 2);
        let frame3 = BufferedFrame::new(vec![5, 4, 3, 2, 1], 3);

        // Same data should have same hash
        assert_eq!(frame1.hash, frame2.hash);

        // Different data should have different hash
        assert_ne!(frame1.hash, frame3.hash);
    }

    #[tokio::test]
    async fn test_buffered_frame_difference() {
        let frame1 = BufferedFrame::new(vec![1u8; 100], 1);
        let frame2 = BufferedFrame::new(vec![1u8; 100], 2);
        let frame3 = BufferedFrame::new(vec![2u8; 100], 3);

        // Same frames should not be different
        assert!(!frame1.is_different_from(&frame2, 0.1));

        // Different frames should be detected
        assert!(frame1.is_different_from(&frame3, 0.1));
    }

    #[test]
    fn test_frame_metadata() {
        let metadata = FrameMetadata {
            frame_number: 42,
            timestamp: std::time::SystemTime::now(),
            size: 1024,
            dimensions: Some((640, 480)),
            processed: true,
            processing_time: Some(Duration::from_millis(50)),
        };

        assert_eq!(metadata.frame_number, 42);
        assert_eq!(metadata.size, 1024);
        assert_eq!(metadata.dimensions, Some((640, 480)));
        assert!(metadata.processed);
    }

    #[test]
    fn test_stream_stats_update() {
        let mut stats = StreamStats::default();

        stats.update_avg_processing_time(Duration::from_millis(100));
        stats.frames_processed = 1;

        assert_eq!(stats.total_processing_time.as_millis(), 100);
    }
}
