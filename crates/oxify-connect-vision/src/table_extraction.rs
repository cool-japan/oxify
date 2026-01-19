//! Table extraction and structure detection.
//!
//! This module provides functionality to detect and extract tables from images,
//! including cell-level extraction, structure preservation, and export to various formats.

use crate::types::{OcrResult, TextBlock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a detected table in an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Bounding box of the entire table [x, y, width, height]
    pub bbox: [f32; 4],
    /// Number of rows in the table
    pub rows: usize,
    /// Number of columns in the table
    pub cols: usize,
    /// Table cells organized by position
    pub cells: Vec<TableCell>,
    /// Confidence score for table detection
    pub confidence: f32,
}

/// Represents a single cell in a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableCell {
    /// Row index (0-based)
    pub row: usize,
    /// Column index (0-based)
    pub col: usize,
    /// Row span (for merged cells)
    pub row_span: usize,
    /// Column span (for merged cells)
    pub col_span: usize,
    /// Cell text content
    pub text: String,
    /// Bounding box [x, y, width, height]
    pub bbox: [f32; 4],
    /// Confidence score
    pub confidence: f32,
    /// Whether this is a header cell
    pub is_header: bool,
}

impl TableCell {
    /// Create a new table cell.
    pub fn new(row: usize, col: usize, text: String, bbox: [f32; 4]) -> Self {
        Self {
            row,
            col,
            row_span: 1,
            col_span: 1,
            text,
            bbox,
            confidence: 1.0,
            is_header: false,
        }
    }

    /// Mark cell as a header.
    pub fn with_header(mut self, is_header: bool) -> Self {
        self.is_header = is_header;
        self
    }

    /// Set cell confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
}

/// Table extraction configuration.
#[derive(Debug, Clone)]
pub struct TableExtractionConfig {
    /// Minimum confidence threshold for table detection
    pub min_confidence: f32,
    /// Minimum number of rows to consider as a table
    pub min_rows: usize,
    /// Minimum number of columns to consider as a table
    pub min_cols: usize,
    /// Enable header row detection
    pub detect_headers: bool,
    /// Cell padding tolerance (pixels)
    pub cell_padding: f32,
}

impl Default for TableExtractionConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.7,
            min_rows: 2,
            min_cols: 2,
            detect_headers: true,
            cell_padding: 5.0,
        }
    }
}

/// Table extractor for detecting and extracting tables from OCR results.
pub struct TableExtractor {
    config: TableExtractionConfig,
}

impl TableExtractor {
    /// Create a new table extractor with default configuration.
    pub fn new() -> Self {
        Self {
            config: TableExtractionConfig::default(),
        }
    }

    /// Create a new table extractor with custom configuration.
    pub fn with_config(config: TableExtractionConfig) -> Self {
        Self { config }
    }

    /// Extract tables from OCR result.
    pub fn extract_tables(&self, ocr_result: &OcrResult) -> Vec<Table> {
        let mut tables = Vec::new();

        // Group text blocks into potential table regions
        let table_candidates = self.detect_table_regions(&ocr_result.blocks);

        for candidate in table_candidates {
            if let Some(table) = self.build_table(candidate) {
                if table.rows >= self.config.min_rows && table.cols >= self.config.min_cols {
                    tables.push(table);
                }
            }
        }

        tables
    }

    /// Detect potential table regions from text blocks.
    fn detect_table_regions<'a>(&self, blocks: &'a [TextBlock]) -> Vec<Vec<&'a TextBlock>> {
        let mut regions = Vec::new();

        // Simple heuristic: group blocks with similar y-coordinates (rows)
        // and detect regular column patterns

        // Sort blocks by y-coordinate
        let mut sorted_blocks: Vec<&TextBlock> = blocks.iter().collect();
        sorted_blocks.sort_by(|a, b| {
            a.bbox[1]
                .partial_cmp(&b.bbox[1])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Group into rows based on y-coordinate similarity
        let mut rows: Vec<Vec<&TextBlock>> = Vec::new();
        let mut current_row: Vec<&TextBlock> = Vec::new();
        let mut current_y = 0.0;

        for block in sorted_blocks {
            let block_y = block.bbox[1];

            if current_row.is_empty() {
                current_y = block_y;
                current_row.push(block);
            } else if (block_y - current_y).abs() < self.config.cell_padding * 2.0 {
                // Same row
                current_row.push(block);
            } else {
                // New row
                if !current_row.is_empty() {
                    rows.push(current_row.clone());
                }
                current_row.clear();
                current_row.push(block);
                current_y = block_y;
            }
        }
        if !current_row.is_empty() {
            rows.push(current_row);
        }

        // Detect table-like patterns: consecutive rows with similar column counts
        let mut table_rows: Vec<Vec<&TextBlock>> = Vec::new();

        for row in rows {
            // Sort row blocks by x-coordinate
            let mut sorted_row = row.clone();
            sorted_row.sort_by(|a, b| {
                a.bbox[0]
                    .partial_cmp(&b.bbox[0])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            if sorted_row.len() >= self.config.min_cols {
                table_rows.push(sorted_row);
            } else if !table_rows.is_empty() {
                // End of potential table
                if table_rows.len() >= self.config.min_rows {
                    regions.push(table_rows.iter().flatten().copied().collect());
                }
                table_rows.clear();
            }
        }

        if !table_rows.is_empty() && table_rows.len() >= self.config.min_rows {
            regions.push(table_rows.iter().flatten().copied().collect());
        }

        regions
    }

    /// Build a table structure from a group of text blocks.
    fn build_table(&self, blocks: Vec<&TextBlock>) -> Option<Table> {
        if blocks.is_empty() {
            return None;
        }

        // Calculate table bounding box
        let min_x = blocks
            .iter()
            .map(|b| b.bbox[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = blocks
            .iter()
            .map(|b| b.bbox[1])
            .fold(f32::INFINITY, f32::min);
        let max_x = blocks
            .iter()
            .map(|b| b.bbox[0] + b.bbox[2])
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = blocks
            .iter()
            .map(|b| b.bbox[1] + b.bbox[3])
            .fold(f32::NEG_INFINITY, f32::max);

        let bbox = [min_x, min_y, max_x - min_x, max_y - min_y];

        // Detect row and column positions
        let row_positions = self.detect_row_positions(&blocks);
        let col_positions = self.detect_column_positions(&blocks);

        let rows = row_positions.len();
        let cols = col_positions.len();

        if rows < self.config.min_rows || cols < self.config.min_cols {
            return None;
        }

        // Build cells
        let mut cells = Vec::new();

        for block in &blocks {
            let row = self.find_row_index(&row_positions, block.bbox[1]);
            let col = self.find_col_index(&col_positions, block.bbox[0]);

            if let (Some(r), Some(c)) = (row, col) {
                let is_header = self.config.detect_headers && r == 0;
                let cell = TableCell::new(r, c, block.text.clone(), block.bbox)
                    .with_header(is_header)
                    .with_confidence(block.confidence);

                cells.push(cell);
            }
        }

        // Calculate average confidence
        let avg_confidence = if !cells.is_empty() {
            cells.iter().map(|c| c.confidence).sum::<f32>() / cells.len() as f32
        } else {
            0.0
        };

        Some(Table {
            bbox,
            rows,
            cols,
            cells,
            confidence: avg_confidence,
        })
    }

    /// Detect row positions from text blocks.
    fn detect_row_positions(&self, blocks: &[&TextBlock]) -> Vec<f32> {
        let mut y_coords: Vec<f32> = blocks.iter().map(|b| b.bbox[1]).collect();
        y_coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mut positions = Vec::new();
        let mut last_y = f32::NEG_INFINITY;

        for &y in &y_coords {
            if (y - last_y).abs() > self.config.cell_padding * 2.0 {
                positions.push(y);
                last_y = y;
            }
        }

        positions
    }

    /// Detect column positions from text blocks.
    fn detect_column_positions(&self, blocks: &[&TextBlock]) -> Vec<f32> {
        let mut x_coords: Vec<f32> = blocks.iter().map(|b| b.bbox[0]).collect();
        x_coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mut positions = Vec::new();
        let mut last_x = f32::NEG_INFINITY;

        for &x in &x_coords {
            if (x - last_x).abs() > self.config.cell_padding * 2.0 {
                positions.push(x);
                last_x = x;
            }
        }

        positions
    }

    /// Find row index for a y-coordinate.
    fn find_row_index(&self, row_positions: &[f32], y: f32) -> Option<usize> {
        for (idx, &pos) in row_positions.iter().enumerate() {
            if (y - pos).abs() < self.config.cell_padding * 3.0 {
                return Some(idx);
            }
        }
        None
    }

    /// Find column index for an x-coordinate.
    fn find_col_index(&self, col_positions: &[f32], x: f32) -> Option<usize> {
        for (idx, &pos) in col_positions.iter().enumerate() {
            if (x - pos).abs() < self.config.cell_padding * 3.0 {
                return Some(idx);
            }
        }
        None
    }
}

impl Default for TableExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Table {
    /// Export table to CSV format.
    pub fn to_csv(&self) -> String {
        let mut output = String::new();

        // Create a grid to hold cell contents
        let mut grid: HashMap<(usize, usize), String> = HashMap::new();

        for cell in &self.cells {
            grid.insert((cell.row, cell.col), cell.text.clone());
        }

        // Generate CSV rows
        for row in 0..self.rows {
            let mut row_data = Vec::new();
            for col in 0..self.cols {
                let cell_text = grid.get(&(row, col)).cloned().unwrap_or_default();
                // Escape quotes and commas
                let escaped = if cell_text.contains(',') || cell_text.contains('"') {
                    format!("\"{}\"", cell_text.replace('"', "\"\""))
                } else {
                    cell_text
                };
                row_data.push(escaped);
            }
            output.push_str(&row_data.join(","));
            output.push('\n');
        }

        output
    }

    /// Export table to Markdown format.
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        // Create a grid to hold cell contents
        let mut grid: HashMap<(usize, usize), String> = HashMap::new();

        for cell in &self.cells {
            grid.insert((cell.row, cell.col), cell.text.clone());
        }

        // Generate markdown table
        for row in 0..self.rows {
            output.push('|');
            for col in 0..self.cols {
                let cell_text = grid.get(&(row, col)).cloned().unwrap_or_default();
                output.push(' ');
                output.push_str(&cell_text);
                output.push_str(" |");
            }
            output.push('\n');

            // Add separator after header row
            if row == 0 {
                output.push('|');
                for _ in 0..self.cols {
                    output.push_str("---|");
                }
                output.push('\n');
            }
        }

        output
    }

    /// Export table to HTML format.
    pub fn to_html(&self) -> String {
        let mut output = String::from("<table>\n");

        // Create a grid to hold cell contents
        let mut grid: HashMap<(usize, usize), &TableCell> = HashMap::new();

        for cell in &self.cells {
            grid.insert((cell.row, cell.col), cell);
        }

        // Generate HTML table
        for row in 0..self.rows {
            output.push_str("  <tr>\n");
            for col in 0..self.cols {
                if let Some(cell) = grid.get(&(row, col)) {
                    let tag = if cell.is_header { "th" } else { "td" };
                    output.push_str(&format!("    <{}>{}</{}>\n", tag, cell.text, tag));
                } else {
                    output.push_str("    <td></td>\n");
                }
            }
            output.push_str("  </tr>\n");
        }

        output.push_str("</table>");
        output
    }

    /// Get all cells in a specific row.
    pub fn get_row(&self, row_index: usize) -> Vec<&TableCell> {
        self.cells.iter().filter(|c| c.row == row_index).collect()
    }

    /// Get all cells in a specific column.
    pub fn get_column(&self, col_index: usize) -> Vec<&TableCell> {
        self.cells.iter().filter(|c| c.col == col_index).collect()
    }

    /// Get header cells (first row if header detection is enabled).
    pub fn get_headers(&self) -> Vec<&TableCell> {
        self.cells.iter().filter(|c| c.is_header).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_cell_creation() {
        let cell = TableCell::new(0, 0, "Header".to_string(), [0.0, 0.0, 100.0, 20.0])
            .with_header(true)
            .with_confidence(0.95);

        assert_eq!(cell.row, 0);
        assert_eq!(cell.col, 0);
        assert_eq!(cell.text, "Header");
        assert!(cell.is_header);
        assert_eq!(cell.confidence, 0.95);
    }

    #[test]
    fn test_table_to_csv() {
        let cells = vec![
            TableCell::new(0, 0, "Name".to_string(), [0.0, 0.0, 50.0, 20.0]),
            TableCell::new(0, 1, "Age".to_string(), [50.0, 0.0, 50.0, 20.0]),
            TableCell::new(1, 0, "Alice".to_string(), [0.0, 20.0, 50.0, 20.0]),
            TableCell::new(1, 1, "30".to_string(), [50.0, 20.0, 50.0, 20.0]),
        ];

        let table = Table {
            bbox: [0.0, 0.0, 100.0, 40.0],
            rows: 2,
            cols: 2,
            cells,
            confidence: 0.9,
        };

        let csv = table.to_csv();
        assert!(csv.contains("Name,Age"));
        assert!(csv.contains("Alice,30"));
    }

    #[test]
    fn test_table_to_markdown() {
        let cells = vec![
            TableCell::new(0, 0, "Header1".to_string(), [0.0, 0.0, 50.0, 20.0]).with_header(true),
            TableCell::new(0, 1, "Header2".to_string(), [50.0, 0.0, 50.0, 20.0]).with_header(true),
            TableCell::new(1, 0, "Data1".to_string(), [0.0, 20.0, 50.0, 20.0]),
            TableCell::new(1, 1, "Data2".to_string(), [50.0, 20.0, 50.0, 20.0]),
        ];

        let table = Table {
            bbox: [0.0, 0.0, 100.0, 40.0],
            rows: 2,
            cols: 2,
            cells,
            confidence: 0.9,
        };

        let md = table.to_markdown();
        assert!(md.contains("Header1"));
        assert!(md.contains("---"));
        assert!(md.contains("Data1"));
    }

    #[test]
    fn test_table_extractor_config() {
        let config = TableExtractionConfig {
            min_confidence: 0.8,
            min_rows: 3,
            min_cols: 3,
            detect_headers: false,
            cell_padding: 10.0,
        };

        let extractor = TableExtractor::with_config(config.clone());
        assert_eq!(extractor.config.min_confidence, 0.8);
        assert_eq!(extractor.config.min_rows, 3);
        assert!(!extractor.config.detect_headers);
    }

    #[test]
    fn test_table_get_row() {
        let cells = vec![
            TableCell::new(0, 0, "A".to_string(), [0.0, 0.0, 20.0, 20.0]),
            TableCell::new(0, 1, "B".to_string(), [20.0, 0.0, 20.0, 20.0]),
            TableCell::new(1, 0, "C".to_string(), [0.0, 20.0, 20.0, 20.0]),
        ];

        let table = Table {
            bbox: [0.0, 0.0, 40.0, 40.0],
            rows: 2,
            cols: 2,
            cells,
            confidence: 0.9,
        };

        let row0 = table.get_row(0);
        assert_eq!(row0.len(), 2);
        assert_eq!(row0[0].text, "A");
    }

    #[test]
    fn test_table_get_headers() {
        let cells = vec![
            TableCell::new(0, 0, "Header1".to_string(), [0.0, 0.0, 50.0, 20.0]).with_header(true),
            TableCell::new(0, 1, "Header2".to_string(), [50.0, 0.0, 50.0, 20.0]).with_header(true),
            TableCell::new(1, 0, "Data".to_string(), [0.0, 20.0, 50.0, 20.0]),
        ];

        let table = Table {
            bbox: [0.0, 0.0, 100.0, 40.0],
            rows: 2,
            cols: 2,
            cells,
            confidence: 0.9,
        };

        let headers = table.get_headers();
        assert_eq!(headers.len(), 2);
        assert!(headers.iter().all(|h| h.is_header));
    }
}
