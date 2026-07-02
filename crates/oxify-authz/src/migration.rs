//! Database migration utilities

use crate::*;
use oxisql_core::Connection;
use oxisql_migrate::runner::MigrationRunner;
use oxisql_pool::sqlite::SqlitePool;

/// Absolute path to this crate's `migrations/` directory, resolved at compile
/// time via `CARGO_MANIFEST_DIR` so the runtime working directory does not
/// affect migration discovery.
const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

/// Name of the tracking table maintained by `oxisql-migrate` (its default).
const TRACKER_TABLE: &str = "_oxisql_migrations";

/// Migration manager for authorization database
pub struct MigrationManager {
    pool: SqlitePool,
}

impl MigrationManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Run all pending migrations.
    ///
    /// Uses the `oxisql-migrate` directory runner, which scans
    /// `MIGRATIONS_DIR` (this crate's `migrations/` directory) for files
    /// named `<14-digit-timestamp>__<name>.sql`,
    /// records applied versions in the `_oxisql_migrations` tracking table, and
    /// executes each pending file via [`Connection::execute_batch`] so that the
    /// multi-statement schema file runs as a single logical migration.
    ///
    /// A mandatory post-run count gate confirms that at least one migration was
    /// recorded; this catches the silent-skip failure mode where a migration
    /// filename does not match the scanner's `<timestamp>__<name>.sql` pattern
    /// (unmatched files are ignored without error by the scanner).
    pub async fn run_migrations(&self) -> Result<()> {
        tracing::info!("Running authorization database migrations...");

        // A single pooled connection is used for both the migration run and the
        // count gate so that, for in-memory databases (where each pool slot is
        // an independent database), both observe the same underlying state.
        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;

        let mut runner = MigrationRunner::new(MIGRATIONS_DIR);
        let applied = runner
            .run_with_conn(&*conn)
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Migration failed: {e}")))?;
        tracing::info!("Applied {applied} pending migration(s)");

        // Mandatory migration-count gate.
        let recorded = self.applied_migration_count(&*conn).await?;
        if recorded == 0 {
            return Err(AuthzError::DatabaseError(format!(
                "Migration gate failed: no migrations recorded in {TRACKER_TABLE} after run \
                 (check that migration files match the required <timestamp>__<name>.sql pattern)"
            )));
        }
        tracing::info!("Migration gate passed: {recorded} migration(s) recorded");

        tracing::info!("Authorization migrations completed successfully");
        Ok(())
    }

    /// Count the migrations recorded as applied in the `_oxisql_migrations`
    /// tracker table, using the supplied connection.
    async fn applied_migration_count(&self, conn: &dyn Connection) -> Result<u64> {
        let rows = conn
            .query(&format!("SELECT COUNT(*) AS n FROM {TRACKER_TABLE}"), &[])
            .await
            .map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to read migration tracker: {e}"))
            })?;
        let count: i64 = match rows.first() {
            Some(row) => row.try_get("n").map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to read migration count: {e}"))
            })?,
            None => 0,
        };
        Ok(count.max(0) as u64)
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

        let conn =
            self.pool.get().await.map_err(|e| {
                AuthzError::DatabaseError(format!("Failed to acquire connection: {e}"))
            })?;

        // Delete audit logs older than 90 days
        let deleted = conn
            .execute(
                "DELETE FROM authz_audit_log WHERE timestamp < datetime('now', '-90 days')",
                &[],
            )
            .await
            .map_err(|e| AuthzError::DatabaseError(format!("Cleanup failed: {e}")))?;

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_migrations() {
        // Pure-Rust in-memory SQLite pool (Limbo backend). Sequential pooled
        // checkouts reuse the same connection, so the in-memory schema persists
        // across calls within this test.
        let pool = oxisql_pool::sqlite::new_sqlite_compat_pool(":memory:", 4)
            .await
            .expect("failed to create sqlite pool");
        let manager = MigrationManager::new(pool);

        // First run applies the single init migration and passes the gate.
        manager
            .run_migrations()
            .await
            .expect("run_migrations failed");

        // Idempotency: a second run applies nothing new and must still pass the
        // count gate (the tracker already records the init migration).
        manager
            .run_migrations()
            .await
            .expect("second run_migrations failed");
    }
}
