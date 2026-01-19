//! Database model types for SQLite

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Database row for workflows (SQLite compatible)
/// Note: tags is stored as JSON string, not array
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct WorkflowRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
    pub definition: String,   // JSON string
    pub tags: Option<String>, // JSON array string
}

/// Database row for executions (SQLite compatible)
#[derive(Debug, Clone, FromRow)]
pub struct ExecutionRow {
    pub id: String,
    pub workflow_id: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub state: String,
    pub context: String,      // JSON string
    pub node_results: String, // JSON string
    pub variables: String,    // JSON string
    pub error_message: Option<String>,
}

/// Database row for users (SQLite compatible)
#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub full_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login: Option<String>,
    pub is_active: bool,
    pub is_verified: bool,
}

/// Database row for user roles
#[derive(Debug, Clone, FromRow)]
pub struct UserRoleRow {
    pub user_id: String,
    pub role: String,
    pub granted_at: String,
}

/// Database row for user permissions
#[derive(Debug, Clone, FromRow)]
pub struct UserPermissionRow {
    pub user_id: String,
    pub permission: String,
    pub granted_at: String,
}
