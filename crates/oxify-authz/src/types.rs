//! Core types for the authorization system
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY
//! License: MIT OR Apache-2.0 (compatible with OxiRS)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

/// Request context for conditional permission evaluation
#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    /// Client IP address
    pub client_ip: Option<IpAddr>,

    /// Custom attributes (e.g., user role, device type, location)
    pub attributes: HashMap<String, String>,

    /// Timestamp of the request (defaults to now)
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl RequestContext {
    /// Create a new request context with current timestamp
    pub fn new() -> Self {
        Self {
            client_ip: None,
            attributes: HashMap::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Set the client IP address
    pub fn with_client_ip(mut self, ip: IpAddr) -> Self {
        self.client_ip = Some(ip);
        self
    }

    /// Add a custom attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Get an attribute value
    pub fn get_attribute(&self, key: &str) -> Option<&String> {
        self.attributes.get(key)
    }
}

/// Conditions that can be attached to relationships
/// Ported from OxiRS rebac.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelationshipCondition {
    /// Time-based condition
    TimeWindow {
        not_before: Option<chrono::DateTime<chrono::Utc>>,
        not_after: Option<chrono::DateTime<chrono::Utc>>,
    },

    /// IP address condition (CIDR notation supported)
    IpAddress { allowed_ips: Vec<String> },

    /// Custom attribute-based condition
    Attribute { key: String, value: String },

    /// Combined condition (ALL must be satisfied)
    All {
        conditions: Vec<RelationshipCondition>,
    },

    /// Combined condition (ANY can be satisfied)
    Any {
        conditions: Vec<RelationshipCondition>,
    },
}

impl RelationshipCondition {
    /// Check if this condition is satisfied with a request context
    pub fn is_satisfied_with_context(&self, context: &RequestContext) -> bool {
        match self {
            RelationshipCondition::TimeWindow {
                not_before,
                not_after,
            } => {
                let now = context.timestamp;
                let after_start = not_before.is_none_or(|start| now >= start);
                let before_end = not_after.is_none_or(|end| now <= end);
                after_start && before_end
            }
            RelationshipCondition::IpAddress { allowed_ips } => {
                if let Some(client_ip) = context.client_ip {
                    // Check if client IP matches any allowed IP
                    // For simplicity, we do exact string matching
                    // In production, you'd want CIDR matching
                    let client_ip_str = client_ip.to_string();
                    allowed_ips.iter().any(|ip| {
                        // Support exact match or CIDR (basic)
                        if ip.contains('/') {
                            // CIDR notation - for now, just check prefix
                            // Full CIDR matching would require ipnetwork crate
                            client_ip_str.starts_with(ip.split('/').next().unwrap_or(""))
                        } else {
                            // Exact match
                            &client_ip_str == ip
                        }
                    })
                } else {
                    // No client IP provided - deny by default for security
                    false
                }
            }
            RelationshipCondition::Attribute { key, value } => {
                // Check if the request context has the required attribute with the expected value
                context.get_attribute(key) == Some(value)
            }
            RelationshipCondition::All { conditions } => {
                // All conditions must be satisfied
                conditions
                    .iter()
                    .all(|c| c.is_satisfied_with_context(context))
            }
            RelationshipCondition::Any { conditions } => {
                // At least one condition must be satisfied
                conditions
                    .iter()
                    .any(|c| c.is_satisfied_with_context(context))
            }
        }
    }

    /// Check if this condition is satisfied (without context)
    /// Deprecated: Use is_satisfied_with_context instead
    pub fn is_satisfied(&self) -> bool {
        // Create a default context and check
        let context = RequestContext::new();
        self.is_satisfied_with_context(&context)
    }
}

/// Namespace configuration defining relations and their permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    pub name: String,
    pub relations: Vec<RelationConfig>,
}

/// Relation configuration with computed/inherited permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationConfig {
    pub name: String,

    /// Relations that inherit this relation
    /// Example: "viewer" inherits from "owner" (owners can also view)
    pub inherits_from: Vec<String>,

    /// Union of relations (subject has ANY of these relations)
    pub union: Vec<String>,

    /// Intersection of relations (subject has ALL of these relations)
    pub intersection: Vec<String>,
}

impl NamespaceConfig {
    /// Create a document namespace with standard permissions
    pub fn document_namespace() -> Self {
        NamespaceConfig {
            name: "document".to_string(),
            relations: vec![
                RelationConfig {
                    name: "owner".to_string(),
                    inherits_from: vec![],
                    union: vec![],
                    intersection: vec![],
                },
                RelationConfig {
                    name: "editor".to_string(),
                    inherits_from: vec!["owner".to_string()],
                    union: vec![],
                    intersection: vec![],
                },
                RelationConfig {
                    name: "viewer".to_string(),
                    inherits_from: vec!["owner".to_string(), "editor".to_string()],
                    union: vec![],
                    intersection: vec![],
                },
            ],
        }
    }

    /// Create a folder namespace with hierarchical permissions
    pub fn folder_namespace() -> Self {
        NamespaceConfig {
            name: "folder".to_string(),
            relations: vec![
                RelationConfig {
                    name: "parent".to_string(),
                    inherits_from: vec![],
                    union: vec![],
                    intersection: vec![],
                },
                RelationConfig {
                    name: "owner".to_string(),
                    inherits_from: vec![],
                    union: vec![],
                    intersection: vec![],
                },
                RelationConfig {
                    name: "viewer".to_string(),
                    inherits_from: vec!["owner".to_string()],
                    union: vec![],
                    intersection: vec![],
                },
            ],
        }
    }
}

/// Authorization decision with audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthzDecision {
    pub allowed: bool,
    pub reason: String,
    pub depth: usize, // How many hops in the relation graph
    pub cached: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_configs() {
        let doc_ns = NamespaceConfig::document_namespace();
        assert_eq!(doc_ns.name, "document");
        assert_eq!(doc_ns.relations.len(), 3);

        let viewer_relation = doc_ns
            .relations
            .iter()
            .find(|r| r.name == "viewer")
            .unwrap();
        assert_eq!(viewer_relation.inherits_from.len(), 2);
    }
}
