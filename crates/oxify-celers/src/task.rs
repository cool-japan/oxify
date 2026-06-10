//! [`OxifyWorkflowTask`] — a CeleRS [`Task`](celers_core::Task) that executes OxiFY workflows.

use crate::OXIFY_WORKFLOW_TASK_NAME;
use oxify_engine::Engine;
use oxify_model::execution::ExecutionContext;
use oxify_model::Workflow;
use serde::{Deserialize, Serialize};

/// Task input: the workflow to execute plus optional seed variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTaskInput {
    pub workflow: Workflow,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, serde_json::Value>,
}

/// The canonical CeleRS task name — must match on both submit and worker sides.
pub const TASK_NAME: &str = OXIFY_WORKFLOW_TASK_NAME;

/// CeleRS task that executes an OxiFY workflow using a local [`Engine`].
///
/// # Operational note
///
/// The [`Engine`] is created per-invocation (stateless). The worker process
/// must be started with whatever provider credentials are required for the
/// workflow nodes (e.g. `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`).
pub struct OxifyWorkflowTask;

#[async_trait::async_trait]
impl celers_core::Task for OxifyWorkflowTask {
    type Input = WorkflowTaskInput;
    type Output = ExecutionContext;

    fn name(&self) -> &str {
        TASK_NAME
    }

    async fn execute(&self, input: Self::Input) -> celers_core::Result<Self::Output> {
        let engine = Engine::new();
        engine
            .execute(&input.workflow)
            .await
            .map_err(|e| celers_core::CelersError::TaskExecution(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_task_input_serde_roundtrip() {
        let wf = oxify_model::test_utils::create_test_workflow("celers_serde", 0);
        let input = WorkflowTaskInput {
            workflow: wf.clone(),
            variables: Default::default(),
        };
        let bytes = serde_json::to_vec(&input).expect("serialize");
        let decoded: WorkflowTaskInput = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(decoded.workflow.metadata.id, wf.metadata.id);
    }

    #[test]
    fn test_workflow_task_input_serde_with_variables() {
        let wf = oxify_model::test_utils::create_test_workflow("celers_vars", 0);
        let mut vars = std::collections::HashMap::new();
        vars.insert("key".to_string(), serde_json::json!("value"));
        let input = WorkflowTaskInput {
            workflow: wf.clone(),
            variables: vars,
        };
        let bytes = serde_json::to_vec(&input).expect("serialize");
        let decoded: WorkflowTaskInput = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(
            decoded.variables.get("key"),
            Some(&serde_json::json!("value"))
        );
        assert_eq!(decoded.workflow.metadata.id, wf.metadata.id);
    }

    #[tokio::test]
    async fn test_oxify_workflow_task_execute_simple() {
        use celers_core::Task;
        use oxify_model::execution::ExecutionState;

        // node_count=0 → start+end only via builder (no LLM nodes, no provider needed)
        let wf = oxify_model::test_utils::create_test_workflow("test_celers_exec", 0);
        let input = WorkflowTaskInput {
            workflow: wf,
            variables: Default::default(),
        };
        let result = OxifyWorkflowTask.execute(input).await.expect("execute");
        assert!(
            matches!(result.state, ExecutionState::Completed),
            "expected Completed, got {:?}",
            result.state
        );
    }
}
