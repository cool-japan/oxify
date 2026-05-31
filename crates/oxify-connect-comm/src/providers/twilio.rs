use std::env;

use tracing::{debug, instrument};

use crate::{
    errors::{CommError, Result},
    types::{Channel, MessageBody, MessageReceipt, OutgoingMessage, Recipient},
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Twilio SMS provider.
#[derive(Debug, Clone)]
pub struct TwilioConfig {
    /// Twilio Account SID (username for HTTP Basic auth).
    pub account_sid: String,
    /// Twilio Auth Token (password for HTTP Basic auth).
    pub auth_token: String,
    /// The phone number to send messages from (E.164 format, e.g. `+15005550006`).
    pub from_number: String,
    /// Base URL for the Twilio REST API.  Override in tests to point at a mock
    /// server (default: `https://api.twilio.com`).
    pub base_url: String,
}

impl Default for TwilioConfig {
    fn default() -> Self {
        Self {
            account_sid: String::new(),
            auth_token: String::new(),
            from_number: String::new(),
            base_url: "https://api.twilio.com".to_string(),
        }
    }
}

impl TwilioConfig {
    /// Build configuration from environment variables.
    ///
    /// * `TWILIO_ACCOUNT_SID` — required; returns [`CommError::Auth`] when absent or empty.
    /// * `TWILIO_AUTH_TOKEN` — required; returns [`CommError::Auth`] when absent or empty.
    /// * `TWILIO_FROM_NUMBER` — required; returns [`CommError::Auth`] when absent or empty.
    pub fn from_env() -> Result<Self> {
        let account_sid = env::var("TWILIO_ACCOUNT_SID")
            .map_err(|_| CommError::Auth("TWILIO_ACCOUNT_SID not set".to_string()))?;

        if account_sid.trim().is_empty() {
            return Err(CommError::Auth("TWILIO_ACCOUNT_SID not set".to_string()));
        }

        let auth_token = env::var("TWILIO_AUTH_TOKEN")
            .map_err(|_| CommError::Auth("TWILIO_AUTH_TOKEN not set".to_string()))?;

        if auth_token.trim().is_empty() {
            return Err(CommError::Auth("TWILIO_AUTH_TOKEN not set".to_string()));
        }

        let from_number = env::var("TWILIO_FROM_NUMBER")
            .map_err(|_| CommError::Auth("TWILIO_FROM_NUMBER not set".to_string()))?;

        if from_number.trim().is_empty() {
            return Err(CommError::Auth("TWILIO_FROM_NUMBER not set".to_string()));
        }

        Ok(Self {
            account_sid,
            auth_token,
            from_number,
            base_url: "https://api.twilio.com".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Twilio SMS communication provider that uses the Twilio Messaging REST API
/// directly via `reqwest`.  No third-party Twilio SDK is required.
///
/// Authentication is performed via HTTP Basic auth with the Account SID as the
/// username and the Auth Token as the password, as required by the Twilio API.
pub struct TwilioProvider {
    cfg: TwilioConfig,
    http: reqwest::Client,
}

impl TwilioProvider {
    /// Create a new `TwilioProvider` from the supplied configuration.
    pub fn new(cfg: TwilioConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| CommError::Http(format!("failed to build HTTP client: {e}")))?;

        Ok(Self { cfg, http })
    }

    /// Create a new `TwilioProvider` reading credentials from the environment.
    pub fn from_env() -> Result<Self> {
        Self::new(TwilioConfig::from_env()?)
    }
}

// ---------------------------------------------------------------------------
// Internal Twilio API types
// ---------------------------------------------------------------------------

/// Form-encoded body for the Twilio Messages API.
/// Field names are capitalised to match the Twilio API specification.
#[derive(serde::Serialize)]
struct SmsFormBody<'a> {
    #[serde(rename = "From")]
    from: &'a str,
    #[serde(rename = "To")]
    to: &'a str,
    #[serde(rename = "Body")]
    body: &'a str,
}

/// Partial deserialisation of the Twilio 201 Created response body.
#[derive(serde::Deserialize)]
struct TwilioMessageResponse {
    sid: String,
}

// ---------------------------------------------------------------------------
// MessageProvider implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl super::MessageProvider for TwilioProvider {
    fn provider_name(&self) -> &str {
        "twilio"
    }

    #[instrument(skip(self, msg), fields(provider = "twilio"))]
    async fn send_message(&self, msg: OutgoingMessage) -> Result<MessageReceipt> {
        // Extract destination phone number from recipient.
        let to: String = match &msg.to {
            Recipient::User(phone) => {
                debug!(to = %phone, "sending Twilio SMS to user number");
                phone.clone()
            }
            Recipient::Channel(phone) => {
                debug!(to = %phone, "sending Twilio SMS to channel-style number");
                phone.clone()
            }
            Recipient::Email(e) => {
                return Err(CommError::Unsupported(format!(
                    "Twilio does not support email recipients; got: {e}"
                )));
            }
        };

        // Extract text from the message body — Twilio only handles plain text.
        let text: String = match &msg.body {
            MessageBody::PlainText(t) => t.clone(),
            MessageBody::Markdown(t) => t.clone(),
            MessageBody::Html(t) => t.clone(),
        };

        let url = format!(
            "{}/2010-04-01/Accounts/{}/Messages.json",
            self.cfg.base_url, self.cfg.account_sid
        );
        debug!(%url, %to, "posting Twilio SMS message");

        let response = self
            .http
            .post(&url)
            .basic_auth(&self.cfg.account_sid, Some(&self.cfg.auth_token))
            .form(&SmsFormBody {
                from: &self.cfg.from_number,
                to: &to,
                body: &text,
            })
            .send()
            .await
            .map_err(|e| CommError::Http(format!("POST Messages.json failed: {e}")))?;

        // Capture status BEFORE consuming the response body.
        let status = response.status();

        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(CommError::Http(format!("Twilio API {status}: {body_text}")));
        }

        let parsed: TwilioMessageResponse = response.json().await.map_err(|e| {
            CommError::Serialization(format!("deserialize Twilio Messages.json response: {e}"))
        })?;

        Ok(MessageReceipt {
            message_id: parsed.sid,
            timestamp: chrono::Utc::now(),
        })
    }

    #[instrument(skip(self), fields(provider = "twilio"))]
    async fn list_channels(&self) -> Result<Vec<Channel>> {
        Err(CommError::Unsupported(
            "Twilio SMS does not have the concept of channels".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{basic_auth, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::providers::MessageProvider;
    use crate::types::{MessageBody, OutgoingMessage, Recipient};

    fn provider_with_base_url(base_url: &str) -> TwilioProvider {
        let cfg = TwilioConfig {
            account_sid: "AC_test_sid".to_string(),
            auth_token: "test_auth_token".to_string(),
            from_number: "+15005550006".to_string(),
            base_url: base_url.to_string(),
        };
        TwilioProvider::new(cfg).expect("build TwilioProvider")
    }

    // -----------------------------------------------------------------------
    // test 1 — successful SMS send returns 201 with SID
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_send_sms_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/2010-04-01/Accounts/AC_test_sid/Messages.json"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "sid": "SM123",
                "status": "queued"
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage {
            to: Recipient::User("+15551234567".to_string()),
            subject: None,
            body: MessageBody::PlainText("Hello from Twilio test!".to_string()),
            attachments: vec![],
        };

        let receipt = provider.send_message(msg).await.expect("send_message");
        assert_eq!(receipt.message_id, "SM123");
    }

    // -----------------------------------------------------------------------
    // test 2 — basic auth credentials are forwarded correctly
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_send_sms_auth_header() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/2010-04-01/Accounts/AC_test_sid/Messages.json"))
            .and(basic_auth("AC_test_sid", "test_auth_token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "sid": "SM456",
                "status": "queued"
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage::simple(
            Recipient::User("+15551234567".to_string()),
            "Auth header test",
        );

        let receipt = provider
            .send_message(msg)
            .await
            .expect("send_message with basic auth");
        assert_eq!(receipt.message_id, "SM456");
    }

    // -----------------------------------------------------------------------
    // test 3 — HTTP 4xx response → CommError::Http containing status
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_send_sms_http_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/2010-04-01/Accounts/AC_test_sid/Messages.json"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "code": 21211,
                "message": "The 'To' number is not a valid phone number"
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let msg =
            OutgoingMessage::simple(Recipient::User("not-a-number".to_string()), "Should fail");

        let result = provider.send_message(msg).await;
        assert!(result.is_err());

        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("400"),
            "expected HTTP 400 in error message, got: {err_str}"
        );
    }

    // -----------------------------------------------------------------------
    // test 4 — Email recipient → CommError::Unsupported
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_send_sms_unsupported_email() {
        let provider = provider_with_base_url("http://localhost:9999");
        let msg = OutgoingMessage::simple(
            Recipient::Email("foo@bar.com".to_string()),
            "This should never be sent",
        );

        let result = provider.send_message(msg).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(err, CommError::Unsupported(_)),
            "expected Unsupported error, got: {err}"
        );
        assert!(
            err.to_string().contains("foo@bar.com"),
            "error should mention the email address: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // test 5 — list_channels always returns Unsupported
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_channels_unsupported() {
        let provider = TwilioProvider::new(TwilioConfig::default()).expect("build TwilioProvider");

        let result = provider.list_channels().await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(err, CommError::Unsupported(_)),
            "expected Unsupported error, got: {err}"
        );
        assert!(
            err.to_string().contains("channels"),
            "error should mention channels: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // test 6 — provider_name returns "twilio"
    // -----------------------------------------------------------------------

    #[test]
    fn test_provider_name() {
        let provider = TwilioProvider::new(TwilioConfig::default()).expect("build TwilioProvider");
        assert_eq!(provider.provider_name(), "twilio");
    }

    // -----------------------------------------------------------------------
    // test 7 — from_env() without TWILIO_ACCOUNT_SID → Auth error
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_from_env_missing_sid() {
        unsafe {
            std::env::remove_var("TWILIO_ACCOUNT_SID");
        }

        let result = TwilioConfig::from_env();
        assert!(
            result.is_err(),
            "expected error when TWILIO_ACCOUNT_SID is not set"
        );

        let err = result.unwrap_err();
        assert!(
            matches!(err, CommError::Auth(_)),
            "expected Auth error, got: {err}"
        );
        assert!(
            err.to_string().contains("TWILIO_ACCOUNT_SID"),
            "error should mention TWILIO_ACCOUNT_SID: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // test 8 — Default::default() sets base_url correctly
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_default_base_url() {
        let cfg = TwilioConfig::default();
        assert_eq!(cfg.base_url, "https://api.twilio.com");
        assert!(cfg.account_sid.is_empty());
        assert!(cfg.auth_token.is_empty());
        assert!(cfg.from_number.is_empty());
    }

    // -----------------------------------------------------------------------
    // helper — header_exists is imported; verify compilation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_authorization_header_present() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/2010-04-01/Accounts/AC_test_sid/Messages.json"))
            .and(header_exists("authorization"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "sid": "SM789",
                "status": "queued"
            })))
            .mount(&server)
            .await;

        let provider = provider_with_base_url(&server.uri());
        let msg = OutgoingMessage::simple(
            Recipient::Channel("+15559876543".to_string()),
            "Channel-style phone number",
        );

        let receipt = provider
            .send_message(msg)
            .await
            .expect("send_message with channel recipient");
        assert_eq!(receipt.message_id, "SM789");
    }
}
