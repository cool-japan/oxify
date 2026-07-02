//! Database-backed checkpoint storage
//!
//! Ported from the retired `sqlx` API to the Pure-Rust OxiSQL stack
//! (`oxisql_core::Connection` + the shared [`row_ext`](crate::row_ext) mapping
//! helpers), mirroring [`crate::execution_store`]. All JSON payloads
//! (`context`, `completed_nodes`, `node_results`) are persisted as TEXT via
//! `serde_json::to_string`, integers as `INTEGER`, the paused flag as `0`/`1`,
//! and `created_at` as an RFC 3339 string.

use crate::row_ext::RowExt;
use crate::{DatabasePool, Result, StorageError};
use chrono::{DateTime, Utc};
use oxify_model::{ExecutionContext, NodeExecutionResult, NodeId, WorkflowId};
use oxisql_core::{Connection, Row};
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

/// The nine columns selected by every read query in this module.
const CHECKPOINT_COLUMNS: &str = "id, workflow_id, execution_id, context, completed_nodes, \
     node_results, current_level, paused, reason, created_at";

/// Map a full `execution_checkpoints` row onto [`ExecutionCheckpoint`].
///
/// JSON columns are deserialized, UUID/timestamp columns are parsed, and the
/// integer `current_level` / `paused` columns are normalized back to `usize` /
/// `bool`. A malformed row surfaces as a [`StorageError`] rather than a panic.
fn row_to_checkpoint(row: &Row) -> Result<ExecutionCheckpoint> {
    let id_str: String = row.col("id")?;
    let workflow_id_str: String = row.col("workflow_id")?;
    let execution_id_str: String = row.col("execution_id")?;
    let context_json: String = row.col("context")?;
    let completed_nodes_json: String = row.col("completed_nodes")?;
    let node_results_json: String = row.col("node_results")?;
    let current_level: i64 = row.col("current_level")?;
    let paused: i64 = row.col("paused")?;
    let reason: String = row.col("reason")?;
    let created_at_str: String = row.col("created_at")?;

    let parse_uuid = |value: &str, field: &str| -> Result<Uuid> {
        Uuid::parse_str(value).map_err(|e| {
            StorageError::ValidationError(format!("invalid {field} in checkpoint row: {e}"))
        })
    };

    let context: ExecutionContext = serde_json::from_str(&context_json)?;
    let completed_nodes: Vec<NodeId> = serde_json::from_str(&completed_nodes_json)?;
    let node_results: HashMap<NodeId, NodeExecutionResult> =
        serde_json::from_str(&node_results_json)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            StorageError::ValidationError(format!("invalid created_at in checkpoint row: {e}"))
        })?;

    Ok(ExecutionCheckpoint {
        id: parse_uuid(&id_str, "id")?,
        workflow_id: parse_uuid(&workflow_id_str, "workflow_id")?,
        execution_id: parse_uuid(&execution_id_str, "execution_id")?,
        context,
        completed_nodes,
        node_results,
        current_level: current_level as usize,
        paused: paused != 0,
        created_at,
        reason,
    })
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

    /// Idempotently create the `execution_checkpoints` table.
    ///
    /// Production deployments create this table (with its foreign key and
    /// indexes) through the migration runner
    /// ([`DatabasePool::migrate`](crate::DatabasePool::migrate) applies
    /// `migrations/20251130000007__execution_checkpoints.sql`). This helper is
    /// a lightweight, dependency-free bootstrap for embedded and test contexts
    /// that do not run the full migration set; it is safe to call repeatedly
    /// and never overwrites an existing table.
    pub async fn ensure_table(&self) -> Result<()> {
        let conn = self.pool.acquire().await?;
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS execution_checkpoints (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                execution_id TEXT NOT NULL,
                context TEXT NOT NULL,
                completed_nodes TEXT NOT NULL DEFAULT '[]',
                node_results TEXT NOT NULL DEFAULT '{}',
                current_level INTEGER NOT NULL DEFAULT 0,
                paused INTEGER NOT NULL DEFAULT 0,
                reason TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
            &[],
        )
        .await?;
        Ok(())
    }

    /// Save a checkpoint
    pub async fn save(&self, checkpoint: &ExecutionCheckpoint) -> Result<Uuid> {
        let id = checkpoint.id.to_string();
        let workflow_id = checkpoint.workflow_id.to_string();
        let execution_id = checkpoint.execution_id.to_string();
        let context_json = serde_json::to_string(&checkpoint.context)?;
        let completed_nodes_json = serde_json::to_string(&checkpoint.completed_nodes)?;
        let node_results_json = serde_json::to_string(&checkpoint.node_results)?;
        let current_level = checkpoint.current_level as i64;
        let paused = i64::from(checkpoint.paused);
        let created_at = checkpoint.created_at.to_rfc3339();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r#"
            INSERT INTO execution_checkpoints
            (id, workflow_id, execution_id, context, completed_nodes, node_results,
             current_level, paused, reason, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            &[
                &id,
                &workflow_id,
                &execution_id,
                &context_json,
                &completed_nodes_json,
                &node_results_json,
                &current_level,
                &paused,
                &checkpoint.reason,
                &created_at,
            ],
        )
        .await?;

        Ok(checkpoint.id)
    }

    /// Load a checkpoint by ID
    pub async fn load(&self, id: Uuid) -> Result<Option<ExecutionCheckpoint>> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                &format!("SELECT {CHECKPOINT_COLUMNS} FROM execution_checkpoints WHERE id = $1"),
                &[&id_str],
            )
            .await?;

        match rows.first() {
            Some(row) => Ok(Some(row_to_checkpoint(row)?)),
            None => Ok(None),
        }
    }

    /// Load the latest checkpoint for an execution
    pub async fn load_latest(&self, execution_id: Uuid) -> Result<Option<ExecutionCheckpoint>> {
        let execution_id_str = execution_id.to_string();
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                &format!(
                    "SELECT {CHECKPOINT_COLUMNS} FROM execution_checkpoints \
                     WHERE execution_id = $1 ORDER BY created_at DESC LIMIT 1"
                ),
                &[&execution_id_str],
            )
            .await?;

        match rows.first() {
            Some(row) => Ok(Some(row_to_checkpoint(row)?)),
            None => Ok(None),
        }
    }

    /// List checkpoints for an execution
    pub async fn list_by_execution(&self, execution_id: Uuid) -> Result<Vec<ExecutionCheckpoint>> {
        let execution_id_str = execution_id.to_string();
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                &format!(
                    "SELECT {CHECKPOINT_COLUMNS} FROM execution_checkpoints \
                     WHERE execution_id = $1 ORDER BY created_at DESC"
                ),
                &[&execution_id_str],
            )
            .await?;

        let checkpoints = rows
            .iter()
            .filter_map(|row| row_to_checkpoint(row).ok())
            .collect();

        Ok(checkpoints)
    }

    /// Delete a checkpoint
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                "DELETE FROM execution_checkpoints WHERE id = $1",
                &[&id_str],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Delete all checkpoints for an execution
    pub async fn delete_by_execution(&self, execution_id: Uuid) -> Result<u64> {
        let execution_id_str = execution_id.to_string();
        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                "DELETE FROM execution_checkpoints WHERE execution_id = $1",
                &[&execution_id_str],
            )
            .await?;

        Ok(rows_affected)
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
        let stored_result = checkpoint
            .node_results
            .get(&node_id)
            .expect("node result present");
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
            let stored_result = checkpoint
                .node_results
                .get(&node_id)
                .expect("node result present");
            match &stored_result.result {
                oxify_model::ExecutionResult::Success(val) => {
                    assert_eq!(val, &serde_json::json!({"value": i}));
                }
                _ => panic!("Expected Success result"),
            }
            assert_eq!(
                stored_result
                    .metrics
                    .as_ref()
                    .expect("metrics present")
                    .duration_ms,
                Some(i * 10)
            );
        }
    }
}
