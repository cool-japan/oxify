//! Permission Delegation System
//!
//! Allows users to delegate their permissions to other users with optional time limits
//! and specific scopes.
//!
//! ## Features
//!
//! - **Time-Limited Delegations**: Set expiration times for delegated permissions
//! - **Scoped Delegations**: Delegate specific permissions, not all
//! - **Revocable**: Delegations can be revoked at any time
//! - **Audit Trail**: All delegations are tracked for compliance
//!
//! ## Example
//!
//! ```rust
//! use oxify_authz::delegation::{Delegation, DelegationManager};
//! use oxify_authz::Subject;
//! use chrono::{Duration, Utc};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = DelegationManager::new();
//!
//! // Alice delegates "editor" permission on "document:123" to Bob for 24 hours
//! let delegation = Delegation::new(
//!     Subject::User("alice".to_string()),
//!     Subject::User("bob".to_string()),
//!     "document",
//!     "123",
//!     "editor",
//! )
//! .with_expiration(Utc::now() + Duration::hours(24));
//!
//! manager.create_delegation(delegation).await?;
//!
//! // Check if Bob can act as editor through delegation
//! let has_delegation = manager.check_delegation(
//!     &Subject::User("bob".to_string()),
//!     "document",
//!     "123",
//!     "editor",
//! ).await?;
//! assert!(has_delegation);
//! # Ok(())
//! # }
//! ```

use crate::{AuthzError, RelationTuple, Result, Subject};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Delegation of permissions from one subject to another
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Delegation {
    /// Unique delegation ID
    pub id: String,

    /// Subject who is delegating (delegator)
    pub delegator: Subject,

    /// Subject receiving the delegation (delegate)
    pub delegate: Subject,

    /// Namespace of the resource
    pub namespace: String,

    /// Object ID of the resource
    pub object_id: String,

    /// Relation being delegated
    pub relation: String,

    /// When the delegation was created
    pub created_at: DateTime<Utc>,

    /// When the delegation expires (None = never expires)
    pub expires_at: Option<DateTime<Utc>>,

    /// Whether the delegation has been revoked
    pub revoked: bool,

    /// When the delegation was revoked (if applicable)
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Delegation {
    /// Create a new delegation
    pub fn new(
        delegator: Subject,
        delegate: Subject,
        namespace: impl Into<String>,
        object_id: impl Into<String>,
        relation: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            delegator,
            delegate,
            namespace: namespace.into(),
            object_id: object_id.into(),
            relation: relation.into(),
            created_at: Utc::now(),
            expires_at: None,
            revoked: false,
            revoked_at: None,
        }
    }

    /// Set expiration time for the delegation
    pub fn with_expiration(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Check if this delegation is currently active
    pub fn is_active(&self) -> bool {
        if self.revoked {
            return false;
        }

        if let Some(expires_at) = self.expires_at {
            Utc::now() < expires_at
        } else {
            true
        }
    }

    /// Revoke this delegation
    pub fn revoke(&mut self) {
        self.revoked = true;
        self.revoked_at = Some(Utc::now());
    }

    /// Convert to a relation tuple (for compatibility with existing authorization)
    pub fn to_relation_tuple(&self) -> RelationTuple {
        RelationTuple::new(
            &self.namespace,
            &self.relation,
            &self.object_id,
            self.delegate.clone(),
        )
    }
}

/// Manager for permission delegations
pub struct DelegationManager {
    /// Active delegations by ID
    delegations: Arc<RwLock<HashMap<String, Delegation>>>,

    /// Index: delegate -> list of delegation IDs
    by_delegate: Arc<RwLock<HashMap<String, Vec<String>>>>,

    /// Index: delegator -> list of delegation IDs
    by_delegator: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl DelegationManager {
    /// Create a new delegation manager
    pub fn new() -> Self {
        Self {
            delegations: Arc::new(RwLock::new(HashMap::new())),
            by_delegate: Arc::new(RwLock::new(HashMap::new())),
            by_delegator: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new delegation
    pub async fn create_delegation(&self, delegation: Delegation) -> Result<String> {
        let id = delegation.id.clone();
        let delegate_key = delegation.delegate.to_string();
        let delegator_key = delegation.delegator.to_string();

        // Store the delegation
        let mut delegations = self.delegations.write().await;
        delegations.insert(id.clone(), delegation.clone());

        // Index by delegate
        let mut by_delegate = self.by_delegate.write().await;
        by_delegate
            .entry(delegate_key)
            .or_insert_with(Vec::new)
            .push(id.clone());

        // Index by delegator
        let mut by_delegator = self.by_delegator.write().await;
        by_delegator
            .entry(delegator_key)
            .or_insert_with(Vec::new)
            .push(id.clone());

        Ok(id)
    }

    /// Revoke a delegation by ID
    pub async fn revoke_delegation(&self, delegation_id: &str) -> Result<()> {
        let mut delegations = self.delegations.write().await;
        if let Some(delegation) = delegations.get_mut(delegation_id) {
            delegation.revoke();
            Ok(())
        } else {
            Err(AuthzError::InvalidTuple(format!(
                "Delegation not found: {}",
                delegation_id
            )))
        }
    }

    /// Get a delegation by ID
    pub async fn get_delegation(&self, delegation_id: &str) -> Result<Option<Delegation>> {
        let delegations = self.delegations.read().await;
        Ok(delegations.get(delegation_id).cloned())
    }

    /// Check if a subject has a delegated permission
    pub async fn check_delegation(
        &self,
        delegate: &Subject,
        namespace: &str,
        object_id: &str,
        relation: &str,
    ) -> Result<bool> {
        let delegate_key = delegate.to_string();

        // Get all delegations for this delegate
        let by_delegate = self.by_delegate.read().await;
        if let Some(delegation_ids) = by_delegate.get(&delegate_key) {
            let delegations = self.delegations.read().await;

            for id in delegation_ids {
                if let Some(delegation) = delegations.get(id) {
                    // Check if this delegation matches and is active
                    if delegation.namespace == namespace
                        && delegation.object_id == object_id
                        && delegation.relation == relation
                        && delegation.is_active()
                    {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    /// List all active delegations for a delegate
    pub async fn list_delegations_for_delegate(
        &self,
        delegate: &Subject,
    ) -> Result<Vec<Delegation>> {
        let delegate_key = delegate.to_string();
        let by_delegate = self.by_delegate.read().await;
        let delegations_guard = self.delegations.read().await;

        let mut result = Vec::new();

        if let Some(delegation_ids) = by_delegate.get(&delegate_key) {
            for id in delegation_ids {
                if let Some(delegation) = delegations_guard.get(id) {
                    if delegation.is_active() {
                        result.push(delegation.clone());
                    }
                }
            }
        }

        Ok(result)
    }

    /// List all delegations created by a delegator
    pub async fn list_delegations_by_delegator(
        &self,
        delegator: &Subject,
    ) -> Result<Vec<Delegation>> {
        let delegator_key = delegator.to_string();
        let by_delegator = self.by_delegator.read().await;
        let delegations_guard = self.delegations.read().await;

        let mut result = Vec::new();

        if let Some(delegation_ids) = by_delegator.get(&delegator_key) {
            for id in delegation_ids {
                if let Some(delegation) = delegations_guard.get(id) {
                    result.push(delegation.clone());
                }
            }
        }

        Ok(result)
    }

    /// Clean up expired and revoked delegations
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let mut delegations = self.delegations.write().await;
        let expired_ids: Vec<String> = delegations
            .iter()
            .filter(|(_, d)| !d.is_active())
            .map(|(id, _)| id.clone())
            .collect();

        for id in &expired_ids {
            delegations.remove(id);
        }

        // Clean up indices
        let mut by_delegate = self.by_delegate.write().await;
        let mut by_delegator = self.by_delegator.write().await;

        for entries in by_delegate.values_mut() {
            entries.retain(|id| !expired_ids.contains(id));
        }

        for entries in by_delegator.values_mut() {
            entries.retain(|id| !expired_ids.contains(id));
        }

        Ok(expired_ids.len())
    }
}

impl Default for DelegationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[tokio::test]
    async fn test_delegation_creation() {
        let manager = DelegationManager::new();

        let delegation = Delegation::new(
            Subject::User("alice".to_string()),
            Subject::User("bob".to_string()),
            "document",
            "123",
            "editor",
        );

        let id = manager.create_delegation(delegation).await.unwrap();
        assert!(!id.is_empty());

        let retrieved = manager.get_delegation(&id).await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_delegation_check() {
        let manager = DelegationManager::new();

        let delegation = Delegation::new(
            Subject::User("alice".to_string()),
            Subject::User("bob".to_string()),
            "document",
            "123",
            "editor",
        );

        manager.create_delegation(delegation).await.unwrap();

        let has_delegation = manager
            .check_delegation(
                &Subject::User("bob".to_string()),
                "document",
                "123",
                "editor",
            )
            .await
            .unwrap();

        assert!(has_delegation);

        let no_delegation = manager
            .check_delegation(
                &Subject::User("charlie".to_string()),
                "document",
                "123",
                "editor",
            )
            .await
            .unwrap();

        assert!(!no_delegation);
    }

    #[tokio::test]
    async fn test_delegation_expiration() {
        let manager = DelegationManager::new();

        // Create delegation that expired 1 hour ago
        let delegation = Delegation::new(
            Subject::User("alice".to_string()),
            Subject::User("bob".to_string()),
            "document",
            "123",
            "editor",
        )
        .with_expiration(Utc::now() - Duration::hours(1));

        manager.create_delegation(delegation).await.unwrap();

        let has_delegation = manager
            .check_delegation(
                &Subject::User("bob".to_string()),
                "document",
                "123",
                "editor",
            )
            .await
            .unwrap();

        assert!(!has_delegation);
    }

    #[tokio::test]
    async fn test_delegation_revocation() {
        let manager = DelegationManager::new();

        let delegation = Delegation::new(
            Subject::User("alice".to_string()),
            Subject::User("bob".to_string()),
            "document",
            "123",
            "editor",
        );

        let id = manager.create_delegation(delegation).await.unwrap();

        // Should have delegation
        let has_delegation = manager
            .check_delegation(
                &Subject::User("bob".to_string()),
                "document",
                "123",
                "editor",
            )
            .await
            .unwrap();
        assert!(has_delegation);

        // Revoke delegation
        manager.revoke_delegation(&id).await.unwrap();

        // Should not have delegation anymore
        let has_delegation = manager
            .check_delegation(
                &Subject::User("bob".to_string()),
                "document",
                "123",
                "editor",
            )
            .await
            .unwrap();
        assert!(!has_delegation);
    }

    #[tokio::test]
    async fn test_list_delegations() {
        let manager = DelegationManager::new();

        let delegation1 = Delegation::new(
            Subject::User("alice".to_string()),
            Subject::User("bob".to_string()),
            "document",
            "123",
            "editor",
        );

        let delegation2 = Delegation::new(
            Subject::User("alice".to_string()),
            Subject::User("bob".to_string()),
            "document",
            "456",
            "viewer",
        );

        manager.create_delegation(delegation1).await.unwrap();
        manager.create_delegation(delegation2).await.unwrap();

        let delegations = manager
            .list_delegations_for_delegate(&Subject::User("bob".to_string()))
            .await
            .unwrap();

        assert_eq!(delegations.len(), 2);
    }

    #[test]
    fn test_delegation_active_status() {
        let mut delegation = Delegation::new(
            Subject::User("alice".to_string()),
            Subject::User("bob".to_string()),
            "document",
            "123",
            "editor",
        );

        assert!(delegation.is_active());

        delegation.revoke();
        assert!(!delegation.is_active());
    }
}
