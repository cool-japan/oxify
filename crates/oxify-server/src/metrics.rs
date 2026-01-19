//! Prometheus metrics for HTTP server monitoring
//!
//! Provides request metrics, latency histograms, and error rates.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder,
};
use std::sync::Arc;
use std::time::Instant;

/// Prometheus metrics registry
#[derive(Clone, Debug)]
pub struct MetricsRegistry {
    registry: Arc<Registry>,
    http_requests_total: IntCounterVec,
    http_request_duration_seconds: HistogramVec,
    http_requests_in_flight: IntGauge,
    http_errors_total: IntCounterVec,
}

impl MetricsRegistry {
    /// Create a new metrics registry
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // HTTP requests total counter
        let http_requests_total = IntCounterVec::new(
            Opts::new("http_requests_total", "Total number of HTTP requests")
                .namespace("oxify")
                .subsystem("server"),
            &["method", "path", "status"],
        )?;
        registry.register(Box::new(http_requests_total.clone()))?;

        // HTTP request duration histogram
        let http_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "http_request_duration_seconds",
                "HTTP request latency in seconds",
            )
            .namespace("oxify")
            .subsystem("server")
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ]),
            &["method", "path"],
        )?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;

        // HTTP requests in flight gauge
        let http_requests_in_flight = IntGauge::new(
            "http_requests_in_flight",
            "Number of HTTP requests currently being processed",
        )?;
        registry.register(Box::new(http_requests_in_flight.clone()))?;

        // HTTP errors total counter
        let http_errors_total = IntCounterVec::new(
            Opts::new("http_errors_total", "Total number of HTTP errors")
                .namespace("oxify")
                .subsystem("server"),
            &["method", "path", "status"],
        )?;
        registry.register(Box::new(http_errors_total.clone()))?;

        Ok(Self {
            registry: Arc::new(registry),
            http_requests_total,
            http_request_duration_seconds,
            http_requests_in_flight,
            http_errors_total,
        })
    }

    /// Get the Prometheus registry
    pub fn registry(&self) -> Arc<Registry> {
        self.registry.clone()
    }

    /// Render metrics in Prometheus text format
    pub fn render(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        String::from_utf8(buffer)
            .map_err(|_| prometheus::Error::Msg("UTF-8 encoding error".to_string()))
    }

    /// Record an HTTP request
    pub fn record_request(&self, method: &str, path: &str, status: u16, duration: f64) {
        let status_str = status.to_string();

        // Increment request counter
        self.http_requests_total
            .with_label_values(&[method, path, &status_str])
            .inc();

        // Record duration
        self.http_request_duration_seconds
            .with_label_values(&[method, path])
            .observe(duration);

        // Record errors (4xx and 5xx)
        if status >= 400 {
            self.http_errors_total
                .with_label_values(&[method, path, &status_str])
                .inc();
        }
    }

    /// Increment in-flight requests
    pub fn inc_in_flight(&self) {
        self.http_requests_in_flight.inc();
    }

    /// Decrement in-flight requests
    pub fn dec_in_flight(&self) {
        self.http_requests_in_flight.dec();
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to create metrics registry")
    }
}

/// Metrics middleware
///
/// Records HTTP request metrics for Prometheus.
pub async fn metrics_middleware(
    metrics: Arc<MetricsRegistry>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().to_string();
    let path = sanitize_path(request.uri().path());

    // Increment in-flight requests
    metrics.inc_in_flight();

    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed().as_secs_f64();

    // Decrement in-flight requests
    metrics.dec_in_flight();

    let status = response.status().as_u16();

    // Record metrics
    metrics.record_request(&method, &path, status, duration);

    response
}

/// Sanitize path for metrics (group dynamic segments)
fn sanitize_path(path: &str) -> String {
    // Replace UUIDs and IDs with placeholders to avoid high cardinality
    let mut sanitized = path.to_string();

    // Replace UUID-like patterns
    let uuid_pattern =
        regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap();
    sanitized = uuid_pattern.replace_all(&sanitized, ":id").to_string();

    // Replace numeric IDs
    let id_pattern = regex::Regex::new(r"/\d+(/|$)").unwrap();
    sanitized = id_pattern.replace_all(&sanitized, "/:id$1").to_string();

    sanitized
}

/// Metrics endpoint handler
pub async fn metrics_handler(
    metrics: Arc<MetricsRegistry>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    match metrics.render() {
        Ok(body) => Ok((
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4")],
            body,
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to render metrics: {}", e),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new();
        assert!(registry.is_ok());
    }

    #[test]
    fn test_record_request() {
        let registry = MetricsRegistry::new().unwrap();
        registry.record_request("GET", "/api/test", 200, 0.123);
        registry.record_request("POST", "/api/test", 201, 0.456);
        registry.record_request("GET", "/api/test", 500, 0.789);

        let output = registry.render().unwrap();
        assert!(output.contains("http_requests_total"));
        assert!(output.contains("http_request_duration_seconds"));
    }

    #[test]
    fn test_in_flight_requests() {
        let registry = MetricsRegistry::new().unwrap();
        registry.inc_in_flight();
        registry.inc_in_flight();
        registry.dec_in_flight();

        let output = registry.render().unwrap();
        assert!(output.contains("http_requests_in_flight"));
    }

    #[test]
    fn test_error_recording() {
        let registry = MetricsRegistry::new().unwrap();
        registry.record_request("GET", "/api/test", 404, 0.1);
        registry.record_request("POST", "/api/test", 500, 0.2);

        let output = registry.render().unwrap();
        assert!(output.contains("http_errors_total"));
    }

    #[test]
    fn test_sanitize_path() {
        assert_eq!(sanitize_path("/api/users/123"), "/api/users/:id");
        assert_eq!(
            sanitize_path("/api/users/550e8400-e29b-41d4-a716-446655440000"),
            "/api/users/:id"
        );
        assert_eq!(sanitize_path("/api/users"), "/api/users");
    }

    #[test]
    fn test_metrics_render() {
        let registry = MetricsRegistry::new().unwrap();
        registry.record_request("GET", "/health", 200, 0.001);

        let output = registry.render().unwrap();
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
        assert!(output.contains("oxify_server"));
    }
}
