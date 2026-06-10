//! JSON Schema generation for workflow models
//!
//! This module provides functionality to generate JSON Schema documents
//! from workflow models for validation, documentation, and integration.

use crate::{NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// JSON Schema document
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct JsonSchema {
    /// Schema version (always "<https://json-schema.org/draft/2020-12/schema>")
    #[serde(rename = "$schema")]
    pub schema: String,

    /// Schema identifier
    #[serde(rename = "$id", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Schema title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Schema description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Type of the schema
    #[serde(rename = "type")]
    pub schema_type: String,

    /// Properties for object types
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, JsonSchema>>,

    /// Required property names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,

    /// Items schema for array types
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    /// Enum values for enums
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,

    /// Additional properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<Box<JsonSchema>>,

    /// Schema definitions
    #[serde(rename = "$defs", skip_serializing_if = "Option::is_none")]
    pub definitions: Option<HashMap<String, JsonSchema>>,

    /// Minimum value for numbers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    /// Maximum value for numbers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,

    /// Pattern for strings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// Format for strings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// One of (union types)
    #[serde(rename = "oneOf", skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<JsonSchema>>,

    /// Any of (union types with overlap)
    #[serde(rename = "anyOf", skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<JsonSchema>>,

    /// All of (intersection types)
    #[serde(rename = "allOf", skip_serializing_if = "Option::is_none")]
    pub all_of: Option<Vec<JsonSchema>>,

    /// Reference to another schema
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

impl JsonSchema {
    /// Create a new empty schema
    pub fn new(schema_type: &str) -> Self {
        Self {
            schema: "https://json-schema.org/draft/2020-12/schema".to_string(),
            id: None,
            title: None,
            description: None,
            schema_type: schema_type.to_string(),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            additional_properties: None,
            definitions: None,
            minimum: None,
            maximum: None,
            pattern: None,
            format: None,
            one_of: None,
            any_of: None,
            all_of: None,
            reference: None,
        }
    }

    /// Create a string schema
    pub fn string() -> Self {
        Self::new("string")
    }

    /// Create a number schema
    pub fn number() -> Self {
        Self::new("number")
    }

    /// Create an integer schema
    pub fn integer() -> Self {
        Self::new("integer")
    }

    /// Create a boolean schema
    pub fn boolean() -> Self {
        Self::new("boolean")
    }

    /// Create an array schema
    pub fn array(items: JsonSchema) -> Self {
        let mut schema = Self::new("array");
        schema.items = Some(Box::new(items));
        schema
    }

    /// Create an object schema
    pub fn object() -> Self {
        let mut schema = Self::new("object");
        schema.properties = Some(HashMap::new());
        schema
    }

    /// Create a reference schema
    pub fn reference(ref_path: &str) -> Self {
        let mut schema = Self::new("object");
        schema.reference = Some(ref_path.to_string());
        schema
    }

    /// Set title
    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    /// Set description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Set ID
    pub fn with_id(mut self, id: String) -> Self {
        self.id = Some(id);
        self
    }

    /// Add a property to an object schema
    pub fn add_property(&mut self, name: String, schema: JsonSchema) {
        if self.properties.is_none() {
            self.properties = Some(HashMap::new());
        }
        if let Some(props) = &mut self.properties {
            props.insert(name, schema);
        }
    }

    /// Add a required property
    pub fn add_required(&mut self, name: String) {
        if self.required.is_none() {
            self.required = Some(Vec::new());
        }
        if let Some(req) = &mut self.required {
            req.push(name);
        }
    }

    /// Set enum values
    pub fn with_enum(mut self, values: Vec<Value>) -> Self {
        self.enum_values = Some(values);
        self
    }

    /// Set pattern for strings
    pub fn with_pattern(mut self, pattern: String) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Set format for strings
    pub fn with_format(mut self, format: String) -> Self {
        self.format = Some(format);
        self
    }

    /// Add a definition
    pub fn add_definition(&mut self, name: String, schema: JsonSchema) {
        if self.definitions.is_none() {
            self.definitions = Some(HashMap::new());
        }
        if let Some(defs) = &mut self.definitions {
            defs.insert(name, schema);
        }
    }
}

/// Schema generator for workflows
pub struct WorkflowSchemaGenerator {
    /// Include optional fields
    pub include_optional: bool,

    /// Include examples
    pub include_examples: bool,

    /// Include descriptions
    pub include_descriptions: bool,
}

impl WorkflowSchemaGenerator {
    /// Create a new schema generator with default settings
    pub fn new() -> Self {
        Self {
            include_optional: true,
            include_examples: false,
            include_descriptions: true,
        }
    }

    /// Generate JSON Schema for a workflow
    pub fn generate_workflow_schema(&self) -> JsonSchema {
        let mut schema = JsonSchema::object()
            .with_id("https://oxify.dev/schemas/workflow.json".to_string())
            .with_title("Workflow".to_string());

        if self.include_descriptions {
            schema =
                schema.with_description("A workflow defining a sequence of operations".to_string());
        }

        // Add basic properties
        schema.add_property(
            "id".to_string(),
            JsonSchema::string().with_format("uuid".to_string()),
        );
        schema.add_required("id".to_string());

        schema.add_property("metadata".to_string(), self.generate_metadata_schema());
        schema.add_required("metadata".to_string());

        schema.add_property(
            "nodes".to_string(),
            JsonSchema::array(self.generate_node_schema()),
        );
        schema.add_required("nodes".to_string());

        schema.add_property(
            "edges".to_string(),
            JsonSchema::array(self.generate_edge_schema()),
        );
        schema.add_required("edges".to_string());

        // Add node type definitions
        self.add_node_type_definitions(&mut schema);

        schema
    }

    /// Generate schema for workflow metadata
    fn generate_metadata_schema(&self) -> JsonSchema {
        let mut schema = JsonSchema::object();

        if self.include_descriptions {
            schema = schema.with_description("Workflow metadata".to_string());
        }

        schema.add_property("name".to_string(), JsonSchema::string());
        schema.add_required("name".to_string());

        schema.add_property("description".to_string(), JsonSchema::string());

        schema.add_property("version".to_string(), JsonSchema::string());
        schema.add_required("version".to_string());

        schema.add_property(
            "created_at".to_string(),
            JsonSchema::string().with_format("date-time".to_string()),
        );
        schema.add_required("created_at".to_string());

        schema.add_property(
            "updated_at".to_string(),
            JsonSchema::string().with_format("date-time".to_string()),
        );
        schema.add_required("updated_at".to_string());

        schema.add_property("tags".to_string(), JsonSchema::array(JsonSchema::string()));

        schema
    }

    /// Generate schema for a node
    fn generate_node_schema(&self) -> JsonSchema {
        let mut schema = JsonSchema::object();

        if self.include_descriptions {
            schema = schema.with_description("A workflow node".to_string());
        }

        schema.add_property(
            "id".to_string(),
            JsonSchema::string().with_format("uuid".to_string()),
        );
        schema.add_required("id".to_string());

        schema.add_property("name".to_string(), JsonSchema::string());
        schema.add_required("name".to_string());

        schema.add_property("kind".to_string(), self.generate_node_kind_schema());
        schema.add_required("kind".to_string());

        schema
    }

    /// Generate schema for node kind
    fn generate_node_kind_schema(&self) -> JsonSchema {
        JsonSchema::string().with_enum(vec![
            json!("Start"),
            json!("End"),
            json!("LLM"),
            json!("Retriever"),
            json!("Code"),
            json!("IfElse"),
            json!("Tool"),
            json!("Loop"),
            json!("TryCatch"),
            json!("SubWorkflow"),
            json!("Switch"),
            json!("Parallel"),
            json!("Approval"),
            json!("Form"),
            json!("Vision"),
            json!("Custom"),
        ])
    }

    /// Generate schema for an edge
    fn generate_edge_schema(&self) -> JsonSchema {
        let mut schema = JsonSchema::object();

        if self.include_descriptions {
            schema = schema.with_description("A workflow edge connecting two nodes".to_string());
        }

        schema.add_property(
            "id".to_string(),
            JsonSchema::string().with_format("uuid".to_string()),
        );
        schema.add_required("id".to_string());

        schema.add_property(
            "from".to_string(),
            JsonSchema::string().with_format("uuid".to_string()),
        );
        schema.add_required("from".to_string());

        schema.add_property(
            "to".to_string(),
            JsonSchema::string().with_format("uuid".to_string()),
        );
        schema.add_required("to".to_string());

        schema
    }

    /// Add node type definitions to schema
    fn add_node_type_definitions(&self, schema: &mut JsonSchema) {
        // LLM config
        let mut llm_config = JsonSchema::object();
        llm_config.add_property("model".to_string(), JsonSchema::string());
        llm_config.add_property("prompt".to_string(), JsonSchema::string());
        llm_config.add_property("temperature".to_string(), JsonSchema::number());
        llm_config.add_property("max_tokens".to_string(), JsonSchema::integer());
        schema.add_definition("LlmConfig".to_string(), llm_config);

        // Script config
        let mut script_config = JsonSchema::object();
        script_config.add_property(
            "language".to_string(),
            JsonSchema::string().with_enum(vec![
                json!("Python"),
                json!("JavaScript"),
                json!("TypeScript"),
                json!("Bash"),
            ]),
        );
        script_config.add_property("code".to_string(), JsonSchema::string());
        schema.add_definition("ScriptConfig".to_string(), script_config);

        // Condition
        let mut condition = JsonSchema::object();
        condition.add_property("expression".to_string(), JsonSchema::string());
        schema.add_definition("Condition".to_string(), condition);

        // CustomConfig: plugin dispatch node
        let mut custom_config = JsonSchema::object();
        custom_config.add_property("plugin_id".to_string(), JsonSchema::string());
        custom_config.add_required("plugin_id".to_string());
        custom_config.add_property("plugin_version".to_string(), JsonSchema::string());
        custom_config.add_property("config".to_string(), JsonSchema::object());
        schema.add_definition("CustomConfig".to_string(), custom_config);
    }

    /// Generate schema for a specific node type
    pub fn generate_node_type_schema(&self, node_kind: &NodeKind) -> JsonSchema {
        match node_kind {
            NodeKind::Start | NodeKind::End => {
                JsonSchema::object().with_description(format!("{:?} node", node_kind))
            }
            NodeKind::LLM(_) => JsonSchema::reference("#/$defs/LlmConfig"),
            NodeKind::Code(_) => JsonSchema::reference("#/$defs/ScriptConfig"),
            NodeKind::Custom(_) => JsonSchema::reference("#/$defs/CustomConfig"),
            NodeKind::Retriever(_)
            | NodeKind::IfElse(_)
            | NodeKind::Tool(_)
            | NodeKind::Loop(_)
            | NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Switch(_)
            | NodeKind::Parallel(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_) => {
                JsonSchema::object().with_description(format!("{:?} node configuration", node_kind))
            }
        }
    }

    /// Validate a workflow against the schema
    pub fn validate_workflow(&self, workflow: &Workflow) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Basic validation
        if workflow.nodes.is_empty() {
            errors.push("Workflow must have at least one node".to_string());
        }

        // Check for start node
        if !workflow
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Start))
        {
            errors.push("Workflow must have a Start node".to_string());
        }

        // Check for end node
        if !workflow
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::End))
        {
            errors.push("Workflow must have an End node".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for WorkflowSchemaGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a JSON Schema for a workflow
pub fn generate_workflow_schema() -> JsonSchema {
    WorkflowSchemaGenerator::new().generate_workflow_schema()
}

/// Export schema as JSON string
pub fn schema_to_json(schema: &JsonSchema) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(schema)
}

/// Export schema as JSON value
pub fn schema_to_value(schema: &JsonSchema) -> Result<Value, serde_json::Error> {
    serde_json::to_value(schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Node, NodeKind};

    #[test]
    fn test_create_basic_schema() {
        let schema = JsonSchema::string();
        assert_eq!(schema.schema_type, "string");
        assert_eq!(
            schema.schema,
            "https://json-schema.org/draft/2020-12/schema"
        );
    }

    #[test]
    fn test_create_object_schema() {
        let mut schema = JsonSchema::object();
        schema.add_property("name".to_string(), JsonSchema::string());
        schema.add_required("name".to_string());

        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.is_some());
        assert_eq!(schema.properties.as_ref().unwrap().len(), 1);
        assert_eq!(schema.required.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_create_array_schema() {
        let schema = JsonSchema::array(JsonSchema::string());
        assert_eq!(schema.schema_type, "array");
        assert!(schema.items.is_some());
    }

    #[test]
    fn test_enum_schema() {
        let schema = JsonSchema::string().with_enum(vec![
            json!("option1"),
            json!("option2"),
            json!("option3"),
        ]);

        assert!(schema.enum_values.is_some());
        assert_eq!(schema.enum_values.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_generate_workflow_schema() {
        let generator = WorkflowSchemaGenerator::new();
        let schema = generator.generate_workflow_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.is_some());
        assert!(schema.required.is_some());

        let props = schema.properties.as_ref().unwrap();
        assert!(props.contains_key("id"));
        assert!(props.contains_key("metadata"));
        assert!(props.contains_key("nodes"));
        assert!(props.contains_key("edges"));

        let required = schema.required.as_ref().unwrap();
        assert!(required.contains(&"id".to_string()));
        assert!(required.contains(&"metadata".to_string()));
        assert!(required.contains(&"nodes".to_string()));
        assert!(required.contains(&"edges".to_string()));
    }

    #[test]
    fn test_schema_serialization() {
        let schema = JsonSchema::string()
            .with_title("Name".to_string())
            .with_description("A person's name".to_string());

        let json = schema_to_json(&schema).unwrap();
        assert!(json.contains("Name"));
        assert!(json.contains("person's name"));
    }

    #[test]
    fn test_validate_workflow_missing_start() {
        let mut workflow = Workflow::new("Test".to_string());
        let end_node = Node::new("End".to_string(), NodeKind::End);
        workflow.add_node(end_node);

        let generator = WorkflowSchemaGenerator::new();
        let result = generator.validate_workflow(&workflow);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Start")));
    }

    #[test]
    fn test_validate_workflow_missing_end() {
        let mut workflow = Workflow::new("Test".to_string());
        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        workflow.add_node(start_node);

        let generator = WorkflowSchemaGenerator::new();
        let result = generator.validate_workflow(&workflow);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("End")));
    }

    #[test]
    fn test_validate_valid_workflow() {
        let mut workflow = Workflow::new("Test".to_string());

        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start_node.id;
        workflow.add_node(start_node);

        let end_node = Node::new("End".to_string(), NodeKind::End);
        let end_id = end_node.id;
        workflow.add_node(end_node);

        workflow.add_edge(Edge::new(start_id, end_id));

        let generator = WorkflowSchemaGenerator::new();
        let result = generator.validate_workflow(&workflow);

        assert!(result.is_ok());
    }

    #[test]
    fn test_node_kind_schema() {
        let generator = WorkflowSchemaGenerator::new();
        let schema = generator.generate_node_kind_schema();

        assert_eq!(schema.schema_type, "string");
        assert!(schema.enum_values.is_some());

        let enums = schema.enum_values.as_ref().unwrap();
        assert!(enums.contains(&json!("Start")));
        assert!(enums.contains(&json!("End")));
        assert!(enums.contains(&json!("LLM")));
    }

    #[test]
    fn test_metadata_schema() {
        let generator = WorkflowSchemaGenerator::new();
        let schema = generator.generate_metadata_schema();

        assert_eq!(schema.schema_type, "object");
        let props = schema.properties.as_ref().unwrap();
        assert!(props.contains_key("name"));
        assert!(props.contains_key("version"));
        assert!(props.contains_key("created_at"));
        assert!(props.contains_key("updated_at"));
    }

    #[test]
    fn test_reference_schema() {
        let schema = JsonSchema::reference("#/$defs/LlmConfig");
        assert!(schema.reference.is_some());
        assert_eq!(schema.reference.unwrap(), "#/$defs/LlmConfig");
    }

    #[test]
    fn test_schema_with_pattern() {
        let schema = JsonSchema::string().with_pattern("^[a-zA-Z0-9_-]+$".to_string());

        assert!(schema.pattern.is_some());
        assert_eq!(schema.pattern.unwrap(), "^[a-zA-Z0-9_-]+$");
    }

    #[test]
    fn test_schema_with_format() {
        let schema = JsonSchema::string().with_format("uuid".to_string());

        assert!(schema.format.is_some());
        assert_eq!(schema.format.unwrap(), "uuid");
    }

    #[test]
    fn test_schema_definitions() {
        let mut schema = JsonSchema::object();
        schema.add_definition("CustomType".to_string(), JsonSchema::string());

        assert!(schema.definitions.is_some());
        assert_eq!(schema.definitions.as_ref().unwrap().len(), 1);
    }
}
