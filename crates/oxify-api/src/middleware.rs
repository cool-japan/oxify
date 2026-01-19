//! API middleware
//!
//! Custom rate limiting implementation using token bucket algorithm.
//! Compatible with axum 0.8.7 and tower 0.5.2.

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use futures::future::BoxFuture;
use std::{
    collections::HashMap,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tower::{Layer, Service};

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Time window duration
    pub window: Duration,
    /// Enable per-IP rate limiting (vs global)
    pub per_ip: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,               // 100 requests
            window: Duration::from_secs(60), // per minute
            per_ip: true,
        }
    }
}

impl RateLimitConfig {
    /// Create config for small deployment (100 req/min per IP)
    #[allow(dead_code)]
    pub fn small() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
            per_ip: true,
        }
    }

    /// Create config for medium deployment (500 req/min per IP)
    pub fn medium() -> Self {
        Self {
            max_requests: 500,
            window: Duration::from_secs(60),
            per_ip: true,
        }
    }

    /// Create config for large deployment (1000 req/min per IP)
    #[allow(dead_code)]
    pub fn large() -> Self {
        Self {
            max_requests: 1000,
            window: Duration::from_secs(60),
            per_ip: true,
        }
    }

    /// Create global rate limit (not per-IP)
    #[allow(dead_code)]
    pub fn global(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            per_ip: false,
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: Instant,
    capacity: f64,
    refill_rate: f64, // tokens per second
}

impl TokenBucket {
    fn new(capacity: u32, window: Duration) -> Self {
        let capacity_f64 = capacity as f64;
        let refill_rate = capacity_f64 / window.as_secs_f64();

        Self {
            tokens: capacity_f64,
            last_update: Instant::now(),
            capacity: capacity_f64,
            refill_rate,
        }
    }

    fn try_consume(&mut self) -> bool {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_update = now;
    }

    fn remaining(&self) -> u32 {
        self.tokens.floor() as u32
    }

    fn time_until_reset(&self) -> Duration {
        if self.tokens >= self.capacity {
            Duration::from_secs(0)
        } else {
            let tokens_needed = self.capacity - self.tokens;
            let seconds = tokens_needed / self.refill_rate;
            Duration::from_secs_f64(seconds)
        }
    }
}

/// Rate limit check result
#[derive(Debug)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub limit: u32,
    pub remaining: u32,
    pub reset: Duration,
}

/// Rate limiter state
#[derive(Clone)]
pub struct RateLimiterState {
    config: RateLimitConfig,
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

impl RateLimiterState {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn check_rate_limit(&self, key: &str) -> RateLimitResult {
        let mut buckets = self.buckets.write().await;

        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.max_requests, self.config.window));

        let allowed = bucket.try_consume();
        let remaining = bucket.remaining();
        let reset = bucket.time_until_reset();

        RateLimitResult {
            allowed,
            limit: self.config.max_requests,
            remaining,
            reset,
        }
    }

    /// Clean up old buckets (call periodically)
    #[allow(dead_code)]
    pub async fn cleanup(&self) {
        let mut buckets = self.buckets.write().await;
        let cutoff = Instant::now() - self.config.window * 2;

        buckets.retain(|_, bucket| bucket.last_update > cutoff);
    }
}

/// Extract real IP address from request headers
fn extract_real_ip(headers: &HeaderMap) -> String {
    // Try X-Forwarded-For first (standard proxy header)
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(value) = forwarded.to_str() {
            // X-Forwarded-For can contain multiple IPs, take the first (client IP)
            if let Some(ip) = value.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }

    // Try X-Real-IP (used by some reverse proxies)
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            return value.to_string();
        }
    }

    // Fallback to a default key if we can't extract IP
    "unknown".to_string()
}

/// Tower layer for rate limiting
#[derive(Clone)]
pub struct RateLimitLayer {
    state: RateLimiterState,
}

impl RateLimitLayer {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            state: RateLimiterState::new(config),
        }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// Tower service for rate limiting
#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    state: RateLimiterState,
}

impl<S> Service<Request> for RateLimitService<S>
where
    S: Service<Request, Response = Response> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let state = self.state.clone();
        let per_ip = state.config.per_ip;

        // Extract IP before moving req
        let headers = req.headers();
        let key = if per_ip {
            extract_real_ip(headers)
        } else {
            "global".to_string()
        };

        let future = self.inner.call(req);

        Box::pin(async move {
            // Check rate limit
            let result = state.check_rate_limit(&key).await;

            // Create rate limit headers
            let mut headers = HeaderMap::new();
            if let Ok(limit) = HeaderValue::from_str(&result.limit.to_string()) {
                headers.insert("X-RateLimit-Limit", limit);
            }
            if let Ok(remaining) = HeaderValue::from_str(&result.remaining.to_string()) {
                headers.insert("X-RateLimit-Remaining", remaining);
            }
            if let Ok(reset) = HeaderValue::from_str(&result.reset.as_secs().to_string()) {
                headers.insert("X-RateLimit-Reset", reset);
            }

            if !result.allowed {
                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    "Rate limit exceeded. Please try again later.",
                )
                    .into_response();

                // Add rate limit headers to error response
                response.headers_mut().extend(headers);
                return Ok(response);
            }

            // Process request and add headers to successful response
            let mut response = future.await?;
            response.headers_mut().extend(headers);
            Ok(response)
        })
    }
}

// ==================== API Versioning Middleware ====================

/// API version information
#[derive(Debug, Clone)]
pub struct ApiVersion {
    /// Current API version (e.g., "v1")
    pub version: String,
    /// Is this version deprecated?
    pub deprecated: bool,
    /// Sunset date (when deprecated version will be removed) in RFC 3339 format
    pub sunset_date: Option<String>,
    /// Link to migration guide for deprecated versions
    pub migration_guide: Option<String>,
}

impl ApiVersion {
    /// Create current stable version
    pub fn v1() -> Self {
        Self {
            version: "v1".to_string(),
            deprecated: false,
            sunset_date: None,
            migration_guide: None,
        }
    }

    /// Create deprecated version with sunset date
    #[allow(dead_code)]
    pub fn deprecated(
        version: String,
        sunset_date: impl Into<String>,
        migration_guide: Option<String>,
    ) -> Self {
        Self {
            version,
            deprecated: true,
            sunset_date: Some(sunset_date.into()),
            migration_guide,
        }
    }
}

/// Layer for adding API version headers
#[derive(Clone)]
pub struct ApiVersionLayer {
    version: ApiVersion,
}

impl ApiVersionLayer {
    pub fn new(version: ApiVersion) -> Self {
        Self { version }
    }
}

impl<S> Layer<S> for ApiVersionLayer {
    type Service = ApiVersionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiVersionService {
            inner,
            version: self.version.clone(),
        }
    }
}

/// Service for adding API version headers
#[derive(Clone)]
pub struct ApiVersionService<S> {
    inner: S,
    version: ApiVersion,
}

impl<S> Service<Request> for ApiVersionService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let version = self.version.clone();
        let future = self.inner.call(req);

        Box::pin(async move {
            let mut response = future.await?;
            let headers = response.headers_mut();

            // Add API version header
            if let Ok(version_header) = HeaderValue::from_str(&version.version) {
                headers.insert("X-API-Version", version_header);
            }

            // Add deprecation headers if version is deprecated
            if version.deprecated {
                headers.insert("Deprecation", HeaderValue::from_static("true"));

                if let Some(sunset) = &version.sunset_date {
                    if let Ok(sunset_header) = HeaderValue::from_str(sunset) {
                        headers.insert("Sunset", sunset_header);
                    }
                }

                if let Some(guide) = &version.migration_guide {
                    if let Ok(link_header) =
                        HeaderValue::from_str(&format!("<{}>; rel=\"deprecation\"", guide))
                    {
                        headers.insert("Link", link_header);
                    }
                }

                // Add deprecation warning in a custom header
                let warning = format!(
                    "299 - \"API version {} is deprecated{}\"",
                    version.version,
                    version
                        .sunset_date
                        .as_ref()
                        .map(|d| format!(". Will be removed on {}", d))
                        .unwrap_or_default()
                );
                if let Ok(warning_header) = HeaderValue::from_str(&warning) {
                    headers.insert("Warning", warning_header);
                }
            }

            Ok(response)
        })
    }
}

// ==================== HTTP Metrics Middleware ====================

use std::sync::atomic::{AtomicU64, Ordering};

/// Per-endpoint metrics
#[derive(Debug, Default)]
struct EndpointMetrics {
    /// Total requests for this endpoint
    count: AtomicU64,
    /// Total duration for this endpoint
    total_duration_ms: AtomicU64,
    /// Status code counts
    status_2xx: AtomicU64,
    status_3xx: AtomicU64,
    status_4xx: AtomicU64,
    status_5xx: AtomicU64,
}

impl EndpointMetrics {
    fn new() -> Self {
        Self::default()
    }

    fn record(&self, status: u16, duration_ms: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_duration_ms
            .fetch_add(duration_ms, Ordering::Relaxed);

        match status {
            200..=299 => {
                self.status_2xx.fetch_add(1, Ordering::Relaxed);
            }
            300..=399 => {
                self.status_3xx.fetch_add(1, Ordering::Relaxed);
            }
            400..=499 => {
                self.status_4xx.fetch_add(1, Ordering::Relaxed);
            }
            500..=599 => {
                self.status_5xx.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    fn get_stats(&self) -> (u64, f64, u64, u64, u64, u64) {
        let count = self.count.load(Ordering::Relaxed);
        let total_duration = self.total_duration_ms.load(Ordering::Relaxed);
        let avg_duration = if count > 0 {
            total_duration as f64 / count as f64
        } else {
            0.0
        };
        let status_2xx = self.status_2xx.load(Ordering::Relaxed);
        let status_3xx = self.status_3xx.load(Ordering::Relaxed);
        let status_4xx = self.status_4xx.load(Ordering::Relaxed);
        let status_5xx = self.status_5xx.load(Ordering::Relaxed);

        (
            count,
            avg_duration,
            status_2xx,
            status_3xx,
            status_4xx,
            status_5xx,
        )
    }
}

/// HTTP request metrics for Prometheus
#[derive(Debug)]
pub struct HttpMetrics {
    /// Total HTTP requests
    requests_total: AtomicU64,
    /// Requests by status code (2xx, 3xx, 4xx, 5xx)
    requests_2xx: AtomicU64,
    requests_3xx: AtomicU64,
    requests_4xx: AtomicU64,
    requests_5xx: AtomicU64,
    /// Active requests (currently being processed)
    active_requests: AtomicU64,
    /// Total request duration in milliseconds (for average calculation)
    total_duration_ms: AtomicU64,
    /// Request count for duration tracking
    duration_sample_count: AtomicU64,
    /// Requests by HTTP method
    requests_get: AtomicU64,
    requests_post: AtomicU64,
    requests_put: AtomicU64,
    requests_delete: AtomicU64,
    requests_patch: AtomicU64,
    requests_options: AtomicU64,
    requests_head: AtomicU64,
    requests_other: AtomicU64,
    /// Active workflow executions (currently running)
    active_executions: AtomicU64,
    /// Per-endpoint metrics
    endpoint_metrics: Arc<RwLock<HashMap<String, Arc<EndpointMetrics>>>>,
}

impl Default for HttpMetrics {
    fn default() -> Self {
        Self {
            requests_total: AtomicU64::new(0),
            requests_2xx: AtomicU64::new(0),
            requests_3xx: AtomicU64::new(0),
            requests_4xx: AtomicU64::new(0),
            requests_5xx: AtomicU64::new(0),
            active_requests: AtomicU64::new(0),
            total_duration_ms: AtomicU64::new(0),
            duration_sample_count: AtomicU64::new(0),
            requests_get: AtomicU64::new(0),
            requests_post: AtomicU64::new(0),
            requests_put: AtomicU64::new(0),
            requests_delete: AtomicU64::new(0),
            requests_patch: AtomicU64::new(0),
            requests_options: AtomicU64::new(0),
            requests_head: AtomicU64::new(0),
            requests_other: AtomicU64::new(0),
            active_executions: AtomicU64::new(0),
            endpoint_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl HttpMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Normalize endpoint path by replacing IDs with placeholders
    /// Examples:
    /// - /api/v1/workflows/123 -> /api/v1/workflows/{id}
    /// - /api/v1/executions/abc-def-123 -> /api/v1/executions/{id}
    fn normalize_path(path: &str) -> String {
        let segments: Vec<&str> = path.split('/').collect();
        let mut normalized = Vec::new();

        for (i, segment) in segments.iter().enumerate() {
            if segment.is_empty() {
                normalized.push("");
                continue;
            }

            // Check if segment looks like an ID (UUID, number, or alphanumeric)
            let is_id = segment.parse::<u64>().is_ok() // numeric ID
                || segment.contains('-') // UUID-like
                || (i > 0 && segments.get(i - 1).is_some() &&
                    (segments[i - 1].ends_with("workflows")
                     || segments[i - 1].ends_with("executions")
                     || segments[i - 1].ends_with("schedules")
                     || segments[i - 1].ends_with("webhooks")
                     || segments[i - 1].ends_with("secrets")
                     || segments[i - 1].ends_with("templates")
                     || segments[i - 1].ends_with("snapshots")));

            if is_id {
                normalized.push("{id}");
            } else {
                normalized.push(segment);
            }
        }

        normalized.join("/")
    }

    /// Record a request with its status code, duration, endpoint path, and HTTP method
    pub fn record_request(&self, status: u16, duration_ms: u64, path: &str, method: &str) {
        // Record global metrics
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.total_duration_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        self.duration_sample_count.fetch_add(1, Ordering::Relaxed);

        // Track by status code category
        match status {
            200..=299 => {
                self.requests_2xx.fetch_add(1, Ordering::Relaxed);
            }
            300..=399 => {
                self.requests_3xx.fetch_add(1, Ordering::Relaxed);
            }
            400..=499 => {
                self.requests_4xx.fetch_add(1, Ordering::Relaxed);
            }
            500..=599 => {
                self.requests_5xx.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }

        // Track by HTTP method
        match method.to_uppercase().as_str() {
            "GET" => {
                self.requests_get.fetch_add(1, Ordering::Relaxed);
            }
            "POST" => {
                self.requests_post.fetch_add(1, Ordering::Relaxed);
            }
            "PUT" => {
                self.requests_put.fetch_add(1, Ordering::Relaxed);
            }
            "DELETE" => {
                self.requests_delete.fetch_add(1, Ordering::Relaxed);
            }
            "PATCH" => {
                self.requests_patch.fetch_add(1, Ordering::Relaxed);
            }
            "OPTIONS" => {
                self.requests_options.fetch_add(1, Ordering::Relaxed);
            }
            "HEAD" => {
                self.requests_head.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.requests_other.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Record per-endpoint metrics
        let normalized_path = Self::normalize_path(path);
        let endpoint_metrics = self.endpoint_metrics.clone();

        // Use async block to avoid holding the lock too long
        tokio::spawn(async move {
            let mut metrics = endpoint_metrics.write().await;
            let entry = metrics
                .entry(normalized_path)
                .or_insert_with(|| Arc::new(EndpointMetrics::new()));
            entry.record(status, duration_ms);
        });
    }

    /// Increment active requests counter
    pub fn inc_active(&self) {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active requests counter
    pub fn dec_active(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// Increment active executions counter
    pub fn inc_active_execution(&self) {
        self.active_executions.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active executions counter
    pub fn dec_active_execution(&self) {
        self.active_executions.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get Prometheus-formatted metrics (async version)
    pub async fn to_prometheus_format(&self) -> String {
        let total = self.requests_total.load(Ordering::Relaxed);
        let requests_2xx = self.requests_2xx.load(Ordering::Relaxed);
        let requests_3xx = self.requests_3xx.load(Ordering::Relaxed);
        let requests_4xx = self.requests_4xx.load(Ordering::Relaxed);
        let requests_5xx = self.requests_5xx.load(Ordering::Relaxed);
        let active = self.active_requests.load(Ordering::Relaxed);
        let total_duration = self.total_duration_ms.load(Ordering::Relaxed);
        let sample_count = self.duration_sample_count.load(Ordering::Relaxed);

        // Load per-method counts
        let requests_get = self.requests_get.load(Ordering::Relaxed);
        let requests_post = self.requests_post.load(Ordering::Relaxed);
        let requests_put = self.requests_put.load(Ordering::Relaxed);
        let requests_delete = self.requests_delete.load(Ordering::Relaxed);
        let requests_patch = self.requests_patch.load(Ordering::Relaxed);
        let requests_options = self.requests_options.load(Ordering::Relaxed);
        let requests_head = self.requests_head.load(Ordering::Relaxed);
        let requests_other = self.requests_other.load(Ordering::Relaxed);

        // Load execution metrics
        let active_executions = self.active_executions.load(Ordering::Relaxed);

        let avg_duration = if sample_count > 0 {
            total_duration as f64 / sample_count as f64
        } else {
            0.0
        };

        let error_rate = if total > 0 {
            (requests_4xx + requests_5xx) as f64 / total as f64
        } else {
            0.0
        };

        let mut output = format!(
            "# HELP oxify_http_requests_total Total number of HTTP requests\n\
             # TYPE oxify_http_requests_total counter\n\
             oxify_http_requests_total {}\n\
             \n\
             # HELP oxify_http_requests_by_status HTTP requests by status code category\n\
             # TYPE oxify_http_requests_by_status counter\n\
             oxify_http_requests_by_status{{status=\"2xx\"}} {}\n\
             oxify_http_requests_by_status{{status=\"3xx\"}} {}\n\
             oxify_http_requests_by_status{{status=\"4xx\"}} {}\n\
             oxify_http_requests_by_status{{status=\"5xx\"}} {}\n\
             \n\
             # HELP oxify_http_requests_by_method HTTP requests by method\n\
             # TYPE oxify_http_requests_by_method counter\n\
             oxify_http_requests_by_method{{method=\"GET\"}} {}\n\
             oxify_http_requests_by_method{{method=\"POST\"}} {}\n\
             oxify_http_requests_by_method{{method=\"PUT\"}} {}\n\
             oxify_http_requests_by_method{{method=\"DELETE\"}} {}\n\
             oxify_http_requests_by_method{{method=\"PATCH\"}} {}\n\
             oxify_http_requests_by_method{{method=\"OPTIONS\"}} {}\n\
             oxify_http_requests_by_method{{method=\"HEAD\"}} {}\n\
             oxify_http_requests_by_method{{method=\"OTHER\"}} {}\n\
             \n\
             # HELP oxify_http_requests_active Currently active HTTP requests\n\
             # TYPE oxify_http_requests_active gauge\n\
             oxify_http_requests_active {}\n\
             \n\
             # HELP oxify_http_request_duration_ms_avg Average HTTP request duration in milliseconds\n\
             # TYPE oxify_http_request_duration_ms_avg gauge\n\
             oxify_http_request_duration_ms_avg {:.2}\n\
             \n\
             # HELP oxify_http_error_rate HTTP error rate (4xx + 5xx / total)\n\
             # TYPE oxify_http_error_rate gauge\n\
             oxify_http_error_rate {:.4}\n\
             \n\
             # HELP oxify_active_executions Currently active workflow executions\n\
             # TYPE oxify_active_executions gauge\n\
             oxify_active_executions {}\n",
            total,
            requests_2xx,
            requests_3xx,
            requests_4xx,
            requests_5xx,
            requests_get,
            requests_post,
            requests_put,
            requests_delete,
            requests_patch,
            requests_options,
            requests_head,
            requests_other,
            active,
            avg_duration,
            error_rate,
            active_executions
        );

        // Add per-endpoint metrics
        let endpoint_metrics = self.endpoint_metrics.read().await;
        if !endpoint_metrics.is_empty() {
            output.push_str(
                "\n# HELP oxify_http_requests_by_endpoint HTTP requests by endpoint\n\
                 # TYPE oxify_http_requests_by_endpoint counter\n",
            );

            for (endpoint, metrics) in endpoint_metrics.iter() {
                let (count, avg_dur, status_2xx, status_3xx, status_4xx, status_5xx) =
                    metrics.get_stats();
                output.push_str(&format!(
                    "oxify_http_requests_by_endpoint{{endpoint=\"{}\"}} {}\n",
                    endpoint, count
                ));

                // Add per-endpoint duration
                output.push_str(&format!(
                    "# HELP oxify_http_request_duration_by_endpoint_ms_avg Average request duration by endpoint\n\
                     # TYPE oxify_http_request_duration_by_endpoint_ms_avg gauge\n\
                     oxify_http_request_duration_by_endpoint_ms_avg{{endpoint=\"{}\"}} {:.2}\n",
                    endpoint, avg_dur
                ));

                // Add per-endpoint status codes
                if status_2xx > 0 {
                    output.push_str(&format!(
                        "oxify_http_requests_by_endpoint_status{{endpoint=\"{}\",status=\"2xx\"}} {}\n",
                        endpoint, status_2xx
                    ));
                }
                if status_3xx > 0 {
                    output.push_str(&format!(
                        "oxify_http_requests_by_endpoint_status{{endpoint=\"{}\",status=\"3xx\"}} {}\n",
                        endpoint, status_3xx
                    ));
                }
                if status_4xx > 0 {
                    output.push_str(&format!(
                        "oxify_http_requests_by_endpoint_status{{endpoint=\"{}\",status=\"4xx\"}} {}\n",
                        endpoint, status_4xx
                    ));
                }
                if status_5xx > 0 {
                    output.push_str(&format!(
                        "oxify_http_requests_by_endpoint_status{{endpoint=\"{}\",status=\"5xx\"}} {}\n",
                        endpoint, status_5xx
                    ));
                }
            }
        }

        output
    }
}

/// Layer for HTTP metrics collection
#[derive(Clone)]
pub struct HttpMetricsLayer {
    pub metrics: Arc<HttpMetrics>,
}

impl HttpMetricsLayer {
    pub fn new(metrics: Arc<HttpMetrics>) -> Self {
        Self { metrics }
    }
}

impl<S> Layer<S> for HttpMetricsLayer {
    type Service = HttpMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpMetricsService {
            inner,
            metrics: self.metrics.clone(),
        }
    }
}

/// Service for HTTP metrics collection
#[derive(Clone)]
pub struct HttpMetricsService<S> {
    inner: S,
    metrics: Arc<HttpMetrics>,
}

impl<S> Service<Request> for HttpMetricsService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let metrics = self.metrics.clone();
        let start_time = Instant::now();

        // Extract request path and method before moving req
        let path = req.uri().path().to_string();
        let method = req.method().to_string();

        // Increment active requests
        metrics.inc_active();

        let future = self.inner.call(req);

        Box::pin(async move {
            let response = future.await;

            // Decrement active requests
            metrics.dec_active();

            // Record metrics
            let duration_ms = start_time.elapsed().as_millis() as u64;

            match &response {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    metrics.record_request(status, duration_ms, &path, &method);
                }
                Err(_) => {
                    // Record as 500 for error responses
                    metrics.record_request(500, duration_ms, &path, &method);
                }
            }

            response
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10, Duration::from_secs(60));

        // Should allow 10 requests
        for _ in 0..10 {
            assert!(bucket.try_consume());
        }

        // 11th request should fail
        assert!(!bucket.try_consume());
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(2, Duration::from_secs(1));

        // Consume all tokens
        assert!(bucket.try_consume());
        assert!(bucket.try_consume());
        assert!(!bucket.try_consume());

        // Wait for refill (simulate by manipulating last_update)
        bucket.last_update = Instant::now() - Duration::from_secs(1);
        bucket.refill();

        // Should have tokens again
        assert!(bucket.try_consume());
    }

    #[tokio::test]
    async fn test_rate_limiter_state() {
        let config = RateLimitConfig {
            max_requests: 5,
            window: Duration::from_secs(60),
            per_ip: true,
        };
        let state = RateLimiterState::new(config);

        // Should allow 5 requests
        for i in 0..5 {
            let result = state.check_rate_limit("test-key").await;
            assert!(result.allowed, "Request {} should be allowed", i + 1);
            assert_eq!(result.limit, 5);
            assert_eq!(result.remaining, 4 - i as u32);
        }

        // 6th request should fail
        let result = state.check_rate_limit("test-key").await;
        assert!(!result.allowed, "6th request should be denied");
        assert_eq!(result.remaining, 0);

        // Different key should work
        let result = state.check_rate_limit("other-key").await;
        assert!(result.allowed, "Different key should be allowed");
        assert_eq!(result.remaining, 4);
    }

    #[test]
    fn test_api_version_v1() {
        let version = ApiVersion::v1();
        assert_eq!(version.version, "v1");
        assert!(!version.deprecated);
        assert!(version.sunset_date.is_none());
        assert!(version.migration_guide.is_none());
    }

    #[test]
    fn test_api_version_deprecated() {
        let version = ApiVersion::deprecated(
            "v0".to_string(),
            "2026-01-31T00:00:00Z",
            Some("https://example.com/migration".to_string()),
        );
        assert_eq!(version.version, "v0");
        assert!(version.deprecated);
        assert_eq!(
            version.sunset_date,
            Some("2026-01-31T00:00:00Z".to_string())
        );
        assert_eq!(
            version.migration_guide,
            Some("https://example.com/migration".to_string())
        );
    }

    #[tokio::test]
    async fn test_http_metrics_basic() {
        let metrics = HttpMetrics::new();

        // Record some requests
        metrics.record_request(200, 100, "/api/v1/workflows", "GET");
        metrics.record_request(404, 50, "/api/v1/executions/123", "GET");
        metrics.record_request(500, 200, "/health", "POST");

        // Wait for async recording to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Verify counts
        assert_eq!(metrics.requests_total.load(Ordering::Relaxed), 3);
        assert_eq!(metrics.requests_2xx.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.requests_4xx.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.requests_5xx.load(Ordering::Relaxed), 1);

        // Verify method tracking
        assert_eq!(metrics.requests_get.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.requests_post.load(Ordering::Relaxed), 1);

        // Verify duration tracking
        assert_eq!(metrics.total_duration_ms.load(Ordering::Relaxed), 350);
        assert_eq!(metrics.duration_sample_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_http_metrics_active_tracking() {
        let metrics = HttpMetrics::new();

        assert_eq!(metrics.active_requests.load(Ordering::Relaxed), 0);

        metrics.inc_active();
        assert_eq!(metrics.active_requests.load(Ordering::Relaxed), 1);

        metrics.inc_active();
        assert_eq!(metrics.active_requests.load(Ordering::Relaxed), 2);

        metrics.dec_active();
        assert_eq!(metrics.active_requests.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_http_metrics_prometheus_format() {
        let metrics = HttpMetrics::new();

        metrics.record_request(200, 100, "/api/v1/workflows", "GET");
        metrics.record_request(200, 200, "/api/v1/workflows", "POST");
        metrics.record_request(404, 50, "/api/v1/executions/123", "GET");
        metrics.record_request(500, 300, "/health", "DELETE");

        // Wait a bit for async recording to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let output = metrics.to_prometheus_format().await;

        // Check that output contains expected metrics
        assert!(output.contains("oxify_http_requests_total 4"));
        assert!(output.contains("oxify_http_requests_by_status{status=\"2xx\"} 2"));
        assert!(output.contains("oxify_http_requests_by_status{status=\"4xx\"} 1"));
        assert!(output.contains("oxify_http_requests_by_status{status=\"5xx\"} 1"));
        assert!(output.contains("oxify_http_request_duration_ms_avg 162.50"));
        assert!(output.contains("oxify_http_error_rate 0.5000"));

        // Check per-method metrics
        assert!(output.contains("oxify_http_requests_by_method{method=\"GET\"} 2"));
        assert!(output.contains("oxify_http_requests_by_method{method=\"POST\"} 1"));
        assert!(output.contains("oxify_http_requests_by_method{method=\"DELETE\"} 1"));
    }

    #[test]
    fn test_path_normalization() {
        // Test numeric IDs
        assert_eq!(
            HttpMetrics::normalize_path("/api/v1/workflows/123"),
            "/api/v1/workflows/{id}"
        );

        // Test UUID-like IDs
        assert_eq!(
            HttpMetrics::normalize_path("/api/v1/executions/abc-def-123"),
            "/api/v1/executions/{id}"
        );

        // Test non-ID paths
        assert_eq!(
            HttpMetrics::normalize_path("/api/v1/workflows"),
            "/api/v1/workflows"
        );

        // Test health endpoint
        assert_eq!(HttpMetrics::normalize_path("/health"), "/health");

        // Test nested IDs
        assert_eq!(
            HttpMetrics::normalize_path("/api/v1/workflows/123/executions/456"),
            "/api/v1/workflows/{id}/executions/{id}"
        );
    }

    #[tokio::test]
    async fn test_per_endpoint_metrics() {
        let metrics = HttpMetrics::new();

        // Record requests to different endpoints
        metrics.record_request(200, 100, "/api/v1/workflows", "GET");
        metrics.record_request(200, 150, "/api/v1/workflows", "POST");
        metrics.record_request(200, 50, "/api/v1/workflows/123", "GET"); // Normalizes to /api/v1/workflows/{id}
        metrics.record_request(404, 75, "/api/v1/workflows/456", "DELETE"); // Also normalizes to /api/v1/workflows/{id}
        metrics.record_request(500, 200, "/health", "GET");

        // Wait for async recording to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let output = metrics.to_prometheus_format().await;

        // Check per-endpoint metrics are present
        assert!(output.contains("oxify_http_requests_by_endpoint"));
        assert!(output.contains("/api/v1/workflows"));
        assert!(output.contains("/api/v1/workflows/{id}"));
        assert!(output.contains("/health"));
    }

    #[tokio::test]
    async fn test_http_method_metrics() {
        let metrics = HttpMetrics::new();

        // Record requests with different HTTP methods
        metrics.record_request(200, 100, "/api/v1/workflows", "GET");
        metrics.record_request(200, 150, "/api/v1/workflows", "GET");
        metrics.record_request(201, 200, "/api/v1/workflows", "POST");
        metrics.record_request(200, 120, "/api/v1/workflows/123", "PUT");
        metrics.record_request(204, 80, "/api/v1/workflows/123", "DELETE");
        metrics.record_request(200, 90, "/api/v1/workflows/456", "PATCH");
        metrics.record_request(200, 50, "/api/v1/workflows", "OPTIONS");
        metrics.record_request(200, 30, "/api/v1/workflows", "HEAD");

        // Wait for async recording to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify method counts
        assert_eq!(metrics.requests_get.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.requests_post.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.requests_put.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.requests_delete.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.requests_patch.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.requests_options.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.requests_head.load(Ordering::Relaxed), 1);

        // Check Prometheus output
        let output = metrics.to_prometheus_format().await;
        assert!(output.contains("oxify_http_requests_by_method{method=\"GET\"} 2"));
        assert!(output.contains("oxify_http_requests_by_method{method=\"POST\"} 1"));
        assert!(output.contains("oxify_http_requests_by_method{method=\"PUT\"} 1"));
        assert!(output.contains("oxify_http_requests_by_method{method=\"DELETE\"} 1"));
        assert!(output.contains("oxify_http_requests_by_method{method=\"PATCH\"} 1"));
        assert!(output.contains("oxify_http_requests_by_method{method=\"OPTIONS\"} 1"));
        assert!(output.contains("oxify_http_requests_by_method{method=\"HEAD\"} 1"));
    }

    #[tokio::test]
    async fn test_http_method_case_insensitive() {
        let metrics = HttpMetrics::new();

        // Record requests with different case variations
        metrics.record_request(200, 100, "/api/v1/workflows", "get");
        metrics.record_request(200, 150, "/api/v1/workflows", "GET");
        metrics.record_request(201, 200, "/api/v1/workflows", "Post");
        metrics.record_request(200, 120, "/api/v1/workflows", "post");

        // Wait for async recording to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Should normalize to uppercase and count correctly
        assert_eq!(metrics.requests_get.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.requests_post.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_http_method_other() {
        let metrics = HttpMetrics::new();

        // Record requests with uncommon HTTP methods
        metrics.record_request(200, 100, "/api/v1/workflows", "CONNECT");
        metrics.record_request(200, 150, "/api/v1/workflows", "TRACE");
        metrics.record_request(200, 200, "/api/v1/workflows", "CUSTOM");

        // Wait for async recording to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Should count as "other"
        assert_eq!(metrics.requests_other.load(Ordering::Relaxed), 3);

        // Check Prometheus output
        let output = metrics.to_prometheus_format().await;
        assert!(output.contains("oxify_http_requests_by_method{method=\"OTHER\"} 3"));
    }

    #[test]
    fn test_active_executions_basic() {
        let metrics = HttpMetrics::new();

        // Should start at 0
        assert_eq!(metrics.active_executions.load(Ordering::Relaxed), 0);

        // Increment
        metrics.inc_active_execution();
        assert_eq!(metrics.active_executions.load(Ordering::Relaxed), 1);

        metrics.inc_active_execution();
        assert_eq!(metrics.active_executions.load(Ordering::Relaxed), 2);

        // Decrement
        metrics.dec_active_execution();
        assert_eq!(metrics.active_executions.load(Ordering::Relaxed), 1);

        metrics.dec_active_execution();
        assert_eq!(metrics.active_executions.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_active_executions_prometheus_format() {
        let metrics = HttpMetrics::new();

        // Start with some active executions
        metrics.inc_active_execution();
        metrics.inc_active_execution();
        metrics.inc_active_execution();

        let output = metrics.to_prometheus_format().await;

        // Check that active executions metric is present
        assert!(output.contains("oxify_active_executions"));
        assert!(output.contains("oxify_active_executions 3"));
    }

    #[test]
    fn test_active_executions_concurrent() {
        let metrics = HttpMetrics::new();

        // Simulate multiple concurrent executions
        metrics.inc_active_execution();
        metrics.inc_active_execution();
        metrics.inc_active_execution();
        metrics.inc_active_execution();
        metrics.inc_active_execution();

        assert_eq!(metrics.active_executions.load(Ordering::Relaxed), 5);

        // Some complete
        metrics.dec_active_execution();
        metrics.dec_active_execution();

        assert_eq!(metrics.active_executions.load(Ordering::Relaxed), 3);

        // Rest complete
        metrics.dec_active_execution();
        metrics.dec_active_execution();
        metrics.dec_active_execution();

        assert_eq!(metrics.active_executions.load(Ordering::Relaxed), 0);
    }
}
