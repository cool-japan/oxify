//! # OAuth2 Scopes → ReBAC Mapping
//!
//! Bridge OAuth2/OIDC authorization with ReBAC authorization engine.
//! Automatically create relation tuples from OAuth2 scopes and JWT claims.
//!
//! ## Features
//!
//! - **Scope Mapping**: Convert OAuth2 scopes to ReBAC relation tuples
//! - **JWT Claim Extraction**: Extract user identity and attributes from JWT tokens
//! - **Role Mapping**: Map OAuth2 roles to ReBAC relationships
//! - **Organization Mapping**: Map organizational claims to resource hierarchies
//!
//! ## Example
//!
//! ```rust
//! use oxify_authz::oauth2::{OAuth2Mapper, ScopeMapping, TokenClaims};
//! use oxify_authz::HybridRebacEngine;
//! use std::collections::HashMap;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = HybridRebacEngine::for_testing().await?;
//! let mapper = OAuth2Mapper::new();
//!
//! // Define scope mappings
//! let mut mappings = vec![
//!     ScopeMapping::new("read:documents", "document", "*", "viewer"),
//!     ScopeMapping::new("write:documents", "document", "*", "editor"),
//! ];
//!
//! // Parse JWT claims
//! let claims = TokenClaims {
//!     sub: "user:alice".to_string(),
//!     scope: Some("read:documents write:documents".to_string()),
//!     roles: Some(vec!["admin".to_string()]),
//!     groups: Some(vec!["engineering".to_string()]),
//!     ..Default::default()
//! };
//!
//! // Generate tuples from claims
//! let tuples = mapper.claims_to_tuples(&claims, &mappings)?;
//!
//! // Write tuples to engine
//! for tuple in tuples {
//!     engine.write_tuple(tuple).await?;
//! }
//! # Ok(())
//! # }
//! ```

use crate::{RelationTuple, Subject};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OAuth2/OIDC token claims
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenClaims {
    /// Subject (user ID)
    pub sub: String,

    /// Issuer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,

    /// Audience
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<Vec<String>>,

    /// Expiration time (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,

    /// Issued at time (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,

    /// Scopes (space-separated string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Roles (custom claim)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,

    /// Groups (custom claim)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,

    /// Organization ID (custom claim)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,

    /// Tenant ID (custom claim)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Custom claims
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Mapping from OAuth2 scope to ReBAC relation tuple
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeMapping {
    /// OAuth2 scope (e.g., "read:documents")
    pub scope: String,

    /// Target namespace in ReBAC
    pub namespace: String,

    /// Target object ID (supports wildcards and templates)
    /// - "*" for all objects in namespace
    /// - "{org_id}" for templated values from claims
    pub object_id: String,

    /// Relation to grant (e.g., "viewer", "editor")
    pub relation: String,
}

impl ScopeMapping {
    /// Create a new scope mapping
    pub fn new(
        scope: impl Into<String>,
        namespace: impl Into<String>,
        object_id: impl Into<String>,
        relation: impl Into<String>,
    ) -> Self {
        Self {
            scope: scope.into(),
            namespace: namespace.into(),
            object_id: object_id.into(),
            relation: relation.into(),
        }
    }

    /// Resolve object ID template with claims
    pub fn resolve_object_id(&self, claims: &TokenClaims) -> Vec<String> {
        if self.object_id == "*" {
            // Wildcard - would need to query all objects
            // For now, return empty to avoid granting universal access
            vec![]
        } else if self.object_id.contains('{') {
            // Template substitution
            let mut result = self.object_id.clone();

            // Replace {org_id}
            if let Some(ref org_id) = claims.org_id {
                result = result.replace("{org_id}", org_id);
            }

            // Replace {tenant_id}
            if let Some(ref tenant_id) = claims.tenant_id {
                result = result.replace("{tenant_id}", tenant_id);
            }

            // Replace {sub}
            result = result.replace("{sub}", &claims.sub);

            vec![result]
        } else {
            // Literal object ID
            vec![self.object_id.clone()]
        }
    }
}

/// Role to ReBAC mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleMapping {
    /// Role name (e.g., "admin", "manager")
    pub role: String,

    /// Resource namespace
    pub namespace: String,

    /// Object ID pattern
    pub object_id: String,

    /// Relation to grant
    pub relation: String,
}

/// OAuth2 to ReBAC mapper
pub struct OAuth2Mapper {
    /// Role mappings
    role_mappings: Vec<RoleMapping>,
}

impl OAuth2Mapper {
    /// Create a new OAuth2 mapper
    pub fn new() -> Self {
        Self {
            role_mappings: Vec::new(),
        }
    }

    /// Add a role mapping
    pub fn add_role_mapping(&mut self, mapping: RoleMapping) {
        self.role_mappings.push(mapping);
    }

    /// Convert token claims to ReBAC tuples based on scope mappings
    pub fn claims_to_tuples(
        &self,
        claims: &TokenClaims,
        scope_mappings: &[ScopeMapping],
    ) -> Result<Vec<RelationTuple>, String> {
        let mut tuples = Vec::new();

        // Parse scopes from claims
        let scopes: Vec<&str> = claims
            .scope
            .as_ref()
            .map(|s| s.split_whitespace().collect())
            .unwrap_or_default();

        // Extract subject from claims
        let subject = if claims.sub.starts_with("user:") {
            Subject::User(claims.sub.clone())
        } else {
            Subject::User(format!("user:{}", claims.sub))
        };

        // Map scopes to tuples
        for scope in scopes {
            for mapping in scope_mappings {
                if mapping.scope == scope {
                    let object_ids = mapping.resolve_object_id(claims);

                    for object_id in object_ids {
                        tuples.push(RelationTuple::new(
                            mapping.namespace.clone(),
                            mapping.relation.clone(),
                            object_id,
                            subject.clone(),
                        ));
                    }
                }
            }
        }

        // Map roles to tuples
        if let Some(ref roles) = claims.roles {
            for role in roles {
                for mapping in &self.role_mappings {
                    if &mapping.role == role {
                        let object_id = if mapping.object_id.contains('{') {
                            // Template resolution
                            let mut resolved = mapping.object_id.clone();
                            if let Some(ref org_id) = claims.org_id {
                                resolved = resolved.replace("{org_id}", org_id);
                            }
                            resolved
                        } else {
                            mapping.object_id.clone()
                        };

                        tuples.push(RelationTuple::new(
                            mapping.namespace.clone(),
                            mapping.relation.clone(),
                            object_id,
                            subject.clone(),
                        ));
                    }
                }
            }
        }

        // Map groups to tuples (groups as UserSets)
        if let Some(ref groups) = claims.groups {
            for group in groups {
                // Create membership tuples: user is member of group
                tuples.push(RelationTuple::new(
                    "group",
                    "member",
                    group.clone(),
                    subject.clone(),
                ));
            }
        }

        Ok(tuples)
    }

    /// Extract organization membership from claims
    pub fn extract_org_membership(&self, claims: &TokenClaims) -> Option<RelationTuple> {
        claims.org_id.as_ref().map(|org_id| {
            let subject = if claims.sub.starts_with("user:") {
                Subject::User(claims.sub.clone())
            } else {
                Subject::User(format!("user:{}", claims.sub))
            };

            RelationTuple::new("organization", "member", org_id.clone(), subject)
        })
    }

    /// Generate tuples from JWT claims (convenience method)
    pub fn jwt_to_tuples(
        &self,
        jwt_payload: &str,
        scope_mappings: &[ScopeMapping],
    ) -> Result<Vec<RelationTuple>, String> {
        let claims: TokenClaims = serde_json::from_str(jwt_payload)
            .map_err(|e| format!("Failed to parse JWT claims: {}", e))?;

        self.claims_to_tuples(&claims, scope_mappings)
    }
}

impl Default for OAuth2Mapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_mapping() {
        let mapping = ScopeMapping::new("read:documents", "document", "doc123", "viewer");

        assert_eq!(mapping.scope, "read:documents");
        assert_eq!(mapping.namespace, "document");
        assert_eq!(mapping.object_id, "doc123");
        assert_eq!(mapping.relation, "viewer");
    }

    #[test]
    fn test_template_resolution() {
        let mapping = ScopeMapping::new("read:org_documents", "document", "org_{org_id}", "viewer");

        let claims = TokenClaims {
            sub: "alice".to_string(),
            org_id: Some("acme".to_string()),
            ..Default::default()
        };

        let resolved = mapping.resolve_object_id(&claims);
        assert_eq!(resolved, vec!["org_acme"]);
    }

    #[test]
    fn test_claims_to_tuples() {
        let mapper = OAuth2Mapper::new();

        let claims = TokenClaims {
            sub: "alice".to_string(),
            scope: Some("read:documents write:documents".to_string()),
            ..Default::default()
        };

        let mappings = vec![
            ScopeMapping::new("read:documents", "document", "doc123", "viewer"),
            ScopeMapping::new("write:documents", "document", "doc123", "editor"),
        ];

        let tuples = mapper.claims_to_tuples(&claims, &mappings).unwrap();

        assert_eq!(tuples.len(), 2);
        assert_eq!(tuples[0].namespace, "document");
        assert_eq!(tuples[0].relation, "viewer");
        assert_eq!(tuples[1].relation, "editor");
    }

    #[test]
    fn test_role_mapping() {
        let mut mapper = OAuth2Mapper::new();

        mapper.add_role_mapping(RoleMapping {
            role: "admin".to_string(),
            namespace: "organization".to_string(),
            object_id: "{org_id}".to_string(),
            relation: "owner".to_string(),
        });

        let claims = TokenClaims {
            sub: "alice".to_string(),
            roles: Some(vec!["admin".to_string()]),
            org_id: Some("acme".to_string()),
            ..Default::default()
        };

        let tuples = mapper.claims_to_tuples(&claims, &[]).unwrap();

        assert_eq!(tuples.len(), 1);
        assert_eq!(tuples[0].namespace, "organization");
        assert_eq!(tuples[0].object_id, "acme");
        assert_eq!(tuples[0].relation, "owner");
    }

    #[test]
    fn test_group_mapping() {
        let mapper = OAuth2Mapper::new();

        let claims = TokenClaims {
            sub: "alice".to_string(),
            groups: Some(vec!["engineering".to_string(), "admins".to_string()]),
            ..Default::default()
        };

        let tuples = mapper.claims_to_tuples(&claims, &[]).unwrap();

        assert_eq!(tuples.len(), 2);
        assert_eq!(tuples[0].namespace, "group");
        assert_eq!(tuples[0].relation, "member");
        assert_eq!(tuples[0].object_id, "engineering");
    }

    #[test]
    fn test_org_membership() {
        let mapper = OAuth2Mapper::new();

        let claims = TokenClaims {
            sub: "alice".to_string(),
            org_id: Some("acme".to_string()),
            ..Default::default()
        };

        let tuple = mapper.extract_org_membership(&claims).unwrap();

        assert_eq!(tuple.namespace, "organization");
        assert_eq!(tuple.relation, "member");
        assert_eq!(tuple.object_id, "acme");
    }
}
