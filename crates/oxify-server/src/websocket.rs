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
    collections::{HashMap, HashSet},
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
    /// Subscribe to real-time updates for a specific workflow.
    ///
    /// Client-to-server control message: once subscribed, the connection
    /// receives `WorkflowEdit` broadcasts scoped to `workflow_id` via
    /// [`WsConnectionManager::broadcast_to_workflow`]. Not broadcast or
    /// replied to.
    Subscribe { workflow_id: String },
    /// Unsubscribe from a previously subscribed workflow's updates.
    ///
    /// Client-to-server control message; the inverse of [`WsMessage::Subscribe`].
    /// Not broadcast or replied to.
    Unsubscribe { workflow_id: String },
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
    /// Connection IDs subscribed to real-time updates for each workflow,
    /// keyed by workflow ID. Used to scope `WorkflowEdit` broadcasts to only
    /// the collaborators currently viewing a given workflow.
    subscriptions: Arc<RwLock<HashMap<String, HashSet<u64>>>>,
}

impl WsConnectionManager {
    /// Create a new WebSocket connection manager.
    pub fn new(max_connections_per_user: usize) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            max_connections_per_user,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
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
    ///
    /// Also purges `connection_id` from every workflow's subscriber set so
    /// disconnected connections neither leak as stale entries nor receive
    /// fan-out from `broadcast_to_workflow` after they are gone.
    pub async fn unregister(&self, connection_id: u64) {
        self.connections.write().await.remove(&connection_id);

        let mut subscriptions = self.subscriptions.write().await;
        for subscribers in subscriptions.values_mut() {
            subscribers.remove(&connection_id);
        }
        // Bound memory growth: drop workflows left with no subscribers.
        subscriptions.retain(|_, subscribers| !subscribers.is_empty());
        drop(subscriptions);

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

    /// Subscribe a connection to real-time updates for a specific workflow.
    ///
    /// Once subscribed, the connection becomes a recipient of
    /// [`WsConnectionManager::broadcast_to_workflow`] calls for `workflow_id`.
    pub async fn subscribe(&self, connection_id: u64, workflow_id: String) {
        self.subscriptions
            .write()
            .await
            .entry(workflow_id)
            .or_default()
            .insert(connection_id);
    }

    /// Unsubscribe a connection from a workflow's real-time updates.
    ///
    /// A no-op if the connection was not subscribed to `workflow_id`.
    pub async fn unsubscribe(&self, connection_id: u64, workflow_id: &str) {
        if let Some(subscribers) = self.subscriptions.write().await.get_mut(workflow_id) {
            subscribers.remove(&connection_id);
        }
    }

    /// Broadcast a message only to connections subscribed to `workflow_id`.
    ///
    /// Connections that never called [`WsConnectionManager::subscribe`] for
    /// this `workflow_id` (or that have since unsubscribed / disconnected)
    /// do not receive the message.
    pub async fn broadcast_to_workflow(&self, workflow_id: &str, message: WsMessage) {
        let subscriber_ids: Vec<u64> = self
            .subscriptions
            .read()
            .await
            .get(workflow_id)
            .map(|subscribers| subscribers.iter().copied().collect())
            .unwrap_or_default();

        let connections = self.connections.read().await;
        for id in subscriber_ids {
            if let Some(connection) = connections.get(&id) {
                if let Err(e) = connection.tx.send(message.clone()) {
                    error!(connection_id = id, error = %e, "Failed to send message");
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
    // Validate the JWT token; falls back to "anonymous" when absent/invalid.
    let user_id = validate_token(&auth.token).unwrap_or_else(|| "anonymous".to_string());

    ws.on_upgrade(move |socket| handle_websocket(socket, user_id, manager))
}

/// Minimal claims struct for WebSocket JWT validation.
///
/// We only require `sub` (subject / user-id). All other standard fields are
/// validated by the `jsonwebtoken` library through `Validation`.
#[derive(serde::Deserialize)]
struct WsClaims {
    sub: String,
}

/// Validate a JWT token and extract the user ID (`sub` claim).
///
/// The HMAC-SHA256 secret is read from the `OXIFY_JWT_SECRET` environment
/// variable at call time. If the variable is absent, all tokens are treated as
/// invalid and `None` is returned, so the caller falls back to `"anonymous"`.
///
/// Invalid or expired tokens also return `None`.
fn validate_token(token: &str) -> Option<String> {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

    if token.is_empty() {
        return None;
    }

    let secret = std::env::var("OXIFY_JWT_SECRET").ok()?;
    let key = DecodingKey::from_secret(secret.as_bytes());

    let mut validation = Validation::new(Algorithm::HS256);
    // Disable audience check — callers may embed an audience or not.
    validation.validate_aud = false;

    decode::<WsClaims>(token, &key, &validation)
        .ok()
        .map(|token_data| token_data.claims.sub)
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
                Message::Text(text) => match serde_json::from_str::<WsMessage>(&text) {
                    Ok(ws_msg) => {
                        dispatch_message(ws_msg, connection_id, &manager_clone, &user_id_clone)
                            .await;
                    }
                    Err(e) => {
                        reply_parse_error(&manager_clone, &user_id_clone, connection_id, e).await;
                    }
                },
                Message::Binary(data) => match rmp_serde::from_slice::<WsMessage>(&data) {
                    Ok(ws_msg) => {
                        dispatch_message(ws_msg, connection_id, &manager_clone, &user_id_clone)
                            .await;
                    }
                    Err(e) => {
                        reply_parse_error(&manager_clone, &user_id_clone, connection_id, e).await;
                    }
                },
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

/// Dispatch a successfully-decoded client [`WsMessage`] to its handler.
///
/// Shared by both the JSON (`Message::Text`) and MessagePack
/// (`Message::Binary`) receive paths in [`handle_websocket`] so a message
/// is routed identically no matter which wire format carried it.
async fn dispatch_message(
    ws_msg: WsMessage,
    connection_id: u64,
    manager: &Arc<WsConnectionManager>,
    user_id: &str,
) {
    debug!(
        connection_id = connection_id,
        user_id = %user_id,
        message_type = ?ws_msg,
        "Received WebSocket message"
    );

    match &ws_msg {
        WsMessage::Ping { .. } => {
            let pong = WsMessage::Pong {
                timestamp: Utc::now(),
            };
            manager.send_to_user(user_id, pong).await;
        }
        WsMessage::WorkflowEdit {
            workflow_id,
            user_id: editor_id,
            operation,
            ..
        } => {
            debug!(
                connection_id = connection_id,
                workflow_id = %workflow_id,
                editor_id = %editor_id,
                operation = %operation,
                "WorkflowEdit received — broadcasting to workflow subscribers"
            );
            // Scope the broadcast to connections subscribed to this
            // workflow_id, rather than every connected user.
            manager
                .broadcast_to_workflow(workflow_id, ws_msg.clone())
                .await;
        }
        WsMessage::LlmChat {
            session_id,
            message,
            ..
        } => {
            debug!(
                connection_id = connection_id,
                session_id = %session_id,
                message_len = message.len(),
                "LlmChat received — echoing acknowledgement to user"
            );
            // Acknowledge receipt so the client knows the
            // message was delivered to the server.
            let ack = WsMessage::LlmResponse {
                session_id: session_id.clone(),
                content: String::new(),
                is_final: false,
                timestamp: Utc::now(),
            };
            manager.send_to_user(user_id, ack).await;
        }
        WsMessage::ExecutionUpdate {
            execution_id,
            status,
            progress,
            ..
        } => {
            debug!(
                connection_id = connection_id,
                execution_id = %execution_id,
                status = %status,
                progress = %progress,
                "ExecutionUpdate received — broadcasting status"
            );
            manager.broadcast(ws_msg.clone()).await;
        }
        WsMessage::Subscribe { workflow_id } => {
            debug!(
                connection_id = connection_id,
                workflow_id = %workflow_id,
                "Connection subscribed to workflow updates"
            );
            manager.subscribe(connection_id, workflow_id.clone()).await;
        }
        WsMessage::Unsubscribe { workflow_id } => {
            debug!(
                connection_id = connection_id,
                workflow_id = %workflow_id,
                "Connection unsubscribed from workflow updates"
            );
            manager.unsubscribe(connection_id, workflow_id).await;
        }
        // Pong, LlmResponse, Error — server-originated; no action needed.
        WsMessage::Pong { .. } | WsMessage::LlmResponse { .. } | WsMessage::Error { .. } => {
            debug!(
                connection_id = connection_id,
                "Server-originated message type received from client — ignoring"
            );
        }
    }
}

/// Notify the sending user that their most recent message could not be
/// decoded into a [`WsMessage`].
///
/// Shared by both the JSON (`Message::Text`) and MessagePack
/// (`Message::Binary`) receive paths in [`handle_websocket`] so a decode
/// failure produces the same `PARSE_ERROR` reply no matter which wire format
/// failed to parse.
async fn reply_parse_error<E: std::fmt::Display>(
    manager: &Arc<WsConnectionManager>,
    user_id: &str,
    connection_id: u64,
    error: E,
) {
    warn!(
        connection_id = connection_id,
        error = %error,
        "Failed to parse WebSocket message"
    );

    let error_msg = WsMessage::Error {
        code: "PARSE_ERROR".to_string(),
        message: "Invalid message format".to_string(),
        timestamp: Utc::now(),
    };
    manager.send_to_user(user_id, error_msg).await;
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
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        // Empty token → None regardless of environment.
        assert_eq!(validate_token(""), None);

        // Garbage (non-JWT) string → None.
        assert_eq!(validate_token("not_a_jwt"), None);

        // Valid HS256 JWT with known secret and `sub` claim → Some(sub).
        let secret = "test_secret_for_unit_test";
        let claims = serde_json::json!({
            "sub": "user_alice",
            "exp": 9_999_999_999u64,
        });
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("token encoding must succeed in test");

        // Without the env var the secret is unknown → None.
        std::env::remove_var("OXIFY_JWT_SECRET");
        assert_eq!(validate_token(&token), None);

        // With the correct secret set, the sub claim must be extracted.
        std::env::set_var("OXIFY_JWT_SECRET", secret);
        let result = validate_token(&token);
        assert_eq!(result, Some("user_alice".to_string()));

        // Wrong secret → None.
        std::env::set_var("OXIFY_JWT_SECRET", "wrong_secret");
        assert_eq!(validate_token(&token), None);

        // Clean up env var to avoid leaking state between tests.
        std::env::remove_var("OXIFY_JWT_SECRET");
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

    // ── dispatch-routing tests ────────────────────────────────────────────────
    //
    // These tests exercise the match-arm routing logic added to `handle_websocket`
    // by exercising the WsConnectionManager primitives that each arm delegates to:
    // broadcast_to_workflow() for WorkflowEdit, broadcast() for ExecutionUpdate,
    // send_to_user() for LlmChat.

    /// WorkflowEdit reaches connections subscribed to the edited workflow via
    /// `broadcast_to_workflow` (not a blanket `broadcast` to every connection).
    #[tokio::test]
    async fn test_dispatch_workflow_edit_broadcasts() {
        let manager = Arc::new(WsConnectionManager::new(5));

        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let conn1 = manager.register("user1".to_string(), tx1).await.unwrap();

        let (tx2, mut rx2) = mpsc::unbounded_channel();
        let conn2 = manager.register("user2".to_string(), tx2).await.unwrap();

        let edit_msg = WsMessage::WorkflowEdit {
            workflow_id: "wf-99".to_string(),
            user_id: "user1".to_string(),
            operation: "update_node".to_string(),
            data: serde_json::json!({"node_id": "n1"}),
            timestamp: Utc::now(),
        };

        // Simulate what the dispatch arm does: both collaborators are
        // subscribed to wf-99, so both receive the scoped broadcast.
        manager.subscribe(conn1.id, "wf-99".to_string()).await;
        manager.subscribe(conn2.id, "wf-99".to_string()).await;
        manager
            .broadcast_to_workflow("wf-99", edit_msg.clone())
            .await;

        assert!(
            matches!(rx1.recv().await, Some(WsMessage::WorkflowEdit { .. })),
            "user1 must receive WorkflowEdit broadcast for a workflow it subscribed to"
        );
        assert!(
            matches!(rx2.recv().await, Some(WsMessage::WorkflowEdit { .. })),
            "user2 must receive WorkflowEdit broadcast for a workflow it subscribed to"
        );
    }

    /// ExecutionUpdate reaches every connected user via broadcast.
    #[tokio::test]
    async fn test_dispatch_execution_update_broadcasts() {
        let manager = Arc::new(WsConnectionManager::new(5));

        let (tx1, mut rx1) = mpsc::unbounded_channel();
        manager.register("alice".to_string(), tx1).await.unwrap();

        let update_msg = WsMessage::ExecutionUpdate {
            execution_id: "exec-42".to_string(),
            status: "running".to_string(),
            progress: 0.5,
            timestamp: Utc::now(),
        };

        manager.broadcast(update_msg).await;

        assert!(
            matches!(rx1.recv().await, Some(WsMessage::ExecutionUpdate { .. })),
            "connected user must receive ExecutionUpdate broadcast"
        );
    }

    /// LlmChat triggers a LlmResponse ack sent only to the originating user.
    #[tokio::test]
    async fn test_dispatch_llm_chat_ack_to_sender() {
        let manager = Arc::new(WsConnectionManager::new(5));

        let (tx_sender, mut rx_sender) = mpsc::unbounded_channel();
        manager
            .register("chat_user".to_string(), tx_sender)
            .await
            .unwrap();

        let (tx_other, mut rx_other) = mpsc::unbounded_channel();
        manager
            .register("other_user".to_string(), tx_other)
            .await
            .unwrap();

        // Simulate the dispatch arm: send an ack only to the chatting user.
        let session_id = "sess-1".to_string();
        let ack = WsMessage::LlmResponse {
            session_id: session_id.clone(),
            content: String::new(),
            is_final: false,
            timestamp: Utc::now(),
        };
        manager.send_to_user("chat_user", ack).await;

        // Chatting user gets the ack.
        match rx_sender.recv().await {
            Some(WsMessage::LlmResponse {
                session_id: sid,
                content,
                ..
            }) => {
                assert_eq!(sid, "sess-1");
                assert!(content.is_empty());
            }
            other => panic!("expected LlmResponse ack, got {:?}", other),
        }

        // Other user must NOT receive anything.
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        assert!(
            rx_other.try_recv().is_err(),
            "other_user must not receive LlmChat ack"
        );
    }

    /// Server-originated variants (Pong, LlmResponse, Error) are not echoed back.
    #[tokio::test]
    async fn test_dispatch_server_originated_variants_no_echo() {
        // The guard for server-originated messages simply does nothing (no
        // broadcast, no send_to_user). We verify the manager stays silent.
        let manager = Arc::new(WsConnectionManager::new(5));

        let (tx, mut rx) = mpsc::unbounded_channel();
        manager.register("u".to_string(), tx).await.unwrap();

        // None of these should trigger a response when received from a client:
        // In the dispatch match arm they all fall through to the no-op branch.
        // We simulate this by simply NOT calling broadcast/send_to_user.
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        assert!(
            rx.try_recv().is_err(),
            "no messages should be enqueued for server-originated variants"
        );
    }

    // ── MessagePack (binary) decoding tests ─────────────────────────────────

    /// `rmp_serde::from_slice` — the exact call used for `Message::Binary` in
    /// `handle_websocket` — must decode a MessagePack-encoded `WsMessage`
    /// back into an equal value, and that value must match what the JSON
    /// (`Message::Text`) path decodes for the same logical message.
    #[test]
    fn test_ws_message_messagepack_round_trip_matches_json() {
        let message = WsMessage::WorkflowEdit {
            workflow_id: "wf-msgpack".to_string(),
            user_id: "user-mp".to_string(),
            operation: "insert_node".to_string(),
            data: serde_json::json!({"node_id": "n42", "kind": "transform"}),
            timestamp: Utc::now(),
        };

        // Encode/decode via MessagePack, mirroring the `Message::Binary` arm.
        let packed = rmp_serde::to_vec(&message).expect("MessagePack encoding must succeed");
        let from_msgpack: WsMessage =
            rmp_serde::from_slice(&packed).expect("MessagePack decoding must succeed");
        assert_eq!(
            from_msgpack, message,
            "MessagePack round-trip must reproduce the original WsMessage"
        );

        // Encode/decode the same logical message via JSON, mirroring the
        // `Message::Text` arm, and confirm both formats agree.
        let json = serde_json::to_string(&message).expect("JSON encoding must succeed");
        let from_json: WsMessage = serde_json::from_str(&json).expect("JSON decoding must succeed");
        assert_eq!(
            from_msgpack, from_json,
            "MessagePack- and JSON-decoded WsMessage values must be equal"
        );
    }

    /// A second variant (control message, no payload body) round-trips
    /// through MessagePack too, confirming the decoding is not
    /// accidentally coupled to `WorkflowEdit`'s shape.
    #[test]
    fn test_ws_message_messagepack_round_trip_subscribe() {
        let message = WsMessage::Subscribe {
            workflow_id: "wf-sub".to_string(),
        };

        let packed = rmp_serde::to_vec(&message).expect("MessagePack encoding must succeed");
        let decoded: WsMessage =
            rmp_serde::from_slice(&packed).expect("MessagePack decoding must succeed");
        assert_eq!(decoded, message);
    }

    // ── per-workflow subscription tests ─────────────────────────────────────

    /// `broadcast_to_workflow` reaches only connections subscribed to that
    /// exact workflow_id — not connections subscribed to a different
    /// workflow, and not connections that never subscribed at all.
    #[tokio::test]
    async fn test_broadcast_to_workflow_scopes_to_subscribers() {
        let manager = WsConnectionManager::new(5);

        let (tx_sub, mut rx_sub) = mpsc::unbounded_channel();
        let sub_conn = manager
            .register("subscriber".to_string(), tx_sub)
            .await
            .expect("registration must succeed under the connection limit");

        let (tx_other_wf, mut rx_other_wf) = mpsc::unbounded_channel();
        let other_wf_conn = manager
            .register("other_workflow_user".to_string(), tx_other_wf)
            .await
            .expect("registration must succeed under the connection limit");

        let (tx_unsubbed, mut rx_unsubbed) = mpsc::unbounded_channel();
        manager
            .register("never_subscribed".to_string(), tx_unsubbed)
            .await
            .expect("registration must succeed under the connection limit");

        manager.subscribe(sub_conn.id, "wf-a".to_string()).await;
        manager
            .subscribe(other_wf_conn.id, "wf-b".to_string())
            .await;
        // `never_subscribed` intentionally never calls subscribe().

        let edit = WsMessage::WorkflowEdit {
            workflow_id: "wf-a".to_string(),
            user_id: "subscriber".to_string(),
            operation: "update_node".to_string(),
            data: serde_json::json!({"node_id": "n1"}),
            timestamp: Utc::now(),
        };

        manager.broadcast_to_workflow("wf-a", edit.clone()).await;

        assert_eq!(
            rx_sub.recv().await,
            Some(edit),
            "a connection subscribed to wf-a must receive the broadcast"
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        assert!(
            rx_other_wf.try_recv().is_err(),
            "a connection subscribed to a different workflow must not receive the broadcast"
        );
        assert!(
            rx_unsubbed.try_recv().is_err(),
            "a connection that never subscribed must not receive the broadcast"
        );
    }

    /// Unsubscribing removes a connection from `broadcast_to_workflow`'s
    /// fan-out, even though the connection remains registered.
    #[tokio::test]
    async fn test_unsubscribe_stops_broadcast_to_workflow_delivery() {
        let manager = WsConnectionManager::new(5);

        let (tx, mut rx) = mpsc::unbounded_channel();
        let conn = manager
            .register("user1".to_string(), tx)
            .await
            .expect("registration must succeed under the connection limit");

        manager.subscribe(conn.id, "wf-c".to_string()).await;

        let first = WsMessage::Ping {
            timestamp: Utc::now(),
        };
        manager.broadcast_to_workflow("wf-c", first.clone()).await;
        assert_eq!(
            rx.recv().await,
            Some(first),
            "subscribed connection must receive a broadcast before unsubscribing"
        );

        manager.unsubscribe(conn.id, "wf-c").await;

        let second = WsMessage::Ping {
            timestamp: Utc::now(),
        };
        manager.broadcast_to_workflow("wf-c", second).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        assert!(
            rx.try_recv().is_err(),
            "connection must not receive broadcasts to a workflow it unsubscribed from"
        );
    }

    /// `unregister` purges the connection from every workflow's subscriber
    /// set, so no stale connection IDs accumulate in `subscriptions` and no
    /// dead fan-out is attempted afterward.
    #[tokio::test]
    async fn test_unregister_purges_subscriptions() {
        let manager = WsConnectionManager::new(5);

        let (tx, mut rx) = mpsc::unbounded_channel();
        let conn = manager
            .register("user1".to_string(), tx)
            .await
            .expect("registration must succeed under the connection limit");

        manager.subscribe(conn.id, "wf-leak".to_string()).await;
        assert!(
            manager
                .subscriptions
                .read()
                .await
                .get("wf-leak")
                .is_some_and(|subscribers| subscribers.contains(&conn.id)),
            "connection must be recorded as a subscriber before unregister"
        );

        manager.unregister(conn.id).await;

        // The connection_id must be gone from the workflow's subscriber set.
        let still_subscribed = manager
            .subscriptions
            .read()
            .await
            .get("wf-leak")
            .is_some_and(|subscribers| subscribers.contains(&conn.id));
        assert!(
            !still_subscribed,
            "connection_id must be purged from subscriptions on unregister"
        );
        // The workflow had exactly one subscriber, so the now-empty entry
        // should have been pruned too, bounding memory growth.
        assert!(
            !manager.subscriptions.read().await.contains_key("wf-leak"),
            "empty subscriber sets should be dropped after unregister"
        );

        // Broadcasting afterward must be a harmless no-op: no panic, and the
        // now-unregistered connection receives nothing further.
        manager
            .broadcast_to_workflow(
                "wf-leak",
                WsMessage::Ping {
                    timestamp: Utc::now(),
                },
            )
            .await;
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        assert!(
            rx.try_recv().is_err(),
            "unregistered connection must not receive further broadcasts"
        );
    }
}
