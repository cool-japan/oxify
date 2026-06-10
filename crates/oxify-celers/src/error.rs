use thiserror::Error;

#[derive(Debug, Error)]
pub enum CelersBridgeError {
    #[error("serialization error: {0}")]
    Serialize(String),
    #[error("deserialization error: {0}")]
    Deserialize(String),
    #[error("broker error: {0}")]
    Broker(String),
    #[error("task execution failed: {0}")]
    TaskFailed(String),
    #[error("result store error: {0}")]
    ResultStore(String),
    #[error("await_result timed out")]
    Timeout,
}

/// Convert from `CelersError` so `?` works in bridge code.
#[cfg(any(feature = "redis", feature = "test-utils"))]
impl From<celers_core::CelersError> for CelersBridgeError {
    fn from(e: celers_core::CelersError) -> Self {
        CelersBridgeError::Broker(e.to_string())
    }
}
