//! Worker bootstrap — start a CeleRS dequeue loop that executes OxiFY workflows.
//!
//! # Critical note
//!
//! The stock `celers::Worker` does **not** store results (verified: zero
//! references to `ResultStore` or `store_result` in the upstream worker
//! implementation). This module implements its own dequeue loop that stores
//! results so [`OxifyCelersClient::await_result`](crate::client::OxifyCelersClient::await_result)
//! works correctly.

use crate::task::OxifyWorkflowTask;
use celers_core::{Broker, ResultStore, TaskRegistry, TaskResultValue};
use tracing::{error, info};

/// Start an OxiFY workflow worker connected to Redis.
///
/// This function **blocks** (runs a dequeue loop indefinitely). Call it in a
/// dedicated task or as the sole work in a worker binary.
///
/// # Arguments
///
/// * `redis_url`   — e.g. `"redis://localhost:6379"`
/// * `queue_name`  — e.g. `"oxify_workflows"`
pub async fn run_worker(redis_url: &str, queue_name: &str) -> crate::Result<()> {
    let broker = celers::RedisBroker::new(redis_url, queue_name)
        .map_err(|e| crate::CelersBridgeError::Broker(e.to_string()))?;
    let results = celers::RedisResultBackend::new(redis_url)
        .map_err(|e| crate::CelersBridgeError::Broker(e.to_string()))?;
    let registry = TaskRegistry::new();
    registry.register(OxifyWorkflowTask).await;
    run_loop(broker, results, registry).await
}

/// Generic dequeue loop — injectable broker/results for hermetic tests.
///
/// Unlike `celers::Worker::run()`, this loop **stores results** so clients
/// polling `ResultStore::get_result` receive the outcome.
pub async fn run_loop<B, R>(broker: B, results: R, registry: TaskRegistry) -> crate::Result<()>
where
    B: Broker,
    R: ResultStore,
{
    info!("OxiFY CeleRS worker started");
    loop {
        let msg = broker
            .dequeue()
            .await
            .map_err(|e| crate::CelersBridgeError::Broker(e.to_string()))?;

        let Some(msg) = msg else {
            // Nothing in queue — back off briefly before polling again.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        };

        let task_id = msg.task_id();
        let receipt = msg.receipt_handle.clone();

        // Mark as started (best-effort — ignore store failures for status updates).
        let _ = results
            .store_result(task_id, TaskResultValue::Started)
            .await;

        match registry.execute(&msg.task).await {
            Ok(bytes) => {
                // The registry serialised the ExecutionContext via serde_json.
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
                if let Err(e) = results
                    .store_result(task_id, TaskResultValue::Success(value))
                    .await
                {
                    error!("failed to store result for task {}: {}", task_id, e);
                }
                if let Err(e) = broker.ack(&task_id, receipt.as_deref()).await {
                    error!("failed to ack task {}: {}", task_id, e);
                }
            }
            Err(e) => {
                error!("task {} failed: {}", task_id, e);
                if let Err(re) = results
                    .store_result(
                        task_id,
                        TaskResultValue::Failure {
                            error: e.to_string(),
                            traceback: None,
                        },
                    )
                    .await
                {
                    error!("failed to store failure for task {}: {}", task_id, re);
                }
                if let Err(re) = broker.reject(&task_id, receipt.as_deref(), false).await {
                    error!("failed to reject task {}: {}", task_id, re);
                }
            }
        }
    }
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use super::*;
    use celers::dev_utils::MockBroker;
    use celers_core::{TaskId, TaskResultValue};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Minimal in-process result store for hermetic tests.
    #[derive(Clone)]
    struct MockResultStore {
        results: Arc<Mutex<HashMap<TaskId, TaskResultValue>>>,
    }

    impl MockResultStore {
        fn new() -> Self {
            Self {
                results: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl ResultStore for MockResultStore {
        async fn store_result(
            &self,
            task_id: TaskId,
            result: TaskResultValue,
        ) -> celers_core::Result<()> {
            self.results
                .lock()
                .expect("MockResultStore lock poisoned")
                .insert(task_id, result);
            Ok(())
        }

        async fn get_result(
            &self,
            task_id: TaskId,
        ) -> celers_core::Result<Option<TaskResultValue>> {
            let guard = self.results.lock().expect("MockResultStore lock poisoned");
            Ok(guard.get(&task_id).cloned())
        }

        async fn get_state(&self, task_id: TaskId) -> celers_core::Result<celers_core::TaskState> {
            let guard = self.results.lock().expect("MockResultStore lock poisoned");
            let state = match guard.get(&task_id) {
                None => celers_core::TaskState::Pending,
                Some(TaskResultValue::Pending) => celers_core::TaskState::Pending,
                Some(TaskResultValue::Started) => celers_core::TaskState::Running,
                Some(TaskResultValue::Success(_)) => celers_core::TaskState::Succeeded(vec![]),
                Some(TaskResultValue::Failure { error, .. }) => {
                    celers_core::TaskState::Failed(error.clone())
                }
                Some(TaskResultValue::Revoked) => celers_core::TaskState::Revoked,
                _ => celers_core::TaskState::Pending,
            };
            Ok(state)
        }

        async fn forget(&self, task_id: TaskId) -> celers_core::Result<()> {
            self.results
                .lock()
                .expect("MockResultStore lock poisoned")
                .remove(&task_id);
            Ok(())
        }

        async fn has_result(&self, task_id: TaskId) -> celers_core::Result<bool> {
            Ok(self
                .results
                .lock()
                .expect("MockResultStore lock poisoned")
                .contains_key(&task_id))
        }
    }

    #[tokio::test]
    async fn test_e2e_mock_broker_run_loop() {
        use crate::client::OxifyCelersClient;
        use oxify_model::execution::ExecutionState;

        let broker = MockBroker::new();
        let result_store = MockResultStore::new();

        let client = OxifyCelersClient::new(broker.clone(), result_store.clone());

        // start→end workflow with no LLM nodes — no provider credentials needed.
        let wf = oxify_model::test_utils::create_test_workflow("e2e_mock", 0);
        let task_id = client.submit(&wf).await.expect("submit");

        // Register OxifyWorkflowTask and spawn the worker loop.
        let registry = TaskRegistry::new();
        registry.register(OxifyWorkflowTask).await;

        let broker_clone = broker.clone();
        let results_clone = result_store.clone();
        let handle =
            tokio::spawn(async move { run_loop(broker_clone, results_clone, registry).await });

        // Poll until result is available (up to 5 s).
        let ctx = client
            .await_result(task_id, Duration::from_secs(5))
            .await
            .expect("await_result");

        handle.abort();

        assert!(
            matches!(ctx.state, ExecutionState::Completed),
            "expected Completed, got {:?}",
            ctx.state
        );
    }

    /// Live Redis integration test — skipped unless Redis is reachable.
    #[tokio::test]
    #[ignore]
    async fn test_live_redis_worker_round_trip() {
        let redis_url = "redis://localhost:6379";
        let queue_name = "oxify_test_queue";

        let client = crate::client::OxifyCelersClient::with_redis(redis_url, queue_name)
            .expect("build client");

        let wf = oxify_model::test_utils::create_test_workflow("live_redis", 0);
        let task_id = client.submit(&wf).await.expect("submit");

        // Spawn the worker in the background.
        let _worker_handle = tokio::spawn(async move {
            run_worker(redis_url, queue_name)
                .await
                .expect("worker failed");
        });

        let ctx = client
            .await_result(task_id, Duration::from_secs(30))
            .await
            .expect("await_result");

        assert!(
            matches!(ctx.state, oxify_model::execution::ExecutionState::Completed),
            "expected Completed, got {:?}",
            ctx.state
        );
    }
}
