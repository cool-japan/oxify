//! Resource Limits Module
//!
//! Provides comprehensive resource limiting for workflow executions:
//! - Per-workflow memory limits
//! - Per-workflow timeout limits
//! - Per-user execution quotas
//! - Token budget limits for LLM calls
//!
//! # Example
//!
//! ```ignore
//! use oxify_engine::resource_limits::{ResourceLimits, LimitsBuilder};
//!
//! let limits = LimitsBuilder::new()
//!     .max_memory_mb(512)
//!     .max_execution_time_secs(300)
//!     .max_tokens_per_execution(10000)
//!     .max_concurrent_nodes(10)
//!     .build();
//!
//! let enforcer = ResourceEnforcer::new(limits);
//! enforcer.check_memory()?;
//! enforcer.check_token_budget(500)?;
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;

/// Resource limit errors
#[derive(Error, Debug, Clone)]
pub enum ResourceLimitError {
    #[error("Memory limit exceeded: used {used_mb}MB, limit {limit_mb}MB")]
    MemoryLimitExceeded { used_mb: u64, limit_mb: u64 },

    #[error("Execution timeout: elapsed {elapsed_secs}s, limit {limit_secs}s")]
    ExecutionTimeout { elapsed_secs: u64, limit_secs: u64 },

    #[error("Token budget exceeded: used {used_tokens}, limit {limit_tokens}")]
    TokenBudgetExceeded { used_tokens: u64, limit_tokens: u64 },

    #[error("Concurrent execution limit reached: current {current}, limit {limit}")]
    ConcurrentLimitReached { current: u64, limit: u64 },

    #[error("User quota exceeded: user {user_id}, {resource} - used {used}, limit {limit}")]
    UserQuotaExceeded {
        user_id: String,
        resource: String,
        used: u64,
        limit: u64,
    },

    #[error("Node execution limit exceeded: executed {executed}, limit {limit}")]
    NodeLimitExceeded { executed: u64, limit: u64 },

    #[error("API call limit exceeded: calls {calls}, limit {limit}")]
    ApiCallLimitExceeded { calls: u64, limit: u64 },
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in MB (0 = unlimited)
    pub max_memory_mb: u64,
    /// Maximum execution time in seconds (0 = unlimited)
    pub max_execution_time_secs: u64,
    /// Maximum tokens per execution (0 = unlimited)
    pub max_tokens_per_execution: u64,
    /// Maximum tokens per LLM call (0 = unlimited)
    pub max_tokens_per_call: u64,
    /// Maximum concurrent node executions (0 = unlimited)
    pub max_concurrent_nodes: u64,
    /// Maximum total nodes to execute (0 = unlimited)
    pub max_total_nodes: u64,
    /// Maximum API calls per execution (0 = unlimited)
    pub max_api_calls: u64,
    /// Maximum retries per node (0 = no retries)
    pub max_retries_per_node: u64,
    /// Soft limit warning threshold (percentage of limit)
    pub warning_threshold_percent: u8,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: 512,
            max_execution_time_secs: 300,      // 5 minutes
            max_tokens_per_execution: 100_000, // ~$0.10 at GPT-4 prices
            max_tokens_per_call: 10_000,
            max_concurrent_nodes: 10,
            max_total_nodes: 1000,
            max_api_calls: 100,
            max_retries_per_node: 3,
            warning_threshold_percent: 80,
        }
    }
}

impl ResourceLimits {
    /// Create unlimited resource limits
    pub fn unlimited() -> Self {
        Self {
            max_memory_mb: 0,
            max_execution_time_secs: 0,
            max_tokens_per_execution: 0,
            max_tokens_per_call: 0,
            max_concurrent_nodes: 0,
            max_total_nodes: 0,
            max_api_calls: 0,
            max_retries_per_node: u64::MAX,
            warning_threshold_percent: 100,
        }
    }

    /// Create strict limits for development/testing
    pub fn strict() -> Self {
        Self {
            max_memory_mb: 128,
            max_execution_time_secs: 60, // 1 minute
            max_tokens_per_execution: 5_000,
            max_tokens_per_call: 1_000,
            max_concurrent_nodes: 3,
            max_total_nodes: 100,
            max_api_calls: 20,
            max_retries_per_node: 1,
            warning_threshold_percent: 70,
        }
    }

    /// Create production limits
    pub fn production() -> Self {
        Self {
            max_memory_mb: 1024,
            max_execution_time_secs: 600, // 10 minutes
            max_tokens_per_execution: 500_000,
            max_tokens_per_call: 50_000,
            max_concurrent_nodes: 20,
            max_total_nodes: 5000,
            max_api_calls: 500,
            max_retries_per_node: 3,
            warning_threshold_percent: 85,
        }
    }
}

/// Builder for creating resource limits
pub struct LimitsBuilder {
    limits: ResourceLimits,
}

impl LimitsBuilder {
    pub fn new() -> Self {
        Self {
            limits: ResourceLimits::default(),
        }
    }

    pub fn max_memory_mb(mut self, mb: u64) -> Self {
        self.limits.max_memory_mb = mb;
        self
    }

    pub fn max_execution_time_secs(mut self, secs: u64) -> Self {
        self.limits.max_execution_time_secs = secs;
        self
    }

    pub fn max_tokens_per_execution(mut self, tokens: u64) -> Self {
        self.limits.max_tokens_per_execution = tokens;
        self
    }

    pub fn max_tokens_per_call(mut self, tokens: u64) -> Self {
        self.limits.max_tokens_per_call = tokens;
        self
    }

    pub fn max_concurrent_nodes(mut self, nodes: u64) -> Self {
        self.limits.max_concurrent_nodes = nodes;
        self
    }

    pub fn max_total_nodes(mut self, nodes: u64) -> Self {
        self.limits.max_total_nodes = nodes;
        self
    }

    pub fn max_api_calls(mut self, calls: u64) -> Self {
        self.limits.max_api_calls = calls;
        self
    }

    pub fn max_retries_per_node(mut self, retries: u64) -> Self {
        self.limits.max_retries_per_node = retries;
        self
    }

    pub fn warning_threshold_percent(mut self, percent: u8) -> Self {
        self.limits.warning_threshold_percent = percent.min(100);
        self
    }

    pub fn build(self) -> ResourceLimits {
        self.limits
    }
}

impl Default for LimitsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource usage tracking for an execution
#[derive(Debug, Default)]
pub struct ResourceUsage {
    /// Current memory usage in bytes
    memory_bytes: AtomicU64,
    /// Total tokens consumed
    total_tokens: AtomicU64,
    /// Total API calls made
    api_calls: AtomicU64,
    /// Nodes executed
    nodes_executed: AtomicU64,
    /// Current concurrent nodes
    concurrent_nodes: AtomicU64,
    /// Start time of execution
    start_time: Option<Instant>,
}

impl ResourceUsage {
    /// Create new resource usage tracker
    pub fn new() -> Self {
        Self {
            memory_bytes: AtomicU64::new(0),
            total_tokens: AtomicU64::new(0),
            api_calls: AtomicU64::new(0),
            nodes_executed: AtomicU64::new(0),
            concurrent_nodes: AtomicU64::new(0),
            start_time: Some(Instant::now()),
        }
    }

    /// Add memory usage
    pub fn add_memory(&self, bytes: u64) {
        self.memory_bytes.fetch_add(bytes, Ordering::SeqCst);
    }

    /// Release memory
    pub fn release_memory(&self, bytes: u64) {
        self.memory_bytes.fetch_sub(
            bytes.min(self.memory_bytes.load(Ordering::SeqCst)),
            Ordering::SeqCst,
        );
    }

    /// Add tokens consumed
    pub fn add_tokens(&self, tokens: u64) {
        self.total_tokens.fetch_add(tokens, Ordering::SeqCst);
    }

    /// Increment API calls
    pub fn add_api_call(&self) {
        self.api_calls.fetch_add(1, Ordering::SeqCst);
    }

    /// Increment nodes executed
    pub fn add_node_execution(&self) {
        self.nodes_executed.fetch_add(1, Ordering::SeqCst);
    }

    /// Increment concurrent nodes
    pub fn start_node(&self) -> u64 {
        self.concurrent_nodes.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Decrement concurrent nodes
    pub fn finish_node(&self) {
        self.concurrent_nodes.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get current memory usage in MB
    pub fn memory_mb(&self) -> u64 {
        self.memory_bytes.load(Ordering::SeqCst) / (1024 * 1024)
    }

    /// Get total tokens consumed
    pub fn tokens(&self) -> u64 {
        self.total_tokens.load(Ordering::SeqCst)
    }

    /// Get total API calls
    pub fn api_calls(&self) -> u64 {
        self.api_calls.load(Ordering::SeqCst)
    }

    /// Get nodes executed
    pub fn nodes_executed(&self) -> u64 {
        self.nodes_executed.load(Ordering::SeqCst)
    }

    /// Get current concurrent nodes
    pub fn concurrent_nodes(&self) -> u64 {
        self.concurrent_nodes.load(Ordering::SeqCst)
    }

    /// Get elapsed time in seconds
    pub fn elapsed_secs(&self) -> u64 {
        self.start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0)
    }

    /// Create a snapshot of current usage
    pub fn snapshot(&self) -> UsageSnapshot {
        UsageSnapshot {
            memory_mb: self.memory_mb(),
            total_tokens: self.tokens(),
            api_calls: self.api_calls(),
            nodes_executed: self.nodes_executed(),
            concurrent_nodes: self.concurrent_nodes(),
            elapsed_secs: self.elapsed_secs(),
        }
    }
}

/// Snapshot of resource usage at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub memory_mb: u64,
    pub total_tokens: u64,
    pub api_calls: u64,
    pub nodes_executed: u64,
    pub concurrent_nodes: u64,
    pub elapsed_secs: u64,
}

/// Resource warning types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceWarning {
    MemoryApproachingLimit { usage_percent: u8 },
    TokensApproachingLimit { usage_percent: u8 },
    TimeApproachingLimit { usage_percent: u8 },
    ApiCallsApproachingLimit { usage_percent: u8 },
}

/// Resource enforcer that validates limits
pub struct ResourceEnforcer {
    limits: ResourceLimits,
    usage: Arc<ResourceUsage>,
}

impl ResourceEnforcer {
    /// Create a new resource enforcer
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            usage: Arc::new(ResourceUsage::new()),
        }
    }

    /// Get the usage tracker
    pub fn usage(&self) -> Arc<ResourceUsage> {
        Arc::clone(&self.usage)
    }

    /// Check if memory limit is exceeded
    pub fn check_memory(&self) -> Result<Option<ResourceWarning>, ResourceLimitError> {
        if self.limits.max_memory_mb == 0 {
            return Ok(None);
        }

        let used = self.usage.memory_mb();
        let limit = self.limits.max_memory_mb;

        if used > limit {
            return Err(ResourceLimitError::MemoryLimitExceeded {
                used_mb: used,
                limit_mb: limit,
            });
        }

        let usage_percent = ((used as f64 / limit as f64) * 100.0) as u8;
        if usage_percent >= self.limits.warning_threshold_percent {
            return Ok(Some(ResourceWarning::MemoryApproachingLimit {
                usage_percent,
            }));
        }

        Ok(None)
    }

    /// Check if execution time is exceeded
    pub fn check_execution_time(&self) -> Result<Option<ResourceWarning>, ResourceLimitError> {
        if self.limits.max_execution_time_secs == 0 {
            return Ok(None);
        }

        let elapsed = self.usage.elapsed_secs();
        let limit = self.limits.max_execution_time_secs;

        if elapsed > limit {
            return Err(ResourceLimitError::ExecutionTimeout {
                elapsed_secs: elapsed,
                limit_secs: limit,
            });
        }

        let usage_percent = ((elapsed as f64 / limit as f64) * 100.0) as u8;
        if usage_percent >= self.limits.warning_threshold_percent {
            return Ok(Some(ResourceWarning::TimeApproachingLimit {
                usage_percent,
            }));
        }

        Ok(None)
    }

    /// Check if token budget allows more tokens
    pub fn check_token_budget(
        &self,
        additional_tokens: u64,
    ) -> Result<Option<ResourceWarning>, ResourceLimitError> {
        if self.limits.max_tokens_per_execution == 0 {
            return Ok(None);
        }

        let used = self.usage.tokens();
        let limit = self.limits.max_tokens_per_execution;

        if used + additional_tokens > limit {
            return Err(ResourceLimitError::TokenBudgetExceeded {
                used_tokens: used + additional_tokens,
                limit_tokens: limit,
            });
        }

        let usage_percent = (((used + additional_tokens) as f64 / limit as f64) * 100.0) as u8;
        if usage_percent >= self.limits.warning_threshold_percent {
            return Ok(Some(ResourceWarning::TokensApproachingLimit {
                usage_percent,
            }));
        }

        Ok(None)
    }

    /// Check if we can start another concurrent node
    pub fn check_concurrent_nodes(&self) -> Result<(), ResourceLimitError> {
        if self.limits.max_concurrent_nodes == 0 {
            return Ok(());
        }

        let current = self.usage.concurrent_nodes();
        let limit = self.limits.max_concurrent_nodes;

        if current >= limit {
            return Err(ResourceLimitError::ConcurrentLimitReached { current, limit });
        }

        Ok(())
    }

    /// Check if we can execute more nodes
    pub fn check_node_limit(&self) -> Result<(), ResourceLimitError> {
        if self.limits.max_total_nodes == 0 {
            return Ok(());
        }

        let executed = self.usage.nodes_executed();
        let limit = self.limits.max_total_nodes;

        if executed >= limit {
            return Err(ResourceLimitError::NodeLimitExceeded { executed, limit });
        }

        Ok(())
    }

    /// Check if we can make more API calls
    pub fn check_api_call_limit(&self) -> Result<Option<ResourceWarning>, ResourceLimitError> {
        if self.limits.max_api_calls == 0 {
            return Ok(None);
        }

        let calls = self.usage.api_calls();
        let limit = self.limits.max_api_calls;

        if calls >= limit {
            return Err(ResourceLimitError::ApiCallLimitExceeded { calls, limit });
        }

        let usage_percent = ((calls as f64 / limit as f64) * 100.0) as u8;
        if usage_percent >= self.limits.warning_threshold_percent {
            return Ok(Some(ResourceWarning::ApiCallsApproachingLimit {
                usage_percent,
            }));
        }

        Ok(None)
    }

    /// Check all resource limits
    pub fn check_all(&self) -> Result<Vec<ResourceWarning>, ResourceLimitError> {
        let mut warnings = Vec::new();

        if let Some(w) = self.check_memory()? {
            warnings.push(w);
        }
        if let Some(w) = self.check_execution_time()? {
            warnings.push(w);
        }
        if let Some(w) = self.check_token_budget(0)? {
            warnings.push(w);
        }
        self.check_concurrent_nodes()?;
        self.check_node_limit()?;
        if let Some(w) = self.check_api_call_limit()? {
            warnings.push(w);
        }

        Ok(warnings)
    }

    /// Get a usage snapshot
    pub fn snapshot(&self) -> UsageSnapshot {
        self.usage.snapshot()
    }

    /// Get limits configuration
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }
}

/// Per-user quota configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserQuota {
    /// User identifier
    pub user_id: String,
    /// Maximum executions per day
    pub max_executions_per_day: u64,
    /// Maximum tokens per day
    pub max_tokens_per_day: u64,
    /// Maximum workflows
    pub max_workflows: u64,
    /// Maximum concurrent executions
    pub max_concurrent_executions: u64,
}

impl UserQuota {
    /// Create default user quota
    pub fn default_for_user(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            max_executions_per_day: 1000,
            max_tokens_per_day: 1_000_000,
            max_workflows: 100,
            max_concurrent_executions: 10,
        }
    }

    /// Create a free tier quota
    pub fn free_tier(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            max_executions_per_day: 100,
            max_tokens_per_day: 50_000,
            max_workflows: 10,
            max_concurrent_executions: 2,
        }
    }

    /// Create a pro tier quota
    pub fn pro_tier(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            max_executions_per_day: 10_000,
            max_tokens_per_day: 10_000_000,
            max_workflows: 1000,
            max_concurrent_executions: 50,
        }
    }

    /// Create an unlimited quota (for admins/enterprise)
    pub fn unlimited(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            max_executions_per_day: u64::MAX,
            max_tokens_per_day: u64::MAX,
            max_workflows: u64::MAX,
            max_concurrent_executions: u64::MAX,
        }
    }
}

/// Per-user usage tracking
#[derive(Debug, Default)]
pub struct UserUsage {
    executions_today: AtomicU64,
    tokens_today: AtomicU64,
    workflows_count: AtomicU64,
    concurrent_executions: AtomicU64,
}

impl UserUsage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_execution(&self) {
        self.executions_today.fetch_add(1, Ordering::SeqCst);
    }

    pub fn add_tokens(&self, tokens: u64) {
        self.tokens_today.fetch_add(tokens, Ordering::SeqCst);
    }

    pub fn set_workflow_count(&self, count: u64) {
        self.workflows_count.store(count, Ordering::SeqCst);
    }

    pub fn start_execution(&self) -> u64 {
        self.concurrent_executions.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn finish_execution(&self) {
        self.concurrent_executions.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn reset_daily(&self) {
        self.executions_today.store(0, Ordering::SeqCst);
        self.tokens_today.store(0, Ordering::SeqCst);
    }
}

/// User quota manager
pub struct UserQuotaManager {
    quotas: Arc<RwLock<HashMap<String, UserQuota>>>,
    usage: Arc<RwLock<HashMap<String, Arc<UserUsage>>>>,
}

impl Default for UserQuotaManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UserQuotaManager {
    pub fn new() -> Self {
        Self {
            quotas: Arc::new(RwLock::new(HashMap::new())),
            usage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set quota for a user
    pub async fn set_quota(&self, quota: UserQuota) {
        let mut quotas = self.quotas.write().await;
        quotas.insert(quota.user_id.clone(), quota);
    }

    /// Get or create usage tracker for a user
    pub async fn get_usage(&self, user_id: &str) -> Arc<UserUsage> {
        let mut usage = self.usage.write().await;
        usage
            .entry(user_id.to_string())
            .or_insert_with(|| Arc::new(UserUsage::new()))
            .clone()
    }

    /// Check if user can start an execution
    pub async fn check_can_execute(&self, user_id: &str) -> Result<(), ResourceLimitError> {
        let quotas = self.quotas.read().await;
        let quota = quotas
            .get(user_id)
            .cloned()
            .unwrap_or_else(|| UserQuota::default_for_user(user_id));
        drop(quotas);

        let usage = self.get_usage(user_id).await;

        // Check daily executions
        let executions = usage.executions_today.load(Ordering::SeqCst);
        if executions >= quota.max_executions_per_day {
            return Err(ResourceLimitError::UserQuotaExceeded {
                user_id: user_id.to_string(),
                resource: "executions_per_day".to_string(),
                used: executions,
                limit: quota.max_executions_per_day,
            });
        }

        // Check concurrent executions
        let concurrent = usage.concurrent_executions.load(Ordering::SeqCst);
        if concurrent >= quota.max_concurrent_executions {
            return Err(ResourceLimitError::UserQuotaExceeded {
                user_id: user_id.to_string(),
                resource: "concurrent_executions".to_string(),
                used: concurrent,
                limit: quota.max_concurrent_executions,
            });
        }

        Ok(())
    }

    /// Check if user can use more tokens
    pub async fn check_token_usage(
        &self,
        user_id: &str,
        additional_tokens: u64,
    ) -> Result<(), ResourceLimitError> {
        let quotas = self.quotas.read().await;
        let quota = quotas
            .get(user_id)
            .cloned()
            .unwrap_or_else(|| UserQuota::default_for_user(user_id));
        drop(quotas);

        let usage = self.get_usage(user_id).await;
        let tokens = usage.tokens_today.load(Ordering::SeqCst);

        if tokens + additional_tokens > quota.max_tokens_per_day {
            return Err(ResourceLimitError::UserQuotaExceeded {
                user_id: user_id.to_string(),
                resource: "tokens_per_day".to_string(),
                used: tokens + additional_tokens,
                limit: quota.max_tokens_per_day,
            });
        }

        Ok(())
    }

    /// Reset all daily counters (should be called daily)
    pub async fn reset_daily_counters(&self) {
        let usage = self.usage.read().await;
        for user_usage in usage.values() {
            user_usage.reset_daily();
        }
    }
}

/// Token budget tracker for LLM calls
#[derive(Debug)]
pub struct TokenBudget {
    /// Maximum tokens allowed
    max_tokens: u64,
    /// Tokens consumed
    consumed: AtomicU64,
    /// Reserved tokens (for pending requests)
    reserved: AtomicU64,
}

impl TokenBudget {
    /// Create a new token budget
    pub fn new(max_tokens: u64) -> Self {
        Self {
            max_tokens,
            consumed: AtomicU64::new(0),
            reserved: AtomicU64::new(0),
        }
    }

    /// Reserve tokens for an upcoming request
    pub fn reserve(&self, tokens: u64) -> Result<TokenReservation<'_>, ResourceLimitError> {
        let available = self.available();
        if tokens > available {
            return Err(ResourceLimitError::TokenBudgetExceeded {
                used_tokens: self.consumed.load(Ordering::SeqCst) + tokens,
                limit_tokens: self.max_tokens,
            });
        }

        self.reserved.fetch_add(tokens, Ordering::SeqCst);
        Ok(TokenReservation {
            reserved: tokens,
            budget: self,
        })
    }

    /// Get available tokens (not consumed or reserved)
    pub fn available(&self) -> u64 {
        let consumed = self.consumed.load(Ordering::SeqCst);
        let reserved = self.reserved.load(Ordering::SeqCst);
        self.max_tokens.saturating_sub(consumed + reserved)
    }

    /// Get consumed tokens
    pub fn consumed(&self) -> u64 {
        self.consumed.load(Ordering::SeqCst)
    }

    /// Get remaining budget
    pub fn remaining(&self) -> u64 {
        self.max_tokens
            .saturating_sub(self.consumed.load(Ordering::SeqCst))
    }

    /// Get usage percentage
    pub fn usage_percent(&self) -> f64 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        (self.consumed.load(Ordering::SeqCst) as f64 / self.max_tokens as f64) * 100.0
    }
}

/// A reservation of tokens that will be consumed or released
pub struct TokenReservation<'a> {
    reserved: u64,
    budget: &'a TokenBudget,
}

impl<'a> TokenReservation<'a> {
    /// Consume the reserved tokens (call after successful LLM response)
    pub fn consume(self, actual_tokens: u64) {
        // Release the reservation
        self.budget
            .reserved
            .fetch_sub(self.reserved, Ordering::SeqCst);
        // Add actual consumption
        self.budget
            .consumed
            .fetch_add(actual_tokens, Ordering::SeqCst);
        // Prevent drop from being called
        std::mem::forget(self);
    }
}

impl Drop for TokenReservation<'_> {
    fn drop(&mut self) {
        // If dropped without consuming, release the reservation
        self.budget
            .reserved
            .fetch_sub(self.reserved, Ordering::SeqCst);
    }
}

/// Per-node resource usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResourceUsage {
    pub node_id: String,
    pub memory_bytes: u64,
    pub tokens: u64,
    pub api_calls: u64,
    pub execution_time_ms: u64,
}

impl NodeResourceUsage {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            memory_bytes: 0,
            tokens: 0,
            api_calls: 0,
            execution_time_ms: 0,
        }
    }

    /// Convert memory to MB
    pub fn memory_mb(&self) -> f64 {
        self.memory_bytes as f64 / (1024.0 * 1024.0)
    }
}

/// Resource usage tracker with per-node granularity
#[derive(Debug, Default)]
pub struct DetailedResourceUsage {
    /// Global resource usage
    global: Arc<ResourceUsage>,
    /// Per-node resource usage
    node_usage: Arc<RwLock<HashMap<String, NodeResourceUsage>>>,
}

impl DetailedResourceUsage {
    pub fn new() -> Self {
        Self {
            global: Arc::new(ResourceUsage::new()),
            node_usage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get global usage tracker
    pub fn global(&self) -> Arc<ResourceUsage> {
        Arc::clone(&self.global)
    }

    /// Record node execution
    pub async fn record_node_execution(
        &self,
        node_id: String,
        memory_bytes: u64,
        tokens: u64,
        api_calls: u64,
        execution_time_ms: u64,
    ) {
        let mut nodes = self.node_usage.write().await;
        nodes.insert(
            node_id.clone(),
            NodeResourceUsage {
                node_id,
                memory_bytes,
                tokens,
                api_calls,
                execution_time_ms,
            },
        );
    }

    /// Get usage for a specific node
    pub async fn get_node_usage(&self, node_id: &str) -> Option<NodeResourceUsage> {
        let nodes = self.node_usage.read().await;
        nodes.get(node_id).cloned()
    }

    /// Get all node usage data
    pub async fn get_all_node_usage(&self) -> Vec<NodeResourceUsage> {
        let nodes = self.node_usage.read().await;
        nodes.values().cloned().collect()
    }

    /// Get top N nodes by memory usage
    pub async fn top_memory_consumers(&self, n: usize) -> Vec<NodeResourceUsage> {
        let mut nodes = self.get_all_node_usage().await;
        nodes.sort_by_key(|x| std::cmp::Reverse(x.memory_bytes));
        nodes.into_iter().take(n).collect()
    }

    /// Get top N nodes by token usage
    pub async fn top_token_consumers(&self, n: usize) -> Vec<NodeResourceUsage> {
        let mut nodes = self.get_all_node_usage().await;
        nodes.sort_by_key(|x| std::cmp::Reverse(x.tokens));
        nodes.into_iter().take(n).collect()
    }
}

/// Resource usage history for analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageHistoryEntry {
    pub timestamp_secs: u64,
    pub snapshot: UsageSnapshot,
}

/// Resource usage history tracker
#[derive(Debug)]
pub struct ResourceUsageHistory {
    entries: Arc<RwLock<Vec<UsageHistoryEntry>>>,
    max_entries: usize,
}

impl ResourceUsageHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            max_entries,
        }
    }

    /// Record a snapshot
    pub async fn record(&self, snapshot: UsageSnapshot) {
        let mut entries = self.entries.write().await;
        entries.push(UsageHistoryEntry {
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            snapshot,
        });

        // Keep only the latest max_entries
        if entries.len() > self.max_entries {
            entries.remove(0);
        }
    }

    /// Get all history entries
    pub async fn get_all(&self) -> Vec<UsageHistoryEntry> {
        let entries = self.entries.read().await;
        entries.clone()
    }

    /// Get usage growth rate (MB/sec, tokens/sec, etc.)
    pub async fn get_growth_rates(&self) -> Option<UsageGrowthRates> {
        let entries = self.entries.read().await;
        if entries.len() < 2 {
            return None;
        }

        let first = &entries[0];
        let last = &entries[entries.len() - 1];
        let time_diff = last.timestamp_secs.saturating_sub(first.timestamp_secs);

        if time_diff == 0 {
            return None;
        }

        Some(UsageGrowthRates {
            memory_mb_per_sec: (last
                .snapshot
                .memory_mb
                .saturating_sub(first.snapshot.memory_mb)) as f64
                / time_diff as f64,
            tokens_per_sec: (last
                .snapshot
                .total_tokens
                .saturating_sub(first.snapshot.total_tokens)) as f64
                / time_diff as f64,
            api_calls_per_sec: (last
                .snapshot
                .api_calls
                .saturating_sub(first.snapshot.api_calls)) as f64
                / time_diff as f64,
        })
    }

    /// Clear history
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }
}

/// Resource usage growth rates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageGrowthRates {
    pub memory_mb_per_sec: f64,
    pub tokens_per_sec: f64,
    pub api_calls_per_sec: f64,
}

/// Real-time resource monitor
pub struct ResourceMonitor {
    enforcer: Arc<ResourceEnforcer>,
    history: Arc<ResourceUsageHistory>,
    monitoring_interval_ms: u64,
}

impl ResourceMonitor {
    pub fn new(
        enforcer: Arc<ResourceEnforcer>,
        monitoring_interval_ms: u64,
        max_history_entries: usize,
    ) -> Self {
        Self {
            enforcer,
            history: Arc::new(ResourceUsageHistory::new(max_history_entries)),
            monitoring_interval_ms,
        }
    }

    /// Start monitoring in the background
    pub fn start_monitoring(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(
                self.monitoring_interval_ms,
            ));

            loop {
                interval.tick().await;

                // Take a snapshot and record it
                let snapshot = self.enforcer.snapshot();
                self.history.record(snapshot).await;

                // Check all limits and log warnings
                match self.enforcer.check_all() {
                    Ok(warnings) => {
                        for warning in warnings {
                            // In a real implementation, you might emit events here
                            eprintln!("Resource warning: {:?}", warning);
                        }
                    }
                    Err(e) => {
                        // Resource limit exceeded
                        eprintln!("Resource limit exceeded: {}", e);
                        // In a real implementation, you might trigger workflow termination
                        break;
                    }
                }
            }
        })
    }

    /// Get the usage history
    pub fn history(&self) -> Arc<ResourceUsageHistory> {
        Arc::clone(&self.history)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory_mb, 512);
        assert_eq!(limits.max_execution_time_secs, 300);
        assert_eq!(limits.max_tokens_per_execution, 100_000);
    }

    #[test]
    fn test_limits_builder() {
        let limits = LimitsBuilder::new()
            .max_memory_mb(1024)
            .max_execution_time_secs(600)
            .max_tokens_per_execution(50_000)
            .build();

        assert_eq!(limits.max_memory_mb, 1024);
        assert_eq!(limits.max_execution_time_secs, 600);
        assert_eq!(limits.max_tokens_per_execution, 50_000);
    }

    #[test]
    fn test_resource_usage_tracking() {
        let usage = ResourceUsage::new();

        usage.add_memory(1024 * 1024 * 100); // 100 MB
        assert_eq!(usage.memory_mb(), 100);

        usage.add_tokens(1000);
        usage.add_tokens(500);
        assert_eq!(usage.tokens(), 1500);

        usage.add_api_call();
        usage.add_api_call();
        assert_eq!(usage.api_calls(), 2);

        usage.add_node_execution();
        assert_eq!(usage.nodes_executed(), 1);
    }

    #[test]
    fn test_resource_enforcer_memory_limit() {
        let limits = LimitsBuilder::new().max_memory_mb(100).build();
        let enforcer = ResourceEnforcer::new(limits);

        // Add 50 MB - should be OK
        enforcer.usage().add_memory(50 * 1024 * 1024);
        assert!(enforcer.check_memory().is_ok());

        // Add 60 MB more - should exceed limit
        enforcer.usage().add_memory(60 * 1024 * 1024);
        let result = enforcer.check_memory();
        assert!(result.is_err());
    }

    #[test]
    fn test_token_budget() {
        let budget = TokenBudget::new(1000);

        // Reserve 500 tokens
        let reservation = budget.reserve(500).unwrap();
        assert_eq!(budget.available(), 500);

        // Consume 400 tokens
        reservation.consume(400);
        assert_eq!(budget.consumed(), 400);
        assert_eq!(budget.remaining(), 600);
    }

    #[test]
    fn test_token_budget_exceeded() {
        let budget = TokenBudget::new(100);

        // Try to reserve more than available
        let result = budget.reserve(150);
        assert!(result.is_err());
    }

    #[test]
    fn test_user_quota_tiers() {
        let free = UserQuota::free_tier("user1");
        assert_eq!(free.max_executions_per_day, 100);
        assert_eq!(free.max_tokens_per_day, 50_000);

        let pro = UserQuota::pro_tier("user2");
        assert_eq!(pro.max_executions_per_day, 10_000);
        assert_eq!(pro.max_tokens_per_day, 10_000_000);
    }

    #[tokio::test]
    async fn test_user_quota_manager() {
        let manager = UserQuotaManager::new();

        // Set a quota
        let quota = UserQuota::free_tier("test_user");
        manager.set_quota(quota).await;

        // Check execution limit
        let result = manager.check_can_execute("test_user").await;
        assert!(result.is_ok());

        // Record some executions
        let usage = manager.get_usage("test_user").await;
        for _ in 0..100 {
            usage.add_execution();
        }

        // Should now be at limit
        let result = manager.check_can_execute("test_user").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_resource_warning_threshold() {
        let limits = LimitsBuilder::new()
            .max_tokens_per_execution(1000)
            .warning_threshold_percent(80)
            .build();
        let enforcer = ResourceEnforcer::new(limits);

        // Add 850 tokens - should trigger warning
        enforcer.usage().add_tokens(850);
        let result = enforcer.check_token_budget(0).unwrap();
        assert!(matches!(
            result,
            Some(ResourceWarning::TokensApproachingLimit { .. })
        ));
    }

    #[tokio::test]
    async fn test_detailed_resource_usage() {
        let usage = DetailedResourceUsage::new();

        // Record usage for node1
        usage
            .record_node_execution("node1".to_string(), 100_000_000, 500, 2, 1000)
            .await;

        // Record usage for node2
        usage
            .record_node_execution("node2".to_string(), 200_000_000, 300, 1, 2000)
            .await;

        // Check node usage
        let node1_usage = usage.get_node_usage("node1").await.unwrap();
        assert_eq!(node1_usage.tokens, 500);
        assert_eq!(node1_usage.api_calls, 2);

        // Check all node usage
        let all_usage = usage.get_all_node_usage().await;
        assert_eq!(all_usage.len(), 2);

        // Check top memory consumers
        let top_memory = usage.top_memory_consumers(1).await;
        assert_eq!(top_memory[0].node_id, "node2");
        assert_eq!(top_memory[0].memory_bytes, 200_000_000);

        // Check top token consumers
        let top_tokens = usage.top_token_consumers(1).await;
        assert_eq!(top_tokens[0].node_id, "node1");
        assert_eq!(top_tokens[0].tokens, 500);
    }

    #[tokio::test]
    async fn test_resource_usage_history() {
        let history = ResourceUsageHistory::new(100);

        // Record snapshots with longer delays to ensure different timestamps
        for i in 0..5 {
            let snapshot = UsageSnapshot {
                memory_mb: i * 10,
                total_tokens: i * 100,
                api_calls: i * 5,
                nodes_executed: i,
                concurrent_nodes: 1,
                elapsed_secs: i,
            };
            history.record(snapshot).await;
            // Add longer delay to ensure different timestamps (100ms to ensure at least 1 second diff)
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }

        // Check history
        let entries = history.get_all().await;
        assert_eq!(entries.len(), 5);

        // Check growth rates
        let rates = history.get_growth_rates().await;
        assert!(rates.is_some());
        let rates = rates.unwrap();
        assert!(rates.memory_mb_per_sec >= 0.0);
        assert!(rates.tokens_per_sec >= 0.0);

        // Clear history
        history.clear().await;
        let entries = history.get_all().await;
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn test_resource_usage_history_max_entries() {
        let history = ResourceUsageHistory::new(3);

        // Record 5 snapshots
        for i in 0..5 {
            let snapshot = UsageSnapshot {
                memory_mb: i,
                total_tokens: i * 100,
                api_calls: i,
                nodes_executed: i,
                concurrent_nodes: 1,
                elapsed_secs: i,
            };
            history.record(snapshot).await;
        }

        // Should only keep the latest 3
        let entries = history.get_all().await;
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].snapshot.memory_mb, 2);
        assert_eq!(entries[2].snapshot.memory_mb, 4);
    }

    #[tokio::test]
    async fn test_node_resource_usage_memory_conversion() {
        let usage = NodeResourceUsage {
            node_id: "test".to_string(),
            memory_bytes: 1024 * 1024 * 50, // 50 MB
            tokens: 1000,
            api_calls: 5,
            execution_time_ms: 2000,
        };

        assert_eq!(usage.memory_mb(), 50.0);
    }

    #[tokio::test]
    async fn test_resource_monitor() {
        let limits = ResourceLimits::default();
        let enforcer = Arc::new(ResourceEnforcer::new(limits));
        let monitor = Arc::new(ResourceMonitor::new(enforcer.clone(), 50, 100));

        // Start monitoring
        let _handle = monitor.clone().start_monitoring();

        // Add some usage
        enforcer.usage().add_memory(10 * 1024 * 1024); // 10 MB
        enforcer.usage().add_tokens(1000);

        // Wait for a few monitoring cycles
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Check that history has been recorded
        let history_entries = monitor.history().get_all().await;
        assert!(!history_entries.is_empty());
    }
}
