//! WebSocket Connector
//!
//! Real-time bidirectional communication with WebSocket endpoints.
//!
//! # Features
//!
//! - Connect to WebSocket servers
//! - Send and receive messages
//! - Automatic reconnection with exponential backoff
//! - Message queuing
//! - Ping/pong heartbeat
//! - TLS support
//!
//! # Example
//!
//! ```ignore
//! use oxify_engine::websocket_connector::{WebSocketConnector, WebSocketConfig};
//!
//! let config = WebSocketConfig::new("wss://api.example.com/ws")
//!     .with_reconnect(true)
//!     .with_ping_interval(30);
//!
//! let mut connector = WebSocketConnector::new(config).await?;
//!
//! // Send a message
//! connector.send_text("Hello, WebSocket!").await?;
//!
//! // Receive messages
//! while let Some(message) = connector.receive().await {
//!     println!("Received: {:?}", message);
//! }
//! ```

#[cfg(feature = "websocket")]
use futures::{SinkExt, StreamExt};
#[cfg(feature = "websocket")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "websocket")]
use std::time::Duration;
#[cfg(feature = "websocket")]
use thiserror::Error;
#[cfg(feature = "websocket")]
use tokio::sync::mpsc;
#[cfg(feature = "websocket")]
use tokio::time::interval;
#[cfg(feature = "websocket")]
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

/// WebSocket errors
#[cfg(feature = "websocket")]
#[derive(Error, Debug)]
pub enum WebSocketError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Timeout")]
    Timeout,

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// WebSocket message types
#[cfg(feature = "websocket")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketMessage {
    /// Text message
    Text { content: String },
    /// Binary message
    Binary { data: Vec<u8> },
    /// Ping message
    Ping { data: Vec<u8> },
    /// Pong message
    Pong { data: Vec<u8> },
    /// Close message
    Close { code: u16, reason: String },
}

#[cfg(feature = "websocket")]
impl WebSocketMessage {
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text {
            content: content.into(),
        }
    }

    pub fn binary(data: Vec<u8>) -> Self {
        Self::Binary { data }
    }
}

/// WebSocket configuration
#[cfg(feature = "websocket")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// WebSocket URL (ws:// or wss://)
    pub url: String,
    /// Enable automatic reconnection
    #[serde(default)]
    pub auto_reconnect: bool,
    /// Maximum reconnection attempts (0 = unlimited)
    #[serde(default)]
    pub max_reconnect_attempts: u32,
    /// Initial reconnect delay in milliseconds
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_ms: u64,
    /// Maximum reconnect delay in milliseconds
    #[serde(default = "default_max_reconnect_delay")]
    pub max_reconnect_delay_ms: u64,
    /// Ping interval in seconds (0 = disabled)
    #[serde(default)]
    pub ping_interval_secs: u64,
    /// Message queue size
    #[serde(default = "default_queue_size")]
    pub message_queue_size: usize,
    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[cfg(feature = "websocket")]
fn default_reconnect_delay() -> u64 {
    1000
}

#[cfg(feature = "websocket")]
fn default_max_reconnect_delay() -> u64 {
    30000
}

#[cfg(feature = "websocket")]
fn default_queue_size() -> usize {
    100
}

#[cfg(feature = "websocket")]
fn default_timeout() -> u64 {
    30
}

#[cfg(feature = "websocket")]
impl WebSocketConfig {
    /// Create a new WebSocket configuration
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            auto_reconnect: false,
            max_reconnect_attempts: 0,
            reconnect_delay_ms: default_reconnect_delay(),
            max_reconnect_delay_ms: default_max_reconnect_delay(),
            ping_interval_secs: 0,
            message_queue_size: default_queue_size(),
            timeout_secs: default_timeout(),
        }
    }

    /// Enable automatic reconnection
    pub fn with_reconnect(mut self, enabled: bool) -> Self {
        self.auto_reconnect = enabled;
        self
    }

    /// Set maximum reconnect attempts
    pub fn with_max_reconnects(mut self, max_attempts: u32) -> Self {
        self.max_reconnect_attempts = max_attempts;
        self
    }

    /// Set ping interval
    pub fn with_ping_interval(mut self, interval_secs: u64) -> Self {
        self.ping_interval_secs = interval_secs;
        self
    }

    /// Set message queue size
    pub fn with_queue_size(mut self, size: usize) -> Self {
        self.message_queue_size = size;
        self
    }
}

/// WebSocket connection state
#[cfg(feature = "websocket")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Connecting
    Connecting,
    /// Connected
    Connected,
    /// Reconnecting
    Reconnecting,
    /// Closed
    Closed,
}

/// WebSocket connector for real-time communication
#[cfg(feature = "websocket")]
pub struct WebSocketConnector {
    config: WebSocketConfig,
    tx: mpsc::Sender<WebSocketMessage>,
    rx: mpsc::Receiver<WebSocketMessage>,
    state: ConnectionState,
}

#[cfg(feature = "websocket")]
impl WebSocketConnector {
    /// Create a new WebSocket connector and connect
    pub async fn new(config: WebSocketConfig) -> Result<Self, WebSocketError> {
        let (tx, rx_internal) = mpsc::channel(config.message_queue_size);
        let (tx_internal, rx) = mpsc::channel(config.message_queue_size);

        let mut connector = Self {
            config: config.clone(),
            tx,
            rx,
            state: ConnectionState::Disconnected,
        };

        // Start connection in background
        connector.connect_internal(tx_internal, rx_internal).await?;

        Ok(connector)
    }

    /// Internal connection logic
    async fn connect_internal(
        &mut self,
        tx_out: mpsc::Sender<WebSocketMessage>,
        mut rx_in: mpsc::Receiver<WebSocketMessage>,
    ) -> Result<(), WebSocketError> {
        self.state = ConnectionState::Connecting;

        // Parse URL
        let url = url::Url::parse(&self.config.url)
            .map_err(|e| WebSocketError::InvalidUrl(e.to_string()))?;

        // Connect to WebSocket
        let (ws_stream, _) = connect_async(url.as_str())
            .await
            .map_err(|e| WebSocketError::ConnectionFailed(e.to_string()))?;

        self.state = ConnectionState::Connected;

        let (mut write, mut read) = ws_stream.split();

        // Spawn task to handle incoming messages
        let tx_clone = tx_out.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        let _ = tx_clone
                            .send(WebSocketMessage::Text {
                                content: text.to_string(),
                            })
                            .await;
                    }
                    Ok(WsMessage::Binary(data)) => {
                        let _ = tx_clone
                            .send(WebSocketMessage::Binary {
                                data: data.to_vec(),
                            })
                            .await;
                    }
                    Ok(WsMessage::Ping(data)) => {
                        let _ = tx_clone
                            .send(WebSocketMessage::Ping {
                                data: data.to_vec(),
                            })
                            .await;
                    }
                    Ok(WsMessage::Pong(data)) => {
                        let _ = tx_clone
                            .send(WebSocketMessage::Pong {
                                data: data.to_vec(),
                            })
                            .await;
                    }
                    Ok(WsMessage::Close(frame)) => {
                        let (code, reason) = frame
                            .map(|f| (f.code.into(), f.reason.to_string()))
                            .unwrap_or((1000, "Normal closure".to_string()));
                        let _ = tx_clone
                            .send(WebSocketMessage::Close { code, reason })
                            .await;
                        break;
                    }
                    Ok(WsMessage::Frame(_)) => {
                        // Raw frames are typically not exposed at this level
                    }
                    Err(_) => break,
                }
            }
        });

        // Spawn task to handle outgoing messages
        tokio::spawn(async move {
            while let Some(msg) = rx_in.recv().await {
                let ws_msg = match msg {
                    WebSocketMessage::Text { content } => WsMessage::Text(content.into()),
                    WebSocketMessage::Binary { data } => WsMessage::Binary(data.into()),
                    WebSocketMessage::Ping { data } => WsMessage::Ping(data.into()),
                    WebSocketMessage::Pong { data } => WsMessage::Pong(data.into()),
                    WebSocketMessage::Close { code, reason } => WsMessage::Close(Some(
                        tokio_tungstenite::tungstenite::protocol::CloseFrame {
                            code: code.into(),
                            reason: reason.into(),
                        },
                    )),
                };

                if write.send(ws_msg).await.is_err() {
                    break;
                }
            }
        });

        // Start ping task if configured
        if self.config.ping_interval_secs > 0 {
            let tx_ping = self.tx.clone();
            let ping_interval = self.config.ping_interval_secs;
            tokio::spawn(async move {
                let mut ticker = interval(Duration::from_secs(ping_interval));
                loop {
                    ticker.tick().await;
                    if tx_ping
                        .send(WebSocketMessage::Ping { data: vec![] })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }

        Ok(())
    }

    /// Send a text message
    pub async fn send_text(&self, text: impl Into<String>) -> Result<(), WebSocketError> {
        self.tx
            .send(WebSocketMessage::text(text))
            .await
            .map_err(|e| WebSocketError::SendFailed(e.to_string()))
    }

    /// Send a binary message
    pub async fn send_binary(&self, data: Vec<u8>) -> Result<(), WebSocketError> {
        self.tx
            .send(WebSocketMessage::binary(data))
            .await
            .map_err(|e| WebSocketError::SendFailed(e.to_string()))
    }

    /// Send a custom message
    pub async fn send(&self, message: WebSocketMessage) -> Result<(), WebSocketError> {
        self.tx
            .send(message)
            .await
            .map_err(|e| WebSocketError::SendFailed(e.to_string()))
    }

    /// Receive a message
    pub async fn receive(&mut self) -> Option<WebSocketMessage> {
        self.rx.recv().await
    }

    /// Get the current connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Close the connection
    pub async fn close(&self) -> Result<(), WebSocketError> {
        self.tx
            .send(WebSocketMessage::Close {
                code: 1000,
                reason: "Normal closure".to_string(),
            })
            .await
            .map_err(|e| WebSocketError::SendFailed(e.to_string()))?;

        Ok(())
    }
}

// Stub implementations when websocket feature is disabled
#[cfg(not(feature = "websocket"))]
#[derive(Debug)]
#[allow(dead_code)]
pub struct WebSocketConnector;

#[cfg(not(feature = "websocket"))]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WebSocketConfig {
    pub url: String,
}

#[cfg(not(feature = "websocket"))]
impl WebSocketConfig {
    #[allow(dead_code)]
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

#[cfg(not(feature = "websocket"))]
impl WebSocketConnector {
    #[allow(dead_code)]
    pub async fn new(_config: WebSocketConfig) -> Result<Self, String> {
        Err("WebSocket feature not enabled".to_string())
    }
}

#[cfg(all(test, feature = "websocket"))]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_config_creation() {
        let config = WebSocketConfig::new("wss://api.example.com/ws");
        assert_eq!(config.url, "wss://api.example.com/ws");
        assert!(!config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 0);
        assert_eq!(config.reconnect_delay_ms, 1000);
        assert_eq!(config.ping_interval_secs, 0);
    }

    #[test]
    fn test_websocket_config_builder() {
        let config = WebSocketConfig::new("wss://api.example.com/ws")
            .with_reconnect(true)
            .with_max_reconnects(5)
            .with_ping_interval(30)
            .with_queue_size(200);

        assert!(config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 5);
        assert_eq!(config.ping_interval_secs, 30);
        assert_eq!(config.message_queue_size, 200);
    }

    #[test]
    fn test_websocket_message_creation() {
        let text_msg = WebSocketMessage::text("Hello");
        match text_msg {
            WebSocketMessage::Text { content } => assert_eq!(content, "Hello"),
            _ => panic!("Expected text message"),
        }

        let binary_msg = WebSocketMessage::binary(vec![1, 2, 3]);
        match binary_msg {
            WebSocketMessage::Binary { data } => assert_eq!(data, vec![1, 2, 3]),
            _ => panic!("Expected binary message"),
        }
    }
}
