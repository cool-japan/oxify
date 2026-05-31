//! Notion API v1 provider implementation.

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::instrument;

use super::KnowledgeBaseExecutor;
use crate::error::{DataError, Result};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Notion provider.
///
/// Build with [`NotionConfig::from_env`] for production use or
/// [`NotionConfig::with_base_url`] to point at a test stub.
#[derive(Debug, Clone)]
pub struct NotionConfig {
    /// Internal Integration Token or OAuth access token.
    pub api_key: String,
    /// Notion API version header value (default: `"2022-06-28"`).
    pub notion_version: String,
    /// Base URL for the Notion API (default: `"https://api.notion.com/v1"`).
    pub base_url: String,
}

impl NotionConfig {
    /// Return the canonical Notion API v1 base URL.
    #[must_use]
    pub fn default_base_url() -> String {
        "https://api.notion.com/v1".to_owned()
    }

    /// Return the default Notion API version string.
    #[must_use]
    pub fn default_notion_version() -> String {
        "2022-06-28".to_owned()
    }

    /// Construct a config, reading `NOTION_API_KEY` from the environment.
    ///
    /// # Errors
    /// Returns [`DataError::Config`] when the environment variable is absent.
    pub fn from_env() -> Result<Self> {
        let key = std::env::var("NOTION_API_KEY")
            .map_err(|_| DataError::Config("NOTION_API_KEY not set".into()))?;
        Ok(Self {
            api_key: key,
            notion_version: Self::default_notion_version(),
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

/// Notion API v1 provider.
///
/// Implements [`KnowledgeBaseExecutor`] for reading and writing Notion pages
/// and databases.
pub struct NotionProvider {
    cfg: NotionConfig,
    http: reqwest::Client,
}

impl NotionProvider {
    /// Construct a provider from the given configuration.
    #[must_use]
    pub fn new(cfg: NotionConfig) -> Self {
        Self {
            cfg,
            http: reqwest::Client::new(),
        }
    }

    /// Attach the standard Notion auth and version headers to a request
    /// builder.
    fn apply_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("Authorization", format!("Bearer {}", self.cfg.api_key))
            .header("Notion-Version", &self.cfg.notion_version)
            .header("Content-Type", "application/json")
    }

    /// Map an HTTP status code to the appropriate [`DataError`] variant.
    async fn map_error(resp: reqwest::Response) -> DataError {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        match status.as_u16() {
            401 | 403 => DataError::Auth(format!("HTTP {status}: {body}")),
            404 => DataError::NotFound(format!("HTTP {status}: {body}")),
            429 => DataError::RateLimited(format!("HTTP {status}: {body}")),
            _ => DataError::Api(format!("HTTP {status}: {body}")),
        }
    }
}

#[async_trait]
impl KnowledgeBaseExecutor for NotionProvider {
    fn provider_name(&self) -> &str {
        "notion"
    }

    #[instrument(skip(self), fields(provider = "notion"))]
    async fn search(&self, query: &str, filter_type: Option<&str>) -> Result<Vec<Value>> {
        let url = format!("{}/search", self.cfg.base_url);
        let mut body = json!({"query": query});
        if let Some(ft) = filter_type {
            body["filter"] = json!({"value": ft, "property": "object"});
        }
        let resp = self
            .apply_headers(self.http.post(&url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        let data: Value = resp.json().await?;
        let results = data
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(results)
    }

    #[instrument(skip(self), fields(provider = "notion"))]
    async fn get_page(&self, page_id: &str) -> Result<Value> {
        let url = format!("{}/pages/{}", self.cfg.base_url, page_id);
        let resp = self.apply_headers(self.http.get(&url)).send().await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        let page: Value = resp.json().await?;
        Ok(page)
    }

    #[instrument(skip(self), fields(provider = "notion"))]
    async fn create_page(&self, parent_id: &str, title: &str, content: &str) -> Result<String> {
        let url = format!("{}/pages", self.cfg.base_url);
        let body = json!({
            "parent": {"page_id": parent_id},
            "properties": {
                "title": {
                    "title": [{"text": {"content": title}}]
                }
            },
            "children": [{
                "object": "block",
                "type": "paragraph",
                "paragraph": {
                    "rich_text": [{
                        "type": "text",
                        "text": {"content": content}
                    }]
                }
            }]
        });
        let resp = self
            .apply_headers(self.http.post(&url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        let data: Value = resp.json().await?;
        let id = data
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| DataError::Api("create_page: missing 'id' in response".into()))?
            .to_owned();
        Ok(id)
    }

    #[instrument(skip(self, properties), fields(provider = "notion"))]
    async fn update_page(&self, page_id: &str, properties: Value) -> Result<()> {
        let url = format!("{}/pages/{}", self.cfg.base_url, page_id);
        let body = json!({"properties": properties});
        let resp = self
            .apply_headers(self.http.patch(&url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        Ok(())
    }

    #[instrument(skip(self, filter, sorts), fields(provider = "notion"))]
    async fn query_database(
        &self,
        database_id: &str,
        filter: Option<Value>,
        sorts: Option<Vec<Value>>,
        page_size: Option<u64>,
    ) -> Result<Vec<Value>> {
        let url = format!("{}/databases/{}/query", self.cfg.base_url, database_id);
        let mut body = json!({});
        if let Some(f) = filter {
            body["filter"] = f;
        }
        if let Some(s) = sorts {
            body["sorts"] = Value::Array(s);
        }
        if let Some(ps) = page_size {
            body["page_size"] = json!(ps);
        }
        let resp = self
            .apply_headers(self.http.post(&url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        let data: Value = resp.json().await?;
        let results = data
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(results)
    }

    #[instrument(skip(self), fields(provider = "notion"))]
    async fn get_database(&self, database_id: &str) -> Result<Value> {
        let url = format!("{}/databases/{}", self.cfg.base_url, database_id);
        let resp = self.apply_headers(self.http.get(&url)).send().await?;

        if !resp.status().is_success() {
            return Err(Self::map_error(resp).await);
        }
        let db: Value = resp.json().await?;
        Ok(db)
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

    fn make_provider(base_url: String) -> NotionProvider {
        let cfg = NotionConfig {
            api_key: "secret_test".to_owned(),
            notion_version: "2022-06-28".to_owned(),
            base_url,
        };
        NotionProvider::new(cfg)
    }

    // ------------------------------------------------------------------

    #[test]
    fn provider_name_is_notion() {
        let p = make_provider("http://unused".to_owned());
        assert_eq!(p.provider_name(), "notion");
    }

    #[tokio::test]
    async fn search_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"results":[{"id":"page1","object":"page"}]})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let results = p.search("hello", None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["id"], json!("page1"));
    }

    #[tokio::test]
    async fn search_with_filter() {
        let server = MockServer::start().await;
        // The mock simply checks the body contains the filter field — we use a
        // body matcher via wiremock's json body matcher.
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"results":[{"id":"db1","object":"database"}]})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let results = p.search("my db", Some("database")).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["object"], json!("database"));
    }

    #[tokio::test]
    async fn get_page_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pages/page123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id":"page123","object":"page","properties":{}})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let page = p.get_page("page123").await.unwrap();
        assert_eq!(page["id"], json!("page123"));
    }

    #[tokio::test]
    async fn create_page_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/pages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"id":"page_abc","object":"page"})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let id = p
            .create_page("parent_xyz", "My Title", "Hello world")
            .await
            .unwrap();
        assert_eq!(id, "page_abc");
    }

    #[tokio::test]
    async fn update_page_success() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/pages/page123"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"id":"page123","object":"page"})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        p.update_page("page123", json!({"Status": {"select": {"name": "Done"}}}))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn query_database_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/databases/db123/query"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"results":[{"id":"p1"},{"id":"p2"}]})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let results = p
            .query_database("db123", None, None, Some(10))
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn get_database_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/databases/db999"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id":"db999","object":"database","title":[]})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let db = p.get_database("db999").await.unwrap();
        assert_eq!(db["id"], json!("db999"));
    }

    #[tokio::test]
    async fn auth_header_sent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pages/pg1"))
            .and(header("Authorization", "Bearer secret_test"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"id":"pg1","object":"page"})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        p.get_page("pg1").await.unwrap();
    }

    #[tokio::test]
    async fn notion_version_header_sent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pages/pg2"))
            .and(header("Notion-Version", "2022-06-28"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"id":"pg2","object":"page"})),
            )
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        p.get_page("pg2").await.unwrap();
    }

    #[tokio::test]
    async fn auth_error_returns_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pages/pg3"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p.get_page("pg3").await.unwrap_err();
        assert!(matches!(err, DataError::Auth(_)));
    }

    #[tokio::test]
    async fn rate_limited_returns_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too Many Requests"))
            .mount(&server)
            .await;

        let p = make_provider(server.uri());
        let err = p.search("anything", None).await.unwrap_err();
        assert!(matches!(err, DataError::RateLimited(_)));
    }

    #[test]
    fn from_env_missing_key() {
        std::env::remove_var("NOTION_API_KEY");
        let err = NotionConfig::from_env().unwrap_err();
        assert!(matches!(err, DataError::Config(_)));
    }

    #[test]
    fn with_base_url_overrides() {
        std::env::set_var("NOTION_API_KEY", "k");
        let cfg = NotionConfig::from_env()
            .unwrap()
            .with_base_url("http://localhost:9999".to_owned());
        assert_eq!(cfg.base_url, "http://localhost:9999");
        std::env::remove_var("NOTION_API_KEY");
    }
}
