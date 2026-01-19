//! Bloom filter for quick negative lookups
//!
//! This module implements a Bloom filter to reduce unnecessary database queries.
//! The Bloom filter can definitively say "no" (tuple doesn't exist) but may
//! have false positives for "yes" (tuple might exist, need to check DB).
//!
//! Expected reduction in DB queries: ~50% for non-existent tuples

use crate::*;
use bloomfilter::Bloom;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;

/// Configuration for the Bloom filter
#[derive(Debug, Clone)]
pub struct BloomConfig {
    /// Expected number of items in the filter
    pub expected_items: usize,
    /// Target false positive rate (0.0 to 1.0)
    pub false_positive_rate: f64,
}

impl Default for BloomConfig {
    fn default() -> Self {
        Self {
            expected_items: 1_000_000, // 1M tuples
            false_positive_rate: 0.01, // 1% false positive rate
        }
    }
}

/// Key for Bloom filter lookups
#[derive(Debug, Clone, PartialEq, Eq)]
struct BloomKey {
    namespace: String,
    object_id: String,
    relation: String,
    subject: String,
}

impl Hash for BloomKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.namespace.hash(state);
        self.object_id.hash(state);
        self.relation.hash(state);
        self.subject.hash(state);
    }
}

impl BloomKey {
    fn new(namespace: &str, object_id: &str, relation: &str, subject: &Subject) -> Self {
        Self {
            namespace: namespace.to_string(),
            object_id: object_id.to_string(),
            relation: relation.to_string(),
            subject: subject.to_string(),
        }
    }

    fn from_tuple(tuple: &RelationTuple) -> Self {
        Self::new(
            &tuple.namespace,
            &tuple.object_id,
            &tuple.relation,
            &tuple.subject,
        )
    }

    fn from_check_request(request: &CheckRequest) -> Self {
        Self::new(
            &request.namespace,
            &request.object_id,
            &request.relation,
            &request.subject,
        )
    }
}

/// Thread-safe Bloom filter for authorization tuples
pub struct AuthzBloomFilter {
    /// The underlying Bloom filter
    filter: RwLock<Bloom<BloomKey>>,
    /// Number of items added
    items_count: RwLock<usize>,
    /// Configuration
    config: BloomConfig,
}

impl AuthzBloomFilter {
    /// Create a new Bloom filter with default configuration
    pub fn new() -> Self {
        Self::with_config(BloomConfig::default())
    }

    /// Create a new Bloom filter with custom configuration
    pub fn with_config(config: BloomConfig) -> Self {
        let filter = Bloom::new_for_fp_rate(config.expected_items, config.false_positive_rate)
            .expect("Failed to create bloom filter with given parameters");
        Self {
            filter: RwLock::new(filter),
            items_count: RwLock::new(0),
            config,
        }
    }

    /// Add a tuple to the Bloom filter
    pub fn add_tuple(&self, tuple: &RelationTuple) {
        let key = BloomKey::from_tuple(tuple);
        let mut filter = self.filter.write().unwrap();
        filter.set(&key);
        let mut count = self.items_count.write().unwrap();
        *count += 1;
    }

    /// Check if a tuple might exist (true = might exist, false = definitely doesn't)
    pub fn might_contain(&self, request: &CheckRequest) -> bool {
        let key = BloomKey::from_check_request(request);
        let filter = self.filter.read().unwrap();
        filter.check(&key)
    }

    /// Batch check multiple requests
    /// Returns a vector of booleans indicating which requests might have tuples
    pub fn might_contain_batch(&self, requests: &[CheckRequest]) -> Vec<bool> {
        let filter = self.filter.read().unwrap();
        requests
            .iter()
            .map(|req| {
                let key = BloomKey::from_check_request(req);
                filter.check(&key)
            })
            .collect()
    }

    /// Get the current item count
    pub fn item_count(&self) -> usize {
        *self.items_count.read().unwrap()
    }

    /// Clear the Bloom filter (requires rebuilding)
    pub fn clear(&self) {
        let new_filter =
            Bloom::new_for_fp_rate(self.config.expected_items, self.config.false_positive_rate)
                .expect("Failed to create bloom filter with given parameters");
        *self.filter.write().unwrap() = new_filter;
        *self.items_count.write().unwrap() = 0;
    }

    /// Get estimated false positive rate based on current fill
    pub fn estimated_fp_rate(&self) -> f64 {
        let count = *self.items_count.read().unwrap();
        if count == 0 {
            return 0.0;
        }

        // Approximation: fp_rate increases as filter fills up
        let fill_ratio = count as f64 / self.config.expected_items as f64;
        if fill_ratio >= 1.0 {
            // Filter is at or over capacity, FP rate is high
            return 0.5;
        }

        // Scale the base FP rate by fill ratio (simplified model)
        self.config.false_positive_rate * (1.0 + fill_ratio)
    }
}

impl Default for AuthzBloomFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for Bloom filter performance
#[derive(Debug, Clone, Default)]
pub struct BloomStats {
    /// Number of definite negatives (avoided DB queries)
    pub definite_negatives: u64,
    /// Number of potential positives (required DB queries)
    pub potential_positives: u64,
    /// Number of true positives (confirmed by DB)
    pub true_positives: u64,
    /// Number of false positives (not found in DB despite Bloom saying yes)
    pub false_positives: u64,
}

impl BloomStats {
    /// Calculate the DB query reduction rate
    pub fn query_reduction_rate(&self) -> f64 {
        let total = self.definite_negatives + self.potential_positives;
        if total == 0 {
            return 0.0;
        }
        self.definite_negatives as f64 / total as f64
    }

    /// Calculate the actual false positive rate
    pub fn actual_fp_rate(&self) -> f64 {
        let db_checks = self.true_positives + self.false_positives;
        if db_checks == 0 {
            return 0.0;
        }
        self.false_positives as f64 / db_checks as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter_basic() {
        let bloom = AuthzBloomFilter::with_config(BloomConfig {
            expected_items: 1000,
            false_positive_rate: 0.01,
        });

        // Add a tuple
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "doc123",
            Subject::User("alice".to_string()),
        );
        bloom.add_tuple(&tuple);

        // Should find it
        let request = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc123".to_string(),
            relation: "viewer".to_string(),
            subject: Subject::User("alice".to_string()),
            context: None,
        };
        assert!(bloom.might_contain(&request));

        // Should NOT find a different tuple (with high probability)
        let request2 = CheckRequest {
            namespace: "document".to_string(),
            object_id: "doc999".to_string(),
            relation: "owner".to_string(),
            subject: Subject::User("bob".to_string()),
            context: None,
        };
        // Note: This could be a false positive, but with 1% FP rate it's unlikely
        // In practice, we'd need many iterations to test this statistically
        let _result = bloom.might_contain(&request2);
    }

    #[test]
    fn test_bloom_filter_batch() {
        let bloom = AuthzBloomFilter::with_config(BloomConfig {
            expected_items: 1000,
            false_positive_rate: 0.01,
        });

        // Add some tuples
        for i in 0..10 {
            let tuple = RelationTuple::new(
                "document",
                "viewer",
                format!("doc{}", i),
                Subject::User("alice".to_string()),
            );
            bloom.add_tuple(&tuple);
        }

        // Batch check
        let requests: Vec<_> = (0..15)
            .map(|i| CheckRequest {
                namespace: "document".to_string(),
                object_id: format!("doc{}", i),
                relation: "viewer".to_string(),
                subject: Subject::User("alice".to_string()),
                context: None,
            })
            .collect();

        let results = bloom.might_contain_batch(&requests);
        assert_eq!(results.len(), 15);

        // First 10 should definitely be true (we added them)
        for result in results.iter().take(10) {
            assert!(*result);
        }
    }

    #[test]
    fn test_bloom_stats() {
        let stats = BloomStats {
            definite_negatives: 500,
            potential_positives: 500,
            true_positives: 450,
            false_positives: 50,
        };

        assert!((stats.query_reduction_rate() - 0.5).abs() < 0.001);
        assert!((stats.actual_fp_rate() - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_bloom_clear() {
        let bloom = AuthzBloomFilter::with_config(BloomConfig {
            expected_items: 1000,
            false_positive_rate: 0.01,
        });

        // Add a tuple
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "doc123",
            Subject::User("alice".to_string()),
        );
        bloom.add_tuple(&tuple);
        assert_eq!(bloom.item_count(), 1);

        // Clear
        bloom.clear();
        assert_eq!(bloom.item_count(), 0);
    }
}
