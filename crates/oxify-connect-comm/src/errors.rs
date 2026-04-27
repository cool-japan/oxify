use thiserror::Error;

/// Errors that can occur in communication providers.
#[derive(Error, Debug)]
pub enum CommError {
    /// An HTTP-level error (network, status code, etc.)
    #[error("HTTP error: {0}")]
    Http(String),

    /// Authentication or authorisation failure.
    #[error("auth error: {0}")]
    Auth(String),

    /// JSON serialization / deserialization failure.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// The operation is not supported by this provider.
    #[error("unsupported operation: {0}")]
    Unsupported(String),

    /// A provider-specific error that does not fit the categories above.
    #[error("provider error: {0}")]
    Provider(String),
}

/// Convenience alias so callers write `errors::Result<T>`.
pub type Result<T> = std::result::Result<T, CommError>;
