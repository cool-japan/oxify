//! Metadata filtering for vector search
//!
//! Provides pre-filtering and post-filtering capabilities to constrain
//! search results based on metadata attributes.
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::filter::{Filter, FilterCondition, FilterValue};
//!
//! // Filter by document type
//! let filter = Filter::new()
//!     .eq("type", "article")
//!     .gte("year", 2020);
//!
//! // Filter with OR conditions
//! let filter = Filter::any(vec![
//!     Filter::new().eq("category", "tech"),
//!     Filter::new().eq("category", "science"),
//! ]);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Value types for metadata filtering
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterValue {
    /// String value
    String(String),
    /// Integer value
    Int(i64),
    /// Float value
    Float(f64),
    /// Boolean value
    Bool(bool),
    /// List of strings (for IN operator)
    StringList(Vec<String>),
    /// List of integers (for IN operator)
    IntList(Vec<i64>),
}

impl From<&str> for FilterValue {
    fn from(s: &str) -> Self {
        FilterValue::String(s.to_string())
    }
}

impl From<String> for FilterValue {
    fn from(s: String) -> Self {
        FilterValue::String(s)
    }
}

impl From<i64> for FilterValue {
    fn from(v: i64) -> Self {
        FilterValue::Int(v)
    }
}

impl From<i32> for FilterValue {
    fn from(v: i32) -> Self {
        FilterValue::Int(v as i64)
    }
}

impl From<f64> for FilterValue {
    fn from(v: f64) -> Self {
        FilterValue::Float(v)
    }
}

impl From<bool> for FilterValue {
    fn from(v: bool) -> Self {
        FilterValue::Bool(v)
    }
}

impl From<Vec<String>> for FilterValue {
    fn from(v: Vec<String>) -> Self {
        FilterValue::StringList(v)
    }
}

impl From<Vec<&str>> for FilterValue {
    fn from(v: Vec<&str>) -> Self {
        FilterValue::StringList(v.into_iter().map(|s| s.to_string()).collect())
    }
}

impl From<Vec<i64>> for FilterValue {
    fn from(v: Vec<i64>) -> Self {
        FilterValue::IntList(v)
    }
}

/// Filter condition operators
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterCondition {
    /// Equal to
    Eq(String, FilterValue),
    /// Not equal to
    Ne(String, FilterValue),
    /// Greater than
    Gt(String, FilterValue),
    /// Greater than or equal to
    Gte(String, FilterValue),
    /// Less than
    Lt(String, FilterValue),
    /// Less than or equal to
    Lte(String, FilterValue),
    /// Value is in list
    In(String, FilterValue),
    /// Value is not in list
    NotIn(String, FilterValue),
    /// Field contains substring
    Contains(String, String),
    /// Field starts with prefix
    StartsWith(String, String),
    /// All conditions must match (AND)
    All(Vec<FilterCondition>),
    /// Any condition must match (OR)
    Any(Vec<FilterCondition>),
    /// Negate condition (NOT)
    Not(Box<FilterCondition>),
}

/// Metadata filter builder
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Filter {
    conditions: Vec<FilterCondition>,
}

impl Filter {
    /// Create a new empty filter
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }

    /// Create a filter that matches ALL conditions (AND)
    pub fn all(filters: Vec<Filter>) -> Self {
        let conditions: Vec<FilterCondition> =
            filters.into_iter().flat_map(|f| f.conditions).collect();
        Self { conditions }
    }

    /// Create a filter that matches ANY condition (OR)
    pub fn any(filters: Vec<Filter>) -> Self {
        let inner: Vec<FilterCondition> = filters
            .into_iter()
            .map(|f| {
                if f.conditions.len() == 1 {
                    f.conditions
                        .into_iter()
                        .next()
                        .expect("invariant: conditions.len() == 1 guard passed")
                } else {
                    FilterCondition::All(f.conditions)
                }
            })
            .collect();
        Self {
            conditions: vec![FilterCondition::Any(inner)],
        }
    }

    /// Add equality condition
    pub fn eq<K: Into<String>, V: Into<FilterValue>>(mut self, key: K, value: V) -> Self {
        self.conditions
            .push(FilterCondition::Eq(key.into(), value.into()));
        self
    }

    /// Add not-equal condition
    pub fn ne<K: Into<String>, V: Into<FilterValue>>(mut self, key: K, value: V) -> Self {
        self.conditions
            .push(FilterCondition::Ne(key.into(), value.into()));
        self
    }

    /// Add greater-than condition
    pub fn gt<K: Into<String>, V: Into<FilterValue>>(mut self, key: K, value: V) -> Self {
        self.conditions
            .push(FilterCondition::Gt(key.into(), value.into()));
        self
    }

    /// Add greater-than-or-equal condition
    pub fn gte<K: Into<String>, V: Into<FilterValue>>(mut self, key: K, value: V) -> Self {
        self.conditions
            .push(FilterCondition::Gte(key.into(), value.into()));
        self
    }

    /// Add less-than condition
    pub fn lt<K: Into<String>, V: Into<FilterValue>>(mut self, key: K, value: V) -> Self {
        self.conditions
            .push(FilterCondition::Lt(key.into(), value.into()));
        self
    }

    /// Add less-than-or-equal condition
    pub fn lte<K: Into<String>, V: Into<FilterValue>>(mut self, key: K, value: V) -> Self {
        self.conditions
            .push(FilterCondition::Lte(key.into(), value.into()));
        self
    }

    /// Add IN condition (value must be in list)
    pub fn in_list<K: Into<String>, V: Into<FilterValue>>(mut self, key: K, values: V) -> Self {
        self.conditions
            .push(FilterCondition::In(key.into(), values.into()));
        self
    }

    /// Add NOT IN condition (value must not be in list)
    pub fn not_in<K: Into<String>, V: Into<FilterValue>>(mut self, key: K, values: V) -> Self {
        self.conditions
            .push(FilterCondition::NotIn(key.into(), values.into()));
        self
    }

    /// Add contains condition (string contains substring)
    pub fn contains<K: Into<String>, V: Into<String>>(mut self, key: K, substring: V) -> Self {
        self.conditions
            .push(FilterCondition::Contains(key.into(), substring.into()));
        self
    }

    /// Add starts_with condition
    pub fn starts_with<K: Into<String>, V: Into<String>>(mut self, key: K, prefix: V) -> Self {
        self.conditions
            .push(FilterCondition::StartsWith(key.into(), prefix.into()));
        self
    }

    /// Get the conditions
    pub fn conditions(&self) -> &[FilterCondition] {
        &self.conditions
    }

    /// Check if filter is empty
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }

    /// Evaluate filter against metadata
    pub fn matches(&self, metadata: &Metadata) -> bool {
        self.conditions
            .iter()
            .all(|c| evaluate_condition(c, metadata))
    }
}

/// Metadata storage for a single entity
pub type Metadata = HashMap<String, FilterValue>;

/// Evaluate a single condition against metadata
fn evaluate_condition(condition: &FilterCondition, metadata: &Metadata) -> bool {
    match condition {
        FilterCondition::Eq(key, expected) => metadata.get(key) == Some(expected),
        FilterCondition::Ne(key, expected) => metadata.get(key) != Some(expected),
        FilterCondition::Gt(key, expected) => metadata
            .get(key)
            .is_some_and(|v| compare_values(v, expected) == Some(std::cmp::Ordering::Greater)),
        FilterCondition::Gte(key, expected) => metadata.get(key).is_some_and(|v| {
            matches!(
                compare_values(v, expected),
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            )
        }),
        FilterCondition::Lt(key, expected) => metadata
            .get(key)
            .is_some_and(|v| compare_values(v, expected) == Some(std::cmp::Ordering::Less)),
        FilterCondition::Lte(key, expected) => metadata.get(key).is_some_and(|v| {
            matches!(
                compare_values(v, expected),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            )
        }),
        FilterCondition::In(key, values) => {
            metadata.get(key).is_some_and(|v| value_in_list(v, values))
        }
        FilterCondition::NotIn(key, values) => {
            metadata.get(key).is_none_or(|v| !value_in_list(v, values))
        }
        FilterCondition::Contains(key, substring) => metadata.get(key).is_some_and(|v| {
            if let FilterValue::String(s) = v {
                s.contains(substring)
            } else {
                false
            }
        }),
        FilterCondition::StartsWith(key, prefix) => metadata.get(key).is_some_and(|v| {
            if let FilterValue::String(s) = v {
                s.starts_with(prefix)
            } else {
                false
            }
        }),
        FilterCondition::All(conditions) => {
            conditions.iter().all(|c| evaluate_condition(c, metadata))
        }
        FilterCondition::Any(conditions) => {
            conditions.iter().any(|c| evaluate_condition(c, metadata))
        }
        FilterCondition::Not(condition) => !evaluate_condition(condition, metadata),
    }
}

/// Compare two filter values
fn compare_values(a: &FilterValue, b: &FilterValue) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (FilterValue::Int(a), FilterValue::Int(b)) => Some(a.cmp(b)),
        (FilterValue::Float(a), FilterValue::Float(b)) => a.partial_cmp(b),
        (FilterValue::Int(a), FilterValue::Float(b)) => (*a as f64).partial_cmp(b),
        (FilterValue::Float(a), FilterValue::Int(b)) => a.partial_cmp(&(*b as f64)),
        (FilterValue::String(a), FilterValue::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Check if value is in list
fn value_in_list(value: &FilterValue, list: &FilterValue) -> bool {
    match (value, list) {
        (FilterValue::String(v), FilterValue::StringList(l)) => l.contains(v),
        (FilterValue::Int(v), FilterValue::IntList(l)) => l.contains(v),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata() -> Metadata {
        let mut m = HashMap::new();
        m.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        m.insert("year".to_string(), FilterValue::Int(2023));
        m.insert("score".to_string(), FilterValue::Float(0.95));
        m.insert("published".to_string(), FilterValue::Bool(true));
        m.insert(
            "category".to_string(),
            FilterValue::String("tech".to_string()),
        );
        m
    }

    #[test]
    fn test_eq_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().eq("type", "article");
        assert!(filter.matches(&metadata));

        let filter = Filter::new().eq("type", "book");
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_ne_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().ne("type", "book");
        assert!(filter.matches(&metadata));

        let filter = Filter::new().ne("type", "article");
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_gt_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().gt("year", 2020i64);
        assert!(filter.matches(&metadata));

        let filter = Filter::new().gt("year", 2023i64);
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_gte_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().gte("year", 2023i64);
        assert!(filter.matches(&metadata));

        let filter = Filter::new().gte("year", 2024i64);
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_lt_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().lt("year", 2026i64);
        assert!(filter.matches(&metadata));

        let filter = Filter::new().lt("year", 2023i64);
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_lte_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().lte("year", 2023i64);
        assert!(filter.matches(&metadata));

        let filter = Filter::new().lte("year", 2022i64);
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_in_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().in_list("type", vec!["article", "book"]);
        assert!(filter.matches(&metadata));

        let filter = Filter::new().in_list("type", vec!["book", "journal"]);
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_not_in_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().not_in("type", vec!["book", "journal"]);
        assert!(filter.matches(&metadata));

        let filter = Filter::new().not_in("type", vec!["article", "journal"]);
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_contains_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().contains("type", "art");
        assert!(filter.matches(&metadata));

        let filter = Filter::new().contains("type", "xyz");
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_starts_with_filter() {
        let metadata = test_metadata();

        let filter = Filter::new().starts_with("type", "art");
        assert!(filter.matches(&metadata));

        let filter = Filter::new().starts_with("type", "icle");
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_combined_filters() {
        let metadata = test_metadata();

        // All conditions must match (AND)
        let filter = Filter::new().eq("type", "article").gte("year", 2020i64);
        assert!(filter.matches(&metadata));

        let filter = Filter::new().eq("type", "article").gt("year", 2026i64);
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_or_filters() {
        let metadata = test_metadata();

        // Any condition can match (OR)
        let filter = Filter::any(vec![
            Filter::new().eq("category", "tech"),
            Filter::new().eq("category", "science"),
        ]);
        assert!(filter.matches(&metadata));

        let filter = Filter::any(vec![
            Filter::new().eq("category", "sports"),
            Filter::new().eq("category", "music"),
        ]);
        assert!(!filter.matches(&metadata));
    }

    #[test]
    fn test_missing_field() {
        let metadata = test_metadata();

        // Missing field returns false for eq
        let filter = Filter::new().eq("nonexistent", "value");
        assert!(!filter.matches(&metadata));

        // Missing field returns true for ne
        let filter = Filter::new().ne("nonexistent", "value");
        assert!(filter.matches(&metadata));
    }

    #[test]
    fn test_float_comparison() {
        let metadata = test_metadata();

        let filter = Filter::new().gt("score", 0.9);
        assert!(filter.matches(&metadata));

        let filter = Filter::new().lt("score", 1.0);
        assert!(filter.matches(&metadata));
    }

    #[test]
    fn test_filter_is_empty() {
        let filter = Filter::new();
        assert!(filter.is_empty());

        let filter = Filter::new().eq("key", "value");
        assert!(!filter.is_empty());
    }
}
