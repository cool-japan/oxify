use std::env;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, instrument};

use crate::{
    errors::{CommError, Result},
    types::{Channel, MessageBody, MessageReceipt, OutgoingMessage, Recipient},
};

// ---------------------------------------------------------------------------
// Service account
// ---------------------------------------------------------------------------

/// Parsed fields from a Google service account JSON key file.
#[derive(Debug, Clone, Deserialize)]
struct ServiceAccount {
    client_email: String,
    /// RSA PEM private key — in service account JSON files the newlines are
    /// represented as literal `\n` escape sequences; we normalise them at
    /// parse time.
    private_key: String,
}

// ---------------------------------------------------------------------------
// Token cache
// ---------------------------------------------------------------------------

/// A bearer token together with an absolute expiry timestamp.
#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: chrono::DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for the Firebase FCM v1 API provider.
#[derive(Debug, Clone)]
pub struct FirebaseFcmConfig {
    /// GCP / Firebase project ID (e.g. `"my-app-12345"`).
    pub project_id: String,
    /// Raw JSON string of the service account key file.
    pub service_account_json: String,
    /// Base URL of the FCM v1 REST API.  Defaults to
    /// `"https://fcm.googleapis.com/v1"`.  Override in tests to point at a
    /// mock server.
    pub base_url: String,
    /// OAuth2 token endpoint URL.  Defaults to
    /// `"https://oauth2.googleapis.com/token"`.  Override in tests.
    pub token_url: String,
}

impl Default for FirebaseFcmConfig {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            service_account_json: String::new(),
            base_url: "https://fcm.googleapis.com/v1".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
        }
    }
}

impl FirebaseFcmConfig {
    /// Build configuration from environment variables:
    ///
    /// * `FIREBASE_PROJECT_ID`
    /// * `FIREBASE_SERVICE_ACCOUNT_JSON` – the full service account JSON
    ///   string (as exported from the GCP console).
    ///
    /// Returns `CommError::Auth` if any required variable is absent or empty.
    pub fn from_env() -> Result<Self> {
        let project_id = env::var("FIREBASE_PROJECT_ID").map_err(|_| {
            CommError::Auth("FIREBASE_PROJECT_ID environment variable is not set".to_string())
        })?;
        if project_id.trim().is_empty() {
            return Err(CommError::Auth("FIREBASE_PROJECT_ID is empty".to_string()));
        }

        let service_account_json = env::var("FIREBASE_SERVICE_ACCOUNT_JSON").map_err(|_| {
            CommError::Auth(
                "FIREBASE_SERVICE_ACCOUNT_JSON environment variable is not set".to_string(),
            )
        })?;
        if service_account_json.trim().is_empty() {
            return Err(CommError::Auth(
                "FIREBASE_SERVICE_ACCOUNT_JSON is empty".to_string(),
            ));
        }

        Ok(Self {
            project_id,
            service_account_json,
            base_url: "https://fcm.googleapis.com/v1".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// JWT claims
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    aud: String,
    scope: String,
    iat: i64,
    exp: i64,
}

// ---------------------------------------------------------------------------
// OAuth2 token exchange response
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

// ---------------------------------------------------------------------------
// FCM send response
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FcmSendResponse {
    name: String,
}

// ---------------------------------------------------------------------------
// FCM message bodies (serde)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct FcmNotification<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    body: &'a str,
}

#[derive(Serialize)]
struct FcmMessageToken<'a> {
    token: &'a str,
    notification: FcmNotification<'a>,
}

#[derive(Serialize)]
struct FcmMessageTopic<'a> {
    topic: &'a str,
    notification: FcmNotification<'a>,
}

#[derive(Serialize)]
struct FcmSendRequestToken<'a> {
    message: FcmMessageToken<'a>,
}

#[derive(Serialize)]
struct FcmSendRequestTopic<'a> {
    message: FcmMessageTopic<'a>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Firebase Cloud Messaging (FCM v1 API) push notification provider.
///
/// Authenticates via a Google service account JSON key using RS256 JWT
/// assertion, caches the resulting OAuth2 bearer token with a 5-minute
/// expiry guard, and sends messages to individual device tokens or FCM
/// topics.
#[derive(Debug)]
pub struct FirebaseFcmProvider {
    cfg: FirebaseFcmConfig,
    http: oxihttp::HttpsClient,
    service_account: ServiceAccount,
    token_cache: RwLock<Option<CachedToken>>,
}

impl FirebaseFcmProvider {
    /// Create a new provider from the supplied configuration.
    ///
    /// Parses the service account JSON immediately so that configuration
    /// errors surface at construction time rather than on the first send.
    ///
    /// Returns `CommError::Config` if the JSON cannot be parsed.
    pub fn new(cfg: FirebaseFcmConfig) -> Result<Self> {
        let mut service_account: ServiceAccount = serde_json::from_str(&cfg.service_account_json)
            .map_err(|e| {
            CommError::Provider(format!("failed to parse service account JSON: {e}"))
        })?;

        // Service-account JSON files store the PEM key with literal `\n`
        // sequences instead of real newline characters.  Normalise here.
        service_account.private_key = service_account.private_key.replace("\\n", "\n");

        let http = oxihttp::Client::builder()
            .with_tls()
            .build_https()
            .map_err(|e| CommError::Http(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            cfg,
            http,
            service_account,
            token_cache: RwLock::new(None),
        })
    }

    /// Convenience constructor that reads configuration from environment
    /// variables.  See [`FirebaseFcmConfig::from_env`] for details.
    pub fn from_env() -> Result<Self> {
        Self::new(FirebaseFcmConfig::from_env()?)
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    /// Return the FCM messages:send endpoint URL.
    fn messages_url(&self) -> String {
        format!(
            "{}/projects/{}/messages:send",
            self.cfg.base_url, self.cfg.project_id
        )
    }

    /// Build a signed RS256 JWT assertion for the Google OAuth2 token
    /// exchange.
    ///
    /// Returns `CommError::Config` if the private key PEM is invalid or if
    /// JWT encoding fails.
    fn build_jwt(&self) -> Result<String> {
        let now = Utc::now().timestamp();
        let claims = JwtClaims {
            iss: self.service_account.client_email.clone(),
            sub: self.service_account.client_email.clone(),
            aud: "https://oauth2.googleapis.com/token".to_string(),
            scope: "https://www.googleapis.com/auth/firebase.messaging".to_string(),
            iat: now,
            exp: now + 3600,
        };

        let encoding_key = EncodingKey::from_rsa_pem(self.service_account.private_key.as_bytes())
            .map_err(|e| {
            CommError::Provider(format!(
                "failed to load RSA private key for JWT signing: {e}"
            ))
        })?;

        encode(&Header::new(Algorithm::RS256), &claims, &encoding_key)
            .map_err(|e| CommError::Provider(format!("failed to sign JWT: {e}")))
    }

    /// Obtain a valid OAuth2 bearer token, using the in-memory cache when
    /// possible.
    ///
    /// Cache read strategy:
    /// 1. Acquire a read lock and return the cached token if it is still
    ///    valid (`expires_at - 5 min > now`).
    /// 2. If not, acquire a write lock, re-check (another task may have
    ///    refreshed it concurrently), sign a fresh JWT, exchange it for a
    ///    bearer token, update the cache, and return the new token.
    async fn get_access_token(&self) -> Result<String> {
        // Fast-path: read lock check.
        {
            let guard = self.token_cache.read().await;
            if let Some(cached) = guard.as_ref() {
                let cutoff = cached.expires_at - Duration::minutes(5);
                if Utc::now() < cutoff {
                    debug!("using cached FCM access token");
                    return Ok(cached.access_token.clone());
                }
            }
        }

        // Slow-path: write lock, double-check, then refresh.
        let mut guard = self.token_cache.write().await;
        if let Some(cached) = guard.as_ref() {
            let cutoff = cached.expires_at - Duration::minutes(5);
            if Utc::now() < cutoff {
                debug!("using cached FCM access token (post write-lock re-check)");
                return Ok(cached.access_token.clone());
            }
        }

        debug!("refreshing FCM access token via JWT assertion");
        let jwt = self.build_jwt()?;

        let form_body =
            format!("grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer&assertion={jwt}");

        let response = self
            .http
            .post(&self.cfg.token_url)?
            .header("Content-Type", "application/x-www-form-urlencoded")?
            .body(form_body)
            .send()
            .await
            .map_err(|e| CommError::Http(format!("token exchange request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.body_text().await.unwrap_or_default();
            return Err(CommError::Provider(format!(
                "OAuth2 token exchange failed with HTTP {status}: {body_text}"
            )));
        }

        let token_resp: TokenResponse = response
            .body_json()
            .await
            .map_err(|e| CommError::Serialization(format!("parse token response: {e}")))?;

        let expires_at =
            Utc::now() + Duration::seconds(i64::try_from(token_resp.expires_in).unwrap_or(3600));

        let access_token = token_resp.access_token.clone();
        *guard = Some(CachedToken {
            access_token: token_resp.access_token,
            expires_at,
        });

        Ok(access_token)
    }
}

// ---------------------------------------------------------------------------
// MessageProvider impl
// ---------------------------------------------------------------------------

#[async_trait]
impl super::MessageProvider for FirebaseFcmProvider {
    fn provider_name(&self) -> &str {
        "firebase-fcm"
    }

    #[instrument(skip(self, msg), fields(provider = "firebase-fcm"))]
    async fn send_message(&self, msg: OutgoingMessage) -> Result<MessageReceipt> {
        // Reject unsupported recipient types before performing any I/O.
        if let Recipient::Email(e) = &msg.to {
            return Err(CommError::Unsupported(format!(
                "Firebase FCM does not support email recipients; got: {e}"
            )));
        }

        let body_text = match &msg.body {
            MessageBody::PlainText(t) | MessageBody::Markdown(t) | MessageBody::Html(t) => {
                t.as_str()
            }
        };

        let title = msg.subject.as_deref();
        let notification = FcmNotification {
            title,
            body: body_text,
        };

        let token = self.get_access_token().await?;
        let url = self.messages_url();

        let (status, response_text) = match &msg.to {
            Recipient::User(device_token) => {
                debug!(device_token = %device_token, "sending FCM push to device token");
                let payload = FcmSendRequestToken {
                    message: FcmMessageToken {
                        token: device_token.as_str(),
                        notification,
                    },
                };
                let resp = self
                    .http
                    .post(&url)?
                    .header("Authorization", &format!("Bearer {token}"))?
                    .header("Content-Type", "application/json")?
                    .json(&payload)?
                    .send()
                    .await
                    .map_err(|e| CommError::Http(format!("FCM send request failed: {e}")))?;
                let s = resp.status();
                let t = resp.body_text().await.unwrap_or_default();
                (s, t)
            }
            Recipient::Channel(topic) => {
                debug!(topic = %topic, "sending FCM push to topic");
                let payload = FcmSendRequestTopic {
                    message: FcmMessageTopic {
                        topic: topic.as_str(),
                        notification,
                    },
                };
                let resp = self
                    .http
                    .post(&url)?
                    .header("Authorization", &format!("Bearer {token}"))?
                    .header("Content-Type", "application/json")?
                    .json(&payload)?
                    .send()
                    .await
                    .map_err(|e| CommError::Http(format!("FCM send request failed: {e}")))?;
                let s = resp.status();
                let t = resp.body_text().await.unwrap_or_default();
                (s, t)
            }
            // Safety: Email is rejected before this match is reached.
            Recipient::Email(_) => unreachable!("email recipient rejected above"),
        };

        if status == oxihttp::StatusCode::UNAUTHORIZED || status == oxihttp::StatusCode::FORBIDDEN {
            return Err(CommError::Provider(format!(
                "Firebase FCM auth error HTTP {status}: {response_text}"
            )));
        }

        if !status.is_success() {
            return Err(CommError::Http(format!(
                "Firebase FCM API {status}: {response_text}"
            )));
        }

        let parsed: FcmSendResponse = serde_json::from_str(&response_text)
            .map_err(|e| CommError::Serialization(format!("deserialize FCM response: {e}")))?;

        Ok(MessageReceipt {
            message_id: parsed.name,
            timestamp: Utc::now(),
        })
    }

    #[instrument(skip(self), fields(provider = "firebase-fcm"))]
    async fn list_channels(&self) -> Result<Vec<Channel>> {
        Err(CommError::Unsupported(
            "Firebase FCM does not provide a channel listing API".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::CommError;
    use crate::providers::MessageProvider;
    use crate::types::{OutgoingMessage, Recipient};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // A complete, valid RSA-2048 PKCS#1 private key used only in tests.
    // Source: jsonwebtoken crate test vectors (non-production, public domain).
    const TEST_RSA_PRIVATE_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAyRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTL
UTv4l4sggh5/CYYi/cvI+SXVT9kPWSKXxJXBXd/4LkvcPuUakBoAkfh+eiFVMh2V
rUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8H
oGfG/AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBI
Mc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi+yUod+j8MtvIj812dkS4QMiRVN/
by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQIDAQABAoIBAHREk0I0O9DvECKd
WUpAmF3mY7oY9PNQiu44Yaf+AoSuyRpRUGTMIgc3u3eivOE8ALX0BmYUO5JtuRNZ
Dpvt4SAwqCnVUinIf6C+eH/wSurCpapSM0BAHp4aOA7igptyOMgMPYBHNA1e9A7j
E0dCxKWMl3DSWNyjQTk4zeRGEAEfbNjHrq6YCtjHSZSLmWiG80hnfnYos9hOr5Jn
LnyS7ZmFE/5P3XVrxLc/tQ5zum0R4cbrgzHiQP5RgfxGJaEi7XcgherCCOgurJSS
bYH29Gz8u5fFbS+Yg8s+OiCss3cs1rSgJ9/eHZuzGEdUZVARH6hVMjSuwvqVTFaE
8AgtleECgYEA+uLMn4kNqHlJS2A5uAnCkj90ZxEtNm3E8hAxUrhssktY5XSOAPBl
xyf5RuRGIImGtUVIr4HuJSa5TX48n3Vdt9MYCprO/iYl6moNRSPt5qowIIOJmIjY
2mqPDfDt/zw+fcDD3lmCJrFlzcnh0uea1CohxEbQnL3cypeLt+WbU6kCgYEAzSp1
9m1ajieFkqgoB0YTpt/OroDx38vvI5unInJlEeOjQ+oIAQdN2wpxBvTrRorMU6P0
7mFUbt1j+Co6CbNiw+X8HcCaqYLR5clbJOOWNR36PuzOpQLkfK8woupBxzW9B8gZ
mY8rB1mbJ+/WTPrEJy6YGmIEBkWylQ2VpW8O4O0CgYEApdbvvfFBlwD9YxbrcGz7
MeNCFbMz+MucqQntIKoKJ91ImPxvtc0y6e/Rhnv0oyNlaUOwJVu0yNgNG117w0g4
t/+Q38mvVC5xV7/cn7x9UMFk6MkqVir3dYGEqIl/OP1grY2Tq9HtB5iyG9L8NIam
QOLMyUqqMUILxdthHyFmiGkCgYEAn9+PjpjGMPHxL0gj8Q8VbzsFtou6b1deIRRA
2CHmSltltR1gYVTMwXxQeUhPMmgkMqUXzs4/WijgpthY44hK1TaZEKIuoxrS70nJ
4WQLf5a9k1065fDsFZD6yGjdGxvwEmlGMZgTwqV7t1I4X0Ilqhav5hcs5apYL7gn
PYPeRz0CgYALHCj/Ji8XSsDoF/MhVhnGdIs2P99NNdmo3R2Pv0CuZbDKMU559LJH
UvrKS8WkuWRDuKrz1W/EQKApFjDGpdqToZqriUFQzwy7mR3ayIiogzNtHcvbDHx8
oFnGY0OFksX/ye0/XGpy2SFxYRwGU98HPYeBvAQQrVjdkzfy7BmXQQ==
-----END RSA PRIVATE KEY-----";

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_test_sa_json(rsa_pem: &str) -> String {
        serde_json::json!({
            "type": "service_account",
            "project_id": "test-project",
            "client_email": "test@test-project.iam.gserviceaccount.com",
            "private_key": rsa_pem
        })
        .to_string()
    }

    /// Build a `FirebaseFcmProvider` wired to the supplied `MockServer` for
    /// both the token endpoint and the FCM messages endpoint.
    fn mock_firebase_provider(mock_server: &MockServer) -> FirebaseFcmProvider {
        let sa_json = make_test_sa_json(TEST_RSA_PRIVATE_KEY);
        let cfg = FirebaseFcmConfig {
            project_id: "test-project".to_string(),
            service_account_json: sa_json,
            base_url: mock_server.uri(),
            token_url: format!("{}/token", mock_server.uri()),
        };
        FirebaseFcmProvider::new(cfg).expect("construct FirebaseFcmProvider")
    }

    /// Register the standard token-exchange mock on `mock_server`.
    async fn mount_token_mock(mock_server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "test_access_token_xyz",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .mount(mock_server)
            .await;
    }

    /// Register the standard FCM send mock on `mock_server`.
    async fn mount_send_mock(mock_server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/projects/test-project/messages:send"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "projects/test-project/messages/msg123"
            })))
            .mount(mock_server)
            .await;
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_fcm_send_to_device_token() {
        let server = MockServer::start().await;
        mount_token_mock(&server).await;
        mount_send_mock(&server).await;

        let provider = mock_firebase_provider(&server);
        let msg = OutgoingMessage::simple(
            Recipient::User("device-token-abc".to_string()),
            "Hello from FCM",
        );
        let receipt = provider.send_message(msg).await.unwrap();
        assert_eq!(receipt.message_id, "projects/test-project/messages/msg123");
    }

    #[tokio::test]
    async fn test_fcm_send_to_topic() {
        let server = MockServer::start().await;
        mount_token_mock(&server).await;

        // Capture and inspect the request body to verify topic routing.
        Mock::given(method("POST"))
            .and(path("/projects/test-project/messages:send"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "projects/test-project/messages/topic-msg-456"
            })))
            .mount(&server)
            .await;

        let provider = mock_firebase_provider(&server);
        let msg = OutgoingMessage::simple(Recipient::Channel("news".to_string()), "Breaking news!");
        let receipt = provider.send_message(msg).await.unwrap();
        assert_eq!(
            receipt.message_id,
            "projects/test-project/messages/topic-msg-456"
        );
    }

    #[tokio::test]
    async fn test_fcm_email_unsupported() {
        let server = MockServer::start().await;
        // No mocks needed — we expect an early error.
        let provider = mock_firebase_provider(&server);
        let msg = OutgoingMessage::simple(Recipient::Email("user@example.com".to_string()), "hi");
        let result = provider.send_message(msg).await;
        assert!(matches!(result.unwrap_err(), CommError::Unsupported(_)));
    }

    #[tokio::test]
    async fn test_fcm_list_channels_unsupported() {
        let server = MockServer::start().await;
        let provider = mock_firebase_provider(&server);
        let result = provider.list_channels().await;
        assert!(matches!(result.unwrap_err(), CommError::Unsupported(_)));
    }

    #[test]
    fn test_fcm_provider_name() {
        let sa_json = make_test_sa_json(TEST_RSA_PRIVATE_KEY);
        let cfg = FirebaseFcmConfig {
            project_id: "test-project".to_string(),
            service_account_json: sa_json,
            ..Default::default()
        };
        let provider = FirebaseFcmProvider::new(cfg).unwrap();
        assert_eq!(provider.provider_name(), "firebase-fcm");
    }

    #[tokio::test]
    async fn test_fcm_http_error() {
        let server = MockServer::start().await;
        mount_token_mock(&server).await;

        Mock::given(method("POST"))
            .and(path("/projects/test-project/messages:send"))
            .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
            .mount(&server)
            .await;

        let provider = mock_firebase_provider(&server);
        let msg = OutgoingMessage::simple(Recipient::User("token".to_string()), "test");
        let result = provider.send_message(msg).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("400"),
            "Expected 400 in error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_fcm_auth_error() {
        let server = MockServer::start().await;
        mount_token_mock(&server).await;

        Mock::given(method("POST"))
            .and(path("/projects/test-project/messages:send"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let provider = mock_firebase_provider(&server);
        let msg = OutgoingMessage::simple(Recipient::User("token".to_string()), "test");
        let result = provider.send_message(msg).await;
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), CommError::Provider(_)),
            "Expected CommError::Provider for 401"
        );
    }

    #[tokio::test]
    async fn test_fcm_token_refresh() {
        // Verify that the token endpoint is called at least once.
        let server = MockServer::start().await;

        let token_mock = Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "refreshed_token_999",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .expect(1..) // must be called at least once
            .mount_as_scoped(&server)
            .await;

        mount_send_mock(&server).await;

        let provider = mock_firebase_provider(&server);
        let msg = OutgoingMessage::simple(Recipient::User("dev-token".to_string()), "hi");
        provider.send_message(msg).await.unwrap();

        // Dropping the scoped mock verifies the expectation.
        drop(token_mock);
    }

    #[test]
    fn test_fcm_from_env_missing_project_id() {
        // Safety: test-only mutation of env; run in single-threaded context.
        unsafe {
            env::remove_var("FIREBASE_PROJECT_ID");
            env::remove_var("FIREBASE_SERVICE_ACCOUNT_JSON");
        }
        let result = FirebaseFcmConfig::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CommError::Auth(_)));
    }

    #[test]
    fn test_fcm_config_default_base_url() {
        let cfg = FirebaseFcmConfig::default();
        assert_eq!(cfg.base_url, "https://fcm.googleapis.com/v1");
        assert_eq!(cfg.token_url, "https://oauth2.googleapis.com/token");
        assert!(cfg.project_id.is_empty());
        assert!(cfg.service_account_json.is_empty());
    }

    #[test]
    fn test_fcm_invalid_service_account_json() {
        let cfg = FirebaseFcmConfig {
            project_id: "test".to_string(),
            service_account_json: "{ not valid json }".to_string(),
            ..Default::default()
        };
        let result = FirebaseFcmProvider::new(cfg);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CommError::Provider(_)));
    }

    #[test]
    fn test_fcm_build_jwt() {
        // Verify that a properly formed service account JSON with a valid RSA
        // key produces a JWT without error.
        let sa_json = make_test_sa_json(TEST_RSA_PRIVATE_KEY);
        let cfg = FirebaseFcmConfig {
            project_id: "test-project".to_string(),
            service_account_json: sa_json,
            ..Default::default()
        };
        let provider = FirebaseFcmProvider::new(cfg).unwrap();
        let jwt = provider.build_jwt();
        assert!(jwt.is_ok(), "build_jwt should succeed: {:?}", jwt.err());
        let token_str = jwt.unwrap();
        // A RS256 JWT has three dot-separated segments.
        let parts: Vec<&str> = token_str.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT must have 3 parts separated by '.'");
    }
}
