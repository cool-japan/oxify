use thiserror::Error;

/// Errors that can occur in GraphQL client operations.
#[derive(Error, Debug)]
pub enum GraphQlError {
    /// An HTTP-level error (network, status code, etc.)
    #[error("HTTP error: {0}")]
    Http(String),

    /// Authentication or authorisation failure.
    #[error("auth error: {0}")]
    Auth(String),

    /// JSON serialization / deserialization failure.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// A GraphQL-level error returned in the `errors` array of the response.
    #[error("GraphQL error: {0}")]
    GraphQl(String),

    /// Configuration error (missing environment variables, invalid values, etc.)
    #[error("config error: {0}")]
    Config(String),

    /// A transport-level error (failed to build HTTP client, connection refused, etc.)
    #[error("transport error: {0}")]
    Transport(String),
}

/// Convenience alias so callers write `errors::Result<T>`.
pub type Result<T> = std::result::Result<T, GraphQlError>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_http_display() {
        let err = GraphQlError::Http("status 404".to_string());
        assert!(err.to_string().contains("HTTP error"));
        assert!(err.to_string().contains("status 404"));
    }

    #[test]
    fn test_error_auth_display() {
        let err = GraphQlError::Auth("token missing".to_string());
        assert!(err.to_string().contains("auth error"));
        assert!(err.to_string().contains("token missing"));
    }

    #[test]
    fn test_error_serialization_display() {
        let err = GraphQlError::Serialization("invalid json".to_string());
        assert!(err.to_string().contains("serialization error"));
        assert!(err.to_string().contains("invalid json"));
    }

    #[test]
    fn test_error_graphql_display() {
        let err = GraphQlError::GraphQl("field not found".to_string());
        assert!(err.to_string().contains("GraphQL error"));
        assert!(err.to_string().contains("field not found"));
    }

    #[test]
    fn test_error_config_display() {
        let err = GraphQlError::Config("GRAPHQL_ENDPOINT not set".to_string());
        assert!(err.to_string().contains("config error"));
        assert!(err.to_string().contains("GRAPHQL_ENDPOINT"));
    }

    #[test]
    fn test_error_transport_display() {
        let err = GraphQlError::Transport("connection refused".to_string());
        assert!(err.to_string().contains("transport error"));
        assert!(err.to_string().contains("connection refused"));
    }
}
