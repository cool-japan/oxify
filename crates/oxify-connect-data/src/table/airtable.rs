//! Airtable REST API v0 provider implementation.

use async_trait::async_trait;
use oxify_model::http_util::append_query_params;
use serde_json::{json, Value};
use tracing::instrument;

use super::TableExecutor;
use crate::error::{DataError, Result};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Airtable provider.
///
/// Build with [`AirtableConfig::from_env`] for production use or
/// [`AirtableConfig::with_base_url`] to point at a test stub.
#[derive(Debug, Clone)]
pub struct AirtableConfig {
    /// Personal access token (PAT) or OAuth access token.
    pub api_key: String,
    /// Base URL for the Airtable API (default: `"https://api.airtable.com/v0"`).
    pub base_url: String,
}

impl AirtableConfig {
    /// Return the canonical Airtable REST API v0 base URL.
    #[must_use]
    pub fn default_base_url() -> String {
        "https://api.airtable.com/v0".to_owned()
    }

    /// Construct a config, reading `AIRTABLE_API_KEY` from the environment.
    ///
    /// # Errors
    /// Returns [`DataError::Config`] when the environment variable is absent.
    pub fn from_env() -> Result<Self> {
        let key = std::env::var("AIRTABLE_API_KEY")
            .map_err(|_| DataError::Config("AIRTABLE_API_KEY not set".into()))?;
        Ok(Self {
            api_key: key,
            base_url: Self::default_base_url(),
        })
    }

    /// Override the API base URL — useful for pointing at a WireMock stub
    /// during tests.
    #[must_use]
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Airtable REST API v0 provider.
///
/// Implements [`TableExecutor`] for record CRUD against Airtable bases.
pub struct AirtableProvider {
    cfg: AirtableConfig,
    http: oxihttp::HttpsClient,
}

impl AirtableProvider {
    /// Construct a provider from the given configuration.
    ///
    /// # Errors
    /// Returns [`DataError::Config`] if the underlying HTTPS client cannot be
    /// constructed (e.g. TLS trust-store initialization failure).
    pub fn new(cfg: AirtableConfig) -> Result<Self> {
        let http = oxihttp::Client::builder()
            .with_tls()
            .build_https()
            .map_err(|e| DataError::Config(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { cfg, http })
    }

    /// Produce the `Authorization` header value for the current API key.
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.cfg.api_key)
    }

    /// Map an HTTP status code to the appropriate [`DataError`] variant.
    async fn map_error(resp: oxihttp::Response) -> DataError {
        let status = resp.status();
        let body = resp.body_text().await.unwrap_or_default();
        match status.as_u16() {
            401 | 403 => DataError::Auth(format!("HTTP {status}: {body}")),
            404 => DataError::NotFound(format!("HTTP {status}: {body}")),
            422 => DataError::Api(format!("HTTP {status}: {body}")),
            429 => DataError::RateLimited(format!("HTTP {status}: {body}")),
            _ => DataError::Api(format!("HTTP {status}: {body}")),
        }
    }

    /// Build the record-list URL and optional query parameters, then send the
    /// request.  Shared by [`list_records`] and [`search_records`].
    async fn list_records_inner(
        &self,
        base_id: &str,
        table_id: &str,
        filter_formula: Option<&str>,
        max_records: Option<u64>,
    ) -> Result<Vec<Value>> {
        let url = format!("{}/{}/{}", self.cfg.base_url, base_id, table_id);

        // Build query params; avoid allocating if there are none.
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(max) = max_records {
            params.push(("maxRecords", max.to_string()));
        }
        if let Some(formula) = filter_formula {
            params.push(("filterByFormula", formula.to_owned()));
        }
        let url = if params.is_empty() {
            url
        } else {
            append_query_params(&url, &params)
        };

        let resp = self
            .http
            .get(&url)?
            .header("Authorization", &self.auth_header())?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }

        let data: Value = resp.body_json().await?;
        let records = data
            .get("records")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(records)
    }
}

#[async_trait]
impl TableExecutor for AirtableProvider {
    fn provider_name(&self) -> &str {
        "airtable"
    }

    #[instrument(skip(self), fields(provider = "airtable"))]
    async fn list_records(
        &self,
        base_id: &str,
        table_id: &str,
        filter_formula: Option<&str>,
        max_records: Option<u64>,
    ) -> Result<Vec<Value>> {
        self.list_records_inner(base_id, table_id, filter_formula, max_records)
            .await
    }

    #[instrument(skip(self), fields(provider = "airtable"))]
    async fn get_record(&self, base_id: &str, table_id: &str, record_id: &str) -> Result<Value> {
        let url = format!(
            "{}/{}/{}/{}",
            self.cfg.base_url, base_id, table_id, record_id
        );
        let resp = self
            .http
            .get(&url)?
            .header("Authorization", &self.auth_header())?
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        let record: Value = resp.body_json().await?;
        Ok(record)
    }

    #[instrument(skip(self, fields), fields(provider = "airtable"))]
    async fn create_record(&self, base_id: &str, table_id: &str, fields: Value) -> Result<String> {
        let url = format!("{}/{}/{}", self.cfg.base_url, base_id, table_id);
        let body = json!({"fields": fields});
        let resp = self
            .http
            .post(&url)?
            .header("Authorization", &self.auth_header())?
            .json(&body)?
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        let data: Value = resp.body_json().await?;
        let id = data
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| DataError::Api("create_record: missing 'id' in response".into()))?
            .to_owned();
        Ok(id)
    }

    #[instrument(skip(self, fields), fields(provider = "airtable"))]
    async fn update_record(
        &self,
        base_id: &str,
        table_id: &str,
        record_id: &str,
        fields: Value,
    ) -> Result<()> {
        let url = format!(
            "{}/{}/{}/{}",
            self.cfg.base_url, base_id, table_id, record_id
        );
        let body = json!({"fields": fields});
        let resp = self
            .http
            .patch(&url)?
            .header("Authorization", &self.auth_header())?
            .json(&body)?
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        Ok(())
    }

    #[instrument(skip(self), fields(provider = "airtable"))]
    async fn delete_record(&self, base_id: &str, table_id: &str, record_id: &str) -> Result<()> {
        let url = format!(
            "{}/{}/{}/{}",
            self.cfg.base_url, base_id, table_id, record_id
        );
        let resp = self
            .http
            .delete(&url)?
            .header("Authorization", &self.auth_header())?
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        Ok(())
    }

    #[instrument(skip(self), fields(provider = "airtable"))]
    async fn search_records(
        &self,
        base_id: &str,
        table_id: &str,
        formula: &str,
    ) -> Result<Vec<Value>> {
        self.list_records_inner(base_id, table_id, Some(formula), None)
            .await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn make_provider(base_url: String) -> AirtableProvider {
        let cfg = AirtableConfig {
            api_key: "test-key".to_owned(),
            base_url,
        };
        AirtableProvider::new(cfg).expect("failed to build AirtableProvider")
    }

    // ------------------------------------------------------------------

    #[test]
    fn provider_name_is_airtable() {
        let p = make_provider("http://unused".to_owned());
        assert_eq!(p.provider_name(), "airtable");
    }

    #[tokio::test]
    async fn list_records_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/appBASE/tblTABLE"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"records":[{"id":"rec1","fields":{"Name":"Alice"},"createdTime":"2024-01-01T00:00:00.000Z"}]}),
            ))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let records = p
            .list_records("appBASE", "tblTABLE", None, None)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id"], json!("rec1"));
    }

    #[tokio::test]
    async fn list_records_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/appBASE/tblTABLE"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"records":[]})))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let records = p
            .list_records("appBASE", "tblTABLE", None, None)
            .await
            .unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn get_record_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/appBASE/tblTABLE/recXYZ"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id":"recXYZ","fields":{"Name":"Bob"},"createdTime":"2024-01-01T00:00:00.000Z"}),
            ))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let rec = p.get_record("appBASE", "tblTABLE", "recXYZ").await.unwrap();
        assert_eq!(rec["id"], json!("recXYZ"));
        assert_eq!(rec["fields"]["Name"], json!("Bob"));
    }

    #[tokio::test]
    async fn create_record_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/appBASE/tblTABLE"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id":"recABC","fields":{"Name":"Charlie"},"createdTime":"2024-01-01T00:00:00.000Z"}),
            ))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let id = p
            .create_record("appBASE", "tblTABLE", json!({"Name":"Charlie"}))
            .await
            .unwrap();
        assert_eq!(id, "recABC");
    }

    #[tokio::test]
    async fn update_record_success() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/appBASE/tblTABLE/recABC"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id":"recABC","fields":{"Name":"Dave"}})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        p.update_record("appBASE", "tblTABLE", "recABC", json!({"Name":"Dave"}))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_record_success() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/appBASE/tblTABLE/recABC"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"deleted":true,"id":"recABC"})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        p.delete_record("appBASE", "tblTABLE", "recABC")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn search_records_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/appBASE/tblTABLE"))
            .and(query_param("filterByFormula", "{Name}='Eve'"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[{"id":"recEVE","fields":{"Name":"Eve"}}]})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let records = p
            .search_records("appBASE", "tblTABLE", "{Name}='Eve'")
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id"], json!("recEVE"));
    }

    #[tokio::test]
    async fn auth_error_returns_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/appBASE/tblTABLE"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p
            .list_records("appBASE", "tblTABLE", None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, DataError::Auth(_)));
    }

    #[tokio::test]
    async fn not_found_returns_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/appBASE/tblTABLE/recNONE"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p
            .get_record("appBASE", "tblTABLE", "recNONE")
            .await
            .unwrap_err();
        assert!(matches!(err, DataError::NotFound(_)));
    }

    #[tokio::test]
    async fn rate_limited_returns_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/appBASE/tblTABLE"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too Many Requests"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p
            .list_records("appBASE", "tblTABLE", None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, DataError::RateLimited(_)));
    }

    #[tokio::test]
    async fn auth_header_is_sent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/appBASE/tblTABLE"))
            .and(header("Authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"records":[]})))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        p.list_records("appBASE", "tblTABLE", None, None)
            .await
            .unwrap();
    }

    #[test]
    fn from_env_missing_key() {
        std::env::remove_var("AIRTABLE_API_KEY");
        let err = AirtableConfig::from_env().unwrap_err();
        assert!(matches!(err, DataError::Config(_)));
    }

    #[test]
    fn with_base_url_overrides() {
        std::env::set_var("AIRTABLE_API_KEY", "k");
        let cfg = AirtableConfig::from_env()
            .unwrap()
            .with_base_url("http://localhost:9999".to_owned());
        assert_eq!(cfg.base_url, "http://localhost:9999");
        std::env::remove_var("AIRTABLE_API_KEY");
    }
}
