//! Query Profiling and Analysis
//!
//! Tools for analyzing and profiling vector search queries to help optimize performance.
//!
//! ## Features
//!
//! - **Query Profiling**: Detailed performance analysis of search operations
//! - **Bottleneck Detection**: Identify performance bottlenecks
//! - **Recommendations**: Get optimization suggestions based on query patterns
//! - **Index Health**: Check index health and identify issues
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::profiling::{QueryProfiler, ProfilingConfig};
//! use oxify_vector::{VectorSearchIndex, SearchConfig};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create index
//! let mut embeddings = HashMap::new();
//! embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
//! let mut index = VectorSearchIndex::new(SearchConfig::default());
//! index.build(&embeddings)?;
//!
//! // Profile a query
//! let config = ProfilingConfig::default();
//! let mut profiler = QueryProfiler::new(config);
//!
//! let query = vec![0.2, 0.3, 0.4];
//! let profile = profiler.profile_search(|| {
//!     index.search(&query, 10)
//! })?;
//!
//! println!("Query took: {:?}", profile.total_duration);
//! println!("Recommendations: {:?}", profile.recommendations);
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::types::SearchResult;

/// Profiling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilingConfig {
    /// Enable detailed timing breakdowns
    pub detailed_timing: bool,
    /// Enable memory profiling
    pub memory_profiling: bool,
    /// Threshold for slow query detection (milliseconds)
    pub slow_query_threshold_ms: u64,
    /// Enable automatic recommendations
    pub enable_recommendations: bool,
}

impl Default for ProfilingConfig {
    fn default() -> Self {
        Self {
            detailed_timing: true,
            memory_profiling: false,
            slow_query_threshold_ms: 100,
            enable_recommendations: true,
        }
    }
}

/// Performance bottleneck type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bottleneck {
    /// Query vector is too large (high dimensionality)
    HighDimensionality,
    /// Dataset is too large for current strategy
    DatasetSize,
    /// Filter is not selective enough
    FilterSelectivity,
    /// k value is too high
    HighK,
    /// No specific bottleneck detected
    None,
}

/// Optimization recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Recommendation type
    pub category: String,
    /// Description of the recommendation
    pub description: String,
    /// Expected performance improvement
    pub impact: ImpactLevel,
}

/// Impact level of a recommendation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactLevel {
    /// High impact (>50% improvement)
    High,
    /// Medium impact (20-50% improvement)
    Medium,
    /// Low impact (<20% improvement)
    Low,
}

/// Query profile result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryProfile {
    /// Total query duration
    pub total_duration: Duration,
    /// Number of results returned
    pub result_count: usize,
    /// Detected bottleneck
    pub bottleneck: Bottleneck,
    /// Optimization recommendations
    pub recommendations: Vec<Recommendation>,
    /// Whether this is considered a slow query
    pub is_slow_query: bool,
}

/// Query profiler
#[derive(Debug, Clone)]
pub struct QueryProfiler {
    config: ProfilingConfig,
}

impl QueryProfiler {
    /// Create a new query profiler
    pub fn new(config: ProfilingConfig) -> Self {
        Self { config }
    }

    /// Profile a search operation
    ///
    /// # Arguments
    /// * `f` - Function that performs the search
    pub fn profile_search<F>(&mut self, f: F) -> Result<QueryProfile>
    where
        F: FnOnce() -> Result<Vec<SearchResult>>,
    {
        let start = Instant::now();
        let results = f()?;
        let duration = start.elapsed();

        let result_count = results.len();
        let is_slow_query = duration.as_millis() > self.config.slow_query_threshold_ms as u128;

        let bottleneck = self.detect_bottleneck(&results, duration);
        let recommendations = if self.config.enable_recommendations {
            self.generate_recommendations(&bottleneck, duration, result_count)
        } else {
            Vec::new()
        };

        Ok(QueryProfile {
            total_duration: duration,
            result_count,
            bottleneck,
            recommendations,
            is_slow_query,
        })
    }

    /// Detect performance bottlenecks
    fn detect_bottleneck(&self, _results: &[SearchResult], duration: Duration) -> Bottleneck {
        // Analyze query characteristics to detect bottlenecks
        if duration.as_millis() > 1000 {
            // Very slow query - likely dataset size issue
            Bottleneck::DatasetSize
        } else {
            Bottleneck::None
        }
    }

    /// Generate optimization recommendations
    fn generate_recommendations(
        &self,
        bottleneck: &Bottleneck,
        duration: Duration,
        result_count: usize,
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        match bottleneck {
            Bottleneck::DatasetSize => {
                recommendations.push(Recommendation {
                    category: "Index Strategy".to_string(),
                    description:
                        "Consider using HNSW or IVF-PQ for approximate search on large datasets"
                            .to_string(),
                    impact: ImpactLevel::High,
                });
            }
            Bottleneck::HighDimensionality => {
                recommendations.push(Recommendation {
                    category: "Dimensionality".to_string(),
                    description: "Consider using dimensionality reduction (PCA) or quantization"
                        .to_string(),
                    impact: ImpactLevel::Medium,
                });
            }
            Bottleneck::FilterSelectivity => {
                recommendations.push(Recommendation {
                    category: "Filtering".to_string(),
                    description: "Use pre-filtering for highly selective filters".to_string(),
                    impact: ImpactLevel::Medium,
                });
            }
            Bottleneck::HighK => {
                recommendations.push(Recommendation {
                    category: "Query Parameters".to_string(),
                    description: "Reduce k value if you don't need all top results".to_string(),
                    impact: ImpactLevel::Low,
                });
            }
            Bottleneck::None => {}
        }

        // Add general recommendations for slow queries
        if duration.as_millis() > self.config.slow_query_threshold_ms as u128 && result_count > 100
        {
            recommendations.push(Recommendation {
                category: "Result Count".to_string(),
                description: "Consider reducing k to improve query speed".to_string(),
                impact: ImpactLevel::Low,
            });
        }

        recommendations
    }

    /// Get profiler configuration
    pub fn config(&self) -> &ProfilingConfig {
        &self.config
    }
}

/// Index health checker
#[derive(Debug)]
pub struct IndexHealthChecker;

impl IndexHealthChecker {
    /// Create a new health checker
    pub fn new() -> Self {
        Self
    }

    /// Check index health and return recommendations
    ///
    /// # Arguments
    /// * `num_vectors` - Number of vectors in index
    /// * `dimensions` - Vector dimensions
    /// * `avg_query_time_ms` - Average query time in milliseconds
    pub fn check_health(
        &self,
        num_vectors: usize,
        dimensions: usize,
        avg_query_time_ms: f64,
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Check if dimensionality is too high
        if dimensions > 1024 {
            recommendations.push(Recommendation {
                category: "Dimensionality".to_string(),
                description: format!(
                    "Vector dimensionality ({}) is very high. Consider dimensionality reduction.",
                    dimensions
                ),
                impact: ImpactLevel::Medium,
            });
        }

        // Check if dataset is large but queries are slow
        if num_vectors > 100_000 && avg_query_time_ms > 50.0 {
            recommendations.push(Recommendation {
                category: "Index Strategy".to_string(),
                description: "Large dataset with slow queries. Consider using HNSW or IVF-PQ."
                    .to_string(),
                impact: ImpactLevel::High,
            });
        }

        // Check if dataset is huge
        if num_vectors > 10_000_000 {
            recommendations.push(Recommendation {
                category: "Scalability".to_string(),
                description: "Very large dataset. Consider distributed search with sharding."
                    .to_string(),
                impact: ImpactLevel::High,
            });
        }

        // Check if queries are consistently slow
        if avg_query_time_ms > 100.0 {
            recommendations.push(Recommendation {
                category: "Performance".to_string(),
                description:
                    "Queries are slow. Consider enabling SIMD optimizations or using quantization."
                        .to_string(),
                impact: ImpactLevel::High,
            });
        }

        recommendations
    }
}

impl Default for IndexHealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiling_config_default() {
        let config = ProfilingConfig::default();
        assert!(config.detailed_timing);
        assert!(config.enable_recommendations);
        assert_eq!(config.slow_query_threshold_ms, 100);
    }

    #[test]
    fn test_query_profiler_creation() {
        let config = ProfilingConfig::default();
        let profiler = QueryProfiler::new(config);
        assert!(profiler.config().enable_recommendations);
    }

    #[test]
    fn test_profile_fast_query() {
        let config = ProfilingConfig::default();
        let mut profiler = QueryProfiler::new(config);

        let profile = profiler
            .profile_search(|| -> Result<Vec<SearchResult>> {
                // Simulate fast query
                std::thread::sleep(Duration::from_millis(10));
                Ok(vec![SearchResult {
                    entity_id: "doc1".to_string(),
                    score: 0.95,
                    distance: 0.05,
                    rank: 1,
                }])
            })
            .unwrap();

        assert_eq!(profile.result_count, 1);
        assert!(!profile.is_slow_query);
    }

    #[test]
    fn test_profile_slow_query() {
        let config = ProfilingConfig {
            slow_query_threshold_ms: 50,
            ..Default::default()
        };
        let mut profiler = QueryProfiler::new(config);

        let profile = profiler
            .profile_search(|| -> Result<Vec<SearchResult>> {
                // Simulate slow query
                std::thread::sleep(Duration::from_millis(150));
                Ok(vec![])
            })
            .unwrap();

        assert!(profile.is_slow_query);
        assert!(profile.total_duration.as_millis() >= 150);
    }

    #[test]
    fn test_bottleneck_detection_slow_query() {
        let config = ProfilingConfig::default();
        let profiler = QueryProfiler::new(config);

        let results = vec![];
        let duration = Duration::from_millis(2000);

        let bottleneck = profiler.detect_bottleneck(&results, duration);
        assert_eq!(bottleneck, Bottleneck::DatasetSize);
    }

    #[test]
    fn test_bottleneck_detection_fast_query() {
        let config = ProfilingConfig::default();
        let profiler = QueryProfiler::new(config);

        let results = vec![];
        let duration = Duration::from_millis(10);

        let bottleneck = profiler.detect_bottleneck(&results, duration);
        assert_eq!(bottleneck, Bottleneck::None);
    }

    #[test]
    fn test_generate_recommendations_dataset_size() {
        let config = ProfilingConfig::default();
        let profiler = QueryProfiler::new(config);

        let recommendations = profiler.generate_recommendations(
            &Bottleneck::DatasetSize,
            Duration::from_millis(100),
            10,
        );

        assert!(!recommendations.is_empty());
        assert_eq!(recommendations[0].category, "Index Strategy");
        assert_eq!(recommendations[0].impact, ImpactLevel::High);
    }

    #[test]
    fn test_generate_recommendations_high_k() {
        let config = ProfilingConfig::default();
        let profiler = QueryProfiler::new(config);

        let recommendations =
            profiler.generate_recommendations(&Bottleneck::None, Duration::from_millis(150), 200);

        assert!(!recommendations.is_empty());
        // Should recommend reducing k for slow queries with many results
    }

    #[test]
    fn test_index_health_checker_creation() {
        let checker = IndexHealthChecker::new();
        let recommendations = checker.check_health(1000, 768, 10.0);
        assert!(recommendations.is_empty()); // Small dataset, reasonable performance
    }

    #[test]
    fn test_index_health_high_dimensionality() {
        let checker = IndexHealthChecker::new();
        let recommendations = checker.check_health(10_000, 2048, 10.0);

        assert!(!recommendations.is_empty());
        assert!(recommendations
            .iter()
            .any(|r| r.category == "Dimensionality"));
    }

    #[test]
    fn test_index_health_large_dataset_slow() {
        let checker = IndexHealthChecker::new();
        let recommendations = checker.check_health(200_000, 768, 100.0);

        assert!(!recommendations.is_empty());
        assert!(recommendations
            .iter()
            .any(|r| r.category == "Index Strategy" || r.category == "Performance"));
    }

    #[test]
    fn test_index_health_very_large_dataset() {
        let checker = IndexHealthChecker::new();
        let recommendations = checker.check_health(15_000_000, 768, 50.0);

        assert!(!recommendations.is_empty());
        assert!(recommendations.iter().any(|r| r.category == "Scalability"));
    }

    #[test]
    fn test_recommendation_impact_levels() {
        let high_impact = Recommendation {
            category: "Test".to_string(),
            description: "Test".to_string(),
            impact: ImpactLevel::High,
        };

        let medium_impact = Recommendation {
            category: "Test".to_string(),
            description: "Test".to_string(),
            impact: ImpactLevel::Medium,
        };

        let low_impact = Recommendation {
            category: "Test".to_string(),
            description: "Test".to_string(),
            impact: ImpactLevel::Low,
        };

        assert_eq!(high_impact.impact, ImpactLevel::High);
        assert_eq!(medium_impact.impact, ImpactLevel::Medium);
        assert_eq!(low_impact.impact, ImpactLevel::Low);
    }
}
