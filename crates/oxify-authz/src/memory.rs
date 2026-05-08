//! In-memory ReBAC manager
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY
//! License: MIT OR Apache-2.0 (compatible with OxiRS)

use crate::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory ReBAC manager
/// Ported from OxiRS for high-performance hot-path authorization
pub struct InMemoryRebacManager {
    /// Relationship tuples indexed by subject
    tuples_by_subject: Arc<RwLock<HashMap<String, Vec<RelationTuple>>>>,

    /// Relationship tuples indexed by object
    tuples_by_object: Arc<RwLock<HashMap<String, Vec<RelationTuple>>>>,

    /// Relationship graph for path-based checks
    graph: Arc<RwLock<RelationshipGraph>>,
}

/// Relationship graph for traversal-based authorization
#[derive(Debug, Default)]
struct RelationshipGraph {
    /// Edges in the graph: (subject, relation) -> Vec<object>
    edges: HashMap<(String, String), Vec<String>>,
}

impl RelationshipGraph {
    fn add_edge(&mut self, subject: String, relation: String, object: String) {
        self.edges
            .entry((subject, relation))
            .or_default()
            .push(object);
    }

    fn remove_edge(&mut self, subject: &str, relation: &str, object: &str) {
        if let Some(objects) = self
            .edges
            .get_mut(&(subject.to_string(), relation.to_string()))
        {
            objects.retain(|o| o != object);
        }
    }

    /// Check if there's a path from subject to object via the given relation.
    ///
    /// Performs BFS over the edge map to find transitive reachability.
    /// Cycles are handled via a `visited` set; the traversal is bounded by O(V+E).
    ///
    /// Example: given edges `user:alice --member--> group:eng` and
    /// `group:eng --member--> namespace:docs`, `has_path("user:alice", "member", "namespace:docs")`
    /// returns `true` even though there is no direct edge.
    fn has_path(&self, subject: &str, relation: &str, object: &str) -> bool {
        use std::collections::{HashSet, VecDeque};

        let target = object.to_string();
        let rel = relation.to_string();

        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        queue.push_back(subject.to_string());
        visited.insert(subject.to_string());

        while let Some(current) = queue.pop_front() {
            if let Some(neighbors) = self.edges.get(&(current.clone(), rel.clone())) {
                for neighbor in neighbors {
                    if *neighbor == target {
                        return true;
                    }
                    if visited.insert(neighbor.clone()) {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        false
    }
}

impl InMemoryRebacManager {
    /// Create a new in-memory ReBAC manager
    pub fn new() -> Self {
        Self {
            tuples_by_subject: Arc::new(RwLock::new(HashMap::new())),
            tuples_by_object: Arc::new(RwLock::new(HashMap::new())),
            graph: Arc::new(RwLock::new(RelationshipGraph::default())),
        }
    }

    /// Initialize with predefined tuples (for testing/demo)
    pub async fn with_tuples(tuples: Vec<RelationTuple>) -> Result<Self> {
        let manager = Self::new();
        for tuple in tuples {
            manager.add_tuple(tuple).await?;
        }
        Ok(manager)
    }

    /// Add a relationship tuple
    pub async fn add_tuple(&self, tuple: RelationTuple) -> Result<()> {
        let subject_key = tuple.subject.to_string();
        let object_key = format!("{}:{}", tuple.namespace, tuple.object_id);

        // Add to subject index (check for duplicates)
        {
            let mut tuples_by_subject = self.tuples_by_subject.write().await;
            let subject_tuples = tuples_by_subject
                .entry(subject_key.clone())
                .or_insert_with(Vec::new);

            // Only add if not already present (compare by namespace, object_id, relation, subject)
            if !subject_tuples.iter().any(|t| {
                t.namespace == tuple.namespace
                    && t.object_id == tuple.object_id
                    && t.relation == tuple.relation
                    && t.subject == tuple.subject
            }) {
                subject_tuples.push(tuple.clone());
            }
        }

        // Add to object index (check for duplicates)
        {
            let mut tuples_by_object = self.tuples_by_object.write().await;
            let object_tuples = tuples_by_object
                .entry(object_key.clone())
                .or_insert_with(Vec::new);

            // Only add if not already present
            if !object_tuples.iter().any(|t| {
                t.namespace == tuple.namespace
                    && t.object_id == tuple.object_id
                    && t.relation == tuple.relation
                    && t.subject == tuple.subject
            }) {
                object_tuples.push(tuple.clone());
            }
        }

        // Add to graph
        {
            let mut graph = self.graph.write().await;
            graph.add_edge(subject_key, tuple.relation.clone(), object_key);
        }

        Ok(())
    }

    /// Remove a relationship tuple
    pub async fn remove_tuple(&self, tuple: &RelationTuple) -> Result<()> {
        let subject_key = tuple.subject.to_string();
        let object_key = format!("{}:{}", tuple.namespace, tuple.object_id);

        // Remove from subject index
        {
            let mut tuples_by_subject = self.tuples_by_subject.write().await;
            if let Some(tuples) = tuples_by_subject.get_mut(&subject_key) {
                tuples.retain(|t| {
                    !(t.namespace == tuple.namespace
                        && t.object_id == tuple.object_id
                        && t.relation == tuple.relation
                        && t.subject == tuple.subject)
                });
            }
        }

        // Remove from object index
        {
            let mut tuples_by_object = self.tuples_by_object.write().await;
            if let Some(tuples) = tuples_by_object.get_mut(&object_key) {
                tuples.retain(|t| {
                    !(t.namespace == tuple.namespace
                        && t.object_id == tuple.object_id
                        && t.relation == tuple.relation
                        && t.subject == tuple.subject)
                });
            }
        }

        // Remove from graph
        {
            let mut graph = self.graph.write().await;
            graph.remove_edge(&subject_key, &tuple.relation, &object_key);
        }

        Ok(())
    }

    /// Check if a subject has a relation to an object
    pub async fn check(&self, request: &CheckRequest) -> Result<CheckResponse> {
        let subject_key = request.subject.to_string();
        let object_key = format!("{}:{}", request.namespace, request.object_id);

        // Check if relationship exists in graph
        let graph = self.graph.read().await;
        let has_relation = graph.has_path(&subject_key, &request.relation, &object_key);

        if !has_relation {
            return Ok(CheckResponse {
                allowed: false,
                cached: false,
            });
        }

        // Check conditions
        let tuples_by_subject = self.tuples_by_subject.read().await;
        if let Some(tuples) = tuples_by_subject.get(&subject_key) {
            for tuple in tuples {
                if tuple.namespace == request.namespace
                    && tuple.object_id == request.object_id
                    && tuple.relation == request.relation
                    && !tuple.is_condition_satisfied()
                {
                    return Ok(CheckResponse {
                        allowed: false,
                        cached: false,
                    });
                }
            }
        }

        Ok(CheckResponse {
            allowed: true,
            cached: false,
        })
    }

    /// List all tuples for a subject
    pub async fn list_subject_tuples(&self, subject: &Subject) -> Result<Vec<RelationTuple>> {
        let subject_key = subject.to_string();
        let tuples_by_subject = self.tuples_by_subject.read().await;
        Ok(tuples_by_subject
            .get(&subject_key)
            .cloned()
            .unwrap_or_default())
    }

    /// List all tuples for an object
    pub async fn list_object_tuples(
        &self,
        namespace: &str,
        object_id: &str,
    ) -> Result<Vec<RelationTuple>> {
        let object_key = format!("{}:{}", namespace, object_id);
        let tuples_by_object = self.tuples_by_object.read().await;
        Ok(tuples_by_object
            .get(&object_key)
            .cloned()
            .unwrap_or_default())
    }

    /// Batch check multiple requests
    pub async fn batch_check(&self, requests: &[CheckRequest]) -> Result<Vec<CheckResponse>> {
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            results.push(self.check(request).await?);
        }
        Ok(results)
    }
}

impl Default for InMemoryRebacManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_relationship() {
        let manager = InMemoryRebacManager::new();

        // Add relationship: alice can read document:public
        let tuple = RelationTuple::new(
            "document",
            "can_read",
            "public",
            Subject::User("alice".to_string()),
        );
        manager.add_tuple(tuple).await.unwrap();

        // Check if alice can read document:public
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "public".to_string(),
            relation: "can_read".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        let response = manager.check(&request).await.unwrap();
        assert!(response.allowed);

        // Check if alice can write document:public (should fail)
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "public".to_string(),
            relation: "can_write".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        let response = manager.check(&request).await.unwrap();
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn test_list_tuples() {
        let manager = InMemoryRebacManager::new();

        // Add multiple relationships for alice
        manager
            .add_tuple(RelationTuple::new(
                "document",
                "can_read",
                "public",
                Subject::User("alice".to_string()),
            ))
            .await
            .unwrap();
        manager
            .add_tuple(RelationTuple::new(
                "document",
                "can_write",
                "private",
                Subject::User("alice".to_string()),
            ))
            .await
            .unwrap();

        // List all tuples for alice
        let tuples = manager
            .list_subject_tuples(&Subject::User("alice".to_string()))
            .await
            .unwrap();
        assert_eq!(tuples.len(), 2);
    }

    #[tokio::test]
    async fn test_time_based_condition() {
        let manager = InMemoryRebacManager::new();

        // Add relationship with time window (already expired)
        let tuple = RelationTuple::with_condition(
            "document",
            "temporary",
            "can_read",
            Subject::User("alice".to_string()),
            RelationshipCondition::TimeWindow {
                not_before: Some(chrono::Utc::now() - chrono::Duration::hours(2)),
                not_after: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
            },
        );
        manager.add_tuple(tuple).await.unwrap();

        // Check should fail due to expired time window
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "temporary".to_string(),
            relation: "can_read".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        let response = manager.check(&request).await.unwrap();
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn test_remove_tuple() {
        let manager = InMemoryRebacManager::new();

        // Add relationship
        let tuple = RelationTuple::new(
            "document",
            "can_read",
            "public",
            Subject::User("alice".to_string()),
        );
        manager.add_tuple(tuple.clone()).await.unwrap();

        // Verify it exists
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "public".to_string(),
            relation: "can_read".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        let response = manager.check(&request).await.unwrap();
        assert!(response.allowed);

        // Remove relationship
        manager.remove_tuple(&tuple).await.unwrap();

        // Verify it's gone
        let response = manager.check(&request).await.unwrap();
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn test_batch_check() {
        let manager = InMemoryRebacManager::new();

        // Add multiple relationships
        manager
            .add_tuple(RelationTuple::new(
                "document",
                "can_read",
                "doc1",
                Subject::User("alice".to_string()),
            ))
            .await
            .unwrap();
        manager
            .add_tuple(RelationTuple::new(
                "document",
                "can_read",
                "doc2",
                Subject::User("alice".to_string()),
            ))
            .await
            .unwrap();

        // Batch check
        let requests = vec![
            CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc1".to_string(),
                relation: "can_read".to_string(),
                subject: Subject::User("alice".to_string()),
                context: None,
            },
            CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc2".to_string(),
                relation: "can_read".to_string(),
                subject: Subject::User("alice".to_string()),
                context: None,
            },
            CheckRequest {
                namespace: "document".to_string(),
                object_id: "doc3".to_string(),
                relation: "can_read".to_string(),
                subject: Subject::User("alice".to_string()),
                context: None,
            },
        ];

        let responses = manager.batch_check(&requests).await.unwrap();
        assert_eq!(responses.len(), 3);
        assert!(responses[0].allowed);
        assert!(responses[1].allowed);
        assert!(!responses[2].allowed);
    }
}
