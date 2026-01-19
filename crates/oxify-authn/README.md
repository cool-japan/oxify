# oxify-authn

**Security Kernel** - Enterprise-Grade Authentication for OxiFY

## Overview

`oxify-authn` is the authentication layer of OxiFY's **Zero Trust Security** architecture. It provides comprehensive authentication mechanisms designed for enterprise LLM platforms with strict security requirements.

**Codename**: The Gatekeeper
**Status**: ✅ ALL PHASES COMPLETE - Production Ready (v2.2)
**Ported from**: [OxiRS](https://github.com/cool-japan/oxirs) - Battle-tested in production semantic web applications

### Enterprise-Grade Security

Unlike basic authentication systems, oxify-authn provides a complete enterprise authentication suite:
- **Multiple Auth Methods**: JWT, OAuth2/OIDC, SAML 2.0, LDAP/AD, WebAuthn/FIDO2, Certificate-based (mTLS)
- **Advanced Security**: MFA/TOTP, session management, token revocation, rate limiting, password policies
- **Risk-Based Auth**: Location anomaly detection, device fingerprinting, impossible travel detection
- **Token Rotation**: Automatic refresh token rotation with family tracking and grace periods
- **API Key Management**: Scoped keys with rate limiting, expiration, and usage tracking
- **Security Analytics**: Real-time metrics, behavioral profiling, AI-powered anomaly detection
- **IdP Integration**: Azure AD, Okta, Auth0 with automatic OIDC discovery
- **Performance**: <1ms JWT validation, comprehensive benchmarks included

## Features

### Core Authentication (Phase 1) ✅
- **JWT Authentication**: HS256/HS384/HS512, RS256/RS384/RS512, ES256/ES384 support
- **OAuth2/OIDC**: Authorization Code Flow with PKCE (GitHub, Google, custom providers)
- **Password Management**: Argon2 hashing with configurable strength policies
- **TOTP/MFA**: RFC 6238 TOTP with backup codes and QR enrollment

### Enterprise Features (Phase 2) ✅
- **Session Management**: Stateful sessions with multi-device support, IP/UA binding
- **Token Revocation**: Blacklist with automatic cleanup and revocation reasons
- **Rate Limiting**: Per-IP and per-user limits with exponential backoff
- **Password Policies**: NIST-compliant configurable requirements
- **Audit Events**: Comprehensive event tracking (login, MFA, tokens, OAuth, security)
- **SAML 2.0**: Service Provider with AuthnRequest, Response validation, metadata
- **LDAP/AD**: User authentication, attribute retrieval, group membership, connection pooling
- **WebAuthn/FIDO2**: Passwordless authentication with platform/cross-platform authenticators

### Advanced Security (Phase 3) ✅
- **Risk-Based Auth**: Location anomaly, device fingerprinting, impossible travel detection
- **Token Rotation**: One-time use tokens, family tracking, sliding window expiration
- **Login History**: Pattern tracking for behavioral analysis

### Integration Features (Phase 4) ✅
- **API Key Management**: Scoped keys with prefix, rate limits, IP whitelist, expiration
- **Security Metrics**: Authentication metrics, performance tracking, KPIs, time-series analysis
- **Certificate Auth**: X.509 validation, mTLS, chain validation, CRL/OCSP revocation
- **IdP Support**: Azure AD, Okta, Auth0 with OIDC discovery and multi-tenant
- **AI Security**: Statistical anomaly detection, behavioral profiling, threat intelligence

### Quality & Performance (Phase 5) ✅
- **Benchmarks**: Comprehensive performance benchmarks using Criterion
- **Examples**: Integration examples for quick start
- **Documentation**: 15 doc tests, 154 unit tests, zero warnings policy

## Installation

```toml
[dependencies]
oxify-authn = { path = "../crates/security/oxify-authn" }

# Or with specific features
oxify-authn = { path = "../crates/security/oxify-authn", features = ["jwt", "oauth", "password"] }
```

### Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `jwt` | JWT token management | ✅ Yes |
| `oauth` | OAuth2/OIDC support | ✅ Yes |
| `password` | Password hashing (Argon2) | ✅ Yes |
| `mfa` | TOTP multi-factor auth | ❌ No |
| `session` | Stateful session management | ❌ No |
| `revocation` | Token revocation/blacklist | ❌ No |
| `ratelimit` | Rate limiting | ❌ No |
| `saml` | SAML 2.0 Service Provider | ❌ No |
| `ldap` | LDAP/AD integration | ❌ No |
| `webauthn` | WebAuthn/FIDO2 passwordless | ❌ No |
| `risk` | Risk-based authentication | ❌ No |
| `rotation` | Token rotation | ❌ No |
| `apikey` | API key management | ❌ No |
| `metrics` | Security metrics and analytics | ❌ No |
| `cert` | Certificate-based authentication | ❌ No |
| `idp` | Advanced IdP support (Azure AD, Okta, Auth0) | ❌ No |
| `ai` | AI-powered security (anomaly detection, behavioral profiling) | ❌ No |

## Quick Start

### JWT Authentication

```rust
use oxify_authn::{JwtManager, JwtConfig, User, Permission};
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create JWT manager with development config
    let config = JwtConfig::development();
    let jwt_manager = JwtManager::new(&config)?;

    // Create a user
    let user = User {
        username: "alice".to_string(),
        roles: vec!["admin".to_string()],
        email: Some("alice@example.com".to_string()),
        full_name: Some("Alice Smith".to_string()),
        last_login: Some(Utc::now()),
        permissions: vec![Permission::Read, Permission::Write],
    };

    // Generate token
    let token = jwt_manager.generate_token(&user)?;
    println!("Token: {}", token);

    // Validate token
    let validation = jwt_manager.validate_token(&token)?;
    println!("User: {}", validation.user.username);

    Ok(())
}
```

### Password Hashing

```rust
use oxify_authn::{PasswordManager, PasswordStrength};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let password_manager = PasswordManager::new();

    // Check password strength
    let strength = password_manager.check_password_strength("MyP@ssw0rd123!");
    match strength {
        PasswordStrength::VeryStrong => println!("Great password!"),
        PasswordStrength::Strong => println!("Good password"),
        _ => println!("Consider a stronger password"),
    }

    // Hash password
    let hash = password_manager.hash_password("MyP@ssw0rd123!")?;
    println!("Hash: {}", hash);

    // Verify password
    let is_valid = password_manager.verify_password("MyP@ssw0rd123!", &hash)?;
    assert!(is_valid);

    Ok(())
}
```

### OAuth2 Authentication

```rust
use oxify_authn::{OAuth2Service, OAuth2Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure GitHub OAuth
    let config = OAuth2Config::github(
        "your-client-id",
        "your-client-secret",
    );

    let oauth_service = OAuth2Service::new(config);

    // Generate authorization URL with PKCE
    let redirect_uri = "http://localhost:3000/auth/callback";
    let (auth_url, state) = oauth_service
        .generate_authorization_url(redirect_uri, true)
        .await?;

    println!("Visit: {}", auth_url);

    // After user authorizes, exchange code for token
    // let tokens = oauth_service.exchange_code(code, redirect_uri).await?;
    // let user_info = oauth_service.fetch_user_info(&tokens.access_token).await?;

    Ok(())
}
```

## Core Components

### `JwtManager`

Manages JWT token lifecycle.

```rust
pub struct JwtManager {
    // Configuration (algorithm, secret, expiration, etc.)
}

impl JwtManager {
    pub fn new(config: &JwtConfig) -> Result<Self>;
    pub fn generate_token(&self, user: &User) -> Result<String>;
    pub fn validate_token(&self, token: &str) -> Result<TokenValidation>;
    pub fn refresh_token(&self, token: &str) -> Result<String>;
    pub fn extract_token_from_header(header: &str) -> Option<&str>;
}
```

**Supported Algorithms**: HS256, HS384, HS512, RS256, RS384, RS512, ES256, ES384

### `PasswordManager`

Handles password hashing and verification using Argon2.

```rust
pub struct PasswordManager {
    // Argon2 configuration
}

impl PasswordManager {
    pub fn new() -> Self;
    pub fn hash_password(&self, password: &str) -> Result<String>;
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool>;
    pub fn check_password_strength(&self, password: &str) -> PasswordStrength;
}
```

**Password Strength Scoring**:
- Length (8+, 12+, 16+ chars)
- Character diversity (uppercase, lowercase, digits, special)
- Returns: VeryWeak, Weak, Medium, Strong, VeryStrong

### `OAuth2Service`

OAuth2 and OIDC client for third-party authentication.

```rust
pub struct OAuth2Service {
    config: OAuth2Config,
    // HTTP client, state storage
}

impl OAuth2Service {
    pub async fn generate_authorization_url(
        &self,
        redirect_uri: &str,
        use_pkce: bool,
    ) -> Result<(String, String)>;

    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse>;

    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenResponse>;

    pub async fn fetch_user_info(
        &self,
        access_token: &str,
    ) -> Result<serde_json::Value>;
}
```

**Provider Presets**:
- `OAuth2Config::github(client_id, client_secret)`
- `OAuth2Config::google(client_id, client_secret)`
- `OAuth2Config::custom(...)` for any provider

## Authentication Flow Examples

### 1. API Token Authentication

```rust
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

pub async fn auth_middleware(
    State(jwt_manager): State<Arc<JwtManager>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract Authorization header
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Extract token
    let token = JwtManager::extract_token_from_header(auth_header)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Validate token
    let validation = jwt_manager
        .validate_token(token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Add user to request extensions
    request.extensions_mut().insert(validation.user);

    Ok(next.run(request).await)
}
```

### 2. OAuth2 Login Flow

```rust
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn oauth_login(
    State(oauth_service): State<Arc<OAuth2Service>>,
) -> impl IntoResponse {
    let (auth_url, state) = oauth_service
        .generate_authorization_url("http://localhost:3000/auth/callback", true)
        .await
        .unwrap();

    // Store state in session for CSRF validation
    Redirect::to(&auth_url)
}

async fn oauth_callback(
    State(oauth_service): State<Arc<OAuth2Service>>,
    State(jwt_manager): State<Arc<JwtManager>>,
    Query(params): Query<CallbackQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Validate CSRF state (implement state storage)

    // Exchange code for tokens
    let tokens = oauth_service
        .exchange_code(&params.code, "http://localhost:3000/auth/callback")
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Fetch user info from provider
    let user_info = oauth_service
        .fetch_user_info(&tokens.access_token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Create internal user from OAuth user info
    let user = User {
        username: user_info["login"].as_str().unwrap().to_string(),
        email: user_info["email"].as_str().map(String::from),
        // ... populate other fields
        roles: vec!["user".to_string()],
        permissions: vec![Permission::Read],
        full_name: user_info["name"].as_str().map(String::from),
        last_login: Some(chrono::Utc::now()),
    };

    // Generate JWT for internal use
    let jwt = jwt_manager
        .generate_token(&user)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "token": jwt,
        "user": user,
    })))
}
```

### 3. Password-Based Login

```rust
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    user: User,
}

async fn login(
    State(password_manager): State<Arc<PasswordManager>>,
    State(jwt_manager): State<Arc<JwtManager>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    // Fetch user from database (implement your user storage)
    // let stored_user = db.get_user(&req.username).await?;

    // Verify password
    // let is_valid = password_manager
    //     .verify_password(&req.password, &stored_user.password_hash)
    //     .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // if !is_valid {
    //     return Err(StatusCode::UNAUTHORIZED);
    // }

    // Create user object (mock for example)
    let user = User {
        username: req.username.clone(),
        roles: vec!["user".to_string()],
        email: Some(format!("{}@example.com", req.username)),
        full_name: Some(req.username.clone()),
        last_login: Some(chrono::Utc::now()),
        permissions: vec![Permission::Read, Permission::Write],
    };

    // Generate JWT
    let token = jwt_manager
        .generate_token(&user)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LoginResponse { token, user }))
}
```

### 4. SAML 2.0 Authentication

```rust
use oxify_authn::{ServiceProvider, SpConfigBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure SAML Service Provider
    let sp_config = SpConfigBuilder::new(
        "https://myapp.example.com/saml/metadata",
        "https://myapp.example.com/saml/acs",
    )
    .entity_id("https://myapp.example.com")
    .require_response_signature(true)
    .require_assertion_signature(true)
    .build()?;

    let sp = ServiceProvider::new(sp_config);

    // Generate AuthnRequest
    let (authn_request, relay_state) = sp.generate_authn_request(
        "https://idp.example.com/saml/sso",
        None,
    )?;

    // Build redirect URL
    let redirect_url = sp.build_redirect_url(&authn_request, &relay_state)?;
    println!("Redirect to: {}", redirect_url);

    // Later, in the ACS endpoint, validate the SAML Response
    // let assertion = sp.validate_response(saml_response_xml)?;
    // let user_email = assertion.attributes.get("email");

    Ok(())
}
```

### 5. LDAP/Active Directory Authentication

```rust
use oxify_authn::{LdapAuthenticator, LdapConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure LDAP with connection pooling
    let config = LdapConfig::builder()
        .url("ldaps://ldap.example.com:636")
        .base_dn("dc=example,dc=com")
        .bind_dn("cn=admin,dc=example,dc=com")
        .bind_password("admin_password")
        .user_search_filter("(&(objectClass=person)(uid={username}))")
        .add_user_attribute("mail")
        .add_user_attribute("cn")
        .add_user_attribute("uid")
        .use_tls(true)
        .timeout_seconds(30)
        .max_connections(20)  // Connection pool size (default: 10)
        .build()?;

    // Authenticator uses connection pooling to limit concurrent connections
    let ldap_auth = LdapAuthenticator::new(config);

    // Authenticate user
    let ldap_user = ldap_auth.authenticate("alice", "password123").await?;
    println!("Authenticated: {} ({})", ldap_user.username, ldap_user.email);

    // Get user groups
    let groups = ldap_auth.get_user_groups("alice").await?;
    println!("Groups: {:?}", groups);

    Ok(())
}
```

### 6. WebAuthn/FIDO2 Passwordless Authentication

```rust
use oxify_authn::{WebAuthnAuthenticator, WebAuthnConfigBuilder, InMemoryCredentialStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = WebAuthnConfigBuilder::new(
        "example.com",
        "https://example.com",
    )
    .user_verification_required(true)
    .build()?;

    let credential_store = InMemoryCredentialStore::new();
    let webauthn = WebAuthnAuthenticator::new(config, credential_store);

    // Registration flow
    let user_id = uuid::Uuid::new_v4();
    let (registration_challenge, state) = webauthn.start_registration(
        &user_id,
        "alice",
        "Alice Wonderland",
    )?;

    // Send registration_challenge to client
    // Client creates credential and returns response
    // webauthn.finish_registration(user_id, response, state)?;

    // Authentication flow
    let (auth_challenge, state) = webauthn.start_authentication(&user_id)?;
    // Send auth_challenge to client
    // Client signs challenge and returns response
    // webauthn.finish_authentication(user_id, response, state)?;

    Ok(())
}
```

### 7. Risk-Based Authentication

```rust
use oxify_authn::{RiskAnalyzer, RiskConfig, LoginContextBuilder, RiskLevel};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = RiskConfig::default();
    let mut analyzer = RiskAnalyzer::new(config);

    // Analyze login attempt
    let context = LoginContextBuilder::new("alice")
        .ip("203.0.113.45")
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .location("New York", "US", 40.7128, -74.0060)
        .build();

    let assessment = analyzer.analyze_login(&context)?;

    match assessment.risk_level {
        RiskLevel::High | RiskLevel::Critical => {
            println!("High risk login! Require step-up authentication");
            println!("Reasons: {:?}", assessment.reasons);
        }
        _ => {
            println!("Normal risk level, proceed with login");
        }
    }

    Ok(())
}
```

### 8. API Key Management

```rust
use oxify_authn::{ApiKeyManager, ApiKeyConfig, ApiKeyScope};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ApiKeyConfig {
        prefix: "sk_live_".to_string(),
        default_expiration_secs: 86400 * 30, // 30 days
        ..Default::default()
    };

    let mut manager = ApiKeyManager::new(config);

    // Generate API key
    let generated = manager.generate_key(
        "user123",
        "Production API Key",
        vec![ApiKeyScope::Read, ApiKeyScope::Write],
        None, // No expiration
        None, // No rate limit
        None, // No IP whitelist
    )?;

    println!("API Key: {}", generated.key);
    println!("Store this securely - it won't be shown again!");

    // Validate API key
    let validation = manager.validate_key(&generated.key)?;
    println!("User: {}, Scopes: {:?}", validation.user_id, validation.scopes);

    Ok(())
}
```

### 9. Security Metrics and Analytics

```rust
use oxify_authn::{MetricsCollector, AuthEvent};
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut collector = MetricsCollector::new();

    // Record authentication events
    collector.record_event(AuthEvent::LoginSuccess {
        user_id: "alice".to_string(),
        ip: "192.0.2.1".to_string(),
    });

    collector.record_event(AuthEvent::LoginFailure {
        username: "bob".to_string(),
        ip: "192.0.2.2".to_string(),
        reason: "Invalid password".to_string(),
    });

    // Get metrics
    let metrics = collector.get_metrics();
    println!("Security Score: {}", metrics.security_score);
    println!("Success Rate: {:.2}%", metrics.success_rate * 100.0);
    println!("Total Logins: {}", metrics.total_logins);
    println!("Failed Logins: {}", metrics.failed_logins);

    // Get time-series data
    let time_series = collector.get_time_series_data(3600)?; // 1 hour intervals
    for point in time_series {
        println!("{}: {} logins", point.timestamp, point.total_events);
    }

    Ok(())
}
```

## Configuration

### Development Config

```rust
let config = JwtConfig::development();
// Uses HS256, "dev_secret_key_change_in_production", 24h expiration
```

### Production Config

```rust
use oxify_authn::{JwtConfig, Algorithm};

let config = JwtConfig {
    algorithm: Algorithm::RS256,
    secret: std::env::var("JWT_SECRET")?,
    issuer: "oxify.io".to_string(),
    audience: vec!["api.oxify.io".to_string()],
    expiration_secs: 3600,  // 1 hour
};
```

## Security Best Practices

1. **JWT Secrets**: Use strong, random secrets in production (min 32 bytes for HS256)
2. **Token Expiration**: Keep JWT expiration short (1-24 hours) and use refresh tokens
3. **HTTPS Only**: Always use HTTPS in production for token transmission
4. **Password Policy**: Enforce minimum strength requirements
5. **PKCE**: Always use PKCE for OAuth2 public clients (SPAs, mobile apps)
6. **CSRF Protection**: Validate OAuth2 state parameter to prevent CSRF attacks
7. **Rate Limiting**: Implement rate limiting on authentication endpoints
8. **Audit Logging**: Log all authentication events for security monitoring

## Testing

Run the comprehensive test suite:

```bash
cd crates/security/oxify-authn

# Run all tests (154 unit tests + 15 doc tests)
cargo test --all-features

# Run specific feature tests
cargo test --features "jwt,password"
cargo test --features "saml,ldap"
cargo test --features "webauthn,risk,ai"

# Run benchmarks
cargo bench --all-features

# Run examples
cargo run --example simple_auth --all-features
```

**Test Coverage:**
- ✅ 156 unit tests (100% passing)
- ✅ 15 doc tests (100% passing)
- ✅ Zero warnings policy enforced
- ✅ Clippy clean
- ✅ All features tested with integration examples

## Performance

Run benchmarks with `cargo bench --all-features`:

- **JWT generation**: <1ms (HS256), ~2ms (RS256)
- **JWT validation**: <1ms (HS256), ~1ms (RS256)
- **Password hashing** (Argon2): ~100ms (intentionally slow for security)
- **Password verification**: ~100ms
- **Session creation**: <1ms (in-memory)
- **Session validation**: <1ms (in-memory)
- **Rate limiting check**: <1µs (in-memory)
- **API key validation**: <1ms (SHA256 hash)
- **Metrics collection**: <10µs per event
- **Risk analysis**: ~1ms (with history)
- **OAuth2 token exchange**: 200-500ms (network dependent)
- **SAML AuthnRequest generation**: ~2ms
- **LDAP authentication**: 50-200ms (network dependent)

See `benches/auth_benchmarks.rs` for detailed benchmark suite.

## Dependencies

Core dependencies:
- `jsonwebtoken` - JWT encoding/decoding (HS256/RS256/ES256)
- `argon2` - Argon2id password hashing
- `oauth2` - OAuth2/OIDC client
- `reqwest` - HTTP client for OAuth2 and IdP integration
- `chrono` - Timestamp handling
- `serde` / `serde_json` - Serialization
- `uuid` - Unique identifier generation

Optional dependencies (by feature):
- `totp-rs` - TOTP/MFA support (`mfa`)
- `quick-xml` - SAML XML parsing (`saml`)
- `flate2` - SAML compression (`saml`)
- `ldap3` - LDAP/AD authentication (`ldap`)
- `webauthn-rs` - WebAuthn/FIDO2 support (`webauthn`)
- `url` - URL parsing for SAML and WebAuthn (`saml`, `webauthn`)

All dependencies are production-ready and actively maintained.

## License

Apache-2.0

## Attribution

Ported from [OxiRS](https://github.com/cool-japan/oxirs) with permission. Original implementation by the OxiLabs team.
