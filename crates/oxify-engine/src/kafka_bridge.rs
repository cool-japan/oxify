//! Kafka event-bus bridge for OxiFY
//!
//! Enables multi-instance OxiFY deployments to share workflow events by forwarding
//! events from the in-process [`EventBus`] to an external Kafka cluster.
//!
//! # Topic naming
//!
//! Events are published to Kafka topics of the form:
//! `{prefix}-{event_type_with_dots_replaced_by_dashes}`
//!
//! For example, with `topic_prefix = "oxify.events"`, a `workflow.started` event
//! for workflow `550e8400-...` would be published to:
//! `oxify.events-workflow-started`
//!
//! The workflow ID is used as the Kafka message key to ensure partition affinity
//! (all events for the same workflow land on the same partition).

use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::event_bus::EventBus;

/// Configuration for the Kafka bridge.
#[derive(Debug, Clone)]
pub struct KafkaBridgeConfig {
    /// Comma-separated list of Kafka bootstrap broker addresses.
    ///
    /// Example: `"broker1:9092,broker2:9092"`.
    pub brokers: String,

    /// Topic prefix for published events.
    ///
    /// Full topic names take the form `{prefix}-{event_type}` where dots in
    /// `event_type` are replaced with dashes (Kafka topic names cannot contain
    /// periods in many deployments).
    pub topic_prefix: String,

    /// Optional filter list of event types to forward.
    ///
    /// `None` means forward every event; `Some(vec)` forwards only those
    /// whose `event_type` appears in the list.
    pub publish_filter: Option<Vec<String>>,

    /// Optional SASL credentials for authenticating to the Kafka cluster.
    pub credentials: Option<KafkaCreds>,
}

impl Default for KafkaBridgeConfig {
    fn default() -> Self {
        Self {
            brokers: "localhost:9092".to_string(),
            topic_prefix: "oxify.events".to_string(),
            publish_filter: None,
            credentials: None,
        }
    }
}

/// SASL authentication credentials for the Kafka cluster.
#[derive(Debug, Clone)]
pub enum KafkaCreds {
    /// SASL/PLAIN username + password authentication.
    SaslPlain {
        /// SASL username.
        username: String,
        /// SASL password.
        password: String,
    },

    /// SASL/SCRAM-SHA-256 or SASL/SCRAM-SHA-512 authentication.
    SaslScram {
        /// SCRAM mechanism string, e.g. `"SCRAM-SHA-256"` or `"SCRAM-SHA-512"`.
        mechanism: String,
        /// SASL username.
        username: String,
        /// SASL password.
        password: String,
    },
}

/// Errors that can occur in the Kafka bridge.
#[derive(thiserror::Error, Debug)]
pub enum KafkaBridgeError {
    /// The producer could not be created (connection to broker failed).
    #[error("failed to connect to Kafka at {url}: {source}")]
    Connect {
        /// The broker address(es) that were used.
        url: String,
        /// The underlying rdkafka error.
        #[source]
        source: rdkafka::error::KafkaError,
    },

    /// A message could not be published to Kafka.
    #[error("failed to publish to Kafka: {0}")]
    Publish(String),

    /// Serialisation of an event failed.
    #[error("failed to decode event: {0}")]
    Decode(String),
}

/// Bridge between the in-process [`EventBus`] and an external Kafka cluster.
///
/// When [`KafkaBridge::connect`] is called a background Tokio task is spawned
/// that subscribes to the event bus and forwards each qualifying event to Kafka.
/// The task runs until the bridge is dropped or [`KafkaBridge::shutdown`] is
/// called explicitly.
pub struct KafkaBridge {
    /// Live rdkafka future producer.
    producer: FutureProducer,
    /// Configuration snapshot used during construction.
    cfg: KafkaBridgeConfig,
    /// Handle to the spawned forwarding task.
    pub_handle: JoinHandle<()>,
}

impl KafkaBridge {
    /// Connect to a Kafka cluster and start forwarding events from `bus`.
    ///
    /// The supplied `bus` is cloned (cheaply via [`Arc`]) so the bridge does
    /// not take exclusive ownership.
    pub async fn connect(
        bus: Arc<EventBus>,
        cfg: KafkaBridgeConfig,
    ) -> Result<Self, KafkaBridgeError> {
        let mut client_cfg = ClientConfig::new();
        client_cfg.set("bootstrap.servers", &cfg.brokers);

        if let Some(ref creds) = cfg.credentials {
            match creds {
                KafkaCreds::SaslPlain { username, password } => {
                    client_cfg.set("security.protocol", "SASL_PLAINTEXT");
                    client_cfg.set("sasl.mechanisms", "PLAIN");
                    client_cfg.set("sasl.username", username);
                    client_cfg.set("sasl.password", password);
                }
                KafkaCreds::SaslScram {
                    mechanism,
                    username,
                    password,
                } => {
                    client_cfg.set("security.protocol", "SASL_PLAINTEXT");
                    client_cfg.set("sasl.mechanisms", mechanism);
                    client_cfg.set("sasl.username", username);
                    client_cfg.set("sasl.password", password);
                }
            }
        }

        let producer: FutureProducer =
            client_cfg
                .create()
                .map_err(|source| KafkaBridgeError::Connect {
                    url: cfg.brokers.clone(),
                    source,
                })?;

        let mut rx = bus.subscribe();
        let task_producer = producer.clone();
        let task_prefix = cfg.topic_prefix.clone();
        let task_filter = cfg.publish_filter.clone();

        let pub_handle = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if !Self::should_publish(&event.event_type, &task_filter) {
                            continue;
                        }

                        let topic = Self::topic_for(&event.event_type, &task_prefix);
                        let key = event.workflow_id.to_string();

                        let payload = match serde_json::to_vec(&event) {
                            Ok(serialised) => serialised,
                            Err(err) => {
                                tracing::warn!(
                                    event_id = %event.id,
                                    event_type = %event.event_type,
                                    error = %err,
                                    "kafka_bridge: failed to serialise event; skipping"
                                );
                                continue;
                            }
                        };

                        let record = FutureRecord::to(&topic)
                            .payload(payload.as_slice())
                            .key(key.as_str());

                        if let Err((err, _)) =
                            task_producer.send(record, Duration::from_secs(5)).await
                        {
                            tracing::warn!(
                                topic = %topic,
                                error = %err,
                                "kafka_bridge: failed to publish event to Kafka"
                            );
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped = skipped,
                            "kafka_bridge: broadcast channel lagged; {} events dropped",
                            skipped,
                        );
                        // Continue — do not exit the loop on lag.
                    }
                    Err(RecvError::Closed) => {
                        tracing::info!("kafka_bridge: event bus closed; forwarding task exiting");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            producer,
            cfg,
            pub_handle,
        })
    }

    /// Gracefully shut down the bridge.
    ///
    /// Aborts the background forwarding task and drops the producer,
    /// which flushes any in-flight messages.
    pub fn shutdown(self) {
        self.pub_handle.abort();
        // Drop `producer` — rdkafka's FutureProducer flushes on drop.
        drop(self.producer);
    }

    /// Return the configuration that was used to create this bridge.
    pub fn config(&self) -> &KafkaBridgeConfig {
        &self.cfg
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Build the Kafka topic name for the given `event_type` and `prefix`.
    ///
    /// Dots in `event_type` are replaced with dashes because many Kafka
    /// deployments disallow `.` in topic names (it conflicts with JMX metrics).
    ///
    /// Format: `{prefix}-{event_type_with_dashes}`
    fn topic_for(event_type: &str, prefix: &str) -> String {
        format!("{}-{}", prefix, event_type.replace('.', "-"))
    }

    /// Returns `true` when the event should be forwarded to Kafka.
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

    // ------------------------------------------------------------------
    // topic_for
    // ------------------------------------------------------------------

    #[test]
    fn test_topic_for_replaces_dots() {
        let topic = KafkaBridge::topic_for("workflow.started", "oxify.events");
        assert_eq!(topic, "oxify.events-workflow-started");
    }

    #[test]
    fn test_topic_for_no_dots_in_event_type() {
        let topic = KafkaBridge::topic_for("heartbeat", "oxify.events");
        assert_eq!(topic, "oxify.events-heartbeat");
    }

    // ------------------------------------------------------------------
    // KafkaBridgeConfig::default
    // ------------------------------------------------------------------

    #[test]
    fn test_config_default_values() {
        let cfg = KafkaBridgeConfig::default();

        assert_eq!(cfg.brokers, "localhost:9092");
        assert_eq!(cfg.topic_prefix, "oxify.events");
        assert!(
            cfg.publish_filter.is_none(),
            "default filter should forward all events"
        );
        assert!(
            cfg.credentials.is_none(),
            "default config should require no credentials"
        );
    }

    // ------------------------------------------------------------------
    // should_publish
    // ------------------------------------------------------------------

    #[test]
    fn test_should_publish_no_filter() {
        let filter: Option<Vec<String>> = None;

        assert!(
            KafkaBridge::should_publish("workflow.started", &filter),
            "with no filter every event type must pass"
        );
        assert!(
            KafkaBridge::should_publish("workflow.completed", &filter),
            "with no filter every event type must pass"
        );
        assert!(
            KafkaBridge::should_publish("node.failed", &filter),
            "with no filter every event type must pass"
        );
    }

    #[test]
    fn test_should_publish_filter_match() {
        let filter = Some(vec!["workflow.started".to_string()]);

        assert!(
            KafkaBridge::should_publish("workflow.started", &filter),
            "workflow.started is in the allow-list and must pass"
        );
    }

    #[test]
    fn test_should_publish_filter_no_match() {
        let filter = Some(vec!["workflow.started".to_string()]);

        assert!(
            !KafkaBridge::should_publish("workflow.completed", &filter),
            "workflow.completed is NOT in the allow-list and must be blocked"
        );
    }

    // ------------------------------------------------------------------
    // Live-Kafka round-trip (requires a running Kafka broker; skipped by default)
    // ------------------------------------------------------------------

    #[ignore]
    #[tokio::test]
    async fn test_connect_roundtrip() {
        use crate::event_bus::WorkflowEvent;

        let bus = Arc::new(EventBus::default());

        let cfg = KafkaBridgeConfig {
            brokers: "localhost:9092".to_string(),
            topic_prefix: "oxify.test".to_string(),
            publish_filter: None,
            credentials: None,
        };

        let bridge = KafkaBridge::connect(Arc::clone(&bus), cfg)
            .await
            .expect("must connect to local Kafka");

        let workflow_id = uuid::Uuid::new_v4();
        let event = WorkflowEvent::new(
            "workflow.started".to_string(),
            workflow_id,
            serde_json::json!({"status": "started"}),
        );

        bus.publish(event.clone())
            .await
            .expect("publish must succeed");

        // Allow the forwarding task to pick up and forward the message.
        tokio::time::sleep(Duration::from_millis(500)).await;

        bridge.shutdown();
    }
}
