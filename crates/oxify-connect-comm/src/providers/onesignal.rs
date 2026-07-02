use std::env;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::{
    errors::{CommError, Result},
    types::{Channel, MessageBody, MessageReceipt, OutgoingMessage, Recipient},
};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OneSignalConfig {
    pub app_id: String,
    pub rest_api_key: String,
    pub base_url: String,
}

impl Default for OneSignalConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            rest_api_key: String::new(),
            base_url: "https://onesignal.com/api/v1".to_string(),
        }
    }
}

impl OneSignalConfig {
    pub fn from_env() -> Result<Self> {
        let app_id = env::var("ONESIGNAL_APP_ID").map_err(|_| {
            CommError::Auth("ONESIGNAL_APP_ID environment variable is not set".to_string())
        })?;
        if app_id.trim().is_empty() {
            return Err(CommError::Auth("ONESIGNAL_APP_ID is empty".to_string()));
        }
        let rest_api_key = env::var("ONESIGNAL_REST_API_KEY").map_err(|_| {
            CommError::Auth("ONESIGNAL_REST_API_KEY environment variable is not set".to_string())
        })?;
        if rest_api_key.trim().is_empty() {
            return Err(CommError::Auth(
                "ONESIGNAL_REST_API_KEY is empty".to_string(),
            ));
        }
        Ok(Self {
            app_id,
            rest_api_key,
            base_url: "https://onesignal.com/api/v1".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct OneSignalProvider {
    cfg: OneSignalConfig,
    http: oxihttp::HttpsClient,
}

impl OneSignalProvider {
    pub fn new(cfg: OneSignalConfig) -> Result<Self> {
        let http = oxihttp::Client::builder()
            .with_tls()
            .build_https()
            .map_err(|e| CommError::Http(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { cfg, http })
    }

    pub fn from_env() -> Result<Self> {
        Self::new(OneSignalConfig::from_env()?)
    }
}

// ---------------------------------------------------------------------------
// Private serde types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct NotificationBody<'a> {
    app_id: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    include_player_ids: Vec<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    included_segments: Vec<&'a str>,
    contents: std::collections::HashMap<&'static str, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headings: Option<std::collections::HashMap<&'static str, String>>,
}

#[derive(Deserialize)]
struct NotificationResponse {
    id: String,
    #[serde(default)]
    errors: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// MessageProvider impl
// ---------------------------------------------------------------------------

#[async_trait]
impl super::MessageProvider for OneSignalProvider {
    fn provider_name(&self) -> &str {
        "onesignal"
    }

    #[instrument(skip(self, msg), fields(provider = "onesignal"))]
    async fn send_message(&self, msg: OutgoingMessage) -> Result<MessageReceipt> {
        let (include_player_ids, included_segments) = match &msg.to {
            Recipient::User(id) => {
                debug!(player_id = %id, "sending OneSignal push to player");
                (vec![id.as_str()], vec![])
            }
            Recipient::Channel(seg) => {
                debug!(segment = %seg, "sending OneSignal push to segment");
                (vec![], vec![seg.as_str()])
            }
            Recipient::Email(e) => {
                return Err(CommError::Unsupported(format!(
                    "OneSignal does not support email recipients; got: {e}"
                )));
            }
        };

        let text = match &msg.body {
            MessageBody::PlainText(t) | MessageBody::Markdown(t) | MessageBody::Html(t) => {
                t.clone()
            }
        };

        let mut contents = std::collections::HashMap::new();
        contents.insert("en", text);

        let headings = msg.subject.map(|s| {
            let mut m = std::collections::HashMap::new();
            m.insert("en", s);
            m
        });

        let body = NotificationBody {
            app_id: &self.cfg.app_id,
            include_player_ids,
            included_segments,
            contents,
            headings,
        };

        let url = format!("{}/notifications", self.cfg.base_url);
        let response = self
            .http
            .post(&url)?
            .header("Content-Type", "application/json")?
            .header("Authorization", &format!("Basic {}", self.cfg.rest_api_key))?
            .json(&body)?
            .send()
            .await
            .map_err(|e| CommError::Http(format!("POST notifications failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.body_text().await.unwrap_or_default();
            return Err(CommError::Http(format!(
                "OneSignal API {status}: {body_text}"
            )));
        }

        let parsed = response
            .body_json::<NotificationResponse>()
            .await
            .map_err(|e| {
                CommError::Serialization(format!("deserialize OneSignal response: {e}"))
            })?;

        if let Some(errors) = &parsed.errors {
            let is_empty = match errors {
                serde_json::Value::Array(arr) => arr.is_empty(),
                serde_json::Value::Object(map) => map.is_empty(),
                serde_json::Value::Null => true,
                _ => false,
            };
            if !is_empty {
                return Err(CommError::Provider(format!("OneSignal errors: {errors}")));
            }
        }

        Ok(MessageReceipt {
            message_id: parsed.id,
            timestamp: chrono::Utc::now(),
        })
    }

    #[instrument(skip(self), fields(provider = "onesignal"))]
    async fn list_channels(&self) -> Result<Vec<Channel>> {
        Err(CommError::Unsupported(
            "OneSignal does not have the concept of channels; use segments via Recipient::Channel"
                .to_string(),
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
    use wiremock::matchers::{header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn provider_with_base_url(base_url: &str) -> OneSignalProvider {
        let cfg = OneSignalConfig {
            app_id: "test_app_id".to_string(),
            rest_api_key: "test_rest_key".to_string(),
            base_url: base_url.to_string(),
        };
        OneSignalProvider::new(cfg).expect("build OneSignalProvider")
    }

    #[tokio::test]
    async fn test_send_push_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notifications"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "notif-uuid-123", "recipients": 1})),
            )
            .mount(&server)
            .await;
        let provider = provider_with_base_url(&server.uri());
        let msg =
            OutgoingMessage::simple(Recipient::User("player-device-001".to_string()), "Hello!");
        let receipt = provider.send_message(msg).await.unwrap();
        assert_eq!(receipt.message_id, "notif-uuid-123");
    }

    #[tokio::test]
    async fn test_auth_header_present() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notifications"))
            .and(header_exists("authorization"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "auth-notif", "recipients": 1})),
            )
            .mount(&server)
            .await;
        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage::simple(Recipient::Channel("All".to_string()), "Broadcast!");
        let result = provider.send_message(msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_push_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notifications"))
            .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
            .mount(&server)
            .await;
        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage::simple(Recipient::User("x".to_string()), "test");
        let result = provider.send_message(msg).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("400"),
            "Expected 400 in error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_send_push_api_errors_in_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "",
                "recipients": 0,
                "errors": ["All included players are not subscribed"]
            })))
            .mount(&server)
            .await;
        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage::simple(Recipient::User("bad-player".to_string()), "test");
        let result = provider.send_message(msg).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CommError::Provider(_)));
    }

    #[tokio::test]
    async fn test_send_push_api_errors_object_in_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "",
                "recipients": 0,
                "errors": {"invalid_player_ids": ["bad-id-1"]}
            })))
            .mount(&server)
            .await;
        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage::simple(Recipient::User("bad-id-1".to_string()), "test");
        let result = provider.send_message(msg).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CommError::Provider(_)));
    }

    #[tokio::test]
    async fn test_message_with_subject_sends_headings() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notifications"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "heading-notif", "recipients": 1})),
            )
            .mount(&server)
            .await;
        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage {
            to: Recipient::User("player-123".to_string()),
            subject: Some("Important Alert".to_string()),
            body: MessageBody::PlainText("Check this out".to_string()),
            attachments: vec![],
        };
        let receipt = provider.send_message(msg).await.unwrap();
        assert_eq!(receipt.message_id, "heading-notif");
    }

    #[tokio::test]
    async fn test_email_recipient_unsupported() {
        let provider = OneSignalProvider::new(OneSignalConfig::default()).unwrap();
        let msg = OutgoingMessage::simple(Recipient::Email("user@example.com".to_string()), "hi");
        let result = provider.send_message(msg).await;
        assert!(matches!(result.unwrap_err(), CommError::Unsupported(_)));
    }

    #[tokio::test]
    async fn test_list_channels_unsupported() {
        let provider = OneSignalProvider::new(OneSignalConfig::default()).unwrap();
        let result = provider.list_channels().await;
        assert!(matches!(result.unwrap_err(), CommError::Unsupported(_)));
    }

    #[test]
    fn test_provider_name() {
        let provider = OneSignalProvider::new(OneSignalConfig::default()).unwrap();
        assert_eq!(provider.provider_name(), "onesignal");
    }

    #[test]
    fn test_from_env_missing_app_id() {
        unsafe {
            std::env::remove_var("ONESIGNAL_APP_ID");
        }
        let result = OneSignalConfig::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CommError::Auth(_)));
    }

    #[test]
    fn test_config_default_base_url() {
        let cfg = OneSignalConfig::default();
        assert_eq!(cfg.base_url, "https://onesignal.com/api/v1");
        assert!(cfg.app_id.is_empty());
        assert!(cfg.rest_api_key.is_empty());
    }
}
