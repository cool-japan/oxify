//! Performance benchmarks for oxify-authn
//!
//! Run with: cargo bench --all-features

use criterion::Criterion;
use oxify_authn::{
    ApiKeyManager, ApiKeyScope, JwtConfig, JwtManager, MetricsCollector, PasswordManager,
    PasswordPolicy, Permission, RateLimitConfig, RateLimiter, SessionConfig, SessionInfo,
    SessionManager, User,
};
use std::hint::black_box;

// JWT Benchmarks
fn bench_jwt_generation(c: &mut Criterion) {
    let config = JwtConfig::development();
    let jwt_manager = JwtManager::new(&config).unwrap();

    let user = User {
        username: "benchmark_user".to_string(),
        roles: vec!["user".to_string()],
        email: Some("bench@example.com".to_string()),
        full_name: Some("Benchmark User".to_string()),
        last_login: None,
        permissions: vec![Permission::Read],
    };

    c.bench_function("jwt_generation", |b| {
        b.iter(|| jwt_manager.generate_token(black_box(&user)).unwrap());
    });
}

fn bench_jwt_validation(c: &mut Criterion) {
    let config = JwtConfig::development();
    let jwt_manager = JwtManager::new(&config).unwrap();

    let user = User {
        username: "benchmark_user".to_string(),
        roles: vec!["user".to_string()],
        email: None,
        full_name: None,
        last_login: None,
        permissions: vec![],
    };

    let token = jwt_manager.generate_token(&user).unwrap();

    c.bench_function("jwt_validation", |b| {
        b.iter(|| jwt_manager.validate_token(black_box(&token)).unwrap());
    });
}

// Password Benchmarks
#[cfg(feature = "password")]
fn bench_password_hashing(c: &mut Criterion) {
    let manager = PasswordManager::new();
    let password = "SecurePassword123!";

    c.bench_function("password_hashing", |b| {
        b.iter(|| manager.hash_password(black_box(password)).unwrap());
    });
}

#[cfg(feature = "password")]
fn bench_password_verification(c: &mut Criterion) {
    let manager = PasswordManager::new();
    let password = "SecurePassword123!";
    let hash = manager.hash_password(password).unwrap();

    c.bench_function("password_verification", |b| {
        b.iter(|| {
            manager
                .verify_password(black_box(password), black_box(&hash))
                .unwrap();
        });
    });
}

#[cfg(feature = "password")]
fn bench_password_strength_analysis(c: &mut Criterion) {
    let manager = PasswordManager::new();
    let password = "VeryStrong123!@#$%Password";

    c.bench_function("password_strength_analysis", |b| {
        b.iter(|| manager.check_password_strength(black_box(password)));
    });
}

#[cfg(feature = "password")]
fn bench_password_policy_check(c: &mut Criterion) {
    let policy = PasswordPolicy::strict();
    let manager = PasswordManager::with_policy(policy);
    let password = "StrongPassword123!";

    c.bench_function("password_policy_check", |b| {
        b.iter(|| manager.validate_password(black_box(password), black_box(Some("testuser"))));
    });
}

// Session Benchmarks
#[cfg(feature = "session")]
fn bench_session_operations(c: &mut Criterion) {
    use tokio::runtime::Runtime;
    let rt = Runtime::new().unwrap();

    c.bench_function("session_create_and_validate", |b| {
        b.iter(|| {
            let config = SessionConfig::default();
            let manager = SessionManager::new(config);

            rt.block_on(async {
                let info = SessionInfo {
                    user_id: "bench_user".to_string(),
                    ip_address: Some("192.168.1.1".to_string()),
                    user_agent: Some("Mozilla/5.0".to_string()),
                    device_name: None,
                };
                let session = manager.create_session(black_box(info)).await.unwrap();
                manager.validate_session(black_box(&session.id)).await.ok();
            });
        });
    });
}

// Rate Limiting Benchmarks
#[cfg(feature = "ratelimit")]
fn bench_rate_limit_check(c: &mut Criterion) {
    let config = RateLimitConfig::default();
    let limiter = RateLimiter::new(config);

    c.bench_function("rate_limit_check", |b| {
        b.iter(|| limiter.check_ip(black_box("192.168.1.100")));
    });
}

// API Key Benchmarks
#[cfg(feature = "apikey")]
fn bench_apikey_operations(c: &mut Criterion) {
    use tokio::runtime::Runtime;
    let rt = Runtime::new().unwrap();

    c.bench_function("apikey_generate_and_validate", |b| {
        b.iter(|| {
            let manager = ApiKeyManager::new();

            rt.block_on(async {
                let generated = manager
                    .generate_key(black_box("bench_user"), vec![ApiKeyScope::Read])
                    .await
                    .unwrap();
                manager.validate_key(black_box(generated.key())).await.ok();
            });
        });
    });
}

// Metrics Benchmarks
#[cfg(feature = "metrics")]
fn bench_metrics_recording(c: &mut Criterion) {
    use tokio::runtime::Runtime;
    let rt = Runtime::new().unwrap();
    let collector = MetricsCollector::new();

    c.bench_function("metrics_record_auth_success", |b| {
        b.iter(|| {
            rt.block_on(async {
                collector
                    .record_auth_success(black_box("bench_user"), black_box("192.168.1.1"))
                    .await;
            });
        });
    });
}

// Main benchmark runner with conditional feature support
fn all_benches() {
    let mut criterion = Criterion::default().configure_from_args();

    // JWT benchmarks (always available)
    bench_jwt_generation(&mut criterion);
    bench_jwt_validation(&mut criterion);

    // Optional feature benchmarks
    #[cfg(feature = "password")]
    {
        bench_password_hashing(&mut criterion);
        bench_password_verification(&mut criterion);
        bench_password_strength_analysis(&mut criterion);
        bench_password_policy_check(&mut criterion);
    }

    #[cfg(feature = "session")]
    {
        bench_session_operations(&mut criterion);
    }

    #[cfg(feature = "ratelimit")]
    {
        bench_rate_limit_check(&mut criterion);
    }

    #[cfg(feature = "apikey")]
    {
        bench_apikey_operations(&mut criterion);
    }

    #[cfg(feature = "metrics")]
    {
        bench_metrics_recording(&mut criterion);
    }

    criterion.final_summary();
}

fn main() {
    all_benches();
}
