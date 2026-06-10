//! Webhook payload transformation
//!
//! Transform incoming webhook payloads before passing to workflows.
//!
//! # Example
//!
//! ```rust
//! use oxify_model::{
//!     PayloadTransform, TransformOperation, FieldMapping,
//!     TransformPipeline, PayloadTransformError,
//! };
//! use serde_json::json;
//!
//! // Create a transform that extracts and renames fields
//! let transform = PayloadTransform::new()
//!     .extract_field("$.data.user", "user")
//!     .rename_field("$.action", "event_type")
//!     .add_default("source", json!("webhook"))
//!     .filter_fields(&["user", "event_type", "source"]);
//!
//! // Apply the transform
//! let input = json!({
//!     "action": "user.created",
//!     "data": {
//!         "user": { "id": 123, "name": "Alice" }
//!     }
//! });
//!
//! let result = transform.apply(&input).unwrap();
//! // result = { "user": { "id": 123, "name": "Alice" }, "event_type": "user.created", "source": "webhook" }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Error type for payload transformation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadTransformError {
    /// JSONPath not found in payload
    PathNotFound(String),
    /// Invalid JSONPath expression
    InvalidPath(String),
    /// Value transformation failed
    TransformFailed(String),
    /// Type conversion error
    TypeConversionError(String),
    /// Invalid operation configuration
    InvalidOperation(String),
}

impl std::fmt::Display for PayloadTransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadTransformError::PathNotFound(path) => {
                write!(f, "Path not found in payload: {}", path)
            }
            PayloadTransformError::InvalidPath(path) => {
                write!(f, "Invalid JSONPath expression: {}", path)
            }
            PayloadTransformError::TransformFailed(msg) => {
                write!(f, "Transform failed: {}", msg)
            }
            PayloadTransformError::TypeConversionError(msg) => {
                write!(f, "Type conversion error: {}", msg)
            }
            PayloadTransformError::InvalidOperation(msg) => {
                write!(f, "Invalid operation: {}", msg)
            }
        }
    }
}

impl std::error::Error for PayloadTransformError {}

/// A single transformation operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformOperation {
    /// Extract a value from a path and store in output
    Extract {
        /// JSONPath-like source path (e.g., "$.data.user.id")
        source_path: String,
        /// Target field name in output
        target_field: String,
        /// If true, skip if source not found (otherwise error)
        optional: bool,
        /// Default value if not found (only used if optional)
        default: Option<Value>,
    },

    /// Rename a field
    Rename {
        /// Original field path
        from: String,
        /// New field name
        to: String,
    },

    /// Add a constant value to the output
    AddConstant {
        /// Target field name
        field: String,
        /// Constant value to add
        value: Value,
    },

    /// Remove specific fields from the payload
    RemoveFields {
        /// Fields to remove
        fields: Vec<String>,
    },

    /// Keep only specific fields (whitelist)
    FilterFields {
        /// Fields to keep
        fields: Vec<String>,
    },

    /// Transform a string value using a pattern
    StringTransform {
        /// Source field path
        source_path: String,
        /// Target field name
        target_field: String,
        /// Transform type
        transform: StringTransformType,
    },

    /// Map values to other values
    MapValue {
        /// Source field path
        source_path: String,
        /// Target field name
        target_field: String,
        /// Value mappings (from -> to)
        mappings: HashMap<String, Value>,
        /// Default value if no mapping matches
        default: Option<Value>,
    },

    /// Template string with variable substitution
    Template {
        /// Target field name
        target_field: String,
        /// Template string with {{path}} placeholders
        template: String,
    },

    /// Flatten nested object
    Flatten {
        /// Source path to the nested object
        source_path: String,
        /// Prefix to add to flattened keys
        prefix: Option<String>,
        /// Separator between prefix and key
        separator: String,
    },

    /// Wrap payload in a new structure
    Wrap {
        /// Field name to wrap payload under
        wrapper_field: String,
    },

    /// Custom transformation using an expression
    Custom {
        /// Target field name
        target_field: String,
        /// Expression (placeholder for future expression engine)
        expression: String,
    },
}

/// String transformation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StringTransformType {
    /// Convert to uppercase
    Uppercase,
    /// Convert to lowercase
    Lowercase,
    /// Trim whitespace
    Trim,
    /// Replace substring
    Replace { from: String, to: String },
    /// Extract using regex (first capture group)
    Regex { pattern: String },
    /// Split and take nth element
    Split { delimiter: String, index: usize },
    /// Prefix with string
    Prefix { prefix: String },
    /// Suffix with string
    Suffix { suffix: String },
}

/// Field mapping for bulk rename/extract operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FieldMapping {
    /// Source path
    pub source: String,
    /// Target field name
    pub target: String,
    /// Whether this mapping is optional
    pub optional: bool,
    /// Default value if optional and not found
    pub default: Option<Value>,
}

/// Payload transformation configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PayloadTransform {
    /// Ordered list of transformation operations
    pub operations: Vec<TransformOperation>,
    /// Whether to start with empty object (true) or clone input (false)
    pub start_empty: bool,
    /// Fail on any error (true) or continue and skip failed operations (false)
    pub strict: bool,
}

impl PayloadTransform {
    /// Create a new empty transform
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            start_empty: false,
            strict: true,
        }
    }

    /// Create a transform that starts with an empty object
    pub fn empty() -> Self {
        Self {
            operations: Vec::new(),
            start_empty: true,
            strict: true,
        }
    }

    /// Set strict mode
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Add an extract operation
    pub fn extract_field(mut self, source_path: &str, target_field: &str) -> Self {
        self.operations.push(TransformOperation::Extract {
            source_path: source_path.to_string(),
            target_field: target_field.to_string(),
            optional: false,
            default: None,
        });
        self
    }

    /// Add an optional extract operation with default
    pub fn extract_field_or(
        mut self,
        source_path: &str,
        target_field: &str,
        default: Value,
    ) -> Self {
        self.operations.push(TransformOperation::Extract {
            source_path: source_path.to_string(),
            target_field: target_field.to_string(),
            optional: true,
            default: Some(default),
        });
        self
    }

    /// Add a rename operation
    pub fn rename_field(mut self, from: &str, to: &str) -> Self {
        self.operations.push(TransformOperation::Rename {
            from: from.to_string(),
            to: to.to_string(),
        });
        self
    }

    /// Add a constant value
    pub fn add_default(mut self, field: &str, value: Value) -> Self {
        self.operations.push(TransformOperation::AddConstant {
            field: field.to_string(),
            value,
        });
        self
    }

    /// Remove specific fields
    pub fn remove_fields(mut self, fields: &[&str]) -> Self {
        self.operations.push(TransformOperation::RemoveFields {
            fields: fields.iter().map(|s| s.to_string()).collect(),
        });
        self
    }

    /// Keep only specific fields
    pub fn filter_fields(mut self, fields: &[&str]) -> Self {
        self.operations.push(TransformOperation::FilterFields {
            fields: fields.iter().map(|s| s.to_string()).collect(),
        });
        self
    }

    /// Add a string transform operation
    pub fn string_transform(
        mut self,
        source_path: &str,
        target_field: &str,
        transform: StringTransformType,
    ) -> Self {
        self.operations.push(TransformOperation::StringTransform {
            source_path: source_path.to_string(),
            target_field: target_field.to_string(),
            transform,
        });
        self
    }

    /// Add a value mapping operation
    pub fn map_value(
        mut self,
        source_path: &str,
        target_field: &str,
        mappings: HashMap<String, Value>,
        default: Option<Value>,
    ) -> Self {
        self.operations.push(TransformOperation::MapValue {
            source_path: source_path.to_string(),
            target_field: target_field.to_string(),
            mappings,
            default,
        });
        self
    }

    /// Add a template operation
    pub fn template(mut self, target_field: &str, template: &str) -> Self {
        self.operations.push(TransformOperation::Template {
            target_field: target_field.to_string(),
            template: template.to_string(),
        });
        self
    }

    /// Add a wrap operation
    pub fn wrap(mut self, wrapper_field: &str) -> Self {
        self.operations.push(TransformOperation::Wrap {
            wrapper_field: wrapper_field.to_string(),
        });
        self
    }

    /// Add a flatten operation
    pub fn flatten(mut self, source_path: &str, prefix: Option<&str>, separator: &str) -> Self {
        self.operations.push(TransformOperation::Flatten {
            source_path: source_path.to_string(),
            prefix: prefix.map(|s| s.to_string()),
            separator: separator.to_string(),
        });
        self
    }

    /// Add a custom operation
    pub fn add_operation(mut self, operation: TransformOperation) -> Self {
        self.operations.push(operation);
        self
    }

    /// Apply the transformation to a payload
    pub fn apply(&self, input: &Value) -> Result<Value, PayloadTransformError> {
        let mut result = if self.start_empty {
            Value::Object(serde_json::Map::new())
        } else {
            input.clone()
        };

        for operation in &self.operations {
            match self.apply_operation(&mut result, input, operation) {
                Ok(()) => {}
                Err(e) if self.strict => return Err(e),
                Err(_) => continue, // Skip failed operations in non-strict mode
            }
        }

        Ok(result)
    }

    fn apply_operation(
        &self,
        result: &mut Value,
        input: &Value,
        operation: &TransformOperation,
    ) -> Result<(), PayloadTransformError> {
        match operation {
            TransformOperation::Extract {
                source_path,
                target_field,
                optional,
                default,
            } => {
                let value = get_value_by_path(input, source_path);
                match value {
                    Some(v) => set_field(result, target_field, v.clone()),
                    None if *optional => {
                        if let Some(def) = default {
                            set_field(result, target_field, def.clone());
                        }
                    }
                    None => return Err(PayloadTransformError::PathNotFound(source_path.clone())),
                }
            }

            TransformOperation::Rename { from, to } => {
                if let Some(value) = get_value_by_path(result, from) {
                    let v = value.clone();
                    remove_field(result, from);
                    set_field(result, to, v);
                }
            }

            TransformOperation::AddConstant { field, value } => {
                set_field(result, field, value.clone());
            }

            TransformOperation::RemoveFields { fields } => {
                for field in fields {
                    remove_field(result, field);
                }
            }

            TransformOperation::FilterFields { fields } => {
                if let Value::Object(map) = result {
                    let fields_set: std::collections::HashSet<_> = fields.iter().collect();
                    map.retain(|k, _| fields_set.contains(k));
                }
            }

            TransformOperation::StringTransform {
                source_path,
                target_field,
                transform,
            } => {
                if let Some(Value::String(s)) = get_value_by_path(input, source_path) {
                    let transformed = apply_string_transform(s, transform)?;
                    set_field(result, target_field, Value::String(transformed));
                }
            }

            TransformOperation::MapValue {
                source_path,
                target_field,
                mappings,
                default,
            } => {
                if let Some(value) = get_value_by_path(input, source_path) {
                    let key = match value {
                        Value::String(s) => s.clone(),
                        v => v.to_string(),
                    };
                    if let Some(mapped) = mappings.get(&key) {
                        set_field(result, target_field, mapped.clone());
                    } else if let Some(def) = default {
                        set_field(result, target_field, def.clone());
                    }
                }
            }

            TransformOperation::Template {
                target_field,
                template,
            } => {
                let rendered = render_template(template, input);
                set_field(result, target_field, Value::String(rendered));
            }

            TransformOperation::Flatten {
                source_path,
                prefix,
                separator,
            } => {
                if let Some(Value::Object(map)) = get_value_by_path(input, source_path) {
                    for (key, value) in map {
                        let new_key = match prefix {
                            Some(p) => format!("{}{}{}", p, separator, key),
                            None => key.clone(),
                        };
                        set_field(result, &new_key, value.clone());
                    }
                }
            }

            TransformOperation::Wrap { wrapper_field } => {
                let current = result.clone();
                *result = serde_json::json!({
                    wrapper_field: current
                });
            }

            TransformOperation::Custom {
                target_field,
                expression,
            } => {
                // Placeholder for custom expression evaluation
                // For now, just store the expression as a string
                set_field(result, target_field, Value::String(expression.clone()));
            }
        }

        Ok(())
    }
}

/// Transformation pipeline for chaining multiple transforms
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TransformPipeline {
    /// Name of the pipeline
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Transforms to apply in order
    pub transforms: Vec<PayloadTransform>,
}

impl TransformPipeline {
    /// Create a new pipeline
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            transforms: Vec::new(),
        }
    }

    /// Add description
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Add a transform to the pipeline
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, transform: PayloadTransform) -> Self {
        self.transforms.push(transform);
        self
    }

    /// Apply all transforms in sequence
    pub fn apply(&self, input: &Value) -> Result<Value, PayloadTransformError> {
        let mut result = input.clone();
        for transform in &self.transforms {
            result = transform.apply(&result)?;
        }
        Ok(result)
    }
}

// Helper functions

/// Get a value from a JSONPath-like path
fn get_value_by_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.strip_prefix("$.").unwrap_or(path);
    let parts: Vec<&str> = path.split('.').collect();

    let mut current = value;
    for part in parts {
        if part.is_empty() {
            continue;
        }
        // Handle array index like "items[0]"
        if let Some(idx_start) = part.find('[') {
            let field = &part[..idx_start];
            let idx_end = part.find(']')?;
            let idx: usize = part[idx_start + 1..idx_end].parse().ok()?;

            current = current.get(field)?;
            current = current.get(idx)?;
        } else {
            current = current.get(part)?;
        }
    }

    Some(current)
}

/// Set a field in a JSON object
fn set_field(value: &mut Value, field: &str, new_value: Value) {
    if let Value::Object(map) = value {
        map.insert(field.to_string(), new_value);
    }
}

/// Remove a field from a JSON object
fn remove_field(value: &mut Value, path: &str) {
    let path = path.strip_prefix("$.").unwrap_or(path);
    if let Value::Object(map) = value {
        map.remove(path);
    }
}

/// Apply a string transformation
fn apply_string_transform(
    s: &str,
    transform: &StringTransformType,
) -> Result<String, PayloadTransformError> {
    match transform {
        StringTransformType::Uppercase => Ok(s.to_uppercase()),
        StringTransformType::Lowercase => Ok(s.to_lowercase()),
        StringTransformType::Trim => Ok(s.trim().to_string()),
        StringTransformType::Replace { from, to } => Ok(s.replace(from, to)),
        StringTransformType::Regex { pattern } => {
            let re = regex::Regex::new(pattern)
                .map_err(|e| PayloadTransformError::InvalidOperation(e.to_string()))?;
            if let Some(caps) = re.captures(s) {
                if let Some(m) = caps.get(1) {
                    return Ok(m.as_str().to_string());
                }
            }
            Ok(String::new())
        }
        StringTransformType::Split { delimiter, index } => {
            let parts: Vec<&str> = s.split(delimiter).collect();
            Ok(parts.get(*index).unwrap_or(&"").to_string())
        }
        StringTransformType::Prefix { prefix } => Ok(format!("{}{}", prefix, s)),
        StringTransformType::Suffix { suffix } => Ok(format!("{}{}", s, suffix)),
    }
}

/// Render a template with {{path}} placeholders
fn render_template(template: &str, input: &Value) -> String {
    let re = regex::Regex::new(r"\{\{([^}]+)\}\}").expect("invariant: valid regex literal");
    re.replace_all(template, |caps: &regex::Captures| {
        let path = &caps[1];
        get_value_by_path(input, path)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_field() {
        let transform = PayloadTransform::empty().extract_field("$.data.user", "user");

        let input = json!({
            "data": {
                "user": { "id": 123, "name": "Alice" }
            }
        });

        let result = transform.apply(&input).unwrap();
        assert_eq!(result["user"]["id"], 123);
        assert_eq!(result["user"]["name"], "Alice");
    }

    #[test]
    fn test_extract_optional_with_default() {
        let transform = PayloadTransform::empty().extract_field_or(
            "$.missing.field",
            "value",
            json!("default"),
        );

        let input = json!({ "other": "data" });
        let result = transform.apply(&input).unwrap();
        assert_eq!(result["value"], "default");
    }

    #[test]
    fn test_rename_field() {
        let transform = PayloadTransform::new().rename_field("$.old_name", "new_name");

        let input = json!({
            "old_name": "value",
            "other": "data"
        });

        let result = transform.apply(&input).unwrap();
        assert_eq!(result["new_name"], "value");
        assert!(result.get("old_name").is_none());
    }

    #[test]
    fn test_add_constant() {
        let transform = PayloadTransform::new()
            .add_default("source", json!("webhook"))
            .add_default("version", json!(1));

        let input = json!({ "data": "test" });
        let result = transform.apply(&input).unwrap();
        assert_eq!(result["source"], "webhook");
        assert_eq!(result["version"], 1);
        assert_eq!(result["data"], "test");
    }

    #[test]
    fn test_remove_fields() {
        let transform = PayloadTransform::new().remove_fields(&["secret", "internal"]);

        let input = json!({
            "data": "keep",
            "secret": "remove",
            "internal": "remove"
        });

        let result = transform.apply(&input).unwrap();
        assert_eq!(result["data"], "keep");
        assert!(result.get("secret").is_none());
        assert!(result.get("internal").is_none());
    }

    #[test]
    fn test_filter_fields() {
        let transform = PayloadTransform::new().filter_fields(&["id", "name"]);

        let input = json!({
            "id": 123,
            "name": "test",
            "secret": "hidden",
            "internal": "hidden"
        });

        let result = transform.apply(&input).unwrap();
        assert_eq!(result["id"], 123);
        assert_eq!(result["name"], "test");
        assert!(result.get("secret").is_none());
        assert!(result.get("internal").is_none());
    }

    #[test]
    fn test_string_transform_uppercase() {
        let transform = PayloadTransform::new().string_transform(
            "$.action",
            "action_upper",
            StringTransformType::Uppercase,
        );

        let input = json!({ "action": "user.created" });
        let result = transform.apply(&input).unwrap();
        assert_eq!(result["action_upper"], "USER.CREATED");
    }

    #[test]
    fn test_string_transform_split() {
        let transform = PayloadTransform::new().string_transform(
            "$.action",
            "entity",
            StringTransformType::Split {
                delimiter: ".".to_string(),
                index: 0,
            },
        );

        let input = json!({ "action": "user.created" });
        let result = transform.apply(&input).unwrap();
        assert_eq!(result["entity"], "user");
    }

    #[test]
    fn test_map_value() {
        let mut mappings = HashMap::new();
        mappings.insert("created".to_string(), json!("new"));
        mappings.insert("updated".to_string(), json!("modified"));
        mappings.insert("deleted".to_string(), json!("removed"));

        let transform = PayloadTransform::new().map_value(
            "$.action",
            "status",
            mappings,
            Some(json!("unknown")),
        );

        let input = json!({ "action": "created" });
        let result = transform.apply(&input).unwrap();
        assert_eq!(result["status"], "new");

        let input2 = json!({ "action": "other" });
        let result2 = transform.apply(&input2).unwrap();
        assert_eq!(result2["status"], "unknown");
    }

    #[test]
    fn test_template() {
        let transform = PayloadTransform::new()
            .template("message", "User {{$.user.name}} performed {{$.action}}");

        let input = json!({
            "user": { "name": "Alice" },
            "action": "login"
        });

        let result = transform.apply(&input).unwrap();
        assert_eq!(result["message"], "User Alice performed login");
    }

    #[test]
    fn test_wrap() {
        let transform = PayloadTransform::new().wrap("payload");

        let input = json!({ "data": "test" });
        let result = transform.apply(&input).unwrap();
        assert_eq!(result["payload"]["data"], "test");
    }

    #[test]
    fn test_flatten() {
        let transform = PayloadTransform::new().flatten("$.metadata", Some("meta"), "_");

        let input = json!({
            "id": 1,
            "metadata": {
                "created": "2026-01-01",
                "version": "1.0"
            }
        });

        let result = transform.apply(&input).unwrap();
        assert_eq!(result["meta_created"], "2026-01-01");
        assert_eq!(result["meta_version"], "1.0");
    }

    #[test]
    fn test_complex_pipeline() {
        let transform = PayloadTransform::empty()
            .extract_field("$.repository.full_name", "repo")
            .extract_field("$.sender.login", "actor")
            .extract_field("$.action", "event")
            .add_default("source", json!("github"))
            .string_transform("$.action", "event_type", StringTransformType::Uppercase);

        let input = json!({
            "action": "opened",
            "repository": {
                "full_name": "org/repo"
            },
            "sender": {
                "login": "user123"
            }
        });

        let result = transform.apply(&input).unwrap();
        assert_eq!(result["repo"], "org/repo");
        assert_eq!(result["actor"], "user123");
        assert_eq!(result["event"], "opened");
        assert_eq!(result["source"], "github");
        assert_eq!(result["event_type"], "OPENED");
    }

    #[test]
    fn test_non_strict_mode() {
        let transform = PayloadTransform::new()
            .strict(false)
            .extract_field("$.missing", "value") // This would fail
            .add_default("added", json!("success"));

        let input = json!({ "other": "data" });
        let result = transform.apply(&input).unwrap(); // Should not error
        assert_eq!(result["added"], "success");
        assert_eq!(result["other"], "data");
    }

    #[test]
    fn test_pipeline() {
        let pipeline = TransformPipeline::new("github_webhook")
            .with_description("Transform GitHub webhook payloads")
            .add(
                PayloadTransform::empty()
                    .extract_field("$.repository.name", "repo")
                    .extract_field("$.action", "event"),
            )
            .add(PayloadTransform::new().add_default("processed", json!(true)));

        let input = json!({
            "action": "push",
            "repository": { "name": "test-repo" }
        });

        let result = pipeline.apply(&input).unwrap();
        assert_eq!(result["repo"], "test-repo");
        assert_eq!(result["event"], "push");
        assert_eq!(result["processed"], true);
    }

    #[test]
    fn test_array_index_access() {
        let transform = PayloadTransform::empty().extract_field("$.items[0].name", "first_item");

        let input = json!({
            "items": [
                { "name": "first" },
                { "name": "second" }
            ]
        });

        let result = transform.apply(&input).unwrap();
        assert_eq!(result["first_item"], "first");
    }
}
