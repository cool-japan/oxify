use std::env;

use lettre::{
    message::{header::ContentType, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use tracing::{debug, instrument};

use crate::{
    errors::{CommError, Result},
    types::{Channel, MessageBody, MessageReceipt, OutgoingMessage, Recipient},
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the SMTP email provider.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    /// Hostname of the SMTP relay (e.g. `smtp.gmail.com`).
    pub host: String,
    /// TCP port — typically 587 (STARTTLS) or 465 (SMTPS).
    pub port: u16,
    /// SMTP authentication username.
    pub username: String,
    /// SMTP authentication password or app-password.
    pub password: String,
    /// RFC-5321 email address used as the sender `From:` address.
    pub from_address: String,
    /// Optional display name paired with `from_address`.
    pub from_name: Option<String>,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 587,
            username: String::new(),
            password: String::new(),
            from_address: String::new(),
            from_name: None,
        }
    }
}

impl SmtpConfig {
    /// Build configuration from environment variables:
    ///
    /// | Variable        | Field            | Required |
    /// |-----------------|------------------|----------|
    /// | `SMTP_HOST`     | `host`           | yes      |
    /// | `SMTP_PORT`     | `port`           | no (587) |
    /// | `SMTP_USERNAME` | `username`       | yes      |
    /// | `SMTP_PASSWORD` | `password`       | yes      |
    /// | `SMTP_FROM`     | `from_address`   | yes      |
    /// | `SMTP_FROM_NAME`| `from_name`      | no       |
    pub fn from_env() -> Result<Self> {
        let host = env::var("SMTP_HOST").map_err(|_| {
            CommError::Auth("SMTP_HOST environment variable is not set".to_string())
        })?;

        let port = env::var("SMTP_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(587);

        let username = env::var("SMTP_USERNAME").map_err(|_| {
            CommError::Auth("SMTP_USERNAME environment variable is not set".to_string())
        })?;

        let password = env::var("SMTP_PASSWORD").map_err(|_| {
            CommError::Auth("SMTP_PASSWORD environment variable is not set".to_string())
        })?;

        let from_address = env::var("SMTP_FROM").map_err(|_| {
            CommError::Auth("SMTP_FROM environment variable is not set".to_string())
        })?;

        let from_name = env::var("SMTP_FROM_NAME").ok();

        Ok(Self {
            host,
            port,
            username,
            password,
            from_address,
            from_name,
        })
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// SMTP email provider backed by the `lettre` crate with Tokio async support.
pub struct SmtpProvider {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    /// Formatted sender address: `"Display Name <addr>"` or just `"addr"`.
    from: String,
}

impl SmtpProvider {
    /// Construct an `SmtpProvider` from `cfg`, establishing the connection
    /// pool (credentials are validated at send-time, not here).
    pub fn new(cfg: SmtpConfig) -> Result<Self> {
        let creds = Credentials::new(cfg.username.clone(), cfg.password.clone());

        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)
            .map_err(|e| CommError::Provider(format!("SMTP relay construction failed: {e}")))?
            .credentials(creds)
            .port(cfg.port)
            .build();

        let from = match &cfg.from_name {
            Some(name) => format!("{name} <{}>", cfg.from_address),
            None => cfg.from_address.clone(),
        };

        Ok(Self { transport, from })
    }

    /// Construct an `SmtpProvider` reading credentials from the environment.
    pub fn from_env() -> Result<Self> {
        Self::new(SmtpConfig::from_env()?)
    }

    /// Build a `lettre::Message` from an [`OutgoingMessage`].
    fn build_lettre_message(&self, msg: &OutgoingMessage) -> Result<Message> {
        let to_addr = match &msg.to {
            Recipient::Email(addr) => addr.clone(),
            other => {
                return Err(CommError::Unsupported(format!(
                    "SMTP provider requires an Email recipient; got: {other:?}"
                )))
            }
        };

        let subject = msg
            .subject
            .clone()
            .unwrap_or_else(|| "(no subject)".to_string());

        let builder = Message::builder()
            .from(
                self.from
                    .parse()
                    .map_err(|e| CommError::Provider(format!("invalid From address: {e}")))?,
            )
            .to(to_addr
                .parse()
                .map_err(|e| CommError::Provider(format!("invalid To address: {e}")))?)
            .subject(subject);

        let lettre_msg = match &msg.body {
            MessageBody::Html(html) => {
                // Build an alternative (plain + HTML) multipart body so that
                // text-only clients can still read the message.
                let plain_fallback = strip_html_tags(html);
                builder
                    .multipart(MultiPart::alternative_plain_html(
                        plain_fallback,
                        html.clone(),
                    ))
                    .map_err(|e| CommError::Provider(format!("build HTML message: {e}")))?
            }
            MessageBody::PlainText(text) | MessageBody::Markdown(text) => builder
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(text.clone()),
                )
                .map_err(|e| CommError::Provider(format!("build plain message: {e}")))?,
        };

        Ok(lettre_msg)
    }
}

/// Minimal HTML-strip helper — removes `<tag>` tokens so that the plain-text
/// alternative in a multi-part email is readable without a full HTML parser.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut inside_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => result.push(ch),
            _ => {}
        }
    }

    result
}

// ---------------------------------------------------------------------------
// MessageProvider implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl super::MessageProvider for SmtpProvider {
    fn provider_name(&self) -> &str {
        "smtp"
    }

    #[instrument(skip(self, msg), fields(provider = "smtp"))]
    async fn send_message(&self, msg: OutgoingMessage) -> Result<MessageReceipt> {
        debug!(to = ?msg.to, "sending SMTP message");

        let email = self.build_lettre_message(&msg)?;

        self.transport
            .send(email)
            .await
            .map_err(|e| CommError::Provider(format!("SMTP send failed: {e}")))?;

        Ok(MessageReceipt {
            message_id: format!(
                "smtp-{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        Err(CommError::Unsupported(
            "SMTP does not have the concept of channels".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MessageProvider;
    use crate::types::{MessageBody, OutgoingMessage, Recipient};

    #[tokio::test]
    async fn test_smtp_list_channels_unsupported() {
        // We cannot construct a real SmtpProvider without a live SMTP server,
        // but we CAN test list_channels by constructing a minimal provider
        // against localhost (the transport is lazy so this won't fail).
        let cfg = SmtpConfig {
            host: "127.0.0.1".to_string(),
            port: 25,
            username: "user".to_string(),
            password: "pass".to_string(),
            from_address: "test@example.com".to_string(),
            from_name: None,
        };

        let provider = SmtpProvider::new(cfg).expect("provider construction");
        let result = provider.list_channels().await;

        assert!(result.is_err());
        match result.unwrap_err() {
            CommError::Unsupported(_) => {}
            other => panic!("expected Unsupported, got: {other}"),
        }
    }

    #[test]
    fn test_smtp_build_message_html() {
        let cfg = SmtpConfig {
            host: "127.0.0.1".to_string(),
            port: 587,
            username: "u".to_string(),
            password: "p".to_string(),
            from_address: "sender@example.com".to_string(),
            from_name: Some("Sender Name".to_string()),
        };

        let provider = SmtpProvider::new(cfg).expect("construction");

        let msg = OutgoingMessage {
            to: Recipient::Email("recipient@example.com".to_string()),
            subject: Some("Test HTML email".to_string()),
            body: MessageBody::Html("<h1>Hello</h1><p>World</p>".to_string()),
            attachments: vec![],
        };

        let built = provider.build_lettre_message(&msg);
        assert!(built.is_ok(), "expected Ok, got: {:?}", built.unwrap_err());

        // Verify the message serializes to bytes without panicking.
        let email_bytes = built.unwrap().formatted();
        assert!(!email_bytes.is_empty());
    }

    #[test]
    fn test_strip_html_tags() {
        let html = "<h1>Hello</h1><p>World</p>";
        let plain = strip_html_tags(html);
        assert_eq!(plain, "HelloWorld");
    }
}
