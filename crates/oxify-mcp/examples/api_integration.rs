//! Example demonstrating integration with oxify-api MCP endpoints
//!
//! This example shows how to:
//! 1. Set up a registry with multiple MCP servers
//! 2. Query server information via the API
//! 3. List available tools across servers
//! 4. Invoke tools through the API
//! 5. Monitor server health and metrics

use oxify_mcp::{FilesystemServer, GitServer, McpRegistry, ShellServer, WebServer};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("=== oxify-mcp API Integration Example ===\n");

    // 1. Create a registry and register multiple servers
    println!("1. Creating MCP registry and registering servers...");
    let registry = McpRegistry::new();

    // Register filesystem server
    let fs_server = FilesystemServer::new(PathBuf::from("/tmp"));
    registry
        .register("filesystem".to_string(), fs_server)
        .await?;
    println!("   ✓ Registered filesystem server");

    // Register shell server
    let shell_server = ShellServer::default();
    registry.register("shell".to_string(), shell_server).await?;
    println!("   ✓ Registered shell server");

    // Register git server
    let git_server = GitServer::new(PathBuf::from("/tmp"));
    registry.register("git".to_string(), git_server).await?;
    println!("   ✓ Registered git server");

    // Register web server
    let web_server = WebServer::new();
    registry.register("web".to_string(), web_server).await?;
    println!("   ✓ Registered web server");

    println!();

    // 2. List all registered servers (simulates GET /api/v1/mcp/servers)
    println!("2. Listing all registered MCP servers:");
    let all_metrics = registry.get_all_server_metrics().await;
    for metrics in &all_metrics {
        println!("   • Server: {}", metrics.server_id);
        println!("     Health: {:?}", metrics.health);
        println!("     Weight: {}", metrics.weight);
        println!("     Requests: {}", metrics.request_count);
        println!("     Errors: {}", metrics.error_count);
        println!("     Avg Response Time: {}ms", metrics.avg_response_time_ms);
        if !metrics.tags.is_empty() {
            println!("     Tags: {:?}", metrics.tags);
        }
        println!();
    }

    // 3. List all available tools (simulates POST /api/v1/mcp/tools)
    println!("3. Listing all available tools across servers:");
    let all_tools = registry.list_all_tools().await?;
    let mut total_tools = 0;
    for (server_id, tools) in &all_tools {
        println!("   Server: {}", server_id);
        for tool in tools {
            println!("     - {}: {:?}", tool.name, tool.description);
            total_tools += 1;
        }
    }
    println!("   Total tools: {}\n", total_tools);

    // 4. Get registry statistics (simulates GET /api/v1/mcp/stats)
    println!("4. Registry Statistics:");
    let stats = registry.get_stats().await;
    println!("   Total Servers: {}", stats.server_count);
    println!("   Total Tools: {}", stats.total_tools);

    let health_status = registry.get_server_health_status().await;
    let mut healthy = 0;
    let mut degraded = 0;
    let mut unhealthy = 0;
    for health in health_status.values() {
        match health {
            oxify_mcp::ServerHealth::Healthy => healthy += 1,
            oxify_mcp::ServerHealth::Degraded => degraded += 1,
            oxify_mcp::ServerHealth::Unhealthy => unhealthy += 1,
        }
    }
    println!("   Servers by Health:");
    println!("     - Healthy: {}", healthy);
    println!("     - Degraded: {}", degraded);
    println!("     - Unhealthy: {}", unhealthy);
    println!();

    // 5. Invoke a tool (simulates POST /api/v1/mcp/tools/invoke)
    println!("5. Invoking a tool (fs_exists on filesystem server):");
    let start = std::time::Instant::now();
    let result = registry
        .invoke_tool(
            "filesystem",
            "fs_exists",
            serde_json::json!({
                "path": "/tmp"
            }),
        )
        .await?;
    let elapsed = start.elapsed();

    println!("   Result: {}", result);
    println!("   Execution time: {:?}", elapsed);
    println!();

    // 6. Find servers that provide a specific tool
    println!("6. Finding servers with 'fs_read' tool:");
    let servers_with_tool = registry.find_tool("fs_read").await?;
    println!("   Servers: {:?}", servers_with_tool);
    println!();

    // 7. Demonstrate load balancing with weighted servers
    println!("7. Setting up weighted load balancing:");
    registry.set_server_weight("filesystem", 3).await?;
    registry.set_server_weight("shell", 1).await?;
    println!("   ✓ Set filesystem weight to 3");
    println!("   ✓ Set shell weight to 1");
    println!();

    // 8. Select servers using different strategies
    println!("8. Server selection with load balancing:");
    if let Some(server) = registry.select_server(None).await {
        println!("   Round-robin selected: {}", server);
    }

    if let Some(server) = registry
        .select_server(Some(oxify_mcp::LoadBalanceStrategy::LeastConnections))
        .await
    {
        println!("   Least connections selected: {}", server);
    }

    if let Some(server) = registry
        .select_server(Some(oxify_mcp::LoadBalanceStrategy::Random))
        .await
    {
        println!("   Random selected: {}", server);
    }
    println!();

    // 9. Demonstrate failover capability
    println!("9. Invoking tool with automatic failover:");
    let result = registry
        .invoke_tool_with_failover(
            "fs_exists",
            serde_json::json!({
                "path": "/tmp"
            }),
            3, // max retries
        )
        .await?;
    println!("   Result: {}", result);
    println!();

    // 10. Get updated metrics after operations
    println!("10. Updated server metrics after operations:");
    let updated_metrics = registry.get_server_metrics("filesystem").await;
    if let Some(metrics) = updated_metrics {
        println!("   Filesystem Server:");
        println!("     Requests: {}", metrics.request_count);
        println!("     Errors: {}", metrics.error_count);
        println!("     Error rate: {:.2}%", metrics.error_rate() * 100.0);
        println!("     Avg Response Time: {}ms", metrics.avg_response_time_ms);
    }

    println!("\n=== Example completed successfully! ===");
    println!("\nAPI Endpoints Summary:");
    println!("  GET  /api/v1/mcp/servers       - List all servers with metrics");
    println!("  POST /api/v1/mcp/tools         - List all available tools");
    println!("  POST /api/v1/mcp/tools/invoke  - Invoke a specific tool");
    println!("  GET  /api/v1/mcp/stats         - Get registry statistics");

    Ok(())
}
