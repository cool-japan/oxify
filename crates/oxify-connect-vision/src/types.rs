//! Core types for OCR/Vision processing results.

use serde::{Deserialize, Serialize};

/// Result of OCR/Vision processing on an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    /// Extracted plain text (all blocks concatenated).
    pub text: String,

    /// Layout-preserved Markdown representation.
    /// Tables, headers, and lists are formatted appropriately.
    pub markdown: String,

    /// Individual text blocks with position and metadata.
    pub blocks: Vec<TextBlock>,

    /// Processing metadata.
    pub metadata: OcrMetadata,
}

impl OcrResult {
    /// Create a new empty OCR result.
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            markdown: String::new(),
            blocks: Vec::new(),
            metadata: OcrMetadata::default(),
        }
    }

    /// Create a simple OCR result with just text.
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            markdown: text.clone(),
            text,
            blocks: Vec::new(),
            metadata: OcrMetadata::default(),
        }
    }
}

/// A single text block extracted from an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    /// Extracted text content.
    pub text: String,

    /// Bounding box coordinates: [x1, y1, x2, y2].
    /// Coordinates are normalized to [0, 1] range relative to image dimensions.
    pub bbox: [f32; 4],

    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,

    /// Semantic role of this block.
    pub role: BlockRole,

    /// Reading order index (0-based).
    pub order: usize,
}

impl TextBlock {
    /// Create a new text block with default values.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bbox: [0.0, 0.0, 1.0, 1.0],
            confidence: 1.0,
            role: BlockRole::Text,
            order: 0,
        }
    }

    /// Set the bounding box.
    pub fn with_bbox(mut self, bbox: [f32; 4]) -> Self {
        self.bbox = bbox;
        self
    }

    /// Set the confidence score.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    /// Set the block role.
    pub fn with_role(mut self, role: BlockRole) -> Self {
        self.role = role;
        self
    }

    /// Set the reading order.
    pub fn with_order(mut self, order: usize) -> Self {
        self.order = order;
        self
    }
}

/// Semantic role of a text block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockRole {
    /// Document title.
    Title,
    /// Section header.
    Header,
    /// Regular text paragraph.
    #[default]
    Text,
    /// Table content.
    Table,
    /// List item.
    List,
    /// Image caption.
    Caption,
    /// Footer content.
    Footer,
    /// Page number.
    PageNumber,
    /// Code block.
    Code,
    /// Quote or citation.
    Quote,
    /// Other/unknown role.
    Other,
}

impl BlockRole {
    /// Get the Markdown prefix for this role.
    pub fn markdown_prefix(&self) -> &'static str {
        match self {
            BlockRole::Title => "# ",
            BlockRole::Header => "## ",
            BlockRole::List => "- ",
            BlockRole::Quote => "> ",
            BlockRole::Code => "```\n",
            _ => "",
        }
    }

    /// Get the Markdown suffix for this role.
    pub fn markdown_suffix(&self) -> &'static str {
        match self {
            BlockRole::Code => "\n```",
            _ => "",
        }
    }
}

/// Metadata about the OCR processing.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OcrMetadata {
    /// Provider used for OCR.
    pub provider: String,

    /// Model name/version.
    pub model: Option<String>,

    /// Processing time in milliseconds.
    pub processing_time_ms: u64,

    /// Image dimensions (width, height).
    pub image_size: Option<(u32, u32)>,

    /// Detected language(s).
    pub languages: Vec<String>,

    /// Number of pages (for multi-page documents).
    pub page_count: u32,

    /// Current page number (1-indexed).
    pub current_page: u32,
}

/// Input image data for OCR processing.
#[derive(Debug, Clone)]
pub enum ImageInput {
    /// Raw bytes of the image.
    Bytes(Vec<u8>),

    /// Base64-encoded image data.
    Base64(String),

    /// File path to the image.
    Path(String),

    /// URL to fetch the image from.
    Url(String),
}

impl ImageInput {
    /// Convert to raw bytes.
    pub async fn to_bytes(&self) -> Result<Vec<u8>, VisionInputError> {
        match self {
            ImageInput::Bytes(bytes) => Ok(bytes.clone()),
            ImageInput::Base64(encoded) => {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|e| VisionInputError::Base64Decode(e.to_string()))
            }
            ImageInput::Path(path) => tokio::fs::read(path)
                .await
                .map_err(|e| VisionInputError::FileRead(e.to_string())),
            ImageInput::Url(_url) => {
                // URL fetching would require reqwest dependency
                Err(VisionInputError::UrlNotSupported)
            }
        }
    }
}

/// Error type for image input processing.
#[derive(Debug, thiserror::Error)]
pub enum VisionInputError {
    #[error("Failed to decode base64: {0}")]
    Base64Decode(String),

    #[error("Failed to read file: {0}")]
    FileRead(String),

    #[error("URL input is not supported in this build")]
    UrlNotSupported,
}

/// Output format for OCR results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Plain text output.
    Text,
    /// Markdown-formatted output (preserves layout).
    #[default]
    Markdown,
    /// Structured JSON output.
    Json,
    /// All formats (for maximum flexibility).
    All,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocr_result_from_text() {
        let result = OcrResult::from_text("Hello, World!");
        assert_eq!(result.text, "Hello, World!");
        assert_eq!(result.markdown, "Hello, World!");
        assert!(result.blocks.is_empty());
    }

    #[test]
    fn test_text_block_builder() {
        let block = TextBlock::new("Test")
            .with_bbox([0.1, 0.2, 0.3, 0.4])
            .with_confidence(0.95)
            .with_role(BlockRole::Header)
            .with_order(1);

        assert_eq!(block.text, "Test");
        assert_eq!(block.bbox, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(block.confidence, 0.95);
        assert_eq!(block.role, BlockRole::Header);
        assert_eq!(block.order, 1);
    }

    #[test]
    fn test_block_role_markdown() {
        assert_eq!(BlockRole::Title.markdown_prefix(), "# ");
        assert_eq!(BlockRole::Header.markdown_prefix(), "## ");
        assert_eq!(BlockRole::List.markdown_prefix(), "- ");
        assert_eq!(BlockRole::Text.markdown_prefix(), "");
    }

    #[test]
    fn test_ocr_result_serialization() {
        let result = OcrResult::from_text("Test");
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: OcrResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.text, result.text);
    }
}
