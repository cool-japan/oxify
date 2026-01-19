//! JSONB query helpers for PostgreSQL
//!
//! Provides utilities for working with PostgreSQL JSONB columns, which are commonly
//! used for storing workflow contexts, execution variables, and other semi-structured data.
//!
//! # Features
//!
//! - Type-safe JSONB query building
//! - Path-based JSON navigation
//! - Filtering and searching in JSONB columns
//! - JSONB aggregation queries
//! - Index-aware query optimization hints
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::jsonb_helpers::{JsonbQuery, JsonbPath};
//!
//! // Build a query to find workflows with specific context values
//! let query = JsonbQuery::new("executions")
//!     .where_jsonb_contains("context", r#"{"status": "active"}"#)
//!     .where_jsonb_path_equals("context", &["user", "role"], "admin")
//!     .build();
//!
//! // Execute query with SQLx
//! let results = sqlx::query(&query.sql)
//!     .bind_all(&query.params)
//!     .fetch_all(pool)
//!     .await?;
//! ```

use serde_json::Value as JsonValue;

/// A builder for JSONB queries
#[derive(Debug, Clone)]
pub struct JsonbQuery {
    table: String,
    conditions: Vec<String>,
    params: Vec<String>,
    param_counter: usize,
}

impl JsonbQuery {
    /// Create a new JSONB query builder for the given table
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            conditions: Vec::new(),
            params: Vec::new(),
            param_counter: 1,
        }
    }

    /// Add a condition to check if a JSONB column contains the given JSON
    ///
    /// Uses the `@>` operator which can utilize GIN indexes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// query.where_jsonb_contains("context", r#"{"status": "active"}"#);
    /// ```
    pub fn where_jsonb_contains(mut self, column: &str, json: &str) -> Self {
        self.conditions
            .push(format!("{} @> ${}", column, self.param_counter));
        self.params.push(json.to_string());
        self.param_counter += 1;
        self
    }

    /// Add a condition to check if the JSONB column is contained by the given JSON
    ///
    /// Uses the `<@` operator which can utilize GIN indexes.
    pub fn where_jsonb_contained_by(mut self, column: &str, json: &str) -> Self {
        self.conditions
            .push(format!("{} <@ ${}", column, self.param_counter));
        self.params.push(json.to_string());
        self.param_counter += 1;
        self
    }

    /// Add a condition to check if a JSONB key exists
    ///
    /// Uses the `?` operator for key existence checking.
    ///
    /// # Example
    ///
    /// ```ignore
    /// query.where_jsonb_has_key("context", "user_id");
    /// ```
    pub fn where_jsonb_has_key(mut self, column: &str, key: &str) -> Self {
        self.conditions
            .push(format!("{} ? ${}", column, self.param_counter));
        self.params.push(key.to_string());
        self.param_counter += 1;
        self
    }

    /// Add a condition to check if any of the given keys exist
    ///
    /// Uses the `?|` operator for OR key existence checking.
    pub fn where_jsonb_has_any_key(mut self, column: &str, keys: &[&str]) -> Self {
        let keys_array = format!(
            "ARRAY[{}]",
            keys.iter()
                .map(|k| format!("'{}'", k))
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.conditions
            .push(format!("{} ?| {}", column, keys_array));
        self
    }

    /// Add a condition to check if all of the given keys exist
    ///
    /// Uses the `?&` operator for AND key existence checking.
    pub fn where_jsonb_has_all_keys(mut self, column: &str, keys: &[&str]) -> Self {
        let keys_array = format!(
            "ARRAY[{}]",
            keys.iter()
                .map(|k| format!("'{}'", k))
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.conditions
            .push(format!("{} ?& {}", column, keys_array));
        self
    }

    /// Add a condition to check a value at a specific JSON path
    ///
    /// Uses the `->>` operator to extract text and compare.
    ///
    /// # Example
    ///
    /// ```ignore
    /// query.where_jsonb_path_equals("context", &["user", "name"], "Alice");
    /// ```
    pub fn where_jsonb_path_equals(mut self, column: &str, path: &[&str], value: &str) -> Self {
        let path_expr = format!("{}->>{}", column, self.build_path(path));
        self.conditions
            .push(format!("{} = ${}", path_expr, self.param_counter));
        self.params.push(value.to_string());
        self.param_counter += 1;
        self
    }

    /// Add a condition to check if a value at a path matches a pattern
    ///
    /// Uses LIKE for pattern matching on extracted text.
    pub fn where_jsonb_path_like(mut self, column: &str, path: &[&str], pattern: &str) -> Self {
        let path_expr = format!("{}->>{}", column, self.build_path(path));
        self.conditions
            .push(format!("{} LIKE ${}", path_expr, self.param_counter));
        self.params.push(pattern.to_string());
        self.param_counter += 1;
        self
    }

    /// Add a condition to check if a numeric value at a path is within a range
    ///
    /// Uses the `->>` operator with CAST to numeric for comparison.
    pub fn where_jsonb_path_numeric_between(
        mut self,
        column: &str,
        path: &[&str],
        min: f64,
        max: f64,
    ) -> Self {
        let path_expr = format!("({}->>{})::numeric", column, self.build_path(path));
        self.conditions
            .push(format!("{} BETWEEN {} AND {}", path_expr, min, max));
        self
    }

    /// Build the SQL query
    pub fn build(self) -> BuiltQuery {
        let where_clause = if self.conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.conditions.join(" AND "))
        };

        let sql = format!("SELECT * FROM {}{}", self.table, where_clause);

        BuiltQuery {
            sql,
            params: self.params,
        }
    }

    /// Build a count query
    pub fn build_count(self) -> BuiltQuery {
        let where_clause = if self.conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.conditions.join(" AND "))
        };

        let sql = format!("SELECT COUNT(*) FROM {}{}", self.table, where_clause);

        BuiltQuery {
            sql,
            params: self.params,
        }
    }

    /// Helper to build JSON path expression
    fn build_path(&self, path: &[&str]) -> String {
        if path.is_empty() {
            return "'null'".to_string();
        }

        if path.len() == 1 {
            return format!("'{}'", path[0]);
        }

        // For nested paths: ->>'key1'->'key2'->'key3'
        let mut result = format!("'{}'", path[0]);
        for key in &path[1..path.len() - 1] {
            result.push_str(&format!("->'{}'", key));
        }
        result
    }
}

/// A built JSONB query with SQL and parameters
#[derive(Debug, Clone)]
pub struct BuiltQuery {
    /// The SQL query string
    pub sql: String,
    /// The query parameters (JSONB strings)
    pub params: Vec<String>,
}

/// Helper for building JSON paths
#[derive(Debug, Clone)]
pub struct JsonbPath {
    segments: Vec<String>,
}

impl JsonbPath {
    /// Create a new JSON path
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Add a key segment to the path
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.segments.push(key.into());
        self
    }

    /// Add an array index segment to the path
    pub fn index(mut self, index: usize) -> Self {
        self.segments.push(index.to_string());
        self
    }

    /// Build the path as a PostgreSQL JSON path expression
    pub fn build(&self) -> String {
        if self.segments.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        for (i, segment) in self.segments.iter().enumerate() {
            if i == 0 {
                result.push_str(&format!("'{}'", segment));
            } else if segment.parse::<usize>().is_ok() {
                result.push_str(&format!("->{}", segment));
            } else {
                result.push_str(&format!("->'{}'", segment));
            }
        }
        result
    }

    /// Build the path as a text extraction expression (using ->>)
    pub fn build_text_extract(&self) -> String {
        if self.segments.is_empty() {
            return String::new();
        }

        if self.segments.len() == 1 {
            return format!("'{}'", self.segments[0]);
        }

        let mut result = format!("'{}'", self.segments[0]);
        for segment in &self.segments[1..self.segments.len() - 1] {
            if segment.parse::<usize>().is_ok() {
                result.push_str(&format!("->{}", segment));
            } else {
                result.push_str(&format!("->'{}'", segment));
            }
        }
        result
    }
}

impl Default for JsonbPath {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for common JSONB operations
pub mod helpers {
    use super::*;

    /// Build a SQL fragment to check if a JSONB column contains a key-value pair
    pub fn contains_kv(column: &str, key: &str, value: &JsonValue) -> String {
        let json_obj = serde_json::json!({ key: value });
        format!("{} @> '{}'::jsonb", column, json_obj)
    }

    /// Build a SQL fragment to extract a text value from a JSONB path
    pub fn extract_text(column: &str, path: &[&str]) -> String {
        if path.is_empty() {
            return column.to_string();
        }

        let mut result = column.to_string();
        for (i, segment) in path.iter().enumerate() {
            if i == path.len() - 1 {
                result.push_str(&format!("->>'{}'", segment));
            } else {
                result.push_str(&format!("->'{}'", segment));
            }
        }
        result
    }

    /// Build a SQL fragment to extract a JSONB value from a path
    pub fn extract_jsonb(column: &str, path: &[&str]) -> String {
        if path.is_empty() {
            return column.to_string();
        }

        let mut result = column.to_string();
        for segment in path {
            result.push_str(&format!("->'{}'", segment));
        }
        result
    }

    /// Build a SQL fragment to check if a JSONB array contains a value
    pub fn array_contains(column: &str, value: &JsonValue) -> String {
        format!("{} @> '[{}]'::jsonb", column, value)
    }

    /// Build a SQL fragment to get the length of a JSONB array
    pub fn array_length(column: &str) -> String {
        format!("jsonb_array_length({})", column)
    }

    /// Build a SQL fragment to check if a JSONB value is null
    pub fn is_null(column: &str, path: &[&str]) -> String {
        let path_expr = extract_jsonb(column, path);
        format!("{} = 'null'::jsonb", path_expr)
    }

    /// Build a SQL fragment for full-text search in JSONB values
    pub fn text_search(column: &str, path: &[&str], query: &str) -> String {
        let text_expr = extract_text(column, path);
        format!(
            "to_tsvector('english', {}) @@ plainto_tsquery('english', '{}')",
            text_expr, query
        )
    }
}

/// SQL snippets for creating JSONB indexes
pub mod index_hints {
    /// SQL to create a GIN index for JSONB containment queries
    ///
    /// Optimizes queries using `@>`, `<@`, `?`, `?|`, `?&` operators.
    pub fn gin_index(table: &str, column: &str) -> String {
        format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_{}  ON {} USING GIN ({})",
            table, column, table, column
        )
    }

    /// SQL to create a GIN index with jsonb_path_ops for containment queries only
    ///
    /// More efficient but only supports `@>` operator.
    pub fn gin_path_ops_index(table: &str, column: &str) -> String {
        format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_{}_path_ops ON {} USING GIN ({} jsonb_path_ops)",
            table, column, table, column
        )
    }

    /// SQL to create a B-tree index on a specific JSONB field
    ///
    /// Optimizes queries that extract and compare specific fields.
    pub fn btree_field_index(table: &str, column: &str, field: &str) -> String {
        format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_{}_{}  ON {} (({}->>'{}'))",
            table, column, field, table, column, field
        )
    }

    /// SQL to create an expression index for numeric comparisons
    pub fn numeric_field_index(table: &str, column: &str, field: &str) -> String {
        format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_{}_{}  ON {} ((({}->'{}')::numeric))",
            table, column, field, table, column, field
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonb_contains_query() {
        let query = JsonbQuery::new("executions")
            .where_jsonb_contains("context", r#"{"status": "active"}"#)
            .build();

        assert!(query.sql.contains("context @> $1"));
        assert_eq!(query.params.len(), 1);
    }

    #[test]
    fn test_jsonb_has_key_query() {
        let query = JsonbQuery::new("workflows")
            .where_jsonb_has_key("metadata", "version")
            .build();

        assert!(query.sql.contains("metadata ? $1"));
        assert_eq!(query.params[0], "version");
    }

    #[test]
    fn test_jsonb_path_equals_query() {
        let query = JsonbQuery::new("executions")
            .where_jsonb_path_equals("context", &["user", "role"], "admin")
            .build();

        assert!(query.sql.contains("context->>'user' = $1"));
        assert_eq!(query.params[0], "admin");
    }

    #[test]
    fn test_jsonb_multiple_conditions() {
        let query = JsonbQuery::new("executions")
            .where_jsonb_contains("context", r#"{"status": "active"}"#)
            .where_jsonb_has_key("context", "user_id")
            .build();

        assert!(query.sql.contains("WHERE"));
        assert!(query.sql.contains("AND"));
        assert_eq!(query.params.len(), 2);
    }

    #[test]
    fn test_jsonb_path_builder() {
        let path = JsonbPath::new().key("user").key("profile").key("name");
        let built = path.build();

        assert!(built.contains("'user'"));
        assert!(built.contains("'profile'"));
        assert!(built.contains("'name'"));
    }

    #[test]
    fn test_jsonb_path_with_index() {
        let path = JsonbPath::new().key("items").index(0).key("value");
        let built = path.build();

        assert!(built.contains("'items'"));
        assert!(built.contains("->0"));
        assert!(built.contains("'value'"));
    }

    #[test]
    fn test_count_query() {
        let query = JsonbQuery::new("executions")
            .where_jsonb_has_key("context", "workflow_id")
            .build_count();

        assert!(query.sql.contains("SELECT COUNT(*)"));
        assert!(query.sql.contains("FROM executions"));
    }

    #[test]
    fn test_helper_contains_kv() {
        let value = serde_json::json!("test_value");
        let sql = helpers::contains_kv("context", "key", &value);

        assert!(sql.contains("context @>"));
        assert!(sql.contains("test_value"));
    }

    #[test]
    fn test_helper_extract_text() {
        let sql = helpers::extract_text("context", &["user", "name"]);
        assert!(sql.contains("context"));
        assert!(sql.contains("->'user'"));
        assert!(sql.contains("->>'name'"));
    }

    #[test]
    fn test_helper_array_length() {
        let sql = helpers::array_length("items");
        assert_eq!(sql, "jsonb_array_length(items)");
    }

    #[test]
    fn test_gin_index_creation() {
        let sql = index_hints::gin_index("executions", "context");
        assert!(sql.contains("CREATE INDEX"));
        assert!(sql.contains("USING GIN"));
        assert!(sql.contains("executions"));
        assert!(sql.contains("context"));
    }

    #[test]
    fn test_btree_field_index_creation() {
        let sql = index_hints::btree_field_index("executions", "context", "workflow_id");
        assert!(sql.contains("CREATE INDEX"));
        assert!(sql.contains("->>'workflow_id'"));
    }

    #[test]
    fn test_has_any_key() {
        let query = JsonbQuery::new("workflows")
            .where_jsonb_has_any_key("metadata", &["version", "author"])
            .build();

        assert!(query.sql.contains("metadata ?|"));
        assert!(query.sql.contains("ARRAY"));
    }

    #[test]
    fn test_has_all_keys() {
        let query = JsonbQuery::new("workflows")
            .where_jsonb_has_all_keys("metadata", &["version", "author", "timestamp"])
            .build();

        assert!(query.sql.contains("metadata ?&"));
        assert!(query.sql.contains("ARRAY"));
    }
}
