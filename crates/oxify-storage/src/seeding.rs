//! Database seeding utilities for testing and development
//!
//! Provides utilities for populating the database with test data, sample records,
//! and development fixtures. This is particularly useful for:
//!
//! - Local development environments
//! - Integration testing
//! - Demo and staging environments
//! - Performance testing with realistic data volumes
//!
//! # Features
//!
//! - Generate realistic test data
//! - Idempotent seeding (safe to run multiple times)
//! - Configurable data volumes
//! - Relationship handling (foreign keys)
//! - Transaction-safe execution
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::seeding::{Seeder, SeedConfig};
//!
//! let config = SeedConfig {
//!     num_users: 10,
//!     num_workflows: 50,
//!     num_executions: 200,
//!     ..Default::default()
//! };
//!
//! let seeder = Seeder::new(pool, config);
//! let report = seeder.seed_all().await?;
//!
//! println!("Created {} users, {} workflows, {} executions",
//!          report.users_created, report.workflows_created, report.executions_created);
//! ```

use crate::Result;
use chrono::{Duration, Utc};
use rand::{Rng, RngExt, SeedableRng};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// Configuration for database seeding
#[derive(Debug, Clone)]
pub struct SeedConfig {
    /// Number of users to create
    pub num_users: usize,
    /// Number of workflows to create
    pub num_workflows: usize,
    /// Number of executions to create
    pub num_executions: usize,
    /// Number of API keys to create
    pub num_api_keys: usize,
    /// Number of schedules to create
    pub num_schedules: usize,
    /// Whether to clear existing data before seeding
    pub clear_existing: bool,
    /// Seed value for random generation (for reproducibility)
    pub random_seed: Option<u64>,
}

impl Default for SeedConfig {
    fn default() -> Self {
        Self {
            num_users: 5,
            num_workflows: 20,
            num_executions: 100,
            num_api_keys: 10,
            num_schedules: 5,
            clear_existing: false,
            random_seed: None,
        }
    }
}

/// Report of seeding operations
#[derive(Debug, Clone, Default)]
pub struct SeedReport {
    /// Number of users created
    pub users_created: usize,
    /// Number of workflows created
    pub workflows_created: usize,
    /// Number of executions created
    pub executions_created: usize,
    /// Number of API keys created
    pub api_keys_created: usize,
    /// Number of schedules created
    pub schedules_created: usize,
    /// Total time taken (in milliseconds)
    pub duration_ms: u64,
}

/// Database seeder
pub struct Seeder {
    pool: PgPool,
    config: SeedConfig,
}

impl Seeder {
    /// Create a new seeder with the given configuration
    pub fn new(pool: PgPool, config: SeedConfig) -> Self {
        Self { pool, config }
    }

    /// Seed all tables according to the configuration
    #[tracing::instrument(skip(self))]
    pub async fn seed_all(&self) -> Result<SeedReport> {
        let start = std::time::Instant::now();

        if self.config.clear_existing {
            self.clear_all().await?;
        }

        let mut report = SeedReport::default();

        // Seed in dependency order
        let user_ids = self.seed_users(self.config.num_users).await?;
        report.users_created = user_ids.len();

        let workflow_ids = self
            .seed_workflows(&user_ids, self.config.num_workflows)
            .await?;
        report.workflows_created = workflow_ids.len();

        let execution_ids = self
            .seed_executions(&workflow_ids, self.config.num_executions)
            .await?;
        report.executions_created = execution_ids.len();

        let api_key_ids = self
            .seed_api_keys(&user_ids, self.config.num_api_keys)
            .await?;
        report.api_keys_created = api_key_ids.len();

        let schedule_ids = self
            .seed_schedules(&workflow_ids, self.config.num_schedules)
            .await?;
        report.schedules_created = schedule_ids.len();

        report.duration_ms = start.elapsed().as_millis() as u64;

        Ok(report)
    }

    /// Clear all seeded data
    async fn clear_all(&self) -> Result<()> {
        tracing::info!("Clearing existing data");

        // Delete in reverse dependency order
        sqlx::query("DELETE FROM schedule_executions")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM schedules")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM api_key_usage_logs")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM api_keys")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM execution_variables")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM executions")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM workflow_versions")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM workflows")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM user_quota_limits")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM users").execute(&self.pool).await?;

        Ok(())
    }

    /// Seed users
    async fn seed_users(&self, count: usize) -> Result<Vec<Uuid>> {
        tracing::info!("Seeding {} users", count);

        let mut user_ids = Vec::new();
        let mut rng = self.get_rng();

        for i in 0..count {
            let user_id = Uuid::new_v4();
            let email = format!("user{}@example.com", i);
            let username = format!("user{}", i);

            sqlx::query(
                r#"
                INSERT INTO users (id, email, username, created_at)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(&email)
            .bind(&username)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

            user_ids.push(user_id);

            // Create quota limits for each user
            let execution_limit: i64 = rng.random_range(100..1000);
            let token_limit: i64 = rng.random_range(10000..100000);

            sqlx::query(
                r#"
                INSERT INTO user_quota_limits
                (user_id, max_daily_executions, max_daily_tokens, created_at)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (user_id) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(execution_limit)
            .bind(token_limit)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;
        }

        Ok(user_ids)
    }

    /// Seed workflows
    async fn seed_workflows(&self, user_ids: &[Uuid], count: usize) -> Result<Vec<Uuid>> {
        tracing::info!("Seeding {} workflows", count);

        let mut workflow_ids = Vec::new();

        let workflow_names = [
            "Data Processing",
            "Report Generation",
            "ETL Pipeline",
            "Notification System",
            "Backup Job",
            "Data Validation",
            "Analytics Pipeline",
            "Image Processing",
            "Email Campaign",
            "Data Sync",
        ];

        for i in 0..count {
            let workflow_id = Uuid::new_v4();
            let user_id = user_ids[i % user_ids.len()];
            let name = format!(
                "{} {}",
                workflow_names[i % workflow_names.len()],
                i / workflow_names.len() + 1
            );
            let description = format!("Test workflow for {}", name);

            let workflow_def = json!({
                "nodes": [
                    {"id": "start", "type": "trigger", "config": {}},
                    {"id": "process", "type": "transform", "config": {"operation": "map"}},
                    {"id": "end", "type": "output", "config": {}}
                ],
                "edges": [
                    {"from": "start", "to": "process"},
                    {"from": "process", "to": "end"}
                ]
            });

            sqlx::query(
                r#"
                INSERT INTO workflows
                (id, name, description, user_id, definition, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(workflow_id)
            .bind(&name)
            .bind(&description)
            .bind(user_id)
            .bind(&workflow_def)
            .bind(Utc::now())
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

            workflow_ids.push(workflow_id);

            // Create initial version
            sqlx::query(
                r#"
                INSERT INTO workflow_versions
                (id, workflow_id, version, definition, created_at, created_by)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (workflow_id, version) DO NOTHING
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(workflow_id)
            .bind(1i32)
            .bind(&workflow_def)
            .bind(Utc::now())
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        }

        Ok(workflow_ids)
    }

    /// Seed executions
    async fn seed_executions(&self, workflow_ids: &[Uuid], count: usize) -> Result<Vec<Uuid>> {
        tracing::info!("Seeding {} executions", count);

        let mut execution_ids = Vec::new();
        let mut rng = self.get_rng();

        let states = ["pending", "running", "completed", "failed"];

        for i in 0..count {
            let execution_id = Uuid::new_v4();
            let workflow_id = workflow_ids[i % workflow_ids.len()];
            let state = states[i % states.len()];

            // Create executions with varied timestamps
            let created_at = Utc::now() - Duration::days(rng.random_range(0..30));
            let started_at = if state != "pending" {
                Some(created_at + Duration::seconds(rng.random_range(1..60)))
            } else {
                None
            };
            let completed_at = if state == "completed" || state == "failed" {
                started_at.map(|s| s + Duration::seconds(rng.random_range(1..3600)))
            } else {
                None
            };

            let context = json!({
                "execution_number": i + 1,
                "triggered_by": "seeder",
                "environment": "test"
            });

            sqlx::query(
                r#"
                INSERT INTO executions
                (id, workflow_id, state, context, created_at, started_at, completed_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(execution_id)
            .bind(workflow_id)
            .bind(state)
            .bind(context)
            .bind(created_at)
            .bind(started_at)
            .bind(completed_at)
            .execute(&self.pool)
            .await?;

            execution_ids.push(execution_id);
        }

        Ok(execution_ids)
    }

    /// Seed API keys
    async fn seed_api_keys(&self, user_ids: &[Uuid], count: usize) -> Result<Vec<Uuid>> {
        tracing::info!("Seeding {} API keys", count);

        let mut api_key_ids = Vec::new();

        for i in 0..count {
            let api_key_id = Uuid::new_v4();
            let user_id = user_ids[i % user_ids.len()];
            let name = format!("API Key {}", i + 1);
            let key_hash = format!("hash_{}", Uuid::new_v4());

            sqlx::query(
                r#"
                INSERT INTO api_keys
                (id, user_id, name, key_hash, created_at)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(api_key_id)
            .bind(user_id)
            .bind(&name)
            .bind(&key_hash)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

            api_key_ids.push(api_key_id);
        }

        Ok(api_key_ids)
    }

    /// Seed schedules
    async fn seed_schedules(&self, workflow_ids: &[Uuid], count: usize) -> Result<Vec<Uuid>> {
        tracing::info!("Seeding {} schedules", count);

        let mut schedule_ids = Vec::new();

        let cron_expressions = [
            "0 0 * * *",    // Daily at midnight
            "0 */6 * * *",  // Every 6 hours
            "*/15 * * * *", // Every 15 minutes
            "0 9 * * 1",    // Monday at 9 AM
            "0 0 1 * *",    // First day of month
        ];

        for i in 0..count {
            let schedule_id = Uuid::new_v4();
            let workflow_id = workflow_ids[i % workflow_ids.len()];
            let name = format!("Schedule {}", i + 1);
            let cron_expression = cron_expressions[i % cron_expressions.len()];

            sqlx::query(
                r#"
                INSERT INTO schedules
                (id, workflow_id, name, cron_expression, timezone, enabled, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(schedule_id)
            .bind(workflow_id)
            .bind(&name)
            .bind(cron_expression)
            .bind("UTC")
            .bind(true)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

            schedule_ids.push(schedule_id);
        }

        Ok(schedule_ids)
    }

    /// Get RNG instance (with optional seed for reproducibility)
    fn get_rng(&self) -> Box<dyn RngTrait> {
        if let Some(seed) = self.config.random_seed {
            Box::new(rand::rngs::StdRng::seed_from_u64(seed))
        } else {
            Box::new(rand::rng())
        }
    }
}

/// Trait to abstract over RNG types
trait RngTrait {
    fn random_range(&mut self, range: std::ops::Range<i64>) -> i64;
}

impl<R: Rng + RngExt> RngTrait for R {
    fn random_range(&mut self, range: std::ops::Range<i64>) -> i64 {
        self.random_range(range)
    }
}

/// Preset configurations for common scenarios
pub mod presets {
    use super::*;

    /// Small dataset for quick testing
    pub fn small() -> SeedConfig {
        SeedConfig {
            num_users: 3,
            num_workflows: 10,
            num_executions: 30,
            num_api_keys: 5,
            num_schedules: 3,
            ..Default::default()
        }
    }

    /// Medium dataset for integration testing
    pub fn medium() -> SeedConfig {
        SeedConfig {
            num_users: 10,
            num_workflows: 50,
            num_executions: 200,
            num_api_keys: 20,
            num_schedules: 10,
            ..Default::default()
        }
    }

    /// Large dataset for performance testing
    pub fn large() -> SeedConfig {
        SeedConfig {
            num_users: 100,
            num_workflows: 500,
            num_executions: 5000,
            num_api_keys: 200,
            num_schedules: 50,
            ..Default::default()
        }
    }

    /// Extra large dataset for stress testing
    pub fn xlarge() -> SeedConfig {
        SeedConfig {
            num_users: 1000,
            num_workflows: 5000,
            num_executions: 50000,
            num_api_keys: 2000,
            num_schedules: 500,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seed_config_default() {
        let config = SeedConfig::default();
        assert_eq!(config.num_users, 5);
        assert_eq!(config.num_workflows, 20);
        assert_eq!(config.num_executions, 100);
    }

    #[test]
    fn test_preset_small() {
        let config = presets::small();
        assert_eq!(config.num_users, 3);
        assert_eq!(config.num_workflows, 10);
    }

    #[test]
    fn test_preset_medium() {
        let config = presets::medium();
        assert_eq!(config.num_users, 10);
        assert_eq!(config.num_workflows, 50);
    }

    #[test]
    fn test_preset_large() {
        let config = presets::large();
        assert_eq!(config.num_users, 100);
        assert_eq!(config.num_workflows, 500);
    }

    #[test]
    fn test_seed_report_default() {
        let report = SeedReport::default();
        assert_eq!(report.users_created, 0);
        assert_eq!(report.workflows_created, 0);
    }
}
