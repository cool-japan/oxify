use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// GraphQlRequest
// ---------------------------------------------------------------------------

/// A GraphQL request containing a query/mutation, optional variables, and an
/// optional operation name (used when the document contains multiple operations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQlRequest {
    /// The GraphQL query or mutation document string.
    pub query: String,
    /// Variables to pass to the operation.  Defaults to `null`.
    pub variables: serde_json::Value,
    /// Optional name identifying which operation to execute.
    pub operation_name: Option<String>,
}

impl Default for GraphQlRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            variables: serde_json::Value::Null,
            operation_name: None,
        }
    }
}

impl GraphQlRequest {
    /// Construct a request with only a query document; variables will be
    /// `null` and no operation name will be sent.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            variables: serde_json::Value::Null,
            operation_name: None,
        }
    }
}

// ---------------------------------------------------------------------------
// GraphQlResponseError
// ---------------------------------------------------------------------------

/// A single error entry from the GraphQL `errors` response array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQlResponseError {
    /// Human-readable error message.
    pub message: String,
    /// The path within the response where the error occurred (if applicable).
    pub path: Option<serde_json::Value>,
    /// Source locations in the query document (if provided by the server).
    pub locations: Option<serde_json::Value>,
    /// Arbitrary server-side extension data.
    pub extensions: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// AuthConfig
// ---------------------------------------------------------------------------

/// Authentication strategy to attach to every GraphQL request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthConfig {
    /// No authentication — requests are sent without any credential header.
    #[default]
    None,
    /// Bearer-token authentication (`Authorization: Bearer <token>`).
    Bearer { token: String },
    /// API-key authentication.  When `in_header` is `true` the key/value pair
    /// is added as a request header; when `false` it is appended as a query
    /// parameter.
    ApiKey {
        key: String,
        value: String,
        #[serde(default)]
        in_header: bool,
    },
    /// HTTP Basic authentication (`Authorization: Basic <base64>`).
    Basic { username: String, password: String },
    /// Arbitrary single custom header (e.g. `X-Api-Token: <value>`).
    Custom { header: String, value: String },
}

impl AuthConfig {
    /// Construct a `Bearer` auth config from the given token string.
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer {
            token: token.into(),
        }
    }

    /// Construct an `ApiKey` auth config with the key sent as a header by default.
    pub fn api_key(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::ApiKey {
            key: key.into(),
            value: value.into(),
            in_header: true,
        }
    }

    /// Construct a `Basic` auth config.
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: username.into(),
            password: password.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// RetryConfig
// ---------------------------------------------------------------------------

/// Controls exponential-backoff retry behaviour for the HTTP provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not counting the initial attempt).
    pub max_retries: u32,
    /// Delay before the first retry, in milliseconds.
    pub initial_delay_ms: u64,
    /// Upper bound on inter-retry delay, in milliseconds.
    pub max_delay_ms: u64,
    /// Multiplicative factor applied to the delay after each failed attempt.
    pub backoff_multiplier: f64,
    /// HTTP status codes that should trigger a retry.
    pub retry_status_codes: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
            retry_status_codes: vec![408, 429, 500, 502, 503, 504],
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_request_default() {
        let req = GraphQlRequest::default();
        assert!(req.query.is_empty());
        assert_eq!(req.variables, serde_json::Value::Null);
        assert!(req.operation_name.is_none());
    }

    #[test]
    fn test_graphql_request_new() {
        let req = GraphQlRequest::new("query { me { id } }");
        assert_eq!(req.query, "query { me { id } }");
        assert_eq!(req.variables, serde_json::Value::Null);
        assert!(req.operation_name.is_none());
    }

    #[test]
    fn test_graphql_request_serde_roundtrip() {
        let req = GraphQlRequest {
            query: "query Test { user { name } }".to_string(),
            variables: serde_json::json!({ "id": 42 }),
            operation_name: Some("Test".to_string()),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: GraphQlRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.query, req.query);
        assert_eq!(restored.variables, req.variables);
        assert_eq!(restored.operation_name, req.operation_name);
    }

    #[test]
    fn test_auth_config_default_is_none() {
        let auth = AuthConfig::default();
        assert!(matches!(auth, AuthConfig::None));
    }

    #[test]
    fn test_auth_config_bearer_constructor() {
        let auth = AuthConfig::bearer("my-secret-token");
        match auth {
            AuthConfig::Bearer { token } => assert_eq!(token, "my-secret-token"),
            other => panic!("expected Bearer, got: {other:?}"),
        }
    }

    #[test]
    fn test_auth_config_api_key_constructor() {
        let auth = AuthConfig::api_key("X-Api-Key", "abc123");
        match auth {
            AuthConfig::ApiKey {
                key,
                value,
                in_header,
            } => {
                assert_eq!(key, "X-Api-Key");
                assert_eq!(value, "abc123");
                assert!(
                    in_header,
                    "api_key constructor should default to in_header=true"
                );
            }
            other => panic!("expected ApiKey, got: {other:?}"),
        }
    }

    #[test]
    fn test_auth_config_basic_constructor() {
        let auth = AuthConfig::basic("user", "pass");
        match auth {
            AuthConfig::Basic { username, password } => {
                assert_eq!(username, "user");
                assert_eq!(password, "pass");
            }
            other => panic!("expected Basic, got: {other:?}"),
        }
    }

    #[test]
    fn test_auth_config_serde_roundtrip_bearer() {
        let auth = AuthConfig::bearer("tok");
        let json = serde_json::to_string(&auth).expect("serialize");
        let restored: AuthConfig = serde_json::from_str(&json).expect("deserialize");
        match restored {
            AuthConfig::Bearer { token } => assert_eq!(token, "tok"),
            other => panic!("expected Bearer after roundtrip, got: {other:?}"),
        }
    }

    #[test]
    fn test_retry_config_defaults() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.initial_delay_ms, 100);
        assert_eq!(cfg.max_delay_ms, 5000);
        assert!((cfg.backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert!(cfg.retry_status_codes.contains(&429));
        assert!(cfg.retry_status_codes.contains(&500));
        assert!(cfg.retry_status_codes.contains(&408));
    }

    #[test]
    fn test_graphql_response_error_serde() {
        let json_str = r#"{
            "message": "field not found",
            "path": ["user", "name"],
            "locations": null,
            "extensions": null
        }"#;
        let err: GraphQlResponseError = serde_json::from_str(json_str).expect("deserialize");
        assert_eq!(err.message, "field not found");
        assert!(err.path.is_some());
        assert!(err.locations.is_none());
    }
}
