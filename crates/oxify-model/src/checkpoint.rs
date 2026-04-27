//! Checkpoint configuration and storage abstraction
//!
//! This module provides checkpoint frequency configuration and storage traits
//! for saving and restoring workflow execution state.

use crate::{ExecutionCheckpoint, ExecutionId, WorkflowId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Configuration for checkpoint frequency and behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CheckpointConfig {
    /// Enable automatic checkpointing
    pub enabled: bool,

    /// Checkpoint frequency strategy
    pub frequency: CheckpointFrequency,

    /// Maximum number of checkpoints to retain per execution
    pub max_checkpoints: usize,

    /// Automatically checkpoint after long-running nodes (threshold in ms)
    pub auto_checkpoint_threshold_ms: Option<u64>,

    /// Compress checkpoint data
    pub compress: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            frequency: CheckpointFrequency::EveryNNodes(5),
            max_checkpoints: 10,
            auto_checkpoint_threshold_ms: Some(60000), // 1 minute
            compress: false,
        }
    }
}

/// Checkpoint frequency strategies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum CheckpointFrequency {
    /// Checkpoint after every N nodes complete
    EveryNNodes(usize),

    /// Checkpoint at specific time intervals (in seconds)
    TimeInterval(u64),

    /// Checkpoint before specific node types (e.g., before expensive LLM calls)
    BeforeNodeTypes(Vec<String>),

    /// Manual checkpointing only (no automatic checkpoints)
    Manual,

    /// Checkpoint at every node completion (maximum safety, higher overhead)
    Always,
}

/// Storage abstraction for checkpoints
pub trait CheckpointStorage: Send + Sync {
    /// Save a checkpoint
    fn save_checkpoint(
        &self,
        execution_id: ExecutionId,
        checkpoint: &ExecutionCheckpoint,
    ) -> Result<CheckpointId, CheckpointError>;

    /// Load the latest checkpoint for an execution
    fn load_latest_checkpoint(
        &self,
        execution_id: ExecutionId,
    ) -> Result<Option<ExecutionCheckpoint>, CheckpointError>;

    /// Load a specific checkpoint by ID
    fn load_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<ExecutionCheckpoint>, CheckpointError>;

    /// List all checkpoints for an execution
    fn list_checkpoints(
        &self,
        execution_id: ExecutionId,
    ) -> Result<Vec<CheckpointMetadata>, CheckpointError>;

    /// Delete old checkpoints beyond retention limit
    fn prune_checkpoints(
        &self,
        execution_id: ExecutionId,
        keep_count: usize,
    ) -> Result<usize, CheckpointError>;

    /// Delete all checkpoints for an execution
    fn delete_checkpoints(&self, execution_id: ExecutionId) -> Result<usize, CheckpointError>;
}

/// Unique identifier for a checkpoint
pub type CheckpointId = uuid::Uuid;

/// Metadata about a stored checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CheckpointMetadata {
    /// Unique checkpoint identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = "uuid"))]
    pub id: CheckpointId,

    /// Execution this checkpoint belongs to
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = "uuid"))]
    pub execution_id: ExecutionId,

    /// Workflow being executed
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = "uuid"))]
    pub workflow_id: WorkflowId,

    /// When the checkpoint was created
    pub created_at: DateTime<Utc>,

    /// Number of nodes completed at checkpoint time
    pub completed_node_count: usize,

    /// Size of checkpoint data in bytes
    pub size_bytes: usize,

    /// Whether the checkpoint is compressed
    pub compressed: bool,
}

/// Errors that can occur during checkpoint operations
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("Checkpoint not found: {0}")]
    NotFound(CheckpointId),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Decompression error: {0}")]
    DecompressionError(String),

    #[error("Invalid checkpoint data: {0}")]
    InvalidData(String),
}

/// In-memory checkpoint storage implementation (for testing/development)
#[derive(Debug, Default)]
pub struct InMemoryCheckpointStorage {
    checkpoints: std::sync::RwLock<HashMap<CheckpointId, (ExecutionId, ExecutionCheckpoint)>>,
    metadata: std::sync::RwLock<HashMap<CheckpointId, CheckpointMetadata>>,
}

impl InMemoryCheckpointStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CheckpointStorage for InMemoryCheckpointStorage {
    fn save_checkpoint(
        &self,
        execution_id: ExecutionId,
        checkpoint: &ExecutionCheckpoint,
    ) -> Result<CheckpointId, CheckpointError> {
        let checkpoint_id = uuid::Uuid::new_v4();

        // Serialize to estimate size
        let data = serde_json::to_vec(checkpoint)
            .map_err(|e| CheckpointError::SerializationError(e.to_string()))?;

        let metadata = CheckpointMetadata {
            id: checkpoint_id,
            execution_id,
            workflow_id: uuid::Uuid::new_v4(), // Would be passed from context
            created_at: checkpoint.timestamp,
            completed_node_count: checkpoint.completed_nodes.len(),
            size_bytes: data.len(),
            compressed: false,
        };

        self.checkpoints
            .write()
            .unwrap()
            .insert(checkpoint_id, (execution_id, checkpoint.clone()));
        self.metadata
            .write()
            .unwrap()
            .insert(checkpoint_id, metadata);

        Ok(checkpoint_id)
    }

    fn load_latest_checkpoint(
        &self,
        execution_id: ExecutionId,
    ) -> Result<Option<ExecutionCheckpoint>, CheckpointError> {
        let checkpoints = self.checkpoints.read().unwrap();

        // Find latest checkpoint for this execution
        let latest = checkpoints
            .iter()
            .filter(|(_, (exec_id, _))| *exec_id == execution_id)
            .map(|(id, (_, checkpoint))| (*id, checkpoint))
            .max_by_key(|(_, checkpoint)| checkpoint.timestamp);

        Ok(latest.map(|(_, checkpoint)| checkpoint.clone()))
    }

    fn load_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<ExecutionCheckpoint>, CheckpointError> {
        let checkpoints = self.checkpoints.read().unwrap();
        Ok(checkpoints
            .get(&checkpoint_id)
            .map(|(_, checkpoint)| checkpoint.clone()))
    }

    fn list_checkpoints(
        &self,
        execution_id: ExecutionId,
    ) -> Result<Vec<CheckpointMetadata>, CheckpointError> {
        let metadata = self.metadata.read().unwrap();
        let mut list: Vec<_> = metadata
            .values()
            .filter(|m| m.execution_id == execution_id)
            .cloned()
            .collect();

        // Sort by creation time (newest first)
        list.sort_by_key(|x| std::cmp::Reverse(x.created_at));

        Ok(list)
    }

    fn prune_checkpoints(
        &self,
        execution_id: ExecutionId,
        keep_count: usize,
    ) -> Result<usize, CheckpointError> {
        let list = self.list_checkpoints(execution_id)?;

        if list.len() <= keep_count {
            return Ok(0);
        }

        let to_delete = &list[keep_count..];
        let mut checkpoints = self.checkpoints.write().unwrap();
        let mut metadata = self.metadata.write().unwrap();

        for meta in to_delete {
            checkpoints.remove(&meta.id);
            metadata.remove(&meta.id);
        }

        Ok(to_delete.len())
    }

    fn delete_checkpoints(&self, execution_id: ExecutionId) -> Result<usize, CheckpointError> {
        let list = self.list_checkpoints(execution_id)?;
        let mut checkpoints = self.checkpoints.write().unwrap();
        let mut metadata = self.metadata.write().unwrap();

        for meta in &list {
            checkpoints.remove(&meta.id);
            metadata.remove(&meta.id);
        }

        Ok(list.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExecutionState;

    #[test]
    fn test_checkpoint_config_default() {
        let config = CheckpointConfig::default();
        assert!(config.enabled);
        assert_eq!(config.frequency, CheckpointFrequency::EveryNNodes(5));
        assert_eq!(config.max_checkpoints, 10);
    }

    #[test]
    fn test_checkpoint_frequency_variants() {
        let freq1 = CheckpointFrequency::EveryNNodes(10);
        let freq2 = CheckpointFrequency::TimeInterval(60);
        let freq3 = CheckpointFrequency::Manual;
        let freq4 = CheckpointFrequency::Always;

        assert_eq!(freq1, CheckpointFrequency::EveryNNodes(10));
        assert_ne!(freq2, freq3);
        assert_ne!(freq3, freq4);
    }

    #[test]
    fn test_in_memory_storage_save_load() {
        let storage = InMemoryCheckpointStorage::new();
        let execution_id = uuid::Uuid::new_v4();

        let checkpoint = ExecutionCheckpoint {
            timestamp: Utc::now(),
            completed_nodes: vec![uuid::Uuid::new_v4()],
            variables: HashMap::new(),
            state: ExecutionState::Running,
        };

        // Save checkpoint
        let checkpoint_id = storage.save_checkpoint(execution_id, &checkpoint).unwrap();

        // Load checkpoint by ID
        let loaded = storage.load_checkpoint(checkpoint_id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().completed_nodes, checkpoint.completed_nodes);

        // Load latest checkpoint
        let latest = storage.load_latest_checkpoint(execution_id).unwrap();
        assert!(latest.is_some());
    }

    #[test]
    fn test_list_checkpoints() {
        let storage = InMemoryCheckpointStorage::new();
        let execution_id = uuid::Uuid::new_v4();

        // Create multiple checkpoints
        for i in 0..3 {
            let checkpoint = ExecutionCheckpoint {
                timestamp: Utc::now(),
                completed_nodes: vec![uuid::Uuid::new_v4(); i + 1],
                variables: HashMap::new(),
                state: ExecutionState::Running,
            };
            storage.save_checkpoint(execution_id, &checkpoint).unwrap();
        }

        let list = storage.list_checkpoints(execution_id).unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_prune_checkpoints() {
        let storage = InMemoryCheckpointStorage::new();
        let execution_id = uuid::Uuid::new_v4();

        // Create 5 checkpoints
        for _ in 0..5 {
            let checkpoint = ExecutionCheckpoint {
                timestamp: Utc::now(),
                completed_nodes: vec![uuid::Uuid::new_v4()],
                variables: HashMap::new(),
                state: ExecutionState::Running,
            };
            storage.save_checkpoint(execution_id, &checkpoint).unwrap();
        }

        // Prune to keep only 2
        let deleted = storage.prune_checkpoints(execution_id, 2).unwrap();
        assert_eq!(deleted, 3);

        let remaining = storage.list_checkpoints(execution_id).unwrap();
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn test_delete_all_checkpoints() {
        let storage = InMemoryCheckpointStorage::new();
        let execution_id = uuid::Uuid::new_v4();

        // Create checkpoints
        for _ in 0..3 {
            let checkpoint = ExecutionCheckpoint {
                timestamp: Utc::now(),
                completed_nodes: vec![uuid::Uuid::new_v4()],
                variables: HashMap::new(),
                state: ExecutionState::Running,
            };
            storage.save_checkpoint(execution_id, &checkpoint).unwrap();
        }

        // Delete all
        let deleted = storage.delete_checkpoints(execution_id).unwrap();
        assert_eq!(deleted, 3);

        let remaining = storage.list_checkpoints(execution_id).unwrap();
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn test_multiple_executions() {
        let storage = InMemoryCheckpointStorage::new();
        let exec1 = uuid::Uuid::new_v4();
        let exec2 = uuid::Uuid::new_v4();

        // Create checkpoints for two different executions
        for exec_id in [exec1, exec2] {
            for _ in 0..2 {
                let checkpoint = ExecutionCheckpoint {
                    timestamp: Utc::now(),
                    completed_nodes: vec![uuid::Uuid::new_v4()],
                    variables: HashMap::new(),
                    state: ExecutionState::Running,
                };
                storage.save_checkpoint(exec_id, &checkpoint).unwrap();
            }
        }

        // Each execution should have 2 checkpoints
        assert_eq!(storage.list_checkpoints(exec1).unwrap().len(), 2);
        assert_eq!(storage.list_checkpoints(exec2).unwrap().len(), 2);

        // Delete one execution's checkpoints
        storage.delete_checkpoints(exec1).unwrap();

        // exec1 should have no checkpoints, exec2 should still have 2
        assert_eq!(storage.list_checkpoints(exec1).unwrap().len(), 0);
        assert_eq!(storage.list_checkpoints(exec2).unwrap().len(), 2);
    }
}
