//! Deployment commands for OxiFY workflows

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use oxify_model::http_util::append_query_params;
use oxify_model::Workflow;
use oxihttp::HttpsClient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Build a fresh oxihttp HTTPS-capable client for the deploy commands.
///
/// The client transparently handles both `http://` and `https://` server
/// URLs, since deployment targets default to a local `http://` server but
/// may be pointed at a remote `https://` endpoint.
fn build_client() -> Result<HttpsClient> {
    oxihttp::Client::builder()
        .with_tls()
        .build_https()
        .context("Failed to build HTTP client")
}

#[derive(Debug, Args)]
pub struct DeployArgs {
    #[command(subcommand)]
    pub command: DeployCommand,
}

#[derive(Debug, Subcommand)]
pub enum DeployCommand {
    /// Deploy a workflow to the OxiFY server
    Deploy {
        /// Path to workflow file
        #[arg(short, long)]
        file: PathBuf,

        /// Server URL (default: http://localhost:8080)
        #[arg(short, long, default_value = "http://localhost:8080")]
        server: String,

        /// API token for authentication
        #[arg(short, long)]
        token: Option<String>,

        /// Enable the workflow immediately after deployment
        #[arg(short, long)]
        enable: bool,
    },

    /// Undeploy a workflow from the server
    Undeploy {
        /// Workflow ID or name
        workflow: String,

        /// Server URL (default: http://localhost:8080)
        #[arg(short, long, default_value = "http://localhost:8080")]
        server: String,

        /// API token for authentication
        #[arg(short, long)]
        token: Option<String>,

        /// Force undeploy even if workflow is running
        #[arg(short, long)]
        force: bool,
    },

    /// Check deployment status
    Status {
        /// Workflow ID or name (optional, shows all if not provided)
        workflow: Option<String>,

        /// Server URL (default: http://localhost:8080)
        #[arg(short, long, default_value = "http://localhost:8080")]
        server: String,

        /// API token for authentication
        #[arg(short, long)]
        token: Option<String>,
    },

    /// List all deployed workflows
    List {
        /// Server URL (default: http://localhost:8080)
        #[arg(short, long, default_value = "http://localhost:8080")]
        server: String,

        /// API token for authentication
        #[arg(short, long)]
        token: Option<String>,

        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Update a deployed workflow
    Update {
        /// Path to workflow file
        #[arg(short, long)]
        file: PathBuf,

        /// Server URL (default: http://localhost:8080)
        #[arg(short, long, default_value = "http://localhost:8080")]
        server: String,

        /// API token for authentication
        #[arg(short, long)]
        token: Option<String>,

        /// Create new version instead of updating in-place
        #[arg(short, long)]
        new_version: bool,
    },

    /// Configure deployment settings
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Set default server URL
    SetServer {
        /// Server URL
        url: String,
    },

    /// Set API token
    SetToken {
        /// API token
        token: String,
    },

    /// Show current configuration
    Show,

    /// Clear all configuration
    Clear,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeployConfig {
    server_url: Option<String>,
    api_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeploymentInfo {
    id: String,
    name: String,
    version: u32,
    status: String,
    deployed_at: String,
    last_execution: Option<String>,
    execution_count: u64,
}

pub async fn execute(args: DeployArgs) -> Result<()> {
    match args.command {
        DeployCommand::Deploy {
            file,
            server,
            token,
            enable,
        } => deploy_workflow(file, server, token, enable).await,

        DeployCommand::Undeploy {
            workflow,
            server,
            token,
            force,
        } => undeploy_workflow(workflow, server, token, force).await,

        DeployCommand::Status {
            workflow,
            server,
            token,
        } => check_status(workflow, server, token).await,

        DeployCommand::List {
            server,
            token,
            verbose,
        } => list_deployments(server, token, verbose).await,

        DeployCommand::Update {
            file,
            server,
            token,
            new_version,
        } => update_deployment(file, server, token, new_version).await,

        DeployCommand::Config { action } => manage_config(action),
    }
}

async fn deploy_workflow(
    file: PathBuf,
    server: String,
    token: Option<String>,
    enable: bool,
) -> Result<()> {
    println!("📦 Deploying workflow from: {}", file.display());

    // Read workflow file
    let content = std::fs::read_to_string(&file)
        .with_context(|| format!("Failed to read workflow file: {}", file.display()))?;

    let workflow: Workflow = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse workflow file: {}", file.display()))?;

    println!(
        "   Workflow: {} (ID: {})",
        workflow.metadata.name, workflow.metadata.id
    );

    // Validate workflow
    if let Err(e) = workflow.validate() {
        anyhow::bail!("Workflow validation failed: {}", e);
    }

    // Create HTTP client
    let client = build_client()?;
    let url = format!("{}/api/v1/workflows", server);

    // Build request
    let mut req = client.post(&url)?.json(&workflow)?;

    if let Some(token) = token {
        req = req.bearer_token(&token)?;
    }

    // Send request
    println!("🚀 Uploading to server: {}", server);
    let response = req.send().await.context("Failed to connect to server")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .body_text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Deployment failed with status {}: {}", status, error_text);
    }

    println!("✅ Workflow deployed successfully!");

    if enable {
        println!("   Status: Enabled");
    }

    Ok(())
}

async fn undeploy_workflow(
    workflow: String,
    server: String,
    token: Option<String>,
    force: bool,
) -> Result<()> {
    println!("🗑️  Undeploying workflow: {}", workflow);

    if force {
        println!("   Force mode: enabled");
    }

    let client = build_client()?;
    let base_url = format!("{}/api/v1/workflows/{}", server, workflow);
    let url = if force {
        append_query_params(&base_url, &[("force", "true")])
    } else {
        base_url
    };

    let mut req = client.delete(&url)?;

    if let Some(token) = token {
        req = req.bearer_token(&token)?;
    }

    let response = req.send().await.context("Failed to connect to server")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .body_text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Undeployment failed with status {}: {}", status, error_text);
    }

    println!("✅ Workflow undeployed successfully!");

    Ok(())
}

async fn check_status(
    workflow: Option<String>,
    server: String,
    token: Option<String>,
) -> Result<()> {
    let client = build_client()?;

    if let Some(workflow_id) = workflow {
        // Check specific workflow
        println!("📊 Checking status for: {}", workflow_id);

        let url = format!("{}/api/v1/workflows/{}", server, workflow_id);
        let mut req = client.get(&url)?;

        if let Some(token) = token {
            req = req.bearer_token(&token)?;
        }

        let response = req.send().await.context("Failed to connect to server")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get workflow status: {}", response.status());
        }

        let info: DeploymentInfo = response.body_json().await?;
        print_deployment_info(&info, true);
    } else {
        // Check all workflows
        println!("📊 Checking status for all deployments...");
        list_deployments(server, token, false).await?;
    }

    Ok(())
}

async fn list_deployments(server: String, token: Option<String>, verbose: bool) -> Result<()> {
    println!("📋 Listing deployments from: {}", server);

    let client = build_client()?;
    let url = format!("{}/api/v1/workflows", server);

    let mut req = client.get(&url)?;

    if let Some(token) = token {
        req = req.bearer_token(&token)?;
    }

    let response = req.send().await.context("Failed to connect to server")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to list deployments: {}", response.status());
    }

    let deployments: Vec<DeploymentInfo> = response.body_json().await?;

    if deployments.is_empty() {
        println!("   No deployments found.");
        return Ok(());
    }

    println!("\n   Found {} deployment(s):\n", deployments.len());

    for info in deployments {
        print_deployment_info(&info, verbose);
        println!();
    }

    Ok(())
}

async fn update_deployment(
    file: PathBuf,
    server: String,
    token: Option<String>,
    new_version: bool,
) -> Result<()> {
    println!("🔄 Updating deployment from: {}", file.display());

    if new_version {
        println!("   Creating new version...");
    }

    // Read workflow file
    let content = std::fs::read_to_string(&file)
        .with_context(|| format!("Failed to read workflow file: {}", file.display()))?;

    let workflow: Workflow = serde_json::from_str(&content)?;

    let client = build_client()?;
    let base_url = format!("{}/api/v1/workflows/{}", server, workflow.metadata.id);
    let url = if new_version {
        append_query_params(&base_url, &[("new_version", "true")])
    } else {
        base_url
    };

    let mut req = client.put(&url)?.json(&workflow)?;

    if let Some(token) = token {
        req = req.bearer_token(&token)?;
    }

    let response = req.send().await.context("Failed to connect to server")?;

    if !response.status().is_success() {
        anyhow::bail!("Update failed: {}", response.status());
    }

    println!("✅ Workflow updated successfully!");

    Ok(())
}

fn manage_config(action: ConfigAction) -> Result<()> {
    let config_path = get_config_path()?;

    match action {
        ConfigAction::SetServer { url } => {
            let mut config = load_config(&config_path)?;
            config.server_url = Some(url.clone());
            save_config(&config_path, &config)?;
            println!("✅ Default server set to: {}", url);
        }

        ConfigAction::SetToken { token } => {
            let mut config = load_config(&config_path)?;
            config.api_token = Some(token);
            save_config(&config_path, &config)?;
            println!("✅ API token configured");
        }

        ConfigAction::Show => {
            let config = load_config(&config_path)?;
            println!("Current deployment configuration:");
            println!(
                "  Server URL: {}",
                config.server_url.as_deref().unwrap_or("(not set)")
            );
            println!(
                "  API Token:  {}",
                if config.api_token.is_some() {
                    "(configured)"
                } else {
                    "(not set)"
                }
            );
        }

        ConfigAction::Clear => {
            std::fs::remove_file(&config_path).ok();
            println!("✅ Configuration cleared");
        }
    }

    Ok(())
}

fn get_config_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    let oxify_dir = config_dir.join("oxify");
    std::fs::create_dir_all(&oxify_dir)?;
    Ok(oxify_dir.join("deploy.json"))
}

fn load_config(path: &PathBuf) -> Result<DeployConfig> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(DeployConfig {
            server_url: None,
            api_token: None,
        })
    }
}

fn save_config(path: &PathBuf, config: &DeployConfig) -> Result<()> {
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn print_deployment_info(info: &DeploymentInfo, verbose: bool) {
    println!("   📦 {} (v{})", info.name, info.version);
    println!("      ID: {}", info.id);
    println!("      Status: {}", info.status);

    if verbose {
        println!("      Deployed: {}", info.deployed_at);
        println!("      Executions: {}", info.execution_count);
        if let Some(last_exec) = &info.last_execution {
            println!("      Last execution: {}", last_exec);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deploy_config_serialization() {
        let config = DeployConfig {
            server_url: Some("http://localhost:8080".to_string()),
            api_token: Some("test-token".to_string()),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: DeployConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.server_url, deserialized.server_url);
        assert_eq!(config.api_token, deserialized.api_token);
    }

    #[test]
    fn test_deploy_config_default() {
        let config = DeployConfig {
            server_url: None,
            api_token: None,
        };

        assert!(config.server_url.is_none());
        assert!(config.api_token.is_none());
    }

    #[test]
    fn test_deployment_info_serialization() {
        let info = DeploymentInfo {
            id: "test-id".to_string(),
            name: "Test Workflow".to_string(),
            version: 1,
            status: "active".to_string(),
            deployed_at: "2026-01-01T00:00:00Z".to_string(),
            last_execution: Some("2026-01-01T12:00:00Z".to_string()),
            execution_count: 42,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: DeploymentInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info.id, deserialized.id);
        assert_eq!(info.name, deserialized.name);
        assert_eq!(info.version, deserialized.version);
        assert_eq!(info.execution_count, deserialized.execution_count);
    }
}
