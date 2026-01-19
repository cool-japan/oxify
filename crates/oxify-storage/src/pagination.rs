//! Pagination utilities for database queries
//!
//! This module provides standardized pagination helpers for both cursor-based
//! and offset-based pagination strategies.
//!
//! # Pagination Strategies
//!
//! ## Offset-Based Pagination
//! Simple pagination using `OFFSET` and `LIMIT`. Best for small datasets
//! or when users need to jump to arbitrary pages.
//!
//! ```ignore
//! use oxify_storage::pagination::{PaginationRequest, paginate_query};
//!
//! let request = PaginationRequest::offset(0, 20);
//! let (items, response) = paginate_query(
//!     pool,
//!     "SELECT * FROM workflows WHERE user_id = $1",
//!     &[user_id],
//!     request,
//! ).await?;
//! ```
//!
//! ## Cursor-Based Pagination
//! Uses a cursor (typically a unique identifier or timestamp) for efficient
//! pagination. Best for large datasets and infinite scroll scenarios.
//!
//! ```ignore
//! use oxify_storage::pagination::{PaginationRequest, CursorDirection};
//!
//! let request = PaginationRequest::cursor(
//!     Some("last_id_from_previous_page"),
//!     20,
//!     CursorDirection::Forward,
//! );
//! ```

use serde::{Deserialize, Serialize};

/// Default page size if not specified
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// Maximum allowed page size to prevent excessive memory usage
pub const MAX_PAGE_SIZE: u32 = 1000;

/// Minimum allowed page size
pub const MIN_PAGE_SIZE: u32 = 1;

/// Pagination strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaginationStrategy {
    /// Offset-based pagination (page number + size)
    Offset {
        /// Zero-based offset
        offset: u64,
        /// Number of items per page
        limit: u32,
    },
    /// Cursor-based pagination (cursor + size)
    Cursor {
        /// Cursor value (typically an ID or timestamp)
        cursor: Option<String>,
        /// Number of items to fetch
        limit: u32,
        /// Direction to paginate
        direction: CursorDirection,
    },
}

/// Direction for cursor-based pagination
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorDirection {
    /// Fetch items after the cursor
    Forward,
    /// Fetch items before the cursor
    Backward,
}

/// Pagination request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaginationRequest {
    /// Pagination strategy
    pub strategy: PaginationStrategy,
}

impl PaginationRequest {
    /// Create an offset-based pagination request
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::pagination::PaginationRequest;
    /// let request = PaginationRequest::offset(0, 20);
    /// ```
    pub fn offset(offset: u64, limit: u32) -> Self {
        let limit = Self::validate_limit(limit);
        Self {
            strategy: PaginationStrategy::Offset { offset, limit },
        }
    }

    /// Create a cursor-based pagination request
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::pagination::{PaginationRequest, CursorDirection};
    /// let request = PaginationRequest::cursor(None, 20, CursorDirection::Forward);
    /// ```
    pub fn cursor(cursor: Option<String>, limit: u32, direction: CursorDirection) -> Self {
        let limit = Self::validate_limit(limit);
        Self {
            strategy: PaginationStrategy::Cursor {
                cursor,
                limit,
                direction,
            },
        }
    }

    /// Get the limit value
    pub fn limit(&self) -> u32 {
        match &self.strategy {
            PaginationStrategy::Offset { limit, .. } => *limit,
            PaginationStrategy::Cursor { limit, .. } => *limit,
        }
    }

    /// Validate and clamp limit to allowed range
    fn validate_limit(limit: u32) -> u32 {
        limit.clamp(MIN_PAGE_SIZE, MAX_PAGE_SIZE)
    }
}

impl Default for PaginationRequest {
    fn default() -> Self {
        Self::offset(0, DEFAULT_PAGE_SIZE)
    }
}

/// Pagination response with metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaginationResponse<T> {
    /// Items in the current page
    pub items: Vec<T>,
    /// Total number of items (if available)
    pub total: Option<u64>,
    /// Current page number (for offset pagination)
    pub page: Option<u64>,
    /// Number of items per page
    pub page_size: u32,
    /// Whether there are more items available
    pub has_more: bool,
    /// Cursor for the next page (for cursor pagination)
    pub next_cursor: Option<String>,
    /// Cursor for the previous page (for cursor pagination)
    pub prev_cursor: Option<String>,
}

impl<T> PaginationResponse<T> {
    /// Create a new pagination response for offset-based pagination
    pub fn offset(items: Vec<T>, total: u64, page: u64, page_size: u32) -> Self {
        let has_more = (page + 1) * (page_size as u64) < total;
        Self {
            items,
            total: Some(total),
            page: Some(page),
            page_size,
            has_more,
            next_cursor: None,
            prev_cursor: None,
        }
    }

    /// Create a new pagination response for cursor-based pagination
    pub fn cursor(
        items: Vec<T>,
        page_size: u32,
        next_cursor: Option<String>,
        prev_cursor: Option<String>,
    ) -> Self {
        let has_more = next_cursor.is_some();
        Self {
            items,
            total: None,
            page: None,
            page_size,
            has_more,
            next_cursor,
            prev_cursor,
        }
    }

    /// Map the items to a different type
    pub fn map<U, F>(self, f: F) -> PaginationResponse<U>
    where
        F: FnMut(T) -> U,
    {
        PaginationResponse {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
            page: self.page,
            page_size: self.page_size,
            has_more: self.has_more,
            next_cursor: self.next_cursor,
            prev_cursor: self.prev_cursor,
        }
    }
}

/// Page information for offset-based pagination
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageInfo {
    /// Current page number (0-based)
    pub page: u64,
    /// Number of items per page
    pub page_size: u32,
    /// Total number of items
    pub total_items: u64,
    /// Total number of pages
    pub total_pages: u64,
    /// Whether there is a next page
    pub has_next: bool,
    /// Whether there is a previous page
    pub has_prev: bool,
}

impl PageInfo {
    /// Create page information from total count and pagination request
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::pagination::{PageInfo, PaginationRequest};
    /// let request = PaginationRequest::offset(40, 20);
    /// let info = PageInfo::from_total(100, &request);
    /// assert_eq!(info.page, 2);
    /// assert_eq!(info.total_pages, 5);
    /// assert!(info.has_next);
    /// assert!(info.has_prev);
    /// ```
    pub fn from_total(total: u64, request: &PaginationRequest) -> Self {
        match &request.strategy {
            PaginationStrategy::Offset { offset, limit } => {
                let page_size = *limit;
                let page = offset / page_size as u64;
                let total_pages = total.div_ceil(page_size as u64);
                let has_next = page + 1 < total_pages;
                let has_prev = page > 0;

                Self {
                    page,
                    page_size,
                    total_items: total,
                    total_pages,
                    has_next,
                    has_prev,
                }
            }
            PaginationStrategy::Cursor { .. } => {
                // For cursor pagination, we don't have traditional page numbers
                Self {
                    page: 0,
                    page_size: request.limit(),
                    total_items: total,
                    total_pages: 1,
                    has_next: false,
                    has_prev: false,
                }
            }
        }
    }
}

/// Builder for constructing SQL LIMIT/OFFSET clauses
pub struct PaginationBuilder {
    request: PaginationRequest,
}

impl PaginationBuilder {
    /// Create a new pagination builder
    pub fn new(request: PaginationRequest) -> Self {
        Self { request }
    }

    /// Get the LIMIT value
    pub fn limit(&self) -> u32 {
        self.request.limit()
    }

    /// Get the OFFSET value (for offset-based pagination)
    pub fn offset(&self) -> Option<u64> {
        match &self.request.strategy {
            PaginationStrategy::Offset { offset, .. } => Some(*offset),
            PaginationStrategy::Cursor { .. } => None,
        }
    }

    /// Build LIMIT clause SQL
    pub fn limit_clause(&self) -> String {
        format!("LIMIT {}", self.limit())
    }

    /// Build OFFSET clause SQL (for offset-based pagination)
    pub fn offset_clause(&self) -> String {
        if let Some(offset) = self.offset() {
            format!("OFFSET {offset}")
        } else {
            String::new()
        }
    }

    /// Build complete LIMIT/OFFSET SQL
    ///
    /// # Examples
    /// ```
    /// # use oxify_storage::pagination::{PaginationRequest, PaginationBuilder};
    /// let request = PaginationRequest::offset(40, 20);
    /// let builder = PaginationBuilder::new(request);
    /// assert_eq!(builder.build_sql(), "LIMIT 20 OFFSET 40");
    /// ```
    pub fn build_sql(&self) -> String {
        let mut sql = self.limit_clause();
        let offset = self.offset_clause();
        if !offset.is_empty() {
            sql.push(' ');
            sql.push_str(&offset);
        }
        sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_request_offset() {
        let request = PaginationRequest::offset(20, 10);
        assert_eq!(request.limit(), 10);
        match request.strategy {
            PaginationStrategy::Offset { offset, limit } => {
                assert_eq!(offset, 20);
                assert_eq!(limit, 10);
            }
            _ => panic!("Expected Offset strategy"),
        }
    }

    #[test]
    fn test_pagination_request_cursor() {
        let request =
            PaginationRequest::cursor(Some("cursor123".to_string()), 15, CursorDirection::Forward);
        assert_eq!(request.limit(), 15);
        match request.strategy {
            PaginationStrategy::Cursor {
                cursor,
                limit,
                direction,
            } => {
                assert_eq!(cursor, Some("cursor123".to_string()));
                assert_eq!(limit, 15);
                assert_eq!(direction, CursorDirection::Forward);
            }
            _ => panic!("Expected Cursor strategy"),
        }
    }

    #[test]
    fn test_pagination_request_default() {
        let request = PaginationRequest::default();
        assert_eq!(request.limit(), DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn test_pagination_limit_validation() {
        // Test limit too high
        let request = PaginationRequest::offset(0, 5000);
        assert_eq!(request.limit(), MAX_PAGE_SIZE);

        // Test limit too low
        let request = PaginationRequest::offset(0, 0);
        assert_eq!(request.limit(), MIN_PAGE_SIZE);

        // Test valid limit
        let request = PaginationRequest::offset(0, 50);
        assert_eq!(request.limit(), 50);
    }

    #[test]
    fn test_pagination_response_offset() {
        let items = vec![1, 2, 3, 4, 5];
        let response = PaginationResponse::offset(items, 100, 2, 20);

        assert_eq!(response.items.len(), 5);
        assert_eq!(response.total, Some(100));
        assert_eq!(response.page, Some(2));
        assert_eq!(response.page_size, 20);
        assert!(response.has_more);
    }

    #[test]
    fn test_pagination_response_cursor() {
        let items = vec!["a", "b", "c"];
        let response = PaginationResponse::cursor(
            items,
            20,
            Some("next_cursor".to_string()),
            Some("prev_cursor".to_string()),
        );

        assert_eq!(response.items.len(), 3);
        assert_eq!(response.total, None);
        assert!(response.has_more);
        assert_eq!(response.next_cursor, Some("next_cursor".to_string()));
        assert_eq!(response.prev_cursor, Some("prev_cursor".to_string()));
    }

    #[test]
    fn test_pagination_response_map() {
        let items = vec![1, 2, 3];
        let response = PaginationResponse::offset(items, 10, 0, 20);
        let mapped = response.map(|x| x * 2);

        assert_eq!(mapped.items, vec![2, 4, 6]);
        assert_eq!(mapped.total, Some(10));
    }

    #[test]
    fn test_page_info_from_total() {
        let request = PaginationRequest::offset(40, 20);
        let info = PageInfo::from_total(100, &request);

        assert_eq!(info.page, 2);
        assert_eq!(info.page_size, 20);
        assert_eq!(info.total_items, 100);
        assert_eq!(info.total_pages, 5);
        assert!(info.has_next);
        assert!(info.has_prev);
    }

    #[test]
    fn test_page_info_first_page() {
        let request = PaginationRequest::offset(0, 20);
        let info = PageInfo::from_total(100, &request);

        assert_eq!(info.page, 0);
        assert!(!info.has_prev);
        assert!(info.has_next);
    }

    #[test]
    fn test_page_info_last_page() {
        let request = PaginationRequest::offset(80, 20);
        let info = PageInfo::from_total(100, &request);

        assert_eq!(info.page, 4);
        assert!(info.has_prev);
        assert!(!info.has_next);
    }

    #[test]
    fn test_pagination_builder() {
        let request = PaginationRequest::offset(40, 20);
        let builder = PaginationBuilder::new(request);

        assert_eq!(builder.limit(), 20);
        assert_eq!(builder.offset(), Some(40));
        assert_eq!(builder.limit_clause(), "LIMIT 20");
        assert_eq!(builder.offset_clause(), "OFFSET 40");
        assert_eq!(builder.build_sql(), "LIMIT 20 OFFSET 40");
    }

    #[test]
    fn test_pagination_builder_no_offset() {
        let request = PaginationRequest::offset(0, 10);
        let builder = PaginationBuilder::new(request);

        assert_eq!(builder.build_sql(), "LIMIT 10 OFFSET 0");
    }

    #[test]
    fn test_pagination_builder_cursor() {
        let request =
            PaginationRequest::cursor(Some("abc".to_string()), 25, CursorDirection::Forward);
        let builder = PaginationBuilder::new(request);

        assert_eq!(builder.limit(), 25);
        assert_eq!(builder.offset(), None);
        assert_eq!(builder.build_sql(), "LIMIT 25");
    }
}
