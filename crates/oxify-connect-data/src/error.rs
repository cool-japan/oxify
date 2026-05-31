use thiserror::Error;

#[derive(Error, Debug)]
pub enum DataError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Unsupported: {0}")]
    Unsupported(String),
}

impl From<reqwest::Error> for DataError {
    fn from(e: reqwest::Error) -> Self {
        DataError::Http(e.to_string())
    }
}

impl From<serde_json::Error> for DataError {
    fn from(e: serde_json::Error) -> Self {
        DataError::Serialization(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, DataError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_variants() {
        assert!(DataError::Config("x".into()).to_string().contains('x'));
        assert!(DataError::Auth("x".into()).to_string().contains('x'));
        assert!(DataError::Http("x".into()).to_string().contains('x'));
        assert!(DataError::NotFound("x".into()).to_string().contains('x'));
        assert!(DataError::RateLimited("x".into()).to_string().contains('x'));
        assert!(DataError::Api("x".into()).to_string().contains('x'));
        assert!(DataError::Serialization("x".into())
            .to_string()
            .contains('x'));
        assert!(DataError::Unsupported("x".into()).to_string().contains('x'));
    }

    #[test]
    fn from_serde_error() {
        let bad: serde_json::Result<serde_json::Value> = serde_json::from_str("not-json");
        let err = DataError::from(bad.unwrap_err());
        assert!(matches!(err, DataError::Serialization(_)));
    }
}
