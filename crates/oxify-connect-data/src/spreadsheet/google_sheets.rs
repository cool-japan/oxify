//! Google Sheets API v4 provider implementation.

use async_trait::async_trait;
use oxify_model::http_util::append_query_params;
use serde_json::{json, Value};
use tracing::instrument;

use super::SpreadsheetExecutor;
use crate::error::{DataError, Result};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Google Sheets provider.
///
/// Build with [`GoogleSheetsConfig::from_env`] for production use or
/// [`GoogleSheetsConfig::with_base_url`] to point at a test stub.
#[derive(Debug, Clone)]
pub struct GoogleSheetsConfig {
    /// OAuth 2.0 access token.
    pub access_token: String,
    /// Base URL for the Sheets API (default: `"https://sheets.googleapis.com/v4"`).
    pub base_url: String,
}

impl GoogleSheetsConfig {
    /// Return the canonical Google Sheets API v4 base URL.
    #[must_use]
    pub fn default_base_url() -> String {
        "https://sheets.googleapis.com/v4".to_owned()
    }

    /// Construct a config, reading `GOOGLE_SHEETS_ACCESS_TOKEN` from the
    /// environment.
    ///
    /// # Errors
    /// Returns [`DataError::Config`] when the environment variable is absent.
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("GOOGLE_SHEETS_ACCESS_TOKEN")
            .map_err(|_| DataError::Config("GOOGLE_SHEETS_ACCESS_TOKEN not set".into()))?;
        Ok(Self {
            access_token: token,
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

/// Google Sheets API v4 provider.
///
/// Implements [`SpreadsheetExecutor`] for reading and writing spreadsheet data
/// via the Google Sheets REST API.
pub struct GoogleSheetsProvider {
    cfg: GoogleSheetsConfig,
    http: oxihttp::HttpsClient,
}

impl GoogleSheetsProvider {
    /// Construct a provider from the given configuration.
    ///
    /// # Errors
    /// Returns [`DataError::Config`] if the underlying HTTPS client cannot be
    /// constructed (e.g. TLS trust-store initialization failure).
    pub fn new(cfg: GoogleSheetsConfig) -> Result<Self> {
        let http = oxihttp::Client::builder()
            .with_tls()
            .build_https()
            .map_err(|e| DataError::Config(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { cfg, http })
    }

    /// Produce the `Authorization` header value for the current access token.
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.cfg.access_token)
    }

    /// Map an HTTP status code to the appropriate [`DataError`] variant,
    /// incorporating the response body for context.
    async fn map_error(resp: oxihttp::Response) -> DataError {
        let status = resp.status();
        let body = resp.body_text().await.unwrap_or_default();
        match status.as_u16() {
            401 | 403 => DataError::Auth(format!("HTTP {status}: {body}")),
            404 => DataError::NotFound(format!("HTTP {status}: {body}")),
            429 => DataError::RateLimited(format!("HTTP {status}: {body}")),
            _ => DataError::Api(format!("HTTP {status}: {body}")),
        }
    }
}

#[async_trait]
impl SpreadsheetExecutor for GoogleSheetsProvider {
    fn provider_name(&self) -> &str {
        "google-sheets"
    }

    #[instrument(skip(self), fields(provider = "google-sheets"))]
    async fn get_values(&self, spreadsheet_id: &str, range: &str) -> Result<Vec<Vec<Value>>> {
        let url = format!(
            "{}/spreadsheets/{}/values/{}",
            self.cfg.base_url, spreadsheet_id, range
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

        let body: Value = resp.body_json().await?;
        let rows = body
            .get("values")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.as_array().cloned().unwrap_or_default())
            .collect();
        Ok(rows)
    }

    #[instrument(skip(self, values), fields(provider = "google-sheets"))]
    async fn update_values(
        &self,
        spreadsheet_id: &str,
        range: &str,
        values: Vec<Vec<Value>>,
    ) -> Result<u64> {
        let url = format!(
            "{}/spreadsheets/{}/values/{}",
            self.cfg.base_url, spreadsheet_id, range
        );
        let body = json!({
            "range": range,
            "majorDimension": "ROWS",
            "values": values,
        });
        let url = append_query_params(&url, &[("valueInputOption", "USER_ENTERED")]);
        let resp = self
            .http
            .put(&url)?
            .header("Authorization", &self.auth_header())?
            .json(&body)?
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }

        let data: Value = resp.body_json().await?;
        let updated = data
            .get("updatedCells")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        Ok(updated)
    }

    #[instrument(skip(self, values), fields(provider = "google-sheets"))]
    async fn append_values(
        &self,
        spreadsheet_id: &str,
        range: &str,
        values: Vec<Vec<Value>>,
    ) -> Result<u64> {
        let url = format!(
            "{}/spreadsheets/{}/values/{}:append",
            self.cfg.base_url, spreadsheet_id, range
        );
        let body = json!({
            "range": range,
            "majorDimension": "ROWS",
            "values": values,
        });
        let url = append_query_params(
            &url,
            &[
                ("valueInputOption", "USER_ENTERED"),
                ("insertDataOption", "INSERT_ROWS"),
            ],
        );
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
        let updated = data
            .pointer("/updates/updatedCells")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        Ok(updated)
    }

    #[instrument(skip(self), fields(provider = "google-sheets"))]
    async fn clear_range(&self, spreadsheet_id: &str, range: &str) -> Result<()> {
        let url = format!(
            "{}/spreadsheets/{}/values/{}:clear",
            self.cfg.base_url, spreadsheet_id, range
        );
        let resp = self
            .http
            .post(&url)?
            .header("Authorization", &self.auth_header())?
            .json(&json!({}))?
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        Ok(())
    }

    #[instrument(skip(self), fields(provider = "google-sheets"))]
    async fn batch_get(
        &self,
        spreadsheet_id: &str,
        ranges: &[&str],
    ) -> Result<Vec<(String, Vec<Vec<Value>>)>> {
        let url = format!(
            "{}/spreadsheets/{}/values:batchGet",
            self.cfg.base_url, spreadsheet_id
        );
        // Build multi-value "ranges" query params manually so each range gets
        // its own key.
        let query: Vec<(&str, &str)> = ranges.iter().map(|r| ("ranges", *r)).collect();
        let url = append_query_params(&url, &query);
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
        let value_ranges = data
            .get("valueRanges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut result = Vec::with_capacity(value_ranges.len());
        for vr in value_ranges {
            let range_name = vr
                .get("range")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let grid: Vec<Vec<Value>> = vr
                .get("values")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|row| row.as_array().cloned().unwrap_or_default())
                .collect();
            result.push((range_name, grid));
        }
        Ok(result)
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

    fn make_provider(base_url: String) -> GoogleSheetsProvider {
        let cfg = GoogleSheetsConfig {
            access_token: "test-token".to_owned(),
            base_url,
        };
        GoogleSheetsProvider::new(cfg).expect("failed to build GoogleSheetsProvider")
    }

    // ------------------------------------------------------------------

    #[test]
    fn provider_name_is_google_sheets() {
        let p = make_provider("http://unused".to_owned());
        assert_eq!(p.provider_name(), "google-sheets");
    }

    #[tokio::test]
    async fn get_values_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spreadsheets/sid/values/A1:B2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"values":[["A1","B1"],["A2","B2"]]})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let grid = p.get_values("sid", "A1:B2").await.unwrap();
        assert_eq!(grid.len(), 2);
        assert_eq!(grid[0].len(), 2);
        assert_eq!(grid[0][0], json!("A1"));
    }

    #[tokio::test]
    async fn get_values_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spreadsheets/sid/values/A1:B2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let grid = p.get_values("sid", "A1:B2").await.unwrap();
        assert!(grid.is_empty());
    }

    #[tokio::test]
    async fn update_values_success() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/spreadsheets/sid/values/A1:B2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"updatedCells":4})))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let count = p
            .update_values(
                "sid",
                "A1:B2",
                vec![vec![json!("x"), json!("y")], vec![json!("a"), json!("b")]],
            )
            .await
            .unwrap();
        assert_eq!(count, 4);
    }

    #[tokio::test]
    async fn append_values_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/spreadsheets/sid/values/A1:B1:append"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"updates":{"updatedCells":2}})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let count = p
            .append_values("sid", "A1:B1", vec![vec![json!("x"), json!("y")]])
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn clear_range_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/spreadsheets/sid/values/A1:B2:clear"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"clearedRange":"A1:B2"})))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        p.clear_range("sid", "A1:B2").await.unwrap();
    }

    #[tokio::test]
    async fn batch_get_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spreadsheets/sid/values:batchGet"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"valueRanges":[{"range":"A1:B2","values":[["x"]]}]})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let results = p.batch_get("sid", &["A1:B2"]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "A1:B2");
        assert_eq!(results[0].1[0][0], json!("x"));
    }

    #[tokio::test]
    async fn auth_error_returns_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spreadsheets/sid/values/A1"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p.get_values("sid", "A1").await.unwrap_err();
        assert!(matches!(err, DataError::Auth(_)));
    }

    #[tokio::test]
    async fn forbidden_returns_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spreadsheets/sid/values/A1"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p.get_values("sid", "A1").await.unwrap_err();
        assert!(matches!(err, DataError::Auth(_)));
    }

    #[tokio::test]
    async fn not_found_returns_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spreadsheets/no-such/values/A1"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p.get_values("no-such", "A1").await.unwrap_err();
        assert!(matches!(err, DataError::NotFound(_)));
    }

    #[tokio::test]
    async fn rate_limited_returns_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spreadsheets/sid/values/A1"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too Many Requests"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p.get_values("sid", "A1").await.unwrap_err();
        assert!(matches!(err, DataError::RateLimited(_)));
    }

    #[tokio::test]
    async fn auth_header_is_sent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spreadsheets/sid/values/A1"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        p.get_values("sid", "A1").await.unwrap();
    }

    #[test]
    fn from_env_missing_token() {
        // Ensure variable is absent in this test process.
        std::env::remove_var("GOOGLE_SHEETS_ACCESS_TOKEN");
        let err = GoogleSheetsConfig::from_env().unwrap_err();
        assert!(matches!(err, DataError::Config(_)));
    }

    #[test]
    fn with_base_url_overrides() {
        std::env::set_var("GOOGLE_SHEETS_ACCESS_TOKEN", "tok");
        let cfg = GoogleSheetsConfig::from_env()
            .unwrap()
            .with_base_url("http://localhost:9999".to_owned());
        assert_eq!(cfg.base_url, "http://localhost:9999");
        std::env::remove_var("GOOGLE_SHEETS_ACCESS_TOKEN");
    }
}
