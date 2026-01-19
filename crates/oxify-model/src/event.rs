//! Execution event tracking for workflows
//!
//! This module provides detailed event tracking for workflow execution,
//! enabling debugging, monitoring, and audit trails.

use crate::{ExecutionResult, NodeId, NodeKind, NodeMetrics, WorkflowId, WorkflowMetadata};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Unique identifier for an execution event
pub type EventId = uuid::Uuid;

/// Unique identifier for a workflow execution
pub type ExecutionId = uuid::Uuid;

/// Execution event tracking node-level and workflow-level activities
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExecutionEvent {
    /// Unique event identifier
    #[cfg_attr(feature = "openapi", schema(value_type = uuid::Uuid))]
    pub id: EventId,
    /// Execution this event belongs to
    #[cfg_attr(feature = "openapi", schema(value_type = uuid::Uuid))]
    pub execution_id: ExecutionId,
    /// Workflow being executed
    #[cfg_attr(feature = "openapi", schema(value_type = uuid::Uuid))]
    pub workflow_id: WorkflowId,
    /// Node associated with this event (if applicable)
    #[cfg_attr(feature = "openapi", schema(value_type = Option<uuid::Uuid>))]
    pub node_id: Option<NodeId>,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event type classification
    pub event_type: EventType,
    /// Detailed event information
    pub details: EventDetails,
}

/// Event type classification for filtering and querying
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum EventType {
    /// Workflow execution started
    WorkflowStarted,
    /// Workflow execution completed successfully
    WorkflowCompleted,
    /// Workflow execution failed
    WorkflowFailed,
    /// Workflow execution cancelled
    WorkflowCancelled,
    /// Node execution started
    NodeStarted,
    /// Node execution completed successfully
    NodeCompleted,
    /// Node execution failed
    NodeFailed,
    /// Node execution skipped
    NodeSkipped,
    /// Variable value changed
    VariableChanged,
    /// Error occurred during execution
    ErrorOccurred,
    /// Checkpoint created
    CheckpointCreated,
    /// Execution resumed from checkpoint
    ExecutionResumed,
}

/// Detailed event information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(tag = "type")]
pub enum EventDetails {
    /// Workflow started event
    WorkflowStarted {
        /// Workflow metadata
        metadata: WorkflowMetadata,
        /// Input parameters
        #[serde(default)]
        input: HashMap<String, Value>,
    },
    /// Workflow completed successfully
    WorkflowCompleted {
        /// Total execution duration in milliseconds
        duration_ms: u64,
        /// Final execution result
        result: ExecutionResult,
    },
    /// Workflow failed
    WorkflowFailed {
        /// Error message
        error: String,
        /// Total execution duration in milliseconds
        duration_ms: u64,
        /// Stack trace if available
        #[serde(skip_serializing_if = "Option::is_none")]
        stack_trace: Option<String>,
    },
    /// Workflow cancelled by user
    WorkflowCancelled {
        /// Reason for cancellation
        reason: String,
        /// Partial execution duration
        duration_ms: u64,
    },
    /// Node started execution
    NodeStarted {
        /// Type of node being executed
        node_kind: NodeKind,
        /// Node input data
        #[serde(default)]
        input: HashMap<String, Value>,
    },
    /// Node completed successfully
    NodeCompleted {
        /// Type of node executed
        node_kind: NodeKind,
        /// Node execution duration in milliseconds
        duration_ms: u64,
        /// Performance metrics
        metrics: NodeMetrics,
        /// Node output data
        #[serde(default)]
        output: HashMap<String, Value>,
    },
    /// Node failed
    NodeFailed {
        /// Type of node that failed
        node_kind: NodeKind,
        /// Error message
        error: String,
        /// Stack trace if available
        #[serde(skip_serializing_if = "Option::is_none")]
        stack_trace: Option<String>,
        /// Retry attempt number (0 for first attempt)
        retry_attempt: u32,
    },
    /// Node skipped (e.g., due to conditional logic)
    NodeSkipped {
        /// Type of node skipped
        node_kind: NodeKind,
        /// Reason for skipping
        reason: String,
    },
    /// Variable changed
    VariableChanged {
        /// Variable name
        variable_name: String,
        /// Previous value (None if newly created)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_value: Option<Value>,
        /// New value
        new_value: Value,
        /// Source of the change (node ID or "system")
        source: String,
    },
    /// Error occurred
    ErrorOccurred {
        /// Error message
        error: String,
        /// Stack trace if available
        #[serde(skip_serializing_if = "Option::is_none")]
        stack_trace: Option<String>,
        /// Additional error context
        #[serde(default)]
        context: HashMap<String, Value>,
    },
    /// Checkpoint created
    CheckpointCreated {
        /// Checkpoint identifier
        checkpoint_id: String,
        /// Number of nodes completed
        nodes_completed: usize,
        /// Current execution state
        state: String,
    },
    /// Execution resumed
    ExecutionResumed {
        /// Checkpoint used for resumption
        checkpoint_id: String,
        /// Number of nodes to skip
        nodes_to_skip: usize,
    },
}

impl ExecutionEvent {
    /// Create a new workflow started event
    pub fn workflow_started(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        metadata: WorkflowMetadata,
        input: HashMap<String, Value>,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: None,
            timestamp: Utc::now(),
            event_type: EventType::WorkflowStarted,
            details: EventDetails::WorkflowStarted { metadata, input },
        }
    }

    /// Create a new workflow completed event
    pub fn workflow_completed(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        duration_ms: u64,
        result: ExecutionResult,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: None,
            timestamp: Utc::now(),
            event_type: EventType::WorkflowCompleted,
            details: EventDetails::WorkflowCompleted {
                duration_ms,
                result,
            },
        }
    }

    /// Create a new workflow failed event
    pub fn workflow_failed(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        duration_ms: u64,
        error: String,
        stack_trace: Option<String>,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: None,
            timestamp: Utc::now(),
            event_type: EventType::WorkflowFailed,
            details: EventDetails::WorkflowFailed {
                error,
                duration_ms,
                stack_trace,
            },
        }
    }

    /// Create a new workflow cancelled event
    pub fn workflow_cancelled(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        duration_ms: u64,
        reason: String,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: None,
            timestamp: Utc::now(),
            event_type: EventType::WorkflowCancelled,
            details: EventDetails::WorkflowCancelled {
                reason,
                duration_ms,
            },
        }
    }

    /// Create a new node started event
    pub fn node_started(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        node_id: NodeId,
        node_kind: NodeKind,
        input: HashMap<String, Value>,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: Some(node_id),
            timestamp: Utc::now(),
            event_type: EventType::NodeStarted,
            details: EventDetails::NodeStarted { node_kind, input },
        }
    }

    /// Create a new node completed event
    pub fn node_completed(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        node_id: NodeId,
        node_kind: NodeKind,
        duration_ms: u64,
        metrics: NodeMetrics,
        output: HashMap<String, Value>,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: Some(node_id),
            timestamp: Utc::now(),
            event_type: EventType::NodeCompleted,
            details: EventDetails::NodeCompleted {
                node_kind,
                duration_ms,
                metrics,
                output,
            },
        }
    }

    /// Create a new node failed event
    pub fn node_failed(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        node_id: NodeId,
        node_kind: NodeKind,
        error: String,
        stack_trace: Option<String>,
        retry_attempt: u32,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: Some(node_id),
            timestamp: Utc::now(),
            event_type: EventType::NodeFailed,
            details: EventDetails::NodeFailed {
                node_kind,
                error,
                stack_trace,
                retry_attempt,
            },
        }
    }

    /// Create a new node skipped event
    pub fn node_skipped(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        node_id: NodeId,
        node_kind: NodeKind,
        reason: String,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: Some(node_id),
            timestamp: Utc::now(),
            event_type: EventType::NodeSkipped,
            details: EventDetails::NodeSkipped { node_kind, reason },
        }
    }

    /// Create a new variable changed event
    pub fn variable_changed(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        node_id: Option<NodeId>,
        variable_name: String,
        old_value: Option<Value>,
        new_value: Value,
        source: String,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id,
            timestamp: Utc::now(),
            event_type: EventType::VariableChanged,
            details: EventDetails::VariableChanged {
                variable_name,
                old_value,
                new_value,
                source,
            },
        }
    }

    /// Create a new error occurred event
    pub fn error_occurred(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        node_id: Option<NodeId>,
        error: String,
        stack_trace: Option<String>,
        context: HashMap<String, Value>,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id,
            timestamp: Utc::now(),
            event_type: EventType::ErrorOccurred,
            details: EventDetails::ErrorOccurred {
                error,
                stack_trace,
                context,
            },
        }
    }

    /// Create a new checkpoint created event
    pub fn checkpoint_created(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        checkpoint_id: String,
        nodes_completed: usize,
        state: String,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: None,
            timestamp: Utc::now(),
            event_type: EventType::CheckpointCreated,
            details: EventDetails::CheckpointCreated {
                checkpoint_id,
                nodes_completed,
                state,
            },
        }
    }

    /// Create a new execution resumed event
    pub fn execution_resumed(
        execution_id: ExecutionId,
        workflow_id: WorkflowId,
        checkpoint_id: String,
        nodes_to_skip: usize,
    ) -> Self {
        Self {
            id: EventId::new_v4(),
            execution_id,
            workflow_id,
            node_id: None,
            timestamp: Utc::now(),
            event_type: EventType::ExecutionResumed,
            details: EventDetails::ExecutionResumed {
                checkpoint_id,
                nodes_to_skip,
            },
        }
    }
}

/// Event timeline for an execution
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EventTimeline {
    /// All events in chronological order
    pub events: Vec<ExecutionEvent>,
}

impl EventTimeline {
    /// Create a new empty timeline
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Add an event to the timeline
    pub fn push(&mut self, event: ExecutionEvent) {
        self.events.push(event);
    }

    /// Get events by type
    pub fn filter_by_type(&self, event_type: EventType) -> Vec<&ExecutionEvent> {
        self.events
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    /// Get events by node
    pub fn filter_by_node(&self, node_id: NodeId) -> Vec<&ExecutionEvent> {
        self.events
            .iter()
            .filter(|e| e.node_id == Some(node_id))
            .collect()
    }

    /// Get events in time range
    pub fn filter_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<&ExecutionEvent> {
        self.events
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect()
    }

    /// Get total execution duration in milliseconds
    pub fn total_duration_ms(&self) -> Option<u64> {
        let start = self.events.first()?.timestamp;
        let end = self.events.last()?.timestamp;
        Some((end - start).num_milliseconds() as u64)
    }

    /// Count events by type
    pub fn count_by_type(&self, event_type: EventType) -> usize {
        self.events
            .iter()
            .filter(|e| e.event_type == event_type)
            .count()
    }

    /// Get all error events
    pub fn errors(&self) -> Vec<&ExecutionEvent> {
        self.events
            .iter()
            .filter(|e| {
                matches!(
                    e.event_type,
                    EventType::NodeFailed | EventType::WorkflowFailed | EventType::ErrorOccurred
                )
            })
            .collect()
    }

    /// Check if execution was successful
    pub fn is_successful(&self) -> bool {
        self.events
            .iter()
            .any(|e| e.event_type == EventType::WorkflowCompleted)
    }

    /// Check if execution failed
    pub fn is_failed(&self) -> bool {
        self.events
            .iter()
            .any(|e| e.event_type == EventType::WorkflowFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_started_event() {
        let execution_id = ExecutionId::new_v4();
        let workflow_id = WorkflowId::new_v4();
        let metadata = WorkflowMetadata::new("test-workflow".to_string());

        let event = ExecutionEvent::workflow_started(
            execution_id,
            workflow_id,
            metadata.clone(),
            HashMap::new(),
        );

        assert_eq!(event.execution_id, execution_id);
        assert_eq!(event.workflow_id, workflow_id);
        assert_eq!(event.event_type, EventType::WorkflowStarted);
        assert!(event.node_id.is_none());
    }

    #[test]
    fn test_node_events() {
        let execution_id = ExecutionId::new_v4();
        let workflow_id = WorkflowId::new_v4();
        let node_id = NodeId::new_v4();

        let started = ExecutionEvent::node_started(
            execution_id,
            workflow_id,
            node_id,
            NodeKind::Start,
            HashMap::new(),
        );
        assert_eq!(started.event_type, EventType::NodeStarted);
        assert_eq!(started.node_id, Some(node_id));

        let metrics = NodeMetrics::default();
        let completed = ExecutionEvent::node_completed(
            execution_id,
            workflow_id,
            node_id,
            NodeKind::Start,
            100,
            metrics,
            HashMap::new(),
        );
        assert_eq!(completed.event_type, EventType::NodeCompleted);
    }

    #[test]
    fn test_event_timeline() {
        let mut timeline = EventTimeline::new();
        let execution_id = ExecutionId::new_v4();
        let workflow_id = WorkflowId::new_v4();

        let metadata = WorkflowMetadata::new("test".to_string());

        timeline.push(ExecutionEvent::workflow_started(
            execution_id,
            workflow_id,
            metadata,
            HashMap::new(),
        ));

        assert_eq!(timeline.events.len(), 1);
        assert_eq!(timeline.count_by_type(EventType::WorkflowStarted), 1);
    }

    #[test]
    fn test_timeline_filtering() {
        let mut timeline = EventTimeline::new();
        let execution_id = ExecutionId::new_v4();
        let workflow_id = WorkflowId::new_v4();
        let node_id = NodeId::new_v4();

        // Add various events
        timeline.push(ExecutionEvent::node_started(
            execution_id,
            workflow_id,
            node_id,
            NodeKind::Start,
            HashMap::new(),
        ));

        timeline.push(ExecutionEvent::node_failed(
            execution_id,
            workflow_id,
            node_id,
            NodeKind::Start,
            "Test error".to_string(),
            None,
            0,
        ));

        let node_events = timeline.filter_by_node(node_id);
        assert_eq!(node_events.len(), 2);

        let errors = timeline.errors();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_variable_changed_event() {
        let execution_id = ExecutionId::new_v4();
        let workflow_id = WorkflowId::new_v4();
        let node_id = NodeId::new_v4();

        let event = ExecutionEvent::variable_changed(
            execution_id,
            workflow_id,
            Some(node_id),
            "counter".to_string(),
            Some(Value::from(0)),
            Value::from(1),
            node_id.to_string(),
        );

        assert_eq!(event.event_type, EventType::VariableChanged);
        if let EventDetails::VariableChanged { variable_name, .. } = &event.details {
            assert_eq!(variable_name, "counter");
        } else {
            panic!("Expected VariableChanged event details");
        }
    }

    #[test]
    fn test_timeline_success_check() {
        let mut timeline = EventTimeline::new();
        let execution_id = ExecutionId::new_v4();
        let workflow_id = WorkflowId::new_v4();

        let metadata = WorkflowMetadata::new("test".to_string());

        timeline.push(ExecutionEvent::workflow_started(
            execution_id,
            workflow_id,
            metadata,
            HashMap::new(),
        ));

        assert!(!timeline.is_successful());
        assert!(!timeline.is_failed());

        let result = ExecutionResult::Success(Value::Null);
        timeline.push(ExecutionEvent::workflow_completed(
            execution_id,
            workflow_id,
            1000,
            result,
        ));

        assert!(timeline.is_successful());
        assert!(!timeline.is_failed());
    }
}
