//! YAML serialization and deserialization for workflows
//!
//! This module provides utilities for reading and writing workflows
//! in YAML format, which is commonly used for configuration files.

use crate::{Workflow, WorkflowTemplate};
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during YAML operations
#[derive(Debug, Error)]
pub enum YamlError {
    /// YAML serialization error
    #[error("YAML serialization error: {0}")]
    Serialization(#[from] serde_yaml::Error),

    /// File I/O error
    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid YAML format
    #[error("Invalid YAML format: {0}")]
    InvalidFormat(String),
}

/// Serialize a workflow to a YAML string
pub fn workflow_to_yaml(workflow: &Workflow) -> Result<String, YamlError> {
    serde_yaml::to_string(workflow).map_err(YamlError::from)
}

/// Deserialize a workflow from a YAML string
pub fn workflow_from_yaml(yaml: &str) -> Result<Workflow, YamlError> {
    serde_yaml::from_str(yaml).map_err(YamlError::from)
}

/// Save a workflow to a YAML file
pub fn save_workflow_yaml<P: AsRef<Path>>(workflow: &Workflow, path: P) -> Result<(), YamlError> {
    let yaml = workflow_to_yaml(workflow)?;
    fs::write(path, yaml).map_err(YamlError::from)
}

/// Load a workflow from a YAML file
pub fn load_workflow_yaml<P: AsRef<Path>>(path: P) -> Result<Workflow, YamlError> {
    let yaml = fs::read_to_string(path)?;
    workflow_from_yaml(&yaml)
}

/// Serialize a workflow template to a YAML string
pub fn template_to_yaml(template: &WorkflowTemplate) -> Result<String, YamlError> {
    serde_yaml::to_string(template).map_err(YamlError::from)
}

/// Deserialize a workflow template from a YAML string
pub fn template_from_yaml(yaml: &str) -> Result<WorkflowTemplate, YamlError> {
    serde_yaml::from_str(yaml).map_err(YamlError::from)
}

/// Save a workflow template to a YAML file
pub fn save_template_yaml<P: AsRef<Path>>(
    template: &WorkflowTemplate,
    path: P,
) -> Result<(), YamlError> {
    let yaml = template_to_yaml(template)?;
    fs::write(path, yaml).map_err(YamlError::from)
}

/// Load a workflow template from a YAML file
pub fn load_template_yaml<P: AsRef<Path>>(path: P) -> Result<WorkflowTemplate, YamlError> {
    let yaml = fs::read_to_string(path)?;
    template_from_yaml(&yaml)
}

/// Convert a workflow from JSON to YAML
pub fn json_to_yaml(json: &str) -> Result<String, YamlError> {
    let workflow: Workflow = serde_json::from_str(json)
        .map_err(|e| YamlError::InvalidFormat(format!("Invalid JSON: {}", e)))?;
    workflow_to_yaml(&workflow)
}

/// Convert a workflow from YAML to JSON
pub fn yaml_to_json(yaml: &str) -> Result<String, YamlError> {
    let workflow = workflow_from_yaml(yaml)?;
    serde_json::to_string_pretty(&workflow)
        .map_err(|e| YamlError::InvalidFormat(format!("JSON serialization failed: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Node, NodeKind};
    use tempfile::NamedTempFile;

    #[test]
    fn test_workflow_to_yaml() {
        let mut workflow = Workflow::new("Test Workflow".to_string());

        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start_node.id;
        workflow.add_node(start_node);

        let end_node = Node::new("End".to_string(), NodeKind::End);
        let end_id = end_node.id;
        workflow.add_node(end_node);

        workflow.add_edge(Edge::new(start_id, end_id));

        let yaml = workflow_to_yaml(&workflow).unwrap();

        // Check that YAML contains expected fields
        assert!(yaml.contains("name: Test Workflow"));
        assert!(yaml.contains("nodes:"));
        assert!(yaml.contains("edges:"));
    }

    #[test]
    fn test_workflow_from_yaml() {
        let yaml = r#"
id: 550e8400-e29b-41d4-a716-446655440000
metadata:
  id: 550e8400-e29b-41d4-a716-446655440000
  name: Test Workflow
  description: A test workflow
  version: "1.0.0"
  created_at: "2026-01-01T00:00:00Z"
  updated_at: "2026-01-01T00:00:00Z"
  tags:
    - test
nodes:
  - id: 550e8400-e29b-41d4-a716-446655440001
    name: Start
    kind:
      type: Start
    position: null
    retry_config: null
    timeout_config: null
  - id: 550e8400-e29b-41d4-a716-446655440002
    name: End
    kind:
      type: End
    position: null
    retry_config: null
    timeout_config: null
edges:
  - id: 550e8400-e29b-41d4-a716-446655440003
    from: 550e8400-e29b-41d4-a716-446655440001
    to: 550e8400-e29b-41d4-a716-446655440002
"#;

        let workflow = workflow_from_yaml(yaml).unwrap();

        assert_eq!(workflow.metadata.name, "Test Workflow");
        assert_eq!(
            workflow.metadata.description,
            Some("A test workflow".to_string())
        );
        assert_eq!(workflow.metadata.version, "1.0.0");
        assert_eq!(workflow.nodes.len(), 2);
        assert_eq!(workflow.edges.len(), 1);
    }

    #[test]
    fn test_save_and_load_workflow_yaml() {
        let mut workflow = Workflow::new("File Test".to_string());

        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        workflow.add_node(start_node);

        // Create a temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();

        // Save workflow
        save_workflow_yaml(&workflow, &temp_path).unwrap();

        // Load workflow
        let loaded_workflow = load_workflow_yaml(&temp_path).unwrap();

        assert_eq!(loaded_workflow.metadata.name, "File Test");
        assert_eq!(loaded_workflow.nodes.len(), 1);
    }

    #[test]
    fn test_json_to_yaml_conversion() {
        let json = r#"{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "metadata": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "Conversion Test",
    "description": null,
    "version": "1.0.0",
    "created_at": "2026-01-01T00:00:00Z",
    "updated_at": "2026-01-01T00:00:00Z",
    "tags": []
  },
  "nodes": [],
  "edges": []
}"#;

        let yaml = json_to_yaml(json).unwrap();

        assert!(yaml.contains("name: Conversion Test"));
        assert!(yaml.contains("version:"));
    }

    #[test]
    fn test_yaml_to_json_conversion() {
        let yaml = r#"
id: 550e8400-e29b-41d4-a716-446655440000
metadata:
  id: 550e8400-e29b-41d4-a716-446655440000
  name: YAML Test
  description: null
  version: "1.0.0"
  created_at: "2026-01-01T00:00:00Z"
  updated_at: "2026-01-01T00:00:00Z"
  tags: []
nodes: []
edges: []
"#;

        let json = yaml_to_json(yaml).unwrap();

        assert!(json.contains("YAML Test"));
        assert!(json.contains("version"));
    }

    #[test]
    fn test_roundtrip_yaml_serialization() {
        let mut workflow = Workflow::new("Roundtrip Test".to_string());
        workflow.metadata.description = Some("Test description".to_string());
        workflow.metadata.tags.push("tag1".to_string());

        let start_node = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start_node.id;
        workflow.add_node(start_node);

        let end_node = Node::new("End".to_string(), NodeKind::End);
        let end_id = end_node.id;
        workflow.add_node(end_node);

        workflow.add_edge(Edge::new(start_id, end_id));

        // Serialize to YAML
        let yaml = workflow_to_yaml(&workflow).unwrap();

        // Deserialize from YAML
        let loaded = workflow_from_yaml(&yaml).unwrap();

        // Verify data integrity
        assert_eq!(loaded.metadata.name, workflow.metadata.name);
        assert_eq!(loaded.metadata.description, workflow.metadata.description);
        assert_eq!(loaded.metadata.tags, workflow.metadata.tags);
        assert_eq!(loaded.nodes.len(), workflow.nodes.len());
        assert_eq!(loaded.edges.len(), workflow.edges.len());
    }

    #[test]
    fn test_template_yaml_serialization() {
        use crate::{ParameterType, TemplateParameter, WorkflowTemplate};

        let mut template = WorkflowTemplate {
            id: uuid::Uuid::new_v4(),
            name: "Test Template".to_string(),
            description: Some("A test template".to_string()),
            version: "1.0.0".to_string(),
            author: Some("Test Author".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            category: Some("testing".to_string()),
            tags: vec!["test".to_string()],
            parameters: vec![],
            workflow_json: "{}".to_string(),
            usage_count: 0,
            is_public: true,
            owner_id: Some(uuid::Uuid::new_v4()),
        };

        template.parameters.push(TemplateParameter {
            name: "test_param".to_string(),
            label: "Test Parameter".to_string(),
            description: Some("A test parameter".to_string()),
            param_type: ParameterType::String,
            required: true,
            default_value: None,
            validation: None,
            allowed_values: vec![],
            group: None,
            order: 0,
        });

        let yaml = template_to_yaml(&template).unwrap();
        assert!(yaml.contains("Test Template"));
        assert!(yaml.contains("test_param"));

        let loaded = template_from_yaml(&yaml).unwrap();
        assert_eq!(loaded.name, template.name);
        assert_eq!(loaded.parameters.len(), 1);
    }

    #[test]
    fn test_invalid_yaml() {
        let invalid_yaml = "this is not: valid: yaml:::::";
        let result = workflow_from_yaml(invalid_yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_yaml_error_display() {
        let invalid_yaml = "invalid: yaml: structure: {{{";
        let result = workflow_from_yaml(invalid_yaml);

        match result {
            Err(e) => {
                let error_msg = format!("{}", e);
                assert!(error_msg.contains("YAML serialization error"));
            }
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }
}
