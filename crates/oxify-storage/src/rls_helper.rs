//! PostgreSQL Row-Level Security (RLS) Helper
//!
//! This module provides utilities for managing PostgreSQL Row-Level Security policies,
//! which enable automatic row filtering based on the current database user or application context.
//!
//! # Use Cases
//!
//! - **Multi-tenant SaaS applications**: Isolate data by tenant/organization
//! - **User data privacy**: Users can only see their own data
//! - **Role-based access control**: Different access levels for different roles
//! - **Compliance requirements**: Enforce data access policies at the database level
//!
//! # How RLS Works
//!
//! Row-Level Security adds an automatic WHERE clause to every query based on defined policies.
//! Instead of writing `WHERE user_id = current_user_id()` in every query, RLS does it automatically.
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{RlsManager, RlsPolicy};
//!
//! let rls = RlsManager::new(pool.clone());
//!
//! // Enable RLS on the workflows table
//! rls.enable_rls("workflows").await?;
//!
//! // Create a policy: users can only see their own workflows
//! let policy = RlsPolicy::new("user_isolation_policy", "workflows")
//!     .for_all_operations()
//!     .using_expression("user_id = current_setting('app.current_user_id')::uuid");
//!
//! rls.create_policy(policy).await?;
//!
//! // Set the current user in your application
//! rls.set_current_user(&user_id).await?;
//!
//! // Now all queries are automatically filtered by user_id!
//! // SELECT * FROM workflows  ->  SELECT * FROM workflows WHERE user_id = '...'
//! ```

use crate::{Result, StorageError};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// PostgreSQL Row-Level Security policy operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RlsOperation {
    /// SELECT operations
    Select,
    /// INSERT operations
    Insert,
    /// UPDATE operations
    Update,
    /// DELETE operations
    Delete,
    /// All operations (SELECT, INSERT, UPDATE, DELETE)
    All,
}

impl RlsOperation {
    /// Convert to PostgreSQL policy command string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Select => "SELECT",
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::All => "ALL",
        }
    }
}

/// PostgreSQL Row-Level Security policy
///
/// Represents a policy that can be applied to a table to enforce row-level filtering.
#[derive(Debug, Clone)]
pub struct RlsPolicy {
    /// Policy name (must be unique per table)
    pub name: String,
    /// Table name the policy applies to
    pub table_name: String,
    /// Operation types this policy applies to (SELECT, INSERT, UPDATE, DELETE, ALL)
    pub operation: RlsOperation,
    /// SQL expression for the USING clause (what rows are visible)
    pub using_expression: Option<String>,
    /// SQL expression for the WITH CHECK clause (what rows can be inserted/updated)
    pub with_check_expression: Option<String>,
    /// Role this policy applies to (defaults to PUBLIC if None)
    pub role: Option<String>,
}

impl RlsPolicy {
    /// Create a new RLS policy
    pub fn new(name: impl Into<String>, table_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            table_name: table_name.into(),
            operation: RlsOperation::All,
            using_expression: None,
            with_check_expression: None,
            role: None,
        }
    }

    /// Set the operation type for this policy
    pub fn for_operation(mut self, operation: RlsOperation) -> Self {
        self.operation = operation;
        self
    }

    /// Apply policy to all operations (SELECT, INSERT, UPDATE, DELETE)
    pub fn for_all_operations(mut self) -> Self {
        self.operation = RlsOperation::All;
        self
    }

    /// Set the USING clause expression (what rows are visible)
    pub fn using_expression(mut self, expression: impl Into<String>) -> Self {
        self.using_expression = Some(expression.into());
        self
    }

    /// Set the WITH CHECK clause expression (what rows can be inserted/updated)
    pub fn with_check_expression(mut self, expression: impl Into<String>) -> Self {
        self.with_check_expression = Some(expression.into());
        self
    }

    /// Set the role this policy applies to (defaults to PUBLIC)
    pub fn for_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    /// Build the CREATE POLICY SQL statement
    pub fn to_sql(&self) -> String {
        let mut sql = format!("CREATE POLICY {} ON {}", self.name, self.table_name);

        sql.push_str(&format!(" FOR {}", self.operation.as_str()));

        if let Some(ref role) = self.role {
            sql.push_str(&format!(" TO {}", role));
        }

        if let Some(ref using_expr) = self.using_expression {
            sql.push_str(&format!(" USING ({})", using_expr));
        }

        if let Some(ref check_expr) = self.with_check_expression {
            sql.push_str(&format!(" WITH CHECK ({})", check_expr));
        }

        sql
    }
}

/// Row-Level Security manager
///
/// Provides methods for enabling/disabling RLS and creating/managing policies.
#[derive(Clone)]
pub struct RlsManager {
    pool: Arc<PgPool>,
}

impl RlsManager {
    /// Create a new RLS manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // ========================================================================
    // RLS Enable/Disable
    // ========================================================================

    /// Enable Row-Level Security on a table
    ///
    /// Once enabled, all queries on this table will be subject to RLS policies.
    /// Without any policies defined, the table becomes read-only for non-superusers.
    #[tracing::instrument(skip(self))]
    pub async fn enable_rls(&self, table_name: &str) -> Result<()> {
        let sql = format!("ALTER TABLE {} ENABLE ROW LEVEL SECURITY", table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Disable Row-Level Security on a table
    #[tracing::instrument(skip(self))]
    pub async fn disable_rls(&self, table_name: &str) -> Result<()> {
        let sql = format!("ALTER TABLE {} DISABLE ROW LEVEL SECURITY", table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Force Row-Level Security even for table owner
    ///
    /// By default, table owners bypass RLS. This forces RLS for all users.
    #[tracing::instrument(skip(self))]
    pub async fn force_rls(&self, table_name: &str) -> Result<()> {
        let sql = format!("ALTER TABLE {} FORCE ROW LEVEL SECURITY", table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Disable forced Row-Level Security
    #[tracing::instrument(skip(self))]
    pub async fn no_force_rls(&self, table_name: &str) -> Result<()> {
        let sql = format!("ALTER TABLE {} NO FORCE ROW LEVEL SECURITY", table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Policy Management
    // ========================================================================

    /// Create a Row-Level Security policy
    #[tracing::instrument(skip(self))]
    pub async fn create_policy(&self, policy: RlsPolicy) -> Result<()> {
        let sql = policy.to_sql();
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop a Row-Level Security policy
    #[tracing::instrument(skip(self))]
    pub async fn drop_policy(&self, policy_name: &str, table_name: &str) -> Result<()> {
        let sql = format!("DROP POLICY {} ON {}", policy_name, table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Drop a policy if it exists
    #[tracing::instrument(skip(self))]
    pub async fn drop_policy_if_exists(&self, policy_name: &str, table_name: &str) -> Result<()> {
        let sql = format!("DROP POLICY IF EXISTS {} ON {}", policy_name, table_name);
        sqlx::query(&sql)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// List all policies for a table
    #[tracing::instrument(skip(self))]
    pub async fn list_policies(&self, table_name: &str) -> Result<Vec<PolicyInfo>> {
        let policies: Vec<PolicyInfo> = sqlx::query_as(
            r"
            SELECT
                policyname as name,
                tablename as table_name,
                cmd as operation,
                qual as using_expression,
                with_check as with_check_expression
            FROM pg_policies
            WHERE tablename = $1
            ORDER BY policyname
            ",
        )
        .bind(table_name)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(policies)
    }

    /// Check if a policy exists
    #[tracing::instrument(skip(self))]
    pub async fn policy_exists(&self, policy_name: &str, table_name: &str) -> Result<bool> {
        let result: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM pg_policies
                WHERE policyname = $1 AND tablename = $2
            )",
        )
        .bind(policy_name)
        .bind(table_name)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    // ========================================================================
    // Application Context Management
    // ========================================================================

    /// Set the current user ID in the session
    ///
    /// This sets a session variable that can be used in RLS policies.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Set current user
    /// rls.set_current_user(&user_id).await?;
    ///
    /// // Create policy that uses this
    /// let policy = RlsPolicy::new("user_policy", "workflows")
    ///     .using_expression("user_id = current_setting('app.current_user_id')::uuid");
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn set_current_user(&self, user_id: &Uuid) -> Result<()> {
        sqlx::query("SELECT set_config('app.current_user_id', $1, false)")
            .bind(user_id.to_string())
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Set the current tenant/organization ID in the session
    #[tracing::instrument(skip(self))]
    pub async fn set_current_tenant(&self, tenant_id: &str) -> Result<()> {
        sqlx::query("SELECT set_config('app.current_tenant_id', $1, false)")
            .bind(tenant_id)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Set the current user role in the session
    #[tracing::instrument(skip(self))]
    pub async fn set_current_role(&self, role: &str) -> Result<()> {
        sqlx::query("SELECT set_config('app.current_role', $1, false)")
            .bind(role)
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Clear all application context variables
    #[tracing::instrument(skip(self))]
    pub async fn clear_context(&self) -> Result<()> {
        sqlx::query(
            "SELECT
                set_config('app.current_user_id', NULL, false),
                set_config('app.current_tenant_id', NULL, false),
                set_config('app.current_role', NULL, false)",
        )
        .execute(self.pool.as_ref())
        .await
        .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Policy Templates
    // ========================================================================

    /// Create a tenant isolation policy
    ///
    /// Users can only access rows belonging to their tenant.
    pub fn tenant_isolation_policy(
        policy_name: &str,
        table_name: &str,
        tenant_column: &str,
    ) -> RlsPolicy {
        RlsPolicy::new(policy_name, table_name)
            .for_all_operations()
            .using_expression(format!(
                "{} = current_setting('app.current_tenant_id')",
                tenant_column
            ))
    }

    /// Create a user isolation policy
    ///
    /// Users can only access their own rows.
    pub fn user_isolation_policy(
        policy_name: &str,
        table_name: &str,
        user_column: &str,
    ) -> RlsPolicy {
        RlsPolicy::new(policy_name, table_name)
            .for_all_operations()
            .using_expression(format!(
                "{} = current_setting('app.current_user_id')::uuid",
                user_column
            ))
    }

    /// Create a read-only policy for a specific role
    pub fn read_only_policy(policy_name: &str, table_name: &str, role: &str) -> RlsPolicy {
        RlsPolicy::new(policy_name, table_name)
            .for_operation(RlsOperation::Select)
            .for_role(role)
            .using_expression("true")
    }

    /// Create an admin bypass policy
    ///
    /// Admin users can see all rows.
    pub fn admin_bypass_policy(policy_name: &str, table_name: &str) -> RlsPolicy {
        RlsPolicy::new(policy_name, table_name)
            .for_all_operations()
            .using_expression("current_setting('app.current_role') = 'admin'")
    }
}

/// Policy information from pg_policies
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PolicyInfo {
    /// Policy name
    pub name: String,
    /// Table name
    pub table_name: String,
    /// Operation type (SELECT, INSERT, UPDATE, DELETE, ALL)
    pub operation: String,
    /// USING clause expression
    pub using_expression: Option<String>,
    /// WITH CHECK clause expression
    pub with_check_expression: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rls_operation_as_str() {
        assert_eq!(RlsOperation::Select.as_str(), "SELECT");
        assert_eq!(RlsOperation::Insert.as_str(), "INSERT");
        assert_eq!(RlsOperation::Update.as_str(), "UPDATE");
        assert_eq!(RlsOperation::Delete.as_str(), "DELETE");
        assert_eq!(RlsOperation::All.as_str(), "ALL");
    }

    #[test]
    fn test_rls_policy_new() {
        let policy = RlsPolicy::new("test_policy", "users");
        assert_eq!(policy.name, "test_policy");
        assert_eq!(policy.table_name, "users");
        assert_eq!(policy.operation, RlsOperation::All);
        assert!(policy.using_expression.is_none());
        assert!(policy.with_check_expression.is_none());
        assert!(policy.role.is_none());
    }

    #[test]
    fn test_rls_policy_builder() {
        let policy = RlsPolicy::new("test_policy", "users")
            .for_operation(RlsOperation::Select)
            .using_expression("user_id = current_user_id()")
            .for_role("app_user");

        assert_eq!(policy.operation, RlsOperation::Select);
        assert_eq!(
            policy.using_expression,
            Some("user_id = current_user_id()".to_string())
        );
        assert_eq!(policy.role, Some("app_user".to_string()));
    }

    #[test]
    fn test_rls_policy_to_sql_basic() {
        let policy = RlsPolicy::new("user_policy", "workflows")
            .for_all_operations()
            .using_expression("user_id = current_user_id()");

        let sql = policy.to_sql();
        assert!(sql.contains("CREATE POLICY user_policy ON workflows"));
        assert!(sql.contains("FOR ALL"));
        assert!(sql.contains("USING (user_id = current_user_id())"));
    }

    #[test]
    fn test_rls_policy_to_sql_with_role() {
        let policy = RlsPolicy::new("read_policy", "documents")
            .for_operation(RlsOperation::Select)
            .using_expression("true")
            .for_role("reader");

        let sql = policy.to_sql();
        assert!(sql.contains("CREATE POLICY read_policy ON documents"));
        assert!(sql.contains("FOR SELECT"));
        assert!(sql.contains("TO reader"));
        assert!(sql.contains("USING (true)"));
    }

    #[test]
    fn test_rls_policy_to_sql_with_check() {
        let policy = RlsPolicy::new("insert_policy", "posts")
            .for_operation(RlsOperation::Insert)
            .with_check_expression("status = 'draft'");

        let sql = policy.to_sql();
        assert!(sql.contains("CREATE POLICY insert_policy ON posts"));
        assert!(sql.contains("FOR INSERT"));
        assert!(sql.contains("WITH CHECK (status = 'draft')"));
    }

    #[test]
    fn test_rls_policy_to_sql_complete() {
        let policy = RlsPolicy::new("complete_policy", "articles")
            .for_operation(RlsOperation::Update)
            .using_expression("author_id = current_user_id()")
            .with_check_expression("published = false")
            .for_role("editor");

        let sql = policy.to_sql();
        assert!(sql.contains("CREATE POLICY complete_policy ON articles"));
        assert!(sql.contains("FOR UPDATE"));
        assert!(sql.contains("TO editor"));
        assert!(sql.contains("USING (author_id = current_user_id())"));
        assert!(sql.contains("WITH CHECK (published = false)"));
    }

    #[test]
    fn test_tenant_isolation_policy() {
        let policy = RlsManager::tenant_isolation_policy("tenant_policy", "workflows", "tenant_id");

        assert_eq!(policy.name, "tenant_policy");
        assert_eq!(policy.table_name, "workflows");
        assert_eq!(policy.operation, RlsOperation::All);
        assert_eq!(
            policy.using_expression,
            Some("tenant_id = current_setting('app.current_tenant_id')".to_string())
        );
    }

    #[test]
    fn test_user_isolation_policy() {
        let policy = RlsManager::user_isolation_policy("user_policy", "documents", "user_id");

        assert_eq!(policy.name, "user_policy");
        assert_eq!(policy.table_name, "documents");
        assert_eq!(policy.operation, RlsOperation::All);
        assert_eq!(
            policy.using_expression,
            Some("user_id = current_setting('app.current_user_id')::uuid".to_string())
        );
    }

    #[test]
    fn test_read_only_policy() {
        let policy = RlsManager::read_only_policy("read_policy", "public_data", "viewer");

        assert_eq!(policy.name, "read_policy");
        assert_eq!(policy.table_name, "public_data");
        assert_eq!(policy.operation, RlsOperation::Select);
        assert_eq!(policy.role, Some("viewer".to_string()));
        assert_eq!(policy.using_expression, Some("true".to_string()));
    }

    #[test]
    fn test_admin_bypass_policy() {
        let policy = RlsManager::admin_bypass_policy("admin_policy", "sensitive_data");

        assert_eq!(policy.name, "admin_policy");
        assert_eq!(policy.table_name, "sensitive_data");
        assert_eq!(policy.operation, RlsOperation::All);
        assert_eq!(
            policy.using_expression,
            Some("current_setting('app.current_role') = 'admin'".to_string())
        );
    }
}
