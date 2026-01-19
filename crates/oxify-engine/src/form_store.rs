//! Form submission storage for human-in-the-loop workflows
//!
//! This module provides storage and management for pending form submissions.

use crate::ExecutionContext;
use oxify_model::{FormConfig, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Unique identifier for a form submission request
pub type FormId = Uuid;

/// Status of a form submission request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FormStatus {
    Pending,
    Submitted,
    TimedOut,
}

/// A form submission request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormSubmissionRequest {
    /// Unique ID for this form
    pub id: FormId,

    /// Workflow execution ID
    pub execution_id: String,

    /// Node ID requesting form submission
    pub node_id: NodeId,

    /// Form configuration
    pub config: FormConfig,

    /// Current status
    pub status: FormStatus,

    /// Execution context at form request time
    pub context: serde_json::Value,

    /// Timestamp when form was requested
    pub requested_at: std::time::SystemTime,

    /// Timestamp when form was submitted
    pub submitted_at: Option<std::time::SystemTime>,

    /// Who submitted the form (user ID)
    pub submitted_by: Option<String>,

    /// Form data submitted by user
    pub form_data: Option<serde_json::Value>,
}

impl FormSubmissionRequest {
    /// Create a new pending form submission request
    pub fn new(
        execution_id: String,
        node_id: NodeId,
        config: FormConfig,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            execution_id,
            node_id,
            config,
            status: FormStatus::Pending,
            context: serde_json::to_value(context).unwrap_or(serde_json::Value::Null),
            requested_at: std::time::SystemTime::now(),
            submitted_at: None,
            submitted_by: None,
            form_data: None,
        }
    }

    /// Check if form submission has timed out
    pub fn is_timed_out(&self) -> bool {
        if let Some(timeout_seconds) = self.config.timeout_seconds {
            if let Ok(elapsed) = self.requested_at.elapsed() {
                return elapsed.as_secs() > timeout_seconds;
            }
        }
        false
    }

    /// Submit this form
    pub fn submit(&mut self, user_id: String, form_data: serde_json::Value) {
        self.status = FormStatus::Submitted;
        self.submitted_at = Some(std::time::SystemTime::now());
        self.submitted_by = Some(user_id);
        self.form_data = Some(form_data);
    }

    /// Mark as timed out
    pub fn mark_timed_out(&mut self) {
        self.status = FormStatus::TimedOut;
        self.submitted_at = Some(std::time::SystemTime::now());
    }
}

/// In-memory form submission store
#[derive(Debug, Clone)]
pub struct FormStore {
    forms: Arc<RwLock<HashMap<FormId, FormSubmissionRequest>>>,
}

impl FormStore {
    /// Create a new form store
    pub fn new() -> Self {
        Self {
            forms: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a new form submission request
    pub fn add(&self, request: FormSubmissionRequest) -> FormId {
        let id = request.id;
        self.forms.write().unwrap().insert(id, request);
        id
    }

    /// Get a form submission request by ID
    pub fn get(&self, id: FormId) -> Option<FormSubmissionRequest> {
        self.forms.read().unwrap().get(&id).cloned()
    }

    /// List all pending form submissions
    pub fn list_pending(&self) -> Vec<FormSubmissionRequest> {
        self.forms
            .read()
            .unwrap()
            .values()
            .filter(|f| f.status == FormStatus::Pending)
            .cloned()
            .collect()
    }

    /// List pending form submissions for a specific execution
    pub fn list_pending_for_execution(&self, execution_id: &str) -> Vec<FormSubmissionRequest> {
        self.forms
            .read()
            .unwrap()
            .values()
            .filter(|f| f.execution_id == execution_id && f.status == FormStatus::Pending)
            .cloned()
            .collect()
    }

    /// Update a form submission request
    pub fn update(&self, request: FormSubmissionRequest) {
        self.forms.write().unwrap().insert(request.id, request);
    }

    /// Submit a form
    pub fn submit(&self, id: FormId, user_id: String, form_data: serde_json::Value) -> bool {
        if let Some(mut request) = self.get(id) {
            request.submit(user_id, form_data);
            self.update(request);
            true
        } else {
            false
        }
    }

    /// Clean up old form submissions (resolved more than N seconds ago)
    pub fn cleanup_old(&self, max_age_seconds: u64) {
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(max_age_seconds);

        self.forms.write().unwrap().retain(|_, request| {
            if let Some(submitted_at) = request.submitted_at {
                submitted_at > cutoff
            } else {
                true // Keep pending forms
            }
        });
    }
}

impl Default for FormStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{FormField, FormFieldType, NodeId};

    #[test]
    fn test_form_request_creation() {
        use uuid::Uuid;

        let config = FormConfig {
            title: "User Information".to_string(),
            description: Some("Please provide your information".to_string()),
            fields: vec![FormField {
                id: "name".to_string(),
                label: "Name".to_string(),
                field_type: FormFieldType::Text,
                required: true,
                default_value: None,
                validation: None,
                options: Vec::new(),
            }],
            timeout_seconds: Some(3600),
            allowed_submitters: vec!["user1".to_string()],
        };

        let ctx = ExecutionContext::new(Uuid::new_v4());
        let request =
            FormSubmissionRequest::new("exec123".to_string(), NodeId::new_v4(), config, &ctx);

        assert_eq!(request.status, FormStatus::Pending);
        assert!(request.submitted_at.is_none());
        assert!(request.submitted_by.is_none());
    }

    #[test]
    fn test_form_submission_workflow() {
        use uuid::Uuid;

        let config = FormConfig {
            title: "Test Form".to_string(),
            description: None,
            fields: vec![],
            timeout_seconds: None,
            allowed_submitters: vec![],
        };

        let ctx = ExecutionContext::new(Uuid::new_v4());
        let mut request =
            FormSubmissionRequest::new("exec123".to_string(), NodeId::new_v4(), config, &ctx);

        let form_data = serde_json::json!({"name": "John Doe", "email": "john@example.com"});
        request.submit("user1".to_string(), form_data.clone());

        assert_eq!(request.status, FormStatus::Submitted);
        assert!(request.submitted_at.is_some());
        assert_eq!(request.submitted_by, Some("user1".to_string()));
        assert_eq!(request.form_data, Some(form_data));
    }

    #[test]
    fn test_form_store() {
        use uuid::Uuid;

        let store = FormStore::new();

        let config = FormConfig {
            title: "Test Form".to_string(),
            description: None,
            fields: vec![],
            timeout_seconds: None,
            allowed_submitters: vec![],
        };

        let ctx = ExecutionContext::new(Uuid::new_v4());
        let request =
            FormSubmissionRequest::new("exec123".to_string(), NodeId::new_v4(), config, &ctx);

        let id = store.add(request.clone());

        // Should be in pending list
        let pending = store.list_pending();
        assert_eq!(pending.len(), 1);

        // Submit it
        let form_data = serde_json::json!({"test": "data"});
        store.submit(id, "user1".to_string(), form_data.clone());

        // Should no longer be in pending list
        let pending = store.list_pending();
        assert_eq!(pending.len(), 0);

        // Should be retrievable
        let retrieved = store.get(id).unwrap();
        assert_eq!(retrieved.status, FormStatus::Submitted);
        assert_eq!(retrieved.form_data, Some(form_data));
    }
}
