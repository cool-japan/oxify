//! Filter expression language for vector search
//!
//! This module provides a unified filter expression language that can be
//! translated to various vector database query formats.

use serde::{Deserialize, Serialize};

/// Filter expression for metadata filtering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FilterExpr {
    /// Equality comparison: field == value
    Eq { field: String, value: FilterValue },
    /// Not equal: field != value
    Ne { field: String, value: FilterValue },
    /// Greater than: field > value
    Gt { field: String, value: FilterValue },
    /// Greater than or equal: field >= value
    Gte { field: String, value: FilterValue },
    /// Less than: field < value
    Lt { field: String, value: FilterValue },
    /// Less than or equal: field <= value
    Lte { field: String, value: FilterValue },
    /// Field contains value (for strings)
    Contains { field: String, value: String },
    /// Field starts with value (for strings)
    StartsWith { field: String, value: String },
    /// Field ends with value (for strings)
    EndsWith { field: String, value: String },
    /// Field value is in list
    In {
        field: String,
        values: Vec<FilterValue>,
    },
    /// Field value is not in list
    NotIn {
        field: String,
        values: Vec<FilterValue>,
    },
    /// Field exists and is not null
    Exists { field: String },
    /// Field is null or doesn't exist
    IsNull { field: String },
    /// Logical AND of multiple expressions
    And { exprs: Vec<FilterExpr> },
    /// Logical OR of multiple expressions
    Or { exprs: Vec<FilterExpr> },
    /// Logical NOT of expression
    Not { expr: Box<FilterExpr> },
}

/// Value types that can be used in filter expressions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FilterValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
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
    fn from(n: i64) -> Self {
        FilterValue::Int(n)
    }
}

impl From<i32> for FilterValue {
    fn from(n: i32) -> Self {
        FilterValue::Int(n as i64)
    }
}

impl From<f64> for FilterValue {
    fn from(n: f64) -> Self {
        FilterValue::Float(n)
    }
}

impl From<bool> for FilterValue {
    fn from(b: bool) -> Self {
        FilterValue::Bool(b)
    }
}

impl FilterExpr {
    /// Create an equality filter
    pub fn eq(field: impl Into<String>, value: impl Into<FilterValue>) -> Self {
        FilterExpr::Eq {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a not-equal filter
    pub fn ne(field: impl Into<String>, value: impl Into<FilterValue>) -> Self {
        FilterExpr::Ne {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a greater-than filter
    pub fn gt(field: impl Into<String>, value: impl Into<FilterValue>) -> Self {
        FilterExpr::Gt {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a greater-than-or-equal filter
    pub fn gte(field: impl Into<String>, value: impl Into<FilterValue>) -> Self {
        FilterExpr::Gte {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a less-than filter
    pub fn lt(field: impl Into<String>, value: impl Into<FilterValue>) -> Self {
        FilterExpr::Lt {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a less-than-or-equal filter
    pub fn lte(field: impl Into<String>, value: impl Into<FilterValue>) -> Self {
        FilterExpr::Lte {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a contains filter (for strings)
    pub fn contains(field: impl Into<String>, value: impl Into<String>) -> Self {
        FilterExpr::Contains {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create an IN filter
    pub fn in_list(field: impl Into<String>, values: Vec<FilterValue>) -> Self {
        FilterExpr::In {
            field: field.into(),
            values,
        }
    }

    /// Create a NOT IN filter
    pub fn not_in(field: impl Into<String>, values: Vec<FilterValue>) -> Self {
        FilterExpr::NotIn {
            field: field.into(),
            values,
        }
    }

    /// Create an AND filter
    pub fn and(exprs: Vec<FilterExpr>) -> Self {
        FilterExpr::And { exprs }
    }

    /// Create an OR filter
    pub fn or(exprs: Vec<FilterExpr>) -> Self {
        FilterExpr::Or { exprs }
    }

    /// Create a NOT filter
    pub fn negate(expr: FilterExpr) -> Self {
        FilterExpr::Not {
            expr: Box::new(expr),
        }
    }

    /// Convert to JSON value for generic provider usage
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// Convert to Qdrant filter format
    pub fn to_qdrant(&self) -> serde_json::Value {
        match self {
            FilterExpr::Eq { field, value } => {
                serde_json::json!({
                    "must": [{
                        "key": field,
                        "match": { "value": value_to_json(value) }
                    }]
                })
            }
            FilterExpr::Ne { field, value } => {
                serde_json::json!({
                    "must_not": [{
                        "key": field,
                        "match": { "value": value_to_json(value) }
                    }]
                })
            }
            FilterExpr::Gt { field, value } => {
                serde_json::json!({
                    "must": [{
                        "key": field,
                        "range": { "gt": value_to_json(value) }
                    }]
                })
            }
            FilterExpr::Gte { field, value } => {
                serde_json::json!({
                    "must": [{
                        "key": field,
                        "range": { "gte": value_to_json(value) }
                    }]
                })
            }
            FilterExpr::Lt { field, value } => {
                serde_json::json!({
                    "must": [{
                        "key": field,
                        "range": { "lt": value_to_json(value) }
                    }]
                })
            }
            FilterExpr::Lte { field, value } => {
                serde_json::json!({
                    "must": [{
                        "key": field,
                        "range": { "lte": value_to_json(value) }
                    }]
                })
            }
            FilterExpr::In { field, values } => {
                let vals: Vec<serde_json::Value> = values.iter().map(value_to_json).collect();
                serde_json::json!({
                    "must": [{
                        "key": field,
                        "match": { "any": vals }
                    }]
                })
            }
            FilterExpr::And { exprs } => {
                let must: Vec<serde_json::Value> = exprs.iter().map(|e| e.to_qdrant()).collect();
                serde_json::json!({ "must": must })
            }
            FilterExpr::Or { exprs } => {
                let should: Vec<serde_json::Value> = exprs.iter().map(|e| e.to_qdrant()).collect();
                serde_json::json!({ "should": should })
            }
            FilterExpr::Not { expr } => {
                serde_json::json!({ "must_not": [expr.to_qdrant()] })
            }
            _ => serde_json::Value::Null,
        }
    }

    /// Convert to Milvus filter expression string
    pub fn to_milvus(&self) -> String {
        match self {
            FilterExpr::Eq { field, value } => {
                format!("{} == {}", field, value_to_milvus(value))
            }
            FilterExpr::Ne { field, value } => {
                format!("{} != {}", field, value_to_milvus(value))
            }
            FilterExpr::Gt { field, value } => {
                format!("{} > {}", field, value_to_milvus(value))
            }
            FilterExpr::Gte { field, value } => {
                format!("{} >= {}", field, value_to_milvus(value))
            }
            FilterExpr::Lt { field, value } => {
                format!("{} < {}", field, value_to_milvus(value))
            }
            FilterExpr::Lte { field, value } => {
                format!("{} <= {}", field, value_to_milvus(value))
            }
            FilterExpr::In { field, values } => {
                let vals: Vec<String> = values.iter().map(value_to_milvus).collect();
                format!("{} in [{}]", field, vals.join(", "))
            }
            FilterExpr::NotIn { field, values } => {
                let vals: Vec<String> = values.iter().map(value_to_milvus).collect();
                format!("{} not in [{}]", field, vals.join(", "))
            }
            FilterExpr::And { exprs } => {
                let parts: Vec<String> = exprs.iter().map(|e| e.to_milvus()).collect();
                format!("({})", parts.join(" and "))
            }
            FilterExpr::Or { exprs } => {
                let parts: Vec<String> = exprs.iter().map(|e| e.to_milvus()).collect();
                format!("({})", parts.join(" or "))
            }
            FilterExpr::Not { expr } => {
                format!("not ({})", expr.to_milvus())
            }
            FilterExpr::Contains { field, value } => {
                format!("{} like \"%{}%\"", field, value)
            }
            FilterExpr::StartsWith { field, value } => {
                format!("{} like \"{}%\"", field, value)
            }
            FilterExpr::EndsWith { field, value } => {
                format!("{} like \"%{}\"", field, value)
            }
            FilterExpr::Exists { field } => {
                format!("{} != \"\"", field)
            }
            FilterExpr::IsNull { field } => {
                format!("{} == \"\"", field)
            }
        }
    }

    /// Convert to Pinecone filter format
    pub fn to_pinecone(&self) -> serde_json::Value {
        match self {
            FilterExpr::Eq { field, value } => {
                serde_json::json!({ field: { "$eq": value_to_json(value) } })
            }
            FilterExpr::Ne { field, value } => {
                serde_json::json!({ field: { "$ne": value_to_json(value) } })
            }
            FilterExpr::Gt { field, value } => {
                serde_json::json!({ field: { "$gt": value_to_json(value) } })
            }
            FilterExpr::Gte { field, value } => {
                serde_json::json!({ field: { "$gte": value_to_json(value) } })
            }
            FilterExpr::Lt { field, value } => {
                serde_json::json!({ field: { "$lt": value_to_json(value) } })
            }
            FilterExpr::Lte { field, value } => {
                serde_json::json!({ field: { "$lte": value_to_json(value) } })
            }
            FilterExpr::In { field, values } => {
                let vals: Vec<serde_json::Value> = values.iter().map(value_to_json).collect();
                serde_json::json!({ field: { "$in": vals } })
            }
            FilterExpr::NotIn { field, values } => {
                let vals: Vec<serde_json::Value> = values.iter().map(value_to_json).collect();
                serde_json::json!({ field: { "$nin": vals } })
            }
            FilterExpr::And { exprs } => {
                let parts: Vec<serde_json::Value> = exprs.iter().map(|e| e.to_pinecone()).collect();
                serde_json::json!({ "$and": parts })
            }
            FilterExpr::Or { exprs } => {
                let parts: Vec<serde_json::Value> = exprs.iter().map(|e| e.to_pinecone()).collect();
                serde_json::json!({ "$or": parts })
            }
            _ => serde_json::Value::Null,
        }
    }

    /// Convert to Weaviate GraphQL where clause
    pub fn to_weaviate(&self) -> String {
        match self {
            FilterExpr::Eq { field, value } => {
                let (op, val) = value_to_weaviate(value);
                format!(
                    r#"{{ path: ["{}"], operator: Equal, {}: {} }}"#,
                    field, op, val
                )
            }
            FilterExpr::Ne { field, value } => {
                let (op, val) = value_to_weaviate(value);
                format!(
                    r#"{{ path: ["{}"], operator: NotEqual, {}: {} }}"#,
                    field, op, val
                )
            }
            FilterExpr::Gt { field, value } => {
                let (op, val) = value_to_weaviate(value);
                format!(
                    r#"{{ path: ["{}"], operator: GreaterThan, {}: {} }}"#,
                    field, op, val
                )
            }
            FilterExpr::Gte { field, value } => {
                let (op, val) = value_to_weaviate(value);
                format!(
                    r#"{{ path: ["{}"], operator: GreaterThanEqual, {}: {} }}"#,
                    field, op, val
                )
            }
            FilterExpr::Lt { field, value } => {
                let (op, val) = value_to_weaviate(value);
                format!(
                    r#"{{ path: ["{}"], operator: LessThan, {}: {} }}"#,
                    field, op, val
                )
            }
            FilterExpr::Lte { field, value } => {
                let (op, val) = value_to_weaviate(value);
                format!(
                    r#"{{ path: ["{}"], operator: LessThanEqual, {}: {} }}"#,
                    field, op, val
                )
            }
            FilterExpr::Contains { field, value } => {
                format!(
                    r#"{{ path: ["{}"], operator: Like, valueText: "*{}*" }}"#,
                    field, value
                )
            }
            FilterExpr::And { exprs } => {
                let parts: Vec<String> = exprs.iter().map(|e| e.to_weaviate()).collect();
                format!(r#"{{ operator: And, operands: [{}] }}"#, parts.join(", "))
            }
            FilterExpr::Or { exprs } => {
                let parts: Vec<String> = exprs.iter().map(|e| e.to_weaviate()).collect();
                format!(r#"{{ operator: Or, operands: [{}] }}"#, parts.join(", "))
            }
            FilterExpr::Not { expr } => {
                format!(r#"{{ operator: Not, operands: [{}] }}"#, expr.to_weaviate())
            }
            _ => String::new(),
        }
    }

    /// Convert to ChromaDB where clause
    pub fn to_chromadb(&self) -> serde_json::Value {
        match self {
            FilterExpr::Eq { field, value } => {
                serde_json::json!({ field: value_to_json(value) })
            }
            FilterExpr::Ne { field, value } => {
                serde_json::json!({ field: { "$ne": value_to_json(value) } })
            }
            FilterExpr::Gt { field, value } => {
                serde_json::json!({ field: { "$gt": value_to_json(value) } })
            }
            FilterExpr::Gte { field, value } => {
                serde_json::json!({ field: { "$gte": value_to_json(value) } })
            }
            FilterExpr::Lt { field, value } => {
                serde_json::json!({ field: { "$lt": value_to_json(value) } })
            }
            FilterExpr::Lte { field, value } => {
                serde_json::json!({ field: { "$lte": value_to_json(value) } })
            }
            FilterExpr::In { field, values } => {
                let vals: Vec<serde_json::Value> = values.iter().map(value_to_json).collect();
                serde_json::json!({ field: { "$in": vals } })
            }
            FilterExpr::NotIn { field, values } => {
                let vals: Vec<serde_json::Value> = values.iter().map(value_to_json).collect();
                serde_json::json!({ field: { "$nin": vals } })
            }
            FilterExpr::And { exprs } => {
                let parts: Vec<serde_json::Value> = exprs.iter().map(|e| e.to_chromadb()).collect();
                serde_json::json!({ "$and": parts })
            }
            FilterExpr::Or { exprs } => {
                let parts: Vec<serde_json::Value> = exprs.iter().map(|e| e.to_chromadb()).collect();
                serde_json::json!({ "$or": parts })
            }
            _ => serde_json::Value::Null,
        }
    }

    /// Convert to pgvector SQL WHERE clause
    pub fn to_pgvector(&self, param_offset: usize) -> (String, Vec<serde_json::Value>) {
        let mut params = Vec::new();
        let sql = self.to_pgvector_inner(&mut params, param_offset);
        (sql, params)
    }

    fn to_pgvector_inner(&self, params: &mut Vec<serde_json::Value>, offset: usize) -> String {
        let idx = offset + params.len() + 1;
        match self {
            FilterExpr::Eq { field, value } => {
                params.push(value_to_json(value));
                format!("payload->>'{}' = ${}", field, idx)
            }
            FilterExpr::Ne { field, value } => {
                params.push(value_to_json(value));
                format!("payload->>'{}' != ${}", field, idx)
            }
            FilterExpr::Gt { field, value } => {
                params.push(value_to_json(value));
                format!("(payload->>'{0}')::numeric > ${1}", field, idx)
            }
            FilterExpr::Gte { field, value } => {
                params.push(value_to_json(value));
                format!("(payload->>'{0}')::numeric >= ${1}", field, idx)
            }
            FilterExpr::Lt { field, value } => {
                params.push(value_to_json(value));
                format!("(payload->>'{0}')::numeric < ${1}", field, idx)
            }
            FilterExpr::Lte { field, value } => {
                params.push(value_to_json(value));
                format!("(payload->>'{0}')::numeric <= ${1}", field, idx)
            }
            FilterExpr::Contains { field, value } => {
                params.push(serde_json::json!(format!("%{}%", value)));
                format!("payload->>'{}' LIKE ${}", field, idx)
            }
            FilterExpr::StartsWith { field, value } => {
                params.push(serde_json::json!(format!("{}%", value)));
                format!("payload->>'{}' LIKE ${}", field, idx)
            }
            FilterExpr::EndsWith { field, value } => {
                params.push(serde_json::json!(format!("%{}", value)));
                format!("payload->>'{}' LIKE ${}", field, idx)
            }
            FilterExpr::In { field, values } => {
                let placeholders: Vec<String> = values
                    .iter()
                    .map(|v| {
                        params.push(value_to_json(v));
                        format!("${}", offset + params.len())
                    })
                    .collect();
                format!("payload->>'{}' IN ({})", field, placeholders.join(", "))
            }
            FilterExpr::NotIn { field, values } => {
                let placeholders: Vec<String> = values
                    .iter()
                    .map(|v| {
                        params.push(value_to_json(v));
                        format!("${}", offset + params.len())
                    })
                    .collect();
                format!("payload->>'{}' NOT IN ({})", field, placeholders.join(", "))
            }
            FilterExpr::Exists { field } => {
                format!("payload ? '{}'", field)
            }
            FilterExpr::IsNull { field } => {
                format!("payload->>'{}' IS NULL", field)
            }
            FilterExpr::And { exprs } => {
                let parts: Vec<String> = exprs
                    .iter()
                    .map(|e| e.to_pgvector_inner(params, offset))
                    .collect();
                format!("({})", parts.join(" AND "))
            }
            FilterExpr::Or { exprs } => {
                let parts: Vec<String> = exprs
                    .iter()
                    .map(|e| e.to_pgvector_inner(params, offset))
                    .collect();
                format!("({})", parts.join(" OR "))
            }
            FilterExpr::Not { expr } => {
                format!("NOT ({})", expr.to_pgvector_inner(params, offset))
            }
        }
    }
}

fn value_to_json(value: &FilterValue) -> serde_json::Value {
    match value {
        FilterValue::String(s) => serde_json::json!(s),
        FilterValue::Int(n) => serde_json::json!(n),
        FilterValue::Float(n) => serde_json::json!(n),
        FilterValue::Bool(b) => serde_json::json!(b),
    }
}

fn value_to_milvus(value: &FilterValue) -> String {
    match value {
        FilterValue::String(s) => format!("\"{}\"", s),
        FilterValue::Int(n) => n.to_string(),
        FilterValue::Float(n) => n.to_string(),
        FilterValue::Bool(b) => b.to_string(),
    }
}

fn value_to_weaviate(value: &FilterValue) -> (&'static str, String) {
    match value {
        FilterValue::String(s) => ("valueText", format!("\"{}\"", s)),
        FilterValue::Int(n) => ("valueInt", n.to_string()),
        FilterValue::Float(n) => ("valueNumber", n.to_string()),
        FilterValue::Bool(b) => ("valueBoolean", b.to_string()),
    }
}

/// Post-filter search results based on filter expression
pub fn post_filter(
    results: Vec<crate::SearchResult>,
    filter: &FilterExpr,
) -> Vec<crate::SearchResult> {
    results
        .into_iter()
        .filter(|r| evaluate_filter(&r.payload, filter))
        .collect()
}

fn evaluate_filter(payload: &serde_json::Value, filter: &FilterExpr) -> bool {
    match filter {
        FilterExpr::Eq { field, value } => get_field(payload, field)
            .map(|v| compare_values(v, value) == std::cmp::Ordering::Equal)
            .unwrap_or(false),
        FilterExpr::Ne { field, value } => get_field(payload, field)
            .map(|v| compare_values(v, value) != std::cmp::Ordering::Equal)
            .unwrap_or(true),
        FilterExpr::Gt { field, value } => get_field(payload, field)
            .map(|v| compare_values(v, value) == std::cmp::Ordering::Greater)
            .unwrap_or(false),
        FilterExpr::Gte { field, value } => get_field(payload, field)
            .map(|v| {
                let cmp = compare_values(v, value);
                cmp == std::cmp::Ordering::Greater || cmp == std::cmp::Ordering::Equal
            })
            .unwrap_or(false),
        FilterExpr::Lt { field, value } => get_field(payload, field)
            .map(|v| compare_values(v, value) == std::cmp::Ordering::Less)
            .unwrap_or(false),
        FilterExpr::Lte { field, value } => get_field(payload, field)
            .map(|v| {
                let cmp = compare_values(v, value);
                cmp == std::cmp::Ordering::Less || cmp == std::cmp::Ordering::Equal
            })
            .unwrap_or(false),
        FilterExpr::Contains { field, value } => get_field(payload, field)
            .and_then(|v| v.as_str().map(|s| s.contains(value)))
            .unwrap_or(false),
        FilterExpr::StartsWith { field, value } => get_field(payload, field)
            .and_then(|v| v.as_str().map(|s| s.starts_with(value)))
            .unwrap_or(false),
        FilterExpr::EndsWith { field, value } => get_field(payload, field)
            .and_then(|v| v.as_str().map(|s| s.ends_with(value)))
            .unwrap_or(false),
        FilterExpr::In { field, values } => get_field(payload, field)
            .map(|v| {
                values
                    .iter()
                    .any(|fv| compare_values(v, fv) == std::cmp::Ordering::Equal)
            })
            .unwrap_or(false),
        FilterExpr::NotIn { field, values } => get_field(payload, field)
            .map(|v| {
                !values
                    .iter()
                    .any(|fv| compare_values(v, fv) == std::cmp::Ordering::Equal)
            })
            .unwrap_or(true),
        FilterExpr::Exists { field } => get_field(payload, field).is_some(),
        FilterExpr::IsNull { field } => get_field(payload, field).is_none(),
        FilterExpr::And { exprs } => exprs.iter().all(|e| evaluate_filter(payload, e)),
        FilterExpr::Or { exprs } => exprs.iter().any(|e| evaluate_filter(payload, e)),
        FilterExpr::Not { expr } => !evaluate_filter(payload, expr),
    }
}

fn get_field<'a>(payload: &'a serde_json::Value, field: &str) -> Option<&'a serde_json::Value> {
    payload.get(field)
}

fn compare_values(json_val: &serde_json::Value, filter_val: &FilterValue) -> std::cmp::Ordering {
    match (json_val, filter_val) {
        (serde_json::Value::String(s1), FilterValue::String(s2)) => s1.cmp(s2),
        (serde_json::Value::Number(n1), FilterValue::Int(n2)) => n1
            .as_i64()
            .map(|n| n.cmp(n2))
            .unwrap_or(std::cmp::Ordering::Less),
        (serde_json::Value::Number(n1), FilterValue::Float(n2)) => n1
            .as_f64()
            .map(|n| n.partial_cmp(n2).unwrap_or(std::cmp::Ordering::Less))
            .unwrap_or(std::cmp::Ordering::Less),
        (serde_json::Value::Bool(b1), FilterValue::Bool(b2)) => b1.cmp(b2),
        _ => std::cmp::Ordering::Less,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_eq() {
        let filter = FilterExpr::eq("category", "tech");
        let milvus = filter.to_milvus();
        assert_eq!(milvus, r#"category == "tech""#);

        let pinecone = filter.to_pinecone();
        assert_eq!(
            pinecone,
            serde_json::json!({ "category": { "$eq": "tech" } })
        );
    }

    #[test]
    fn test_filter_and() {
        let filter = FilterExpr::and(vec![
            FilterExpr::eq("category", "tech"),
            FilterExpr::gt("score", 0.5f64),
        ]);

        let milvus = filter.to_milvus();
        assert!(milvus.contains("category == \"tech\""));
        assert!(milvus.contains("score > 0.5"));
        assert!(milvus.contains(" and "));
    }

    #[test]
    fn test_post_filter() {
        let results = vec![
            crate::SearchResult {
                id: "1".to_string(),
                score: 0.9,
                payload: serde_json::json!({ "category": "tech", "score": 0.8 }),
                vector: None,
            },
            crate::SearchResult {
                id: "2".to_string(),
                score: 0.8,
                payload: serde_json::json!({ "category": "science", "score": 0.6 }),
                vector: None,
            },
            crate::SearchResult {
                id: "3".to_string(),
                score: 0.7,
                payload: serde_json::json!({ "category": "tech", "score": 0.4 }),
                vector: None,
            },
        ];

        let filter = FilterExpr::eq("category", "tech");
        let filtered = post_filter(results, &filter);

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, "1");
        assert_eq!(filtered[1].id, "3");
    }

    #[test]
    fn test_in_filter() {
        let filter = FilterExpr::in_list(
            "category",
            vec![FilterValue::from("tech"), FilterValue::from("science")],
        );

        let milvus = filter.to_milvus();
        assert!(milvus.contains("in ["));

        let pinecone = filter.to_pinecone();
        assert!(pinecone.get("category").is_some());
    }
}
