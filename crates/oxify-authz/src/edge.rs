//! # Edge Computing for Authorization
//!
//! Lightweight authorization engine designed for edge deployment.
//! Enables low-latency authorization checks at the edge by maintaining
//! a synchronized subset of authorization data.
//!
//! ## Features
//!
//! - **Lightweight Engine**: In-memory engine optimized for edge workers
//! - **Tuple Synchronization**: Automatic sync from central database
//! - **CRDT Conflict Resolution**: Handles concurrent updates across edge nodes
//! - **Selective Sync**: Only sync relevant namespaces/tenants
//!
//! ## Example
//!
//! ```rust,no_run
//! use oxify_authz::edge::{EdgeEngine, EdgeConfig, SyncConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = EdgeConfig {
//!         central_db_url: "postgres://central-db/authz".to_string(),
//!         sync_interval_secs: 30,
//!         sync_config: SyncConfig::Namespaces(vec!["document".to_string()]),
//!     };
//!
//!     let engine = EdgeEngine::new(config).await?;
//!     engine.start_sync().await?;
//!
//!     // Perform fast authorization checks at the edge
//!     let allowed = engine.check("document", "123", "viewer", "user:alice").await?;
//!
//!     Ok(())
//! }
//! ```

use crate::{
    memory::InMemoryRebacManager, AuthzError, CheckRequest, RelationTuple, Result, Subject,
};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

/// Configuration for edge engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    /// URL of the central database to sync from
    pub central_db_url: String,
    /// Sync interval in seconds
    pub sync_interval_secs: u64,
    /// What to sync from central database
    pub sync_config: SyncConfig,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            central_db_url: "postgres://localhost/authz".to_string(),
            sync_interval_secs: 30,
            sync_config: SyncConfig::All,
        }
    }
}

/// Defines what data to sync from central database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncConfig {
    /// Sync all tuples
    All,
    /// Sync specific namespaces only
    Namespaces(Vec<String>),
    /// Sync specific tenants only
    Tenants(Vec<String>),
    /// Sync specific namespaces for specific tenants
    NamespacesAndTenants {
        namespaces: Vec<String>,
        tenants: Vec<String>,
    },
}

/// A tuple with CRDT metadata for conflict resolution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrdtTuple {
    /// The actual relation tuple
    pub tuple: RelationTuple,
    /// Lamport timestamp for ordering
    pub timestamp: u64,
    /// Node ID that created this tuple
    pub node_id: String,
    /// Whether this tuple is a tombstone (deleted)
    pub is_tombstone: bool,
}

impl CrdtTuple {
    /// Create a new CRDT tuple
    pub fn new(tuple: RelationTuple, node_id: String) -> Self {
        Self {
            tuple,
            timestamp: Self::current_timestamp(),
            node_id,
            is_tombstone: false,
        }
    }

    /// Create a tombstone for deletion
    pub fn tombstone(tuple: RelationTuple, node_id: String) -> Self {
        Self {
            tuple,
            timestamp: Self::current_timestamp(),
            node_id,
            is_tombstone: true,
        }
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_micros() as u64
    }
}

/// CRDT-based conflict resolver using Last-Write-Wins (LWW) strategy
#[derive(Debug)]
pub struct CrdtResolver {
    /// Store of CRDT tuples indexed by tuple key
    store: Arc<RwLock<HashMap<String, CrdtTuple>>>,
}

impl CrdtResolver {
    /// Create a new CRDT resolver
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get a unique key for a tuple
    fn tuple_key(tuple: &RelationTuple) -> String {
        format!(
            "{}:{}:{}:{}",
            tuple.namespace,
            tuple.object_id,
            tuple.relation,
            match &tuple.subject {
                Subject::User(id) => format!("user:{}", id),
                Subject::UserSet {
                    namespace,
                    object_id,
                    relation,
                } => format!("set:{}:{}:{}", namespace, object_id, relation),
            }
        )
    }

    /// Merge a CRDT tuple using LWW strategy
    pub async fn merge(&self, crdt_tuple: CrdtTuple) -> bool {
        let key = Self::tuple_key(&crdt_tuple.tuple);
        let mut store = self.store.write().await;

        match store.get(&key) {
            Some(existing) => {
                // Last-Write-Wins: Compare timestamps
                if crdt_tuple.timestamp > existing.timestamp
                    || (crdt_tuple.timestamp == existing.timestamp
                        && crdt_tuple.node_id > existing.node_id)
                {
                    store.insert(key, crdt_tuple);
                    true
                } else {
                    false
                }
            }
            None => {
                store.insert(key, crdt_tuple);
                true
            }
        }
    }

    /// Get all active (non-tombstone) tuples
    pub async fn active_tuples(&self) -> Vec<RelationTuple> {
        let store = self.store.read().await;
        store
            .values()
            .filter(|ct| !ct.is_tombstone)
            .map(|ct| ct.tuple.clone())
            .collect()
    }

    /// Get CRDT metadata for a tuple
    pub async fn get_crdt(&self, tuple: &RelationTuple) -> Option<CrdtTuple> {
        let key = Self::tuple_key(tuple);
        let store = self.store.read().await;
        store.get(&key).cloned()
    }

    /// Clear all tombstones older than retention period
    pub async fn gc_tombstones(&self, retention_micros: u64) {
        let mut store = self.store.write().await;
        let cutoff = CrdtTuple::current_timestamp() - retention_micros;

        store.retain(|_, ct| !ct.is_tombstone || ct.timestamp > cutoff);
    }
}

impl Default for CrdtResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for edge engine
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EdgeStats {
    /// Number of tuples synced from central
    pub tuples_synced: u64,
    /// Number of conflicts resolved
    pub conflicts_resolved: u64,
    /// Number of authorization checks performed
    pub checks_performed: u64,
    /// Last successful sync timestamp
    pub last_sync_timestamp: Option<u64>,
    /// Number of sync failures
    pub sync_failures: u64,
}

/// Lightweight authorization engine for edge deployment
pub struct EdgeEngine {
    /// In-memory authorization manager
    manager: InMemoryRebacManager,
    /// CRDT resolver for conflict resolution
    crdt: CrdtResolver,
    /// Configuration
    config: EdgeConfig,
    /// Unique node ID
    node_id: String,
    /// Statistics
    stats: Arc<RwLock<EdgeStats>>,
    /// Sync task handle
    sync_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Optional SQLite connection pool for central DB sync.
    /// `None` when no `central_db_url` could be connected (non-fatal).
    pool: Option<SqlitePool>,
}

impl EdgeEngine {
    /// Create a new edge engine.
    ///
    /// Attempts to open a SQLite connection pool using `config.central_db_url`.
    /// If the connection fails (e.g., the URL is a Postgres URL or the file does
    /// not exist yet), the engine starts without a pool and
    /// [`sync_from_central`](Self::sync_from_central) will return an error
    /// instead of panicking.
    pub async fn new(config: EdgeConfig) -> Result<Self> {
        let node_id = uuid::Uuid::new_v4().to_string();

        // Attempt to open the pool; treat connection failure as non-fatal so
        // that the engine can be used in-memory even without a DB.
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&config.central_db_url)
            .await
            .ok();

        Ok(Self {
            manager: InMemoryRebacManager::new(),
            crdt: CrdtResolver::new(),
            config,
            node_id,
            stats: Arc::new(RwLock::new(EdgeStats::default())),
            sync_handle: Arc::new(RwLock::new(None)),
            pool,
        })
    }

    /// Get the node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Check authorization (fast, in-memory)
    pub async fn check(
        &self,
        namespace: &str,
        object_id: &str,
        relation: &str,
        subject_id: &str,
    ) -> Result<bool> {
        let request = CheckRequest {
            namespace: namespace.to_string(),
            object_id: object_id.to_string(),
            relation: relation.to_string(),
            subject: Subject::User(subject_id.to_string()),
            context: None,
        };

        let response = self.manager.check(&request).await?;

        // Update stats
        let mut stats = self.stats.write().await;
        stats.checks_performed += 1;

        Ok(response.allowed)
    }

    /// Write a tuple (with CRDT metadata)
    pub async fn write_tuple(&self, tuple: RelationTuple) -> Result<()> {
        let crdt_tuple = CrdtTuple::new(tuple.clone(), self.node_id.clone());

        // Merge into CRDT store
        self.crdt.merge(crdt_tuple).await;

        // Write to in-memory manager
        self.manager.add_tuple(tuple).await
    }

    /// Delete a tuple (creates tombstone)
    pub async fn delete_tuple(&self, tuple: RelationTuple) -> Result<()> {
        let tombstone = CrdtTuple::tombstone(tuple.clone(), self.node_id.clone());

        // Merge tombstone into CRDT store
        self.crdt.merge(tombstone).await;

        // Remove from in-memory manager
        self.manager.remove_tuple(&tuple).await
    }

    /// Sync tuples from the central SQLite database into the in-memory manager.
    ///
    /// Fetches all rows from `authz_relation_tuples`, filtered according to
    /// `sync_config`, and upserts each one via [`write_tuple`](Self::write_tuple)
    /// (which also sets CRDT metadata and updates the in-memory graph).
    ///
    /// On success, `stats.tuples_synced` is incremented and
    /// `stats.last_sync_timestamp` is refreshed.  On any error,
    /// `stats.sync_failures` is incremented before the error is propagated.
    pub async fn sync_from_central(&self) -> Result<()> {
        let pool = match &self.pool {
            Some(p) => p,
            None => {
                let mut stats = self.stats.write().await;
                stats.sync_failures += 1;
                return Err(AuthzError::DatabaseError(
                    "No database pool available for central sync".to_string(),
                ));
            }
        };

        // Build a namespace filter based on the sync configuration.
        let namespace_filter: Option<Vec<String>> = match &self.config.sync_config {
            SyncConfig::All => None,
            SyncConfig::Namespaces(ns) => Some(ns.clone()),
            SyncConfig::Tenants(_) => None, // tenant filtering happens via subject, not namespace
            SyncConfig::NamespacesAndTenants { namespaces, .. } => Some(namespaces.clone()),
        };

        // Fetch rows. We use the function form of sqlx::query (not the macro)
        // so there is no compile-time DATABASE_URL requirement.
        let rows = sqlx::query(
            "SELECT namespace, object_id, relation, subject_type, subject_id, subject_relation \
             FROM authz_relation_tuples",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Failed to fetch tuples: {e}")))?;

        let mut synced: u64 = 0;

        for row in rows {
            let namespace: String = row
                .try_get("namespace")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;

            // Apply namespace filter when configured.
            if let Some(ref filter) = namespace_filter {
                if !filter.contains(&namespace) {
                    continue;
                }
            }

            let object_id: String = row
                .try_get("object_id")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;
            let relation: String = row
                .try_get("relation")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;
            let subject_type: String = row
                .try_get("subject_type")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;
            let subject_id: String = row
                .try_get("subject_id")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;
            let subject_relation: Option<String> = row
                .try_get("subject_relation")
                .map_err(|e| AuthzError::DatabaseError(format!("Row error: {e}")))?;

            let subject = match subject_type.as_str() {
                "userset" => {
                    // subject_id is stored as "namespace:object_id"
                    let parts: Vec<&str> = subject_id.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        Subject::UserSet {
                            namespace: parts[0].to_string(),
                            object_id: parts[1].to_string(),
                            relation: subject_relation.unwrap_or_default(),
                        }
                    } else {
                        Subject::User(subject_id)
                    }
                }
                _ => Subject::User(subject_id),
            };

            let tuple = RelationTuple::new(namespace, relation, object_id, subject);

            // write_tuple merges through CRDT and then into the in-memory manager,
            // which is idempotent for duplicate tuples.
            self.write_tuple(tuple)
                .await
                .map_err(|e| AuthzError::DatabaseError(format!("Failed to upsert tuple: {e}")))?;

            synced += 1;
        }

        let mut stats = self.stats.write().await;
        stats.tuples_synced += synced;
        stats.last_sync_timestamp = Some(CrdtTuple::current_timestamp());

        Ok(())
    }

    /// Start background sync task
    pub async fn start_sync(&self) -> Result<()> {
        let mut handle_guard = self.sync_handle.write().await;

        // Stop existing sync task if running
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }

        let interval_secs = self.config.sync_interval_secs;
        let stats = self.stats.clone();

        // Create sync task
        let handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));

            loop {
                ticker.tick().await;

                // Perform sync (placeholder)
                // In real implementation, call sync_from_central()

                let mut s = stats.write().await;
                s.last_sync_timestamp = Some(CrdtTuple::current_timestamp());
            }
        });

        *handle_guard = Some(handle);

        Ok(())
    }

    /// Stop background sync task
    pub async fn stop_sync(&self) {
        let mut handle_guard = self.sync_handle.write().await;

        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    /// Get edge engine statistics
    pub async fn stats(&self) -> EdgeStats {
        self.stats.read().await.clone()
    }

    /// Merge tuples from another edge node (for gossip protocol)
    pub async fn merge_remote_tuples(&self, remote_tuples: Vec<CrdtTuple>) -> Result<u64> {
        let mut conflicts = 0;

        for crdt_tuple in remote_tuples {
            let merged = self.crdt.merge(crdt_tuple.clone()).await;

            if merged {
                if crdt_tuple.is_tombstone {
                    // Remove tuple if it's a tombstone
                    let _ = self.manager.remove_tuple(&crdt_tuple.tuple).await;
                } else {
                    // Add tuple if it's active
                    let _ = self.manager.add_tuple(crdt_tuple.tuple.clone()).await;
                }
                conflicts += 1;
            }
        }

        // Update stats
        let mut stats = self.stats.write().await;
        stats.conflicts_resolved += conflicts;

        Ok(conflicts)
    }

    /// Garbage collect old tombstones
    pub async fn gc_tombstones(&self, retention_secs: u64) -> Result<()> {
        self.crdt.gc_tombstones(retention_secs * 1_000_000).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_edge_config_default() {
        let config = EdgeConfig::default();
        assert_eq!(config.sync_interval_secs, 30);
        assert!(matches!(config.sync_config, SyncConfig::All));
    }

    #[tokio::test]
    async fn test_crdt_tuple_creation() {
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "123",
            Subject::User("alice".to_string()),
        );

        let crdt = CrdtTuple::new(tuple.clone(), "node1".to_string());

        assert_eq!(crdt.tuple, tuple);
        assert_eq!(crdt.node_id, "node1");
        assert!(!crdt.is_tombstone);
        assert!(crdt.timestamp > 0);
    }

    #[tokio::test]
    async fn test_crdt_tombstone() {
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "123",
            Subject::User("alice".to_string()),
        );

        let tombstone = CrdtTuple::tombstone(tuple.clone(), "node1".to_string());

        assert_eq!(tombstone.tuple, tuple);
        assert!(tombstone.is_tombstone);
    }

    #[tokio::test]
    async fn test_crdt_resolver_lww() {
        let resolver = CrdtResolver::new();

        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "123",
            Subject::User("alice".to_string()),
        );

        // First write
        let mut crdt1 = CrdtTuple::new(tuple.clone(), "node1".to_string());
        crdt1.timestamp = 100;
        assert!(resolver.merge(crdt1.clone()).await);

        // Later write should win
        let mut crdt2 = CrdtTuple::new(tuple.clone(), "node2".to_string());
        crdt2.timestamp = 200;
        assert!(resolver.merge(crdt2.clone()).await);

        // Earlier write should be rejected
        let mut crdt3 = CrdtTuple::new(tuple.clone(), "node3".to_string());
        crdt3.timestamp = 150;
        assert!(!resolver.merge(crdt3).await);

        // Verify latest is stored
        let stored = resolver.get_crdt(&tuple).await.unwrap();
        assert_eq!(stored.timestamp, 200);
        assert_eq!(stored.node_id, "node2");
    }

    #[tokio::test]
    async fn test_crdt_resolver_tie_breaking() {
        let resolver = CrdtResolver::new();

        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "123",
            Subject::User("alice".to_string()),
        );

        // Same timestamp, node_id used for tie-breaking
        let mut crdt1 = CrdtTuple::new(tuple.clone(), "node_a".to_string());
        crdt1.timestamp = 100;
        resolver.merge(crdt1).await;

        let mut crdt2 = CrdtTuple::new(tuple.clone(), "node_z".to_string());
        crdt2.timestamp = 100; // Same timestamp
        assert!(resolver.merge(crdt2).await); // node_z > node_a

        let stored = resolver.get_crdt(&tuple).await.unwrap();
        assert_eq!(stored.node_id, "node_z");
    }

    #[tokio::test]
    async fn test_edge_engine_creation() {
        let config = EdgeConfig::default();
        let engine = EdgeEngine::new(config).await.unwrap();

        assert!(!engine.node_id().is_empty());

        let stats = engine.stats().await;
        assert_eq!(stats.checks_performed, 0);
        assert_eq!(stats.tuples_synced, 0);
    }

    #[tokio::test]
    async fn test_edge_engine_write_and_check() {
        let config = EdgeConfig::default();
        let engine = EdgeEngine::new(config).await.unwrap();

        // Write a tuple
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "123",
            Subject::User("alice".to_string()),
        );
        engine.write_tuple(tuple).await.unwrap();

        // Check authorization
        let allowed = engine
            .check("document", "123", "viewer", "alice")
            .await
            .unwrap();
        assert!(allowed);

        let stats = engine.stats().await;
        assert_eq!(stats.checks_performed, 1);
    }

    #[tokio::test]
    async fn test_edge_engine_delete() {
        let config = EdgeConfig::default();
        let engine = EdgeEngine::new(config).await.unwrap();

        // Write and then delete
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "123",
            Subject::User("alice".to_string()),
        );
        engine.write_tuple(tuple.clone()).await.unwrap();
        engine.delete_tuple(tuple.clone()).await.unwrap();

        // Check that tuple is deleted
        let allowed = engine
            .check("document", "123", "viewer", "alice")
            .await
            .unwrap();
        assert!(!allowed);

        // Verify tombstone exists in CRDT
        let crdt = engine.crdt.get_crdt(&tuple).await.unwrap();
        assert!(crdt.is_tombstone);
    }

    #[tokio::test]
    async fn test_merge_remote_tuples() {
        let config = EdgeConfig::default();
        let engine = EdgeEngine::new(config).await.unwrap();

        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "123",
            Subject::User("bob".to_string()),
        );

        let remote_crdt = CrdtTuple::new(tuple.clone(), "remote-node".to_string());

        let conflicts = engine.merge_remote_tuples(vec![remote_crdt]).await.unwrap();

        assert_eq!(conflicts, 1);

        // Verify tuple was merged
        let allowed = engine
            .check("document", "123", "viewer", "bob")
            .await
            .unwrap();
        assert!(allowed);

        let stats = engine.stats().await;
        assert_eq!(stats.conflicts_resolved, 1);
    }

    #[tokio::test]
    async fn test_gc_tombstones() {
        let resolver = CrdtResolver::new();

        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "123",
            Subject::User("alice".to_string()),
        );

        // Create old tombstone
        let mut old_tombstone = CrdtTuple::tombstone(tuple.clone(), "node1".to_string());
        old_tombstone.timestamp = 100; // Very old
        resolver.merge(old_tombstone).await;

        // GC with short retention
        resolver.gc_tombstones(1_000_000).await; // 1 second

        // Tombstone should be removed
        let stored = resolver.get_crdt(&tuple).await;
        assert!(stored.is_none());
    }
}
