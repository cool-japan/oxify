//! Authorization engine implementing the check API

use crate::*;
use moka::future::Cache;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The main authorization engine
pub struct AuthzEngine {
    pool: SqlitePool,
    cache: Arc<Cache<String, bool>>,
    namespace_configs: Arc<HashMap<String, NamespaceConfig>>,
    /// Bloom filter for quick negative lookups
    bloom_filter: Arc<AuthzBloomFilter>,
    /// Track Bloom filter statistics
    bloom_stats: Arc<BloomStatsTracker>,
}

/// Thread-safe tracker for Bloom filter statistics
pub struct BloomStatsTracker {
    definite_negatives: AtomicU64,
    potential_positives: AtomicU64,
    true_positives: AtomicU64,
    false_positives: AtomicU64,
}

impl Default for BloomStatsTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl BloomStatsTracker {
    pub fn new() -> Self {
        Self {
            definite_negatives: AtomicU64::new(0),
            potential_positives: AtomicU64::new(0),
            true_positives: AtomicU64::new(0),
            false_positives: AtomicU64::new(0),
        }
    }

    pub fn record_definite_negative(&self) {
        self.definite_negatives.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_potential_positive(&self) {
        self.potential_positives.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_true_positive(&self) {
        self.true_positives.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_false_positive(&self) {
        self.false_positives.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> BloomStats {
        BloomStats {
            definite_negatives: self.definite_negatives.load(Ordering::Relaxed),
            potential_positives: self.potential_positives.load(Ordering::Relaxed),
            true_positives: self.true_positives.load(Ordering::Relaxed),
            false_positives: self.false_positives.load(Ordering::Relaxed),
        }
    }
}

impl AuthzEngine {
    /// Create a new authorization engine
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(20)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to connect: {}", e)))?;

        // Cache for authorization checks (100k entries, 1 hour TTL)
        let cache = Cache::builder()
            .max_capacity(100_000)
            .time_to_live(Duration::from_secs(3600))
            .build();

        // Load namespace configurations
        let mut namespace_configs = HashMap::new();
        namespace_configs.insert(
            "document".to_string(),
            NamespaceConfig::document_namespace(),
        );
        namespace_configs.insert("folder".to_string(), NamespaceConfig::folder_namespace());

        // Initialize Bloom filter with 1M capacity and 1% false positive rate
        let bloom_filter = Arc::new(AuthzBloomFilter::with_config(BloomConfig {
            expected_items: 1_000_000,
            false_positive_rate: 0.01,
        }));

        Ok(Self {
            pool,
            cache: Arc::new(cache),
            namespace_configs: Arc::new(namespace_configs),
            bloom_filter,
            bloom_stats: Arc::new(BloomStatsTracker::new()),
        })
    }

    /// Get the Bloom filter statistics
    pub fn bloom_stats(&self) -> BloomStats {
        self.bloom_stats.get_stats()
    }

    /// Get a reference to the Bloom filter
    pub fn bloom_filter(&self) -> &AuthzBloomFilter {
        &self.bloom_filter
    }

    /// Write a relation tuple
    pub async fn write_tuple(&self, tuple: RelationTuple) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO authz_relation_tuples
                (namespace, object_id, relation, subject_type, subject_id, subject_relation)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&tuple.namespace)
        .bind(&tuple.object_id)
        .bind(&tuple.relation)
        .bind(match &tuple.subject {
            Subject::User(_) => "user",
            Subject::UserSet { .. } => "userset",
        })
        .bind(match &tuple.subject {
            Subject::User(id) => id.clone(),
            Subject::UserSet {
                namespace,
                object_id,
                ..
            } => format!("{}:{}", namespace, object_id),
        })
        .bind(match &tuple.subject {
            Subject::User(_) => None,
            Subject::UserSet { relation, .. } => Some(relation.clone()),
        })
        .execute(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to write tuple: {}", e)))?;

        // Add to Bloom filter for quick negative lookups
        self.bloom_filter.add_tuple(&tuple);

        // Invalidate cache for this object
        let cache_key = self.cache_key(&tuple.namespace, &tuple.object_id, &tuple.relation);
        self.cache.invalidate(&cache_key).await;

        Ok(())
    }

    /// Delete a relation tuple
    pub async fn delete_tuple(&self, tuple: RelationTuple) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM authz_relation_tuples
            WHERE namespace = ?
              AND object_id = ?
              AND relation = ?
              AND subject_type = ?
              AND subject_id = ?
            "#,
        )
        .bind(&tuple.namespace)
        .bind(&tuple.object_id)
        .bind(&tuple.relation)
        .bind(match &tuple.subject {
            Subject::User(_) => "user",
            Subject::UserSet { .. } => "userset",
        })
        .bind(match &tuple.subject {
            Subject::User(id) => id.clone(),
            Subject::UserSet {
                namespace,
                object_id,
                ..
            } => format!("{}:{}", namespace, object_id),
        })
        .execute(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to delete tuple: {}", e)))?;

        // Invalidate cache
        let cache_key = self.cache_key(&tuple.namespace, &tuple.object_id, &tuple.relation);
        self.cache.invalidate(&cache_key).await;

        Ok(())
    }

    /// Check if a subject has a relation to an object
    pub async fn check(&self, request: CheckRequest) -> Result<CheckResponse> {
        // Generate cache key
        let cache_key = format!(
            "check:{}:{}:{}:{}",
            request.namespace, request.object_id, request.relation, request.subject
        );

        // Check cache first
        if let Some(allowed) = self.cache.get(&cache_key).await {
            return Ok(CheckResponse {
                allowed,
                cached: true,
            });
        }

        // Perform recursive check
        let allowed = self
            .check_recursive(&request, 0, &mut HashSet::new())
            .await?;

        // Cache the result
        self.cache.insert(cache_key, allowed).await;

        Ok(CheckResponse {
            allowed,
            cached: false,
        })
    }

    /// Recursive check implementation (depth-first search)
    fn check_recursive<'a>(
        &'a self,
        request: &'a CheckRequest,
        depth: usize,
        visited: &'a mut HashSet<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool>> + Send + 'a>> {
        Box::pin(async move {
            // Prevent infinite recursion
            if depth > 10 {
                return Err(AuthzError::CycleDetected);
            }

            let visit_key = format!(
                "{}:{}:{}",
                request.namespace, request.object_id, request.relation
            );
            if visited.contains(&visit_key) {
                return Ok(false); // Already visited, avoid cycle
            }
            visited.insert(visit_key);

            // Direct check: Is there a direct tuple?
            let direct = self.check_direct(request).await?;
            if direct {
                return Ok(true);
            }

            // Check inherited relations
            if let Some(namespace_config) = self.namespace_configs.get(&request.namespace) {
                if let Some(relation_config) = namespace_config
                    .relations
                    .iter()
                    .find(|r| r.name == request.relation)
                {
                    // Check if subject has any inherited relation
                    for inherited_relation in &relation_config.inherits_from {
                        let inherited_request = CheckRequest {
                            namespace: request.namespace.clone(),
                            object_id: request.object_id.clone(),
                            relation: inherited_relation.clone(),
                            subject: request.subject.clone(),
                            context: None,
                        };

                        if self
                            .check_recursive(&inherited_request, depth + 1, visited)
                            .await?
                        {
                            return Ok(true);
                        }
                    }
                }
            }

            // Check userset expansion
            if let Subject::User(user_id) = &request.subject {
                // Find all usersets this user belongs to
                let usersets = self.find_usersets_for_user(user_id).await?;

                for userset in usersets {
                    let userset_request = CheckRequest {
                        namespace: request.namespace.clone(),
                        object_id: request.object_id.clone(),
                        relation: request.relation.clone(),
                        subject: userset,
                        context: None,
                    };

                    if self
                        .check_recursive(&userset_request, depth + 1, visited)
                        .await?
                    {
                        return Ok(true);
                    }
                }
            }

            Ok(false)
        })
    }

    /// Check for a direct tuple match
    async fn check_direct(&self, request: &CheckRequest) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM authz_relation_tuples
            WHERE namespace = ?
              AND object_id = ?
              AND relation = ?
              AND subject_type = ?
              AND subject_id = ?
            "#,
        )
        .bind(&request.namespace)
        .bind(&request.object_id)
        .bind(&request.relation)
        .bind(match &request.subject {
            Subject::User(_) => "user",
            Subject::UserSet { .. } => "userset",
        })
        .bind(match &request.subject {
            Subject::User(id) => id.clone(),
            Subject::UserSet {
                namespace,
                object_id,
                ..
            } => format!("{}:{}", namespace, object_id),
        })
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to check direct: {}", e)))?;

        let count: i64 = row.try_get("count").unwrap_or(0);
        Ok(count > 0)
    }

    /// Find all usersets a user belongs to
    async fn find_usersets_for_user(&self, user_id: &str) -> Result<Vec<Subject>> {
        let rows = sqlx::query(
            r#"
            SELECT namespace, object_id, relation
            FROM authz_relation_tuples
            WHERE subject_type = 'user'
              AND subject_id = ?
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to find usersets: {}", e)))?;

        let mut usersets = Vec::new();
        for row in rows {
            let namespace: String = row.get("namespace");
            let object_id: String = row.get("object_id");
            let relation: String = row.get("relation");

            usersets.push(Subject::UserSet {
                namespace,
                object_id,
                relation,
            });
        }

        Ok(usersets)
    }

    /// Expand a relation to find all subjects
    pub async fn expand(&self, request: ExpandRequest) -> Result<ExpandResponse> {
        let rows = sqlx::query(
            r#"
            SELECT subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE namespace = ?
              AND object_id = ?
              AND relation = ?
            "#,
        )
        .bind(&request.namespace)
        .bind(&request.object_id)
        .bind(&request.relation)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to expand: {}", e)))?;

        let mut subjects = Vec::new();
        for row in rows {
            let subject_type: String = row.get("subject_type");
            let subject_id: String = row.get("subject_id");

            let subject = if subject_type == "user" {
                Subject::User(subject_id)
            } else {
                let subject_relation: Option<String> = row.get("subject_relation");
                let parts: Vec<&str> = subject_id.split(':').collect();
                if let (2, Some(relation)) = (parts.len(), subject_relation) {
                    Subject::UserSet {
                        namespace: parts[0].to_string(),
                        object_id: parts[1].to_string(),
                        relation,
                    }
                } else {
                    continue; // Skip invalid usersets
                }
            };

            subjects.push(subject);
        }

        Ok(ExpandResponse { subjects })
    }

    /// Generate cache key
    fn cache_key(&self, namespace: &str, object_id: &str, relation: &str) -> String {
        format!("{}:{}:{}", namespace, object_id, relation)
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        sqlx::query(include_str!("../migrations/001_init.sql"))
            .execute(&self.pool)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Migration failed: {}", e)))?;

        Ok(())
    }

    /// Batch check multiple authorization requests efficiently
    ///
    /// This method uses Bloom filter to skip definitely non-existent tuples
    /// and PostgreSQL ANY() for efficient batch queries.
    ///
    /// Performance targets:
    /// - 100 checks: <50ms total
    /// - Bloom filter reduces DB queries by ~50%
    pub async fn batch_check(&self, requests: &[CheckRequest]) -> Result<Vec<CheckResponse>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = vec![None; requests.len()];

        // Phase 1: Check cache first
        let mut cache_misses = Vec::new();
        for (idx, request) in requests.iter().enumerate() {
            let cache_key = format!(
                "check:{}:{}:{}:{}",
                request.namespace, request.object_id, request.relation, request.subject
            );

            if let Some(allowed) = self.cache.get(&cache_key).await {
                results[idx] = Some(CheckResponse {
                    allowed,
                    cached: true,
                });
            } else {
                cache_misses.push((idx, request, cache_key));
            }
        }

        if cache_misses.is_empty() {
            return Ok(results
                .into_iter()
                .map(|r| r.expect("invariant: all results populated before return"))
                .collect());
        }

        // Phase 2: Use Bloom filter to filter out definitely non-existent tuples
        let mut bloom_positives = Vec::new();
        for (idx, request, cache_key) in cache_misses {
            if self.bloom_filter.might_contain(request) {
                self.bloom_stats.record_potential_positive();
                bloom_positives.push((idx, request, cache_key));
            } else {
                // Bloom filter says definitely not there
                self.bloom_stats.record_definite_negative();
                results[idx] = Some(CheckResponse {
                    allowed: false,
                    cached: false,
                });
            }
        }

        if bloom_positives.is_empty() {
            return Ok(results
                .into_iter()
                .map(|r| r.expect("invariant: all results populated before return"))
                .collect());
        }

        // Phase 3: Batch query PostgreSQL using ANY() for direct checks
        let db_results = self.batch_check_direct(&bloom_positives).await?;

        // Phase 4: For items not found directly, do recursive checks
        for ((idx, request, cache_key), found) in bloom_positives.into_iter().zip(db_results) {
            let allowed = if found {
                self.bloom_stats.record_true_positive();
                true
            } else {
                // Need to do recursive check for inherited relations
                let recursive_result = self
                    .check_recursive(request, 0, &mut HashSet::new())
                    .await?;
                if recursive_result {
                    self.bloom_stats.record_true_positive();
                } else {
                    self.bloom_stats.record_false_positive();
                }
                recursive_result
            };

            // Cache the result
            self.cache.insert(cache_key, allowed).await;

            results[idx] = Some(CheckResponse {
                allowed,
                cached: false,
            });
        }

        Ok(results
            .into_iter()
            .map(|r| r.expect("invariant: all results populated before return"))
            .collect())
    }

    /// Batch check direct tuples (SQLite version uses individual queries)
    async fn batch_check_direct(
        &self,
        requests: &[(usize, &CheckRequest, String)],
    ) -> Result<Vec<bool>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        // For SQLite, we check each request individually
        // This is less efficient than PostgreSQL's unnest, but SQLite doesn't support arrays
        let mut results = Vec::with_capacity(requests.len());

        for (_, request, _) in requests {
            let subject_type = match &request.subject {
                Subject::User(_) => "user",
                Subject::UserSet { .. } => "userset",
            };
            let subject_id = match &request.subject {
                Subject::User(id) => id.clone(),
                Subject::UserSet {
                    namespace,
                    object_id,
                    ..
                } => format!("{}:{}", namespace, object_id),
            };

            let row = sqlx::query(
                r#"
                SELECT COUNT(*) as count FROM authz_relation_tuples
                WHERE namespace = ?
                  AND object_id = ?
                  AND relation = ?
                  AND subject_type = ?
                  AND subject_id = ?
                "#,
            )
            .bind(&request.namespace)
            .bind(&request.object_id)
            .bind(&request.relation)
            .bind(subject_type)
            .bind(&subject_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Batch check failed: {}", e)))?;

            let count: i64 = row.try_get("count").unwrap_or(0);
            results.push(count > 0);
        }

        Ok(results)
    }

    /// Load existing tuples into the Bloom filter (for warm-up)
    pub async fn warm_bloom_filter(&self) -> Result<usize> {
        let rows = sqlx::query(
            r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to load tuples: {}", e)))?;

        let mut count = 0;
        for row in rows {
            let namespace: String = row.get("namespace");
            let object_id: String = row.get("object_id");
            let relation: String = row.get("relation");
            let subject_type: String = row.get("subject_type");
            let subject_id: String = row.get("subject_id");
            let subject_relation: Option<String> = row.get("subject_relation");

            let subject = if subject_type == "user" {
                Subject::User(subject_id)
            } else {
                let parts: Vec<&str> = subject_id.split(':').collect();
                if parts.len() == 2 {
                    Subject::UserSet {
                        namespace: parts[0].to_string(),
                        object_id: parts[1].to_string(),
                        relation: subject_relation.unwrap_or_default(),
                    }
                } else {
                    continue;
                }
            };

            let tuple = RelationTuple::new(&namespace, &relation, &object_id, subject);
            self.bloom_filter.add_tuple(&tuple);
            count += 1;
        }

        Ok(count)
    }

    /// Parse a database row into a `RelationTuple`.
    ///
    /// Returns `None` when the row contains a malformed userset entry that
    /// should be silently skipped (mirrors the `continue` pattern used in
    /// `warm_bloom_filter`).
    fn row_to_tuple(row: &sqlx::sqlite::SqliteRow) -> Option<RelationTuple> {
        let namespace: String = row
            .try_get("namespace")
            .map_err(|e| {
                tracing::warn!("Failed to get namespace from row: {}", e);
                e
            })
            .ok()?;
        let object_id: String = row
            .try_get("object_id")
            .map_err(|e| {
                tracing::warn!("Failed to get object_id from row: {}", e);
                e
            })
            .ok()?;
        let relation: String = row
            .try_get("relation")
            .map_err(|e| {
                tracing::warn!("Failed to get relation from row: {}", e);
                e
            })
            .ok()?;
        let subject_type: String = row
            .try_get("subject_type")
            .map_err(|e| {
                tracing::warn!("Failed to get subject_type from row: {}", e);
                e
            })
            .ok()?;
        let subject_id: String = row
            .try_get("subject_id")
            .map_err(|e| {
                tracing::warn!("Failed to get subject_id from row: {}", e);
                e
            })
            .ok()?;
        let subject_relation: Option<String> = row
            .try_get("subject_relation")
            .map_err(|e| {
                tracing::warn!("Failed to get subject_relation from row: {}", e);
                e
            })
            .ok()?;

        let subject = if subject_type == "user" {
            Subject::User(subject_id)
        } else {
            let parts: Vec<&str> = subject_id.split(':').collect();
            if parts.len() == 2 {
                Subject::UserSet {
                    namespace: parts[0].to_string(),
                    object_id: parts[1].to_string(),
                    relation: subject_relation.unwrap_or_default(),
                }
            } else {
                return None;
            }
        };

        Some(RelationTuple::new(
            &namespace, &relation, &object_id, subject,
        ))
    }

    /// List all tuples for a given subject.
    ///
    /// Returns every `(namespace, object_id, relation)` combination where the
    /// provided subject appears on the right-hand side of the stored tuple.
    pub async fn list_subject_tuples(&self, subject: &Subject) -> Result<Vec<RelationTuple>> {
        let (subject_type, subject_id) = match subject {
            Subject::User(id) => ("user".to_string(), id.clone()),
            Subject::UserSet {
                namespace,
                object_id,
                ..
            } => (
                "userset".to_string(),
                format!("{}:{}", namespace, object_id),
            ),
        };

        let rows = sqlx::query(
            r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE subject_type = ?
              AND subject_id = ?
            "#,
        )
        .bind(&subject_type)
        .bind(&subject_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to list subject tuples: {}", e)))?;

        Ok(rows.iter().filter_map(Self::row_to_tuple).collect())
    }

    /// List all tuples for a given object (namespace + object_id pair).
    ///
    /// Returns every `(namespace, object_id, relation, subject)` tuple stored
    /// against the named object, allowing callers to enumerate who has any
    /// relation to it.
    pub async fn list_object_tuples(
        &self,
        namespace: &str,
        object_id: &str,
    ) -> Result<Vec<RelationTuple>> {
        let rows = sqlx::query(
            r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE namespace = ?
              AND object_id = ?
            "#,
        )
        .bind(namespace)
        .bind(object_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to list object tuples: {}", e)))?;

        Ok(rows.iter().filter_map(Self::row_to_tuple).collect())
    }

    /// List all tuples belonging to a particular namespace.
    ///
    /// Results are ordered by insertion order (rowid ascending) and limited
    /// to `limit` rows to prevent unbounded scans.  Used by cache warming to
    /// pre-load namespace-scoped permissions.
    pub async fn list_namespace_tuples(
        &self,
        namespace: &str,
        limit: usize,
    ) -> Result<Vec<RelationTuple>> {
        let rows = sqlx::query(
            r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE namespace = ?
            ORDER BY id ASC
            LIMIT ?
            "#,
        )
        .bind(namespace)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to list namespace tuples: {}", e))
        })?;

        Ok(rows.iter().filter_map(Self::row_to_tuple).collect())
    }

    /// List the most recently inserted tuples.
    ///
    /// Uses the `created_at` column (populated by the SQLite `datetime('now')`
    /// default) to find tuples created within the last `days` days.  Results
    /// are ordered newest-first and capped at `limit` rows.
    pub async fn list_recent_tuples(&self, days: u32, limit: usize) -> Result<Vec<RelationTuple>> {
        let cutoff = format!("-{} days", days);
        let rows = sqlx::query(
            r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE created_at >= datetime('now', ?)
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(&cutoff)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to list recent tuples: {}", e)))?;

        Ok(rows.iter().filter_map(Self::row_to_tuple).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a fresh in-memory SQLite engine with the schema already applied.
    async fn make_engine() -> AuthzEngine {
        let engine = AuthzEngine::new("sqlite::memory:")
            .await
            .expect("Failed to create in-memory engine");
        engine.migrate().await.expect("Failed to run migrations");
        engine
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_basic_authorization() {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());

        let engine = AuthzEngine::new(&database_url)
            .await
            .expect("Failed to create engine");
        engine.migrate().await.expect("Migration failed");

        // Write: alice owns document:123
        engine
            .write_tuple(RelationTuple::new(
                "document",
                "owner",
                "123",
                Subject::User("alice".to_string()),
            ))
            .await
            .expect("write_tuple failed");

        // Check: alice can view (owner inherits viewer)
        let response = engine
            .check(CheckRequest {
                namespace: "document".to_string(),
                object_id: "123".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("alice".to_string()),
                context: None,
            })
            .await
            .expect("check failed");

        assert!(response.allowed);
    }

    #[tokio::test]
    async fn test_list_subject_tuples_empty() {
        let engine = make_engine().await;
        let tuples = engine
            .list_subject_tuples(&Subject::User("nobody".to_string()))
            .await
            .expect("list_subject_tuples failed");
        assert!(tuples.is_empty());
    }

    #[tokio::test]
    async fn test_list_subject_tuples_single_user() {
        let engine = make_engine().await;

        engine
            .write_tuple(RelationTuple::new(
                "document",
                "owner",
                "doc1",
                Subject::User("alice".to_string()),
            ))
            .await
            .expect("write_tuple failed");
        engine
            .write_tuple(RelationTuple::new(
                "document",
                "viewer",
                "doc2",
                Subject::User("alice".to_string()),
            ))
            .await
            .expect("write_tuple failed");
        // Tuple for a different user — should not appear in alice's list
        engine
            .write_tuple(RelationTuple::new(
                "document",
                "viewer",
                "doc2",
                Subject::User("bob".to_string()),
            ))
            .await
            .expect("write_tuple failed");

        let tuples = engine
            .list_subject_tuples(&Subject::User("alice".to_string()))
            .await
            .expect("list_subject_tuples failed");

        assert_eq!(tuples.len(), 2);
        assert!(tuples
            .iter()
            .all(|t| t.subject == Subject::User("alice".to_string())));
    }

    #[tokio::test]
    async fn test_list_object_tuples_empty() {
        let engine = make_engine().await;
        let tuples = engine
            .list_object_tuples("document", "nonexistent")
            .await
            .expect("list_object_tuples failed");
        assert!(tuples.is_empty());
    }

    #[tokio::test]
    async fn test_list_object_tuples_multiple_subjects() {
        let engine = make_engine().await;

        for user in &["alice", "bob", "carol"] {
            engine
                .write_tuple(RelationTuple::new(
                    "document",
                    "viewer",
                    "shared_doc",
                    Subject::User(user.to_string()),
                ))
                .await
                .expect("write_tuple failed");
        }
        // Different object — should not appear
        engine
            .write_tuple(RelationTuple::new(
                "document",
                "viewer",
                "private_doc",
                Subject::User("alice".to_string()),
            ))
            .await
            .expect("write_tuple failed");

        let tuples = engine
            .list_object_tuples("document", "shared_doc")
            .await
            .expect("list_object_tuples failed");

        assert_eq!(tuples.len(), 3);
        assert!(tuples.iter().all(|t| t.object_id == "shared_doc"));
    }

    #[tokio::test]
    async fn test_list_namespace_tuples() {
        let engine = make_engine().await;

        engine
            .write_tuple(RelationTuple::new(
                "document",
                "owner",
                "doc1",
                Subject::User("alice".to_string()),
            ))
            .await
            .expect("write_tuple failed");
        engine
            .write_tuple(RelationTuple::new(
                "folder",
                "owner",
                "folder1",
                Subject::User("alice".to_string()),
            ))
            .await
            .expect("write_tuple failed");

        let doc_tuples = engine
            .list_namespace_tuples("document", 100)
            .await
            .expect("list_namespace_tuples failed");
        assert_eq!(doc_tuples.len(), 1);
        assert_eq!(doc_tuples[0].namespace, "document");

        let folder_tuples = engine
            .list_namespace_tuples("folder", 100)
            .await
            .expect("list_namespace_tuples failed");
        assert_eq!(folder_tuples.len(), 1);
        assert_eq!(folder_tuples[0].namespace, "folder");
    }

    #[tokio::test]
    async fn test_list_namespace_tuples_limit() {
        let engine = make_engine().await;

        for i in 0..10 {
            engine
                .write_tuple(RelationTuple::new(
                    "document",
                    "viewer",
                    format!("doc{}", i),
                    Subject::User("alice".to_string()),
                ))
                .await
                .expect("write_tuple failed");
        }

        let limited = engine
            .list_namespace_tuples("document", 5)
            .await
            .expect("list_namespace_tuples failed");
        assert_eq!(limited.len(), 5);
    }

    #[tokio::test]
    async fn test_list_recent_tuples() {
        let engine = make_engine().await;

        engine
            .write_tuple(RelationTuple::new(
                "document",
                "owner",
                "doc_recent",
                Subject::User("alice".to_string()),
            ))
            .await
            .expect("write_tuple failed");

        // Requesting the last 7 days should include the just-inserted row
        let recent = engine
            .list_recent_tuples(7, 100)
            .await
            .expect("list_recent_tuples failed");
        assert!(!recent.is_empty());
        assert!(recent.iter().any(|t| t.object_id == "doc_recent"));
    }

    #[tokio::test]
    async fn test_list_subject_tuples_userset() {
        let engine = make_engine().await;

        let userset = Subject::UserSet {
            namespace: "team".to_string(),
            object_id: "engineering".to_string(),
            relation: "member".to_string(),
        };
        engine
            .write_tuple(RelationTuple::new(
                "document",
                "viewer",
                "doc1",
                userset.clone(),
            ))
            .await
            .expect("write_tuple failed");

        let tuples = engine
            .list_subject_tuples(&userset)
            .await
            .expect("list_subject_tuples failed");
        assert_eq!(tuples.len(), 1);
        assert_eq!(tuples[0].object_id, "doc1");
    }
}
