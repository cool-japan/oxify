# OxiFY AuthZ - Best Practices & Security Patterns

This guide provides production-ready patterns, security recommendations, and anti-patterns to avoid when implementing relationship-based access control (ReBAC) with oxify-authz.

---

## Table of Contents

1. [Security Patterns](#security-patterns)
2. [Anti-Patterns to Avoid](#anti-patterns-to-avoid)
3. [Performance Optimization](#performance-optimization)
4. [Multi-Tenancy Best Practices](#multi-tenancy-best-practices)
5. [Monitoring & Observability](#monitoring--observability)
6. [Testing Strategies](#testing-strategies)
7. [Incident Response](#incident-response)

---

## Security Patterns

### 1. Principle of Least Privilege

**✅ DO:**
```rust
// Grant minimal permissions needed
engine.write_tuple(RelationTuple::new(
    "document", "viewer", "123",
    Subject::User("alice".to_string()),
)).await?;

// Use hierarchical permissions for automatic inheritance
engine.write_tuple(RelationTuple::new(
    "folder", "viewer", "reports",
    Subject::User("alice".to_string()),
)).await?;
```

**❌ DON'T:**
```rust
// Avoid granting admin rights when only read access is needed
engine.write_tuple(RelationTuple::new(
    "document", "admin", "123",  // Too permissive!
    Subject::User("alice".to_string()),
)).await?;
```

### 2. Time-Limited Access

**✅ DO:**
```rust
use chrono::{Utc, Duration};
use oxify_authz::{RelationTuple, RelationshipCondition};

// Grant temporary access that expires
let expires_at = Utc::now() + Duration::hours(24);
let tuple = RelationTuple::with_condition(
    "document", "123", "viewer",
    Subject::User("contractor".to_string()),
    RelationshipCondition::TimeWindow {
        not_before: None,
        not_after: Some(expires_at),
    },
);
engine.write_tuple(tuple).await?;
```

**❌ DON'T:**
```rust
// Avoid permanent access for temporary needs
engine.write_tuple(RelationTuple::new(
    "document", "viewer", "123",
    Subject::User("contractor".to_string()),  // No expiration!
)).await?;
```

### 3. Audit Trail Requirements

**✅ DO:**
```rust
use oxify_authz::{AuditLogger, AuditConfig};

// Enable comprehensive audit logging
let audit_config = AuditConfig {
    sample_rate: 1.0,  // Log 100% of checks in sensitive systems
    log_denied: true,  // Always log denials
    log_mutations: true,  // Track all permission changes
    ..Default::default()
};

let audit_logger = AuditLogger::new(pool.clone(), audit_config);
engine.set_audit_logger(Some(audit_logger));

// Query audit trail for compliance
let report = audit_logger.compliance_report(
    start_time,
    end_time,
    Some("user:admin"),
).await?;
```

**❌ DON'T:**
```rust
// Don't disable audit logging in production
let config = AuditConfig {
    sample_rate: 0.0,  // No logging! Security risk!
    ..Default::default()
};
```

### 4. Defense in Depth

**✅ DO:**
```rust
// Layer 1: API authentication (JWT, API key)
// Layer 2: ReBAC authorization check
let allowed = engine.check(CheckRequest {
    namespace: "document".to_string(),
    object_id: doc_id,
    relation: "edit".to_string(),
    subject: Subject::User(user_id),
    context: Some(RequestContext::new()
        .with_client_ip(client_ip)
        .with_attribute("device_type", "laptop")),
}).await?;

// Layer 3: Application-level validation
if allowed.allowed {
    // Layer 4: Database-level row security (if available)
    update_document(doc_id, changes).await?;
}
```

### 5. Conditional Access Policies

**✅ DO:**
```rust
use std::net::IpAddr;

// Restrict access by IP range (e.g., corporate network)
let tuple = RelationTuple::with_condition(
    "admin_panel", "root", "access",
    Subject::User("admin".to_string()),
    RelationshipCondition::IpAddress {
        allowed_ips: vec![
            "10.0.0.0/8".to_string(),      // Internal network
            "192.168.1.0/24".to_string(),  // VPN
        ],
    },
);

// Combine multiple conditions
let tuple = RelationTuple::with_condition(
    "financial_data", "quarterly_reports", "view",
    Subject::User("auditor".to_string()),
    RelationshipCondition::All {
        conditions: vec![
            RelationshipCondition::TimeWindow {
                not_before: Some(audit_start),
                not_after: Some(audit_end),
            },
            RelationshipCondition::Attribute {
                key: "mfa_verified".to_string(),
                value: "true".to_string(),
            },
        ],
    },
);
```

---

## Anti-Patterns to Avoid

### 1. ❌ Over-Granting Permissions

**Problem:**
```rust
// Granting admin access instead of specific permissions
for user in all_users {
    engine.write_tuple(RelationTuple::new(
        "organization", "admin", "acme-corp",
        Subject::User(user),
    )).await?;
}
```

**Solution:**
```rust
// Use specific roles and groups
for user in managers {
    engine.write_tuple(RelationTuple::new(
        "team", "manager", "engineering",
        Subject::User(user),
    )).await?;
}

for user in engineers {
    engine.write_tuple(RelationTuple::new(
        "team", "member", "engineering",
        Subject::User(user),
    )).await?;
}
```

### 2. ❌ Bypassing Authorization Checks

**Problem:**
```rust
// Directly querying database without authorization
let document = db.query_document(doc_id).await?;
```

**Solution:**
```rust
// Always check permissions first
let allowed = engine.check(CheckRequest::new(
    "document", doc_id, "view",
    Subject::User(user_id),
)).await?;

if !allowed.allowed {
    return Err(AuthzError::PermissionDenied(
        format!("User {} cannot view document {}", user_id, doc_id)
    ));
}

let document = db.query_document(doc_id).await?;
```

### 3. ❌ Storing Sensitive Data in Tuple IDs

**Problem:**
```rust
// Exposing sensitive data in object IDs
engine.write_tuple(RelationTuple::new(
    "medical_record",
    "viewer",
    "patient-john-doe-ssn-123-45-6789",  // PII in ID!
    Subject::User("doctor".to_string()),
)).await?;
```

**Solution:**
```rust
// Use opaque identifiers
engine.write_tuple(RelationTuple::new(
    "medical_record",
    "viewer",
    "mrn-uuid-a1b2c3d4",  // Anonymous ID
    Subject::User("doctor".to_string()),
)).await?;
```

### 4. ❌ Ignoring Denial Events

**Problem:**
```rust
// Silently failing authorization checks
let allowed = engine.check(request).await?;
if !allowed.allowed {
    // No logging, no alert - attacker can probe freely
    return Ok(());
}
```

**Solution:**
```rust
let allowed = engine.check(request).await?;
if !allowed.allowed {
    // Log denial for security monitoring
    warn!(
        "Authorization denied: user={} resource={} action={}",
        request.subject, request.object_id, request.relation
    );

    // Check for anomalies
    if let Some(anomaly) = anomaly_detector.check_anomaly(&event) {
        alert_security_team(anomaly);
    }

    return Err(AuthzError::PermissionDenied("Access denied".to_string()));
}
```

### 5. ❌ Hardcoding Permission Logic

**Problem:**
```rust
// Hardcoded business logic in application
if user.role == "admin" || user.id == resource.owner_id {
    allow_access();
}
```

**Solution:**
```rust
// Centralize in ReBAC engine
let allowed = engine.check(CheckRequest::new(
    "resource", resource.id, "edit",
    Subject::User(user.id),
)).await?;

if allowed.allowed {
    allow_access();
}
```

---

## Performance Optimization

### 1. Batch Operations

**✅ DO:**
```rust
// Batch multiple checks into single query
let subjects = vec![
    Subject::User("alice".to_string()),
    Subject::User("bob".to_string()),
    Subject::User("charlie".to_string()),
];

let results = engine.batch_check(
    "document",
    "123",
    "viewer",
    subjects,
).await?;
```

**❌ DON'T:**
```rust
// Individual queries in a loop (N+1 problem)
for user in users {
    let allowed = engine.check(CheckRequest::new(
        "document", "123", "viewer",
        Subject::User(user),
    )).await?;  // Separate DB query each time!
}
```

### 2. Cache Warming

**✅ DO:**
```rust
use oxify_authz::warming::{WarmingStrategy, WarmingConfig};

// Pre-load hot paths on startup
let warming_config = WarmingConfig {
    strategies: vec![
        WarmingStrategy::HotPath {
            namespaces: vec!["document".to_string(), "team".to_string()],
            relations: vec!["viewer".to_string(), "member".to_string()],
        },
        WarmingStrategy::CriticalTenant {
            tenant_ids: vec!["enterprise-customer-1".to_string()],
        },
    ],
    ..Default::default()
};

engine.warm_cache(warming_config).await?;
```

### 3. Use Leopard Index for Transitive Checks

**✅ DO:**
```rust
// Enable Leopard index for O(1) reachability checks
let hybrid_engine = HybridRebacEngine::new(
    pool.clone(),
    PermissionCache::new(10000),
    Some(LeopardIndex::new()),  // Enable reachability index
).await?;

// Transitive checks are now O(1) instead of graph traversal
let allowed = hybrid_engine.check(CheckRequest::new(
    "organization", "acme-corp", "member",
    Subject::User("alice".to_string()),
)).await?;  // Fast lookup via index
```

---

## Multi-Tenancy Best Practices

### 1. Strict Tenant Isolation

**✅ DO:**
```rust
use oxify_authz::multitenancy::{MultiTenantEngine, TenantContext};

// Always use tenant-aware operations
let tenant_ctx = TenantContext::new("tenant-123".to_string());
let engine = MultiTenantEngine::new(base_engine, tenant_ctx);

// All operations are automatically scoped to tenant
engine.write_tuple(tuple).await?;  // Tenant ID added automatically
```

**❌ DON'T:**
```rust
// Manually managing tenant IDs (error-prone)
let tuple = RelationTuple::new(
    &format!("tenant_{}_document", tenant_id),  // Fragile!
    "viewer",
    "123",
    Subject::User(user_id),
);
```

### 2. Per-Tenant Quotas

**✅ DO:**
```rust
// Enforce resource limits per tenant
let quota_config = QuotaConfig {
    max_tuples: 100_000,
    max_checks_per_minute: 10_000,
    max_subjects_per_resource: 1_000,
};

engine.set_quota(tenant_id, quota_config).await?;
```

### 3. Cross-Tenant Access Audit

**✅ DO:**
```rust
// Explicitly track cross-tenant sharing
if source_tenant != target_tenant {
    audit_logger.log_cross_tenant_access(
        source_tenant,
        target_tenant,
        resource_id,
        user_id,
    ).await?;
}
```

---

## Monitoring & Observability

### 1. Metrics Collection

**✅ DO:**
```rust
use oxify_authz::metrics::AuthzMetrics;

let metrics = engine.get_metrics();
println!("Cache hit rate: {:.2}%", metrics.cache_hit_rate() * 100.0);
println!("Avg check latency: {:?}", metrics.avg_check_latency());
println!("Total checks: {}", metrics.total_checks);

// Export to Prometheus
let json = metrics.to_json()?;
prometheus_exporter.export(json);
```

### 2. Anomaly Detection

**✅ DO:**
```rust
use oxify_authz::anomaly::{AnomalyDetector, AnomalyConfig};

let anomaly_config = AnomalyConfig {
    min_baseline_events: 100,
    zscore_threshold: 3.0,
    enable_privilege_escalation: true,
    ..Default::default()
};

let mut detector = AnomalyDetector::new(anomaly_config);

// Check each access event
let event = AccessEvent {
    subject_id: user_id.to_string(),
    resource_id: resource_id.to_string(),
    relation: relation.to_string(),
    granted: allowed,
    timestamp: SystemTime::now(),
};

if let Some(anomaly) = detector.check_anomaly(&event) {
    match anomaly.severity {
        s if s > 0.8 => alert_security_team_urgent(anomaly),
        s if s > 0.5 => log_security_warning(anomaly),
        _ => log_info(anomaly),
    }
}
```

### 3. Permission Recommendations

**✅ DO:**
```rust
use oxify_authz::recommendations::{RecommendationEngine, RecommendationConfig};

// Regularly analyze permission usage
let rec_engine = RecommendationEngine::new(RecommendationConfig::default());

// Track usage
rec_engine.add_tuple(&tuple);
rec_engine.record_access(user_id, resource_id, relation);

// Generate optimization recommendations monthly
let recommendations = rec_engine.generate_recommendations();
for rec in recommendations.iter().filter(|r| r.priority >= Priority::High) {
    println!("[{}] {}", rec.priority, rec.description);
    println!("  Action: {}", rec.suggested_action);
    println!("  Impact: {}", rec.estimated_impact);
}
```

---

## Testing Strategies

### 1. Property-Based Testing

**✅ DO:**
```rust
use oxify_authz::proptest_helpers::*;
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_permission_symmetry(
        tuple in relation_tuple_strategy()
    ) {
        // If we write a tuple, we should be able to read it back
        runtime.block_on(async {
            engine.write_tuple(tuple.clone()).await?;
            let result = engine.check(CheckRequest::new(
                &tuple.namespace,
                &tuple.object_id,
                &tuple.relation,
                tuple.subject,
            )).await?;
            assert!(result.allowed);
        });
    }
}
```

### 2. Chaos Engineering

**✅ DO:**
```rust
use oxify_authz::chaos::{ChaosTest, FailureScenario};

// Test resilience to database failures
let chaos = ChaosTest::new(engine.clone());
chaos.inject_failure(FailureScenario::DatabaseDown).await?;

// System should gracefully degrade (use cache)
let result = engine.check(request).await;
assert!(result.is_ok() || result.err().unwrap().is_retriable());
```

### 3. Load Testing

**✅ DO:**
```bash
# Use included k6 load test
cd loadtest
k6 run --vus 100 --duration 5m authz_load_test.js

# Verify performance targets
# - p95 < 100ms for cached checks
# - p99 < 500ms for uncached checks
# - Throughput > 10,000 checks/sec
```

---

## Incident Response

### 1. Permission Breach Response

**Immediate Actions:**
```rust
// 1. Revoke compromised permissions
engine.delete_tuple(&compromised_tuple).await?;

// 2. Audit who had access
let audit_events = audit_logger.query_by_resource(
    "document",
    compromised_doc_id,
    incident_timeframe,
).await?;

// 3. Check for unauthorized access
for event in audit_events {
    if !event.allowed && event.timestamp > breach_time {
        investigate_denial_attempt(event);
    }
}

// 4. Invalidate caches
engine.invalidate_cache_for_resource("document", compromised_doc_id).await?;
```

### 2. Privilege Escalation Detection

**Response Playbook:**
```rust
// Automated response to privilege escalation attempts
if anomaly.anomaly_type == AnomalyType::PrivilegeEscalation {
    // 1. Temporarily disable account
    security_service.suspend_user(event.subject_id).await?;

    // 2. Alert SOC team
    alert_security_team(format!(
        "Privilege escalation detected: {} attempted {} access to {}",
        event.subject_id, event.relation, event.resource_id
    ));

    // 3. Freeze related permissions
    engine.freeze_subject_permissions(event.subject_id).await?;

    // 4. Initiate investigation workflow
    incident_tracker.create_ticket(
        IncidentType::PrivilegeEscalation,
        anomaly,
    ).await?;
}
```

### 3. Performance Degradation

**Diagnosis Steps:**
```rust
// 1. Check metrics
let metrics = engine.get_metrics();
if metrics.avg_check_latency() > Duration::from_millis(100) {
    // 2. Analyze cache performance
    println!("Cache hit rate: {:.2}%", metrics.cache_hit_rate() * 100.0);

    // 3. Check database load
    let pool_stats = engine.get_pool_stats().await?;
    println!("Active connections: {}/{}",
        pool_stats.active_connections,
        pool_stats.max_connections
    );

    // 4. Review slow queries
    let slow_queries = engine.get_slow_queries(Duration::from_secs(1)).await?;

    // 5. Scale horizontally if needed
    if pool_stats.active_connections >= pool_stats.max_connections * 0.9 {
        scale_out_read_replicas().await?;
    }
}
```

---

## Summary Checklist

### Security
- [ ] Principle of least privilege enforced
- [ ] Time-limited access for temporary permissions
- [ ] Audit logging enabled (100% for sensitive operations)
- [ ] Conditional access policies for sensitive resources
- [ ] Regular permission reviews scheduled

### Performance
- [ ] Batch operations used for multiple checks
- [ ] Cache warming configured for hot paths
- [ ] Leopard index enabled for transitive checks
- [ ] Connection pooling optimized
- [ ] Load testing performed

### Operations
- [ ] Monitoring and alerting configured
- [ ] Anomaly detection enabled
- [ ] Incident response playbooks documented
- [ ] Regular backup and recovery tested
- [ ] Multi-tenancy isolation verified

### Development
- [ ] Property-based tests written
- [ ] Chaos engineering tests passed
- [ ] Integration tests cover critical paths
- [ ] Documentation up to date
- [ ] Code review checklist includes authz verification

---

**Last Updated:** 2026-01-19
**Version:** 1.0
**Maintainer:** OxiFY Authorization Team
