//! Workflow analytics and usage tracking
//!
//! This module provides detailed analytics for workflow execution patterns,
//! performance metrics, and optimization insights.

use crate::{EventTimeline, EventType, NodeId, WorkflowId};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Workflow execution statistics aggregated over time
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WorkflowAnalytics {
    /// Workflow identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = "uuid"))]
    pub workflow_id: WorkflowId,

    /// Workflow name
    pub workflow_name: String,

    /// Time period for these analytics
    pub period: AnalyticsPeriod,

    /// Execution statistics
    pub execution_stats: ExecutionStats,

    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,

    /// Node-level analytics
    pub node_analytics: Vec<NodeAnalytics>,

    /// Error patterns
    pub error_patterns: Vec<ErrorPattern>,

    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

/// Time period for analytics aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AnalyticsPeriod {
    /// Period start time
    pub start: DateTime<Utc>,

    /// Period end time
    pub end: DateTime<Utc>,

    /// Period type (for display)
    pub period_type: PeriodType,
}

/// Period type for analytics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum PeriodType {
    /// Last hour
    Hourly,
    /// Last 24 hours
    Daily,
    /// Last 7 days
    Weekly,
    /// Last 30 days
    Monthly,
    /// Custom time range
    Custom,
}

/// Execution statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExecutionStats {
    /// Total number of executions
    pub total_executions: u64,

    /// Number of successful executions
    pub successful_executions: u64,

    /// Number of failed executions
    pub failed_executions: u64,

    /// Number of cancelled executions
    pub cancelled_executions: u64,

    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,

    /// Failure rate (0.0 to 1.0)
    pub failure_rate: f64,

    /// Average executions per hour
    pub executions_per_hour: f64,
}

impl ExecutionStats {
    /// Calculate derived metrics
    pub fn calculate_rates(&mut self) {
        if self.total_executions > 0 {
            self.success_rate = self.successful_executions as f64 / self.total_executions as f64;
            self.failure_rate = self.failed_executions as f64 / self.total_executions as f64;
        }
    }
}

/// Performance metrics with percentiles
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PerformanceMetrics {
    /// Average execution duration in milliseconds
    pub avg_duration_ms: f64,

    /// Median (p50) execution duration in milliseconds
    pub p50_duration_ms: u64,

    /// 95th percentile execution duration in milliseconds
    pub p95_duration_ms: u64,

    /// 99th percentile execution duration in milliseconds
    pub p99_duration_ms: u64,

    /// Minimum execution duration in milliseconds
    pub min_duration_ms: u64,

    /// Maximum execution duration in milliseconds
    pub max_duration_ms: u64,

    /// Total token usage across all executions
    pub total_tokens: u64,

    /// Average tokens per execution
    pub avg_tokens: f64,

    /// Total cost in USD
    pub total_cost_usd: f64,

    /// Average cost per execution in USD
    pub avg_cost_usd: f64,
}

/// Node-level analytics for identifying bottlenecks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct NodeAnalytics {
    /// Node identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = "uuid"))]
    pub node_id: NodeId,

    /// Node name
    pub node_name: String,

    /// Node type (e.g., "LLM", "Retriever")
    pub node_type: String,

    /// Number of times this node executed
    pub execution_count: u64,

    /// Number of successful executions
    pub success_count: u64,

    /// Number of failed executions
    pub failure_count: u64,

    /// Average execution duration in milliseconds
    pub avg_duration_ms: f64,

    /// Maximum execution duration in milliseconds
    pub max_duration_ms: u64,

    /// Total duration across all executions
    pub total_duration_ms: u64,

    /// Percentage of total workflow time spent in this node
    pub time_percentage: f64,

    /// Is this a bottleneck (slowest node)?
    pub is_bottleneck: bool,
}

/// Error pattern analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ErrorPattern {
    /// Error message or pattern
    pub error_message: String,

    /// Number of occurrences
    pub occurrence_count: u64,

    /// Percentage of total errors
    pub error_percentage: f64,

    /// Node IDs where this error occurred
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<String>))]
    pub affected_nodes: Vec<NodeId>,

    /// First occurrence timestamp
    pub first_seen: DateTime<Utc>,

    /// Last occurrence timestamp
    pub last_seen: DateTime<Utc>,

    /// Trend (increasing, stable, decreasing)
    pub trend: ErrorTrend,
}

/// Error trend classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum ErrorTrend {
    /// Error frequency is increasing
    Increasing,
    /// Error frequency is stable
    Stable,
    /// Error frequency is decreasing
    Decreasing,
}

/// Analytics builder that processes event timelines
pub struct AnalyticsBuilder {
    workflow_id: WorkflowId,
    workflow_name: String,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    period_type: PeriodType,
    timelines: Vec<EventTimeline>,
}

impl AnalyticsBuilder {
    /// Create a new analytics builder
    pub fn new(
        workflow_id: WorkflowId,
        workflow_name: String,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        period_type: PeriodType,
    ) -> Self {
        Self {
            workflow_id,
            workflow_name,
            period_start,
            period_end,
            period_type,
            timelines: Vec::new(),
        }
    }

    /// Add an event timeline for analysis
    pub fn add_timeline(&mut self, timeline: EventTimeline) {
        self.timelines.push(timeline);
    }

    /// Build the analytics report
    pub fn build(self) -> WorkflowAnalytics {
        let execution_stats = self.calculate_execution_stats();
        let performance_metrics = self.calculate_performance_metrics();
        let node_analytics = self.calculate_node_analytics();
        let error_patterns = self.calculate_error_patterns();

        WorkflowAnalytics {
            workflow_id: self.workflow_id,
            workflow_name: self.workflow_name,
            period: AnalyticsPeriod {
                start: self.period_start,
                end: self.period_end,
                period_type: self.period_type,
            },
            execution_stats,
            performance_metrics,
            node_analytics,
            error_patterns,
            updated_at: Utc::now(),
        }
    }

    fn calculate_execution_stats(&self) -> ExecutionStats {
        let total_executions = self.timelines.len() as u64;
        let successful_executions =
            self.timelines.iter().filter(|t| t.is_successful()).count() as u64;
        let failed_executions = self.timelines.iter().filter(|t| t.is_failed()).count() as u64;
        let cancelled_executions = self
            .timelines
            .iter()
            .filter(|t| {
                t.events
                    .iter()
                    .any(|e| e.event_type == EventType::WorkflowCancelled)
            })
            .count() as u64;

        let period_hours = (self.period_end - self.period_start).num_hours() as f64;
        let executions_per_hour = if period_hours > 0.0 {
            total_executions as f64 / period_hours
        } else {
            0.0
        };

        let mut stats = ExecutionStats {
            total_executions,
            successful_executions,
            failed_executions,
            cancelled_executions,
            success_rate: 0.0,
            failure_rate: 0.0,
            executions_per_hour,
        };

        stats.calculate_rates();
        stats
    }

    fn calculate_performance_metrics(&self) -> PerformanceMetrics {
        // Extract durations from WorkflowCompleted events
        let mut durations: Vec<u64> = self
            .timelines
            .iter()
            .filter_map(|timeline| {
                // Look for WorkflowCompleted event
                timeline.events.iter().find_map(|event| {
                    if let crate::EventDetails::WorkflowCompleted { duration_ms, .. } =
                        &event.details
                    {
                        Some(*duration_ms)
                    } else {
                        None
                    }
                })
            })
            .collect();

        if durations.is_empty() {
            return PerformanceMetrics::default();
        }

        durations.sort_unstable();

        let avg_duration_ms = durations.iter().sum::<u64>() as f64 / durations.len() as f64;
        let min_duration_ms = *durations.first().unwrap_or(&0);
        let max_duration_ms = *durations.last().unwrap_or(&0);

        let p50_idx = (durations.len() as f64 * 0.50) as usize;
        let p95_idx = (durations.len() as f64 * 0.95) as usize;
        let p99_idx = (durations.len() as f64 * 0.99) as usize;

        let p50_duration_ms = durations.get(p50_idx).copied().unwrap_or(0);
        let p95_duration_ms = durations.get(p95_idx).copied().unwrap_or(0);
        let p99_duration_ms = durations.get(p99_idx).copied().unwrap_or(0);

        PerformanceMetrics {
            avg_duration_ms,
            p50_duration_ms,
            p95_duration_ms,
            p99_duration_ms,
            min_duration_ms,
            max_duration_ms,
            total_tokens: 0,
            avg_tokens: 0.0,
            total_cost_usd: 0.0,
            avg_cost_usd: 0.0,
        }
    }

    fn calculate_node_analytics(&self) -> Vec<NodeAnalytics> {
        let mut node_stats: HashMap<NodeId, NodeStats> = HashMap::new();

        // Collect stats for each node
        for timeline in &self.timelines {
            for event in &timeline.events {
                if let Some(node_id) = event.node_id {
                    let stats = node_stats.entry(node_id).or_default();

                    match event.event_type {
                        EventType::NodeStarted => {
                            stats.execution_count += 1;
                        }
                        EventType::NodeCompleted => {
                            stats.success_count += 1;
                            // Extract duration from event details
                            if let crate::EventDetails::NodeCompleted { duration_ms, .. } =
                                &event.details
                            {
                                stats.total_duration_ms += duration_ms;
                                stats.max_duration_ms = stats.max_duration_ms.max(*duration_ms);
                            }
                        }
                        EventType::NodeFailed => {
                            stats.failure_count += 1;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Calculate total workflow time
        let total_workflow_time: u64 = node_stats.values().map(|s| s.total_duration_ms).sum();

        // Convert to NodeAnalytics
        let mut analytics: Vec<NodeAnalytics> = node_stats
            .into_iter()
            .map(|(node_id, stats)| {
                let avg_duration_ms = if stats.success_count > 0 {
                    stats.total_duration_ms as f64 / stats.success_count as f64
                } else {
                    0.0
                };

                let time_percentage = if total_workflow_time > 0 {
                    (stats.total_duration_ms as f64 / total_workflow_time as f64) * 100.0
                } else {
                    0.0
                };

                NodeAnalytics {
                    node_id,
                    node_name: format!("Node-{}", node_id),
                    node_type: "Unknown".to_string(),
                    execution_count: stats.execution_count,
                    success_count: stats.success_count,
                    failure_count: stats.failure_count,
                    avg_duration_ms,
                    max_duration_ms: stats.max_duration_ms,
                    total_duration_ms: stats.total_duration_ms,
                    time_percentage,
                    is_bottleneck: false,
                }
            })
            .collect();

        // Mark the slowest node as bottleneck
        if let Some(slowest) = analytics.iter_mut().max_by_key(|a| a.total_duration_ms) {
            slowest.is_bottleneck = true;
        }

        // Sort by total duration descending
        analytics.sort_by(|a, b| b.total_duration_ms.cmp(&a.total_duration_ms));

        analytics
    }

    fn calculate_error_patterns(&self) -> Vec<ErrorPattern> {
        let mut error_counts: HashMap<String, ErrorStats> = HashMap::new();

        // Collect error statistics
        for timeline in &self.timelines {
            for event in &timeline.events {
                if let EventType::NodeFailed
                | EventType::WorkflowFailed
                | EventType::ErrorOccurred = event.event_type
                {
                    let error_msg = self.extract_error_message(event);
                    let stats =
                        error_counts
                            .entry(error_msg.clone())
                            .or_insert_with(|| ErrorStats {
                                message: error_msg,
                                count: 0,
                                affected_nodes: Vec::new(),
                                first_seen: event.timestamp,
                                last_seen: event.timestamp,
                            });

                    stats.count += 1;
                    if let Some(node_id) = event.node_id {
                        if !stats.affected_nodes.contains(&node_id) {
                            stats.affected_nodes.push(node_id);
                        }
                    }
                    stats.last_seen = stats.last_seen.max(event.timestamp);
                    stats.first_seen = stats.first_seen.min(event.timestamp);
                }
            }
        }

        let total_errors: u64 = error_counts.values().map(|s| s.count).sum();

        // Convert to ErrorPattern
        let mut patterns: Vec<ErrorPattern> = error_counts
            .into_values()
            .map(|stats| {
                let error_percentage = if total_errors > 0 {
                    (stats.count as f64 / total_errors as f64) * 100.0
                } else {
                    0.0
                };

                ErrorPattern {
                    error_message: stats.message,
                    occurrence_count: stats.count,
                    error_percentage,
                    affected_nodes: stats.affected_nodes,
                    first_seen: stats.first_seen,
                    last_seen: stats.last_seen,
                    trend: ErrorTrend::Stable, // Simplified, could be calculated from time series
                }
            })
            .collect();

        // Sort by occurrence count descending
        patterns.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));

        patterns
    }

    fn extract_error_message(&self, event: &crate::ExecutionEvent) -> String {
        use crate::EventDetails;
        match &event.details {
            EventDetails::NodeFailed { error, .. } => error.clone(),
            EventDetails::WorkflowFailed { error, .. } => error.clone(),
            EventDetails::ErrorOccurred { error, .. } => error.clone(),
            _ => "Unknown error".to_string(),
        }
    }
}

#[derive(Default)]
struct NodeStats {
    execution_count: u64,
    success_count: u64,
    failure_count: u64,
    total_duration_ms: u64,
    max_duration_ms: u64,
}

struct ErrorStats {
    message: String,
    count: u64,
    affected_nodes: Vec<NodeId>,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

/// Helper to create common time periods
impl AnalyticsPeriod {
    /// Last hour
    pub fn last_hour() -> Self {
        let end = Utc::now();
        let start = end - Duration::hours(1);
        Self {
            start,
            end,
            period_type: PeriodType::Hourly,
        }
    }

    /// Last 24 hours
    pub fn last_day() -> Self {
        let end = Utc::now();
        let start = end - Duration::days(1);
        Self {
            start,
            end,
            period_type: PeriodType::Daily,
        }
    }

    /// Last 7 days
    pub fn last_week() -> Self {
        let end = Utc::now();
        let start = end - Duration::weeks(1);
        Self {
            start,
            end,
            period_type: PeriodType::Weekly,
        }
    }

    /// Last 30 days
    pub fn last_month() -> Self {
        let end = Utc::now();
        let start = end - Duration::days(30);
        Self {
            start,
            end,
            period_type: PeriodType::Monthly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExecutionEvent, ExecutionId, ExecutionResult, NodeMetrics, WorkflowMetadata};
    use std::collections::HashMap;

    #[test]
    fn test_execution_stats_calculation() {
        let mut stats = ExecutionStats {
            total_executions: 100,
            successful_executions: 90,
            failed_executions: 10,
            cancelled_executions: 0,
            success_rate: 0.0,
            failure_rate: 0.0,
            executions_per_hour: 0.0,
        };

        stats.calculate_rates();

        assert_eq!(stats.success_rate, 0.9);
        assert_eq!(stats.failure_rate, 0.1);
    }

    #[test]
    fn test_analytics_builder_basic() {
        let workflow_id = WorkflowId::new_v4();
        let execution_id = ExecutionId::new_v4();

        let mut builder = AnalyticsBuilder::new(
            workflow_id,
            "test-workflow".to_string(),
            Utc::now() - Duration::hours(1),
            Utc::now(),
            PeriodType::Hourly,
        );

        // Create a successful execution timeline
        let mut timeline = EventTimeline::new();
        timeline.push(ExecutionEvent::workflow_started(
            execution_id,
            workflow_id,
            WorkflowMetadata::new("test".to_string()),
            HashMap::new(),
        ));
        timeline.push(ExecutionEvent::workflow_completed(
            execution_id,
            workflow_id,
            1000,
            ExecutionResult::Success(serde_json::Value::Null),
        ));

        builder.add_timeline(timeline);

        let analytics = builder.build();

        assert_eq!(analytics.execution_stats.total_executions, 1);
        assert_eq!(analytics.execution_stats.successful_executions, 1);
        assert_eq!(analytics.execution_stats.failed_executions, 0);
        assert_eq!(analytics.execution_stats.success_rate, 1.0);
    }

    #[test]
    fn test_performance_metrics_percentiles() {
        let workflow_id = WorkflowId::new_v4();

        let mut builder = AnalyticsBuilder::new(
            workflow_id,
            "test-workflow".to_string(),
            Utc::now() - Duration::hours(1),
            Utc::now(),
            PeriodType::Hourly,
        );

        // Add multiple timelines with different durations
        for duration in [100, 200, 300, 400, 500, 600, 700, 800, 900, 1000] {
            let execution_id = ExecutionId::new_v4();
            let mut timeline = EventTimeline::new();

            timeline.push(ExecutionEvent::workflow_started(
                execution_id,
                workflow_id,
                WorkflowMetadata::new("test".to_string()),
                HashMap::new(),
            ));
            timeline.push(ExecutionEvent::workflow_completed(
                execution_id,
                workflow_id,
                duration,
                ExecutionResult::Success(serde_json::Value::Null),
            ));

            builder.add_timeline(timeline);
        }

        let analytics = builder.build();

        assert_eq!(analytics.performance_metrics.min_duration_ms, 100);
        assert_eq!(analytics.performance_metrics.max_duration_ms, 1000);
        assert!(analytics.performance_metrics.avg_duration_ms > 0.0);
        assert!(analytics.performance_metrics.p50_duration_ms > 0);
        assert!(
            analytics.performance_metrics.p95_duration_ms
                > analytics.performance_metrics.p50_duration_ms
        );
    }

    #[test]
    fn test_node_analytics_bottleneck_detection() {
        let workflow_id = WorkflowId::new_v4();
        let execution_id = ExecutionId::new_v4();
        let fast_node = NodeId::new_v4();
        let slow_node = NodeId::new_v4();

        let mut builder = AnalyticsBuilder::new(
            workflow_id,
            "test-workflow".to_string(),
            Utc::now() - Duration::hours(1),
            Utc::now(),
            PeriodType::Hourly,
        );

        let mut timeline = EventTimeline::new();

        // Fast node
        timeline.push(ExecutionEvent::node_completed(
            execution_id,
            workflow_id,
            fast_node,
            crate::NodeKind::Start,
            100,
            NodeMetrics::default(),
            HashMap::new(),
        ));

        // Slow node (bottleneck)
        timeline.push(ExecutionEvent::node_completed(
            execution_id,
            workflow_id,
            slow_node,
            crate::NodeKind::End,
            1000,
            NodeMetrics::default(),
            HashMap::new(),
        ));

        builder.add_timeline(timeline);
        let analytics = builder.build();

        assert_eq!(analytics.node_analytics.len(), 2);

        // The slowest node should be marked as bottleneck
        let bottleneck = analytics.node_analytics.iter().find(|n| n.is_bottleneck);
        assert!(bottleneck.is_some());
        assert_eq!(bottleneck.unwrap().node_id, slow_node);
    }

    #[test]
    fn test_error_pattern_analysis() {
        let workflow_id = WorkflowId::new_v4();
        let execution_id = ExecutionId::new_v4();
        let node_id = NodeId::new_v4();

        let mut builder = AnalyticsBuilder::new(
            workflow_id,
            "test-workflow".to_string(),
            Utc::now() - Duration::hours(1),
            Utc::now(),
            PeriodType::Hourly,
        );

        let mut timeline = EventTimeline::new();

        // Add multiple failures with same error
        for _ in 0..3 {
            timeline.push(ExecutionEvent::node_failed(
                execution_id,
                workflow_id,
                node_id,
                crate::NodeKind::Start,
                "Connection timeout".to_string(),
                None,
                0,
            ));
        }

        builder.add_timeline(timeline);
        let analytics = builder.build();

        assert_eq!(analytics.error_patterns.len(), 1);
        assert_eq!(analytics.error_patterns[0].occurrence_count, 3);
        assert_eq!(analytics.error_patterns[0].error_percentage, 100.0);
        assert!(analytics.error_patterns[0]
            .affected_nodes
            .contains(&node_id));
    }

    #[test]
    fn test_analytics_period_helpers() {
        let hourly = AnalyticsPeriod::last_hour();
        assert_eq!(hourly.period_type, PeriodType::Hourly);

        let daily = AnalyticsPeriod::last_day();
        assert_eq!(daily.period_type, PeriodType::Daily);

        let weekly = AnalyticsPeriod::last_week();
        assert_eq!(weekly.period_type, PeriodType::Weekly);

        let monthly = AnalyticsPeriod::last_month();
        assert_eq!(monthly.period_type, PeriodType::Monthly);
    }
}
