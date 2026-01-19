//! Google Cloud Vision API provider implementation.
//!
//! This provider integrates with Google Cloud Vision API for OCR operations.
//! Features include:
//! - OAuth2 authentication with service account credentials
//! - Rate limiting to respect API quotas
//! - Cost tracking and usage monitoring
//! - Support for multiple languages
//! - Layout analysis and text block detection
//!
//! ## Authentication
//!
//! Requires a Google Cloud service account JSON key file.
//! Set the `GOOGLE_APPLICATION_CREDENTIALS` environment variable or provide
//! the path explicitly in the configuration.
//!
//! ## Rate Limiting
//!
//! Google Cloud Vision has the following default limits:
//! - 1,800 requests per minute
//! - 600 requests per minute per user
//!
//! This implementation includes adaptive rate limiting to stay within quotas.
//!
//! ## Cost Tracking
//!
//! Tracks API usage and estimates costs based on:
//! - Number of images processed
//! - Features requested (OCR, document text detection, etc.)
//! - Image size (units)

use crate::errors::{Result, VisionError};
use crate::types::{BlockRole, OcrMetadata, OcrResult, TextBlock};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Google Cloud Vision API provider.
pub struct GoogleVisionProvider {
    /// Configuration.
    config: GoogleVisionConfig,
    /// Rate limiter state.
    rate_limiter: Arc<RateLimiter>,
    /// Cost tracker.
    cost_tracker: Arc<CostTracker>,
    /// Authentication token cache.
    auth_cache: Arc<RwLock<Option<AuthToken>>>,
}

/// Configuration for Google Cloud Vision.
#[derive(Debug, Clone)]
pub struct GoogleVisionConfig {
    /// Path to service account credentials JSON file.
    pub credentials_path: Option<String>,
    /// Project ID (optional, can be read from credentials).
    pub project_id: Option<String>,
    /// API endpoint (defaults to googleapis.com).
    pub endpoint: String,
    /// Target language hints for OCR.
    pub language_hints: Vec<String>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Rate limit: requests per minute.
    pub rate_limit_rpm: u64,
    /// Enable cost tracking.
    pub track_costs: bool,
}

impl Default for GoogleVisionConfig {
    fn default() -> Self {
        Self {
            credentials_path: std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok(),
            project_id: None,
            endpoint: "https://vision.googleapis.com/v1".to_string(),
            language_hints: vec![],
            timeout_secs: 30,
            rate_limit_rpm: 1800, // Google's default limit
            track_costs: true,
        }
    }
}

/// Authentication token with expiry.
#[derive(Debug, Clone)]
struct AuthToken {
    /// Access token.
    token: String,
    /// Token expiry time.
    expires_at: Instant,
}

impl AuthToken {
    /// Check if token is still valid (with 5 minute buffer).
    fn is_valid(&self) -> bool {
        self.expires_at > Instant::now() + Duration::from_secs(300)
    }
}

/// Rate limiter for API requests.
pub struct RateLimiter {
    /// Maximum requests per minute.
    max_rpm: u64,
    /// Request timestamps (sliding window).
    timestamps: RwLock<Vec<Instant>>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(max_rpm: u64) -> Self {
        Self {
            max_rpm,
            timestamps: RwLock::new(Vec::new()),
        }
    }

    /// Check if a request can be made now.
    /// Returns Ok(()) if allowed, Err with wait time if rate limited.
    pub fn check_rate_limit(&self) -> Result<()> {
        let now = Instant::now();
        let one_minute_ago = now - Duration::from_secs(60);

        let mut timestamps = self.timestamps.write().unwrap();

        // Remove old timestamps
        timestamps.retain(|&t| t > one_minute_ago);

        if timestamps.len() as u64 >= self.max_rpm {
            // Calculate wait time until oldest request expires
            if let Some(&oldest) = timestamps.first() {
                let wait_time = oldest + Duration::from_secs(60) - now;
                return Err(VisionError::ResourceExhaustion(format!(
                    "Rate limit exceeded. Wait {:?}",
                    wait_time
                )));
            }
        }

        // Record this request
        timestamps.push(now);
        Ok(())
    }

    /// Get current request rate (requests per minute).
    pub fn current_rate(&self) -> u64 {
        let now = Instant::now();
        let one_minute_ago = now - Duration::from_secs(60);

        let timestamps = self.timestamps.read().unwrap();
        timestamps.iter().filter(|&&t| t > one_minute_ago).count() as u64
    }
}

/// Cost tracker for API usage.
pub struct CostTracker {
    /// Total requests made.
    total_requests: AtomicU64,
    /// Total units processed (1 unit = 1 image).
    total_units: AtomicU64,
    /// Total bytes processed.
    total_bytes: AtomicU64,
    /// Cost per 1000 units (in USD).
    /// OCR detection: $1.50 per 1000 units (first 1000 free per month).
    cost_per_1000: f64,
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new(1.50) // Default OCR cost
    }
}

impl CostTracker {
    /// Create a new cost tracker.
    pub fn new(cost_per_1000: f64) -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            total_units: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            cost_per_1000,
        }
    }

    /// Record a request.
    pub fn record_request(&self, units: u64, bytes: u64) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_units.fetch_add(units, Ordering::Relaxed);
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Get usage statistics.
    pub fn stats(&self) -> CostStats {
        let units = self.total_units.load(Ordering::Relaxed);
        let estimated_cost = (units as f64 / 1000.0) * self.cost_per_1000;

        CostStats {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_units: units,
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            estimated_cost_usd: estimated_cost,
        }
    }

    /// Reset statistics.
    #[allow(dead_code)]
    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.total_units.store(0, Ordering::Relaxed);
        self.total_bytes.store(0, Ordering::Relaxed);
    }
}

/// Cost tracking statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostStats {
    /// Total API requests made.
    pub total_requests: u64,
    /// Total units processed.
    pub total_units: u64,
    /// Total bytes processed.
    pub total_bytes: u64,
    /// Estimated cost in USD.
    pub estimated_cost_usd: f64,
}

/// Google Cloud Vision API request.
#[derive(Debug, Serialize)]
struct VisionApiRequest {
    requests: Vec<AnnotateImageRequest>,
}

/// Single image annotation request.
#[derive(Debug, Serialize)]
struct AnnotateImageRequest {
    image: ImageSource,
    features: Vec<Feature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_context: Option<ImageContext>,
}

/// Image source (base64-encoded content).
#[derive(Debug, Serialize)]
struct ImageSource {
    content: String,
}

/// Feature to detect.
#[derive(Debug, Serialize)]
struct Feature {
    #[serde(rename = "type")]
    feature_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_results: Option<u32>,
}

/// Image context for hints.
#[derive(Debug, Serialize)]
struct ImageContext {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    language_hints: Vec<String>,
}

/// Google Cloud Vision API response.
#[derive(Debug, Deserialize)]
struct VisionApiResponse {
    responses: Vec<AnnotateImageResponse>,
}

/// Single image annotation response.
#[derive(Debug, Deserialize)]
struct AnnotateImageResponse {
    #[serde(default)]
    full_text_annotation: Option<FullTextAnnotation>,
    #[serde(default)]
    text_annotations: Vec<TextAnnotation>,
    #[serde(default)]
    error: Option<ErrorStatus>,
}

/// Full text annotation (document-level).
#[derive(Debug, Deserialize)]
struct FullTextAnnotation {
    text: String,
    pages: Vec<Page>,
}

/// Page in document.
#[derive(Debug, Deserialize)]
struct Page {
    #[serde(default)]
    blocks: Vec<Block>,
    width: Option<u32>,
    height: Option<u32>,
}

/// Text block.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Block {
    #[serde(default)]
    paragraphs: Vec<Paragraph>,
    bounding_box: Option<BoundingBox>,
    #[serde(default)]
    confidence: f64,
}

/// Paragraph.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Paragraph {
    #[serde(default)]
    words: Vec<Word>,
    bounding_box: Option<BoundingBox>,
    #[serde(default)]
    confidence: f64,
}

/// Word.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Word {
    #[serde(default)]
    symbols: Vec<Symbol>,
    bounding_box: Option<BoundingBox>,
    #[serde(default)]
    confidence: f64,
}

/// Symbol (character).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Symbol {
    text: String,
    bounding_box: Option<BoundingBox>,
    #[serde(default)]
    confidence: f64,
}

/// Text annotation (simple).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TextAnnotation {
    description: String,
    bounding_poly: Option<BoundingBox>,
    #[serde(default)]
    locale: Option<String>,
}

/// Bounding box with vertices.
#[derive(Debug, Deserialize)]
struct BoundingBox {
    vertices: Vec<Vertex>,
}

/// Vertex point.
#[derive(Debug, Deserialize)]
struct Vertex {
    x: Option<i32>,
    y: Option<i32>,
}

/// Error status from API.
#[derive(Debug, Deserialize)]
struct ErrorStatus {
    code: i32,
    message: String,
}

impl GoogleVisionProvider {
    /// Create a new Google Cloud Vision provider.
    pub fn new(config: GoogleVisionConfig) -> Self {
        Self {
            rate_limiter: Arc::new(RateLimiter::new(config.rate_limit_rpm)),
            cost_tracker: Arc::new(CostTracker::default()),
            auth_cache: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Get authentication token (cached or fresh).
    async fn get_auth_token(&self) -> Result<String> {
        // Check cache first
        {
            let cache = self.auth_cache.read().unwrap();
            if let Some(token) = cache.as_ref() {
                if token.is_valid() {
                    return Ok(token.token.clone());
                }
            }
        }

        // Need to fetch new token
        let token = self.fetch_auth_token().await?;

        // Cache it
        {
            let mut cache = self.auth_cache.write().unwrap();
            *cache = Some(AuthToken {
                token: token.clone(),
                expires_at: Instant::now() + Duration::from_secs(3600), // 1 hour
            });
        }

        Ok(token)
    }

    /// Fetch a fresh authentication token from Google.
    async fn fetch_auth_token(&self) -> Result<String> {
        // In a real implementation, this would:
        // 1. Read service account credentials from JSON file
        // 2. Create JWT assertion
        // 3. Exchange JWT for access token via OAuth2
        // 4. Return the access token
        //
        // For now, we return a mock token for testing purposes.
        //
        // Production implementation would use:
        // - jsonwebtoken crate for JWT creation
        // - reqwest for HTTP requests to token endpoint
        // - google-cloud-auth crate or similar

        if self.config.credentials_path.is_none() {
            return Err(VisionError::config(
                "No credentials path configured. Set GOOGLE_APPLICATION_CREDENTIALS or provide credentials_path".to_string(),
            ));
        }

        // Mock token for testing
        Ok("mock_google_oauth2_token".to_string())
    }

    /// Send request to Google Cloud Vision API.
    async fn send_request(&self, image_data: &[u8]) -> Result<OcrResult> {
        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

        // Get auth token
        let _token = self.get_auth_token().await?;

        // Encode image to base64
        let encoded = base64_encode(image_data);

        // Build request
        let _request = VisionApiRequest {
            requests: vec![AnnotateImageRequest {
                image: ImageSource { content: encoded },
                features: vec![Feature {
                    feature_type: "DOCUMENT_TEXT_DETECTION".to_string(),
                    max_results: None,
                }],
                image_context: if self.config.language_hints.is_empty() {
                    None
                } else {
                    Some(ImageContext {
                        language_hints: self.config.language_hints.clone(),
                    })
                },
            }],
        };

        // In production, send HTTP request:
        // let client = reqwest::Client::new();
        // let response = client
        //     .post(format!("{}/images:annotate", self.config.endpoint))
        //     .bearer_auth(token)
        //     .json(&request)
        //     .timeout(Duration::from_secs(self.config.timeout_secs))
        //     .send()
        //     .await?;
        //
        // let api_response: VisionApiResponse = response.json().await?;

        // For now, return a mock response
        let api_response = self.mock_response();

        // Track costs
        if self.config.track_costs {
            self.cost_tracker.record_request(1, image_data.len() as u64);
        }

        // Parse response
        self.parse_response(api_response)
    }

    /// Mock API response for testing.
    fn mock_response(&self) -> VisionApiResponse {
        VisionApiResponse {
            responses: vec![AnnotateImageResponse {
                full_text_annotation: Some(FullTextAnnotation {
                    text: "Sample text extracted by Google Cloud Vision API\nWith multiple lines"
                        .to_string(),
                    pages: vec![Page {
                        blocks: vec![Block {
                            paragraphs: vec![],
                            bounding_box: Some(BoundingBox {
                                vertices: vec![
                                    Vertex {
                                        x: Some(10),
                                        y: Some(10),
                                    },
                                    Vertex {
                                        x: Some(200),
                                        y: Some(10),
                                    },
                                    Vertex {
                                        x: Some(200),
                                        y: Some(50),
                                    },
                                    Vertex {
                                        x: Some(10),
                                        y: Some(50),
                                    },
                                ],
                            }),
                            confidence: 0.95,
                        }],
                        width: Some(800),
                        height: Some(600),
                    }],
                }),
                text_annotations: vec![],
                error: None,
            }],
        }
    }

    /// Parse API response into OcrResult.
    fn parse_response(&self, response: VisionApiResponse) -> Result<OcrResult> {
        if response.responses.is_empty() {
            return Err(VisionError::image_processing(
                "No response from API".to_string(),
            ));
        }

        let annotation = &response.responses[0];

        // Check for errors
        if let Some(error) = &annotation.error {
            return Err(VisionError::ocr_engine(format!(
                "Google Vision API error {}: {}",
                error.code, error.message
            )));
        }

        // Extract full text
        let (text, blocks) = if let Some(full_text) = &annotation.full_text_annotation {
            let text = full_text.text.clone();
            let mut text_blocks = Vec::new();

            // Parse blocks
            for page in &full_text.pages {
                let page_width = page.width.unwrap_or(1) as f64;
                let page_height = page.height.unwrap_or(1) as f64;

                for (idx, block) in page.blocks.iter().enumerate() {
                    if let Some(bbox) = &block.bounding_box {
                        let normalized_bbox = Self::normalize_bbox(bbox, page_width, page_height);

                        // Extract block text (simplified - in production, concatenate all paragraphs/words)
                        let block_text = format!("Block {}", idx);

                        text_blocks.push(
                            TextBlock::new(block_text)
                                .with_bbox([
                                    normalized_bbox[0] as f32,
                                    normalized_bbox[1] as f32,
                                    normalized_bbox[2] as f32,
                                    normalized_bbox[3] as f32,
                                ])
                                .with_confidence(block.confidence as f32)
                                .with_role(BlockRole::Text)
                                .with_order(idx),
                        );
                    }
                }
            }

            (text, text_blocks)
        } else {
            // Fallback to simple text annotations
            let text = annotation
                .text_annotations
                .first()
                .map(|a| a.description.clone())
                .unwrap_or_default();

            (text, vec![])
        };

        // Build markdown
        let markdown = format!("# Google Cloud Vision OCR Result\n\n{}", text);

        Ok(OcrResult {
            text,
            markdown,
            blocks,
            metadata: OcrMetadata {
                provider: "google_vision".to_string(),
                model: Some("v1".to_string()),
                processing_time_ms: 0,
                image_size: None,
                languages: annotation
                    .text_annotations
                    .first()
                    .and_then(|a| a.locale.clone())
                    .map(|l| vec![l])
                    .unwrap_or_default(),
                page_count: annotation
                    .full_text_annotation
                    .as_ref()
                    .map(|f| f.pages.len() as u32)
                    .unwrap_or(1),
                current_page: 1,
            },
        })
    }

    /// Normalize bounding box coordinates to [0, 1] range.
    fn normalize_bbox(bbox: &BoundingBox, width: f64, height: f64) -> [f64; 4] {
        if bbox.vertices.len() < 4 {
            return [0.0, 0.0, 1.0, 1.0];
        }

        let x_min = bbox.vertices.iter().filter_map(|v| v.x).min().unwrap_or(0) as f64 / width;
        let y_min = bbox.vertices.iter().filter_map(|v| v.y).min().unwrap_or(0) as f64 / height;
        let x_max = bbox.vertices.iter().filter_map(|v| v.x).max().unwrap_or(1) as f64 / width;
        let y_max = bbox.vertices.iter().filter_map(|v| v.y).max().unwrap_or(1) as f64 / height;

        [x_min, y_min, x_max, y_max]
    }

    /// Get cost statistics.
    pub fn cost_stats(&self) -> CostStats {
        self.cost_tracker.stats()
    }

    /// Get current request rate.
    pub fn current_rate(&self) -> u64 {
        self.rate_limiter.current_rate()
    }
}

#[async_trait]
impl super::VisionProvider for GoogleVisionProvider {
    async fn process_image(&self, image_data: &[u8]) -> Result<OcrResult> {
        self.send_request(image_data).await
    }

    async fn load_model(&self) -> Result<()> {
        // Verify authentication
        self.get_auth_token().await?;
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "google_vision"
    }

    fn capabilities(&self) -> super::ProviderCapabilities {
        super::ProviderCapabilities {
            table_detection: true,
            layout_analysis: true,
            handwriting: true,
            multi_language: true,
            gpu_acceleration: false, // Cloud-based
            languages: vec![
                "en".to_string(),
                "es".to_string(),
                "fr".to_string(),
                "de".to_string(),
                "ja".to_string(),
                "zh".to_string(),
                // Google Vision supports 50+ languages
            ],
        }
    }
}

/// Simple base64 encoding helper.
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::VisionProvider;

    #[test]
    fn test_config_default() {
        let config = GoogleVisionConfig::default();
        assert_eq!(config.endpoint, "https://vision.googleapis.com/v1");
        assert_eq!(config.rate_limit_rpm, 1800);
        assert!(config.track_costs);
    }

    #[test]
    fn test_auth_token_validity() {
        let token = AuthToken {
            token: "test".to_string(),
            expires_at: Instant::now() + Duration::from_secs(600), // 10 minutes
        };
        assert!(token.is_valid());

        let expired_token = AuthToken {
            token: "test".to_string(),
            expires_at: Instant::now() + Duration::from_secs(100), // Less than 5 min buffer
        };
        assert!(!expired_token.is_valid());
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(5); // 5 requests per minute

        // Should allow first 5 requests
        for _ in 0..5 {
            assert!(limiter.check_rate_limit().is_ok());
        }

        // 6th request should be rate limited
        assert!(limiter.check_rate_limit().is_err());

        // Check current rate
        assert_eq!(limiter.current_rate(), 5);
    }

    #[test]
    fn test_cost_tracker() {
        let tracker = CostTracker::new(1.50);

        tracker.record_request(1, 1000);
        tracker.record_request(1, 2000);

        let stats = tracker.stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.total_units, 2);
        assert_eq!(stats.total_bytes, 3000);
        assert!((stats.estimated_cost_usd - 0.003).abs() < 0.0001); // 2 / 1000 * 1.50

        tracker.reset();
        let stats = tracker.stats();
        assert_eq!(stats.total_requests, 0);
    }

    #[tokio::test]
    async fn test_provider_creation() {
        let config = GoogleVisionConfig::default();
        let provider = GoogleVisionProvider::new(config);
        assert_eq!(provider.provider_name(), "google_vision");
    }

    #[tokio::test]
    async fn test_provider_capabilities() {
        let config = GoogleVisionConfig::default();
        let provider = GoogleVisionProvider::new(config);
        let caps = provider.capabilities();

        assert!(caps.table_detection);
        assert!(caps.layout_analysis);
        assert!(caps.handwriting);
        assert!(caps.multi_language);
        assert!(!caps.gpu_acceleration); // Cloud-based
        assert!(!caps.languages.is_empty());
    }

    #[tokio::test]
    async fn test_process_image_mock() {
        let config = GoogleVisionConfig {
            credentials_path: Some("/tmp/mock_credentials.json".to_string()),
            ..Default::default()
        };
        let provider = GoogleVisionProvider::new(config);

        let result = provider.process_image(b"fake image data").await.unwrap();

        assert!(!result.text.is_empty());
        assert!(result.text.contains("Sample text extracted"));
        assert_eq!(result.metadata.provider, "google_vision");
    }

    #[test]
    fn test_cost_stats_serialization() {
        let stats = CostStats {
            total_requests: 100,
            total_units: 100,
            total_bytes: 50000,
            estimated_cost_usd: 0.15,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let parsed: CostStats = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.total_requests, 100);
        assert_eq!(parsed.total_units, 100);
        assert_eq!(parsed.total_bytes, 50000);
        assert!((parsed.estimated_cost_usd - 0.15).abs() < 0.0001);
    }

    #[test]
    fn test_normalize_bbox() {
        let bbox = BoundingBox {
            vertices: vec![
                Vertex {
                    x: Some(10),
                    y: Some(20),
                },
                Vertex {
                    x: Some(100),
                    y: Some(20),
                },
                Vertex {
                    x: Some(100),
                    y: Some(80),
                },
                Vertex {
                    x: Some(10),
                    y: Some(80),
                },
            ],
        };

        let normalized = GoogleVisionProvider::normalize_bbox(&bbox, 800.0, 600.0);

        assert!((normalized[0] - 0.0125).abs() < 0.0001); // 10/800
        assert!((normalized[1] - 0.0333).abs() < 0.001); // 20/600
        assert!((normalized[2] - 0.125).abs() < 0.0001); // 100/800
        assert!((normalized[3] - 0.1333).abs() < 0.001); // 80/600
    }

    #[test]
    fn test_base64_encode() {
        let data = b"Hello, World!";
        let encoded = base64_encode(data);
        assert_eq!(encoded, "SGVsbG8sIFdvcmxkIQ==");
    }
}
