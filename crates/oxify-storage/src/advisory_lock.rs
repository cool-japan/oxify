//! PostgreSQL Advisory Locks for distributed coordination
//!
//! This module provides type-safe wrappers around PostgreSQL advisory locks,
//! which are application-level locks that can be used for distributed coordination
//! across multiple database sessions and application instances.
//!
//! # Use Cases
//!
//! - Prevent duplicate execution of scheduled jobs across multiple workers
//! - Coordinate batch processing tasks in distributed systems
//! - Implement distributed mutexes for critical sections
//! - Prevent concurrent schema migrations
//! - Coordinate leader election in multi-instance deployments
//!
//! # Lock Types
//!
//! PostgreSQL provides two types of advisory locks:
//!
//! - **Session-level locks**: Held until explicitly released or session ends
//! - **Transaction-level locks**: Automatically released when transaction commits/rolls back
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{AdvisoryLock, LockId};
//!
//! // Prevent duplicate job execution
//! let lock_id = LockId::from_name("daily-report-job");
//! let lock = AdvisoryLock::new(pool.clone());
//!
//! if lock.try_acquire_session(lock_id).await? {
//!     // Execute job - only one instance will succeed
//!     process_daily_report().await?;
//!     lock.release_session(lock_id).await?;
//! } else {
//!     // Another instance is already running the job
//!     tracing::info!("Job already running on another instance");
//! }
//! ```

use crate::{Result, StorageError};
use sqlx::PgPool;
use std::sync::Arc;

/// Advisory lock identifier
///
/// PostgreSQL advisory locks use a 64-bit integer key. This type provides
/// convenient ways to create lock IDs from integers or strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LockId(i64);

impl LockId {
    /// Create a lock ID from an i64
    pub const fn from_i64(value: i64) -> Self {
        Self(value)
    }

    /// Create a lock ID from a u64 (will be cast to i64)
    pub const fn from_u64(value: u64) -> Self {
        Self(value as i64)
    }

    /// Create a lock ID from a string using a hash function
    ///
    /// This allows using human-readable names for locks. The same string
    /// will always produce the same lock ID.
    pub fn from_name(name: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        Self(hasher.finish() as i64)
    }

    /// Get the underlying i64 value
    pub const fn as_i64(&self) -> i64 {
        self.0
    }

    /// Create a lock ID from two i32 values
    ///
    /// PostgreSQL also supports two-integer advisory locks using `pg_advisory_lock(key1, key2)`.
    /// This combines them into a single 64-bit integer.
    pub const fn from_i32_pair(key1: i32, key2: i32) -> Self {
        let combined = ((key1 as i64) << 32) | (key2 as i64 & 0xFFFFFFFF);
        Self(combined)
    }

    /// Split lock ID into two i32 values for two-integer lock functions
    pub const fn to_i32_pair(&self) -> (i32, i32) {
        let key1 = (self.0 >> 32) as i32;
        let key2 = self.0 as i32;
        (key1, key2)
    }
}

/// PostgreSQL Advisory Lock manager
///
/// Provides methods for acquiring, releasing, and checking advisory locks.
/// All methods are safe to use concurrently from multiple tasks.
#[derive(Clone)]
pub struct AdvisoryLock {
    pool: Arc<PgPool>,
}

impl AdvisoryLock {
    /// Create a new advisory lock manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // ========================================================================
    // Session-level locks (must be explicitly released or session ends)
    // ========================================================================

    /// Try to acquire a session-level advisory lock without blocking
    ///
    /// Returns `true` if the lock was acquired, `false` if it's already held.
    /// The lock must be explicitly released with `release_session()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let lock_id = LockId::from_name("my-lock");
    /// if lock.try_acquire_session(lock_id).await? {
    ///     // Lock acquired, do work
    ///     lock.release_session(lock_id).await?;
    /// }
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn try_acquire_session(&self, lock_id: LockId) -> Result<bool> {
        let result: (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)")
            .bind(lock_id.as_i64())
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Acquire a session-level advisory lock, waiting if necessary
    ///
    /// This will block until the lock is available. The lock must be
    /// explicitly released with `release_session()`.
    ///
    /// # Warning
    ///
    /// This can block indefinitely if the lock is held by another session.
    /// Consider using `try_acquire_session()` or `acquire_session_with_timeout()` instead.
    #[tracing::instrument(skip(self))]
    pub async fn acquire_session(&self, lock_id: LockId) -> Result<()> {
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(lock_id.as_i64())
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Acquire a session-level advisory lock with a timeout
    ///
    /// Returns `true` if the lock was acquired within the timeout,
    /// `false` if the timeout expired.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// let lock_id = LockId::from_name("my-lock");
    /// if lock.acquire_session_with_timeout(lock_id, Duration::from_secs(5)).await? {
    ///     // Lock acquired within 5 seconds
    ///     lock.release_session(lock_id).await?;
    /// } else {
    ///     // Timeout - lock is held by another session
    /// }
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn acquire_session_with_timeout(
        &self,
        lock_id: LockId,
        timeout: std::time::Duration,
    ) -> Result<bool> {
        // Use pg_try_advisory_lock in a loop with short sleeps
        let start = std::time::Instant::now();
        loop {
            if self.try_acquire_session(lock_id).await? {
                return Ok(true);
            }

            if start.elapsed() >= timeout {
                return Ok(false);
            }

            // Sleep for a short duration before retrying
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Release a session-level advisory lock
    ///
    /// Returns `true` if the lock was held and released, `false` if it wasn't held.
    #[tracing::instrument(skip(self))]
    pub async fn release_session(&self, lock_id: LockId) -> Result<bool> {
        let result: (bool,) = sqlx::query_as("SELECT pg_advisory_unlock($1)")
            .bind(lock_id.as_i64())
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Release all session-level advisory locks held by the current session
    #[tracing::instrument(skip(self))]
    pub async fn release_all_session(&self) -> Result<()> {
        sqlx::query("SELECT pg_advisory_unlock_all()")
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Transaction-level locks (automatically released when transaction ends)
    // ========================================================================

    /// Try to acquire a transaction-level advisory lock without blocking
    ///
    /// Returns `true` if the lock was acquired, `false` if it's already held.
    /// The lock is automatically released when the transaction commits or rolls back.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut tx = pool.begin().await?;
    ///
    /// let lock_id = LockId::from_name("workflow-123");
    /// if lock.try_acquire_transaction(lock_id).await? {
    ///     // Update workflow while holding lock
    ///     tx.commit().await?; // Lock automatically released
    /// }
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn try_acquire_transaction(&self, lock_id: LockId) -> Result<bool> {
        let result: (bool,) = sqlx::query_as("SELECT pg_try_advisory_xact_lock($1)")
            .bind(lock_id.as_i64())
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Acquire a transaction-level advisory lock, waiting if necessary
    ///
    /// This will block until the lock is available. The lock is automatically
    /// released when the transaction commits or rolls back.
    ///
    /// # Warning
    ///
    /// This can block indefinitely if the lock is held by another transaction.
    #[tracing::instrument(skip(self))]
    pub async fn acquire_transaction(&self, lock_id: LockId) -> Result<()> {
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(lock_id.as_i64())
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Shared locks (multiple readers, single writer pattern)
    // ========================================================================

    /// Try to acquire a shared session-level advisory lock without blocking
    ///
    /// Multiple sessions can hold the same shared lock simultaneously, but
    /// a shared lock conflicts with an exclusive lock. Useful for implementing
    /// reader-writer patterns.
    #[tracing::instrument(skip(self))]
    pub async fn try_acquire_shared_session(&self, lock_id: LockId) -> Result<bool> {
        let result: (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock_shared($1)")
            .bind(lock_id.as_i64())
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Acquire a shared session-level advisory lock, waiting if necessary
    #[tracing::instrument(skip(self))]
    pub async fn acquire_shared_session(&self, lock_id: LockId) -> Result<()> {
        sqlx::query("SELECT pg_advisory_lock_shared($1)")
            .bind(lock_id.as_i64())
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    /// Release a shared session-level advisory lock
    #[tracing::instrument(skip(self))]
    pub async fn release_shared_session(&self, lock_id: LockId) -> Result<bool> {
        let result: (bool,) = sqlx::query_as("SELECT pg_advisory_unlock_shared($1)")
            .bind(lock_id.as_i64())
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Try to acquire a shared transaction-level advisory lock without blocking
    #[tracing::instrument(skip(self))]
    pub async fn try_acquire_shared_transaction(&self, lock_id: LockId) -> Result<bool> {
        let result: (bool,) = sqlx::query_as("SELECT pg_try_advisory_xact_lock_shared($1)")
            .bind(lock_id.as_i64())
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(result.0)
    }

    /// Acquire a shared transaction-level advisory lock, waiting if necessary
    #[tracing::instrument(skip(self))]
    pub async fn acquire_shared_transaction(&self, lock_id: LockId) -> Result<()> {
        sqlx::query("SELECT pg_advisory_xact_lock_shared($1)")
            .bind(lock_id.as_i64())
            .execute(self.pool.as_ref())
            .await
            .map_err(StorageError::Database)?;

        Ok(())
    }

    // ========================================================================
    // Helper methods
    // ========================================================================

    /// Execute a function while holding a session-level advisory lock
    ///
    /// The lock is automatically released when the function completes or errors.
    ///
    /// # Example
    ///
    /// ```ignore
    /// lock.with_session_lock(LockId::from_name("job-123"), || async {
    ///     // Execute critical section
    ///     process_job().await
    /// }).await?;
    /// ```
    pub async fn with_session_lock<F, Fut, T>(&self, lock_id: LockId, f: F) -> Result<Option<T>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        if !self.try_acquire_session(lock_id).await? {
            return Ok(None);
        }

        let result = f().await;

        // Always release lock, even if function errored
        let _ = self.release_session(lock_id).await;

        result.map(Some)
    }

    /// Execute a function while holding a session-level advisory lock, waiting if necessary
    ///
    /// Similar to `with_session_lock()` but waits for the lock to become available.
    pub async fn with_session_lock_blocking<F, Fut, T>(&self, lock_id: LockId, f: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        self.acquire_session(lock_id).await?;

        let result = f().await;

        // Always release lock, even if function errored
        let _ = self.release_session(lock_id).await;

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_id_from_i64() {
        let lock_id = LockId::from_i64(12345);
        assert_eq!(lock_id.as_i64(), 12345);
    }

    #[test]
    fn test_lock_id_from_u64() {
        let lock_id = LockId::from_u64(12345u64);
        assert_eq!(lock_id.as_i64(), 12345);
    }

    #[test]
    fn test_lock_id_from_name() {
        let lock_id1 = LockId::from_name("my-lock");
        let lock_id2 = LockId::from_name("my-lock");
        let lock_id3 = LockId::from_name("different-lock");

        // Same name produces same lock ID
        assert_eq!(lock_id1, lock_id2);

        // Different name produces different lock ID
        assert_ne!(lock_id1, lock_id3);
    }

    #[test]
    fn test_lock_id_from_i32_pair() {
        let lock_id = LockId::from_i32_pair(100, 200);
        let (key1, key2) = lock_id.to_i32_pair();

        assert_eq!(key1, 100);
        assert_eq!(key2, 200);
    }

    #[test]
    fn test_lock_id_to_i32_pair() {
        let lock_id = LockId::from_i64(0x0000006400000064); // 100 << 32 | 100
        let (key1, key2) = lock_id.to_i32_pair();

        assert_eq!(key1, 100);
        assert_eq!(key2, 100);
    }

    #[test]
    fn test_lock_id_roundtrip() {
        let original = LockId::from_i32_pair(12345, 67890);
        let (key1, key2) = original.to_i32_pair();
        let roundtrip = LockId::from_i32_pair(key1, key2);

        assert_eq!(original, roundtrip);
    }

    #[test]
    fn test_lock_id_equality() {
        let lock1 = LockId::from_i64(12345);
        let lock2 = LockId::from_i64(12345);
        let lock3 = LockId::from_i64(54321);

        assert_eq!(lock1, lock2);
        assert_ne!(lock1, lock3);
    }

    #[test]
    fn test_lock_id_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(LockId::from_i64(1));
        set.insert(LockId::from_i64(1)); // Duplicate
        set.insert(LockId::from_i64(2));

        assert_eq!(set.len(), 2); // Only unique values
    }

    #[test]
    fn test_lock_id_name_consistency() {
        // Same name should always produce the same hash
        let lock1 = LockId::from_name("consistent-name");
        let lock2 = LockId::from_name("consistent-name");

        assert_eq!(lock1.as_i64(), lock2.as_i64());
    }

    #[test]
    fn test_lock_id_negative_values() {
        let lock_id = LockId::from_i64(-12345);
        assert_eq!(lock_id.as_i64(), -12345);
    }
}
