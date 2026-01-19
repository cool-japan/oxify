//! Multi-page PDF document processing.
//!
//! This module provides functionality to process multi-page PDF documents,
//! extracting text and structure from each page with support for page ordering,
//! table of contents generation, and cross-page table handling.

use crate::types::OcrResult;
use crate::VisionProvider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Represents a processed page from a PDF document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfPage {
    /// Page number (1-indexed)
    pub page_number: usize,
    /// OCR result for this page
    pub ocr_result: OcrResult,
    /// Page dimensions (width, height) in points
    pub dimensions: (f32, f32),
    /// Page rotation in degrees
    pub rotation: i32,
}

/// Result of processing a multi-page PDF document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfDocumentResult {
    /// All processed pages in order
    pub pages: Vec<PdfPage>,
    /// Total number of pages
    pub total_pages: usize,
    /// Document metadata
    pub metadata: PdfMetadata,
    /// Combined text from all pages
    pub full_text: String,
}

/// PDF document metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PdfMetadata {
    /// Document title
    pub title: Option<String>,
    /// Document author
    pub author: Option<String>,
    /// Document subject
    pub subject: Option<String>,
    /// Creation date
    pub creation_date: Option<String>,
    /// Modification date
    pub modification_date: Option<String>,
    /// Producer/creator software
    pub producer: Option<String>,
}

/// Configuration for PDF processing.
#[derive(Debug, Clone)]
pub struct PdfProcessingConfig {
    /// Maximum number of pages to process (None for all pages)
    pub max_pages: Option<usize>,
    /// Page range to process (start, end) - 1-indexed
    pub page_range: Option<(usize, usize)>,
    /// Enable table of contents generation
    pub generate_toc: bool,
    /// Combine text from all pages
    pub combine_text: bool,
    /// DPI for rendering PDF pages to images
    pub render_dpi: u32,
}

impl Default for PdfProcessingConfig {
    fn default() -> Self {
        Self {
            max_pages: None,
            page_range: None,
            generate_toc: true,
            combine_text: true,
            render_dpi: 300,
        }
    }
}

/// PDF document processor.
pub struct PdfProcessor {
    config: PdfProcessingConfig,
}

impl PdfProcessor {
    /// Create a new PDF processor with default configuration.
    pub fn new() -> Self {
        Self {
            config: PdfProcessingConfig::default(),
        }
    }

    /// Create a new PDF processor with custom configuration.
    pub fn with_config(config: PdfProcessingConfig) -> Self {
        Self { config }
    }

    /// Process a PDF document from raw bytes.
    ///
    /// Note: This is a stub implementation. Real PDF processing would require
    /// a PDF library like pdf-rs or pdfium to extract pages and render them as images.
    pub async fn process_pdf(
        &self,
        _pdf_data: &[u8],
        _provider: Arc<dyn VisionProvider>,
    ) -> crate::Result<PdfDocumentResult> {
        // Placeholder implementation
        // In a real implementation, this would:
        // 1. Parse the PDF using a PDF library
        // 2. Extract metadata
        // 3. Render each page to an image at specified DPI
        // 4. Process each page image with the OCR provider
        // 5. Combine results

        let metadata = PdfMetadata::default();

        let pages = vec![PdfPage {
            page_number: 1,
            ocr_result: OcrResult::from_text("[PDF processing requires pdf library]"),
            dimensions: (612.0, 792.0), // US Letter size
            rotation: 0,
        }];

        let full_text = if self.config.combine_text {
            pages
                .iter()
                .map(|p| p.ocr_result.text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        } else {
            String::new()
        };

        Ok(PdfDocumentResult {
            total_pages: pages.len(),
            pages,
            metadata,
            full_text,
        })
    }

    /// Process a specific page range from a PDF.
    pub async fn process_page_range(
        &self,
        pdf_data: &[u8],
        provider: Arc<dyn VisionProvider>,
        start_page: usize,
        end_page: usize,
    ) -> crate::Result<PdfDocumentResult> {
        let mut config = self.config.clone();
        config.page_range = Some((start_page, end_page));

        let processor = Self::with_config(config);
        processor.process_pdf(pdf_data, provider).await
    }

    /// Extract metadata from a PDF document.
    pub fn extract_metadata(&self, _pdf_data: &[u8]) -> crate::Result<PdfMetadata> {
        // Placeholder implementation
        Ok(PdfMetadata::default())
    }
}

impl Default for PdfProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfDocumentResult {
    /// Get a specific page by number (1-indexed).
    pub fn get_page(&self, page_number: usize) -> Option<&PdfPage> {
        self.pages.iter().find(|p| p.page_number == page_number)
    }

    /// Generate a table of contents from page headings.
    pub fn generate_toc(&self) -> Vec<TocEntry> {
        let mut toc = Vec::new();

        for page in &self.pages {
            // Find heading blocks in the page
            for block in &page.ocr_result.blocks {
                if matches!(block.role, crate::types::BlockRole::Header) {
                    toc.push(TocEntry {
                        title: block.text.clone(),
                        page_number: page.page_number,
                        level: 1, // Could be enhanced with heading level detection
                    });
                }
            }
        }

        toc
    }

    /// Export to a single markdown document.
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        // Add metadata if available
        if let Some(ref title) = self.metadata.title {
            output.push_str(&format!("# {}\n\n", title));
        }

        // Add pages
        for page in &self.pages {
            output.push_str(&format!("## Page {}\n\n", page.page_number));
            output.push_str(&page.ocr_result.markdown);
            output.push_str("\n\n");
        }

        output
    }

    /// Export to HTML format.
    pub fn to_html(&self) -> String {
        let mut output = String::from("<!DOCTYPE html>\n<html>\n<head>\n");

        if let Some(ref title) = self.metadata.title {
            output.push_str(&format!("  <title>{}</title>\n", title));
        }

        output.push_str("</head>\n<body>\n");

        for page in &self.pages {
            output.push_str(&format!(
                "  <div class=\"page\" data-page=\"{}\">\n",
                page.page_number
            ));
            output.push_str(&format!("    <h2>Page {}</h2>\n", page.page_number));
            output.push_str("    <div class=\"content\">\n");
            output.push_str(&format!("      {}\n", page.ocr_result.text));
            output.push_str("    </div>\n");
            output.push_str("  </div>\n");
        }

        output.push_str("</body>\n</html>");
        output
    }

    /// Search for text across all pages.
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();

        for page in &self.pages {
            if page
                .ocr_result
                .text
                .to_lowercase()
                .contains(&query.to_lowercase())
            {
                results.push(SearchResult {
                    page_number: page.page_number,
                    context: self.extract_context(&page.ocr_result.text, query, 50),
                });
            }
        }

        results
    }

    /// Extract context around a search term.
    fn extract_context(&self, text: &str, query: &str, context_chars: usize) -> String {
        let lower_text = text.to_lowercase();
        let lower_query = query.to_lowercase();

        if let Some(pos) = lower_text.find(&lower_query) {
            let start = pos.saturating_sub(context_chars);
            let end = (pos + query.len() + context_chars).min(text.len());

            let mut context = text[start..end].to_string();

            if start > 0 {
                context = format!("...{}", context);
            }
            if end < text.len() {
                context.push_str("...");
            }

            context
        } else {
            String::new()
        }
    }
}

/// Table of contents entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    /// Entry title
    pub title: String,
    /// Page number where this entry appears
    pub page_number: usize,
    /// Heading level (1-6)
    pub level: usize,
}

/// Search result for text search in PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Page number where match was found
    pub page_number: usize,
    /// Context around the match
    pub context: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_processing_config_default() {
        let config = PdfProcessingConfig::default();
        assert_eq!(config.render_dpi, 300);
        assert!(config.generate_toc);
        assert!(config.combine_text);
    }

    #[test]
    fn test_pdf_page_creation() {
        let page = PdfPage {
            page_number: 1,
            ocr_result: OcrResult::from_text("Test page"),
            dimensions: (612.0, 792.0),
            rotation: 0,
        };

        assert_eq!(page.page_number, 1);
        assert_eq!(page.ocr_result.text, "Test page");
    }

    #[test]
    fn test_pdf_document_get_page() {
        let result = PdfDocumentResult {
            pages: vec![
                PdfPage {
                    page_number: 1,
                    ocr_result: OcrResult::from_text("Page 1"),
                    dimensions: (612.0, 792.0),
                    rotation: 0,
                },
                PdfPage {
                    page_number: 2,
                    ocr_result: OcrResult::from_text("Page 2"),
                    dimensions: (612.0, 792.0),
                    rotation: 0,
                },
            ],
            total_pages: 2,
            metadata: PdfMetadata::default(),
            full_text: String::new(),
        };

        let page1 = result.get_page(1);
        assert!(page1.is_some());
        assert_eq!(page1.unwrap().ocr_result.text, "Page 1");

        let page3 = result.get_page(3);
        assert!(page3.is_none());
    }

    #[test]
    fn test_pdf_document_search() {
        let result = PdfDocumentResult {
            pages: vec![
                PdfPage {
                    page_number: 1,
                    ocr_result: OcrResult::from_text("Hello world from page 1"),
                    dimensions: (612.0, 792.0),
                    rotation: 0,
                },
                PdfPage {
                    page_number: 2,
                    ocr_result: OcrResult::from_text("Different content on page 2"),
                    dimensions: (612.0, 792.0),
                    rotation: 0,
                },
            ],
            total_pages: 2,
            metadata: PdfMetadata::default(),
            full_text: String::new(),
        };

        let results = result.search("world");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].page_number, 1);
    }

    #[test]
    fn test_toc_entry_creation() {
        let entry = TocEntry {
            title: "Chapter 1".to_string(),
            page_number: 5,
            level: 1,
        };

        assert_eq!(entry.title, "Chapter 1");
        assert_eq!(entry.page_number, 5);
        assert_eq!(entry.level, 1);
    }

    #[test]
    fn test_pdf_metadata() {
        let metadata = PdfMetadata {
            title: Some("Test Document".to_string()),
            author: Some("Test Author".to_string()),
            subject: None,
            creation_date: None,
            modification_date: None,
            producer: Some("Test Producer".to_string()),
        };

        assert_eq!(metadata.title.unwrap(), "Test Document");
        assert_eq!(metadata.author.unwrap(), "Test Author");
    }
}
