//! Spreadsheet provider trait and implementations.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::Result;

pub mod google_sheets;

/// Trait for providers that expose spreadsheet-style read/write operations.
///
/// Implementations cover services such as Google Sheets.
#[async_trait]
pub trait SpreadsheetExecutor: Send + Sync {
    /// Human-readable name of the underlying provider.
    fn provider_name(&self) -> &str;

    /// Fetch all values in `range` from the spreadsheet identified by
    /// `spreadsheet_id`.  Returns a row-major grid of JSON values; an empty
    /// grid is returned when the range contains no data.
    async fn get_values(&self, spreadsheet_id: &str, range: &str) -> Result<Vec<Vec<Value>>>;

    /// Overwrite `range` with `values`.  Returns the number of cells updated.
    async fn update_values(
        &self,
        spreadsheet_id: &str,
        range: &str,
        values: Vec<Vec<Value>>,
    ) -> Result<u64>;

    /// Append `values` after the last row in `range`.  Returns the number of
    /// cells inserted.
    async fn append_values(
        &self,
        spreadsheet_id: &str,
        range: &str,
        values: Vec<Vec<Value>>,
    ) -> Result<u64>;

    /// Remove all values from `range` without deleting the cells themselves.
    async fn clear_range(&self, spreadsheet_id: &str, range: &str) -> Result<()>;

    /// Fetch multiple ranges in a single round-trip.  Returns a vec of
    /// `(range_name, grid)` tuples in the same order as `ranges`.
    async fn batch_get(
        &self,
        spreadsheet_id: &str,
        ranges: &[&str],
    ) -> Result<Vec<(String, Vec<Vec<Value>>)>>;
}
