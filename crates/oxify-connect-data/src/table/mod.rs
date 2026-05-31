//! Table-record provider trait and implementations.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::Result;

pub mod airtable;

/// Trait for providers that expose table-record CRUD semantics.
///
/// Implementations cover services such as Airtable.
#[async_trait]
pub trait TableExecutor: Send + Sync {
    /// Human-readable name of the underlying provider.
    fn provider_name(&self) -> &str;

    /// List records in `table_id` inside `base_id`.
    ///
    /// An optional `filter_formula` restricts the result set and
    /// `max_records` caps the number of records returned.
    async fn list_records(
        &self,
        base_id: &str,
        table_id: &str,
        filter_formula: Option<&str>,
        max_records: Option<u64>,
    ) -> Result<Vec<Value>>;

    /// Fetch a single record by its `record_id`.
    async fn get_record(&self, base_id: &str, table_id: &str, record_id: &str) -> Result<Value>;

    /// Create a new record with the provided `fields`.  Returns the newly
    /// assigned record ID.
    async fn create_record(&self, base_id: &str, table_id: &str, fields: Value) -> Result<String>;

    /// Perform a partial update of `record_id` with the provided `fields`.
    async fn update_record(
        &self,
        base_id: &str,
        table_id: &str,
        record_id: &str,
        fields: Value,
    ) -> Result<()>;

    /// Permanently delete `record_id`.
    async fn delete_record(&self, base_id: &str, table_id: &str, record_id: &str) -> Result<()>;

    /// List all records that match `formula` (Airtable filter formula syntax).
    async fn search_records(
        &self,
        base_id: &str,
        table_id: &str,
        formula: &str,
    ) -> Result<Vec<Value>>;
}
