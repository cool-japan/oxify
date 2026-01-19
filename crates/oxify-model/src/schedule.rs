//! Workflow scheduling types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::WorkflowId;

/// Schedule ID type
pub type ScheduleId = Uuid;

/// Workflow schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Schedule {
    /// Unique schedule identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: ScheduleId,

    /// Workflow to execute
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub workflow_id: WorkflowId,

    /// Schedule name
    pub name: String,

    /// Description
    pub description: Option<String>,

    /// Cron expression (e.g., "0 0 * * *" for daily at midnight)
    pub cron: String,

    /// Timezone for schedule (e.g., "UTC", "America/New_York")
    pub timezone: String,

    /// Whether the schedule is enabled
    pub enabled: bool,

    /// Input variables for workflow execution
    pub input_variables: std::collections::HashMap<String, serde_json::Value>,

    /// When the schedule was created
    pub created_at: DateTime<Utc>,

    /// When the schedule was last modified
    pub updated_at: DateTime<Utc>,

    /// Last execution time
    pub last_run: Option<DateTime<Utc>>,

    /// Next scheduled execution time
    pub next_run: Option<DateTime<Utc>>,

    /// Number of times this schedule has run
    pub run_count: u64,

    /// Maximum number of times to run (None = infinite)
    pub max_runs: Option<u64>,

    /// Expiration time (schedule becomes disabled after this)
    pub expires_at: Option<DateTime<Utc>>,
}

impl Schedule {
    /// Create a new schedule
    pub fn new(workflow_id: WorkflowId, name: String, cron: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            workflow_id,
            name,
            description: None,
            cron,
            timezone: "UTC".to_string(),
            enabled: true,
            input_variables: std::collections::HashMap::new(),
            created_at: now,
            updated_at: now,
            last_run: None,
            next_run: None,
            run_count: 0,
            max_runs: None,
            expires_at: None,
        }
    }

    /// Check if schedule should run
    pub fn should_run(&self) -> bool {
        if !self.enabled {
            return false;
        }

        // Check if expired
        if let Some(expires_at) = self.expires_at {
            if Utc::now() > expires_at {
                return false;
            }
        }

        // Check max runs
        if let Some(max_runs) = self.max_runs {
            if self.run_count >= max_runs {
                return false;
            }
        }

        // Check next run time
        if let Some(next_run) = self.next_run {
            return Utc::now() >= next_run;
        }

        false
    }

    /// Mark schedule as executed
    pub fn mark_executed(&mut self) {
        self.last_run = Some(Utc::now());
        self.run_count += 1;
    }

    /// Validate cron expression
    pub fn validate(&self) -> Result<(), String> {
        // Basic validation - check cron has 5 or 6 parts
        let parts: Vec<&str> = self.cron.split_whitespace().collect();
        if parts.len() != 5 && parts.len() != 6 {
            return Err(format!(
                "Invalid cron expression '{}': must have 5 or 6 parts",
                self.cron
            ));
        }

        // Validate timezone
        if self.timezone.is_empty() {
            return Err("Timezone cannot be empty".to_string());
        }

        Ok(())
    }
}

/// Schedule execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScheduleExecution {
    /// Unique execution ID
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: Uuid,

    /// Schedule that triggered this execution
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub schedule_id: ScheduleId,

    /// Workflow execution ID
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub execution_id: Uuid,

    /// When this execution was triggered
    pub triggered_at: DateTime<Utc>,

    /// Whether the execution was successful
    pub success: bool,

    /// Error message if execution failed
    pub error: Option<String>,

    /// Execution duration in milliseconds
    pub duration_ms: Option<u64>,
}

impl ScheduleExecution {
    /// Create a new schedule execution record
    pub fn new(schedule_id: ScheduleId, execution_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            schedule_id,
            execution_id,
            triggered_at: Utc::now(),
            success: false,
            error: None,
            duration_ms: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schedule_creation() {
        let workflow_id = Uuid::new_v4();
        let schedule = Schedule::new(
            workflow_id,
            "Daily Report".to_string(),
            "0 0 * * *".to_string(),
        );

        assert_eq!(schedule.workflow_id, workflow_id);
        assert_eq!(schedule.name, "Daily Report");
        assert_eq!(schedule.cron, "0 0 * * *");
        assert!(schedule.enabled);
        assert_eq!(schedule.run_count, 0);
    }

    #[test]
    fn test_schedule_validation() {
        let mut schedule =
            Schedule::new(Uuid::new_v4(), "Test".to_string(), "0 0 * * *".to_string());

        assert!(schedule.validate().is_ok());

        // Invalid cron (too few parts)
        schedule.cron = "0 0 *".to_string();
        assert!(schedule.validate().is_err());

        // Valid 6-part cron
        schedule.cron = "0 0 0 * * *".to_string();
        assert!(schedule.validate().is_ok());
    }

    #[test]
    fn test_should_run() {
        let mut schedule =
            Schedule::new(Uuid::new_v4(), "Test".to_string(), "0 0 * * *".to_string());

        // Disabled schedule
        schedule.enabled = false;
        assert!(!schedule.should_run());

        // Enabled but no next_run set
        schedule.enabled = true;
        assert!(!schedule.should_run());

        // Max runs reached
        schedule.max_runs = Some(5);
        schedule.run_count = 5;
        schedule.next_run = Some(Utc::now());
        assert!(!schedule.should_run());

        // Expired
        schedule.max_runs = None;
        schedule.run_count = 0;
        schedule.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        assert!(!schedule.should_run());
    }

    #[test]
    fn test_mark_executed() {
        let mut schedule =
            Schedule::new(Uuid::new_v4(), "Test".to_string(), "0 0 * * *".to_string());

        assert_eq!(schedule.run_count, 0);
        assert!(schedule.last_run.is_none());

        schedule.mark_executed();

        assert_eq!(schedule.run_count, 1);
        assert!(schedule.last_run.is_some());
    }
}
