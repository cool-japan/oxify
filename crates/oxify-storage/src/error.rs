//! Error types for storage operations

use thiserror::Error;
use uuid::Uuid;

/// Result type for storage operations
pub type Result<T> = std::result::Result<T, StorageError>;

/// Resource type for not found errors
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    User,
    Workflow,
    WorkflowVersion,
    Execution,
    ApiKey,
    Secret,
    Quota,
    Schedule,
    Webhook,
    Checkpoint,
    AuditLog,
    Metrics,
    Other(String),
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::User => write!(f, "User"),
            ResourceType::Workflow => write!(f, "Workflow"),
            ResourceType::WorkflowVersion => write!(f, "WorkflowVersion"),
            ResourceType::Execution => write!(f, "Execution"),
            ResourceType::ApiKey => write!(f, "ApiKey"),
            ResourceType::Secret => write!(f, "Secret"),
            ResourceType::Quota => write!(f, "Quota"),
            ResourceType::Schedule => write!(f, "Schedule"),
            ResourceType::Webhook => write!(f, "Webhook"),
            ResourceType::Checkpoint => write!(f, "Checkpoint"),
            ResourceType::AuditLog => write!(f, "AuditLog"),
            ResourceType::Metrics => write!(f, "Metrics"),
            ResourceType::Other(s) => write!(f, "{s}"),
        }
    }
}

/// Resource identifier that can be a UUID, string, or other type
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceId {
    Uuid(Uuid),
    String(String),
    Composite(Vec<String>),
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceId::Uuid(id) => write!(f, "{id}"),
            ResourceId::String(s) => write!(f, "{s}"),
            ResourceId::Composite(parts) => write!(f, "{}", parts.join(":")),
        }
    }
}

impl From<Uuid> for ResourceId {
    fn from(id: Uuid) -> Self {
        ResourceId::Uuid(id)
    }
}

impl From<String> for ResourceId {
    fn from(s: String) -> Self {
        ResourceId::String(s)
    }
}

impl From<&str> for ResourceId {
    fn from(s: &str) -> Self {
        ResourceId::String(s.to_string())
    }
}

/// Errors that can occur during storage operations
#[derive(Error, Debug)]
pub enum StorageError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] oxisql_core::OxiSqlError),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Not found error with detailed context
    #[error("{resource_type} not found: {resource_id}")]
    NotFound {
        resource_type: ResourceType,
        resource_id: ResourceId,
    },

    /// Legacy not found error (for backwards compatibility)
    #[error("Resource not found: {0}")]
    NotFoundLegacy(String),

    /// Constraint violation
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    /// Concurrent modification detected (optimistic locking)
    #[error("Concurrent modification: {0}")]
    ConcurrentModification(String),

    /// Migration error
    #[error("Migration error: {0}")]
    Migration(String),

    /// Encryption error
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Backup/restore error
    #[error("Backup error: {0}")]
    BackupError(String),

    /// Batch operation error
    #[error("Batch size {size} exceeds maximum {max}")]
    BatchTooLarge { size: usize, max: usize },
}

/// Map a deadpool connection-checkout failure into a storage error.
///
/// The OxiSQL SQLite pool (`oxisql_pool::sqlite::SqliteCompatPool`) returns
/// `deadpool::managed::PoolError<OxiSqlError>` from `get()`. We fold these
/// checkout failures (timeout, closed pool, backend connect failure) onto
/// `OxiSqlError::ConnectionPool` so they flow through the existing `Database`
/// taxonomy and are classified as retryable by [`StorageError::is_retryable`].
impl From<deadpool::managed::PoolError<oxisql_core::OxiSqlError>> for StorageError {
    fn from(err: deadpool::managed::PoolError<oxisql_core::OxiSqlError>) -> Self {
        StorageError::Database(oxisql_core::OxiSqlError::ConnectionPool(err.to_string()))
    }
}

impl StorageError {
    /// Create a not found error with resource type and ID
    pub fn not_found<T: Into<ResourceId>>(resource_type: ResourceType, resource_id: T) -> Self {
        StorageError::NotFound {
            resource_type,
            resource_id: resource_id.into(),
        }
    }

    /// Create a constraint violation error
    pub fn constraint_violation<S: Into<String>>(message: S) -> Self {
        StorageError::ConstraintViolation(message.into())
    }

    /// Create a validation error
    pub fn validation<S: Into<String>>(message: S) -> Self {
        StorageError::ValidationError(message.into())
    }

    /// Create an encryption error
    pub fn encryption<S: Into<String>>(message: S) -> Self {
        StorageError::EncryptionError(message.into())
    }

    /// Create a migration error
    pub fn migration<S: Into<String>>(message: S) -> Self {
        StorageError::Migration(message.into())
    }

    /// Create a backup error
    pub fn backup<S: Into<String>>(message: S) -> Self {
        StorageError::BackupError(message.into())
    }

    /// Check if this is a not found error
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            StorageError::NotFound { .. } | StorageError::NotFoundLegacy(_)
        )
    }

    /// Check if this is a constraint violation error
    pub fn is_constraint_violation(&self) -> bool {
        matches!(self, StorageError::ConstraintViolation(_))
    }

    /// Check if this is a validation error
    pub fn is_validation_error(&self) -> bool {
        matches!(self, StorageError::ValidationError(_))
    }

    /// Check if this is a database error
    pub fn is_database_error(&self) -> bool {
        matches!(self, StorageError::Database(_))
    }

    /// Get the resource type if this is a not found error
    pub fn resource_type(&self) -> Option<&ResourceType> {
        match self {
            StorageError::NotFound { resource_type, .. } => Some(resource_type),
            _ => None,
        }
    }

    /// Get the resource ID if this is a not found error
    pub fn resource_id(&self) -> Option<&ResourceId> {
        match self {
            StorageError::NotFound { resource_id, .. } => Some(resource_id),
            _ => None,
        }
    }

    /// Check if the error is retryable (transient database errors)
    pub fn is_retryable(&self) -> bool {
        match self {
            StorageError::Database(e) => match e {
                // Connection-pool exhaustion/timeouts and query timeouts are
                // transient by nature and always safe to retry.
                oxisql_core::OxiSqlError::ConnectionPool(_)
                | oxisql_core::OxiSqlError::Timeout(_) => true,
                // SQLite surfaces transient write contention ("database is
                // locked" / "database table is busy") as a generic execution
                // error. OxiSqlError has no typed variant for this class of
                // recoverable failure, so we fall back to a substring match on
                // the message text — the only signal the SQLite/Limbo backend
                // exposes to distinguish these retryable states from permanent
                // execution failures.
                oxisql_core::OxiSqlError::Execution(msg) => {
                    let lowered = msg.to_ascii_lowercase();
                    lowered.contains("locked") || lowered.contains("busy")
                }
                _ => false,
            },
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error_creation() {
        let error = StorageError::not_found(ResourceType::Workflow, Uuid::nil());
        assert!(error.is_not_found());
        assert_eq!(error.resource_type(), Some(&ResourceType::Workflow));
    }

    #[test]
    fn test_validation_error_creation() {
        let error = StorageError::validation("Invalid input");
        assert!(error.is_validation_error());
        assert!(!error.is_not_found());
    }

    #[test]
    fn test_constraint_violation_error_creation() {
        let error = StorageError::constraint_violation("Unique constraint violated");
        assert!(error.is_constraint_violation());
    }

    #[test]
    fn test_resource_type_serialization() {
        let resource_type = ResourceType::Workflow;
        let json = serde_json::to_string(&resource_type).unwrap();
        assert_eq!(json, "\"workflow\"");

        let deserialized: ResourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ResourceType::Workflow);
    }

    #[test]
    fn test_resource_id_serialization() {
        let resource_id = ResourceId::String("test-123".to_string());
        let json = serde_json::to_string(&resource_id).unwrap();
        let deserialized: ResourceId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, resource_id);
    }

    #[test]
    fn test_resource_id_from_uuid() {
        let uuid = Uuid::nil();
        let resource_id: ResourceId = uuid.into();
        matches!(resource_id, ResourceId::Uuid(_));
    }

    #[test]
    fn test_resource_id_from_string() {
        let resource_id: ResourceId = "test".into();
        matches!(resource_id, ResourceId::String(_));
    }

    #[test]
    fn test_error_helper_methods() {
        let migration_err = StorageError::migration("Migration failed");
        assert!(!migration_err.is_not_found());
        assert!(!migration_err.is_validation_error());

        let encryption_err = StorageError::encryption("Encryption failed");
        assert!(!encryption_err.is_constraint_violation());

        let backup_err = StorageError::backup("Backup failed");
        assert!(!backup_err.is_database_error());
    }
}
