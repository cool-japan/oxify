//! PostgreSQL Extension Manager
//!
//! This module provides utilities for managing PostgreSQL extensions programmatically.
//! Extensions add additional functionality to PostgreSQL, such as full-text search,
//! JSON operations, encryption, and more.
//!
//! # Common Extensions
//!
//! - **pg_trgm**: Trigram similarity for fuzzy text search
//! - **uuid-ossp**: UUID generation functions
//! - **hstore**: Key-value store
//! - **pgcrypto**: Cryptographic functions
//! - **postgis**: Geographic information systems
//! - **pg_stat_statements**: Query performance statistics
//! - **btree_gin**: GIN indexes for B-tree data types
//! - **btree_gist**: GiST indexes for B-tree data types
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::ExtensionManager;
//!
//! let ext_manager = ExtensionManager::new(pool.clone());
//!
//! // Install an extension
//! ext_manager.create_extension("pg_trgm").await?;
//!
//! // List installed extensions
//! let extensions = ext_manager.list_extensions().await?;
//! for ext in extensions {
//!     println!("{} version {}", ext.name, ext.version);
//! }
//!
//! // Check if extension is available
//! if ext_manager.is_extension_available("pgcrypto").await? {
//!     ext_manager.create_extension_if_not_exists("pgcrypto").await?;
//! }
//! ```

use crate::{Result, StorageError};
use sqlx::PgPool;
use std::sync::Arc;

/// Information about an installed PostgreSQL extension
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExtensionInfo {
    /// Extension name
    pub name: String,
    /// Installed version
    pub version: String,
    /// Schema where extension is installed
    pub schema: String,
    /// Extension description (if available)
    pub comment: Option<String>,
}

/// Information about an available (but not necessarily installed) extension
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AvailableExtension {
    /// Extension name
    pub name: String,
    /// Default version that would be installed
    pub default_version: String,
    /// Extension comment/description
    pub comment: Option<String>,
}

/// PostgreSQL Extension Manager
///
/// Provides methods for creating, dropping, and managing PostgreSQL extensions.
#[derive(Clone)]
pub struct ExtensionManager {
    pool: Arc<PgPool>,
}

impl ExtensionManager {
    /// Create a new extension manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // ========================================================================
    // Extension Creation and Removal
    // ========================================================================

    /// Create (install) a PostgreSQL extension
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Extension is not available in the PostgreSQL installation
    /// - Insufficient permissions
    /// - Extension is already installed (use `create_extension_if_not_exists` to avoid this)
    #[tracing::instrument(skip(self))]
    pub async fn create_extension(&self, extension_name: &str) -> Result<()> {
        let sql = format!("CREATE EXTENSION {}", extension_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Create extension if it doesn't already exist
    #[tracing::instrument(skip(self))]
    pub async fn create_extension_if_not_exists(&self, extension_name: &str) -> Result<()> {
        let sql = format!("CREATE EXTENSION IF NOT EXISTS {}", extension_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Create extension in a specific schema
    #[tracing::instrument(skip(self))]
    pub async fn create_extension_in_schema(
        &self,
        extension_name: &str,
        schema_name: &str,
    ) -> Result<()> {
        let sql = format!("CREATE EXTENSION {} SCHEMA {}", extension_name, schema_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Create extension with a specific version
    #[tracing::instrument(skip(self))]
    pub async fn create_extension_version(
        &self,
        extension_name: &str,
        version: &str,
    ) -> Result<()> {
        let sql = format!("CREATE EXTENSION {} VERSION '{}'", extension_name, version);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop (uninstall) a PostgreSQL extension
    ///
    /// # Warning
    ///
    /// This will remove all objects created by the extension.
    /// Use `drop_extension_cascade` if the extension has dependencies.
    #[tracing::instrument(skip(self))]
    pub async fn drop_extension(&self, extension_name: &str) -> Result<()> {
        let sql = format!("DROP EXTENSION {}", extension_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop extension if it exists
    #[tracing::instrument(skip(self))]
    pub async fn drop_extension_if_exists(&self, extension_name: &str) -> Result<()> {
        let sql = format!("DROP EXTENSION IF EXISTS {}", extension_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop extension with CASCADE to remove dependent objects
    #[tracing::instrument(skip(self))]
    pub async fn drop_extension_cascade(&self, extension_name: &str) -> Result<()> {
        let sql = format!("DROP EXTENSION {} CASCADE", extension_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Extension Updates
    // ========================================================================

    /// Update an extension to the latest version
    #[tracing::instrument(skip(self))]
    pub async fn update_extension(&self, extension_name: &str) -> Result<()> {
        let sql = format!("ALTER EXTENSION {} UPDATE", extension_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Update an extension to a specific version
    #[tracing::instrument(skip(self))]
    pub async fn update_extension_to_version(
        &self,
        extension_name: &str,
        version: &str,
    ) -> Result<()> {
        let sql = format!("ALTER EXTENSION {} UPDATE TO '{}'", extension_name, version);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Extension Information
    // ========================================================================

    /// List all installed extensions
    #[tracing::instrument(skip(self))]
    pub async fn list_extensions(&self) -> Result<Vec<ExtensionInfo>> {
        let extensions: Vec<ExtensionInfo> = sqlx::query_as(
            r"
            SELECT
                e.extname as name,
                e.extversion as version,
                n.nspname as schema,
                obj_description(e.oid, 'pg_extension') as comment
            FROM pg_extension e
            JOIN pg_namespace n ON e.extnamespace = n.oid
            WHERE n.nspname != 'pg_catalog'
            ORDER BY e.extname
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(extensions)
    }

    /// List all available extensions (installed and not installed)
    #[tracing::instrument(skip(self))]
    pub async fn list_available_extensions(&self) -> Result<Vec<AvailableExtension>> {
        let extensions: Vec<AvailableExtension> = sqlx::query_as(
            r"
            SELECT
                name,
                default_version,
                comment
            FROM pg_available_extensions
            ORDER BY name
            ",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(extensions)
    }

    /// Get information about a specific installed extension
    #[tracing::instrument(skip(self))]
    pub async fn get_extension_info(&self, extension_name: &str) -> Result<Option<ExtensionInfo>> {
        let extension: Option<ExtensionInfo> = sqlx::query_as(
            r"
            SELECT
                e.extname as name,
                e.extversion as version,
                n.nspname as schema,
                obj_description(e.oid, 'pg_extension') as comment
            FROM pg_extension e
            JOIN pg_namespace n ON e.extnamespace = n.oid
            WHERE e.extname = $1
            ",
        )
        .bind(extension_name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(extension)
    }

    /// Check if an extension is installed
    #[tracing::instrument(skip(self))]
    pub async fn is_extension_installed(&self, extension_name: &str) -> Result<bool> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM pg_extension WHERE extname = $1
            )",
        )
        .bind(extension_name)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Check if an extension is available (can be installed)
    #[tracing::instrument(skip(self))]
    pub async fn is_extension_available(&self, extension_name: &str) -> Result<bool> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM pg_available_extensions WHERE name = $1
            )",
        )
        .bind(extension_name)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    // ========================================================================
    // Common Extension Helpers
    // ========================================================================

    /// Install commonly used extensions for a production application
    ///
    /// Installs:
    /// - uuid-ossp: UUID generation
    /// - pg_trgm: Fuzzy text search
    /// - btree_gin: GIN indexes for better performance
    #[tracing::instrument(skip(self))]
    pub async fn install_common_extensions(&self) -> Result<Vec<String>> {
        let mut installed = Vec::new();

        let common_extensions = vec!["uuid-ossp", "pg_trgm", "btree_gin"];

        for ext in common_extensions {
            if self.create_extension_if_not_exists(ext).await.is_ok() {
                installed.push(ext.to_string());
            }
        }

        Ok(installed)
    }

    /// Get the version of a specific installed extension
    #[tracing::instrument(skip(self))]
    pub async fn get_extension_version(&self, extension_name: &str) -> Result<Option<String>> {
        let result: Option<(String,)> =
            sqlx::query_as("SELECT extversion FROM pg_extension WHERE extname = $1")
                .bind(extension_name)
                .fetch_optional(self.pool.as_ref())
                .await
                .map_err(StorageError::Database)?;

        Ok(result.map(|r| r.0))
    }

    /// Check if an extension update is available
    #[tracing::instrument(skip(self))]
    pub async fn is_update_available(&self, extension_name: &str) -> Result<bool> {
        // Query available versions from pg_available_extension_versions
        let result: Option<(String, String)> = sqlx::query_as(
            r"
            SELECT
                e.extversion as current_version,
                v.version as available_version
            FROM pg_extension e
            CROSS JOIN pg_available_extension_versions v
            WHERE e.extname = $1
              AND v.name = $1
              AND v.version > e.extversion
            LIMIT 1
            ",
        )
        .bind(extension_name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_info_structure() {
        let info = ExtensionInfo {
            name: "pg_trgm".to_string(),
            version: "1.6".to_string(),
            schema: "public".to_string(),
            comment: Some("text similarity measurement and index searching".to_string()),
        };

        assert_eq!(info.name, "pg_trgm");
        assert_eq!(info.version, "1.6");
        assert_eq!(info.schema, "public");
        assert!(info.comment.is_some());
    }

    #[test]
    fn test_available_extension_structure() {
        let ext = AvailableExtension {
            name: "pgcrypto".to_string(),
            default_version: "1.3".to_string(),
            comment: Some("cryptographic functions".to_string()),
        };

        assert_eq!(ext.name, "pgcrypto");
        assert_eq!(ext.default_version, "1.3");
        assert!(ext.comment.is_some());
    }
}
