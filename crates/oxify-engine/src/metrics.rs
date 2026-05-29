//! Execution metrics and statistics
//!
//! This module provides metrics collection for workflow execution monitoring.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Execution metrics collector
#[derive(Debug, Default)]
pub struct ExecutionMetrics {
    /// Total workflows executed
    total_workflows: AtomicU64,

    /// Successful workflow completions
    successful_workflows: AtomicU64,

    /// Failed workflow executions
    failed_workflows: AtomicU64,

    /// Total nodes executed
    total_nodes: AtomicU64,

    /// Successful node completions
    successful_nodes: AtomicU64,

    /// Failed node executions
    failed_nodes: AtomicU64,

    /// Retried nodes
    retried_nodes: AtomicU64,

    /// Timed out nodes
    timed_out_nodes: AtomicU64,

    /// Checkpoints created
    checkpoints_created: AtomicU64,

    /// Executions resumed from checkpoint
    executions_resumed: AtomicU64,

    /// Per-workflow execution times (in milliseconds)
    workflow_durations: Arc<RwLock<Vec<u64>>>,

    /// Per-node execution times (in milliseconds)
    node_durations: Arc<RwLock<Vec<u64>>>,

    /// Active executions
    active_executions: Arc<RwLock<HashMap<Uuid, ExecutionInfo>>>,
}

/// Information about an active execution
#[derive(Debug, Clone)]
pub struct ExecutionInfo {
    /// Workflow ID
    pub workflow_id: Uuid,

    /// Execution ID
    pub execution_id: Uuid,

    /// Start time
    pub started_at: Instant,

    /// Current level being executed
    pub current_level: usize,

    /// Completed nodes count
    pub completed_nodes: usize,

    /// Total nodes count
    pub total_nodes: usize,
}

/// Execution statistics snapshot
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionStats {
    /// Total workflows executed
    pub total_workflows: u64,

    /// Successful workflow completions
    pub successful_workflows: u64,

    /// Failed workflow executions
    pub failed_workflows: u64,

    /// Success rate (0.0 - 1.0)
    pub workflow_success_rate: f64,

    /// Total nodes executed
    pub total_nodes: u64,

    /// Successful node completions
    pub successful_nodes: u64,

    /// Failed node executions
    pub failed_nodes: u64,

    /// Retried nodes
    pub retried_nodes: u64,

    /// Timed out nodes
    pub timed_out_nodes: u64,

    /// Checkpoints created
    pub checkpoints_created: u64,

    /// Executions resumed
    pub executions_resumed: u64,

    /// Average workflow duration (milliseconds)
    pub avg_workflow_duration_ms: f64,

    /// P50 workflow duration (milliseconds)
    pub p50_workflow_duration_ms: u64,

    /// P95 workflow duration (milliseconds)
    pub p95_workflow_duration_ms: u64,

    /// P99 workflow duration (milliseconds)
    pub p99_workflow_duration_ms: u64,

    /// Average node duration (milliseconds)
    pub avg_node_duration_ms: f64,

    /// Currently active executions
    pub active_executions: usize,
}

impl ExecutionMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self::default()
    }

    /// Record workflow started
    pub fn record_workflow_started(
        &self,
        workflow_id: Uuid,
        execution_id: Uuid,
        total_nodes: usize,
    ) {
        self.total_workflows.fetch_add(1, Ordering::Relaxed);

        let info = ExecutionInfo {
            workflow_id,
            execution_id,
            started_at: Instant::now(),
            current_level: 0,
            completed_nodes: 0,
            total_nodes,
        };

        self.active_executions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(execution_id, info);
    }

    /// Record workflow completed successfully
    pub fn record_workflow_completed(&self, execution_id: Uuid) {
        self.successful_workflows.fetch_add(1, Ordering::Relaxed);

        if let Some(info) = self
            .active_executions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&execution_id)
        {
            let duration_ms = info.started_at.elapsed().as_millis() as u64;
            self.workflow_durations
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .push(duration_ms);
        }
    }

    /// Record workflow failed
    pub fn record_workflow_failed(&self, execution_id: Uuid) {
        self.failed_workflows.fetch_add(1, Ordering::Relaxed);

        if let Some(info) = self
            .active_executions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&execution_id)
        {
            let duration_ms = info.started_at.elapsed().as_millis() as u64;
            self.workflow_durations
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .push(duration_ms);
        }
    }

    /// Record node started
    pub fn record_node_started(&self, execution_id: Uuid) {
        self.total_nodes.fetch_add(1, Ordering::Relaxed);

        if let Some(info) = self
            .active_executions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&execution_id)
        {
            info.completed_nodes += 1;
        }
    }

    /// Record node completed successfully
    pub fn record_node_completed(&self, duration: Duration) {
        self.successful_nodes.fetch_add(1, Ordering::Relaxed);
        self.node_durations
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .push(duration.as_millis() as u64);
    }

    /// Record node failed
    pub fn record_node_failed(&self) {
        self.failed_nodes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record node retry
    pub fn record_node_retry(&self) {
        self.retried_nodes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record node timeout
    pub fn record_node_timeout(&self) {
        self.timed_out_nodes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record checkpoint created
    pub fn record_checkpoint_created(&self) {
        self.checkpoints_created.fetch_add(1, Ordering::Relaxed);
    }

    /// Record execution resumed
    pub fn record_execution_resumed(&self) {
        self.executions_resumed.fetch_add(1, Ordering::Relaxed);
    }

    /// Update current level for an execution
    pub fn update_level(&self, execution_id: Uuid, level: usize) {
        if let Some(info) = self
            .active_executions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&execution_id)
        {
            info.current_level = level;
        }
    }

    /// Get current statistics
    pub fn get_stats(&self) -> ExecutionStats {
        let total_workflows = self.total_workflows.load(Ordering::Relaxed);
        let successful_workflows = self.successful_workflows.load(Ordering::Relaxed);
        let failed_workflows = self.failed_workflows.load(Ordering::Relaxed);

        let workflow_success_rate = if total_workflows > 0 {
            successful_workflows as f64 / total_workflows as f64
        } else {
            0.0
        };

        let workflow_durations = self
            .workflow_durations
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let (avg_workflow, p50_workflow, p95_workflow, p99_workflow) =
            calculate_percentiles(&workflow_durations);

        let node_durations = self
            .node_durations
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let (avg_node, _, _, _) = calculate_percentiles(&node_durations);

        ExecutionStats {
            total_workflows,
            successful_workflows,
            failed_workflows,
            workflow_success_rate,
            total_nodes: self.total_nodes.load(Ordering::Relaxed),
            successful_nodes: self.successful_nodes.load(Ordering::Relaxed),
            failed_nodes: self.failed_nodes.load(Ordering::Relaxed),
            retried_nodes: self.retried_nodes.load(Ordering::Relaxed),
            timed_out_nodes: self.timed_out_nodes.load(Ordering::Relaxed),
            checkpoints_created: self.checkpoints_created.load(Ordering::Relaxed),
            executions_resumed: self.executions_resumed.load(Ordering::Relaxed),
            avg_workflow_duration_ms: avg_workflow,
            p50_workflow_duration_ms: p50_workflow,
            p95_workflow_duration_ms: p95_workflow,
            p99_workflow_duration_ms: p99_workflow,
            avg_node_duration_ms: avg_node,
            active_executions: self
                .active_executions
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .len(),
        }
    }

    /// Get list of active executions
    pub fn get_active_executions(&self) -> Vec<ExecutionInfo> {
        self.active_executions
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.total_workflows.store(0, Ordering::Relaxed);
        self.successful_workflows.store(0, Ordering::Relaxed);
        self.failed_workflows.store(0, Ordering::Relaxed);
        self.total_nodes.store(0, Ordering::Relaxed);
        self.successful_nodes.store(0, Ordering::Relaxed);
        self.failed_nodes.store(0, Ordering::Relaxed);
        self.retried_nodes.store(0, Ordering::Relaxed);
        self.timed_out_nodes.store(0, Ordering::Relaxed);
        self.checkpoints_created.store(0, Ordering::Relaxed);
        self.executions_resumed.store(0, Ordering::Relaxed);
        self.workflow_durations
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.node_durations
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.active_executions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}

/// Calculate percentiles from a list of values
fn calculate_percentiles(values: &[u64]) -> (f64, u64, u64, u64) {
    if values.is_empty() {
        return (0.0, 0, 0, 0);
    }

    let mut sorted = values.to_vec();
    sorted.sort_unstable();

    let avg = sorted.iter().sum::<u64>() as f64 / sorted.len() as f64;
    let p50 = percentile(&sorted, 50);
    let p95 = percentile(&sorted, 95);
    let p99 = percentile(&sorted, 99);

    (avg, p50, p95, p99)
}

/// Calculate a specific percentile
fn percentile(sorted: &[u64], p: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }

    let idx = (p * sorted.len() / 100).min(sorted.len() - 1);
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_workflow_tracking() {
        let metrics = ExecutionMetrics::new();

        let workflow_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();

        metrics.record_workflow_started(workflow_id, execution_id, 5);

        let active = metrics.get_active_executions();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].execution_id, execution_id);

        metrics.record_workflow_completed(execution_id);

        let stats = metrics.get_stats();
        assert_eq!(stats.total_workflows, 1);
        assert_eq!(stats.successful_workflows, 1);
        assert_eq!(stats.active_executions, 0);
    }

    #[test]
    fn test_metrics_node_tracking() {
        let metrics = ExecutionMetrics::new();

        metrics.record_node_completed(Duration::from_millis(100));
        metrics.record_node_completed(Duration::from_millis(200));
        metrics.record_node_failed();
        metrics.record_node_retry();

        let stats = metrics.get_stats();
        assert_eq!(stats.successful_nodes, 2);
        assert_eq!(stats.failed_nodes, 1);
        assert_eq!(stats.retried_nodes, 1);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = ExecutionMetrics::new();

        let workflow_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();

        metrics.record_workflow_started(workflow_id, execution_id, 5);
        metrics.record_workflow_completed(execution_id);

        let stats = metrics.get_stats();
        assert_eq!(stats.total_workflows, 1);

        metrics.reset();

        let stats = metrics.get_stats();
        assert_eq!(stats.total_workflows, 0);
    }

    #[test]
    fn test_percentile_calculation() {
        let values = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        let (avg, p50, p95, p99) = calculate_percentiles(&values);

        assert!((avg - 55.0).abs() < 0.001);
        // With 10 values, p50 at idx (50*10/100)=5 is the 6th value (60)
        assert_eq!(p50, 60);
        assert_eq!(p95, 100);
        assert_eq!(p99, 100);
    }
}
