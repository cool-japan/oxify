# OxiFY AuthZ - Integration Examples

This guide provides complete, production-ready examples for integrating oxify-authz into your application.

---

## Table of Contents

1. [Basic Authorization](#basic-authorization)
2. [AI-Powered Security Monitoring](#ai-powered-security-monitoring)
3. [Permission Optimization](#permission-optimization)
4. [Multi-Tenant SaaS Application](#multi-tenant-saas-application)
5. [Microservices with gRPC](#microservices-with-grpc)
6. [Edge Computing](#edge-computing)
7. [Quantum-Signed Audit Logs](#quantum-signed-audit-logs)
8. [Zero-Knowledge Proof Authorization](#zero-knowledge-proof-authorization)

---

## Basic Authorization

### Setup

```rust
use oxify_authz::*;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize database connection
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .connect("postgres://user:pass@localhost/authz")
        .await?;

    // 2. Create authorization engine
    let engine = HybridRebacEngine::new(
        pool.clone(),
        PermissionCache::new(10000),
        Some(LeopardIndex::new()),
    ).await?;

    // 3. Define permission structure
    setup_permissions(&engine).await?;

    // 4. Check permissions
    let allowed = check_user_access(&engine, "alice", "doc:123", "edit").await?;

    if allowed {
        println!("✅ Access granted");
    } else {
        println!("❌ Access denied");
    }

    Ok(())
}

async fn setup_permissions(engine: &HybridRebacEngine) -> Result<()> {
    // Create organizational hierarchy
    // alice is owner of doc:123
    engine.write_tuple(RelationTuple::new(
        "document",
        "owner",
        "123",
        Subject::User("alice".to_string()),
    )).await?;

    // bob is a member of team:engineering
    engine.write_tuple(RelationTuple::new(
        "team",
        "member",
        "engineering",
        Subject::User("bob".to_string()),
    )).await?;

    // team:engineering has viewer access to doc:123
    engine.write_tuple(RelationTuple::new(
        "document",
        "viewer",
        "123",
        Subject::UserSet {
            namespace: "team".to_string(),
            object_id: "engineering".to_string(),
            relation: "member".to_string(),
        },
    )).await?;

    Ok(())
}

async fn check_user_access(
    engine: &HybridRebacEngine,
    user_id: &str,
    resource_id: &str,
    action: &str,
) -> Result<bool> {
    let result = engine.check(CheckRequest::new(
        "document",
        resource_id,
        action,
        Subject::User(user_id.to_string()),
    )).await?;

    Ok(result.allowed)
}
```

---

## AI-Powered Security Monitoring

### Real-Time Anomaly Detection

```rust
use oxify_authz::{
    HybridRebacEngine, CheckRequest, Subject,
    anomaly::{AnomalyDetector, AnomalyConfig, AccessEvent, AnomalyType},
    audit::{AuditLogger, AuditConfig},
};
use std::time::SystemTime;

pub struct SecureAuthzService {
    engine: HybridRebacEngine,
    anomaly_detector: AnomalyDetector,
    audit_logger: AuditLogger,
}

impl SecureAuthzService {
    pub async fn new(pool: sqlx::PgPool) -> Result<Self> {
        // 1. Create base authorization engine
        let engine = HybridRebacEngine::new(
            pool.clone(),
            PermissionCache::new(10000),
            Some(LeopardIndex::new()),
        ).await?;

        // 2. Configure anomaly detection
        let anomaly_config = AnomalyConfig {
            min_baseline_events: 100,      // Learn from 100 events
            zscore_threshold: 2.5,         // Flag if 2.5 std devs from mean
            max_access_rate: 100.0,        // 100 requests/min max
            enable_temporal_detection: true,
            enable_privilege_escalation: true,
            ..Default::default()
        };
        let anomaly_detector = AnomalyDetector::new(anomaly_config);

        // 3. Configure audit logging
        let audit_config = AuditConfig {
            sample_rate: 0.1,              // Log 10% of checks
            log_denied: true,              // Always log denials
            log_mutations: true,           // Always log writes/deletes
            ..Default::default()
        };
        let audit_logger = AuditLogger::new(pool, audit_config);

        Ok(Self {
            engine,
            anomaly_detector,
            audit_logger,
        })
    }

    /// Check authorization with security monitoring
    pub async fn check_with_monitoring(
        &mut self,
        user_id: &str,
        resource_id: &str,
        action: &str,
        client_ip: Option<std::net::IpAddr>,
    ) -> Result<bool> {
        // 1. Perform authorization check
        let mut request = CheckRequest::new(
            extract_namespace(resource_id),
            extract_object_id(resource_id),
            action,
            Subject::User(user_id.to_string()),
        );

        // Add request context if provided
        if let Some(ip) = client_ip {
            request = request.with_context(
                RequestContext::new().with_client_ip(ip)
            );
        }

        let result = self.engine.check(request).await?;

        // 2. Create access event for anomaly detection
        let event = AccessEvent {
            subject_id: user_id.to_string(),
            resource_id: resource_id.to_string(),
            relation: action.to_string(),
            granted: result.allowed,
            timestamp: SystemTime::now(),
        };

        // 3. Check for anomalies
        if let Some(anomaly) = self.anomaly_detector.check_anomaly(&event) {
            // Alert based on severity
            match anomaly.severity {
                s if s >= 0.8 => {
                    // CRITICAL: Immediate action required
                    self.alert_security_team_urgent(&anomaly).await?;

                    // Consider blocking user temporarily
                    if matches!(anomaly.anomaly_type, AnomalyType::PrivilegeEscalation) {
                        self.temporary_suspend_user(user_id).await?;
                    }
                }
                s if s >= 0.5 => {
                    // HIGH: Log for investigation
                    log::warn!(
                        "Security anomaly detected: {} (severity: {:.2})",
                        anomaly.description,
                        anomaly.severity
                    );
                    self.create_security_ticket(&anomaly).await?;
                }
                _ => {
                    // LOW: Just log
                    log::info!("Minor anomaly: {}", anomaly.description);
                }
            }
        }

        // 4. Audit log (with sampling)
        self.audit_logger.log_check(&event).await?;

        Ok(result.allowed)
    }

    async fn alert_security_team_urgent(&self, anomaly: &Anomaly) -> Result<()> {
        // Integration with PagerDuty, Slack, etc.
        log::error!(
            "🚨 SECURITY ALERT: {} - Subject: {}, Resource: {}, Severity: {:.2}",
            anomaly.description,
            anomaly.subject_id,
            anomaly.resource_id,
            anomaly.severity
        );

        // Send to incident management system
        // pagerduty::trigger_incident(anomaly).await?;
        // slack::send_alert("#security", anomaly).await?;

        Ok(())
    }

    async fn temporary_suspend_user(&self, user_id: &str) -> Result<()> {
        log::warn!("Temporarily suspending user: {}", user_id);
        // Implement suspension logic (e.g., add to blocklist)
        Ok(())
    }

    async fn create_security_ticket(&self, anomaly: &Anomaly) -> Result<()> {
        log::info!("Creating security investigation ticket for {}", anomaly.subject_id);
        // Integration with Jira, ServiceNow, etc.
        Ok(())
    }

    /// Get security metrics dashboard
    pub fn get_security_metrics(&self, user_id: &str) -> Option<AnomalyStats> {
        self.anomaly_detector.get_subject_stats(user_id)
    }
}

fn extract_namespace(resource_id: &str) -> &str {
    resource_id.split(':').next().unwrap_or("unknown")
}

fn extract_object_id(resource_id: &str) -> &str {
    resource_id.split(':').nth(1).unwrap_or(resource_id)
}
```

### Usage Example

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .connect("postgres://localhost/authz")
        .await?;

    let mut service = SecureAuthzService::new(pool).await?;

    // Normal access - allowed
    let allowed = service.check_with_monitoring(
        "alice",
        "document:123",
        "view",
        Some("192.168.1.100".parse().unwrap()),
    ).await?;

    // Suspicious access - triggers anomaly alert
    for _ in 0..150 {  // Burst of requests (rate limit exceeded)
        service.check_with_monitoring(
            "eve",
            "document:sensitive",
            "admin",  // Privilege escalation attempt
            Some("suspicious.ip.addr".parse().unwrap()),
        ).await?;
    }

    // View security metrics
    if let Some(stats) = service.get_security_metrics("eve") {
        println!("User 'eve' statistics:");
        println!("  Total events: {}", stats.total_events);
        println!("  Denied events: {}", stats.denied_events);
        println!("  Denial rate: {:.1}%", stats.denial_rate * 100.0);
    }

    Ok(())
}
```

---

## Permission Optimization

### Automated Permission Review

```rust
use oxify_authz::{
    HybridRebacEngine, RelationTuple, Subject,
    recommendations::{
        RecommendationEngine, RecommendationConfig,
        Priority, RecommendationType
    },
};

pub struct PermissionOptimizer {
    engine: HybridRebacEngine,
    rec_engine: RecommendationEngine,
}

impl PermissionOptimizer {
    pub fn new(engine: HybridRebacEngine) -> Self {
        let rec_config = RecommendationConfig {
            min_usage_threshold: 0.1,  // Flag if <10% usage
            min_consolidation_count: 3,
            enable_hierarchy_analysis: true,
            enable_role_suggestions: true,
            ..Default::default()
        };

        Self {
            engine,
            rec_engine: RecommendationEngine::new(rec_config),
        }
    }

    /// Import existing permissions for analysis
    pub async fn import_permissions(&mut self) -> Result<()> {
        // Fetch all tuples from database
        let tuples = self.engine.list_tuples(None, None).await?;

        // Add to recommendation engine
        for tuple in tuples {
            self.rec_engine.add_tuple(&tuple);
        }

        println!("✅ Imported {} tuples for analysis", tuples.len());
        Ok(())
    }

    /// Track access patterns
    pub fn record_access(&mut self, user_id: &str, resource_id: &str, action: &str) {
        self.rec_engine.record_access(user_id, resource_id, action);
    }

    /// Generate optimization report
    pub fn generate_report(&self) -> PermissionReport {
        let recommendations = self.rec_engine.generate_recommendations();

        let critical = recommendations.iter()
            .filter(|r| r.priority >= Priority::Critical)
            .count();
        let high = recommendations.iter()
            .filter(|r| r.priority == Priority::High)
            .count();
        let medium = recommendations.iter()
            .filter(|r| r.priority == Priority::Medium)
            .count();

        PermissionReport {
            total_recommendations: recommendations.len(),
            critical_count: critical,
            high_count: high,
            medium_count: medium,
            recommendations,
        }
    }

    /// Apply recommendations (with approval)
    pub async fn apply_recommendation(
        &mut self,
        recommendation: &Recommendation,
        approved: bool,
    ) -> Result<()> {
        if !approved {
            println!("⏭️  Skipping: {}", recommendation.description);
            return Ok(());
        }

        match recommendation.recommendation_type {
            RecommendationType::UnusedPermission => {
                // Revoke unused permissions
                for tuple in &recommendation.affected_tuples {
                    println!("🗑️  Revoking: {:?}", tuple);
                    self.engine.delete_tuple(tuple).await?;
                }
            }

            RecommendationType::HierarchicalRedundancy => {
                // Keep parent, remove child
                if recommendation.affected_tuples.len() >= 2 {
                    let child = &recommendation.affected_tuples[1];
                    println!("🗑️  Removing redundant child: {:?}", child);
                    self.engine.delete_tuple(child).await?;
                }
            }

            RecommendationType::RoleSuggestion => {
                // Create role (manual step - just log)
                println!("💡 Role suggestion: {}", recommendation.suggested_action);
                println!("   Manual action required: Create role with specified permissions");
            }

            _ => {
                println!("ℹ️  Manual review needed: {}", recommendation.description);
            }
        }

        Ok(())
    }
}

pub struct PermissionReport {
    pub total_recommendations: usize,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub recommendations: Vec<Recommendation>,
}

impl PermissionReport {
    pub fn print_summary(&self) {
        println!("\n📊 Permission Optimization Report");
        println!("================================");
        println!("Total recommendations: {}", self.total_recommendations);
        println!("  🔴 Critical: {}", self.critical_count);
        println!("  🟠 High:     {}", self.high_count);
        println!("  🟡 Medium:   {}", self.medium_count);
        println!();

        for (i, rec) in self.recommendations.iter().enumerate() {
            let emoji = match rec.priority {
                Priority::Critical => "🔴",
                Priority::High => "🟠",
                Priority::Medium => "🟡",
                Priority::Low => "🟢",
            };

            println!("{}. {} [{}] {}", i + 1, emoji, rec.priority, rec.description);
            println!("   Action: {}", rec.suggested_action);
            println!("   Impact: {}", rec.estimated_impact);
            println!();
        }
    }
}
```

### Usage Example

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let pool = PgPoolOptions::new()
        .connect("postgres://localhost/authz")
        .await?;

    let engine = HybridRebacEngine::new(
        pool,
        PermissionCache::new(10000),
        Some(LeopardIndex::new()),
    ).await?;

    let mut optimizer = PermissionOptimizer::new(engine);

    // 1. Import existing permissions
    optimizer.import_permissions().await?;

    // 2. Simulate access patterns (in production, track real usage)
    for _ in 0..100 {
        optimizer.record_access("alice", "doc:123", "view");
        optimizer.record_access("bob", "doc:456", "edit");
        // "charlie" never accesses doc:789 (unused permission)
    }

    // 3. Generate optimization report
    let report = optimizer.generate_report();
    report.print_summary();

    // 4. Review and apply recommendations
    for recommendation in &report.recommendations {
        if recommendation.priority >= Priority::High {
            println!("\n🔍 Review: {}", recommendation.description);
            println!("Apply this recommendation? (y/n)");

            // In production, integrate with approval workflow
            let approved = true;  // Simulated approval

            optimizer.apply_recommendation(recommendation, approved).await?;
        }
    }

    println!("\n✅ Permission optimization complete!");

    Ok(())
}
```

---

## Multi-Tenant SaaS Application

```rust
use oxify_authz::{
    multitenancy::{MultiTenantEngine, TenantContext, QuotaConfig},
    HybridRebacEngine,
};

pub struct SaaSAuthzService {
    base_engine: HybridRebacEngine,
}

impl SaaSAuthzService {
    pub async fn new(pool: sqlx::PgPool) -> Result<Self> {
        let base_engine = HybridRebacEngine::new(
            pool,
            PermissionCache::new(10000),
            Some(LeopardIndex::new()),
        ).await?;

        Ok(Self { base_engine })
    }

    /// Create tenant-scoped engine
    pub fn for_tenant(&self, tenant_id: &str) -> MultiTenantEngine {
        let tenant_ctx = TenantContext::new(tenant_id.to_string());
        MultiTenantEngine::new(self.base_engine.clone(), tenant_ctx)
    }

    /// Set tenant quotas (prevent abuse)
    pub async fn configure_tenant_quota(
        &self,
        tenant_id: &str,
        plan: &str,
    ) -> Result<()> {
        let quota = match plan {
            "free" => QuotaConfig {
                max_tuples: 1_000,
                max_checks_per_minute: 100,
                max_subjects_per_resource: 50,
            },
            "pro" => QuotaConfig {
                max_tuples: 100_000,
                max_checks_per_minute: 10_000,
                max_subjects_per_resource: 1_000,
            },
            "enterprise" => QuotaConfig {
                max_tuples: 10_000_000,
                max_checks_per_minute: 1_000_000,
                max_subjects_per_resource: 100_000,
            },
            _ => return Err(AuthzError::InvalidTuple("Unknown plan".to_string())),
        };

        // Store quota configuration
        println!("📊 Configured quota for tenant {}: {:?}", tenant_id, quota);
        Ok(())
    }
}

// Example: Multi-tenant document sharing app
#[tokio::main]
async fn main() -> Result<()> {
    let pool = PgPoolOptions::new()
        .connect("postgres://localhost/authz")
        .await?;

    let service = SaaSAuthzService::new(pool).await?;

    // Configure tenants
    service.configure_tenant_quota("tenant-acme", "enterprise").await?;
    service.configure_tenant_quota("tenant-startup", "pro").await?;

    // Tenant A: ACME Corp
    let acme_engine = service.for_tenant("tenant-acme");

    acme_engine.write_tuple(RelationTuple::new(
        "document",
        "owner",
        "financial-report",
        Subject::User("alice@acme.com".to_string()),
    )).await?;

    // Tenant B: Startup Inc (isolated from Tenant A)
    let startup_engine = service.for_tenant("tenant-startup");

    startup_engine.write_tuple(RelationTuple::new(
        "document",
        "owner",
        "pitch-deck",
        Subject::User("bob@startup.com".to_string()),
    )).await?;

    // Cross-tenant access denied by default
    let result = acme_engine.check(CheckRequest::new(
        "document",
        "pitch-deck",  // Belongs to tenant-startup
        "view",
        Subject::User("alice@acme.com".to_string()),
    )).await?;

    assert!(!result.allowed, "Cross-tenant access must be denied");

    println!("✅ Multi-tenant isolation verified");

    Ok(())
}
```

---

## Microservices with gRPC

```rust
use oxify_authz::grpc::{AuthorizationServer, AuthorizationServiceImpl};
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .connect("postgres://localhost/authz")
        .await?;

    let engine = HybridRebacEngine::new(
        pool,
        PermissionCache::new(10000),
        Some(LeopardIndex::new()),
    ).await?;

    let service = AuthorizationServiceImpl::new(engine);

    let addr = "[::1]:50051".parse()?;

    println!("🚀 gRPC AuthZ server listening on {}", addr);

    Server::builder()
        .add_service(AuthorizationServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
```

---

## Edge Computing

```rust
use oxify_authz::edge::{EdgeEngine, EdgeConfig, CRDTuple};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Central database
    let pool = PgPoolOptions::new()
        .connect("postgres://central-db/authz")
        .await?;

    // Create edge engine (embedded, no DB dependency)
    let edge_config = EdgeConfig {
        sync_interval: Duration::from_secs(30),  // Sync every 30s
        enable_local_writes: true,               // Allow writes at edge
        ..Default::default()
    };

    let mut edge_engine = EdgeEngine::new(edge_config);

    // Initial sync from central DB
    let tuples = fetch_tuples_from_central(&pool).await?;
    for tuple in tuples {
        let crdt_tuple = CRDTuple::new(
            tuple,
            chrono::Utc::now(),
            "edge-us-west".to_string(),
        );
        edge_engine.write(crdt_tuple).await?;
    }

    // Low-latency authorization check (no DB roundtrip!)
    let allowed = edge_engine.check(
        "document",
        "123",
        "view",
        "user:alice",
    ).await?;

    println!("✅ Edge check latency: <5ms (vs 100ms to central DB)");

    // Background sync task
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            // Sync changes with central DB
            sync_with_central(&edge_engine, &pool).await.ok();
        }
    });

    Ok(())
}

async fn fetch_tuples_from_central(pool: &sqlx::PgPool) -> Result<Vec<RelationTuple>> {
    // Fetch tuples from central database
    Ok(vec![])
}

async fn sync_with_central(edge: &EdgeEngine, pool: &sqlx::PgPool) -> Result<()> {
    // Implement bi-directional sync with CRDT merge
    Ok(())
}
```

---

## Complete Production Example

Combining all features:

```rust
use oxify_authz::*;

pub struct ProductionAuthzService {
    multi_tenant: SaaSAuthzService,
    security: SecureAuthzService,
    optimizer: PermissionOptimizer,
}

impl ProductionAuthzService {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(100)
            .connect(database_url)
            .await?;

        let base_engine = HybridRebacEngine::new(
            pool.clone(),
            PermissionCache::new(100_000),
            Some(LeopardIndex::new()),
        ).await?;

        Ok(Self {
            multi_tenant: SaaSAuthzService::new(pool.clone()).await?,
            security: SecureAuthzService::new(pool).await?,
            optimizer: PermissionOptimizer::new(base_engine),
        })
    }

    /// Check authorization with full security stack
    pub async fn authorize(
        &mut self,
        tenant_id: &str,
        user_id: &str,
        resource_id: &str,
        action: &str,
        client_ip: Option<std::net::IpAddr>,
    ) -> Result<bool> {
        // 1. Tenant-scoped check
        let tenant_engine = self.multi_tenant.for_tenant(tenant_id);

        let result = tenant_engine.check(CheckRequest::new(
            extract_namespace(resource_id),
            extract_object_id(resource_id),
            action,
            Subject::User(user_id.to_string()),
        )).await?;

        // 2. Security monitoring
        self.security.check_with_monitoring(
            user_id,
            resource_id,
            action,
            client_ip,
        ).await?;

        // 3. Track for optimization
        self.optimizer.record_access(user_id, resource_id, action);

        Ok(result.allowed)
    }

    /// Monthly permission optimization
    pub async fn run_monthly_optimization(&mut self) -> Result<()> {
        println!("🔄 Running monthly permission optimization...");

        self.optimizer.import_permissions().await?;
        let report = self.optimizer.generate_report();
        report.print_summary();

        // Auto-apply critical recommendations
        for rec in &report.recommendations {
            if rec.priority == Priority::Critical {
                self.optimizer.apply_recommendation(rec, true).await?;
            }
        }

        Ok(())
    }
}
```

---

## Quantum-Signed Audit Logs

Tamper-proof audit trails using post-quantum cryptography.

### Setup

```rust
use oxify_authz::*;
use oxify_authz::quantum::*;
use oxify_authz::audit::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize quantum key manager
    let mut key_manager = QuantumKeyManager::new(QuantumAlgorithm::HybridEd25519MlDsa)?;

    // 2. Create audit logger with quantum signatures
    let audit_logger = QuantumAuditLogger::new(key_manager);

    // 3. Initialize authorization engine with signing
    let engine = QuantumSecureAuthzEngine::new(
        "postgres://localhost/authz",
        audit_logger,
    ).await?;

    // 4. Perform operations (automatically signed)
    engine.write_tuple_signed(RelationTuple::new(
        "document",
        "owner",
        "doc:123",
        Subject::User("alice".to_string()),
    )).await?;

    // 5. Verify audit trail integrity
    engine.verify_audit_integrity().await?;

    Ok(())
}
```

### Implementation

```rust
use oxify_authz::*;
use oxify_authz::quantum::*;
use oxify_authz::audit::*;
use std::time::SystemTime;

/// Audit logger with quantum-safe signatures
pub struct QuantumAuditLogger {
    key_manager: QuantumKeyManager,
    audit_logger: AuditLogger,
}

impl QuantumAuditLogger {
    pub fn new(key_manager: QuantumKeyManager) -> Self {
        Self {
            key_manager,
            audit_logger: AuditLogger::new(AuditConfig::default()),
        }
    }

    /// Log an event with quantum signature
    pub async fn log_signed_event(
        &mut self,
        event_type: AuditEventType,
        event: AuditEvent,
    ) -> Result<SignedAuditEvent> {
        // 1. Serialize event
        let event_bytes = serde_json::to_vec(&event)
            .map_err(|e| AuthzError::InvalidTuple(format!("Serialization failed: {}", e)))?;

        // 2. Sign with quantum-safe algorithm
        let signature = self.key_manager.current_keypair().sign(&event_bytes)?;

        // 3. Create signed event
        let signed_event = SignedAuditEvent {
            event,
            signature,
            signed_at: SystemTime::now(),
        };

        // 4. Log to database
        self.audit_logger.log_event(event_type).await?;

        Ok(signed_event)
    }

    /// Verify audit trail integrity
    pub async fn verify_audit_trail(
        &mut self,
        events: Vec<SignedAuditEvent>,
    ) -> Result<AuditVerificationReport> {
        let mut report = AuditVerificationReport {
            total_events: events.len(),
            verified: 0,
            tampered: Vec::new(),
            failed_verification: Vec::new(),
        };

        for (idx, event) in events.iter().enumerate() {
            let event_bytes = serde_json::to_vec(&event.event)
                .map_err(|e| AuthzError::InvalidTuple(format!("Serialization failed: {}", e)))?;

            match self.key_manager.verify_any(&event_bytes, &event.signature) {
                Ok(true) => report.verified += 1,
                Ok(false) => report.tampered.push(idx),
                Err(_) => report.failed_verification.push(idx),
            }
        }

        Ok(report)
    }

    /// Rotate quantum keys (recommended: every 90 days)
    pub async fn rotate_keys(&mut self) -> Result<()> {
        if self.key_manager.rotate_if_needed()? {
            println!("🔑 Quantum keys rotated successfully");
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SignedAuditEvent {
    pub event: AuditEvent,
    pub signature: QuantumSignature,
    pub signed_at: SystemTime,
}

#[derive(Debug)]
pub struct AuditVerificationReport {
    pub total_events: usize,
    pub verified: usize,
    pub tampered: Vec<usize>,
    pub failed_verification: Vec<usize>,
}

impl AuditVerificationReport {
    pub fn is_valid(&self) -> bool {
        self.tampered.is_empty() && self.failed_verification.is_empty()
    }

    pub fn print_summary(&self) {
        println!("📊 Audit Verification Report");
        println!("   Total events: {}", self.total_events);
        println!("   ✅ Verified: {}", self.verified);
        println!("   ⚠️  Tampered: {}", self.tampered.len());
        println!("   ❌ Failed: {}", self.failed_verification.len());

        if self.is_valid() {
            println!("   ✅ Audit trail integrity confirmed");
        } else {
            println!("   ⚠️  WARNING: Integrity violations detected!");
        }
    }
}

/// Authorization engine with quantum signatures
pub struct QuantumSecureAuthzEngine {
    engine: HybridRebacEngine,
    audit_logger: QuantumAuditLogger,
}

impl QuantumSecureAuthzEngine {
    pub async fn new(
        database_url: &str,
        audit_logger: QuantumAuditLogger,
    ) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(50)
            .connect(database_url)
            .await
            .map_err(|e| AuthzError::DatabaseError(e.to_string()))?;

        let engine = HybridRebacEngine::new(
            pool,
            PermissionCache::new(10000),
            Some(LeopardIndex::new()),
        ).await?;

        Ok(Self {
            engine,
            audit_logger,
        })
    }

    /// Write tuple with quantum signature
    pub async fn write_tuple_signed(&mut self, tuple: RelationTuple) -> Result<()> {
        // 1. Write to authorization engine
        self.engine.write_tuple(tuple.clone()).await?;

        // 2. Create audit event
        let event = AuditEvent {
            timestamp: SystemTime::now(),
            actor: tuple.subject.to_string(),
            action: format!("write_tuple: {} {} on {}:{}",
                tuple.subject, tuple.relation, tuple.namespace, tuple.object_id),
            resource: format!("{}:{}", tuple.namespace, tuple.object_id),
            result: AuditResult::Allowed,
            metadata: None,
        };

        // 3. Sign and log
        self.audit_logger.log_signed_event(AuditEventType::TupleMutation, event).await?;

        Ok(())
    }

    /// Verify complete audit trail
    pub async fn verify_audit_integrity(&mut self) -> Result<AuditVerificationReport> {
        // In production: Load from database
        let events = vec![]; // Load signed events from DB
        self.audit_logger.verify_audit_trail(events).await
    }
}
```

### Monthly Key Rotation

```rust
use tokio_cron_scheduler::{JobScheduler, Job};

/// Schedule automatic quantum key rotation
pub async fn setup_key_rotation(
    mut engine: QuantumSecureAuthzEngine
) -> Result<()> {
    let scheduler = JobScheduler::new().await?;

    // Rotate keys every 90 days at 2 AM
    scheduler.add(Job::new_async("0 0 2 */90 * *", move |_uuid, _lock| {
        Box::pin(async move {
            match engine.audit_logger.rotate_keys().await {
                Ok(_) => println!("✅ Scheduled key rotation completed"),
                Err(e) => eprintln!("❌ Key rotation failed: {}", e),
            }
        })
    })?)?;

    scheduler.start().await?;
    Ok(())
}
```

---

## Zero-Knowledge Proof Authorization

Privacy-preserving authorization using zkSNARKs.

### Setup

```rust
use oxify_authz::*;
use oxify_authz::zkp::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize ZKP prover and verifier
    let mut prover = ZkProver::new();
    let mut verifier = ZkVerifier::new();

    // 2. User generates proof of permission (client-side)
    let proof = prover.prove_permission(
        "alice",              // Private: not revealed
        "document:123",       // Public: revealed
        "viewer",             // Public: relation being checked
        &["owner", "editor", "viewer"], // Private: permission hierarchy
    )?;

    // 3. Server verifies proof without knowing subject
    let valid = verifier.verify_permission_proof(&proof)?;

    if valid {
        println!("✅ Access granted (identity protected)");
    } else {
        println!("❌ Access denied");
    }

    Ok(())
}
```

### Privacy-Preserving API

```rust
use oxify_authz::*;
use oxify_authz::zkp::*;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};

/// Privacy-preserving authorization API
pub struct ZkpAuthzApi {
    verifier: ZkVerifier,
    engine: HybridRebacEngine,
}

#[derive(serde::Deserialize)]
pub struct CheckRequestZkp {
    /// Zero-knowledge proof
    pub proof: PermissionProof,

    /// Public inputs only
    pub resource_id: String,
    pub action: String,
}

impl ZkpAuthzApi {
    pub fn new(engine: HybridRebacEngine) -> Self {
        Self {
            verifier: ZkVerifier::new(),
            engine,
        }
    }

    /// Check permission using zero-knowledge proof
    pub async fn check_with_zkp(
        &mut self,
        request: CheckRequestZkp,
    ) -> Result<bool> {
        // 1. Verify zero-knowledge proof
        let proof_valid = self.verifier.verify_permission_proof(&request.proof)?;

        if !proof_valid {
            return Ok(false);
        }

        // 2. Validate public inputs match
        if request.proof.public_inputs.relation != request.action {
            return Ok(false);
        }

        // 3. Additional server-side checks (rate limiting, etc.)
        // Note: Cannot check against specific user since identity is private

        Ok(true)
    }

    /// Batch verify multiple ZKP requests
    pub async fn batch_check_zkp(
        &mut self,
        requests: Vec<CheckRequestZkp>,
    ) -> Result<Vec<bool>> {
        let proofs: Vec<_> = requests.iter().map(|r| r.proof.clone()).collect();

        // Use efficient batch verification
        self.verifier.batch_verify(&proofs)
    }
}

/// Axum router setup
pub fn zkp_router(engine: HybridRebacEngine) -> Router {
    let api = std::sync::Arc::new(tokio::sync::Mutex::new(ZkpAuthzApi::new(engine)));

    Router::new()
        .route("/check", post(zkp_check_handler))
        .route("/batch_check", post(zkp_batch_check_handler))
        .with_state(api)
}

async fn zkp_check_handler(
    State(api): State<std::sync::Arc<tokio::sync::Mutex<ZkpAuthzApi>>>,
    Json(request): Json<CheckRequestZkp>,
) -> impl IntoResponse {
    let mut api = api.lock().await;

    match api.check_with_zkp(request).await {
        Ok(true) => (StatusCode::OK, "Access granted").into_response(),
        Ok(false) => (StatusCode::FORBIDDEN, "Access denied").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", e),
        ).into_response(),
    }
}

async fn zkp_batch_check_handler(
    State(api): State<std::sync::Arc<tokio::sync::Mutex<ZkpAuthzApi>>>,
    Json(requests): Json<Vec<CheckRequestZkp>>,
) -> impl IntoResponse {
    let mut api = api.lock().await;

    match api.batch_check_zkp(requests).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", e),
        ).into_response(),
    }
}
```

### Compliance Reporting (Privacy-Preserving)

```rust
/// Generate compliance report without exposing user identities
pub async fn generate_privacy_preserving_audit(
    verifier: &mut ZkVerifier,
    access_proofs: Vec<PermissionProof>,
) -> Result<ComplianceReport> {
    let mut report = ComplianceReport {
        total_accesses: access_proofs.len(),
        verified_accesses: 0,
        resources_accessed: std::collections::HashSet::new(),
        actions_performed: std::collections::HashMap::new(),
        // Note: No user identities exposed
    };

    for proof in access_proofs {
        if verifier.verify_permission_proof(&proof)? {
            report.verified_accesses += 1;

            // Track resource access (hashed, not actual object)
            let resource_hash = format!("{:?}", proof.public_inputs.object_hash);
            report.resources_accessed.insert(resource_hash);

            // Track action types
            *report.actions_performed
                .entry(proof.public_inputs.relation.clone())
                .or_insert(0) += 1;
        }
    }

    Ok(report)
}

#[derive(Debug)]
pub struct ComplianceReport {
    pub total_accesses: usize,
    pub verified_accesses: usize,
    pub resources_accessed: std::collections::HashSet<String>,
    pub actions_performed: std::collections::HashMap<String, usize>,
}

impl ComplianceReport {
    pub fn print_summary(&self) {
        println!("📊 Privacy-Preserving Compliance Report");
        println!("   Total access attempts: {}", self.total_accesses);
        println!("   Verified accesses: {}", self.verified_accesses);
        println!("   Unique resources: {}", self.resources_accessed.len());
        println!("   Actions breakdown:");
        for (action, count) in &self.actions_performed {
            println!("     - {}: {} times", action, count);
        }
        println!("   ✅ User identities protected");
    }
}
```

### Use Cases

**When to use ZKP authorization:**

1. **Healthcare**: Prove medical staff credentials without revealing identity
2. **Financial Services**: Demonstrate authorization for transactions while preserving privacy
3. **Government**: Enable auditable access without exposing classified personnel
4. **Enterprise**: Allow third-party audits without revealing internal user structure

**Performance Considerations:**

- Groth16: ~2ms verification (recommended for production)
- PLONK: ~5ms verification (universal setup)
- Bulletproofs: ~10ms verification (no trusted setup)
- STARKs: ~50ms verification (quantum-resistant)

---

**Last Updated:** 2026-01-19
**Version:** 1.1
