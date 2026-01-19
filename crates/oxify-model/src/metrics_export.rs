//! Metrics Export - Export workflow metrics to monitoring systems
//!
//! This module provides functionality to export workflow execution metrics
//! to popular monitoring formats like Prometheus and OpenTelemetry.
//!
//! # Example
//!
//! ```
//! use oxify_model::{
//!     WorkflowAnalytics, MetricsExporter, ExportFormat,
//!     ExecutionStats, PerformanceMetrics, AnalyticsPeriod, PeriodType
//! };
//! use chrono::Utc;
//! use uuid::Uuid;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create analytics data
//! let analytics = WorkflowAnalytics {
//!     workflow_id: Uuid::new_v4(),
//!     workflow_name: "my_workflow".to_string(),
//!     period: AnalyticsPeriod {
//!         start: Utc::now(),
//!         end: Utc::now(),
//!         period_type: PeriodType::Daily,
//!     },
//!     execution_stats: ExecutionStats::default(),
//!     performance_metrics: PerformanceMetrics::default(),
//!     node_analytics: vec![],
//!     error_patterns: vec![],
//!     updated_at: Utc::now(),
//! };
//!
//! // Export to Prometheus format
//! let exporter = MetricsExporter::new("my_workflow");
//! let prometheus_output = exporter.export_prometheus(&analytics)?;
//! println!("{}", prometheus_output);
//!
//! // Export to OpenTelemetry JSON format
//! let otel_output = exporter.export_opentelemetry(&analytics)?;
//! println!("{}", serde_json::to_string_pretty(&otel_output)?);
//! # Ok(())
//! # }
//! ```

use crate::analytics::WorkflowAnalytics;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Export format for metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    /// Prometheus text format
    Prometheus,
    /// OpenTelemetry JSON format
    OpenTelemetry,
    /// InfluxDB line protocol
    InfluxDb,
    /// JSON format
    Json,
}

/// Metrics exporter for workflow analytics
#[derive(Debug, Clone)]
pub struct MetricsExporter {
    /// Workflow name (used as a label/tag)
    workflow_name: String,
    /// Additional labels/tags to include in exports
    labels: HashMap<String, String>,
    /// Namespace for metrics (default: "oxify")
    namespace: String,
}

impl MetricsExporter {
    /// Create a new metrics exporter
    pub fn new(workflow_name: impl Into<String>) -> Self {
        Self {
            workflow_name: workflow_name.into(),
            labels: HashMap::new(),
            namespace: "oxify".to_string(),
        }
    }

    /// Add a label/tag to all exported metrics
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Set the namespace for metrics
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Export analytics to Prometheus text format
    pub fn export_prometheus(&self, analytics: &WorkflowAnalytics) -> Result<String, MetricsError> {
        let mut output = String::new();
        let labels = self.format_prometheus_labels();

        // Execution stats
        output.push_str(&format!(
            "# HELP {}_executions_total Total number of workflow executions\n",
            self.namespace
        ));
        output.push_str(&format!(
            "# TYPE {}_executions_total counter\n",
            self.namespace
        ));
        output.push_str(&format!(
            "{}_executions_total{{{}}} {}\n",
            self.namespace, labels, analytics.execution_stats.total_executions
        ));

        output.push_str(&format!(
            "# HELP {}_executions_successful_total Total number of successful executions\n",
            self.namespace
        ));
        output.push_str(&format!(
            "# TYPE {}_executions_successful_total counter\n",
            self.namespace
        ));
        output.push_str(&format!(
            "{}_executions_successful_total{{{}}} {}\n",
            self.namespace, labels, analytics.execution_stats.successful_executions
        ));

        output.push_str(&format!(
            "# HELP {}_executions_failed_total Total number of failed executions\n",
            self.namespace
        ));
        output.push_str(&format!(
            "# TYPE {}_executions_failed_total counter\n",
            self.namespace
        ));
        output.push_str(&format!(
            "{}_executions_failed_total{{{}}} {}\n",
            self.namespace, labels, analytics.execution_stats.failed_executions
        ));

        // Success rate
        output.push_str(&format!(
            "# HELP {}_success_rate Success rate of workflow executions (0-1)\n",
            self.namespace
        ));
        output.push_str(&format!("# TYPE {}_success_rate gauge\n", self.namespace));
        output.push_str(&format!(
            "{}_success_rate{{{}}} {:.4}\n",
            self.namespace, labels, analytics.execution_stats.success_rate
        ));

        // Performance metrics
        let perf = &analytics.performance_metrics;
        output.push_str(&format!(
            "# HELP {}_duration_seconds Workflow execution duration in seconds\n",
            self.namespace
        ));
        output.push_str(&format!(
            "# TYPE {}_duration_seconds summary\n",
            self.namespace
        ));
        output.push_str(&format!(
            "{}_duration_seconds{{{}quantile=\"0.5\"}} {:.3}\n",
            self.namespace,
            labels,
            perf.p50_duration_ms as f64 / 1000.0
        ));
        output.push_str(&format!(
            "{}_duration_seconds{{{}quantile=\"0.95\"}} {:.3}\n",
            self.namespace,
            labels,
            perf.p95_duration_ms as f64 / 1000.0
        ));
        output.push_str(&format!(
            "{}_duration_seconds{{{}quantile=\"0.99\"}} {:.3}\n",
            self.namespace,
            labels,
            perf.p99_duration_ms as f64 / 1000.0
        ));
        output.push_str(&format!(
            "{}_duration_seconds_sum{{{}}} {:.3}\n",
            self.namespace,
            labels,
            perf.avg_duration_ms / 1000.0 * analytics.execution_stats.total_executions as f64
        ));
        output.push_str(&format!(
            "{}_duration_seconds_count{{{}}} {}\n",
            self.namespace, labels, analytics.execution_stats.total_executions
        ));

        // Node metrics
        for node_stats in &analytics.node_analytics {
            let node_labels = format!("{},node_id=\"{}\"", labels, node_stats.node_id);
            output.push_str(&format!(
                "{}_node_executions_total{{{}}} {}\n",
                self.namespace, node_labels, node_stats.execution_count
            ));
            output.push_str(&format!(
                "{}_node_duration_seconds{{{}}} {:.3}\n",
                self.namespace,
                node_labels,
                node_stats.avg_duration_ms / 1000.0
            ));
        }

        Ok(output)
    }

    /// Export analytics to OpenTelemetry JSON format
    pub fn export_opentelemetry(
        &self,
        analytics: &WorkflowAnalytics,
    ) -> Result<serde_json::Value, MetricsError> {
        let perf = &analytics.performance_metrics;

        let metrics = vec![
            // Execution counters
            self.create_otel_metric(
                "executions.total",
                "counter",
                analytics.execution_stats.total_executions as f64,
                "Number of total executions",
            ),
            self.create_otel_metric(
                "executions.successful",
                "counter",
                analytics.execution_stats.successful_executions as f64,
                "Number of successful executions",
            ),
            self.create_otel_metric(
                "executions.failed",
                "counter",
                analytics.execution_stats.failed_executions as f64,
                "Number of failed executions",
            ),
            // Success rate gauge
            self.create_otel_metric(
                "success_rate",
                "gauge",
                analytics.execution_stats.success_rate,
                "Success rate of executions",
            ),
            // Performance metrics
            self.create_otel_metric(
                "duration.p50",
                "gauge",
                perf.p50_duration_ms as f64,
                "Median execution duration (ms)",
            ),
            self.create_otel_metric(
                "duration.p95",
                "gauge",
                perf.p95_duration_ms as f64,
                "95th percentile execution duration (ms)",
            ),
            self.create_otel_metric(
                "duration.p99",
                "gauge",
                perf.p99_duration_ms as f64,
                "99th percentile execution duration (ms)",
            ),
        ];

        Ok(serde_json::json!({
            "resourceMetrics": [{
                "resource": {
                    "attributes": self.create_otel_attributes()
                },
                "scopeMetrics": [{
                    "scope": {
                        "name": "oxify-model",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "metrics": metrics
                }]
            }]
        }))
    }

    /// Export analytics to InfluxDB line protocol
    pub fn export_influxdb(&self, analytics: &WorkflowAnalytics) -> Result<String, MetricsError> {
        let mut output = String::new();
        let tags = self.format_influxdb_tags();
        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Execution stats
        output.push_str(&format!(
            "workflow_executions,{} total={},successful={},failed={},cancelled={} {}\n",
            tags,
            analytics.execution_stats.total_executions,
            analytics.execution_stats.successful_executions,
            analytics.execution_stats.failed_executions,
            analytics.execution_stats.cancelled_executions,
            timestamp
        ));

        output.push_str(&format!(
            "workflow_rates,{} success_rate={:.4},failure_rate={:.4} {}\n",
            tags,
            analytics.execution_stats.success_rate,
            analytics.execution_stats.failure_rate,
            timestamp
        ));

        // Performance metrics
        let perf = &analytics.performance_metrics;
        output.push_str(&format!(
            "workflow_duration,{} min={},max={},avg={:.3},median={},p95={},p99={} {}\n",
            tags,
            perf.min_duration_ms,
            perf.max_duration_ms,
            perf.avg_duration_ms,
            perf.p50_duration_ms,
            perf.p95_duration_ms,
            perf.p99_duration_ms,
            timestamp
        ));

        // Node metrics
        for node_stats in &analytics.node_analytics {
            output.push_str(&format!(
                "node_metrics,{},node_id={} executions={},avg_duration={:.3} {}\n",
                tags,
                node_stats.node_id,
                node_stats.execution_count,
                node_stats.avg_duration_ms,
                timestamp
            ));
        }

        Ok(output)
    }

    /// Export analytics to generic JSON format
    pub fn export_json(
        &self,
        analytics: &WorkflowAnalytics,
    ) -> Result<serde_json::Value, MetricsError> {
        let perf = &analytics.performance_metrics;
        Ok(serde_json::json!({
            "workflow": self.workflow_name,
            "namespace": self.namespace,
            "labels": self.labels,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "execution_stats": {
                "total": analytics.execution_stats.total_executions,
                "successful": analytics.execution_stats.successful_executions,
                "failed": analytics.execution_stats.failed_executions,
                "cancelled": analytics.execution_stats.cancelled_executions,
                "success_rate": analytics.execution_stats.success_rate,
                "failure_rate": analytics.execution_stats.failure_rate,
            },
            "performance": {
                "min_duration_ms": perf.min_duration_ms,
                "max_duration_ms": perf.max_duration_ms,
                "avg_duration_ms": perf.avg_duration_ms,
                "p50_duration_ms": perf.p50_duration_ms,
                "p95_duration_ms": perf.p95_duration_ms,
                "p99_duration_ms": perf.p99_duration_ms,
            },
            "node_analytics": analytics.node_analytics,
            "error_patterns": analytics.error_patterns,
        }))
    }

    /// Export analytics in the specified format
    pub fn export(
        &self,
        analytics: &WorkflowAnalytics,
        format: ExportFormat,
    ) -> Result<String, MetricsError> {
        match format {
            ExportFormat::Prometheus => self.export_prometheus(analytics),
            ExportFormat::OpenTelemetry => {
                let json = self.export_opentelemetry(analytics)?;
                serde_json::to_string_pretty(&json)
                    .map_err(|e| MetricsError::SerializationError(e.to_string()))
            }
            ExportFormat::InfluxDb => self.export_influxdb(analytics),
            ExportFormat::Json => {
                let json = self.export_json(analytics)?;
                serde_json::to_string_pretty(&json)
                    .map_err(|e| MetricsError::SerializationError(e.to_string()))
            }
        }
    }

    // Helper methods

    fn format_prometheus_labels(&self) -> String {
        let mut labels = vec![format!("workflow=\"{}\"", self.workflow_name)];
        for (k, v) in &self.labels {
            labels.push(format!("{}=\"{}\"", k, v));
        }
        labels.join(",")
    }

    fn format_influxdb_tags(&self) -> String {
        let mut tags = vec![format!("workflow={}", self.workflow_name)];
        for (k, v) in &self.labels {
            tags.push(format!("{}={}", k, v));
        }
        tags.join(",")
    }

    fn create_otel_attributes(&self) -> Vec<serde_json::Value> {
        let mut attrs = vec![serde_json::json!({
            "key": "workflow.name",
            "value": { "stringValue": self.workflow_name }
        })];

        for (k, v) in &self.labels {
            attrs.push(serde_json::json!({
                "key": k,
                "value": { "stringValue": v }
            }));
        }

        attrs
    }

    fn create_otel_metric(
        &self,
        name: &str,
        metric_type: &str,
        value: f64,
        description: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "name": format!("{}.{}", self.namespace, name),
            "description": description,
            "unit": match metric_type {
                "counter" => "1",
                "gauge" => "1",
                _ => "1"
            },
            metric_type: {
                "dataPoints": [{
                    "asDouble": value,
                    "timeUnixNano": chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).to_string(),
                    "attributes": self.create_otel_attributes()
                }]
            }
        })
    }
}

/// Errors that can occur during metrics export
#[derive(Debug, Error)]
pub enum MetricsError {
    /// Failed to serialize metrics
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Invalid metric data
    #[error("Invalid metric data: {0}")]
    InvalidData(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::{
        AnalyticsPeriod, ExecutionStats, NodeAnalytics, PerformanceMetrics, PeriodType,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_analytics() -> WorkflowAnalytics {
        WorkflowAnalytics {
            workflow_id: Uuid::new_v4(),
            workflow_name: "test_workflow".to_string(),
            period: AnalyticsPeriod {
                start: Utc::now(),
                end: Utc::now(),
                period_type: PeriodType::Daily,
            },
            execution_stats: ExecutionStats {
                total_executions: 100,
                successful_executions: 85,
                failed_executions: 10,
                cancelled_executions: 5,
                success_rate: 0.85,
                failure_rate: 0.10,
                executions_per_hour: 10.0,
            },
            performance_metrics: PerformanceMetrics {
                avg_duration_ms: 1500.0,
                p50_duration_ms: 1200,
                p95_duration_ms: 3000,
                p99_duration_ms: 4500,
                min_duration_ms: 100,
                max_duration_ms: 5000,
                total_tokens: 1000000,
                avg_tokens: 10000.0,
                total_cost_usd: 100.0,
                avg_cost_usd: 1.0,
            },
            node_analytics: vec![NodeAnalytics {
                node_id: Uuid::new_v4(),
                node_name: "node1".to_string(),
                node_type: "LLM".to_string(),
                execution_count: 100,
                success_count: 95,
                failure_count: 5,
                avg_duration_ms: 500.0,
                max_duration_ms: 1000,
                total_duration_ms: 50000,
                time_percentage: 33.0,
                is_bottleneck: false,
            }],
            error_patterns: vec![],
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_metrics_exporter_creation() {
        let exporter = MetricsExporter::new("test_workflow");
        assert_eq!(exporter.workflow_name, "test_workflow");
        assert_eq!(exporter.namespace, "oxify");
        assert!(exporter.labels.is_empty());
    }

    #[test]
    fn test_with_label() {
        let exporter = MetricsExporter::new("test")
            .with_label("env", "production")
            .with_label("region", "us-east-1");

        assert_eq!(exporter.labels.len(), 2);
        assert_eq!(exporter.labels.get("env"), Some(&"production".to_string()));
        assert_eq!(
            exporter.labels.get("region"),
            Some(&"us-east-1".to_string())
        );
    }

    #[test]
    fn test_with_namespace() {
        let exporter = MetricsExporter::new("test").with_namespace("custom");
        assert_eq!(exporter.namespace, "custom");
    }

    #[test]
    fn test_export_prometheus() {
        let exporter = MetricsExporter::new("test_workflow");
        let analytics = create_test_analytics();
        let result = exporter.export_prometheus(&analytics);

        assert!(result.is_ok());
        let output = result.unwrap();

        // Check for expected metric names
        assert!(output.contains("oxify_executions_total"));
        assert!(output.contains("oxify_executions_successful_total"));
        assert!(output.contains("oxify_executions_failed_total"));
        assert!(output.contains("oxify_success_rate"));
        assert!(output.contains("oxify_duration_seconds"));

        // Check for expected values
        assert!(output.contains("100")); // total executions
        assert!(output.contains("85")); // successful
        assert!(output.contains("10")); // failed
        assert!(output.contains("0.85")); // success rate

        // Check for labels
        assert!(output.contains("workflow=\"test_workflow\""));
    }

    #[test]
    fn test_export_opentelemetry() {
        let exporter = MetricsExporter::new("test_workflow");
        let analytics = create_test_analytics();
        let result = exporter.export_opentelemetry(&analytics);

        assert!(result.is_ok());
        let json = result.unwrap();

        // Check structure
        assert!(json["resourceMetrics"].is_array());
        assert!(json["resourceMetrics"][0]["scopeMetrics"].is_array());
        assert!(json["resourceMetrics"][0]["scopeMetrics"][0]["metrics"].is_array());

        // Check metrics
        let metrics = json["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
            .as_array()
            .unwrap();
        assert!(!metrics.is_empty());

        // Check for specific metrics
        let metric_names: Vec<String> = metrics
            .iter()
            .filter_map(|m| m["name"].as_str().map(String::from))
            .collect();

        assert!(metric_names.iter().any(|n| n.contains("executions.total")));
        assert!(metric_names.iter().any(|n| n.contains("success_rate")));
    }

    #[test]
    fn test_export_influxdb() {
        let exporter = MetricsExporter::new("test_workflow");
        let analytics = create_test_analytics();
        let result = exporter.export_influxdb(&analytics);

        assert!(result.is_ok());
        let output = result.unwrap();

        // Check for expected measurements
        assert!(output.contains("workflow_executions"));
        assert!(output.contains("workflow_rates"));
        assert!(output.contains("workflow_duration"));
        assert!(output.contains("node_metrics"));

        // Check for tags
        assert!(output.contains("workflow=test_workflow"));

        // Check for fields
        assert!(output.contains("total=100"));
        assert!(output.contains("successful=85"));
        assert!(output.contains("failed=10"));
    }

    #[test]
    fn test_export_json() {
        let exporter = MetricsExporter::new("test_workflow");
        let analytics = create_test_analytics();
        let result = exporter.export_json(&analytics);

        assert!(result.is_ok());
        let json = result.unwrap();

        assert_eq!(json["workflow"], "test_workflow");
        assert_eq!(json["namespace"], "oxify");
        assert_eq!(json["execution_stats"]["total"], 100);
        assert_eq!(json["execution_stats"]["successful"], 85);
        assert_eq!(json["execution_stats"]["failed"], 10);
        assert_eq!(json["execution_stats"]["success_rate"], 0.85);
    }

    #[test]
    fn test_export_with_format() {
        let exporter = MetricsExporter::new("test");
        let analytics = create_test_analytics();

        // Test all formats
        assert!(exporter
            .export(&analytics, ExportFormat::Prometheus)
            .is_ok());
        assert!(exporter
            .export(&analytics, ExportFormat::OpenTelemetry)
            .is_ok());
        assert!(exporter.export(&analytics, ExportFormat::InfluxDb).is_ok());
        assert!(exporter.export(&analytics, ExportFormat::Json).is_ok());
    }

    #[test]
    fn test_prometheus_labels_formatting() {
        let exporter = MetricsExporter::new("test")
            .with_label("env", "prod")
            .with_label("region", "us");

        let labels = exporter.format_prometheus_labels();
        assert!(labels.contains("workflow=\"test\""));
        assert!(labels.contains("env=\"prod\""));
        assert!(labels.contains("region=\"us\""));
    }

    #[test]
    fn test_influxdb_tags_formatting() {
        let exporter = MetricsExporter::new("test").with_label("env", "prod");

        let tags = exporter.format_influxdb_tags();
        assert!(tags.contains("workflow=test"));
        assert!(tags.contains("env=prod"));
    }

    #[test]
    fn test_otel_attributes_creation() {
        let exporter = MetricsExporter::new("test").with_label("env", "prod");

        let attrs = exporter.create_otel_attributes();
        assert!(!attrs.is_empty());

        // Check workflow.name attribute
        let workflow_attr = attrs.iter().find(|a| a["key"] == "workflow.name");
        assert!(workflow_attr.is_some());
    }

    #[test]
    fn test_export_format_enum() {
        assert_eq!(ExportFormat::Prometheus, ExportFormat::Prometheus);
        assert_ne!(ExportFormat::Prometheus, ExportFormat::Json);
    }

    #[test]
    fn test_prometheus_node_metrics() {
        let exporter = MetricsExporter::new("test");
        let analytics = create_test_analytics();
        let output = exporter.export_prometheus(&analytics).unwrap();

        // Check node-specific metrics
        assert!(output.contains("node_id="));
        assert!(output.contains("oxify_node_executions_total"));
        assert!(output.contains("oxify_node_duration_seconds"));
    }

    #[test]
    fn test_influxdb_node_metrics() {
        let exporter = MetricsExporter::new("test");
        let analytics = create_test_analytics();
        let output = exporter.export_influxdb(&analytics).unwrap();

        assert!(output.contains("node_metrics"));
        assert!(output.contains("node_id="));
        assert!(output.contains("executions=100"));
    }

    #[test]
    fn test_custom_namespace() {
        let exporter = MetricsExporter::new("test").with_namespace("custom_ns");
        let analytics = create_test_analytics();
        let output = exporter.export_prometheus(&analytics).unwrap();

        assert!(output.contains("custom_ns_executions_total"));
        assert!(output.contains("custom_ns_success_rate"));
    }

    #[test]
    fn test_metrics_with_multiple_labels() {
        let exporter = MetricsExporter::new("test")
            .with_label("env", "staging")
            .with_label("region", "eu-west-1")
            .with_label("team", "platform");

        let analytics = create_test_analytics();
        let output = exporter.export_prometheus(&analytics).unwrap();

        assert!(output.contains("env=\"staging\""));
        assert!(output.contains("region=\"eu-west-1\""));
        assert!(output.contains("team=\"platform\""));
    }
}
