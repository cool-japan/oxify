//! Authorization engine implementing the check API

use crate::*;
use moka::future::Cache;
use oxisql_core::{Connection, Row};
use oxisql_pool::sqlite::{new_sqlite_compat_pool, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Translate a database URL into a path understood by the pure-Rust SQLite
/// (Limbo) backend used by `oxisql`.
///
/// Accepted forms and their normalisation:
/// - `sqlite::memory:`, `sqlite://:memory:`, `:memory:` → `":memory:"`
/// - `sqlite:/path/to.db`, `sqlite:///path/to.db`, `/path/to.db` → the file path
///
/// Returns `None` for URLs carrying a non-SQLite scheme (e.g. `postgres://…`),
/// signalling that the SQLite backend cannot serve that URL.
pub(crate) fn sqlite_path_from_url(database_url: &str) -> Option<String> {
    let rest = if let Some(r) = database_url.strip_prefix("sqlite://") {
        r
    } else if let Some(r) = database_url.strip_prefix("sqlite:") {
        r
    } else if database_url.contains("://") {
        // A non-SQLite scheme — unsupported by this backend.
        return None;
    } else {
        database_url
    };

    let normalised = if rest.is_empty() || rest == ":memory:" || rest == "memory:" {
        ":memory:"
    } else {
        rest
    };
    Some(normalised.to_string())
}

/// Pool size used for a file-backed SQLite database.
///
/// Every pooled connection re-opens the same on-disk file, so all slots
/// observe identical, shared state; a larger pool simply allows more
/// concurrent authorization checks in flight at once.
const FILE_POOL_SIZE: usize = 20;

/// Pool size forced for an in-memory (`:memory:`) SQLite database.
///
/// The pure-Rust SQLite (Limbo) backend gives every pooled connection to an
/// in-memory database its own **independent, empty** database — pool slots
/// share no state (this mirrors upstream SQLite's own semantics for private
/// in-memory connections; see the `oxisql_pool::sqlite_compat` module docs,
/// which state plainly: "For in-memory databases (`:memory:`) each pool slot
/// is independent — there is no shared state between pool slots"). A pool
/// size greater than 1 against `:memory:` would therefore silently split
/// authorization state across multiple invisible databases as soon as more
/// than one connection is checked out concurrently — e.g. two overlapping
/// `check`/`write_tuple` calls served by a gRPC handler — corrupting
/// permission-check results with no error raised. Every method on
/// `AuthzEngine` acquires-and-drops its own pooled connection per call, so
/// nothing short of a single-connection pool eliminates the hazard for good.
/// Capping the pool at one connection makes every checkout observe the same
/// database, at the cost of serialising concurrent access — the same
/// trade-off recommended upstream (e.g. sqlx's own `:memory:` guidance) for a
/// single-instance in-memory SQLite database.
const MEMORY_POOL_SIZE: usize = 1;

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
        let path = sqlite_path_from_url(database_url).ok_or_else(|| {
            AuthzError::DatabaseError(format!(
                "Unsupported database URL for the SQLite backend: {database_url}"
            ))
        })?;
        // See `MEMORY_POOL_SIZE` for why `:memory:` cannot safely use a
        // multi-connection pool.
        let pool_size = if path == ":memory:" {
            MEMORY_POOL_SIZE
        } else {
            FILE_POOL_SIZE
        };
        let pool = new_sqlite_compat_pool(path, pool_size)
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
        // Bind order: $1 namespace, $2 object_id, $3 relation,
        //             $4 subject_type, $5 subject_id, $6 subject_relation.
        let subject_type = match &tuple.subject {
            Subject::User(_) => "user",
            Subject::UserSet { .. } => "userset",
        };
        let subject_id = match &tuple.subject {
            Subject::User(id) => id.clone(),
            Subject::UserSet {
                namespace,
                object_id,
                ..
            } => format!("{}:{}", namespace, object_id),
        };
        let subject_relation: Option<String> = match &tuple.subject {
            Subject::User(_) => None,
            Subject::UserSet { relation, .. } => Some(relation.clone()),
        };

        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO authz_relation_tuples
                (namespace, object_id, relation, subject_type, subject_id, subject_relation)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            &[
                &tuple.namespace,  // $1
                &tuple.object_id,  // $2
                &tuple.relation,   // $3
                &subject_type,     // $4
                &subject_id,       // $5
                &subject_relation, // $6
            ],
        )
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
        // Bind order: $1 namespace, $2 object_id, $3 relation,
        //             $4 subject_type, $5 subject_id.
        let subject_type = match &tuple.subject {
            Subject::User(_) => "user",
            Subject::UserSet { .. } => "userset",
        };
        let subject_id = match &tuple.subject {
            Subject::User(id) => id.clone(),
            Subject::UserSet {
                namespace,
                object_id,
                ..
            } => format!("{}:{}", namespace, object_id),
        };

        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        conn.execute(
            r#"
            DELETE FROM authz_relation_tuples
            WHERE namespace = $1
              AND object_id = $2
              AND relation = $3
              AND subject_type = $4
              AND subject_id = $5
            "#,
            &[
                &tuple.namespace, // $1
                &tuple.object_id, // $2
                &tuple.relation,  // $3
                &subject_type,    // $4
                &subject_id,      // $5
            ],
        )
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
        // Bind order: $1 namespace, $2 object_id, $3 relation,
        //             $4 subject_type, $5 subject_id.
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

        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        let rows = conn
            .query(
                r#"
            SELECT COUNT(*) as count FROM authz_relation_tuples
            WHERE namespace = $1
              AND object_id = $2
              AND relation = $3
              AND subject_type = $4
              AND subject_id = $5
            "#,
                &[
                    &request.namespace, // $1
                    &request.object_id, // $2
                    &request.relation,  // $3
                    &subject_type,      // $4
                    &subject_id,        // $5
                ],
            )
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to check direct: {}", e)))?;

        let count: i64 = match rows.first() {
            Some(row) => row.try_get("count").unwrap_or(0),
            None => 0,
        };
        Ok(count > 0)
    }

    /// Find all usersets a user belongs to
    async fn find_usersets_for_user(&self, user_id: &str) -> Result<Vec<Subject>> {
        // Bind order: $1 subject_id (the user id). `subject_type = 'user'` is a
        // SQL string literal, not a bind parameter.
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        let rows = conn
            .query(
                r#"
            SELECT namespace, object_id, relation
            FROM authz_relation_tuples
            WHERE subject_type = 'user'
              AND subject_id = $1
            "#,
                &[&user_id], // $1
            )
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to find usersets: {}", e)))?;

        let mut usersets = Vec::new();
        for row in rows {
            let namespace: String = row
                .try_get("namespace")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;
            let object_id: String = row
                .try_get("object_id")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;
            let relation: String = row
                .try_get("relation")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;

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
        // Bind order: $1 namespace, $2 object_id, $3 relation.
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        let rows = conn
            .query(
                r#"
            SELECT subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE namespace = $1
              AND object_id = $2
              AND relation = $3
            "#,
                &[
                    &request.namespace, // $1
                    &request.object_id, // $2
                    &request.relation,  // $3
                ],
            )
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to expand: {}", e)))?;

        let mut subjects = Vec::new();
        for row in rows {
            let subject_type: String = row
                .try_get("subject_type")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;
            let subject_id: String = row
                .try_get("subject_id")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;

            let subject = if subject_type == "user" {
                Subject::User(subject_id)
            } else {
                let subject_relation: Option<String> = row
                    .try_get("subject_relation")
                    .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;
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
        // The schema file is a multi-statement SQL script; `execute_batch`
        // splits it (quote/comment aware) and runs each statement in turn.
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        conn.execute_batch(include_str!("../migrations/20260702000001__init.sql"))
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

        // A single pooled connection is reused for every probe in this batch.
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;

        for (_, request, _) in requests {
            // Bind order: $1 namespace, $2 object_id, $3 relation,
            //             $4 subject_type, $5 subject_id.
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

            let rows = conn
                .query(
                    r#"
                SELECT COUNT(*) as count FROM authz_relation_tuples
                WHERE namespace = $1
                  AND object_id = $2
                  AND relation = $3
                  AND subject_type = $4
                  AND subject_id = $5
                "#,
                    &[
                        &request.namespace, // $1
                        &request.object_id, // $2
                        &request.relation,  // $3
                        &subject_type,      // $4
                        &subject_id,        // $5
                    ],
                )
                .await
                .map_err(|e| AuthzError::DatabaseError(format!("Batch check failed: {}", e)))?;

            let count: i64 = match rows.first() {
                Some(row) => row.try_get("count").unwrap_or(0),
                None => 0,
            };
            results.push(count > 0);
        }

        Ok(results)
    }

    /// Load existing tuples into the Bloom filter (for warm-up)
    pub async fn warm_bloom_filter(&self) -> Result<usize> {
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        let rows = conn
            .query(
                r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            "#,
                &[],
            )
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Failed to load tuples: {}", e)))?;

        // `row_to_tuple` applies the same parse-and-skip-malformed-userset logic
        // used elsewhere, returning `None` for rows that should be ignored.
        let mut count = 0;
        for tuple in rows.iter().filter_map(Self::row_to_tuple) {
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
    fn row_to_tuple(row: &Row) -> Option<RelationTuple> {
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

        // Bind order: $1 subject_type, $2 subject_id.
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        let rows = conn
            .query(
                r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE subject_type = $1
              AND subject_id = $2
            "#,
                &[
                    &subject_type, // $1
                    &subject_id,   // $2
                ],
            )
            .await
            .map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to list subject tuples: {}", e))
            })?;

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
        // Bind order: $1 namespace, $2 object_id.
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        let rows = conn
            .query(
                r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE namespace = $1
              AND object_id = $2
            "#,
                &[
                    &namespace, // $1
                    &object_id, // $2
                ],
            )
            .await
            .map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to list object tuples: {}", e))
            })?;

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
        // Bind order: $1 namespace. `limit` is an internal integer inlined as a
        // SQL literal rather than passed as a second bound parameter: the
        // pure-Rust SQLite (Limbo) backend mishandles a query that has a bound
        // LIMIT *together with* another bound parameter elsewhere (verified
        // empirically — a lone bound LIMIT works fine, but combining it with a
        // WHERE-clause parameter such as $1 above silently returns zero rows).
        // Inlining a value that is always an internal `i64`/`usize` (never
        // untrusted input) sidesteps the interaction entirely and is
        // injection-safe.
        let limit = limit as i64;
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        let sql = format!(
            r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE namespace = $1
            ORDER BY id ASC
            LIMIT {limit}
            "#
        );
        let rows = conn.query(&sql, &[&namespace]).await.map_err(|e| {
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
        // Bind order: $1 cutoff modifier (used inside datetime('now', $1)).
        // `limit` is inlined as a SQL literal rather than bound as $2: the
        // pure-Rust SQLite (Limbo) backend mishandles a bound LIMIT combined
        // with another bound parameter in the same query — verified
        // empirically, this specific shape (`datetime('now', $1) ... LIMIT
        // $2`) does not merely return wrong rows but panics inside the VDBE
        // interpreter (`unreachable code: DecrJumpZero on non-integer
        // register`). A lone bound LIMIT (no other params) is unaffected.
        // Inlining a value that is always an internal `u32`/`usize` (never
        // untrusted input) sidesteps the interaction entirely and is
        // injection-safe.
        let cutoff = format!("-{} days", days);
        let limit = limit as i64;
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;
        let sql = format!(
            r#"
            SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation
            FROM authz_relation_tuples
            WHERE created_at >= datetime('now', $1)
            ORDER BY created_at DESC
            LIMIT {limit}
            "#
        );
        let rows = conn.query(&sql, &[&cutoff]).await.map_err(|e| {
            AuthzError::DatabaseError(format!("Failed to list recent tuples: {}", e))
        })?;

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

    /// An in-memory database MUST be served from a single-connection pool:
    /// the Limbo backend gives every additional pool slot its own
    /// independent, empty database (no shared state between slots), so a
    /// pool size > 1 would silently corrupt authorization state as soon as
    /// two connections were checked out concurrently. See `MEMORY_POOL_SIZE`.
    #[tokio::test]
    async fn test_memory_url_forces_single_connection_pool() {
        for url in ["sqlite::memory:", "sqlite://:memory:", ":memory:"] {
            let engine = AuthzEngine::new(url)
                .await
                .unwrap_or_else(|e| panic!("Failed to create in-memory engine for {url}: {e}"));
            assert_eq!(
                engine.pool.max_size(),
                MEMORY_POOL_SIZE,
                "in-memory SQLite pool for {url} must be capped at exactly \
                 {MEMORY_POOL_SIZE} connection(s) to avoid split-brain state"
            );
        }
    }

    /// File-backed SQLite does not suffer the in-memory isolation problem —
    /// every connection re-opens the same file — so it should keep the
    /// larger, concurrency-friendly pool size.
    #[tokio::test]
    async fn test_file_url_uses_multi_connection_pool() {
        let path =
            std::env::temp_dir().join(format!("oxify_authz_pool_test_{}.db", uuid::Uuid::new_v4()));
        let url = format!("sqlite:{}", path.display());

        let engine = AuthzEngine::new(&url)
            .await
            .expect("Failed to create file-backed engine");
        assert_eq!(
            engine.pool.max_size(),
            FILE_POOL_SIZE,
            "file-backed SQLite pool should retain the multi-connection size"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// End-to-end proof that the single-connection cap actually preserves
    /// consistency under concurrency: many `write_tuple` calls are fired
    /// concurrently at a `:memory:` engine, then a subsequent read must see
    /// every one of them. Before the `MEMORY_POOL_SIZE` fix this was only
    /// accidentally true for strictly sequential access; a multi-threaded
    /// runtime with real concurrent checkouts would otherwise be able to
    /// split writes across independent, invisible in-memory databases.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_concurrent_writes_are_all_visible_on_memory_engine() {
        let engine = Arc::new(make_engine().await);

        let mut handles = Vec::new();
        for i in 0..25 {
            let engine = Arc::clone(&engine);
            handles.push(tokio::spawn(async move {
                engine
                    .write_tuple(RelationTuple::new(
                        "document",
                        "viewer",
                        "concurrent_doc",
                        Subject::User(format!("user{i}")),
                    ))
                    .await
                    .expect("concurrent write_tuple failed");
            }));
        }
        for handle in handles {
            handle.await.expect("writer task panicked");
        }

        let tuples = engine
            .list_object_tuples("document", "concurrent_doc")
            .await
            .expect("list_object_tuples failed");
        assert_eq!(
            tuples.len(),
            25,
            "all 25 concurrently-written tuples must be visible from a single \
             shared in-memory database"
        );
    }

    #[tokio::test]
    async fn test_basic_authorization() {
        // Deterministic fresh in-memory engine with the schema applied.
        let engine = make_engine().await;

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

        // Check: alice can view (viewer inherits from owner via the namespace
        // config), exercising the recursive permission path end to end.
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

        // Negative control: bob has no tuples, so he must be denied.
        let denied = engine
            .check(CheckRequest {
                namespace: "document".to_string(),
                object_id: "123".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("bob".to_string()),
                context: None,
            })
            .await
            .expect("check failed");

        assert!(!denied.allowed);
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
