use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Recipient
// ---------------------------------------------------------------------------

/// The destination of an outgoing message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Recipient {
    /// A Slack channel ID or name (e.g. `#general`, `C0123456789`).
    Channel(String),
    /// An RFC-5321 email address.
    Email(String),
    /// A provider-specific user identifier.
    User(String),
}

// ---------------------------------------------------------------------------
// MessageBody
// ---------------------------------------------------------------------------

/// The content of an outgoing message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageBody {
    /// Plain UTF-8 text with no markup.
    PlainText(String),
    /// Markdown-formatted text (rendered by providers that support it).
    Markdown(String),
    /// HTML markup (used for SMTP multi-part emails, etc.)
    Html(String),
}

// ---------------------------------------------------------------------------
// Attachment
// ---------------------------------------------------------------------------

/// A binary attachment that can accompany an outgoing message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// The file name to present to the recipient.
    pub filename: String,
    /// MIME content-type string (e.g. `application/pdf`).
    pub content_type: String,
    /// Raw bytes of the attachment.
    pub data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// OutgoingMessage
// ---------------------------------------------------------------------------

/// A fully-specified message ready to be sent by a [`crate::providers::MessageProvider`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Where the message should be delivered.
    pub to: Recipient,
    /// Optional subject line (required for email, optional for others).
    pub subject: Option<String>,
    /// The message body.
    pub body: MessageBody,
    /// Zero or more binary attachments.
    pub attachments: Vec<Attachment>,
}

impl OutgoingMessage {
    /// Construct a minimal plain-text message with no subject and no attachments.
    pub fn simple(to: Recipient, text: impl Into<String>) -> Self {
        Self {
            to,
            subject: None,
            body: MessageBody::PlainText(text.into()),
            attachments: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// MessageReceipt
// ---------------------------------------------------------------------------

/// A confirmation returned after a message has been accepted by the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReceipt {
    /// Provider-assigned identifier for the sent message.
    pub message_id: String,
    /// UTC timestamp at which the provider accepted the message.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Channel
// ---------------------------------------------------------------------------

/// Metadata about a messaging channel (Slack channel, mailing list, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    /// Provider-assigned channel identifier.
    pub id: String,
    /// Human-readable name of the channel.
    pub name: String,
    /// Whether this channel has restricted membership.
    pub is_private: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recipient_serde_roundtrip() {
        let cases: Vec<Recipient> = vec![
            Recipient::Channel("C0123456789".to_string()),
            Recipient::Email("alice@example.com".to_string()),
            Recipient::User("U0987654321".to_string()),
        ];

        for original in cases {
            let json = serde_json::to_string(&original).expect("serialize");
            let restored: Recipient = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(original, restored);
        }
    }

    #[test]
    fn test_outgoing_message_simple_constructor() {
        let to = Recipient::Channel("#general".to_string());
        let msg = OutgoingMessage::simple(to.clone(), "hello world");

        assert_eq!(msg.to, to);
        assert_eq!(msg.subject, None);
        assert!(msg.attachments.is_empty());
        match msg.body {
            MessageBody::PlainText(t) => assert_eq!(t, "hello world"),
            _ => panic!("expected PlainText body"),
        }
    }
}
