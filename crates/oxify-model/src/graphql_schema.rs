//! GraphQL schema generation for workflow models
//!
//! This module provides functionality to generate GraphQL Schema Definition Language (SDL)
//! from workflow models for API integration.

use std::collections::HashMap;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// GraphQL type definition
#[derive(Debug, Clone)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct GraphQLType {
    /// Type name
    pub name: String,

    /// Type kind (Object, Enum, Interface, etc.)
    pub kind: GraphQLTypeKind,

    /// Description
    pub description: Option<String>,

    /// Fields for object types
    pub fields: Vec<GraphQLField>,

    /// Enum values for enum types
    pub enum_values: Vec<String>,

    /// Implemented interfaces
    pub interfaces: Vec<String>,
}

/// GraphQL type kind
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum GraphQLTypeKind {
    /// Object type
    Object,
    /// Enum type
    Enum,
    /// Interface type
    Interface,
    /// Input object type
    Input,
}

/// GraphQL field definition
#[derive(Debug, Clone)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct GraphQLField {
    /// Field name
    pub name: String,

    /// Field type
    pub field_type: String,

    /// Whether the field is required (non-null)
    pub required: bool,

    /// Whether the field is a list
    pub is_list: bool,

    /// Description
    pub description: Option<String>,

    /// Arguments for the field
    pub arguments: Vec<GraphQLArgument>,
}

/// GraphQL argument definition
#[derive(Debug, Clone)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct GraphQLArgument {
    /// Argument name
    pub name: String,

    /// Argument type
    pub arg_type: String,

    /// Whether the argument is required
    pub required: bool,

    /// Default value
    pub default_value: Option<String>,
}

impl GraphQLType {
    /// Create a new GraphQL type
    pub fn new(name: String, kind: GraphQLTypeKind) -> Self {
        Self {
            name,
            kind,
            description: None,
            fields: Vec::new(),
            enum_values: Vec::new(),
            interfaces: Vec::new(),
        }
    }

    /// Set description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Add a field
    pub fn add_field(&mut self, field: GraphQLField) {
        self.fields.push(field);
    }

    /// Add an enum value
    pub fn add_enum_value(&mut self, value: String) {
        self.enum_values.push(value);
    }

    /// Add an interface
    pub fn add_interface(&mut self, interface: String) {
        self.interfaces.push(interface);
    }

    /// Convert to SDL string
    pub fn to_sdl(&self) -> String {
        let mut lines = Vec::new();

        // Add description if present
        if let Some(desc) = &self.description {
            lines.push(format!("\"\"\"{}\"\"\"", desc));
        }

        // Add type definition
        match self.kind {
            GraphQLTypeKind::Object => {
                let interfaces = if self.interfaces.is_empty() {
                    String::new()
                } else {
                    format!(" implements {}", self.interfaces.join(" & "))
                };
                lines.push(format!("type {}{} {{", self.name, interfaces));
                for field in &self.fields {
                    lines.push(format!("  {}", field.to_sdl()));
                }
                lines.push("}".to_string());
            }
            GraphQLTypeKind::Enum => {
                lines.push(format!("enum {} {{", self.name));
                for value in &self.enum_values {
                    lines.push(format!("  {}", value));
                }
                lines.push("}".to_string());
            }
            GraphQLTypeKind::Interface => {
                lines.push(format!("interface {} {{", self.name));
                for field in &self.fields {
                    lines.push(format!("  {}", field.to_sdl()));
                }
                lines.push("}".to_string());
            }
            GraphQLTypeKind::Input => {
                lines.push(format!("input {} {{", self.name));
                for field in &self.fields {
                    lines.push(format!("  {}", field.to_sdl()));
                }
                lines.push("}".to_string());
            }
        }

        lines.join("\n")
    }
}

impl GraphQLField {
    /// Create a new GraphQL field
    pub fn new(name: String, field_type: String) -> Self {
        Self {
            name,
            field_type,
            required: false,
            is_list: false,
            description: None,
            arguments: Vec::new(),
        }
    }

    /// Set required flag
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set list flag
    pub fn list(mut self) -> Self {
        self.is_list = true;
        self
    }

    /// Set description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Add an argument
    pub fn add_argument(&mut self, argument: GraphQLArgument) {
        self.arguments.push(argument);
    }

    /// Convert to SDL string
    pub fn to_sdl(&self) -> String {
        let mut result = String::new();

        // Add description if present
        if let Some(desc) = &self.description {
            result.push_str(&format!("\"\"\"{}\"\"\" ", desc));
        }

        result.push_str(&self.name);

        // Add arguments
        if !self.arguments.is_empty() {
            result.push('(');
            let args: Vec<String> = self.arguments.iter().map(|a| a.to_sdl()).collect();
            result.push_str(&args.join(", "));
            result.push(')');
        }

        result.push_str(": ");

        // Add type with list/required modifiers
        let type_str = if self.is_list {
            format!("[{}]", self.field_type)
        } else {
            self.field_type.clone()
        };

        result.push_str(&type_str);

        if self.required {
            result.push('!');
        }

        result
    }
}

impl GraphQLArgument {
    /// Create a new GraphQL argument
    pub fn new(name: String, arg_type: String) -> Self {
        Self {
            name,
            arg_type,
            required: false,
            default_value: None,
        }
    }

    /// Set required flag
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set default value
    pub fn with_default(mut self, default: String) -> Self {
        self.default_value = Some(default);
        self
    }

    /// Convert to SDL string
    pub fn to_sdl(&self) -> String {
        let mut result = format!("{}: {}", self.name, self.arg_type);

        if self.required {
            result.push('!');
        }

        if let Some(default) = &self.default_value {
            result.push_str(&format!(" = {}", default));
        }

        result
    }
}

/// GraphQL schema generator for workflows
pub struct GraphQLSchemaGenerator {
    /// Include descriptions
    pub include_descriptions: bool,

    /// Generated types
    types: HashMap<String, GraphQLType>,
}

impl GraphQLSchemaGenerator {
    /// Create a new schema generator
    pub fn new() -> Self {
        Self {
            include_descriptions: true,
            types: HashMap::new(),
        }
    }

    /// Generate GraphQL schema for workflows
    pub fn generate_workflow_schema(&mut self) -> String {
        // Generate core types
        self.generate_workflow_type();
        self.generate_metadata_type();
        self.generate_node_type();
        self.generate_edge_type();
        self.generate_node_kind_enum();
        self.generate_execution_state_enum();
        self.generate_query_type();
        self.generate_mutation_type();

        // Generate SDL
        self.to_sdl()
    }

    /// Generate the Workflow type
    fn generate_workflow_type(&mut self) {
        let mut workflow_type = GraphQLType::new("Workflow".to_string(), GraphQLTypeKind::Object);

        if self.include_descriptions {
            workflow_type = workflow_type
                .with_description("A workflow defining a sequence of operations".to_string());
        }

        workflow_type.add_field(GraphQLField::new("id".to_string(), "ID".to_string()).required());

        workflow_type.add_field(
            GraphQLField::new("metadata".to_string(), "WorkflowMetadata".to_string()).required(),
        );

        workflow_type.add_field(
            GraphQLField::new("nodes".to_string(), "Node".to_string())
                .list()
                .required(),
        );

        workflow_type.add_field(
            GraphQLField::new("edges".to_string(), "Edge".to_string())
                .list()
                .required(),
        );

        self.types.insert("Workflow".to_string(), workflow_type);
    }

    /// Generate the WorkflowMetadata type
    fn generate_metadata_type(&mut self) {
        let mut metadata_type =
            GraphQLType::new("WorkflowMetadata".to_string(), GraphQLTypeKind::Object);

        if self.include_descriptions {
            metadata_type =
                metadata_type.with_description("Workflow metadata and description".to_string());
        }

        metadata_type
            .add_field(GraphQLField::new("name".to_string(), "String".to_string()).required());

        metadata_type.add_field(GraphQLField::new(
            "description".to_string(),
            "String".to_string(),
        ));

        metadata_type
            .add_field(GraphQLField::new("version".to_string(), "String".to_string()).required());

        metadata_type.add_field(
            GraphQLField::new("createdAt".to_string(), "DateTime".to_string()).required(),
        );

        metadata_type.add_field(
            GraphQLField::new("updatedAt".to_string(), "DateTime".to_string()).required(),
        );

        metadata_type.add_field(
            GraphQLField::new("tags".to_string(), "String".to_string())
                .list()
                .required(),
        );

        self.types
            .insert("WorkflowMetadata".to_string(), metadata_type);
    }

    /// Generate the Node type
    fn generate_node_type(&mut self) {
        let mut node_type = GraphQLType::new("Node".to_string(), GraphQLTypeKind::Object);

        if self.include_descriptions {
            node_type = node_type.with_description("A workflow node".to_string());
        }

        node_type.add_field(GraphQLField::new("id".to_string(), "ID".to_string()).required());

        node_type.add_field(GraphQLField::new("name".to_string(), "String".to_string()).required());

        node_type
            .add_field(GraphQLField::new("kind".to_string(), "NodeKind".to_string()).required());

        self.types.insert("Node".to_string(), node_type);
    }

    /// Generate the Edge type
    fn generate_edge_type(&mut self) {
        let mut edge_type = GraphQLType::new("Edge".to_string(), GraphQLTypeKind::Object);

        if self.include_descriptions {
            edge_type = edge_type.with_description("An edge connecting two nodes".to_string());
        }

        edge_type.add_field(GraphQLField::new("id".to_string(), "ID".to_string()).required());

        edge_type.add_field(GraphQLField::new("from".to_string(), "ID".to_string()).required());

        edge_type.add_field(GraphQLField::new("to".to_string(), "ID".to_string()).required());

        self.types.insert("Edge".to_string(), edge_type);
    }

    /// Generate the NodeKind enum
    fn generate_node_kind_enum(&mut self) {
        let mut node_kind = GraphQLType::new("NodeKind".to_string(), GraphQLTypeKind::Enum);

        if self.include_descriptions {
            node_kind = node_kind.with_description("Types of nodes in a workflow".to_string());
        }

        node_kind.add_enum_value("START".to_string());
        node_kind.add_enum_value("END".to_string());
        node_kind.add_enum_value("LLM".to_string());
        node_kind.add_enum_value("RETRIEVER".to_string());
        node_kind.add_enum_value("CODE".to_string());
        node_kind.add_enum_value("IF_ELSE".to_string());
        node_kind.add_enum_value("TOOL".to_string());
        node_kind.add_enum_value("LOOP".to_string());
        node_kind.add_enum_value("TRY_CATCH".to_string());
        node_kind.add_enum_value("SUB_WORKFLOW".to_string());
        node_kind.add_enum_value("SWITCH".to_string());
        node_kind.add_enum_value("PARALLEL".to_string());
        node_kind.add_enum_value("APPROVAL".to_string());
        node_kind.add_enum_value("FORM".to_string());

        self.types.insert("NodeKind".to_string(), node_kind);
    }

    /// Generate the ExecutionState enum
    fn generate_execution_state_enum(&mut self) {
        let mut exec_state = GraphQLType::new("ExecutionState".to_string(), GraphQLTypeKind::Enum);

        if self.include_descriptions {
            exec_state = exec_state.with_description("State of a workflow execution".to_string());
        }

        exec_state.add_enum_value("PENDING".to_string());
        exec_state.add_enum_value("RUNNING".to_string());
        exec_state.add_enum_value("COMPLETED".to_string());
        exec_state.add_enum_value("FAILED".to_string());
        exec_state.add_enum_value("CANCELLED".to_string());

        self.types.insert("ExecutionState".to_string(), exec_state);
    }

    /// Generate the Query type
    fn generate_query_type(&mut self) {
        let mut query_type = GraphQLType::new("Query".to_string(), GraphQLTypeKind::Object);

        // Get workflow by ID
        let mut get_workflow = GraphQLField::new("workflow".to_string(), "Workflow".to_string());
        get_workflow
            .add_argument(GraphQLArgument::new("id".to_string(), "ID".to_string()).required());
        query_type.add_field(get_workflow);

        // List workflows
        let mut list_workflows = GraphQLField::new("workflows".to_string(), "Workflow".to_string())
            .list()
            .required();
        list_workflows.add_argument(
            GraphQLArgument::new("limit".to_string(), "Int".to_string())
                .with_default("10".to_string()),
        );
        list_workflows.add_argument(
            GraphQLArgument::new("offset".to_string(), "Int".to_string())
                .with_default("0".to_string()),
        );
        query_type.add_field(list_workflows);

        self.types.insert("Query".to_string(), query_type);
    }

    /// Generate the Mutation type
    fn generate_mutation_type(&mut self) {
        let mut mutation_type = GraphQLType::new("Mutation".to_string(), GraphQLTypeKind::Object);

        // Create workflow
        let mut create_workflow =
            GraphQLField::new("createWorkflow".to_string(), "Workflow".to_string()).required();
        create_workflow.add_argument(
            GraphQLArgument::new("name".to_string(), "String".to_string()).required(),
        );
        create_workflow.add_argument(GraphQLArgument::new(
            "description".to_string(),
            "String".to_string(),
        ));
        mutation_type.add_field(create_workflow);

        // Delete workflow
        let mut delete_workflow =
            GraphQLField::new("deleteWorkflow".to_string(), "Boolean".to_string()).required();
        delete_workflow
            .add_argument(GraphQLArgument::new("id".to_string(), "ID".to_string()).required());
        mutation_type.add_field(delete_workflow);

        self.types.insert("Mutation".to_string(), mutation_type);
    }

    /// Convert all types to SDL
    pub fn to_sdl(&self) -> String {
        let mut sdl = Vec::new();

        // Add custom scalars
        sdl.push("scalar DateTime".to_string());
        sdl.push("".to_string());

        // Add types in order: enums, objects, query, mutation
        let type_order = vec![
            "NodeKind",
            "ExecutionState",
            "Edge",
            "Node",
            "WorkflowMetadata",
            "Workflow",
            "Query",
            "Mutation",
        ];

        for type_name in type_order {
            if let Some(gql_type) = self.types.get(type_name) {
                sdl.push(gql_type.to_sdl());
                sdl.push("".to_string());
            }
        }

        sdl.join("\n")
    }
}

impl Default for GraphQLSchemaGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a GraphQL schema for workflows
pub fn generate_graphql_schema() -> String {
    let mut generator = GraphQLSchemaGenerator::new();
    generator.generate_workflow_schema()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_field_sdl() {
        let field = GraphQLField::new("name".to_string(), "String".to_string()).required();
        assert_eq!(field.to_sdl(), "name: String!");
    }

    #[test]
    fn test_graphql_field_list() {
        let field = GraphQLField::new("tags".to_string(), "String".to_string())
            .list()
            .required();
        assert_eq!(field.to_sdl(), "tags: [String]!");
    }

    #[test]
    fn test_graphql_argument() {
        let arg = GraphQLArgument::new("id".to_string(), "ID".to_string()).required();
        assert_eq!(arg.to_sdl(), "id: ID!");
    }

    #[test]
    fn test_graphql_argument_with_default() {
        let arg = GraphQLArgument::new("limit".to_string(), "Int".to_string())
            .with_default("10".to_string());
        assert_eq!(arg.to_sdl(), "limit: Int = 10");
    }

    #[test]
    fn test_enum_type_sdl() {
        let mut enum_type = GraphQLType::new("Status".to_string(), GraphQLTypeKind::Enum);
        enum_type.add_enum_value("ACTIVE".to_string());
        enum_type.add_enum_value("INACTIVE".to_string());

        let sdl = enum_type.to_sdl();
        assert!(sdl.contains("enum Status"));
        assert!(sdl.contains("ACTIVE"));
        assert!(sdl.contains("INACTIVE"));
    }

    #[test]
    fn test_object_type_sdl() {
        let mut object_type = GraphQLType::new("User".to_string(), GraphQLTypeKind::Object);
        object_type.add_field(GraphQLField::new("id".to_string(), "ID".to_string()).required());
        object_type
            .add_field(GraphQLField::new("name".to_string(), "String".to_string()).required());

        let sdl = object_type.to_sdl();
        assert!(sdl.contains("type User"));
        assert!(sdl.contains("id: ID!"));
        assert!(sdl.contains("name: String!"));
    }

    #[test]
    fn test_generate_workflow_schema() {
        let mut generator = GraphQLSchemaGenerator::new();
        let schema = generator.generate_workflow_schema();

        assert!(schema.contains("type Workflow"));
        assert!(schema.contains("type Node"));
        assert!(schema.contains("type Edge"));
        assert!(schema.contains("enum NodeKind"));
        assert!(schema.contains("type Query"));
        assert!(schema.contains("type Mutation"));
    }

    #[test]
    fn test_schema_has_node_kinds() {
        let mut generator = GraphQLSchemaGenerator::new();
        let schema = generator.generate_workflow_schema();

        assert!(schema.contains("START"));
        assert!(schema.contains("END"));
        assert!(schema.contains("LLM"));
        assert!(schema.contains("RETRIEVER"));
    }

    #[test]
    fn test_schema_has_execution_states() {
        let mut generator = GraphQLSchemaGenerator::new();
        let schema = generator.generate_workflow_schema();

        assert!(schema.contains("PENDING"));
        assert!(schema.contains("RUNNING"));
        assert!(schema.contains("COMPLETED"));
        assert!(schema.contains("FAILED"));
    }

    #[test]
    fn test_query_type_has_workflow_field() {
        let mut generator = GraphQLSchemaGenerator::new();
        generator.generate_query_type();

        if let Some(query_type) = generator.types.get("Query") {
            assert!(query_type.fields.iter().any(|f| f.name == "workflow"));
            assert!(query_type.fields.iter().any(|f| f.name == "workflows"));
        } else {
            panic!("Query type not found");
        }
    }

    #[test]
    fn test_mutation_type_has_create_workflow() {
        let mut generator = GraphQLSchemaGenerator::new();
        generator.generate_mutation_type();

        if let Some(mutation_type) = generator.types.get("Mutation") {
            assert!(mutation_type
                .fields
                .iter()
                .any(|f| f.name == "createWorkflow"));
            assert!(mutation_type
                .fields
                .iter()
                .any(|f| f.name == "deleteWorkflow"));
        } else {
            panic!("Mutation type not found");
        }
    }

    #[test]
    fn test_field_with_description() {
        let field = GraphQLField::new("name".to_string(), "String".to_string())
            .with_description("The name of the user".to_string())
            .required();

        let sdl = field.to_sdl();
        assert!(sdl.contains("The name of the user"));
    }

    #[test]
    fn test_type_with_description() {
        let type_def = GraphQLType::new("User".to_string(), GraphQLTypeKind::Object)
            .with_description("A user in the system".to_string());

        let sdl = type_def.to_sdl();
        assert!(sdl.contains("A user in the system"));
    }

    #[test]
    fn test_field_with_arguments() {
        let mut field = GraphQLField::new("users".to_string(), "User".to_string()).list();
        field.add_argument(GraphQLArgument::new("limit".to_string(), "Int".to_string()));
        field.add_argument(GraphQLArgument::new(
            "offset".to_string(),
            "Int".to_string(),
        ));

        let sdl = field.to_sdl();
        assert!(sdl.contains("users("));
        assert!(sdl.contains("limit: Int"));
        assert!(sdl.contains("offset: Int"));
    }
}
