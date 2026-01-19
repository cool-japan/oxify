//! Schedule management commands

use anyhow::{Context, Result};
use clap::Subcommand;
use oxify_model::{Schedule, ScheduleId};

#[derive(Subcommand)]
pub enum ScheduleCommands {
    /// Create a new schedule
    Create {
        /// Workflow ID to schedule
        workflow_id: String,
        /// Schedule name
        name: String,
        /// Cron expression (e.g., "0 0 * * *" for daily at midnight)
        cron: String,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
        /// Timezone (default: UTC)
        #[arg(short, long, default_value = "UTC")]
        timezone: String,
        /// Disable the schedule on creation
        #[arg(long)]
        disabled: bool,
    },
    /// List all schedules
    List {
        /// Filter by workflow ID
        #[arg(short, long)]
        workflow: Option<String>,
        /// Show only enabled schedules
        #[arg(short, long)]
        enabled: bool,
    },
    /// Get schedule details
    Get {
        /// Schedule ID
        schedule_id: String,
    },
    /// Update a schedule
    Update {
        /// Schedule ID
        schedule_id: String,
        /// New cron expression
        #[arg(short, long)]
        cron: Option<String>,
        /// New timezone
        #[arg(short, long)]
        timezone: Option<String>,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// Enable the schedule
        #[arg(long)]
        enable: bool,
        /// Disable the schedule
        #[arg(long)]
        disable: bool,
    },
    /// Delete a schedule
    Delete {
        /// Schedule ID
        schedule_id: String,
    },
    /// Show execution history for a schedule
    History {
        /// Schedule ID
        schedule_id: String,
        /// Number of executions to show
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
}

pub async fn handle_schedule_command(command: ScheduleCommands) -> Result<()> {
    match command {
        ScheduleCommands::Create {
            workflow_id,
            name,
            cron,
            description,
            timezone,
            disabled,
        } => {
            let wf_id = workflow_id
                .parse()
                .with_context(|| format!("Invalid workflow ID: {}", workflow_id))?;

            let mut schedule = Schedule::new(wf_id, name, cron);
            schedule.description = description;
            schedule.timezone = timezone;
            schedule.enabled = !disabled;

            // Validate schedule
            schedule.validate().map_err(|e| anyhow::anyhow!(e))?;

            println!("✓ Schedule created successfully!");
            println!("  ID:          {}", schedule.id);
            println!("  Workflow:    {}", schedule.workflow_id);
            println!("  Name:        {}", schedule.name);
            println!("  Cron:        {}", schedule.cron);
            println!("  Timezone:    {}", schedule.timezone);
            println!("  Enabled:     {}", schedule.enabled);

            if let Some(desc) = &schedule.description {
                println!("  Description: {}", desc);
            }

            println!("\nNote: Schedule has been created locally.");
            println!("      Use 'oxify workflow create' to persist to the API.");

            Ok(())
        }
        ScheduleCommands::List { workflow, enabled } => {
            println!("Schedules");
            println!("=========\n");

            if let Some(wf) = workflow {
                println!("Filtered by workflow: {}", wf);
            }
            if enabled {
                println!("Showing only enabled schedules");
            }

            println!("\nNote: This command requires API integration.");
            println!("      Use the API to list schedules from the database.");

            Ok(())
        }
        ScheduleCommands::Get { schedule_id } => {
            let _id: ScheduleId = schedule_id
                .parse()
                .with_context(|| format!("Invalid schedule ID: {}", schedule_id))?;

            println!("Schedule Details");
            println!("================\n");
            println!("Note: This command requires API integration.");
            println!("      Use the API to get schedule details from the database.");

            Ok(())
        }
        ScheduleCommands::Update {
            schedule_id,
            cron,
            timezone,
            description,
            enable,
            disable,
        } => {
            let _id: ScheduleId = schedule_id
                .parse()
                .with_context(|| format!("Invalid schedule ID: {}", schedule_id))?;

            if enable && disable {
                anyhow::bail!("Cannot specify both --enable and --disable");
            }

            println!("Schedule Update");
            println!("===============\n");
            println!("Updating schedule: {}", schedule_id);

            if let Some(c) = cron {
                println!("  New cron: {}", c);
            }
            if let Some(tz) = timezone {
                println!("  New timezone: {}", tz);
            }
            if let Some(desc) = description {
                println!("  New description: {}", desc);
            }
            if enable {
                println!("  Enabling schedule");
            }
            if disable {
                println!("  Disabling schedule");
            }

            println!("\nNote: This command requires API integration.");
            println!("      Use the API to update schedules in the database.");

            Ok(())
        }
        ScheduleCommands::Delete { schedule_id } => {
            let _id: ScheduleId = schedule_id
                .parse()
                .with_context(|| format!("Invalid schedule ID: {}", schedule_id))?;

            println!("Schedule Deletion");
            println!("=================\n");
            println!("Deleting schedule: {}", schedule_id);
            println!("\nNote: This command requires API integration.");
            println!("      Use the API to delete schedules from the database.");

            Ok(())
        }
        ScheduleCommands::History { schedule_id, limit } => {
            let _id: ScheduleId = schedule_id
                .parse()
                .with_context(|| format!("Invalid schedule ID: {}", schedule_id))?;

            println!("Schedule Execution History");
            println!("==========================\n");
            println!("Schedule: {}", schedule_id);
            println!("Showing last {} executions\n", limit);

            println!("Note: This command requires API integration.");
            println!("      Use the API to get execution history from the database.");

            Ok(())
        }
    }
}

/// Helper to format a schedule for display
#[allow(dead_code)]
fn format_schedule(schedule: &Schedule) -> String {
    let status = if schedule.enabled { "✓" } else { "✗" };
    let last_run = schedule
        .last_run
        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Never".to_string());

    format!(
        "{} {} | {} | {} | Last run: {} | Runs: {}",
        status, schedule.name, schedule.cron, schedule.timezone, last_run, schedule.run_count
    )
}

/// Helper to format duration
#[allow(dead_code)]
fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{:.1}m", ms as f64 / 60_000.0)
    }
}

/// Helper to parse input variables from command line
#[allow(dead_code)]
fn parse_input_vars(
    vars: Vec<String>,
) -> Result<std::collections::HashMap<String, serde_json::Value>> {
    let mut map = std::collections::HashMap::new();

    for var in vars {
        let parts: Vec<&str> = var.splitn(2, '=').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid variable format: {}. Expected key=value", var);
        }

        let key = parts[0].to_string();
        let value = parts[1];

        // Try to parse as JSON, fallback to string
        let json_value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));

        map.insert(key, json_value);
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_input_vars() {
        let vars = vec![
            "name=John".to_string(),
            "age=30".to_string(),
            "active=true".to_string(),
        ];

        let result = parse_input_vars(vars).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(
            result.get("name").unwrap(),
            &serde_json::Value::String("John".to_string())
        );
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(1500), "1.5s");
        assert_eq!(format_duration(90000), "1.5m");
    }
}
