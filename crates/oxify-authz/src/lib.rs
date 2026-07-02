//! # OxiFY Authorization Engine (ReBAC)
//!
//! Google Zanzibar-style Relationship-Based Access Control (ReBAC) implementation.
//!
//! ## Architecture
//!
//! This crate provides fine-grained authorization based on relationships between
//! entities rather than traditional role-based access control (RBAC).
//!
//! **Key Concepts:**
//! - **Relation Tuples**: `(namespace, object_id, relation, subject)` representing relationships
//! - **Check API**: Determines if a subject can perform an action on an object
//! - **Expand API**: Returns all subjects with a specific relation to an object
//! - **Reachability Index**: Leopard indexing for O(1) authorization checks
//!
//! ## Example
//!
//! ```no_run
//! use oxify_authz::*;
//!
//! # async fn example() -> Result<()> {
//! // Backed by the pure-Rust SQLite (Limbo) engine via `oxisql`. Use
//! // `sqlite::memory:` for an ephemeral database or `sqlite:/path/to.db`
//! // for a persistent one.
//! let engine = AuthzEngine::new("sqlite:/var/lib/oxify/authz.db").await?;
//! engine.migrate().await?;
//!
//! // Define: User alice is an owner of document:123
//! engine.write_tuple(RelationTuple::new(
//!     "document",
//!     "owner",
//!     "123",
//!     Subject::User("alice".to_string()),
//! )).await?;
//!
//! // Check: Can alice view document:123?
//! let allowed = engine.check(CheckRequest {
//!     namespace: "document".to_string(),
//!     object_id: "123".to_string(),
//!     relation: "viewer".to_string(),
//!     subject: Subject::User("alice".to_string()),
//!     context: None,
//! }).await?;
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

// Core modules (SQLite compatible)
pub mod anomaly;
pub mod audit;
pub mod bloom;
pub mod cache;
pub mod chaos;
// pub mod citus;        // Disabled - PostgreSQL Citus specific
pub mod delegation;
pub mod edge;
pub mod engine;
#[cfg(feature = "grpc")]
pub mod grpc;
pub mod hybrid;
pub mod leopard;
pub mod memory;
pub mod metrics;
pub mod migration;
// pub mod multiregion;  // Disabled - PostgreSQL replicas
pub mod multitenancy;
pub mod oauth2;
// pub mod partitioning; // Disabled - PostgreSQL partitioning
// pub mod pooling;      // Disabled - PgPoolOptions
pub mod profiling;
#[cfg(any(test, feature = "proptest-support"))]
pub mod proptest_helpers;
pub mod quantum;
pub mod query_optimizer;
pub mod recommendations;
pub mod redis_cache;
// pub mod replica;      // Disabled - PostgreSQL read replicas
#[cfg(test)]
pub mod test_helpers;
pub mod types;
pub mod warming;
pub mod zkp;

pub use anomaly::*;
pub use audit::*;
pub use bloom::*;
pub use cache::*;
pub use chaos::*;
// pub use citus::*;
pub use delegation::*;
pub use edge::*;
pub use engine::*;
#[cfg(feature = "grpc")]
pub use grpc::*;
pub use hybrid::*;
pub use leopard::*;
pub use memory::*;
pub use metrics::*;
// pub use multiregion::*;
pub use multitenancy::*;
pub use oauth2::*;
// pub use partitioning::*;
// pub use pooling::*;
pub use profiling::*;
pub use quantum::*;
pub use query_optimizer::*;
pub use recommendations::*;
pub use redis_cache::*;
// pub use replica::*;
pub use types::*;
pub use warming::*;
pub use zkp::*;

pub type Result<T> = std::result::Result<T, AuthzError>;

#[derive(Error, Debug)]
pub enum AuthzError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Invalid relation tuple: {0}")]
    InvalidTuple(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Cycle detected in relation graph")]
    CycleDetected,

    #[error("Namespace not found: {0}")]
    NamespaceNotFound(String),
}

/// Subject in a relation tuple
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Subject {
    /// A user identified by ID
    User(String),

    /// A user set (e.g., "team:A#members")
    /// Format: "namespace:object_id#relation"
    UserSet {
        namespace: String,
        object_id: String,
        relation: String,
    },
}

impl Subject {
    /// Parse subject from string format
    /// Examples:
    /// - "user:alice" → User("alice")
    /// - "team:A#members" → UserSet { namespace: "team", object_id: "A", relation: "members" }
    pub fn from_string(s: &str) -> Result<Self> {
        if let Some(userset) = s.strip_prefix("userset:") {
            let parts: Vec<&str> = userset.split(&[':', '#'][..]).collect();
            if parts.len() == 3 {
                Ok(Subject::UserSet {
                    namespace: parts[0].to_string(),
                    object_id: parts[1].to_string(),
                    relation: parts[2].to_string(),
                })
            } else {
                Err(AuthzError::InvalidTuple(format!(
                    "Invalid userset format: {}",
                    s
                )))
            }
        } else if let Some(user_id) = s.strip_prefix("user:") {
            Ok(Subject::User(user_id.to_string()))
        } else {
            Err(AuthzError::InvalidTuple(format!(
                "Invalid subject format: {}",
                s
            )))
        }
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subject::User(id) => write!(f, "user:{}", id),
            Subject::UserSet {
                namespace,
                object_id,
                relation,
            } => write!(f, "userset:{}:{}#{}", namespace, object_id, relation),
        }
    }
}

/// Relation tuple representing a relationship
/// Example: (document, doc123, owner, user:alice) means "alice owns doc123"
/// Extended with optional conditions from OxiRS
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationTuple {
    pub namespace: String,
    pub object_id: String,
    pub relation: String,
    pub subject: Subject,

    /// Optional condition (e.g., time window, IP address)
    /// Ported from OxiRS rebac.rs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<RelationshipCondition>,
}

impl RelationTuple {
    /// Create a new relation tuple without conditions
    pub fn new(
        namespace: impl Into<String>,
        relation: impl Into<String>,
        object_id: impl Into<String>,
        subject: Subject,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            object_id: object_id.into(),
            relation: relation.into(),
            subject,
            condition: None,
        }
    }

    /// Create a relation tuple with a condition
    pub fn with_condition(
        namespace: impl Into<String>,
        object_id: impl Into<String>,
        relation: impl Into<String>,
        subject: Subject,
        condition: RelationshipCondition,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            object_id: object_id.into(),
            relation: relation.into(),
            subject,
            condition: Some(condition),
        }
    }

    /// Check if this tuple's condition is satisfied (without context)
    pub fn is_condition_satisfied(&self) -> bool {
        self.condition.as_ref().is_none_or(|c| c.is_satisfied())
    }

    /// Check if this tuple's condition is satisfied with a request context
    pub fn is_condition_satisfied_with_context(&self, context: &RequestContext) -> bool {
        self.condition
            .as_ref()
            .is_none_or(|c| c.is_satisfied_with_context(context))
    }
}

/// Request to check if a subject has a relation to an object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRequest {
    pub namespace: String,
    pub object_id: String,
    pub relation: String,
    pub subject: Subject,
    /// Optional request context for conditional permissions
    #[serde(skip)]
    pub context: Option<RequestContext>,
}

impl CheckRequest {
    /// Create a new check request
    pub fn new(
        namespace: impl Into<String>,
        object_id: impl Into<String>,
        relation: impl Into<String>,
        subject: Subject,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            object_id: object_id.into(),
            relation: relation.into(),
            subject,
            context: None,
        }
    }

    /// Add request context for conditional permissions
    pub fn with_context(mut self, context: RequestContext) -> Self {
        self.context = Some(context);
        self
    }
}

/// Response from a check request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResponse {
    pub allowed: bool,
    pub cached: bool,
}

/// Request to expand a relation (find all subjects)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandRequest {
    pub namespace: String,
    pub object_id: String,
    pub relation: String,
}

/// Response from an expand request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandResponse {
    pub subjects: Vec<Subject>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subject_parsing() {
        let user = Subject::from_string("user:alice").unwrap();
        assert_eq!(user, Subject::User("alice".to_string()));

        let userset = Subject::from_string("userset:team:A#members").unwrap();
        assert_eq!(
            userset,
            Subject::UserSet {
                namespace: "team".to_string(),
                object_id: "A".to_string(),
                relation: "members".to_string(),
            }
        );
    }

    #[test]
    fn test_subject_serialization() {
        let user = Subject::User("bob".to_string());
        assert_eq!(user.to_string(), "user:bob");

        let userset = Subject::UserSet {
            namespace: "group".to_string(),
            object_id: "admins".to_string(),
            relation: "member".to_string(),
        };
        assert_eq!(userset.to_string(), "userset:group:admins#member");
    }
}
