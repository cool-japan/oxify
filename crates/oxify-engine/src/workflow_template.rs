//! Workflow templates for parameterized workflow generation
//!
//! This module provides support for creating reusable workflow templates
//! with parameters that can be validated and instantiated dynamically.

use oxify_model::Workflow;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during template operations
#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("Parameter validation failed: {0}")]
    ValidationError(String),

    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    #[error("Invalid parameter value: {0}")]
    InvalidValue(String),

    #[error("Template instantiation failed: {0}")]
    InstantiationError(String),

    #[error("Parameter type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },
}

pub type Result<T> = std::result::Result<T, TemplateError>;

/// Parameter definition for a workflow template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDef {
    /// Parameter name
    pub name: String,

    /// Parameter description
    pub description: Option<String>,

    /// Parameter type
    pub param_type: ParameterType,

    /// Whether this parameter is required
    pub required: bool,

    /// Default value if not provided
    pub default: Option<Value>,

    /// Validation rules
    #[serde(default)]
    pub validation: Vec<ValidationRule>,
}

impl ParameterDef {
    /// Create a new required parameter definition
    pub fn required(name: impl Into<String>, param_type: ParameterType) -> Self {
        Self {
            name: name.into(),
            description: None,
            param_type,
            required: true,
            default: None,
            validation: Vec::new(),
        }
    }

    /// Create a new optional parameter definition
    pub fn optional(name: impl Into<String>, param_type: ParameterType, default: Value) -> Self {
        Self {
            name: name.into(),
            description: None,
            param_type,
            required: false,
            default: Some(default),
            validation: Vec::new(),
        }
    }

    /// Add a description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a validation rule
    pub fn with_validation(mut self, rule: ValidationRule) -> Self {
        self.validation.push(rule);
        self
    }

    /// Validate a parameter value
    pub fn validate(&self, value: &Value) -> Result<()> {
        // Check type
        if !self.param_type.matches(value) {
            return Err(TemplateError::TypeMismatch {
                expected: format!("{:?}", self.param_type),
                actual: format!("{:?}", value),
            });
        }

        // Apply validation rules
        for rule in &self.validation {
            rule.validate(value)?;
        }

        Ok(())
    }
}

/// Parameter types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ParameterType {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Any,
}

impl ParameterType {
    /// Check if a value matches this type
    pub fn matches(&self, value: &Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Number => value.is_number(),
            Self::Boolean => value.is_boolean(),
            Self::Object => value.is_object(),
            Self::Array => value.is_array(),
            Self::Any => true,
        }
    }
}

/// Validation rules for parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationRule {
    /// Minimum value (for numbers)
    Min(f64),
    /// Maximum value (for numbers)
    Max(f64),
    /// Minimum length (for strings and arrays)
    MinLength(usize),
    /// Maximum length (for strings and arrays)
    MaxLength(usize),
    /// Regular expression pattern (for strings)
    Pattern(String),
    /// Allowed values (enum)
    OneOf(Vec<Value>),
}

impl ValidationRule {
    /// Validate a value against this rule
    pub fn validate(&self, value: &Value) -> Result<()> {
        match self {
            Self::Min(min) => {
                if let Some(num) = value.as_f64() {
                    if num < *min {
                        return Err(TemplateError::ValidationError(format!(
                            "Value {} is less than minimum {}",
                            num, min
                        )));
                    }
                }
            }
            Self::Max(max) => {
                if let Some(num) = value.as_f64() {
                    if num > *max {
                        return Err(TemplateError::ValidationError(format!(
                            "Value {} is greater than maximum {}",
                            num, max
                        )));
                    }
                }
            }
            Self::MinLength(min_len) => {
                let len = match value {
                    Value::String(s) => s.len(),
                    Value::Array(arr) => arr.len(),
                    _ => 0,
                };
                if len < *min_len {
                    return Err(TemplateError::ValidationError(format!(
                        "Length {} is less than minimum {}",
                        len, min_len
                    )));
                }
            }
            Self::MaxLength(max_len) => {
                let len = match value {
                    Value::String(s) => s.len(),
                    Value::Array(arr) => arr.len(),
                    _ => 0,
                };
                if len > *max_len {
                    return Err(TemplateError::ValidationError(format!(
                        "Length {} is greater than maximum {}",
                        len, max_len
                    )));
                }
            }
            Self::Pattern(pattern) => {
                if let Some(s) = value.as_str() {
                    let re = regex::Regex::new(pattern).map_err(|e| {
                        TemplateError::ValidationError(format!("Invalid regex pattern: {}", e))
                    })?;
                    if !re.is_match(s) {
                        return Err(TemplateError::ValidationError(format!(
                            "Value '{}' does not match pattern '{}'",
                            s, pattern
                        )));
                    }
                }
            }
            Self::OneOf(allowed) => {
                if !allowed.contains(value) {
                    return Err(TemplateError::ValidationError(format!(
                        "Value {:?} is not one of {:?}",
                        value, allowed
                    )));
                }
            }
        }
        Ok(())
    }
}

/// A workflow template with parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    /// Template name
    pub name: String,

    /// Template description
    pub description: Option<String>,

    /// Template version
    pub version: String,

    /// Parameter definitions
    pub parameters: Vec<ParameterDef>,

    /// Base workflow structure (can contain parameter placeholders)
    pub workflow: Workflow,

    /// Tags for organization
    #[serde(default)]
    pub tags: Vec<String>,
}

impl WorkflowTemplate {
    /// Create a new workflow template
    pub fn new(name: impl Into<String>, workflow: Workflow) -> Self {
        Self {
            name: name.into(),
            description: None,
            version: "1.0.0".to_string(),
            parameters: Vec::new(),
            workflow,
            tags: Vec::new(),
        }
    }

    /// Add a parameter definition
    pub fn with_parameter(mut self, param: ParameterDef) -> Self {
        self.parameters.push(param);
        self
    }

    /// Add multiple parameter definitions
    pub fn with_parameters(mut self, params: Vec<ParameterDef>) -> Self {
        self.parameters.extend(params);
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Validate parameters
    pub fn validate_parameters(&self, params: &HashMap<String, Value>) -> Result<()> {
        // Check all required parameters are present
        for param_def in &self.parameters {
            if param_def.required && !params.contains_key(&param_def.name) {
                return Err(TemplateError::MissingParameter(param_def.name.clone()));
            }

            // Validate provided parameters
            if let Some(value) = params.get(&param_def.name) {
                param_def.validate(value)?;
            }
        }

        Ok(())
    }

    /// Instantiate workflow with parameters
    pub fn instantiate(&self, params: HashMap<String, Value>) -> Result<Workflow> {
        // Validate parameters
        self.validate_parameters(&params)?;

        // Create parameter map with defaults
        let mut resolved_params = HashMap::new();
        for param_def in &self.parameters {
            if let Some(value) = params.get(&param_def.name) {
                resolved_params.insert(param_def.name.clone(), value.clone());
            } else if let Some(default) = &param_def.default {
                resolved_params.insert(param_def.name.clone(), default.clone());
            }
        }

        // Clone base workflow
        let mut workflow = self.workflow.clone();

        // Apply parameters to workflow
        workflow = self.apply_parameters(workflow, &resolved_params)?;

        Ok(workflow)
    }

    /// Apply parameters to workflow (replace placeholders)
    fn apply_parameters(
        &self,
        mut workflow: Workflow,
        params: &HashMap<String, Value>,
    ) -> Result<Workflow> {
        // Replace parameter placeholders in workflow metadata
        if let Ok(json_str) = serde_json::to_string(&workflow.metadata) {
            let replaced = self.replace_placeholders(&json_str, params);
            if let Ok(metadata) = serde_json::from_str(&replaced) {
                workflow.metadata = metadata;
            }
        }

        // Replace parameter placeholders in nodes
        for node in &mut workflow.nodes {
            if let Ok(json_str) = serde_json::to_string(&node) {
                let replaced = self.replace_placeholders(&json_str, params);
                if let Ok(new_node) = serde_json::from_str(&replaced) {
                    *node = new_node;
                }
            }
        }

        Ok(workflow)
    }

    /// Replace parameter placeholders in a string
    fn replace_placeholders(&self, text: &str, params: &HashMap<String, Value>) -> String {
        let mut result = text.to_string();

        for (key, value) in params {
            let placeholder = format!("{{{{param.{}}}}}", key);
            let replacement = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&placeholder, &replacement);
        }

        result
    }

    /// Get parameter definition by name
    pub fn get_parameter(&self, name: &str) -> Option<&ParameterDef> {
        self.parameters.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, Node, NodeKind};
    use serde_json::json;

    #[test]
    fn test_parameter_def_required() {
        let param = ParameterDef::required("api_key", ParameterType::String);
        assert_eq!(param.name, "api_key");
        assert!(param.required);
        assert!(param.default.is_none());
    }

    #[test]
    fn test_parameter_def_optional() {
        let param = ParameterDef::optional("timeout", ParameterType::Number, json!(30));
        assert_eq!(param.name, "timeout");
        assert!(!param.required);
        assert_eq!(param.default, Some(json!(30)));
    }

    #[test]
    fn test_parameter_type_matches() {
        assert!(ParameterType::String.matches(&json!("hello")));
        assert!(ParameterType::Number.matches(&json!(42)));
        assert!(ParameterType::Boolean.matches(&json!(true)));
        assert!(ParameterType::Array.matches(&json!([1, 2, 3])));
        assert!(ParameterType::Object.matches(&json!({"key": "value"})));
        assert!(ParameterType::Any.matches(&json!("anything")));
    }

    #[test]
    fn test_validation_rule_min() {
        let rule = ValidationRule::Min(10.0);
        assert!(rule.validate(&json!(15)).is_ok());
        assert!(rule.validate(&json!(5)).is_err());
    }

    #[test]
    fn test_validation_rule_max() {
        let rule = ValidationRule::Max(100.0);
        assert!(rule.validate(&json!(50)).is_ok());
        assert!(rule.validate(&json!(150)).is_err());
    }

    #[test]
    fn test_validation_rule_min_length() {
        let rule = ValidationRule::MinLength(3);
        assert!(rule.validate(&json!("hello")).is_ok());
        assert!(rule.validate(&json!("hi")).is_err());
        assert!(rule.validate(&json!([1, 2, 3, 4])).is_ok());
    }

    #[test]
    fn test_validation_rule_pattern() {
        let rule = ValidationRule::Pattern(r"^\w+@\w+\.\w+$".to_string());
        assert!(rule.validate(&json!("user@example.com")).is_ok());
        assert!(rule.validate(&json!("invalid-email")).is_err());
    }

    #[test]
    fn test_validation_rule_one_of() {
        let rule = ValidationRule::OneOf(vec![json!("dev"), json!("staging"), json!("prod")]);
        assert!(rule.validate(&json!("dev")).is_ok());
        assert!(rule.validate(&json!("prod")).is_ok());
        assert!(rule.validate(&json!("test")).is_err());
    }

    #[test]
    fn test_workflow_template_creation() {
        let workflow = Workflow::new("Test Workflow".to_string());
        let template = WorkflowTemplate::new("test_template", workflow)
            .with_description("A test template")
            .with_version("1.0.0");

        assert_eq!(template.name, "test_template");
        assert_eq!(template.description, Some("A test template".to_string()));
        assert_eq!(template.version, "1.0.0");
    }

    #[test]
    fn test_workflow_template_with_parameters() {
        let workflow = Workflow::new("Test Workflow".to_string());
        let template = WorkflowTemplate::new("test_template", workflow)
            .with_parameter(ParameterDef::required("api_key", ParameterType::String))
            .with_parameter(ParameterDef::optional(
                "timeout",
                ParameterType::Number,
                json!(30),
            ));

        assert_eq!(template.parameters.len(), 2);
    }

    #[test]
    fn test_validate_parameters_missing_required() {
        let workflow = Workflow::new("Test Workflow".to_string());
        let template = WorkflowTemplate::new("test_template", workflow)
            .with_parameter(ParameterDef::required("api_key", ParameterType::String));

        let params = HashMap::new();
        let result = template.validate_parameters(&params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_parameters_valid() {
        let workflow = Workflow::new("Test Workflow".to_string());
        let template = WorkflowTemplate::new("test_template", workflow)
            .with_parameter(ParameterDef::required("api_key", ParameterType::String))
            .with_parameter(ParameterDef::optional(
                "timeout",
                ParameterType::Number,
                json!(30),
            ));

        let mut params = HashMap::new();
        params.insert("api_key".to_string(), json!("secret123"));
        params.insert("timeout".to_string(), json!(60));

        let result = template.validate_parameters(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_instantiate_workflow() {
        let mut workflow = Workflow::new("Test Workflow".to_string());
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);
        let start_id = start.id;
        let end_id = end.id;
        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        let template = WorkflowTemplate::new("test_template", workflow).with_parameter(
            ParameterDef::optional("name", ParameterType::String, json!("default")),
        );

        let mut params = HashMap::new();
        params.insert("name".to_string(), json!("custom"));

        let result = template.instantiate(params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_replace_placeholders() {
        let workflow = Workflow::new("Test Workflow".to_string());
        let template = WorkflowTemplate::new("test_template", workflow);

        let mut params = HashMap::new();
        params.insert("api_key".to_string(), json!("secret123"));
        params.insert("timeout".to_string(), json!(60));

        let text = "API Key: {{param.api_key}}, Timeout: {{param.timeout}}";
        let result = template.replace_placeholders(text, &params);

        assert!(result.contains("secret123"));
        assert!(result.contains("60"));
    }
}
