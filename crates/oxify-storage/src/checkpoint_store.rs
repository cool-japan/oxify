//! Database-backed checkpoint storage

use crate::{DatabasePool, Result};
use chrono::{DateTime, Utc};
use oxify_model::{ExecutionContext, NodeExecutionResult, NodeId, WorkflowId};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;

/// Execution checkpoint for pause/resume
#[derive(Debug, Clone)]
pub struct ExecutionCheckpoint {
    pub id: Uuid,
    pub workflow_id: WorkflowId,
    pub execution_id: Uuid,
    pub context: ExecutionContext,
    pub completed_nodes: Vec<NodeId>,
    pub node_results: HashMap<NodeId, NodeExecutionResult>,
    pub current_level: usize,
    pub paused: bool,
    pub created_at: DateTime<Utc>,
    pub reason: String,
}

impl ExecutionCheckpoint {
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
            created_at: Utc::now(),
            reason,
        }
    }

    pub fn add_node_result(&mut self, node_id: NodeId, result: NodeExecutionResult) {
        self.node_results.insert(node_id, result);
    }

    pub fn is_node_completed(&self, node_id: NodeId) -> bool {
        self.completed_nodes.contains(&node_id)
    }
}

/// Database checkpoint store
#[derive(Clone)]
pub struct DatabaseCheckpointStore {
    pool: DatabasePool,
}

impl DatabaseCheckpointStore {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Save a checkpoint
    pub async fn save(&self, checkpoint: &ExecutionCheckpoint) -> Result<Uuid> {
        let context_json = serde_json::to_value(&checkpoint.context)?;
        let completed_nodes_json = serde_json::to_value(&checkpoint.completed_nodes)?;
        let node_results_json = serde_json::to_value(&checkpoint.node_results)?;

        sqlx::query(
            r"
            INSERT INTO execution_checkpoints
            (id, workflow_id, execution_id, context, completed_nodes, node_results,
             current_level, paused, reason, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ",
        )
        .bind(checkpoint.id)
        .bind(checkpoint.workflow_id)
        .bind(checkpoint.execution_id)
        .bind(&context_json)
        .bind(&completed_nodes_json)
        .bind(&node_results_json)
        .bind(checkpoint.current_level as i32)
        .bind(checkpoint.paused)
        .bind(&checkpoint.reason)
        .bind(checkpoint.created_at)
        .execute(self.pool.pool())
        .await?;

        Ok(checkpoint.id)
    }

    /// Load a checkpoint by ID
    pub async fn load(&self, id: Uuid) -> Result<Option<ExecutionCheckpoint>> {
        let row = sqlx::query(
            r"
            SELECT id, workflow_id, execution_id, context, completed_nodes, node_results,
                   current_level, paused, reason, created_at
            FROM execution_checkpoints
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => {
                let context: ExecutionContext = serde_json::from_value(row.get("context"))?;
                let completed_nodes: Vec<NodeId> =
                    serde_json::from_value(row.get("completed_nodes"))?;
                let node_results: HashMap<NodeId, NodeExecutionResult> =
                    serde_json::from_value(row.get("node_results"))?;

                Ok(Some(ExecutionCheckpoint {
                    id: row.get("id"),
                    workflow_id: row.get("workflow_id"),
                    execution_id: row.get("execution_id"),
                    context,
                    completed_nodes,
                    node_results,
                    current_level: row.get::<i32, _>("current_level") as usize,
                    paused: row.get("paused"),
                    created_at: row.get("created_at"),
                    reason: row.get("reason"),
                }))
            }
            None => Ok(None),
        }
    }

    /// Load the latest checkpoint for an execution
    pub async fn load_latest(&self, execution_id: Uuid) -> Result<Option<ExecutionCheckpoint>> {
        let row = sqlx::query(
            r"
            SELECT id, workflow_id, execution_id, context, completed_nodes, node_results,
                   current_level, paused, reason, created_at
            FROM execution_checkpoints
            WHERE execution_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind(execution_id)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => {
                let context: ExecutionContext = serde_json::from_value(row.get("context"))?;
                let completed_nodes: Vec<NodeId> =
                    serde_json::from_value(row.get("completed_nodes"))?;
                let node_results: HashMap<NodeId, NodeExecutionResult> =
                    serde_json::from_value(row.get("node_results"))?;

                Ok(Some(ExecutionCheckpoint {
                    id: row.get("id"),
                    workflow_id: row.get("workflow_id"),
                    execution_id: row.get("execution_id"),
                    context,
                    completed_nodes,
                    node_results,
                    current_level: row.get::<i32, _>("current_level") as usize,
                    paused: row.get("paused"),
                    created_at: row.get("created_at"),
                    reason: row.get("reason"),
                }))
            }
            None => Ok(None),
        }
    }

    /// List checkpoints for an execution
    pub async fn list_by_execution(&self, execution_id: Uuid) -> Result<Vec<ExecutionCheckpoint>> {
        let rows = sqlx::query(
            r"
            SELECT id, workflow_id, execution_id, context, completed_nodes, node_results,
                   current_level, paused, reason, created_at
            FROM execution_checkpoints
            WHERE execution_id = $1
            ORDER BY created_at DESC
            ",
        )
        .bind(execution_id)
        .fetch_all(self.pool.pool())
        .await?;

        let checkpoints = rows
            .into_iter()
            .filter_map(|row| {
                let context: ExecutionContext = serde_json::from_value(row.get("context")).ok()?;
                let completed_nodes: Vec<NodeId> =
                    serde_json::from_value(row.get("completed_nodes")).ok()?;
                let node_results: HashMap<NodeId, NodeExecutionResult> =
                    serde_json::from_value(row.get("node_results")).ok()?;

                Some(ExecutionCheckpoint {
                    id: row.get("id"),
                    workflow_id: row.get("workflow_id"),
                    execution_id: row.get("execution_id"),
                    context,
                    completed_nodes,
                    node_results,
                    current_level: row.get::<i32, _>("current_level") as usize,
                    paused: row.get("paused"),
                    created_at: row.get("created_at"),
                    reason: row.get("reason"),
                })
            })
            .collect();

        Ok(checkpoints)
    }

    /// Delete a checkpoint
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            r"
            DELETE FROM execution_checkpoints
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all checkpoints for an execution
    pub async fn delete_by_execution(&self, execution_id: Uuid) -> Result<u64> {
        let result = sqlx::query(
            r"
            DELETE FROM execution_checkpoints
            WHERE execution_id = $1
            ",
        )
        .bind(execution_id)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{ExecutionContext, NodeExecutionResult};

    #[test]
    fn test_checkpoint_new() {
        let workflow_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();
        let context = ExecutionContext::new(workflow_id);
        let completed_nodes = vec![Uuid::new_v4(), Uuid::new_v4()];
        let current_level = 2;
        let reason = "pause_requested".to_string();

        let checkpoint = ExecutionCheckpoint::new(
            workflow_id,
            execution_id,
            context.clone(),
            completed_nodes.clone(),
            current_level,
            reason.clone(),
        );

        assert_eq!(checkpoint.workflow_id, workflow_id);
        assert_eq!(checkpoint.execution_id, execution_id);
        assert_eq!(checkpoint.current_level, current_level);
        assert_eq!(checkpoint.reason, reason);
        assert_eq!(checkpoint.completed_nodes, completed_nodes);
        assert!(!checkpoint.paused);
        assert!(checkpoint.node_results.is_empty());
    }

    #[test]
    fn test_checkpoint_add_node_result() {
        let workflow_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();
        let context = ExecutionContext::new(workflow_id);

        let mut checkpoint = ExecutionCheckpoint::new(
            workflow_id,
            execution_id,
            context,
            vec![],
            0,
            "test".to_string(),
        );

        let node_id = Uuid::new_v4();
        let result = NodeExecutionResult {
            started_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            result: oxify_model::ExecutionResult::Success(serde_json::json!({"status": "success"})),
            retry_count: 0,
            metrics: None,
        };

        checkpoint.add_node_result(node_id, result.clone());

        assert_eq!(checkpoint.node_results.len(), 1);
        assert!(checkpoint.node_results.contains_key(&node_id));
        let stored_result = checkpoint.node_results.get(&node_id).unwrap();
        match &stored_result.result {
            oxify_model::ExecutionResult::Success(val) => {
                assert_eq!(val, &serde_json::json!({"status": "success"}));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[test]
    fn test_checkpoint_is_node_completed() {
        let workflow_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();
        let context = ExecutionContext::new(workflow_id);

        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        let node3 = Uuid::new_v4();

        let checkpoint = ExecutionCheckpoint::new(
            workflow_id,
            execution_id,
            context,
            vec![node1, node2],
            1,
            "test".to_string(),
        );

        assert!(checkpoint.is_node_completed(node1));
        assert!(checkpoint.is_node_completed(node2));
        assert!(!checkpoint.is_node_completed(node3));
    }

    #[test]
    fn test_checkpoint_multiple_node_results() {
        let workflow_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();
        let context = ExecutionContext::new(workflow_id);

        let mut checkpoint = ExecutionCheckpoint::new(
            workflow_id,
            execution_id,
            context,
            vec![],
            0,
            "test".to_string(),
        );

        // Add multiple node results
        let mut node_ids = Vec::new();
        for i in 1..=5 {
            let node_id = Uuid::new_v4();
            node_ids.push((node_id, i));
            let result = NodeExecutionResult {
                started_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                result: oxify_model::ExecutionResult::Success(serde_json::json!({"value": i})),
                retry_count: 0,
                metrics: Some(oxify_model::NodeMetrics {
                    duration_ms: Some(i * 10),
                    ..Default::default()
                }),
            };
            checkpoint.add_node_result(node_id, result);
        }

        assert_eq!(checkpoint.node_results.len(), 5);

        // Verify all results are stored correctly
        for (node_id, i) in node_ids {
            assert!(checkpoint.node_results.contains_key(&node_id));
            let stored_result = checkpoint.node_results.get(&node_id).unwrap();
            match &stored_result.result {
                oxify_model::ExecutionResult::Success(val) => {
                    assert_eq!(val, &serde_json::json!({"value": i}));
                }
                _ => panic!("Expected Success result"),
            }
            assert_eq!(
                stored_result.metrics.as_ref().unwrap().duration_ms,
                Some(i * 10)
            );
        }
    }
}
