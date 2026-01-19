//! PostgreSQL EXPLAIN plan analysis and optimization recommendations
//!
//! Provides utilities to analyze PostgreSQL EXPLAIN output and generate
//! actionable optimization recommendations. This is essential for:
//!
//! - Identifying slow query patterns
//! - Finding missing indexes
//! - Detecting sequential scans on large tables
//! - Analyzing join strategies
//! - Optimizing query performance
//!
//! # Features
//!
//! - Parse EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) output
//! - Detect performance anti-patterns
//! - Generate index recommendations
//! - Analyze cost estimates vs actual times
//! - Identify inefficient operations
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::explain_analyzer::{ExplainAnalyzer, ExplainFormat};
//!
//! let analyzer = ExplainAnalyzer::new(pool);
//!
//! // Analyze a slow query
//! let result = analyzer
//!     .explain("SELECT * FROM executions WHERE workflow_id = $1")
//!     .with_analyze()
//!     .with_buffers()
//!     .execute()
//!     .await?;
//!
//! // Get recommendations
//! for recommendation in result.recommendations {
//!     println!("{}: {}", recommendation.severity, recommendation.message);
//!     if let Some(sql) = recommendation.suggested_fix {
//!         println!("Fix: {}", sql);
//!     }
//! }
//! ```

use crate::{Result, StorageError};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;

/// EXPLAIN format option
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplainFormat {
    /// Text format (default)
    Text,
    /// JSON format (easiest to parse)
    Json,
    /// XML format
    Xml,
    /// YAML format
    Yaml,
}

impl ExplainFormat {
    fn as_str(&self) -> &'static str {
        match self {
            ExplainFormat::Text => "TEXT",
            ExplainFormat::Json => "JSON",
            ExplainFormat::Xml => "XML",
            ExplainFormat::Yaml => "YAML",
        }
    }
}

/// Builder for EXPLAIN queries
pub struct ExplainQuery<'a> {
    pool: &'a PgPool,
    query: String,
    analyze: bool,
    verbose: bool,
    buffers: bool,
    timing: bool,
    costs: bool,
    settings: bool,
    wal: bool,
    format: ExplainFormat,
}

impl<'a> ExplainQuery<'a> {
    /// Create a new EXPLAIN query builder
    fn new(pool: &'a PgPool, query: impl Into<String>) -> Self {
        Self {
            pool,
            query: query.into(),
            analyze: false,
            verbose: false,
            buffers: false,
            timing: true,
            costs: true,
            settings: false,
            wal: false,
            format: ExplainFormat::Json,
        }
    }

    /// Include actual execution statistics (ANALYZE)
    pub fn with_analyze(mut self) -> Self {
        self.analyze = true;
        self
    }

    /// Include verbose output
    pub fn with_verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// Include buffer usage statistics
    pub fn with_buffers(mut self) -> Self {
        self.buffers = true;
        self
    }

    /// Disable timing (only with ANALYZE)
    pub fn without_timing(mut self) -> Self {
        self.timing = false;
        self
    }

    /// Disable cost estimates
    pub fn without_costs(mut self) -> Self {
        self.costs = false;
        self
    }

    /// Include modified settings
    pub fn with_settings(mut self) -> Self {
        self.settings = true;
        self
    }

    /// Include WAL usage
    pub fn with_wal(mut self) -> Self {
        self.wal = true;
        self
    }

    /// Set output format
    pub fn format(mut self, format: ExplainFormat) -> Self {
        self.format = format;
        self
    }

    /// Execute the EXPLAIN query and analyze results
    pub async fn execute(self) -> Result<ExplainResult> {
        let explain_sql = self.build_explain_sql();

        let result: String = sqlx::query_scalar(&explain_sql)
            .fetch_one(self.pool)
            .await?;

        let plan = if self.format == ExplainFormat::Json {
            serde_json::from_str(&result).map_err(StorageError::Serialization)?
        } else {
            // For non-JSON formats, return as raw text
            serde_json::json!({ "raw": result })
        };

        Ok(ExplainResult {
            plan,
            query: self.query.clone(),
            analyzed: self.analyze,
            recommendations: Vec::new(),
        })
    }

    /// Build the EXPLAIN SQL statement
    fn build_explain_sql(&self) -> String {
        let mut options = Vec::new();

        if self.analyze {
            options.push("ANALYZE");
        }
        if self.verbose {
            options.push("VERBOSE");
        }
        if self.buffers {
            options.push("BUFFERS");
        }
        if !self.timing && self.analyze {
            options.push("TIMING FALSE");
        }
        if !self.costs {
            options.push("COSTS FALSE");
        }
        if self.settings {
            options.push("SETTINGS");
        }
        if self.wal {
            options.push("WAL");
        }

        let format_str = format!("FORMAT {}", self.format.as_str());
        options.push(&format_str);

        format!("EXPLAIN ({}) {}", options.join(", "), self.query)
    }
}

/// Result of an EXPLAIN analysis
#[derive(Debug, Clone)]
pub struct ExplainResult {
    /// The execution plan (JSON format)
    pub plan: JsonValue,
    /// The original query
    pub query: String,
    /// Whether ANALYZE was used
    pub analyzed: bool,
    /// Generated recommendations
    pub recommendations: Vec<Recommendation>,
}

/// Severity of a recommendation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Critical performance issue
    Critical,
    /// Warning about suboptimal performance
    Warning,
    /// Informational suggestion
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Critical => write!(f, "CRITICAL"),
            Severity::Warning => write!(f, "WARNING"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

/// Performance recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Severity level
    pub severity: Severity,
    /// Description of the issue
    pub message: String,
    /// Suggested fix (SQL or explanation)
    pub suggested_fix: Option<String>,
    /// Affected table or operation
    pub context: Option<String>,
}

/// PostgreSQL EXPLAIN analyzer
pub struct ExplainAnalyzer {
    pool: PgPool,
}

impl ExplainAnalyzer {
    /// Create a new EXPLAIN analyzer
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Start building an EXPLAIN query
    pub fn explain(&self, query: impl Into<String>) -> ExplainQuery<'_> {
        ExplainQuery::new(&self.pool, query)
    }

    /// Analyze an EXPLAIN result and generate recommendations
    pub fn analyze_plan(&self, mut result: ExplainResult) -> ExplainResult {
        if let Some(plan_array) = result.plan.as_array() {
            if let Some(first_plan) = plan_array.first() {
                self.analyze_node(first_plan, &mut result.recommendations);
            }
        } else if let Some(plan_obj) = result.plan.get("Plan") {
            self.analyze_node(plan_obj, &mut result.recommendations);
        }

        result
    }

    /// Recursively analyze a plan node
    fn analyze_node(&self, node: &JsonValue, recommendations: &mut Vec<Recommendation>) {
        Self::analyze_node_impl(node, recommendations);
    }

    fn analyze_node_impl(node: &JsonValue, recommendations: &mut Vec<Recommendation>) {
        if let Some(node_type) = node.get("Node Type").and_then(|v| v.as_str()) {
            // Check for sequential scans
            if node_type == "Seq Scan" {
                if let Some(relation) = node.get("Relation Name").and_then(|v| v.as_str()) {
                    let rows = node
                        .get("Plan Rows")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    if rows > 1000.0 {
                        recommendations.push(Recommendation {
                            severity: Severity::Warning,
                            message: format!(
                                "Sequential scan on table '{}' with {} estimated rows",
                                relation, rows as i64
                            ),
                            suggested_fix: Some(format!(
                                "Consider adding an index on table '{}' for the filtered columns",
                                relation
                            )),
                            context: Some(relation.to_string()),
                        });
                    }
                }
            }

            // Check for inefficient joins
            if node_type.contains("Join") && !node_type.contains("Hash") {
                let rows = node
                    .get("Plan Rows")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                if rows > 10000.0 {
                    recommendations.push(Recommendation {
                        severity: Severity::Info,
                        message: format!(
                            "{} producing {} rows - consider if this can be optimized",
                            node_type, rows as i64
                        ),
                        suggested_fix: None,
                        context: None,
                    });
                }
            }

            // Check for high cost operations
            if let Some(total_cost) = node.get("Total Cost").and_then(|v| v.as_f64()) {
                if total_cost > 10000.0 {
                    recommendations.push(Recommendation {
                        severity: Severity::Warning,
                        message: format!(
                            "High cost operation ({}) with total cost: {:.2}",
                            node_type, total_cost
                        ),
                        suggested_fix: Some(
                            "Review query structure and consider optimization".to_string(),
                        ),
                        context: None,
                    });
                }
            }

            // Check for actual vs estimated row mismatch
            if let (Some(plan_rows), Some(actual_rows)) = (
                node.get("Plan Rows").and_then(|v| v.as_f64()),
                node.get("Actual Rows").and_then(|v| v.as_f64()),
            ) {
                if plan_rows > 0.0 {
                    let ratio = (actual_rows / plan_rows).abs();
                    if !(0.1..=10.0).contains(&ratio) {
                        recommendations.push(Recommendation {
                            severity: Severity::Warning,
                            message: format!(
                                "Row estimate mismatch: estimated {} vs actual {}",
                                plan_rows as i64, actual_rows as i64
                            ),
                            suggested_fix: Some(
                                "Run ANALYZE on affected tables to update statistics".to_string(),
                            ),
                            context: None,
                        });
                    }
                }
            }
        }

        // Recursively analyze child plans
        if let Some(plans) = node.get("Plans").and_then(|v| v.as_array()) {
            for child in plans {
                Self::analyze_node_impl(child, recommendations);
            }
        }
    }

    /// Quick analysis of a query (EXPLAIN ANALYZE with recommendations)
    pub async fn quick_analyze(&self, query: impl Into<String>) -> Result<ExplainResult> {
        let result = self
            .explain(query)
            .with_analyze()
            .with_buffers()
            .execute()
            .await?;

        Ok(self.analyze_plan(result))
    }
}

/// Helper functions for common analysis tasks
pub mod helpers {
    use super::*;

    /// Extract the total execution time from an EXPLAIN ANALYZE result
    pub fn extract_execution_time(plan: &JsonValue) -> Option<f64> {
        if let Some(array) = plan.as_array() {
            return array
                .first()
                .and_then(|p| p.get("Execution Time"))
                .and_then(|v| v.as_f64());
        }
        plan.get("Execution Time").and_then(|v| v.as_f64())
    }

    /// Extract the planning time from an EXPLAIN ANALYZE result
    pub fn extract_planning_time(plan: &JsonValue) -> Option<f64> {
        if let Some(array) = plan.as_array() {
            return array
                .first()
                .and_then(|p| p.get("Planning Time"))
                .and_then(|v| v.as_f64());
        }
        plan.get("Planning Time").and_then(|v| v.as_f64())
    }

    /// Extract total cost estimate
    pub fn extract_total_cost(plan: &JsonValue) -> Option<f64> {
        if let Some(array) = plan.as_array() {
            return array
                .first()
                .and_then(|p| p.get("Plan"))
                .and_then(|p| p.get("Total Cost"))
                .and_then(|v| v.as_f64());
        }
        plan.get("Plan")
            .and_then(|p| p.get("Total Cost"))
            .and_then(|v| v.as_f64())
    }

    /// Check if the plan contains a sequential scan
    pub fn contains_seq_scan(plan: &JsonValue) -> bool {
        fn check_node(node: &JsonValue) -> bool {
            if let Some(node_type) = node.get("Node Type").and_then(|v| v.as_str()) {
                if node_type == "Seq Scan" {
                    return true;
                }
            }

            if let Some(plans) = node.get("Plans").and_then(|v| v.as_array()) {
                for child in plans {
                    if check_node(child) {
                        return true;
                    }
                }
            }

            false
        }

        if let Some(array) = plan.as_array() {
            array.iter().any(check_node)
        } else {
            check_node(plan)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_format() {
        assert_eq!(ExplainFormat::Json.as_str(), "JSON");
        assert_eq!(ExplainFormat::Text.as_str(), "TEXT");
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(Severity::Critical.to_string(), "CRITICAL");
        assert_eq!(Severity::Warning.to_string(), "WARNING");
        assert_eq!(Severity::Info.to_string(), "INFO");
    }

    #[test]
    fn test_recommendation_creation() {
        let rec = Recommendation {
            severity: Severity::Warning,
            message: "Test message".to_string(),
            suggested_fix: Some("Test fix".to_string()),
            context: Some("test_table".to_string()),
        };

        assert_eq!(rec.severity, Severity::Warning);
        assert!(rec.suggested_fix.is_some());
    }

    #[test]
    fn test_extract_execution_time() {
        let plan = serde_json::json!([{
            "Execution Time": 123.45
        }]);

        let time = helpers::extract_execution_time(&plan);
        assert_eq!(time, Some(123.45));
    }

    #[test]
    fn test_extract_planning_time() {
        let plan = serde_json::json!([{
            "Planning Time": 5.67
        }]);

        let time = helpers::extract_planning_time(&plan);
        assert_eq!(time, Some(5.67));
    }

    #[test]
    fn test_extract_total_cost() {
        let plan = serde_json::json!([{
            "Plan": {
                "Total Cost": 1000.0
            }
        }]);

        let cost = helpers::extract_total_cost(&plan);
        assert_eq!(cost, Some(1000.0));
    }

    #[test]
    fn test_contains_seq_scan() {
        let plan_with_seq = serde_json::json!([{
            "Node Type": "Seq Scan",
            "Relation Name": "test_table"
        }]);

        assert!(helpers::contains_seq_scan(&plan_with_seq));

        let plan_without_seq = serde_json::json!([{
            "Node Type": "Index Scan",
            "Relation Name": "test_table"
        }]);

        assert!(!helpers::contains_seq_scan(&plan_without_seq));
    }

    #[test]
    fn test_contains_seq_scan_nested() {
        let plan = serde_json::json!([{
            "Node Type": "Hash Join",
            "Plans": [{
                "Node Type": "Seq Scan",
                "Relation Name": "test_table"
            }]
        }]);

        assert!(helpers::contains_seq_scan(&plan));
    }
}
