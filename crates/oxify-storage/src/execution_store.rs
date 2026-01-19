//! Execution storage implementation for SQLite

use crate::{DatabasePool, Result, StorageError};
use oxify_model::{ExecutionContext, ExecutionState, WorkflowId};
use sqlx::Row;
use uuid::Uuid;

/// Maximum number of variables allowed in an execution context
const MAX_VARIABLES: usize = 1000;

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

        sqlx::query(
            r#"
            INSERT INTO executions (id, workflow_id, started_at, completed_at, state, context, node_results, variables)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&workflow_id)
        .bind(Some(&started_at))
        .bind(&completed_at)
        .bind(&state)
        .bind(&context_json)
        .bind(&node_results)
        .bind(&variables)
        .execute(self.pool.pool())
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

        let mut tx = self.pool.pool().begin().await?;

        for ctx in contexts {
            let id = ctx.execution_id.to_string();
            let workflow_id = ctx.workflow_id.to_string();
            let started_at = ctx.started_at.to_rfc3339();
            let completed_at = ctx.completed_at.map(|t| t.to_rfc3339());
            let state = format!("{:?}", ctx.state);
            let context_json = serde_json::to_string(ctx)?;
            let node_results = serde_json::to_string(&ctx.node_results)?;
            let variables = serde_json::to_string(&ctx.variables)?;

            sqlx::query(
                r#"
                INSERT INTO executions (id, workflow_id, started_at, completed_at, state, context, node_results, variables)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&workflow_id)
            .bind(Some(&started_at))
            .bind(&completed_at)
            .bind(&state)
            .bind(&context_json)
            .bind(&node_results)
            .bind(&variables)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(contexts.len() as u64)
    }

    /// Get an execution by ID
    #[tracing::instrument(skip(self), fields(execution_id = %id))]
    pub async fn get(&self, id: &Uuid) -> Result<Option<ExecutionContext>> {
        let id_str = id.to_string();
        let row = sqlx::query(
            r#"
            SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
            FROM executions
            WHERE id = ?
            "#,
        )
        .bind(&id_str)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => {
                let context_str: String = row.get("context");
                let ctx: ExecutionContext = serde_json::from_str(&context_str)?;
                Ok(Some(ctx))
            }
            None => Ok(None),
        }
    }

    /// List all executions
    pub async fn list(&self) -> Result<Vec<(Uuid, ExecutionContext)>> {
        let rows = sqlx::query(
            r#"
            SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
            FROM executions
            ORDER BY started_at DESC
            "#,
        )
        .fetch_all(self.pool.pool())
        .await?;

        let executions: Vec<(Uuid, ExecutionContext)> = rows
            .into_iter()
            .filter_map(|row| {
                let id_str: String = row.get("id");
                let context_str: String = row.get("context");
                let id = Uuid::parse_str(&id_str).ok()?;
                let ctx: ExecutionContext = serde_json::from_str(&context_str).ok()?;
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
        let rows = sqlx::query(
            r#"
            SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
            FROM executions
            WHERE workflow_id = ?
            ORDER BY started_at DESC
            "#,
        )
        .bind(&workflow_id_str)
        .fetch_all(self.pool.pool())
        .await?;

        let executions: Vec<(Uuid, ExecutionContext)> = rows
            .into_iter()
            .filter_map(|row| {
                let id_str: String = row.get("id");
                let context_str: String = row.get("context");
                let id = Uuid::parse_str(&id_str).ok()?;
                let ctx: ExecutionContext = serde_json::from_str(&context_str).ok()?;
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
        let rows = sqlx::query(
            r#"
            SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
            FROM executions
            ORDER BY started_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.pool())
        .await?;

        let executions: Vec<(Uuid, ExecutionContext)> = rows
            .into_iter()
            .filter_map(|row| {
                let id_str: String = row.get("id");
                let context_str: String = row.get("context");
                let id = Uuid::parse_str(&id_str).ok()?;
                let ctx: ExecutionContext = serde_json::from_str(&context_str).ok()?;
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

        let result = sqlx::query(
            r#"
            UPDATE executions
            SET completed_at = ?, state = ?, context = ?, node_results = ?, variables = ?, error_message = ?
            WHERE id = ?
            "#,
        )
        .bind(&completed_at)
        .bind(&state)
        .bind(&context_json)
        .bind(&node_results)
        .bind(&variables)
        .bind(&error_message)
        .bind(&id_str)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete an execution
    #[tracing::instrument(skip(self), fields(execution_id = %id))]
    pub async fn delete(&self, id: &Uuid) -> Result<bool> {
        let id_str = id.to_string();
        let result = sqlx::query(
            r#"
            DELETE FROM executions
            WHERE id = ?
            "#,
        )
        .bind(&id_str)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Count executions by state
    pub async fn count_by_state(&self, state: &str) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM executions
            WHERE state = ?
            "#,
        )
        .bind(state)
        .fetch_one(self.pool.pool())
        .await?;

        let count: i64 = row.get("count");
        Ok(count)
    }

    /// Get active executions (Running or Paused)
    pub async fn get_active(&self) -> Result<Vec<(Uuid, ExecutionContext)>> {
        let rows = sqlx::query(
            r#"
            SELECT id, workflow_id, started_at, completed_at, state, context, node_results, variables, error_message
            FROM executions
            WHERE state IN ('Running', 'Paused')
            ORDER BY started_at DESC
            "#,
        )
        .fetch_all(self.pool.pool())
        .await?;

        let executions: Vec<(Uuid, ExecutionContext)> = rows
            .into_iter()
            .filter_map(|row| {
                let id_str: String = row.get("id");
                let context_str: String = row.get("context");
                let id = Uuid::parse_str(&id_str).ok()?;
                let ctx: ExecutionContext = serde_json::from_str(&context_str).ok()?;
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
        let result = sqlx::query(
            r#"
            DELETE FROM executions WHERE workflow_id = ?
            "#,
        )
        .bind(&workflow_id_str)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Archive completed executions older than the specified date
    /// Returns the number of executions archived (deleted)
    #[tracing::instrument(skip(self), fields(before = %before))]
    pub async fn archive_completed(&self, before: chrono::DateTime<chrono::Utc>) -> Result<u64> {
        let before_str = before.to_rfc3339();
        let result = sqlx::query(
            r#"
            DELETE FROM executions
            WHERE completed_at IS NOT NULL
            AND completed_at < ?
            "#,
        )
        .bind(&before_str)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::ExecutionContext;

    async fn setup_test_pool() -> Result<DatabasePool> {
        let config = crate::DatabaseConfig {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite::memory:".to_string()),
            ..Default::default()
        };
        DatabasePool::new(config).await
    }

    #[tokio::test]
    #[ignore] // Requires database
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
