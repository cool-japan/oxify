//! Azure Computer Vision API provider implementation.
//!
//! This provider integrates with the Azure AI Vision Image Analysis 4.0 REST API
//! (imageanalysis:analyze endpoint) for OCR operations using the `read` feature.
//!
//! ## Authentication
//!
//! Requires an Azure Cognitive Services resource.
//! Set the following environment variables:
//!   - `AZURE_VISION_ENDPOINT` — e.g. `https://<region>.cognitiveservices.azure.com`
//!   - `AZURE_VISION_KEY`      — the Ocp-Apim-Subscription-Key value
//!
//! ## Rate Limiting
//!
//! Azure Computer Vision free tier allows 20 transactions per second (TPS).
//! The sliding-window rate limiter enforces this per-second quota.
//!
//! ## Cost Tracking
//!
//! Azure Computer Vision Read API pricing: **$1.00 per 1 000 calls**
//! (first 5 000 calls per month are free on the free tier).
//! Cost is tracked at `$0.001 / call`.

use crate::errors::{Result, VisionError};
use crate::types::{OcrMetadata, OcrResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Azure Computer Vision provider.
#[derive(Debug, Clone)]
pub struct AzureVisionConfig {
    /// Azure Cognitive Services endpoint.
    /// e.g. `https://<region>.cognitiveservices.azure.com`
    pub endpoint: String,
    /// API key (Ocp-Apim-Subscription-Key).
    pub api_key: String,
    /// API version string for the Image Analysis 4.0 endpoint.
    pub api_version: String,
    /// Optional language hint (BCP-47 tag, e.g. `"en"`, `"ja"`).
    pub language: Option<String>,
    /// HTTP request timeout in seconds.
    pub timeout_secs: u64,
    /// Rate limit: maximum requests per second (free tier default = 20).
    pub rate_limit_rps: u64,
    /// Whether to accumulate cost statistics.
    pub track_costs: bool,
}

impl Default for AzureVisionConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            api_key: String::new(),
            api_version: "2024-02-01".to_string(),
            language: None,
            timeout_secs: 30,
            rate_limit_rps: 20,
            track_costs: true,
        }
    }
}

impl AzureVisionConfig {
    /// Build configuration from environment variables.
    ///
    /// Reads `AZURE_VISION_ENDPOINT` and `AZURE_VISION_KEY`.
    /// All other fields take their default values.
    pub fn from_env() -> Result<Self> {
        let endpoint = std::env::var("AZURE_VISION_ENDPOINT").map_err(|_| {
            VisionError::config("AZURE_VISION_ENDPOINT environment variable is not set")
        })?;
        let api_key = std::env::var("AZURE_VISION_KEY")
            .map_err(|_| VisionError::config("AZURE_VISION_KEY environment variable is not set"))?;
        Ok(Self {
            endpoint,
            api_key,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// RateLimiter — sliding-window per-second quota
// ---------------------------------------------------------------------------

/// Sliding-window rate limiter that enforces a maximum number of requests
/// within the most recent one-second window.
pub struct RateLimiter {
    /// Maximum allowed requests per second.
    max_rps: u64,
    /// Timestamps of past requests inside the sliding window.
    timestamps: RwLock<Vec<Instant>>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given requests-per-second limit.
    pub fn new(max_rps: u64) -> Self {
        Self {
            max_rps,
            timestamps: RwLock::new(Vec::new()),
        }
    }

    /// Check whether a new request can be issued right now.
    ///
    /// Returns `Ok(())` when allowed, or
    /// `Err(VisionError::ResourceExhaustion)` with the required wait duration
    /// when the window is saturated.
    pub fn check_and_record(&self) -> Result<()> {
        let now = Instant::now();
        let one_second_ago = now - Duration::from_secs(1);

        let mut timestamps = self
            .timestamps
            .write()
            .map_err(|_| VisionError::Other("rate limiter lock poisoned".to_string()))?;

        // Evict timestamps that have fallen out of the sliding window.
        timestamps.retain(|&t| t > one_second_ago);

        if timestamps.len() as u64 >= self.max_rps {
            // Oldest timestamp in the window tells us how long we need to wait.
            if let Some(&oldest) = timestamps.first() {
                let wait = oldest + Duration::from_secs(1) - now;
                return Err(VisionError::ResourceExhaustion(format!(
                    "Azure Vision rate limit ({} rps) exceeded. Retry after {:?}",
                    self.max_rps, wait
                )));
            }
        }

        timestamps.push(now);
        Ok(())
    }

    /// Return the number of requests recorded inside the current one-second window.
    pub fn requests_in_window(&self) -> u64 {
        let now = Instant::now();
        let one_second_ago = now - Duration::from_secs(1);
        let timestamps = self.timestamps.read().unwrap_or_else(|e| e.into_inner());
        timestamps.iter().filter(|&&t| t > one_second_ago).count() as u64
    }

    /// Return the age of the oldest request timestamp in the current window,
    /// or `None` if the window is empty.
    #[allow(dead_code)]
    pub fn oldest_request_age(&self) -> Option<Duration> {
        let now = Instant::now();
        let one_second_ago = now - Duration::from_secs(1);
        let timestamps = self.timestamps.read().unwrap_or_else(|e| e.into_inner());
        timestamps
            .iter()
            .filter(|&&t| t > one_second_ago)
            .min()
            .map(|&t| now.duration_since(t))
    }
}

// ---------------------------------------------------------------------------
// CostTracker
// ---------------------------------------------------------------------------

/// Accumulated cost statistics for Azure Computer Vision calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostStats {
    /// Total number of API calls recorded.
    pub total_calls: u64,
    /// Estimated total cost in USD.
    pub total_cost_usd: f64,
    /// Total number of pages processed (each call counts as one page here).
    pub total_pages: u64,
}

/// Thread-safe accumulator for call counts and estimated cost.
pub struct CostTracker {
    /// Atomic counter for total calls.
    total_calls: AtomicU64,
    /// Atomic counter for total pages processed.
    total_pages: AtomicU64,
    /// Accumulated USD cost (requires a mutex for floating-point safety).
    total_cost_usd: std::sync::Mutex<f64>,
    /// Per-call cost in USD.
    cost_per_call: f64,
}

impl CostTracker {
    /// Create a cost tracker with the given per-call cost in USD.
    pub fn new(cost_per_call: f64) -> Self {
        Self {
            total_calls: AtomicU64::new(0),
            total_pages: AtomicU64::new(0),
            total_cost_usd: std::sync::Mutex::new(0.0),
            cost_per_call,
        }
    }

    /// Record a completed API call with the given page count.
    pub fn record_call(&self, pages: u32) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.total_pages
            .fetch_add(u64::from(pages), Ordering::Relaxed);
        if let Ok(mut cost) = self.total_cost_usd.lock() {
            *cost += self.cost_per_call;
        }
    }

    /// Return a snapshot of the accumulated statistics.
    pub fn stats(&self) -> CostStats {
        let total_cost_usd = self.total_cost_usd.lock().map(|g| *g).unwrap_or(0.0);
        CostStats {
            total_calls: self.total_calls.load(Ordering::Relaxed),
            total_cost_usd,
            total_pages: self.total_pages.load(Ordering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// Azure REST API response types
// ---------------------------------------------------------------------------

/// Top-level response from the `imageanalysis:analyze` endpoint.
#[derive(Debug, Deserialize)]
struct AzureAnalyzeResponse {
    /// Read (OCR) result returned when the `read` feature is requested.
    #[serde(rename = "readResult")]
    read_result: Option<ReadResult>,
}

/// Read (OCR) result container.
#[derive(Debug, Deserialize)]
struct ReadResult {
    /// Blocks of text recognized in the image.
    blocks: Vec<TextBlockData>,
}

/// A block of recognized text lines.
#[derive(Debug, Deserialize)]
struct TextBlockData {
    /// Lines that make up this text block.
    lines: Vec<LineData>,
}

/// A single line of recognized text.
#[derive(Debug, Deserialize)]
struct LineData {
    /// Plain text content of the line.
    text: String,
    /// Optional polygon bounding the line text.
    /// Preserved for callers that need layout information; not used internally.
    #[serde(rename = "boundingPolygon")]
    #[allow(dead_code)]
    bounding_polygon: Option<Vec<PolygonPoint>>,
}

/// A single vertex in a bounding polygon.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PolygonPoint {
    x: f32,
    y: f32,
}

// ---------------------------------------------------------------------------
// Response conversion
// ---------------------------------------------------------------------------

/// Convert an `AzureAnalyzeResponse` into the crate-level `OcrResult`.
///
/// All text lines from all blocks are joined with newlines.
/// No spatial layout reconstruction is attempted here — full block-level
/// layout analysis would require image dimension information that is not
/// directly present in the analyze response.
fn convert_response(resp: AzureAnalyzeResponse) -> OcrResult {
    let full_text = match resp.read_result {
        None => String::new(),
        Some(read_result) => {
            let lines: Vec<String> = read_result
                .blocks
                .iter()
                .flat_map(|block| block.lines.iter().map(|line| line.text.clone()))
                .collect();
            lines.join("\n")
        }
    };

    let markdown = if full_text.is_empty() {
        String::new()
    } else {
        format!("# Azure Computer Vision OCR Result\n\n{}", full_text)
    };

    OcrResult {
        text: full_text,
        markdown,
        blocks: vec![],
        metadata: OcrMetadata {
            provider: "azure_vision".to_string(),
            model: Some("image-analysis-4.0".to_string()),
            processing_time_ms: 0,
            image_size: None,
            languages: vec![],
            page_count: 1,
            current_page: 1,
        },
    }
}

// ---------------------------------------------------------------------------
// AzureVisionProvider
// ---------------------------------------------------------------------------

/// OCR provider backed by the Azure Computer Vision Image Analysis 4.0 API.
pub struct AzureVisionProvider {
    config: AzureVisionConfig,
    rate_limiter: Arc<RateLimiter>,
    cost_tracker: Arc<CostTracker>,
    client: Client,
}

impl AzureVisionProvider {
    /// Create a new provider from the given configuration.
    pub fn new(config: AzureVisionConfig) -> Self {
        let rps = config.rate_limit_rps;
        let rate_limiter = Arc::new(RateLimiter::new(rps));
        let cost_tracker = Arc::new(CostTracker::new(0.001));
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();
        Self {
            config,
            rate_limiter,
            cost_tracker,
            client,
        }
    }

    /// Return a snapshot of accumulated cost statistics.
    pub fn cost_stats(&self) -> CostStats {
        self.cost_tracker.stats()
    }

    /// Return the number of requests inside the current one-second window.
    pub fn requests_in_window(&self) -> u64 {
        self.rate_limiter.requests_in_window()
    }

    /// Build the full URL for the `imageanalysis:analyze` endpoint.
    fn build_url(&self) -> String {
        let base = self.config.endpoint.trim_end_matches('/');
        let mut url = format!(
            "{}/computervision/imageanalysis:analyze?api-version={}&features=read",
            base, self.config.api_version
        );
        if let Some(lang) = &self.config.language {
            url.push_str("&language=");
            url.push_str(lang);
        }
        url
    }

    /// Execute the HTTP request and return the parsed OCR result.
    async fn call_api(&self, image_data: &[u8]) -> Result<OcrResult> {
        // Enforce rate limit before issuing the request.
        self.rate_limiter.check_and_record()?;

        let url = self.build_url();

        let response = self
            .client
            .post(&url)
            .header("Ocp-Apim-Subscription-Key", &self.config.api_key)
            .header("Content-Type", "application/octet-stream")
            .body(image_data.to_vec())
            .send()
            .await
            .map_err(|e| VisionError::OcrEngine(format!("Azure HTTP request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let code = status.as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(VisionError::OcrEngine(format!(
                "Azure Vision API returned HTTP {}: {}",
                code, body
            )));
        }

        let resp: AzureAnalyzeResponse = response.json().await.map_err(|e| {
            VisionError::OcrEngine(format!("Failed to parse Azure response: {}", e))
        })?;

        if self.config.track_costs {
            self.cost_tracker.record_call(1);
        }

        Ok(convert_response(resp))
    }
}

#[async_trait]
impl super::VisionProvider for AzureVisionProvider {
    async fn process_image(&self, image_data: &[u8]) -> Result<OcrResult> {
        self.call_api(image_data).await
    }

    /// No local model to load — this is a cloud provider.
    async fn load_model(&self) -> Result<()> {
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "azure_vision"
    }

    fn capabilities(&self) -> super::ProviderCapabilities {
        super::ProviderCapabilities {
            table_detection: false,
            layout_analysis: true,
            handwriting: true,
            multi_language: true,
            gpu_acceleration: false,
            languages: vec![
                "en".to_string(),
                "es".to_string(),
                "fr".to_string(),
                "de".to_string(),
                "it".to_string(),
                "pt".to_string(),
                "zh".to_string(),
                "ja".to_string(),
                "ko".to_string(),
                "ar".to_string(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::VisionProvider;

    // -----------------------------------------------------------------------
    // Config tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_default() {
        let cfg = AzureVisionConfig::default();
        assert_eq!(cfg.api_version, "2024-02-01");
        assert_eq!(cfg.rate_limit_rps, 20);
        assert_eq!(cfg.timeout_secs, 30);
        assert!(cfg.track_costs);
        assert!(cfg.endpoint.is_empty());
        assert!(cfg.api_key.is_empty());
        assert!(cfg.language.is_none());
    }

    #[test]
    fn test_config_from_env_missing_endpoint() {
        // Ensure the env vars are absent for this test.
        std::env::remove_var("AZURE_VISION_ENDPOINT");
        std::env::remove_var("AZURE_VISION_KEY");
        let result = AzureVisionConfig::from_env();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("AZURE_VISION_ENDPOINT"));
    }

    #[test]
    fn test_config_from_env_missing_key() {
        std::env::set_var(
            "AZURE_VISION_ENDPOINT",
            "https://test.cognitiveservices.azure.com",
        );
        std::env::remove_var("AZURE_VISION_KEY");
        let result = AzureVisionConfig::from_env();
        // clean up regardless of outcome
        std::env::remove_var("AZURE_VISION_ENDPOINT");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("AZURE_VISION_KEY"));
    }

    // -----------------------------------------------------------------------
    // Provider creation / metadata
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_name() {
        let p = AzureVisionProvider::new(AzureVisionConfig::default());
        assert_eq!(p.provider_name(), "azure_vision");
    }

    #[test]
    fn test_capabilities() {
        let p = AzureVisionProvider::new(AzureVisionConfig::default());
        let caps = p.capabilities();
        assert!(caps.multi_language);
        assert!(caps.layout_analysis);
        assert!(caps.handwriting);
        assert!(!caps.gpu_acceleration);
        assert!(!caps.table_detection);
        assert!(!caps.languages.is_empty());
        assert!(caps.languages.contains(&"en".to_string()));
        assert!(caps.languages.contains(&"ja".to_string()));
    }

    #[test]
    fn test_provider_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AzureVisionProvider>();
    }

    // -----------------------------------------------------------------------
    // CostTracker tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_cost_tracker_new() {
        let tracker = CostTracker::new(0.001);
        let stats = tracker.stats();
        assert_eq!(stats.total_calls, 0);
        assert_eq!(stats.total_pages, 0);
        assert!((stats.total_cost_usd - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cost_tracker_accumulates() {
        let tracker = CostTracker::new(0.001);
        tracker.record_call(1);
        tracker.record_call(1);
        let stats = tracker.stats();
        assert_eq!(stats.total_calls, 2);
        assert_eq!(stats.total_pages, 2);
        assert!((stats.total_cost_usd - 0.002).abs() < 1e-9);
    }

    #[test]
    fn test_cost_tracker_multi_page() {
        let tracker = CostTracker::new(0.001);
        tracker.record_call(3);
        let stats = tracker.stats();
        assert_eq!(stats.total_calls, 1);
        assert_eq!(stats.total_pages, 3);
        assert!((stats.total_cost_usd - 0.001).abs() < 1e-9);
    }

    #[test]
    fn test_cost_stats_serialization() {
        let stats = CostStats {
            total_calls: 10,
            total_cost_usd: 0.01,
            total_pages: 10,
        };
        let json = serde_json::to_string(&stats).expect("serialization failed");
        let parsed: CostStats = serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(parsed.total_calls, 10);
        assert_eq!(parsed.total_pages, 10);
        assert!((parsed.total_cost_usd - 0.01).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // RateLimiter tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check_and_record().is_ok());
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(3);
        for _ in 0..3 {
            assert!(limiter.check_and_record().is_ok());
        }
        let result = limiter.check_and_record();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("rate limit") || msg.contains("Rate limit"));
    }

    #[test]
    fn test_rate_limiter_requests_in_window() {
        let limiter = RateLimiter::new(10);
        for _ in 0..4 {
            let _ = limiter.check_and_record();
        }
        assert_eq!(limiter.requests_in_window(), 4);
    }

    #[test]
    fn test_rate_limiter_oldest_request_age_empty() {
        let limiter = RateLimiter::new(10);
        assert!(limiter.oldest_request_age().is_none());
    }

    #[test]
    fn test_rate_limiter_oldest_request_age_after_record() {
        let limiter = RateLimiter::new(10);
        let _ = limiter.check_and_record();
        let age = limiter.oldest_request_age();
        assert!(age.is_some());
        // The oldest request was just made — its age must be very small.
        assert!(age.unwrap() < Duration::from_millis(500));
    }

    // -----------------------------------------------------------------------
    // URL builder tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_url_no_language() {
        let cfg = AzureVisionConfig {
            endpoint: "https://eastus.cognitiveservices.azure.com".to_string(),
            api_version: "2024-02-01".to_string(),
            language: None,
            ..Default::default()
        };
        let provider = AzureVisionProvider::new(cfg);
        let url = provider.build_url();
        assert!(url.contains("imageanalysis:analyze"));
        assert!(url.contains("features=read"));
        assert!(url.contains("api-version=2024-02-01"));
        assert!(!url.contains("language="));
    }

    #[test]
    fn test_build_url_with_language() {
        let cfg = AzureVisionConfig {
            endpoint: "https://westeurope.cognitiveservices.azure.com/".to_string(),
            api_version: "2024-02-01".to_string(),
            language: Some("ja".to_string()),
            ..Default::default()
        };
        let provider = AzureVisionProvider::new(cfg);
        let url = provider.build_url();
        // Trailing slash on endpoint must be stripped.
        assert!(!url.contains("//computervision"));
        assert!(url.contains("language=ja"));
    }

    // -----------------------------------------------------------------------
    // Response conversion tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_response_parsing_empty() {
        let resp = AzureAnalyzeResponse { read_result: None };
        let result = convert_response(resp);
        assert_eq!(result.text, "");
        assert_eq!(result.markdown, "");
        assert_eq!(result.metadata.provider, "azure_vision");
    }

    #[test]
    fn test_response_parsing_with_lines() {
        let resp = AzureAnalyzeResponse {
            read_result: Some(ReadResult {
                blocks: vec![TextBlockData {
                    lines: vec![
                        LineData {
                            text: "Hello".to_string(),
                            bounding_polygon: None,
                        },
                        LineData {
                            text: "World".to_string(),
                            bounding_polygon: None,
                        },
                    ],
                }],
            }),
        };
        let result = convert_response(resp);
        assert!(result.text.contains("Hello"));
        assert!(result.text.contains("World"));
        assert!(result.markdown.contains("Hello"));
        assert!(result.markdown.contains("Azure Computer Vision OCR Result"));
    }

    #[test]
    fn test_response_parsing_multi_block() {
        let resp = AzureAnalyzeResponse {
            read_result: Some(ReadResult {
                blocks: vec![
                    TextBlockData {
                        lines: vec![LineData {
                            text: "Block one".to_string(),
                            bounding_polygon: None,
                        }],
                    },
                    TextBlockData {
                        lines: vec![LineData {
                            text: "Block two".to_string(),
                            bounding_polygon: None,
                        }],
                    },
                ],
            }),
        };
        let result = convert_response(resp);
        assert!(result.text.contains("Block one"));
        assert!(result.text.contains("Block two"));
    }

    #[test]
    fn test_response_metadata_fields() {
        let resp = AzureAnalyzeResponse { read_result: None };
        let result = convert_response(resp);
        assert_eq!(result.metadata.provider, "azure_vision");
        assert_eq!(result.metadata.model.as_deref(), Some("image-analysis-4.0"));
        assert_eq!(result.metadata.page_count, 1);
        assert_eq!(result.metadata.current_page, 1);
    }

    // -----------------------------------------------------------------------
    // Integration test (requires real credentials — skipped by default)
    // -----------------------------------------------------------------------

    #[tokio::test]
    #[ignore]
    async fn test_azure_real_api() {
        let cfg = AzureVisionConfig::from_env()
            .expect("AZURE_VISION_KEY and AZURE_VISION_ENDPOINT must be set");
        let provider = AzureVisionProvider::new(cfg);

        // Minimal 1×1 white PNG.
        let png_bytes: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 207,
            192, 0, 0, 0, 2, 0, 1, 227, 33, 188, 51, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        let result = provider
            .process_image(png_bytes)
            .await
            .expect("Azure OCR call should succeed");

        // A 1×1 white image won't contain text, but the call itself must succeed.
        let _ = result.text;
    }
}
