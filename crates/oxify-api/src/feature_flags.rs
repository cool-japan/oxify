// Feature flags module for dynamic feature enablement
// Allows enabling/disabling features via environment variables

use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;

/// Feature flags configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    /// Enable GraphQL API
    pub graphql_enabled: bool,

    /// Enable OpenTelemetry tracing
    pub otel_enabled: bool,

    /// Enable WebSocket support
    pub websocket_enabled: bool,

    /// Enable batch operations
    pub batch_operations_enabled: bool,

    /// Enable workflow versioning
    pub versioning_enabled: bool,

    /// Enable secrets management
    pub secrets_enabled: bool,

    /// Enable checkpoint/pause/resume
    pub checkpoints_enabled: bool,

    /// Enable workflow snapshots and rollback
    pub rollback_enabled: bool,

    /// Enable vector database integration
    pub vector_db_enabled: bool,

    /// Enable webhook support
    pub webhooks_enabled: bool,

    /// Enable workflow analytics
    pub analytics_enabled: bool,

    /// Enable experimental features
    pub experimental_enabled: bool,

    /// Enable rate limiting
    pub rate_limiting_enabled: bool,

    /// Enable authentication
    pub auth_enabled: bool,

    /// Enable authorization (ReBAC)
    pub authz_enabled: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            graphql_enabled: true,
            otel_enabled: true,
            websocket_enabled: true,
            batch_operations_enabled: true,
            versioning_enabled: true,
            secrets_enabled: true,
            checkpoints_enabled: true,
            rollback_enabled: true,
            vector_db_enabled: true,
            webhooks_enabled: true,
            analytics_enabled: true,
            experimental_enabled: false,
            rate_limiting_enabled: true,
            auth_enabled: true,
            authz_enabled: true,
        }
    }
}

impl FeatureFlags {
    /// Load feature flags from environment variables
    pub fn from_env() -> Self {
        Self {
            graphql_enabled: env_bool("FEATURE_GRAPHQL", true),
            otel_enabled: env_bool("FEATURE_OTEL", true),
            websocket_enabled: env_bool("FEATURE_WEBSOCKET", true),
            batch_operations_enabled: env_bool("FEATURE_BATCH_OPS", true),
            versioning_enabled: env_bool("FEATURE_VERSIONING", true),
            secrets_enabled: env_bool("FEATURE_SECRETS", true),
            checkpoints_enabled: env_bool("FEATURE_CHECKPOINTS", true),
            rollback_enabled: env_bool("FEATURE_ROLLBACK", true),
            vector_db_enabled: env_bool("FEATURE_VECTOR_DB", true),
            webhooks_enabled: env_bool("FEATURE_WEBHOOKS", true),
            analytics_enabled: env_bool("FEATURE_ANALYTICS", true),
            experimental_enabled: env_bool("FEATURE_EXPERIMENTAL", false),
            rate_limiting_enabled: env_bool("FEATURE_RATE_LIMITING", true),
            auth_enabled: env_bool("FEATURE_AUTH", true),
            authz_enabled: env_bool("FEATURE_AUTHZ", true),
        }
    }

    /// Create with all features enabled (for testing)
    #[allow(dead_code)]
    pub fn all_enabled() -> Self {
        Self {
            graphql_enabled: true,
            otel_enabled: true,
            websocket_enabled: true,
            batch_operations_enabled: true,
            versioning_enabled: true,
            secrets_enabled: true,
            checkpoints_enabled: true,
            rollback_enabled: true,
            vector_db_enabled: true,
            webhooks_enabled: true,
            analytics_enabled: true,
            experimental_enabled: true,
            rate_limiting_enabled: true,
            auth_enabled: true,
            authz_enabled: true,
        }
    }

    /// Create with all features disabled (for testing)
    #[allow(dead_code)]
    pub fn all_disabled() -> Self {
        Self {
            graphql_enabled: false,
            otel_enabled: false,
            websocket_enabled: false,
            batch_operations_enabled: false,
            versioning_enabled: false,
            secrets_enabled: false,
            checkpoints_enabled: false,
            rollback_enabled: false,
            vector_db_enabled: false,
            webhooks_enabled: false,
            analytics_enabled: false,
            experimental_enabled: false,
            rate_limiting_enabled: false,
            auth_enabled: false,
            authz_enabled: false,
        }
    }

    /// Check if a feature is enabled
    pub fn is_enabled(&self, feature: &str) -> bool {
        match feature {
            "graphql" => self.graphql_enabled,
            "otel" => self.otel_enabled,
            "websocket" => self.websocket_enabled,
            "batch_operations" => self.batch_operations_enabled,
            "versioning" => self.versioning_enabled,
            "secrets" => self.secrets_enabled,
            "checkpoints" => self.checkpoints_enabled,
            "rollback" => self.rollback_enabled,
            "vector_db" => self.vector_db_enabled,
            "webhooks" => self.webhooks_enabled,
            "analytics" => self.analytics_enabled,
            "experimental" => self.experimental_enabled,
            "rate_limiting" => self.rate_limiting_enabled,
            "auth" => self.auth_enabled,
            "authz" => self.authz_enabled,
            _ => false,
        }
    }

    /// Get list of enabled features
    pub fn enabled_features(&self) -> Vec<String> {
        let mut features = Vec::new();

        if self.graphql_enabled {
            features.push("graphql".to_string());
        }
        if self.otel_enabled {
            features.push("otel".to_string());
        }
        if self.websocket_enabled {
            features.push("websocket".to_string());
        }
        if self.batch_operations_enabled {
            features.push("batch_operations".to_string());
        }
        if self.versioning_enabled {
            features.push("versioning".to_string());
        }
        if self.secrets_enabled {
            features.push("secrets".to_string());
        }
        if self.checkpoints_enabled {
            features.push("checkpoints".to_string());
        }
        if self.rollback_enabled {
            features.push("rollback".to_string());
        }
        if self.vector_db_enabled {
            features.push("vector_db".to_string());
        }
        if self.webhooks_enabled {
            features.push("webhooks".to_string());
        }
        if self.analytics_enabled {
            features.push("analytics".to_string());
        }
        if self.experimental_enabled {
            features.push("experimental".to_string());
        }
        if self.rate_limiting_enabled {
            features.push("rate_limiting".to_string());
        }
        if self.auth_enabled {
            features.push("auth".to_string());
        }
        if self.authz_enabled {
            features.push("authz".to_string());
        }

        features
    }
}

/// Helper to parse boolean from environment variable
fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .and_then(|v| match v.to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

/// Shareable feature flags
pub type SharedFeatureFlags = Arc<FeatureFlags>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_flags() {
        let flags = FeatureFlags::default();
        assert!(flags.graphql_enabled);
        assert!(flags.auth_enabled);
        assert!(!flags.experimental_enabled);
    }

    #[test]
    fn test_all_enabled() {
        let flags = FeatureFlags::all_enabled();
        assert!(flags.graphql_enabled);
        assert!(flags.experimental_enabled);
        assert!(flags.auth_enabled);
    }

    #[test]
    fn test_all_disabled() {
        let flags = FeatureFlags::all_disabled();
        assert!(!flags.graphql_enabled);
        assert!(!flags.experimental_enabled);
        assert!(!flags.auth_enabled);
    }

    #[test]
    fn test_is_enabled() {
        let flags = FeatureFlags::default();
        assert!(flags.is_enabled("graphql"));
        assert!(flags.is_enabled("auth"));
        assert!(!flags.is_enabled("experimental"));
        assert!(!flags.is_enabled("unknown_feature"));
    }

    #[test]
    fn test_enabled_features() {
        let flags = FeatureFlags::default();
        let enabled = flags.enabled_features();
        assert!(enabled.contains(&"graphql".to_string()));
        assert!(enabled.contains(&"auth".to_string()));
        assert!(!enabled.contains(&"experimental".to_string()));
    }

    #[test]
    fn test_env_bool() {
        assert!(env_bool("NONEXISTENT_VAR", true));
        assert!(!env_bool("NONEXISTENT_VAR", false));

        env::set_var("TEST_BOOL_TRUE", "true");
        assert!(env_bool("TEST_BOOL_TRUE", false));

        env::set_var("TEST_BOOL_FALSE", "false");
        assert!(!env_bool("TEST_BOOL_FALSE", true));

        env::set_var("TEST_BOOL_1", "1");
        assert!(env_bool("TEST_BOOL_1", false));

        env::set_var("TEST_BOOL_0", "0");
        assert!(!env_bool("TEST_BOOL_0", true));
    }
}
