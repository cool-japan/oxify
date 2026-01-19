//! Query optimization utilities for authorization checks
//!
//! This module provides tools to analyze and optimize database queries,
//! helping identify slow queries and suggesting improvements.
//!
//! # Example
//!
//! ```no_run
//! use oxify_authz::query_optimizer::*;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let optimizer = QueryOptimizer::new();
//!
//! // Analyze a query
//! let analysis = QueryAnalysis {
//!     query_type: QueryType::Check,
//!     execution_time_ms: 150.0,
//!     rows_scanned: 10000,
//!     rows_returned: 1,
//!     uses_index: false,
//!     cache_hit: false,
//! };
//!
//! let suggestions = optimizer.analyze(&analysis);
//! for suggestion in suggestions {
//!     println!("Optimization: {:?}", suggestion);
//! }
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Query optimizer for authorization operations
#[derive(Clone)]
pub struct QueryOptimizer {
    /// Performance thresholds for different query types
    thresholds: HashMap<QueryType, PerformanceThreshold>,
}

/// Type of authorization query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QueryType {
    /// Direct permission check
    Check,
    /// Permission expansion (find all subjects)
    Expand,
    /// Write tuple operation
    Write,
    /// Delete tuple operation
    Delete,
    /// Batch check operation
    BatchCheck,
    /// List tuples query
    List,
    /// Transitive check (multi-hop)
    TransitiveCheck,
}

/// Performance thresholds for a query type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThreshold {
    /// Maximum acceptable execution time in milliseconds
    pub max_execution_time_ms: f64,
    /// Maximum acceptable rows scanned
    pub max_rows_scanned: u64,
    /// Target cache hit rate (0.0 - 1.0)
    pub target_cache_hit_rate: f64,
}

/// Analysis of a query execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAnalysis {
    /// Type of query
    pub query_type: QueryType,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Number of rows scanned
    pub rows_scanned: u64,
    /// Number of rows returned
    pub rows_returned: u64,
    /// Whether an index was used
    pub uses_index: bool,
    /// Whether the result came from cache
    pub cache_hit: bool,
}

/// Optimization suggestion
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    /// Severity of the issue
    pub severity: Severity,
    /// Category of the suggestion
    pub category: Category,
    /// Description of the suggestion
    pub description: String,
    /// Potential impact (e.g., "50% reduction in query time")
    pub potential_impact: String,
}

/// Severity level of an optimization suggestion
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    /// Minor optimization opportunity
    Low,
    /// Moderate optimization opportunity
    Medium,
    /// Important optimization opportunity
    High,
    /// Critical issue affecting performance
    Critical,
}

/// Category of optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    /// Index-related optimization
    Indexing,
    /// Caching optimization
    Caching,
    /// Query structure optimization
    QueryStructure,
    /// Data volume optimization
    DataVolume,
    /// General performance
    Performance,
}

impl QueryOptimizer {
    /// Create a new query optimizer with default thresholds
    pub fn new() -> Self {
        let mut thresholds = HashMap::new();

        // Set default thresholds for each query type
        thresholds.insert(
            QueryType::Check,
            PerformanceThreshold {
                max_execution_time_ms: 3.0, // 3ms for cached checks
                max_rows_scanned: 100,
                target_cache_hit_rate: 0.95,
            },
        );

        thresholds.insert(
            QueryType::Expand,
            PerformanceThreshold {
                max_execution_time_ms: 10.0,
                max_rows_scanned: 1000,
                target_cache_hit_rate: 0.80,
            },
        );

        thresholds.insert(
            QueryType::Write,
            PerformanceThreshold {
                max_execution_time_ms: 5.0,
                max_rows_scanned: 10,
                target_cache_hit_rate: 0.0, // Writes don't benefit from read cache
            },
        );

        thresholds.insert(
            QueryType::Delete,
            PerformanceThreshold {
                max_execution_time_ms: 5.0,
                max_rows_scanned: 10,
                target_cache_hit_rate: 0.0,
            },
        );

        thresholds.insert(
            QueryType::BatchCheck,
            PerformanceThreshold {
                max_execution_time_ms: 50.0, // 50ms for 100 checks
                max_rows_scanned: 10000,
                target_cache_hit_rate: 0.90,
            },
        );

        thresholds.insert(
            QueryType::List,
            PerformanceThreshold {
                max_execution_time_ms: 20.0,
                max_rows_scanned: 10000,
                target_cache_hit_rate: 0.50,
            },
        );

        thresholds.insert(
            QueryType::TransitiveCheck,
            PerformanceThreshold {
                max_execution_time_ms: 10.0,
                max_rows_scanned: 500,
                target_cache_hit_rate: 0.85,
            },
        );

        Self { thresholds }
    }

    /// Analyze a query and provide optimization suggestions
    pub fn analyze(&self, analysis: &QueryAnalysis) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        let threshold = self
            .thresholds
            .get(&analysis.query_type)
            .cloned()
            .unwrap_or(PerformanceThreshold {
                max_execution_time_ms: 10.0,
                max_rows_scanned: 1000,
                target_cache_hit_rate: 0.90,
            });

        // Check execution time
        if analysis.execution_time_ms > threshold.max_execution_time_ms {
            let severity = if analysis.execution_time_ms > threshold.max_execution_time_ms * 3.0 {
                Severity::Critical
            } else if analysis.execution_time_ms > threshold.max_execution_time_ms * 2.0 {
                Severity::High
            } else {
                Severity::Medium
            };

            suggestions.push(OptimizationSuggestion {
                severity,
                category: Category::Performance,
                description: format!(
                    "Query execution time ({:.2}ms) exceeds threshold ({:.2}ms)",
                    analysis.execution_time_ms, threshold.max_execution_time_ms
                ),
                potential_impact: "Reduce latency by optimizing query or adding caching"
                    .to_string(),
            });
        }

        // Check index usage
        if !analysis.uses_index && analysis.rows_scanned > 100 {
            suggestions.push(OptimizationSuggestion {
                severity: Severity::High,
                category: Category::Indexing,
                description: format!(
                    "Query scanned {} rows without using an index",
                    analysis.rows_scanned
                ),
                potential_impact: "Adding an index could reduce query time by 90%+".to_string(),
            });
        }

        // Check rows scanned vs returned ratio
        if analysis.rows_returned > 0 {
            let scan_ratio = analysis.rows_scanned as f64 / analysis.rows_returned as f64;
            if scan_ratio > 100.0 && analysis.rows_scanned > 1000 {
                suggestions.push(OptimizationSuggestion {
                    severity: Severity::Medium,
                    category: Category::QueryStructure,
                    description: format!(
                        "Query scanned {} rows but returned only {} (ratio: {:.1}:1)",
                        analysis.rows_scanned, analysis.rows_returned, scan_ratio
                    ),
                    potential_impact: "Optimize query filters or add covering indexes".to_string(),
                });
            }
        }

        // Check cache hit
        if !analysis.cache_hit
            && matches!(
                analysis.query_type,
                QueryType::Check | QueryType::TransitiveCheck | QueryType::Expand
            )
        {
            suggestions.push(OptimizationSuggestion {
                severity: Severity::Medium,
                category: Category::Caching,
                description: "Cache miss for a cacheable query type".to_string(),
                potential_impact: "Improve cache hit rate to reduce database load".to_string(),
            });
        }

        // Check data volume
        if analysis.rows_scanned > threshold.max_rows_scanned {
            suggestions.push(OptimizationSuggestion {
                severity: Severity::Medium,
                category: Category::DataVolume,
                description: format!(
                    "Query scanned {} rows, exceeding threshold of {}",
                    analysis.rows_scanned, threshold.max_rows_scanned
                ),
                potential_impact: "Consider data archival or partitioning strategies".to_string(),
            });
        }

        suggestions
    }

    /// Set custom threshold for a query type
    pub fn set_threshold(&mut self, query_type: QueryType, threshold: PerformanceThreshold) {
        self.thresholds.insert(query_type, threshold);
    }

    /// Get threshold for a query type
    pub fn get_threshold(&self, query_type: QueryType) -> Option<&PerformanceThreshold> {
        self.thresholds.get(&query_type)
    }

    /// Generate optimization report from multiple analyses
    pub fn generate_report(&self, analyses: &[QueryAnalysis]) -> OptimizationReport {
        let mut suggestions_by_severity = HashMap::new();
        let mut total_suggestions = 0;

        for analysis in analyses {
            let suggestions = self.analyze(analysis);
            total_suggestions += suggestions.len();

            for suggestion in suggestions {
                *suggestions_by_severity
                    .entry(suggestion.severity)
                    .or_insert(0) += 1;
            }
        }

        let critical_count = *suggestions_by_severity
            .get(&Severity::Critical)
            .unwrap_or(&0);
        let high_count = *suggestions_by_severity.get(&Severity::High).unwrap_or(&0);
        let medium_count = *suggestions_by_severity.get(&Severity::Medium).unwrap_or(&0);
        let low_count = *suggestions_by_severity.get(&Severity::Low).unwrap_or(&0);

        OptimizationReport {
            total_queries: analyses.len(),
            total_suggestions,
            critical_issues: critical_count,
            high_priority: high_count,
            medium_priority: medium_count,
            low_priority: low_count,
        }
    }
}

impl Default for QueryOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary report of optimization analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationReport {
    /// Total number of queries analyzed
    pub total_queries: usize,
    /// Total optimization suggestions
    pub total_suggestions: usize,
    /// Number of critical issues
    pub critical_issues: usize,
    /// Number of high-priority suggestions
    pub high_priority: usize,
    /// Number of medium-priority suggestions
    pub medium_priority: usize,
    /// Number of low-priority suggestions
    pub low_priority: usize,
}

impl OptimizationReport {
    /// Check if there are any critical issues
    pub fn has_critical_issues(&self) -> bool {
        self.critical_issues > 0
    }

    /// Get overall health score (0-100)
    pub fn health_score(&self) -> u8 {
        if self.total_queries == 0 {
            return 100;
        }

        let penalty = (self.critical_issues * 20)
            + (self.high_priority * 10)
            + (self.medium_priority * 5)
            + (self.low_priority * 2);

        let score = 100_i32 - penalty as i32;
        score.clamp(0, 100) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimizer_basic() {
        let optimizer = QueryOptimizer::new();

        let analysis = QueryAnalysis {
            query_type: QueryType::Check,
            execution_time_ms: 5.0, // Exceeds 3ms threshold
            rows_scanned: 100,
            rows_returned: 1,
            uses_index: true,
            cache_hit: false,
        };

        let suggestions = optimizer.analyze(&analysis);
        assert!(!suggestions.is_empty());

        // Should suggest caching improvement
        assert!(suggestions
            .iter()
            .any(|s| s.category == Category::Performance));
    }

    #[test]
    fn test_missing_index_detection() {
        let optimizer = QueryOptimizer::new();

        let analysis = QueryAnalysis {
            query_type: QueryType::Check,
            execution_time_ms: 2.0,
            rows_scanned: 10000, // Many rows scanned
            rows_returned: 1,
            uses_index: false, // No index used
            cache_hit: false,
        };

        let suggestions = optimizer.analyze(&analysis);

        // Should suggest adding an index
        assert!(suggestions.iter().any(|s| s.category == Category::Indexing));
    }

    #[test]
    fn test_scan_ratio_detection() {
        let optimizer = QueryOptimizer::new();

        let analysis = QueryAnalysis {
            query_type: QueryType::List,
            execution_time_ms: 15.0,
            rows_scanned: 10000,
            rows_returned: 10, // Poor scan ratio (1000:1)
            uses_index: true,
            cache_hit: false,
        };

        let suggestions = optimizer.analyze(&analysis);

        // Should suggest query structure improvement
        assert!(suggestions
            .iter()
            .any(|s| s.category == Category::QueryStructure));
    }

    #[test]
    fn test_cache_miss_detection() {
        let optimizer = QueryOptimizer::new();

        let analysis = QueryAnalysis {
            query_type: QueryType::Check,
            execution_time_ms: 2.0,
            rows_scanned: 10,
            rows_returned: 1,
            uses_index: true,
            cache_hit: false, // Cache miss for cacheable query
        };

        let suggestions = optimizer.analyze(&analysis);

        // Should suggest caching improvement
        assert!(suggestions.iter().any(|s| s.category == Category::Caching));
    }

    #[test]
    fn test_custom_threshold() {
        let mut optimizer = QueryOptimizer::new();

        let custom_threshold = PerformanceThreshold {
            max_execution_time_ms: 1.0,
            max_rows_scanned: 50,
            target_cache_hit_rate: 0.99,
        };

        optimizer.set_threshold(QueryType::Check, custom_threshold);

        let analysis = QueryAnalysis {
            query_type: QueryType::Check,
            execution_time_ms: 1.5, // Exceeds custom threshold
            rows_scanned: 10,
            rows_returned: 1,
            uses_index: true,
            cache_hit: true,
        };

        let suggestions = optimizer.analyze(&analysis);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_optimization_report() {
        let optimizer = QueryOptimizer::new();

        let analyses = vec![
            QueryAnalysis {
                query_type: QueryType::Check,
                execution_time_ms: 50.0, // Critical
                rows_scanned: 10000,
                rows_returned: 1,
                uses_index: false,
                cache_hit: false,
            },
            QueryAnalysis {
                query_type: QueryType::Check,
                execution_time_ms: 2.0, // Good
                rows_scanned: 10,
                rows_returned: 1,
                uses_index: true,
                cache_hit: true,
            },
        ];

        let report = optimizer.generate_report(&analyses);
        assert_eq!(report.total_queries, 2);
        assert!(report.total_suggestions > 0);
    }

    #[test]
    fn test_health_score() {
        let report = OptimizationReport {
            total_queries: 100,
            total_suggestions: 5,
            critical_issues: 1,
            high_priority: 2,
            medium_priority: 1,
            low_priority: 1,
        };

        let score = report.health_score();
        // 100 - (1*20 + 2*10 + 1*5 + 1*2) = 100 - 47 = 53
        assert_eq!(score, 53);
    }

    #[test]
    fn test_perfect_health_score() {
        let report = OptimizationReport {
            total_queries: 100,
            total_suggestions: 0,
            critical_issues: 0,
            high_priority: 0,
            medium_priority: 0,
            low_priority: 0,
        };

        assert_eq!(report.health_score(), 100);
        assert!(!report.has_critical_issues());
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
    }

    #[test]
    fn test_data_volume_detection() {
        let optimizer = QueryOptimizer::new();

        let analysis = QueryAnalysis {
            query_type: QueryType::Check,
            execution_time_ms: 2.0,
            rows_scanned: 1000, // Exceeds threshold
            rows_returned: 1,
            uses_index: true,
            cache_hit: true,
        };

        let suggestions = optimizer.analyze(&analysis);

        // Should suggest data volume optimization
        assert!(suggestions
            .iter()
            .any(|s| s.category == Category::DataVolume));
    }
}
