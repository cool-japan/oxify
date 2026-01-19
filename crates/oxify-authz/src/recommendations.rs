//! Permission Recommendations
//!
//! This module analyzes existing permission tuples and access patterns to suggest
//! optimizations, identify redundant permissions, and detect over-permissive grants.
//!
//! # Features
//! - Detect redundant permissions (can be simplified via hierarchy)
//! - Identify unused or rarely-used permissions
//! - Suggest tuple consolidations
//! - Recommend role-based patterns
//! - Find over-permissive grants
//!
//! # Example
//! ```rust,ignore
//! use oxify_authz::recommendations::{RecommendationEngine, RecommendationConfig};
//!
//! let config = RecommendationConfig::default();
//! let mut engine = RecommendationEngine::new(config);
//!
//! // Analyze tuples
//! engine.add_tuple(&tuple);
//! engine.record_access("user:alice", "doc:123", "read");
//!
//! // Get recommendations
//! let recommendations = engine.generate_recommendations();
//! for rec in recommendations {
//!     println!("{}: {}", rec.priority, rec.description);
//! }
//! ```

use crate::{RelationTuple, Subject};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

/// Configuration for recommendation engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationConfig {
    /// Minimum usage threshold (0.0-1.0) below which permissions are flagged as unused
    pub min_usage_threshold: f64,

    /// Time window for analyzing access patterns
    pub analysis_window: Duration,

    /// Minimum number of similar permissions to suggest consolidation
    pub min_consolidation_count: usize,

    /// Enable hierarchical redundancy detection
    pub enable_hierarchy_analysis: bool,

    /// Enable role pattern suggestions
    pub enable_role_suggestions: bool,
}

impl Default for RecommendationConfig {
    fn default() -> Self {
        Self {
            min_usage_threshold: 0.1,                             // Flag if < 10% usage
            analysis_window: Duration::from_secs(30 * 24 * 3600), // 30 days
            min_consolidation_count: 3,
            enable_hierarchy_analysis: true,
            enable_role_suggestions: true,
        }
    }
}

/// Type of recommendation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecommendationType {
    /// Permission is granted but never or rarely used
    UnusedPermission,

    /// Multiple similar permissions can be consolidated
    Consolidation,

    /// Permission is redundant due to hierarchy
    HierarchicalRedundancy,

    /// Suggest creating a role for common permission pattern
    RoleSuggestion,

    /// Permission is overly broad
    OverPermissive,

    /// Conflicting permissions exist
    Conflict,
}

/// Priority level for recommendations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::Low => write!(f, "LOW"),
            Priority::Medium => write!(f, "MEDIUM"),
            Priority::High => write!(f, "HIGH"),
            Priority::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A recommendation for permission optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub recommendation_type: RecommendationType,
    pub priority: Priority,
    pub description: String,
    pub affected_tuples: Vec<RelationTuple>,
    pub suggested_action: String,
    pub estimated_impact: String,
}

/// Track usage of a permission tuple
#[derive(Debug, Clone)]
struct TupleUsage {
    tuple: RelationTuple,
    access_count: usize,
    last_accessed: Option<SystemTime>,
    granted_count: usize,
}

/// Main recommendation engine
pub struct RecommendationEngine {
    config: RecommendationConfig,
    tuple_usage: HashMap<String, TupleUsage>,
    access_patterns: HashMap<(String, String), usize>, // (subject_id, resource_id) -> count
}

impl RecommendationEngine {
    /// Create a new recommendation engine
    pub fn new(config: RecommendationConfig) -> Self {
        Self {
            config,
            tuple_usage: HashMap::new(),
            access_patterns: HashMap::new(),
        }
    }

    /// Add a tuple to be analyzed
    pub fn add_tuple(&mut self, tuple: &RelationTuple) {
        let key = self.tuple_key(tuple);
        self.tuple_usage.entry(key).or_insert_with(|| TupleUsage {
            tuple: tuple.clone(),
            access_count: 0,
            last_accessed: None,
            granted_count: 0,
        });
    }

    /// Record an access event (used for usage tracking)
    pub fn record_access(&mut self, subject_id: &str, resource_id: &str, relation: &str) {
        // Find matching tuples
        let now = SystemTime::now();
        for usage in self.tuple_usage.values_mut() {
            if Self::matches_access_static(&usage.tuple, subject_id, resource_id, relation) {
                usage.access_count += 1;
                usage.last_accessed = Some(now);
                usage.granted_count += 1;
            }
        }

        // Track access patterns
        let key = (subject_id.to_string(), resource_id.to_string());
        *self.access_patterns.entry(key).or_insert(0) += 1;
    }

    fn matches_access_static(
        tuple: &RelationTuple,
        subject_id: &str,
        resource_id: &str,
        relation: &str,
    ) -> bool {
        // Check if the tuple could grant this access
        let resource_matches = format!("{}:{}", tuple.namespace, tuple.object_id) == resource_id;

        let subject_matches = match &tuple.subject {
            Subject::User(id) => id == subject_id,
            Subject::UserSet {
                namespace: _,
                object_id: _,
                relation: _,
            } => false,
        };

        resource_matches && subject_matches && tuple.relation == relation
    }

    /// Generate recommendations based on collected data
    pub fn generate_recommendations(&self) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // 1. Detect unused permissions
        recommendations.extend(self.detect_unused_permissions());

        // 2. Detect hierarchical redundancies
        if self.config.enable_hierarchy_analysis {
            recommendations.extend(self.detect_hierarchical_redundancy());
        }

        // 3. Suggest consolidations
        recommendations.extend(self.suggest_consolidations());

        // 4. Suggest role patterns
        if self.config.enable_role_suggestions {
            recommendations.extend(self.suggest_roles());
        }

        // 5. Detect conflicts
        recommendations.extend(self.detect_conflicts());

        // Sort by priority (highest first)
        recommendations.sort_by(|a, b| b.priority.cmp(&a.priority));

        recommendations
    }

    fn detect_unused_permissions(&self) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        for usage in self.tuple_usage.values() {
            // Calculate usage rate
            let total_possible_accesses = self.estimate_total_accesses(&usage.tuple);
            let usage_rate = if total_possible_accesses > 0 {
                usage.access_count as f64 / total_possible_accesses as f64
            } else {
                0.0
            };

            // Check if permission is unused or rarely used
            if usage_rate < self.config.min_usage_threshold {
                let priority = if usage.access_count == 0 {
                    Priority::High
                } else {
                    Priority::Medium
                };

                let description = if usage.access_count == 0 {
                    format!(
                        "Permission never used: {:?} {} on {}:{}",
                        usage.tuple.subject,
                        usage.tuple.relation,
                        usage.tuple.namespace,
                        usage.tuple.object_id
                    )
                } else {
                    format!(
                        "Permission rarely used ({:.1}% usage): {:?} {} on {}:{}",
                        usage_rate * 100.0,
                        usage.tuple.subject,
                        usage.tuple.relation,
                        usage.tuple.namespace,
                        usage.tuple.object_id
                    )
                };

                recommendations.push(Recommendation {
                    recommendation_type: RecommendationType::UnusedPermission,
                    priority,
                    description,
                    affected_tuples: vec![usage.tuple.clone()],
                    suggested_action: "Consider revoking this permission if no longer needed"
                        .to_string(),
                    estimated_impact: format!("Remove {} unused permission(s)", 1),
                });
            }
        }

        recommendations
    }

    fn detect_hierarchical_redundancy(&self) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Group tuples by subject
        let mut subject_tuples: HashMap<String, Vec<&TupleUsage>> = HashMap::new();
        for usage in self.tuple_usage.values() {
            let subject_key = format!("{:?}", usage.tuple.subject);
            subject_tuples.entry(subject_key).or_default().push(usage);
        }

        // Check for redundant permissions due to hierarchy
        for (subject, tuples) in subject_tuples {
            if tuples.len() < 2 {
                continue;
            }

            // Look for parent-child relationships
            for i in 0..tuples.len() {
                for j in 0..tuples.len() {
                    if i == j {
                        continue;
                    }

                    if self.is_hierarchically_redundant(&tuples[i].tuple, &tuples[j].tuple) {
                        recommendations.push(Recommendation {
                            recommendation_type: RecommendationType::HierarchicalRedundancy,
                            priority: Priority::Medium,
                            description: format!(
                                "Redundant permission for {}: parent resource already grants access",
                                subject
                            ),
                            affected_tuples: vec![tuples[i].tuple.clone(), tuples[j].tuple.clone()],
                            suggested_action: "Remove child permission, keep parent".to_string(),
                            estimated_impact: "Simplify permission model".to_string(),
                        });
                    }
                }
            }
        }

        recommendations
    }

    fn is_hierarchically_redundant(&self, tuple1: &RelationTuple, tuple2: &RelationTuple) -> bool {
        // Check if tuple1 is redundant because tuple2 provides the same or broader access
        // For now, we do simple namespace-based checking since the current RelationTuple
        // doesn't have explicit parent references
        if tuple1.relation != tuple2.relation {
            return false;
        }

        // Check if they're in the same namespace
        if tuple1.namespace != tuple2.namespace {
            return false;
        }

        // Simple heuristic: if object IDs suggest parent-child relationship
        // e.g., "folder/subfolder" and "folder"
        tuple1
            .object_id
            .starts_with(&format!("{}/", tuple2.object_id))
    }

    fn suggest_consolidations(&self) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Group tuples by relation
        let mut relation_tuples: HashMap<String, Vec<&TupleUsage>> = HashMap::new();
        for usage in self.tuple_usage.values() {
            relation_tuples
                .entry(usage.tuple.relation.clone())
                .or_default()
                .push(usage);
        }

        // Look for consolidation opportunities
        for (relation, tuples) in relation_tuples {
            if tuples.len() < self.config.min_consolidation_count {
                continue;
            }

            // Group by resource namespace
            let mut namespace_groups: HashMap<String, Vec<&TupleUsage>> = HashMap::new();
            for usage in tuples {
                let namespace = &usage.tuple.namespace;
                namespace_groups
                    .entry(namespace.clone())
                    .or_default()
                    .push(usage);
            }

            for (namespace, group) in namespace_groups {
                if group.len() >= self.config.min_consolidation_count {
                    let affected: Vec<RelationTuple> =
                        group.iter().map(|u| u.tuple.clone()).collect();

                    recommendations.push(Recommendation {
                        recommendation_type: RecommendationType::Consolidation,
                        priority: Priority::Low,
                        description: format!(
                            "Found {} similar permissions for relation '{}' in namespace '{}'",
                            group.len(), relation, namespace
                        ),
                        affected_tuples: affected,
                        suggested_action: format!(
                            "Consider creating a role or group for users with '{}' access to '{}' resources",
                            relation, namespace
                        ),
                        estimated_impact: format!("Consolidate {} tuples into 1 role assignment", group.len()),
                    });
                }
            }
        }

        recommendations
    }

    fn suggest_roles(&self) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Analyze access patterns to find common permission sets
        let mut subject_permissions: HashMap<String, HashSet<String>> = HashMap::new();

        for usage in self.tuple_usage.values() {
            let subject_key = format!("{:?}", usage.tuple.subject);
            let permission_key = format!("{}#{}", usage.tuple.namespace, usage.tuple.relation);
            subject_permissions
                .entry(subject_key)
                .or_default()
                .insert(permission_key);
        }

        // Find common permission sets (potential roles)
        let mut permission_set_counts: HashMap<Vec<String>, Vec<String>> = HashMap::new();
        for (subject, permissions) in subject_permissions {
            let mut sorted_perms: Vec<String> = permissions.into_iter().collect();
            sorted_perms.sort();
            permission_set_counts
                .entry(sorted_perms)
                .or_default()
                .push(subject);
        }

        // Suggest roles for common patterns
        for (permissions, subjects) in permission_set_counts {
            if subjects.len() >= 3 && permissions.len() >= 2 {
                recommendations.push(Recommendation {
                    recommendation_type: RecommendationType::RoleSuggestion,
                    priority: Priority::Medium,
                    description: format!(
                        "Found {} users with identical permission set ({} permissions)",
                        subjects.len(),
                        permissions.len()
                    ),
                    affected_tuples: vec![],
                    suggested_action: format!(
                        "Create a role with permissions: {:?}",
                        permissions.iter().take(5).collect::<Vec<_>>()
                    ),
                    estimated_impact: format!(
                        "Replace {} individual permission grants with 1 role assignment each",
                        subjects.len()
                    ),
                });
            }
        }

        recommendations
    }

    fn detect_conflicts(&self) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Group tuples by (subject, resource)
        let mut subject_resource_map: HashMap<(String, String), Vec<&TupleUsage>> = HashMap::new();

        for usage in self.tuple_usage.values() {
            let subject_key = format!("{:?}", usage.tuple.subject);
            let resource_key = format!("{}:{}", usage.tuple.namespace, usage.tuple.object_id);
            subject_resource_map
                .entry((subject_key, resource_key))
                .or_default()
                .push(usage);
        }

        // Check for potentially conflicting permissions
        for ((subject, resource), tuples) in subject_resource_map {
            if tuples.len() > 1 {
                let relations: Vec<&str> =
                    tuples.iter().map(|u| u.tuple.relation.as_str()).collect();

                // Check for conflicting relations (e.g., read + admin might indicate over-permission)
                if relations.contains(&"admin") && relations.len() > 1 {
                    recommendations.push(Recommendation {
                        recommendation_type: RecommendationType::Conflict,
                        priority: Priority::Low,
                        description: format!(
                            "Multiple permission levels for {} on {}: {:?}",
                            subject, resource, relations
                        ),
                        affected_tuples: tuples.iter().map(|u| u.tuple.clone()).collect(),
                        suggested_action: "Review if all permissions are necessary (admin typically includes other permissions)".to_string(),
                        estimated_impact: "Potential cleanup of redundant permissions".to_string(),
                    });
                }
            }
        }

        recommendations
    }

    fn estimate_total_accesses(&self, tuple: &RelationTuple) -> usize {
        // Estimate how many times this permission could have been used
        // based on subject's total activity
        let subject_key = format!("{:?}", tuple.subject);
        self.access_patterns
            .iter()
            .filter(|((subj, _), _)| *subj == subject_key)
            .map(|(_, count)| count)
            .sum()
    }

    fn tuple_key(&self, tuple: &RelationTuple) -> String {
        format!(
            "{:?}#{}#{}:{}",
            tuple.subject, tuple.relation, tuple.namespace, tuple.object_id
        )
    }

    /// Get usage statistics for a specific tuple
    pub fn get_tuple_usage(&self, tuple: &RelationTuple) -> Option<UsageStats> {
        let key = self.tuple_key(tuple);
        let usage = self.tuple_usage.get(&key)?;

        Some(UsageStats {
            access_count: usage.access_count,
            granted_count: usage.granted_count,
            last_accessed: usage.last_accessed,
        })
    }

    /// Clear old data
    pub fn cleanup(&mut self, cutoff: SystemTime) {
        self.tuple_usage
            .retain(|_, usage| usage.last_accessed.is_some_and(|t| t >= cutoff));
    }
}

/// Usage statistics for a tuple
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub access_count: usize,
    pub granted_count: usize,
    pub last_accessed: Option<SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_simple_tuple(
        subject: &str,
        relation: &str,
        resource_ns: &str,
        resource_id: &str,
    ) -> RelationTuple {
        RelationTuple {
            namespace: resource_ns.to_string(),
            object_id: resource_id.to_string(),
            relation: relation.to_string(),
            subject: Subject::User(subject.to_string()),
            condition: None,
        }
    }

    #[test]
    fn test_recommendation_engine_creation() {
        let config = RecommendationConfig::default();
        let engine = RecommendationEngine::new(config);
        assert_eq!(engine.tuple_usage.len(), 0);
    }

    #[test]
    fn test_add_tuple() {
        let config = RecommendationConfig::default();
        let mut engine = RecommendationEngine::new(config);

        let tuple = create_simple_tuple("user:alice", "read", "doc", "123");
        engine.add_tuple(&tuple);

        assert_eq!(engine.tuple_usage.len(), 1);
    }

    #[test]
    fn test_record_access() {
        let config = RecommendationConfig::default();
        let mut engine = RecommendationEngine::new(config);

        let tuple = create_simple_tuple("user:alice", "read", "doc", "123");
        engine.add_tuple(&tuple);

        engine.record_access("user:alice", "doc:123", "read");

        let stats = engine.get_tuple_usage(&tuple).unwrap();
        assert_eq!(stats.access_count, 1);
        assert_eq!(stats.granted_count, 1);
        assert!(stats.last_accessed.is_some());
    }

    #[test]
    fn test_detect_unused_permissions() {
        let config = RecommendationConfig {
            min_usage_threshold: 0.5,
            ..Default::default()
        };
        let mut engine = RecommendationEngine::new(config);

        // Add unused tuple
        let unused_tuple = create_simple_tuple("user:bob", "write", "doc", "456");
        engine.add_tuple(&unused_tuple);

        // Add used tuple
        let used_tuple = create_simple_tuple("user:alice", "read", "doc", "123");
        engine.add_tuple(&used_tuple);
        for _ in 0..10 {
            engine.record_access("user:alice", "doc:123", "read");
        }

        let recommendations = engine.generate_recommendations();

        // Should recommend removing unused permission
        let unused_recs: Vec<_> = recommendations
            .iter()
            .filter(|r| r.recommendation_type == RecommendationType::UnusedPermission)
            .collect();

        assert!(!unused_recs.is_empty());
    }

    #[test]
    fn test_suggest_consolidations() {
        let config = RecommendationConfig {
            min_consolidation_count: 3,
            ..Default::default()
        };
        let mut engine = RecommendationEngine::new(config);

        // Add many similar permissions
        for i in 0..5 {
            let tuple =
                create_simple_tuple(&format!("user:{}", i), "read", "doc", &format!("{}", i));
            engine.add_tuple(&tuple);
        }

        let recommendations = engine.generate_recommendations();

        // Should suggest consolidation
        let consolidation_recs: Vec<_> = recommendations
            .iter()
            .filter(|r| r.recommendation_type == RecommendationType::Consolidation)
            .collect();

        assert!(!consolidation_recs.is_empty());
    }

    #[test]
    fn test_suggest_roles() {
        let config = RecommendationConfig {
            enable_role_suggestions: true,
            ..Default::default()
        };
        let mut engine = RecommendationEngine::new(config);

        // Add identical permission sets for multiple users
        for i in 0..4 {
            let tuple1 = create_simple_tuple(&format!("user:{}", i), "read", "doc", "shared");
            let tuple2 = create_simple_tuple(&format!("user:{}", i), "write", "doc", "shared");
            engine.add_tuple(&tuple1);
            engine.add_tuple(&tuple2);
        }

        let recommendations = engine.generate_recommendations();

        // Should suggest role creation
        let role_recs: Vec<_> = recommendations
            .iter()
            .filter(|r| r.recommendation_type == RecommendationType::RoleSuggestion)
            .collect();

        assert!(!role_recs.is_empty());
    }

    #[test]
    fn test_cleanup() {
        let config = RecommendationConfig::default();
        let mut engine = RecommendationEngine::new(config);

        let tuple = create_simple_tuple("user:alice", "read", "doc", "123");
        engine.add_tuple(&tuple);
        engine.record_access("user:alice", "doc:123", "read");

        assert_eq!(engine.tuple_usage.len(), 1);

        // Cleanup with future cutoff removes all data
        let future = SystemTime::now() + Duration::from_secs(3600);
        engine.cleanup(future);

        assert_eq!(engine.tuple_usage.len(), 0);
    }
}
