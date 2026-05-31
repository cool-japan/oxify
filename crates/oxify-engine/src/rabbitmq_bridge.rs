//! RabbitMQ event-bus bridge for OxiFY
//!
//! Enables multi-instance OxiFY deployments to share workflow events by forwarding
//! events from the in-process [`EventBus`] to an external RabbitMQ broker via AMQP.
//!
//! # Routing key naming
//!
//! Events are published to a topic exchange using routing keys of the form:
//! `{prefix}.{event_type}.{workflow_id}`
//!
//! For example, with `routing_key_prefix = "oxify"` and exchange `"oxify.events"`,
//! a `workflow.started` event for workflow `550e8400-...` would be published with
//! routing key: `oxify.workflow.started.550e8400-...`

use lapin::{
    options::{BasicPublishOptions, ExchangeDeclareOptions},
    types::FieldTable,
    BasicProperties, Connection, ConnectionProperties, ExchangeKind,
};
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::event_bus::EventBus;

/// Configuration for the RabbitMQ bridge.
#[derive(Debug, Clone)]
pub struct RabbitMqBridgeConfig {
    /// AMQP broker URL.
    ///
    /// Example: `"amqp://user:pass@localhost:5672/%2f"`.
    pub url: String,

    /// Name of the AMQP topic exchange to publish events on.
    pub exchange: String,

    /// Routing key prefix for published events.
    ///
    /// Full routing keys take the form `{prefix}.{event_type}.{workflow_id}`.
    pub routing_key_prefix: String,

    /// Optional filter list of event types to forward.
    ///
    /// `None` means forward every event; `Some(vec)` forwards only those
    /// whose `event_type` appears in the list.
    pub publish_filter: Option<Vec<String>>,

    /// Whether to declare the exchange as durable.
    ///
    /// Durable exchanges survive broker restarts.
    pub durable: bool,
}

impl Default for RabbitMqBridgeConfig {
    fn default() -> Self {
        Self {
            url: "amqp://localhost:5672/%2f".to_string(),
            exchange: "oxify.events".to_string(),
            routing_key_prefix: "oxify".to_string(),
            publish_filter: None,
            durable: true,
        }
    }
}

/// Errors that can occur in the RabbitMQ bridge.
#[derive(thiserror::Error, Debug)]
pub enum RabbitMqBridgeError {
    /// The AMQP connection to the broker failed.
    #[error("failed to connect to RabbitMQ at {url}: {source}")]
    Connect {
        /// The URL that was dialed.
        url: String,
        /// The underlying lapin error.
        #[source]
        source: lapin::Error,
    },

    /// A message could not be published to the exchange.
    #[error("failed to publish to RabbitMQ: {0}")]
    Publish(String),

    /// Serialisation of an event failed.
    #[error("failed to decode event: {0}")]
    Decode(String),

    /// An AMQP channel operation failed.
    #[error("AMQP channel error: {0}")]
    Channel(String),
}

/// Bridge between the in-process [`EventBus`] and an external RabbitMQ broker.
///
/// When [`RabbitMqBridge::connect`] is called, a background Tokio task is
/// spawned that connects to RabbitMQ, declares the configured topic exchange,
/// then subscribes to the event bus and forwards each qualifying event.
/// The task runs until the bridge is dropped or [`RabbitMqBridge::shutdown`]
/// is called explicitly.
pub struct RabbitMqBridge {
    /// Configuration snapshot used during construction.
    cfg: RabbitMqBridgeConfig,
    /// Handle to the spawned forwarding task.
    pub_handle: JoinHandle<()>,
}

impl RabbitMqBridge {
    /// Connect to RabbitMQ and start forwarding events from `bus`.
    ///
    /// The connection and channel are established inside the spawned task.
    /// If the initial connect or exchange declare fails inside the task,
    /// it logs a warning and exits — the task performs best-effort delivery
    /// matching the NATS bridge pattern.
    ///
    /// The supplied `bus` is cloned (cheaply via [`Arc`]) so the bridge does
    /// not take exclusive ownership.
    pub async fn connect(
        bus: Arc<EventBus>,
        cfg: RabbitMqBridgeConfig,
    ) -> Result<Self, RabbitMqBridgeError> {
        // Perform an eager connection check so callers get a synchronous error
        // for obviously-wrong URLs before the background task is spawned.
        let conn = Connection::connect(&cfg.url, ConnectionProperties::default())
            .await
            .map_err(|source| RabbitMqBridgeError::Connect {
                url: cfg.url.clone(),
                source,
            })?;

        let channel = conn
            .create_channel()
            .await
            .map_err(|e| RabbitMqBridgeError::Channel(e.to_string()))?;

        // Declare the exchange up-front so we fail fast on misconfiguration.
        channel
            .exchange_declare(
                cfg.exchange.as_str().into(),
                ExchangeKind::Topic,
                ExchangeDeclareOptions {
                    durable: cfg.durable,
                    ..ExchangeDeclareOptions::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| RabbitMqBridgeError::Channel(format!("exchange declare: {}", e)))?;

        let mut rx = bus.subscribe();
        let task_cfg = cfg.clone();

        let pub_handle = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if !Self::should_publish(&event.event_type, &task_cfg.publish_filter) {
                            continue;
                        }

                        let routing_key = Self::routing_key_for(
                            &task_cfg.routing_key_prefix,
                            &event.event_type,
                            event.workflow_id,
                        );

                        let payload = match serde_json::to_vec(&event) {
                            Ok(serialised) => serialised,
                            Err(err) => {
                                tracing::warn!(
                                    event_id = %event.id,
                                    event_type = %event.event_type,
                                    error = %err,
                                    "rabbitmq_bridge: failed to serialise event; skipping"
                                );
                                continue;
                            }
                        };

                        let properties =
                            BasicProperties::default().with_content_type("application/json".into());

                        let publish_result = channel
                            .basic_publish(
                                task_cfg.exchange.as_str().into(),
                                routing_key.as_str().into(),
                                BasicPublishOptions::default(),
                                &payload,
                                properties,
                            )
                            .await;

                        match publish_result {
                            Ok(confirm) => {
                                if let Err(err) = confirm.await {
                                    tracing::warn!(
                                        routing_key = %routing_key,
                                        error = %err,
                                        "rabbitmq_bridge: publish confirm failed"
                                    );
                                }
                            }
                            Err(err) => {
                                tracing::warn!(
                                    routing_key = %routing_key,
                                    error = %err,
                                    "rabbitmq_bridge: failed to publish event to RabbitMQ"
                                );
                            }
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped = skipped,
                            "rabbitmq_bridge: broadcast channel lagged; {} events dropped",
                            skipped,
                        );
                        // Continue — do not exit the loop on lag.
                    }
                    Err(RecvError::Closed) => {
                        tracing::info!(
                            "rabbitmq_bridge: event bus closed; forwarding task exiting"
                        );
                        break;
                    }
                }
            }
        });

        Ok(Self { cfg, pub_handle })
    }

    /// Gracefully shut down the bridge.
    ///
    /// Aborts the background forwarding task.
    pub fn shutdown(self) {
        self.pub_handle.abort();
    }

    /// Return the configuration that was used to create this bridge.
    pub fn config(&self) -> &RabbitMqBridgeConfig {
        &self.cfg
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Build the AMQP routing key for the given `prefix`, `event_type`, and `workflow_id`.
    ///
    /// Format: `{prefix}.{event_type}.{workflow_id}`
    fn routing_key_for(prefix: &str, event_type: &str, workflow_id: uuid::Uuid) -> String {
        format!("{}.{}.{}", prefix, event_type, workflow_id)
    }

    /// Returns `true` when the event should be forwarded to RabbitMQ.
    ///
    /// If `filter` is `None` every event passes.  Otherwise only event types
    /// explicitly listed in the filter vec are forwarded.
    fn should_publish(event_type: &str, filter: &Option<Vec<String>>) -> bool {
        match filter {
            None => true,
            Some(allowed) => allowed.iter().any(|f| f == event_type),
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // ------------------------------------------------------------------
    // routing_key_for
    // ------------------------------------------------------------------

    #[test]
    fn test_routing_key_for_formats_correctly() {
        let workflow_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .expect("hard-coded UUID must parse");
        let key = RabbitMqBridge::routing_key_for("oxify", "workflow.started", workflow_id);

        assert_eq!(
            key,
            "oxify.workflow.started.550e8400-e29b-41d4-a716-446655440000"
        );
    }

    // ------------------------------------------------------------------
    // RabbitMqBridgeConfig::default
    // ------------------------------------------------------------------

    #[test]
    fn test_config_default_values() {
        let cfg = RabbitMqBridgeConfig::default();

        assert_eq!(cfg.url, "amqp://localhost:5672/%2f");
        assert_eq!(cfg.exchange, "oxify.events");
        assert_eq!(cfg.routing_key_prefix, "oxify");
        assert!(
            cfg.publish_filter.is_none(),
            "default filter should forward all events"
        );
        assert!(cfg.durable, "default exchange must be durable");
    }

    // ------------------------------------------------------------------
    // should_publish
    // ------------------------------------------------------------------

    #[test]
    fn test_should_publish_no_filter() {
        let filter: Option<Vec<String>> = None;

        assert!(
            RabbitMqBridge::should_publish("workflow.started", &filter),
            "with no filter every event type must pass"
        );
        assert!(
            RabbitMqBridge::should_publish("workflow.completed", &filter),
            "with no filter every event type must pass"
        );
        assert!(
            RabbitMqBridge::should_publish("node.failed", &filter),
            "with no filter every event type must pass"
        );
    }

    #[test]
    fn test_should_publish_filter_match() {
        let filter = Some(vec!["workflow.started".to_string()]);

        assert!(
            RabbitMqBridge::should_publish("workflow.started", &filter),
            "workflow.started is in the allow-list and must pass"
        );
    }

    #[test]
    fn test_should_publish_filter_no_match() {
        let filter = Some(vec!["workflow.started".to_string()]);

        assert!(
            !RabbitMqBridge::should_publish("workflow.completed", &filter),
            "workflow.completed is NOT in the allow-list and must be blocked"
        );
    }

    // ------------------------------------------------------------------
    // Live-RabbitMQ round-trip (requires a running broker; skipped by default)
    // ------------------------------------------------------------------

    #[ignore]
    #[tokio::test]
    async fn test_connect_roundtrip() {
        use crate::event_bus::WorkflowEvent;

        let bus = Arc::new(EventBus::default());

        let cfg = RabbitMqBridgeConfig {
            url: "amqp://localhost:5672/%2f".to_string(),
            exchange: "oxify.test".to_string(),
            routing_key_prefix: "oxify.test".to_string(),
            publish_filter: None,
            durable: false,
        };

        let bridge = RabbitMqBridge::connect(Arc::clone(&bus), cfg)
            .await
            .expect("must connect to local RabbitMQ");

        let workflow_id = Uuid::new_v4();
        let event = WorkflowEvent::new(
            "workflow.started".to_string(),
            workflow_id,
            serde_json::json!({"status": "started"}),
        );

        bus.publish(event.clone())
            .await
            .expect("publish must succeed");

        // Allow the forwarding task to pick up and forward the message.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        bridge.shutdown();
    }
}
