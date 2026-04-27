//! Performance profiling and analysis for workflow execution
//!
//! This module provides tools for profiling workflow execution performance:
//! - Timing analysis for nodes and workflows
//! - Resource usage tracking
//! - Bottleneck detection
//! - Performance recommendations

use oxify_model::NodeId;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Performance profile for a single node execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeProfile {
    /// Node ID
    pub node_id: NodeId,

    /// Node name
    pub node_name: String,

    /// Node type
    pub node_type: String,

    /// Execution duration
    pub duration: Duration,

    /// Start time (relative to workflow start)
    pub start_offset: Duration,

    /// End time (relative to workflow start)
    pub end_offset: Duration,

    /// Retry count
    pub retry_count: usize,

    /// Success status
    pub success: bool,

    /// Error message if failed
    pub error_message: Option<String>,
}

impl NodeProfile {
    /// Create a new node profile
    pub fn new(
        node_id: NodeId,
        node_name: String,
        node_type: String,
        duration: Duration,
        start_offset: Duration,
        end_offset: Duration,
    ) -> Self {
        Self {
            node_id,
            node_name,
            node_type,
            duration,
            start_offset,
            end_offset,
            retry_count: 0,
            success: true,
            error_message: None,
        }
    }

    /// Mark node as failed
    pub fn with_error(mut self, error: String) -> Self {
        self.success = false;
        self.error_message = Some(error);
        self
    }

    /// Set retry count
    pub fn with_retries(mut self, count: usize) -> Self {
        self.retry_count = count;
        self
    }
}

/// Performance profile for an entire workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowProfile {
    /// Workflow execution ID
    pub execution_id: String,

    /// Workflow name
    pub workflow_name: String,

    /// Total execution duration
    pub total_duration: Duration,

    /// Node profiles
    pub node_profiles: Vec<NodeProfile>,

    /// Critical path (longest path through the DAG)
    pub critical_path: Vec<NodeId>,

    /// Critical path duration
    pub critical_path_duration: Duration,

    /// Parallelism efficiency (0.0-1.0)
    pub parallelism_efficiency: f64,

    /// Total node execution time (sum of all nodes)
    pub total_node_time: Duration,
}

impl WorkflowProfile {
    /// Create a new workflow profile
    pub fn new(execution_id: String, workflow_name: String, total_duration: Duration) -> Self {
        Self {
            execution_id,
            workflow_name,
            total_duration,
            node_profiles: Vec::new(),
            critical_path: Vec::new(),
            critical_path_duration: Duration::ZERO,
            parallelism_efficiency: 0.0,
            total_node_time: Duration::ZERO,
        }
    }

    /// Add a node profile
    pub fn add_node_profile(&mut self, profile: NodeProfile) {
        self.total_node_time += profile.duration;
        self.node_profiles.push(profile);
    }

    /// Calculate critical path
    pub fn calculate_critical_path(&mut self) {
        // Find the longest sequential path
        // For simplicity, we'll use the total duration as critical path for now
        self.critical_path_duration = self.total_duration;

        // Calculate parallelism efficiency
        if self.total_duration.as_secs_f64() > 0.0 {
            self.parallelism_efficiency =
                self.total_node_time.as_secs_f64() / self.total_duration.as_secs_f64();
        }
    }

    /// Get slowest nodes (top N)
    pub fn get_slowest_nodes(&self, n: usize) -> Vec<&NodeProfile> {
        let mut sorted = self.node_profiles.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|x| std::cmp::Reverse(x.duration));
        sorted.into_iter().take(n).collect()
    }

    /// Get failed nodes
    pub fn get_failed_nodes(&self) -> Vec<&NodeProfile> {
        self.node_profiles.iter().filter(|p| !p.success).collect()
    }

    /// Get nodes with retries
    pub fn get_retried_nodes(&self) -> Vec<&NodeProfile> {
        self.node_profiles
            .iter()
            .filter(|p| p.retry_count > 0)
            .collect()
    }

    /// Get performance summary
    pub fn summary(&self) -> PerformanceSummary {
        let total_nodes = self.node_profiles.len();
        let failed_nodes = self.get_failed_nodes().len();
        let retried_nodes = self.get_retried_nodes().len();

        let avg_node_duration = if total_nodes > 0 {
            self.total_node_time / total_nodes as u32
        } else {
            Duration::ZERO
        };

        PerformanceSummary {
            total_duration: self.total_duration,
            total_nodes,
            failed_nodes,
            retried_nodes,
            avg_node_duration,
            critical_path_duration: self.critical_path_duration,
            parallelism_efficiency: self.parallelism_efficiency,
        }
    }
}

/// Performance summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    /// Total workflow duration
    pub total_duration: Duration,

    /// Total number of nodes
    pub total_nodes: usize,

    /// Number of failed nodes
    pub failed_nodes: usize,

    /// Number of nodes that required retries
    pub retried_nodes: usize,

    /// Average node execution duration
    pub avg_node_duration: Duration,

    /// Critical path duration
    pub critical_path_duration: Duration,

    /// Parallelism efficiency (0.0-1.0)
    pub parallelism_efficiency: f64,
}

/// Performance bottleneck detected in workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBottleneck {
    /// Type of bottleneck
    pub bottleneck_type: BottleneckType,

    /// Description
    pub description: String,

    /// Severity (0.0-1.0, higher is worse)
    pub severity: f64,

    /// Recommendation to fix
    pub recommendation: String,

    /// Affected nodes
    pub affected_nodes: Vec<NodeId>,
}

/// Type of performance bottleneck
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BottleneckType {
    /// Slow node execution
    SlowNode,

    /// Excessive retries
    ExcessiveRetries,

    /// Sequential execution (low parallelism)
    LowParallelism,

    /// Resource contention
    ResourceContention,

    /// Critical path bottleneck
    CriticalPath,
}

/// Performance analyzer
pub struct PerformanceAnalyzer {
    /// Threshold for slow node (percentile)
    pub slow_node_percentile: f64,

    /// Threshold for low parallelism efficiency
    pub low_parallelism_threshold: f64,

    /// Threshold for excessive retries
    pub retry_threshold: usize,
}

impl Default for PerformanceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceAnalyzer {
    /// Create a new performance analyzer with default thresholds
    pub fn new() -> Self {
        Self {
            slow_node_percentile: 0.9,      // Top 10% slowest nodes
            low_parallelism_threshold: 0.3, // Less than 30% parallelism
            retry_threshold: 2,             // More than 2 retries
        }
    }

    /// Analyze a workflow profile and detect bottlenecks
    pub fn analyze(&self, profile: &WorkflowProfile) -> Vec<PerformanceBottleneck> {
        let mut bottlenecks = Vec::new();

        // Detect slow nodes
        let slow_nodes = self.detect_slow_nodes(profile);
        bottlenecks.extend(slow_nodes);

        // Detect low parallelism
        if let Some(low_par) = self.detect_low_parallelism(profile) {
            bottlenecks.push(low_par);
        }

        // Detect excessive retries
        let retry_issues = self.detect_excessive_retries(profile);
        bottlenecks.extend(retry_issues);

        bottlenecks
    }

    /// Detect slow nodes
    fn detect_slow_nodes(&self, profile: &WorkflowProfile) -> Vec<PerformanceBottleneck> {
        let mut bottlenecks = Vec::new();

        if profile.node_profiles.is_empty() {
            return bottlenecks;
        }

        // Calculate percentile threshold
        let mut durations: Vec<Duration> =
            profile.node_profiles.iter().map(|p| p.duration).collect();
        durations.sort();

        let threshold_idx = ((durations.len() as f64 * self.slow_node_percentile).ceil() as usize)
            .min(durations.len() - 1);
        let threshold = durations[threshold_idx];

        // Find slow nodes
        for node in &profile.node_profiles {
            if node.duration >= threshold && node.duration.as_secs_f64() > 1.0 {
                let severity =
                    node.duration.as_secs_f64() / profile.total_duration.as_secs_f64().max(1.0);

                bottlenecks.push(PerformanceBottleneck {
                    bottleneck_type: BottleneckType::SlowNode,
                    description: format!(
                        "Node '{}' took {:.2}s ({}% of total time)",
                        node.node_name,
                        node.duration.as_secs_f64(),
                        (severity * 100.0) as u32
                    ),
                    severity: severity.min(1.0),
                    recommendation: format!(
                        "Consider optimizing '{}' execution or caching results",
                        node.node_name
                    ),
                    affected_nodes: vec![node.node_id],
                });
            }
        }

        bottlenecks
    }

    /// Detect low parallelism
    fn detect_low_parallelism(&self, profile: &WorkflowProfile) -> Option<PerformanceBottleneck> {
        if profile.parallelism_efficiency < self.low_parallelism_threshold {
            Some(PerformanceBottleneck {
                bottleneck_type: BottleneckType::LowParallelism,
                description: format!(
                    "Low parallelism efficiency: {:.1}%",
                    profile.parallelism_efficiency * 100.0
                ),
                severity: 1.0 - profile.parallelism_efficiency,
                recommendation:
                    "Review workflow structure to identify opportunities for parallel execution"
                        .to_string(),
                affected_nodes: Vec::new(),
            })
        } else {
            None
        }
    }

    /// Detect excessive retries
    fn detect_excessive_retries(&self, profile: &WorkflowProfile) -> Vec<PerformanceBottleneck> {
        let mut bottlenecks = Vec::new();

        for node in &profile.node_profiles {
            if node.retry_count > self.retry_threshold {
                bottlenecks.push(PerformanceBottleneck {
                    bottleneck_type: BottleneckType::ExcessiveRetries,
                    description: format!(
                        "Node '{}' required {} retries",
                        node.node_name, node.retry_count
                    ),
                    severity: (node.retry_count as f64 / 10.0).min(1.0),
                    recommendation: format!(
                        "Investigate reliability issues with '{}' and consider increasing timeout or fixing root cause",
                        node.node_name
                    ),
                    affected_nodes: vec![node.node_id],
                });
            }
        }

        bottlenecks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_node_profile_creation() {
        let profile = NodeProfile::new(
            Uuid::new_v4(),
            "Test Node".to_string(),
            "LLM".to_string(),
            Duration::from_secs(5),
            Duration::from_secs(0),
            Duration::from_secs(5),
        );

        assert_eq!(profile.node_name, "Test Node");
        assert_eq!(profile.duration, Duration::from_secs(5));
        assert!(profile.success);
        assert_eq!(profile.retry_count, 0);
    }

    #[test]
    fn test_workflow_profile() {
        let mut profile = WorkflowProfile::new(
            "exec123".to_string(),
            "Test Workflow".to_string(),
            Duration::from_secs(10),
        );

        let node1 = NodeProfile::new(
            Uuid::new_v4(),
            "Node 1".to_string(),
            "LLM".to_string(),
            Duration::from_secs(3),
            Duration::from_secs(0),
            Duration::from_secs(3),
        );

        let node2 = NodeProfile::new(
            Uuid::new_v4(),
            "Node 2".to_string(),
            "Retriever".to_string(),
            Duration::from_secs(7),
            Duration::from_secs(3),
            Duration::from_secs(10),
        );

        profile.add_node_profile(node1);
        profile.add_node_profile(node2);
        profile.calculate_critical_path();

        assert_eq!(profile.node_profiles.len(), 2);
        assert_eq!(profile.total_node_time, Duration::from_secs(10));
        assert!(profile.parallelism_efficiency > 0.0);
    }

    #[test]
    fn test_slowest_nodes() {
        let mut profile = WorkflowProfile::new(
            "exec123".to_string(),
            "Test".to_string(),
            Duration::from_secs(20),
        );

        for i in 1..=5 {
            let node = NodeProfile::new(
                Uuid::new_v4(),
                format!("Node {}", i),
                "LLM".to_string(),
                Duration::from_secs(i as u64),
                Duration::ZERO,
                Duration::from_secs(i as u64),
            );
            profile.add_node_profile(node);
        }

        let slowest = profile.get_slowest_nodes(2);
        assert_eq!(slowest.len(), 2);
        assert_eq!(slowest[0].node_name, "Node 5");
        assert_eq!(slowest[1].node_name, "Node 4");
    }

    #[test]
    fn test_failed_nodes() {
        let mut profile = WorkflowProfile::new(
            "exec123".to_string(),
            "Test".to_string(),
            Duration::from_secs(10),
        );

        let success_node = NodeProfile::new(
            Uuid::new_v4(),
            "Success".to_string(),
            "LLM".to_string(),
            Duration::from_secs(5),
            Duration::ZERO,
            Duration::from_secs(5),
        );

        let failed_node = NodeProfile::new(
            Uuid::new_v4(),
            "Failed".to_string(),
            "LLM".to_string(),
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(10),
        )
        .with_error("Test error".to_string());

        profile.add_node_profile(success_node);
        profile.add_node_profile(failed_node);

        let failed = profile.get_failed_nodes();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].node_name, "Failed");
    }

    #[test]
    fn test_performance_analyzer() {
        let mut profile = WorkflowProfile::new(
            "exec123".to_string(),
            "Test".to_string(),
            Duration::from_secs(10),
        );

        // Add a slow node
        let slow_node = NodeProfile::new(
            Uuid::new_v4(),
            "Slow Node".to_string(),
            "LLM".to_string(),
            Duration::from_secs(8),
            Duration::ZERO,
            Duration::from_secs(8),
        );

        let fast_node = NodeProfile::new(
            Uuid::new_v4(),
            "Fast Node".to_string(),
            "LLM".to_string(),
            Duration::from_secs(2),
            Duration::from_secs(8),
            Duration::from_secs(10),
        );

        profile.add_node_profile(slow_node);
        profile.add_node_profile(fast_node);
        profile.calculate_critical_path();

        let analyzer = PerformanceAnalyzer::new();
        let bottlenecks = analyzer.analyze(&profile);

        assert!(!bottlenecks.is_empty());
    }

    #[test]
    fn test_excessive_retries_detection() {
        let mut profile = WorkflowProfile::new(
            "exec123".to_string(),
            "Test".to_string(),
            Duration::from_secs(10),
        );

        let retry_node = NodeProfile::new(
            Uuid::new_v4(),
            "Retry Node".to_string(),
            "LLM".to_string(),
            Duration::from_secs(5),
            Duration::ZERO,
            Duration::from_secs(5),
        )
        .with_retries(5);

        profile.add_node_profile(retry_node);

        let analyzer = PerformanceAnalyzer::new();
        let bottlenecks = analyzer.analyze(&profile);

        let retry_bottlenecks: Vec<_> = bottlenecks
            .iter()
            .filter(|b| b.bottleneck_type == BottleneckType::ExcessiveRetries)
            .collect();

        assert_eq!(retry_bottlenecks.len(), 1);
    }
}
