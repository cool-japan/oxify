//! Basic usage example for oxify-mcp
//!
//! This example demonstrates:
//! - Creating and registering MCP servers
//! - Invoking tools on servers
//! - Listing available tools
//! - Using the registry to manage multiple servers

use oxify_mcp::{FilesystemServer, GitServer, McpRegistry, ShellServer, WebServer};
use serde_json::json;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== OxiFY MCP Basic Usage Example ===\n");

    // Create a registry to manage multiple servers
    let registry = McpRegistry::new();

    // 1. Register a Filesystem Server
    println!("1. Registering FilesystemServer...");
    let temp_dir = std::env::temp_dir().join("oxify-mcp-example");
    std::fs::create_dir_all(&temp_dir)?;

    let fs_server = FilesystemServer::new(temp_dir.clone());
    registry
        .register("filesystem".to_string(), fs_server)
        .await?;

    // 2. Register a Shell Server
    println!("2. Registering ShellServer...");
    let shell_server = ShellServer::default();
    registry.register("shell".to_string(), shell_server).await?;

    // 3. Register a Web Server
    println!("3. Registering WebServer...");
    let web_server = WebServer::new();
    registry.register("web".to_string(), web_server).await?;

    // 4. Register a Git Server
    println!("4. Registering GitServer...");
    let git_server = GitServer::new(PathBuf::from("."));
    registry.register("git".to_string(), git_server).await?;

    println!("\n=== Registry Statistics ===");
    let stats = registry.get_stats().await;
    println!("Registered servers: {}", stats.server_count);
    println!("Total tools available: {}", stats.total_tools);

    // List all server IDs
    println!("\n=== Registered Servers ===");
    let server_ids = registry.list_server_ids().await;
    for id in &server_ids {
        println!("  - {}", id);
    }

    // List all tools
    println!("\n=== Available Tools ===");
    let all_tools = registry.list_all_tools().await?;
    for (server_id, tools) in &all_tools {
        println!("\n{}:", server_id);
        for tool in tools {
            println!(
                "  - {} {}",
                tool.name,
                tool.description
                    .as_ref()
                    .map(|d| format!("({})", d))
                    .unwrap_or_default()
            );
        }
    }

    // Example 1: Use Filesystem Server
    println!("\n=== Example 1: Filesystem Operations ===");

    // Write a file
    let write_result = registry
        .invoke_tool(
            "filesystem",
            "fs_write",
            json!({
                "path": "example.txt",
                "content": "Hello from OxiFY MCP!"
            }),
        )
        .await?;
    println!(
        "Write result: {}",
        serde_json::to_string_pretty(&write_result)?
    );

    // Read the file
    let read_result = registry
        .invoke_tool(
            "filesystem",
            "fs_read",
            json!({
                "path": "example.txt"
            }),
        )
        .await?;
    println!(
        "Read result: {}",
        serde_json::to_string_pretty(&read_result)?
    );

    // List directory
    let list_result = registry
        .invoke_tool(
            "filesystem",
            "fs_list",
            json!({
                "path": "."
            }),
        )
        .await?;
    println!(
        "List result: {}",
        serde_json::to_string_pretty(&list_result)?
    );

    // Example 2: Use Shell Server
    println!("\n=== Example 2: Shell Operations ===");

    // Execute echo command
    let shell_result = registry
        .invoke_tool(
            "shell",
            "shell_exec",
            json!({
                "command": "echo Hello from shell!"
            }),
        )
        .await?;
    println!(
        "Shell result: {}",
        serde_json::to_string_pretty(&shell_result)?
    );

    // Check which command
    let which_result = registry
        .invoke_tool(
            "shell",
            "shell_which",
            json!({
                "command": "ls"
            }),
        )
        .await?;
    println!(
        "Which result: {}",
        serde_json::to_string_pretty(&which_result)?
    );

    // Example 3: Use Git Server
    println!("\n=== Example 3: Git Operations ===");

    // Get git status
    let git_status = registry.invoke_tool("git", "git_status", json!({})).await?;
    println!("Git status: {}", serde_json::to_string_pretty(&git_status)?);

    // Example 4: Find a tool across servers
    println!("\n=== Example 4: Tool Discovery ===");

    let servers_with_read = registry.find_tool("fs_read").await?;
    println!(
        "Tool 'fs_read' is available on servers: {:?}",
        servers_with_read
    );

    // Cleanup
    println!("\n=== Cleanup ===");
    std::fs::remove_dir_all(&temp_dir)?;
    println!("Temporary directory removed");

    println!("\n=== Example Complete ===");

    Ok(())
}
