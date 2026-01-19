//! Caching layer for authorization decisions
//!
//! Implements a multi-level cache to minimize database queries:
//! 1. In-memory cache (moka) for hot paths
//! 2. Optional Redis cache for distributed deployments

use std::time::Duration;

/// Cache statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            self.hits as f64 / (self.hits + self.misses) as f64
        }
    }
}

/// Cache invalidation strategy
#[derive(Debug, Clone)]
pub enum InvalidationStrategy {
    /// Invalidate immediately
    Immediate,

    /// Invalidate after a delay (eventual consistency)
    Delayed(Duration),

    /// Invalidate specific patterns
    Pattern(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_stats() {
        let stats = CacheStats {
            hits: 80,
            misses: 20,
            ..Default::default()
        };

        assert_eq!(stats.hit_rate(), 0.8);
    }
}
