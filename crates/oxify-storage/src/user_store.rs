//! User storage implementation

use crate::models::UserRow;
use crate::{DatabasePool, Result};
use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

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

        sqlx::query(
            r"
            INSERT INTO users (id, username, email, password_hash, full_name, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&id_str)
        .bind(&username)
        .bind(&email)
        .bind(&password_hash)
        .bind(&full_name)
        .bind(&now)
        .bind(&now)
        .execute(self.pool.pool())
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
        let row = sqlx::query(
            r"
            SELECT id, username, email, password_hash, full_name, created_at, updated_at, last_login, is_active, is_verified
            FROM users
            WHERE id = ?
            ",
        )
        .bind(id.to_string())
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(UserRow {
                id: row.get("id"),
                username: row.get("username"),
                email: row.get("email"),
                password_hash: row.get("password_hash"),
                full_name: row.get("full_name"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                last_login: row.get("last_login"),
                is_active: row.get("is_active"),
                is_verified: row.get("is_verified"),
            })),
            None => Ok(None),
        }
    }

    /// Get a user by email
    pub async fn get_by_email(&self, email: &str) -> Result<Option<UserRow>> {
        let row = sqlx::query(
            r"
            SELECT id, username, email, password_hash, full_name, created_at, updated_at, last_login, is_active, is_verified
            FROM users
            WHERE email = ?
            ",
        )
        .bind(email)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(UserRow {
                id: row.get("id"),
                username: row.get("username"),
                email: row.get("email"),
                password_hash: row.get("password_hash"),
                full_name: row.get("full_name"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                last_login: row.get("last_login"),
                is_active: row.get("is_active"),
                is_verified: row.get("is_verified"),
            })),
            None => Ok(None),
        }
    }

    /// Get a user by username
    pub async fn get_by_username(&self, username: &str) -> Result<Option<UserRow>> {
        let row = sqlx::query(
            r"
            SELECT id, username, email, password_hash, full_name, created_at, updated_at, last_login, is_active, is_verified
            FROM users
            WHERE username = ?
            ",
        )
        .bind(username)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(UserRow {
                id: row.get("id"),
                username: row.get("username"),
                email: row.get("email"),
                password_hash: row.get("password_hash"),
                full_name: row.get("full_name"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                last_login: row.get("last_login"),
                is_active: row.get("is_active"),
                is_verified: row.get("is_verified"),
            })),
            None => Ok(None),
        }
    }

    /// Update user's last login timestamp
    pub async fn update_last_login(&self, id: &Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r"
            UPDATE users
            SET last_login = ?
            WHERE id = ?
            ",
        )
        .bind(&now)
        .bind(id.to_string())
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Update user's full name
    #[allow(dead_code)]
    pub async fn update_full_name(&self, id: &Uuid, full_name: Option<String>) -> Result<()> {
        sqlx::query(
            r"
            UPDATE users
            SET full_name = ?
            WHERE id = ?
            ",
        )
        .bind(full_name)
        .bind(id.to_string())
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Delete a user
    #[allow(dead_code)]
    pub async fn delete(&self, id: &Uuid) -> Result<()> {
        sqlx::query(
            r"
            DELETE FROM users
            WHERE id = ?
            ",
        )
        .bind(id.to_string())
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// List all users
    #[allow(dead_code)]
    pub async fn list(&self) -> Result<Vec<UserRow>> {
        let rows = sqlx::query(
            r"
            SELECT id, username, email, password_hash, full_name, created_at, updated_at, last_login, is_active, is_verified
            FROM users
            ORDER BY created_at DESC
            ",
        )
        .fetch_all(self.pool.pool())
        .await?;

        let users = rows
            .into_iter()
            .map(|row| UserRow {
                id: row.get("id"),
                username: row.get("username"),
                email: row.get("email"),
                password_hash: row.get("password_hash"),
                full_name: row.get("full_name"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                last_login: row.get("last_login"),
                is_active: row.get("is_active"),
                is_verified: row.get("is_verified"),
            })
            .collect();

        Ok(users)
    }

    /// Get user roles
    pub async fn get_roles(&self, user_id: &str) -> Result<Vec<String>> {
        let roles = sqlx::query(
            r"
            SELECT role
            FROM user_roles
            WHERE user_id = ?
            ORDER BY granted_at
            ",
        )
        .bind(user_id)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(roles.into_iter().map(|row| row.get("role")).collect())
    }

    /// Add role to user
    pub async fn add_role(&self, user_id: &Uuid, role: String) -> Result<()> {
        sqlx::query(
            r"
            INSERT OR IGNORE INTO user_roles (user_id, role)
            VALUES (?, ?)
            ",
        )
        .bind(user_id.to_string())
        .bind(role)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Remove role from user
    #[allow(dead_code)]
    pub async fn remove_role(&self, user_id: &Uuid, role: &str) -> Result<()> {
        sqlx::query(
            r"
            DELETE FROM user_roles
            WHERE user_id = ? AND role = ?
            ",
        )
        .bind(user_id.to_string())
        .bind(role)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Get user permissions
    pub async fn get_permissions(&self, user_id: &str) -> Result<Vec<String>> {
        let permissions = sqlx::query(
            r"
            SELECT permission
            FROM user_permissions
            WHERE user_id = ?
            ORDER BY granted_at
            ",
        )
        .bind(user_id)
        .fetch_all(self.pool.pool())
        .await?;

        Ok(permissions
            .into_iter()
            .map(|row| row.get("permission"))
            .collect())
    }

    /// Add permission to user
    pub async fn add_permission(&self, user_id: &Uuid, permission: String) -> Result<()> {
        sqlx::query(
            r"
            INSERT OR IGNORE INTO user_permissions (user_id, permission)
            VALUES (?, ?)
            ",
        )
        .bind(user_id.to_string())
        .bind(permission)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Remove permission from user
    #[allow(dead_code)]
    pub async fn remove_permission(&self, user_id: &Uuid, permission: &str) -> Result<()> {
        sqlx::query(
            r"
            DELETE FROM user_permissions
            WHERE user_id = ? AND permission = ?
            ",
        )
        .bind(user_id.to_string())
        .bind(permission)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Check if user exists by email
    pub async fn exists_by_email(&self, email: &str) -> Result<bool> {
        let row = sqlx::query(
            r"
            SELECT COUNT(*) as count
            FROM users
            WHERE email = ?
            ",
        )
        .bind(email)
        .fetch_one(self.pool.pool())
        .await?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }
}
