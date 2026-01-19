//! Form field detection and extraction.
//!
//! This module provides functionality to detect and extract form fields from images,
//! including key-value pairs, checkboxes, radio buttons, and signatures.

use crate::types::{OcrResult, TextBlock};
use serde::{Deserialize, Serialize};

/// Represents a detected form field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    /// Field name/label
    pub name: String,
    /// Field value
    pub value: String,
    /// Field type
    pub field_type: FieldType,
    /// Bounding box [x, y, width, height]
    pub bbox: [f32; 4],
    /// Confidence score
    pub confidence: f32,
}

/// Type of form field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType {
    /// Text input field
    Text,
    /// Checkbox
    Checkbox,
    /// Radio button
    RadioButton,
    /// Signature field
    Signature,
    /// Date field
    Date,
    /// Email field
    Email,
    /// Phone number field
    Phone,
    /// Currency/amount field
    Currency,
    /// Unknown/other field type
    Other,
}

/// Result of form field detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormDetectionResult {
    /// Detected form fields
    pub fields: Vec<FormField>,
    /// Checkbox states
    pub checkboxes: Vec<Checkbox>,
    /// Radio button groups
    pub radio_groups: Vec<RadioGroup>,
    /// Detected signatures
    pub signatures: Vec<Signature>,
}

/// Represents a detected checkbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkbox {
    /// Label text
    pub label: String,
    /// Whether the checkbox is checked
    pub checked: bool,
    /// Bounding box [x, y, width, height]
    pub bbox: [f32; 4],
    /// Confidence score
    pub confidence: f32,
}

/// Represents a group of related radio buttons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioGroup {
    /// Group name/label
    pub name: String,
    /// Radio button options
    pub options: Vec<RadioButton>,
    /// Index of selected option (if any)
    pub selected: Option<usize>,
}

/// Represents a single radio button.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioButton {
    /// Option label
    pub label: String,
    /// Whether this option is selected
    pub selected: bool,
    /// Bounding box [x, y, width, height]
    pub bbox: [f32; 4],
    /// Confidence score
    pub confidence: f32,
}

/// Represents a detected signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// Signature field label
    pub label: Option<String>,
    /// Whether a signature is present
    pub has_signature: bool,
    /// Bounding box [x, y, width, height]
    pub bbox: [f32; 4],
    /// Quality score (0.0 - 1.0)
    pub quality: f32,
}

/// Configuration for form field detection.
#[derive(Debug, Clone)]
pub struct FormDetectionConfig {
    /// Minimum confidence for field detection
    pub min_confidence: f32,
    /// Enable checkbox detection
    pub detect_checkboxes: bool,
    /// Enable radio button detection
    pub detect_radio_buttons: bool,
    /// Enable signature detection
    pub detect_signatures: bool,
    /// Maximum distance to consider label-value pairs (pixels)
    pub max_label_value_distance: f32,
}

impl Default for FormDetectionConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.7,
            detect_checkboxes: true,
            detect_radio_buttons: true,
            detect_signatures: true,
            max_label_value_distance: 100.0,
        }
    }
}

/// Form field detector.
pub struct FormDetector {
    config: FormDetectionConfig,
}

impl FormDetector {
    /// Create a new form detector with default configuration.
    pub fn new() -> Self {
        Self {
            config: FormDetectionConfig::default(),
        }
    }

    /// Create a new form detector with custom configuration.
    pub fn with_config(config: FormDetectionConfig) -> Self {
        Self { config }
    }

    /// Detect form fields from OCR result.
    pub fn detect_fields(&self, ocr_result: &OcrResult) -> FormDetectionResult {
        let mut fields = Vec::new();
        let checkboxes = if self.config.detect_checkboxes {
            self.detect_checkboxes(&ocr_result.blocks)
        } else {
            Vec::new()
        };

        let radio_groups = if self.config.detect_radio_buttons {
            self.detect_radio_groups(&ocr_result.blocks)
        } else {
            Vec::new()
        };

        let signatures = if self.config.detect_signatures {
            self.detect_signatures(&ocr_result.blocks)
        } else {
            Vec::new()
        };

        // Detect key-value pairs
        fields.extend(self.detect_key_value_pairs(&ocr_result.blocks));

        FormDetectionResult {
            fields,
            checkboxes,
            radio_groups,
            signatures,
        }
    }

    /// Detect key-value pairs from text blocks.
    fn detect_key_value_pairs(&self, blocks: &[TextBlock]) -> Vec<FormField> {
        let mut fields = Vec::new();

        // Simple heuristic: look for patterns like "Label: Value" or "Label _____"
        for (i, block) in blocks.iter().enumerate() {
            if block.text.contains(':') {
                // Split on colon
                let parts: Vec<&str> = block.text.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let name = parts[0].trim().to_string();
                    let value = parts[1].trim().to_string();

                    if !name.is_empty() {
                        let field_type = self.infer_field_type(&value);
                        fields.push(FormField {
                            name,
                            value,
                            field_type,
                            bbox: block.bbox,
                            confidence: block.confidence,
                        });
                    }
                }
            } else if i + 1 < blocks.len() {
                // Check if next block is close enough to be a value
                let next_block = &blocks[i + 1];
                let distance = self.calculate_distance(block.bbox, next_block.bbox);

                if distance < self.config.max_label_value_distance {
                    fields.push(FormField {
                        name: block.text.clone(),
                        value: next_block.text.clone(),
                        field_type: self.infer_field_type(&next_block.text),
                        bbox: self.merge_bboxes(block.bbox, next_block.bbox),
                        confidence: (block.confidence + next_block.confidence) / 2.0,
                    });
                }
            }
        }

        fields
    }

    /// Detect checkboxes from text blocks.
    fn detect_checkboxes(&self, blocks: &[TextBlock]) -> Vec<Checkbox> {
        let mut checkboxes = Vec::new();

        for block in blocks {
            // Look for checkbox markers like [x], [ ], ☑, ☐
            if block.text.trim() == "[x]" || block.text.trim() == "[X]" || block.text.contains('☑')
            {
                checkboxes.push(Checkbox {
                    label: String::new(), // Would need to find adjacent label
                    checked: true,
                    bbox: block.bbox,
                    confidence: block.confidence,
                });
            } else if block.text.trim() == "[ ]" || block.text.contains('☐') {
                checkboxes.push(Checkbox {
                    label: String::new(),
                    checked: false,
                    bbox: block.bbox,
                    confidence: block.confidence,
                });
            }
        }

        checkboxes
    }

    /// Detect radio button groups from text blocks.
    fn detect_radio_groups(&self, _blocks: &[TextBlock]) -> Vec<RadioGroup> {
        // Stub implementation
        Vec::new()
    }

    /// Detect signatures from text blocks.
    fn detect_signatures(&self, blocks: &[TextBlock]) -> Vec<Signature> {
        let mut signatures = Vec::new();

        for block in blocks {
            // Look for signature-related keywords
            let lower_text = block.text.to_lowercase();
            if lower_text.contains("signature")
                || lower_text.contains("sign here")
                || lower_text.contains("signed")
            {
                signatures.push(Signature {
                    label: Some(block.text.clone()),
                    has_signature: false, // Would need image analysis to determine
                    bbox: block.bbox,
                    quality: 0.0,
                });
            }
        }

        signatures
    }

    /// Infer field type from value content.
    fn infer_field_type(&self, value: &str) -> FieldType {
        let value_lower = value.to_lowercase();

        if value.contains('@') && value.contains('.') {
            FieldType::Email
        } else if value.chars().filter(|c| c.is_ascii_digit()).count() >= 10 {
            FieldType::Phone
        } else if value.contains('$')
            || value.contains('€')
            || value.contains('£')
            || value.contains('¥')
        {
            FieldType::Currency
        } else if value_lower.contains("date")
            || value.contains('/') && value.chars().filter(|c| c.is_ascii_digit()).count() >= 6
        {
            FieldType::Date
        } else {
            FieldType::Text
        }
    }

    /// Calculate distance between two bounding boxes.
    fn calculate_distance(&self, bbox1: [f32; 4], bbox2: [f32; 4]) -> f32 {
        let x1_center = bbox1[0] + bbox1[2] / 2.0;
        let y1_center = bbox1[1] + bbox1[3] / 2.0;
        let x2_center = bbox2[0] + bbox2[2] / 2.0;
        let y2_center = bbox2[1] + bbox2[3] / 2.0;

        let dx = x2_center - x1_center;
        let dy = y2_center - y1_center;

        (dx * dx + dy * dy).sqrt()
    }

    /// Merge two bounding boxes into one.
    fn merge_bboxes(&self, bbox1: [f32; 4], bbox2: [f32; 4]) -> [f32; 4] {
        let min_x = bbox1[0].min(bbox2[0]);
        let min_y = bbox1[1].min(bbox2[1]);
        let max_x = (bbox1[0] + bbox1[2]).max(bbox2[0] + bbox2[2]);
        let max_y = (bbox1[1] + bbox1[3]).max(bbox2[1] + bbox2[3]);

        [min_x, min_y, max_x - min_x, max_y - min_y]
    }
}

impl Default for FormDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl FormDetectionResult {
    /// Get all fields of a specific type.
    pub fn get_fields_by_type(&self, field_type: FieldType) -> Vec<&FormField> {
        self.fields
            .iter()
            .filter(|f| f.field_type == field_type)
            .collect()
    }

    /// Get a field by name.
    pub fn get_field(&self, name: &str) -> Option<&FormField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get all checked checkboxes.
    pub fn get_checked_boxes(&self) -> Vec<&Checkbox> {
        self.checkboxes.iter().filter(|c| c.checked).collect()
    }

    /// Export to JSON format.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Export field values to a flat key-value structure.
    pub fn to_key_value_map(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();

        for field in &self.fields {
            map.insert(field.name.clone(), field.value.clone());
        }

        for checkbox in &self.checkboxes {
            map.insert(
                checkbox.label.clone(),
                if checkbox.checked { "true" } else { "false" }.to_string(),
            );
        }

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_form_field_creation() {
        let field = FormField {
            name: "Email".to_string(),
            value: "test@example.com".to_string(),
            field_type: FieldType::Email,
            bbox: [0.0, 0.0, 200.0, 30.0],
            confidence: 0.95,
        };

        assert_eq!(field.name, "Email");
        assert_eq!(field.field_type, FieldType::Email);
    }

    #[test]
    fn test_checkbox_creation() {
        let checkbox = Checkbox {
            label: "I agree".to_string(),
            checked: true,
            bbox: [0.0, 0.0, 20.0, 20.0],
            confidence: 0.9,
        };

        assert!(checkbox.checked);
        assert_eq!(checkbox.label, "I agree");
    }

    #[test]
    fn test_field_type_inference() {
        let detector = FormDetector::new();

        assert_eq!(
            detector.infer_field_type("test@example.com"),
            FieldType::Email
        );
        assert_eq!(detector.infer_field_type("$100.50"), FieldType::Currency);
        assert_eq!(detector.infer_field_type("555-1234-5678"), FieldType::Phone);
    }

    #[test]
    fn test_form_detection_config() {
        let config = FormDetectionConfig {
            min_confidence: 0.8,
            detect_checkboxes: false,
            detect_radio_buttons: false,
            detect_signatures: true,
            max_label_value_distance: 50.0,
        };

        let detector = FormDetector::with_config(config.clone());
        assert_eq!(detector.config.min_confidence, 0.8);
        assert!(!detector.config.detect_checkboxes);
        assert!(detector.config.detect_signatures);
    }

    #[test]
    fn test_form_result_get_checked_boxes() {
        let result = FormDetectionResult {
            fields: Vec::new(),
            checkboxes: vec![
                Checkbox {
                    label: "Option 1".to_string(),
                    checked: true,
                    bbox: [0.0, 0.0, 20.0, 20.0],
                    confidence: 0.9,
                },
                Checkbox {
                    label: "Option 2".to_string(),
                    checked: false,
                    bbox: [0.0, 30.0, 20.0, 20.0],
                    confidence: 0.9,
                },
            ],
            radio_groups: Vec::new(),
            signatures: Vec::new(),
        };

        let checked = result.get_checked_boxes();
        assert_eq!(checked.len(), 1);
        assert_eq!(checked[0].label, "Option 1");
    }

    #[test]
    fn test_signature_detection() {
        let signature = Signature {
            label: Some("Signature:".to_string()),
            has_signature: false,
            bbox: [0.0, 0.0, 200.0, 50.0],
            quality: 0.0,
        };

        assert!(!signature.has_signature);
        assert_eq!(signature.quality, 0.0);
    }
}
