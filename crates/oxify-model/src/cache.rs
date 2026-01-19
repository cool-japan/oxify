//! Cache Management Module
//!
//! This module provides comprehensive caching utilities for workflow execution,
//! including cache key generation, TTL management, invalidation strategies,
//! and cache warming capabilities.
//!
//! # Features
//!
//! - **Cache Key Generation:** Deterministic key generation for LLM prompts, code, and retrieval
//! - **TTL Management:** Time-based expiration with customizable policies
//! - **Invalidation Strategies:** Pattern-based and dependency-based invalidation
//! - **Cache Warming:** Preload frequently used results
//! - **Cache Statistics:** Track hit rates, misses, and performance metrics
//!
//! # Example
//!
//! ```rust
//! use oxify_model::cache::{CacheKeyGenerator, CacheConfig, CachePolicy};
//! use std::time::Duration;
//!
//! // Generate cache key for LLM prompt
//! let key = CacheKeyGenerator::llm_prompt_key(
//!     "gpt-4",
//!     "Summarize this text",
//!     &[("temperature", "0.7")],
//! );
//!
//! // Configure cache policy
//! let config = CacheConfig::default()
//!     .with_ttl(Duration::from_secs(3600))
//!     .with_max_size(1000);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

/// Cache policy defining caching behavior
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CachePolicy {
    /// No caching
    NoCache,
    /// Cache with time-to-live
    Ttl(Duration),
    /// Cache until explicitly invalidated
    Indefinite,
    /// Cache with least-recently-used eviction
    Lru {
        max_size: usize,
        ttl: Option<Duration>,
    },
    /// Cache with least-frequently-used eviction
    Lfu {
        max_size: usize,
        ttl: Option<Duration>,
    },
}

impl Default for CachePolicy {
    fn default() -> Self {
        CachePolicy::Ttl(Duration::from_secs(3600)) // 1 hour default
    }
}

/// Cache invalidation strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InvalidationStrategy {
    /// Invalidate all cache entries
    All,
    /// Invalidate entries matching a pattern
    Pattern(String),
    /// Invalidate entries for specific node IDs
    NodeIds(Vec<String>),
    /// Invalidate entries older than duration
    OlderThan(Duration),
    /// Invalidate entries by tags
    Tags(Vec<String>),
    /// Invalidate entries with specific prefix
    Prefix(String),
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Default cache policy
    pub policy: CachePolicy,
    /// Enable cache warming on startup
    pub enable_warming: bool,
    /// Maximum cache size in bytes (0 = unlimited)
    pub max_size_bytes: u64,
    /// Enable cache compression
    pub enable_compression: bool,
    /// Invalidation strategy
    pub invalidation_strategy: InvalidationStrategy,
    /// Enable cache statistics tracking
    pub enable_stats: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            policy: CachePolicy::default(),
            enable_warming: false,
            max_size_bytes: 0,
            enable_compression: false,
            invalidation_strategy: InvalidationStrategy::OlderThan(Duration::from_secs(86400)), // 24 hours
            enable_stats: true,
        }
    }
}

impl CacheConfig {
    /// Set the TTL duration
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.policy = CachePolicy::Ttl(ttl);
        self
    }

    /// Set the maximum cache size
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.policy = match self.policy {
            CachePolicy::Lru { ttl, .. } => CachePolicy::Lru { max_size, ttl },
            CachePolicy::Lfu { ttl, .. } => CachePolicy::Lfu { max_size, ttl },
            _ => CachePolicy::Lru {
                max_size,
                ttl: Some(Duration::from_secs(3600)),
            },
        };
        self
    }

    /// Enable cache warming
    pub fn with_warming(mut self, enable: bool) -> Self {
        self.enable_warming = enable;
        self
    }

    /// Set maximum cache size in bytes
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_size_bytes = max_bytes;
        self
    }

    /// Enable compression
    pub fn with_compression(mut self, enable: bool) -> Self {
        self.enable_compression = enable;
        self
    }

    /// Set invalidation strategy
    pub fn with_invalidation(mut self, strategy: InvalidationStrategy) -> Self {
        self.invalidation_strategy = strategy;
        self
    }
}

/// Cache entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Cache key
    pub key: String,
    /// Cached value (serialized)
    pub value: Vec<u8>,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last access timestamp
    pub last_accessed: SystemTime,
    /// Access count
    pub access_count: u64,
    /// Size in bytes
    pub size_bytes: usize,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// TTL duration
    pub ttl: Option<Duration>,
}

impl CacheEntry {
    /// Check if the entry has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            if let Ok(elapsed) = self.created_at.elapsed() {
                return elapsed > ttl;
            }
        }
        false
    }

    /// Update access metadata
    pub fn record_access(&mut self) {
        self.last_accessed = SystemTime::now();
        self.access_count += 1;
    }

    /// Get age of the entry
    pub fn age(&self) -> Duration {
        self.created_at.elapsed().unwrap_or(Duration::from_secs(0))
    }
}

/// Cache key generator utilities
pub struct CacheKeyGenerator;

impl CacheKeyGenerator {
    /// Generate cache key for LLM prompt
    ///
    /// Creates a deterministic cache key based on:
    /// - Model name
    /// - Prompt text
    /// - Configuration parameters (temperature, max_tokens, etc.)
    pub fn llm_prompt_key(model: &str, prompt: &str, params: &[(&str, &str)]) -> String {
        use std::collections::BTreeMap;

        // Sort params for deterministic key generation
        let sorted_params: BTreeMap<_, _> = params.iter().map(|(k, v)| (*k, *v)).collect();
        let params_str = sorted_params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        format!(
            "llm:{}:{}:{}",
            model,
            Self::hash_string(prompt),
            Self::hash_string(&params_str)
        )
    }

    /// Generate cache key for code execution
    pub fn code_execution_key(
        runtime: &str,
        code: &str,
        input_vars: &HashMap<String, String>,
    ) -> String {
        use std::collections::BTreeMap;

        let sorted_vars: BTreeMap<_, _> = input_vars.iter().collect();
        let vars_str = sorted_vars
            .iter()
            .map(|(k, v)| format!("{}={}", k, Self::hash_string(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!(
            "code:{}:{}:{}",
            runtime,
            Self::hash_string(code),
            Self::hash_string(&vars_str)
        )
    }

    /// Generate cache key for vector retrieval
    pub fn vector_retrieval_key(
        collection: &str,
        query: &str,
        top_k: usize,
        filters: &HashMap<String, String>,
    ) -> String {
        use std::collections::BTreeMap;

        let sorted_filters: BTreeMap<_, _> = filters.iter().collect();
        let filters_str = sorted_filters
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        format!(
            "vector:{}:{}:{}:{}",
            collection,
            Self::hash_string(query),
            top_k,
            Self::hash_string(&filters_str)
        )
    }

    /// Generate cache key for workflow execution
    pub fn workflow_execution_key(
        workflow_id: &str,
        version: &str,
        inputs: &HashMap<String, String>,
    ) -> String {
        use std::collections::BTreeMap;

        let sorted_inputs: BTreeMap<_, _> = inputs.iter().collect();
        let inputs_str = sorted_inputs
            .iter()
            .map(|(k, v)| format!("{}={}", k, Self::hash_string(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!(
            "workflow:{}:{}:{}",
            workflow_id,
            version,
            Self::hash_string(&inputs_str)
        )
    }

    /// Hash a string to create a shorter, deterministic identifier
    fn hash_string(s: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total cache hits
    pub hits: u64,
    /// Total cache misses
    pub misses: u64,
    /// Total evictions
    pub evictions: u64,
    /// Total entries in cache
    pub entry_count: usize,
    /// Total size in bytes
    pub total_bytes: u64,
    /// Hit rate percentage
    pub hit_rate: f64,
    /// Average access count per entry
    pub avg_access_count: f64,
}

impl CacheStats {
    /// Calculate hit rate
    pub fn calculate_hit_rate(&mut self) {
        let total = self.hits + self.misses;
        self.hit_rate = if total > 0 {
            (self.hits as f64 / total as f64) * 100.0
        } else {
            0.0
        };
    }

    /// Record a cache hit
    pub fn record_hit(&mut self) {
        self.hits += 1;
        self.calculate_hit_rate();
    }

    /// Record a cache miss
    pub fn record_miss(&mut self) {
        self.misses += 1;
        self.calculate_hit_rate();
    }

    /// Record an eviction
    pub fn record_eviction(&mut self) {
        self.evictions += 1;
    }

    /// Update entry statistics
    pub fn update_stats(&mut self, entry_count: usize, total_bytes: u64, total_accesses: u64) {
        self.entry_count = entry_count;
        self.total_bytes = total_bytes;
        self.avg_access_count = if entry_count > 0 {
            total_accesses as f64 / entry_count as f64
        } else {
            0.0
        };
    }
}

/// Cache warming configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheWarmingConfig {
    /// Enable automatic warming on startup
    pub enabled: bool,
    /// Node IDs to warm
    pub node_ids: Vec<String>,
    /// Maximum entries to warm
    pub max_entries: usize,
    /// Warming strategy
    pub strategy: WarmingStrategy,
}

/// Cache warming strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WarmingStrategy {
    /// Warm most frequently accessed entries
    MostFrequent,
    /// Warm most recently accessed entries
    MostRecent,
    /// Warm entries matching specific patterns
    Pattern(Vec<String>),
    /// Warm all entries
    All,
}

impl Default for CacheWarmingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_ids: Vec::new(),
            max_entries: 100,
            strategy: WarmingStrategy::MostFrequent,
        }
    }
}

/// Cache invalidation plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidationPlan {
    /// Strategy to use
    pub strategy: InvalidationStrategy,
    /// Estimated entries to invalidate
    pub estimated_count: usize,
    /// Tags affected
    pub affected_tags: Vec<String>,
    /// Node IDs affected
    pub affected_nodes: Vec<String>,
}

impl InvalidationPlan {
    /// Create a new invalidation plan
    pub fn new(strategy: InvalidationStrategy) -> Self {
        Self {
            strategy,
            estimated_count: 0,
            affected_tags: Vec::new(),
            affected_nodes: Vec::new(),
        }
    }

    /// Add affected tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.affected_tags = tags;
        self
    }

    /// Add affected node IDs
    pub fn with_nodes(mut self, nodes: Vec<String>) -> Self {
        self.affected_nodes = nodes;
        self
    }

    /// Set estimated count
    pub fn with_count(mut self, count: usize) -> Self {
        self.estimated_count = count;
        self
    }
}

/// Cache manager for workflow caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManager {
    /// Cache configuration
    pub config: CacheConfig,
    /// Cache statistics
    pub stats: CacheStats,
    /// Cache entries
    entries: HashMap<String, CacheEntry>,
    /// Tag index for fast lookups
    tag_index: HashMap<String, HashSet<String>>,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            stats: CacheStats::default(),
            entries: HashMap::new(),
            tag_index: HashMap::new(),
        }
    }

    /// Get a cache entry
    pub fn get(&mut self, key: &str) -> Option<&CacheEntry> {
        if let Some(entry) = self.entries.get_mut(key) {
            if entry.is_expired() {
                self.stats.record_miss();
                self.entries.remove(key);
                return None;
            }
            entry.record_access();
            self.stats.record_hit();
            self.entries.get(key)
        } else {
            self.stats.record_miss();
            None
        }
    }

    /// Put a cache entry
    pub fn put(&mut self, mut entry: CacheEntry) {
        // Check size limits
        if self.config.max_size_bytes > 0 {
            let total_size = self.total_size();
            if total_size + entry.size_bytes as u64 > self.config.max_size_bytes {
                self.evict_entries(entry.size_bytes);
            }
        }

        // Update tag index
        for tag in &entry.tags {
            self.tag_index
                .entry(tag.clone())
                .or_default()
                .insert(entry.key.clone());
        }

        // Set TTL from policy if not specified
        if entry.ttl.is_none() {
            entry.ttl = match &self.config.policy {
                CachePolicy::Ttl(duration) => Some(*duration),
                CachePolicy::Lru { ttl, .. } | CachePolicy::Lfu { ttl, .. } => *ttl,
                _ => None,
            };
        }

        self.entries.insert(entry.key.clone(), entry);
        self.update_stats();
    }

    /// Invalidate cache entries based on strategy
    pub fn invalidate(&mut self, strategy: &InvalidationStrategy) -> usize {
        let keys_to_remove: Vec<String> = match strategy {
            InvalidationStrategy::All => self.entries.keys().cloned().collect(),

            InvalidationStrategy::Pattern(pattern) => self
                .entries
                .keys()
                .filter(|k| k.contains(pattern))
                .cloned()
                .collect(),

            InvalidationStrategy::NodeIds(node_ids) => self
                .entries
                .keys()
                .filter(|k| node_ids.iter().any(|nid| k.contains(nid)))
                .cloned()
                .collect(),

            InvalidationStrategy::OlderThan(duration) => self
                .entries
                .iter()
                .filter(|(_, entry)| entry.age() > *duration)
                .map(|(k, _)| k.clone())
                .collect(),

            InvalidationStrategy::Tags(tags) => {
                let mut keys = HashSet::new();
                for tag in tags {
                    if let Some(tag_keys) = self.tag_index.get(tag) {
                        keys.extend(tag_keys.iter().cloned());
                    }
                }
                keys.into_iter().collect()
            }

            InvalidationStrategy::Prefix(prefix) => self
                .entries
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect(),
        };

        let count = keys_to_remove.len();
        for key in keys_to_remove {
            self.remove(&key);
        }
        count
    }

    /// Remove a specific entry
    pub fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            // Remove from tag index
            for tag in &entry.tags {
                if let Some(tag_keys) = self.tag_index.get_mut(tag) {
                    tag_keys.remove(key);
                    if tag_keys.is_empty() {
                        self.tag_index.remove(tag);
                    }
                }
            }
            self.stats.record_eviction();
        }
        self.update_stats();
    }

    /// Evict entries to make space
    fn evict_entries(&mut self, needed_space: usize) {
        let strategy = &self.config.policy;
        match strategy {
            CachePolicy::Lru { .. } => self.evict_lru(needed_space),
            CachePolicy::Lfu { .. } => self.evict_lfu(needed_space),
            _ => self.evict_oldest(needed_space),
        }
    }

    /// Evict least recently used entries
    fn evict_lru(&mut self, needed_space: usize) {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by_key(|(_, e)| e.last_accessed);

        let mut freed = 0;
        let mut to_remove = Vec::new();

        for (key, entry) in entries {
            to_remove.push(key.clone());
            freed += entry.size_bytes;
            if freed >= needed_space {
                break;
            }
        }

        for key in to_remove {
            self.remove(&key);
        }
    }

    /// Evict least frequently used entries
    fn evict_lfu(&mut self, needed_space: usize) {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by_key(|(_, e)| e.access_count);

        let mut freed = 0;
        let mut to_remove = Vec::new();

        for (key, entry) in entries {
            to_remove.push(key.clone());
            freed += entry.size_bytes;
            if freed >= needed_space {
                break;
            }
        }

        for key in to_remove {
            self.remove(&key);
        }
    }

    /// Evict oldest entries
    fn evict_oldest(&mut self, needed_space: usize) {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by_key(|(_, e)| e.created_at);

        let mut freed = 0;
        let mut to_remove = Vec::new();

        for (key, entry) in entries {
            to_remove.push(key.clone());
            freed += entry.size_bytes;
            if freed >= needed_space {
                break;
            }
        }

        for key in to_remove {
            self.remove(&key);
        }
    }

    /// Get total cache size in bytes
    fn total_size(&self) -> u64 {
        self.entries.values().map(|e| e.size_bytes as u64).sum()
    }

    /// Update statistics
    fn update_stats(&mut self) {
        let total_accesses: u64 = self.entries.values().map(|e| e.access_count).sum();
        self.stats
            .update_stats(self.entries.len(), self.total_size(), total_accesses);
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Clear all cache entries
    pub fn clear(&mut self) {
        let count = self.entries.len();
        self.entries.clear();
        self.tag_index.clear();
        for _ in 0..count {
            self.stats.record_eviction();
        }
        self.update_stats();
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert!(config.enable_stats);
        assert!(!config.enable_warming);
        assert_eq!(config.max_size_bytes, 0);
    }

    #[test]
    fn test_cache_config_builder() {
        let config = CacheConfig::default()
            .with_ttl(Duration::from_secs(1800))
            .with_max_size(500)
            .with_warming(true)
            .with_compression(true);

        assert!(config.enable_warming);
        assert!(config.enable_compression);
        assert!(matches!(config.policy, CachePolicy::Lru { .. }));
    }

    #[test]
    fn test_llm_prompt_key_generation() {
        let key1 = CacheKeyGenerator::llm_prompt_key(
            "gpt-4",
            "Hello world",
            &[("temperature", "0.7"), ("max_tokens", "100")],
        );

        let key2 = CacheKeyGenerator::llm_prompt_key(
            "gpt-4",
            "Hello world",
            &[("max_tokens", "100"), ("temperature", "0.7")],
        );

        // Keys should be the same regardless of param order
        assert_eq!(key1, key2);
        assert!(key1.starts_with("llm:gpt-4:"));
    }

    #[test]
    fn test_code_execution_key_generation() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "10".to_string());
        vars.insert("y".to_string(), "20".to_string());

        let key = CacheKeyGenerator::code_execution_key("rust", "fn main() {}", &vars);
        assert!(key.starts_with("code:rust:"));
    }

    #[test]
    fn test_vector_retrieval_key_generation() {
        let mut filters = HashMap::new();
        filters.insert("category".to_string(), "tech".to_string());

        let key = CacheKeyGenerator::vector_retrieval_key("docs", "search query", 10, &filters);
        assert!(key.starts_with("vector:docs:"));
        assert!(key.contains(":10:"));
    }

    #[test]
    fn test_workflow_execution_key_generation() {
        let mut inputs = HashMap::new();
        inputs.insert("user_id".to_string(), "123".to_string());

        let key = CacheKeyGenerator::workflow_execution_key("workflow-1", "v1.0.0", &inputs);
        assert!(key.starts_with("workflow:workflow-1:v1.0.0:"));
    }

    #[test]
    fn test_cache_entry_expiration() {
        let entry = CacheEntry {
            key: "test".to_string(),
            value: vec![1, 2, 3],
            created_at: SystemTime::now() - Duration::from_secs(3700),
            last_accessed: SystemTime::now(),
            access_count: 1,
            size_bytes: 3,
            tags: vec![],
            ttl: Some(Duration::from_secs(3600)), // 1 hour
        };

        assert!(entry.is_expired());
    }

    #[test]
    fn test_cache_entry_not_expired() {
        let entry = CacheEntry {
            key: "test".to_string(),
            value: vec![1, 2, 3],
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 1,
            size_bytes: 3,
            tags: vec![],
            ttl: Some(Duration::from_secs(3600)),
        };

        assert!(!entry.is_expired());
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut stats = CacheStats::default();

        stats.record_hit();
        stats.record_hit();
        stats.record_miss();

        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 66.666).abs() < 0.01);
    }

    #[test]
    fn test_cache_manager_put_and_get() {
        let config = CacheConfig::default();
        let mut manager = CacheManager::new(config);

        let entry = CacheEntry {
            key: "test-key".to_string(),
            value: vec![1, 2, 3, 4],
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            size_bytes: 4,
            tags: vec!["test".to_string()],
            ttl: Some(Duration::from_secs(3600)),
        };

        manager.put(entry);

        let retrieved = manager.get("test-key");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, vec![1, 2, 3, 4]);
        assert_eq!(manager.stats.hits, 1);
    }

    #[test]
    fn test_cache_manager_miss() {
        let config = CacheConfig::default();
        let mut manager = CacheManager::new(config);

        let result = manager.get("nonexistent");
        assert!(result.is_none());
        assert_eq!(manager.stats.misses, 1);
    }

    #[test]
    fn test_cache_invalidation_all() {
        let config = CacheConfig::default();
        let mut manager = CacheManager::new(config);

        for i in 0..5 {
            let entry = CacheEntry {
                key: format!("key-{}", i),
                value: vec![i as u8],
                created_at: SystemTime::now(),
                last_accessed: SystemTime::now(),
                access_count: 0,
                size_bytes: 1,
                tags: vec![],
                ttl: None,
            };
            manager.put(entry);
        }

        let count = manager.invalidate(&InvalidationStrategy::All);
        assert_eq!(count, 5);
        assert_eq!(manager.entries.len(), 0);
    }

    #[test]
    fn test_cache_invalidation_pattern() {
        let config = CacheConfig::default();
        let mut manager = CacheManager::new(config);

        let entry1 = CacheEntry {
            key: "llm:gpt-4:test".to_string(),
            value: vec![1],
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            size_bytes: 1,
            tags: vec![],
            ttl: None,
        };

        let entry2 = CacheEntry {
            key: "code:rust:test".to_string(),
            value: vec![2],
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            size_bytes: 1,
            tags: vec![],
            ttl: None,
        };

        manager.put(entry1);
        manager.put(entry2);

        let count = manager.invalidate(&InvalidationStrategy::Pattern("llm:".to_string()));
        assert_eq!(count, 1);
        assert_eq!(manager.entries.len(), 1);
    }

    #[test]
    fn test_cache_invalidation_tags() {
        let config = CacheConfig::default();
        let mut manager = CacheManager::new(config);

        let entry1 = CacheEntry {
            key: "key1".to_string(),
            value: vec![1],
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            size_bytes: 1,
            tags: vec!["tag1".to_string()],
            ttl: None,
        };

        let entry2 = CacheEntry {
            key: "key2".to_string(),
            value: vec![2],
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            size_bytes: 1,
            tags: vec!["tag2".to_string()],
            ttl: None,
        };

        manager.put(entry1);
        manager.put(entry2);

        let count = manager.invalidate(&InvalidationStrategy::Tags(vec!["tag1".to_string()]));
        assert_eq!(count, 1);
        assert!(manager.get("key2").is_some());
    }

    #[test]
    fn test_cache_invalidation_prefix() {
        let config = CacheConfig::default();
        let mut manager = CacheManager::new(config);

        let entry1 = CacheEntry {
            key: "workflow:123:v1".to_string(),
            value: vec![1],
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            size_bytes: 1,
            tags: vec![],
            ttl: None,
        };

        let entry2 = CacheEntry {
            key: "llm:gpt-4:test".to_string(),
            value: vec![2],
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            size_bytes: 1,
            tags: vec![],
            ttl: None,
        };

        manager.put(entry1);
        manager.put(entry2);

        let count = manager.invalidate(&InvalidationStrategy::Prefix("workflow:".to_string()));
        assert_eq!(count, 1);
        assert!(manager.get("llm:gpt-4:test").is_some());
    }

    #[test]
    fn test_cache_manager_clear() {
        let config = CacheConfig::default();
        let mut manager = CacheManager::new(config);

        for i in 0..3 {
            let entry = CacheEntry {
                key: format!("key-{}", i),
                value: vec![i as u8],
                created_at: SystemTime::now(),
                last_accessed: SystemTime::now(),
                access_count: 0,
                size_bytes: 1,
                tags: vec![],
                ttl: None,
            };
            manager.put(entry);
        }

        manager.clear();
        assert_eq!(manager.entries.len(), 0);
        assert_eq!(manager.stats.evictions, 3);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let config = CacheConfig::default().with_max_bytes(10);
        let mut manager = CacheManager::new(config);

        // Add entries that exceed max size
        for i in 0..5 {
            let entry = CacheEntry {
                key: format!("key-{}", i),
                value: vec![i as u8],
                created_at: SystemTime::now(),
                last_accessed: SystemTime::now() - Duration::from_secs((5 - i) as u64),
                access_count: 0,
                size_bytes: 3,
                tags: vec![],
                ttl: None,
            };
            manager.put(entry);
        }

        // Should have evicted some entries to stay under limit
        assert!(manager.total_size() <= 10);
    }

    #[test]
    fn test_invalidation_plan() {
        let plan = InvalidationPlan::new(InvalidationStrategy::All)
            .with_tags(vec!["tag1".to_string()])
            .with_nodes(vec!["node1".to_string()])
            .with_count(10);

        assert_eq!(plan.estimated_count, 10);
        assert_eq!(plan.affected_tags.len(), 1);
        assert_eq!(plan.affected_nodes.len(), 1);
    }
}
