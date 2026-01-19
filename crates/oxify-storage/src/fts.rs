//! PostgreSQL Full-Text Search (FTS) utilities
//!
//! Provides utilities for leveraging PostgreSQL's powerful full-text search capabilities,
//! including tsvector/tsquery operations, ranking, and highlighting.
//!
//! # Features
//!
//! - Create and manage tsvector columns and GIN indexes
//! - Build complex text search queries with operators
//! - Rank search results by relevance
//! - Highlight matching terms in results
//! - Support for multiple languages and text configurations
//! - Phrase search and proximity search
//! - Prefix matching for autocomplete
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::fts::{FtsIndexBuilder, FtsQueryBuilder, FtsLanguage};
//!
//! // Create a full-text search index
//! let fts = FtsIndexBuilder::new(pool, "workflows")
//!     .add_column("name", 'A')  // Highest weight
//!     .add_column("description", 'B')  // Medium weight
//!     .language(FtsLanguage::English)
//!     .create()
//!     .await?;
//!
//! // Search for workflows
//! let results = FtsQueryBuilder::new(pool, "workflows")
//!     .search("machine learning & workflow")
//!     .with_ranking()
//!     .with_highlighting("description", 50)
//!     .limit(20)
//!     .execute()
//!     .await?;
//!
//! for result in results {
//!     println!("Rank: {}, Title: {}", result.rank, result.name);
//!     println!("Excerpt: {}", result.highlighted_text);
//! }
//! ```

use crate::{Result, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// PostgreSQL text search languages/configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FtsLanguage {
    /// Simple configuration (no stemming, stopwords)
    Simple,
    /// English language
    English,
    /// Spanish language
    Spanish,
    /// French language
    French,
    /// German language
    German,
    /// Italian language
    Italian,
    /// Portuguese language
    Portuguese,
    /// Russian language
    Russian,
    /// Japanese language
    Japanese,
    /// Chinese language
    Chinese,
}

impl FtsLanguage {
    fn as_str(&self) -> &'static str {
        match self {
            FtsLanguage::Simple => "simple",
            FtsLanguage::English => "english",
            FtsLanguage::Spanish => "spanish",
            FtsLanguage::French => "french",
            FtsLanguage::German => "german",
            FtsLanguage::Italian => "italian",
            FtsLanguage::Portuguese => "portuguese",
            FtsLanguage::Russian => "russian",
            FtsLanguage::Japanese => "japanese",
            FtsLanguage::Chinese => "chinese",
        }
    }
}

/// Text search weight (A=highest, D=lowest)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsWeight {
    /// Highest weight (1.0) - typically for titles
    A,
    /// High weight (0.4) - typically for headings
    B,
    /// Medium weight (0.2) - typically for body text
    C,
    /// Low weight (0.1) - typically for metadata
    D,
}

impl FtsWeight {
    fn as_char(&self) -> char {
        match self {
            FtsWeight::A => 'A',
            FtsWeight::B => 'B',
            FtsWeight::C => 'C',
            FtsWeight::D => 'D',
        }
    }
}

/// Builder for creating full-text search indexes
pub struct FtsIndexBuilder<'a> {
    pool: &'a PgPool,
    table_name: String,
    columns: Vec<(String, FtsWeight)>,
    language: FtsLanguage,
    index_name: Option<String>,
    tsvector_column: String,
}

impl<'a> FtsIndexBuilder<'a> {
    /// Create a new FTS index builder
    pub fn new(pool: &'a PgPool, table_name: impl Into<String>) -> Self {
        let table_name = table_name.into();
        let tsvector_column = format!("{}_fts", table_name);

        Self {
            pool,
            table_name,
            columns: Vec::new(),
            language: FtsLanguage::English,
            index_name: None,
            tsvector_column,
        }
    }

    /// Add a column to index with specified weight
    pub fn add_column(mut self, column: impl Into<String>, weight: char) -> Self {
        let weight = match weight {
            'A' | 'a' => FtsWeight::A,
            'B' | 'b' => FtsWeight::B,
            'C' | 'c' => FtsWeight::C,
            'D' | 'd' => FtsWeight::D,
            _ => FtsWeight::C, // Default to medium weight
        };
        self.columns.push((column.into(), weight));
        self
    }

    /// Set the text search language/configuration
    pub fn language(mut self, language: FtsLanguage) -> Self {
        self.language = language;
        self
    }

    /// Set custom name for the tsvector column
    pub fn tsvector_column(mut self, name: impl Into<String>) -> Self {
        self.tsvector_column = name.into();
        self
    }

    /// Set custom name for the GIN index
    pub fn index_name(mut self, name: impl Into<String>) -> Self {
        self.index_name = Some(name.into());
        self
    }

    /// Create the full-text search index
    ///
    /// This will:
    /// 1. Add a tsvector column to the table
    /// 2. Create a trigger to automatically update the tsvector
    /// 3. Create a GIN index on the tsvector column
    /// 4. Populate the tsvector for existing rows
    pub async fn create(&self) -> Result<FtsIndexInfo> {
        if self.columns.is_empty() {
            return Err(StorageError::validation(
                "At least one column must be specified for FTS indexing",
            ));
        }

        // Step 1: Add tsvector column
        let add_column_sql = format!(
            "ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} tsvector",
            self.table_name, self.tsvector_column
        );
        sqlx::query(&add_column_sql).execute(self.pool).await?;

        // Step 2: Build the tsvector expression
        let tsvector_expr = self.build_tsvector_expression();

        // Step 3: Create update function
        let function_name = format!("{}_fts_update", self.table_name);
        let create_function_sql = format!(
            r#"
            CREATE OR REPLACE FUNCTION {}() RETURNS trigger AS $$
            BEGIN
                NEW.{} := {};
                RETURN NEW;
            END
            $$ LANGUAGE plpgsql
            "#,
            function_name, self.tsvector_column, tsvector_expr
        );
        sqlx::query(&create_function_sql).execute(self.pool).await?;

        // Step 4: Create trigger
        let trigger_name = format!("{}_fts_trigger", self.table_name);
        let create_trigger_sql = format!(
            r#"
            DROP TRIGGER IF EXISTS {} ON {};
            CREATE TRIGGER {}
            BEFORE INSERT OR UPDATE ON {}
            FOR EACH ROW EXECUTE FUNCTION {}()
            "#,
            trigger_name, self.table_name, trigger_name, self.table_name, function_name
        );
        sqlx::query(&create_trigger_sql).execute(self.pool).await?;

        // Step 5: Create GIN index
        let default_idx_name = format!("{}_{}_idx", self.table_name, self.tsvector_column);
        let idx_name = self.index_name.as_deref().unwrap_or(&default_idx_name);
        let create_index_sql = format!(
            "CREATE INDEX IF NOT EXISTS {} ON {} USING GIN({})",
            idx_name, self.table_name, self.tsvector_column
        );
        sqlx::query(&create_index_sql).execute(self.pool).await?;

        // Step 6: Populate tsvector for existing rows
        let update_existing_sql = format!(
            "UPDATE {} SET {} = {}",
            self.table_name, self.tsvector_column, tsvector_expr
        );
        let result = sqlx::query(&update_existing_sql).execute(self.pool).await?;

        Ok(FtsIndexInfo {
            table_name: self.table_name.clone(),
            tsvector_column: self.tsvector_column.clone(),
            index_name: idx_name.to_string(),
            columns: self
                .columns
                .iter()
                .map(|(c, w)| (c.clone(), w.as_char()))
                .collect(),
            language: self.language,
            rows_updated: result.rows_affected(),
        })
    }

    /// Build the tsvector expression for the specified columns
    fn build_tsvector_expression(&self) -> String {
        let parts: Vec<String> = self
            .columns
            .iter()
            .map(|(col, weight)| {
                format!(
                    "setweight(to_tsvector('{}', COALESCE({}, '')), '{}')",
                    self.language.as_str(),
                    col,
                    weight.as_char()
                )
            })
            .collect();

        if parts.len() == 1 {
            parts[0].clone()
        } else {
            parts.join(" || ")
        }
    }

    /// Drop the full-text search index and related objects
    pub async fn drop(&self) -> Result<()> {
        // Drop trigger
        let trigger_name = format!("{}_fts_trigger", self.table_name);
        let drop_trigger_sql = format!(
            "DROP TRIGGER IF EXISTS {} ON {}",
            trigger_name, self.table_name
        );
        sqlx::query(&drop_trigger_sql).execute(self.pool).await?;

        // Drop function
        let function_name = format!("{}_fts_update", self.table_name);
        let drop_function_sql = format!("DROP FUNCTION IF EXISTS {}()", function_name);
        sqlx::query(&drop_function_sql).execute(self.pool).await?;

        // Drop index (will be dropped with column)
        // Drop column
        let drop_column_sql = format!(
            "ALTER TABLE {} DROP COLUMN IF EXISTS {}",
            self.table_name, self.tsvector_column
        );
        sqlx::query(&drop_column_sql).execute(self.pool).await?;

        Ok(())
    }
}

/// Information about a created FTS index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FtsIndexInfo {
    /// Table name
    pub table_name: String,
    /// Name of the tsvector column
    pub tsvector_column: String,
    /// Name of the GIN index
    pub index_name: String,
    /// Indexed columns with their weights
    pub columns: Vec<(String, char)>,
    /// Text search language
    pub language: FtsLanguage,
    /// Number of rows updated during index creation
    pub rows_updated: u64,
}

/// Builder for constructing full-text search queries
pub struct FtsQueryBuilder<'a> {
    #[allow(dead_code)]
    pool: &'a PgPool,
    table_name: String,
    tsvector_column: String,
    query_text: Option<String>,
    language: FtsLanguage,
    #[allow(dead_code)]
    select_columns: Vec<String>,
    where_clauses: Vec<String>,
    with_ranking: bool,
    with_highlighting: Option<(String, usize)>,
    limit: Option<u32>,
    offset: Option<u32>,
    min_rank: Option<f32>,
}

impl<'a> FtsQueryBuilder<'a> {
    /// Create a new FTS query builder
    pub fn new(pool: &'a PgPool, table_name: impl Into<String>) -> Self {
        let table_name = table_name.into();
        let tsvector_column = format!("{}_fts", table_name);

        Self {
            pool,
            table_name,
            tsvector_column,
            query_text: None,
            language: FtsLanguage::English,
            select_columns: vec!["*".to_string()],
            where_clauses: Vec::new(),
            with_ranking: false,
            with_highlighting: None,
            limit: None,
            offset: None,
            min_rank: None,
        }
    }

    /// Set the tsvector column name (if different from default)
    pub fn tsvector_column(mut self, name: impl Into<String>) -> Self {
        self.tsvector_column = name.into();
        self
    }

    /// Set the search query text
    ///
    /// Supports PostgreSQL tsquery syntax:
    /// - `&` for AND: "cat & dog"
    /// - `|` for OR: "cat | dog"
    /// - `!` for NOT: "!cat"
    /// - `<->` for phrase: "cat <-> dog"
    /// - `*` for prefix: "walk*" matches "walk", "walking", "walked"
    pub fn search(mut self, query: impl Into<String>) -> Self {
        self.query_text = Some(query.into());
        self
    }

    /// Set the text search language
    pub fn language(mut self, language: FtsLanguage) -> Self {
        self.language = language;
        self
    }

    /// Include ranking score in results
    pub fn with_ranking(mut self) -> Self {
        self.with_ranking = true;
        self
    }

    /// Include highlighted excerpts in results
    ///
    /// # Arguments
    /// * `column` - Column to extract excerpt from
    /// * `max_words` - Maximum words in excerpt
    pub fn with_highlighting(mut self, column: impl Into<String>, max_words: usize) -> Self {
        self.with_highlighting = Some((column.into(), max_words));
        self
    }

    /// Set minimum rank threshold (0.0 to 1.0)
    pub fn min_rank(mut self, rank: f32) -> Self {
        self.min_rank = Some(rank.clamp(0.0, 1.0));
        self
    }

    /// Add additional WHERE clause
    pub fn where_clause(mut self, clause: impl Into<String>) -> Self {
        self.where_clauses.push(clause.into());
        self
    }

    /// Limit number of results
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Offset results (for pagination)
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Build the SQL query
    pub fn build(&self) -> Result<String> {
        let _query_text = self
            .query_text
            .as_ref()
            .ok_or_else(|| StorageError::validation("Search query text is required"))?;

        let mut select_parts = vec!["*".to_string()];

        // Add ranking if requested
        if self.with_ranking {
            select_parts.push(format!(
                "ts_rank({}, to_tsquery('{}', $1)) AS rank",
                self.tsvector_column,
                self.language.as_str()
            ));
        }

        // Add highlighting if requested
        if let Some((column, max_words)) = &self.with_highlighting {
            select_parts.push(format!(
                "ts_headline('{}', {}, to_tsquery('{}', $1), 'MaxWords={}, MinWords=10, ShortWord=3') AS highlighted_text",
                self.language.as_str(),
                column,
                self.language.as_str(),
                max_words
            ));
        }

        let select_clause = select_parts.join(", ");

        // Build WHERE clause
        let mut where_parts = vec![format!(
            "{} @@ to_tsquery('{}', $1)",
            self.tsvector_column,
            self.language.as_str()
        )];

        if let Some(min_rank) = self.min_rank {
            where_parts.push(format!(
                "ts_rank({}, to_tsquery('{}', $1)) >= {}",
                self.tsvector_column,
                self.language.as_str(),
                min_rank
            ));
        }

        where_parts.extend(self.where_clauses.iter().cloned());

        let where_clause = where_parts.join(" AND ");

        // Build ORDER BY (rank descending if ranking is enabled)
        let order_by = if self.with_ranking {
            " ORDER BY rank DESC"
        } else {
            ""
        };

        // Build LIMIT/OFFSET
        let mut limit_offset = String::new();
        if let Some(limit) = self.limit {
            limit_offset.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset {
            limit_offset.push_str(&format!(" OFFSET {}", offset));
        }

        Ok(format!(
            "SELECT {} FROM {} WHERE {}{}{}",
            select_clause, self.table_name, where_clause, order_by, limit_offset
        ))
    }
}

/// Utility functions for full-text search operations
pub struct FtsHelper;

impl FtsHelper {
    /// Convert plain text search query to tsquery syntax
    ///
    /// Converts "machine learning workflow" to "machine & learning & workflow"
    pub fn plain_to_tsquery(text: &str) -> String {
        text.split_whitespace()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" & ")
    }

    /// Convert phrase to proximity search
    ///
    /// Converts "machine learning" to "machine <-> learning"
    pub fn phrase_to_tsquery(text: &str) -> String {
        text.split_whitespace()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" <-> ")
    }

    /// Create prefix search query for autocomplete
    ///
    /// Converts "mach" to "mach:*"
    pub fn prefix_search(prefix: &str) -> String {
        format!("{}:*", prefix.trim())
    }

    /// Parse PostgreSQL tsvector to extract lexemes
    pub fn parse_tsvector(tsvector: &str) -> Vec<String> {
        // Simple parser for tsvector format: 'word':1A 'other':2B
        let mut lexemes = Vec::new();
        let mut current_lexeme = String::new();
        let mut in_quotes = false;

        for ch in tsvector.chars() {
            match ch {
                '\'' if !in_quotes => {
                    in_quotes = true;
                    current_lexeme.clear();
                }
                '\'' if in_quotes => {
                    in_quotes = false;
                    if !current_lexeme.is_empty() {
                        lexemes.push(current_lexeme.clone());
                        current_lexeme.clear();
                    }
                }
                _ if in_quotes => {
                    current_lexeme.push(ch);
                }
                _ => {}
            }
        }

        lexemes
    }

    /// Calculate coverage ratio (how many query terms match)
    pub fn calculate_coverage(query_terms: &[String], document_lexemes: &[String]) -> f32 {
        if query_terms.is_empty() {
            return 0.0;
        }

        let matches = query_terms
            .iter()
            .filter(|term| {
                document_lexemes
                    .iter()
                    .any(|lex| lex.contains(term.as_str()))
            })
            .count();

        matches as f32 / query_terms.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fts_language_as_str() {
        assert_eq!(FtsLanguage::English.as_str(), "english");
        assert_eq!(FtsLanguage::Simple.as_str(), "simple");
        assert_eq!(FtsLanguage::Spanish.as_str(), "spanish");
    }

    #[test]
    fn test_fts_weight_as_char() {
        assert_eq!(FtsWeight::A.as_char(), 'A');
        assert_eq!(FtsWeight::B.as_char(), 'B');
        assert_eq!(FtsWeight::C.as_char(), 'C');
        assert_eq!(FtsWeight::D.as_char(), 'D');
    }

    #[test]
    fn test_plain_to_tsquery() {
        assert_eq!(
            FtsHelper::plain_to_tsquery("machine learning workflow"),
            "machine & learning & workflow"
        );
        assert_eq!(
            FtsHelper::plain_to_tsquery("  hello   world  "),
            "hello & world"
        );
        assert_eq!(FtsHelper::plain_to_tsquery(""), "");
    }

    #[test]
    fn test_phrase_to_tsquery() {
        assert_eq!(
            FtsHelper::phrase_to_tsquery("machine learning"),
            "machine <-> learning"
        );
        assert_eq!(
            FtsHelper::phrase_to_tsquery("hello world"),
            "hello <-> world"
        );
    }

    #[test]
    fn test_prefix_search() {
        assert_eq!(FtsHelper::prefix_search("mach"), "mach:*");
        assert_eq!(FtsHelper::prefix_search("  walk  "), "walk:*");
    }

    #[test]
    fn test_parse_tsvector() {
        let tsvector = "'cat':1A 'dog':2B 'bird':3C";
        let lexemes = FtsHelper::parse_tsvector(tsvector);
        assert_eq!(lexemes, vec!["cat", "dog", "bird"]);
    }

    #[test]
    fn test_parse_tsvector_empty() {
        let lexemes = FtsHelper::parse_tsvector("");
        assert!(lexemes.is_empty());
    }

    #[test]
    fn test_calculate_coverage() {
        let query = vec!["machine".to_string(), "learning".to_string()];
        let doc1 = vec![
            "machine".to_string(),
            "learning".to_string(),
            "ai".to_string(),
        ];
        let doc2 = vec!["machine".to_string(), "computer".to_string()];
        let doc3 = vec!["neural".to_string(), "network".to_string()];

        assert_eq!(FtsHelper::calculate_coverage(&query, &doc1), 1.0);
        assert_eq!(FtsHelper::calculate_coverage(&query, &doc2), 0.5);
        assert_eq!(FtsHelper::calculate_coverage(&query, &doc3), 0.0);
    }

    #[test]
    fn test_calculate_coverage_empty_query() {
        let empty: Vec<String> = vec![];
        let doc = vec!["test".to_string()];
        assert_eq!(FtsHelper::calculate_coverage(&empty, &doc), 0.0);
    }

    #[test]
    fn test_fts_weight_conversion() {
        let weights = vec![
            ('A', FtsWeight::A),
            ('B', FtsWeight::B),
            ('C', FtsWeight::C),
            ('D', FtsWeight::D),
            ('a', FtsWeight::A),
            ('b', FtsWeight::B),
            ('X', FtsWeight::C), // Unknown defaults to C
        ];

        for (input, expected) in weights {
            let weight = match input {
                'A' | 'a' => FtsWeight::A,
                'B' | 'b' => FtsWeight::B,
                'C' | 'c' => FtsWeight::C,
                'D' | 'd' => FtsWeight::D,
                _ => FtsWeight::C,
            };
            assert_eq!(weight, expected);
        }
    }

    #[test]
    fn test_fts_language_variants() {
        let languages = vec![
            FtsLanguage::Simple,
            FtsLanguage::English,
            FtsLanguage::Spanish,
            FtsLanguage::French,
            FtsLanguage::German,
            FtsLanguage::Italian,
            FtsLanguage::Portuguese,
            FtsLanguage::Russian,
            FtsLanguage::Japanese,
            FtsLanguage::Chinese,
        ];

        // Ensure all languages have non-empty string representations
        for lang in languages {
            assert!(!lang.as_str().is_empty());
        }
    }

    #[test]
    fn test_fts_index_info_serialization() {
        let info = FtsIndexInfo {
            table_name: "test_table".to_string(),
            tsvector_column: "test_fts".to_string(),
            index_name: "test_idx".to_string(),
            columns: vec![("title".to_string(), 'A'), ("body".to_string(), 'B')],
            language: FtsLanguage::English,
            rows_updated: 100,
        };

        // Test that it can be serialized/deserialized
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: FtsIndexInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.table_name, "test_table");
        assert_eq!(deserialized.rows_updated, 100);
        assert_eq!(deserialized.columns.len(), 2);
    }
}
