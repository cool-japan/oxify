//! Multi-Server Orchestration Example for oxify-mcp
//!
//! This example demonstrates advanced MCP features:
//! - Load balancing strategies (Round-robin, Weighted, Least Connections)
//! - Server groups and tag-based routing
//! - Health monitoring and metrics
//! - Failover with automatic retry
//! - Authentication configuration
//!
//! Run this example:
//! ```bash
//! cargo run -p oxify-mcp --example multi_server_orchestration
//! ```

use oxify_mcp::{
    ApiKeyAuth, AuthConfig, AuthMethod, CredentialStore, FilesystemServer, LoadBalanceStrategy,
    McpRegistry, ServerHealth, ShellServer,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("=== OxiFY MCP Multi-Server Orchestration Example ===\n");

    // Demo 1: Load Balancing Strategies
    demo_load_balancing().await?;

    println!("\n{}\n", "=".repeat(70));

    // Demo 2: Server Groups and Tags
    demo_server_groups_and_tags().await?;

    println!("\n{}\n", "=".repeat(70));

    // Demo 3: Health Monitoring and Metrics
    demo_health_monitoring().await?;

    println!("\n{}\n", "=".repeat(70));

    // Demo 4: Failover with Automatic Retry
    demo_failover().await?;

    println!("\n{}\n", "=".repeat(70));

    // Demo 5: Authentication Configuration
    demo_authentication()?;

    println!("\n=== All Multi-Server Demos Complete! ===");
    Ok(())
}

/// Demo 1: Load Balancing Strategies
async fn demo_load_balancing() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 1: Load Balancing Strategies ---\n");

    // Create registry with Round-robin strategy
    let registry = McpRegistry::with_config(
        LoadBalanceStrategy::RoundRobin,
        30, // health check interval
        3,  // unhealthy threshold
        2,  // recovery threshold
    );

    // Create multiple filesystem servers (simulating multiple instances)
    let temp_base = std::env::temp_dir().join("oxify-mcp-lb-demo");

    for i in 1..=3 {
        let server_dir = temp_base.join(format!("server_{}", i));
        std::fs::create_dir_all(&server_dir)?;
        let fs_server = FilesystemServer::new(server_dir);
        registry
            .register(format!("fs_server_{}", i), fs_server)
            .await?;
    }

    println!("Registered 3 filesystem servers\n");

    // Demonstrate Round-robin selection
    println!("Round-Robin Selection (distributes evenly):");
    for i in 1..=6 {
        if let Some(selected) = registry
            .select_server(Some(LoadBalanceStrategy::RoundRobin))
            .await
        {
            println!("  Request {}: Selected -> {}", i, selected);
        }
    }

    // Set different weights for weighted round-robin
    println!("\nSetting server weights: server_1=1, server_2=2, server_3=3");
    registry.set_server_weight("fs_server_1", 1).await?;
    registry.set_server_weight("fs_server_2", 2).await?;
    registry.set_server_weight("fs_server_3", 3).await?;

    // Demonstrate Weighted Round-robin selection
    println!("\nWeighted Round-Robin Selection (proportional to weights):");
    let mut counts = std::collections::HashMap::new();
    for _ in 0..60 {
        if let Some(selected) = registry
            .select_server(Some(LoadBalanceStrategy::WeightedRoundRobin))
            .await
        {
            *counts.entry(selected).or_insert(0) += 1;
        }
    }
    for (server, count) in &counts {
        println!("  {} selected {} times", server, count);
    }

    // Demonstrate Least Connections selection
    println!("\nLeast Connections Strategy:");
    println!("  (Selects server with fewest active connections)");
    if let Some(selected) = registry
        .select_server(Some(LoadBalanceStrategy::LeastConnections))
        .await
    {
        println!("  Selected: {}", selected);
    }

    // Cleanup
    std::fs::remove_dir_all(&temp_base)?;
    Ok(())
}

/// Demo 2: Server Groups and Tags
async fn demo_server_groups_and_tags() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 2: Server Groups and Tags ---\n");

    let registry = McpRegistry::new();
    let temp_base = std::env::temp_dir().join("oxify-mcp-groups-demo");

    // Register servers with different groups
    println!("Setting up servers in different groups:");

    // Production group
    let prod_dir = temp_base.join("prod_fs");
    std::fs::create_dir_all(&prod_dir)?;
    registry
        .register_with_group(
            "prod_fs_1".to_string(),
            FilesystemServer::new(prod_dir),
            "production".to_string(),
        )
        .await?;
    println!("  - prod_fs_1 in group 'production'");

    // Staging group
    let staging_dir = temp_base.join("staging_fs");
    std::fs::create_dir_all(&staging_dir)?;
    registry
        .register_with_group(
            "staging_fs_1".to_string(),
            FilesystemServer::new(staging_dir),
            "staging".to_string(),
        )
        .await?;
    println!("  - staging_fs_1 in group 'staging'");

    // Register servers with tags
    println!("\nSetting up servers with tags:");
    registry
        .register_with_tags(
            "high_perf_shell".to_string(),
            ShellServer::default(),
            vec!["high-performance".to_string(), "critical".to_string()],
        )
        .await?;
    println!("  - high_perf_shell with tags ['high-performance', 'critical']");

    registry
        .register_with_tags(
            "logging_shell".to_string(),
            ShellServer::default(),
            vec!["logging".to_string(), "monitoring".to_string()],
        )
        .await?;
    println!("  - logging_shell with tags ['logging', 'monitoring']");

    // Select from specific groups
    println!("\nGroup-based Selection:");
    if let Some(server) = registry.select_server_from_group("production").await {
        println!("  Production group -> {}", server);
    }
    if let Some(server) = registry.select_server_from_group("staging").await {
        println!("  Staging group -> {}", server);
    }

    // List servers by group
    println!("\nServers in 'production' group:");
    for id in registry.list_servers_in_group("production").await {
        println!("  - {}", id);
    }

    // Select by tag
    println!("\nTag-based Selection:");
    if let Some(server) = registry.select_server_by_tag("high-performance").await {
        println!("  Tag 'high-performance' -> {}", server);
    }
    if let Some(server) = registry.select_server_by_tag("logging").await {
        println!("  Tag 'logging' -> {}", server);
    }

    // List servers by tag
    println!("\nServers with 'critical' tag:");
    for id in registry.list_servers_with_tag("critical").await {
        println!("  - {}", id);
    }

    // Cleanup
    std::fs::remove_dir_all(&temp_base)?;
    Ok(())
}

/// Demo 3: Health Monitoring and Metrics
async fn demo_health_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 3: Health Monitoring and Metrics ---\n");

    let registry = McpRegistry::new();
    let temp_base = std::env::temp_dir().join("oxify-mcp-health-demo");

    // Register servers
    for i in 1..=3 {
        let server_dir = temp_base.join(format!("server_{}", i));
        std::fs::create_dir_all(&server_dir)?;
        registry
            .register(format!("server_{}", i), FilesystemServer::new(server_dir))
            .await?;
    }

    println!("Initial Health Status:");
    let health_status = registry.get_server_health_status().await;
    for (server_id, health) in &health_status {
        println!("  {} -> {:?}", server_id, health);
    }

    // Simulate health changes
    println!("\nSimulating health changes...");
    registry
        .set_server_health("server_2", ServerHealth::Degraded)
        .await?;
    registry
        .set_server_health("server_3", ServerHealth::Unhealthy)
        .await?;

    println!("\nUpdated Health Status:");
    let health_status = registry.get_server_health_status().await;
    for (server_id, health) in &health_status {
        println!("  {} -> {:?}", server_id, health);
    }

    // Simulate some requests to generate metrics
    println!("\nMaking requests to generate metrics...");
    let test_dir = temp_base.join("server_1");
    std::fs::write(test_dir.join("test.txt"), "test data")?;

    for _ in 0..5 {
        let _ = registry
            .invoke_tool(
                "server_1",
                "fs_read",
                json!({
                    "path": "test.txt"
                }),
            )
            .await;
    }

    // Display metrics
    println!("\nServer Metrics:");
    let all_metrics = registry.get_all_server_metrics().await;
    for metrics in &all_metrics {
        println!("\n  {}:", metrics.server_id);
        println!("    Health: {:?}", metrics.health);
        println!("    Weight: {}", metrics.weight);
        println!("    Active Connections: {}", metrics.active_connections);
        println!("    Total Requests: {}", metrics.request_count);
        println!("    Error Count: {}", metrics.error_count);
        println!("    Error Rate: {:.2}%", metrics.error_rate() * 100.0);
        println!("    Avg Response Time: {}ms", metrics.avg_response_time_ms);
        if let Some(group) = &metrics.group {
            println!("    Group: {}", group);
        }
        if !metrics.tags.is_empty() {
            println!("    Tags: {:?}", metrics.tags);
        }
    }

    // Show that unhealthy servers are excluded from selection
    println!("\nServer Selection (only healthy servers):");
    for i in 1..=5 {
        if let Some(selected) = registry.select_server(None).await {
            println!("  Request {}: Selected -> {}", i, selected);
        } else {
            println!("  Request {}: No healthy server available!", i);
        }
    }

    // Cleanup
    std::fs::remove_dir_all(&temp_base)?;
    Ok(())
}

/// Demo 4: Failover with Automatic Retry
async fn demo_failover() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 4: Failover with Automatic Retry ---\n");

    let registry = McpRegistry::new();
    let temp_base = std::env::temp_dir().join("oxify-mcp-failover-demo");

    // Register multiple servers
    println!("Registering 3 filesystem servers...");
    for i in 1..=3 {
        let server_dir = temp_base.join(format!("server_{}", i));
        std::fs::create_dir_all(&server_dir)?;
        // Create a test file in each server
        std::fs::write(
            server_dir.join("data.txt"),
            format!("Data from server {}", i),
        )?;
        registry
            .register(format!("fs_{}", i), FilesystemServer::new(server_dir))
            .await?;
    }

    // Mark some servers as unhealthy to test failover
    println!("\nSimulating server failures:");
    println!("  - fs_1: Marked as Unhealthy");
    registry
        .set_server_health("fs_1", ServerHealth::Unhealthy)
        .await?;
    println!("  - fs_2: Healthy (will handle requests)");
    println!("  - fs_3: Healthy (backup)");

    // Use failover to read a file
    println!("\nUsing invoke_tool_with_failover (max 3 retries):");
    let result = registry
        .invoke_tool_with_failover(
            "fs_read",
            json!({
                "path": "data.txt"
            }),
            3, // max retries
        )
        .await?;

    println!("  Result: {}", serde_json::to_string_pretty(&result)?);

    // Show that failover skipped unhealthy server
    println!("\nNote: Failover automatically skipped unhealthy server (fs_1)");

    // Display updated metrics after failover
    println!("\nMetrics after failover:");
    for id in ["fs_1", "fs_2", "fs_3"] {
        if let Some(metrics) = registry.get_server_metrics(id).await {
            println!(
                "  {}: requests={}, errors={}, health={:?}",
                id, metrics.request_count, metrics.error_count, metrics.health
            );
        }
    }

    // Cleanup
    std::fs::remove_dir_all(&temp_base)?;
    Ok(())
}

/// Demo 5: Authentication Configuration
fn demo_authentication() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 5: Authentication Configuration ---\n");

    println!("OxiFY MCP supports multiple authentication methods:\n");

    // API Key Authentication
    println!("1. API Key Authentication:");
    let api_key_auth = AuthMethod::ApiKey(ApiKeyAuth {
        api_key: "sk-test-key-12345".to_string(),
        header_name: "X-API-Key".to_string(),
        prefix: None,
    });
    println!("   Header: X-API-Key: sk-test-key-12345");

    // API Key with Bearer prefix
    println!("\n2. API Key with Bearer Prefix:");
    let api_key_bearer = AuthMethod::ApiKey(ApiKeyAuth {
        api_key: "pk_live_abc123".to_string(),
        header_name: "Authorization".to_string(),
        prefix: Some("Bearer".to_string()),
    });
    println!("   Header: Authorization: Bearer pk_live_abc123");

    // Basic Authentication
    println!("\n3. Basic Authentication:");
    let basic_auth = AuthMethod::Basic(oxify_mcp::BasicAuth {
        username: "admin".to_string(),
        password: "secure_password".to_string(),
    });
    println!("   Header: Authorization: Basic <base64(admin:secure_password)>");

    // Bearer Token
    println!("\n4. Bearer Token Authentication:");
    let bearer_auth = AuthMethod::Bearer(oxify_mcp::BearerAuth {
        token: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...".to_string(),
    });
    println!("   Header: Authorization: Bearer eyJhbGci...");

    // Custom Headers
    println!("\n5. Custom Headers Authentication:");
    let mut custom_headers = std::collections::HashMap::new();
    custom_headers.insert("X-Tenant-ID".to_string(), "tenant-123".to_string());
    custom_headers.insert("X-Request-ID".to_string(), "req-456".to_string());
    let _custom_auth = AuthMethod::CustomHeader(oxify_mcp::CustomHeaderAuth {
        headers: custom_headers,
    });
    println!("   Headers: X-Tenant-ID: tenant-123, X-Request-ID: req-456");

    // Credential Store for multiple servers
    println!("\n6. Credential Store (Multi-Server):");
    let mut store = CredentialStore::new();

    store.add(
        "openai_server",
        AuthConfig {
            method: api_key_auth,
            scopes: vec!["read".to_string(), "write".to_string()],
        },
    );

    store.add(
        "internal_server",
        AuthConfig {
            method: basic_auth,
            scopes: vec!["admin".to_string()],
        },
    );

    store.add(
        "jwt_server",
        AuthConfig {
            method: bearer_auth,
            scopes: vec!["api".to_string()],
        },
    );

    store.add(
        "cloud_server",
        AuthConfig {
            method: api_key_bearer,
            scopes: vec!["execute".to_string()],
        },
    );

    println!("   Configured credentials for:");
    for server_id in store.server_ids() {
        if let Some(config) = store.get(server_id) {
            let auth_type = match &config.method {
                AuthMethod::None => "None",
                AuthMethod::ApiKey(_) => "API Key",
                AuthMethod::Basic(_) => "Basic Auth",
                AuthMethod::Bearer(_) => "Bearer Token",
                AuthMethod::CustomHeader(_) => "Custom Headers",
                AuthMethod::OAuth2(_) => "OAuth2",
            };
            println!(
                "   - {}: {} (scopes: {:?})",
                server_id, auth_type, config.scopes
            );
        }
    }

    // Note about usage
    println!("\n   Note: Use AuthenticatedHttpTransport with CredentialStore");
    println!("   for automatic authentication header injection.");

    Ok(())
}
