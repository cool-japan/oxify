//! Approval storage for human-in-the-loop workflows
//!
//! This module provides storage and management for pending approvals.

use crate::ExecutionContext;
use oxify_model::{ApprovalConfig, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Unique identifier for an approval request
pub type ApprovalId = Uuid;

/// Status of an approval request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    TimedOut,
}

/// An approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique ID for this approval
    pub id: ApprovalId,

    /// Workflow execution ID
    pub execution_id: String,

    /// Node ID requesting approval
    pub node_id: NodeId,

    /// Approval configuration
    pub config: ApprovalConfig,

    /// Current status
    pub status: ApprovalStatus,

    /// Execution context at approval time
    pub context: serde_json::Value,

    /// Timestamp when approval was requested
    pub requested_at: std::time::SystemTime,

    /// Timestamp when approval was resolved (approved/rejected)
    pub resolved_at: Option<std::time::SystemTime>,

    /// Who resolved the approval (user ID)
    pub resolved_by: Option<String>,

    /// Comments/notes from the approver
    pub comments: Option<String>,
}

impl ApprovalRequest {
    /// Create a new pending approval request
    pub fn new(
        execution_id: String,
        node_id: NodeId,
        config: ApprovalConfig,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            execution_id,
            node_id,
            config,
            status: ApprovalStatus::Pending,
            context: serde_json::to_value(context).unwrap_or(serde_json::Value::Null),
            requested_at: std::time::SystemTime::now(),
            resolved_at: None,
            resolved_by: None,
            comments: None,
        }
    }

    /// Check if approval has timed out
    pub fn is_timed_out(&self) -> bool {
        if let Some(timeout_seconds) = self.config.timeout_seconds {
            if let Ok(elapsed) = self.requested_at.elapsed() {
                return elapsed.as_secs() > timeout_seconds;
            }
        }
        false
    }

    /// Approve this request
    pub fn approve(&mut self, user_id: String, comments: Option<String>) {
        self.status = ApprovalStatus::Approved;
        self.resolved_at = Some(std::time::SystemTime::now());
        self.resolved_by = Some(user_id);
        self.comments = comments;
    }

    /// Reject this request
    pub fn reject(&mut self, user_id: String, comments: Option<String>) {
        self.status = ApprovalStatus::Rejected;
        self.resolved_at = Some(std::time::SystemTime::now());
        self.resolved_by = Some(user_id);
        self.comments = comments;
    }

    /// Mark as timed out
    pub fn mark_timed_out(&mut self) {
        self.status = ApprovalStatus::TimedOut;
        self.resolved_at = Some(std::time::SystemTime::now());
    }
}

/// In-memory approval store
#[derive(Debug, Clone)]
pub struct ApprovalStore {
    approvals: Arc<RwLock<HashMap<ApprovalId, ApprovalRequest>>>,
}

impl ApprovalStore {
    /// Create a new approval store
    pub fn new() -> Self {
        Self {
            approvals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a new approval request
    pub fn add(&self, request: ApprovalRequest) -> ApprovalId {
        let id = request.id;
        self.approvals.write().unwrap_or_else(|e| e.into_inner()).insert(id, request);
        id
    }

    /// Get an approval request by ID
    pub fn get(&self, id: ApprovalId) -> Option<ApprovalRequest> {
        self.approvals.read().unwrap_or_else(|e| e.into_inner()).get(&id).cloned()
    }

    /// List all pending approvals
    pub fn list_pending(&self) -> Vec<ApprovalRequest> {
        self.approvals
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|a| a.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    /// List pending approvals for a specific execution
    pub fn list_pending_for_execution(&self, execution_id: &str) -> Vec<ApprovalRequest> {
        self.approvals
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|a| a.execution_id == execution_id && a.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    /// Update an approval request
    pub fn update(&self, request: ApprovalRequest) {
        self.approvals.write().unwrap_or_else(|e| e.into_inner()).insert(request.id, request);
    }

    /// Approve a request
    pub fn approve(&self, id: ApprovalId, user_id: String, comments: Option<String>) -> bool {
        if let Some(mut request) = self.get(id) {
            request.approve(user_id, comments);
            self.update(request);
            true
        } else {
            false
        }
    }

    /// Reject a request
    pub fn reject(&self, id: ApprovalId, user_id: String, comments: Option<String>) -> bool {
        if let Some(mut request) = self.get(id) {
            request.reject(user_id, comments);
            self.update(request);
            true
        } else {
            false
        }
    }

    /// Clean up old approvals (resolved more than N seconds ago)
    pub fn cleanup_old(&self, max_age_seconds: u64) {
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(max_age_seconds);

        self.approvals.write().unwrap_or_else(|e| e.into_inner()).retain(|_, request| {
            if let Some(resolved_at) = request.resolved_at {
                resolved_at > cutoff
            } else {
                true // Keep pending approvals
            }
        });
    }
}

impl Default for ApprovalStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::NodeId;

    #[test]
    fn test_approval_request_creation() {
        use uuid::Uuid;

        let config = ApprovalConfig {
            message: "Approve deployment".to_string(),
            description: Some("Deploy to production".to_string()),
            approvers: vec!["admin".to_string()],
            timeout_seconds: Some(3600),
            context_data: serde_json::json!({"env": "production"}),
        };

        let ctx = ExecutionContext::new(Uuid::new_v4());
        let request = ApprovalRequest::new("exec123".to_string(), NodeId::new_v4(), config, &ctx);

        assert_eq!(request.status, ApprovalStatus::Pending);
        assert!(request.resolved_at.is_none());
        assert!(request.resolved_by.is_none());
    }

    #[test]
    fn test_approval_workflow() {
        use uuid::Uuid;

        let config = ApprovalConfig {
            message: "Approve".to_string(),
            description: None,
            approvers: vec![],
            timeout_seconds: None,
            context_data: serde_json::Value::Null,
        };

        let ctx = ExecutionContext::new(Uuid::new_v4());
        let mut request =
            ApprovalRequest::new("exec123".to_string(), NodeId::new_v4(), config, &ctx);

        request.approve("user1".to_string(), Some("LGTM".to_string()));

        assert_eq!(request.status, ApprovalStatus::Approved);
        assert!(request.resolved_at.is_some());
        assert_eq!(request.resolved_by, Some("user1".to_string()));
        assert_eq!(request.comments, Some("LGTM".to_string()));
    }

    #[test]
    fn test_approval_store() {
        use uuid::Uuid;

        let store = ApprovalStore::new();

        let config = ApprovalConfig {
            message: "Approve".to_string(),
            description: None,
            approvers: vec![],
            timeout_seconds: None,
            context_data: serde_json::Value::Null,
        };

        let ctx = ExecutionContext::new(Uuid::new_v4());
        let request = ApprovalRequest::new("exec123".to_string(), NodeId::new_v4(), config, &ctx);

        let id = store.add(request.clone());

        // Should be in pending list
        let pending = store.list_pending();
        assert_eq!(pending.len(), 1);

        // Approve it
        store.approve(id, "user1".to_string(), None);

        // Should no longer be in pending list
        let pending = store.list_pending();
        assert_eq!(pending.len(), 0);

        // Should be retrievable
        let retrieved = store.get(id).unwrap();
        assert_eq!(retrieved.status, ApprovalStatus::Approved);
    }
}
