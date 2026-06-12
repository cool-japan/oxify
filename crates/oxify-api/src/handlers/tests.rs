//! Auto-generated test module (consolidated from inline `#[cfg(test)] mod` blocks)

use crate::storage::ExecutionStoreBackend;
use crate::types::*;
use oxify_model::ExecutionContext;

use super::*;

#[cfg(test)]
mod tests_2 {
    use super::*;
    use oxify_engine::execution_events;
    use oxify_model::{Edge, Node, NodeKind, Workflow, WorkflowMetadata};
    /// Build a minimal Start → End workflow for testing.
    fn build_test_workflow() -> Workflow {
        let mut workflow = Workflow::new("Test Workflow".to_string());
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);
        let start_id = start.id;
        let end_id = end.id;
        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));
        workflow
    }
    /// Verify that the execution_id returned by `execute_workflow` logic is
    /// the same id used as the storage key, so `execution_store.get(returned_id)`
    /// reliably finds the row immediately after creation.
    #[tokio::test]
    async fn test_execute_workflow_execution_id_consistency() {
        let state = AppState::new();
        let workflow = build_test_workflow();
        let workflow_id = state
            .workflow_store
            .create(workflow.clone())
            .await
            .expect("workflow create");
        let mut ctx = ExecutionContext::new(workflow_id);
        let execution_id = ctx.execution_id;
        ctx.set_variable("test_key".to_string(), serde_json::Value::from("test_val"));
        let stored_id = state
            .execution_store
            .create(ctx.clone())
            .await
            .expect("execution create");
        assert_eq!(
            stored_id, execution_id,
            "storage key must equal ctx.execution_id"
        );
        let found = state
            .execution_store
            .get(&execution_id)
            .await
            .expect("get ok")
            .expect("row must exist");
        assert_eq!(found.execution_id, execution_id);
    }
    /// Verify that execute_with_context preserves the caller's execution_id
    /// and emits events carrying that same id on the event bus.
    #[tokio::test]
    async fn test_execute_with_context_emits_bus_events_with_correct_id() {
        let state = AppState::new();
        let mut rx = state.event_bus.subscribe();
        let workflow = build_test_workflow();
        let workflow_id = state
            .workflow_store
            .create(workflow.clone())
            .await
            .expect("workflow create");
        let ctx = ExecutionContext::new(workflow_id);
        let execution_id = ctx.execution_id;
        let result = state
            .engine
            .execute_with_context(
                &workflow,
                ctx,
                oxify_engine::ExecutionConfig::new().with_events(),
            )
            .await
            .expect("execution should succeed");
        assert_eq!(result.execution_id, execution_id);
        let mut saw_started = false;
        let mut saw_completed = false;
        while let Ok(ev) = rx.try_recv() {
            if ev.execution_id != Some(execution_id) {
                continue;
            }
            match ev.event_type.as_str() {
                s if s == execution_events::WORKFLOW_STARTED => saw_started = true,
                s if s == execution_events::WORKFLOW_COMPLETED => saw_completed = true,
                _ => {}
            }
        }
        assert!(saw_started, "expected workflow.started event on the bus");
        assert!(
            saw_completed,
            "expected workflow.completed event on the bus"
        );
    }
    /// Verify the in-memory ExecutionStore uses ctx.execution_id as the key,
    /// making update() reliable after create().
    #[tokio::test]
    async fn test_in_memory_store_create_uses_execution_id() {
        use crate::storage::ExecutionStoreBackend;
        let store = ExecutionStoreBackend::new_in_memory();
        let workflow_id = WorkflowMetadata::new("w".to_string()).id;
        let ctx = ExecutionContext::new(workflow_id);
        let expected_id = ctx.execution_id;
        let returned_id = store.create(ctx.clone()).await.expect("create ok");
        assert_eq!(
            returned_id, expected_id,
            "create must return ctx.execution_id as the storage key"
        );
        let found = store.get(&expected_id).await.expect("get ok");
        assert!(
            found.is_some(),
            "row must be retrievable by ctx.execution_id"
        );
        let mut updated_ctx = ctx;
        updated_ctx.state = oxify_model::ExecutionState::Completed;
        let update_result = store
            .update(&expected_id, updated_ctx)
            .await
            .expect("update ok");
        assert!(
            update_result.is_some(),
            "update must find the row by the same id"
        );
    }
}
