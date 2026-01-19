//! Redis-Backed Budget Persistence
//!
//! This module provides distributed budget tracking using Redis as a backing store.
//! This allows budget state to be shared across multiple application instances.
//!
//! # Example
//!
//! ```rust,ignore
//! use oxify_connect_llm::{RedisBudgetStore, BudgetLimit};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let budget_store = RedisBudgetStore::new("redis://localhost:6379").await?;
//!
//! // Set a budget
//! budget_store.set_budget("user-123", BudgetLimit::dollars(100)).await?;
//!
//! // Record usage (atomically across all instances)
//! budget_store.record_usage("user-123", 150).await?; // 150 cents = $1.50
//!
//! // Check if can afford
//! let can_afford = budget_store.can_afford("user-123", 50).await?;
//! println!("Can afford: {}", can_afford);
//!
//! // Get remaining budget
//! let remaining = budget_store.get_remaining("user-123").await?;
//! println!("Remaining: ${:.2}", remaining as f64 / 100.0);
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "redis-cache")]
use crate::{BudgetLimit, LlmError, Result};
#[cfg(feature = "redis-cache")]
use redis::{aio::ConnectionManager, AsyncCommands, Client};

/// Redis-backed budget store for distributed budget tracking
#[cfg(feature = "redis-cache")]
#[derive(Clone)]
pub struct RedisBudgetStore {
    client: ConnectionManager,
    key_prefix: String,
}

#[cfg(feature = "redis-cache")]
impl RedisBudgetStore {
    /// Create a new Redis budget store
    ///
    /// # Arguments
    /// * `redis_url` - Redis connection URL (e.g., "redis://localhost:6379")
    ///
    /// # Example
    /// ```rust,ignore
    /// let store = RedisBudgetStore::new("redis://localhost:6379").await?;
    /// ```
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = Client::open(redis_url)
            .map_err(|e| LlmError::ConfigError(format!("Failed to create Redis client: {}", e)))?;

        let connection = ConnectionManager::new(client)
            .await
            .map_err(|e| LlmError::ConfigError(format!("Failed to connect to Redis: {}", e)))?;

        Ok(Self {
            client: connection,
            key_prefix: "oxify:budget:".to_string(),
        })
    }

    /// Create a new Redis budget store with a custom key prefix
    pub async fn with_prefix(redis_url: &str, key_prefix: String) -> Result<Self> {
        let mut store = Self::new(redis_url).await?;
        store.key_prefix = key_prefix;
        Ok(store)
    }

    /// Set a budget limit for an entity (user, workflow, etc.)
    ///
    /// # Arguments
    /// * `entity_id` - The entity identifier (e.g., user ID, workflow ID)
    /// * `budget` - The budget limit
    pub async fn set_budget(&self, entity_id: &str, budget: BudgetLimit) -> Result<()> {
        let mut conn = self.client.clone();
        let key = format!("{}{}:limit", self.key_prefix, entity_id);
        let budget_cents = budget.as_cents();

        conn.set::<_, _, ()>(&key, budget_cents)
            .await
            .map_err(|e| LlmError::Other(format!("Failed to set budget: {}", e)))?;

        Ok(())
    }

    /// Get the budget limit for an entity
    pub async fn get_budget(&self, entity_id: &str) -> Result<Option<BudgetLimit>> {
        let mut conn = self.client.clone();
        let key = format!("{}{}:limit", self.key_prefix, entity_id);

        let budget_cents: Option<u64> = conn
            .get(&key)
            .await
            .map_err(|e| LlmError::Other(format!("Failed to get budget: {}", e)))?;

        Ok(budget_cents.map(BudgetLimit::cents))
    }

    /// Record usage for an entity (atomically)
    ///
    /// # Arguments
    /// * `entity_id` - The entity identifier
    /// * `cost_cents` - The cost to record in cents
    pub async fn record_usage(&self, entity_id: &str, cost_cents: u64) -> Result<()> {
        let mut conn = self.client.clone();
        let key = format!("{}{}:used", self.key_prefix, entity_id);

        conn.incr::<_, _, ()>(&key, cost_cents)
            .await
            .map_err(|e| LlmError::Other(format!("Failed to record usage: {}", e)))?;

        Ok(())
    }

    /// Get the total usage for an entity
    pub async fn get_usage(&self, entity_id: &str) -> Result<u64> {
        let mut conn = self.client.clone();
        let key = format!("{}{}:used", self.key_prefix, entity_id);

        let usage: Option<u64> = conn
            .get(&key)
            .await
            .map_err(|e| LlmError::Other(format!("Failed to get usage: {}", e)))?;

        Ok(usage.unwrap_or(0))
    }

    /// Get the remaining budget for an entity
    pub async fn get_remaining(&self, entity_id: &str) -> Result<u64> {
        let budget = self.get_budget(entity_id).await?;
        let usage = self.get_usage(entity_id).await?;

        if let Some(budget_limit) = budget {
            let budget_cents = budget_limit.as_cents();
            Ok(budget_cents.saturating_sub(usage))
        } else {
            // No budget set = unlimited
            Ok(u64::MAX)
        }
    }

    /// Check if an entity can afford a certain cost
    ///
    /// # Arguments
    /// * `entity_id` - The entity identifier
    /// * `cost_cents` - The cost to check in cents
    pub async fn can_afford(&self, entity_id: &str, cost_cents: u64) -> Result<bool> {
        let budget = self.get_budget(entity_id).await?;
        let usage = self.get_usage(entity_id).await?;

        if let Some(budget_limit) = budget {
            let budget_cents = budget_limit.as_cents();
            Ok(usage + cost_cents <= budget_cents)
        } else {
            // No budget set = always can afford
            Ok(true)
        }
    }

    /// Atomically check and record usage (with budget enforcement)
    ///
    /// Returns Ok(true) if the usage was recorded, Ok(false) if budget would be exceeded
    ///
    /// # Arguments
    /// * `entity_id` - The entity identifier
    /// * `cost_cents` - The cost to record in cents
    pub async fn check_and_record(&self, entity_id: &str, cost_cents: u64) -> Result<bool> {
        // Note: This is not truly atomic in Redis without Lua scripting
        // For production use, consider using a Lua script for atomicity
        let can_afford = self.can_afford(entity_id, cost_cents).await?;

        if can_afford {
            self.record_usage(entity_id, cost_cents).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Reset usage for an entity
    pub async fn reset_usage(&self, entity_id: &str) -> Result<()> {
        let mut conn = self.client.clone();
        let key = format!("{}{}:used", self.key_prefix, entity_id);

        conn.del::<_, ()>(&key)
            .await
            .map_err(|e| LlmError::Other(format!("Failed to reset usage: {}", e)))?;

        Ok(())
    }

    /// Delete all budget data for an entity
    pub async fn delete(&self, entity_id: &str) -> Result<()> {
        let mut conn = self.client.clone();
        let limit_key = format!("{}{}:limit", self.key_prefix, entity_id);
        let used_key = format!("{}{}:used", self.key_prefix, entity_id);

        conn.del::<_, ()>(&[&limit_key, &used_key])
            .await
            .map_err(|e| LlmError::Other(format!("Failed to delete budget data: {}", e)))?;

        Ok(())
    }

    /// Get budget statistics for an entity
    pub async fn get_stats(&self, entity_id: &str) -> Result<RedisBudgetStats> {
        let budget = self.get_budget(entity_id).await?;
        let usage = self.get_usage(entity_id).await?;
        let remaining = self.get_remaining(entity_id).await?;

        let remaining_percent = budget.as_ref().map(|b| {
            let budget_cents = b.as_cents();
            if budget_cents == 0 {
                0.0
            } else {
                (remaining as f64 / budget_cents as f64) * 100.0
            }
        });

        let has_budget = budget.is_some();

        Ok(RedisBudgetStats {
            entity_id: entity_id.to_string(),
            budget_limit: budget,
            usage_cents: usage,
            usage_usd: usage as f64 / 100.0,
            remaining_cents: if has_budget { Some(remaining) } else { None },
            remaining_usd: if has_budget {
                Some(remaining as f64 / 100.0)
            } else {
                None
            },
            remaining_percent,
        })
    }
}

/// Budget statistics from Redis
#[cfg(feature = "redis-cache")]
#[derive(Debug, Clone)]
pub struct RedisBudgetStats {
    /// Entity ID (user, workflow, etc.)
    pub entity_id: String,
    /// Budget limit (if set)
    pub budget_limit: Option<BudgetLimit>,
    /// Total usage in cents
    pub usage_cents: u64,
    /// Total usage in USD
    pub usage_usd: f64,
    /// Remaining budget in cents (if budget set)
    pub remaining_cents: Option<u64>,
    /// Remaining budget in USD (if budget set)
    pub remaining_usd: Option<f64>,
    /// Remaining budget percentage (0-100, if budget set)
    pub remaining_percent: Option<f64>,
}

#[cfg(all(test, feature = "redis-cache"))]
mod tests {
    use super::*;

    // Note: These tests require a running Redis server on localhost:6379
    // Run with: cargo test --features redis-cache -- --ignored

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_budget_set_and_get() {
        let store = RedisBudgetStore::new("redis://localhost:6379")
            .await
            .unwrap();

        let entity_id = "test-user-1";

        // Set budget
        store
            .set_budget(entity_id, BudgetLimit::cents(1000))
            .await
            .unwrap();

        // Get budget
        let budget = store.get_budget(entity_id).await.unwrap();
        assert_eq!(budget, Some(BudgetLimit::cents(1000)));

        // Cleanup
        store.delete(entity_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_budget_record_usage() {
        let store = RedisBudgetStore::new("redis://localhost:6379")
            .await
            .unwrap();

        let entity_id = "test-user-2";

        // Set budget
        store
            .set_budget(entity_id, BudgetLimit::cents(1000))
            .await
            .unwrap();

        // Record usage
        store.record_usage(entity_id, 100).await.unwrap();
        store.record_usage(entity_id, 150).await.unwrap();

        // Check usage
        let usage = store.get_usage(entity_id).await.unwrap();
        assert_eq!(usage, 250);

        // Check remaining
        let remaining = store.get_remaining(entity_id).await.unwrap();
        assert_eq!(remaining, 750);

        // Cleanup
        store.delete(entity_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_budget_can_afford() {
        let store = RedisBudgetStore::new("redis://localhost:6379")
            .await
            .unwrap();

        let entity_id = "test-user-3";

        // Set budget
        store
            .set_budget(entity_id, BudgetLimit::cents(1000))
            .await
            .unwrap();

        // Can afford 500
        assert!(store.can_afford(entity_id, 500).await.unwrap());

        // Record usage
        store.record_usage(entity_id, 800).await.unwrap();

        // Can afford 200
        assert!(store.can_afford(entity_id, 200).await.unwrap());

        // Cannot afford 201
        assert!(!store.can_afford(entity_id, 201).await.unwrap());

        // Cleanup
        store.delete(entity_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_budget_check_and_record() {
        let store = RedisBudgetStore::new("redis://localhost:6379")
            .await
            .unwrap();

        let entity_id = "test-user-4";

        // Set budget
        store
            .set_budget(entity_id, BudgetLimit::cents(1000))
            .await
            .unwrap();

        // Check and record - should succeed
        let recorded = store.check_and_record(entity_id, 600).await.unwrap();
        assert!(recorded);

        // Check usage
        let usage = store.get_usage(entity_id).await.unwrap();
        assert_eq!(usage, 600);

        // Check and record - should fail (would exceed budget)
        let recorded = store.check_and_record(entity_id, 500).await.unwrap();
        assert!(!recorded);

        // Usage should not have changed
        let usage = store.get_usage(entity_id).await.unwrap();
        assert_eq!(usage, 600);

        // Cleanup
        store.delete(entity_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_budget_stats() {
        let store = RedisBudgetStore::new("redis://localhost:6379")
            .await
            .unwrap();

        let entity_id = "test-user-5";

        // Set budget
        store
            .set_budget(entity_id, BudgetLimit::cents(1000))
            .await
            .unwrap();

        // Record usage
        store.record_usage(entity_id, 300).await.unwrap();

        // Get stats
        let stats = store.get_stats(entity_id).await.unwrap();
        assert_eq!(stats.entity_id, entity_id);
        assert_eq!(stats.budget_limit, Some(BudgetLimit::cents(1000)));
        assert_eq!(stats.usage_cents, 300);
        assert_eq!(stats.usage_usd, 3.0);
        assert_eq!(stats.remaining_cents, Some(700));
        assert_eq!(stats.remaining_usd, Some(7.0));
        assert_eq!(stats.remaining_percent, Some(70.0));

        // Cleanup
        store.delete(entity_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_budget_reset() {
        let store = RedisBudgetStore::new("redis://localhost:6379")
            .await
            .unwrap();

        let entity_id = "test-user-6";

        // Set budget and record usage
        store
            .set_budget(entity_id, BudgetLimit::cents(1000))
            .await
            .unwrap();
        store.record_usage(entity_id, 500).await.unwrap();

        // Reset usage
        store.reset_usage(entity_id).await.unwrap();

        // Check usage is zero
        let usage = store.get_usage(entity_id).await.unwrap();
        assert_eq!(usage, 0);

        // Budget should still be set
        let budget = store.get_budget(entity_id).await.unwrap();
        assert_eq!(budget, Some(BudgetLimit::cents(1000)));

        // Cleanup
        store.delete(entity_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_budget_no_limit() {
        let store = RedisBudgetStore::new("redis://localhost:6379")
            .await
            .unwrap();

        let entity_id = "test-user-7";

        // No budget set - should always be able to afford
        assert!(store.can_afford(entity_id, u64::MAX).await.unwrap());

        // Record usage
        store.record_usage(entity_id, 1000000).await.unwrap();

        // Still can afford anything
        assert!(store.can_afford(entity_id, u64::MAX).await.unwrap());

        // Cleanup
        store.delete(entity_id).await.unwrap();
    }
}
