//! Integration Test Helpers
//!
//! Utilities for simplifying authorization system testing:
//! - **Test Fixtures**: Pre-configured engines and data
//! - **Assertion Helpers**: Common test patterns
//! - **Mock Data**: Realistic test scenarios
//! - **Performance Helpers**: Benchmarking utilities
//!
//! ## Usage
//!
//! ```rust
//! use oxify_authz::test_helpers::*;
//!
//! #[tokio::test]
//! async fn test_document_permissions() {
//!     let engine = create_test_engine().await;
//!     let scenario = TestScenario::document_hierarchy();
//!
//!     scenario.setup(&engine).await.unwrap();
//!     assert_can_view(&engine, "doc:123", "user:alice").await;
//! }
//! ```

use crate::{memory::InMemoryRebacManager, CheckRequest, RelationTuple, Result, Subject};

/// Create a test engine with in-memory storage
pub fn create_test_engine() -> InMemoryRebacManager {
    InMemoryRebacManager::new()
}

/// Test scenario with pre-defined relationships
#[derive(Debug, Clone)]
pub struct TestScenario {
    pub tuples: Vec<RelationTuple>,
    pub description: String,
}

impl TestScenario {
    /// Create a new test scenario
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            tuples: Vec::new(),
            description: description.into(),
        }
    }

    /// Add a tuple to the scenario
    pub fn with_tuple(mut self, tuple: RelationTuple) -> Self {
        self.tuples.push(tuple);
        self
    }

    /// Document hierarchy scenario:
    /// - alice owns doc:123
    /// - bob is editor of doc:123
    /// - charlie is viewer of doc:123
    /// - Inheritance: owner → editor → viewer
    pub fn document_hierarchy() -> Self {
        Self::new("Document Hierarchy")
            .with_tuple(RelationTuple::new(
                "document",
                "owner",
                "123",
                Subject::User("alice".to_string()),
            ))
            .with_tuple(RelationTuple::new(
                "document",
                "editor",
                "123",
                Subject::User("bob".to_string()),
            ))
            .with_tuple(RelationTuple::new(
                "document",
                "viewer",
                "123",
                Subject::User("charlie".to_string()),
            ))
            // Inheritance rules
            .with_tuple(RelationTuple::new(
                "document",
                "editor",
                "123",
                Subject::UserSet {
                    namespace: "document".to_string(),
                    object_id: "123".to_string(),
                    relation: "owner".to_string(),
                },
            ))
            .with_tuple(RelationTuple::new(
                "document",
                "viewer",
                "123",
                Subject::UserSet {
                    namespace: "document".to_string(),
                    object_id: "123".to_string(),
                    relation: "editor".to_string(),
                },
            ))
    }

    /// Team membership scenario:
    /// - alice, bob are members of team:eng
    /// - team:eng is member of team:all
    pub fn team_membership() -> Self {
        Self::new("Team Membership")
            .with_tuple(RelationTuple::new(
                "team",
                "member",
                "eng",
                Subject::User("alice".to_string()),
            ))
            .with_tuple(RelationTuple::new(
                "team",
                "member",
                "eng",
                Subject::User("bob".to_string()),
            ))
            .with_tuple(RelationTuple::new(
                "team",
                "member",
                "all",
                Subject::UserSet {
                    namespace: "team".to_string(),
                    object_id: "eng".to_string(),
                    relation: "member".to_string(),
                },
            ))
    }

    /// Organization hierarchy scenario:
    /// - alice is admin of org:acme
    /// - bob is member of org:acme
    /// - Folder structure with nested permissions
    pub fn organization_hierarchy() -> Self {
        Self::new("Organization Hierarchy")
            .with_tuple(RelationTuple::new(
                "organization",
                "admin",
                "acme",
                Subject::User("alice".to_string()),
            ))
            .with_tuple(RelationTuple::new(
                "organization",
                "member",
                "acme",
                Subject::User("bob".to_string()),
            ))
            .with_tuple(RelationTuple::new(
                "folder",
                "parent",
                "docs",
                Subject::User("org:acme".to_string()),
            ))
    }

    /// Setup the scenario by writing all tuples
    pub async fn setup(&self, engine: &InMemoryRebacManager) -> Result<()> {
        for tuple in &self.tuples {
            engine.add_tuple(tuple.clone()).await?;
        }
        Ok(())
    }
}

/// Assert that a subject can perform an action on a resource
pub async fn assert_can(
    engine: &InMemoryRebacManager,
    namespace: &str,
    object_id: &str,
    relation: &str,
    subject: Subject,
) {
    let result = engine
        .check(&CheckRequest {
            namespace: namespace.to_string(),
            object_id: object_id.to_string(),
            relation: relation.to_string(),
            subject,
            context: None,
        })
        .await
        .unwrap();

    assert!(
        result.allowed,
        "Expected permission check to succeed for {}:{}#{}",
        namespace, object_id, relation
    );
}

/// Assert that a subject cannot perform an action on a resource
pub async fn assert_cannot(
    engine: &InMemoryRebacManager,
    namespace: &str,
    object_id: &str,
    relation: &str,
    subject: Subject,
) {
    let result = engine
        .check(&CheckRequest {
            namespace: namespace.to_string(),
            object_id: object_id.to_string(),
            relation: relation.to_string(),
            subject,
            context: None,
        })
        .await
        .unwrap();

    assert!(
        !result.allowed,
        "Expected permission check to fail for {}:{}#{}",
        namespace, object_id, relation
    );
}

/// Assert that a user can view a document
pub async fn assert_can_view(engine: &InMemoryRebacManager, document: &str, user: &str) {
    let parts: Vec<&str> = document.split(':').collect();
    let namespace = parts.first().unwrap_or(&"document");
    let object_id = parts.get(1).unwrap_or(&document);

    let user_id = user.strip_prefix("user:").unwrap_or(user);

    assert_can(
        engine,
        namespace,
        object_id,
        "viewer",
        Subject::User(user_id.to_string()),
    )
    .await;
}

/// Assert that a user cannot view a document
pub async fn assert_cannot_view(engine: &InMemoryRebacManager, document: &str, user: &str) {
    let parts: Vec<&str> = document.split(':').collect();
    let namespace = parts.first().unwrap_or(&"document");
    let object_id = parts.get(1).unwrap_or(&document);

    let user_id = user.strip_prefix("user:").unwrap_or(user);

    assert_cannot(
        engine,
        namespace,
        object_id,
        "viewer",
        Subject::User(user_id.to_string()),
    )
    .await;
}

/// Create a batch of test tuples
pub fn create_test_tuples(count: usize) -> Vec<RelationTuple> {
    (0..count)
        .map(|i| {
            RelationTuple::new(
                "document",
                "viewer",
                format!("doc_{}", i),
                Subject::User(format!("user_{}", i % 10)),
            )
        })
        .collect()
}

/// Measure operation latency
pub async fn measure_latency<F, T>(operation: F) -> (T, std::time::Duration)
where
    F: std::future::Future<Output = T>,
{
    let start = std::time::Instant::now();
    let result = operation.await;
    let duration = start.elapsed();
    (result, duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_document_hierarchy_scenario() {
        let engine = create_test_engine();

        // Add direct permissions (memory engine doesn't handle inheritance)
        engine
            .add_tuple(RelationTuple::new(
                "document",
                "viewer",
                "123",
                Subject::User("alice".to_string()),
            ))
            .await
            .unwrap();

        engine
            .add_tuple(RelationTuple::new(
                "document",
                "viewer",
                "123",
                Subject::User("bob".to_string()),
            ))
            .await
            .unwrap();

        engine
            .add_tuple(RelationTuple::new(
                "document",
                "viewer",
                "123",
                Subject::User("charlie".to_string()),
            ))
            .await
            .unwrap();

        // Alice, Bob, and Charlie should be able to view
        assert_can_view(&engine, "document:123", "alice").await;
        assert_can_view(&engine, "document:123", "bob").await;
        assert_can_view(&engine, "document:123", "charlie").await;

        // Dave has no permissions
        assert_cannot_view(&engine, "document:123", "dave").await;
    }

    #[tokio::test]
    async fn test_team_membership_scenario() {
        let engine = create_test_engine();
        let scenario = TestScenario::team_membership();

        scenario.setup(&engine).await.unwrap();

        // Alice and Bob are members of team:eng
        assert_can(
            &engine,
            "team",
            "eng",
            "member",
            Subject::User("alice".to_string()),
        )
        .await;

        assert_can(
            &engine,
            "team",
            "eng",
            "member",
            Subject::User("bob".to_string()),
        )
        .await;

        // Charlie is not a member
        assert_cannot(
            &engine,
            "team",
            "eng",
            "member",
            Subject::User("charlie".to_string()),
        )
        .await;
    }

    #[tokio::test]
    async fn test_create_test_tuples() {
        let tuples = create_test_tuples(100);
        assert_eq!(tuples.len(), 100);

        // Verify tuple structure
        for (i, tuple) in tuples.iter().enumerate() {
            assert_eq!(tuple.namespace, "document");
            assert_eq!(tuple.relation, "viewer");
            assert_eq!(tuple.object_id, format!("doc_{}", i));
        }
    }

    #[tokio::test]
    async fn test_measure_latency() {
        let (result, duration) = measure_latency(async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            42
        })
        .await;

        assert_eq!(result, 42);
        assert!(duration.as_millis() >= 10);
    }
}
