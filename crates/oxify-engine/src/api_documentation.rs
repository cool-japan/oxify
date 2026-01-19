//! API Documentation Generator
//!
//! Automatically generates OpenAPI/Swagger documentation from REST API interactions.
//!
//! # Features
//!
//! - Capture REST API requests and responses
//! - Generate OpenAPI 3.0 specifications
//! - Schema inference from JSON payloads
//! - Export to JSON and YAML formats
//! - Support for multiple authentication schemes
//! - Request/response example generation
//!
//! # Example
//!
//! ```ignore
//! use oxify_engine::api_documentation::{ApiDocGenerator, ApiEndpoint};
//!
//! let mut generator = ApiDocGenerator::new("My API", "1.0.0");
//!
//! // Record an API interaction
//! generator.record_request("GET", "/users/{id}", None, Some(response_body));
//!
//! // Generate OpenAPI spec
//! let openapi_json = generator.generate_openapi_json()?;
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// API documentation generator
#[derive(Debug, Clone)]
pub struct ApiDocGenerator {
    /// API title
    title: String,
    /// API version
    version: String,
    /// API description
    description: Option<String>,
    /// Base URL
    base_url: Option<String>,
    /// Captured endpoints
    endpoints: HashMap<String, ApiEndpoint>,
    /// Global security schemes
    security_schemes: HashMap<String, SecurityScheme>,
}

/// API endpoint documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    /// HTTP method
    pub method: String,
    /// URL path
    pub path: String,
    /// Endpoint description
    pub description: Option<String>,
    /// Path parameters
    pub path_parameters: Vec<Parameter>,
    /// Query parameters
    pub query_parameters: Vec<Parameter>,
    /// Request body schema
    pub request_schema: Option<JsonSchema>,
    /// Response schemas by status code
    pub response_schemas: HashMap<u16, JsonSchema>,
    /// Request examples
    pub request_examples: Vec<Value>,
    /// Response examples by status code
    pub response_examples: HashMap<u16, Vec<Value>>,
    /// Tags for organization
    pub tags: Vec<String>,
}

/// Parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name
    pub name: String,
    /// Parameter description
    pub description: Option<String>,
    /// Whether parameter is required
    pub required: bool,
    /// Parameter type
    pub param_type: ApiParameterType,
    /// Example value
    pub example: Option<String>,
}

/// API parameter type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiParameterType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
}

/// JSON schema for request/response bodies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchema {
    /// Schema type
    #[serde(rename = "type")]
    pub schema_type: String,
    /// Properties (for object types)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, JsonSchema>>,
    /// Required properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    /// Items schema (for array types)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,
    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Example value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
}

/// Security scheme definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SecurityScheme {
    #[serde(rename = "http")]
    Http {
        scheme: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        bearer_format: Option<String>,
    },
    #[serde(rename = "apiKey")]
    ApiKey {
        name: String,
        #[serde(rename = "in")]
        location: String,
    },
    #[serde(rename = "oauth2")]
    OAuth2 { flows: HashMap<String, OAuthFlow> },
}

/// OAuth2 flow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthFlow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    pub scopes: HashMap<String, String>,
}

impl ApiDocGenerator {
    /// Create a new API documentation generator
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            version: version.into(),
            description: None,
            base_url: None,
            endpoints: HashMap::new(),
            security_schemes: HashMap::new(),
        }
    }

    /// Set API description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set base URL
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Add a security scheme
    pub fn add_security_scheme(&mut self, name: impl Into<String>, scheme: SecurityScheme) {
        self.security_schemes.insert(name.into(), scheme);
    }

    /// Record an API request/response interaction
    pub fn record_request(
        &mut self,
        method: &str,
        path: &str,
        request_body: Option<&Value>,
        response_body: Option<&Value>,
        status_code: u16,
    ) {
        let key = format!("{}:{}", method.to_uppercase(), path);

        let endpoint = self
            .endpoints
            .entry(key.clone())
            .or_insert_with(|| ApiEndpoint {
                method: method.to_uppercase(),
                path: path.to_string(),
                description: None,
                path_parameters: Self::extract_path_parameters(path),
                query_parameters: Vec::new(),
                request_schema: None,
                response_schemas: HashMap::new(),
                request_examples: Vec::new(),
                response_examples: HashMap::new(),
                tags: Vec::new(),
            });

        // Infer request schema from body
        if let Some(body) = request_body {
            if endpoint.request_schema.is_none() {
                endpoint.request_schema = Some(Self::infer_schema(body));
            }
            endpoint.request_examples.push(body.clone());
        }

        // Infer response schema from body
        if let Some(body) = response_body {
            endpoint
                .response_schemas
                .entry(status_code)
                .or_insert_with(|| Self::infer_schema(body));
            endpoint
                .response_examples
                .entry(status_code)
                .or_default()
                .push(body.clone());
        }
    }

    /// Extract path parameters from path template
    fn extract_path_parameters(path: &str) -> Vec<Parameter> {
        let mut params = Vec::new();
        for segment in path.split('/') {
            if segment.starts_with('{') && segment.ends_with('}') {
                let name = segment.trim_start_matches('{').trim_end_matches('}');
                params.push(Parameter {
                    name: name.to_string(),
                    description: None,
                    required: true,
                    param_type: ApiParameterType::String,
                    example: None,
                });
            }
        }
        params
    }

    /// Infer JSON schema from a value
    fn infer_schema(value: &Value) -> JsonSchema {
        match value {
            Value::Null => JsonSchema {
                schema_type: "null".to_string(),
                properties: None,
                required: None,
                items: None,
                description: None,
                example: Some(value.clone()),
            },
            Value::Bool(_) => JsonSchema {
                schema_type: "boolean".to_string(),
                properties: None,
                required: None,
                items: None,
                description: None,
                example: Some(value.clone()),
            },
            Value::Number(n) => {
                let schema_type = if n.is_f64() { "number" } else { "integer" };
                JsonSchema {
                    schema_type: schema_type.to_string(),
                    properties: None,
                    required: None,
                    items: None,
                    description: None,
                    example: Some(value.clone()),
                }
            }
            Value::String(_) => JsonSchema {
                schema_type: "string".to_string(),
                properties: None,
                required: None,
                items: None,
                description: None,
                example: Some(value.clone()),
            },
            Value::Array(arr) => {
                let items_schema = if let Some(first) = arr.first() {
                    Self::infer_schema(first)
                } else {
                    JsonSchema {
                        schema_type: "object".to_string(),
                        properties: None,
                        required: None,
                        items: None,
                        description: None,
                        example: None,
                    }
                };
                JsonSchema {
                    schema_type: "array".to_string(),
                    properties: None,
                    required: None,
                    items: Some(Box::new(items_schema)),
                    description: None,
                    example: Some(value.clone()),
                }
            }
            Value::Object(obj) => {
                let mut properties = HashMap::new();
                let mut required = Vec::new();

                for (key, val) in obj {
                    properties.insert(key.clone(), Self::infer_schema(val));
                    required.push(key.clone());
                }

                JsonSchema {
                    schema_type: "object".to_string(),
                    properties: Some(properties),
                    required: Some(required),
                    items: None,
                    description: None,
                    example: Some(value.clone()),
                }
            }
        }
    }

    /// Generate OpenAPI 3.0 specification as JSON
    pub fn generate_openapi_json(&self) -> Result<String, serde_json::Error> {
        let spec = self.build_openapi_spec();
        serde_json::to_string_pretty(&spec)
    }

    /// Generate OpenAPI 3.0 specification as Value
    fn build_openapi_spec(&self) -> Value {
        let mut paths = json!({});

        for endpoint in self.endpoints.values() {
            let path_obj = paths
                .as_object_mut()
                .unwrap()
                .entry(&endpoint.path)
                .or_insert_with(|| json!({}));

            let operation = self.build_operation(endpoint);
            path_obj
                .as_object_mut()
                .unwrap()
                .insert(endpoint.method.to_lowercase(), operation);
        }

        let mut spec = json!({
            "openapi": "3.0.0",
            "info": {
                "title": self.title,
                "version": self.version,
            },
            "paths": paths,
        });

        if let Some(desc) = &self.description {
            spec["info"]["description"] = json!(desc);
        }

        if !self.security_schemes.is_empty() {
            spec["components"] = json!({
                "securitySchemes": self.security_schemes
            });
        }

        spec
    }

    /// Build operation object for an endpoint
    fn build_operation(&self, endpoint: &ApiEndpoint) -> Value {
        let mut operation = json!({});

        if let Some(desc) = &endpoint.description {
            operation["description"] = json!(desc);
        }

        if !endpoint.tags.is_empty() {
            operation["tags"] = json!(endpoint.tags);
        }

        // Parameters
        let mut parameters = Vec::new();
        for param in &endpoint.path_parameters {
            parameters.push(json!({
                "name": param.name,
                "in": "path",
                "required": param.required,
                "schema": {
                    "type": format!("{:?}", param.param_type).to_lowercase()
                }
            }));
        }
        for param in &endpoint.query_parameters {
            parameters.push(json!({
                "name": param.name,
                "in": "query",
                "required": param.required,
                "schema": {
                    "type": format!("{:?}", param.param_type).to_lowercase()
                }
            }));
        }
        if !parameters.is_empty() {
            operation["parameters"] = json!(parameters);
        }

        // Request body
        if let Some(schema) = &endpoint.request_schema {
            operation["requestBody"] = json!({
                "content": {
                    "application/json": {
                        "schema": schema
                    }
                }
            });
        }

        // Responses
        let mut responses = json!({});
        for (status, schema) in &endpoint.response_schemas {
            responses[status.to_string()] = json!({
                "description": format!("Response with status {}", status),
                "content": {
                    "application/json": {
                        "schema": schema
                    }
                }
            });
        }
        if !responses.as_object().unwrap().is_empty() {
            operation["responses"] = responses;
        } else {
            // Default response
            operation["responses"] = json!({
                "200": {
                    "description": "Successful response"
                }
            });
        }

        operation
    }

    /// Get all endpoints
    pub fn endpoints(&self) -> &HashMap<String, ApiEndpoint> {
        &self.endpoints
    }

    /// Get endpoint count
    pub fn endpoint_count(&self) -> usize {
        self.endpoints.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_doc_generator_creation() {
        let generator = ApiDocGenerator::new("Test API", "1.0.0");
        assert_eq!(generator.title, "Test API");
        assert_eq!(generator.version, "1.0.0");
        assert_eq!(generator.endpoint_count(), 0);
    }

    #[test]
    fn test_record_request_with_response() {
        let mut generator = ApiDocGenerator::new("Test API", "1.0.0");

        let request_body = json!({
            "name": "John Doe",
            "age": 30
        });

        let response_body = json!({
            "id": 123,
            "name": "John Doe",
            "age": 30,
            "created_at": "2023-01-01T00:00:00Z"
        });

        generator.record_request(
            "POST",
            "/users",
            Some(&request_body),
            Some(&response_body),
            201,
        );

        assert_eq!(generator.endpoint_count(), 1);
        let key = "POST:/users";
        assert!(generator.endpoints().contains_key(key));
    }

    #[test]
    fn test_extract_path_parameters() {
        let params = ApiDocGenerator::extract_path_parameters("/users/{id}/posts/{postId}");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "id");
        assert_eq!(params[1].name, "postId");
        assert!(params[0].required);
        assert!(params[1].required);
    }

    #[test]
    fn test_infer_schema_object() {
        let value = json!({
            "name": "John",
            "age": 30,
            "active": true
        });

        let schema = ApiDocGenerator::infer_schema(&value);
        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.is_some());

        let properties = schema.properties.unwrap();
        assert_eq!(properties.len(), 3);
        assert_eq!(properties.get("name").unwrap().schema_type, "string");
        assert_eq!(properties.get("age").unwrap().schema_type, "integer");
        assert_eq!(properties.get("active").unwrap().schema_type, "boolean");
    }

    #[test]
    fn test_infer_schema_array() {
        let value = json!([
            {"id": 1, "name": "Item 1"},
            {"id": 2, "name": "Item 2"}
        ]);

        let schema = ApiDocGenerator::infer_schema(&value);
        assert_eq!(schema.schema_type, "array");
        assert!(schema.items.is_some());

        let items = schema.items.unwrap();
        assert_eq!(items.schema_type, "object");
    }

    #[test]
    fn test_generate_openapi_json() {
        let mut generator = ApiDocGenerator::new("Test API", "1.0.0")
            .with_description("A test API")
            .with_base_url("https://api.example.com");

        let response_body = json!({
            "id": 1,
            "name": "Test User"
        });

        generator.record_request("GET", "/users/{id}", None, Some(&response_body), 200);

        let openapi_json = generator.generate_openapi_json().unwrap();
        assert!(openapi_json.contains("openapi"));
        assert!(openapi_json.contains("3.0.0"));
        assert!(openapi_json.contains("Test API"));
        assert!(openapi_json.contains("/users/{id}"));
    }

    #[test]
    fn test_security_scheme_bearer() {
        let mut generator = ApiDocGenerator::new("Test API", "1.0.0");

        generator.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http {
                scheme: "bearer".to_string(),
                bearer_format: Some("JWT".to_string()),
            },
        );

        let openapi_json = generator.generate_openapi_json().unwrap();
        assert!(openapi_json.contains("securitySchemes"));
        assert!(openapi_json.contains("bearerAuth"));
    }

    #[test]
    fn test_multiple_endpoints() {
        let mut generator = ApiDocGenerator::new("Test API", "1.0.0");

        generator.record_request("GET", "/users", None, Some(&json!([])), 200);
        generator.record_request("POST", "/users", Some(&json!({"name": "John"})), None, 201);
        generator.record_request("GET", "/users/{id}", None, Some(&json!({"id": 1})), 200);

        assert_eq!(generator.endpoint_count(), 3);
    }
}
