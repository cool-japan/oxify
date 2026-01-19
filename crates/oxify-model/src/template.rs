//! Workflow templates for parameterized workflow creation
//!
//! Templates allow creating reusable workflow patterns with configurable parameters.

use crate::Workflow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Template ID type
pub type TemplateId = Uuid;

/// A workflow template with parameterized values
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WorkflowTemplate {
    /// Unique template identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: TemplateId,

    /// Template name
    pub name: String,

    /// Template description
    pub description: Option<String>,

    /// Template category (e.g., "RAG", "Agent", "Data Processing")
    pub category: Option<String>,

    /// Tags for discovery
    #[serde(default)]
    pub tags: Vec<String>,

    /// Template version
    pub version: String,

    /// Template parameters (configurable values)
    pub parameters: Vec<TemplateParameter>,

    /// Base workflow JSON (with parameter placeholders)
    /// Placeholders use format: {{param_name}}
    pub workflow_json: String,

    /// Template author
    pub author: Option<String>,

    /// Creation timestamp
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub created_at: DateTime<Utc>,

    /// Last updated timestamp
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub updated_at: DateTime<Utc>,

    /// Number of times this template has been instantiated
    #[serde(default)]
    pub usage_count: u64,

    /// Is this template public
    #[serde(default)]
    pub is_public: bool,

    /// Owner user ID
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub owner_id: Option<Uuid>,
}

/// A parameter in a workflow template
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TemplateParameter {
    /// Parameter name (used in placeholders)
    pub name: String,

    /// Display label for UI
    pub label: String,

    /// Parameter description
    pub description: Option<String>,

    /// Parameter type
    pub param_type: ParameterType,

    /// Default value (JSON)
    pub default_value: Option<serde_json::Value>,

    /// Whether this parameter is required
    #[serde(default)]
    pub required: bool,

    /// Validation rules
    #[serde(default)]
    pub validation: Option<ParameterValidation>,

    /// Allowed values (for enum types)
    #[serde(default)]
    pub allowed_values: Vec<serde_json::Value>,

    /// Group name for UI organization
    pub group: Option<String>,

    /// Display order within group
    #[serde(default)]
    pub order: u32,
}

/// Parameter types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum ParameterType {
    /// String value
    String,

    /// Integer number
    Integer,

    /// Floating point number
    Float,

    /// Boolean value
    Boolean,

    /// JSON object
    Object,

    /// JSON array
    Array,

    /// Selection from allowed values
    Enum,

    /// Secret reference (won't be stored in plain text)
    Secret,

    /// LLM model selection
    Model,

    /// Vector database collection
    Collection,
}

/// Parameter validation rules
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ParameterValidation {
    /// Minimum value (for numbers)
    pub min: Option<f64>,

    /// Maximum value (for numbers)
    pub max: Option<f64>,

    /// Minimum length (for strings)
    pub min_length: Option<usize>,

    /// Maximum length (for strings)
    pub max_length: Option<usize>,

    /// Regex pattern (for strings)
    pub pattern: Option<String>,

    /// Custom validation message
    pub message: Option<String>,
}

impl WorkflowTemplate {
    /// Create a new workflow template
    pub fn new(name: String, workflow_json: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            category: None,
            tags: Vec::new(),
            version: "1.0.0".to_string(),
            parameters: Vec::new(),
            workflow_json,
            author: None,
            created_at: now,
            updated_at: now,
            usage_count: 0,
            is_public: false,
            owner_id: None,
        }
    }

    /// Add a parameter to the template
    pub fn add_parameter(&mut self, param: TemplateParameter) {
        self.parameters.push(param);
    }

    /// Validate parameter values against the template
    pub fn validate_parameters(
        &self,
        values: &HashMap<String, serde_json::Value>,
    ) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        for param in &self.parameters {
            if param.required && !values.contains_key(&param.name) {
                errors.push(format!("Required parameter '{}' is missing", param.name));
                continue;
            }

            if let Some(value) = values.get(&param.name) {
                // Type validation
                if !self.validate_type(&param.param_type, value) {
                    errors.push(format!(
                        "Parameter '{}' has invalid type, expected {:?}",
                        param.name, param.param_type
                    ));
                }

                // Range/length validation
                if let Some(ref validation) = param.validation {
                    if let Some(err) = self.validate_value(value, validation, &param.name) {
                        errors.push(err);
                    }
                }

                // Enum validation
                if param.param_type == ParameterType::Enum
                    && !param.allowed_values.is_empty()
                    && !param.allowed_values.contains(value)
                {
                    errors.push(format!(
                        "Parameter '{}' must be one of: {:?}",
                        param.name, param.allowed_values
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_type(&self, param_type: &ParameterType, value: &serde_json::Value) -> bool {
        match param_type {
            ParameterType::String
            | ParameterType::Secret
            | ParameterType::Model
            | ParameterType::Collection => value.is_string(),
            ParameterType::Integer => value.is_i64() || value.is_u64(),
            ParameterType::Float => value.is_f64() || value.is_i64() || value.is_u64(),
            ParameterType::Boolean => value.is_boolean(),
            ParameterType::Object => value.is_object(),
            ParameterType::Array => value.is_array(),
            ParameterType::Enum => true, // Validated separately
        }
    }

    fn validate_value(
        &self,
        value: &serde_json::Value,
        validation: &ParameterValidation,
        name: &str,
    ) -> Option<String> {
        // Number validation
        if let Some(num) = value.as_f64() {
            if let Some(min) = validation.min {
                if num < min {
                    return Some(format!("Parameter '{}' must be >= {}", name, min));
                }
            }
            if let Some(max) = validation.max {
                if num > max {
                    return Some(format!("Parameter '{}' must be <= {}", name, max));
                }
            }
        }

        // String validation
        if let Some(s) = value.as_str() {
            if let Some(min_len) = validation.min_length {
                if s.len() < min_len {
                    return Some(format!(
                        "Parameter '{}' must be at least {} characters",
                        name, min_len
                    ));
                }
            }
            if let Some(max_len) = validation.max_length {
                if s.len() > max_len {
                    return Some(format!(
                        "Parameter '{}' must be at most {} characters",
                        name, max_len
                    ));
                }
            }
            if let Some(ref pattern) = validation.pattern {
                // Note: Full regex validation would require regex crate
                // For now, just check if pattern is provided
                if !pattern.is_empty() {
                    // Pattern validation would go here
                }
            }
        }

        None
    }

    /// Instantiate the template with parameter values
    pub fn instantiate(
        &self,
        values: &HashMap<String, serde_json::Value>,
    ) -> Result<Workflow, String> {
        // Validate parameters first
        if let Err(errors) = self.validate_parameters(values) {
            return Err(format!(
                "Parameter validation failed: {}",
                errors.join(", ")
            ));
        }

        // Apply parameter substitution
        let mut workflow_str = self.workflow_json.clone();

        for param in &self.parameters {
            let placeholder = format!("{{{{{}}}}}", param.name);
            let value = values
                .get(&param.name)
                .or(param.default_value.as_ref())
                .map(|v| {
                    if v.is_string() {
                        v.as_str().unwrap_or("").to_string()
                    } else {
                        v.to_string()
                    }
                })
                .unwrap_or_default();

            workflow_str = workflow_str.replace(&placeholder, &value);
        }

        // Parse the workflow
        let workflow: Workflow = serde_json::from_str(&workflow_str)
            .map_err(|e| format!("Failed to parse instantiated workflow: {}", e))?;

        Ok(workflow)
    }

    /// Create a template from an existing workflow
    pub fn from_workflow(workflow: &Workflow, name: String) -> Result<Self, String> {
        let workflow_json = serde_json::to_string_pretty(workflow)
            .map_err(|e| format!("Failed to serialize workflow: {}", e))?;

        Ok(Self::new(name, workflow_json))
    }

    /// Extract parameter placeholders from the workflow JSON
    pub fn extract_placeholders(&self) -> Vec<String> {
        let mut placeholders = Vec::new();

        // Simple pattern matching without regex crate
        let chars: Vec<char> = self.workflow_json.chars().collect();
        let mut i = 0;
        while i < chars.len().saturating_sub(3) {
            if chars[i] == '{' && chars[i + 1] == '{' {
                let start = i + 2;
                let mut end = start;
                while end < chars.len().saturating_sub(1)
                    && !(chars[end] == '}' && chars[end + 1] == '}')
                {
                    end += 1;
                }
                if end < chars.len().saturating_sub(1) {
                    let name: String = chars[start..end].iter().collect();
                    let trimmed = name.trim().to_string();
                    if !trimmed.is_empty() && !placeholders.contains(&trimmed) {
                        placeholders.push(trimmed);
                    }
                }
                i = end + 2;
            } else {
                i += 1;
            }
        }

        placeholders
    }
}

impl TemplateParameter {
    /// Create a new string parameter
    pub fn string(name: &str, label: &str) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            description: None,
            param_type: ParameterType::String,
            default_value: None,
            required: false,
            validation: None,
            allowed_values: Vec::new(),
            group: None,
            order: 0,
        }
    }

    /// Create a new integer parameter
    pub fn integer(name: &str, label: &str) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            description: None,
            param_type: ParameterType::Integer,
            default_value: None,
            required: false,
            validation: None,
            allowed_values: Vec::new(),
            group: None,
            order: 0,
        }
    }

    /// Create a new boolean parameter
    pub fn boolean(name: &str, label: &str) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            description: None,
            param_type: ParameterType::Boolean,
            default_value: None,
            required: false,
            validation: None,
            allowed_values: Vec::new(),
            group: None,
            order: 0,
        }
    }

    /// Create an enum parameter with allowed values
    pub fn enumeration(name: &str, label: &str, allowed: Vec<&str>) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            description: None,
            param_type: ParameterType::Enum,
            default_value: None,
            required: false,
            validation: None,
            allowed_values: allowed
                .into_iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect(),
            group: None,
            order: 0,
        }
    }

    /// Create a model selection parameter
    pub fn model(name: &str, label: &str) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            description: None,
            param_type: ParameterType::Model,
            default_value: None,
            required: false,
            validation: None,
            allowed_values: Vec::new(),
            group: None,
            order: 0,
        }
    }

    /// Set as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set default value
    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default_value = Some(value);
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Set validation
    pub fn with_validation(mut self, validation: ParameterValidation) -> Self {
        self.validation = Some(validation);
        self
    }

    /// Set group
    pub fn in_group(mut self, group: &str) -> Self {
        self.group = Some(group.to_string());
        self
    }

    /// Set order
    pub fn with_order(mut self, order: u32) -> Self {
        self.order = order;
        self
    }
}

/// Request to instantiate a template
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct InstantiateTemplateRequest {
    /// Workflow name for the new instance
    pub workflow_name: String,

    /// Parameter values
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Template gallery item (for listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TemplateListItem {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: TemplateId,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub version: String,
    pub author: Option<String>,
    pub usage_count: u64,
    pub is_public: bool,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub created_at: DateTime<Utc>,
}

impl From<&WorkflowTemplate> for TemplateListItem {
    fn from(template: &WorkflowTemplate) -> Self {
        Self {
            id: template.id,
            name: template.name.clone(),
            description: template.description.clone(),
            category: template.category.clone(),
            tags: template.tags.clone(),
            version: template.version.clone(),
            author: template.author.clone(),
            usage_count: template.usage_count,
            is_public: template.is_public,
            created_at: template.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_creation() {
        let workflow_json = r#"{"metadata": {"name": "{{workflow_name}}"}}"#;
        let mut template =
            WorkflowTemplate::new("Test Template".to_string(), workflow_json.to_string());

        template.add_parameter(
            TemplateParameter::string("workflow_name", "Workflow Name")
                .required()
                .with_description("Name of the workflow"),
        );

        assert_eq!(template.name, "Test Template");
        assert_eq!(template.parameters.len(), 1);
        assert!(template.parameters[0].required);
    }

    #[test]
    fn test_placeholder_extraction() {
        let workflow_json =
            r#"{"name": "{{name}}", "model": "{{model}}", "temp": {{temperature}}}"#;
        let template = WorkflowTemplate::new("Test".to_string(), workflow_json.to_string());

        let placeholders = template.extract_placeholders();

        assert_eq!(placeholders.len(), 3);
        assert!(placeholders.contains(&"name".to_string()));
        assert!(placeholders.contains(&"model".to_string()));
        assert!(placeholders.contains(&"temperature".to_string()));
    }

    #[test]
    fn test_parameter_validation() {
        let mut template = WorkflowTemplate::new("Test".to_string(), "{}".to_string());
        template.add_parameter(
            TemplateParameter::integer("count", "Count")
                .required()
                .with_validation(ParameterValidation {
                    min: Some(1.0),
                    max: Some(100.0),
                    ..Default::default()
                }),
        );

        // Missing required parameter
        let values = HashMap::new();
        assert!(template.validate_parameters(&values).is_err());

        // Invalid value (too low)
        let mut values = HashMap::new();
        values.insert("count".to_string(), serde_json::json!(0));
        assert!(template.validate_parameters(&values).is_err());

        // Valid value
        let mut values = HashMap::new();
        values.insert("count".to_string(), serde_json::json!(50));
        assert!(template.validate_parameters(&values).is_ok());
    }

    #[test]
    fn test_enum_parameter() {
        let param = TemplateParameter::enumeration(
            "provider",
            "LLM Provider",
            vec!["openai", "anthropic", "ollama"],
        )
        .required();

        assert_eq!(param.param_type, ParameterType::Enum);
        assert_eq!(param.allowed_values.len(), 3);
        assert!(param.required);
    }
}
