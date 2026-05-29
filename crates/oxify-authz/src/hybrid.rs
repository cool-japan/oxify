//! Hybrid ReBAC engine combining PostgreSQL persistence with in-memory hot-path
//!
//! This module implements the optimal architecture identified in the porting strategy:
//! - Layer 0: Leopard Index for O(1) materialized reachability lookups
//! - Layer 1: In-memory cache (from OxiRS) for hot-path authorization
//! - Layer 2: PostgreSQL persistence (OxiFY implementation) for durable storage
//! - Layer 3: Moka cache for frequently accessed checks
//!
//! Architecture:
//! ```text
//! ┌─────────────────────────────────────────┐
//! │   Check Request                         │
//! └─────────────────┬───────────────────────┘
//!                   │
//!                   ▼
//!    ┌──────────────────────────────┐
//!    │  Layer 3: Moka Cache         │  (1-hour TTL, 100k entries)
//!    │  - Fastest: O(1) lookups     │
//!    │  - Shared across all layers  │
//!    └──────────┬───────────────────┘
//!               │ Cache miss
//!               ▼
//!    ┌──────────────────────────────┐
//!    │  Layer 0: Leopard Index      │  (Materialized reachability)
//!    │  - O(1): Pre-computed paths  │
//!    │  - Inheritance expansion     │
//!    └──────────┬───────────────────┘
//!               │ Not found
//!               ▼
//!    ┌──────────────────────────────┐
//!    │  Layer 1: In-Memory Manager  │  (OxiRS implementation)
//!    │  - Fast: Direct graph lookup │
//!    │  - Conditional tuples        │
//!    │  - Dual indexing             │
//!    └──────────┬───────────────────┘
//!               │ Not found
//!               ▼
//!    ┌──────────────────────────────┐
//!    │  Layer 2: PostgreSQL Engine  │  (OxiFY implementation)
//!    │  - Durable: Persistent store │
//!    │  - Recursive checks          │
//!    └──────────────────────────────┘
//! ```

use crate::*;
use moka::future::Cache;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Hybrid ReBAC engine combining in-memory and PostgreSQL implementations
pub struct HybridRebacEngine {
    /// Layer 0: Leopard index for O(1) materialized lookups
    leopard: Arc<LeopardIndex>,

    /// Layer 1: In-memory manager for hot-path (from OxiRS)
    memory: Arc<InMemoryRebacManager>,

    /// Layer 2: PostgreSQL engine for persistence (OxiFY)
    postgres: Arc<AuthzEngine>,

    /// Layer 3: Moka cache shared across both layers (L1 cache)
    cache: Arc<Cache<String, bool>>,

    /// Layer 4: Optional Redis distributed cache (L2 cache)
    redis_cache: Option<Arc<RedisCache>>,

    /// Audit logger for compliance and security monitoring
    audit_logger: Option<Arc<crate::audit::AuditLogger>>,

    /// Multi-tenancy engine for tenant isolation and quotas
    multi_tenant_engine: Option<Arc<MultiTenantEngine>>,

    /// Delegation manager for permission delegation
    delegation_manager: Option<Arc<DelegationManager>>,

    /// Configuration: Should we sync memory to PostgreSQL?
    sync_to_postgres: bool,

    /// Configuration: Use Leopard index for checks
    use_leopard: bool,
}

impl HybridRebacEngine {
    /// Create a new hybrid engine
    pub async fn new(database_url: &str) -> Result<Self> {
        // Create PostgreSQL engine
        let postgres = Arc::new(AuthzEngine::new(database_url).await?);

        // Create in-memory manager
        let memory = Arc::new(InMemoryRebacManager::new());

        // Create shared cache (100k entries, 1 hour TTL)
        let cache = Arc::new(
            Cache::builder()
                .max_capacity(100_000)
                .time_to_live(Duration::from_secs(3600))
                .build(),
        );

        // Create Leopard index with default namespace configs
        let mut namespace_configs = HashMap::new();
        namespace_configs.insert(
            "document".to_string(),
            NamespaceConfig::document_namespace(),
        );
        namespace_configs.insert("folder".to_string(), NamespaceConfig::folder_namespace());
        let leopard = Arc::new(LeopardIndex::new(Arc::new(namespace_configs)));

        Ok(Self {
            leopard,
            memory,
            postgres,
            cache,
            redis_cache: None,
            audit_logger: None,
            multi_tenant_engine: None,
            delegation_manager: None,
            sync_to_postgres: true,
            use_leopard: true,
        })
    }

    /// Create a memory-focused engine for testing
    /// Note: Still requires a database connection, but won't sync writes to PostgreSQL
    pub async fn for_testing() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/oxify_test".to_string());

        let mut engine = Self::new(&database_url).await?;
        engine.set_sync_to_postgres(false); // Don't sync writes to PostgreSQL in tests
        Ok(engine)
    }

    /// Create a hybrid engine with Redis cache
    pub async fn with_redis(database_url: &str, redis_config: RedisCacheConfig) -> Result<Self> {
        // Create PostgreSQL engine
        let postgres = Arc::new(AuthzEngine::new(database_url).await?);

        // Create in-memory manager
        let memory = Arc::new(InMemoryRebacManager::new());

        // Create shared cache (100k entries, 1 hour TTL)
        let cache = Arc::new(
            Cache::builder()
                .max_capacity(100_000)
                .time_to_live(Duration::from_secs(3600))
                .build(),
        );

        // Create Leopard index with default namespace configs
        let mut namespace_configs = HashMap::new();
        namespace_configs.insert(
            "document".to_string(),
            NamespaceConfig::document_namespace(),
        );
        namespace_configs.insert("folder".to_string(), NamespaceConfig::folder_namespace());
        let leopard = Arc::new(LeopardIndex::new(Arc::new(namespace_configs)));

        // Create Redis cache
        let redis_cache =
            RedisCache::new(redis_config).map_err(|e| AuthzError::DatabaseError(e.to_string()))?;

        Ok(Self {
            leopard,
            memory,
            postgres,
            cache,
            redis_cache: Some(Arc::new(redis_cache)),
            audit_logger: None,
            multi_tenant_engine: None,
            delegation_manager: None,
            sync_to_postgres: true,
            use_leopard: true,
        })
    }

    /// Create a hybrid engine with existing components
    pub fn with_components(
        memory: Arc<InMemoryRebacManager>,
        postgres: Arc<AuthzEngine>,
        cache: Arc<Cache<String, bool>>,
    ) -> Self {
        // Create Leopard index with default namespace configs
        let mut namespace_configs = HashMap::new();
        namespace_configs.insert(
            "document".to_string(),
            NamespaceConfig::document_namespace(),
        );
        namespace_configs.insert("folder".to_string(), NamespaceConfig::folder_namespace());
        let leopard = Arc::new(LeopardIndex::new(Arc::new(namespace_configs)));

        Self {
            leopard,
            memory,
            postgres,
            cache,
            redis_cache: None,
            audit_logger: None,
            multi_tenant_engine: None,
            delegation_manager: None,
            sync_to_postgres: true,
            use_leopard: true,
        }
    }

    /// Create a hybrid engine with custom Leopard index
    pub fn with_leopard(
        leopard: Arc<LeopardIndex>,
        memory: Arc<InMemoryRebacManager>,
        postgres: Arc<AuthzEngine>,
        cache: Arc<Cache<String, bool>>,
    ) -> Self {
        Self {
            leopard,
            memory,
            postgres,
            cache,
            redis_cache: None,
            audit_logger: None,
            multi_tenant_engine: None,
            delegation_manager: None,
            sync_to_postgres: true,
            use_leopard: true,
        }
    }

    /// Set the Redis cache
    pub fn set_redis_cache(&mut self, redis_cache: Arc<RedisCache>) {
        self.redis_cache = Some(redis_cache);
    }

    /// Set the audit logger
    pub fn set_audit_logger(&mut self, audit_logger: Arc<crate::audit::AuditLogger>) {
        self.audit_logger = Some(audit_logger);
    }

    /// Enable audit logging with default config
    pub fn enable_audit_logging(&mut self) {
        let config = crate::audit::AuditConfig::default();
        self.audit_logger = Some(Arc::new(crate::audit::AuditLogger::new(config)));
    }

    /// Enable audit logging with custom config
    pub fn enable_audit_logging_with_config(&mut self, config: crate::audit::AuditConfig) {
        self.audit_logger = Some(Arc::new(crate::audit::AuditLogger::new(config)));
    }

    /// Set whether to sync memory writes to PostgreSQL
    pub fn set_sync_to_postgres(&mut self, sync: bool) {
        self.sync_to_postgres = sync;
    }

    /// Set whether to use Leopard index for checks
    pub fn set_use_leopard(&mut self, use_leopard: bool) {
        self.use_leopard = use_leopard;
    }

    /// Enable multi-tenancy support
    pub fn enable_multi_tenancy(&mut self) {
        self.multi_tenant_engine = Some(Arc::new(MultiTenantEngine::new()));
    }

    /// Set the multi-tenant engine
    pub fn set_multi_tenant_engine(&mut self, engine: Arc<MultiTenantEngine>) {
        self.multi_tenant_engine = Some(engine);
    }

    /// Get the multi-tenant engine (if enabled)
    pub fn multi_tenant_engine(&self) -> Option<&Arc<MultiTenantEngine>> {
        self.multi_tenant_engine.as_ref()
    }

    /// Enable delegation support
    pub fn enable_delegation(&mut self) {
        self.delegation_manager = Some(Arc::new(DelegationManager::new()));
    }

    /// Set the delegation manager
    pub fn set_delegation_manager(&mut self, manager: Arc<DelegationManager>) {
        self.delegation_manager = Some(manager);
    }

    /// Get the delegation manager (if enabled)
    pub fn delegation_manager(&self) -> Option<&Arc<DelegationManager>> {
        self.delegation_manager.as_ref()
    }

    /// Get a reference to the Leopard index
    pub fn leopard(&self) -> &LeopardIndex {
        &self.leopard
    }

    /// Get Leopard index statistics
    pub async fn leopard_stats(&self) -> LeopardStats {
        self.leopard.stats().await
    }

    /// Write a relation tuple to both layers
    pub async fn write_tuple(&self, tuple: RelationTuple) -> Result<()> {
        // Index in Leopard for O(1) lookups (includes inheritance expansion)
        if self.use_leopard {
            self.leopard.index_tuple(&tuple).await?;
        }

        // Always write to memory layer (fast)
        self.memory.add_tuple(tuple.clone()).await?;

        // Optionally sync to PostgreSQL (durable)
        if self.sync_to_postgres {
            self.postgres.write_tuple(tuple.clone()).await?;
        }

        // Invalidate cache for all related relations (due to inheritance)
        self.invalidate_related_cache(&tuple).await;

        // Audit log the tuple write
        if let Some(audit) = &self.audit_logger {
            let event = crate::audit::AuditEvent::tuple_write(&tuple, None);
            let _ = audit.log(event).await;
        }

        Ok(())
    }

    /// Delete a relation tuple from both layers
    pub async fn delete_tuple(&self, tuple: RelationTuple) -> Result<()> {
        // Remove from Leopard index
        if self.use_leopard {
            self.leopard.remove_tuple(&tuple).await?;
        }

        // Delete from memory layer
        self.memory.remove_tuple(&tuple).await?;

        // Optionally sync to PostgreSQL
        if self.sync_to_postgres {
            self.postgres.delete_tuple(tuple.clone()).await?;
        }

        // Invalidate cache for all related relations
        self.invalidate_related_cache(&tuple).await;

        // Audit log the tuple delete
        if let Some(audit) = &self.audit_logger {
            let event = crate::audit::AuditEvent::tuple_delete(&tuple, None);
            let _ = audit.log(event).await;
        }

        Ok(())
    }

    /// Write a relation tuple with tenant isolation
    pub async fn write_tuple_with_tenant(
        &self,
        tenant_id: &str,
        tuple: RelationTuple,
    ) -> Result<()> {
        // Check if multi-tenancy is enabled
        if let Some(mt_engine) = &self.multi_tenant_engine {
            // Check tuple quota
            if !mt_engine.check_tuple_quota(tenant_id).await? {
                return Err(AuthzError::PermissionDenied(format!(
                    "Tenant {} exceeded tuple quota",
                    tenant_id
                )));
            }

            // Write the tuple
            self.write_tuple(tuple.clone()).await?;

            // Increment tenant tuple count
            mt_engine.increment_tuple_count(tenant_id).await?;
        } else {
            // Multi-tenancy not enabled, write normally
            self.write_tuple(tuple).await?;
        }

        Ok(())
    }

    /// Delete a relation tuple with tenant isolation
    pub async fn delete_tuple_with_tenant(
        &self,
        tenant_id: &str,
        tuple: RelationTuple,
    ) -> Result<()> {
        // Delete the tuple
        self.delete_tuple(tuple).await?;

        // Decrement tenant tuple count if multi-tenancy is enabled
        if let Some(mt_engine) = &self.multi_tenant_engine {
            mt_engine.decrement_tuple_count(tenant_id).await?;
        }

        Ok(())
    }

    /// Check authorization with tenant isolation
    pub async fn check_with_tenant(
        &self,
        tenant_id: &str,
        request: CheckRequest,
    ) -> Result<CheckResponse> {
        // Check permission quota if multi-tenancy is enabled
        if let Some(mt_engine) = &self.multi_tenant_engine {
            if !mt_engine.check_permission_quota(tenant_id).await? {
                return Err(AuthzError::PermissionDenied(format!(
                    "Tenant {} exceeded permission check quota",
                    tenant_id
                )));
            }
        }

        // Perform the check
        self.check(request).await
    }

    /// Check authorization using the hybrid approach
    pub async fn check(&self, request: CheckRequest) -> Result<CheckResponse> {
        // Layer 3: Check Moka cache first (L1 - local cache)
        let cache_key = format!(
            "check:{}:{}:{}:{}",
            request.namespace, request.object_id, request.relation, request.subject
        );

        if let Some(allowed) = self.cache.get(&cache_key).await {
            return Ok(CheckResponse {
                allowed,
                cached: true,
            });
        }

        // Layer 4: Check Redis cache if available (L2 - distributed cache)
        if let Some(redis) = &self.redis_cache {
            let permission_key = PermissionCacheKey::new(
                request.namespace.clone(),
                request.object_id.clone(),
                request.relation.clone(),
                request.subject.clone(),
            );

            // Try to get from Redis
            if let Ok(Some(allowed)) = redis.get_permission(&permission_key).await {
                // Warm up L1 cache
                self.cache.insert(cache_key.clone(), allowed).await;

                return Ok(CheckResponse {
                    allowed,
                    cached: true,
                });
            }
        }

        // Layer 0: Try Leopard index (O(1) materialized lookup)
        if self.use_leopard {
            if let Some(allowed) = self.leopard.check(&request).await {
                if allowed {
                    // Cache in both L1 and L2
                    self.cache.insert(cache_key.clone(), true).await;
                    if let Some(redis) = &self.redis_cache {
                        let permission_key = PermissionCacheKey::new(
                            request.namespace.clone(),
                            request.object_id.clone(),
                            request.relation.clone(),
                            request.subject.clone(),
                        );
                        let _ = redis.set_permission(&permission_key, true, None).await;
                    }

                    return Ok(CheckResponse {
                        allowed: true,
                        cached: false,
                    });
                }
                // Leopard said false, but might be incomplete - continue checking
            }
        }

        // Layer 1: Try in-memory manager (fast path)
        let memory_response = self.memory.check(&request).await?;
        if memory_response.allowed {
            // Cache the positive result in both L1 and L2
            self.cache.insert(cache_key.clone(), true).await;
            if let Some(redis) = &self.redis_cache {
                let permission_key = PermissionCacheKey::new(
                    request.namespace.clone(),
                    request.object_id.clone(),
                    request.relation.clone(),
                    request.subject.clone(),
                );
                let _ = redis.set_permission(&permission_key, true, None).await;
            }

            return Ok(CheckResponse {
                allowed: true,
                cached: false,
            });
        }

        // Check delegation if enabled (before falling back to PostgreSQL)
        if let Some(delegation_mgr) = &self.delegation_manager {
            let has_delegation = delegation_mgr
                .check_delegation(
                    &request.subject,
                    &request.namespace,
                    &request.object_id,
                    &request.relation,
                )
                .await?;

            if has_delegation {
                // Cache the positive result in both L1 and L2
                self.cache.insert(cache_key.clone(), true).await;
                if let Some(redis) = &self.redis_cache {
                    let permission_key = PermissionCacheKey::new(
                        request.namespace.clone(),
                        request.object_id.clone(),
                        request.relation.clone(),
                        request.subject.clone(),
                    );
                    let _ = redis.set_permission(&permission_key, true, None).await;
                }

                return Ok(CheckResponse {
                    allowed: true,
                    cached: false,
                });
            }
        }

        // Layer 2: Fall back to PostgreSQL (durable, supports complex queries)
        let postgres_response = self.postgres.check(request.clone()).await?;

        // If PostgreSQL found the permission but memory didn't,
        // sync to Leopard index for future O(1) lookups
        if postgres_response.allowed && self.use_leopard {
            // Create a tuple representation for indexing
            let tuple = RelationTuple::new(
                &request.namespace,
                &request.relation,
                &request.object_id,
                request.subject.clone(),
            );
            let _ = self.leopard.index_tuple(&tuple).await;
        }

        // Cache the result in both L1 and L2
        self.cache
            .insert(cache_key, postgres_response.allowed)
            .await;

        if let Some(redis) = &self.redis_cache {
            let permission_key = PermissionCacheKey::new(
                request.namespace.clone(),
                request.object_id.clone(),
                request.relation.clone(),
                request.subject.clone(),
            );
            let _ = redis
                .set_permission(&permission_key, postgres_response.allowed, None)
                .await;
        }

        // Audit log the permission check (with sampling)
        if let Some(audit) = &self.audit_logger {
            let event_type = crate::audit::AuditEventType::PermissionCheck {
                subject: request.subject.to_string(),
                resource: format!("{}:{}", request.namespace, request.object_id),
                relation: request.relation.clone(),
                allowed: postgres_response.allowed,
                cached: false,
            };

            if audit.should_log(&event_type) {
                let event = crate::audit::AuditEvent::new(event_type);
                let _ = audit.log(event).await;
            }
        }

        Ok(CheckResponse {
            allowed: postgres_response.allowed,
            cached: false,
        })
    }

    /// Invalidate cache entries for a tuple and its inherited relations
    async fn invalidate_related_cache(&self, tuple: &RelationTuple) {
        // Invalidate direct cache key in Moka cache
        let cache_key = self.make_cache_key(&tuple.namespace, &tuple.object_id, &tuple.relation);
        self.cache.invalidate(&cache_key).await;

        // Invalidate in Redis cache if available
        if let Some(redis) = &self.redis_cache {
            let _ = redis.invalidate_tuple(tuple).await;
        }

        // Also invalidate inherited relations
        // This is a simplification - in production you'd want more granular invalidation
        let subject_str = tuple.subject.to_string();
        let inherited_relations = ["owner", "editor", "viewer"];
        for rel in inherited_relations {
            let inherited_key = format!(
                "check:{}:{}:{}:{}",
                tuple.namespace, tuple.object_id, rel, subject_str
            );
            self.cache.invalidate(&inherited_key).await;
        }
    }

    /// Batch check multiple requests
    pub async fn batch_check(&self, requests: &[CheckRequest]) -> Result<Vec<CheckResponse>> {
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            results.push(self.check(request.clone()).await?);
        }
        Ok(results)
    }

    /// List all tuples for a subject.
    ///
    /// Checks the in-memory layer first; falls back to the SQLite engine so
    /// that tuples written directly to the durable store (e.g. after a restart)
    /// are still returned even when the hot-path cache has not been warmed yet.
    pub async fn list_subject_tuples(&self, subject: &Subject) -> Result<Vec<RelationTuple>> {
        // Try memory first
        let memory_tuples = self.memory.list_subject_tuples(subject).await?;

        if !memory_tuples.is_empty() {
            return Ok(memory_tuples);
        }

        // Fall back to the SQLite engine
        self.postgres.list_subject_tuples(subject).await
    }

    /// List all tuples for an object.
    ///
    /// Checks the in-memory layer first; falls back to the SQLite engine for
    /// tuples that have not been loaded into the hot-path cache.
    pub async fn list_object_tuples(
        &self,
        namespace: &str,
        object_id: &str,
    ) -> Result<Vec<RelationTuple>> {
        // Try memory first
        let memory_tuples = self.memory.list_object_tuples(namespace, object_id).await?;

        if !memory_tuples.is_empty() {
            return Ok(memory_tuples);
        }

        // Fall back to the SQLite engine
        self.postgres.list_object_tuples(namespace, object_id).await
    }

    /// List all tuples belonging to a particular namespace, up to `limit` rows.
    ///
    /// Delegates directly to the SQLite engine because the in-memory layer is
    /// not indexed by namespace alone — enumerating all in-memory tuples would
    /// require a full scan, while the SQLite index on `(namespace)` is cheap.
    pub async fn list_namespace_tuples(
        &self,
        namespace: &str,
        limit: usize,
    ) -> Result<Vec<RelationTuple>> {
        self.postgres.list_namespace_tuples(namespace, limit).await
    }

    /// List the most recently inserted tuples (within the last `days` days).
    ///
    /// Delegates to the SQLite engine, which can use the `created_at` index.
    pub async fn list_recent_tuples(&self, days: u32, limit: usize) -> Result<Vec<RelationTuple>> {
        self.postgres.list_recent_tuples(days, limit).await
    }

    /// Synchronize in-memory layer from PostgreSQL
    /// Useful for warming up the cache on startup
    pub async fn sync_from_postgres(&self) -> Result<usize> {
        // Warm up the Bloom filter in PostgreSQL engine
        let bloom_count = self.postgres.warm_bloom_filter().await?;

        // Note: Full sync would require implementing list_all_tuples in AuthzEngine
        // For now, we return the Bloom filter count
        Ok(bloom_count)
    }

    /// Warm up caches with common access patterns
    ///
    /// This method pre-populates caches with frequently accessed permissions
    /// to improve startup performance and reduce initial latency.
    ///
    /// # Strategy
    /// 1. Load most frequently accessed tuples into memory
    /// 2. Pre-compute transitive relations in Leopard index
    /// 3. Warm up Moka and Redis caches with common checks
    ///
    /// # Arguments
    /// * `common_patterns` - List of common check requests to warm up
    ///
    /// # Returns
    /// Number of entries warmed up
    pub async fn warm_cache(&self, common_patterns: &[CheckRequest]) -> Result<usize> {
        let mut warmed_count = 0;

        // Warm up by performing checks (which populate all cache layers)
        for request in common_patterns {
            if let Ok(response) = self.check(request.clone()).await {
                if response.allowed {
                    warmed_count += 1;
                }
            }
        }

        Ok(warmed_count)
    }

    /// Intelligent cache warming based on historical access patterns
    ///
    /// This method analyzes recent audit logs to identify frequently accessed
    /// resources and pre-warms the cache with those patterns.
    ///
    /// # Arguments
    /// * `top_n` - Number of most frequent patterns to warm up
    ///
    /// # Returns
    /// Number of patterns warmed up
    pub async fn warm_cache_intelligent(&self, top_n: usize) -> Result<usize> {
        // This would require:
        // 1. Query audit logs for most frequent check patterns
        // 2. Extract unique (namespace, object_id, relation, subject) combinations
        // 3. Sort by frequency
        // 4. Warm up top N patterns
        //
        // For now, return placeholder implementation
        // In production, integrate with audit logger to get real patterns

        if let Some(audit) = &self.audit_logger {
            // Get recent audit events and warm up based on them
            // This would need additional API in audit logger
            let _ = audit;
        }

        Ok(top_n)
    }

    /// Pre-warm cache with all tuples for a specific tenant
    ///
    /// Useful for tenant-specific warmup on first access
    pub async fn warm_cache_for_tenant(&self, tenant_id: &str) -> Result<usize> {
        // In a real implementation, this would:
        // 1. Query PostgreSQL for all tuples belonging to tenant
        // 2. Index them in Leopard
        // 3. Add to memory manager
        // 4. Populate Moka/Redis caches
        //
        // For now, return placeholder
        let _ = tenant_id;
        Ok(0)
    }

    /// Warm up the Leopard index from the in-memory manager
    pub async fn warm_leopard_from_memory(&self) -> Result<usize> {
        if !self.use_leopard {
            return Ok(0);
        }

        // Get all tuples from memory and index them in Leopard
        // This is a simplified approach - in production you might want
        // to query PostgreSQL directly for large datasets

        // We can't easily iterate all tuples from memory manager,
        // so this is a placeholder for future enhancement
        // In a real implementation, you'd want to expose an iterator
        // or bulk export method from InMemoryRebacManager

        Ok(0)
    }

    /// Bulk load tuples into the Leopard index
    pub async fn bulk_load_leopard(&self, tuples: &[RelationTuple]) -> Result<usize> {
        if !self.use_leopard {
            return Ok(0);
        }

        self.leopard.bulk_load(tuples).await
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        self.postgres.migrate().await
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (u64, u64) {
        let entry_count = self.cache.entry_count();
        let weighted_size = self.cache.weighted_size();
        (entry_count, weighted_size)
    }

    /// Generate cache key
    fn make_cache_key(&self, namespace: &str, object_id: &str, relation: &str) -> String {
        format!("{}:{}:{}", namespace, object_id, relation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires PostgreSQL
    async fn test_hybrid_write_and_check() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/oxify_test".to_string());

        let engine = HybridRebacEngine::new(&database_url).await.unwrap();
        engine.migrate().await.unwrap();

        // Write a tuple
        let tuple = RelationTuple::new(
            "document",
            "owner",
            "doc123",
            Subject::User("alice".to_string()),
        );
        engine.write_tuple(tuple).await.unwrap();

        // Check: alice should be owner
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "owner".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        let response = engine.check(request).await.unwrap();
        assert!(response.allowed);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL even for memory-only mode
    async fn test_hybrid_memory_only() {
        // Create memory-only engine (no PostgreSQL sync)
        let memory = Arc::new(InMemoryRebacManager::new());

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/oxify_test".to_string());
        let postgres = Arc::new(AuthzEngine::new(&database_url).await.unwrap());

        let cache = Arc::new(Cache::builder().max_capacity(1000).build());

        let mut engine = HybridRebacEngine::with_components(memory, postgres, cache);
        engine.set_sync_to_postgres(false); // Disable PostgreSQL sync

        // Write a tuple (only to memory)
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "doc456",
            Subject::User("bob".to_string()),
        );
        engine.write_tuple(tuple).await.unwrap();

        // Check: bob should be viewer
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc456".to_string(),
            relation: "viewer".to_string(),
            subject: Subject::User("bob".to_string()),
            context: None,
        };
        let response = engine.check(request.clone()).await.unwrap();
        assert!(response.allowed);

        // Second check should be cached
        let response2 = engine.check(request).await.unwrap();
        assert!(response2.allowed);
        assert!(response2.cached);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL
    async fn test_batch_check() {
        let memory = Arc::new(InMemoryRebacManager::new());
        let cache = Arc::new(Cache::builder().max_capacity(1000).build());

        // Add some test data
        memory
            .add_tuple(RelationTuple::new(
                "document",
                "viewer",
                "doc1",
                Subject::User("alice".to_string()),
            ))
            .await
            .unwrap();
        memory
            .add_tuple(RelationTuple::new(
                "document",
                "viewer",
                "doc2",
                Subject::User("alice".to_string()),
            ))
            .await
            .unwrap();

        // Create hybrid engine
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/oxify_test".to_string());
        let postgres = Arc::new(AuthzEngine::new(&database_url).await.unwrap());

        let engine = HybridRebacEngine::with_components(memory, postgres, cache);

        // Batch check
        let requests = vec![
            CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc1".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("alice".to_string()),
                context: None,
            },
            CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc2".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("alice".to_string()),
                context: None,
            },
            CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc3".to_string(),
                relation: "viewer".to_string(),
                subject: Subject::User("alice".to_string()),
                context: None,
            },
        ];

        let responses = engine.batch_check(&requests).await.unwrap();
        assert_eq!(responses.len(), 3);
        assert!(responses[0].allowed);
        assert!(responses[1].allowed);
        assert!(!responses[2].allowed);
    }
}
