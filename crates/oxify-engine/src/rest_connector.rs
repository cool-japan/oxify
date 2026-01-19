//! Generic REST API Connector
//!
//! A flexible connector for integrating with any REST API.
//!
//! # Features
//!
//! - Support for all HTTP methods (GET, POST, PUT, PATCH, DELETE)
//! - Authentication (Bearer, API Key, Basic, OAuth2)
//! - Request/response transformation
//! - Rate limiting
//! - Retry with exponential backoff
//! - Response caching
//! - Request templates with variable substitution
//!
//! # Example
//!
//! ```ignore
//! use oxify_engine::rest_connector::{RestConnector, RestConfig, AuthConfig};
//!
//! let config = RestConfig::new("https://api.example.com")
//!     .with_auth(AuthConfig::bearer("token123"))
//!     .with_timeout_secs(30)
//!     .with_retry(3);
//!
//! let connector = RestConnector::new(config);
//!
//! let response = connector.get("/users/1").await?;
//! let data = connector.post("/users", json!({"name": "John"})).await?;
//! ```

use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

/// REST connector errors
#[derive(Error, Debug)]
pub enum RestConnectorError {
    #[error("HTTP error: {status} - {message}")]
    HttpError { status: u16, message: String },

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Timeout")]
    Timeout,

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Response parsing error: {0}")]
    ResponseParsingError(String),
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthConfig {
    /// No authentication
    None,
    /// Bearer token authentication
    Bearer { token: String },
    /// API key authentication (header or query param)
    ApiKey {
        key: String,
        value: String,
        #[serde(default)]
        in_header: bool,
    },
    /// Basic authentication
    Basic { username: String, password: String },
    /// OAuth2 client credentials
    OAuth2 {
        client_id: String,
        client_secret: String,
        token_url: String,
        #[serde(default)]
        scopes: Vec<String>,
    },
    /// Custom header
    Custom { header: String, value: String },
}

impl AuthConfig {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer {
            token: token.into(),
        }
    }

    pub fn api_key(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::ApiKey {
            key: key.into(),
            value: value.into(),
            in_header: true,
        }
    }

    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: username.into(),
            password: password.into(),
        }
    }
}

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Time window in seconds
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_secs: 60,
        }
    }
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Initial delay in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Status codes that trigger retry
    pub retry_status_codes: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
            retry_status_codes: vec![408, 429, 500, 502, 503, 504],
        }
    }
}

/// REST connector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestConfig {
    /// Base URL for all requests
    pub base_url: String,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Default headers for all requests
    #[serde(default)]
    pub default_headers: HashMap<String, String>,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Rate limit configuration
    pub rate_limit: Option<RateLimitConfig>,
    /// Retry configuration
    pub retry: Option<RetryConfig>,
    /// Enable response caching
    #[serde(default)]
    pub enable_cache: bool,
    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
}

fn default_timeout() -> u64 {
    30
}

fn default_cache_ttl() -> u64 {
    300
}

impl RestConfig {
    /// Create a new REST configuration
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            auth: AuthConfig::None,
            default_headers: HashMap::new(),
            timeout_secs: default_timeout(),
            rate_limit: None,
            retry: None,
            enable_cache: false,
            cache_ttl_secs: default_cache_ttl(),
        }
    }

    /// Set authentication
    pub fn with_auth(mut self, auth: AuthConfig) -> Self {
        self.auth = auth;
        self
    }

    /// Set timeout
    pub fn with_timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Enable retry with default configuration
    pub fn with_retry(mut self, max_retries: u32) -> Self {
        self.retry = Some(RetryConfig {
            max_retries,
            ..Default::default()
        });
        self
    }

    /// Set rate limit
    pub fn with_rate_limit(mut self, max_requests: u32, window_secs: u64) -> Self {
        self.rate_limit = Some(RateLimitConfig {
            max_requests,
            window_secs,
        });
        self
    }

    /// Enable caching
    pub fn with_cache(mut self, ttl_secs: u64) -> Self {
        self.enable_cache = true;
        self.cache_ttl_secs = ttl_secs;
        self
    }

    /// Add a default header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.insert(key.into(), value.into());
        self
    }
}

/// Cached response entry
struct CacheEntry {
    response: Value,
    expires_at: Instant,
}

/// Rate limiter state
struct RateLimiterState {
    request_times: Vec<Instant>,
}

impl RateLimiterState {
    fn new() -> Self {
        Self {
            request_times: Vec::new(),
        }
    }

    fn can_make_request(&mut self, config: &RateLimitConfig) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(config.window_secs);

        // Remove old requests outside the window
        self.request_times
            .retain(|t| now.duration_since(*t) < window);

        self.request_times.len() < config.max_requests as usize
    }

    fn record_request(&mut self) {
        self.request_times.push(Instant::now());
    }
}

/// REST API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestResponse {
    /// HTTP status code
    pub status: u16,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body (JSON)
    pub body: Value,
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Whether the response was from cache
    pub from_cache: bool,
}

impl RestResponse {
    /// Check if the response is successful (2xx)
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Get a field from the response body
    pub fn get(&self, path: &str) -> Option<&Value> {
        let mut current = &self.body;
        for part in path.split('.') {
            current = current.get(part)?;
        }
        Some(current)
    }
}

/// Generic REST API Connector
pub struct RestConnector {
    config: RestConfig,
    client: Client,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    rate_limiter: Arc<RwLock<RateLimiterState>>,
    oauth_token: Arc<RwLock<Option<(String, Instant)>>>,
}

impl RestConnector {
    /// Create a new REST connector
    pub fn new(config: RestConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();

        Self {
            config,
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            rate_limiter: Arc::new(RwLock::new(RateLimiterState::new())),
            oauth_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Make a GET request
    pub async fn get(&self, path: &str) -> Result<RestResponse, RestConnectorError> {
        self.request(Method::GET, path, None, None).await
    }

    /// Make a GET request with query parameters
    pub async fn get_with_params(
        &self,
        path: &str,
        params: HashMap<String, String>,
    ) -> Result<RestResponse, RestConnectorError> {
        self.request(Method::GET, path, None, Some(params)).await
    }

    /// Make a POST request
    pub async fn post(&self, path: &str, body: Value) -> Result<RestResponse, RestConnectorError> {
        self.request(Method::POST, path, Some(body), None).await
    }

    /// Make a PUT request
    pub async fn put(&self, path: &str, body: Value) -> Result<RestResponse, RestConnectorError> {
        self.request(Method::PUT, path, Some(body), None).await
    }

    /// Make a PATCH request
    pub async fn patch(&self, path: &str, body: Value) -> Result<RestResponse, RestConnectorError> {
        self.request(Method::PATCH, path, Some(body), None).await
    }

    /// Make a DELETE request
    pub async fn delete(&self, path: &str) -> Result<RestResponse, RestConnectorError> {
        self.request(Method::DELETE, path, None, None).await
    }

    /// Make a generic request
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        query_params: Option<HashMap<String, String>>,
    ) -> Result<RestResponse, RestConnectorError> {
        // Check cache for GET requests
        if method == Method::GET && self.config.enable_cache {
            let cache_key = self.cache_key(path, &query_params);
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                if entry.expires_at > Instant::now() {
                    return Ok(RestResponse {
                        status: 200,
                        headers: HashMap::new(),
                        body: entry.response.clone(),
                        response_time_ms: 0,
                        from_cache: true,
                    });
                }
            }
        }

        // Check rate limit
        if let Some(ref rate_limit) = self.config.rate_limit {
            let mut limiter = self.rate_limiter.write().await;
            if !limiter.can_make_request(rate_limit) {
                return Err(RestConnectorError::RateLimitExceeded);
            }
            limiter.record_request();
        }

        // Execute with retry
        let retry_config = self.config.retry.clone().unwrap_or_default();
        let mut last_error = None;
        let mut delay = retry_config.initial_delay_ms;

        for attempt in 0..=retry_config.max_retries {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                delay = (delay as f64 * retry_config.backoff_multiplier) as u64;
                delay = delay.min(retry_config.max_delay_ms);
            }

            match self
                .execute_request(&method, path, body.clone(), query_params.clone())
                .await
            {
                Ok(response) => {
                    // Cache successful GET responses
                    if method == Method::GET && self.config.enable_cache && response.is_success() {
                        let cache_key = self.cache_key(path, &query_params);
                        let mut cache = self.cache.write().await;
                        cache.insert(
                            cache_key,
                            CacheEntry {
                                response: response.body.clone(),
                                expires_at: Instant::now()
                                    + Duration::from_secs(self.config.cache_ttl_secs),
                            },
                        );
                    }
                    return Ok(response);
                }
                Err(e) => {
                    // Check if we should retry this error
                    let should_retry = match &e {
                        RestConnectorError::HttpError { status, .. } => {
                            retry_config.retry_status_codes.contains(status)
                        }
                        RestConnectorError::Timeout => true,
                        _ => false,
                    };

                    if !should_retry || attempt == retry_config.max_retries {
                        return Err(e);
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| RestConnectorError::RequestFailed("Unknown error".into())))
    }

    /// Execute a single request
    async fn execute_request(
        &self,
        method: &Method,
        path: &str,
        body: Option<Value>,
        query_params: Option<HashMap<String, String>>,
    ) -> Result<RestResponse, RestConnectorError> {
        let url = format!("{}{}", self.config.base_url, path);
        let start = Instant::now();

        let mut request = self.client.request(method.clone(), &url);

        // Add default headers
        for (key, value) in &self.config.default_headers {
            request = request.header(key.as_str(), value.as_str());
        }

        // Add authentication
        request = self.apply_auth(request).await?;

        // Add query parameters
        if let Some(params) = query_params {
            request = request.query(&params);
        }

        // Add body
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .json(&body);
        }

        // Execute request
        let response = request
            .send()
            .await
            .map_err(|e| RestConnectorError::RequestFailed(e.to_string()))?;

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let body_text = response
            .text()
            .await
            .map_err(|e| RestConnectorError::ResponseParsingError(e.to_string()))?;

        let body: Value = if body_text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&body_text).unwrap_or_else(|_| json!({"raw": body_text}))
        };

        let response_time_ms = start.elapsed().as_millis() as u64;

        if !StatusCode::from_u16(status)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            .is_success()
        {
            return Err(RestConnectorError::HttpError {
                status,
                message: body
                    .get("message")
                    .or_else(|| body.get("error"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string(),
            });
        }

        Ok(RestResponse {
            status,
            headers,
            body,
            response_time_ms,
            from_cache: false,
        })
    }

    /// Apply authentication to the request
    async fn apply_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, RestConnectorError> {
        match &self.config.auth {
            AuthConfig::None => Ok(request),
            AuthConfig::Bearer { token } => Ok(request.bearer_auth(token)),
            AuthConfig::ApiKey {
                key,
                value,
                in_header,
            } => {
                if *in_header {
                    Ok(request.header(key.as_str(), value.as_str()))
                } else {
                    Ok(request.query(&[(key.as_str(), value.as_str())]))
                }
            }
            AuthConfig::Basic { username, password } => {
                Ok(request.basic_auth(username, Some(password)))
            }
            AuthConfig::OAuth2 {
                client_id,
                client_secret,
                token_url,
                scopes,
            } => {
                // Check if we have a valid cached token
                {
                    let token_guard = self.oauth_token.read().await;
                    if let Some((token, expires)) = token_guard.as_ref() {
                        if *expires > Instant::now() {
                            return Ok(request.bearer_auth(token));
                        }
                    }
                }

                // Get a new token
                let token_response = self
                    .client
                    .post(token_url)
                    .form(&[
                        ("grant_type", "client_credentials"),
                        ("client_id", client_id),
                        ("client_secret", client_secret),
                        ("scope", &scopes.join(" ")),
                    ])
                    .send()
                    .await
                    .map_err(|e| {
                        RestConnectorError::AuthenticationFailed(format!(
                            "Failed to get OAuth2 token: {}",
                            e
                        ))
                    })?;

                let token_data: Value = token_response.json().await.map_err(|e| {
                    RestConnectorError::AuthenticationFailed(format!(
                        "Failed to parse token response: {}",
                        e
                    ))
                })?;

                let access_token = token_data
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        RestConnectorError::AuthenticationFailed(
                            "No access_token in response".into(),
                        )
                    })?
                    .to_string();

                let expires_in = token_data
                    .get("expires_in")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(3600);

                // Cache the token
                {
                    let mut token_guard = self.oauth_token.write().await;
                    *token_guard = Some((
                        access_token.clone(),
                        Instant::now() + Duration::from_secs(expires_in - 60), // Subtract 60s for safety
                    ));
                }

                Ok(request.bearer_auth(access_token))
            }
            AuthConfig::Custom { header, value } => {
                Ok(request.header(header.as_str(), value.as_str()))
            }
        }
    }

    /// Generate cache key
    fn cache_key(&self, path: &str, params: &Option<HashMap<String, String>>) -> String {
        let mut key = format!("{}:{}", self.config.base_url, path);
        if let Some(params) = params {
            let mut sorted_params: Vec<_> = params.iter().collect();
            sorted_params.sort_by_key(|(k, _)| *k);
            for (k, v) in sorted_params {
                key.push_str(&format!("&{}={}", k, v));
            }
        }
        key
    }

    /// Clear the response cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read().await;
        let total = cache.len();
        let valid = cache
            .values()
            .filter(|e| e.expires_at > Instant::now())
            .count();
        (total, valid)
    }
}

/// Builder for creating REST request templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestTemplate {
    /// HTTP method
    pub method: String,
    /// URL path (can contain template variables like {{user_id}})
    pub path: String,
    /// Request headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Request body template (JSON with {{variable}} placeholders)
    pub body: Option<Value>,
    /// Query parameters
    #[serde(default)]
    pub query_params: HashMap<String, String>,
}

impl RequestTemplate {
    /// Create a new GET template
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            method: "GET".to_string(),
            path: path.into(),
            headers: HashMap::new(),
            body: None,
            query_params: HashMap::new(),
        }
    }

    /// Create a new POST template
    pub fn post(path: impl Into<String>, body: Value) -> Self {
        Self {
            method: "POST".to_string(),
            path: path.into(),
            headers: HashMap::new(),
            body: Some(body),
            query_params: HashMap::new(),
        }
    }

    /// Add a header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Add a query parameter
    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query_params.insert(key.into(), value.into());
        self
    }

    /// Render the template with variables
    pub fn render(
        &self,
        variables: &HashMap<String, Value>,
    ) -> (String, Option<Value>, HashMap<String, String>) {
        let path = substitute_variables(&self.path, variables);
        let body = self
            .body
            .as_ref()
            .map(|b| substitute_json_variables(b, variables));
        let query_params: HashMap<String, String> = self
            .query_params
            .iter()
            .map(|(k, v)| (k.clone(), substitute_variables(v, variables)))
            .collect();

        (path, body, query_params)
    }
}

/// Substitute {{variable}} placeholders in a string
fn substitute_variables(template: &str, variables: &HashMap<String, Value>) -> String {
    let mut result = template.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key);
        let replacement = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => value.to_string(),
        };
        result = result.replace(&placeholder, &replacement);
    }
    result
}

/// Substitute {{variable}} placeholders in JSON
fn substitute_json_variables(value: &Value, variables: &HashMap<String, Value>) -> Value {
    match value {
        Value::String(s) => {
            let substituted = substitute_variables(s, variables);
            // Try to parse as JSON if it looks like a complete substitution
            if s.starts_with("{{") && s.ends_with("}}") {
                if let Ok(parsed) = serde_json::from_str(&substituted) {
                    return parsed;
                }
            }
            Value::String(substituted)
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| substitute_json_variables(v, variables))
                .collect(),
        ),
        Value::Object(obj) => Value::Object(
            obj.iter()
                .map(|(k, v)| (k.clone(), substitute_json_variables(v, variables)))
                .collect(),
        ),
        other => other.clone(),
    }
}

// ============================================================================
// GraphQL Support
// ============================================================================

/// GraphQL query request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLQuery {
    /// GraphQL query string
    pub query: String,
    /// Optional operation name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,
    /// Query variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, Value>>,
}

impl GraphQLQuery {
    /// Create a new GraphQL query
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            operation_name: None,
            variables: None,
        }
    }

    /// Set the operation name
    pub fn with_operation(mut self, name: impl Into<String>) -> Self {
        self.operation_name = Some(name.into());
        self
    }

    /// Add variables to the query
    pub fn with_variables(mut self, variables: HashMap<String, Value>) -> Self {
        self.variables = Some(variables);
        self
    }

    /// Add a single variable
    pub fn with_variable(mut self, key: impl Into<String>, value: Value) -> Self {
        self.variables
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value);
        self
    }
}

impl RestConnector {
    /// Execute a GraphQL query
    pub async fn graphql(
        &self,
        endpoint: impl AsRef<str>,
        query: GraphQLQuery,
    ) -> Result<RestResponse, RestConnectorError> {
        let body = serde_json::to_value(&query)
            .map_err(|e| RestConnectorError::SerializationError(e.to_string()))?;

        self.post(endpoint.as_ref(), body).await
    }

    /// Execute a GraphQL mutation
    pub async fn graphql_mutation(
        &self,
        endpoint: impl AsRef<str>,
        mutation: GraphQLQuery,
    ) -> Result<RestResponse, RestConnectorError> {
        // Mutations are handled the same way as queries in GraphQL
        self.graphql(endpoint, mutation).await
    }
}

// ============================================================================
// Circuit Breaker Pattern
// ============================================================================

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are rejected
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failure threshold before opening circuit
    pub failure_threshold: u32,
    /// Success threshold in half-open state to close circuit
    pub success_threshold: u32,
    /// Timeout before transitioning from open to half-open (in seconds)
    pub timeout_secs: u64,
    /// Time window for counting failures (in seconds)
    pub window_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout_secs: 60,
            window_secs: 60,
        }
    }
}

/// Circuit breaker state tracker
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitBreakerState>>,
}

#[derive(Debug)]
struct CircuitBreakerState {
    current_state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    last_state_change: Instant,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(CircuitBreakerState {
                current_state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
                last_state_change: Instant::now(),
            })),
        }
    }

    /// Check if request is allowed
    pub async fn is_request_allowed(&self) -> bool {
        let mut state = self.state.write().await;

        match state.current_state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                let elapsed = state.last_state_change.elapsed().as_secs();
                if elapsed >= self.config.timeout_secs {
                    // Transition to half-open
                    state.current_state = CircuitState::HalfOpen;
                    state.success_count = 0;
                    state.last_state_change = Instant::now();
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful request
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;

        match state.current_state {
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    // Close the circuit
                    state.current_state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.last_state_change = Instant::now();
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success
                state.failure_count = 0;
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        let now = Instant::now();

        // Check if we're in a new time window
        if let Some(last_failure) = state.last_failure_time {
            if now.duration_since(last_failure).as_secs() > self.config.window_secs {
                state.failure_count = 0;
            }
        }

        state.last_failure_time = Some(now);
        state.failure_count += 1;

        match state.current_state {
            CircuitState::Closed => {
                if state.failure_count >= self.config.failure_threshold {
                    // Open the circuit
                    state.current_state = CircuitState::Open;
                    state.last_state_change = now;
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open state opens the circuit again
                state.current_state = CircuitState::Open;
                state.success_count = 0;
                state.last_state_change = now;
            }
            CircuitState::Open => {}
        }
    }

    /// Get current state
    pub async fn get_state(&self) -> CircuitState {
        self.state.read().await.current_state
    }

    /// Reset the circuit breaker
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        state.current_state = CircuitState::Closed;
        state.failure_count = 0;
        state.success_count = 0;
        state.last_failure_time = None;
        state.last_state_change = Instant::now();
    }
}

// ============================================================================
// Request/Response Interceptors
// ============================================================================

/// Request interceptor trait
#[allow(dead_code)]
pub trait RequestInterceptor: Send + Sync {
    /// Intercept and potentially modify the request
    fn intercept(
        &self,
        method: &Method,
        url: &str,
        headers: &mut HashMap<String, String>,
        body: &mut Option<Value>,
    ) -> Result<(), RestConnectorError>;
}

/// Response interceptor trait
#[allow(dead_code)]
pub trait ResponseInterceptor: Send + Sync {
    /// Intercept and potentially modify the response
    fn intercept(&self, response: &mut RestResponse) -> Result<(), RestConnectorError>;
}

/// Logging interceptor for requests
#[derive(Debug, Clone)]
pub struct LoggingInterceptor {
    pub log_request: bool,
    pub log_response: bool,
}

impl LoggingInterceptor {
    pub fn new() -> Self {
        Self {
            log_request: true,
            log_response: true,
        }
    }
}

impl Default for LoggingInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestInterceptor for LoggingInterceptor {
    fn intercept(
        &self,
        method: &Method,
        url: &str,
        _headers: &mut HashMap<String, String>,
        body: &mut Option<Value>,
    ) -> Result<(), RestConnectorError> {
        if self.log_request {
            eprintln!("[REST] {} {}", method, url);
            if let Some(b) = body {
                eprintln!("[REST] Body: {}", b);
            }
        }
        Ok(())
    }
}

impl ResponseInterceptor for LoggingInterceptor {
    fn intercept(&self, response: &mut RestResponse) -> Result<(), RestConnectorError> {
        if self.log_response {
            eprintln!(
                "[REST] Response: {} ({}ms)",
                response.status, response.response_time_ms
            );
        }
        Ok(())
    }
}

/// Header injection interceptor
#[derive(Debug, Clone)]
pub struct HeaderInjectionInterceptor {
    headers: HashMap<String, String>,
}

impl HeaderInjectionInterceptor {
    pub fn new(headers: HashMap<String, String>) -> Self {
        Self { headers }
    }
}

impl RequestInterceptor for HeaderInjectionInterceptor {
    fn intercept(
        &self,
        _method: &Method,
        _url: &str,
        headers: &mut HashMap<String, String>,
        _body: &mut Option<Value>,
    ) -> Result<(), RestConnectorError> {
        headers.extend(self.headers.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rest_config_builder() {
        let config = RestConfig::new("https://api.example.com")
            .with_auth(AuthConfig::bearer("token123"))
            .with_timeout_secs(60)
            .with_retry(3)
            .with_rate_limit(100, 60)
            .with_cache(300);

        assert_eq!(config.base_url, "https://api.example.com");
        assert_eq!(config.timeout_secs, 60);
        assert!(config.retry.is_some());
        assert!(config.rate_limit.is_some());
        assert!(config.enable_cache);
    }

    #[test]
    fn test_auth_config_variants() {
        let bearer = AuthConfig::bearer("token");
        assert!(matches!(bearer, AuthConfig::Bearer { .. }));

        let api_key = AuthConfig::api_key("X-API-Key", "secret");
        assert!(matches!(api_key, AuthConfig::ApiKey { .. }));

        let basic = AuthConfig::basic("user", "pass");
        assert!(matches!(basic, AuthConfig::Basic { .. }));
    }

    #[test]
    fn test_request_template() {
        let template = RequestTemplate::get("/users/{{user_id}}").with_query("limit", "10");

        let mut vars = HashMap::new();
        vars.insert("user_id".to_string(), json!("123"));

        let (path, _, _) = template.render(&vars);
        assert_eq!(path, "/users/123");
    }

    #[test]
    fn test_variable_substitution() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), json!("John"));
        vars.insert("age".to_string(), json!(30));

        let template = "Hello {{name}}, you are {{age}} years old";
        let result = substitute_variables(template, &vars);
        assert_eq!(result, "Hello John, you are 30 years old");
    }

    #[test]
    fn test_json_variable_substitution() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), json!("John"));
        vars.insert("age".to_string(), json!(30));

        let body = json!({
            "name": "{{name}}",
            "age": "{{age}}",
            "items": ["{{name}}", "test"]
        });

        let result = substitute_json_variables(&body, &vars);
        assert_eq!(result["name"], "John");
        assert_eq!(result["items"][0], "John");
    }

    #[test]
    fn test_rate_limiter() {
        let config = RateLimitConfig {
            max_requests: 2,
            window_secs: 60,
        };
        let mut limiter = RateLimiterState::new();

        assert!(limiter.can_make_request(&config));
        limiter.record_request();

        assert!(limiter.can_make_request(&config));
        limiter.record_request();

        assert!(!limiter.can_make_request(&config));
    }

    #[test]
    fn test_rest_response() {
        let response = RestResponse {
            status: 200,
            headers: HashMap::new(),
            body: json!({
                "user": {
                    "name": "John",
                    "email": "john@example.com"
                }
            }),
            response_time_ms: 100,
            from_cache: false,
        };

        assert!(response.is_success());
        assert_eq!(response.get("user.name").unwrap(), &json!("John"));
    }

    #[test]
    fn test_cache_key_generation() {
        let config = RestConfig::new("https://api.example.com");
        let connector = RestConnector::new(config);

        let key1 = connector.cache_key("/users", &None);
        assert_eq!(key1, "https://api.example.com:/users");

        let mut params = HashMap::new();
        params.insert("page".to_string(), "1".to_string());
        params.insert("limit".to_string(), "10".to_string());

        let key2 = connector.cache_key("/users", &Some(params));
        assert!(key2.contains("limit=10"));
        assert!(key2.contains("page=1"));
    }

    #[test]
    fn test_graphql_query_builder() {
        let query = GraphQLQuery::new("query { users { id name } }")
            .with_operation("GetUsers")
            .with_variable("limit", json!(10));

        assert_eq!(query.query, "query { users { id name } }");
        assert_eq!(query.operation_name, Some("GetUsers".to_string()));
        assert!(query.variables.is_some());
        assert_eq!(query.variables.unwrap().get("limit"), Some(&json!(10)));
    }

    #[test]
    fn test_graphql_query_with_variables() {
        let mut vars = HashMap::new();
        vars.insert("id".to_string(), json!("123"));
        vars.insert("status".to_string(), json!("active"));

        let query = GraphQLQuery::new(
            "query($id: ID!, $status: String) { user(id: $id, status: $status) { name } }",
        )
        .with_variables(vars.clone());

        assert!(query.variables.is_some());
        assert_eq!(query.variables.as_ref().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closed_state() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_secs: 60,
            window_secs: 60,
        };
        let breaker = CircuitBreaker::new(config);

        assert!(breaker.is_request_allowed().await);
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_secs: 1,
            window_secs: 60,
        };
        let breaker = CircuitBreaker::new(config);

        // Record 3 failures to open the circuit
        breaker.record_failure().await;
        breaker.record_failure().await;
        breaker.record_failure().await;

        assert_eq!(breaker.get_state().await, CircuitState::Open);
        assert!(!breaker.is_request_allowed().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout_secs: 1, // 1 second timeout for quick test
            window_secs: 60,
        };
        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.record_failure().await;
        breaker.record_failure().await;
        assert_eq!(breaker.get_state().await, CircuitState::Open);

        // Wait for timeout to transition to half-open
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Should allow request and transition to half-open
        assert!(breaker.is_request_allowed().await);
        assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes_after_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout_secs: 1,
            window_secs: 60,
        };
        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.record_failure().await;
        breaker.record_failure().await;
        assert_eq!(breaker.get_state().await, CircuitState::Open);

        // Wait and transition to half-open
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert!(breaker.is_request_allowed().await);

        // Record successes to close the circuit
        breaker.record_success().await;
        breaker.record_success().await;

        assert_eq!(breaker.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_reset() {
        let config = CircuitBreakerConfig::default();
        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        for _ in 0..5 {
            breaker.record_failure().await;
        }
        assert_eq!(breaker.get_state().await, CircuitState::Open);

        // Reset
        breaker.reset().await;
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
        assert!(breaker.is_request_allowed().await);
    }

    #[test]
    fn test_logging_interceptor() {
        let interceptor = LoggingInterceptor::new();
        assert!(interceptor.log_request);
        assert!(interceptor.log_response);
    }

    #[test]
    fn test_header_injection_interceptor() {
        let mut headers_to_inject = HashMap::new();
        headers_to_inject.insert("X-Custom-Header".to_string(), "value123".to_string());

        let interceptor = HeaderInjectionInterceptor::new(headers_to_inject);

        let mut request_headers = HashMap::new();
        let mut body = None;

        interceptor
            .intercept(
                &Method::GET,
                "https://example.com",
                &mut request_headers,
                &mut body,
            )
            .unwrap();

        assert_eq!(
            request_headers.get("X-Custom-Header"),
            Some(&"value123".to_string())
        );
    }
}
