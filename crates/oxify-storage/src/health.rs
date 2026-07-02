//! Health check system for production monitoring
//!
//! This module provides comprehensive health checks for the database and storage layer.
//! It's designed to integrate with container orchestration systems (Kubernetes, Docker)
//! and monitoring platforms (Prometheus, Datadog).
//!
//! # Features
//!
//! - **Connection Health**: Verify database connectivity and responsiveness
//! - **Pool Health**: Check connection pool utilization and availability
//! - **Table Checks**: Verify critical tables exist and are accessible
//! - **Replication Lag**: Monitor replication delay (if applicable)
//! - **Disk Space**: Check available storage space
//! - **Overall Status**: Aggregate health status with severity levels
//!
//! # Example
//!
//! ```ignore
//! use oxify_storage::{DatabasePool, HealthCheck, HealthStatus};
//!
//! let pool = DatabasePool::new(config).await?;
//! let health_check = HealthCheck::new(pool.clone());
//!
//! // Quick check (liveness probe)
//! let is_alive = health_check.is_alive().await?;
//! if !is_alive {
//!     panic!("Database is not responding!");
//! }
//!
//! // Detailed check (readiness probe)
//! let health = health_check.check().await?;
//! match health.status {
//!     HealthStatus::Healthy => {
//!         println!("System is healthy");
//!     }
//!     HealthStatus::Degraded => {
//!         println!("System is degraded: {}", health.message);
//!     }
//!     HealthStatus::Unhealthy => {
//!         eprintln!("System is unhealthy: {}", health.message);
//!         // Don't accept new requests
//!     }
//! }
//!
//! // Export for monitoring
//! let metrics = health_check.export_metrics().await?;
//! for (key, value) in metrics {
//!     prometheus::gauge(format!("oxify_health_{}", key), value);
//! }
//! ```

use crate::{DatabasePool, Result};
use oxisql_core::{Connection, OxiSqlError, Row, ToSqlValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Overall health status of the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// System is fully operational
    Healthy,
    /// System is operational but with degraded performance
    Degraded,
    /// System is not operational
    Unhealthy,
}

/// Individual component health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name (e.g., "database", "connection_pool")
    pub name: String,
    /// Health status of this component
    pub status: HealthStatus,
    /// Human-readable message
    pub message: String,
    /// Check duration in milliseconds
    pub duration_ms: u64,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Comprehensive health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Overall system health status
    pub status: HealthStatus,
    /// Overall message
    pub message: String,
    /// Individual component checks
    pub components: Vec<ComponentHealth>,
    /// Total check duration in milliseconds
    pub total_duration_ms: u64,
    /// Timestamp of the check
    pub timestamp: i64,
}

impl HealthReport {
    /// Check if the system is healthy
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }

    /// Check if the system is degraded
    pub fn is_degraded(&self) -> bool {
        self.status == HealthStatus::Degraded
    }

    /// Check if the system is unhealthy
    pub fn is_unhealthy(&self) -> bool {
        self.status == HealthStatus::Unhealthy
    }

    /// Get all unhealthy components
    pub fn unhealthy_components(&self) -> Vec<&ComponentHealth> {
        self.components
            .iter()
            .filter(|c| c.status == HealthStatus::Unhealthy)
            .collect()
    }

    /// Get all degraded components
    pub fn degraded_components(&self) -> Vec<&ComponentHealth> {
        self.components
            .iter()
            .filter(|c| c.status == HealthStatus::Degraded)
            .collect()
    }
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Timeout for database ping (default: 5 seconds)
    pub ping_timeout: Duration,
    /// Pool utilization threshold for degraded status (default: 0.8)
    pub pool_degraded_threshold: f64,
    /// Pool utilization threshold for unhealthy status (default: 0.95)
    pub pool_unhealthy_threshold: f64,
    /// Tables to check for existence (default: critical tables)
    pub required_tables: Vec<String>,
    /// Enable replication lag check (default: false)
    pub check_replication_lag: bool,
    /// Maximum acceptable replication lag in seconds (default: 10)
    pub max_replication_lag_secs: i64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            ping_timeout: Duration::from_secs(5),
            pool_degraded_threshold: 0.8,
            pool_unhealthy_threshold: 0.95,
            required_tables: vec![
                "users".to_string(),
                "workflows".to_string(),
                "executions".to_string(),
                "user_quotas".to_string(),
            ],
            check_replication_lag: false,
            max_replication_lag_secs: 10,
        }
    }
}

/// Health check service
pub struct HealthCheck {
    pool: DatabasePool,
    config: HealthCheckConfig,
}

impl HealthCheck {
    /// Create a new health check service with default configuration
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            config: HealthCheckConfig::default(),
        }
    }

    /// Create a new health check service with custom configuration
    pub fn with_config(pool: DatabasePool, config: HealthCheckConfig) -> Self {
        Self { pool, config }
    }

    /// Quick liveness check - just verifies database is responding
    ///
    /// This is suitable for Kubernetes liveness probes.
    /// It only checks if the database connection works.
    pub async fn is_alive(&self) -> Result<bool> {
        let result = self.fetch_one_row("SELECT 1", &[]).await;

        Ok(result.is_ok())
    }

    /// Comprehensive health check
    ///
    /// This is suitable for Kubernetes readiness probes.
    /// It checks all system components and returns detailed status.
    pub async fn check(&self) -> Result<HealthReport> {
        let start = Instant::now();
        let mut components = Vec::new();

        // Check 1: Database connectivity
        components.push(self.check_database_connectivity().await);

        // Check 2: Connection pool health
        components.push(self.check_connection_pool().await);

        // Check 3: Required tables existence
        components.push(self.check_required_tables().await);

        // Check 4: Replication lag (if enabled)
        if self.config.check_replication_lag {
            components.push(self.check_replication_lag().await);
        }

        // Check 5: Database size and disk space
        components.push(self.check_disk_space().await);

        // Determine overall status
        let has_unhealthy = components
            .iter()
            .any(|c| c.status == HealthStatus::Unhealthy);
        let has_degraded = components
            .iter()
            .any(|c| c.status == HealthStatus::Degraded);

        let (status, message) = if has_unhealthy {
            (
                HealthStatus::Unhealthy,
                "One or more critical components are unhealthy".to_string(),
            )
        } else if has_degraded {
            (
                HealthStatus::Degraded,
                "System is operational but degraded".to_string(),
            )
        } else {
            (HealthStatus::Healthy, "All systems operational".to_string())
        };

        Ok(HealthReport {
            status,
            message,
            components,
            total_duration_ms: start.elapsed().as_millis() as u64,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// Export health metrics in a format suitable for monitoring systems
    pub async fn export_metrics(&self) -> Result<HashMap<String, f64>> {
        let report = self.check().await?;
        let mut metrics = HashMap::new();

        // Overall health (1 = healthy, 0.5 = degraded, 0 = unhealthy)
        let health_value = match report.status {
            HealthStatus::Healthy => 1.0,
            HealthStatus::Degraded => 0.5,
            HealthStatus::Unhealthy => 0.0,
        };
        metrics.insert("overall_health".to_string(), health_value);

        // Component health
        for component in &report.components {
            let value = match component.status {
                HealthStatus::Healthy => 1.0,
                HealthStatus::Degraded => 0.5,
                HealthStatus::Unhealthy => 0.0,
            };
            metrics.insert(format!("component_{}", component.name), value);
            metrics.insert(
                format!("component_{}_duration_ms", component.name),
                component.duration_ms as f64,
            );
        }

        // Total check duration
        metrics.insert(
            "check_duration_ms".to_string(),
            report.total_duration_ms as f64,
        );

        Ok(metrics)
    }

    // Query helpers
    //
    // OxiSQL's `Connection` trait (unlike `sqlx::Pool`) does not run a query
    // directly; a connection must be checked out from the pool first via
    // `DatabasePool::acquire`, then `Connection::query` is called on it. These
    // helpers centralise that acquire+query+error-propagation boilerplate so
    // each check below reads the same as its old `sqlx::query(..).fetch_one(..)`
    // / `.fetch_all(..)` counterpart.

    /// Acquire a connection and run `sql`, returning all result rows.
    async fn fetch_rows(&self, sql: &str, params: &[&dyn ToSqlValue]) -> Result<Vec<Row>> {
        let conn = self.pool.acquire().await?;
        let rows = conn.query(sql, params).await?;
        Ok(rows)
    }

    /// Acquire a connection and run `sql`, returning the first result row.
    ///
    /// Mirrors `sqlx`'s `fetch_one`: fails with [`OxiSqlError::Other`] if the
    /// query produces no rows.
    async fn fetch_one_row(&self, sql: &str, params: &[&dyn ToSqlValue]) -> Result<Row> {
        let mut rows = self.fetch_rows(sql, params).await?;
        if rows.is_empty() {
            return Err(OxiSqlError::Other("query returned no rows".to_string()).into());
        }
        Ok(rows.remove(0))
    }

    // Individual health checks

    async fn check_database_connectivity(&self) -> ComponentHealth {
        let start = Instant::now();
        let result = self.fetch_one_row("SELECT version()", &[]).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(_) => ComponentHealth {
                name: "database".to_string(),
                status: HealthStatus::Healthy,
                message: "Database is responding".to_string(),
                duration_ms,
                metadata: None,
            },
            Err(e) => ComponentHealth {
                name: "database".to_string(),
                status: HealthStatus::Unhealthy,
                message: format!("Database connection failed: {e}"),
                duration_ms,
                metadata: None,
            },
        }
    }

    async fn check_connection_pool(&self) -> ComponentHealth {
        let start = Instant::now();
        let metrics = self.pool.metrics();
        let duration_ms = start.elapsed().as_millis() as u64;

        let utilization = metrics.stats.utilization();
        let mut metadata = HashMap::new();
        metadata.insert("utilization".to_string(), format!("{utilization:.2}"));
        metadata.insert(
            "active_connections".to_string(),
            metrics.stats.size.to_string(),
        );
        metadata.insert(
            "idle_connections".to_string(),
            metrics.stats.num_idle.to_string(),
        );

        let (status, message) = if utilization >= self.config.pool_unhealthy_threshold {
            (
                HealthStatus::Unhealthy,
                format!(
                    "Pool is at critical capacity ({:.1}% utilization)",
                    utilization * 100.0
                ),
            )
        } else if utilization >= self.config.pool_degraded_threshold {
            (
                HealthStatus::Degraded,
                format!(
                    "Pool is under high load ({:.1}% utilization)",
                    utilization * 100.0
                ),
            )
        } else {
            (
                HealthStatus::Healthy,
                format!("Pool is healthy ({:.1}% utilization)", utilization * 100.0),
            )
        };

        ComponentHealth {
            name: "connection_pool".to_string(),
            status,
            message,
            duration_ms,
            metadata: Some(metadata),
        }
    }

    async fn check_required_tables(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut missing_tables = Vec::new();

        for table in &self.config.required_tables {
            // NOTE: this used to query `information_schema.tables WHERE
            // table_schema = 'public'`, which is Postgres-only syntax that
            // always fails against the SQLite backend this crate actually
            // runs on. Query SQLite's own system catalog instead.
            let result = self
                .fetch_rows(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = $1",
                    &[table],
                )
                .await;

            match result {
                Ok(rows) => {
                    if rows.is_empty() {
                        missing_tables.push(table.clone());
                    }
                }
                Err(_) => {
                    missing_tables.push(table.clone());
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        if missing_tables.is_empty() {
            ComponentHealth {
                name: "tables".to_string(),
                status: HealthStatus::Healthy,
                message: "All required tables exist".to_string(),
                duration_ms,
                metadata: None,
            }
        } else {
            let mut metadata = HashMap::new();
            metadata.insert("missing_tables".to_string(), missing_tables.join(", "));

            ComponentHealth {
                name: "tables".to_string(),
                status: HealthStatus::Unhealthy,
                message: format!("Missing tables: {}", missing_tables.join(", ")),
                duration_ms,
                metadata: Some(metadata),
            }
        }
    }

    async fn check_replication_lag(&self) -> ComponentHealth {
        let start = Instant::now();

        // Check if we're on a replica
        let result: Result<i64> = async {
            let row = self
                .fetch_one_row(
                    "SELECT CASE WHEN pg_is_in_recovery() THEN
                        EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp()))::bigint
                     ELSE 0 END as lag_seconds",
                    &[],
                )
                .await?;
            let lag_secs: i64 = row.try_get("lag_seconds")?;
            Ok(lag_secs)
        }
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(lag_secs) => {
                let mut metadata = HashMap::new();
                metadata.insert("lag_seconds".to_string(), lag_secs.to_string());

                let (status, message) = if lag_secs == 0 {
                    (
                        HealthStatus::Healthy,
                        "Not a replica or no replication lag".to_string(),
                    )
                } else if lag_secs > self.config.max_replication_lag_secs {
                    (
                        HealthStatus::Degraded,
                        format!("Replication lag is {lag_secs} seconds"),
                    )
                } else {
                    (
                        HealthStatus::Healthy,
                        format!("Replication lag is {lag_secs} seconds"),
                    )
                };

                ComponentHealth {
                    name: "replication".to_string(),
                    status,
                    message,
                    duration_ms,
                    metadata: Some(metadata),
                }
            }
            Err(e) => ComponentHealth {
                name: "replication".to_string(),
                status: HealthStatus::Degraded,
                message: format!("Could not check replication lag: {e}"),
                duration_ms,
                metadata: None,
            },
        }
    }

    async fn check_disk_space(&self) -> ComponentHealth {
        let start = Instant::now();

        let result: Result<(i64, String)> = async {
            let row = self
                .fetch_one_row(
                    "SELECT
                        pg_database_size(current_database()) as db_size,
                        pg_size_pretty(pg_database_size(current_database())) as db_size_pretty",
                    &[],
                )
                .await?;
            let db_size: i64 = row.try_get("db_size")?;
            let db_size_pretty: String = row.try_get("db_size_pretty")?;
            Ok((db_size, db_size_pretty))
        }
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok((db_size, db_size_pretty)) => {
                let mut metadata = HashMap::new();
                metadata.insert("database_size".to_string(), db_size_pretty.clone());
                metadata.insert("database_size_bytes".to_string(), db_size.to_string());

                ComponentHealth {
                    name: "disk_space".to_string(),
                    status: HealthStatus::Healthy,
                    message: format!("Database size: {db_size_pretty}"),
                    duration_ms,
                    metadata: Some(metadata),
                }
            }
            Err(e) => ComponentHealth {
                name: "disk_space".to_string(),
                status: HealthStatus::Degraded,
                message: format!("Could not check disk space: {e}"),
                duration_ms,
                metadata: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_health_check_config() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.ping_timeout, Duration::from_secs(5));
        assert_eq!(config.pool_degraded_threshold, 0.8);
        assert_eq!(config.pool_unhealthy_threshold, 0.95);
        assert!(!config.required_tables.is_empty());
        assert!(!config.check_replication_lag);
    }

    #[test]
    fn test_health_status() {
        let report = HealthReport {
            status: HealthStatus::Healthy,
            message: "All good".to_string(),
            components: vec![],
            total_duration_ms: 100,
            timestamp: 0,
        };

        assert!(report.is_healthy());
        assert!(!report.is_degraded());
        assert!(!report.is_unhealthy());
    }

    #[test]
    fn test_health_report_filters() {
        let report = HealthReport {
            status: HealthStatus::Degraded,
            message: "Some issues".to_string(),
            components: vec![
                ComponentHealth {
                    name: "db".to_string(),
                    status: HealthStatus::Healthy,
                    message: "OK".to_string(),
                    duration_ms: 10,
                    metadata: None,
                },
                ComponentHealth {
                    name: "pool".to_string(),
                    status: HealthStatus::Degraded,
                    message: "High load".to_string(),
                    duration_ms: 5,
                    metadata: None,
                },
                ComponentHealth {
                    name: "disk".to_string(),
                    status: HealthStatus::Unhealthy,
                    message: "Low space".to_string(),
                    duration_ms: 8,
                    metadata: None,
                },
            ],
            total_duration_ms: 23,
            timestamp: 0,
        };

        assert_eq!(report.degraded_components().len(), 1);
        assert_eq!(report.unhealthy_components().len(), 1);
        assert_eq!(report.degraded_components()[0].name, "pool");
        assert_eq!(report.unhealthy_components()[0].name, "disk");
    }
}
