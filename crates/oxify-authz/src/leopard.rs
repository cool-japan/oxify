//! Leopard Indexing for optimized reachability queries
//!
//! Implements the Leopard indexing strategy from Google Zanzibar to achieve
//! O(1) authorization checks for pre-computed relationship paths.
//!
//! ## How it works
//!
//! Instead of traversing the relationship graph at query time, Leopard pre-computes
//! and materializes transitive closures for frequently accessed relationships.
//!
//! Example: If `owner` inherits `viewer`, and `alice` is an `owner` of `doc1`,
//! the index stores both:
//! - `(alice, owner, doc1)` - direct relationship
//! - `(alice, viewer, doc1)` - computed/inherited relationship
//!
//! ## Performance
//!
//! - Reads: O(1) lookup in materialized index
//! - Writes: O(n) where n is the depth of inheritance chain
//! - Space: O(tuples × avg_inheritance_depth)

use crate::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Type alias for subject index: subject -> set of (namespace, object_id, relation)
type SubjectIndex = Arc<RwLock<HashMap<String, HashSet<(String, String, String)>>>>;

/// Type alias for object index: (namespace, object_id, relation) -> set of subjects
type ObjectIndex = Arc<RwLock<HashMap<(String, String, String), HashSet<String>>>>;

/// Type alias for direct tuples: set of (namespace, object_id, relation, subject)
type DirectTuples = Arc<RwLock<HashSet<(String, String, String, String)>>>;

/// Configuration for Leopard indexing
#[derive(Debug, Clone)]
pub struct LeopardConfig {
    /// Maximum depth for transitive closure computation
    pub max_depth: usize,
    /// Whether to eagerly compute all transitive closures on startup
    pub eager_compute: bool,
    /// Namespaces to index (empty = all)
    pub indexed_namespaces: Vec<String>,
}

impl Default for LeopardConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            eager_compute: false,
            indexed_namespaces: Vec::new(),
        }
    }
}

/// A materialized reachability entry
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReachabilityEntry {
    /// The subject that has access
    pub subject: String,
    /// The namespace of the object
    pub namespace: String,
    /// The object ID
    pub object_id: String,
    /// The relation (may be computed/inherited)
    pub relation: String,
    /// Depth in the inheritance chain (0 = direct)
    pub depth: usize,
    /// Whether this is a direct or computed relationship
    pub is_computed: bool,
}

/// Statistics for Leopard index
#[derive(Debug, Clone, Default)]
pub struct LeopardStats {
    /// Number of direct entries
    pub direct_entries: u64,
    /// Number of computed entries
    pub computed_entries: u64,
    /// Number of index hits
    pub hits: u64,
    /// Number of index misses
    pub misses: u64,
}

impl LeopardStats {
    /// Calculate hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Total entries in index
    pub fn total_entries(&self) -> u64 {
        self.direct_entries + self.computed_entries
    }
}

/// Leopard reachability index
pub struct LeopardIndex {
    /// Primary index: subject -> set of (namespace, object_id, relation)
    by_subject: SubjectIndex,

    /// Reverse index: (namespace, object_id, relation) -> set of subjects
    by_object: ObjectIndex,

    /// Direct tuples for tracking what needs recomputation
    direct_tuples: DirectTuples,

    /// Namespace configurations for inheritance rules
    namespace_configs: Arc<HashMap<String, NamespaceConfig>>,

    /// Configuration (reserved for future use)
    #[allow(dead_code)]
    config: LeopardConfig,

    /// Statistics
    stats: Arc<RwLock<LeopardStats>>,
}

impl LeopardIndex {
    /// Create a new Leopard index
    pub fn new(namespace_configs: Arc<HashMap<String, NamespaceConfig>>) -> Self {
        Self::with_config(namespace_configs, LeopardConfig::default())
    }

    /// Create a new Leopard index with custom configuration
    pub fn with_config(
        namespace_configs: Arc<HashMap<String, NamespaceConfig>>,
        config: LeopardConfig,
    ) -> Self {
        Self {
            by_subject: Arc::new(RwLock::new(HashMap::new())),
            by_object: Arc::new(RwLock::new(HashMap::new())),
            direct_tuples: Arc::new(RwLock::new(HashSet::new())),
            namespace_configs,
            config,
            stats: Arc::new(RwLock::new(LeopardStats::default())),
        }
    }

    /// Index a new tuple and compute transitive closures
    pub async fn index_tuple(&self, tuple: &RelationTuple) -> Result<usize> {
        let subject_key = tuple.subject.to_string();

        // Track direct tuple
        {
            let mut direct = self.direct_tuples.write().await;
            direct.insert((
                subject_key.clone(),
                tuple.namespace.clone(),
                tuple.object_id.clone(),
                tuple.relation.clone(),
            ));
        }

        // Add direct entry
        self.add_entry(
            &subject_key,
            &tuple.namespace,
            &tuple.object_id,
            &tuple.relation,
            false,
        )
        .await;

        // Compute inherited relations
        let mut computed_count = 0;
        if let Some(ns_config) = self.namespace_configs.get(&tuple.namespace) {
            // Find all relations that inherit from this relation
            for rel_config in &ns_config.relations {
                if rel_config.inherits_from.contains(&tuple.relation) {
                    // This relation inherits from the tuple's relation
                    // So the subject also has this relation
                    self.add_entry(
                        &subject_key,
                        &tuple.namespace,
                        &tuple.object_id,
                        &rel_config.name,
                        true,
                    )
                    .await;
                    computed_count += 1;
                }
            }
        }

        Ok(computed_count + 1) // direct + computed
    }

    /// Remove a tuple and its computed entries from the index
    pub async fn remove_tuple(&self, tuple: &RelationTuple) -> Result<usize> {
        let subject_key = tuple.subject.to_string();

        // Remove from direct tuples
        {
            let mut direct = self.direct_tuples.write().await;
            direct.remove(&(
                subject_key.clone(),
                tuple.namespace.clone(),
                tuple.object_id.clone(),
                tuple.relation.clone(),
            ));
        }

        // Remove direct entry
        self.remove_entry(
            &subject_key,
            &tuple.namespace,
            &tuple.object_id,
            &tuple.relation,
        )
        .await;

        // Remove inherited relations
        let mut removed_count = 1;
        if let Some(ns_config) = self.namespace_configs.get(&tuple.namespace) {
            for rel_config in &ns_config.relations {
                if rel_config.inherits_from.contains(&tuple.relation) {
                    // Check if subject still has this inherited relation through another path
                    let still_has = self
                        .check_has_through_other_path(
                            &subject_key,
                            &tuple.namespace,
                            &tuple.object_id,
                            &rel_config.name,
                            &tuple.relation,
                        )
                        .await;

                    if !still_has {
                        self.remove_entry(
                            &subject_key,
                            &tuple.namespace,
                            &tuple.object_id,
                            &rel_config.name,
                        )
                        .await;
                        removed_count += 1;
                    }
                }
            }
        }

        Ok(removed_count)
    }

    /// Check if a subject has a relation (O(1) lookup)
    pub async fn check(&self, request: &CheckRequest) -> Option<bool> {
        let subject_key = request.subject.to_string();
        let key = (
            request.namespace.clone(),
            request.object_id.clone(),
            request.relation.clone(),
        );

        let by_subject = self.by_subject.read().await;
        if let Some(entries) = by_subject.get(&subject_key) {
            let result = entries.contains(&key);

            // Update stats
            let mut stats = self.stats.write().await;
            if result {
                stats.hits += 1;
            } else {
                stats.misses += 1;
            }

            Some(result)
        } else {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
            Some(false)
        }
    }

    /// Get all subjects with a specific relation to an object
    pub async fn expand(&self, namespace: &str, object_id: &str, relation: &str) -> Vec<String> {
        let key = (
            namespace.to_string(),
            object_id.to_string(),
            relation.to_string(),
        );

        let by_object = self.by_object.read().await;
        by_object
            .get(&key)
            .map(|subjects| subjects.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all relations a subject has to objects
    pub async fn list_subject_access(&self, subject: &Subject) -> Vec<(String, String, String)> {
        let subject_key = subject.to_string();

        let by_subject = self.by_subject.read().await;
        by_subject
            .get(&subject_key)
            .map(|entries| entries.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get statistics
    pub async fn stats(&self) -> LeopardStats {
        self.stats.read().await.clone()
    }

    /// Clear the index
    pub async fn clear(&self) {
        self.by_subject.write().await.clear();
        self.by_object.write().await.clear();
        self.direct_tuples.write().await.clear();
        *self.stats.write().await = LeopardStats::default();
    }

    /// Bulk load tuples into the index
    pub async fn bulk_load(&self, tuples: &[RelationTuple]) -> Result<usize> {
        let mut total = 0;
        for tuple in tuples {
            total += self.index_tuple(tuple).await?;
        }
        Ok(total)
    }

    // Internal helper methods

    async fn add_entry(
        &self,
        subject: &str,
        namespace: &str,
        object_id: &str,
        relation: &str,
        is_computed: bool,
    ) {
        let key = (
            namespace.to_string(),
            object_id.to_string(),
            relation.to_string(),
        );

        // Add to subject index
        {
            let mut by_subject = self.by_subject.write().await;
            by_subject
                .entry(subject.to_string())
                .or_default()
                .insert(key.clone());
        }

        // Add to object index
        {
            let mut by_object = self.by_object.write().await;
            by_object
                .entry(key)
                .or_default()
                .insert(subject.to_string());
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            if is_computed {
                stats.computed_entries += 1;
            } else {
                stats.direct_entries += 1;
            }
        }
    }

    async fn remove_entry(&self, subject: &str, namespace: &str, object_id: &str, relation: &str) {
        let key = (
            namespace.to_string(),
            object_id.to_string(),
            relation.to_string(),
        );

        // Remove from subject index
        {
            let mut by_subject = self.by_subject.write().await;
            if let Some(entries) = by_subject.get_mut(subject) {
                entries.remove(&key);
                if entries.is_empty() {
                    by_subject.remove(subject);
                }
            }
        }

        // Remove from object index
        {
            let mut by_object = self.by_object.write().await;
            if let Some(subjects) = by_object.get_mut(&key) {
                subjects.remove(subject);
                if subjects.is_empty() {
                    by_object.remove(&key);
                }
            }
        }
    }

    async fn check_has_through_other_path(
        &self,
        subject: &str,
        namespace: &str,
        object_id: &str,
        relation: &str,
        excluded_source: &str,
    ) -> bool {
        // Check if the subject still has the relation through another inheritance path
        if let Some(ns_config) = self.namespace_configs.get(namespace) {
            if let Some(rel_config) = ns_config.relations.iter().find(|r| r.name == relation) {
                let direct = self.direct_tuples.read().await;

                // Check each possible source relation (except the excluded one)
                for source_rel in &rel_config.inherits_from {
                    if source_rel != excluded_source
                        && direct.contains(&(
                            subject.to_string(),
                            namespace.to_string(),
                            object_id.to_string(),
                            source_rel.clone(),
                        ))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_namespace_configs() -> Arc<HashMap<String, NamespaceConfig>> {
        let mut configs = HashMap::new();
        configs.insert(
            "document".to_string(),
            NamespaceConfig::document_namespace(),
        );
        configs.insert("folder".to_string(), NamespaceConfig::folder_namespace());
        Arc::new(configs)
    }

    #[tokio::test]
    async fn test_leopard_direct_lookup() {
        let configs = test_namespace_configs();
        let index = LeopardIndex::new(configs);

        // Add a direct relationship
        let tuple = RelationTuple::new(
            "document",
            "owner",
            "doc123",
            Subject::User("alice".to_string()),
        );
        index.index_tuple(&tuple).await.unwrap();

        // Check direct relationship
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "owner".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        assert_eq!(index.check(&request).await, Some(true));

        // Check non-existent relationship
        let request2 = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "owner".to_string(),
            subject: Subject::User("bob".to_string()),
            context: None,
        };
        assert_eq!(index.check(&request2).await, Some(false));
    }

    #[tokio::test]
    async fn test_leopard_inherited_relations() {
        let configs = test_namespace_configs();
        let index = LeopardIndex::new(configs);

        // Add owner relationship
        let tuple = RelationTuple::new(
            "document",
            "owner",
            "doc123",
            Subject::User("alice".to_string()),
        );
        let count = index.index_tuple(&tuple).await.unwrap();

        // Should have indexed direct + inherited (viewer inherits from owner)
        assert!(count >= 2);

        // Check inherited viewer relationship
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "viewer".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        assert_eq!(index.check(&request).await, Some(true));

        // Check inherited editor relationship
        let request2 = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "editor".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        assert_eq!(index.check(&request2).await, Some(true));
    }

    #[tokio::test]
    async fn test_leopard_remove_tuple() {
        let configs = test_namespace_configs();
        let index = LeopardIndex::new(configs);

        // Add and then remove
        let tuple = RelationTuple::new(
            "document",
            "owner",
            "doc123",
            Subject::User("alice".to_string()),
        );
        index.index_tuple(&tuple).await.unwrap();
        index.remove_tuple(&tuple).await.unwrap();

        // Should no longer have access
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "owner".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        assert_eq!(index.check(&request).await, Some(false));

        // Inherited relations should also be gone
        let request2 = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "viewer".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        assert_eq!(index.check(&request2).await, Some(false));
    }

    #[tokio::test]
    async fn test_leopard_expand() {
        let configs = test_namespace_configs();
        let index = LeopardIndex::new(configs);

        // Add multiple users as viewers
        for user in ["alice", "bob", "charlie"] {
            let tuple = RelationTuple::new(
                "document",
                "viewer",
                "doc123",
                Subject::User(user.to_string()),
            );
            index.index_tuple(&tuple).await.unwrap();
        }

        // Expand should return all viewers
        let viewers = index.expand("document", "doc123", "viewer").await;
        assert_eq!(viewers.len(), 3);
        assert!(viewers.contains(&"user:alice".to_string()));
        assert!(viewers.contains(&"user:bob".to_string()));
        assert!(viewers.contains(&"user:charlie".to_string()));
    }

    #[tokio::test]
    async fn test_leopard_stats() {
        let configs = test_namespace_configs();
        let index = LeopardIndex::new(configs);

        // Add some tuples
        let tuple = RelationTuple::new(
            "document",
            "owner",
            "doc123",
            Subject::User("alice".to_string()),
        );
        index.index_tuple(&tuple).await.unwrap();

        let stats = index.stats().await;
        assert!(stats.direct_entries > 0);
        assert!(stats.computed_entries > 0); // Due to inheritance

        // Do some checks
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "owner".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        index.check(&request).await;
        index.check(&request).await;

        let stats = index.stats().await;
        assert_eq!(stats.hits, 2);
    }

    #[tokio::test]
    async fn test_leopard_bulk_load() {
        let configs = test_namespace_configs();
        let index = LeopardIndex::new(configs);

        let tuples: Vec<RelationTuple> = (0..100)
            .map(|i| {
                RelationTuple::new(
                    "document",
                    "viewer",
                    format!("doc{}", i),
                    Subject::User(format!("user{}", i % 10)),
                )
            })
            .collect();

        let count = index.bulk_load(&tuples).await.unwrap();
        assert_eq!(count, 100); // No inheritance for viewer

        // Verify some entries
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc50".to_string(),
            relation: "viewer".to_string(),
            subject: Subject::User("user0".to_string()),
            context: None,
        };
        assert_eq!(index.check(&request).await, Some(true));
    }
}
