//! API handlers for checkpoint/pause/resume operations

use crate::checkpoint_types::*;
use crate::handlers::AppState;
use crate::types::ErrorResponse;
use crate::user_types::ApiUser;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use oxify_storage::ExecutionCheckpoint;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

/// Pause an execution and create a checkpoint
#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/pause",
    params(
        ("id" = Uuid, Path, description = "Execution ID")
    ),
    request_body = PauseExecutionRequest,
    responses(
        (status = 200, description = "Execution paused", body = PauseExecutionResponse),
        (status = 404, description = "Execution not found", body = ErrorResponse),
        (status = 503, description = "Checkpointing not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn pause_execution(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<PauseExecutionRequest>,
) -> Result<Json<PauseExecutionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Pausing execution: {}", id);

    let checkpoint_store = state.checkpoint_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Checkpoint/pause functionality is not enabled".to_string(),
            }),
        )
    })?;

    // Get the execution
    let execution = state
        .execution_store
        .get(&id)
        .await
        .map_err(|e| {
            error!("Failed to get execution: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get execution: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("Execution {} not found", id),
                }),
            )
        })?;

    // Create checkpoint
    let reason = req.reason.unwrap_or_else(|| "manual_pause".to_string());

    let mut checkpoint = ExecutionCheckpoint::new(
        execution.workflow_id,
        id,
        execution.clone(),
        vec![], // Would need to track completed nodes in execution
        0,      // Would need to track current level
        reason,
    );
    checkpoint.paused = true;

    let checkpoint_id = checkpoint_store.save(&checkpoint).await.map_err(|e| {
        error!("Failed to save checkpoint: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to save checkpoint: {}", e),
            }),
        )
    })?;

    info!("Execution {} paused with checkpoint {}", id, checkpoint_id);

    Ok(Json(PauseExecutionResponse {
        checkpoint_id,
        message: "Execution paused successfully".to_string(),
    }))
}

/// Resume a paused execution from its latest checkpoint.
///
/// Loads the most recent checkpoint for `id`, bridges it into the engine-side
/// checkpoint type, and re-drives the workflow from where it left off in a
/// background task (mirroring [`crate::handlers::execute_workflow`]). The
/// engine's [`Engine::execute_from_checkpoint`](oxify_engine::Engine::execute_from_checkpoint)
/// skips every already-completed level and node, so side-effecting nodes that
/// ran before the pause are never re-executed on resume.
#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/resume",
    params(
        ("id" = Uuid, Path, description = "Execution ID")
    ),
    responses(
        (
            status = 202,
            description = "Execution resume initiated",
            body = ResumeExecutionResponse
        ),
        (status = 404, description = "Execution or checkpoint not found", body = ErrorResponse),
        (status = 503, description = "Checkpointing not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn resume_execution(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<ResumeExecutionResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Resuming execution: {}", id);

    let checkpoint_store = state.checkpoint_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Checkpoint/resume functionality is not enabled".to_string(),
            }),
        )
    })?;

    // Load the latest checkpoint (storage-layer type).
    let checkpoint = checkpoint_store
        .load_latest(id)
        .await
        .map_err(|e| {
            error!("Failed to load checkpoint: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to load checkpoint: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!("No checkpoint found for execution {}", id),
                }),
            )
        })?;

    // Fetch the workflow this checkpoint belongs to (needed to re-drive the DAG).
    let workflow = state
        .workflow_store
        .get(&checkpoint.workflow_id)
        .await
        .map_err(|e| {
            error!("Failed to get workflow: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to get workflow: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "NotFound".to_string(),
                    message: format!(
                        "Workflow {} for execution {} not found",
                        checkpoint.workflow_id, id
                    ),
                }),
            )
        })?;

    // Bridge the storage checkpoint into the engine checkpoint the executor consumes.
    let engine_checkpoint = storage_checkpoint_to_engine(checkpoint);

    // Drive the resume in the background and return 202 immediately, mirroring the
    // execute_workflow pattern so callers poll / stream the execution for completion.
    state.http_metrics.inc_active_execution();
    let engine = state.engine.clone();
    let execution_store = state.execution_store.clone();
    let metrics = state.http_metrics.clone();
    tokio::spawn(async move {
        let result = engine
            .execute_from_checkpoint(&workflow, engine_checkpoint)
            .await;
        metrics.dec_active_execution();
        match result {
            Ok(mut result_ctx) => {
                // `execute_from_checkpoint` runs every remaining level/node but,
                // unlike the normal run path, does not itself mark the context
                // completed; do that here before persisting the final state.
                result_ctx.state = oxify_model::ExecutionState::Completed;
                result_ctx.mark_completed();
                match execution_store.update(&id, result_ctx).await {
                    Ok(Some(_)) => info!("Execution {} resumed and completed", id),
                    Ok(None) => {
                        error!(
                            "Failed to update resumed execution {}: execution not found",
                            id
                        )
                    }
                    Err(e) => error!("Failed to update resumed execution {}: {}", id, e),
                }
            }
            Err(e) => error!("Resume of execution {} failed: {}", id, e),
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(ResumeExecutionResponse {
            execution_id: id,
            message: "Execution resume initiated".to_string(),
        }),
    ))
}

/// Bridge the storage-layer [`oxify_storage::ExecutionCheckpoint`] into the
/// engine-layer [`oxify_engine::ExecutionCheckpoint`] consumed by
/// [`Engine::execute_from_checkpoint`](oxify_engine::Engine::execute_from_checkpoint).
///
/// The two structs are field-for-field identical except for `created_at`:
/// storage persists a [`chrono::DateTime<chrono::Utc>`], whereas the engine
/// checkpoint holds a [`std::time::SystemTime`]. Both denote the same absolute
/// instant, and chrono provides a total `From<DateTime<Utc>> for SystemTime`
/// conversion, so the bridge is lossless. Every other field (ids, context,
/// completed nodes, node results, level, paused flag, reason) is moved across
/// unchanged.
fn storage_checkpoint_to_engine(
    checkpoint: ExecutionCheckpoint,
) -> oxify_engine::ExecutionCheckpoint {
    oxify_engine::ExecutionCheckpoint {
        id: checkpoint.id,
        workflow_id: checkpoint.workflow_id,
        execution_id: checkpoint.execution_id,
        context: checkpoint.context,
        completed_nodes: checkpoint.completed_nodes,
        node_results: checkpoint.node_results,
        current_level: checkpoint.current_level,
        paused: checkpoint.paused,
        created_at: std::time::SystemTime::from(checkpoint.created_at),
        reason: checkpoint.reason,
    }
}

/// List checkpoints for an execution
#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/checkpoints",
    params(
        ("id" = Uuid, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Checkpoints retrieved", body = ListCheckpointsResponse),
        (status = 503, description = "Checkpointing not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_execution_checkpoints(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<ListCheckpointsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing checkpoints for execution: {}", id);

    let checkpoint_store = state.checkpoint_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Checkpoint functionality is not enabled".to_string(),
            }),
        )
    })?;

    let checkpoints = checkpoint_store.list_by_execution(id).await.map_err(|e| {
        error!("Failed to list checkpoints: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "StorageError".to_string(),
                message: format!("Failed to list checkpoints: {}", e),
            }),
        )
    })?;

    let summaries: Vec<CheckpointSummary> = checkpoints
        .into_iter()
        .map(|c| CheckpointSummary {
            id: c.id,
            workflow_id: c.workflow_id,
            execution_id: c.execution_id,
            completed_nodes_count: c.completed_nodes.len(),
            current_level: c.current_level,
            paused: c.paused,
            reason: c.reason,
            created_at: c.created_at,
        })
        .collect();

    let total = summaries.len();

    Ok(Json(ListCheckpointsResponse {
        checkpoints: summaries,
        total,
    }))
}

/// Delete all checkpoints for an execution
#[utoipa::path(
    delete,
    path = "/api/v1/executions/{id}/checkpoints",
    params(
        ("id" = Uuid, Path, description = "Execution ID")
    ),
    responses(
        (status = 200, description = "Checkpoints deleted", body = DeleteCheckpointsResponse),
        (status = 503, description = "Checkpointing not enabled", body = ErrorResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_execution_checkpoints(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<ApiUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteCheckpointsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Deleting checkpoints for execution: {}", id);

    let checkpoint_store = state.checkpoint_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ServiceUnavailable".to_string(),
                message: "Checkpoint functionality is not enabled".to_string(),
            }),
        )
    })?;

    let deleted_count = checkpoint_store
        .delete_by_execution(id)
        .await
        .map_err(|e| {
            error!("Failed to delete checkpoints: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "StorageError".to_string(),
                    message: format!("Failed to delete checkpoints: {}", e),
                }),
            )
        })?;

    info!("Deleted {} checkpoints for execution {}", deleted_count, id);

    Ok(Json(DeleteCheckpointsResponse {
        deleted_count,
        message: format!("Deleted {} checkpoints", deleted_count),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::AppState;
    use oxify_engine::{EngineBuilder, NodePlugin, PluginRegistry};
    use oxify_model::{
        CustomConfig, Edge, ExecutionContext, ExecutionResult, ExecutionState, Node,
        NodeExecutionResult, NodeKind, Workflow,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// A plugin that increments a shared counter on every execution. It lets a
    /// test observe whether an already-completed node is (incorrectly)
    /// re-executed during resume — the counter must only ever reflect the nodes
    /// that were genuinely still pending.
    struct CountingPlugin {
        counter: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl NodePlugin for CountingPlugin {
        fn name(&self) -> &str {
            "counter"
        }
        fn version(&self) -> &str {
            "1.0.0"
        }
        fn supported_node_types(&self) -> Vec<String> {
            vec!["counter".to_string()]
        }
        async fn execute(
            &self,
            _node: &Node,
            _context: &ExecutionContext,
        ) -> std::result::Result<ExecutionResult, String> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(ExecutionResult::Success(serde_json::json!({
                "counted": true
            })))
        }
    }

    fn counter_node(name: &str) -> Node {
        Node::new(
            name.to_string(),
            NodeKind::Custom(CustomConfig {
                plugin_id: "counter".to_string(),
                plugin_version: None,
                config: serde_json::Value::Null,
            }),
        )
    }

    /// End-to-end resume: a checkpoint with `Start` + `Count1` already completed
    /// is resumed; only `Count2` and `End` must run, the completed nodes must not
    /// be re-executed, and the persisted context must reach `Completed` with all
    /// four node results present.
    #[tokio::test]
    async fn test_resume_skips_completed_nodes_and_completes() {
        // Engine wired with the shared counting plugin.
        let counter = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(PluginRegistry::new());
        registry
            .register(Arc::new(CountingPlugin {
                counter: counter.clone(),
            }))
            .await
            .expect("register counting plugin");
        let engine = Arc::new(EngineBuilder::new().with_plugin_registry(registry).build());

        // Workflow: Start -> Count1 -> Count2 -> End (exactly one Start / one End).
        let mut workflow = Workflow::new("resume-test".to_string());
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let count1 = counter_node("Count1");
        let count2 = counter_node("Count2");
        let end = Node::new("End".to_string(), NodeKind::End);
        let (start_id, count1_id, count2_id, end_id) = (start.id, count1.id, count2.id, end.id);
        workflow.add_node(start);
        workflow.add_node(count1);
        workflow.add_node(count2);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, count1_id));
        workflow.add_edge(Edge::new(count1_id, count2_id));
        workflow.add_edge(Edge::new(count2_id, end_id));
        workflow.validate().expect("workflow should be valid");
        let workflow_id = workflow.metadata.id;

        // In-memory (`:memory:`, single connection) checkpoint store.
        let config = oxify_storage::DatabaseConfig {
            database_url: ":memory:".to_string(),
            max_connections: 1,
            min_connections: 1,
        };
        let pool = oxify_storage::DatabasePool::new(config)
            .await
            .expect("create in-memory pool");
        let checkpoint_store =
            Arc::new(oxify_storage::checkpoint_store::DatabaseCheckpointStore::new(pool.clone()));
        checkpoint_store
            .ensure_table()
            .await
            .expect("create checkpoint table");

        // App state: in-memory stores + counting engine + real checkpoint store.
        let mut app_state = AppState::new();
        app_state.engine = engine;
        app_state.checkpoint_store = Some(checkpoint_store.clone());
        app_state
            .workflow_store
            .create(workflow)
            .await
            .expect("store workflow");

        // Initial running execution record.
        let mut exec_ctx = ExecutionContext::new(workflow_id);
        let execution_id = exec_ctx.execution_id;
        app_state
            .execution_store
            .create(exec_ctx.clone())
            .await
            .expect("store execution");

        // Checkpoint snapshot: Start + Count1 already completed, with distinctive
        // sentinel results that a recompute would overwrite.
        let sentinel_start = NodeExecutionResult::new().complete(ExecutionResult::Success(
            serde_json::json!({ "sentinel": "start" }),
        ));
        let sentinel_count1 = NodeExecutionResult::new().complete(ExecutionResult::Success(
            serde_json::json!({ "sentinel": "count1" }),
        ));
        exec_ctx.record_node_result(start_id, sentinel_start);
        exec_ctx.record_node_result(count1_id, sentinel_count1.clone());

        let checkpoint = oxify_storage::ExecutionCheckpoint::new(
            workflow_id,
            execution_id,
            exec_ctx,
            vec![start_id, count1_id],
            0,
            "test_pause".to_string(),
        );
        checkpoint_store
            .save(&checkpoint)
            .await
            .expect("save checkpoint");

        // Invoke the real resume handler.
        let state = Arc::new(app_state);
        let user = ApiUser::new(
            "tester".to_string(),
            "tester@example.com".to_string(),
            "hash".to_string(),
        );
        let (status, _body) =
            resume_execution(State(state.clone()), Extension(user), Path(execution_id))
                .await
                .expect("resume should be accepted");
        assert_eq!(status, StatusCode::ACCEPTED);

        // The handler returns 202 immediately; wait for the background task.
        let mut final_ctx = None;
        for _ in 0..200 {
            let ctx = state
                .execution_store
                .get(&execution_id)
                .await
                .expect("get execution")
                .expect("execution exists");
            if ctx.state == ExecutionState::Completed {
                final_ctx = Some(ctx);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let final_ctx = final_ctx.expect("resume did not complete within timeout");

        // (a) The completed Count1 node was NOT re-executed: only Count2 ran, so
        //     the shared counter is exactly 1 (it would be 2 had Count1 re-run).
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "only the still-pending counter node should have executed on resume"
        );

        // (b) Final context is Completed with every node's result present.
        assert_eq!(final_ctx.state, ExecutionState::Completed);
        assert!(final_ctx.completed_at.is_some());
        for node_id in [start_id, count1_id, count2_id, end_id] {
            assert!(
                final_ctx.node_results.contains_key(&node_id),
                "missing result for node {node_id}"
            );
        }

        // Count1's sentinel survived untouched (proving no recompute)...
        assert_eq!(
            final_ctx.node_results.get(&count1_id).map(|r| &r.result),
            Some(&sentinel_count1.result),
            "completed node result must be preserved, not recomputed"
        );
        // ...while Count2 carries the plugin's freshly produced output.
        assert_eq!(
            final_ctx.node_results.get(&count2_id).map(|r| &r.result),
            Some(&ExecutionResult::Success(
                serde_json::json!({ "counted": true })
            )),
        );
    }

    /// The storage -> engine checkpoint bridge is lossless, including the
    /// `DateTime<Utc>` -> `SystemTime` timestamp conversion.
    #[test]
    fn test_storage_checkpoint_to_engine_bridge_preserves_fields() {
        let workflow_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();
        let ctx = ExecutionContext::new(workflow_id);
        let node_a = Uuid::new_v4();

        let mut storage_checkpoint = oxify_storage::ExecutionCheckpoint::new(
            workflow_id,
            execution_id,
            ctx,
            vec![node_a],
            3,
            "bridge_test".to_string(),
        );
        storage_checkpoint.paused = true;
        let expected_instant = std::time::SystemTime::from(storage_checkpoint.created_at);
        let expected_id = storage_checkpoint.id;

        let engine_checkpoint = storage_checkpoint_to_engine(storage_checkpoint);

        assert_eq!(engine_checkpoint.id, expected_id);
        assert_eq!(engine_checkpoint.workflow_id, workflow_id);
        assert_eq!(engine_checkpoint.execution_id, execution_id);
        assert_eq!(engine_checkpoint.completed_nodes, vec![node_a]);
        assert_eq!(engine_checkpoint.current_level, 3);
        assert!(engine_checkpoint.paused);
        assert_eq!(engine_checkpoint.reason, "bridge_test");
        assert_eq!(engine_checkpoint.created_at, expected_instant);
    }
}
