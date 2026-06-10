//! [`OxifyCelersClient`] — submit workflows to CeleRS and await results.

use crate::{CelersBridgeError, Result};
use celers_core::{Broker, ResultStore, SerializedTask, TaskId, TaskResultValue};
use oxify_model::{execution::ExecutionContext, Workflow};
use std::time::Duration;

/// Submit OxiFY workflows to a CeleRS broker and poll for results.
///
/// Generic over `B: Broker` and `R: ResultStore` so that tests can inject
/// mock implementations without a live Redis instance.
pub struct OxifyCelersClient<B, R> {
    broker: B,
    results: R,
}

impl<B: Broker, R: ResultStore> OxifyCelersClient<B, R> {
    /// Create a client from an existing broker and result store.
    pub fn new(broker: B, results: R) -> Self {
        Self { broker, results }
    }

    /// Serialize the workflow and enqueue it for a remote worker.
    ///
    /// Returns the [`TaskId`] to pass to [`await_result`](Self::await_result).
    pub async fn submit(&self, workflow: &Workflow) -> Result<TaskId> {
        let input = crate::task::WorkflowTaskInput {
            workflow: workflow.clone(),
            variables: Default::default(),
        };
        let payload =
            serde_json::to_vec(&input).map_err(|e| CelersBridgeError::Serialize(e.to_string()))?;
        let task = SerializedTask::new(crate::OXIFY_WORKFLOW_TASK_NAME.to_string(), payload);
        self.broker
            .enqueue(task)
            .await
            .map_err(CelersBridgeError::from)
    }

    /// Poll the result store until the task completes or the timeout elapses.
    ///
    /// Uses bounded exponential back-off: 50 ms → 2 s.
    pub async fn await_result(&self, id: TaskId, timeout: Duration) -> Result<ExecutionContext> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut delay = Duration::from_millis(50);
        loop {
            match self
                .results
                .get_result(id)
                .await
                .map_err(|e| CelersBridgeError::ResultStore(e.to_string()))?
            {
                Some(TaskResultValue::Success(value)) => {
                    return serde_json::from_value::<ExecutionContext>(value)
                        .map_err(|e| CelersBridgeError::Deserialize(e.to_string()));
                }
                Some(TaskResultValue::Failure { error, .. }) => {
                    return Err(CelersBridgeError::TaskFailed(error));
                }
                Some(TaskResultValue::Revoked) => {
                    return Err(CelersBridgeError::TaskFailed(
                        "task was revoked".to_string(),
                    ));
                }
                Some(TaskResultValue::Rejected { reason }) => {
                    return Err(CelersBridgeError::TaskFailed(reason));
                }
                _ => {} // Pending / Received / Started / Retry — keep polling
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(CelersBridgeError::Timeout);
            }
            tokio::time::sleep(delay.min(Duration::from_secs(2))).await;
            delay = (delay * 2).min(Duration::from_secs(2));
        }
    }
}

/// Convenience constructor for production use with Redis.
impl OxifyCelersClient<celers::RedisBroker, celers::RedisResultBackend> {
    /// Build a production client backed by Redis.
    ///
    /// `redis_url` example: `"redis://localhost:6379"`
    pub fn with_redis(redis_url: &str, queue_name: &str) -> crate::Result<Self> {
        let broker = celers::RedisBroker::new(redis_url, queue_name)
            .map_err(|e| CelersBridgeError::Broker(e.to_string()))?;
        let results = celers::RedisResultBackend::new(redis_url)
            .map_err(|e| CelersBridgeError::Broker(e.to_string()))?;
        Ok(Self::new(broker, results))
    }
}
