//! Database migration utilities

use crate::*;
use sqlx::sqlite::SqlitePool;

/// Migration manager for authorization database
pub struct MigrationManager {
    pool: SqlitePool,
}

impl MigrationManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Run all pending migrations
    pub async fn run_migrations(&self) -> Result<()> {
        tracing::info!("Running authorization database migrations...");

        // Run the initial schema
        sqlx::query(include_str!("../migrations/001_init.sql"))
            .execute(&self.pool)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Migration 001 failed: {}", e)))?;

        tracing::info!("Authorization migrations completed successfully");
        Ok(())
    }

    /// Refresh the reachability index (Leopard Index)
    /// For SQLite, this is a no-op as we don't have PostgreSQL stored procedures
    pub async fn refresh_index(&self) -> Result<()> {
        tracing::info!("Refreshing reachability index (no-op for SQLite)...");
        // SQLite doesn't support stored procedures like PostgreSQL
        // The index is maintained through application-level logic
        tracing::info!("Reachability index refresh skipped (SQLite)");
        Ok(())
    }

    /// Clean up old audit logs
    pub async fn cleanup_audit_logs(&self) -> Result<u64> {
        tracing::info!("Cleaning up old audit logs...");

        // Delete audit logs older than 90 days
        let result = sqlx::query(
            r#"
            DELETE FROM authz_audit_log
            WHERE timestamp < datetime('now', '-90 days')
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AuthzError::DatabaseError(format!("Cleanup failed: {}", e)))?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_migrations() {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());

        let pool = SqlitePool::connect(&database_url).await.unwrap();
        let manager = MigrationManager::new(pool);

        manager.run_migrations().await.unwrap();
    }
}
