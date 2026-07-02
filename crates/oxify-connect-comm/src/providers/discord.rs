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

/// Configuration for the Discord Bot-Token HTTP provider.
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    /// Bot token (without `Bot ` prefix — the provider adds it automatically).
    pub token: String,
    /// Guild (server) ID required for [`DiscordProvider::list_channels`].
    /// When `None`, `list_channels` returns [`CommError::Unsupported`].
    pub guild_id: Option<String>,
    /// Base URL for the Discord REST API.  Override in tests to point at a mock
    /// server (default: `https://discord.com/api/v10`).
    pub base_url: String,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            guild_id: None,
            base_url: "https://discord.com/api/v10".to_string(),
        }
    }
}

impl DiscordConfig {
    /// Build configuration from environment variables.
    ///
    /// * `DISCORD_BOT_TOKEN` — required; returns [`CommError::Auth`] when
    ///   absent or empty.
    /// * `DISCORD_GUILD_ID` — optional; when present, enables
    ///   [`DiscordProvider::list_channels`].
    pub fn from_env() -> Result<Self> {
        let token = env::var("DISCORD_BOT_TOKEN").map_err(|_| {
            CommError::Auth("DISCORD_BOT_TOKEN environment variable is not set".to_string())
        })?;

        if token.trim().is_empty() {
            return Err(CommError::Auth(
                "DISCORD_BOT_TOKEN environment variable is empty".to_string(),
            ));
        }

        let guild_id = env::var("DISCORD_GUILD_ID")
            .ok()
            .filter(|s| !s.trim().is_empty());

        Ok(Self {
            token,
            guild_id,
            base_url: "https://discord.com/api/v10".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Discord communication provider that uses the Discord REST API directly via
/// `oxihttp`.  No third-party Discord SDK is required.
///
/// All requests are authenticated with a Bot token supplied in the
/// `Authorization: Bot <token>` header as required by the Discord API.
pub struct DiscordProvider {
    cfg: DiscordConfig,
    http: oxihttp::HttpsClient,
}

impl DiscordProvider {
    /// Create a new `DiscordProvider` from the supplied configuration.
    pub fn new(cfg: DiscordConfig) -> Result<Self> {
        let http = oxihttp::Client::builder()
            .with_tls()
            .build_https()
            .map_err(|e| CommError::Http(format!("failed to build HTTP client: {e}")))?;

        Ok(Self { cfg, http })
    }

    /// Create a new `DiscordProvider` reading credentials from the environment.
    pub fn from_env() -> Result<Self> {
        Self::new(DiscordConfig::from_env()?)
    }

    /// Convert a [`MessageBody`] to the plain-text / markdown string that
    /// Discord expects in the `content` field.  Discord renders markdown
    /// natively; HTML is forwarded as-is to avoid stripping complexity.
    fn body_to_content(body: &MessageBody) -> String {
        match body {
            MessageBody::PlainText(t) => t.clone(),
            MessageBody::Markdown(t) => t.clone(),
            MessageBody::Html(t) => t.clone(),
        }
    }

    /// Return the `Authorization: Bot <token>` header value.
    fn auth_header(&self) -> String {
        format!("Bot {}", self.cfg.token)
    }
}

// ---------------------------------------------------------------------------
// Internal Discord API types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DiscordChannel {
    id: String,
    name: String,
    #[serde(rename = "type")]
    channel_type: u8,
}

#[derive(Deserialize)]
struct DiscordMessage {
    id: String,
    #[allow(dead_code)]
    channel_id: String,
}

#[derive(Deserialize)]
struct DiscordDmChannel {
    id: String,
}

#[derive(Serialize)]
struct SendMessageBody<'a> {
    content: &'a str,
}

#[derive(Serialize)]
struct CreateDmBody<'a> {
    recipient_id: &'a str,
}

// ---------------------------------------------------------------------------
// Channel type constants (Discord API)
// ---------------------------------------------------------------------------

/// `GUILD_TEXT` channel type.
const DISCORD_CHANNEL_TYPE_GUILD_TEXT: u8 = 0;
/// `GUILD_ANNOUNCEMENT` channel type (formerly News).
const DISCORD_CHANNEL_TYPE_GUILD_ANNOUNCEMENT: u8 = 5;

// ---------------------------------------------------------------------------
// MessageProvider implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl super::MessageProvider for DiscordProvider {
    fn provider_name(&self) -> &str {
        "discord"
    }

    #[instrument(skip(self, msg), fields(provider = "discord"))]
    async fn send_message(&self, msg: OutgoingMessage) -> Result<MessageReceipt> {
        let content = Self::body_to_content(&msg.body);

        let channel_id: String = match &msg.to {
            Recipient::Channel(id) => {
                debug!(channel_id = %id, "sending Discord message to channel");
                id.clone()
            }
            Recipient::User(user_id) => {
                // Discord requires creating a DM channel first.
                let dm_url = format!("{}/users/@me/channels", self.cfg.base_url);
                debug!(user_id = %user_id, %dm_url, "creating Discord DM channel");

                let dm_response = self
                    .http
                    .post(&dm_url)?
                    .header("Authorization", &self.auth_header())?
                    .json(&CreateDmBody {
                        recipient_id: user_id,
                    })?
                    .send()
                    .await
                    .map_err(|e| CommError::Http(e.to_string()))?;

                let dm_status = dm_response.status();
                if !dm_status.is_success() {
                    let body_text = dm_response.body_text().await.unwrap_or_default();
                    return Err(CommError::Http(format!(
                        "Discord API {dm_status}: {body_text}"
                    )));
                }

                let dm_channel: DiscordDmChannel = dm_response.body_json().await.map_err(|e| {
                    CommError::Serialization(format!("deserialize DM channel response: {e}"))
                })?;

                debug!(dm_channel_id = %dm_channel.id, "DM channel created");
                dm_channel.id
            }
            Recipient::Email(e) => {
                return Err(CommError::Unsupported(format!(
                    "Discord does not support email recipients; got: {e}"
                )));
            }
        };

        let url = format!("{}/channels/{channel_id}/messages", self.cfg.base_url);
        debug!(%url, "posting Discord message");

        let response = self
            .http
            .post(&url)?
            .header("Authorization", &self.auth_header())?
            .json(&SendMessageBody { content: &content })?
            .send()
            .await
            .map_err(|e| CommError::Http(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.body_text().await.unwrap_or_default();
            return Err(CommError::Http(format!(
                "Discord API {status}: {body_text}"
            )));
        }

        let discord_msg: DiscordMessage = response.body_json().await.map_err(|e| {
            CommError::Serialization(format!("deserialize Discord message response: {e}"))
        })?;

        Ok(MessageReceipt {
            message_id: discord_msg.id,
            timestamp: chrono::Utc::now(),
        })
    }

    #[instrument(skip(self), fields(provider = "discord"))]
    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let guild_id = self.cfg.guild_id.as_ref().ok_or_else(|| {
            CommError::Unsupported("list_channels requires DISCORD_GUILD_ID to be set".to_string())
        })?;

        let url = format!("{}/guilds/{guild_id}/channels", self.cfg.base_url);
        debug!(%url, %guild_id, "listing Discord channels");

        let response = self
            .http
            .get(&url)?
            .header("Authorization", &self.auth_header())?
            .send()
            .await
            .map_err(|e| CommError::Http(format!("GET guilds/{guild_id}/channels failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.body_text().await.unwrap_or_default();
            return Err(CommError::Http(format!(
                "Discord API {status}: {body_text}"
            )));
        }

        let discord_channels: Vec<DiscordChannel> = response.body_json().await.map_err(|e| {
            CommError::Serialization(format!("deserialize guilds/{guild_id}/channels: {e}"))
        })?;

        let channels = discord_channels
            .into_iter()
            .filter(|c| {
                c.channel_type == DISCORD_CHANNEL_TYPE_GUILD_TEXT
                    || c.channel_type == DISCORD_CHANNEL_TYPE_GUILD_ANNOUNCEMENT
            })
            .map(|c| Channel {
                id: c.id,
                name: c.name,
                is_private: false,
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

    fn provider_with_base_url(base_url: &str) -> DiscordProvider {
        let cfg = DiscordConfig {
            token: "test-bot-token".to_string(),
            guild_id: Some("GUILD001".to_string()),
            base_url: base_url.to_string(),
        };
        DiscordProvider::new(cfg).expect("provider construction")
    }

    fn provider_no_guild(base_url: &str) -> DiscordProvider {
        let cfg = DiscordConfig {
            token: "test-bot-token".to_string(),
            guild_id: None,
            base_url: base_url.to_string(),
        };
        DiscordProvider::new(cfg).expect("provider construction")
    }

    // -----------------------------------------------------------------------
    // test 1 — list_channels filters by type
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_discord_list_channels_ok() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/guilds/GUILD001/channels"))
            .and(header("Authorization", "Bot test-bot-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": "C001", "name": "general",       "type": 0 },
                { "id": "C002", "name": "voice-lounge",  "type": 2 },
                { "id": "C003", "name": "announcements", "type": 5 },
            ])))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let channels = provider.list_channels().await.expect("list_channels");

        // type 2 (GUILD_VOICE) should be filtered out; types 0 and 5 are kept
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].id, "C001");
        assert_eq!(channels[0].name, "general");
        assert!(!channels[0].is_private);
        assert_eq!(channels[1].id, "C003");
        assert_eq!(channels[1].name, "announcements");
    }

    // -----------------------------------------------------------------------
    // test 2 — list_channels without guild_id → Unsupported
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_discord_list_channels_no_guild_id() {
        let server = MockServer::start().await;
        let provider = provider_no_guild(&server.uri());

        let result = provider.list_channels().await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(err, CommError::Unsupported(_)),
            "expected Unsupported, got: {err}"
        );
        assert!(err.to_string().contains("DISCORD_GUILD_ID"));
    }

    // -----------------------------------------------------------------------
    // test 3 — send_message to a channel succeeds
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_discord_send_channel_message_ok() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/channels/C001/messages"))
            .and(header("Authorization", "Bot test-bot-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "1234",
                "channel_id": "C001"
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
        assert_eq!(receipt.message_id, "1234");
    }

    // -----------------------------------------------------------------------
    // test 4 — non-2xx response → CommError::Http containing status
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_discord_send_message_api_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/channels/CBAD/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "code": 10003,
                "message": "Unknown Channel"
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage::simple(Recipient::Channel("CBAD".to_string()), "oops");

        let result = provider.send_message(msg).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("400"),
            "expected HTTP 400 in error, got: {err_str}"
        );
    }

    // -----------------------------------------------------------------------
    // test 5 — DM flow: create DM channel then post message
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_discord_send_dm_ok() {
        let server = MockServer::start().await;

        // Step 1: create DM channel
        Mock::given(method("POST"))
            .and(path("/users/@me/channels"))
            .and(header("Authorization", "Bot test-bot-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "DM001"
            })))
            .mount(&server)
            .await;

        // Step 2: post to DM channel
        Mock::given(method("POST"))
            .and(path("/channels/DM001/messages"))
            .and(header("Authorization", "Bot test-bot-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "MSG999",
                "channel_id": "DM001"
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage {
            to: Recipient::User("USER_42".to_string()),
            subject: None,
            body: MessageBody::PlainText("Hey there!".to_string()),
            attachments: vec![],
        };

        let receipt = provider.send_message(msg).await.expect("send_message DM");
        assert_eq!(receipt.message_id, "MSG999");
    }

    // -----------------------------------------------------------------------
    // test 6 — Email recipient → Unsupported error
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_discord_email_recipient_unsupported() {
        let server = MockServer::start().await;
        let provider = provider_with_base_url(&server.uri());

        let msg = OutgoingMessage::simple(
            Recipient::Email("alice@example.com".to_string()),
            "not going anywhere",
        );

        let result = provider.send_message(msg).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(err, CommError::Unsupported(_)),
            "expected Unsupported, got: {err}"
        );
        assert!(err.to_string().contains("alice@example.com"));
    }

    // -----------------------------------------------------------------------
    // test 7 — Default::default() sets base_url correctly
    // -----------------------------------------------------------------------

    #[test]
    fn test_discord_config_default_base_url() {
        let cfg = DiscordConfig::default();
        assert_eq!(cfg.base_url, "https://discord.com/api/v10");
        assert!(cfg.token.is_empty());
        assert!(cfg.guild_id.is_none());
    }

    // -----------------------------------------------------------------------
    // test 8 — from_env() without DISCORD_BOT_TOKEN → Auth error
    // -----------------------------------------------------------------------

    #[test]
    fn test_discord_config_from_env_missing_token() {
        // Ensure the variable is not set in this test's environment.
        // We use std::env::remove_var in a scoped manner.  Tests run in the
        // same process so we guard with a mutex-free approach: simply remove
        // the variable; other tests do not rely on it being present.
        unsafe {
            env::remove_var("DISCORD_BOT_TOKEN");
        }

        let result = DiscordConfig::from_env();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(err, CommError::Auth(_)),
            "expected Auth error, got: {err}"
        );
        assert!(
            err.to_string().contains("DISCORD_BOT_TOKEN"),
            "error should mention the variable name: {err}"
        );
    }
}
