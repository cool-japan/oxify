# oxify-server

Production-ready HTTP server foundation for OxiFY, built on Axum and Tower.

## Overview

`oxify-server` provides a robust, batteries-included HTTP server runtime for building LLM workflow APIs. It handles the operational concerns (logging, tracing, graceful shutdown, middleware) so you can focus on building workflow endpoints.

**Ported from**: [OxiRS](https://github.com/cool-japan/oxirs) - Battle-tested in production semantic web applications.

## Features

- **Axum Framework**: Modern, ergonomic async web framework
- **Tower Middleware**: Composable middleware stack
- **Graceful Shutdown**: Signal handling (SIGINT, SIGTERM)
- **Request Tracing**: Automatic request ID generation and propagation
- **Structured Logging**: Integration with tracing ecosystem
- **CORS Support**: Configurable cross-origin resource sharing
- **Compression**: Gzip, Brotli, Deflate (optional feature)
- **Authentication**: Built-in JWT middleware integration
- **Production-Ready**: Comprehensive configuration and error handling

## Installation

```toml
[dependencies]
oxify-server = { path = "../crates/api/oxify-server" }

# Or with features
oxify-server = { path = "../crates/api/oxify-server", features = ["compression", "cors"] }
```

### Feature Flags

- `compression` (default): HTTP response compression (gzip, brotli, deflate)
- `cors` (default): CORS middleware support

## Quick Start

### Basic Server

```rust
use oxify_server::{ServerConfig, ServerRuntime};
use axum::{routing::get, Router, Json};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create router
    let app = Router::new()
        .route("/", get(|| async {
            Json(json!({"status": "ok"}))
        }));

    // Configure and run server
    let config = ServerConfig::development();
    let server = ServerRuntime::new(config).with_router(app);

    server.run().await?;
    Ok(())
}
```

### With Authentication

```rust
use oxify_server::{ServerConfig, ServerRuntime, auth_middleware};
use oxify_authn::{JwtManager, JwtConfig};
use axum::{
    routing::{get, post},
    Router,
    middleware,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize JWT manager
    let jwt_config = JwtConfig::development();
    let jwt_manager = Arc::new(JwtManager::new(&jwt_config)?);

    // Build router with protected routes
    let app = Router::new()
        .route("/", get(public_handler))
        .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                jwt_manager.clone(),
                auth_middleware,
            ))
        .route("/login", post(login_handler))
        .with_state(jwt_manager);

    // Run server
    let config = ServerConfig::development();
    let server = ServerRuntime::new(config).with_router(app);

    server.run().await?;
    Ok(())
}

async fn public_handler() -> &'static str {
    "Public endpoint"
}

async fn protected_handler() -> &'static str {
    "Protected endpoint - requires valid JWT"
}

async fn login_handler() -> &'static str {
    "Login endpoint"
}
```

## Core Components

### `ServerRuntime`

Main server runtime that manages the Axum server lifecycle.

```rust
pub struct ServerRuntime {
    config: ServerConfig,
    router: Option<Router>,
}

impl ServerRuntime {
    pub fn new(config: ServerConfig) -> Self;
    pub fn with_router(self, router: Router) -> Self;
    pub async fn run(self) -> Result<()>;
}
```

### `ServerConfig`

Server configuration with sensible defaults.

```rust
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub request_logging: bool,
    pub request_id_header: String,
    pub cors: bool,
    pub compression: bool,
    pub graceful_shutdown_timeout_secs: u64,
}

impl ServerConfig {
    pub fn development() -> Self;  // localhost:3000, all features enabled
    pub fn production() -> Self;   // 0.0.0.0:8080, production settings
}
```

**Development Config**:
- Host: `127.0.0.1`
- Port: `3000`
- Request logging: enabled
- CORS: enabled (all origins)
- Compression: enabled

**Production Config**:
- Host: `0.0.0.0`
- Port: `8080`
- Request logging: enabled
- CORS: disabled (configure manually)
- Compression: enabled
- Graceful shutdown: 30s timeout

## Middleware

### Request ID Middleware

Automatically adds a unique request ID to each request.

```rust
use oxify_server::request_id_middleware;

let app = Router::new()
    .route("/", get(handler))
    .layer(axum::middleware::from_fn(request_id_middleware));
```

Request ID is available in:
- Response header: `x-request-id`
- Request extensions: `request.extensions().get::<RequestId>()`

### Logging Middleware

Logs request/response details with structured tracing.

```rust
use oxify_server::logging_middleware;

let app = Router::new()
    .route("/", get(handler))
    .layer(axum::middleware::from_fn(logging_middleware));
```

Logs include:
- Method and path
- Status code
- Duration
- Request ID

### Authentication Middleware

JWT-based authentication middleware.

```rust
use oxify_server::auth_middleware;
use oxify_authn::JwtManager;
use std::sync::Arc;

let jwt_manager = Arc::new(JwtManager::new(&config)?);

let app = Router::new()
    .route("/protected", get(handler))
    .layer(middleware::from_fn_with_state(
        jwt_manager.clone(),
        auth_middleware,
    ));
```

Public routes (skip authentication):
- `/health`
- `/metrics`
- `/login`
- `/register`

### CORS Middleware

Cross-origin resource sharing configuration.

```rust
use oxify_server::cors_layer;

let app = Router::new()
    .route("/", get(handler))
    .layer(cors_layer());
```

Default CORS settings:
- Allow all origins (development)
- Allow methods: GET, POST, PUT, DELETE, OPTIONS
- Allow headers: Content-Type, Authorization
- Max age: 3600 seconds

## Complete Example

```rust
use oxify_server::{ServerConfig, ServerRuntime};
use oxify_authn::{JwtManager, JwtConfig, User, Permission};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::Level;

#[derive(Clone)]
struct AppState {
    jwt_manager: Arc<JwtManager>,
}

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    // Initialize state
    let jwt_config = JwtConfig::development();
    let jwt_manager = Arc::new(JwtManager::new(&jwt_config)?);
    let app_state = AppState { jwt_manager };

    // Build application
    let app = Router::new()
        .route("/", get(root))
        .route("/login", post(login))
        .with_state(app_state);

    // Run server
    let config = ServerConfig::development();
    let server = ServerRuntime::new(config).with_router(app);

    tracing::info!("Server starting at http://127.0.0.1:3000");
    server.run().await?;

    Ok(())
}

async fn root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "OxiFY API",
        "version": "0.1.0",
        "status": "running"
    }))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    // In production, verify password against database
    let user = User {
        username: req.username.clone(),
        roles: vec!["user".to_string()],
        email: Some(format!("{}@example.com", req.username)),
        full_name: Some(req.username.clone()),
        last_login: Some(chrono::Utc::now()),
        permissions: vec![Permission::Read, Permission::Write],
    };

    let token = state.jwt_manager
        .generate_token(&user)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LoginResponse { token, user }))
}
```

## Graceful Shutdown

The server automatically handles shutdown signals:

```bash
# Send SIGINT (Ctrl+C)
kill -INT <pid>

# Send SIGTERM
kill -TERM <pid>
```

Shutdown sequence:
1. Stop accepting new connections
2. Wait for active requests to complete (up to timeout)
3. Force shutdown after timeout

Configure timeout:

```rust
let config = ServerConfig {
    graceful_shutdown_timeout_secs: 60,  // 60 second timeout
    ..ServerConfig::default()
};
```

## Production Deployment

### Docker

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/oxify-api /usr/local/bin/
EXPOSE 8080
CMD ["oxify-api"]
```

### Environment Variables

```bash
# Server configuration
export OXIFY_HOST="0.0.0.0"
export OXIFY_PORT="8080"
export OXIFY_LOG_LEVEL="info"

# Authentication
export JWT_SECRET="your-secret-key-min-32-bytes"
export JWT_EXPIRATION_SECS="3600"

# Database
export DATABASE_URL="postgresql://user:pass@localhost/oxify"
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: oxify-api
spec:
  replicas: 3
  selector:
    matchLabels:
      app: oxify-api
  template:
    metadata:
      labels:
        app: oxify-api
    spec:
      containers:
      - name: oxify-api
        image: oxify/api:latest
        ports:
        - containerPort: 8080
        env:
        - name: OXIFY_HOST
          value: "0.0.0.0"
        - name: OXIFY_PORT
          value: "8080"
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: oxify-secrets
              key: jwt-secret
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
```

## Monitoring

### Health Check Endpoint

```rust
use axum::{routing::get, Router, Json};

let app = Router::new()
    .route("/health", get(health_check));

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}
```

### Metrics (Future)

Integration with Prometheus metrics is planned:

```rust
use oxify_server::metrics_middleware;

let app = Router::new()
    .route("/", get(handler))
    .layer(metrics_middleware());

// Exposes /metrics endpoint with:
// - http_requests_total
// - http_request_duration_seconds
// - http_requests_in_flight
```

## Testing

Run the test suite:

```bash
cd crates/api/oxify-server
cargo test

# Run with all features
cargo test --all-features
```

## Performance

- Startup time: <100ms
- Request overhead: <1ms (middleware processing)
- Graceful shutdown: <30s (configurable)
- Concurrent connections: Limited by Tokio runtime (default: CPU cores * 2)

## Dependencies

Core dependencies:
- `axum` - Web framework
- `tower` / `tower-http` - Middleware
- `tokio` - Async runtime
- `tracing` - Structured logging

Integration:
- `oxify-authn` - Authentication middleware
- `oxify-authz` - Authorization (future)

## License

Apache-2.0

## Attribution

Ported from [OxiRS](https://github.com/cool-japan/oxirs) with permission. Original implementation by the OxiLabs team.
