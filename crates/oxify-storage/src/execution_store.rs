//! Execution storage implementation for SQLite

use crate::models::ExecutionRow;
use crate::row_ext::{row_to, RowExt};
use crate::{DatabasePool, Result, StorageError};
use oxify_model::{ExecutionContext, ExecutionState, WorkflowId};
use oxisql_core::{Connection, OxiSqlError, Row};
use uuid::Uuid;

/// Maximum number of variables allowed in an execution context
const MAX_VARIABLES: usize = 1000;

/// Map a full `executions` row onto [`ExecutionRow`].
///
/// All read queries in this module select the same nine columns, so a single
/// mapping closure (built via the [`row_to!`] macro) is shared across them.
fn map_execution_row(row: &Row) -> std::result::Result<ExecutionRow, OxiSqlError> {
    row_to!(ExecutionRow {
        id: "id",
        workflow_id: "workflow_id",
        started_at: "started_at",
        completed_at: "completed_at",
        state: "state",
        context: "context",
        node_results: "node_results",
        variables: "variables",
        error_message: "error_message",
    })(row)
}

/// Execution storage layer
#[derive(Clone)]
pub struct ExecutionStore {
    pool: DatabasePool,
}

impl ExecutionStore {
    /// Create a new execution store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Create a new execution record
    #[tracing::instrument(skip(self, ctx), fields(execution_id = %ctx.execution_id, workflow_id = %ctx.workflow_id))]
    pub async fn create(&self, ctx: &ExecutionContext) -> Result<Uuid> {
        // Validate variable count
        if ctx.variables.len() > MAX_VARIABLES {
            return Err(StorageError::ValidationError(format!(
                "Execution has {} variables, which exceeds the maximum of {}",
                ctx.variables.len(),
                MAX_VARIABLES
            )));
        }

        let id = ctx.execution_id.to_string();
        let workflow_id = ctx.workflow_id.to_string();
        let started_at = ctx.started_at.to_rfc3339();
        let completed_at = ctx.completed_at.map(|t| t.to_rfc3339());
        let state = format!("{:?}", ctx.state);
        let context_json = serde_json::to_string(ctx)?;
        let node_results = serde_json::to_string(&ctx.node_results)?;
        let variables = serde_json::to_string(&ctx.variables)?;

        let conn = self.pool.acquire().await?;
        conn.execute(
            r#"
            INSERT INTO executions (id, workflow_id, started_at, completed_at, state, context, node_results, variables)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            &[
                &id,
                &workflow_id,
                &started_at,
                &completed_at,
                &state,
                &context_json,
                &node_results,
                &variables,
            ],
        )
        .await?;

        Ok(ctx.execution_id)
    }

    /// Batch create multiple execution records
    ///
    /// This is more efficient than calling `create()` multiple times
    /// as it uses a single database transaction.
    ///
    /// Returns the number of executions created.
    #[tracing::instrument(skip(self, contexts), fields(batch_size = contexts.len()))]
    pub async fn batch_create(&self, contexts: &[ExecutionContext]) -> Result<u64> {
        if contexts.is_empty() {
            return Ok(0);
        }

        // Validate all contexts first
        for ctx in contexts {
            if ctx.variables.len() > MAX_VARIABLES {
                return Err(StorageError::ValidationError(format!(
                    "Execution {} has {} variables, which exceeds the maximum of {}",
                    ctx.execution_id,
                    ctx.variables.len(),
                    MAX_VARIABLES
                )));
            }
        }

        let conn = self.pool.acquire().await?;
        let mut tx = conn.transaction().await?;

        for ctx in contexts {
            let id = ctx.execution_id.to_string();
            let workflow_id = ctx.workflow_id.to_string();
            let started_at = ctx.started_at.to_rfc3339();
            let completed_at = ctx.completed_at.map(|t| t.to_rfc3339());
            let state = format!("{:?}", ctx.state);
            let context_json = serde_json::to_string(ctx)?;
            let node_results = serde_json::to_string(&ctx.node_results)?;
            let variables = serde_json::to_string(&ctx.variables)?;

            let insert_result = tx
                .execute(
                    r#"
                    INSERT INTO executions (id, workflow_id, started_at, completed_at, state, context, node_results, variables)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#,
                    &[
                        &id,
                        &workflow_id,
                        &started_at,
                        &completed_at,
                        &state,
                        &context_json,
                        &node_results,
                        &variables,
                    ],
                )
                .await;

            if let Err(e) = insert_result {
                tx.rollback().await?;
                return Err(StorageError::Database(e));
            }
        }

        tx.commit().await?;

        Ok(contexts.len() as u64)
    }

    /// Get an execution by ID
    #[tracing::instrument(skip(self), fields(execution_id = %id))]
    pub async fn get(&self, id: &Uuid) -> Result<Option<ExecutionContext>> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
                FROM executions
                WHERE id = $1
                "#,
                &[&id_str],
            )
            .await?;

        match rows.first() {
            Some(row) => {
                let erow = map_execution_row(row)?;
                let ctx: ExecutionContext = serde_json::from_str(&erow.context)?;
                Ok(Some(ctx))
            }
            None => Ok(None),
        }
    }

    /// List all executions
    pub async fn list(&self) -> Result<Vec<(Uuid, ExecutionContext)>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
                FROM executions
                ORDER BY started_at DESC
                "#,
                &[],
            )
            .await?;

        let executions: Vec<(Uuid, ExecutionContext)> = rows
            .iter()
            .filter_map(|row| {
                let erow = map_execution_row(row).ok()?;
                let id = Uuid::parse_str(&erow.id).ok()?;
                let ctx: ExecutionContext = serde_json::from_str(&erow.context).ok()?;
                Some((id, ctx))
            })
            .collect();

        Ok(executions)
    }

    /// List executions for a specific workflow
    pub async fn list_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<Vec<(Uuid, ExecutionContext)>> {
        let workflow_id_str = workflow_id.to_string();
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
                FROM executions
                WHERE workflow_id = $1
                ORDER BY started_at DESC
                "#,
                &[&workflow_id_str],
            )
            .await?;

        let executions: Vec<(Uuid, ExecutionContext)> = rows
            .iter()
            .filter_map(|row| {
                let erow = map_execution_row(row).ok()?;
                let id = Uuid::parse_str(&erow.id).ok()?;
                let ctx: ExecutionContext = serde_json::from_str(&erow.context).ok()?;
                Some((id, ctx))
            })
            .collect();

        Ok(executions)
    }

    /// List executions with pagination
    pub async fn list_paginated(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(Uuid, ExecutionContext)>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
                FROM executions
                ORDER BY started_at DESC
                LIMIT $1 OFFSET $2
                "#,
                &[&limit, &offset],
            )
            .await?;

        let executions: Vec<(Uuid, ExecutionContext)> = rows
            .iter()
            .filter_map(|row| {
                let erow = map_execution_row(row).ok()?;
                let id = Uuid::parse_str(&erow.id).ok()?;
                let ctx: ExecutionContext = serde_json::from_str(&erow.context).ok()?;
                Some((id, ctx))
            })
            .collect();

        Ok(executions)
    }

    /// Update an execution
    #[tracing::instrument(skip(self, ctx), fields(execution_id = %id, new_state = ?ctx.state))]
    pub async fn update(&self, id: &Uuid, ctx: &ExecutionContext) -> Result<bool> {
        // Validate variable count
        if ctx.variables.len() > MAX_VARIABLES {
            return Err(StorageError::ValidationError(format!(
                "Execution has {} variables, which exceeds the maximum of {}",
                ctx.variables.len(),
                MAX_VARIABLES
            )));
        }

        let id_str = id.to_string();
        let state = format!("{:?}", ctx.state);
        let completed_at = ctx.completed_at.map(|t| t.to_rfc3339());
        let context_json = serde_json::to_string(ctx)?;
        let node_results = serde_json::to_string(&ctx.node_results)?;
        let variables = serde_json::to_string(&ctx.variables)?;

        // Extract error message if state is Failed
        let error_message = match &ctx.state {
            ExecutionState::Failed(msg) => Some(msg.clone()),
            _ => None,
        };

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r#"
                UPDATE executions
                SET completed_at = $1, state = $2, context = $3, node_results = $4, variables = $5, error_message = $6
                WHERE id = $7
                "#,
                &[
                    &completed_at,
                    &state,
                    &context_json,
                    &node_results,
                    &variables,
                    &error_message,
                    &id_str,
                ],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Delete an execution
    #[tracing::instrument(skip(self), fields(execution_id = %id))]
    pub async fn delete(&self, id: &Uuid) -> Result<bool> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r#"
                DELETE FROM executions
                WHERE id = $1
                "#,
                &[&id_str],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Count executions by state
    pub async fn count_by_state(&self, state: &str) -> Result<i64> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT COUNT(*) as count
                FROM executions
                WHERE state = $1
                "#,
                &[&state],
            )
            .await?;

        let row = rows.first().ok_or_else(|| {
            StorageError::Database(OxiSqlError::Execution(
                "COUNT(*) query returned no rows".to_string(),
            ))
        })?;
        let count: i64 = row.col("count")?;
        Ok(count)
    }

    /// Get active executions (Running or Paused)
    pub async fn get_active(&self) -> Result<Vec<(Uuid, ExecutionContext)>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
                FROM executions
                WHERE state IN ('Running', 'Paused')
                ORDER BY started_at DESC
                "#,
                &[],
            )
            .await?;

        let executions: Vec<(Uuid, ExecutionContext)> = rows
            .iter()
            .filter_map(|row| {
                let erow = map_execution_row(row).ok()?;
                let id = Uuid::parse_str(&erow.id).ok()?;
                let ctx: ExecutionContext = serde_json::from_str(&erow.context).ok()?;
                Some((id, ctx))
            })
            .collect();

        Ok(executions)
    }

    /// Delete all executions for a specific workflow
    /// Returns the number of executions deleted
    #[tracing::instrument(skip(self), fields(workflow_id = %workflow_id))]
    pub async fn delete_by_workflow(&self, workflow_id: &WorkflowId) -> Result<u64> {
        let workflow_id_str = workflow_id.to_string();
        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r#"
                DELETE FROM executions WHERE workflow_id = $1
                "#,
                &[&workflow_id_str],
            )
            .await?;

        Ok(rows_affected)
    }

    /// Archive completed executions older than the specified date
    /// Returns the number of executions archived (deleted)
    #[tracing::instrument(skip(self), fields(before = %before))]
    pub async fn archive_completed(&self, before: chrono::DateTime<chrono::Utc>) -> Result<u64> {
        let before_str = before.to_rfc3339();
        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r#"
                DELETE FROM executions
                WHERE completed_at IS NOT NULL
                AND completed_at < $1
                "#,
                &[&before_str],
            )
            .await?;

        Ok(rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::ExecutionContext;

    async fn setup_test_pool() -> Result<DatabasePool> {
        // `:memory:` SQLite databases are per-connection in oxisql's pool (each
        // pool slot opens its own isolated in-memory database, with no shared
        // state between slots), so the pool is pinned to a single connection
        // here to guarantee that the schema applied by `migrate()` is visible to
        // every subsequent `pool.acquire()` call made by the store in this test.
        let config = crate::DatabaseConfig {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite::memory:".to_string()),
            max_connections: 1,
            min_connections: 1,
        };
        DatabasePool::new(config).await
    }

    #[tokio::test]
    async fn test_execution_crud() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;

        let store = ExecutionStore::new(pool);

        // Create test execution
        let workflow_id = Uuid::new_v4();
        let mut ctx = ExecutionContext::new(workflow_id);

        // Create
        let id = store.create(&ctx).await?;
        assert_eq!(id, ctx.execution_id);

        // Get
        let fetched = store.get(&id).await?;
        assert!(fetched.is_some());

        // Update
        ctx.state = ExecutionState::Completed;
        ctx.mark_completed();
        let result = store.update(&id, &ctx).await?;
        assert!(result);

        // Delete
        let result = store.delete(&id).await?;
        assert!(result);

        Ok(())
    }
}
