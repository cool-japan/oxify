//! API-specific user types

use oxify_authn::Permission;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API user with extended fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub permissions: Vec<Permission>,
    pub full_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl ApiUser {
    /// Create a new API user
    pub fn new(username: String, email: String, password_hash: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            username,
            email,
            password_hash,
            roles: vec!["user".to_string()],
            permissions: vec![Permission::Read],
            full_name: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Convert to oxify-authn User for JWT generation
    #[allow(dead_code)]
    pub fn to_authn_user(&self) -> oxify_authn::User {
        oxify_authn::User {
            username: self.username.clone(),
            roles: self.roles.clone(),
            email: Some(self.email.clone()),
            full_name: self.full_name.clone(),
            last_login: Some(chrono::Utc::now()),
            permissions: self.permissions.clone(),
        }
    }

    /// Create from database UserRow with roles and permissions
    pub fn from_db_row(
        row: oxify_storage::UserRow,
        roles: Vec<String>,
        permissions: Vec<String>,
    ) -> Self {
        // Convert permission strings to Permission enum
        let permissions = permissions
            .iter()
            .filter_map(|p| match p.as_str() {
                "Read" => Some(Permission::Read),
                "Write" => Some(Permission::Write),
                "Admin" => Some(Permission::Admin),
                "GlobalAdmin" => Some(Permission::GlobalAdmin),
                "GlobalRead" => Some(Permission::GlobalRead),
                "GlobalWrite" => Some(Permission::GlobalWrite),
                "DatasetCreate" => Some(Permission::DatasetCreate),
                "DatasetDelete" => Some(Permission::DatasetDelete),
                "DatasetManage" => Some(Permission::DatasetManage),
                "UserManage" => Some(Permission::UserManage),
                "SystemConfig" => Some(Permission::SystemConfig),
                "SystemMetrics" => Some(Permission::SystemMetrics),
                _ => None,
            })
            .collect();

        // Parse UUID from string
        let id = Uuid::parse_str(&row.id).unwrap_or_else(|_| Uuid::new_v4());

        // Parse DateTime from string (RFC3339 format)
        let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        let updated_at = chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        Self {
            id,
            username: row.username,
            email: row.email,
            password_hash: row.password_hash,
            roles,
            permissions,
            full_name: row.full_name,
            created_at,
            updated_at,
        }
    }
}
