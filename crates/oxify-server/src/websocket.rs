//! WebSocket implementation for bidirectional real-time communication.
//!
//! Provides WebSocket support for multi-user workflow editing, real-time chat
//! for LLM interactions, and live execution monitoring.

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use futures::{stream::StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// WebSocket message types for different use cases.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Workflow editing messages
    WorkflowEdit {
        workflow_id: String,
        user_id: String,
        operation: String,
        data: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
    /// LLM chat messages
    LlmChat {
        session_id: String,
        user_id: String,
        message: String,
        timestamp: DateTime<Utc>,
    },
    /// LLM response streaming
    LlmResponse {
        session_id: String,
        content: String,
        is_final: bool,
        timestamp: DateTime<Utc>,
    },
    /// Execution monitoring messages
    ExecutionUpdate {
        execution_id: String,
        status: String,
        progress: f32,
        timestamp: DateTime<Utc>,
    },
    /// Heartbeat/ping message
    Ping { timestamp: DateTime<Utc> },
    /// Pong response
    Pong { timestamp: DateTime<Utc> },
    /// Error message
    Error {
        code: String,
        message: String,
        timestamp: DateTime<Utc>,
    },
}

/// WebSocket authentication query parameters.
#[derive(Debug, Deserialize)]
pub struct WsAuthQuery {
    /// JWT authentication token
    pub token: String,
}

/// WebSocket connection metadata.
#[derive(Debug, Clone)]
pub struct WsConnection {
    /// Connection ID
    pub id: u64,
    /// User ID
    pub user_id: String,
    /// Connection start time
    pub connected_at: DateTime<Utc>,
    /// Channel sender for this connection
    pub tx: mpsc::UnboundedSender<WsMessage>,
}

/// WebSocket connection manager for tracking active connections.
pub struct WsConnectionManager {
    /// Active connections indexed by connection ID
    connections: Arc<RwLock<HashMap<u64, WsConnection>>>,
    /// Next connection ID (atomic counter)
    next_id: AtomicU64,
    /// Max connections per user
    max_connections_per_user: usize,
}

impl WsConnectionManager {
    /// Create a new WebSocket connection manager.
    pub fn new(max_connections_per_user: usize) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            max_connections_per_user,
        }
    }

    /// Register a new WebSocket connection.
    ///
    /// Returns `None` if the user has reached the max connections limit.
    pub async fn register(
        &self,
        user_id: String,
        tx: mpsc::UnboundedSender<WsMessage>,
    ) -> Option<WsConnection> {
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
                "User reached max WebSocket connections"
            );
            return None;
        }

        drop(connections);

        let connection = WsConnection {
            id: self.next_id.fetch_add(1, Ordering::SeqCst),
            user_id,
            connected_at: Utc::now(),
            tx,
        };

        self.connections
            .write()
            .await
            .insert(connection.id, connection.clone());

        info!(
            connection_id = connection.id,
            "WebSocket connection registered"
        );

        Some(connection)
    }

    /// Unregister a WebSocket connection.
    pub async fn unregister(&self, connection_id: u64) {
        self.connections.write().await.remove(&connection_id);
        info!(
            connection_id = connection_id,
            "WebSocket connection unregistered"
        );
    }

    /// Get the number of active connections.
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Get connections for a specific user.
    pub async fn user_connections(&self, user_id: &str) -> Vec<WsConnection> {
        self.connections
            .read()
            .await
            .values()
            .filter(|c| c.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Broadcast a message to all connections.
    pub async fn broadcast(&self, message: WsMessage) {
        let connections = self.connections.read().await;
        for connection in connections.values() {
            if let Err(e) = connection.tx.send(message.clone()) {
                error!(connection_id = connection.id, error = %e, "Failed to send message");
            }
        }
    }

    /// Send a message to a specific user's connections.
    pub async fn send_to_user(&self, user_id: &str, message: WsMessage) {
        let connections = self.connections.read().await;
        for connection in connections.values() {
            if connection.user_id == user_id {
                if let Err(e) = connection.tx.send(message.clone()) {
                    error!(
                        connection_id = connection.id,
                        user_id = %user_id,
                        error = %e,
                        "Failed to send message to user"
                    );
                }
            }
        }
    }
}

/// WebSocket upgrade handler with authentication.
///
/// # Example
/// ```no_run
/// use axum::{Router, routing::get};
/// use oxify_server::websocket::{ws_handler, WsConnectionManager};
/// use std::sync::Arc;
///
/// # #[tokio::main]
/// # async fn main() {
/// let manager = Arc::new(WsConnectionManager::new(10));
/// let app: Router<Arc<WsConnectionManager>> = Router::new()
///     .route("/ws", get(ws_handler))
///     .with_state(manager);
/// # }
/// ```
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(auth): Query<WsAuthQuery>,
    State(manager): State<Arc<WsConnectionManager>>,
) -> Response {
    // TODO: Validate JWT token and extract user_id
    // For now, use a placeholder user_id
    let user_id = validate_token(&auth.token).unwrap_or_else(|| "anonymous".to_string());

    ws.on_upgrade(move |socket| handle_websocket(socket, user_id, manager))
}

/// Validate JWT token and extract user ID.
///
/// This is a placeholder implementation. In production, this should:
/// - Parse the JWT token
/// - Validate the signature
/// - Check expiration
/// - Extract the user_id claim
fn validate_token(token: &str) -> Option<String> {
    // Placeholder: In production, use jsonwebtoken crate
    if token.is_empty() {
        None
    } else {
        Some(format!("user_{}", token))
    }
}

/// Handle WebSocket connection lifecycle.
async fn handle_websocket(socket: WebSocket, user_id: String, manager: Arc<WsConnectionManager>) {
    let (mut sender, mut receiver) = socket.split();

    // Create channel for outgoing messages
    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();

    // Register connection
    let connection = match manager.register(user_id.clone(), tx).await {
        Some(conn) => conn,
        None => {
            // Max connections reached
            let _ = sender
                .send(Message::Close(Some(CloseFrame {
                    code: axum::extract::ws::close_code::POLICY,
                    reason: "Max connections reached".into(),
                })))
                .await;
            return;
        }
    };

    let connection_id = connection.id;

    debug!(
        connection_id = connection_id,
        user_id = %user_id,
        "WebSocket connection established"
    );

    // Spawn task to send outgoing messages
    let mut send_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let json = match serde_json::to_string(&message) {
                Ok(j) => j,
                Err(e) => {
                    error!(error = %e, "Failed to serialize message");
                    continue;
                }
            };

            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Spawn task to receive incoming messages
    let manager_clone = manager.clone();
    let user_id_clone = user_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    match serde_json::from_str::<WsMessage>(&text) {
                        Ok(ws_msg) => {
                            debug!(
                                connection_id = connection_id,
                                user_id = %user_id_clone,
                                message_type = ?ws_msg,
                                "Received WebSocket message"
                            );

                            // Handle ping/pong
                            if matches!(ws_msg, WsMessage::Ping { .. }) {
                                let pong = WsMessage::Pong {
                                    timestamp: Utc::now(),
                                };
                                manager_clone.send_to_user(&user_id_clone, pong).await;
                            }

                            // TODO: Handle other message types (WorkflowEdit, LlmChat, etc.)
                        }
                        Err(e) => {
                            warn!(
                                connection_id = connection_id,
                                error = %e,
                                "Failed to parse WebSocket message"
                            );

                            let error_msg = WsMessage::Error {
                                code: "PARSE_ERROR".to_string(),
                                message: "Invalid message format".to_string(),
                                timestamp: Utc::now(),
                            };
                            manager_clone.send_to_user(&user_id_clone, error_msg).await;
                        }
                    }
                }
                Message::Binary(_) => {
                    // TODO: Support MessagePack for binary messages
                    warn!(
                        connection_id = connection_id,
                        "Binary messages not yet supported"
                    );
                }
                Message::Ping(_data) => {
                    // Axum handles Pong automatically, but we can log it
                    debug!(connection_id = connection_id, "Received ping");
                    // Send pong back manually if needed
                    if let Some(conn) = manager_clone.connections.read().await.get(&connection_id) {
                        let pong = WsMessage::Pong {
                            timestamp: Utc::now(),
                        };
                        let _ = conn.tx.send(pong);
                    }
                }
                Message::Pong(_) => {
                    debug!(connection_id = connection_id, "Received pong");
                }
                Message::Close(_) => {
                    debug!(connection_id = connection_id, "Received close frame");
                    break;
                }
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => {
            debug!(connection_id = connection_id, "Send task completed");
            recv_task.abort();
        }
        _ = &mut recv_task => {
            debug!(connection_id = connection_id, "Receive task completed");
            send_task.abort();
        }
    }

    // Unregister connection
    manager.unregister(connection_id).await;

    debug!(
        connection_id = connection_id,
        user_id = %user_id,
        "WebSocket connection closed"
    );
}

/// WebSocket error response helper.
pub fn ws_error_response(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ws_connection_manager_register() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let manager = WsConnectionManager::new(5);

        let conn = manager.register("user1".to_string(), tx).await;

        assert!(conn.is_some());
        let conn = conn.unwrap();
        assert_eq!(conn.user_id, "user1");
        assert_eq!(manager.connection_count().await, 1);
    }

    #[tokio::test]
    async fn test_ws_connection_manager_max_connections() {
        let manager = WsConnectionManager::new(2);

        // Register 2 connections (should succeed)
        let (tx1, _rx1) = mpsc::unbounded_channel();
        let conn1 = manager.register("user1".to_string(), tx1).await;
        assert!(conn1.is_some());

        let (tx2, _rx2) = mpsc::unbounded_channel();
        let conn2 = manager.register("user1".to_string(), tx2).await;
        assert!(conn2.is_some());

        // Try to register 3rd connection (should fail)
        let (tx3, _rx3) = mpsc::unbounded_channel();
        let conn3 = manager.register("user1".to_string(), tx3).await;
        assert!(conn3.is_none());

        assert_eq!(manager.connection_count().await, 2);
    }

    #[tokio::test]
    async fn test_ws_connection_manager_unregister() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let manager = WsConnectionManager::new(5);

        let conn = manager.register("user1".to_string(), tx).await.unwrap();

        assert_eq!(manager.connection_count().await, 1);

        manager.unregister(conn.id).await;
        assert_eq!(manager.connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_ws_connection_manager_user_connections() {
        let manager = WsConnectionManager::new(5);

        let (tx1, _rx1) = mpsc::unbounded_channel();
        manager.register("user1".to_string(), tx1).await.unwrap();

        let (tx2, _rx2) = mpsc::unbounded_channel();
        manager.register("user1".to_string(), tx2).await.unwrap();

        let (tx3, _rx3) = mpsc::unbounded_channel();
        manager.register("user2".to_string(), tx3).await.unwrap();

        let user1_conns = manager.user_connections("user1").await;
        assert_eq!(user1_conns.len(), 2);

        let user2_conns = manager.user_connections("user2").await;
        assert_eq!(user2_conns.len(), 1);
    }

    #[tokio::test]
    async fn test_ws_connection_manager_broadcast() {
        let manager = WsConnectionManager::new(5);

        let (tx1, mut rx1) = mpsc::unbounded_channel();
        manager.register("user1".to_string(), tx1).await.unwrap();

        let (tx2, mut rx2) = mpsc::unbounded_channel();
        manager.register("user2".to_string(), tx2).await.unwrap();

        let message = WsMessage::Ping {
            timestamp: Utc::now(),
        };

        manager.broadcast(message.clone()).await;

        // Both receivers should get the message
        assert_eq!(rx1.recv().await, Some(message.clone()));
        assert_eq!(rx2.recv().await, Some(message));
    }

    #[tokio::test]
    async fn test_ws_connection_manager_send_to_user() {
        let manager = WsConnectionManager::new(5);

        let (tx1, mut rx1) = mpsc::unbounded_channel();
        manager.register("user1".to_string(), tx1).await.unwrap();

        let (tx2, mut rx2) = mpsc::unbounded_channel();
        manager.register("user2".to_string(), tx2).await.unwrap();

        let message = WsMessage::Ping {
            timestamp: Utc::now(),
        };

        manager.send_to_user("user1", message.clone()).await;

        // Only user1 should receive the message
        assert_eq!(rx1.recv().await, Some(message));

        // user2 should not have any messages
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert!(rx2.try_recv().is_err());
    }

    #[test]
    fn test_ws_message_serialization() {
        let message = WsMessage::WorkflowEdit {
            workflow_id: "wf1".to_string(),
            user_id: "user1".to_string(),
            operation: "update".to_string(),
            data: serde_json::json!({"key": "value"}),
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("workflow_edit"));
        assert!(json.contains("wf1"));

        let deserialized: WsMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(message, deserialized);
    }

    #[test]
    fn test_validate_token() {
        // Empty token should return None
        assert_eq!(validate_token(""), None);

        // Non-empty token should return Some
        let result = validate_token("test_token");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "user_test_token");
    }

    #[tokio::test]
    async fn test_ws_message_ping_pong() {
        let ping = WsMessage::Ping {
            timestamp: Utc::now(),
        };
        let pong = WsMessage::Pong {
            timestamp: Utc::now(),
        };

        assert!(matches!(ping, WsMessage::Ping { .. }));
        assert!(matches!(pong, WsMessage::Pong { .. }));
    }

    #[tokio::test]
    async fn test_ws_message_error() {
        let error = WsMessage::Error {
            code: "TEST_ERROR".to_string(),
            message: "Test error message".to_string(),
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("TEST_ERROR"));
        assert!(json.contains("Test error message"));
    }
}
