//! Schedule storage implementation

use crate::{DatabasePool, Result, StorageError};
use chrono::{DateTime, Utc};
use oxify_model::{Schedule, ScheduleExecution, ScheduleId, WorkflowId};
use uuid::Uuid;

/// Validates a cron expression format
/// Basic validation for: second minute hour day month day-of-week
/// or: minute hour day month day-of-week (5 fields)
fn validate_cron(cron: &str) -> Result<()> {
    let parts: Vec<&str> = cron.split_whitespace().collect();

    // Cron expressions should have either 5 or 6 fields
    if parts.len() != 5 && parts.len() != 6 {
        return Err(StorageError::ValidationError(format!(
            "Invalid cron expression: expected 5 or 6 fields, got {}",
            parts.len()
        )));
    }

    // Basic field validation
    for (idx, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(StorageError::ValidationError(format!(
                "Invalid cron expression: field {} is empty",
                idx + 1
            )));
        }
    }

    Ok(())
}

/// Validates timezone string (basic check for common formats)
fn validate_timezone(tz: &str) -> Result<()> {
    // Check if it's a valid timezone identifier format
    // Common formats: UTC, America/New_York, Europe/London, etc.
    if tz.is_empty() {
        return Err(StorageError::ValidationError(
            "Timezone cannot be empty".to_string(),
        ));
    }

    // Basic validation: should not contain invalid characters
    if !tz
        .chars()
        .all(|c| c.is_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '+')
    {
        return Err(StorageError::ValidationError(format!(
            "Invalid timezone format: {tz}"
        )));
    }

    Ok(())
}

/// Schedule storage layer
#[derive(Clone)]
pub struct ScheduleStore {
    pool: DatabasePool,
}

impl ScheduleStore {
    /// Create a new schedule store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Create a new schedule
    pub async fn create(&self, schedule: &Schedule) -> Result<ScheduleId> {
        // Validate cron expression
        validate_cron(&schedule.cron)?;

        // Validate timezone
        validate_timezone(&schedule.timezone)?;

        let input_vars = serde_json::to_value(&schedule.input_variables)?;

        sqlx::query(
            r"
            INSERT INTO schedules
            (id, workflow_id, name, description, cron, timezone, enabled,
             input_variables, last_run, next_run, run_count, max_runs, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ",
        )
        .bind(schedule.id)
        .bind(schedule.workflow_id)
        .bind(&schedule.name)
        .bind(&schedule.description)
        .bind(&schedule.cron)
        .bind(&schedule.timezone)
        .bind(schedule.enabled)
        .bind(input_vars)
        .bind(schedule.last_run)
        .bind(schedule.next_run)
        .bind(schedule.run_count as i64)
        .bind(schedule.max_runs.map(|v| v as i64))
        .bind(schedule.expires_at)
        .execute(self.pool.pool())
        .await?;

        Ok(schedule.id)
    }

    /// Get a schedule by ID
    pub async fn get(&self, id: &ScheduleId) -> Result<Option<Schedule>> {
        #[derive(sqlx::FromRow)]
        struct ScheduleRow {
            id: Uuid,
            workflow_id: Uuid,
            name: String,
            description: Option<String>,
            cron: String,
            timezone: String,
            enabled: bool,
            input_variables: serde_json::Value,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            last_run: Option<DateTime<Utc>>,
            next_run: Option<DateTime<Utc>>,
            run_count: i64,
            max_runs: Option<i64>,
            expires_at: Option<DateTime<Utc>>,
        }

        let row = sqlx::query_as::<_, ScheduleRow>(
            r"
            SELECT id, workflow_id, name, description, cron, timezone, enabled,
                   input_variables, created_at, updated_at, last_run, next_run,
                   run_count, max_runs, expires_at
            FROM schedules
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => {
                let input_variables = serde_json::from_value(row.input_variables)?;
                Ok(Some(Schedule {
                    id: row.id,
                    workflow_id: row.workflow_id,
                    name: row.name,
                    description: row.description,
                    cron: row.cron,
                    timezone: row.timezone,
                    enabled: row.enabled,
                    input_variables,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    last_run: row.last_run,
                    next_run: row.next_run,
                    run_count: row.run_count as u64,
                    max_runs: row.max_runs.map(|v| v as u64),
                    expires_at: row.expires_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// List all schedules
    pub async fn list(&self) -> Result<Vec<Schedule>> {
        self.list_with_filter(None, None).await
    }

    /// List schedules for a specific workflow
    pub async fn list_by_workflow(&self, workflow_id: &WorkflowId) -> Result<Vec<Schedule>> {
        self.list_with_filter(Some(workflow_id), None).await
    }

    /// List enabled schedules
    pub async fn list_enabled(&self) -> Result<Vec<Schedule>> {
        self.list_with_filter(None, Some(true)).await
    }

    /// List schedules with optional filters
    async fn list_with_filter(
        &self,
        workflow_id: Option<&WorkflowId>,
        enabled: Option<bool>,
    ) -> Result<Vec<Schedule>> {
        #[derive(sqlx::FromRow)]
        struct ScheduleRow {
            id: Uuid,
            workflow_id: Uuid,
            name: String,
            description: Option<String>,
            cron: String,
            timezone: String,
            enabled: bool,
            input_variables: serde_json::Value,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            last_run: Option<DateTime<Utc>>,
            next_run: Option<DateTime<Utc>>,
            run_count: i64,
            max_runs: Option<i64>,
            expires_at: Option<DateTime<Utc>>,
        }

        let mut query = String::from(
            r"
            SELECT id, workflow_id, name, description, cron, timezone, enabled,
                   input_variables, created_at, updated_at, last_run, next_run,
                   run_count, max_runs, expires_at
            FROM schedules
            WHERE 1=1
            ",
        );

        if workflow_id.is_some() {
            query.push_str(" AND workflow_id = $1");
        }
        if enabled.is_some() {
            if workflow_id.is_some() {
                query.push_str(" AND enabled = $2");
            } else {
                query.push_str(" AND enabled = $1");
            }
        }

        query.push_str(" ORDER BY created_at DESC");

        let mut q = sqlx::query_as::<_, ScheduleRow>(&query);
        if let Some(wid) = workflow_id {
            q = q.bind(wid);
        }
        if let Some(e) = enabled {
            q = q.bind(e);
        }

        let rows = q.fetch_all(self.pool.pool()).await?;

        let schedules = rows
            .into_iter()
            .filter_map(|row| {
                let input_variables = serde_json::from_value(row.input_variables).ok()?;
                Some(Schedule {
                    id: row.id,
                    workflow_id: row.workflow_id,
                    name: row.name,
                    description: row.description,
                    cron: row.cron,
                    timezone: row.timezone,
                    enabled: row.enabled,
                    input_variables,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    last_run: row.last_run,
                    next_run: row.next_run,
                    run_count: row.run_count as u64,
                    max_runs: row.max_runs.map(|v| v as u64),
                    expires_at: row.expires_at,
                })
            })
            .collect();

        Ok(schedules)
    }

    /// Update a schedule
    pub async fn update(&self, id: &ScheduleId, schedule: &Schedule) -> Result<bool> {
        // Validate cron expression
        validate_cron(&schedule.cron)?;

        // Validate timezone
        validate_timezone(&schedule.timezone)?;

        let input_vars = serde_json::to_value(&schedule.input_variables)?;

        let result = sqlx::query(
            r"
            UPDATE schedules
            SET name = $2, description = $3, cron = $4, timezone = $5, enabled = $6,
                input_variables = $7, last_run = $8, next_run = $9, run_count = $10,
                max_runs = $11, expires_at = $12, updated_at = $13
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(&schedule.name)
        .bind(&schedule.description)
        .bind(&schedule.cron)
        .bind(&schedule.timezone)
        .bind(schedule.enabled)
        .bind(input_vars)
        .bind(schedule.last_run)
        .bind(schedule.next_run)
        .bind(schedule.run_count as i64)
        .bind(schedule.max_runs.map(|v| v as i64))
        .bind(schedule.expires_at)
        .bind(Utc::now())
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete a schedule
    pub async fn delete(&self, id: &ScheduleId) -> Result<bool> {
        let result = sqlx::query(
            r"
            DELETE FROM schedules
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Record a schedule execution
    pub async fn record_execution(&self, execution: &ScheduleExecution) -> Result<Uuid> {
        sqlx::query(
            r"
            INSERT INTO schedule_executions
            (id, schedule_id, execution_id, triggered_at, success, error, duration_ms)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ",
        )
        .bind(execution.id)
        .bind(execution.schedule_id)
        .bind(execution.execution_id)
        .bind(execution.triggered_at)
        .bind(execution.success)
        .bind(&execution.error)
        .bind(execution.duration_ms.map(|v| v as i64))
        .execute(self.pool.pool())
        .await?;

        Ok(execution.id)
    }

    /// Get execution history for a schedule
    pub async fn get_execution_history(
        &self,
        schedule_id: &ScheduleId,
        limit: i64,
    ) -> Result<Vec<ScheduleExecution>> {
        #[derive(sqlx::FromRow)]
        struct ExecutionRow {
            id: Uuid,
            schedule_id: Uuid,
            execution_id: Uuid,
            triggered_at: DateTime<Utc>,
            success: bool,
            error: Option<String>,
            duration_ms: Option<i64>,
        }

        let rows = sqlx::query_as::<_, ExecutionRow>(
            r"
            SELECT id, schedule_id, execution_id, triggered_at, success, error, duration_ms
            FROM schedule_executions
            WHERE schedule_id = $1
            ORDER BY triggered_at DESC
            LIMIT $2
            ",
        )
        .bind(schedule_id)
        .bind(limit)
        .fetch_all(self.pool.pool())
        .await?;

        let executions = rows
            .into_iter()
            .map(|row| ScheduleExecution {
                id: row.id,
                schedule_id: row.schedule_id,
                execution_id: row.execution_id,
                triggered_at: row.triggered_at,
                success: row.success,
                error: row.error,
                duration_ms: row.duration_ms.map(|v| v as u64),
            })
            .collect();

        Ok(executions)
    }

    /// Disable a schedule (set enabled = false)
    /// Returns true if the schedule was found and updated
    pub async fn disable(&self, id: &ScheduleId) -> Result<bool> {
        let result = sqlx::query(
            r"
            UPDATE schedules
            SET enabled = false, updated_at = $2
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(chrono::Utc::now())
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Enable a schedule (set enabled = true)
    /// Returns true if the schedule was found and updated
    pub async fn enable(&self, id: &ScheduleId) -> Result<bool> {
        let result = sqlx::query(
            r"
            UPDATE schedules
            SET enabled = true, updated_at = $2
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(chrono::Utc::now())
        .execute(self.pool.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    async fn setup_test_pool() -> Result<DatabasePool> {
        let config = crate::DatabaseConfig {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost/oxify_test".to_string()),
            ..Default::default()
        };
        DatabasePool::new(config).await
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_schedule_crud() -> Result<()> {
        let pool = setup_test_pool().await?;
        pool.migrate().await?;

        let store = ScheduleStore::new(pool);

        // Create test schedule
        let workflow_id = Uuid::new_v4();
        let schedule = Schedule::new(
            workflow_id,
            "Daily Report".to_string(),
            "0 0 * * *".to_string(),
        );
        let id = schedule.id;

        // Create
        store.create(&schedule).await?;

        // Get
        let fetched = store.get(&id).await?;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, "Daily Report");

        // Update
        let mut updated = schedule.clone();
        updated.name = "Updated Report".to_string();
        let result = store.update(&id, &updated).await?;
        assert!(result);

        // Delete
        let result = store.delete(&id).await?;
        assert!(result);

        // Verify deleted
        let fetched = store.get(&id).await?;
        assert!(fetched.is_none());

        Ok(())
    }
}
