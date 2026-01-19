//! Webhook management commands

use anyhow::{Context, Result};
use clap::Subcommand;
use oxify_model::{Webhook, WebhookId};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Subcommand)]
pub enum WebhookCommands {
    /// Create a new webhook
    Create {
        /// Webhook name
        name: String,
        /// Workflow ID to trigger
        workflow_id: String,
        /// Event types to listen for (comma-separated, e.g., "push,pull_request")
        #[arg(short, long)]
        events: Option<String>,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
        /// Required headers (key=value pairs, can be specified multiple times)
        #[arg(long)]
        header: Vec<String>,
        /// IP whitelist (comma-separated IPs)
        #[arg(long)]
        ip_whitelist: Option<String>,
        /// Maximum request body size in bytes
        #[arg(long, default_value = "1048576")]
        max_body_size: usize,
        /// Timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u32,
        /// Disable the webhook on creation
        #[arg(long)]
        disabled: bool,
    },
    /// List all webhooks
    List {
        /// Show only enabled webhooks
        #[arg(short, long)]
        enabled: bool,
        /// Show webhook URLs
        #[arg(short, long)]
        urls: bool,
    },
    /// Get webhook details
    Get {
        /// Webhook ID
        webhook_id: String,
        /// Show secret
        #[arg(short, long)]
        show_secret: bool,
    },
    /// Update a webhook
    Update {
        /// Webhook ID
        webhook_id: String,
        /// New name
        #[arg(short, long)]
        name: Option<String>,
        /// New event types (comma-separated)
        #[arg(short, long)]
        events: Option<String>,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// Enable the webhook
        #[arg(long)]
        enable: bool,
        /// Disable the webhook
        #[arg(long)]
        disable: bool,
    },
    /// Delete a webhook
    Delete {
        /// Webhook ID
        webhook_id: String,
    },
    /// List webhook events
    Events {
        /// Webhook ID
        webhook_id: String,
        /// Number of events to show
        #[arg(short, long, default_value = "10")]
        limit: u32,
        /// Filter by status (pending, processing, completed, failed, rejected)
        #[arg(short, long)]
        status: Option<String>,
    },
    /// Show webhook statistics
    Stats {
        /// Webhook ID
        webhook_id: String,
    },
}

pub async fn handle_webhook_command(command: WebhookCommands) -> Result<()> {
    match command {
        WebhookCommands::Create {
            name,
            workflow_id,
            events,
            description,
            header,
            ip_whitelist,
            max_body_size,
            timeout,
            disabled,
        } => {
            let wf_id = workflow_id
                .parse::<Uuid>()
                .with_context(|| format!("Invalid workflow ID: {}", workflow_id))?;

            // Parse event types
            let event_types = if let Some(e) = events {
                e.split(',').map(|s| s.trim().to_string()).collect()
            } else {
                vec!["*".to_string()] // Default to all events
            };

            // Parse required headers
            let required_headers = parse_headers(header)?;

            // Parse IP whitelist
            let ip_whitelist_vec = if let Some(ips) = ip_whitelist {
                ips.split(',').map(|s| s.trim().to_string()).collect()
            } else {
                vec![]
            };

            // Generate a random owner ID and secret for local creation
            let owner_id = Uuid::new_v4();
            let secret = Uuid::new_v4().to_string();

            let mut webhook = Webhook::new(name.clone(), wf_id, secret, owner_id);

            // Set additional properties
            webhook.event_types = event_types.clone();
            webhook.required_headers = required_headers.clone();
            webhook.ip_whitelist = ip_whitelist_vec.clone();
            webhook.max_body_size = max_body_size;
            webhook.timeout_seconds = timeout;
            webhook.enabled = !disabled;
            webhook.description = description.clone();

            println!("✓ Webhook created successfully!");
            println!("  ID:          {}", webhook.id);
            println!("  Name:        {}", webhook.name);
            println!("  Workflow:    {}", webhook.workflow_id);
            println!("  Events:      {}", event_types.join(", "));
            println!("  Enabled:     {}", !disabled);
            println!("  Max body:    {} bytes", max_body_size);
            println!("  Timeout:     {}s", timeout);

            if let Some(desc) = &description {
                println!("  Description: {}", desc);
            }

            if !required_headers.is_empty() {
                println!("  Headers:     {}", required_headers.len());
                for key in required_headers.keys() {
                    println!("    - {}: ***", key);
                }
            }

            if !ip_whitelist_vec.is_empty() {
                println!("  IP Whitelist: {}", ip_whitelist_vec.join(", "));
            }

            println!("\nWebhook URL: /api/v1/webhooks/events/{}", webhook.id);
            println!("Secret: {}", webhook.secret);
            println!("\nNote: Webhook has been created locally.");
            println!("      Use the API to persist to the database.");

            Ok(())
        }
        WebhookCommands::List { enabled, urls } => {
            println!("Webhooks");
            println!("========\n");

            if enabled {
                println!("Showing only enabled webhooks");
            }

            println!("\nNote: This command requires API integration.");
            println!("      Use the API to list webhooks from the database.");

            if urls {
                println!("\nWebhook URLs will be displayed in format:");
                println!("  /api/v1/webhooks/events/{{webhook_id}}");
            }

            Ok(())
        }
        WebhookCommands::Get {
            webhook_id,
            show_secret,
        } => {
            let _id: WebhookId = webhook_id
                .parse()
                .with_context(|| format!("Invalid webhook ID: {}", webhook_id))?;

            println!("Webhook Details");
            println!("===============\n");

            if show_secret {
                println!("Note: Secret will be displayed.");
            }

            println!("Note: This command requires API integration.");
            println!("      Use the API to get webhook details from the database.");

            Ok(())
        }
        WebhookCommands::Update {
            webhook_id,
            name,
            events,
            description,
            enable,
            disable,
        } => {
            let _id: WebhookId = webhook_id
                .parse()
                .with_context(|| format!("Invalid webhook ID: {}", webhook_id))?;

            if enable && disable {
                anyhow::bail!("Cannot specify both --enable and --disable");
            }

            println!("Webhook Update");
            println!("==============\n");
            println!("Updating webhook: {}", webhook_id);

            if let Some(n) = name {
                println!("  New name: {}", n);
            }
            if let Some(e) = events {
                println!("  New events: {}", e);
            }
            if let Some(desc) = description {
                println!("  New description: {}", desc);
            }
            if enable {
                println!("  Enabling webhook");
            }
            if disable {
                println!("  Disabling webhook");
            }

            println!("\nNote: This command requires API integration.");
            println!("      Use the API to update webhooks in the database.");

            Ok(())
        }
        WebhookCommands::Delete { webhook_id } => {
            let _id: WebhookId = webhook_id
                .parse()
                .with_context(|| format!("Invalid webhook ID: {}", webhook_id))?;

            println!("Webhook Deletion");
            println!("================\n");
            println!("Deleting webhook: {}", webhook_id);
            println!("\nNote: This command requires API integration.");
            println!("      Use the API to delete webhooks from the database.");

            Ok(())
        }
        WebhookCommands::Events {
            webhook_id,
            limit,
            status,
        } => {
            let _id: WebhookId = webhook_id
                .parse()
                .with_context(|| format!("Invalid webhook ID: {}", webhook_id))?;

            println!("Webhook Events");
            println!("==============\n");
            println!("Webhook: {}", webhook_id);
            println!("Showing last {} events", limit);

            if let Some(s) = status {
                println!("Filtered by status: {}", s);
            }

            println!("\nNote: This command requires API integration.");
            println!("      Use the API to get webhook events from the database.");

            Ok(())
        }
        WebhookCommands::Stats { webhook_id } => {
            let _id: WebhookId = webhook_id
                .parse()
                .with_context(|| format!("Invalid webhook ID: {}", webhook_id))?;

            println!("Webhook Statistics");
            println!("==================\n");
            println!("Webhook: {}", webhook_id);

            println!("\nNote: This command requires API integration.");
            println!("      Use the API to get webhook statistics from the database.");
            println!("\nStats will include:");
            println!("  - Total events");
            println!("  - Successful events");
            println!("  - Failed events");
            println!("  - Pending events");
            println!("  - Average processing time");
            println!("  - Last event timestamp");

            Ok(())
        }
    }
}

/// Helper to parse header arguments
fn parse_headers(headers: Vec<String>) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();

    for header in headers {
        let parts: Vec<&str> = header.splitn(2, '=').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid header format: {}. Expected key=value", header);
        }

        map.insert(parts[0].to_string(), parts[1].to_string());
    }

    Ok(map)
}

/// Helper to format a webhook for display
#[allow(dead_code)]
fn format_webhook(webhook: &Webhook) -> String {
    let status = if webhook.enabled { "✓" } else { "✗" };
    let last_trigger = webhook
        .last_triggered_at
        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Never".to_string());

    format!(
        "{} {} | Events: {} | Last: {} | Triggers: {}",
        status,
        webhook.name,
        webhook.event_types.join(","),
        last_trigger,
        webhook.trigger_count
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_headers() {
        let headers = vec![
            "X-GitHub-Event=push".to_string(),
            "X-Hub-Signature=abc123".to_string(),
        ];

        let result = parse_headers(headers).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("X-GitHub-Event").unwrap(), "push");
        assert_eq!(result.get("X-Hub-Signature").unwrap(), "abc123");
    }

    #[test]
    fn test_parse_headers_invalid() {
        let headers = vec!["invalid".to_string()];
        let result = parse_headers(headers);
        assert!(result.is_err());
    }
}
