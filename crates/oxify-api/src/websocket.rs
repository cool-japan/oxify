//! WebSocket support for real-time workflow execution updates
//!
//! This module provides bidirectional WebSocket communication for:
//! - Subscribing to execution updates
//! - Sending commands to running executions (pause, cancel, resume)
//! - Real-time notifications
//!
//! # Endpoints
//!
//! - `WS /api/v1/ws` - Main WebSocket endpoint
//!
//! # Protocol
//!
//! Messages are JSON-encoded with the following format:
//!
//! ## Client -> Server (Commands)
//! ```json
//! {"type": "subscribe", "execution_id": "uuid"}
//! {"type": "unsubscribe", "execution_id": "uuid"}
//! {"type": "pause", "execution_id": "uuid"}
//! {"type": "cancel", "execution_id": "uuid"}
//! {"type": "resume", "execution_id": "uuid"}
//! {"type": "ping"}
//! ```
//!
//! ## Server -> Client (Events)
//! ```json
//! {"type": "execution_update", "execution_id": "uuid", "state": "Running", "progress": {...}}
//! {"type": "node_complete", "execution_id": "uuid", "node_id": "uuid", "result": {...}}
//! {"type": "execution_complete", "execution_id": "uuid", "state": "Completed"}
//! {"type": "error", "message": "..."}
//! {"type": "pong"}
//! {"type": "subscribed", "execution_id": "uuid"}
//! {"type": "unsubscribed", "execution_id": "uuid"}
//! ```

use crate::handlers::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use oxify_model::ExecutionState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// WebSocket message types from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Subscribe to execution updates
    Subscribe { execution_id: Uuid },
    /// Unsubscribe from execution updates
    Unsubscribe { execution_id: Uuid },
    /// Pause an execution
    Pause { execution_id: Uuid },
    /// Cancel an execution
    Cancel { execution_id: Uuid },
    /// Resume a paused execution
    Resume { execution_id: Uuid },
    /// Ping to keep connection alive
    Ping,
}

/// WebSocket message types to client
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Execution state updated
    ExecutionUpdate {
        execution_id: Uuid,
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<ExecutionProgress>,
    },
    /// A node completed execution
    NodeComplete {
        execution_id: Uuid,
        node_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
    },
    /// Execution completed (success or failure)
    ExecutionComplete {
        execution_id: Uuid,
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Error message
    Error { message: String },
    /// Pong response
    Pong,
    /// Subscription confirmed
    Subscribed { execution_id: Uuid },
    /// Unsubscription confirmed
    Unsubscribed { execution_id: Uuid },
    /// Command acknowledged
    CommandAck {
        execution_id: Uuid,
        command: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Execution progress information
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionProgress {
    /// Number of nodes completed
    pub nodes_completed: usize,
    /// Total number of nodes
    pub nodes_total: usize,
    /// Current node being executed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node: Option<String>,
    /// Percentage complete (0-100)
    pub percent_complete: u8,
}

/// Broadcast channel for execution updates
pub type ExecutionBroadcast = broadcast::Sender<ServerMessage>;

/// WebSocket connection state management
#[derive(Clone)]
pub struct WebSocketState {
    /// Broadcast channels for each execution
    broadcasts: Arc<RwLock<HashMap<Uuid, ExecutionBroadcast>>>,
    /// Connected clients count per execution
    subscribers: Arc<RwLock<HashMap<Uuid, usize>>>,
}

impl Default for WebSocketState {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketState {
    /// Create a new WebSocket state manager
    pub fn new() -> Self {
        Self {
            broadcasts: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a broadcast channel for an execution
    pub async fn get_broadcast(&self, execution_id: Uuid) -> broadcast::Receiver<ServerMessage> {
        let mut broadcasts = self.broadcasts.write().await;

        if let Some(sender) = broadcasts.get(&execution_id) {
            sender.subscribe()
        } else {
            let (tx, rx) = broadcast::channel(100);
            broadcasts.insert(execution_id, tx);
            rx
        }
    }

    /// Send a message to all subscribers of an execution
    pub async fn broadcast(&self, execution_id: Uuid, message: ServerMessage) {
        let broadcasts = self.broadcasts.read().await;

        if let Some(sender) = broadcasts.get(&execution_id) {
            if let Err(e) = sender.send(message) {
                debug!("No subscribers for execution {}: {}", execution_id, e);
            }
        }
    }

    /// Increment subscriber count for an execution
    pub async fn add_subscriber(&self, execution_id: Uuid) {
        let mut subscribers = self.subscribers.write().await;
        *subscribers.entry(execution_id).or_insert(0) += 1;
    }

    /// Decrement subscriber count and cleanup if no subscribers
    pub async fn remove_subscriber(&self, execution_id: Uuid) {
        let mut subscribers = self.subscribers.write().await;
        if let Some(count) = subscribers.get_mut(&execution_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                subscribers.remove(&execution_id);
                // Also remove broadcast channel
                let mut broadcasts = self.broadcasts.write().await;
                broadcasts.remove(&execution_id);
            }
        }
    }

    /// Get current subscriber count for an execution
    #[allow(dead_code)]
    pub async fn subscriber_count(&self, execution_id: Uuid) -> usize {
        let subscribers = self.subscribers.read().await;
        subscribers.get(&execution_id).copied().unwrap_or(0)
    }
}

/// Global WebSocket state
static WS_STATE: std::sync::LazyLock<WebSocketState> =
    std::sync::LazyLock::new(WebSocketState::new);

/// Get the global WebSocket state
pub fn get_ws_state() -> &'static WebSocketState {
    &WS_STATE
}

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle a WebSocket connection
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let ws_state = get_ws_state();

    // Track subscriptions for this connection
    let subscriptions: Arc<RwLock<HashSet<Uuid>>> = Arc::new(RwLock::new(HashSet::new()));
    let subscriptions_clone = subscriptions.clone();

    // Spawn task to handle incoming messages
    let state_clone = state.clone();
    let incoming_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(msg) => {
                            let response =
                                handle_client_message(msg, &state_clone, &subscriptions_clone)
                                    .await;
                            if let Some(resp) = response {
                                // Response will be sent via the broadcast channel or directly
                                debug!("Handled message: {:?}", resp);
                            }
                        }
                        Err(e) => {
                            warn!("Invalid message format: {}", e);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed");
                    break;
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received ping: {:?}", data);
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(Message::Binary(_)) => {
                    warn!("Binary messages not supported");
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }
    });

    // Spawn task to forward broadcast messages to client
    let subscriptions_for_broadcast = subscriptions.clone();
    let broadcast_task = tokio::spawn(async move {
        loop {
            let subs = subscriptions_for_broadcast.read().await;
            if subs.is_empty() {
                drop(subs);
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            // Get receivers for all subscriptions
            let mut receivers = Vec::new();
            for &exec_id in subs.iter() {
                receivers.push((exec_id, ws_state.get_broadcast(exec_id).await));
            }
            drop(subs);

            // Process one message from any receiver
            for (_exec_id, mut rx) in receivers {
                if let Ok(msg) = rx.try_recv() {
                    let json = serde_json::to_string(&msg).unwrap_or_default();
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        return; // Connection closed
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = incoming_task => {
            debug!("Incoming task completed");
        }
        _ = broadcast_task => {
            debug!("Broadcast task completed");
        }
    }

    // Cleanup subscriptions
    let subs = subscriptions.read().await;
    for &exec_id in subs.iter() {
        ws_state.remove_subscriber(exec_id).await;
    }
}

/// Handle a client message and return optional response
async fn handle_client_message(
    msg: ClientMessage,
    state: &Arc<AppState>,
    subscriptions: &Arc<RwLock<HashSet<Uuid>>>,
) -> Option<ServerMessage> {
    let ws_state = get_ws_state();

    match msg {
        ClientMessage::Subscribe { execution_id } => {
            // Verify execution exists
            if state
                .execution_store
                .get(&execution_id)
                .await
                .ok()
                .flatten()
                .is_none()
            {
                return Some(ServerMessage::Error {
                    message: format!("Execution {} not found", execution_id),
                });
            }

            // Add subscription
            subscriptions.write().await.insert(execution_id);
            ws_state.add_subscriber(execution_id).await;

            Some(ServerMessage::Subscribed { execution_id })
        }

        ClientMessage::Unsubscribe { execution_id } => {
            subscriptions.write().await.remove(&execution_id);
            ws_state.remove_subscriber(execution_id).await;

            Some(ServerMessage::Unsubscribed { execution_id })
        }

        ClientMessage::Pause { execution_id } => {
            match state.execution_store.get(&execution_id).await {
                Ok(Some(mut execution)) => {
                    if matches!(execution.state, ExecutionState::Running) {
                        execution.state = ExecutionState::Paused;
                        if state
                            .execution_store
                            .update(&execution_id, execution)
                            .await
                            .is_ok()
                        {
                            // Broadcast the state change
                            ws_state
                                .broadcast(
                                    execution_id,
                                    ServerMessage::ExecutionUpdate {
                                        execution_id,
                                        state: "Paused".to_string(),
                                        progress: None,
                                    },
                                )
                                .await;

                            return Some(ServerMessage::CommandAck {
                                execution_id,
                                command: "pause".to_string(),
                                success: true,
                                message: None,
                            });
                        }
                    }
                    Some(ServerMessage::CommandAck {
                        execution_id,
                        command: "pause".to_string(),
                        success: false,
                        message: Some("Execution is not running".to_string()),
                    })
                }
                _ => Some(ServerMessage::Error {
                    message: format!("Execution {} not found", execution_id),
                }),
            }
        }

        ClientMessage::Cancel { execution_id } => {
            match state.execution_store.get(&execution_id).await {
                Ok(Some(mut execution)) => {
                    if matches!(
                        execution.state,
                        ExecutionState::Running | ExecutionState::Paused
                    ) {
                        execution.state = ExecutionState::Cancelled;
                        execution.completed_at = Some(Utc::now());
                        if state
                            .execution_store
                            .update(&execution_id, execution)
                            .await
                            .is_ok()
                        {
                            // Broadcast the state change
                            ws_state
                                .broadcast(
                                    execution_id,
                                    ServerMessage::ExecutionComplete {
                                        execution_id,
                                        state: "Cancelled".to_string(),
                                        error: None,
                                    },
                                )
                                .await;

                            return Some(ServerMessage::CommandAck {
                                execution_id,
                                command: "cancel".to_string(),
                                success: true,
                                message: None,
                            });
                        }
                    }
                    Some(ServerMessage::CommandAck {
                        execution_id,
                        command: "cancel".to_string(),
                        success: false,
                        message: Some("Execution cannot be cancelled".to_string()),
                    })
                }
                _ => Some(ServerMessage::Error {
                    message: format!("Execution {} not found", execution_id),
                }),
            }
        }

        ClientMessage::Resume { execution_id } => {
            match state.execution_store.get(&execution_id).await {
                Ok(Some(mut execution)) => {
                    if matches!(execution.state, ExecutionState::Paused) {
                        execution.state = ExecutionState::Running;
                        if state
                            .execution_store
                            .update(&execution_id, execution)
                            .await
                            .is_ok()
                        {
                            // Broadcast the state change
                            ws_state
                                .broadcast(
                                    execution_id,
                                    ServerMessage::ExecutionUpdate {
                                        execution_id,
                                        state: "Running".to_string(),
                                        progress: None,
                                    },
                                )
                                .await;

                            return Some(ServerMessage::CommandAck {
                                execution_id,
                                command: "resume".to_string(),
                                success: true,
                                message: None,
                            });
                        }
                    }
                    Some(ServerMessage::CommandAck {
                        execution_id,
                        command: "resume".to_string(),
                        success: false,
                        message: Some("Execution is not paused".to_string()),
                    })
                }
                _ => Some(ServerMessage::Error {
                    message: format!("Execution {} not found", execution_id),
                }),
            }
        }

        ClientMessage::Ping => Some(ServerMessage::Pong),
    }
}

/// Helper function to broadcast execution updates (for use by execution engine)
#[allow(dead_code)]
pub async fn broadcast_execution_update(
    execution_id: Uuid,
    state: &str,
    progress: Option<ExecutionProgress>,
) {
    get_ws_state()
        .broadcast(
            execution_id,
            ServerMessage::ExecutionUpdate {
                execution_id,
                state: state.to_string(),
                progress,
            },
        )
        .await;
}

/// Helper function to broadcast node completion (for use by execution engine)
#[allow(dead_code)]
pub async fn broadcast_node_complete(
    execution_id: Uuid,
    node_id: &str,
    result: Option<serde_json::Value>,
) {
    get_ws_state()
        .broadcast(
            execution_id,
            ServerMessage::NodeComplete {
                execution_id,
                node_id: node_id.to_string(),
                result,
            },
        )
        .await;
}

/// Helper function to broadcast execution completion (for use by execution engine)
#[allow(dead_code)]
pub async fn broadcast_execution_complete(execution_id: Uuid, state: &str, error: Option<String>) {
    get_ws_state()
        .broadcast(
            execution_id,
            ServerMessage::ExecutionComplete {
                execution_id,
                state: state.to_string(),
                error,
            },
        )
        .await;
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_deserialize_subscribe() {
        let json =
            r#"{"type": "subscribe", "execution_id": "550e8400-e29b-41d4-a716-446655440000"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Subscribe { execution_id } => {
                assert_eq!(
                    execution_id.to_string(),
                    "550e8400-e29b-41d4-a716-446655440000"
                );
            }
            _ => panic!("Expected Subscribe message"),
        }
    }

    #[test]
    fn test_client_message_deserialize_ping() {
        let json = r#"{"type": "ping"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Ping));
    }

    #[test]
    fn test_server_message_serialize_execution_update() {
        let msg = ServerMessage::ExecutionUpdate {
            execution_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            state: "Running".to_string(),
            progress: Some(ExecutionProgress {
                nodes_completed: 2,
                nodes_total: 5,
                current_node: Some("process".to_string()),
                percent_complete: 40,
            }),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("execution_update"));
        assert!(json.contains("Running"));
        assert!(json.contains("nodes_completed"));
    }

    #[test]
    fn test_server_message_serialize_error() {
        let msg = ServerMessage::Error {
            message: "Test error".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("Test error"));
    }

    #[test]
    fn test_server_message_serialize_pong() {
        let msg = ServerMessage::Pong;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("pong"));
    }

    #[test]
    fn test_execution_progress() {
        let progress = ExecutionProgress {
            nodes_completed: 3,
            nodes_total: 10,
            current_node: Some("transform".to_string()),
            percent_complete: 30,
        };

        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("nodes_completed"));
        assert!(json.contains("transform"));
        assert!(json.contains("30"));
    }

    #[tokio::test]
    async fn test_ws_state_new() {
        let state = WebSocketState::new();
        let exec_id = Uuid::new_v4();

        // Should start with 0 subscribers
        assert_eq!(state.subscriber_count(exec_id).await, 0);
    }

    #[tokio::test]
    async fn test_ws_state_add_remove_subscriber() {
        let state = WebSocketState::new();
        let exec_id = Uuid::new_v4();

        state.add_subscriber(exec_id).await;
        assert_eq!(state.subscriber_count(exec_id).await, 1);

        state.add_subscriber(exec_id).await;
        assert_eq!(state.subscriber_count(exec_id).await, 2);

        state.remove_subscriber(exec_id).await;
        assert_eq!(state.subscriber_count(exec_id).await, 1);

        state.remove_subscriber(exec_id).await;
        assert_eq!(state.subscriber_count(exec_id).await, 0);
    }

    #[tokio::test]
    async fn test_ws_state_broadcast() {
        let state = WebSocketState::new();
        let exec_id = Uuid::new_v4();

        // Get a receiver
        let mut rx = state.get_broadcast(exec_id).await;

        // Broadcast a message
        state.broadcast(exec_id, ServerMessage::Pong).await;

        // Should receive the message
        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg, ServerMessage::Pong));
    }

    #[test]
    fn test_command_ack_serialize() {
        let msg = ServerMessage::CommandAck {
            execution_id: Uuid::new_v4(),
            command: "pause".to_string(),
            success: true,
            message: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("command_ack"));
        assert!(json.contains("pause"));
        assert!(json.contains("true"));
    }
}
