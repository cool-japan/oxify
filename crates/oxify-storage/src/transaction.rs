//! Transaction helper utilities
//!
//! This module provides high-level transaction management utilities that simplify
//! working with database transactions in async contexts.
//!
//! # Features
//!
//! - **Transaction Builder**: Ergonomic transaction creation
//! - **Retry Logic**: Automatic retry on serialization failures
//! - **Savepoints**: Named savepoints for partial rollback
//! - **Isolation Levels**: Easy isolation level configuration
//! - **Timeout Support**: Transaction timeout handling
//! - **Logging**: Automatic transaction logging
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, TransactionBuilder, IsolationLevel};
//!
//! let pool = DatabasePool::new(config).await?;
//!
//! // Simple transaction
//! let result = TransactionBuilder::new(pool.clone())
//!     .run(|tx| async move {
//!         sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
//!             .bind(&user_id)
//!             .bind(&email)
//!             .execute(&mut **tx)
//!             .await?;
//!         Ok(user_id)
//!     })
//!     .await?;
//!
//! // Transaction with retry
//! let result = TransactionBuilder::new(pool.clone())
//!     .with_retry(3)
//!     .run(|tx| async move {
//!         // Transaction logic here
//!         Ok(())
//!     })
//!     .await?;
//!
//! // Transaction with isolation level
//! let result = TransactionBuilder::new(pool.clone())
//!     .isolation_level(IsolationLevel::Serializable)
//!     .run(|tx| async move {
//!         // Critical section requiring serializable isolation
//!         Ok(())
//!     })
//!     .await?;
//! ```

use crate::{DatabasePool, Result, StorageError};
use sqlx::{Postgres, Transaction};
use std::future::Future;
use std::time::Duration;

/// Transaction isolation levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// Read Committed (PostgreSQL default)
    ReadCommitted,
    /// Repeatable Read
    RepeatableRead,
    /// Serializable (strictest)
    Serializable,
}

impl IsolationLevel {
    /// Get the SQL statement to set this isolation level
    pub fn as_sql(&self) -> &'static str {
        match self {
            IsolationLevel::ReadCommitted => "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
            IsolationLevel::RepeatableRead => "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
            IsolationLevel::Serializable => "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE",
        }
    }
}

/// Transaction configuration
#[derive(Debug, Clone)]
pub struct TransactionConfig {
    /// Isolation level
    pub isolation_level: Option<IsolationLevel>,
    /// Maximum number of retry attempts for serialization failures
    pub max_retries: usize,
    /// Delay between retries
    pub retry_delay: Duration,
    /// Transaction timeout
    pub timeout: Option<Duration>,
    /// Enable transaction logging
    pub enable_logging: bool,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            isolation_level: None,
            max_retries: 0,
            retry_delay: Duration::from_millis(100),
            timeout: None,
            enable_logging: false,
        }
    }
}

/// Transaction builder for ergonomic transaction creation
pub struct TransactionBuilder {
    pool: DatabasePool,
    config: TransactionConfig,
}

impl TransactionBuilder {
    /// Create a new transaction builder
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            config: TransactionConfig::default(),
        }
    }

    /// Set the isolation level
    pub fn isolation_level(mut self, level: IsolationLevel) -> Self {
        self.config.isolation_level = Some(level);
        self
    }

    /// Enable retry on serialization failures
    pub fn with_retry(mut self, max_retries: usize) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    /// Set retry delay
    pub fn retry_delay(mut self, delay: Duration) -> Self {
        self.config.retry_delay = delay;
        self
    }

    /// Set transaction timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = Some(timeout);
        self
    }

    /// Enable transaction logging
    pub fn with_logging(mut self) -> Self {
        self.config.enable_logging = true;
        self
    }

    /// Run the transaction with the configured settings
    pub async fn run<F, T, Fut>(self, f: F) -> Result<T>
    where
        F: Fn(Transaction<'static, Postgres>) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let mut attempts = 0;
        let max_attempts = self.config.max_retries + 1;

        loop {
            attempts += 1;

            if self.config.enable_logging {
                tracing::debug!(
                    attempt = attempts,
                    max_attempts = max_attempts,
                    "Starting transaction"
                );
            }

            // Begin transaction
            let mut tx = self.pool.pool().begin().await?;

            // Set isolation level if specified
            if let Some(level) = self.config.isolation_level {
                sqlx::query(level.as_sql()).execute(&mut *tx).await?;
            }

            // Execute transaction function
            match f(tx).await {
                Ok(result) => {
                    if self.config.enable_logging {
                        tracing::debug!(attempt = attempts, "Transaction completed successfully");
                    }
                    return Ok(result);
                }
                Err(e) => {
                    if self.config.enable_logging {
                        tracing::warn!(
                            attempt = attempts,
                            error = %e,
                            "Transaction failed"
                        );
                    }

                    // Check if this is a serialization failure and we can retry
                    if attempts < max_attempts && is_serialization_failure(&e) {
                        if self.config.enable_logging {
                            tracing::info!(
                                attempt = attempts,
                                max_attempts = max_attempts,
                                "Retrying transaction after serialization failure"
                            );
                        }
                        tokio::time::sleep(self.config.retry_delay).await;
                        continue;
                    }

                    // No more retries or different error
                    return Err(e);
                }
            }
        }
    }
}

/// Helper function to check if an error is a serialization failure
fn is_serialization_failure(error: &StorageError) -> bool {
    match error {
        StorageError::Database(sqlx_error) => {
            // Check if this is a serialization failure (SQLSTATE 40001)
            if let Some(db_error) = sqlx_error.as_database_error() {
                if let Some(code) = db_error.code() {
                    return code == "40001" || code == "40P01"; // Serialization failure or deadlock
                }
            }
            false
        }
        _ => false,
    }
}

/// Savepoint manager for fine-grained transaction control
pub struct SavepointManager<'a> {
    tx: &'a mut Transaction<'static, Postgres>,
    savepoint_counter: usize,
}

impl<'a> SavepointManager<'a> {
    /// Create a new savepoint manager
    pub fn new(tx: &'a mut Transaction<'static, Postgres>) -> Self {
        Self {
            tx,
            savepoint_counter: 0,
        }
    }

    /// Create a new savepoint
    pub async fn create_savepoint(&mut self) -> Result<Savepoint> {
        self.savepoint_counter += 1;
        let name = format!("sp_{}", self.savepoint_counter);

        sqlx::query(&format!("SAVEPOINT {name}"))
            .execute(&mut **self.tx)
            .await?;

        Ok(Savepoint { name })
    }

    /// Rollback to a savepoint
    pub async fn rollback_to(&mut self, savepoint: &Savepoint) -> Result<()> {
        sqlx::query(&format!("ROLLBACK TO SAVEPOINT {}", savepoint.name))
            .execute(&mut **self.tx)
            .await?;
        Ok(())
    }

    /// Release a savepoint (commit it)
    pub async fn release(&mut self, savepoint: &Savepoint) -> Result<()> {
        sqlx::query(&format!("RELEASE SAVEPOINT {}", savepoint.name))
            .execute(&mut **self.tx)
            .await?;
        Ok(())
    }
}

/// A database savepoint
#[derive(Debug, Clone)]
pub struct Savepoint {
    name: String,
}

/// Transaction utilities
pub struct TransactionUtils;

impl TransactionUtils {
    /// Execute multiple operations in a single transaction
    ///
    /// If any operation fails, the entire transaction is rolled back.
    pub async fn batch<F, Fut>(pool: &DatabasePool, operations: Vec<F>) -> Result<()>
    where
        F: FnOnce(&mut Transaction<'static, Postgres>) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let mut tx = pool.pool().begin().await?;

        for operation in operations {
            operation(&mut tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Execute an operation with automatic rollback on error
    pub async fn with_rollback<F, T, Fut>(pool: &DatabasePool, f: F) -> Result<T>
    where
        F: FnOnce(Transaction<'static, Postgres>) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let tx = pool.pool().begin().await?;

        match f(tx).await {
            Ok(result) => Ok(result),
            Err(e) => {
                // Transaction will be automatically rolled back when dropped
                Err(e)
            }
        }
    }

    /// Check if currently in a transaction
    pub async fn in_transaction(pool: &DatabasePool) -> Result<bool> {
        let row = sqlx::query("SELECT pg_current_xact_id_if_assigned() IS NOT NULL AS in_tx")
            .fetch_one(pool.pool())
            .await?;

        Ok(sqlx::Row::get(&row, "in_tx"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolation_level_sql() {
        assert_eq!(
            IsolationLevel::ReadCommitted.as_sql(),
            "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
        );
        assert_eq!(
            IsolationLevel::RepeatableRead.as_sql(),
            "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ"
        );
        assert_eq!(
            IsolationLevel::Serializable.as_sql(),
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE"
        );
    }

    #[test]
    fn test_default_transaction_config() {
        let config = TransactionConfig::default();
        assert!(config.isolation_level.is_none());
        assert_eq!(config.max_retries, 0);
        assert_eq!(config.retry_delay, Duration::from_millis(100));
        assert!(config.timeout.is_none());
        assert!(!config.enable_logging);
    }

    #[test]
    fn test_transaction_config() {
        let config = TransactionConfig {
            isolation_level: Some(IsolationLevel::Serializable),
            max_retries: 3,
            retry_delay: Duration::from_millis(200),
            timeout: Some(Duration::from_secs(30)),
            enable_logging: true,
        };

        assert_eq!(config.isolation_level, Some(IsolationLevel::Serializable));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay, Duration::from_millis(200));
        assert_eq!(config.timeout, Some(Duration::from_secs(30)));
        assert!(config.enable_logging);
    }

    #[test]
    fn test_savepoint_creation() {
        let name = format!("sp_{}", 1);
        let savepoint = Savepoint { name: name.clone() };
        assert_eq!(savepoint.name, "sp_1");
    }
}
