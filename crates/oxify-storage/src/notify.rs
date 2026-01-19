//! PostgreSQL LISTEN/NOTIFY Support
//!
//! This module provides real-time event notifications using PostgreSQL's
//! LISTEN/NOTIFY mechanism for distributed systems coordination.
//!
//! ## Features
//!
//! - Workflow execution state change notifications
//! - Quota update notifications
//! - Schedule trigger notifications
//! - Custom event notifications
//! - Automatic reconnection on connection loss
//! - Event filtering and routing
//!
//! ## Usage
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, notify::{NotificationService, NotificationEvent}};
//!
//! let pool = DatabasePool::new(config).await?;
//! let notify_service = NotificationService::new(pool).await?;
//!
//! // Subscribe to workflow execution events
//! let mut receiver = notify_service.subscribe_execution_events().await?;
//!
//! // Listen for events
//! while let Some(event) = receiver.recv().await {
//!     match event {
//!         NotificationEvent::ExecutionStarted { execution_id, workflow_id } => {
//!             println!("Execution {} started for workflow {}", execution_id, workflow_id);
//!         }
//!         NotificationEvent::ExecutionCompleted { execution_id, state } => {
//!             println!("Execution {} completed with state {}", execution_id, state);
//!         }
//!         _ => {}
//!     }
//! }
//! ```

use crate::{DatabasePool, Result, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgListener, PgNotification};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Notification channels for different event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationChannel {
    /// Workflow execution events
    ExecutionEvents,
    /// Quota update events
    QuotaEvents,
    /// Schedule trigger events
    ScheduleEvents,
    /// Secret management events
    SecretEvents,
    /// Custom application events
    CustomEvents,
}

impl NotificationChannel {
    /// Get the PostgreSQL channel name
    pub fn channel_name(&self) -> &'static str {
        match self {
            Self::ExecutionEvents => "oxify_execution_events",
            Self::QuotaEvents => "oxify_quota_events",
            Self::ScheduleEvents => "oxify_schedule_events",
            Self::SecretEvents => "oxify_secret_events",
            Self::CustomEvents => "oxify_custom_events",
        }
    }
}

/// Notification event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationEvent {
    /// Execution started
    ExecutionStarted {
        execution_id: Uuid,
        workflow_id: Uuid,
        user_id: Uuid,
    },
    /// Execution completed
    ExecutionCompleted {
        execution_id: Uuid,
        workflow_id: Uuid,
        state: String,
        duration_ms: Option<i64>,
    },
    /// Execution failed
    ExecutionFailed {
        execution_id: Uuid,
        workflow_id: Uuid,
        error: String,
    },
    /// Quota limit reached
    QuotaLimitReached {
        user_id: Uuid,
        quota_type: String,
        current: i32,
        limit: i32,
    },
    /// Quota reset
    QuotaReset { user_id: Uuid, reset_type: String },
    /// Schedule triggered
    ScheduleTriggered {
        schedule_id: Uuid,
        workflow_id: Uuid,
        execution_id: Uuid,
    },
    /// Secret created
    SecretCreated {
        secret_id: Uuid,
        user_id: Uuid,
        name: String,
    },
    /// Secret rotated
    SecretRotated { secret_id: Uuid, user_id: Uuid },
    /// Custom event with arbitrary payload
    Custom {
        event_type: String,
        payload: serde_json::Value,
    },
}

/// Notification service for PostgreSQL LISTEN/NOTIFY
pub struct NotificationService {
    pool: DatabasePool,
    listeners: Arc<RwLock<HashMap<NotificationChannel, broadcast::Sender<NotificationEvent>>>>,
}

impl NotificationService {
    /// Create a new notification service
    pub async fn new(pool: DatabasePool) -> Result<Self> {
        let service = Self {
            pool,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        };

        Ok(service)
    }

    /// Start listening on a specific channel
    ///
    /// This spawns a background task that listens for PostgreSQL notifications
    /// and broadcasts them to all subscribers.
    pub async fn start_listening(&self, channel: NotificationChannel) -> Result<()> {
        // Check if already listening
        {
            let listeners = self.listeners.read().await;
            if listeners.contains_key(&channel) {
                return Ok(());
            }
        }

        // Create broadcast channel
        let (tx, _) = broadcast::channel(1000);

        // Store the sender
        {
            let mut listeners = self.listeners.write().await;
            listeners.insert(channel, tx.clone());
        }

        // Spawn listener task
        let pool = self.pool.clone();
        let listeners = Arc::clone(&self.listeners);

        tokio::spawn(async move {
            if let Err(e) = Self::listen_loop(pool, channel, listeners).await {
                error!("Notification listener error for {:?}: {}", channel, e);
            }
        });

        info!("Started listening on channel: {:?}", channel);
        Ok(())
    }

    /// Listen loop for a specific channel
    async fn listen_loop(
        pool: DatabasePool,
        channel: NotificationChannel,
        listeners: Arc<RwLock<HashMap<NotificationChannel, broadcast::Sender<NotificationEvent>>>>,
    ) -> Result<()> {
        loop {
            // Create listener
            let mut listener = PgListener::connect_with(pool.pool()).await?;
            listener.listen(channel.channel_name()).await?;

            info!(
                "Connected to PostgreSQL LISTEN on {}",
                channel.channel_name()
            );

            // Listen for notifications
            loop {
                match listener.recv().await {
                    Ok(notification) => {
                        debug!(
                            "Received notification on {}: {}",
                            channel.channel_name(),
                            notification.payload()
                        );

                        // Parse and broadcast event
                        if let Err(e) =
                            Self::handle_notification(&listeners, channel, notification).await
                        {
                            warn!("Failed to handle notification: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Notification receive error: {}", e);
                        break; // Reconnect
                    }
                }
            }

            // Wait before reconnecting
            warn!("Reconnecting to {} in 5 seconds...", channel.channel_name());
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    /// Handle a received notification
    async fn handle_notification(
        listeners: &Arc<RwLock<HashMap<NotificationChannel, broadcast::Sender<NotificationEvent>>>>,
        channel: NotificationChannel,
        notification: PgNotification,
    ) -> Result<()> {
        // Parse the notification payload
        let event: NotificationEvent =
            serde_json::from_str(notification.payload()).map_err(|e| {
                StorageError::ValidationError(format!("Invalid notification payload: {e}"))
            })?;

        // Get the broadcast sender
        let tx = {
            let listeners_guard = listeners.read().await;
            listeners_guard.get(&channel).cloned()
        };

        if let Some(tx) = tx {
            // Broadcast to all subscribers (ignore errors if no receivers)
            let _ = tx.send(event);
        }

        Ok(())
    }

    /// Subscribe to execution events
    pub async fn subscribe_execution_events(
        &self,
    ) -> Result<broadcast::Receiver<NotificationEvent>> {
        self.start_listening(NotificationChannel::ExecutionEvents)
            .await?;
        let listeners = self.listeners.read().await;
        let tx = listeners
            .get(&NotificationChannel::ExecutionEvents)
            .ok_or_else(|| {
                StorageError::NotFoundLegacy("Execution events channel not found".to_string())
            })?;
        Ok(tx.subscribe())
    }

    /// Subscribe to quota events
    pub async fn subscribe_quota_events(&self) -> Result<broadcast::Receiver<NotificationEvent>> {
        self.start_listening(NotificationChannel::QuotaEvents)
            .await?;
        let listeners = self.listeners.read().await;
        let tx = listeners
            .get(&NotificationChannel::QuotaEvents)
            .ok_or_else(|| {
                StorageError::NotFoundLegacy("Quota events channel not found".to_string())
            })?;
        Ok(tx.subscribe())
    }

    /// Subscribe to schedule events
    pub async fn subscribe_schedule_events(
        &self,
    ) -> Result<broadcast::Receiver<NotificationEvent>> {
        self.start_listening(NotificationChannel::ScheduleEvents)
            .await?;
        let listeners = self.listeners.read().await;
        let tx = listeners
            .get(&NotificationChannel::ScheduleEvents)
            .ok_or_else(|| {
                StorageError::NotFoundLegacy("Schedule events channel not found".to_string())
            })?;
        Ok(tx.subscribe())
    }

    /// Subscribe to secret events
    pub async fn subscribe_secret_events(&self) -> Result<broadcast::Receiver<NotificationEvent>> {
        self.start_listening(NotificationChannel::SecretEvents)
            .await?;
        let listeners = self.listeners.read().await;
        let tx = listeners
            .get(&NotificationChannel::SecretEvents)
            .ok_or_else(|| {
                StorageError::NotFoundLegacy("Secret events channel not found".to_string())
            })?;
        Ok(tx.subscribe())
    }

    /// Subscribe to custom events
    pub async fn subscribe_custom_events(&self) -> Result<broadcast::Receiver<NotificationEvent>> {
        self.start_listening(NotificationChannel::CustomEvents)
            .await?;
        let listeners = self.listeners.read().await;
        let tx = listeners
            .get(&NotificationChannel::CustomEvents)
            .ok_or_else(|| {
                StorageError::NotFoundLegacy("Custom events channel not found".to_string())
            })?;
        Ok(tx.subscribe())
    }

    /// Publish an event to a channel
    ///
    /// This sends a NOTIFY command to PostgreSQL, which will be received by all listeners.
    pub async fn publish(
        &self,
        channel: NotificationChannel,
        event: &NotificationEvent,
    ) -> Result<()> {
        let payload = serde_json::to_string(event).map_err(|e| {
            StorageError::ValidationError(format!("Failed to serialize event: {e}"))
        })?;

        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(channel.channel_name())
            .bind(&payload)
            .execute(self.pool.pool())
            .await?;

        debug!("Published event to {}: {}", channel.channel_name(), payload);
        Ok(())
    }

    /// Publish execution started event
    pub async fn publish_execution_started(
        &self,
        execution_id: Uuid,
        workflow_id: Uuid,
        user_id: Uuid,
    ) -> Result<()> {
        let event = NotificationEvent::ExecutionStarted {
            execution_id,
            workflow_id,
            user_id,
        };
        self.publish(NotificationChannel::ExecutionEvents, &event)
            .await
    }

    /// Publish execution completed event
    pub async fn publish_execution_completed(
        &self,
        execution_id: Uuid,
        workflow_id: Uuid,
        state: String,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        let event = NotificationEvent::ExecutionCompleted {
            execution_id,
            workflow_id,
            state,
            duration_ms,
        };
        self.publish(NotificationChannel::ExecutionEvents, &event)
            .await
    }

    /// Publish quota limit reached event
    pub async fn publish_quota_limit_reached(
        &self,
        user_id: Uuid,
        quota_type: String,
        current: i32,
        limit: i32,
    ) -> Result<()> {
        let event = NotificationEvent::QuotaLimitReached {
            user_id,
            quota_type,
            current,
            limit,
        };
        self.publish(NotificationChannel::QuotaEvents, &event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_names() {
        assert_eq!(
            NotificationChannel::ExecutionEvents.channel_name(),
            "oxify_execution_events"
        );
        assert_eq!(
            NotificationChannel::QuotaEvents.channel_name(),
            "oxify_quota_events"
        );
        assert_eq!(
            NotificationChannel::ScheduleEvents.channel_name(),
            "oxify_schedule_events"
        );
        assert_eq!(
            NotificationChannel::SecretEvents.channel_name(),
            "oxify_secret_events"
        );
        assert_eq!(
            NotificationChannel::CustomEvents.channel_name(),
            "oxify_custom_events"
        );
    }

    #[test]
    fn test_event_serialization() {
        let event = NotificationEvent::ExecutionStarted {
            execution_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: NotificationEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            NotificationEvent::ExecutionStarted { .. } => {}
            _ => panic!("Expected ExecutionStarted event"),
        }
    }

    #[test]
    fn test_custom_event_serialization() {
        let event = NotificationEvent::Custom {
            event_type: "test_event".to_string(),
            payload: serde_json::json!({
                "key": "value",
                "number": 42
            }),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: NotificationEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            NotificationEvent::Custom {
                event_type,
                payload,
            } => {
                assert_eq!(event_type, "test_event");
                assert_eq!(payload["key"], "value");
                assert_eq!(payload["number"], 42);
            }
            _ => panic!("Expected Custom event"),
        }
    }
}
