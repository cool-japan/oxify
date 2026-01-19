# Migration Guide: From RBAC to ReBAC

This guide helps you migrate from traditional Role-Based Access Control (RBAC) to Relationship-Based Access Control (ReBAC) using oxify-authz.

## Table of Contents

- [Why Migrate?](#why-migrate)
- [Understanding the Differences](#understanding-the-differences)
- [Migration Strategy](#migration-strategy)
- [Step-by-Step Migration](#step-by-step-migration)
- [Common Patterns](#common-patterns)
- [Troubleshooting](#troubleshooting)

---

## Why Migrate?

**RBAC Limitations:**
- ❌ Cannot model complex hierarchical relationships
- ❌ Difficult to implement resource-specific permissions
- ❌ Requires application logic for inheritance
- ❌ No native support for delegation or groups

**ReBAC Advantages:**
- ✅ Native support for hierarchical permissions (folders → documents)
- ✅ Resource-level access control out of the box
- ✅ Flexible relationship modeling (teams, organizations, sharing)
- ✅ Transitive permission resolution (Alice → Team → Document)
- ✅ No application code needed for complex authorization logic

---

## Understanding the Differences

### RBAC Model

```
User → Role → Permission
```

Example:
```rust
// RBAC: Check if user has role
if user.has_role("admin") {
    // Allow access
}
```

**Problems:**
- How do you model "Alice can edit Document 123"?
- How do you model "Members of Team A can view Folder X"?
- Answer: You add complex application logic

### ReBAC Model

```
Subject → Relation → Object
```

Example:
```rust
// ReBAC: Check if user has relation to resource
engine.check(CheckRequest {
    namespace: "document",
    object_id: "123",
    relation: "editor",
    subject: Subject::User("alice".to_string()),
    context: None,
}).await?
```

**Benefits:**
- Direct modeling of resource permissions
- Built-in support for hierarchical relationships
- Automatic transitive resolution

---

## Migration Strategy

### 1. Parallel Run (Recommended)

Run RBAC and ReBAC side-by-side:

```rust
// Phase 1: Write to both systems
async fn grant_permission(user: &str, resource: &str, permission: &str) {
    // Old RBAC system
    rbac.grant_role(user, permission).await?;

    // New ReBAC system
    rebac.write_tuple(RelationTuple::new(
        "document",
        permission,
        resource,
        Subject::User(user.to_string()),
    )).await?;
}

// Phase 2: Read from ReBAC, fallback to RBAC
async fn check_permission(user: &str, resource: &str, permission: &str) -> bool {
    // Try ReBAC first
    if let Ok(allowed) = rebac.check(...).await {
        return allowed;
    }

    // Fallback to RBAC during migration
    rbac.has_permission(user, permission).await.unwrap_or(false)
}

// Phase 3: Read only from ReBAC
async fn check_permission(user: &str, resource: &str, permission: &str) -> bool {
    rebac.check(...).await.unwrap_or(false)
}
```

### 2. Big Bang (Not Recommended)

Switch all at once. Only use for small systems.

### 3. Feature Flag

Use feature flags to gradually roll out ReBAC:

```rust
if feature_flags.is_enabled("rebac_authz") {
    // Use ReBAC
    rebac.check(...).await?
} else {
    // Use RBAC
    rbac.has_permission(...)
}
```

---

## Step-by-Step Migration

### Step 1: Map RBAC Roles to ReBAC Relations

**RBAC Roles:**
```
- admin
- editor
- viewer
```

**ReBAC Namespace Configuration:**
```rust
use oxify_authz::types::NamespaceConfig;

let config = NamespaceConfig {
    namespace: "document".to_string(),
    relations: vec![
        // Direct relations
        "owner".to_string(),
        "editor".to_string(),
        "viewer".to_string(),
    ],
    inheritance: vec![
        // owner inherits editor permissions
        ("owner".to_string(), "editor".to_string()),
        // editor inherits viewer permissions
        ("editor".to_string(), "viewer".to_string()),
    ],
};
```

### Step 2: Migrate Existing Permissions

Create a migration script:

```rust
use oxify_authz::{HybridRebacEngine, RelationTuple, Subject};

async fn migrate_rbac_to_rebac() -> Result<()> {
    let engine = HybridRebacEngine::new("postgres://...").await?;

    // Fetch all RBAC assignments
    let rbac_assignments = fetch_rbac_assignments().await?;

    for assignment in rbac_assignments {
        // Map RBAC role to ReBAC relation
        let relation = match assignment.role.as_str() {
            "admin" => "owner",
            "editor" => "editor",
            "viewer" => "viewer",
            _ => continue,
        };

        // Create ReBAC tuple
        let tuple = RelationTuple::new(
            "document",
            relation,
            &assignment.resource_id,
            Subject::User(assignment.user_id.clone()),
        );

        engine.write_tuple(tuple).await?;
    }

    Ok(())
}
```

### Step 3: Update Authorization Checks

**Before (RBAC):**
```rust
async fn can_edit_document(user_id: &str, doc_id: &str) -> bool {
    let user = get_user(user_id).await;
    user.has_role("admin") || user.has_role("editor")
}
```

**After (ReBAC):**
```rust
async fn can_edit_document(user_id: &str, doc_id: &str) -> bool {
    engine.check(&CheckRequest {
        namespace: "document".to_string(),
        object_id: doc_id.to_string(),
        relation: "editor".to_string(),
        subject: Subject::User(user_id.to_string()),
        context: None,
    }).await
    .map(|res| res.allowed)
    .unwrap_or(false)
}
```

### Step 4: Migrate Complex Scenarios

#### Scenario 1: Team-Based Access

**RBAC (Application Logic):**
```rust
async fn can_view_document(user_id: &str, doc_id: &str) -> bool {
    // Complex application logic
    let teams = get_user_teams(user_id).await;
    for team in teams {
        if document_shared_with_team(doc_id, &team).await {
            return true;
        }
    }
    false
}
```

**ReBAC (Native Support):**
```rust
// 1. Create team membership tuple
engine.write_tuple(RelationTuple::new(
    "team",
    "member",
    "engineering",
    Subject::User("alice".to_string()),
)).await?;

// 2. Grant team access to document
engine.write_tuple(RelationTuple::new(
    "document",
    "viewer",
    "doc123",
    Subject::UserSet {
        namespace: "team".to_string(),
        object_id: "engineering".to_string(),
        relation: "member".to_string(),
    },
)).await?;

// 3. Check permission (automatic transitive resolution)
let allowed = engine.check(&CheckRequest {
    namespace: "document".to_string(),
    object_id: "doc123".to_string(),
    relation: "viewer".to_string(),
    subject: Subject::User("alice".to_string()),
    context: None,
}).await?.allowed; // true
```

#### Scenario 2: Hierarchical Resources

**RBAC (Application Logic):**
```rust
async fn can_access_file(user_id: &str, file_id: &str) -> bool {
    // Check file permission
    if has_permission(user_id, file_id, "read").await {
        return true;
    }

    // Check parent folder permission (manual hierarchy)
    let folder_id = get_parent_folder(file_id).await;
    if has_permission(user_id, &folder_id, "read").await {
        return true;
    }

    // Check grandparent folder, etc...
    false
}
```

**ReBAC (Native Hierarchy):**
```rust
// 1. Define parent relationship
engine.write_tuple(RelationTuple::new(
    "file",
    "parent",
    "file123",
    Subject::UserSet {
        namespace: "folder".to_string(),
        object_id: "folder_projects".to_string(),
        relation: "viewer".to_string(),
    },
)).await?;

// 2. Grant folder access
engine.write_tuple(RelationTuple::new(
    "folder",
    "viewer",
    "folder_projects",
    Subject::User("alice".to_string()),
)).await?;

// 3. Check file permission (automatic parent resolution)
let allowed = engine.check(&CheckRequest {
    namespace: "file".to_string(),
    object_id: "file123".to_string(),
    relation: "viewer".to_string(),
    subject: Subject::User("alice".to_string()),
    context: None,
}).await?.allowed; // true (inherited from parent folder)
```

### Step 5: Test Migration

Create comprehensive tests:

```rust
#[tokio::test]
async fn test_migration_parity() {
    let rbac = setup_rbac().await;
    let rebac = setup_rebac().await;

    // Migrate data
    migrate_rbac_to_rebac().await.unwrap();

    // Test all users and resources
    for user in get_all_users().await {
        for resource in get_all_resources().await {
            for permission in ["read", "write", "delete"] {
                let rbac_result = rbac.check(&user, &resource, permission).await;
                let rebac_result = rebac.check(&user, &resource, permission).await;

                assert_eq!(
                    rbac_result, rebac_result,
                    "Mismatch for user={}, resource={}, permission={}",
                    user, resource, permission
                );
            }
        }
    }
}
```

### Step 6: Monitor and Validate

Track both systems during migration:

```rust
async fn check_permission_with_monitoring(
    user: &str,
    resource: &str,
    permission: &str,
) -> bool {
    let rbac_result = rbac.check(user, resource, permission).await;
    let rebac_result = rebac.check(user, resource, permission).await;

    if rbac_result != rebac_result {
        log::error!(
            "Authorization mismatch: user={}, resource={}, permission={}, \
             rbac={}, rebac={}",
            user, resource, permission, rbac_result, rebac_result
        );

        // Send alert
        metrics.increment("authz.mismatch");
    }

    // During migration, use RBAC result
    rbac_result
}
```

---

## Common Patterns

### Pattern 1: Global Roles → Resource-Specific Permissions

**Before:**
```rust
// Global admin role
user.roles = ["admin"]
```

**After:**
```rust
// Organization admin
engine.write_tuple(RelationTuple::new(
    "organization",
    "admin",
    "org_acme",
    Subject::User("alice".to_string()),
)).await?;
```

### Pattern 2: Group Permissions → UserSets

**Before:**
```rust
// Group has permission
groups["engineering"].permissions = ["read_code"]
users["alice"].groups = ["engineering"]
```

**After:**
```rust
// Alice is member of engineering
engine.write_tuple(RelationTuple::new(
    "group",
    "member",
    "engineering",
    Subject::User("alice".to_string()),
)).await?;

// Engineering group can read code
engine.write_tuple(RelationTuple::new(
    "repository",
    "reader",
    "main_repo",
    Subject::UserSet {
        namespace: "group".to_string(),
        object_id: "engineering".to_string(),
        relation: "member".to_string(),
    },
)).await?;
```

### Pattern 3: Wildcard Permissions → Explicit Tuples

**Before:**
```rust
// User can access all documents
user.permissions = ["documents:*:read"]
```

**After:**
```rust
// Grant at organization level, inherit to all documents
engine.write_tuple(RelationTuple::new(
    "organization",
    "member",
    "acme",
    Subject::User("alice".to_string()),
)).await?;

// Documents inherit organization permissions
engine.write_tuple(RelationTuple::new(
    "document",
    "viewer",
    "doc123",
    Subject::UserSet {
        namespace: "organization".to_string(),
        object_id: "acme".to_string(),
        relation: "member".to_string(),
    },
)).await?;
```

---

## Troubleshooting

### Issue: Slow Migration

**Problem:** Migrating millions of RBAC assignments is slow.

**Solution:** Use batch operations:

```rust
let mut batch = Vec::new();
for assignment in rbac_assignments {
    batch.push(create_tuple(assignment));

    if batch.len() >= 1000 {
        engine.batch_write(&batch).await?;
        batch.clear();
    }
}
```

### Issue: Permission Mismatches

**Problem:** ReBAC returns different results than RBAC.

**Solution:** Enable audit logging and compare:

```rust
let rbac_result = rbac.check(...).await;
let rebac_result = rebac.check(...).await;

if rbac_result != rebac_result {
    audit_log::record_mismatch(...);

    // Use expand to debug
    let subjects = engine.expand(...).await?;
    log::debug!("ReBAC subjects with permission: {:?}", subjects);
}
```

### Issue: Missing Hierarchical Permissions

**Problem:** User can't access child resources.

**Solution:** Check namespace configuration has correct inheritance:

```rust
// Verify parent relation exists
let config = engine.get_namespace_config("document").await?;
assert!(config.inheritance.contains(&("parent".to_string(), "viewer".to_string())));
```

### Issue: Performance Degradation

**Problem:** ReBAC checks are slower than RBAC.

**Solution:** Ensure Leopard index is loaded and Redis cache is enabled:

```rust
// Warm up Leopard index
engine.leopard_index().warm_up().await?;

// Enable Redis L2 cache
let redis = RedisCache::new("redis://localhost").await?;
engine.set_redis_cache(redis).await?;
```

---

## Best Practices

1. **Start Small:** Migrate one namespace at a time
2. **Test Thoroughly:** Validate all permission scenarios
3. **Monitor Closely:** Track mismatches during parallel run
4. **Document Mappings:** Keep clear mapping from RBAC roles to ReBAC relations
5. **Use Feature Flags:** Enable gradual rollout
6. **Maintain Audit Trail:** Log all migration actions
7. **Plan Rollback:** Keep RBAC system running until confident

---

## Next Steps

After successful migration:

1. Remove RBAC system and dependencies
2. Update documentation
3. Train team on ReBAC concepts
4. Optimize ReBAC configuration
5. Implement advanced ReBAC features (delegation, conditional permissions)

---

## Additional Resources

- [ReBAC vs RBAC Comparison](https://research.google/pubs/pub48190/) (Google Zanzibar Paper)
- [Authorization Best Practices](../README.md)
- [API Documentation](https://docs.rs/oxify-authz)
- [Example Applications](../examples/)

---

**Need Help?** Open an issue at https://github.com/oxify/oxify-authz/issues
