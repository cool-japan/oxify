//! Execution Metrics Storage
//!
//! Provides analytics and performance tracking for workflow executions.
//!
//! ## Overview
//!
//! The metrics system tracks:
//! - **Execution Metrics**: Duration, success/failure, token usage, costs
//! - **Node Metrics**: Per-node execution times and outcomes
//! - **System Metrics**: Overall system performance and resource usage
//! - **Workflow Statistics**: Aggregated stats per workflow
//!
//! ## Time-Bucketing Strategy
//!
//! Metrics are stored in hourly buckets for efficient querying and aggregation:
//! - All timestamps are normalized to the start of the hour
//! - Example: Executions from 14:00 to 14:59 are all bucketed as 14:00
//! - This allows for fast range queries and pre-computed aggregations
//!
//! ## Data Aggregation
//!
//! For each time bucket, we store:
//! - Count, sum, min, max, and average for durations
//! - Success and failure counts
//! - Total token usage and costs
//! - Percentiles (p50, p95, p99) for latency analysis
//!
//! ## Usage Example
//!
//! ```ignore
//! use oxify_storage::{MetricsStore, TimeRange};
//! use chrono::{Utc, Duration};
//!
//! let metrics_store = MetricsStore::new(pool);
//!
//! // Record execution metrics
//! metrics_store.record_execution(
//!     workflow_id,
//!     execution_id,
//!     duration_ms,
//!     success,
//!     tokens_used,
//!     cost_cents,
//! ).await?;
//!
//! // Query metrics for the last 24 hours
//! let range = TimeRange {
//!     start: Utc::now() - Duration::days(1),
//!     end: Utc::now(),
//! };
//! let metrics = metrics_store.get_execution_metrics(&workflow_id, &range).await?;
//! println!("Average duration: {}ms", metrics.avg_duration_ms.unwrap_or(0));
//! ```

use crate::{DatabasePool, Result};
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Execution metrics for a workflow (aggregated by time bucket)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub time_bucket: DateTime<Utc>,
    pub total_executions: i32,
    pub successful_executions: i32,
    pub failed_executions: i32,
    pub cancelled_executions: i32,
    pub total_duration_ms: i64,
    pub min_duration_ms: Option<i32>,
    pub max_duration_ms: Option<i32>,
    pub avg_duration_ms: Option<i32>,
    pub p50_duration_ms: Option<i32>,
    pub p95_duration_ms: Option<i32>,
    pub p99_duration_ms: Option<i32>,
    pub total_nodes_executed: i32,
    pub total_node_failures: i32,
    pub total_retries: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub estimated_cost_usd: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// Node-level metrics (aggregated by time bucket)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub node_id: String,
    pub node_type: String,
    pub time_bucket: DateTime<Utc>,
    pub total_executions: i32,
    pub successful_executions: i32,
    pub failed_executions: i32,
    pub total_duration_ms: i64,
    pub min_duration_ms: Option<i32>,
    pub max_duration_ms: Option<i32>,
    pub avg_duration_ms: Option<i32>,
    pub total_retries: i32,
    pub max_retries_single_execution: Option<i32>,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub created_at: DateTime<Utc>,
}

/// System-wide metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub total_workflows: i32,
    pub active_workflows: i32,
    pub total_executions: i64,
    pub active_executions: i32,
    pub executions_last_hour: i32,
    pub executions_last_day: i32,
    pub total_users: i32,
    pub active_users_last_hour: i32,
    pub active_users_last_day: i32,
    pub total_api_keys: i32,
    pub total_secrets: i32,
    pub total_schedules: i32,
    pub total_webhooks: i32,
    pub total_storage_bytes: Option<i64>,
    pub audit_logs_count: i64,
    pub avg_execution_duration_ms: Option<i32>,
    pub success_rate_percent: Option<f64>,
    pub metadata: JsonValue,
}

/// Workflow summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStats {
    pub workflow_id: Uuid,
    pub total_executions: i64,
    pub successful_executions: i64,
    pub failed_executions: i64,
    pub success_rate: f64,
    pub avg_duration_ms: Option<i64>,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub last_executed_at: Option<DateTime<Utc>>,
}

/// Execution record for metrics tracking
#[derive(Debug, Clone)]
pub struct ExecutionRecord {
    pub execution_id: Uuid,
    pub workflow_id: Uuid,
    pub success: bool,
    pub duration_ms: i32,
    pub nodes_executed: i32,
    pub node_failures: i32,
    pub retries: i32,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: Option<f64>,
}

/// Node execution record for metrics tracking
#[derive(Debug, Clone)]
pub struct NodeExecutionRecord {
    pub execution_id: Uuid,
    pub workflow_id: Uuid,
    pub node_id: String,
    pub node_type: String,
    pub success: bool,
    pub duration_ms: i32,
    pub retries: i32,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Time range for metrics queries
#[derive(Debug, Clone, Copy)]
pub enum TimeRange {
    LastHour,
    Last24Hours,
    Last7Days,
    Last30Days,
    Custom {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
}

impl TimeRange {
    pub fn to_interval(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        match self {
            TimeRange::LastHour => (now - Duration::hours(1), now),
            TimeRange::Last24Hours => (now - Duration::hours(24), now),
            TimeRange::Last7Days => (now - Duration::days(7), now),
            TimeRange::Last30Days => (now - Duration::days(30), now),
            TimeRange::Custom { from, to } => (*from, *to),
        }
    }
}

/// Metrics storage layer
#[derive(Clone)]
pub struct MetricsStore {
    pool: DatabasePool,
}

impl MetricsStore {
    /// Create a new metrics store
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Get the start of the current hour (for time bucketing)
    fn current_time_bucket() -> DateTime<Utc> {
        let now = Utc::now();
        now.date_naive()
            .and_hms_opt(now.hour(), 0, 0)
            .expect("Invalid time")
            .and_utc()
    }

    /// Record an execution for metrics tracking
    pub async fn record_execution(&self, record: &ExecutionRecord) -> Result<()> {
        let time_bucket = Self::current_time_bucket();

        // First, store the individual execution duration for percentile calculation
        sqlx::query(
            r"
            INSERT INTO execution_durations (execution_id, workflow_id, time_bucket, duration_ms, created_at)
            VALUES ($1, $2, $3, $4, NOW())
            ",
        )
        .bind(record.execution_id)
        .bind(record.workflow_id)
        .bind(time_bucket)
        .bind(record.duration_ms)
        .execute(self.pool.pool())
        .await?;

        // Upsert execution metrics
        sqlx::query(
            r"
            INSERT INTO execution_metrics (
                id, workflow_id, time_bucket,
                total_executions, successful_executions, failed_executions,
                total_duration_ms, min_duration_ms, max_duration_ms, avg_duration_ms,
                total_nodes_executed, total_node_failures, total_retries,
                total_input_tokens, total_output_tokens, estimated_cost_usd
            )
            VALUES ($1, $2, $3, 1, $4, $5, $6, $6, $6, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (workflow_id, time_bucket)
            DO UPDATE SET
                total_executions = execution_metrics.total_executions + 1,
                successful_executions = execution_metrics.successful_executions + $4,
                failed_executions = execution_metrics.failed_executions + $5,
                total_duration_ms = execution_metrics.total_duration_ms + $6,
                min_duration_ms = LEAST(execution_metrics.min_duration_ms, $6),
                max_duration_ms = GREATEST(execution_metrics.max_duration_ms, $6),
                avg_duration_ms = ((execution_metrics.total_duration_ms + $6) /
                                   (execution_metrics.total_executions + 1))::INTEGER,
                total_nodes_executed = execution_metrics.total_nodes_executed + $7,
                total_node_failures = execution_metrics.total_node_failures + $8,
                total_retries = execution_metrics.total_retries + $9,
                total_input_tokens = execution_metrics.total_input_tokens + $10,
                total_output_tokens = execution_metrics.total_output_tokens + $11,
                estimated_cost_usd = COALESCE(execution_metrics.estimated_cost_usd, 0) + COALESCE($12, 0)
            ",
        )
        .bind(Uuid::new_v4())
        .bind(record.workflow_id)
        .bind(time_bucket)
        .bind(i32::from(record.success))
        .bind(i32::from(!record.success))
        .bind(record.duration_ms)
        .bind(record.nodes_executed)
        .bind(record.node_failures)
        .bind(record.retries)
        .bind(record.input_tokens)
        .bind(record.output_tokens)
        .bind(record.cost_usd)
        .execute(self.pool.pool())
        .await?;

        // Calculate and update percentiles for this time bucket
        self.update_percentiles_for_bucket(record.workflow_id, time_bucket)
            .await?;

        Ok(())
    }

    /// Batch record multiple execution metrics
    ///
    /// This is more efficient than calling `record_execution()` multiple times
    /// as it uses a single database transaction. Percentiles are calculated
    /// once per unique (workflow_id, time_bucket) combination after all inserts.
    ///
    /// Returns the number of execution records processed.
    pub async fn batch_record_executions(&self, records: &[ExecutionRecord]) -> Result<u64> {
        if records.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.pool().begin().await?;

        // Track unique (workflow_id, time_bucket) combinations for percentile updates
        let mut buckets = std::collections::HashSet::new();

        for record in records {
            let time_bucket = Self::current_time_bucket();
            buckets.insert((record.workflow_id, time_bucket));

            // Store individual execution duration for percentile calculation
            sqlx::query(
                r"
                INSERT INTO execution_durations (execution_id, workflow_id, time_bucket, duration_ms, created_at)
                VALUES ($1, $2, $3, $4, NOW())
                ",
            )
            .bind(record.execution_id)
            .bind(record.workflow_id)
            .bind(time_bucket)
            .bind(record.duration_ms)
            .execute(&mut *tx)
            .await?;

            // Upsert execution metrics
            sqlx::query(
                r"
                INSERT INTO execution_metrics (
                    id, workflow_id, time_bucket,
                    total_executions, successful_executions, failed_executions,
                    total_duration_ms, min_duration_ms, max_duration_ms, avg_duration_ms,
                    total_nodes_executed, total_node_failures, total_retries,
                    total_input_tokens, total_output_tokens, estimated_cost_usd
                )
                VALUES ($1, $2, $3, 1, $4, $5, $6, $6, $6, $6, $7, $8, $9, $10, $11, $12)
                ON CONFLICT (workflow_id, time_bucket)
                DO UPDATE SET
                    total_executions = execution_metrics.total_executions + 1,
                    successful_executions = execution_metrics.successful_executions + $4,
                    failed_executions = execution_metrics.failed_executions + $5,
                    total_duration_ms = execution_metrics.total_duration_ms + $6,
                    min_duration_ms = LEAST(execution_metrics.min_duration_ms, $6),
                    max_duration_ms = GREATEST(execution_metrics.max_duration_ms, $6),
                    avg_duration_ms = ((execution_metrics.total_duration_ms + $6) /
                                       (execution_metrics.total_executions + 1))::INTEGER,
                    total_nodes_executed = execution_metrics.total_nodes_executed + $7,
                    total_node_failures = execution_metrics.total_node_failures + $8,
                    total_retries = execution_metrics.total_retries + $9,
                    total_input_tokens = execution_metrics.total_input_tokens + $10,
                    total_output_tokens = execution_metrics.total_output_tokens + $11,
                    estimated_cost_usd = COALESCE(execution_metrics.estimated_cost_usd, 0) + COALESCE($12, 0)
                ",
            )
            .bind(Uuid::new_v4())
            .bind(record.workflow_id)
            .bind(time_bucket)
            .bind(i32::from(record.success))
            .bind(i32::from(!record.success))
            .bind(record.duration_ms)
            .bind(record.nodes_executed)
            .bind(record.node_failures)
            .bind(record.retries)
            .bind(record.input_tokens)
            .bind(record.output_tokens)
            .bind(record.cost_usd)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        // Update percentiles for all affected buckets
        for (workflow_id, time_bucket) in buckets {
            self.update_percentiles_for_bucket(workflow_id, time_bucket)
                .await?;
        }

        Ok(records.len() as u64)
    }

    /// Calculate and update percentiles for a specific time bucket
    ///
    /// Uses PostgreSQL's percentile_cont function to calculate p50, p95, and p99
    /// from all execution durations in the time bucket.
    async fn update_percentiles_for_bucket(
        &self,
        workflow_id: Uuid,
        time_bucket: DateTime<Utc>,
    ) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct Percentiles {
            p50: Option<f64>,
            p95: Option<f64>,
            p99: Option<f64>,
        }

        // Calculate percentiles using PostgreSQL's percentile_cont aggregate function
        let percentiles = sqlx::query_as::<_, Percentiles>(
            r"
            SELECT
                percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms) as p50,
                percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) as p95,
                percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_ms) as p99
            FROM execution_durations
            WHERE workflow_id = $1 AND time_bucket = $2
            ",
        )
        .bind(workflow_id)
        .bind(time_bucket)
        .fetch_one(self.pool.pool())
        .await?;

        // Update the execution_metrics table with calculated percentiles
        sqlx::query(
            r"
            UPDATE execution_metrics
            SET p50_duration_ms = $1,
                p95_duration_ms = $2,
                p99_duration_ms = $3
            WHERE workflow_id = $4 AND time_bucket = $5
            ",
        )
        .bind(percentiles.p50.map(|p| p.round() as i32))
        .bind(percentiles.p95.map(|p| p.round() as i32))
        .bind(percentiles.p99.map(|p| p.round() as i32))
        .bind(workflow_id)
        .bind(time_bucket)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Record a node execution for metrics tracking
    pub async fn record_node_execution(&self, record: &NodeExecutionRecord) -> Result<()> {
        let time_bucket = Self::current_time_bucket();

        sqlx::query(
            r"
            INSERT INTO node_metrics (
                id, workflow_id, node_id, node_type, time_bucket,
                total_executions, successful_executions, failed_executions,
                total_duration_ms, min_duration_ms, max_duration_ms, avg_duration_ms,
                total_retries, max_retries_single_execution,
                total_input_tokens, total_output_tokens
            )
            VALUES ($1, $2, $3, $4, $5, 1, $6, $7, $8, $8, $8, $8, $9, $9, $10, $11)
            ON CONFLICT (workflow_id, node_id, time_bucket)
            DO UPDATE SET
                total_executions = node_metrics.total_executions + 1,
                successful_executions = node_metrics.successful_executions + $6,
                failed_executions = node_metrics.failed_executions + $7,
                total_duration_ms = node_metrics.total_duration_ms + $8,
                min_duration_ms = LEAST(node_metrics.min_duration_ms, $8),
                max_duration_ms = GREATEST(node_metrics.max_duration_ms, $8),
                avg_duration_ms = ((node_metrics.total_duration_ms + $8) /
                                   (node_metrics.total_executions + 1))::INTEGER,
                total_retries = node_metrics.total_retries + $9,
                max_retries_single_execution = GREATEST(node_metrics.max_retries_single_execution, $9),
                total_input_tokens = node_metrics.total_input_tokens + $10,
                total_output_tokens = node_metrics.total_output_tokens + $11
            ",
        )
        .bind(Uuid::new_v4())
        .bind(record.workflow_id)
        .bind(&record.node_id)
        .bind(&record.node_type)
        .bind(time_bucket)
        .bind(i32::from(record.success))
        .bind(i32::from(!record.success))
        .bind(record.duration_ms)
        .bind(record.retries)
        .bind(record.input_tokens)
        .bind(record.output_tokens)
        .execute(self.pool.pool())
        .await?;

        Ok(())
    }

    /// Get execution metrics for a workflow within a time range
    pub async fn get_execution_metrics(
        &self,
        workflow_id: &Uuid,
        time_range: TimeRange,
    ) -> Result<Vec<ExecutionMetrics>> {
        let (from, to) = time_range.to_interval();

        #[derive(sqlx::FromRow)]
        struct MetricsRow {
            id: Uuid,
            workflow_id: Uuid,
            time_bucket: DateTime<Utc>,
            total_executions: i32,
            successful_executions: i32,
            failed_executions: i32,
            cancelled_executions: i32,
            total_duration_ms: i64,
            min_duration_ms: Option<i32>,
            max_duration_ms: Option<i32>,
            avg_duration_ms: Option<i32>,
            p50_duration_ms: Option<i32>,
            p95_duration_ms: Option<i32>,
            p99_duration_ms: Option<i32>,
            total_nodes_executed: i32,
            total_node_failures: i32,
            total_retries: i32,
            total_input_tokens: i64,
            total_output_tokens: i64,
            estimated_cost_usd: Option<rust_decimal::Decimal>,
            created_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, MetricsRow>(
            r"
            SELECT id, workflow_id, time_bucket,
                   total_executions, successful_executions, failed_executions, cancelled_executions,
                   total_duration_ms, min_duration_ms, max_duration_ms, avg_duration_ms,
                   p50_duration_ms, p95_duration_ms, p99_duration_ms,
                   total_nodes_executed, total_node_failures, total_retries,
                   total_input_tokens, total_output_tokens, estimated_cost_usd,
                   created_at
            FROM execution_metrics
            WHERE workflow_id = $1 AND time_bucket >= $2 AND time_bucket <= $3
            ORDER BY time_bucket DESC
            ",
        )
        .bind(workflow_id)
        .bind(from)
        .bind(to)
        .fetch_all(self.pool.pool())
        .await?;

        let metrics = rows
            .into_iter()
            .map(|row| ExecutionMetrics {
                id: row.id,
                workflow_id: row.workflow_id,
                time_bucket: row.time_bucket,
                total_executions: row.total_executions,
                successful_executions: row.successful_executions,
                failed_executions: row.failed_executions,
                cancelled_executions: row.cancelled_executions,
                total_duration_ms: row.total_duration_ms,
                min_duration_ms: row.min_duration_ms,
                max_duration_ms: row.max_duration_ms,
                avg_duration_ms: row.avg_duration_ms,
                p50_duration_ms: row.p50_duration_ms,
                p95_duration_ms: row.p95_duration_ms,
                p99_duration_ms: row.p99_duration_ms,
                total_nodes_executed: row.total_nodes_executed,
                total_node_failures: row.total_node_failures,
                total_retries: row.total_retries,
                total_input_tokens: row.total_input_tokens,
                total_output_tokens: row.total_output_tokens,
                estimated_cost_usd: row
                    .estimated_cost_usd
                    .map(|d| d.to_string().parse().unwrap_or(0.0)),
                created_at: row.created_at,
            })
            .collect();

        Ok(metrics)
    }

    /// Get node metrics for a workflow within a time range
    pub async fn get_node_metrics(
        &self,
        workflow_id: &Uuid,
        time_range: TimeRange,
    ) -> Result<Vec<NodeMetrics>> {
        let (from, to) = time_range.to_interval();

        #[derive(sqlx::FromRow)]
        struct NodeMetricsRow {
            id: Uuid,
            workflow_id: Uuid,
            node_id: String,
            node_type: String,
            time_bucket: DateTime<Utc>,
            total_executions: i32,
            successful_executions: i32,
            failed_executions: i32,
            total_duration_ms: i64,
            min_duration_ms: Option<i32>,
            max_duration_ms: Option<i32>,
            avg_duration_ms: Option<i32>,
            total_retries: i32,
            max_retries_single_execution: Option<i32>,
            total_input_tokens: i64,
            total_output_tokens: i64,
            created_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, NodeMetricsRow>(
            r"
            SELECT id, workflow_id, node_id, node_type, time_bucket,
                   total_executions, successful_executions, failed_executions,
                   total_duration_ms, min_duration_ms, max_duration_ms, avg_duration_ms,
                   total_retries, max_retries_single_execution,
                   total_input_tokens, total_output_tokens, created_at
            FROM node_metrics
            WHERE workflow_id = $1 AND time_bucket >= $2 AND time_bucket <= $3
            ORDER BY time_bucket DESC, node_id
            ",
        )
        .bind(workflow_id)
        .bind(from)
        .bind(to)
        .fetch_all(self.pool.pool())
        .await?;

        let metrics = rows
            .into_iter()
            .map(|row| NodeMetrics {
                id: row.id,
                workflow_id: row.workflow_id,
                node_id: row.node_id,
                node_type: row.node_type,
                time_bucket: row.time_bucket,
                total_executions: row.total_executions,
                successful_executions: row.successful_executions,
                failed_executions: row.failed_executions,
                total_duration_ms: row.total_duration_ms,
                min_duration_ms: row.min_duration_ms,
                max_duration_ms: row.max_duration_ms,
                avg_duration_ms: row.avg_duration_ms,
                total_retries: row.total_retries,
                max_retries_single_execution: row.max_retries_single_execution,
                total_input_tokens: row.total_input_tokens,
                total_output_tokens: row.total_output_tokens,
                created_at: row.created_at,
            })
            .collect();

        Ok(metrics)
    }

    /// Get workflow summary statistics
    pub async fn get_workflow_stats(&self, workflow_id: &Uuid) -> Result<Option<WorkflowStats>> {
        #[derive(sqlx::FromRow)]
        struct StatsRow {
            total_executions: i64,
            successful_executions: i64,
            failed_executions: i64,
            avg_duration_ms: Option<i64>,
            total_tokens: i64,
            total_cost: Option<rust_decimal::Decimal>,
            last_executed_at: Option<DateTime<Utc>>,
        }

        let row = sqlx::query_as::<_, StatsRow>(
            r"
            SELECT
                SUM(total_executions)::BIGINT as total_executions,
                SUM(successful_executions)::BIGINT as successful_executions,
                SUM(failed_executions)::BIGINT as failed_executions,
                AVG(avg_duration_ms)::BIGINT as avg_duration_ms,
                SUM(total_input_tokens + total_output_tokens)::BIGINT as total_tokens,
                SUM(estimated_cost_usd) as total_cost,
                MAX(time_bucket) as last_executed_at
            FROM execution_metrics
            WHERE workflow_id = $1
            ",
        )
        .bind(workflow_id)
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) if row.total_executions > 0 => {
                let success_rate = if row.total_executions > 0 {
                    row.successful_executions as f64 / row.total_executions as f64 * 100.0
                } else {
                    0.0
                };

                Ok(Some(WorkflowStats {
                    workflow_id: *workflow_id,
                    total_executions: row.total_executions,
                    successful_executions: row.successful_executions,
                    failed_executions: row.failed_executions,
                    success_rate,
                    avg_duration_ms: row.avg_duration_ms,
                    total_tokens: row.total_tokens,
                    total_cost: row
                        .total_cost
                        .map_or(0.0, |d| d.to_string().parse().unwrap_or(0.0)),
                    last_executed_at: row.last_executed_at,
                }))
            }
            _ => Ok(None),
        }
    }

    /// Create a system metrics snapshot
    pub async fn create_system_snapshot(&self) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let hour_ago = now - Duration::hours(1);
        let day_ago = now - Duration::days(1);

        // Gather system-wide metrics
        #[derive(sqlx::FromRow)]
        struct WorkflowCount {
            total: i64,
        }

        let workflow_count =
            sqlx::query_as::<_, WorkflowCount>("SELECT COUNT(*) as total FROM workflows")
                .fetch_one(self.pool.pool())
                .await?;

        #[derive(sqlx::FromRow)]
        struct ExecutionCounts {
            total: i64,
            active: i64,
            last_hour: i64,
            last_day: i64,
        }

        let exec_counts = sqlx::query_as::<_, ExecutionCounts>(
            r"
            SELECT
                COUNT(*) as total,
                SUM(CASE WHEN state IN ('Running', 'Paused') THEN 1 ELSE 0 END) as active,
                SUM(CASE WHEN started_at >= $1 THEN 1 ELSE 0 END) as last_hour,
                SUM(CASE WHEN started_at >= $2 THEN 1 ELSE 0 END) as last_day
            FROM executions
            ",
        )
        .bind(hour_ago)
        .bind(day_ago)
        .fetch_one(self.pool.pool())
        .await?;

        #[derive(sqlx::FromRow)]
        struct UserCounts {
            total: i64,
            active_hour: i64,
            active_day: i64,
        }

        let user_counts = sqlx::query_as::<_, UserCounts>(
            r"
            SELECT
                COUNT(*) as total,
                SUM(CASE WHEN last_login >= $1 THEN 1 ELSE 0 END) as active_hour,
                SUM(CASE WHEN last_login >= $2 THEN 1 ELSE 0 END) as active_day
            FROM users
            ",
        )
        .bind(hour_ago)
        .bind(day_ago)
        .fetch_one(self.pool.pool())
        .await?;

        #[derive(sqlx::FromRow)]
        struct ResourceCounts {
            api_keys: i64,
            secrets: i64,
            schedules: i64,
            webhooks: i64,
            audit_logs: i64,
        }

        let resource_counts = sqlx::query_as::<_, ResourceCounts>(
            r"
            SELECT
                (SELECT COUNT(*) FROM api_keys) as api_keys,
                (SELECT COUNT(*) FROM secrets) as secrets,
                (SELECT COUNT(*) FROM schedules) as schedules,
                (SELECT COUNT(*) FROM webhooks) as webhooks,
                (SELECT COUNT(*) FROM audit_logs) as audit_logs
            ",
        )
        .fetch_one(self.pool.pool())
        .await?;

        // Calculate success rate
        let success_rate = if exec_counts.total > 0 {
            #[derive(sqlx::FromRow)]
            struct SuccessCount {
                successful: i64,
            }

            let success = sqlx::query_as::<_, SuccessCount>(
                "SELECT COUNT(*) as successful FROM executions WHERE state = 'Completed'",
            )
            .fetch_one(self.pool.pool())
            .await?;

            Some(success.successful as f64 / exec_counts.total as f64 * 100.0)
        } else {
            None
        };

        // Insert the snapshot
        sqlx::query(
            r"
            INSERT INTO system_metrics (
                id, timestamp,
                total_workflows, active_workflows,
                total_executions, active_executions, executions_last_hour, executions_last_day,
                total_users, active_users_last_hour, active_users_last_day,
                total_api_keys, total_secrets, total_schedules, total_webhooks,
                audit_logs_count, success_rate_percent
            )
            VALUES ($1, $2, $3, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            ",
        )
        .bind(id)
        .bind(now)
        .bind(workflow_count.total as i32)
        .bind(exec_counts.total)
        .bind(exec_counts.active as i32)
        .bind(exec_counts.last_hour as i32)
        .bind(exec_counts.last_day as i32)
        .bind(user_counts.total as i32)
        .bind(user_counts.active_hour as i32)
        .bind(user_counts.active_day as i32)
        .bind(resource_counts.api_keys as i32)
        .bind(resource_counts.secrets as i32)
        .bind(resource_counts.schedules as i32)
        .bind(resource_counts.webhooks as i32)
        .bind(resource_counts.audit_logs)
        .bind(success_rate)
        .execute(self.pool.pool())
        .await?;

        Ok(id)
    }

    /// Get the latest system metrics snapshot
    pub async fn get_latest_system_metrics(&self) -> Result<Option<SystemMetrics>> {
        #[derive(sqlx::FromRow)]
        struct SystemMetricsRow {
            id: Uuid,
            timestamp: DateTime<Utc>,
            total_workflows: i32,
            active_workflows: i32,
            total_executions: i64,
            active_executions: i32,
            executions_last_hour: i32,
            executions_last_day: i32,
            total_users: i32,
            active_users_last_hour: i32,
            active_users_last_day: i32,
            total_api_keys: i32,
            total_secrets: i32,
            total_schedules: i32,
            total_webhooks: i32,
            total_storage_bytes: Option<i64>,
            audit_logs_count: i64,
            avg_execution_duration_ms: Option<i32>,
            success_rate_percent: Option<rust_decimal::Decimal>,
            metadata: JsonValue,
        }

        let row = sqlx::query_as::<_, SystemMetricsRow>(
            r"
            SELECT * FROM system_metrics
            ORDER BY timestamp DESC
            LIMIT 1
            ",
        )
        .fetch_optional(self.pool.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(SystemMetrics {
                id: row.id,
                timestamp: row.timestamp,
                total_workflows: row.total_workflows,
                active_workflows: row.active_workflows,
                total_executions: row.total_executions,
                active_executions: row.active_executions,
                executions_last_hour: row.executions_last_hour,
                executions_last_day: row.executions_last_day,
                total_users: row.total_users,
                active_users_last_hour: row.active_users_last_hour,
                active_users_last_day: row.active_users_last_day,
                total_api_keys: row.total_api_keys,
                total_secrets: row.total_secrets,
                total_schedules: row.total_schedules,
                total_webhooks: row.total_webhooks,
                total_storage_bytes: row.total_storage_bytes,
                audit_logs_count: row.audit_logs_count,
                avg_execution_duration_ms: row.avg_execution_duration_ms,
                success_rate_percent: row
                    .success_rate_percent
                    .map(|d| d.to_string().parse().unwrap_or(0.0)),
                metadata: row.metadata,
            })),
            None => Ok(None),
        }
    }

    /// Get system metrics history
    pub async fn get_system_metrics_history(
        &self,
        time_range: TimeRange,
    ) -> Result<Vec<SystemMetrics>> {
        let (from, to) = time_range.to_interval();

        #[derive(sqlx::FromRow)]
        struct SystemMetricsRow {
            id: Uuid,
            timestamp: DateTime<Utc>,
            total_workflows: i32,
            active_workflows: i32,
            total_executions: i64,
            active_executions: i32,
            executions_last_hour: i32,
            executions_last_day: i32,
            total_users: i32,
            active_users_last_hour: i32,
            active_users_last_day: i32,
            total_api_keys: i32,
            total_secrets: i32,
            total_schedules: i32,
            total_webhooks: i32,
            total_storage_bytes: Option<i64>,
            audit_logs_count: i64,
            avg_execution_duration_ms: Option<i32>,
            success_rate_percent: Option<rust_decimal::Decimal>,
            metadata: JsonValue,
        }

        let rows = sqlx::query_as::<_, SystemMetricsRow>(
            r"
            SELECT * FROM system_metrics
            WHERE timestamp >= $1 AND timestamp <= $2
            ORDER BY timestamp DESC
            ",
        )
        .bind(from)
        .bind(to)
        .fetch_all(self.pool.pool())
        .await?;

        let metrics = rows
            .into_iter()
            .map(|row| SystemMetrics {
                id: row.id,
                timestamp: row.timestamp,
                total_workflows: row.total_workflows,
                active_workflows: row.active_workflows,
                total_executions: row.total_executions,
                active_executions: row.active_executions,
                executions_last_hour: row.executions_last_hour,
                executions_last_day: row.executions_last_day,
                total_users: row.total_users,
                active_users_last_hour: row.active_users_last_hour,
                active_users_last_day: row.active_users_last_day,
                total_api_keys: row.total_api_keys,
                total_secrets: row.total_secrets,
                total_schedules: row.total_schedules,
                total_webhooks: row.total_webhooks,
                total_storage_bytes: row.total_storage_bytes,
                audit_logs_count: row.audit_logs_count,
                avg_execution_duration_ms: row.avg_execution_duration_ms,
                success_rate_percent: row
                    .success_rate_percent
                    .map(|d| d.to_string().parse().unwrap_or(0.0)),
                metadata: row.metadata,
            })
            .collect();

        Ok(metrics)
    }

    /// Recalculate percentiles for all workflows within a time range
    ///
    /// This is useful for backfilling percentile data or fixing incorrect calculations.
    /// Can be called periodically to update percentiles without recording new executions.
    pub async fn recalculate_percentiles(&self, time_range: TimeRange) -> Result<u64> {
        let (from, to) = time_range.to_interval();

        // Get all unique (workflow_id, time_bucket) combinations in the range
        #[derive(sqlx::FromRow)]
        struct BucketKey {
            workflow_id: Uuid,
            time_bucket: DateTime<Utc>,
        }

        let buckets = sqlx::query_as::<_, BucketKey>(
            r"
            SELECT DISTINCT workflow_id, time_bucket
            FROM execution_durations
            WHERE time_bucket >= $1 AND time_bucket <= $2
            ORDER BY time_bucket DESC
            ",
        )
        .bind(from)
        .bind(to)
        .fetch_all(self.pool.pool())
        .await?;

        let mut updated = 0u64;
        for bucket in buckets {
            self.update_percentiles_for_bucket(bucket.workflow_id, bucket.time_bucket)
                .await?;
            updated += 1;
        }

        Ok(updated)
    }

    /// Cleanup old metrics data
    pub async fn cleanup_old_metrics(&self, older_than_days: i64) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(older_than_days);

        let result1 = sqlx::query("DELETE FROM execution_metrics WHERE time_bucket < $1")
            .bind(cutoff)
            .execute(self.pool.pool())
            .await?;

        let result2 = sqlx::query("DELETE FROM node_metrics WHERE time_bucket < $1")
            .bind(cutoff)
            .execute(self.pool.pool())
            .await?;

        let result3 = sqlx::query("DELETE FROM system_metrics WHERE timestamp < $1")
            .bind(cutoff)
            .execute(self.pool.pool())
            .await?;

        // Also cleanup old execution durations (keep only last 30 days for percentiles)
        let duration_cutoff = Utc::now() - Duration::days(30);
        let result4 = sqlx::query("DELETE FROM execution_durations WHERE created_at < $1")
            .bind(duration_cutoff)
            .execute(self.pool.pool())
            .await?;

        Ok(result1.rows_affected()
            + result2.rows_affected()
            + result3.rows_affected()
            + result4.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_range_interval() {
        let (from, to) = TimeRange::LastHour.to_interval();
        let duration = to - from;
        assert_eq!(duration.num_hours(), 1);

        let (from, to) = TimeRange::Last24Hours.to_interval();
        let duration = to - from;
        assert_eq!(duration.num_hours(), 24);

        let (from, to) = TimeRange::Last7Days.to_interval();
        let duration = to - from;
        assert_eq!(duration.num_days(), 7);
    }

    #[test]
    fn test_time_bucket() {
        let bucket = MetricsStore::current_time_bucket();
        assert_eq!(bucket.minute(), 0);
        assert_eq!(bucket.second(), 0);
    }
}
