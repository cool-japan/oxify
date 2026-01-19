//! Event bus for workflow triggers and event-driven execution
//!
//! Supports pub/sub pattern for workflow orchestration

use oxify_model::{NodeId, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Event type identifier
pub type EventType = String;

/// Well-known execution event types
pub mod execution_events {
    /// Workflow execution started
    pub const WORKFLOW_STARTED: &str = "workflow.started";
    /// Workflow execution completed successfully
    pub const WORKFLOW_COMPLETED: &str = "workflow.completed";
    /// Workflow execution failed
    pub const WORKFLOW_FAILED: &str = "workflow.failed";
    /// Workflow execution paused
    pub const WORKFLOW_PAUSED: &str = "workflow.paused";
    /// Workflow execution resumed
    pub const WORKFLOW_RESUMED: &str = "workflow.resumed";

    /// Node execution started
    pub const NODE_STARTED: &str = "node.started";
    /// Node execution completed successfully
    pub const NODE_COMPLETED: &str = "node.completed";
    /// Node execution failed
    pub const NODE_FAILED: &str = "node.failed";
    /// Node execution skipped (condition not met)
    pub const NODE_SKIPPED: &str = "node.skipped";
    /// Node retry attempt
    pub const NODE_RETRY: &str = "node.retry";

    /// Execution level started
    pub const LEVEL_STARTED: &str = "level.started";
    /// Execution level completed
    pub const LEVEL_COMPLETED: &str = "level.completed";

    /// Variable updated in context
    pub const VARIABLE_UPDATED: &str = "variable.updated";

    /// Checkpoint created
    pub const CHECKPOINT_CREATED: &str = "checkpoint.created";
    /// Checkpoint restored
    pub const CHECKPOINT_RESTORED: &str = "checkpoint.restored";

    /// Progress update (for streaming)
    pub const PROGRESS_UPDATE: &str = "progress.update";
}

/// Workflow event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEvent {
    /// Event ID
    pub id: Uuid,

    /// Event type (e.g., "workflow.started", "workflow.completed")
    pub event_type: EventType,

    /// Workflow ID
    pub workflow_id: Uuid,

    /// Execution ID (if applicable)
    pub execution_id: Option<Uuid>,

    /// Event payload
    pub payload: serde_json::Value,

    /// Timestamp
    pub timestamp: std::time::SystemTime,

    /// Source of the event
    pub source: String,
}

impl WorkflowEvent {
    /// Create a new workflow event
    pub fn new(event_type: EventType, workflow_id: Uuid, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            workflow_id,
            execution_id: None,
            payload,
            timestamp: std::time::SystemTime::now(),
            source: "oxify-engine".to_string(),
        }
    }

    /// Create event with execution ID
    pub fn with_execution(mut self, execution_id: Uuid) -> Self {
        self.execution_id = Some(execution_id);
        self
    }

    /// Create event with custom source
    pub fn with_source(mut self, source: String) -> Self {
        self.source = source;
        self
    }

    /// Create a workflow started event
    pub fn workflow_started(workflow_id: Uuid, execution_id: Uuid) -> Self {
        Self::new(
            execution_events::WORKFLOW_STARTED.to_string(),
            workflow_id,
            serde_json::json!({
                "status": "started",
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a workflow completed event
    pub fn workflow_completed(
        workflow_id: Uuid,
        execution_id: Uuid,
        node_count: usize,
        duration_ms: u128,
    ) -> Self {
        Self::new(
            execution_events::WORKFLOW_COMPLETED.to_string(),
            workflow_id,
            serde_json::json!({
                "status": "completed",
                "node_count": node_count,
                "duration_ms": duration_ms,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a workflow failed event
    pub fn workflow_failed(workflow_id: Uuid, execution_id: Uuid, error: &str) -> Self {
        Self::new(
            execution_events::WORKFLOW_FAILED.to_string(),
            workflow_id,
            serde_json::json!({
                "status": "failed",
                "error": error,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a workflow paused event
    pub fn workflow_paused(workflow_id: Uuid, execution_id: Uuid, reason: &str) -> Self {
        Self::new(
            execution_events::WORKFLOW_PAUSED.to_string(),
            workflow_id,
            serde_json::json!({
                "status": "paused",
                "reason": reason,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a workflow resumed event
    pub fn workflow_resumed(workflow_id: Uuid, execution_id: Uuid) -> Self {
        Self::new(
            execution_events::WORKFLOW_RESUMED.to_string(),
            workflow_id,
            serde_json::json!({
                "status": "resumed",
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a node started event
    pub fn node_started(
        workflow_id: Uuid,
        execution_id: Uuid,
        node_id: NodeId,
        node_name: &str,
    ) -> Self {
        Self::new(
            execution_events::NODE_STARTED.to_string(),
            workflow_id,
            serde_json::json!({
                "node_id": node_id.to_string(),
                "node_name": node_name,
                "status": "started",
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a node completed event
    pub fn node_completed(
        workflow_id: Uuid,
        execution_id: Uuid,
        node_id: NodeId,
        node_name: &str,
        duration_ms: u128,
    ) -> Self {
        Self::new(
            execution_events::NODE_COMPLETED.to_string(),
            workflow_id,
            serde_json::json!({
                "node_id": node_id.to_string(),
                "node_name": node_name,
                "status": "completed",
                "duration_ms": duration_ms,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a node failed event
    pub fn node_failed(
        workflow_id: Uuid,
        execution_id: Uuid,
        node_id: NodeId,
        node_name: &str,
        error: &str,
    ) -> Self {
        Self::new(
            execution_events::NODE_FAILED.to_string(),
            workflow_id,
            serde_json::json!({
                "node_id": node_id.to_string(),
                "node_name": node_name,
                "status": "failed",
                "error": error,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a node retry event
    pub fn node_retry(
        workflow_id: Uuid,
        execution_id: Uuid,
        node_id: NodeId,
        node_name: &str,
        attempt: u32,
        max_retries: u32,
    ) -> Self {
        Self::new(
            execution_events::NODE_RETRY.to_string(),
            workflow_id,
            serde_json::json!({
                "node_id": node_id.to_string(),
                "node_name": node_name,
                "attempt": attempt,
                "max_retries": max_retries,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a level started event
    pub fn level_started(
        workflow_id: Uuid,
        execution_id: Uuid,
        level: usize,
        node_count: usize,
    ) -> Self {
        Self::new(
            execution_events::LEVEL_STARTED.to_string(),
            workflow_id,
            serde_json::json!({
                "level": level,
                "node_count": node_count,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a level completed event
    pub fn level_completed(workflow_id: Uuid, execution_id: Uuid, level: usize) -> Self {
        Self::new(
            execution_events::LEVEL_COMPLETED.to_string(),
            workflow_id,
            serde_json::json!({
                "level": level,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a checkpoint created event
    pub fn checkpoint_created(
        workflow_id: Uuid,
        execution_id: Uuid,
        checkpoint_id: Uuid,
        level: usize,
    ) -> Self {
        Self::new(
            execution_events::CHECKPOINT_CREATED.to_string(),
            workflow_id,
            serde_json::json!({
                "checkpoint_id": checkpoint_id.to_string(),
                "level": level,
            }),
        )
        .with_execution(execution_id)
    }

    /// Create a progress update event
    pub fn progress_update(
        workflow_id: Uuid,
        execution_id: Uuid,
        completed_nodes: usize,
        total_nodes: usize,
        current_level: usize,
        total_levels: usize,
    ) -> Self {
        let percentage = if total_nodes > 0 {
            (completed_nodes as f32 / total_nodes as f32) * 100.0
        } else {
            0.0
        };

        Self::new(
            execution_events::PROGRESS_UPDATE.to_string(),
            workflow_id,
            serde_json::json!({
                "completed_nodes": completed_nodes,
                "total_nodes": total_nodes,
                "current_level": current_level,
                "total_levels": total_levels,
                "percentage": percentage,
            }),
        )
        .with_execution(execution_id)
    }
}

/// Event handler function
pub type EventHandler = Arc<dyn Fn(WorkflowEvent) + Send + Sync>;

/// Workflow trigger configuration
#[derive(Debug, Clone)]
pub struct WorkflowTrigger {
    /// Trigger ID
    pub id: Uuid,

    /// Event type to listen for
    pub event_type: EventType,

    /// Workflow to execute
    pub workflow: Workflow,

    /// Filter expression (JSON path or simple key-value match)
    pub filter: Option<String>,

    /// Enabled status
    pub enabled: bool,
}

impl WorkflowTrigger {
    /// Create a new workflow trigger
    pub fn new(event_type: EventType, workflow: Workflow) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            workflow,
            filter: None,
            enabled: true,
        }
    }

    /// Add a filter expression
    pub fn with_filter(mut self, filter: String) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Check if event matches trigger filter
    pub fn matches(&self, event: &WorkflowEvent) -> bool {
        if !self.enabled {
            return false;
        }

        // Check event type
        if event.event_type != self.event_type {
            return false;
        }

        // Check filter (simplified - would use JSON path in production)
        if let Some(filter) = &self.filter {
            // Simple key=value filter
            if let Some((key, value)) = filter.split_once('=') {
                if let Some(event_value) = event.payload.get(key) {
                    return event_value.as_str() == Some(value.trim());
                }
                return false;
            }
        }

        true
    }
}

/// Event bus for pub/sub messaging
pub struct EventBus {
    /// Broadcast channel for events
    sender: broadcast::Sender<WorkflowEvent>,

    /// Event handlers
    handlers: Arc<RwLock<HashMap<EventType, Vec<EventHandler>>>>,

    /// Workflow triggers
    triggers: Arc<RwLock<HashMap<Uuid, WorkflowTrigger>>>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);

        Self {
            sender,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            triggers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Publish an event
    pub async fn publish(&self, event: WorkflowEvent) -> Result<(), String> {
        // Store event type for later
        let event_type = event.event_type.clone();

        // Send to broadcast channel (ignore error if no receivers)
        let _ = self.sender.send(event.clone());

        // Call registered handlers
        let handlers = self.handlers.read().await;
        if let Some(event_handlers) = handlers.get(&event_type) {
            for handler in event_handlers {
                handler(event.clone());
            }
        }

        // Check triggers
        self.check_triggers(&event).await;

        tracing::debug!("Published event: {} ({})", event.id, event_type);

        Ok(())
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<WorkflowEvent> {
        self.sender.subscribe()
    }

    /// Register an event handler
    pub async fn on(&self, event_type: EventType, handler: EventHandler) {
        self.handlers
            .write()
            .await
            .entry(event_type.clone())
            .or_insert_with(Vec::new)
            .push(handler);

        tracing::info!("Registered handler for event type: {}", event_type);
    }

    /// Register a workflow trigger
    pub async fn register_trigger(&self, trigger: WorkflowTrigger) -> Uuid {
        let id = trigger.id;
        self.triggers.write().await.insert(id, trigger);
        tracing::info!("Registered workflow trigger: {}", id);
        id
    }

    /// Unregister a workflow trigger
    pub async fn unregister_trigger(&self, trigger_id: Uuid) -> bool {
        let removed = self.triggers.write().await.remove(&trigger_id).is_some();
        if removed {
            tracing::info!("Unregistered trigger: {}", trigger_id);
        }
        removed
    }

    /// List all triggers
    pub async fn list_triggers(&self) -> Vec<WorkflowTrigger> {
        self.triggers.read().await.values().cloned().collect()
    }

    /// Check triggers and execute matching workflows
    async fn check_triggers(&self, event: &WorkflowEvent) {
        let triggers = self.triggers.read().await;

        for trigger in triggers.values() {
            if trigger.matches(event) {
                tracing::info!("Trigger {} matched event {}", trigger.id, event.id);

                // In a real implementation, this would spawn workflow execution
                // For now, we just log it
            }
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Node, NodeKind, WorkflowMetadata};

    #[test]
    fn test_event_creation() {
        let event = WorkflowEvent::new(
            "test.event".to_string(),
            Uuid::new_v4(),
            serde_json::json!({"key": "value"}),
        );

        assert_eq!(event.event_type, "test.event");
        assert!(event.execution_id.is_none());
    }

    #[test]
    fn test_trigger_matching() {
        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![
                Node::new("Start".to_string(), NodeKind::Start),
                Node::new("End".to_string(), NodeKind::End),
            ],
            edges: vec![],
        };

        let trigger = WorkflowTrigger::new("user.created".to_string(), workflow);

        let matching_event = WorkflowEvent::new(
            "user.created".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        );

        let non_matching_event = WorkflowEvent::new(
            "user.deleted".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        );

        assert!(trigger.matches(&matching_event));
        assert!(!trigger.matches(&non_matching_event));
    }

    #[tokio::test]
    async fn test_event_bus_publish() {
        let bus = EventBus::new(10);

        let event = WorkflowEvent::new(
            "test.event".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        );

        let result = bus.publish(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_bus_subscribe() {
        let bus = EventBus::new(10);
        let mut rx = bus.subscribe();

        let event = WorkflowEvent::new(
            "test.event".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        );

        bus.publish(event.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, event.id);
    }
}
