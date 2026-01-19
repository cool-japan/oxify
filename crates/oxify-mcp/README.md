# oxify-mcp

**Model Context Protocol (MCP) Implementation for Oxify**

A comprehensive Rust implementation of the Model Context Protocol, providing both client and server capabilities with built-in servers for common operations.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-86%20passing-brightgreen.svg)](#testing)

---

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Quick Start](#quick-start)
- [Built-in Servers](#built-in-servers)
- [Authentication](#authentication)
- [Load Balancing](#load-balancing)
- [Custom MCP Servers](#custom-mcp-servers)
- [Integration](#integration)
- [Examples](#examples)
- [Testing](#testing)
- [License](#license)

---

## Overview

The **Model Context Protocol (MCP)** is a standardized protocol for exposing tools and capabilities to AI models. `oxify-mcp` provides:

- **MCP Client**: Connect to external MCP servers via HTTP or stdio
- **MCP Server Trait**: Build custom MCP servers with tool discovery and execution
- **Server Registry**: Manage multiple MCP servers with load balancing and failover
- **Built-in Servers**: Pre-built servers for filesystem, git, shell, web, database, and workflow operations
- **Authentication**: Support for API keys, basic auth, bearer tokens, and custom headers
- **Transport Layer**: HTTP and stdio transports with configurable timeouts and response limits

---

## Features

### ✅ Core MCP Protocol
- Full MCP specification support (JSON-RPC over HTTP/stdio)
- Tool discovery (`list_tools`) with JSON Schema validation
- Tool invocation (`call_tool`) with typed parameters
- Comprehensive error handling

### ✅ Server Registry
- Manage multiple MCP servers simultaneously
- Dynamic server registration/unregistration
- Tool discovery across all registered servers
- Server statistics and health tracking

### ✅ Load Balancing & Failover
- **Strategies**: Round-robin, least connections, random, weighted
- **Health Tracking**: Automatic degradation detection (Healthy/Degraded/Unhealthy)
- **Metrics**: Request/error counts, average response times
- **Failover**: Automatic retry with backup servers
- **Tag-based Selection**: Route tools to specific server groups

### ✅ Authentication
- **API Key**: Header-based API key authentication (with optional prefix)
- **Basic Auth**: HTTP Basic authentication (username:password)
- **Bearer Token**: OAuth2/JWT bearer tokens
- **OAuth2**: Full OAuth2 support with client credentials, authorization code, and refresh token flows
  - Automatic token refresh with expiry tracking
  - PKCE support for authorization code flow
  - Scope management
- **Custom Headers**: Arbitrary custom authentication headers
- **Multi-Server**: Different credentials per server via `CredentialStore`

### ✅ Built-in Servers
- **Filesystem**: Read, write, list, delete, check existence (with sandboxing)
- **Shell**: Execute commands, find binaries (with command whitelisting)
- **Git**: Status, clone, commit, push, pull, log, diff, branch operations
- **Web**: HTTP GET/POST, web scraping
- **Database**: PostgreSQL queries, commands, transactions (requires `database` feature)
- **Workflow**: Expose oxify workflows as MCP tools

### ✅ Security
- Path traversal prevention (filesystem operations)
- Command whitelisting (shell operations)
- Repository path validation (git operations)
- Read-only mode (database operations)
- Response size limits (10MB default)
- Request/response validation

---

## Quick Start

Add `oxify-mcp` to your `Cargo.toml`:

```toml
[dependencies]
oxify-mcp = { path = "../oxify-mcp" }

# For database support:
oxify-mcp = { path = "../oxify-mcp", features = ["database"] }
```

### Basic Usage

```rust
use oxify_mcp::{McpRegistry, McpClient, DefaultMcpClient, McpServer};
use oxify_mcp::servers::{FilesystemServer, ShellServer};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a registry
    let mut registry = McpRegistry::new();

    // Register built-in servers
    registry.register_local_server(
        "filesystem",
        Box::new(FilesystemServer::new("/tmp")?),
    ).await?;

    registry.register_local_server(
        "shell",
        Box::new(ShellServer::new(vec!["ls", "pwd", "echo"])),
    ).await?;

    // List all available tools
    let tools = registry.list_all_tools().await?;
    println!("Available tools: {}", tools.len());

    // Execute a tool
    let result = registry.invoke_tool(
        "fs_read",
        json!({ "path": "example.txt" }),
    ).await?;

    println!("Result: {}", result);

    Ok(())
}
```

### Connecting to External MCP Server

```rust
use oxify_mcp::{DefaultMcpClient, McpClient};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect via HTTP
    let client = DefaultMcpClient::http("http://localhost:8080/mcp", None)?;

    // List tools
    let tools = client.list_tools().await?;

    // Call a tool
    let result = client.call_tool(
        "some_tool",
        json!({ "param": "value" }),
    ).await?;

    Ok(())
}
```

---

## Built-in Servers

### FilesystemServer

Provides secure filesystem operations with sandboxing.

```rust
use oxify_mcp::servers::FilesystemServer;
use serde_json::json;

// Create server with base directory
let server = FilesystemServer::new("/home/user/workspace")?;

// Read file
let content = server.call_tool("fs_read", json!({
    "path": "example.txt"
})).await?;

// Write file
server.call_tool("fs_write", json!({
    "path": "output.txt",
    "content": "Hello, world!"
})).await?;

// List directory
let files = server.call_tool("fs_list", json!({
    "path": "."
})).await?;
```

**Tools**: `fs_read`, `fs_write`, `fs_list`, `fs_delete`, `fs_exists`

**Security**: Prevents path traversal attacks, enforces base directory sandboxing.

### ShellServer

Execute shell commands with whitelisting.

```rust
use oxify_mcp::servers::ShellServer;
use serde_json::json;

// Create server with allowed commands
let server = ShellServer::new(vec!["ls", "pwd", "git", "npm"]);

// Execute command
let result = server.call_tool("shell_exec", json!({
    "command": "ls",
    "args": ["-la"],
    "working_dir": "/tmp"
})).await?;

// Find command
let path = server.call_tool("shell_which", json!({
    "command": "git"
})).await?;
```

**Tools**: `shell_exec`, `shell_which`

**Security**: Only whitelisted commands can be executed.

### GitServer

Git repository operations.

```rust
use oxify_mcp::servers::GitServer;
use serde_json::json;

let server = GitServer::new();

// Get status
let status = server.call_tool("git_status", json!({
    "repo_path": "/path/to/repo"
})).await?;

// Commit changes
server.call_tool("git_commit", json!({
    "repo_path": "/path/to/repo",
    "message": "feat: add new feature"
})).await?;
```

**Tools**: `git_status`, `git_clone`, `git_add`, `git_commit`, `git_push`, `git_pull`, `git_log`, `git_diff`, `git_branch`, `git_checkout`

### WebServer

HTTP operations and web scraping.

```rust
use oxify_mcp::servers::WebServer;
use serde_json::json;

let server = WebServer::new();

// HTTP GET
let html = server.call_tool("http_get", json!({
    "url": "https://example.com"
})).await?;

// HTTP POST
server.call_tool("http_post", json!({
    "url": "https://api.example.com/data",
    "body": "{\"key\": \"value\"}",
    "headers": {"Content-Type": "application/json"}
})).await?;

// Web scraping
let text = server.call_tool("web_scrape", json!({
    "url": "https://example.com",
    "selector": "h1"
})).await?;
```

**Tools**: `http_get`, `http_post`, `web_scrape`, `web_screenshot` (not yet implemented)

### DatabaseServer

PostgreSQL database operations (requires `database` feature).

```rust
use oxify_mcp::servers::{DatabaseServer, DatabaseConfig};
use serde_json::json;

// Create server
let config = DatabaseConfig::postgres("postgres://user:pass@localhost/db")
    .with_max_connections(5)
    .with_read_only(false);

let server = DatabaseServer::new(config).await?;

// Query
let results = server.call_tool("db_query", json!({
    "sql": "SELECT * FROM users WHERE age > 18"
})).await?;

// Execute
server.call_tool("db_execute", json!({
    "sql": "INSERT INTO users (name, age) VALUES ('Alice', 25)"
})).await?;

// Transaction
server.call_tool("db_transaction", json!({
    "statements": [
        {"sql": "UPDATE accounts SET balance = balance - 100 WHERE id = 1"},
        {"sql": "UPDATE accounts SET balance = balance + 100 WHERE id = 2"}
    ]
})).await?;

// Describe table
let schema = server.call_tool("db_describe", json!({
    "table": "users"
})).await?;
```

**Tools**: `db_query`, `db_execute`, `db_transaction`, `db_describe`, `db_tables`

**Features**:
- Connection pooling
- Read-only mode
- Transaction support with rollback
- Type conversion (bool, int, float, text, uuid, timestamp, json, bytea)
- Result truncation (configurable max rows)

### WorkflowServer

Expose oxify workflows as MCP tools.

```rust
use oxify_mcp::servers::WorkflowServer;
use oxify_model::Workflow;

let mut server = WorkflowServer::new();

// Register workflow
let workflow = Workflow {
    id: "sentiment_analysis".to_string(),
    name: "Sentiment Analysis".to_string(),
    description: Some("Analyze sentiment of text".to_string()),
    // ... workflow definition
};

server.register_workflow(workflow, None)?;

// Now "sentiment_analysis" tool is available
let result = server.call_tool("sentiment_analysis", json!({
    "text": "I love this product!"
})).await?;
```

**Features**:
- Automatic tool schema generation from workflow metadata
- Template variable extraction for input parameters
- Custom input schemas and descriptions
- Pluggable executor function

---

## Authentication

### API Key Authentication

```rust
use oxify_mcp::auth::{ApiKeyAuth, AuthenticatedHttpTransport};

let auth = ApiKeyAuth::new("my-api-key")
    .with_prefix("Bearer"); // Optional: "Bearer my-api-key"

let transport = AuthenticatedHttpTransport::new(
    "http://localhost:8080/mcp",
    auth.into(),
    None, // timeout
)?;

let client = DefaultMcpClient::new(Box::new(transport));
```

### Basic Authentication

```rust
use oxify_mcp::auth::{BasicAuth, AuthenticatedHttpTransport};

let auth = BasicAuth::new("username", "password");

let transport = AuthenticatedHttpTransport::new(
    "http://localhost:8080/mcp",
    auth.into(),
    None,
)?;
```

### Bearer Token

```rust
use oxify_mcp::auth::{BearerAuth, AuthenticatedHttpTransport};

let auth = BearerAuth::new("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...");

let transport = AuthenticatedHttpTransport::new(
    "http://localhost:8080/mcp",
    auth.into(),
    None,
)?;
```

### Custom Headers

```rust
use oxify_mcp::auth::{CustomHeaderAuth, AuthenticatedHttpTransport};
use std::collections::HashMap;

let mut headers = HashMap::new();
headers.insert("X-Custom-Auth".to_string(), "secret123".to_string());
headers.insert("X-API-Version".to_string(), "v1".to_string());

let auth = CustomHeaderAuth::new(headers);

let transport = AuthenticatedHttpTransport::new(
    "http://localhost:8080/mcp",
    auth.into(),
    None,
)?;
```

### OAuth2 Authentication

#### Client Credentials Flow

```rust
use oxify_mcp::auth::{OAuth2Auth, AuthConfig, AuthenticatedHttpTransport};

// Create OAuth2 auth with client credentials
let mut auth = OAuth2Auth::client_credentials(
    "https://auth.example.com/oauth/token",
    "your-client-id",
    "your-client-secret",
).with_scopes(vec!["read".to_string(), "write".to_string()]);

// Request initial token
auth.request_token().await?;

// Create authenticated transport
let config = AuthConfig {
    method: AuthMethod::OAuth2(auth),
    scopes: Vec::new(),
};

let transport = AuthenticatedHttpTransport::new(
    "http://localhost:8080/mcp",
    config,
    None,
)?;
```

#### Authorization Code Flow (with PKCE)

```rust
use oxify_mcp::auth::OAuth2Auth;

let mut auth = OAuth2Auth::authorization_code(
    "https://auth.example.com/oauth/token",
    "your-client-id",
    None,  // No client secret (PKCE)
    "authorization-code-from-redirect",
)
.with_pkce("code-verifier-123")  // PKCE support
.with_scopes(vec!["read".to_string()]);

// Exchange authorization code for tokens
auth.request_token().await?;
```

#### Token Refresh

```rust
use oxify_mcp::auth::OAuth2Auth;

// Create OAuth2 auth with existing tokens
let mut auth = OAuth2Auth::with_tokens(
    "https://auth.example.com/oauth/token",
    "your-client-id",
    Some("your-client-secret".to_string()),
    "existing-access-token",
    "existing-refresh-token",
);

// Automatically refresh if expired
let refreshed = auth.refresh_if_needed().await?;
if refreshed {
    println!("Token was refreshed");
}

// Check if token is expired
if auth.is_token_expired() {
    // Token needs refresh
}
```

### Multi-Server Credentials

```rust
use oxify_mcp::auth::{CredentialStore, ApiKeyAuth, BasicAuth};

let mut store = CredentialStore::new();

// Different credentials for each server
store.set_credentials(
    "server1",
    ApiKeyAuth::new("key1").into(),
);

store.set_credentials(
    "server2",
    BasicAuth::new("user", "pass").into(),
);

// Use when registering servers
if let Some(creds) = store.get_credentials("server1") {
    // Create authenticated transport with creds
}
```

---

## Load Balancing

### Round-Robin

```rust
use oxify_mcp::{McpRegistry, LoadBalancingStrategy};

let mut registry = McpRegistry::new();
registry.set_load_balancing_strategy(LoadBalancingStrategy::RoundRobin);

// Register multiple servers for the same tool
// Calls will be distributed evenly
```

### Least Connections

```rust
registry.set_load_balancing_strategy(LoadBalancingStrategy::LeastConnections);
// Routes to server with fewest active connections
```

### Weighted Distribution

```rust
use oxify_mcp::{LoadBalancingStrategy, ServerWeight};

registry.set_load_balancing_strategy(LoadBalancingStrategy::Weighted);

// Set server weights (higher = more traffic)
registry.set_server_weight("server1", 3); // 75% of traffic
registry.set_server_weight("server2", 1); // 25% of traffic
```

### Failover

```rust
// Automatic failover to backup servers
let result = registry.invoke_tool_with_failover(
    "my_tool",
    json!({"param": "value"}),
).await?;

// If first server fails, tries next available server
```

### Tag-based Selection

```rust
// Tag servers by capability
registry.tag_server("server1", "fast");
registry.tag_server("server2", "slow");

// Route to servers with specific tags
let servers = registry.get_servers_by_tag("fast");
```

---

## Custom MCP Servers

Implement the `McpServer` trait to create custom servers:

```rust
use oxify_mcp::{McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct MyCustomServer {
    // Server state
}

#[async_trait]
impl McpServer for MyCustomServer {
    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "my_tool",
                "description": "Does something useful",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "Input parameter"
                        }
                    },
                    "required": ["input"]
                }
            })
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "my_tool" => {
                let input = arguments
                    .get("input")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        oxify_mcp::McpError::InvalidArgument(
                            "input is required".to_string()
                        )
                    })?;

                // Do something with input
                let result = format!("Processed: {}", input);

                Ok(json!({ "result": result }))
            }
            _ => Err(oxify_mcp::McpError::ToolNotFound(name.to_string())),
        }
    }
}
```

### Best Practices

1. **Tool Naming**: Use lowercase with underscores (e.g., `my_tool`, `process_data`)
2. **JSON Schema**: Provide detailed schemas with descriptions
3. **Error Handling**: Return appropriate `McpError` variants
4. **Validation**: Validate all inputs before processing
5. **Security**: Implement appropriate access controls and sandboxing
6. **Performance**: Use async operations for I/O-bound tasks
7. **Testing**: Write comprehensive unit tests for each tool

---

## Integration

### With oxify-engine

The MCP client is integrated with oxify-engine for workflow execution:

```rust
use oxify_mcp::MCP_EXECUTOR;
use serde_json::json;

// MCP tools are automatically available in workflows
// Priority: Local servers → HTTP MCP → HTTP fallback

// Register local server
MCP_EXECUTOR.register_local_server(
    "my_server",
    Box::new(my_server),
).await?;

// Execute tool from workflow
let result = MCP_EXECUTOR.execute_tool(
    "fs_read",
    json!({"path": "data.txt"}),
).await?;
```

### With oxify-api

The MCP registry is fully integrated with oxify-api, providing REST endpoints:

```bash
# List registered servers with metrics
GET /api/v1/mcp/servers

# List available tools (optionally filtered by server_id)
POST /api/v1/mcp/tools
{
  "server_id": "filesystem"  // optional
}

# Invoke a tool
POST /api/v1/mcp/tools/invoke
{
  "server_id": "filesystem",
  "tool_name": "fs_read",
  "parameters": {"path": "example.txt"}
}

# Get registry statistics
GET /api/v1/mcp/stats

```

---

## Examples

See the `examples/` directory:

- **basic_usage.rs**: Demonstrates all built-in servers
- **transport_example.rs**: HTTP and stdio transports
- **multi_server_orchestration.rs**: Advanced registry usage with load balancing
- **api_integration.rs**: Integration with oxify-api MCP endpoints

Run an example:

```bash
cargo run --example basic_usage
cargo run --example transport_example
cargo run --example api_integration
cargo run --features database --example basic_usage
```

---

## Testing

Run all tests:

```bash
# Without database feature
cargo test

# With database feature
cargo test --features database

# Show test output
cargo test -- --nocapture

# Run specific test
cargo test test_fs_read
```

**Test Coverage**:
- ✅ 86 unit tests passing
- ✅ 0 compilation warnings
- ✅ All servers tested (Filesystem, Shell, Git, Web, Database, Workflow)
- ✅ Transport layer tested (HTTP, stdio)
- ✅ Authentication tested (API key, basic, bearer, OAuth2, custom)
- ✅ Registry tested (registration, discovery, load balancing)
- ✅ Security tested (path traversal, command whitelist, validation)

---

## License

Apache-2.0 - See [LICENSE](../../LICENSE) file for details.

---

## Contributing

Contributions are welcome! Please ensure:

1. All tests pass (`cargo test`)
2. No compilation warnings (`cargo check`)
3. Code is formatted (`cargo fmt`)
4. Clippy is happy (`cargo clippy`)

---

## Changelog

### v0.1.0 (Initial Release)

**Features**:
- Full MCP protocol implementation (client, server, transport)
- 6 built-in servers (Filesystem, Shell, Git, Web, Database, Workflow)
- Authentication (API key, basic, bearer, OAuth2 with refresh, custom headers)
  - OAuth2 client credentials flow
  - OAuth2 authorization code flow with PKCE support
  - Automatic token refresh
- Load balancing (round-robin, least connections, weighted, failover)
- Server registry with health tracking
- 86 comprehensive unit tests
- Zero compilation warnings

**Security**:
- Path traversal prevention
- Command whitelisting
- Response size limits
- Request/response validation

**Performance**:
- Async operations throughout
- Connection pooling (database)
- Configurable timeouts
- Response size limits

---

For more information, see the [TODO.md](TODO.md) for development roadmap and implementation details.
