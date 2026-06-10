//! Full page handlers (HTML responses)

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::UiError;
use crate::mock;
use crate::state::AppState;
use crate::templates::{
    DashboardTemplate, ExecutionCompareData, ExecutionCompareTemplate, ExecutionDetailTemplate,
    ExecutionListTemplate, InternalErrorPage, NodeComparisonRow, NodeResultData, NotFoundPage,
    SettingsTemplate, TemplatesPageTemplate, WorkflowDetailTemplate, WorkflowEditTemplate,
    WorkflowListTemplate, WorkflowNewTemplate,
};

/// Wrapper to render an Askama template as HTML with a given status code.
struct HtmlTemplate<T>(T);

impl<T: askama::Template> IntoResponse for HtmlTemplate<T> {
    fn into_response(self) -> axum::response::Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template error: {e}"),
            )
                .into_response(),
        }
    }
}

/// Custom 404 handler
pub async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, HtmlTemplate(NotFoundPage))
}

/// Custom 500 handler — `request_path` is passed explicitly by callers.
pub async fn handler_500(request_path: String) -> impl IntoResponse {
    let error_id = uuid::Uuid::new_v4().to_string();
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        HtmlTemplate(InternalErrorPage {
            error_id,
            request_path,
        }),
    )
}

/// Dashboard home page
pub async fn dashboard(State(state): State<Arc<AppState>>) -> Result<Html<String>, UiError> {
    let stats = if state.is_mock_data_enabled().await {
        mock::mock_dashboard_stats()
    } else {
        state
            .api_client
            .get_dashboard_stats()
            .await
            .unwrap_or_else(|_| mock::mock_dashboard_stats())
    };

    let template = DashboardTemplate {
        title: "Dashboard".to_string(),
        active_workflows: stats.active_workflows,
        running_executions: stats.running_executions,
        completed_today: stats.completed_today,
        failed_today: stats.failed_today,
    };

    Ok(Html(template.render()?))
}

/// Workflow list page
pub async fn workflow_list(State(_state): State<Arc<AppState>>) -> Result<Html<String>, UiError> {
    let template = WorkflowListTemplate {
        title: "Workflows".to_string(),
        workflows: vec![], // Will be loaded via HTMX
    };

    Ok(Html(template.render()?))
}

/// New workflow page
pub async fn workflow_new(State(_state): State<Arc<AppState>>) -> Result<Html<String>, UiError> {
    let template = WorkflowNewTemplate {
        title: "New Workflow".to_string(),
    };

    Ok(Html(template.render()?))
}

/// Workflow detail page
pub async fn workflow_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    let workflow_data = if state.is_mock_data_enabled().await {
        mock::mock_workflow_detail(id)
    } else {
        match state.api_client.get_workflow(id).await {
            Ok(workflow) => mock::MockWorkflowDetail {
                id: workflow.metadata.id,
                name: workflow.metadata.name.clone(),
                description: workflow.metadata.description.clone(),
                version: workflow.metadata.version.clone(),
                status: "active".to_string(),
                node_count: workflow.nodes.len() as u32,
                edge_count: workflow.edges.len() as u32,
                tags: workflow.metadata.tags.clone(),
                created_at: workflow.metadata.created_at.to_rfc3339(),
                updated_at: workflow.metadata.updated_at.to_rfc3339(),
                last_executed: None,
                nodes_json: serde_json::to_value(&workflow.nodes).unwrap_or_default(),
                edges_json: serde_json::to_value(&workflow.edges).unwrap_or_default(),
            },
            Err(_) => mock::mock_workflow_detail(id),
        }
    };

    let template = WorkflowDetailTemplate {
        title: "Workflow Details".to_string(),
        workflow_id: id,
        workflow_name: workflow_data.name,
        workflow_description: workflow_data.description,
        node_count: workflow_data.node_count,
        edge_count: workflow_data.edge_count,
        last_executed: workflow_data.last_executed,
    };

    Ok(Html(template.render()?))
}

/// Workflow edit page (DAG editor)
pub async fn workflow_edit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    let workflow_data = if state.is_mock_data_enabled().await {
        mock::mock_workflow_detail(id)
    } else {
        match state.api_client.get_workflow(id).await {
            Ok(workflow) => mock::MockWorkflowDetail {
                id: workflow.metadata.id,
                name: workflow.metadata.name.clone(),
                description: workflow.metadata.description.clone(),
                version: workflow.metadata.version.clone(),
                status: "active".to_string(),
                node_count: workflow.nodes.len() as u32,
                edge_count: workflow.edges.len() as u32,
                tags: workflow.metadata.tags.clone(),
                created_at: workflow.metadata.created_at.to_rfc3339(),
                updated_at: workflow.metadata.updated_at.to_rfc3339(),
                last_executed: None,
                nodes_json: serde_json::to_value(&workflow.nodes).unwrap_or_default(),
                edges_json: serde_json::to_value(&workflow.edges).unwrap_or_default(),
            },
            Err(_) => mock::mock_workflow_detail(id),
        }
    };

    // Convert to editor format
    let workflow_json = serde_json::json!({
        "nodes": workflow_data.nodes_json,
        "edges": workflow_data.edges_json
    })
    .to_string();

    let template = WorkflowEditTemplate {
        title: "Edit Workflow".to_string(),
        workflow_id: id,
        workflow_name: workflow_data.name,
        workflow_json,
    };

    Ok(Html(template.render()?))
}

/// Execution list page
pub async fn execution_list(State(_state): State<Arc<AppState>>) -> Result<Html<String>, UiError> {
    let template = ExecutionListTemplate {
        title: "Executions".to_string(),
        executions: vec![], // Will be loaded via HTMX
    };

    Ok(Html(template.render()?))
}

/// Execution detail page (with real-time monitoring)
pub async fn execution_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    let execution_data = if state.is_mock_data_enabled().await {
        mock::mock_execution_detail(id)
    } else {
        match state.api_client.get_execution(id).await {
            Ok(execution) => {
                let status = match &execution.state {
                    oxify_model::ExecutionState::Running => "running".to_string(),
                    oxify_model::ExecutionState::Completed => "completed".to_string(),
                    oxify_model::ExecutionState::Failed(msg) => format!("failed: {}", msg),
                    oxify_model::ExecutionState::Cancelled => "cancelled".to_string(),
                    oxify_model::ExecutionState::Paused => "paused".to_string(),
                };
                let progress = match execution.state {
                    oxify_model::ExecutionState::Completed => 100,
                    oxify_model::ExecutionState::Running => {
                        // Calculate progress from node results
                        let completed = execution.node_results.len() as u32;
                        // Assume 5 nodes for calculation
                        (completed * 100) / 5
                    }
                    _ => 0,
                };
                mock::MockExecutionDetail {
                    id: execution.execution_id,
                    workflow_id: execution.workflow_id,
                    workflow_name: "Workflow".to_string(),
                    status,
                    progress,
                    started_at: execution.started_at.to_rfc3339(),
                    completed_at: execution.completed_at.map(|t| t.to_rfc3339()),
                    duration: execution.completed_at.map(|completed| {
                        let duration = completed - execution.started_at;
                        format!(
                            "{}m {}s",
                            duration.num_minutes(),
                            duration.num_seconds() % 60
                        )
                    }),
                    current_node: None,
                    node_results: vec![],
                    variables: serde_json::to_value(&execution.variables).unwrap_or_default(),
                }
            }
            Err(_) => mock::mock_execution_detail(id),
        }
    };

    let template = ExecutionDetailTemplate {
        title: "Execution Details".to_string(),
        execution_id: id,
        workflow_name: execution_data.workflow_name,
        status: execution_data.status,
        progress: execution_data.progress,
        started_at: execution_data.started_at,
    };

    Ok(Html(template.render()?))
}

/// Settings page
pub async fn settings(State(state): State<Arc<AppState>>) -> Result<Html<String>, UiError> {
    let template = SettingsTemplate {
        title: "Settings".to_string(),
        dark_mode: false, // Would be loaded from user preferences
        api_url: state.api_base_url.clone(),
    };

    Ok(Html(template.render()?))
}

/// Template gallery page
pub async fn templates_gallery(
    State(_state): State<Arc<AppState>>,
) -> Result<Html<String>, UiError> {
    let template = TemplatesPageTemplate {
        title: "Templates".to_string(),
        templates: mock::mock_templates(),
    };

    Ok(Html(template.render()?))
}

/// Search results page
pub async fn search(State(_state): State<Arc<AppState>>) -> Result<Html<String>, UiError> {
    #[derive(Template)]
    #[template(path = "pages/search.html")]
    #[allow(dead_code)]
    struct SearchTemplate {
        title: String,
    }

    let template = SearchTemplate {
        title: "Search".to_string(),
    };

    Ok(Html(template.render()?))
}

/// Query parameters for execution comparison
#[derive(Debug, Deserialize)]
pub struct ExecutionCompareQuery {
    pub ids: String, // Comma-separated execution IDs
}

/// Execution comparison page
pub async fn execution_compare(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExecutionCompareQuery>,
) -> Result<Html<String>, UiError> {
    // Parse execution IDs from query string
    let execution_ids: Vec<Uuid> = query
        .ids
        .split(',')
        .filter_map(|id| Uuid::parse_str(id.trim()).ok())
        .collect();

    if execution_ids.is_empty() {
        return Err(UiError::BadRequest(
            "At least one execution ID is required".to_string(),
        ));
    }

    if execution_ids.len() > 4 {
        return Err(UiError::BadRequest(
            "Maximum 4 executions can be compared".to_string(),
        ));
    }

    // Fetch execution data for each ID
    let mut executions = Vec::new();
    let mut all_node_names = HashSet::new();
    let mut execution_node_results: Vec<HashMap<String, NodeResultData>> = Vec::new();

    for exec_id in &execution_ids {
        let exec_data = if state.is_mock_data_enabled().await {
            mock::mock_execution_detail(*exec_id)
        } else {
            match state.api_client.get_execution(*exec_id).await {
                Ok(execution) => {
                    let status = match &execution.state {
                        oxify_model::ExecutionState::Running => "running".to_string(),
                        oxify_model::ExecutionState::Completed => "completed".to_string(),
                        oxify_model::ExecutionState::Failed(msg) => format!("failed: {}", msg),
                        oxify_model::ExecutionState::Cancelled => "cancelled".to_string(),
                        oxify_model::ExecutionState::Paused => "paused".to_string(),
                    };
                    let progress = match execution.state {
                        oxify_model::ExecutionState::Completed => 100,
                        oxify_model::ExecutionState::Running => {
                            let completed = execution.node_results.len() as u32;
                            (completed * 100) / 5
                        }
                        _ => 0,
                    };
                    mock::MockExecutionDetail {
                        id: execution.execution_id,
                        workflow_id: execution.workflow_id,
                        workflow_name: "Workflow".to_string(),
                        status,
                        progress,
                        started_at: execution.started_at.to_rfc3339(),
                        completed_at: execution.completed_at.map(|t| t.to_rfc3339()),
                        duration: execution.completed_at.map(|completed| {
                            let duration = completed - execution.started_at;
                            format!(
                                "{}m {}s",
                                duration.num_minutes(),
                                duration.num_seconds() % 60
                            )
                        }),
                        current_node: None,
                        node_results: vec![],
                        variables: serde_json::to_value(&execution.variables).unwrap_or_default(),
                    }
                }
                Err(_) => mock::mock_execution_detail(*exec_id),
            }
        };

        // Collect node names
        for node in &exec_data.node_results {
            all_node_names.insert(node.0.clone());
        }

        // Build node results map
        let mut node_results = HashMap::new();
        for (node_name, node_status, _progress) in &exec_data.node_results {
            node_results.insert(
                node_name.clone(),
                NodeResultData {
                    status: node_status.clone(),
                    duration: "< 1s".to_string(),
                },
            );
        }

        execution_node_results.push(node_results);

        executions.push(ExecutionCompareData {
            id: exec_data.id,
            workflow_name: exec_data.workflow_name,
            status: exec_data.status,
            progress: exec_data.progress,
            duration: exec_data.duration.unwrap_or_else(|| "--".to_string()),
            started_at: exec_data.started_at,
            completed_at: exec_data.completed_at.unwrap_or_else(|| "--".to_string()),
        });
    }

    // Build node comparison rows
    let node_names: Vec<String> = all_node_names.into_iter().collect();
    let mut node_comparison = Vec::new();

    for node_name in &node_names {
        let mut results = Vec::new();
        for exec_results in &execution_node_results {
            results.push(exec_results.get(node_name).cloned());
        }
        node_comparison.push(NodeComparisonRow {
            node_name: node_name.clone(),
            results,
        });
    }

    let execution_count = execution_ids.len();

    let template = ExecutionCompareTemplate {
        title: "Compare Executions".to_string(),
        execution_ids,
        execution_count,
        executions,
        node_comparison,
    };

    Ok(Html(template.render()?))
}
