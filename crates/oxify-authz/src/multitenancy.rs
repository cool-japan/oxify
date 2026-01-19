//! Multi-Tenancy Support for Authorization
//!
//! Provides tenant isolation, cross-tenant sharing, and per-tenant quotas.
//!
//! ## Features
//!
//! - **Tenant Isolation**: Logical partitioning by tenant_id
//! - **Cross-Tenant Sharing**: Explicit permission grants across tenants
//! - **Per-Tenant Quotas**: Resource limits and rate limiting
//! - **Audit Logging**: Track all cross-tenant access
//!
//! ## Example
//!
//! ```rust
//! use oxify_authz::multitenancy::{TenantContext, MultiTenantEngine, TenantQuota};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let tenant_ctx = TenantContext::new("tenant-123");
//! let engine = MultiTenantEngine::new();
//!
//! // Set tenant quota
//! let quota = TenantQuota::new("tenant-123").with_max_tuples(1000);
//! engine.set_quota(quota).await?;
//!
//! // Check quota before operations
//! let can_create = engine.check_tuple_quota("tenant-123").await?;
//! assert!(can_create);
//!
//! // Increment usage
//! engine.increment_tuple_count("tenant-123").await?;
//! # Ok(())
//! # }
//! ```

use crate::{AuthzError, RelationTuple, Result, Subject};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Resource identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Resource {
    pub namespace: String,
    pub object_id: String,
}

/// Tenant context for multi-tenancy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantContext {
    /// Unique tenant identifier
    pub tenant_id: String,

    /// Optional organization name for display
    pub organization: Option<String>,

    /// Tenant-specific metadata
    pub metadata: HashMap<String, String>,
}

impl TenantContext {
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            organization: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Tenant-specific relation tuple
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantRelationTuple {
    /// Tenant identifier
    pub tenant_id: String,

    /// Standard relation tuple
    pub tuple: RelationTuple,

    /// Cross-tenant flag (if true, accessible across tenants)
    pub cross_tenant: bool,
}

impl TenantRelationTuple {
    pub fn new(tenant_id: String, tuple: RelationTuple) -> Self {
        Self {
            tenant_id,
            tuple,
            cross_tenant: false,
        }
    }

    pub fn with_cross_tenant(mut self, cross_tenant: bool) -> Self {
        self.cross_tenant = cross_tenant;
        self
    }
}

/// Tenant quotas and limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    /// Tenant identifier
    pub tenant_id: String,

    /// Maximum number of relation tuples
    pub max_tuples: usize,

    /// Maximum permission checks per minute
    pub max_checks_per_minute: usize,

    /// Maximum API requests per minute
    pub max_api_requests_per_minute: usize,

    /// Current usage
    pub current_tuples: usize,

    /// Current checks in current window
    pub current_checks: usize,

    /// Current API requests in current window
    pub current_api_requests: usize,
}

impl TenantQuota {
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            max_tuples: 100_000,                // Default: 100k tuples
            max_checks_per_minute: 10_000,      // Default: 10k checks/min
            max_api_requests_per_minute: 1_000, // Default: 1k API req/min
            current_tuples: 0,
            current_checks: 0,
            current_api_requests: 0,
        }
    }

    pub fn with_max_tuples(mut self, max: usize) -> Self {
        self.max_tuples = max;
        self
    }

    pub fn with_max_checks_per_minute(mut self, max: usize) -> Self {
        self.max_checks_per_minute = max;
        self
    }

    /// Check if tuple creation is allowed
    pub fn can_create_tuple(&self) -> bool {
        self.current_tuples < self.max_tuples
    }

    /// Check if permission check is allowed
    pub fn can_check_permission(&self) -> bool {
        self.current_checks < self.max_checks_per_minute
    }

    /// Check if API request is allowed
    pub fn can_make_api_request(&self) -> bool {
        self.current_api_requests < self.max_api_requests_per_minute
    }

    /// Increment tuple count
    pub fn increment_tuples(&mut self) {
        self.current_tuples += 1;
    }

    /// Decrement tuple count
    pub fn decrement_tuples(&mut self) {
        if self.current_tuples > 0 {
            self.current_tuples -= 1;
        }
    }

    /// Increment check count
    pub fn increment_checks(&mut self) {
        self.current_checks += 1;
    }

    /// Increment API request count
    pub fn increment_api_requests(&mut self) {
        self.current_api_requests += 1;
    }

    /// Reset rate limit counters (called every minute)
    pub fn reset_rate_limits(&mut self) {
        self.current_checks = 0;
        self.current_api_requests = 0;
    }
}

/// Multi-tenant authorization engine
#[allow(dead_code)]
pub struct MultiTenantEngine {
    /// Tenant quotas (tenant_id -> quota)
    quotas: Arc<RwLock<HashMap<String, TenantQuota>>>,

    /// Cross-tenant permissions (for audit)
    cross_tenant_log: Arc<RwLock<Vec<CrossTenantAccess>>>,
}

impl MultiTenantEngine {
    pub fn new() -> Self {
        Self {
            quotas: Arc::new(RwLock::new(HashMap::new())),
            cross_tenant_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create or update tenant quota
    pub async fn set_quota(&self, quota: TenantQuota) -> Result<()> {
        let mut quotas = self.quotas.write().await;
        quotas.insert(quota.tenant_id.clone(), quota);
        Ok(())
    }

    /// Get tenant quota
    pub async fn get_quota(&self, tenant_id: &str) -> Result<Option<TenantQuota>> {
        let quotas = self.quotas.read().await;
        Ok(quotas.get(tenant_id).cloned())
    }

    /// Check if tenant can create a tuple
    pub async fn check_tuple_quota(&self, tenant_id: &str) -> Result<bool> {
        let quotas = self.quotas.read().await;
        if let Some(quota) = quotas.get(tenant_id) {
            Ok(quota.can_create_tuple())
        } else {
            // No quota set = allow
            Ok(true)
        }
    }

    /// Check if tenant can perform permission check
    pub async fn check_permission_quota(&self, tenant_id: &str) -> Result<bool> {
        let mut quotas = self.quotas.write().await;
        if let Some(quota) = quotas.get_mut(tenant_id) {
            if quota.can_check_permission() {
                quota.increment_checks();
                Ok(true)
            } else {
                Err(AuthzError::PermissionDenied(format!(
                    "Tenant {} exceeded permission check quota",
                    tenant_id
                )))
            }
        } else {
            // No quota set = allow
            Ok(true)
        }
    }

    /// Increment tuple count for tenant
    pub async fn increment_tuple_count(&self, tenant_id: &str) -> Result<()> {
        let mut quotas = self.quotas.write().await;
        if let Some(quota) = quotas.get_mut(tenant_id) {
            quota.increment_tuples();
        }
        Ok(())
    }

    /// Decrement tuple count for tenant
    pub async fn decrement_tuple_count(&self, tenant_id: &str) -> Result<()> {
        let mut quotas = self.quotas.write().await;
        if let Some(quota) = quotas.get_mut(tenant_id) {
            quota.decrement_tuples();
        }
        Ok(())
    }

    /// Log cross-tenant access
    pub async fn log_cross_tenant_access(&self, access: CrossTenantAccess) -> Result<()> {
        let mut log = self.cross_tenant_log.write().await;
        log.push(access);
        Ok(())
    }

    /// Get cross-tenant access log
    pub async fn get_cross_tenant_log(&self) -> Result<Vec<CrossTenantAccess>> {
        let log = self.cross_tenant_log.read().await;
        Ok(log.clone())
    }

    /// Reset rate limits for all tenants (called every minute)
    pub async fn reset_all_rate_limits(&self) -> Result<()> {
        let mut quotas = self.quotas.write().await;
        for quota in quotas.values_mut() {
            quota.reset_rate_limits();
        }
        Ok(())
    }
}

impl Default for MultiTenantEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Cross-tenant access record for auditing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossTenantAccess {
    /// Source tenant ID
    pub source_tenant: String,

    /// Target tenant ID
    pub target_tenant: String,

    /// Resource accessed
    pub resource: Resource,

    /// Relation checked
    pub relation: String,

    /// Subject performing access
    pub subject: Subject,

    /// Access granted or denied
    pub granted: bool,

    /// Timestamp (Unix timestamp)
    pub timestamp: i64,
}

impl CrossTenantAccess {
    pub fn new(
        source_tenant: String,
        target_tenant: String,
        resource: Resource,
        relation: String,
        subject: Subject,
        granted: bool,
    ) -> Self {
        Self {
            source_tenant,
            target_tenant,
            resource,
            relation,
            subject,
            granted,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// Tenant-aware resource
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantResource {
    pub tenant_id: String,
    pub resource: Resource,
}

impl TenantResource {
    pub fn new(tenant_id: impl Into<String>, resource: Resource) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            resource,
        }
    }

    /// Check if this resource is accessible across tenants
    pub fn is_cross_tenant(&self) -> bool {
        // In a real implementation, check database or cache
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_context_creation() {
        let ctx = TenantContext::new("tenant-123");
        assert_eq!(ctx.tenant_id, "tenant-123");
        assert!(ctx.organization.is_none());

        let ctx = ctx.with_organization("ACME Corp");
        assert_eq!(ctx.organization.unwrap(), "ACME Corp");
    }

    #[test]
    fn test_tenant_quota() {
        let mut quota = TenantQuota::new("tenant-123")
            .with_max_tuples(1000)
            .with_max_checks_per_minute(100);

        assert_eq!(quota.max_tuples, 1000);
        assert_eq!(quota.max_checks_per_minute, 100);

        assert!(quota.can_create_tuple());
        quota.increment_tuples();
        assert_eq!(quota.current_tuples, 1);

        assert!(quota.can_check_permission());
        quota.increment_checks();
        assert_eq!(quota.current_checks, 1);

        quota.reset_rate_limits();
        assert_eq!(quota.current_checks, 0);
    }

    #[tokio::test]
    async fn test_multi_tenant_engine() {
        let engine = MultiTenantEngine::new();

        let quota = TenantQuota::new("tenant-123").with_max_tuples(100);

        assert!(engine.set_quota(quota).await.is_ok());

        let retrieved = engine.get_quota("tenant-123").await;
        assert!(retrieved.is_ok());
        assert!(retrieved.unwrap().is_some());

        assert!(engine.check_tuple_quota("tenant-123").await.is_ok());
        assert!(engine.increment_tuple_count("tenant-123").await.is_ok());
    }

    #[test]
    fn test_cross_tenant_access() {
        let resource = Resource {
            namespace: "document".to_string(),
            object_id: "123".to_string(),
        };

        let subject = Subject::User("alice".to_string());

        let access = CrossTenantAccess::new(
            "tenant-A".to_string(),
            "tenant-B".to_string(),
            resource,
            "viewer".to_string(),
            subject,
            true,
        );

        assert_eq!(access.source_tenant, "tenant-A");
        assert_eq!(access.target_tenant, "tenant-B");
        assert!(access.granted);
    }

    #[test]
    fn test_tenant_resource() {
        let resource = Resource {
            namespace: "document".to_string(),
            object_id: "123".to_string(),
        };

        let tenant_resource = TenantResource::new("tenant-123", resource.clone());

        assert_eq!(tenant_resource.tenant_id, "tenant-123");
        assert_eq!(tenant_resource.resource, resource);
    }
}
