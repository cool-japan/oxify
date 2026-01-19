//! JWT Public Key (JWKS) Caching
//!
//! Caches JWT public keys from JWKS endpoints to reduce latency and external requests.
//!
//! ## Overview
//!
//! When validating JWT tokens with RS256/RS512 signatures, the public key must be fetched
//! from a JWKS (JSON Web Key Set) endpoint. This cache reduces:
//! - Network latency (no HTTP request per validation)
//! - Load on authorization servers
//! - Risk of rate limiting
//!
//! ## JWKS Format
//!
//! JWKS endpoints return a set of public keys in JSON format:
//! ```json
//! {
//!   "keys": [
//!     {
//!       "kid": "2026-01-rsa",
//!       "kty": "RSA",
//!       "use": "sig",
//!       "n": "...",
//!       "e": "AQAB"
//!     }
//!   ]
//! }
//! ```
//!
//! ## Cache Strategy
//!
//! - **Long TTL**: Public keys rarely change (1-hour default)
//! - **Key ID Indexing**: Fast lookup by `kid` (Key ID)
//! - **Automatic Refresh**: Refresh keys before expiration
//! - **Fallback**: On cache miss, fetch from JWKS endpoint
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{JwksCache, JwksCacheConfig};
//!
//! let config = JwksCacheConfig {
//!     default_ttl: std::time::Duration::from_secs(3600), // 1 hour
//!     ..Default::default()
//! };
//! let cache = JwksCache::new(config);
//!
//! // Get public key for token validation
//! if let Some(public_key) = cache.get_key("auth0.com", "2026-01-rsa") {
//!     // Use cached key for validation
//!     validate_token(&token, &public_key)?;
//! } else {
//!     // Cache miss - fetch from JWKS endpoint
//!     let keys = fetch_jwks("https://auth0.com/.well-known/jwks.json").await?;
//!     cache.put_keys("auth0.com", keys);
//! }
//! ```

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration as StdDuration;

/// JSON Web Key (JWK) representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonWebKey {
    /// Key ID
    pub kid: String,
    /// Key type (RSA, EC, etc.)
    pub kty: String,
    /// Public key use (sig, enc)
    pub use_: Option<String>,
    /// Algorithm (RS256, RS512, etc.)
    pub alg: Option<String>,
    /// RSA modulus (base64url)
    pub n: Option<String>,
    /// RSA exponent (base64url)
    pub e: Option<String>,
    /// Elliptic curve (P-256, P-384, P-521)
    pub crv: Option<String>,
    /// EC X coordinate (base64url)
    pub x: Option<String>,
    /// EC Y coordinate (base64url)
    pub y: Option<String>,
}

/// JSON Web Key Set (JWKS)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonWebKeySet {
    pub keys: Vec<JsonWebKey>,
}

/// Cache entry for JWKS
#[derive(Debug, Clone)]
struct JwksCacheEntry {
    /// Indexed keys by kid
    keys: HashMap<String, JsonWebKey>,
    /// Cache expiration time
    expires_at: DateTime<Utc>,
    /// Access statistics
    access_count: u64,
    last_accessed: DateTime<Utc>,
}

impl JwksCacheEntry {
    fn new(keys: HashMap<String, JsonWebKey>, ttl: StdDuration) -> Self {
        let now = Utc::now();
        let ttl_duration = Duration::from_std(ttl).unwrap_or(Duration::seconds(3600));
        Self {
            keys,
            expires_at: now + ttl_duration,
            access_count: 0,
            last_accessed: now,
        }
    }

    fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    fn get_key(&mut self, kid: &str) -> Option<JsonWebKey> {
        self.access_count += 1;
        self.last_accessed = Utc::now();
        self.keys.get(kid).cloned()
    }
}

/// JWKS cache configuration
#[derive(Debug, Clone)]
pub struct JwksCacheConfig {
    /// Maximum number of cached JWKS endpoints
    pub max_issuers: usize,
    /// Default TTL for cached keys
    pub default_ttl: StdDuration,
    /// Enable cache metrics collection
    pub enable_metrics: bool,
}

impl Default for JwksCacheConfig {
    fn default() -> Self {
        Self {
            max_issuers: 100,
            default_ttl: StdDuration::from_secs(3600), // 1 hour
            enable_metrics: true,
        }
    }
}

/// Cache metrics for JWKS
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JwksCacheMetrics {
    pub key_hits: u64,
    pub key_misses: u64,
    pub issuer_hits: u64,
    pub issuer_misses: u64,
    pub refreshes: u64,
    pub evictions: u64,
}

impl JwksCacheMetrics {
    pub fn key_hit_rate(&self) -> f64 {
        let total = self.key_hits + self.key_misses;
        if total == 0 {
            return 0.0;
        }
        self.key_hits as f64 / total as f64
    }

    pub fn issuer_hit_rate(&self) -> f64 {
        let total = self.issuer_hits + self.issuer_misses;
        if total == 0 {
            return 0.0;
        }
        self.issuer_hits as f64 / total as f64
    }
}

/// JWKS public key cache
pub struct JwksCache {
    /// Cached JWKS per issuer
    entries: Arc<RwLock<HashMap<String, JwksCacheEntry>>>,
    config: JwksCacheConfig,
    metrics: Arc<RwLock<JwksCacheMetrics>>,
}

impl JwksCache {
    /// Create a new JWKS cache
    pub fn new(config: JwksCacheConfig) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::with_capacity(config.max_issuers))),
            config,
            metrics: Arc::new(RwLock::new(JwksCacheMetrics::default())),
        }
    }

    /// Get public key for a specific issuer and key ID
    pub fn get_key(&self, issuer: &str, kid: &str) -> Option<JsonWebKey> {
        let mut entries = self
            .entries
            .write()
            .expect("JWKS cache lock poisoned - fatal error in concurrent access");

        if let Some(entry) = entries.get_mut(issuer) {
            if entry.is_expired() {
                entries.remove(issuer);
                if self.config.enable_metrics {
                    let mut metrics = self
                        .metrics
                        .write()
                        .expect("JWKS metrics lock poisoned - fatal error in concurrent access");
                    metrics.issuer_misses += 1;
                    metrics.key_misses += 1;
                }
                return None;
            }

            if self.config.enable_metrics {
                let mut metrics = self
                    .metrics
                    .write()
                    .expect("JWKS metrics lock poisoned - fatal error in concurrent access");
                metrics.issuer_hits += 1;
            }

            if let Some(key) = entry.get_key(kid) {
                if self.config.enable_metrics {
                    let mut metrics = self
                        .metrics
                        .write()
                        .expect("JWKS metrics lock poisoned - fatal error in concurrent access");
                    metrics.key_hits += 1;
                }
                Some(key)
            } else {
                if self.config.enable_metrics {
                    let mut metrics = self
                        .metrics
                        .write()
                        .expect("JWKS metrics lock poisoned - fatal error in concurrent access");
                    metrics.key_misses += 1;
                }
                None
            }
        } else {
            if self.config.enable_metrics {
                let mut metrics = self
                    .metrics
                    .write()
                    .expect("JWKS metrics lock poisoned - fatal error in concurrent access");
                metrics.issuer_misses += 1;
                metrics.key_misses += 1;
            }
            None
        }
    }

    /// Put JWKS for an issuer into cache
    pub fn put_keys(&self, issuer: String, jwks: JsonWebKeySet) {
        let mut entries = self
            .entries
            .write()
            .expect("JWKS cache lock poisoned - fatal error in concurrent access");

        // Build kid index
        let mut key_map = HashMap::new();
        for key in jwks.keys {
            key_map.insert(key.kid.clone(), key);
        }

        // Evict expired entries first
        self.evict_expired(&mut entries);

        // If at capacity, evict LRU entry
        if entries.len() >= self.config.max_issuers {
            self.evict_lru(&mut entries);
        }

        entries.insert(
            issuer,
            JwksCacheEntry::new(key_map, self.config.default_ttl),
        );
    }

    /// Refresh JWKS for an issuer (update TTL)
    pub fn refresh_keys(&self, issuer: &str, jwks: JsonWebKeySet) {
        let mut entries = self
            .entries
            .write()
            .expect("JWKS cache lock poisoned - fatal error in concurrent access");

        // Build kid index
        let mut key_map = HashMap::new();
        for key in jwks.keys {
            key_map.insert(key.kid.clone(), key);
        }

        entries.insert(
            issuer.to_string(),
            JwksCacheEntry::new(key_map, self.config.default_ttl),
        );

        if self.config.enable_metrics {
            let mut metrics = self
                .metrics
                .write()
                .expect("JWKS metrics lock poisoned - fatal error in concurrent access");
            metrics.refreshes += 1;
        }
    }

    /// Invalidate JWKS for a specific issuer
    pub fn invalidate_issuer(&self, issuer: &str) {
        let mut entries = self
            .entries
            .write()
            .expect("JWKS cache lock poisoned - fatal error in concurrent access");
        entries.remove(issuer);
    }

    /// Clear all cached JWKS
    pub fn clear_all(&self) {
        let mut entries = self
            .entries
            .write()
            .expect("JWKS cache lock poisoned - fatal error in concurrent access");
        entries.clear();
    }

    /// Evict expired entries
    fn evict_expired(&self, entries: &mut HashMap<String, JwksCacheEntry>) {
        let now = Utc::now();
        entries.retain(|_, entry| entry.expires_at > now);
    }

    /// Evict least recently used entry
    fn evict_lru(&self, entries: &mut HashMap<String, JwksCacheEntry>) {
        if let Some(lru_issuer) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(issuer, _)| issuer.clone())
        {
            entries.remove(&lru_issuer);

            if self.config.enable_metrics {
                let mut metrics = self
                    .metrics
                    .write()
                    .expect("JWKS metrics lock poisoned - fatal error in concurrent access");
                metrics.evictions += 1;
            }
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> HashMap<String, f64> {
        let entries = self
            .entries
            .read()
            .expect("JWKS cache lock poisoned - fatal error in concurrent access");
        let metrics = self
            .metrics
            .read()
            .expect("JWKS metrics lock poisoned - fatal error in concurrent access");

        let mut stats = HashMap::new();
        stats.insert("issuers".to_string(), entries.len() as f64);
        stats.insert("max_issuers".to_string(), self.config.max_issuers as f64);
        stats.insert(
            "utilization".to_string(),
            entries.len() as f64 / self.config.max_issuers as f64,
        );
        stats.insert("key_hits".to_string(), metrics.key_hits as f64);
        stats.insert("key_misses".to_string(), metrics.key_misses as f64);
        stats.insert("key_hit_rate".to_string(), metrics.key_hit_rate());
        stats.insert("issuer_hits".to_string(), metrics.issuer_hits as f64);
        stats.insert("issuer_misses".to_string(), metrics.issuer_misses as f64);
        stats.insert("issuer_hit_rate".to_string(), metrics.issuer_hit_rate());
        stats.insert("refreshes".to_string(), metrics.refreshes as f64);
        stats.insert("evictions".to_string(), metrics.evictions as f64);

        stats
    }

    /// Get cache metrics
    pub fn metrics(&self) -> JwksCacheMetrics {
        self.metrics
            .read()
            .expect("JWKS metrics lock poisoned - fatal error in concurrent access")
            .clone()
    }

    /// Reset cache metrics
    pub fn reset_metrics(&self) {
        let mut metrics = self
            .metrics
            .write()
            .expect("JWKS metrics lock poisoned - fatal error in concurrent access");
        *metrics = JwksCacheMetrics::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_jwks() -> JsonWebKeySet {
        JsonWebKeySet {
            keys: vec![
                JsonWebKey {
                    kid: "2026-01-rsa".to_string(),
                    kty: "RSA".to_string(),
                    use_: Some("sig".to_string()),
                    alg: Some("RS256".to_string()),
                    n: Some("xGOr-H7A...".to_string()),
                    e: Some("AQAB".to_string()),
                    crv: None,
                    x: None,
                    y: None,
                },
                JsonWebKey {
                    kid: "2026-02-rsa".to_string(),
                    kty: "RSA".to_string(),
                    use_: Some("sig".to_string()),
                    alg: Some("RS512".to_string()),
                    n: Some("yHPs-I8B...".to_string()),
                    e: Some("AQAB".to_string()),
                    crv: None,
                    x: None,
                    y: None,
                },
            ],
        }
    }

    #[test]
    fn test_jwks_cache_basic_operations() {
        let config = JwksCacheConfig {
            max_issuers: 10,
            default_ttl: StdDuration::from_secs(3600),
            enable_metrics: true,
        };
        let cache = JwksCache::new(config);

        let issuer = "auth0.com";
        let jwks = create_test_jwks();

        // Put JWKS into cache
        cache.put_keys(issuer.to_string(), jwks);

        // Get key (hit)
        let key = cache.get_key(issuer, "2026-01-rsa");
        assert!(key.is_some());
        assert_eq!(key.unwrap().alg, Some("RS256".to_string()));

        // Get another key from same issuer
        let key2 = cache.get_key(issuer, "2026-02-rsa");
        assert!(key2.is_some());
        assert_eq!(key2.unwrap().alg, Some("RS512".to_string()));

        // Check metrics
        let metrics = cache.metrics();
        assert_eq!(metrics.key_hits, 2);
        assert_eq!(metrics.key_misses, 0);
        assert_eq!(metrics.issuer_hits, 2);
        assert_eq!(metrics.issuer_misses, 0);
    }

    #[test]
    fn test_jwks_cache_miss() {
        let config = JwksCacheConfig {
            max_issuers: 10,
            default_ttl: StdDuration::from_secs(3600),
            enable_metrics: true,
        };
        let cache = JwksCache::new(config);

        // Get from empty cache (miss)
        let key = cache.get_key("auth0.com", "2026-01-rsa");
        assert!(key.is_none());

        // Check metrics
        let metrics = cache.metrics();
        assert_eq!(metrics.key_hits, 0);
        assert_eq!(metrics.key_misses, 1);
        assert_eq!(metrics.issuer_hits, 0);
        assert_eq!(metrics.issuer_misses, 1);
    }

    #[test]
    fn test_jwks_cache_invalidation() {
        let config = JwksCacheConfig {
            max_issuers: 10,
            default_ttl: StdDuration::from_secs(3600),
            enable_metrics: true,
        };
        let cache = JwksCache::new(config);

        let issuer = "auth0.com";
        let jwks = create_test_jwks();

        // Put and get
        cache.put_keys(issuer.to_string(), jwks);
        assert!(cache.get_key(issuer, "2026-01-rsa").is_some());

        // Invalidate
        cache.invalidate_issuer(issuer);

        // Get after invalidation (miss)
        assert!(cache.get_key(issuer, "2026-01-rsa").is_none());
    }

    #[test]
    fn test_jwks_cache_refresh() {
        let config = JwksCacheConfig {
            max_issuers: 10,
            default_ttl: StdDuration::from_secs(3600),
            enable_metrics: true,
        };
        let cache = JwksCache::new(config);

        let issuer = "auth0.com";
        let jwks = create_test_jwks();

        // Initial put
        cache.put_keys(issuer.to_string(), jwks.clone());

        // Refresh
        cache.refresh_keys(issuer, jwks);

        // Check metrics
        let metrics = cache.metrics();
        assert_eq!(metrics.refreshes, 1);
    }

    #[test]
    fn test_jwks_cache_lru_eviction() {
        let config = JwksCacheConfig {
            max_issuers: 2,
            default_ttl: StdDuration::from_secs(3600),
            enable_metrics: true,
        };
        let cache = JwksCache::new(config);

        let jwks = create_test_jwks();

        // Fill cache to capacity
        cache.put_keys("issuer1.com".to_string(), jwks.clone());
        cache.put_keys("issuer2.com".to_string(), jwks.clone());

        // Access issuer1 to make it recently used
        cache.get_key("issuer1.com", "2026-01-rsa");

        // Add issuer3, should evict issuer2 (LRU)
        cache.put_keys("issuer3.com".to_string(), jwks.clone());

        // issuer1 should still be in cache
        assert!(cache.get_key("issuer1.com", "2026-01-rsa").is_some());

        // issuer3 should be in cache
        assert!(cache.get_key("issuer3.com", "2026-01-rsa").is_some());

        // issuer2 should be evicted
        assert!(cache.get_key("issuer2.com", "2026-01-rsa").is_none());
    }

    #[test]
    fn test_jwks_cache_hit_rates() {
        let metrics = JwksCacheMetrics {
            key_hits: 90,
            key_misses: 10,
            issuer_hits: 85,
            issuer_misses: 15,
            refreshes: 5,
            evictions: 2,
        };

        assert_eq!(metrics.key_hit_rate(), 0.9);
        assert_eq!(metrics.issuer_hit_rate(), 0.85);
    }
}
