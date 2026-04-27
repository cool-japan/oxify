use thiserror::Error;

/// Errors produced by the object storage layer.
#[derive(Error, Debug)]
pub enum StorageError {
    /// The requested object could not be found in the given bucket.
    #[error("object not found: {bucket}/{key}")]
    NotFound { bucket: String, key: String },

    /// The requested bucket does not exist.
    #[error("bucket not found: {0}")]
    BucketNotFound(String),

    /// A provider-level error occurred (e.g. network, permissions).
    #[error("provider error: {0}")]
    Provider(String),

    /// (De)serialization of metadata or object data failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// The requested operation is not supported by this provider.
    #[error("unsupported operation: {0}")]
    Unsupported(String),

    /// The provider was misconfigured.
    #[error("configuration error: {0}")]
    Config(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, StorageError>;

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error_message() {
        let err = StorageError::NotFound {
            bucket: "my-bucket".to_string(),
            key: "data/file.bin".to_string(),
        };
        let msg = err.to_string();
        assert_eq!(msg, "object not found: my-bucket/data/file.bin");
    }

    #[test]
    fn test_bucket_not_found_error_message() {
        let err = StorageError::BucketNotFound("missing-bucket".to_string());
        assert_eq!(err.to_string(), "bucket not found: missing-bucket");
    }

    #[test]
    fn test_config_error_message() {
        let err = StorageError::Config("missing region".to_string());
        assert_eq!(err.to_string(), "configuration error: missing region");
    }

    #[test]
    fn test_unsupported_error_message() {
        let err = StorageError::Unsupported("presigned URLs".to_string());
        assert_eq!(err.to_string(), "unsupported operation: presigned URLs");
    }
}
