//! AWS Textract OCR provider implementation.
//!
//! Integrates with the AWS Textract service to extract text from images and PDFs
//! using the `DetectDocumentText` and `AnalyzeDocument` APIs.
//!
//! ## Authentication
//!
//! Reads AWS credentials from environment variables:
//! * `AWS_ACCESS_KEY_ID`     (required)
//! * `AWS_SECRET_ACCESS_KEY` (required)
//! * `AWS_SESSION_TOKEN`     (optional — for temporary credentials)
//! * `AWS_TEXTRACT_REGION`   (required; fallback to `AWS_DEFAULT_REGION`)
//!
//! ## Textract endpoints
//!
//! * `DetectDocumentText`  — plain text/line extraction, posted to `X-Amz-Target: Textract.DetectDocumentText`
//! * `AnalyzeDocument`     — form/table extraction,   posted to `X-Amz-Target: Textract.AnalyzeDocument`
//!
//! Both use `POST https://textract.{region}.amazonaws.com/` with
//! `Content-Type: application/x-amz-json-1.1`.

use super::aws_sigv4::{sign_request, AwsCredentials};
use crate::errors::{Result, VisionError};
use crate::types::{OcrMetadata, OcrResult};
use async_trait::async_trait;
use base64::Engine;
use chrono::Utc;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Feature types for AnalyzeDocument
// ---------------------------------------------------------------------------

/// Feature type selector for `AnalyzeDocument`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyzeFeature {
    /// Extract key-value pairs from forms.
    Forms,
    /// Extract tables.
    Tables,
}

impl AnalyzeFeature {
    fn as_str(self) -> &'static str {
        match self {
            AnalyzeFeature::Forms => "FORMS",
            AnalyzeFeature::Tables => "TABLES",
        }
    }
}

// ---------------------------------------------------------------------------
// Textract REST response types
// ---------------------------------------------------------------------------

/// Top-level Textract response containing one or more `Block` objects.
#[derive(Debug, Deserialize)]
struct TextractResponse {
    #[serde(rename = "Blocks")]
    blocks: Option<Vec<TextractBlock>>,
}

/// A single Textract block (LINE, WORD, CELL, etc.).
#[derive(Debug, Deserialize)]
struct TextractBlock {
    #[serde(rename = "BlockType")]
    block_type: String,
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "Confidence")]
    #[allow(dead_code)]
    confidence: Option<f64>,
    #[serde(rename = "Geometry")]
    #[allow(dead_code)]
    geometry: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// TextractProvider
// ---------------------------------------------------------------------------

/// OCR provider backed by AWS Textract.
///
/// Supports two APIs:
/// * `DetectDocumentText` — available via `process_image` (implements `VisionProvider`)
/// * `AnalyzeDocument`    — available directly via `analyze_document`
///
/// Use `with_base_url` to point requests at a mock server for testing.
#[derive(Debug, Clone)]
pub struct TextractProvider {
    region: String,
    credentials: AwsCredentials,
    client: oxihttp::HttpsClient,
    /// Base URL seam; default is `"https://textract.{region}.amazonaws.com"`.
    base_url: String,
}

impl TextractProvider {
    /// Create a new Textract provider for the given region and credentials.
    pub fn new(region: String, credentials: AwsCredentials) -> Self {
        let base_url = format!("https://textract.{}.amazonaws.com", region);
        let client = oxihttp::Client::builder()
            .with_tls()
            .build_https()
            .expect("failed to build oxihttp HTTPS client for AWS Textract");
        Self {
            region,
            credentials,
            client,
            base_url,
        }
    }

    /// Override the base URL (e.g. to point at a WireMock server in tests).
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Construct the provider from environment variables.
    ///
    /// Reads `AWS_TEXTRACT_REGION` (fallback: `AWS_DEFAULT_REGION`),
    /// `AWS_ACCESS_KEY_ID`, and `AWS_SECRET_ACCESS_KEY`.
    pub fn from_env() -> Result<Self> {
        let region = std::env::var("AWS_TEXTRACT_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .map_err(|_| {
                VisionError::config("AWS_TEXTRACT_REGION or AWS_DEFAULT_REGION must be set")
            })?;
        let credentials = AwsCredentials::from_env().map_err(VisionError::config)?;
        Ok(Self::new(region, credentials))
    }

    /// Return the full endpoint URL (always ends with `/`).
    pub fn endpoint_url(&self) -> String {
        format!("{}/", self.base_url.trim_end_matches('/'))
    }

    /// Derive the `host` value used in SigV4 signing from `base_url`.
    fn signing_host(&self) -> String {
        let url = self.base_url.trim_end_matches('/');
        // Strip scheme if present.
        let stripped = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        // Strip any path component; keep only host[:port].
        stripped.split('/').next().unwrap_or(stripped).to_string()
    }

    /// Execute a signed POST to the Textract endpoint.
    ///
    /// # Arguments
    /// * `target`     — value for the `X-Amz-Target` header (e.g. `"Textract.DetectDocumentText"`)
    /// * `body_bytes` — raw JSON body bytes
    async fn post_textract(&self, target: &str, body_bytes: &[u8]) -> Result<serde_json::Value> {
        // Generate a fresh datetime stamp.
        let now = Utc::now();
        let datetime = now.format("%Y%m%dT%H%M%SZ").to_string();

        let host = self.signing_host();
        let signing_headers = sign_request(
            "POST",
            &host,
            "/",
            &self.region,
            "textract",
            body_bytes,
            &self.credentials,
            &datetime,
        );

        let mut request = self
            .client
            .post(&self.endpoint_url())?
            .header("Content-Type", "application/x-amz-json-1.1")?
            .header("X-Amz-Target", target)?
            .body(body_bytes.to_vec());

        for (name, value) in &signing_headers {
            request = request.header(name, value)?;
        }

        let response = request
            .send()
            .await
            .map_err(|e| VisionError::OcrEngine(format!("Textract HTTP request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let code = status.as_u16();
            let body = response.body_text().await.unwrap_or_default();
            return Err(VisionError::OcrEngine(format!(
                "Textract API returned HTTP {}: {}",
                code, body
            )));
        }

        response
            .body_json::<serde_json::Value>()
            .await
            .map_err(|e| {
                VisionError::OcrEngine(format!("Failed to parse Textract response: {}", e))
            })
    }

    /// Call `DetectDocumentText` and return the raw response value.
    async fn detect_document_text(&self, image_data: &[u8]) -> Result<TextractResponse> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(image_data);
        let body = serde_json::json!({
            "Document": {
                "Bytes": encoded
            }
        });
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| VisionError::OcrEngine(format!("Failed to serialize request: {}", e)))?;

        let value = self
            .post_textract("Textract.DetectDocumentText", &body_bytes)
            .await?;

        serde_json::from_value::<TextractResponse>(value).map_err(|e| {
            VisionError::OcrEngine(format!("Failed to deserialize Textract response: {}", e))
        })
    }

    /// Call `AnalyzeDocument` with the given feature types and return the raw response.
    pub async fn analyze_document(
        &self,
        image_data: &[u8],
        features: &[AnalyzeFeature],
    ) -> Result<serde_json::Value> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(image_data);
        let feature_strings: Vec<&str> = features.iter().map(|f| f.as_str()).collect();
        let body = serde_json::json!({
            "Document": {
                "Bytes": encoded
            },
            "FeatureTypes": feature_strings
        });
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| VisionError::OcrEngine(format!("Failed to serialize request: {}", e)))?;

        self.post_textract("Textract.AnalyzeDocument", &body_bytes)
            .await
    }

    /// Convert a `TextractResponse` to an `OcrResult`, collecting all LINE-type blocks.
    fn response_to_ocr_result(resp: TextractResponse) -> OcrResult {
        let lines: Vec<String> = resp
            .blocks
            .unwrap_or_default()
            .into_iter()
            .filter(|b| b.block_type == "LINE")
            .filter_map(|b| b.text)
            .collect();

        let full_text = lines.join("\n");
        let markdown = if full_text.is_empty() {
            String::new()
        } else {
            format!("# AWS Textract OCR Result\n\n{}", full_text)
        };

        OcrResult {
            text: full_text,
            markdown,
            blocks: vec![],
            metadata: OcrMetadata {
                provider: "aws-textract".to_string(),
                model: Some("textract-detect-document-text".to_string()),
                processing_time_ms: 0,
                image_size: None,
                languages: vec![],
                page_count: 1,
                current_page: 1,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// VisionProvider impl
// ---------------------------------------------------------------------------

#[async_trait]
impl super::VisionProvider for TextractProvider {
    async fn process_image(&self, image_data: &[u8]) -> Result<OcrResult> {
        let resp = self.detect_document_text(image_data).await?;
        Ok(Self::response_to_ocr_result(resp))
    }

    /// No local model to load — cloud provider.
    async fn load_model(&self) -> Result<()> {
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "aws-textract"
    }

    fn capabilities(&self) -> super::ProviderCapabilities {
        super::ProviderCapabilities {
            table_detection: true,
            layout_analysis: true,
            handwriting: true,
            multi_language: false, // Textract is English-primary
            gpu_acceleration: false,
            languages: vec!["en".to_string()],
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
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // -----------------------------------------------------------------------
    // Provider metadata tests (no network)
    // -----------------------------------------------------------------------

    #[test]
    fn test_textract_provider_name() {
        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider = TextractProvider::new("us-east-1".to_string(), creds);
        assert_eq!(provider.provider_name(), "aws-textract");
    }

    #[test]
    fn test_textract_endpoint_url() {
        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider = TextractProvider::new("us-east-1".to_string(), creds);
        let url = provider.endpoint_url();
        assert_eq!(url, "https://textract.us-east-1.amazonaws.com/");
    }

    #[test]
    fn test_textract_endpoint_url_west() {
        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider = TextractProvider::new("us-west-2".to_string(), creds);
        let url = provider.endpoint_url();
        assert_eq!(url, "https://textract.us-west-2.amazonaws.com/");
    }

    #[test]
    fn test_textract_with_base_url_seam() {
        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider = TextractProvider::new("us-east-1".to_string(), creds)
            .with_base_url("http://localhost:8080".to_string());
        let url = provider.endpoint_url();
        assert_eq!(url, "http://localhost:8080/");
    }

    #[test]
    fn test_textract_from_env_missing_creds() {
        std::env::remove_var("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        std::env::set_var("AWS_TEXTRACT_REGION", "us-east-1");

        let result = TextractProvider::from_env();

        std::env::remove_var("AWS_TEXTRACT_REGION");

        assert!(
            result.is_err(),
            "expected error when credentials are missing"
        );
    }

    #[test]
    fn test_textract_from_env_missing_region() {
        std::env::set_var("AWS_ACCESS_KEY_ID", "TEST_AKID");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "TEST_SECRET");
        std::env::remove_var("AWS_TEXTRACT_REGION");
        std::env::remove_var("AWS_DEFAULT_REGION");

        let result = TextractProvider::from_env();

        std::env::remove_var("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");

        assert!(result.is_err(), "expected error when region is missing");
    }

    #[test]
    fn test_textract_capabilities() {
        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider = TextractProvider::new("us-east-1".to_string(), creds);
        let caps = provider.capabilities();
        assert!(caps.table_detection);
        assert!(caps.layout_analysis);
        assert!(caps.handwriting);
        assert!(!caps.gpu_acceleration);
    }

    // -----------------------------------------------------------------------
    // Base64 encoding test (verify image bytes are properly encoded)
    // -----------------------------------------------------------------------

    #[test]
    fn test_textract_base64_encoding() {
        // Verify that the base64 encoding used in the request body is correct.
        // First 6 bytes of a PNG file: 0x89 'P' 'N' 'G' \r \n
        let test_bytes: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A];
        let encoded = base64::engine::general_purpose::STANDARD.encode(test_bytes);
        // Decode and verify round-trip.
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .expect("should decode cleanly");
        assert_eq!(decoded, test_bytes, "base64 round-trip failed");

        // Verify that the encoding uses the standard (padded) alphabet, not URL-safe.
        // 6 bytes → 8 base64 chars, no padding needed.
        assert_eq!(encoded, "iVBORw0K");
    }

    // -----------------------------------------------------------------------
    // Network tests against WireMock
    // -----------------------------------------------------------------------

    /// Build a realistic Textract DetectDocumentText response JSON.
    fn mock_detect_response(lines: &[&str]) -> serde_json::Value {
        let blocks: Vec<serde_json::Value> = lines
            .iter()
            .enumerate()
            .flat_map(|(i, line)| {
                let line_block = serde_json::json!({
                    "BlockType": "LINE",
                    "Text": line,
                    "Confidence": 99.5,
                    "Geometry": {
                        "BoundingBox": {
                            "Width": 0.5, "Height": 0.02,
                            "Left": 0.1, "Top": 0.05 + (i as f64 * 0.05)
                        }
                    },
                    "Id": format!("line-{}", i)
                });
                // Also add WORD blocks (they should be ignored by extract_text).
                let word_block = serde_json::json!({
                    "BlockType": "WORD",
                    "Text": line,
                    "Confidence": 99.9,
                    "Id": format!("word-{}", i)
                });
                vec![line_block, word_block]
            })
            .collect();

        serde_json::json!({ "Blocks": blocks })
    }

    #[tokio::test]
    async fn test_textract_extract_text_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("X-Amz-Target", "Textract.DetectDocumentText"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_detect_response(&[
                    "Hello World",
                    "This is a test",
                    "OCR line three",
                ])),
            )
            .mount(&mock_server)
            .await;

        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider =
            TextractProvider::new("us-east-1".to_string(), creds).with_base_url(mock_server.uri());

        let result = provider
            .process_image(b"fake-image-data")
            .await
            .expect("should succeed");

        assert!(result.text.contains("Hello World"), "text: {}", result.text);
        assert!(
            result.text.contains("This is a test"),
            "text: {}",
            result.text
        );
        assert!(
            result.text.contains("OCR line three"),
            "text: {}",
            result.text
        );
        assert_eq!(result.metadata.provider, "aws-textract");
        assert!(!result.text.is_empty());
    }

    #[tokio::test]
    async fn test_textract_line_blocks_only() {
        // WORD blocks must not appear in the joined text (only LINE blocks should).
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_detect_response(&["Only this line"])),
            )
            .mount(&mock_server)
            .await;

        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider =
            TextractProvider::new("us-east-1".to_string(), creds).with_base_url(mock_server.uri());

        let result = provider
            .process_image(b"img")
            .await
            .expect("should succeed");
        // The response has 1 LINE + 1 WORD block for the same text.
        // After joining LINEs only, we expect exactly one occurrence per line.
        let occurrences = result.text.matches("Only this line").count();
        assert_eq!(
            occurrences, 1,
            "expected 1 LINE occurrence, got {}",
            occurrences
        );
    }

    #[tokio::test]
    async fn test_textract_api_error_400() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(400).set_body_string(
                r#"{"__type":"InvalidParameterException","message":"Document is invalid"}"#,
            ))
            .mount(&mock_server)
            .await;

        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider =
            TextractProvider::new("us-east-1".to_string(), creds).with_base_url(mock_server.uri());

        let result = provider.process_image(b"bad-data").await;
        assert!(result.is_err(), "expected error for HTTP 400");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("400") || err_msg.contains("Textract"),
            "unexpected error message: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_textract_unauthorized_403() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(403).set_body_string(
                r#"{"__type":"AccessDeniedException","message":"User is not authorized"}"#,
            ))
            .mount(&mock_server)
            .await;

        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider =
            TextractProvider::new("us-east-1".to_string(), creds).with_base_url(mock_server.uri());

        let result = provider.process_image(b"img").await;
        assert!(result.is_err(), "expected error for HTTP 403");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("403") || err_msg.contains("Textract"),
            "unexpected error message: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_textract_empty_blocks_list() {
        // Textract response with empty Blocks array.
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"Blocks": []})),
            )
            .mount(&mock_server)
            .await;

        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider =
            TextractProvider::new("us-east-1".to_string(), creds).with_base_url(mock_server.uri());

        let result = provider
            .process_image(b"blank-image")
            .await
            .expect("should succeed");
        assert_eq!(result.text, "", "expected empty text for empty blocks");
        assert_eq!(
            result.markdown, "",
            "expected empty markdown for empty blocks"
        );
    }

    #[tokio::test]
    async fn test_textract_request_contains_base64_body() {
        // Verify the request body sent to the mock server contains the base64-encoded image.
        use wiremock::matchers::body_partial_json;

        let test_image: &[u8] = b"FAKE_IMAGE_PAYLOAD";
        let expected_b64 = base64::engine::general_purpose::STANDARD.encode(test_image);

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(serde_json::json!({
                "Document": { "Bytes": expected_b64 }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"Blocks": []})),
            )
            .mount(&mock_server)
            .await;

        let creds = AwsCredentials::new("AKID", "SECRET");
        let provider =
            TextractProvider::new("us-east-1".to_string(), creds).with_base_url(mock_server.uri());

        let result = provider.process_image(test_image).await;
        // If the body_partial_json matcher didn't match, wiremock returns 404.
        assert!(
            result.is_ok(),
            "request body must contain base64 image; result: {:?}",
            result
        );
    }
}
