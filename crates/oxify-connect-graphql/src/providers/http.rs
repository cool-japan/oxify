use std::collections::HashMap;
use std::env;
use std::time::Duration;

use async_trait::async_trait;
use hyper_util::client::legacy::connect::HttpConnector;
use oxihttp::OxiHttpsConnector;
use serde_json::{json, Value};
use tracing::{debug, instrument, warn};

use crate::{
    errors::{GraphQlError, Result},
    types::{AuthConfig, GraphQlRequest, RetryConfig},
};

/// Connector type backing [`HttpGraphQlProvider`]'s HTTPS-capable client.
///
/// Named explicitly so the private [`HttpGraphQlProvider::apply_auth`] helper
/// can operate on a concrete `oxihttp::RequestBuilder<GraphQlConnector>`
/// instead of leaking a generic connector parameter through the crate's
/// (already private) internals.  `oxihttp::HttpsClient` is a type alias for
/// `Client<GraphQlConnector>`, and this same connector transparently
/// supports plain `http://` targets (used by the wiremock-backed tests
/// below) as well as `https://` targets (used in production).
type GraphQlConnector = OxiHttpsConnector<HttpConnector>;

// ---------------------------------------------------------------------------
// GraphQlConfig
// ---------------------------------------------------------------------------

/// Configuration for the HTTP-based GraphQL client provider.
#[derive(Debug, Clone)]
pub struct GraphQlConfig {
    /// Full URL of the GraphQL endpoint (e.g. `https://api.example.com/graphql`).
    pub endpoint: String,
    /// Authentication strategy applied to every request.
    pub auth: AuthConfig,
    /// Per-request timeout in seconds.  Defaults to `30`.
    pub timeout_secs: u64,
    /// Headers added to every outgoing request (beyond auth).
    pub default_headers: HashMap<String, String>,
    /// Optional retry configuration.  When `None` no retries are attempted.
    pub retry: Option<RetryConfig>,
}

impl Default for GraphQlConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            auth: AuthConfig::None,
            timeout_secs: 30,
            default_headers: HashMap::new(),
            retry: Some(RetryConfig::default()),
        }
    }
}

impl GraphQlConfig {
    /// Build a `GraphQlConfig` from environment variables.
    ///
    /// **Required**: `GRAPHQL_ENDPOINT`
    /// **Optional**: `GRAPHQL_BEARER_TOKEN` — when present, sets auth to
    /// `AuthConfig::Bearer`.
    ///
    /// Returns `Err(GraphQlError::Config)` if `GRAPHQL_ENDPOINT` is absent or
    /// empty.
    pub fn from_env() -> Result<Self> {
        let endpoint = env::var("GRAPHQL_ENDPOINT").map_err(|_| {
            GraphQlError::Config("GRAPHQL_ENDPOINT environment variable is not set".to_string())
        })?;

        if endpoint.trim().is_empty() {
            return Err(GraphQlError::Config(
                "GRAPHQL_ENDPOINT environment variable is empty".to_string(),
            ));
        }

        let auth = match env::var("GRAPHQL_BEARER_TOKEN") {
            Ok(token) if !token.trim().is_empty() => AuthConfig::bearer(token),
            _ => AuthConfig::None,
        };

        Ok(Self {
            endpoint,
            auth,
            ..Self::default()
        })
    }
}

// ---------------------------------------------------------------------------
// HttpGraphQlProvider
// ---------------------------------------------------------------------------

/// GraphQL client provider that communicates over plain HTTPS using `oxihttp`.
///
/// All GraphQL operations are sent as HTTP POST requests with a JSON body
/// containing `query`, `variables`, and an optional `operationName` field.
pub struct HttpGraphQlProvider {
    cfg: GraphQlConfig,
    http: oxihttp::HttpsClient,
}

impl HttpGraphQlProvider {
    /// Create a new `HttpGraphQlProvider` from the supplied configuration.
    ///
    /// Returns `Err(GraphQlError::Transport)` if the underlying `oxihttp`
    /// client cannot be built (e.g. invalid TLS configuration).
    pub fn new(cfg: GraphQlConfig) -> Result<Self> {
        let http = oxihttp::Client::builder()
            .with_tls()
            .connect_timeout(Duration::from_secs(cfg.timeout_secs))
            .read_timeout(Duration::from_secs(cfg.timeout_secs))
            .build_https()
            .map_err(|e| GraphQlError::Transport(format!("failed to build HTTP client: {e}")))?;

        Ok(Self { cfg, http })
    }

    /// Create a new `HttpGraphQlProvider` reading configuration from the
    /// environment.  See [`GraphQlConfig::from_env`] for required variables.
    pub fn from_env() -> Result<Self> {
        Self::new(GraphQlConfig::from_env()?)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Attach the configured authentication credentials to `req`.
    ///
    /// The `ApiKey { in_header: false, .. }` variant is intentionally a
    /// no-op here: `oxihttp`'s `RequestBuilder` has no `.query()` equivalent,
    /// so that variant is instead applied to the endpoint URL via
    /// [`oxify_model::http_util::append_query_params`] *before* the request
    /// builder is constructed — see [`Self::execute_once`].
    async fn apply_auth(
        &self,
        req: oxihttp::RequestBuilder<GraphQlConnector>,
    ) -> Result<oxihttp::RequestBuilder<GraphQlConnector>> {
        match &self.cfg.auth {
            AuthConfig::None => Ok(req),
            AuthConfig::Bearer { token } => req
                .bearer_token(token)
                .map_err(|e| GraphQlError::Transport(format!("failed to apply bearer auth: {e}"))),
            AuthConfig::ApiKey {
                key,
                value,
                in_header,
            } => {
                if *in_header {
                    req.header(key.as_str(), value.as_str()).map_err(|e| {
                        GraphQlError::Transport(format!("failed to apply API key header: {e}"))
                    })
                } else {
                    Ok(req)
                }
            }
            AuthConfig::Basic { username, password } => req
                .basic_auth(username, Some(password))
                .map_err(|e| GraphQlError::Transport(format!("failed to apply basic auth: {e}"))),
            AuthConfig::Custom { header, value } => {
                req.header(header.as_str(), value.as_str()).map_err(|e| {
                    GraphQlError::Transport(format!("failed to apply custom auth header: {e}"))
                })
            }
        }
    }

    /// Perform a single HTTP POST to the GraphQL endpoint and parse the
    /// response.  Does NOT apply retry logic — see [`Self::execute_with_retry`].
    async fn execute_once(&self, req: &GraphQlRequest) -> Result<Value> {
        // Build the JSON body.  Omit `operationName` when not provided to
        // keep the request lean.
        let body = if let Some(ref op_name) = req.operation_name {
            json!({
                "query": req.query,
                "variables": req.variables,
                "operationName": op_name,
            })
        } else {
            json!({
                "query": req.query,
                "variables": req.variables,
            })
        };

        debug!(endpoint = %self.cfg.endpoint, "executing GraphQL request");

        // `oxihttp`'s `RequestBuilder` has no `.query()` equivalent, so a
        // query-string API key must be baked into the URL before the
        // request builder is constructed.
        let url = if let AuthConfig::ApiKey {
            key,
            value,
            in_header: false,
        } = &self.cfg.auth
        {
            oxify_model::http_util::append_query_params(
                &self.cfg.endpoint,
                &[(key.as_str(), value.as_str())],
            )
        } else {
            self.cfg.endpoint.clone()
        };

        // Start building the request.
        let mut builder = self
            .http
            .post(&url)
            .map_err(|e| GraphQlError::Transport(format!("failed to build request: {e}")))?;

        // Merge default headers.
        for (key, value) in &self.cfg.default_headers {
            builder = builder
                .header(key.as_str(), value.as_str())
                .map_err(|e| GraphQlError::Transport(format!("failed to set header {key}: {e}")))?;
        }

        // Apply authentication.
        builder = self.apply_auth(builder).await?;

        // Send the request.
        let response = builder
            .json(&body)
            .map_err(|e| {
                GraphQlError::Serialization(format!("failed to serialize request body: {e}"))
            })?
            .send()
            .await
            .map_err(|e| GraphQlError::Transport(format!("request send failed: {e}")))?;

        // Capture the status code BEFORE consuming the response body.
        let status = response.status();

        if !status.is_success() {
            let text = response
                .body_text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(GraphQlError::Http(format!("HTTP {status}: {text}")));
        }

        // Parse JSON body.
        let payload: Value = response
            .body_json()
            .await
            .map_err(|e| GraphQlError::Serialization(format!("failed to parse response: {e}")))?;

        // GraphQL-level error inspection: the spec allows a 200 response with
        // an `errors` array alongside (or instead of) `data`.
        if let Some(errors) = payload.get("errors") {
            if let Some(arr) = errors.as_array() {
                if !arr.is_empty() {
                    let messages: Vec<String> = arr
                        .iter()
                        .filter_map(|e| e.get("message"))
                        .filter_map(|m| m.as_str())
                        .map(|s| s.to_string())
                        .collect();
                    let combined = if messages.is_empty() {
                        "unknown GraphQL error".to_string()
                    } else {
                        messages.join("; ")
                    };
                    return Err(GraphQlError::GraphQl(combined));
                }
            }
        }

        Ok(payload["data"].clone())
    }

    /// Execute a GraphQL request, retrying on transient failures according to
    /// `cfg.retry`.  When `retry` is `None` the request is attempted exactly
    /// once.
    #[instrument(skip(self, req), fields(provider = "graphql-http"))]
    async fn execute_with_retry(&self, req: GraphQlRequest) -> Result<Value> {
        let (max_retries, initial_delay_ms, max_delay_ms, backoff_multiplier, retry_codes) =
            match &self.cfg.retry {
                Some(r) => (
                    r.max_retries,
                    r.initial_delay_ms,
                    r.max_delay_ms,
                    r.backoff_multiplier,
                    r.retry_status_codes.clone(),
                ),
                None => (0, 0, 0, 1.0, vec![]),
            };

        let mut last_error: Option<GraphQlError> = None;
        let mut delay_ms = initial_delay_ms as f64;

        // Total attempts = 1 (initial) + max_retries.
        for attempt in 0..=(max_retries) {
            if attempt > 0 {
                let sleep_ms = delay_ms.min(max_delay_ms as f64) as u64;
                warn!(
                    attempt,
                    sleep_ms, "GraphQL request failed; retrying after backoff"
                );
                tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                delay_ms = (delay_ms * backoff_multiplier).min(max_delay_ms as f64);
            }

            match self.execute_once(&req).await {
                Ok(val) => return Ok(val),
                Err(e) => {
                    // Only retry on Http errors whose status code is in the
                    // retry list, or on Transport errors (network-level).
                    let should_retry = match &e {
                        GraphQlError::Http(msg) => {
                            // Extract the numeric status code from the message
                            // format "HTTP <status>: ..."
                            let retryable_by_code = retry_codes.iter().any(|&code| {
                                msg.contains(&format!("HTTP {code}"))
                                    || msg.starts_with(&format!("HTTP {code}"))
                            });
                            // Also retry on any 5xx regardless of retry_codes
                            let retryable_5xx =
                                (500..600u16).any(|code| msg.contains(&format!("HTTP {code}")));
                            retryable_by_code || retryable_5xx
                        }
                        GraphQlError::Transport(_) => true,
                        _ => false,
                    };

                    if !should_retry || attempt == max_retries {
                        return Err(e);
                    }
                    last_error = Some(e);
                }
            }
        }

        // Unreachable in practice but satisfies the compiler.
        Err(last_error.unwrap_or_else(|| {
            GraphQlError::Transport("retry loop exited without result".to_string())
        }))
    }
}

// ---------------------------------------------------------------------------
// GraphQlExecutor implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl super::GraphQlExecutor for HttpGraphQlProvider {
    fn provider_name(&self) -> &str {
        "graphql-http"
    }

    async fn execute(&self, req: GraphQlRequest) -> Result<Value> {
        self.execute_with_retry(req).await
    }

    #[cfg(feature = "introspection")]
    async fn introspect(&self) -> Result<Value> {
        let introspection_query =
            "query IntrospectionQuery { __schema { queryType { name } types { name kind } } }";
        let req = GraphQlRequest::new(introspection_query);
        self.execute_with_retry(req).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::providers::GraphQlExecutor;

    // -----------------------------------------------------------------------
    // Test helper
    // -----------------------------------------------------------------------

    fn provider_with_base_url(base_url: &str) -> HttpGraphQlProvider {
        let cfg = GraphQlConfig {
            endpoint: format!("{}/graphql", base_url),
            auth: AuthConfig::bearer("test-token"),
            // Disable retries to keep tests fast and deterministic.
            retry: None,
            ..GraphQlConfig::default()
        };
        HttpGraphQlProvider::new(cfg).expect("build provider")
    }

    // -----------------------------------------------------------------------
    // Unit tests (no network)
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_default() {
        let cfg = GraphQlConfig::default();
        assert!(cfg.endpoint.is_empty(), "default endpoint should be empty");
        assert!(
            matches!(cfg.auth, AuthConfig::None),
            "default auth should be None"
        );
        assert_eq!(cfg.timeout_secs, 30, "default timeout should be 30 s");
    }

    #[test]
    fn test_config_from_env_missing_endpoint() {
        // Ensure the variable is absent for this test.
        unsafe {
            std::env::remove_var("GRAPHQL_ENDPOINT");
        }
        let result = GraphQlConfig::from_env();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("GRAPHQL_ENDPOINT"),
            "error should mention GRAPHQL_ENDPOINT, got: {err_msg}"
        );
    }

    #[test]
    fn test_auth_config_bearer() {
        let auth = AuthConfig::bearer("tok");
        match auth {
            AuthConfig::Bearer { token } => assert_eq!(token, "tok"),
            other => panic!("expected Bearer, got: {other:?}"),
        }
    }

    #[test]
    fn test_auth_config_api_key() {
        let auth = AuthConfig::api_key("X-Api-Key", "secret");
        match auth {
            AuthConfig::ApiKey {
                key,
                value,
                in_header,
            } => {
                assert_eq!(key, "X-Api-Key");
                assert_eq!(value, "secret");
                assert!(in_header);
            }
            other => panic!("expected ApiKey, got: {other:?}"),
        }
    }

    #[test]
    fn test_auth_config_basic() {
        let auth = AuthConfig::basic("alice", "wonderland");
        match auth {
            AuthConfig::Basic { username, password } => {
                assert_eq!(username, "alice");
                assert_eq!(password, "wonderland");
            }
            other => panic!("expected Basic, got: {other:?}"),
        }
    }

    #[test]
    fn test_retry_config_default() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.initial_delay_ms, 100);
        assert!(
            cfg.retry_status_codes.contains(&429),
            "retry codes should include 429"
        );
    }

    // -----------------------------------------------------------------------
    // Wiremock integration tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "user": { "name": "Alice" } }
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let result = provider
            .execute(GraphQlRequest::new("query { user { name } }"))
            .await;

        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert_eq!(result.unwrap(), json!({ "user": { "name": "Alice" } }));
    }

    #[tokio::test]
    async fn test_execute_graphql_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "errors": [{ "message": "not found" }]
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let result = provider
            .execute(GraphQlRequest::new("query { missing }"))
            .await;

        assert!(result.is_err(), "expected Err for GraphQL errors");
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("not found"),
            "error message should contain 'not found', got: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_execute_http_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(500).set_body_json(json!({ "message": "server fault" })),
            )
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let result = provider
            .execute(GraphQlRequest::new("query { anything }"))
            .await;

        assert!(result.is_err(), "expected Err for HTTP 500");
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("500"),
            "error message should contain '500', got: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_execute_no_data_no_errors() {
        let server = MockServer::start().await;

        // Server returns an empty object — no `data`, no `errors`.
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let result = provider
            .execute(GraphQlRequest::new("query { anything }"))
            .await;

        assert!(result.is_ok(), "expected Ok for empty response");
        assert_eq!(
            result.unwrap(),
            Value::Null,
            "missing data field should yield Value::Null"
        );
    }
}
