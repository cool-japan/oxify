use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("query error: {0}")]
    Query(String),

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("sql error: {0}")]
    Sql(String),

    #[error("document store error: {0}")]
    Document(String),
}

pub type Result<T> = std::result::Result<T, DbError>;

#[cfg(any(feature = "postgres", feature = "mysql"))]
impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        DbError::Sql(e.to_string())
    }
}

#[cfg(feature = "mongodb-store")]
impl From<mongodb::error::Error> for DbError {
    fn from(e: mongodb::error::Error) -> Self {
        DbError::Document(e.to_string())
    }
}

impl DbError {
    pub fn is_retryable(&self) -> bool {
        match self {
            DbError::Sql(msg) => {
                let m = msg.as_str();
                m.contains("40001")
                    || m.contains("40P01")
                    || m.contains("08006")
                    || m.contains("08003")
                    || m.contains("57P03")
                    || m.contains("1213")
                    || m.contains("1205")
                    || m.contains("pool timed out")
                    || m.contains("pool is closed")
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_error_message() {
        let e = DbError::Config("missing DATABASE_URL".to_string());
        assert!(e.to_string().contains("configuration error"));
        assert!(e.to_string().contains("missing DATABASE_URL"));
    }

    #[test]
    fn test_query_error_message() {
        let e = DbError::Query("syntax error".to_string());
        assert!(e.to_string().contains("query error"));
    }

    #[test]
    fn test_unsupported_error_message() {
        let e = DbError::Unsupported("batch not supported".to_string());
        assert!(e.to_string().contains("unsupported operation"));
    }

    #[test]
    fn test_is_retryable_config_is_false() {
        assert!(!DbError::Config("x".to_string()).is_retryable());
    }

    #[test]
    fn test_is_retryable_sql_pool_timeout() {
        let e = DbError::Sql("pool timed out waiting for a connection".to_string());
        assert!(e.is_retryable());
    }

    #[test]
    fn test_is_retryable_sql_serialization_failure() {
        let e = DbError::Sql("error code 40001 serialization failure".to_string());
        assert!(e.is_retryable());
    }

    #[test]
    fn test_is_retryable_document_is_false() {
        let e = DbError::Document("connection refused".to_string());
        assert!(!e.is_retryable());
    }

    #[test]
    fn test_sql_error_message() {
        let e = DbError::Sql("column does not exist".to_string());
        assert!(e.to_string().contains("sql error"));
    }

    #[test]
    fn test_document_error_message() {
        let e = DbError::Document("write concern error".to_string());
        assert!(e.to_string().contains("document store error"));
    }
}
