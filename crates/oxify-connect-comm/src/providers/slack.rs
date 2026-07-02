use std::env;

use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::{
    errors::{CommError, Result},
    types::{Channel, MessageBody, MessageReceipt, OutgoingMessage, Recipient},
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Slack Bot-Token HTTP provider.
#[derive(Debug, Clone)]
pub struct SlackConfig {
    /// Bot token starting with `xoxb-`.
    pub token: String,
    /// Base URL for the Slack Web API.  Override in tests to point at a mock
    /// server (default: `https://slack.com/api`).
    pub base_url: String,
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            base_url: "https://slack.com/api".to_string(),
        }
    }
}

impl SlackConfig {
    /// Build configuration from the `SLACK_BOT_TOKEN` environment variable.
    ///
    /// Returns `Err(CommError::Auth)` when the variable is absent or empty.
    pub fn from_env() -> Result<Self> {
        let token = env::var("SLACK_BOT_TOKEN").map_err(|_| {
            CommError::Auth("SLACK_BOT_TOKEN environment variable is not set".to_string())
        })?;

        if token.trim().is_empty() {
            return Err(CommError::Auth(
                "SLACK_BOT_TOKEN environment variable is empty".to_string(),
            ));
        }

        Ok(Self {
            token,
            base_url: "https://slack.com/api".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Slack communication provider that uses the Slack Web API directly via
/// `oxihttp`.  No third-party Slack SDK is required.
pub struct SlackProvider {
    cfg: SlackConfig,
    http: oxihttp::HttpsClient,
}

impl SlackProvider {
    /// Create a new `SlackProvider` from the supplied configuration.
    pub fn new(cfg: SlackConfig) -> Result<Self> {
        let http = oxihttp::Client::builder()
            .with_tls()
            .build_https()
            .map_err(|e| CommError::Http(format!("failed to build HTTP client: {e}")))?;

        Ok(Self { cfg, http })
    }

    /// Create a new `SlackProvider` reading credentials from the environment.
    pub fn from_env() -> Result<Self> {
        Self::new(SlackConfig::from_env()?)
    }

    /// Extract the channel / user string from a [`Recipient`].
    fn recipient_to_channel(recipient: &Recipient) -> Result<String> {
        match recipient {
            Recipient::Channel(c) => Ok(c.clone()),
            Recipient::User(u) => Ok(u.clone()),
            Recipient::Email(e) => Err(CommError::Unsupported(format!(
                "Slack does not support email recipients; got: {e}"
            ))),
        }
    }

    /// Convert a [`MessageBody`] to the plain-text string that Slack expects in
    /// the `text` field.  Markdown is forwarded as-is (Slack renders mrkdwn).
    fn body_to_text(body: &MessageBody) -> String {
        match body {
            MessageBody::PlainText(t) => t.clone(),
            MessageBody::Markdown(t) => t.clone(),
            MessageBody::Html(t) => t.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal Slack API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PostMessageRequest<'a> {
    channel: &'a str,
    text: &'a str,
}

#[derive(Deserialize)]
struct PostMessageResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    ts: Option<String>,
}

#[derive(Deserialize)]
struct ConversationsListResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    channels: Vec<SlackChannel>,
}

#[derive(Deserialize)]
struct SlackChannel {
    id: String,
    name: String,
    #[serde(default)]
    is_private: bool,
}

// ---------------------------------------------------------------------------
// MessageProvider implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl super::MessageProvider for SlackProvider {
    fn provider_name(&self) -> &str {
        "slack"
    }

    #[instrument(skip(self, msg), fields(provider = "slack"))]
    async fn send_message(&self, msg: OutgoingMessage) -> Result<MessageReceipt> {
        let channel = Self::recipient_to_channel(&msg.to)?;
        let text = Self::body_to_text(&msg.body);

        let url = format!("{}/chat.postMessage", self.cfg.base_url);
        debug!(%url, %channel, "sending Slack message");

        let response = self
            .http
            .post(&url)?
            .bearer_token(&self.cfg.token)?
            .json(&PostMessageRequest {
                channel: &channel,
                text: &text,
            })?
            .send()
            .await
            .map_err(|e| CommError::Http(format!("POST chat.postMessage failed: {e}")))?;

        let status = response.status();
        let body: PostMessageResponse = response
            .body_json()
            .await
            .map_err(|e| CommError::Serialization(format!("deserialize chat.postMessage: {e}")))?;

        if !status.is_success() {
            return Err(CommError::Http(format!("Slack API returned HTTP {status}")));
        }

        if !body.ok {
            let slack_err = body.error.unwrap_or_else(|| "unknown".to_string());
            return Err(CommError::Provider(format!(
                "Slack chat.postMessage error: {slack_err}"
            )));
        }

        let ts = body
            .ts
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis().to_string());

        Ok(MessageReceipt {
            message_id: ts,
            timestamp: chrono::Utc::now(),
        })
    }

    #[instrument(skip(self), fields(provider = "slack"))]
    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let url = format!("{}/conversations.list", self.cfg.base_url);
        debug!(%url, "listing Slack channels");

        let response = self
            .http
            .get(&url)?
            .bearer_token(&self.cfg.token)?
            .send()
            .await
            .map_err(|e| CommError::Http(format!("GET conversations.list failed: {e}")))?;

        let status = response.status();
        let body: ConversationsListResponse = response.body_json().await.map_err(|e| {
            CommError::Serialization(format!("deserialize conversations.list: {e}"))
        })?;

        if !status.is_success() {
            return Err(CommError::Http(format!("Slack API returned HTTP {status}")));
        }

        if !body.ok {
            let slack_err = body.error.unwrap_or_else(|| "unknown".to_string());
            return Err(CommError::Provider(format!(
                "Slack conversations.list error: {slack_err}"
            )));
        }

        let channels = body
            .channels
            .into_iter()
            .map(|c| Channel {
                id: c.id,
                name: c.name,
                is_private: c.is_private,
            })
            .collect();

        Ok(channels)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::providers::MessageProvider;
    use crate::types::{MessageBody, OutgoingMessage, Recipient};

    fn provider_with_base_url(base_url: &str) -> SlackProvider {
        let cfg = SlackConfig {
            token: "xoxb-test-token".to_string(),
            base_url: base_url.to_string(),
        };
        SlackProvider::new(cfg).expect("provider construction")
    }

    #[tokio::test]
    async fn test_slack_list_channels_ok() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/conversations.list"))
            .and(header("Authorization", "Bearer xoxb-test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "channels": [
                    { "id": "C001", "name": "general",  "is_private": false },
                    { "id": "C002", "name": "random",   "is_private": false },
                    { "id": "C003", "name": "eng-leads","is_private": true  },
                ]
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let channels = provider.list_channels().await.expect("list_channels");

        assert_eq!(channels.len(), 3);
        assert_eq!(channels[0].id, "C001");
        assert_eq!(channels[0].name, "general");
        assert!(!channels[0].is_private);
        assert!(channels[2].is_private);
    }

    #[tokio::test]
    async fn test_slack_post_message_ok() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat.postMessage"))
            .and(header("Authorization", "Bearer xoxb-test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "ts": "1712345678.000100",
                "channel": "C001"
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage {
            to: Recipient::Channel("C001".to_string()),
            subject: None,
            body: MessageBody::PlainText("Hello from tests!".to_string()),
            attachments: vec![],
        };

        let receipt = provider.send_message(msg).await.expect("send_message");
        assert_eq!(receipt.message_id, "1712345678.000100");
    }

    #[tokio::test]
    async fn test_slack_post_message_error_response() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat.postMessage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": false,
                "error": "channel_not_found"
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage::simple(Recipient::Channel("C_MISSING".to_string()), "oops");

        let result = provider.send_message(msg).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("channel_not_found"),
            "unexpected error message: {err_str}"
        );
    }
}
