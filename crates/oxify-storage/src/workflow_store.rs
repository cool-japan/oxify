//! Workflow storage implementation for SQLite

use crate::models::WorkflowRow;
use crate::row_ext::{row_to, RowExt};
use crate::{DatabasePool, Result, StorageError};
use chrono::{DateTime, Utc};
use oxify_model::{Workflow, WorkflowId};
use oxisql_core::{Connection, OxiSqlError, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Map a query row selecting the standard `workflows` columns onto a
/// [`WorkflowRow`], using the shared [`row_to!`] helper. Centralising the
/// mapping here keeps every `SELECT` in this store in sync with the
/// [`WorkflowRow`] shape.
fn workflow_row_from(row: &Row) -> std::result::Result<WorkflowRow, OxiSqlError> {
    row_to!(WorkflowRow {
        id: "id",
        name: "name",
        description: "description",
        created_at: "created_at",
        updated_at: "updated_at",
        version: "version",
        definition: "definition",
        tags: "tags",
    })(row)
}

/// Workflow storage layer
#[derive(Clone)]
pub struct WorkflowStore {
    pool: DatabasePool,
}

impl WorkflowStore {
    /// Create a new workflow store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Create a new workflow
    #[tracing::instrument(skip(self, workflow), fields(workflow_id = %workflow.metadata.id, workflow_name = %workflow.metadata.name))]
    pub async fn create(&self, workflow: &Workflow) -> Result<WorkflowId> {
        let id = workflow.metadata.id.to_string();
        let name = &workflow.metadata.name;
        let description = workflow.metadata.description.as_ref();
        let definition = serde_json::to_string(workflow)?;
        let tags = serde_json::to_string(&workflow.metadata.tags)?;

        let conn = self.pool.acquire().await?;
        conn.execute(
            r#"
            INSERT INTO workflows (id, name, description, definition, tags)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            &[&id, name, &description, &definition, &tags],
        )
        .await?;

        Ok(workflow.metadata.id)
    }

    /// Get a workflow by ID
    #[tracing::instrument(skip(self), fields(workflow_id = %id))]
    pub async fn get(&self, id: &WorkflowId) -> Result<Option<Workflow>> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, name, description, created_at, updated_at, version, definition, tags
                FROM workflows
                WHERE id = $1
                "#,
                &[&id_str],
            )
            .await?;

        match rows.first() {
            Some(row) => {
                let workflow_row = workflow_row_from(row)?;
                let workflow: Workflow = serde_json::from_str(&workflow_row.definition)?;
                Ok(Some(workflow))
            }
            None => Ok(None),
        }
    }

    /// List all workflows
    pub async fn list(&self) -> Result<Vec<Workflow>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, name, description, created_at, updated_at, version, definition, tags
                FROM workflows
                ORDER BY created_at DESC
                "#,
                &[],
            )
            .await?;

        let workflows: Vec<Workflow> = rows
            .iter()
            .filter_map(|row| {
                let workflow_row = workflow_row_from(row).ok()?;
                serde_json::from_str(&workflow_row.definition).ok()
            })
            .collect();

        Ok(workflows)
    }

    /// List workflows with pagination
    pub async fn list_paginated(&self, limit: i64, offset: i64) -> Result<Vec<Workflow>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, name, description, created_at, updated_at, version, definition, tags
                FROM workflows
                ORDER BY created_at DESC
                LIMIT $1 OFFSET $2
                "#,
                &[&limit, &offset],
            )
            .await?;

        let workflows: Vec<Workflow> = rows
            .iter()
            .filter_map(|row| {
                let workflow_row = workflow_row_from(row).ok()?;
                serde_json::from_str(&workflow_row.definition).ok()
            })
            .collect();

        Ok(workflows)
    }

    /// Count total workflows
    pub async fn count(&self) -> Result<i64> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query("SELECT COUNT(*) as count FROM workflows", &[])
            .await?;

        let row = rows.first().ok_or_else(|| {
            StorageError::Database(OxiSqlError::Other(
                "COUNT(*) query returned no rows".to_string(),
            ))
        })?;
        let count: i64 = row.col("count")?;
        Ok(count)
    }

    /// Update a workflow
    #[tracing::instrument(skip(self, workflow), fields(workflow_id = %id, workflow_name = %workflow.metadata.name))]
    pub async fn update(&self, id: &WorkflowId, workflow: &Workflow) -> Result<bool> {
        let id_str = id.to_string();
        let name = &workflow.metadata.name;
        let description = workflow.metadata.description.as_ref();
        let definition = serde_json::to_string(workflow)?;
        let tags = serde_json::to_string(&workflow.metadata.tags)?;

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r#"
                UPDATE workflows
                SET name = $1, description = $2, definition = $3, tags = $4, version = version + 1, updated_at = datetime('now')
                WHERE id = $5
                "#,
                &[name, &description, &definition, &tags, &id_str],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Delete a workflow
    #[tracing::instrument(skip(self), fields(workflow_id = %id))]
    pub async fn delete(&self, id: &WorkflowId) -> Result<bool> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r#"
                DELETE FROM workflows
                WHERE id = $1
                "#,
                &[&id_str],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Search workflows by name (case-insensitive)
    pub async fn search(&self, query: &str) -> Result<Vec<Workflow>> {
        let pattern = format!("%{query}%");

        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, name, description, created_at, updated_at, version, definition, tags
                FROM workflows
                WHERE name LIKE $1 OR description LIKE $2
                ORDER BY created_at DESC
                "#,
                &[&pattern, &pattern],
            )
            .await?;

        let workflows: Vec<Workflow> = rows
            .iter()
            .filter_map(|row| {
                let workflow_row = workflow_row_from(row).ok()?;
                serde_json::from_str(&workflow_row.definition).ok()
            })
            .collect();

        Ok(workflows)
    }

    // ==================== Bulk Operations ====================

    /// Bulk create multiple workflows
    /// Returns a list of (id, success, error_message) tuples
    pub async fn bulk_create(&self, workflows: &[Workflow]) -> Result<Vec<BulkOperationResult>> {
        let mut results = Vec::with_capacity(workflows.len());

        for workflow in workflows {
            let result = match self.create(workflow).await {
                Ok(id) => BulkOperationResult {
                    id,
                    success: true,
                    error: None,
                },
                Err(e) => BulkOperationResult {
                    id: workflow.metadata.id,
                    success: false,
                    error: Some(e.to_string()),
                },
            };
            results.push(result);
        }

        Ok(results)
    }

    /// Bulk delete multiple workflows by IDs
    /// Returns a list of (id, success, error_message) tuples
    pub async fn bulk_delete(&self, ids: &[WorkflowId]) -> Result<Vec<BulkOperationResult>> {
        let mut results = Vec::with_capacity(ids.len());

        for id in ids {
            let result = match self.delete(id).await {
                Ok(deleted) => BulkOperationResult {
                    id: *id,
                    success: deleted,
                    error: if deleted {
                        None
                    } else {
                        Some("Workflow not found".to_string())
                    },
                },
                Err(e) => BulkOperationResult {
                    id: *id,
                    success: false,
                    error: Some(e.to_string()),
                },
            };
            results.push(result);
        }

        Ok(results)
    }

    /// Bulk export workflows to JSON format
    pub async fn bulk_export(&self, ids: Option<&[WorkflowId]>) -> Result<WorkflowExport> {
        let workflows = match ids {
            Some(ids) => {
                let mut workflows = Vec::with_capacity(ids.len());
                for id in ids {
                    if let Some(workflow) = self.get(id).await? {
                        workflows.push(workflow);
                    }
                }
                workflows
            }
            None => self.list().await?,
        };

        Ok(WorkflowExport {
            version: "1.0".to_string(),
            exported_at: Utc::now(),
            count: workflows.len(),
            workflows,
        })
    }

    /// Bulk import workflows from export format
    /// Returns import results with statistics
    pub async fn bulk_import(
        &self,
        export: &WorkflowExport,
        options: ImportOptions,
    ) -> Result<ImportResult> {
        let mut result = ImportResult {
            total: export.workflows.len(),
            imported: 0,
            skipped: 0,
            failed: 0,
            errors: Vec::new(),
        };

        for workflow in &export.workflows {
            // Check if workflow already exists
            let existing = self.get(&workflow.metadata.id).await?;

            if existing.is_some() {
                match options.on_conflict {
                    ConflictStrategy::Skip => {
                        result.skipped += 1;
                        continue;
                    }
                    ConflictStrategy::Replace => {
                        match self.update(&workflow.metadata.id, workflow).await {
                            Ok(_) => result.imported += 1,
                            Err(e) => {
                                result.failed += 1;
                                result.errors.push(ImportError {
                                    workflow_id: workflow.metadata.id,
                                    workflow_name: workflow.metadata.name.clone(),
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                    ConflictStrategy::CreateNew => {
                        // Create with new ID
                        let mut new_workflow = workflow.clone();
                        new_workflow.metadata.id = Uuid::new_v4();
                        match self.create(&new_workflow).await {
                            Ok(_) => result.imported += 1,
                            Err(e) => {
                                result.failed += 1;
                                result.errors.push(ImportError {
                                    workflow_id: workflow.metadata.id,
                                    workflow_name: workflow.metadata.name.clone(),
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                    ConflictStrategy::Fail => {
                        return Err(StorageError::ConstraintViolation(format!(
                            "Workflow {} already exists",
                            workflow.metadata.id
                        )));
                    }
                }
            } else {
                match self.create(workflow).await {
                    Ok(_) => result.imported += 1,
                    Err(e) => {
                        result.failed += 1;
                        result.errors.push(ImportError {
                            workflow_id: workflow.metadata.id,
                            workflow_name: workflow.metadata.name.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
        }

        Ok(result)
    }

    /// List workflows by tag
    /// Note: tags are stored as JSON array in SQLite
    pub async fn list_by_tag(&self, tag: &str) -> Result<Vec<Workflow>> {
        // Search for tag in JSON array using LIKE (simple approach)
        let pattern = format!("%\"{tag}\"%");

        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, name, description, created_at, updated_at, version, definition, tags
                FROM workflows
                WHERE tags LIKE $1
                ORDER BY created_at DESC
                "#,
                &[&pattern],
            )
            .await?;

        let workflows: Vec<Workflow> = rows
            .iter()
            .filter_map(|row| {
                let workflow_row = workflow_row_from(row).ok()?;
                serde_json::from_str(&workflow_row.definition).ok()
            })
            .collect();

        Ok(workflows)
    }

    /// List workflows by multiple tags (AND logic - must have all tags)
    pub async fn list_by_tags(&self, tags: &[String]) -> Result<Vec<Workflow>> {
        // Get all workflows and filter in memory for SQLite compatibility
        let all_workflows = self.list().await?;

        let workflows: Vec<Workflow> = all_workflows
            .into_iter()
            .filter(|w| tags.iter().all(|tag| w.metadata.tags.contains(tag)))
            .collect();

        Ok(workflows)
    }

    /// Get workflow IDs only (for lighter queries)
    pub async fn list_ids(&self) -> Result<Vec<WorkflowId>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query("SELECT id FROM workflows ORDER BY created_at DESC", &[])
            .await?;

        let ids: Vec<WorkflowId> = rows
            .iter()
            .filter_map(|row| {
                let id_str: String = row.col("id").ok()?;
                Uuid::parse_str(&id_str).ok()
            })
            .collect();

        Ok(ids)
    }

    /// Check if a workflow exists
    pub async fn exists(&self, id: &WorkflowId) -> Result<bool> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query("SELECT 1 FROM workflows WHERE id = $1 LIMIT 1", &[&id_str])
            .await?;

        Ok(!rows.is_empty())
    }

    /// Get multiple workflows by IDs
    pub async fn get_many(&self, ids: &[WorkflowId]) -> Result<Vec<Workflow>> {
        // For SQLite, we query each ID individually
        // This could be optimized with IN clause and parameter binding
        let mut workflows = Vec::with_capacity(ids.len());

        for id in ids {
            if let Some(workflow) = self.get(id).await? {
                workflows.push(workflow);
            }
        }

        Ok(workflows)
    }
}

/// Result of a bulk operation for a single item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOperationResult {
    pub id: Uuid,
    pub success: bool,
    pub error: Option<String>,
}

/// Workflow export format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExport {
    pub version: String,
    pub exported_at: DateTime<Utc>,
    pub count: usize,
    pub workflows: Vec<Workflow>,
}

/// Import options
#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub on_conflict: ConflictStrategy,
}

/// Strategy for handling conflicts during import
#[derive(Debug, Clone, Copy, Default)]
pub enum ConflictStrategy {
    /// Skip existing workflows
    #[default]
    Skip,
    /// Replace existing workflows
    Replace,
    /// Create new workflows with new IDs
    CreateNew,
    /// Fail if any workflow exists
    Fail,
}

/// Import result with statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub total: usize,
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<ImportError>,
}

/// Import error for a single workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportError {
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Node, NodeKind};

    async fn setup_test_pool() -> Result<DatabasePool> {
        let config = crate::DatabaseConfig {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite::memory:".to_string()),
            ..Default::default()
        };
        DatabasePool::new(config).await
    }

    #[tokio::test]
    async fn test_workflow_crud() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;

        let store = WorkflowStore::new(pool);

        // Create test workflow
        let mut workflow = Workflow::new("Test Workflow".to_string());
        workflow.add_node(Node::new("Start".to_string(), NodeKind::Start));

        // Create
        let id = store.create(&workflow).await?;
        assert_eq!(id, workflow.metadata.id);

        // Get
        let fetched = store.get(&id).await?;
        assert!(fetched.is_some());
        assert_eq!(
            fetched.as_ref().map(|w| w.metadata.name.as_str()),
            Some("Test Workflow")
        );

        // Update
        let mut updated = workflow.clone();
        updated.metadata.name = "Updated Workflow".to_string();
        let result = store.update(&id, &updated).await?;
        assert!(result);

        // Delete
        let result = store.delete(&id).await?;
        assert!(result);

        // Verify deleted
        let fetched = store.get(&id).await?;
        assert!(fetched.is_none());

        Ok(())
    }
}
