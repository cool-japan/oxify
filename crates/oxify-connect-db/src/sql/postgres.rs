use async_trait::async_trait;
use serde_json::Value;
use sqlx::{
    postgres::{PgArguments, PgPool, PgPoolOptions, PgRow},
    Arguments, AssertSqlSafe, Column, Row, TypeInfo,
};

use crate::errors::{DbError, Result};
use crate::sql::{DbConfig, SqlExecutor};

pub struct PostgresProvider {
    pool: PgPool,
}

impl PostgresProvider {
    pub async fn new(cfg: DbConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(cfg.max_connections)
            .min_connections(cfg.min_connections)
            .acquire_timeout(std::time::Duration::from_secs(cfg.acquire_timeout_secs))
            .connect(&cfg.database_url)
            .await?;
        Ok(Self { pool })
    }
}

fn build_pg_args(params: &[Value]) -> Result<PgArguments> {
    let mut args = PgArguments::default();
    for param in params {
        match param {
            Value::Null => args
                .add(None::<String>)
                .map_err(|e| DbError::Query(e.to_string()))?,
            Value::Bool(b) => args.add(*b).map_err(|e| DbError::Query(e.to_string()))?,
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    args.add(i).map_err(|e| DbError::Query(e.to_string()))?;
                } else if let Some(f) = n.as_f64() {
                    args.add(f).map_err(|e| DbError::Query(e.to_string()))?;
                } else {
                    args.add(n.to_string())
                        .map_err(|e| DbError::Query(e.to_string()))?;
                }
            }
            Value::String(s) => args
                .add(s.clone())
                .map_err(|e| DbError::Query(e.to_string()))?,
            _ => args
                .add(param.to_string())
                .map_err(|e| DbError::Query(e.to_string()))?,
        }
    }
    Ok(args)
}

fn extract_pg_value(row: &PgRow, idx: usize, type_name: &str) -> Value {
    match type_name {
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" => row
            .try_get::<Option<String>, _>(idx)
            .ok()
            .flatten()
            .map(Value::String)
            .unwrap_or(Value::Null),
        "INT2" => row
            .try_get::<Option<i16>, _>(idx)
            .ok()
            .flatten()
            .map(|n| Value::Number(n.into()))
            .unwrap_or(Value::Null),
        "INT4" => row
            .try_get::<Option<i32>, _>(idx)
            .ok()
            .flatten()
            .map(|n| Value::Number(n.into()))
            .unwrap_or(Value::Null),
        "INT8" => row
            .try_get::<Option<i64>, _>(idx)
            .ok()
            .flatten()
            .map(|n| Value::Number(n.into()))
            .unwrap_or(Value::Null),
        "FLOAT4" => row
            .try_get::<Option<f32>, _>(idx)
            .ok()
            .flatten()
            .and_then(|f| serde_json::Number::from_f64(f as f64))
            .map(Value::Number)
            .unwrap_or(Value::Null),
        "FLOAT8" => row
            .try_get::<Option<f64>, _>(idx)
            .ok()
            .flatten()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        "BOOL" => row
            .try_get::<Option<bool>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        "BYTEA" => row
            .try_get::<Option<Vec<u8>>, _>(idx)
            .ok()
            .flatten()
            .map(|b| Value::String(b.iter().map(|byte| format!("{byte:02x}")).collect()))
            .unwrap_or(Value::Null),
        "JSON" | "JSONB" => row
            .try_get::<Option<serde_json::Value>, _>(idx)
            .ok()
            .flatten()
            .unwrap_or(Value::Null),
        _ => row
            .try_get::<Option<String>, _>(idx)
            .ok()
            .flatten()
            .map(Value::String)
            .unwrap_or(Value::Null),
    }
}

fn row_to_json(row: &PgRow) -> Value {
    let mut map = serde_json::Map::new();
    for col in row.columns() {
        let type_name = col.type_info().name().to_uppercase();
        let val = extract_pg_value(row, col.ordinal(), &type_name);
        map.insert(col.name().to_string(), val);
    }
    Value::Object(map)
}

#[async_trait]
impl SqlExecutor for PostgresProvider {
    fn provider_name(&self) -> &str {
        "postgres"
    }

    async fn fetch_all(&self, sql: &str, params: &[Value]) -> Result<Vec<Value>> {
        let args = build_pg_args(params)?;
        let rows = sqlx::query_with(AssertSqlSafe(sql.to_string()), args)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(row_to_json).collect())
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64> {
        let args = build_pg_args(params)?;
        let result = sqlx::query_with(AssertSqlSafe(sql.to_string()), args)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    async fn health_check(&self) -> Result<()> {
        sqlx::query(AssertSqlSafe("SELECT 1".to_string()))
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_pg_args_null() {
        let params = vec![Value::Null];
        let result = build_pg_args(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_pg_args_bool() {
        let params = vec![Value::Bool(true), Value::Bool(false)];
        let result = build_pg_args(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_pg_args_integer() {
        let params = vec![Value::Number(42.into())];
        let result = build_pg_args(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_pg_args_float() {
        let params = vec![Value::Number(serde_json::Number::from_f64(3.14).unwrap())];
        let result = build_pg_args(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_pg_args_string() {
        let params = vec![Value::String("hello".to_string())];
        let result = build_pg_args(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_pg_args_mixed() {
        let params = vec![
            Value::Null,
            Value::Bool(true),
            Value::Number(1.into()),
            Value::String("test".to_string()),
        ];
        let result = build_pg_args(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_pg_args_empty() {
        let result = build_pg_args(&[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[tokio::test]
    #[ignore = "Requires database"]
    async fn test_postgres_health_check() {
        let cfg = DbConfig::from_env().expect("DATABASE_URL must be set");
        let provider = PostgresProvider::new(cfg).await.expect("connection failed");
        provider.health_check().await.expect("health check failed");
    }

    #[tokio::test]
    #[ignore = "Requires database"]
    async fn test_postgres_fetch_all() {
        let cfg = DbConfig::from_env().expect("DATABASE_URL must be set");
        let provider = PostgresProvider::new(cfg).await.expect("connection failed");
        let rows = provider
            .fetch_all("SELECT 1 AS n", &[])
            .await
            .expect("query failed");
        assert_eq!(rows.len(), 1);
    }
}
