//! Knowledge-base provider trait and implementations.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::Result;

pub mod notion;

/// Trait for providers that expose knowledge-base or wiki-style read/write
/// operations.
///
/// Implementations cover services such as Notion.
#[async_trait]
pub trait KnowledgeBaseExecutor: Send + Sync {
    /// Human-readable name of the underlying provider.
    fn provider_name(&self) -> &str;

    /// Full-text search across the workspace.  An optional `filter_type`
    /// restricts results to `"page"` or `"database"`.
    async fn search(&self, query: &str, filter_type: Option<&str>) -> Result<Vec<Value>>;

    /// Retrieve a single page object by its ID.
    async fn get_page(&self, page_id: &str) -> Result<Value>;

    /// Create a new page under `parent_id` with the given `title` and plain
    /// text `content`.  Returns the new page's ID.
    async fn create_page(&self, parent_id: &str, title: &str, content: &str) -> Result<String>;

    /// Overwrite page `properties` for the given `page_id`.
    async fn update_page(&self, page_id: &str, properties: Value) -> Result<()>;

    /// Query a Notion database, optionally applying a `filter`, `sorts`, and
    /// `page_size` cap.  Returns the list of matching page objects.
    async fn query_database(
        &self,
        database_id: &str,
        filter: Option<Value>,
        sorts: Option<Vec<Value>>,
        page_size: Option<u64>,
    ) -> Result<Vec<Value>>;

    /// Retrieve the schema / metadata for a Notion database.
    async fn get_database(&self, database_id: &str) -> Result<Value>;
}
