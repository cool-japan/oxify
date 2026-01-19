//! Mock vision provider for testing.

use async_trait::async_trait;

use crate::errors::Result;
use crate::types::{BlockRole, OcrMetadata, OcrResult, TextBlock};

use super::{ProviderCapabilities, VisionProvider};

/// Mock vision provider that returns predefined results.
/// Useful for testing and development.
#[derive(Debug, Clone)]
pub struct MockVisionProvider {
    /// Predefined response text.
    response_text: String,
    /// Simulated processing delay in milliseconds.
    delay_ms: u64,
    /// Whether to simulate layout analysis.
    simulate_layout: bool,
}

impl Default for MockVisionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockVisionProvider {
    /// Create a new mock provider with default settings.
    pub fn new() -> Self {
        Self {
            response_text:
                "This is mock OCR output.\n\nLine 2 of the document.\n\nLine 3 with more text."
                    .to_string(),
            delay_ms: 0,
            simulate_layout: true,
        }
    }

    /// Create a mock provider with custom response text.
    pub fn with_response(text: impl Into<String>) -> Self {
        Self {
            response_text: text.into(),
            delay_ms: 0,
            simulate_layout: true,
        }
    }

    /// Set a simulated delay.
    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Enable/disable layout simulation.
    pub fn with_layout(mut self, simulate: bool) -> Self {
        self.simulate_layout = simulate;
        self
    }

    /// Generate mock text blocks from the response text.
    fn generate_blocks(&self) -> Vec<TextBlock> {
        let lines: Vec<&str> = self.response_text.lines().collect();
        let total_lines = lines.len().max(1) as f32;

        lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !line.is_empty())
            .map(|(i, line)| {
                let y_start = i as f32 / total_lines;
                let y_end = (i + 1) as f32 / total_lines;

                let role = if i == 0 {
                    BlockRole::Title
                } else if line.starts_with('-') || line.starts_with('•') {
                    BlockRole::List
                } else {
                    BlockRole::Text
                };

                TextBlock::new(*line)
                    .with_bbox([0.05, y_start, 0.95, y_end])
                    .with_confidence(0.95)
                    .with_role(role)
                    .with_order(i)
            })
            .collect()
    }

    /// Generate markdown from the response text.
    fn generate_markdown(&self, blocks: &[TextBlock]) -> String {
        blocks
            .iter()
            .map(|block| {
                let prefix = block.role.markdown_prefix();
                let suffix = block.role.markdown_suffix();
                format!("{}{}{}", prefix, block.text, suffix)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[async_trait]
impl VisionProvider for MockVisionProvider {
    async fn process_image(&self, image_data: &[u8]) -> Result<OcrResult> {
        // Simulate processing delay
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }

        let blocks = if self.simulate_layout {
            self.generate_blocks()
        } else {
            vec![]
        };

        let markdown = if self.simulate_layout {
            self.generate_markdown(&blocks)
        } else {
            self.response_text.clone()
        };

        // Try to get image dimensions
        let image_size = image::load_from_memory(image_data)
            .ok()
            .map(|img| (img.width(), img.height()));

        Ok(OcrResult {
            text: self.response_text.clone(),
            markdown,
            blocks,
            metadata: OcrMetadata {
                provider: "mock".to_string(),
                model: Some("mock-v1".to_string()),
                processing_time_ms: self.delay_ms,
                image_size,
                languages: vec!["en".to_string()],
                page_count: 1,
                current_page: 1,
            },
        })
    }

    async fn load_model(&self) -> Result<()> {
        // No-op for mock provider
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            table_detection: true,
            layout_analysis: true,
            handwriting: false,
            multi_language: false,
            gpu_acceleration: false,
            languages: vec!["en".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_basic() {
        let provider = MockVisionProvider::new();
        let result = provider.process_image(b"fake image data").await.unwrap();

        assert!(!result.text.is_empty());
        assert_eq!(result.metadata.provider, "mock");
    }

    #[tokio::test]
    async fn test_mock_provider_custom_response() {
        let provider = MockVisionProvider::with_response("Custom response");
        let result = provider.process_image(b"fake").await.unwrap();

        assert_eq!(result.text, "Custom response");
    }

    #[tokio::test]
    async fn test_mock_provider_layout() {
        let provider = MockVisionProvider::new();
        let result = provider.process_image(b"fake").await.unwrap();

        assert!(!result.blocks.is_empty());
        assert_eq!(result.blocks[0].role, BlockRole::Title);
    }

    #[tokio::test]
    async fn test_mock_provider_no_layout() {
        let provider = MockVisionProvider::new().with_layout(false);
        let result = provider.process_image(b"fake").await.unwrap();

        assert!(result.blocks.is_empty());
    }

    #[test]
    fn test_provider_name() {
        let provider = MockVisionProvider::new();
        assert_eq!(provider.provider_name(), "mock");
    }

    #[test]
    fn test_capabilities() {
        let provider = MockVisionProvider::new();
        let caps = provider.capabilities();
        assert!(caps.layout_analysis);
        assert!(!caps.gpu_acceleration);
    }
}
