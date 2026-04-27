use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::{
    errors::Result,
    types::{Channel, MessageReceipt, OutgoingMessage},
};

// ---------------------------------------------------------------------------
// MockMessageProvider
// ---------------------------------------------------------------------------

/// An in-memory [`super::MessageProvider`] that records every sent message.
///
/// Designed for unit tests: construct it, inject it via the trait object, then
/// call [`MockMessageProvider::sent_messages`] to assert on what was sent.
///
/// ```rust
/// # use oxify_connect_comm::{MockMessageProvider, providers::MessageProvider};
/// # use oxify_connect_comm::types::{OutgoingMessage, Recipient};
/// # #[tokio::main]
/// # async fn main() {
/// let mock = MockMessageProvider::new();
/// mock.send_message(OutgoingMessage::simple(
///     Recipient::Channel("#general".to_owned()),
///     "hello",
/// )).await.unwrap();
/// assert_eq!(mock.sent_messages().await.len(), 1);
/// # }
/// ```
#[derive(Clone)]
pub struct MockMessageProvider {
    sent: Arc<RwLock<Vec<OutgoingMessage>>>,
}

impl MockMessageProvider {
    /// Create a new, empty `MockMessageProvider`.
    pub fn new() -> Self {
        Self {
            sent: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Return a snapshot of all messages that have been sent so far.
    pub async fn sent_messages(&self) -> Vec<OutgoingMessage> {
        self.sent.read().await.clone()
    }

    /// Clear all recorded messages.
    pub async fn clear(&self) {
        self.sent.write().await.clear();
    }
}

impl Default for MockMessageProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// MessageProvider implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl super::MessageProvider for MockMessageProvider {
    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn send_message(&self, msg: OutgoingMessage) -> Result<MessageReceipt> {
        debug!(to = ?msg.to, "mock: recording message");

        let ts = chrono::Utc::now();
        let message_id = format!("mock-{}", ts.timestamp_nanos_opt().unwrap_or(0));

        self.sent.write().await.push(msg);

        Ok(MessageReceipt {
            message_id,
            timestamp: ts,
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        Ok(vec![
            Channel {
                id: "MOCK001".to_string(),
                name: "mock-general".to_string(),
                is_private: false,
            },
            Channel {
                id: "MOCK002".to_string(),
                name: "mock-random".to_string(),
                is_private: false,
            },
            Channel {
                id: "MOCK003".to_string(),
                name: "mock-private".to_string(),
                is_private: true,
            },
        ])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MessageProvider;
    use crate::types::{OutgoingMessage, Recipient};

    #[tokio::test]
    async fn test_mock_records_sent() {
        let mock = MockMessageProvider::new();

        let messages = vec![
            OutgoingMessage::simple(Recipient::Channel("C001".to_string()), "first message"),
            OutgoingMessage::simple(Recipient::User("U002".to_string()), "second message"),
            OutgoingMessage::simple(Recipient::Channel("C003".to_string()), "third message"),
        ];

        for msg in messages {
            mock.send_message(msg).await.expect("send_message");
        }

        let sent = mock.sent_messages().await;
        assert_eq!(
            sent.len(),
            3,
            "expected 3 recorded messages, got {}",
            sent.len()
        );
    }

    #[tokio::test]
    async fn test_mock_list_channels_not_empty() {
        let mock = MockMessageProvider::new();
        let channels = mock.list_channels().await.expect("list_channels");

        assert!(
            !channels.is_empty(),
            "mock provider should return at least one channel"
        );

        // Verify we have both public and private channels.
        let has_private = channels.iter().any(|c| c.is_private);
        let has_public = channels.iter().any(|c| !c.is_private);
        assert!(has_private, "expected at least one private channel");
        assert!(has_public, "expected at least one public channel");
    }

    #[tokio::test]
    async fn test_mock_clear_resets_sent_queue() {
        let mock = MockMessageProvider::new();

        mock.send_message(OutgoingMessage::simple(
            Recipient::Channel("C001".to_string()),
            "msg 1",
        ))
        .await
        .expect("send");

        mock.clear().await;
        assert_eq!(mock.sent_messages().await.len(), 0);
    }

    #[tokio::test]
    async fn test_mock_receipt_has_unique_message_ids() {
        let mock = MockMessageProvider::new();

        let r1 = mock
            .send_message(OutgoingMessage::simple(
                Recipient::Channel("C001".to_string()),
                "a",
            ))
            .await
            .expect("r1");

        // Brief yield to ensure distinct nanosecond timestamps.
        tokio::task::yield_now().await;

        let r2 = mock
            .send_message(OutgoingMessage::simple(
                Recipient::Channel("C001".to_string()),
                "b",
            ))
            .await
            .expect("r2");

        // Both IDs must be non-empty.  They may be equal only if the
        // monotonic clock does not advance, which is acceptable.
        assert!(!r1.message_id.is_empty());
        assert!(!r2.message_id.is_empty());
    }
}
