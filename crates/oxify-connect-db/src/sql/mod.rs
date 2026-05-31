use async_trait::async_trait;
use serde_json::Value;

use crate::errors::{DbError, Result};

#[async_trait]
pub trait SqlExecutor: Send + Sync {
    fn provider_name(&self) -> &str;
    async fn fetch_all(&self, sql: &str, params: &[Value]) -> Result<Vec<Value>>;
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64>;
    async fn health_check(&self) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub database_url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout_secs: 30,
        }
    }
}

impl DbConfig {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| DbError::Config("DATABASE_URL not set".to_string()))?;
        if database_url.trim().is_empty() {
            return Err(DbError::Config("DATABASE_URL is empty".to_string()));
        }
        Ok(Self {
            database_url,
            ..Default::default()
        })
    }
}

#[cfg(feature = "mysql")]
pub mod mysql;
#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "mysql")]
pub use mysql::MySqlProvider;
#[cfg(feature = "postgres")]
pub use postgres::PostgresProvider;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_config_default_values() {
        let cfg = DbConfig {
            database_url: "postgres://localhost/test".to_string(),
            ..Default::default()
        };
        assert_eq!(cfg.max_connections, 10);
        assert_eq!(cfg.min_connections, 1);
        assert_eq!(cfg.acquire_timeout_secs, 30);
    }

    #[test]
    fn test_db_config_from_env_missing() {
        std::env::remove_var("DATABASE_URL");
        let result = DbConfig::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL not set"));
    }

    #[test]
    fn test_db_config_from_env_empty() {
        std::env::set_var("DATABASE_URL", "   ");
        let result = DbConfig::from_env();
        std::env::remove_var("DATABASE_URL");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL is empty"));
    }

    #[test]
    fn test_db_config_from_env_valid() {
        std::env::set_var("DATABASE_URL", "postgres://localhost/mydb");
        let result = DbConfig::from_env();
        std::env::remove_var("DATABASE_URL");
        assert!(result.is_ok());
        let cfg = result.unwrap();
        assert_eq!(cfg.database_url, "postgres://localhost/mydb");
    }
}
