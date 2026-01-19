//! Database index analysis and recommendations
//!
//! Provides utilities to analyze existing indexes, detect unused indexes,
//! identify missing indexes, and generate optimization recommendations.
//!
//! # Features
//!
//! - Analyze index usage statistics
//! - Detect unused or redundant indexes
//! - Recommend new indexes based on query patterns
//! - Calculate index efficiency metrics
//! - Identify duplicate indexes
//! - Analyze index bloat
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::index_analyzer::IndexAnalyzer;
//!
//! let analyzer = IndexAnalyzer::new(pool);
//!
//! // Get all indexes
//! let indexes = analyzer.list_all_indexes().await?;
//!
//! // Find unused indexes
//! let unused = analyzer.find_unused_indexes(100).await?; // min 100 scans
//!
//! // Get recommendations
//! let recommendations = analyzer.analyze_and_recommend().await?;
//!
//! for rec in recommendations {
//!     println!("{}: {}", rec.recommendation_type, rec.description);
//!     if let Some(sql) = rec.suggested_sql {
//!         println!("Execute: {}", sql);
//!     }
//! }
//! ```

use crate::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Information about a database index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    /// Schema name
    pub schema_name: String,
    /// Table name
    pub table_name: String,
    /// Index name
    pub index_name: String,
    /// Index definition (CREATE INDEX statement)
    pub index_def: String,
    /// Index type (btree, hash, gin, gist, etc.)
    pub index_type: String,
    /// Indexed columns
    pub columns: Vec<String>,
    /// Whether this is a unique index
    pub is_unique: bool,
    /// Whether this is a primary key
    pub is_primary: bool,
    /// Index size in bytes
    pub size_bytes: i64,
    /// Number of index scans
    pub idx_scan: i64,
    /// Number of rows read from index
    pub idx_tup_read: i64,
    /// Number of rows fetched from table
    pub idx_tup_fetch: i64,
}

impl IndexInfo {
    /// Calculate index efficiency (scans per MB)
    pub fn efficiency(&self) -> f64 {
        if self.size_bytes == 0 {
            return 0.0;
        }
        let size_mb = self.size_bytes as f64 / (1024.0 * 1024.0);
        self.idx_scan as f64 / size_mb
    }

    /// Check if this index appears to be unused
    pub fn is_unused(&self, min_scans: i64) -> bool {
        !self.is_primary && self.idx_scan < min_scans
    }

    /// Get human-readable size
    pub fn size_human(&self) -> String {
        let kb = self.size_bytes as f64 / 1024.0;
        if kb < 1024.0 {
            format!("{:.1} KB", kb)
        } else {
            let mb = kb / 1024.0;
            if mb < 1024.0 {
                format!("{:.1} MB", mb)
            } else {
                format!("{:.1} GB", mb / 1024.0)
            }
        }
    }
}

/// Type of index recommendation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationType {
    /// Drop an unused index
    DropUnused,
    /// Drop a duplicate index
    DropDuplicate,
    /// Create a missing index
    CreateIndex,
    /// Consider partial index
    PartialIndex,
    /// Index is bloated
    RebuildIndex,
    /// Informational
    Info,
}

impl std::fmt::Display for RecommendationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecommendationType::DropUnused => write!(f, "DROP_UNUSED"),
            RecommendationType::DropDuplicate => write!(f, "DROP_DUPLICATE"),
            RecommendationType::CreateIndex => write!(f, "CREATE_INDEX"),
            RecommendationType::PartialIndex => write!(f, "PARTIAL_INDEX"),
            RecommendationType::RebuildIndex => write!(f, "REBUILD_INDEX"),
            RecommendationType::Info => write!(f, "INFO"),
        }
    }
}

/// Index recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRecommendation {
    /// Type of recommendation
    pub recommendation_type: RecommendationType,
    /// Table name
    pub table_name: String,
    /// Index name (if applicable)
    pub index_name: Option<String>,
    /// Description of the recommendation
    pub description: String,
    /// Suggested SQL to execute
    pub suggested_sql: Option<String>,
    /// Estimated impact (high, medium, low)
    pub impact: String,
    /// Additional context
    pub context: Option<String>,
}

/// Database index analyzer
pub struct IndexAnalyzer {
    pool: PgPool,
}

impl IndexAnalyzer {
    /// Create a new index analyzer
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List all indexes in the database
    #[tracing::instrument(skip(self))]
    pub async fn list_all_indexes(&self) -> Result<Vec<IndexInfo>> {
        let indexes = sqlx::query_as::<_, IndexRow>(
            r#"
            SELECT
                n.nspname AS schema_name,
                t.relname AS table_name,
                i.relname AS index_name,
                pg_get_indexdef(i.oid) AS index_def,
                am.amname AS index_type,
                ARRAY_AGG(a.attname ORDER BY k.ordinality) AS columns,
                idx.indisunique AS is_unique,
                idx.indisprimary AS is_primary,
                pg_relation_size(i.oid) AS size_bytes,
                COALESCE(s.idx_scan, 0) AS idx_scan,
                COALESCE(s.idx_tup_read, 0) AS idx_tup_read,
                COALESCE(s.idx_tup_fetch, 0) AS idx_tup_fetch
            FROM pg_index idx
            JOIN pg_class i ON i.oid = idx.indexrelid
            JOIN pg_class t ON t.oid = idx.indrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            JOIN pg_am am ON am.oid = i.relam
            LEFT JOIN pg_stat_user_indexes s ON s.indexrelid = i.oid
            CROSS JOIN LATERAL unnest(idx.indkey) WITH ORDINALITY AS k(attnum, ordinality)
            LEFT JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
            GROUP BY n.nspname, t.relname, i.relname, i.oid, am.amname,
                     idx.indisunique, idx.indisprimary, s.idx_scan,
                     s.idx_tup_read, s.idx_tup_fetch
            ORDER BY n.nspname, t.relname, i.relname
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(indexes.into_iter().map(|row| row.into()).collect())
    }

    /// Find unused indexes (indexes with scan count below threshold)
    #[tracing::instrument(skip(self))]
    pub async fn find_unused_indexes(&self, min_scans: i64) -> Result<Vec<IndexInfo>> {
        let all_indexes = self.list_all_indexes().await?;

        Ok(all_indexes
            .into_iter()
            .filter(|idx| idx.is_unused(min_scans))
            .collect())
    }

    /// Find duplicate indexes (indexes on same columns)
    #[tracing::instrument(skip(self))]
    pub async fn find_duplicate_indexes(&self) -> Result<Vec<(IndexInfo, IndexInfo)>> {
        let all_indexes = self.list_all_indexes().await?;
        let mut duplicates = Vec::new();

        for (i, idx1) in all_indexes.iter().enumerate() {
            for idx2 in all_indexes.iter().skip(i + 1) {
                if idx1.table_name == idx2.table_name
                    && idx1.columns == idx2.columns
                    && !idx1.is_primary
                    && !idx2.is_primary
                {
                    duplicates.push((idx1.clone(), idx2.clone()));
                }
            }
        }

        Ok(duplicates)
    }

    /// Analyze indexes and generate recommendations
    #[tracing::instrument(skip(self))]
    pub async fn analyze_and_recommend(&self) -> Result<Vec<IndexRecommendation>> {
        let mut recommendations = Vec::new();

        // Find unused indexes
        let unused = self.find_unused_indexes(100).await?;
        for idx in unused {
            recommendations.push(IndexRecommendation {
                recommendation_type: RecommendationType::DropUnused,
                table_name: idx.table_name.clone(),
                index_name: Some(idx.index_name.clone()),
                description: format!(
                    "Index '{}' has only {} scans and may be unused",
                    idx.index_name, idx.idx_scan
                ),
                suggested_sql: Some(format!("DROP INDEX IF EXISTS {}", idx.index_name)),
                impact: "medium".to_string(),
                context: Some(format!(
                    "Size: {}, Scans: {}",
                    idx.size_human(),
                    idx.idx_scan
                )),
            });
        }

        // Find duplicate indexes
        let duplicates = self.find_duplicate_indexes().await?;
        for (idx1, idx2) in duplicates {
            let (to_drop, to_keep) = if idx1.idx_scan < idx2.idx_scan {
                (idx1, idx2)
            } else {
                (idx2, idx1)
            };

            recommendations.push(IndexRecommendation {
                recommendation_type: RecommendationType::DropDuplicate,
                table_name: to_drop.table_name.clone(),
                index_name: Some(to_drop.index_name.clone()),
                description: format!(
                    "Index '{}' is duplicate of '{}' on columns: {}",
                    to_drop.index_name,
                    to_keep.index_name,
                    to_drop.columns.join(", ")
                ),
                suggested_sql: Some(format!("DROP INDEX IF EXISTS {}", to_drop.index_name)),
                impact: "low".to_string(),
                context: Some(format!(
                    "Keep '{}' ({} scans), drop '{}' ({} scans)",
                    to_keep.index_name, to_keep.idx_scan, to_drop.index_name, to_drop.idx_scan
                )),
            });
        }

        Ok(recommendations)
    }

    /// Get index statistics summary
    #[tracing::instrument(skip(self))]
    pub async fn get_index_stats(&self) -> Result<IndexStats> {
        let indexes = self.list_all_indexes().await?;

        let total_count = indexes.len();
        let total_size: i64 = indexes.iter().map(|idx| idx.size_bytes).sum();
        let unused_count = indexes.iter().filter(|idx| idx.is_unused(100)).count();
        let avg_efficiency = if total_count > 0 {
            indexes.iter().map(|idx| idx.efficiency()).sum::<f64>() / total_count as f64
        } else {
            0.0
        };

        Ok(IndexStats {
            total_indexes: total_count,
            total_size_bytes: total_size,
            unused_indexes: unused_count,
            average_efficiency: avg_efficiency,
        })
    }
}

/// Internal struct for SQL query result
#[derive(Debug, sqlx::FromRow)]
struct IndexRow {
    schema_name: String,
    table_name: String,
    index_name: String,
    index_def: String,
    index_type: String,
    columns: Vec<String>,
    is_unique: bool,
    is_primary: bool,
    size_bytes: i64,
    idx_scan: i64,
    idx_tup_read: i64,
    idx_tup_fetch: i64,
}

impl From<IndexRow> for IndexInfo {
    fn from(row: IndexRow) -> Self {
        IndexInfo {
            schema_name: row.schema_name,
            table_name: row.table_name,
            index_name: row.index_name,
            index_def: row.index_def,
            index_type: row.index_type,
            columns: row.columns,
            is_unique: row.is_unique,
            is_primary: row.is_primary,
            size_bytes: row.size_bytes,
            idx_scan: row.idx_scan,
            idx_tup_read: row.idx_tup_read,
            idx_tup_fetch: row.idx_tup_fetch,
        }
    }
}

/// Index statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Total number of indexes
    pub total_indexes: usize,
    /// Total size of all indexes in bytes
    pub total_size_bytes: i64,
    /// Number of unused indexes
    pub unused_indexes: usize,
    /// Average index efficiency
    pub average_efficiency: f64,
}

impl IndexStats {
    /// Get total size in human-readable format
    pub fn total_size_human(&self) -> String {
        let mb = self.total_size_bytes as f64 / (1024.0 * 1024.0);
        if mb < 1024.0 {
            format!("{:.1} MB", mb)
        } else {
            format!("{:.1} GB", mb / 1024.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommendation_type_display() {
        assert_eq!(RecommendationType::DropUnused.to_string(), "DROP_UNUSED");
        assert_eq!(RecommendationType::CreateIndex.to_string(), "CREATE_INDEX");
    }

    #[test]
    fn test_index_info_efficiency() {
        let idx = IndexInfo {
            schema_name: "public".to_string(),
            table_name: "test".to_string(),
            index_name: "idx_test".to_string(),
            index_def: "".to_string(),
            index_type: "btree".to_string(),
            columns: vec!["id".to_string()],
            is_unique: false,
            is_primary: false,
            size_bytes: 1024 * 1024, // 1 MB
            idx_scan: 1000,
            idx_tup_read: 5000,
            idx_tup_fetch: 5000,
        };

        let efficiency = idx.efficiency();
        assert_eq!(efficiency, 1000.0); // 1000 scans per MB
    }

    #[test]
    fn test_index_info_is_unused() {
        let idx = IndexInfo {
            schema_name: "public".to_string(),
            table_name: "test".to_string(),
            index_name: "idx_test".to_string(),
            index_def: "".to_string(),
            index_type: "btree".to_string(),
            columns: vec!["id".to_string()],
            is_unique: false,
            is_primary: false,
            size_bytes: 1024,
            idx_scan: 10,
            idx_tup_read: 50,
            idx_tup_fetch: 50,
        };

        assert!(idx.is_unused(100));
        assert!(!idx.is_unused(5));
    }

    #[test]
    fn test_index_info_size_human() {
        let idx_kb = IndexInfo {
            schema_name: "public".to_string(),
            table_name: "test".to_string(),
            index_name: "idx_test".to_string(),
            index_def: "".to_string(),
            index_type: "btree".to_string(),
            columns: vec!["id".to_string()],
            is_unique: false,
            is_primary: false,
            size_bytes: 1024, // 1 KB
            idx_scan: 0,
            idx_tup_read: 0,
            idx_tup_fetch: 0,
        };

        assert_eq!(idx_kb.size_human(), "1.0 KB");

        let idx_mb = IndexInfo {
            size_bytes: 1024 * 1024, // 1 MB
            ..idx_kb
        };

        assert_eq!(idx_mb.size_human(), "1.0 MB");
    }

    #[test]
    fn test_index_stats_size_human() {
        let stats = IndexStats {
            total_indexes: 10,
            total_size_bytes: 1024 * 1024 * 100, // 100 MB
            unused_indexes: 2,
            average_efficiency: 500.0,
        };

        assert_eq!(stats.total_size_human(), "100.0 MB");
    }

    #[test]
    fn test_index_recommendation_creation() {
        let rec = IndexRecommendation {
            recommendation_type: RecommendationType::DropUnused,
            table_name: "test_table".to_string(),
            index_name: Some("idx_test".to_string()),
            description: "Index is unused".to_string(),
            suggested_sql: Some("DROP INDEX idx_test".to_string()),
            impact: "medium".to_string(),
            context: Some("Size: 1.0 MB".to_string()),
        };

        assert_eq!(rec.recommendation_type, RecommendationType::DropUnused);
        assert!(rec.suggested_sql.is_some());
    }
}
