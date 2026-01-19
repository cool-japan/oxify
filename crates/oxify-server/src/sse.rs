//! Server-Sent Events (SSE) implementation for real-time workflow updates.
//!
//! Provides streaming updates for workflow execution progress with automatic
//! reconnection support, heartbeat messages, and connection management.

use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Response, Sse,
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tracing::{debug, warn};

/// SSE event types for workflow execution updates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseEventType {
    /// Node execution started
    NodeStarted {
        node_id: String,
        timestamp: DateTime<Utc>,
    },
    /// Node execution completed successfully
    NodeCompleted {
        node_id: String,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Workflow execution failed
    WorkflowFailed {
        execution_id: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
    /// Heartbeat to keep connection alive
    Heartbeat { timestamp: DateTime<Utc> },
}

/// SSE connection metadata
#[derive(Debug, Clone)]
pub struct SseConnection {
    /// Connection ID
    pub id: u64,
    /// User ID
    pub user_id: String,
    /// Execution ID being streamed
    pub execution_id: String,
    /// Connection start time
    pub connected_at: DateTime<Utc>,
}

/// SSE connection manager for tracking active connections.
pub struct SseConnectionManager {
    /// Active connections indexed by connection ID
    connections: Arc<RwLock<HashMap<u64, SseConnection>>>,
    /// Next connection ID (atomic counter)
    next_id: AtomicU64,
    /// Max connections per user
    max_connections_per_user: usize,
}

impl SseConnectionManager {
    /// Create a new SSE connection manager.
    pub fn new(max_connections_per_user: usize) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            max_connections_per_user,
        }
    }

    /// Register a new SSE connection.
    ///
    /// Returns `None` if the user has reached the max connections limit.
    pub async fn register(&self, user_id: String, execution_id: String) -> Option<SseConnection> {
        let connections = self.connections.read().await;
        let user_connections = connections
            .values()
            .filter(|c| c.user_id == user_id)
            .count();

        if user_connections >= self.max_connections_per_user {
            warn!(
                user_id = %user_id,
                current = user_connections,
                max = self.max_connections_per_user,
                "User reached max SSE connections"
            );
            return None;
        }

        drop(connections);

        let connection = SseConnection {
            id: self.next_id.fetch_add(1, Ordering::SeqCst),
            user_id,
            execution_id,
            connected_at: Utc::now(),
        };

        self.connections
            .write()
            .await
            .insert(connection.id, connection.clone());

        debug!(connection_id = connection.id, "SSE connection registered");

        Some(connection)
    }

    /// Unregister an SSE connection.
    pub async fn unregister(&self, connection_id: u64) {
        self.connections.write().await.remove(&connection_id);
        debug!(connection_id = connection_id, "SSE connection unregistered");
    }

    /// Get the number of active connections.
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Get connections for a specific user.
    pub async fn user_connections(&self, user_id: &str) -> Vec<SseConnection> {
        self.connections
            .read()
            .await
            .values()
            .filter(|c| c.user_id == user_id)
            .cloned()
            .collect()
    }
}

/// SSE event broadcaster for sending events to connected clients.
pub struct SseEventBroadcaster {
    /// Channel sender for broadcasting events
    tx: mpsc::Sender<SseEventType>,
}

impl SseEventBroadcaster {
    /// Create a new SSE event broadcaster.
    pub fn new(buffer_size: usize) -> (Self, mpsc::Receiver<SseEventType>) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (Self { tx }, rx)
    }

    /// Broadcast an event to all connected clients.
    pub async fn broadcast(
        &self,
        event: SseEventType,
    ) -> Result<(), mpsc::error::SendError<SseEventType>> {
        self.tx.send(event).await
    }

    /// Send node started event.
    pub async fn node_started(
        &self,
        node_id: String,
    ) -> Result<(), mpsc::error::SendError<SseEventType>> {
        self.broadcast(SseEventType::NodeStarted {
            node_id,
            timestamp: Utc::now(),
        })
        .await
    }

    /// Send node completed event.
    pub async fn node_completed(
        &self,
        node_id: String,
        duration_ms: u64,
    ) -> Result<(), mpsc::error::SendError<SseEventType>> {
        self.broadcast(SseEventType::NodeCompleted {
            node_id,
            duration_ms,
            timestamp: Utc::now(),
        })
        .await
    }

    /// Send workflow failed event.
    pub async fn workflow_failed(
        &self,
        execution_id: String,
        error: String,
    ) -> Result<(), mpsc::error::SendError<SseEventType>> {
        self.broadcast(SseEventType::WorkflowFailed {
            execution_id,
            error,
            timestamp: Utc::now(),
        })
        .await
    }

    /// Send heartbeat event.
    pub async fn heartbeat(&self) -> Result<(), mpsc::error::SendError<SseEventType>> {
        self.broadcast(SseEventType::Heartbeat {
            timestamp: Utc::now(),
        })
        .await
    }
}

/// Create an SSE stream for a workflow execution.
///
/// # Arguments
/// * `rx` - Receiver for SSE events
/// * `heartbeat_interval` - Interval for sending heartbeat messages (default: 30s)
///
/// # Returns
/// An SSE response with automatic heartbeat messages.
pub fn create_sse_stream(
    rx: mpsc::Receiver<SseEventType>,
    heartbeat_interval: Option<Duration>,
) -> Response {
    let stream = ReceiverStream::new(rx).map(|event| {
        let event_name = match event {
            SseEventType::NodeStarted { .. } => "node_started",
            SseEventType::NodeCompleted { .. } => "node_completed",
            SseEventType::WorkflowFailed { .. } => "workflow_failed",
            SseEventType::Heartbeat { .. } => "heartbeat",
        };

        let data = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());

        Ok::<Event, Infallible>(Event::default().event(event_name).data(data))
    });

    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(heartbeat_interval.unwrap_or(Duration::from_secs(30)))
            .text("keepalive"),
    );

    sse.into_response()
}

/// SSE handler response for errors.
pub fn sse_error_response(status: StatusCode, message: &str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(message.to_string()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_sse_connection_manager_register() {
        let manager = SseConnectionManager::new(5);

        let conn = manager
            .register("user1".to_string(), "exec1".to_string())
            .await;

        assert!(conn.is_some());
        let conn = conn.unwrap();
        assert_eq!(conn.user_id, "user1");
        assert_eq!(conn.execution_id, "exec1");
        assert_eq!(manager.connection_count().await, 1);
    }

    #[tokio::test]
    async fn test_sse_connection_manager_max_connections() {
        let manager = SseConnectionManager::new(2);

        // Register 2 connections (should succeed)
        let conn1 = manager
            .register("user1".to_string(), "exec1".to_string())
            .await;
        assert!(conn1.is_some());

        let conn2 = manager
            .register("user1".to_string(), "exec2".to_string())
            .await;
        assert!(conn2.is_some());

        // Try to register 3rd connection (should fail)
        let conn3 = manager
            .register("user1".to_string(), "exec3".to_string())
            .await;
        assert!(conn3.is_none());

        assert_eq!(manager.connection_count().await, 2);
    }

    #[tokio::test]
    async fn test_sse_connection_manager_unregister() {
        let manager = SseConnectionManager::new(5);

        let conn = manager
            .register("user1".to_string(), "exec1".to_string())
            .await
            .unwrap();

        assert_eq!(manager.connection_count().await, 1);

        manager.unregister(conn.id).await;
        assert_eq!(manager.connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_sse_connection_manager_user_connections() {
        let manager = SseConnectionManager::new(5);

        manager
            .register("user1".to_string(), "exec1".to_string())
            .await
            .unwrap();
        manager
            .register("user1".to_string(), "exec2".to_string())
            .await
            .unwrap();
        manager
            .register("user2".to_string(), "exec3".to_string())
            .await
            .unwrap();

        let user1_conns = manager.user_connections("user1").await;
        assert_eq!(user1_conns.len(), 2);

        let user2_conns = manager.user_connections("user2").await;
        assert_eq!(user2_conns.len(), 1);
    }

    #[tokio::test]
    async fn test_sse_event_broadcaster_node_started() {
        let (broadcaster, mut rx) = SseEventBroadcaster::new(10);

        broadcaster.node_started("node1".to_string()).await.unwrap();

        let event = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            SseEventType::NodeStarted { node_id, .. } => {
                assert_eq!(node_id, "node1");
            }
            _ => panic!("Expected NodeStarted event"),
        }
    }

    #[tokio::test]
    async fn test_sse_event_broadcaster_node_completed() {
        let (broadcaster, mut rx) = SseEventBroadcaster::new(10);

        broadcaster
            .node_completed("node1".to_string(), 150)
            .await
            .unwrap();

        let event = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            SseEventType::NodeCompleted {
                node_id,
                duration_ms,
                ..
            } => {
                assert_eq!(node_id, "node1");
                assert_eq!(duration_ms, 150);
            }
            _ => panic!("Expected NodeCompleted event"),
        }
    }

    #[tokio::test]
    async fn test_sse_event_broadcaster_workflow_failed() {
        let (broadcaster, mut rx) = SseEventBroadcaster::new(10);

        broadcaster
            .workflow_failed("exec1".to_string(), "Test error".to_string())
            .await
            .unwrap();

        let event = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            SseEventType::WorkflowFailed {
                execution_id,
                error,
                ..
            } => {
                assert_eq!(execution_id, "exec1");
                assert_eq!(error, "Test error");
            }
            _ => panic!("Expected WorkflowFailed event"),
        }
    }

    #[tokio::test]
    async fn test_sse_event_broadcaster_heartbeat() {
        let (broadcaster, mut rx) = SseEventBroadcaster::new(10);

        broadcaster.heartbeat().await.unwrap();

        let event = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            SseEventType::Heartbeat { .. } => {
                // Success
            }
            _ => panic!("Expected Heartbeat event"),
        }
    }

    #[test]
    fn test_sse_event_type_serialization() {
        let event = SseEventType::NodeStarted {
            node_id: "node1".to_string(),
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("node_started"));
        assert!(json.contains("node1"));

        let deserialized: SseEventType = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[tokio::test]
    async fn test_sse_event_buffering() {
        let (broadcaster, mut rx) = SseEventBroadcaster::new(3);

        // Send 3 events (should not block)
        broadcaster.node_started("node1".to_string()).await.unwrap();
        broadcaster.node_started("node2".to_string()).await.unwrap();
        broadcaster.node_started("node3".to_string()).await.unwrap();

        // Receive all 3 events
        for i in 1..=3 {
            let event = timeout(Duration::from_millis(100), rx.recv())
                .await
                .unwrap()
                .unwrap();

            match event {
                SseEventType::NodeStarted { node_id, .. } => {
                    assert_eq!(node_id, format!("node{}", i));
                }
                _ => panic!("Expected NodeStarted event"),
            }
        }
    }
}
