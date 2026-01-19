//! MCP Transport Example
//!
//! This example demonstrates:
//! - Using StdioTransport to communicate with external MCP servers
//! - Using HttpTransport for remote MCP servers
//! - Sending JSON-RPC requests and handling responses

// Note: Imports commented out as they're only needed for the commented-out examples
// use oxify_mcp::{DefaultMcpClient, HttpTransport, McpClient, StdioTransport};
// use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== OxiFY MCP Transport Example ===\n");

    // Example 1: HTTP Transport (commented out as it requires a running server)
    println!("=== Example 1: HTTP Transport ===");
    println!("Note: This example requires a running MCP server at http://localhost:3000");
    println!("Skipping HTTP transport example for now.\n");

    /*
    // Create HTTP transport
    let http_transport = HttpTransport::new("http://localhost:3000".to_string());
    let mut http_client = DefaultMcpClient::new(http_transport, "http-server".to_string());

    // Initialize the client
    http_client.initialize().await?;

    // List available tools
    let tools = http_client.list_tools("http-server").await?;
    println!("Available tools via HTTP:");
    for tool in &tools {
        println!("  - {} {:?}", tool.name, tool.description);
    }

    // Invoke a tool
    let result = http_client
        .invoke_tool(oxify_mcp::McpRequest {
            server_id: "http-server".to_string(),
            tool_name: "example_tool".to_string(),
            parameters: json!({"param": "value"}),
        })
        .await?;

    println!("Tool result: {}", serde_json::to_string_pretty(&result)?);

    // Close the client
    http_client.close().await?;
    */

    // Example 2: Stdio Transport (commented out as it requires an MCP server executable)
    println!("=== Example 2: Stdio Transport ===");
    println!("Note: This example requires an MCP server executable");
    println!("Skipping Stdio transport example for now.\n");

    /*
    // Create Stdio transport (launches a subprocess)
    let stdio_transport = StdioTransport::new("mcp-server", &[]).await?;
    let mut stdio_client = DefaultMcpClient::new(stdio_transport, "stdio-server".to_string());

    // Initialize the client
    stdio_client.initialize().await?;

    // List available tools
    let tools = stdio_client.list_tools("stdio-server").await?;
    println!("Available tools via Stdio:");
    for tool in &tools {
        println!("  - {} {:?}", tool.name, tool.description);
    }

    // Invoke a tool
    let result = stdio_client
        .invoke_tool(oxify_mcp::McpRequest {
            server_id: "stdio-server".to_string(),
            tool_name: "example_tool".to_string(),
            parameters: json!({"param": "value"}),
        })
        .await?;

    println!("Tool result: {}", serde_json::to_string_pretty(&result)?);

    // Close the client
    stdio_client.close().await?;
    */

    // Example 3: Using built-in servers as a starting point
    println!("=== Example 3: Built-in Servers ===");
    println!("For working examples, see the basic_usage.rs example");
    println!("which demonstrates using built-in MCP servers (Filesystem, Shell, Git, Web)");

    println!("\n=== Transport Layer Architecture ===");
    println!("The MCP transport layer provides two transport mechanisms:");
    println!("1. StdioTransport: Launches MCP server as subprocess, communicates via stdin/stdout");
    println!("2. HttpTransport: Connects to remote MCP server via HTTP JSON-RPC");
    println!("\nBoth transports implement the McpTransport trait and can be used interchangeably.");

    println!("\n=== Example Complete ===");

    Ok(())
}
