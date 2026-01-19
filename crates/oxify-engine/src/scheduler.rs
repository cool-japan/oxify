//! Workflow scheduler for cron-based execution
//!
//! This module provides scheduling capabilities for workflows based on cron expressions.

use crate::Engine;
use chrono::{DateTime, Utc};
use cron::Schedule;
use oxify_model::{Workflow, WorkflowSchedule};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Unique identifier for a scheduled workflow
pub type ScheduleId = Uuid;

/// Internal schedule storage type
type ScheduleData = (Workflow, WorkflowSchedule, Schedule);

/// Status of a scheduled workflow execution
#[derive(Debug, Clone)]
pub struct ScheduledExecution {
    /// Schedule ID
    pub schedule_id: ScheduleId,

    /// Workflow ID
    pub workflow_id: Uuid,

    /// Next scheduled run time
    pub next_run: DateTime<Utc>,

    /// Last run time (if any)
    pub last_run: Option<DateTime<Utc>>,

    /// Number of currently active runs
    pub active_runs: u32,

    /// Total successful runs
    pub successful_runs: u64,

    /// Total failed runs
    pub failed_runs: u64,
}

/// Workflow scheduler
pub struct WorkflowScheduler {
    engine: Arc<Engine>,
    schedules: Arc<RwLock<HashMap<ScheduleId, ScheduleData>>>,
    executions: Arc<RwLock<HashMap<ScheduleId, ScheduledExecution>>>,
    running: Arc<RwLock<bool>>,
}

impl WorkflowScheduler {
    /// Create a new workflow scheduler
    pub fn new(engine: Engine) -> Self {
        Self {
            engine: Arc::new(engine),
            schedules: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Add a workflow schedule
    pub fn add_schedule(
        &self,
        workflow: Workflow,
        schedule_config: WorkflowSchedule,
    ) -> Result<ScheduleId, String> {
        // Parse cron expression
        let schedule = Schedule::from_str(&schedule_config.cron)
            .map_err(|e| format!("Invalid cron expression: {}", e))?;

        let schedule_id = Uuid::new_v4();

        // Calculate next run time
        let next_run = schedule
            .upcoming(Utc)
            .next()
            .ok_or("No upcoming schedule found")?;

        // Store cron expression for logging
        let cron_expr = schedule_config.cron.clone();

        // Store schedule
        self.schedules
            .write()
            .unwrap()
            .insert(schedule_id, (workflow.clone(), schedule_config, schedule));

        // Store execution info
        let execution = ScheduledExecution {
            schedule_id,
            workflow_id: workflow.metadata.id,
            next_run,
            last_run: None,
            active_runs: 0,
            successful_runs: 0,
            failed_runs: 0,
        };

        self.executions
            .write()
            .unwrap()
            .insert(schedule_id, execution);

        tracing::info!(
            "Scheduled workflow {} ({}) with cron: {}",
            workflow.metadata.name,
            schedule_id,
            cron_expr
        );

        Ok(schedule_id)
    }

    /// Remove a schedule
    pub fn remove_schedule(&self, schedule_id: ScheduleId) -> bool {
        let removed = self
            .schedules
            .write()
            .unwrap()
            .remove(&schedule_id)
            .is_some();
        if removed {
            self.executions.write().unwrap().remove(&schedule_id);
            tracing::info!("Removed schedule {}", schedule_id);
        }
        removed
    }

    /// Get all scheduled executions
    pub fn list_schedules(&self) -> Vec<ScheduledExecution> {
        self.executions.read().unwrap().values().cloned().collect()
    }

    /// Get a specific scheduled execution
    pub fn get_schedule(&self, schedule_id: ScheduleId) -> Option<ScheduledExecution> {
        self.executions.read().unwrap().get(&schedule_id).cloned()
    }

    /// Start the scheduler (runs in background)
    pub fn start(&self) -> JoinHandle<()> {
        *self.running.write().unwrap() = true;

        let schedules = Arc::clone(&self.schedules);
        let executions = Arc::clone(&self.executions);
        let engine = Arc::clone(&self.engine);
        let running = Arc::clone(&self.running);

        tokio::spawn(async move {
            tracing::info!("Workflow scheduler started");

            while *running.read().unwrap() {
                let now = Utc::now();

                // Get schedules that need to run
                let to_run: Vec<(ScheduleId, Workflow, WorkflowSchedule)> = {
                    let schedules = schedules.read().unwrap();
                    let executions = executions.read().unwrap();

                    executions
                        .iter()
                        .filter_map(|(schedule_id, exec)| {
                            if exec.next_run <= now {
                                schedules.get(schedule_id).map(|(workflow, config, _)| {
                                    (*schedule_id, workflow.clone(), config.clone())
                                })
                            } else {
                                None
                            }
                        })
                        .collect()
                };

                // Execute workflows
                for (schedule_id, workflow, schedule_config) in to_run {
                    // Check if enabled
                    if !schedule_config.enabled {
                        continue;
                    }

                    // Check concurrent run limits
                    let can_run = {
                        let executions = executions.read().unwrap();
                        if let Some(exec) = executions.get(&schedule_id) {
                            if let Some(max_concurrent) = schedule_config.max_concurrent_runs {
                                exec.active_runs < max_concurrent
                            } else {
                                true
                            }
                        } else {
                            false
                        }
                    };

                    if !can_run {
                        tracing::warn!(
                            "Skipping scheduled run for {} - concurrent limit reached",
                            workflow.metadata.name
                        );
                        continue;
                    }

                    // Update execution info
                    {
                        let mut executions = executions.write().unwrap();
                        if let Some(exec) = executions.get_mut(&schedule_id) {
                            exec.active_runs += 1;
                            exec.last_run = Some(now);

                            // Calculate next run
                            let schedules_guard = schedules.read().unwrap();
                            if let Some((_, _, schedule)) = schedules_guard.get(&schedule_id) {
                                exec.next_run = schedule
                                    .upcoming(Utc)
                                    .next()
                                    .unwrap_or_else(|| now + chrono::Duration::hours(1));
                            }
                        }
                    }

                    // Spawn workflow execution
                    let engine_clone = Arc::clone(&engine);
                    let executions_clone = Arc::clone(&executions);
                    let workflow_clone = workflow.clone();

                    tokio::spawn(async move {
                        tracing::info!(
                            "Executing scheduled workflow: {}",
                            workflow_clone.metadata.name
                        );

                        let result = engine_clone.execute_sequential(&workflow_clone).await;

                        // Update execution stats
                        let mut executions = executions_clone.write().unwrap();
                        if let Some(exec) = executions.get_mut(&schedule_id) {
                            exec.active_runs = exec.active_runs.saturating_sub(1);

                            match result {
                                Ok(_) => {
                                    exec.successful_runs += 1;
                                    tracing::info!(
                                        "Scheduled workflow {} completed successfully",
                                        workflow_clone.metadata.name
                                    );
                                }
                                Err(e) => {
                                    exec.failed_runs += 1;
                                    tracing::error!(
                                        "Scheduled workflow {} failed: {}",
                                        workflow_clone.metadata.name,
                                        e
                                    );
                                }
                            }
                        }
                    });
                }

                // Sleep for 1 second before next check
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }

            tracing::info!("Workflow scheduler stopped");
        })
    }

    /// Stop the scheduler
    pub fn stop(&self) {
        *self.running.write().unwrap() = false;
    }

    /// Check if scheduler is running
    pub fn is_running(&self) -> bool {
        *self.running.read().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, Node, NodeKind, WorkflowMetadata};

    #[test]
    fn test_add_schedule() {
        let engine = Engine::new();
        let scheduler = WorkflowScheduler::new(engine);

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test Workflow".to_string()),
            nodes: vec![start.clone(), end.clone()],
            edges: vec![Edge::new(start.id, end.id)],
        };

        let schedule_config = WorkflowSchedule {
            cron: "0 0 0 * * *".to_string(), // Daily at midnight (sec min hour day month weekday)
            timezone: "UTC".to_string(),
            enabled: true,
            max_concurrent_runs: Some(1),
            retry_on_failure: false,
            start_date: None,
            end_date: None,
        };

        let result = scheduler.add_schedule(workflow, schedule_config);
        assert!(result.is_ok());

        let schedules = scheduler.list_schedules();
        assert_eq!(schedules.len(), 1);
    }

    #[test]
    fn test_invalid_cron() {
        let engine = Engine::new();
        let scheduler = WorkflowScheduler::new(engine);

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test Workflow".to_string()),
            nodes: vec![start.clone(), end.clone()],
            edges: vec![Edge::new(start.id, end.id)],
        };

        let schedule_config = WorkflowSchedule {
            cron: "invalid cron".to_string(),
            timezone: "UTC".to_string(),
            enabled: true,
            max_concurrent_runs: None,
            retry_on_failure: false,
            start_date: None,
            end_date: None,
        };

        let result = scheduler.add_schedule(workflow, schedule_config);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_schedule() {
        let engine = Engine::new();
        let scheduler = WorkflowScheduler::new(engine);

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test Workflow".to_string()),
            nodes: vec![start.clone(), end.clone()],
            edges: vec![Edge::new(start.id, end.id)],
        };

        let schedule_config = WorkflowSchedule {
            cron: "0 0 0 * * *".to_string(), // Daily at midnight (sec min hour day month weekday)
            timezone: "UTC".to_string(),
            enabled: true,
            max_concurrent_runs: None,
            retry_on_failure: false,
            start_date: None,
            end_date: None,
        };

        let schedule_id = scheduler.add_schedule(workflow, schedule_config).unwrap();

        assert_eq!(scheduler.list_schedules().len(), 1);

        let removed = scheduler.remove_schedule(schedule_id);
        assert!(removed);

        assert_eq!(scheduler.list_schedules().len(), 0);
    }
}
