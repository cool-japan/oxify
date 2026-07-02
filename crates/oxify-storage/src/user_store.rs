//! User storage implementation

use crate::models::UserRow;
use crate::row_ext::RowExt;
use crate::{DatabasePool, Result};
use chrono::Utc;
use oxisql_core::{Connection, OxiSqlError, Row};
use uuid::Uuid;

/// Map a `users` row into a [`UserRow`].
///
/// `is_active` and `is_verified` are declared as SQLite `INTEGER` columns
/// (0/1) rather than a native `BOOLEAN` type -- the SQLite-compat backend has
/// no declared-type enrichment rule for booleans (see
/// `oxisql_sqlite_compat::types::limbo_to_core_typed`), so these columns come
/// back as `Value::I64` rather than `Value::Bool`. That means they can't be
/// pulled through the generic `row_to!` macro (which would try a direct
/// `Value::Bool` extraction and fail with `OxiSqlError::TypeMismatch`); they
/// are coerced explicitly here instead, the same way other numeric-affinity
/// columns are massaged in the other converted stores.
fn map_user_row(row: &Row) -> std::result::Result<UserRow, OxiSqlError> {
    Ok(UserRow {
        id: row.col("id")?,
        username: row.col("username")?,
        email: row.col("email")?,
        password_hash: row.col("password_hash")?,
        full_name: row.col("full_name")?,
        created_at: row.col("created_at")?,
        updated_at: row.col("updated_at")?,
        last_login: row.col("last_login")?,
        is_active: row.col::<i64>("is_active")? != 0,
        is_verified: row.col::<i64>("is_verified")? != 0,
    })
}

/// User storage operations
pub struct UserStore {
    pool: DatabasePool,
}

impl UserStore {
    /// Create a new user store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Create a new user
    pub async fn create(
        &self,
        id: Uuid,
        username: String,
        email: String,
        password_hash: String,
        full_name: Option<String>,
    ) -> Result<UserRow> {
        let id_str = id.to_string();
        let now = Utc::now().to_rfc3339();

        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            INSERT INTO users (id, username, email, password_hash, full_name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ",
            &[
                &id_str,
                &username,
                &email,
                &password_hash,
                &full_name,
                &now,
                &now,
            ],
        )
        .await?;

        Ok(UserRow {
            id: id_str,
            username,
            email,
            password_hash,
            full_name,
            created_at: now.clone(),
            updated_at: now,
            last_login: None,
            is_active: true,
            is_verified: false,
        })
    }

    /// Get a user by ID
    pub async fn get(&self, id: &Uuid) -> Result<Option<UserRow>> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT id, username, email, password_hash, full_name, created_at, updated_at, last_login, is_active, is_verified
                FROM users
                WHERE id = $1
                ",
                &[&id_str],
            )
            .await?;

        match rows.first() {
            Some(row) => Ok(Some(map_user_row(row)?)),
            None => Ok(None),
        }
    }

    /// Get a user by email
    pub async fn get_by_email(&self, email: &str) -> Result<Option<UserRow>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT id, username, email, password_hash, full_name, created_at, updated_at, last_login, is_active, is_verified
                FROM users
                WHERE email = $1
                ",
                &[&email],
            )
            .await?;

        match rows.first() {
            Some(row) => Ok(Some(map_user_row(row)?)),
            None => Ok(None),
        }
    }

    /// Get a user by username
    pub async fn get_by_username(&self, username: &str) -> Result<Option<UserRow>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT id, username, email, password_hash, full_name, created_at, updated_at, last_login, is_active, is_verified
                FROM users
                WHERE username = $1
                ",
                &[&username],
            )
            .await?;

        match rows.first() {
            Some(row) => Ok(Some(map_user_row(row)?)),
            None => Ok(None),
        }
    }

    /// Update user's last login timestamp
    pub async fn update_last_login(&self, id: &Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            UPDATE users
            SET last_login = $1
            WHERE id = $2
            ",
            &[&now, &id_str],
        )
        .await?;

        Ok(())
    }

    /// Update user's full name
    #[allow(dead_code)]
    pub async fn update_full_name(&self, id: &Uuid, full_name: Option<String>) -> Result<()> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            UPDATE users
            SET full_name = $1
            WHERE id = $2
            ",
            &[&full_name, &id_str],
        )
        .await?;

        Ok(())
    }

    /// Delete a user
    #[allow(dead_code)]
    pub async fn delete(&self, id: &Uuid) -> Result<()> {
        let id_str = id.to_string();
        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            DELETE FROM users
            WHERE id = $1
            ",
            &[&id_str],
        )
        .await?;

        Ok(())
    }

    /// List all users
    #[allow(dead_code)]
    pub async fn list(&self) -> Result<Vec<UserRow>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT id, username, email, password_hash, full_name, created_at, updated_at, last_login, is_active, is_verified
                FROM users
                ORDER BY created_at DESC
                ",
                &[],
            )
            .await?;

        let users = rows
            .iter()
            .map(map_user_row)
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(users)
    }

    /// Get user roles
    pub async fn get_roles(&self, user_id: &str) -> Result<Vec<String>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT role
                FROM user_roles
                WHERE user_id = $1
                ORDER BY granted_at
                ",
                &[&user_id],
            )
            .await?;

        let roles = rows
            .iter()
            .map(|row| row.col::<String>("role"))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(roles)
    }

    /// Add role to user
    pub async fn add_role(&self, user_id: &Uuid, role: String) -> Result<()> {
        let user_id_str = user_id.to_string();
        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            INSERT OR IGNORE INTO user_roles (user_id, role)
            VALUES ($1, $2)
            ",
            &[&user_id_str, &role],
        )
        .await?;

        Ok(())
    }

    /// Remove role from user
    #[allow(dead_code)]
    pub async fn remove_role(&self, user_id: &Uuid, role: &str) -> Result<()> {
        let user_id_str = user_id.to_string();
        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            DELETE FROM user_roles
            WHERE user_id = $1 AND role = $2
            ",
            &[&user_id_str, &role],
        )
        .await?;

        Ok(())
    }

    /// Get user permissions
    pub async fn get_permissions(&self, user_id: &str) -> Result<Vec<String>> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT permission
                FROM user_permissions
                WHERE user_id = $1
                ORDER BY granted_at
                ",
                &[&user_id],
            )
            .await?;

        let permissions = rows
            .iter()
            .map(|row| row.col::<String>("permission"))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(permissions)
    }

    /// Add permission to user
    pub async fn add_permission(&self, user_id: &Uuid, permission: String) -> Result<()> {
        let user_id_str = user_id.to_string();
        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            INSERT OR IGNORE INTO user_permissions (user_id, permission)
            VALUES ($1, $2)
            ",
            &[&user_id_str, &permission],
        )
        .await?;

        Ok(())
    }

    /// Remove permission from user
    #[allow(dead_code)]
    pub async fn remove_permission(&self, user_id: &Uuid, permission: &str) -> Result<()> {
        let user_id_str = user_id.to_string();
        let conn = self.pool.acquire().await?;
        conn.execute(
            r"
            DELETE FROM user_permissions
            WHERE user_id = $1 AND permission = $2
            ",
            &[&user_id_str, &permission],
        )
        .await?;

        Ok(())
    }

    /// Check if user exists by email
    pub async fn exists_by_email(&self, email: &str) -> Result<bool> {
        let conn = self.pool.acquire().await?;
        let rows = conn
            .query(
                r"
                SELECT COUNT(*) as count
                FROM users
                WHERE email = $1
                ",
                &[&email],
            )
            .await?;

        let count: i64 = match rows.first() {
            Some(row) => row.col("count")?,
            None => 0,
        };
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Migration files (from the crate's `migrations/` directory) required
    /// to bring up the `users`, `user_roles`, and `user_permissions` tables
    /// that [`UserStore`] operates on.
    ///
    /// Tests stage only this explicit whitelist into a scratch directory
    /// rather than pointing [`oxisql_migrate::runner::MigrationRunner`] at
    /// the crate's full `migrations/` directory (the way
    /// [`DatabasePool::migrate`] does) because, as of this writing,
    /// `migrations/20251201000005__performance_indexes.sql` creates an
    /// index on a `created_at` column that does not exist on the
    /// `executions` table (which only has `started_at` -- see
    /// `20251130000001__init.sql`). That is a pre-existing bug unrelated to
    /// this file's sqlx -> oxisql conversion (the identical mistake already
    /// existed in the pre-conversion `sql/001_performance_indexes.sql`) and
    /// aborts *every* migration run against a fresh database -- including
    /// this one -- before the `users` table ever gets created. Whitelisting
    /// keeps these tests independent of that unrelated breakage while still
    /// exercising the real on-disk migration files (rather than hand-rolled
    /// schema SQL) for the tables `UserStore` actually touches.
    const REQUIRED_MIGRATIONS: &[&str] = &["20251130000001__init.sql", "20251130000002__users.sql"];

    /// Build a fresh, isolated in-memory database and apply the migrations
    /// [`UserStore`] depends on against it.
    ///
    /// The pool is deliberately sized to a single connection: OxiSQL's
    /// SQLite-compat pool opens an independent `:memory:` database per pool
    /// slot (there is no shared cache between slots), so a pool with more
    /// than one connection would non-deterministically hand back a
    /// completely empty database on some acquisitions. Pinning
    /// `max_connections`/`min_connections` to `1` guarantees every
    /// `self.pool.acquire()` call in `UserStore` observes the same
    /// underlying connection/database for the lifetime of the test.
    async fn setup_test_pool() -> Result<DatabasePool> {
        let config = crate::DatabaseConfig {
            database_url: ":memory:".to_string(),
            max_connections: 1,
            min_connections: 1,
        };
        let pool = DatabasePool::new(config).await?;

        // Stage the required migration files into a private scratch
        // directory under the system temp dir (rather than mutating or
        // reading around the shared `migrations/` directory in place).
        let staged_dir = std::env::temp_dir().join(format!(
            "oxify-user-store-test-migrations-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&staged_dir).map_err(|e| {
            crate::StorageError::Migration(format!(
                "failed to create scratch migrations dir {staged_dir:?}: {e}"
            ))
        })?;
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        for file_name in REQUIRED_MIGRATIONS {
            std::fs::copy(src_dir.join(file_name), staged_dir.join(file_name)).map_err(|e| {
                crate::StorageError::Migration(format!(
                    "failed to stage migration {file_name}: {e}"
                ))
            })?;
        }

        // Apply migrations directly (rather than through
        // `DatabasePool::migrate`, which discards the applied count) so we
        // can gate on the number of migrations actually applied. This is a
        // deliberate safety net against the migration-filename landmine:
        // files that don't match the exact
        // "<14-digit-timestamp>__<name>.sql" pattern are silently skipped
        // rather than erroring, which would otherwise leave the `users` /
        // `user_roles` / `user_permissions` tables missing and every query
        // below failing with a confusing "no such table" error instead of a
        // clear assertion failure here.
        let conn = pool.acquire().await?;
        let mut runner = oxisql_migrate::runner::MigrationRunner::new(&staged_dir);
        let applied = runner
            .run_with_conn(&*conn)
            .await
            .map_err(|e| crate::StorageError::Migration(e.to_string()))?;
        assert_eq!(
            applied,
            REQUIRED_MIGRATIONS.len(),
            "expected every staged migration to be applied against the fresh in-memory database"
        );
        drop(conn);

        let _ = std::fs::remove_dir_all(&staged_dir);

        Ok(pool)
    }

    #[tokio::test]
    async fn create_then_get_roundtrip() -> Result<()> {
        let pool = setup_test_pool().await?;
        let store = UserStore::new(pool);

        let id = Uuid::new_v4();
        let created = store
            .create(
                id,
                "alice".to_string(),
                "alice@example.com".to_string(),
                "hashed-password".to_string(),
                Some("Alice Doe".to_string()),
            )
            .await?;

        assert_eq!(created.id, id.to_string());
        assert_eq!(created.username, "alice");
        assert_eq!(created.email, "alice@example.com");
        assert_eq!(created.password_hash, "hashed-password");
        assert_eq!(created.full_name, Some("Alice Doe".to_string()));
        assert!(created.is_active);
        assert!(!created.is_verified);
        assert!(created.last_login.is_none());

        let fetched = store
            .get(&id)
            .await?
            .expect("user should be fetchable immediately after create");

        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.username, created.username);
        assert_eq!(fetched.email, created.email);
        assert_eq!(fetched.password_hash, created.password_hash);
        assert_eq!(fetched.full_name, created.full_name);
        assert_eq!(fetched.created_at, created.created_at);
        assert_eq!(fetched.updated_at, created.updated_at);
        assert_eq!(fetched.last_login, None);
        assert!(fetched.is_active);
        assert!(!fetched.is_verified);

        Ok(())
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() -> Result<()> {
        let pool = setup_test_pool().await?;
        let store = UserStore::new(pool);

        let missing_id = Uuid::new_v4();
        let result = store.get(&missing_id).await?;
        assert!(
            result.is_none(),
            "fetching a nonexistent user must return Ok(None), not an error or a phantom row"
        );

        Ok(())
    }

    #[tokio::test]
    async fn get_by_email_and_username_roundtrip() -> Result<()> {
        let pool = setup_test_pool().await?;
        let store = UserStore::new(pool);

        let id = Uuid::new_v4();
        store
            .create(
                id,
                "bob".to_string(),
                "bob@example.com".to_string(),
                "bob-hash".to_string(),
                None,
            )
            .await?;

        let by_email = store
            .get_by_email("bob@example.com")
            .await?
            .expect("user should be found by email");
        assert_eq!(by_email.id, id.to_string());
        assert_eq!(by_email.full_name, None);

        let by_username = store
            .get_by_username("bob")
            .await?
            .expect("user should be found by username");
        assert_eq!(by_username.id, id.to_string());

        assert!(store.get_by_email("nobody@example.com").await?.is_none());
        assert!(store.get_by_username("nobody").await?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn update_last_login_and_full_name_updates_fields() -> Result<()> {
        let pool = setup_test_pool().await?;
        let store = UserStore::new(pool);

        let id = Uuid::new_v4();
        store
            .create(
                id,
                "carol".to_string(),
                "carol@example.com".to_string(),
                "carol-hash".to_string(),
                None,
            )
            .await?;

        store.update_last_login(&id).await?;
        let after_login = store
            .get(&id)
            .await?
            .expect("user must still exist after updating last_login");
        assert!(after_login.last_login.is_some());

        store
            .update_full_name(&id, Some("Carol Danvers".to_string()))
            .await?;
        let after_rename = store
            .get(&id)
            .await?
            .expect("user must still exist after updating full_name");
        assert_eq!(after_rename.full_name, Some("Carol Danvers".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn delete_removes_user() -> Result<()> {
        let pool = setup_test_pool().await?;
        let store = UserStore::new(pool);

        let id = Uuid::new_v4();
        store
            .create(
                id,
                "dave".to_string(),
                "dave@example.com".to_string(),
                "dave-hash".to_string(),
                None,
            )
            .await?;
        assert!(store.get(&id).await?.is_some());

        store.delete(&id).await?;
        assert!(store.get(&id).await?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn roles_and_permissions_roundtrip() -> Result<()> {
        let pool = setup_test_pool().await?;
        let store = UserStore::new(pool);

        let id = Uuid::new_v4();
        store
            .create(
                id,
                "erin".to_string(),
                "erin@example.com".to_string(),
                "erin-hash".to_string(),
                None,
            )
            .await?;

        assert!(store.get_roles(&id.to_string()).await?.is_empty());
        assert!(store.get_permissions(&id.to_string()).await?.is_empty());

        store.add_role(&id, "admin".to_string()).await?;
        store.add_role(&id, "editor".to_string()).await?;
        // Adding the same role twice must not fail or duplicate (INSERT OR IGNORE).
        store.add_role(&id, "admin".to_string()).await?;

        store
            .add_permission(&id, "workflows:read".to_string())
            .await?;
        store
            .add_permission(&id, "workflows:write".to_string())
            .await?;

        let mut roles = store.get_roles(&id.to_string()).await?;
        roles.sort();
        assert_eq!(roles, vec!["admin".to_string(), "editor".to_string()]);

        let mut permissions = store.get_permissions(&id.to_string()).await?;
        permissions.sort();
        assert_eq!(
            permissions,
            vec!["workflows:read".to_string(), "workflows:write".to_string()]
        );

        store.remove_role(&id, "editor").await?;
        assert_eq!(
            store.get_roles(&id.to_string()).await?,
            vec!["admin".to_string()]
        );

        store.remove_permission(&id, "workflows:read").await?;
        assert_eq!(
            store.get_permissions(&id.to_string()).await?,
            vec!["workflows:write".to_string()]
        );

        Ok(())
    }

    #[tokio::test]
    async fn exists_by_email_reflects_creation_and_deletion() -> Result<()> {
        let pool = setup_test_pool().await?;
        let store = UserStore::new(pool);

        assert!(!store.exists_by_email("frank@example.com").await?);

        let id = Uuid::new_v4();
        store
            .create(
                id,
                "frank".to_string(),
                "frank@example.com".to_string(),
                "frank-hash".to_string(),
                None,
            )
            .await?;
        assert!(store.exists_by_email("frank@example.com").await?);

        store.delete(&id).await?;
        assert!(!store.exists_by_email("frank@example.com").await?);

        Ok(())
    }

    #[tokio::test]
    async fn list_returns_all_users() -> Result<()> {
        let pool = setup_test_pool().await?;
        let store = UserStore::new(pool);

        assert!(store.list().await?.is_empty());

        let id_a = Uuid::new_v4();
        store
            .create(
                id_a,
                "grace".to_string(),
                "grace@example.com".to_string(),
                "grace-hash".to_string(),
                None,
            )
            .await?;
        let id_b = Uuid::new_v4();
        store
            .create(
                id_b,
                "heidi".to_string(),
                "heidi@example.com".to_string(),
                "heidi-hash".to_string(),
                None,
            )
            .await?;

        let users = store.list().await?;
        assert_eq!(users.len(), 2);
        let mut ids: Vec<String> = users.into_iter().map(|u| u.id).collect();
        ids.sort();
        let mut expected = vec![id_a.to_string(), id_b.to_string()];
        expected.sort();
        assert_eq!(ids, expected);

        Ok(())
    }
}
