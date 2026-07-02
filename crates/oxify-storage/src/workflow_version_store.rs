//! Workflow version history storage

use crate::row_ext::row_to;
use crate::{DatabasePool, Result};
use chrono::{DateTime, Utc};
use oxify_model::{Workflow, WorkflowId};
use oxisql_core::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Workflow version record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowVersion {
    /// Version record ID
    pub id: Uuid,
    /// Workflow ID
    pub workflow_id: WorkflowId,
    /// Version number
    pub version: i32,
    /// Version description/change message
    pub description: Option<String>,
    /// Workflow snapshot at this version
    pub workflow: Workflow,
    /// When this version was created
    pub created_at: DateTime<Utc>,
    /// User who created this version (if available)
    pub created_by: Option<String>,
}

/// Raw `workflow_versions` row shape, mapped from an [`oxisql_core::Row`] via
/// [`row_to!`] before the string/JSON fields are parsed into
/// [`WorkflowVersion`].
struct VersionRow {
    id: String,
    workflow_id: String,
    version: i32,
    description: Option<String>,
    definition: String,
    created_at: String,
    created_by: Option<String>,
}

/// Workflow version storage layer
#[derive(Clone)]
pub struct WorkflowVersionStore {
    pool: DatabasePool,
}

impl WorkflowVersionStore {
    /// Create a new workflow version store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Save a new version of a workflow
    pub async fn save_version(
        &self,
        workflow: &Workflow,
        description: Option<String>,
        created_by: Option<String>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let workflow_id = workflow.metadata.id;
        let version = self.get_next_version(&workflow_id).await?;
        let definition = serde_json::to_string(workflow)?;

        let id_str = id.to_string();
        let workflow_id_str = workflow_id.to_string();
        // Written explicitly (rather than relying on the column's
        // `DEFAULT (datetime('now'))`) so the stored value is RFC 3339, which
        // is what `get_versions`/`get_version` parse it back with via
        // `DateTime::parse_from_rfc3339`. SQLite's `datetime('now')` produces
        // a bare "YYYY-MM-DD HH:MM:SS" string that RFC 3339 parsing rejects.
        let created_at = Utc::now().to_rfc3339();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            INSERT INTO workflow_versions
            (id, workflow_id, version, description, definition, created_at, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ",
            &[
                &id_str,
                &workflow_id_str,
                &version,
                &description,
                &definition,
                &created_at,
                &created_by,
            ],
        )
        .await?;

        Ok(id)
    }

    /// Get version history for a workflow
    pub async fn get_versions(&self, workflow_id: &WorkflowId) -> Result<Vec<WorkflowVersion>> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT id, workflow_id, version, description, definition, created_at, created_by
                FROM workflow_versions
                WHERE workflow_id = $1
                ORDER BY version DESC
                ",
                &[&workflow_id_str],
            )
            .await?;

        let versions = rows
            .iter()
            .map(row_to!(VersionRow {
                id: "id",
                workflow_id: "workflow_id",
                version: "version",
                description: "description",
                definition: "definition",
                created_at: "created_at",
                created_by: "created_by",
            }))
            .filter_map(|raw| {
                let raw = raw.ok()?;

                let id = Uuid::parse_str(&raw.id).ok()?;
                let workflow_id = Uuid::parse_str(&raw.workflow_id).ok()?;
                let workflow: Workflow = serde_json::from_str(&raw.definition).ok()?;
                let created_at = DateTime::parse_from_rfc3339(&raw.created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()?;

                Some(WorkflowVersion {
                    id,
                    workflow_id,
                    version: raw.version,
                    description: raw.description,
                    workflow,
                    created_at,
                    created_by: raw.created_by,
                })
            })
            .collect();

        Ok(versions)
    }

    /// Get a specific version of a workflow
    pub async fn get_version(
        &self,
        workflow_id: &WorkflowId,
        version: i32,
    ) -> Result<Option<WorkflowVersion>> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT id, workflow_id, version, description, definition, created_at, created_by
                FROM workflow_versions
                WHERE workflow_id = $1 AND version = $2
                ",
                &[&workflow_id_str, &version],
            )
            .await?;

        let Some(row) = rows.first() else {
            return Ok(None);
        };

        let raw = row_to!(VersionRow {
            id: "id",
            workflow_id: "workflow_id",
            version: "version",
            description: "description",
            definition: "definition",
            created_at: "created_at",
            created_by: "created_by",
        })(row)?;

        let id = Uuid::parse_str(&raw.id)
            .map_err(|e| crate::StorageError::validation(format!("Invalid UUID: {}", e)))?;
        let workflow_id = Uuid::parse_str(&raw.workflow_id)
            .map_err(|e| crate::StorageError::validation(format!("Invalid UUID: {}", e)))?;
        let workflow: Workflow = serde_json::from_str(&raw.definition)?;
        let created_at = DateTime::parse_from_rfc3339(&raw.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| crate::StorageError::validation(format!("Invalid date: {}", e)))?;

        Ok(Some(WorkflowVersion {
            id,
            workflow_id,
            version: raw.version,
            description: raw.description,
            workflow,
            created_at,
            created_by: raw.created_by,
        }))
    }

    /// Get the next version number for a workflow
    async fn get_next_version(&self, workflow_id: &WorkflowId) -> Result<i32> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT COALESCE(MAX(version), 0) + 1 as next_version
                FROM workflow_versions
                WHERE workflow_id = $1
                ",
                &[&workflow_id_str],
            )
            .await?;

        let row = rows.first().ok_or_else(|| {
            crate::StorageError::Database(oxisql_core::OxiSqlError::Other(
                "aggregate query for next version returned no rows".to_string(),
            ))
        })?;

        let next_version: i32 = row.try_get("next_version")?;
        Ok(next_version)
    }

    /// Get the latest version number for a workflow
    pub async fn get_latest_version(&self, workflow_id: &WorkflowId) -> Result<Option<i32>> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT MAX(version) as latest_version
                FROM workflow_versions
                WHERE workflow_id = $1
                ",
                &[&workflow_id_str],
            )
            .await?;

        let row = rows.first().ok_or_else(|| {
            crate::StorageError::Database(oxisql_core::OxiSqlError::Other(
                "aggregate query for latest version returned no rows".to_string(),
            ))
        })?;

        let latest_version: Option<i32> = row.try_get("latest_version")?;
        Ok(latest_version)
    }

    /// Delete all versions for a workflow
    pub async fn delete_all_versions(&self, workflow_id: &WorkflowId) -> Result<u64> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r"
                DELETE FROM workflow_versions
                WHERE workflow_id = $1
                ",
                &[&workflow_id_str],
            )
            .await?;

        Ok(rows_affected)
    }

    /// Compare two versions of a workflow
    pub async fn compare_versions(
        &self,
        workflow_id: &WorkflowId,
        version1: i32,
        version2: i32,
    ) -> Result<Option<VersionComparison>> {
        let v1 = self.get_version(workflow_id, version1).await?;
        let v2 = self.get_version(workflow_id, version2).await?;

        match (v1, v2) {
            (Some(v1), Some(v2)) => {
                let comparison = VersionComparison {
                    version1: v1.version,
                    version2: v2.version,
                    nodes_added: v2.workflow.nodes.len() as i32 - v1.workflow.nodes.len() as i32,
                    nodes_removed: 0, // Would need more sophisticated diff
                    edges_added: v2.workflow.edges.len() as i32 - v1.workflow.edges.len() as i32,
                    edges_removed: 0, // Would need more sophisticated diff
                    name_changed: v1.workflow.metadata.name != v2.workflow.metadata.name,
                    description_changed: v1.workflow.metadata.description
                        != v2.workflow.metadata.description,
                };
                Ok(Some(comparison))
            }
            _ => Ok(None),
        }
    }

    /// Rollback a workflow to a specific version
    /// This updates the main workflow definition and creates a new version record
    /// Returns true if the rollback was successful
    pub async fn rollback_to_version(
        &self,
        workflow_id: &WorkflowId,
        target_version: i32,
        rollback_description: Option<String>,
        rolled_back_by: Option<String>,
    ) -> Result<bool> {
        // Get the target version
        let version = self
            .get_version(workflow_id, target_version)
            .await?
            .ok_or_else(|| {
                crate::StorageError::not_found(
                    crate::ResourceType::WorkflowVersion,
                    crate::ResourceId::Composite(vec![
                        workflow_id.to_string(),
                        target_version.to_string(),
                    ]),
                )
            })?;

        // Update the main workflows table with the version's definition
        let workflow_json = serde_json::to_string(&version.workflow)?;
        let now = Utc::now().to_rfc3339();
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r"
                UPDATE workflows
                SET name = $1,
                    description = $2,
                    definition = $3,
                    updated_at = $4
                WHERE id = $5
                ",
                &[
                    &version.workflow.metadata.name,
                    &version.workflow.metadata.description,
                    &workflow_json,
                    &now,
                    &workflow_id_str,
                ],
            )
            .await?;
        drop(conn);

        if rows_affected == 0 {
            return Ok(false);
        }

        // Create a new version record to track the rollback
        let description = rollback_description
            .unwrap_or_else(|| format!("Rolled back to version {target_version}"));

        self.save_version(&version.workflow, Some(description), rolled_back_by)
            .await?;

        Ok(true)
    }

    /// Delete versions older than the specified version
    /// Useful for cleaning up old version history
    /// Returns the number of versions deleted
    pub async fn delete_versions_before(
        &self,
        workflow_id: &WorkflowId,
        before_version: i32,
    ) -> Result<u64> {
        let workflow_id_str = workflow_id.to_string();

        let conn = self.pool.acquire().await?;
        let rows_affected = conn
            .execute(
                r"
                DELETE FROM workflow_versions
                WHERE workflow_id = $1 AND version < $2
                ",
                &[&workflow_id_str, &before_version],
            )
            .await?;

        Ok(rows_affected)
    }
}

/// Comparison between two workflow versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionComparison {
    pub version1: i32,
    pub version2: i32,
    pub nodes_added: i32,
    pub nodes_removed: i32,
    pub edges_added: i32,
    pub edges_removed: i32,
    pub name_changed: bool,
    pub description_changed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    async fn setup_test_pool() -> Result<DatabasePool> {
        // `:memory:` SQLite databases are per-connection in oxisql's pool
        // (each pool slot opens its own isolated in-memory database), so the
        // pool is pinned to a single connection here to guarantee that the
        // schema applied by `migrate()` is visible to every subsequent
        // `pool.acquire()` call made by the store in this test.
        let config = crate::DatabaseConfig {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite::memory:".to_string()),
            max_connections: 1,
            min_connections: 1,
        };
        DatabasePool::new(config).await
    }

    #[tokio::test]
    async fn test_version_history() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;

        let store = WorkflowVersionStore::new(pool);

        // Create test workflow
        let workflow = Workflow::new("Test Workflow".to_string());
        let workflow_id = workflow.metadata.id;

        // Save first version
        let v1_id = store
            .save_version(&workflow, Some("Initial version".to_string()), None)
            .await?;
        assert_ne!(v1_id, Uuid::nil());

        // Get versions
        let versions = store.get_versions(&workflow_id).await?;
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, 1);

        // Save second version
        let mut workflow_v2 = workflow.clone();
        workflow_v2.metadata.name = "Updated Workflow".to_string();
        store
            .save_version(&workflow_v2, Some("Updated name".to_string()), None)
            .await?;

        // Get versions again
        let versions = store.get_versions(&workflow_id).await?;
        assert_eq!(versions.len(), 2);

        // Get specific version
        let v1 = store.get_version(&workflow_id, 1).await?;
        assert!(v1.is_some());
        assert_eq!(v1.unwrap().workflow.metadata.name, "Test Workflow");

        Ok(())
    }
}
