# oxify-connect-graphql

**GraphQL Client Connector for OxiFY**

## Overview

`oxify-connect-graphql` provides a zero-dependency, type-safe GraphQL **client** for OxiFY.
It can execute queries and mutations against any spec-compliant GraphQL endpoint, surface
server-side GraphQL errors as typed Rust errors, and retry on transient failures with
exponential backoff.

The crate ships one built-in back-end (`HttpGraphQlProvider`) and exposes the
`GraphQlExecutor` trait so additional back-ends (mocked, WebSocket, etc.) can be plugged
in transparently.

**Status**: v0.2 — HTTP provider production-ready  
**Roadmap**: WebSocket subscriptions, persisted queries, automatic query batching  
**Part of**: OxiFY Enterprise Architecture (Codename: Absolute Zero)

## Architecture

```rust
#[async_trait]
pub trait GraphQlExecutor: Send + Sync {
    fn provider_name(&self) -> &str;
    async fn execute(&self, req: GraphQlRequest) -> Result<Value>;

    // Only available with --features introspection
    #[cfg(feature = "introspection")]
    async fn introspect(&self) -> Result<Value>;
}
```

The single trait method `execute` accepts a [`GraphQlRequest`] and returns the raw
`data` value from the GraphQL response as `serde_json::Value`.  GraphQL-level errors
(the `errors` array) are mapped to `GraphQlError::GraphQl` so callers get a typed `Err`
rather than having to inspect the JSON manually.

## Usage

### Minimal example

```rust,no_run
use oxify_connect_graphql::{HttpGraphQlProvider, GraphQlConfig, GraphQlRequest, AuthConfig};
use oxify_connect_graphql::providers::GraphQlExecutor;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = HttpGraphQlProvider::new(GraphQlConfig {
        endpoint: "https://api.example.com/graphql".to_string(),
        auth: AuthConfig::bearer("my-bearer-token"),
        ..GraphQlConfig::default()
    })?;

    let data = provider
        .execute(GraphQlRequest::new("query { me { id name email } }"))
        .await?;

    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}
```

### Query with variables

```rust,no_run
use oxify_connect_graphql::{HttpGraphQlProvider, GraphQlConfig, GraphQlRequest, AuthConfig};
use oxify_connect_graphql::providers::GraphQlExecutor;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = HttpGraphQlProvider::new(GraphQlConfig {
        endpoint: "https://api.example.com/graphql".to_string(),
        auth: AuthConfig::bearer("my-bearer-token"),
        ..GraphQlConfig::default()
    })?;

    let req = GraphQlRequest {
        query: "query GetUser($id: ID!) { user(id: $id) { name email } }".to_string(),
        variables: json!({ "id": "user-42" }),
        operation_name: Some("GetUser".to_string()),
    };

    let data = provider.execute(req).await?;
    println!("{data}");
    Ok(())
}
```

### Environment-based configuration

```rust,no_run
use oxify_connect_graphql::HttpGraphQlProvider;

// Reads GRAPHQL_ENDPOINT (required) and GRAPHQL_BEARER_TOKEN (optional).
let provider = HttpGraphQlProvider::from_env()?;
```

### Introspection

```toml
# Cargo.toml
[dependencies]
oxify-connect-graphql = { version = "0.2", features = ["introspection"] }
```

```rust,no_run
#[cfg(feature = "introspection")]
{
    use oxify_connect_graphql::providers::GraphQlExecutor;
    let schema = provider.introspect().await?;
    println!("{}", serde_json::to_string_pretty(&schema)?);
}
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| *(none)* | yes | Core HTTP provider: queries, mutations, auth, retry |
| `introspection` | no | Adds `GraphQlExecutor::introspect()` — schema discovery |

### Custom Authentication

Five auth strategies are supported out of the box:

```rust
use oxify_connect_graphql::AuthConfig;

// Bearer token
let auth = AuthConfig::bearer("ey...");

// API key in header
let auth = AuthConfig::api_key("X-Api-Key", "abc123");

// API key as query parameter
let auth = AuthConfig::ApiKey {
    key: "api_key".to_string(),
    value: "abc123".to_string(),
    in_header: false,
};

// HTTP Basic
let auth = AuthConfig::basic("user", "pass");

// Arbitrary single header
let auth = AuthConfig::Custom {
    header: "X-Tenant-ID".to_string(),
    value: "acme-corp".to_string(),
};
```

### GraphQL Error Inspection

When a server returns a 200 response but includes an `errors` array, the crate
surfaces this as a typed `GraphQlError::GraphQl` rather than a successful `Ok`.
This means caller code stays clean:

```rust,no_run
match provider.execute(req).await {
    Ok(data)  => { /* use data */ }
    Err(GraphQlError::GraphQl(msg)) => eprintln!("server said: {msg}"),
    Err(GraphQlError::Http(msg))    => eprintln!("transport: {msg}"),
    Err(e)                          => eprintln!("other: {e}"),
}
```

### Retry with Exponential Backoff

The default `RetryConfig` retries up to 3 times on status codes 408, 429, 500,
502, 503, 504 and on transport-level errors, with 100 ms initial delay doubling
up to 5 s:

```rust
use oxify_connect_graphql::{GraphQlConfig, RetryConfig};

let cfg = GraphQlConfig {
    retry: Some(RetryConfig {
        max_retries: 5,
        initial_delay_ms: 200,
        max_delay_ms: 10_000,
        backoff_multiplier: 2.0,
        retry_status_codes: vec![429, 503],
    }),
    ..GraphQlConfig::default()
};
```

Disable retries entirely by setting `retry: None`.

## Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `endpoint` | `String` | `""` | Full GraphQL endpoint URL |
| `auth` | `AuthConfig` | `None` | Authentication strategy |
| `timeout_secs` | `u64` | `30` | Per-request timeout |
| `default_headers` | `HashMap<String,String>` | `{}` | Headers added to every request |
| `retry` | `Option<RetryConfig>` | `Some(RetryConfig::default())` | Retry policy |

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `GRAPHQL_ENDPOINT` | Yes | Full URL of the GraphQL API |
| `GRAPHQL_BEARER_TOKEN` | No | Bearer token; sets `AuthConfig::Bearer` |

## Error Handling

```rust
use oxify_connect_graphql::GraphQlError;

match err {
    GraphQlError::Http(msg)           => { /* HTTP ≥ 400 */ }
    GraphQlError::Auth(msg)           => { /* credential problem */ }
    GraphQlError::Serialization(msg)  => { /* JSON parse failure */ }
    GraphQlError::GraphQl(msg)        => { /* errors[] from server */ }
    GraphQlError::Config(msg)         => { /* missing env vars etc. */ }
    GraphQlError::Transport(msg)      => { /* network / TLS failure */ }
}
```

## Testing

The test suite uses [wiremock](https://crates.io/crates/wiremock) to spin up a
local mock HTTP server without any external network access.

```bash
# All tests
cargo test -p oxify-connect-graphql

# Include introspection tests
cargo test -p oxify-connect-graphql --features introspection

# Lint
cargo clippy -p oxify-connect-graphql --all-features -- -D warnings
```

Tests are organized per module:

| Module | What is tested |
|--------|----------------|
| `errors` | Display output of every error variant |
| `types` | Serde round-trips, constructor helpers, defaults |
| `providers::http` | Unit tests (config, env, auth helpers) + wiremock integration tests |

## Future Enhancements

- **WebSocket subscriptions** — `graphql-ws` protocol over `tokio-tungstenite`
- **Persisted queries** — send query hash instead of full query text
- **Automatic batching** — collect multiple requests into a single HTTP call
- **Response caching** — LRU cache with configurable TTL for idempotent queries
- **Schema validation** — validate requests against a cached introspection schema
- **File upload** — multipart form per the GraphQL multipart request spec

## See Also

- [`oxify-connect-comm`](../oxify-connect-comm) — Slack, SMTP and messaging connectors
- [`oxify-connect-llm`](../oxify-connect-llm) — LLM provider integrations (OpenAI, Anthropic)
- [`oxify-connect-vector`](../oxify-connect-vector) — Vector database connectors
- [`oxify-engine`](../oxify-engine) — Generic REST connector and engine primitives
