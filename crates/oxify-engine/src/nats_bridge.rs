//! NATS event-bus bridge for OxiFY
//!
//! Enables multi-instance OxiFY deployments to share workflow events by forwarding
//! events from the in-process [`EventBus`] to an external NATS server.
//!
//! # Subject naming
//!
//! Events are published to NATS subjects of the form:
//! `{prefix}.{event_type}.{workflow_id}`
//!
//! For example, with `subject_prefix = "oxify.events"`, a `workflow.started` event
//! for workflow `550e8400-...` would be published to:
//! `oxify.events.workflow.started.550e8400-...`

use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::event_bus::EventBus;

/// Configuration for the NATS bridge.
#[derive(Debug, Clone)]
pub struct NatsBridgeConfig {
    /// NATS server URL, e.g. `"nats://localhost:4222"`.
    pub url: String,

    /// Subject prefix for published events, e.g. `"oxify.events"`.
    ///
    /// Full subjects take the form `{prefix}.{event_type}.{workflow_id}`.
    pub subject_prefix: String,

    /// Optional filter list of event types to forward.
    ///
    /// `None` means forward every event; `Some(vec)` forwards only those
    /// whose `event_type` appears in the list.
    pub publish_filter: Option<Vec<String>>,

    /// Optional credentials for authenticating to the NATS server.
    pub credentials: Option<NatsCreds>,
}

impl Default for NatsBridgeConfig {
    fn default() -> Self {
        Self {
            url: "nats://localhost:4222".to_string(),
            subject_prefix: "oxify.events".to_string(),
            publish_filter: None,
            credentials: None,
        }
    }
}

/// Authentication credentials for the NATS server.
#[derive(Debug, Clone)]
pub enum NatsCreds {
    /// Plain user/password authentication.
    UserPass {
        /// NATS username.
        user: String,
        /// NATS password.
        password: String,
    },
    /// JWT + NKey authentication (NATS 2.x decentralised auth).
    JwtNkey {
        /// Signed JWT token issued by the NATS account server.
        jwt: String,
        /// NKey seed (private key) corresponding to the JWT.
        nkey: String,
    },
}

/// Errors that can occur in the NATS bridge.
#[derive(thiserror::Error, Debug)]
pub enum NatsBridgeError {
    /// The initial TCP connection to the NATS server failed.
    #[error("failed to connect to NATS at {url}: {source}")]
    Connect {
        /// The URL that was dialed.
        url: String,
        /// The underlying connection error.
        #[source]
        source: async_nats::ConnectError,
    },

    /// A message could not be published to NATS.
    #[error("failed to publish to NATS: {0}")]
    Publish(String),

    /// Deserialisation of a received NATS message failed.
    #[error("failed to decode event: {0}")]
    Decode(String),
}

/// Bridge between the in-process [`EventBus`] and an external NATS server.
///
/// When [`NatsBridge::connect`] is called a background Tokio task is spawned
/// that subscribes to the event bus and forwards each qualifying event to NATS.
/// The task runs until the bridge is dropped or [`NatsBridge::shutdown`] is
/// called explicitly.
pub struct NatsBridge {
    /// Live connection to the NATS server.
    client: async_nats::Client,
    /// Configuration snapshot used during construction.
    cfg: NatsBridgeConfig,
    /// Handle to the spawned forwarding task.
    pub_handle: JoinHandle<()>,
}

impl NatsBridge {
    /// Connect to NATS and start forwarding events from `bus`.
    ///
    /// The supplied `bus` is cloned (cheaply via [`Arc`]) so the bridge does
    /// not take exclusive ownership.
    pub async fn connect(
        bus: Arc<EventBus>,
        cfg: NatsBridgeConfig,
    ) -> Result<Self, NatsBridgeError> {
        let client = async_nats::connect(cfg.url.as_str())
            .await
            .map_err(|source| NatsBridgeError::Connect {
                url: cfg.url.clone(),
                source,
            })?;

        let mut rx = bus.subscribe();
        let task_client = client.clone();
        let task_prefix = cfg.subject_prefix.clone();
        let task_filter = cfg.publish_filter.clone();

        let pub_handle = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if !Self::should_publish(&event.event_type, &task_filter) {
                            continue;
                        }

                        let subject =
                            Self::subject_for(&event.event_type, event.workflow_id, &task_prefix);

                        let payload = match serde_json::to_vec(&event) {
                            Ok(serialised) => Bytes::from(serialised),
                            Err(err) => {
                                tracing::warn!(
                                    event_id = %event.id,
                                    event_type = %event.event_type,
                                    error = %err,
                                    "nats_bridge: failed to serialise event; skipping"
                                );
                                continue;
                            }
                        };

                        if let Err(err) = task_client.publish(subject.clone(), payload).await {
                            tracing::warn!(
                                subject = %subject,
                                error = %err,
                                "nats_bridge: failed to publish event to NATS"
                            );
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped = skipped,
                            "nats_bridge: broadcast channel lagged; {} events dropped",
                            skipped,
                        );
                        // Continue — do not exit the loop on lag.
                    }
                    Err(RecvError::Closed) => {
                        tracing::info!("nats_bridge: event bus closed; forwarding task exiting");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            client,
            cfg,
            pub_handle,
        })
    }

    /// Gracefully shut down the bridge.
    ///
    /// Aborts the background forwarding task and flushes pending NATS messages.
    pub async fn shutdown(self) {
        self.pub_handle.abort();
        // Drain any buffered outbound messages before closing.
        if let Err(err) = self.client.flush().await {
            tracing::warn!(error = %err, "nats_bridge: flush on shutdown returned error");
        }
    }

    /// Return the configuration that was used to create this bridge.
    pub fn config(&self) -> &NatsBridgeConfig {
        &self.cfg
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Build the NATS subject for the given `event_type` and `workflow_id`.
    ///
    /// Format: `{prefix}.{event_type}.{workflow_id}`
    fn subject_for(event_type: &str, workflow_id: uuid::Uuid, prefix: &str) -> String {
        format!("{}.{}.{}", prefix, event_type, workflow_id)
    }

    /// Returns `true` when the event should be forwarded to NATS.
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
    // subject_for
    // ------------------------------------------------------------------

    #[test]
    fn test_subject_for_formats_correctly() {
        let workflow_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .expect("hard-coded UUID must parse");
        let subject = NatsBridge::subject_for("workflow.started", workflow_id, "oxify.events");

        assert_eq!(
            subject,
            "oxify.events.workflow.started.550e8400-e29b-41d4-a716-446655440000"
        );
    }

    // ------------------------------------------------------------------
    // NatsBridgeConfig::default
    // ------------------------------------------------------------------

    #[test]
    fn test_config_default_values() {
        let cfg = NatsBridgeConfig::default();

        assert_eq!(cfg.url, "nats://localhost:4222");
        assert_eq!(cfg.subject_prefix, "oxify.events");
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
            NatsBridge::should_publish("workflow.started", &filter),
            "with no filter every event type must pass"
        );
        assert!(
            NatsBridge::should_publish("workflow.completed", &filter),
            "with no filter every event type must pass"
        );
        assert!(
            NatsBridge::should_publish("node.failed", &filter),
            "with no filter every event type must pass"
        );
    }

    #[test]
    fn test_should_publish_with_filter_match() {
        let filter = Some(vec!["workflow.started".to_string()]);

        assert!(
            NatsBridge::should_publish("workflow.started", &filter),
            "workflow.started is in the allow-list and must pass"
        );
    }

    #[test]
    fn test_should_publish_with_filter_no_match() {
        let filter = Some(vec!["workflow.started".to_string()]);

        assert!(
            !NatsBridge::should_publish("workflow.completed", &filter),
            "workflow.completed is NOT in the allow-list and must be blocked"
        );
    }

    // ------------------------------------------------------------------
    // Live-NATS round-trip (requires a running NATS server; skipped by default)
    // ------------------------------------------------------------------

    #[ignore]
    #[tokio::test]
    async fn test_connect_roundtrip() {
        use crate::event_bus::WorkflowEvent;
        use futures::StreamExt as _;

        let bus = Arc::new(EventBus::default());

        let cfg = NatsBridgeConfig {
            url: "nats://localhost:4222".to_string(),
            subject_prefix: "oxify.test".to_string(),
            publish_filter: None,
            credentials: None,
        };

        let bridge = NatsBridge::connect(Arc::clone(&bus), cfg.clone())
            .await
            .expect("must connect to local NATS");

        // Subscribe on NATS side before publishing.
        let nats_client = async_nats::connect("nats://localhost:4222")
            .await
            .expect("must connect to local NATS for subscriber");

        let workflow_id = Uuid::new_v4();
        let mut nats_sub = nats_client
            .subscribe(format!("oxify.test.workflow.started.{}", workflow_id))
            .await
            .expect("subscribe must succeed");

        // Publish an event through the EventBus.
        let event = WorkflowEvent::new(
            "workflow.started".to_string(),
            workflow_id,
            serde_json::json!({"status": "started"}),
        );
        bus.publish(event.clone())
            .await
            .expect("publish must succeed");

        // Allow the forwarding task to pick up the message.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let msg = tokio::time::timeout(std::time::Duration::from_secs(2), nats_sub.next())
            .await
            .expect("timed out waiting for NATS message")
            .expect("subscriber must yield a message");

        let received: WorkflowEvent =
            serde_json::from_slice(&msg.payload).expect("payload must deserialise");
        assert_eq!(received.id, event.id);

        bridge.shutdown().await;
    }
}
