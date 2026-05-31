use async_trait::async_trait;
use serde_json::Value;

use crate::errors::{DbError, Result};

#[async_trait]
pub trait DocumentStore: Send + Sync {
    fn provider_name(&self) -> &str;
    async fn insert_one(&self, collection: &str, doc: Value) -> Result<String>;
    async fn find(&self, collection: &str, filter: Value, limit: Option<i64>)
        -> Result<Vec<Value>>;
    async fn update_one(&self, collection: &str, filter: Value, update: Value) -> Result<u64>;
    async fn delete_one(&self, collection: &str, filter: Value) -> Result<u64>;
    async fn count(&self, collection: &str, filter: Value) -> Result<u64>;
}

#[derive(Debug, Clone)]
pub struct MongoConfig {
    pub uri: String,
    pub database: String,
}

impl Default for MongoConfig {
    fn default() -> Self {
        Self {
            uri: std::env::var("MONGODB_URI")
                .unwrap_or_else(|_| "mongodb://localhost:27017".to_string()),
            database: std::env::var("MONGODB_DATABASE").unwrap_or_else(|_| "oxify".to_string()),
        }
    }
}

impl MongoConfig {
    pub fn from_env() -> Result<Self> {
        let uri = std::env::var("MONGODB_URI")
            .map_err(|_| DbError::Config("MONGODB_URI not set".to_string()))?;
        if uri.trim().is_empty() {
            return Err(DbError::Config("MONGODB_URI is empty".to_string()));
        }
        let database = std::env::var("MONGODB_DATABASE")
            .map_err(|_| DbError::Config("MONGODB_DATABASE not set".to_string()))?;
        if database.trim().is_empty() {
            return Err(DbError::Config("MONGODB_DATABASE is empty".to_string()));
        }
        Ok(Self { uri, database })
    }
}

#[cfg(feature = "mongodb-store")]
pub mod mongo;
#[cfg(feature = "mongodb-store")]
pub use mongo::MongoProvider;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mongo_config_from_env_missing_uri() {
        std::env::remove_var("MONGODB_URI");
        std::env::remove_var("MONGODB_DATABASE");
        let result = MongoConfig::from_env();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("MONGODB_URI not set"));
    }

    #[test]
    fn test_mongo_config_from_env_missing_database() {
        std::env::set_var("MONGODB_URI", "mongodb://localhost:27017");
        std::env::remove_var("MONGODB_DATABASE");
        let result = MongoConfig::from_env();
        std::env::remove_var("MONGODB_URI");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("MONGODB_DATABASE not set"));
    }

    #[test]
    fn test_mongo_config_from_env_valid() {
        std::env::set_var("MONGODB_URI", "mongodb://localhost:27017");
        std::env::set_var("MONGODB_DATABASE", "testdb");
        let result = MongoConfig::from_env();
        std::env::remove_var("MONGODB_URI");
        std::env::remove_var("MONGODB_DATABASE");
        assert!(result.is_ok());
        let cfg = result.unwrap();
        assert_eq!(cfg.uri, "mongodb://localhost:27017");
        assert_eq!(cfg.database, "testdb");
    }

    #[test]
    fn test_mongo_config_default() {
        let cfg = MongoConfig {
            uri: "mongodb://custom:27017".to_string(),
            database: "mydb".to_string(),
        };
        assert_eq!(cfg.uri, "mongodb://custom:27017");
        assert_eq!(cfg.database, "mydb");
    }
}
