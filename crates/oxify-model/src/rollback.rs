//! Execution rollback mechanism for workflows
//!
//! This module provides the ability to create snapshots of execution state
//! and rollback to previous states when needed.
//!
//! # Features
//!
//! - Create point-in-time snapshots of execution state
//! - Store execution history with automatic pruning
//! - Rollback to any previous snapshot
//! - Support for selective rollback (specific variables, node results)
//!
//! # Example
//!
//! ```
//! use oxify_model::rollback::{ExecutionSnapshot, RollbackManager};
//! use oxify_model::execution::ExecutionContext;
//! use uuid::Uuid;
//!
//! let mut manager = RollbackManager::new(10); // Keep 10 snapshots max
//! let mut context = ExecutionContext::new(Uuid::new_v4());
//!
//! // Create a snapshot before risky operation
//! let snapshot = ExecutionSnapshot::from_context(&context);
//! manager.push_snapshot(snapshot);
//!
//! // If operation fails, rollback
//! let result = manager.rollback(&mut context);
//! assert!(result.success);
//! ```

use crate::execution::{ExecutionContext, ExecutionState, NodeExecutionResult};
use crate::NodeId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// A point-in-time snapshot of execution state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExecutionSnapshot {
    /// Unique snapshot identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: Uuid,

    /// Timestamp when snapshot was created
    pub created_at: DateTime<Utc>,

    /// Label/description for this snapshot
    pub label: Option<String>,

    /// Node that triggered this snapshot (if any)
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub trigger_node: Option<NodeId>,

    /// Execution state at snapshot time
    pub state: ExecutionState,

    /// Variables at snapshot time
    pub variables: HashMap<String, serde_json::Value>,

    /// Node results at snapshot time
    #[cfg_attr(feature = "openapi", schema(value_type = HashMap<String, NodeExecutionResult>))]
    pub node_results: HashMap<NodeId, NodeExecutionResult>,

    /// Metadata for the snapshot
    #[serde(default)]
    pub metadata: SnapshotMetadata,
}

/// Metadata about a snapshot
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SnapshotMetadata {
    /// Reason for creating the snapshot
    pub reason: Option<String>,

    /// User who created the snapshot (if applicable)
    pub created_by: Option<String>,

    /// Whether this is an automatic checkpoint
    pub is_auto: bool,

    /// Custom metadata
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl ExecutionSnapshot {
    /// Create a new snapshot from an execution context
    pub fn from_context(ctx: &ExecutionContext) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            label: None,
            trigger_node: None,
            state: ctx.state.clone(),
            variables: ctx.variables.clone(),
            node_results: ctx.node_results.clone(),
            metadata: SnapshotMetadata::default(),
        }
    }

    /// Create a snapshot with a label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Create a snapshot with a trigger node
    pub fn with_trigger_node(mut self, node_id: NodeId) -> Self {
        self.trigger_node = Some(node_id);
        self
    }

    /// Create a snapshot with a reason
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.metadata.reason = Some(reason.into());
        self
    }

    /// Mark as automatic snapshot
    pub fn as_auto(mut self) -> Self {
        self.metadata.is_auto = true;
        self
    }

    /// Apply this snapshot to an execution context
    pub fn apply_to(&self, ctx: &mut ExecutionContext) {
        ctx.state = self.state.clone();
        ctx.variables = self.variables.clone();
        ctx.node_results = self.node_results.clone();
    }

    /// Get the number of completed nodes in this snapshot
    pub fn completed_node_count(&self) -> usize {
        self.node_results.len()
    }

    /// Get the variable count in this snapshot
    pub fn variable_count(&self) -> usize {
        self.variables.len()
    }
}

/// Result of a rollback operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RollbackResult {
    /// Whether rollback was successful
    pub success: bool,

    /// The snapshot that was applied
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub applied_snapshot_id: Option<Uuid>,

    /// Number of node results removed
    pub nodes_removed: usize,

    /// Number of variables changed
    pub variables_changed: usize,

    /// Error message if failed
    pub error: Option<String>,

    /// Timestamp of the rollback
    pub rolled_back_at: DateTime<Utc>,
}

impl RollbackResult {
    /// Create a successful rollback result
    pub fn success(snapshot_id: Uuid, nodes_removed: usize, variables_changed: usize) -> Self {
        Self {
            success: true,
            applied_snapshot_id: Some(snapshot_id),
            nodes_removed,
            variables_changed,
            error: None,
            rolled_back_at: Utc::now(),
        }
    }

    /// Create a failed rollback result
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            applied_snapshot_id: None,
            nodes_removed: 0,
            variables_changed: 0,
            error: Some(error.into()),
            rolled_back_at: Utc::now(),
        }
    }
}

/// Manages execution snapshots and rollback operations
#[derive(Debug, Clone)]
pub struct RollbackManager {
    /// Snapshot history (newest first)
    snapshots: VecDeque<ExecutionSnapshot>,

    /// Maximum number of snapshots to keep
    max_snapshots: usize,

    /// Whether automatic snapshots are enabled
    auto_snapshot: bool,

    /// Auto-snapshot interval (every N nodes)
    auto_snapshot_interval: usize,

    /// Counter for auto-snapshots
    node_counter: usize,
}

impl RollbackManager {
    /// Create a new rollback manager
    ///
    /// # Arguments
    /// * `max_snapshots` - Maximum number of snapshots to keep
    pub fn new(max_snapshots: usize) -> Self {
        Self {
            snapshots: VecDeque::new(),
            max_snapshots,
            auto_snapshot: false,
            auto_snapshot_interval: 5,
            node_counter: 0,
        }
    }

    /// Enable automatic snapshots every N nodes
    pub fn with_auto_snapshot(mut self, interval: usize) -> Self {
        self.auto_snapshot = true;
        self.auto_snapshot_interval = interval;
        self
    }

    /// Push a new snapshot
    ///
    /// If max_snapshots is reached, the oldest snapshot is removed.
    pub fn push_snapshot(&mut self, snapshot: ExecutionSnapshot) {
        // Add to front (newest first)
        self.snapshots.push_front(snapshot);

        // Prune if over limit
        while self.snapshots.len() > self.max_snapshots {
            self.snapshots.pop_back();
        }
    }

    /// Create and push a snapshot from an execution context
    pub fn create_snapshot(&mut self, ctx: &ExecutionContext) -> Uuid {
        let snapshot = ExecutionSnapshot::from_context(ctx);
        let id = snapshot.id;
        self.push_snapshot(snapshot);
        id
    }

    /// Create a labeled snapshot
    pub fn create_labeled_snapshot(
        &mut self,
        ctx: &ExecutionContext,
        label: impl Into<String>,
    ) -> Uuid {
        let snapshot = ExecutionSnapshot::from_context(ctx).with_label(label);
        let id = snapshot.id;
        self.push_snapshot(snapshot);
        id
    }

    /// Called when a node is about to execute
    ///
    /// Creates automatic snapshots if enabled.
    pub fn on_node_execute(&mut self, ctx: &ExecutionContext, node_id: NodeId) -> Option<Uuid> {
        self.node_counter += 1;

        if self.auto_snapshot
            && self
                .node_counter
                .is_multiple_of(self.auto_snapshot_interval)
        {
            let snapshot = ExecutionSnapshot::from_context(ctx)
                .with_trigger_node(node_id)
                .as_auto();
            let id = snapshot.id;
            self.push_snapshot(snapshot);
            Some(id)
        } else {
            None
        }
    }

    /// Rollback to the most recent snapshot
    pub fn rollback(&mut self, ctx: &mut ExecutionContext) -> RollbackResult {
        if let Some(snapshot) = self.snapshots.front() {
            let nodes_before = ctx.node_results.len();
            let vars_before = ctx.variables.clone();

            snapshot.apply_to(ctx);

            let nodes_removed = nodes_before.saturating_sub(ctx.node_results.len());
            let variables_changed = vars_before
                .iter()
                .filter(|(k, v)| ctx.variables.get(*k) != Some(*v))
                .count();

            RollbackResult::success(snapshot.id, nodes_removed, variables_changed)
        } else {
            RollbackResult::failure("No snapshots available")
        }
    }

    /// Rollback to a specific snapshot by ID
    pub fn rollback_to(&mut self, ctx: &mut ExecutionContext, snapshot_id: Uuid) -> RollbackResult {
        if let Some(snapshot) = self.snapshots.iter().find(|s| s.id == snapshot_id) {
            let nodes_before = ctx.node_results.len();
            let vars_before = ctx.variables.clone();

            snapshot.clone().apply_to(ctx);

            let nodes_removed = nodes_before.saturating_sub(ctx.node_results.len());
            let variables_changed = vars_before
                .iter()
                .filter(|(k, v)| ctx.variables.get(*k) != Some(*v))
                .count();

            RollbackResult::success(snapshot_id, nodes_removed, variables_changed)
        } else {
            RollbackResult::failure(format!("Snapshot {} not found", snapshot_id))
        }
    }

    /// Rollback N steps in history
    ///
    /// - `rollback_n(1)` returns to the most recent snapshot (index 0)
    /// - `rollback_n(2)` returns to 2 snapshots ago (index 1)
    /// - `rollback_n(3)` returns to 3 snapshots ago (index 2)
    ///
    /// Note: "Steps" refers to how many states back in history to go,
    /// where each snapshot is one state.
    pub fn rollback_n(&mut self, ctx: &mut ExecutionContext, steps: usize) -> RollbackResult {
        if steps == 0 {
            return RollbackResult::failure("Cannot rollback 0 steps");
        }

        if steps > self.snapshots.len() {
            return RollbackResult::failure(format!(
                "Cannot rollback {} steps, only {} snapshots available",
                steps,
                self.snapshots.len()
            ));
        }

        // Get the snapshot N steps back (1-indexed for user convenience)
        // rollback_n(1) -> index 0, rollback_n(2) -> index 1, etc.
        if let Some(snapshot) = self.snapshots.get(steps - 1) {
            let nodes_before = ctx.node_results.len();
            let vars_before = ctx.variables.clone();

            snapshot.clone().apply_to(ctx);

            let nodes_removed = nodes_before.saturating_sub(ctx.node_results.len());
            let variables_changed = vars_before
                .iter()
                .filter(|(k, v)| ctx.variables.get(*k) != Some(*v))
                .count();

            RollbackResult::success(snapshot.id, nodes_removed, variables_changed)
        } else {
            RollbackResult::failure("Snapshot not found")
        }
    }

    /// Get the most recent snapshot
    pub fn latest_snapshot(&self) -> Option<&ExecutionSnapshot> {
        self.snapshots.front()
    }

    /// Get a snapshot by ID
    pub fn get_snapshot(&self, id: Uuid) -> Option<&ExecutionSnapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// List all snapshots (newest first)
    pub fn list_snapshots(&self) -> Vec<&ExecutionSnapshot> {
        self.snapshots.iter().collect()
    }

    /// Get the number of available snapshots
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Clear all snapshots
    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.node_counter = 0;
    }

    /// Remove snapshots older than a timestamp
    pub fn prune_before(&mut self, timestamp: DateTime<Utc>) -> usize {
        let before = self.snapshots.len();
        self.snapshots.retain(|s| s.created_at >= timestamp);
        before - self.snapshots.len()
    }

    /// Get summary of snapshot history
    pub fn summary(&self) -> RollbackSummary {
        RollbackSummary {
            total_snapshots: self.snapshots.len(),
            max_snapshots: self.max_snapshots,
            auto_snapshot_enabled: self.auto_snapshot,
            auto_snapshot_interval: self.auto_snapshot_interval,
            oldest_snapshot: self.snapshots.back().map(|s| s.created_at),
            newest_snapshot: self.snapshots.front().map(|s| s.created_at),
            nodes_processed: self.node_counter,
        }
    }
}

impl Default for RollbackManager {
    fn default() -> Self {
        Self::new(10)
    }
}

/// Summary of rollback manager state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RollbackSummary {
    /// Total snapshots stored
    pub total_snapshots: usize,

    /// Maximum snapshots allowed
    pub max_snapshots: usize,

    /// Whether auto-snapshot is enabled
    pub auto_snapshot_enabled: bool,

    /// Auto-snapshot interval
    pub auto_snapshot_interval: usize,

    /// Timestamp of oldest snapshot
    pub oldest_snapshot: Option<DateTime<Utc>>,

    /// Timestamp of newest snapshot
    pub newest_snapshot: Option<DateTime<Utc>>,

    /// Total nodes processed
    pub nodes_processed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_context() -> ExecutionContext {
        let mut ctx = ExecutionContext::new(Uuid::new_v4());
        ctx.set_variable("key1".to_string(), serde_json::json!("value1"));
        ctx.set_variable("key2".to_string(), serde_json::json!(42));
        ctx
    }

    #[test]
    fn test_snapshot_creation() {
        let ctx = create_test_context();
        let snapshot = ExecutionSnapshot::from_context(&ctx);

        assert!(snapshot.label.is_none());
        assert!(snapshot.trigger_node.is_none());
        assert_eq!(snapshot.variables.len(), 2);
        assert_eq!(snapshot.state, ExecutionState::Running);
    }

    #[test]
    fn test_snapshot_with_label() {
        let ctx = create_test_context();
        let snapshot = ExecutionSnapshot::from_context(&ctx).with_label("Before LLM call");

        assert_eq!(snapshot.label, Some("Before LLM call".to_string()));
    }

    #[test]
    fn test_snapshot_with_trigger_node() {
        let ctx = create_test_context();
        let node_id = Uuid::new_v4();
        let snapshot = ExecutionSnapshot::from_context(&ctx).with_trigger_node(node_id);

        assert_eq!(snapshot.trigger_node, Some(node_id));
    }

    #[test]
    fn test_snapshot_apply_to() {
        let mut ctx = create_test_context();
        let snapshot = ExecutionSnapshot::from_context(&ctx);

        // Modify context
        ctx.set_variable("key3".to_string(), serde_json::json!("new_value"));
        ctx.state = ExecutionState::Failed("test error".to_string());

        // Apply snapshot
        snapshot.apply_to(&mut ctx);

        assert_eq!(ctx.variables.len(), 2);
        assert!(!ctx.variables.contains_key("key3"));
        assert_eq!(ctx.state, ExecutionState::Running);
    }

    #[test]
    fn test_rollback_manager_push() {
        let mut manager = RollbackManager::new(3);

        for i in 0..5 {
            let mut ctx = create_test_context();
            ctx.set_variable(format!("iter_{}", i), serde_json::json!(i));
            manager.create_snapshot(&ctx);
        }

        // Should only keep 3 snapshots
        assert_eq!(manager.snapshot_count(), 3);
    }

    #[test]
    fn test_rollback_latest() {
        let mut manager = RollbackManager::new(10);
        let mut ctx = create_test_context();

        // Create snapshot
        manager.create_snapshot(&ctx);

        // Modify context
        ctx.set_variable("new_key".to_string(), serde_json::json!("new_value"));
        ctx.state = ExecutionState::Failed("error".to_string());

        // Rollback
        let result = manager.rollback(&mut ctx);

        assert!(result.success);
        assert!(!ctx.variables.contains_key("new_key"));
        assert_eq!(ctx.state, ExecutionState::Running);
    }

    #[test]
    fn test_rollback_to_specific() {
        let mut manager = RollbackManager::new(10);
        let mut ctx = create_test_context();

        // Create first snapshot
        let first_id = manager.create_snapshot(&ctx);

        // Modify and create second snapshot
        ctx.set_variable("modified".to_string(), serde_json::json!(true));
        let _second_id = manager.create_snapshot(&ctx);

        // Modify again
        ctx.set_variable("more_changes".to_string(), serde_json::json!("value"));

        // Rollback to first snapshot
        let result = manager.rollback_to(&mut ctx, first_id);

        assert!(result.success);
        assert!(!ctx.variables.contains_key("modified"));
        assert!(!ctx.variables.contains_key("more_changes"));
    }

    #[test]
    fn test_rollback_n_steps() {
        let mut manager = RollbackManager::new(10);
        let mut ctx = create_test_context();

        // Create snapshots (each snapshot contains variables up to that point)
        // snap0: step_0, snap1: step_0,1, snap2: step_0,1,2, snap3: step_0,1,2,3, snap4: step_0,1,2,3,4
        for i in 0..5 {
            ctx.set_variable(format!("step_{}", i), serde_json::json!(i));
            manager.create_snapshot(&ctx);
        }

        // Add one more variable that isn't in any snapshot
        ctx.set_variable("step_5".to_string(), serde_json::json!(5));

        // Rollback 3 steps goes to the 3rd most recent snapshot (snap2)
        // snap4(idx0) -> snap3(idx1) -> snap2(idx2)
        let result = manager.rollback_n(&mut ctx, 3);

        assert!(result.success);
        // snap2 has step_0, step_1, step_2 but NOT step_3, step_4, step_5
        assert!(ctx.variables.contains_key("step_0"));
        assert!(ctx.variables.contains_key("step_1"));
        assert!(ctx.variables.contains_key("step_2"));
        assert!(!ctx.variables.contains_key("step_3"));
        assert!(!ctx.variables.contains_key("step_4"));
        assert!(!ctx.variables.contains_key("step_5"));
    }

    #[test]
    fn test_rollback_no_snapshots() {
        let mut manager = RollbackManager::new(10);
        let mut ctx = create_test_context();

        let result = manager.rollback(&mut ctx);

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_auto_snapshot() {
        let mut manager = RollbackManager::new(10).with_auto_snapshot(2);
        let ctx = create_test_context();

        // First node - no snapshot
        let result = manager.on_node_execute(&ctx, Uuid::new_v4());
        assert!(result.is_none());

        // Second node - snapshot
        let result = manager.on_node_execute(&ctx, Uuid::new_v4());
        assert!(result.is_some());

        // Third node - no snapshot
        let result = manager.on_node_execute(&ctx, Uuid::new_v4());
        assert!(result.is_none());

        // Fourth node - snapshot
        let result = manager.on_node_execute(&ctx, Uuid::new_v4());
        assert!(result.is_some());

        assert_eq!(manager.snapshot_count(), 2);
    }

    #[test]
    fn test_prune_before() {
        let mut manager = RollbackManager::new(10);
        let ctx = create_test_context();

        // Create some snapshots
        manager.create_snapshot(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let cutoff = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.create_snapshot(&ctx);
        manager.create_snapshot(&ctx);

        let pruned = manager.prune_before(cutoff);
        assert_eq!(pruned, 1);
        assert_eq!(manager.snapshot_count(), 2);
    }

    #[test]
    fn test_rollback_summary() {
        let mut manager = RollbackManager::new(5).with_auto_snapshot(3);
        let ctx = create_test_context();

        manager.create_snapshot(&ctx);
        manager.create_snapshot(&ctx);

        let summary = manager.summary();
        assert_eq!(summary.total_snapshots, 2);
        assert_eq!(summary.max_snapshots, 5);
        assert!(summary.auto_snapshot_enabled);
        assert_eq!(summary.auto_snapshot_interval, 3);
        assert!(summary.oldest_snapshot.is_some());
        assert!(summary.newest_snapshot.is_some());
    }

    #[test]
    fn test_clear() {
        let mut manager = RollbackManager::new(10).with_auto_snapshot(2);
        let ctx = create_test_context();

        manager.create_snapshot(&ctx);
        manager.create_snapshot(&ctx);
        for _ in 0..5 {
            manager.on_node_execute(&ctx, Uuid::new_v4());
        }

        manager.clear();

        assert_eq!(manager.snapshot_count(), 0);
        assert_eq!(manager.summary().nodes_processed, 0);
    }

    #[test]
    fn test_rollback_result_success() {
        let result = RollbackResult::success(Uuid::new_v4(), 3, 2);
        assert!(result.success);
        assert!(result.applied_snapshot_id.is_some());
        assert_eq!(result.nodes_removed, 3);
        assert_eq!(result.variables_changed, 2);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_rollback_result_failure() {
        let result = RollbackResult::failure("No snapshots available");
        assert!(!result.success);
        assert!(result.applied_snapshot_id.is_none());
        assert_eq!(result.error, Some("No snapshots available".to_string()));
    }
}
