//! Checkpoint and recovery system for workflow execution
//!
//! This module provides checkpointing capabilities for long-running workflows,
//! enabling pause/resume and recovery from failures.

use oxify_model::{ExecutionContext, NodeExecutionResult, NodeId, WorkflowId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Unique identifier for a checkpoint
pub type CheckpointId = Uuid;

/// Execution checkpoint containing all state needed to resume
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCheckpoint {
    /// Checkpoint ID
    pub id: CheckpointId,

    /// Workflow ID
    pub workflow_id: WorkflowId,

    /// Execution ID
    pub execution_id: Uuid,

    /// Execution context (variables, results)
    pub context: ExecutionContext,

    /// Completed nodes
    pub completed_nodes: Vec<NodeId>,

    /// Node execution results
    pub node_results: HashMap<NodeId, NodeExecutionResult>,

    /// Current execution level
    pub current_level: usize,

    /// Paused status
    pub paused: bool,

    /// Timestamp when checkpoint was created
    pub created_at: std::time::SystemTime,

    /// Reason for checkpoint (e.g., "manual_pause", "failure", "periodic")
    pub reason: String,
}

impl ExecutionCheckpoint {
    /// Create a new checkpoint
    pub fn new(
        workflow_id: WorkflowId,
        execution_id: Uuid,
        context: ExecutionContext,
        completed_nodes: Vec<NodeId>,
        current_level: usize,
        reason: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            workflow_id,
            execution_id,
            context,
            completed_nodes,
            node_results: HashMap::new(),
            current_level,
            paused: false,
            created_at: std::time::SystemTime::now(),
            reason,
        }
    }

    /// Add a node result
    pub fn add_node_result(&mut self, node_id: NodeId, result: NodeExecutionResult) {
        self.node_results.insert(node_id, result);
    }

    /// Check if a node has been completed
    pub fn is_node_completed(&self, node_id: NodeId) -> bool {
        self.completed_nodes.contains(&node_id)
    }
}

/// Checkpoint storage interface
pub trait CheckpointStore: Send + Sync {
    /// Save a checkpoint
    fn save(&self, checkpoint: &ExecutionCheckpoint) -> Result<CheckpointId, String>;

    /// Load a checkpoint by ID
    fn load(&self, id: CheckpointId) -> Result<ExecutionCheckpoint, String>;

    /// Load the latest checkpoint for an execution
    fn load_latest(&self, execution_id: Uuid) -> Result<ExecutionCheckpoint, String>;

    /// List all checkpoints for a workflow
    fn list_by_workflow(&self, workflow_id: WorkflowId) -> Vec<ExecutionCheckpoint>;

    /// List all checkpoints for an execution
    fn list_by_execution(&self, execution_id: Uuid) -> Vec<ExecutionCheckpoint>;

    /// Delete a checkpoint
    fn delete(&self, id: CheckpointId) -> Result<(), String>;

    /// Delete all checkpoints for an execution
    fn delete_by_execution(&self, execution_id: Uuid) -> Result<(), String>;
}

/// File-based checkpoint storage
pub struct FileCheckpointStore {
    base_path: PathBuf,
    checkpoints: Arc<RwLock<HashMap<CheckpointId, ExecutionCheckpoint>>>,
}

impl FileCheckpointStore {
    /// Create a new file-based checkpoint store
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self, String> {
        let base_path = base_path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        fs::create_dir_all(&base_path)
            .map_err(|e| format!("Failed to create checkpoint directory: {}", e))?;

        let mut store = Self {
            base_path,
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
        };

        // Load existing checkpoints
        store.load_all()?;

        Ok(store)
    }

    /// Load all checkpoints from disk
    fn load_all(&mut self) -> Result<(), String> {
        let entries = fs::read_dir(&self.base_path)
            .map_err(|e| format!("Failed to read checkpoint directory: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(checkpoint) = serde_json::from_str::<ExecutionCheckpoint>(&content) {
                        self.checkpoints
                            .write()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(checkpoint.id, checkpoint);
                    }
                }
            }
        }

        Ok(())
    }

    /// Get checkpoint file path
    fn checkpoint_path(&self, id: CheckpointId) -> PathBuf {
        self.base_path.join(format!("{}.json", id))
    }
}

impl CheckpointStore for FileCheckpointStore {
    fn save(&self, checkpoint: &ExecutionCheckpoint) -> Result<CheckpointId, String> {
        let path = self.checkpoint_path(checkpoint.id);

        let json = serde_json::to_string_pretty(checkpoint)
            .map_err(|e| format!("Failed to serialize checkpoint: {}", e))?;

        fs::write(&path, json).map_err(|e| format!("Failed to write checkpoint file: {}", e))?;

        self.checkpoints
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(checkpoint.id, checkpoint.clone());

        tracing::info!(
            "Saved checkpoint {} for execution {}",
            checkpoint.id,
            checkpoint.execution_id
        );

        Ok(checkpoint.id)
    }

    fn load(&self, id: CheckpointId) -> Result<ExecutionCheckpoint, String> {
        self.checkpoints
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("Checkpoint {} not found", id))
    }

    fn load_latest(&self, execution_id: Uuid) -> Result<ExecutionCheckpoint, String> {
        let checkpoints = self.list_by_execution(execution_id);

        checkpoints
            .into_iter()
            .max_by_key(|c| c.created_at)
            .ok_or_else(|| format!("No checkpoints found for execution {}", execution_id))
    }

    fn list_by_workflow(&self, workflow_id: WorkflowId) -> Vec<ExecutionCheckpoint> {
        self.checkpoints
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|c| c.workflow_id == workflow_id)
            .cloned()
            .collect()
    }

    fn list_by_execution(&self, execution_id: Uuid) -> Vec<ExecutionCheckpoint> {
        self.checkpoints
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|c| c.execution_id == execution_id)
            .cloned()
            .collect()
    }

    fn delete(&self, id: CheckpointId) -> Result<(), String> {
        let path = self.checkpoint_path(id);

        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete checkpoint file: {}", e))?;
        }

        self.checkpoints
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);

        tracing::info!("Deleted checkpoint {}", id);

        Ok(())
    }

    fn delete_by_execution(&self, execution_id: Uuid) -> Result<(), String> {
        let checkpoints = self.list_by_execution(execution_id);

        for checkpoint in checkpoints {
            self.delete(checkpoint.id)?;
        }

        Ok(())
    }
}

/// In-memory checkpoint storage (for testing)
pub struct InMemoryCheckpointStore {
    checkpoints: Arc<RwLock<HashMap<CheckpointId, ExecutionCheckpoint>>>,
}

impl InMemoryCheckpointStore {
    /// Create a new in-memory checkpoint store
    pub fn new() -> Self {
        Self {
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryCheckpointStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointStore for InMemoryCheckpointStore {
    fn save(&self, checkpoint: &ExecutionCheckpoint) -> Result<CheckpointId, String> {
        self.checkpoints
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(checkpoint.id, checkpoint.clone());

        Ok(checkpoint.id)
    }

    fn load(&self, id: CheckpointId) -> Result<ExecutionCheckpoint, String> {
        self.checkpoints
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("Checkpoint {} not found", id))
    }

    fn load_latest(&self, execution_id: Uuid) -> Result<ExecutionCheckpoint, String> {
        let checkpoints = self.list_by_execution(execution_id);

        checkpoints
            .into_iter()
            .max_by_key(|c| c.created_at)
            .ok_or_else(|| format!("No checkpoints found for execution {}", execution_id))
    }

    fn list_by_workflow(&self, workflow_id: WorkflowId) -> Vec<ExecutionCheckpoint> {
        self.checkpoints
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|c| c.workflow_id == workflow_id)
            .cloned()
            .collect()
    }

    fn list_by_execution(&self, execution_id: Uuid) -> Vec<ExecutionCheckpoint> {
        self.checkpoints
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|c| c.execution_id == execution_id)
            .cloned()
            .collect()
    }

    fn delete(&self, id: CheckpointId) -> Result<(), String> {
        self.checkpoints
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        Ok(())
    }

    fn delete_by_execution(&self, execution_id: Uuid) -> Result<(), String> {
        let checkpoints = self.list_by_execution(execution_id);

        for checkpoint in checkpoints {
            self.delete(checkpoint.id)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_creation() {
        let ctx = ExecutionContext::new(Uuid::new_v4());
        let checkpoint = ExecutionCheckpoint::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            ctx,
            vec![],
            0,
            "test".to_string(),
        );

        assert_eq!(checkpoint.current_level, 0);
        assert!(!checkpoint.paused);
        assert_eq!(checkpoint.reason, "test");
    }

    #[test]
    fn test_in_memory_store() {
        let store = InMemoryCheckpointStore::new();
        let ctx = ExecutionContext::new(Uuid::new_v4());
        let execution_id = Uuid::new_v4();

        let checkpoint = ExecutionCheckpoint::new(
            Uuid::new_v4(),
            execution_id,
            ctx,
            vec![],
            0,
            "test".to_string(),
        );

        let id = store.save(&checkpoint).unwrap();

        let loaded = store.load(id).unwrap();
        assert_eq!(loaded.id, checkpoint.id);
        assert_eq!(loaded.execution_id, execution_id);

        let checkpoints = store.list_by_execution(execution_id);
        assert_eq!(checkpoints.len(), 1);

        store.delete(id).unwrap();
        assert!(store.load(id).is_err());
    }

    #[test]
    fn test_load_latest() {
        let store = InMemoryCheckpointStore::new();
        let ctx = ExecutionContext::new(Uuid::new_v4());
        let execution_id = Uuid::new_v4();

        // Create multiple checkpoints
        for i in 0..3 {
            let mut checkpoint = ExecutionCheckpoint::new(
                Uuid::new_v4(),
                execution_id,
                ctx.clone(),
                vec![],
                i,
                format!("checkpoint_{}", i),
            );

            // Ensure different timestamps
            std::thread::sleep(std::time::Duration::from_millis(10));
            checkpoint.created_at = std::time::SystemTime::now();

            store.save(&checkpoint).unwrap();
        }

        let latest = store.load_latest(execution_id).unwrap();
        assert_eq!(latest.current_level, 2);
    }
}
