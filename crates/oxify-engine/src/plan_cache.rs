//! Execution plan caching for workflow optimization
//!
//! This module caches the topological sort results for workflows to avoid
//! recomputing the execution order for repeated workflow executions.

use oxify_model::{NodeId, WorkflowId};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

/// A cached execution plan
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// The execution levels (nodes at each level can run in parallel)
    pub levels: Vec<Vec<NodeId>>,
    /// Hash of the workflow structure
    pub workflow_hash: u64,
}

/// Cache for execution plans
pub struct PlanCache {
    cache: Arc<Mutex<HashMap<WorkflowId, ExecutionPlan>>>,
    max_size: usize,
}

impl PlanCache {
    /// Create a new plan cache
    pub fn new() -> Self {
        Self::with_capacity(100)
    }

    /// Create a new plan cache with specified capacity
    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            max_size,
        }
    }

    /// Get a cached execution plan
    pub fn get(&self, workflow_id: &WorkflowId, workflow_hash: u64) -> Option<ExecutionPlan> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.get(workflow_id).and_then(|plan| {
            // Verify hash matches (workflow structure hasn't changed)
            if plan.workflow_hash == workflow_hash {
                Some(plan.clone())
            } else {
                None
            }
        })
    }

    /// Store an execution plan
    pub fn put(&self, workflow_id: WorkflowId, plan: ExecutionPlan) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());

        // Evict random entry if at capacity (simple LRU-like behavior)
        if cache.len() >= self.max_size && !cache.contains_key(&workflow_id) {
            if let Some(key) = cache.keys().next().cloned() {
                cache.remove(&key);
            }
        }

        cache.insert(workflow_id, plan);
    }

    /// Clear the cache
    #[allow(dead_code)]
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.clear();
    }

    /// Get cache statistics
    #[allow(dead_code)]
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        CacheStats {
            size: cache.len(),
            capacity: self.max_size,
        }
    }
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CacheStats {
    pub size: usize,
    pub capacity: usize,
}

/// Compute a hash of the workflow structure
pub fn hash_workflow_structure(nodes: &[oxify_model::Node], edges: &[oxify_model::Edge]) -> u64 {
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();

    // Hash nodes (ID and kind)
    let mut node_vec: Vec<_> = nodes.iter().collect();
    node_vec.sort_by_key(|n| n.id);

    for node in node_vec {
        node.id.hash(&mut hasher);
        // Hash the node kind discriminant (not the full config)
        std::mem::discriminant(&node.kind).hash(&mut hasher);
    }

    // Hash edges
    let mut edge_vec = edges.to_vec();
    edge_vec.sort_by_key(|e| (e.from, e.to));

    for edge in edge_vec {
        edge.from.hash(&mut hasher);
        edge.to.hash(&mut hasher);
    }

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, Node, NodeKind};
    use uuid::Uuid;

    #[test]
    fn test_plan_cache() {
        let cache = PlanCache::new();
        let workflow_id = Uuid::new_v4();
        let workflow_hash = 12345;

        let plan = ExecutionPlan {
            levels: vec![vec![Uuid::new_v4()], vec![Uuid::new_v4()]],
            workflow_hash,
        };

        // Cache miss
        assert!(cache.get(&workflow_id, workflow_hash).is_none());

        // Store plan
        cache.put(workflow_id, plan.clone());

        // Cache hit
        let cached = cache.get(&workflow_id, workflow_hash);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().levels.len(), 2);

        // Cache miss with different hash
        assert!(cache.get(&workflow_id, 99999).is_none());
    }

    #[test]
    fn test_workflow_hash() {
        let node1_id = Uuid::new_v4();
        let node2_id = Uuid::new_v4();

        let nodes1 = vec![
            Node::new("Start".to_string(), NodeKind::Start),
            Node::new("End".to_string(), NodeKind::End),
        ];

        let edges1 = vec![Edge {
            id: Uuid::new_v4(),
            from: node1_id,
            to: node2_id,
            label: None,
            condition: None,
        }];

        let hash1 = hash_workflow_structure(&nodes1, &edges1);

        // Same structure should produce same hash
        let hash2 = hash_workflow_structure(&nodes1, &edges1);
        assert_eq!(hash1, hash2);

        // Different structure should produce different hash
        let nodes2 = vec![Node::new("Start".to_string(), NodeKind::Start)];
        let hash3 = hash_workflow_structure(&nodes2, &[]);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_cache_eviction() {
        let cache = PlanCache::with_capacity(2);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let plan = ExecutionPlan {
            levels: vec![],
            workflow_hash: 123,
        };

        cache.put(id1, plan.clone());
        cache.put(id2, plan.clone());

        // Cache should be full
        assert_eq!(cache.stats().size, 2);

        // Adding third item should evict one
        cache.put(id3, plan.clone());
        assert_eq!(cache.stats().size, 2);
    }
}
