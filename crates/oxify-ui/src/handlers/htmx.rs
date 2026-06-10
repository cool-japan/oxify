//! HTMX partial handlers (HTML fragment responses)

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::Html,
    Form,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::{ExecutionListQuery, WorkflowListQuery};
use crate::error::UiError;
use crate::mock;
use crate::state::AppState;
use crate::templates::partials::{
    ExecutionListPartial, ExecutionLogs, ExecutionRow, ExecutionRowsPartial, ExecutionStatus,
    NodeFormPartial, TemplateDetailPartial, TemplateListPartial, ToastPartial, WorkflowCard,
    WorkflowListPartial, WorkflowPreview,
};
use crate::validation;

/// Query parameters for workflow search
#[derive(Debug, Deserialize)]
pub struct WorkflowSearchQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub sort: Option<String>,
    pub page: Option<u32>,
}

/// Workflow list partial (HTMX)
pub async fn workflow_list_partial(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WorkflowSearchQuery>,
) -> Result<Html<String>, UiError> {
    let api_query = WorkflowListQuery {
        search: query.q.clone(),
        status: query.status.clone(),
        sort: query.sort.clone(),
        page: query.page,
        per_page: Some(10),
    };

    let workflows_response = if state.is_mock_data_enabled().await {
        mock::mock_workflow_list(api_query)
    } else {
        state
            .api_client
            .list_workflows(api_query.clone())
            .await
            .unwrap_or_else(|_| mock::mock_workflow_list(api_query))
    };

    let workflows: Vec<WorkflowCard> = workflows_response
        .items
        .into_iter()
        .map(|w| WorkflowCard {
            id: w.id,
            name: w.name,
            description: w.description,
            node_count: w.node_count,
            status: w.status,
            last_run: w.last_run,
        })
        .collect();

    let template = WorkflowListPartial {
        workflows,
        query: query.q.unwrap_or_default(),
        page: query.page.unwrap_or(1),
        has_more: workflows_response.has_more,
    };

    Ok(Html(template.render()?))
}

/// Workflow search (HTMX debounced)
pub async fn workflow_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WorkflowSearchQuery>,
) -> Result<Html<String>, UiError> {
    workflow_list_partial(State(state), Query(query)).await
}

/// Single workflow card (HTMX)
pub async fn workflow_card(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    let card = if state.is_mock_data_enabled().await {
        let detail = mock::mock_workflow_detail(id);
        WorkflowCard {
            id: detail.id,
            name: detail.name,
            description: detail.description,
            node_count: detail.node_count,
            status: detail.status,
            last_run: detail.last_executed,
        }
    } else {
        match state.api_client.get_workflow_summary(id).await {
            Ok(summary) => WorkflowCard {
                id: summary.id,
                name: summary.name,
                description: summary.description,
                node_count: summary.node_count,
                status: summary.status,
                last_run: summary.last_run,
            },
            Err(_) => {
                let detail = mock::mock_workflow_detail(id);
                WorkflowCard {
                    id: detail.id,
                    name: detail.name,
                    description: detail.description,
                    node_count: detail.node_count,
                    status: detail.status,
                    last_run: detail.last_executed,
                }
            }
        }
    };

    Ok(Html(card.render()?))
}

/// Delete workflow (HTMX)
pub async fn workflow_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    if !state.is_mock_data_enabled().await {
        // Try to delete via API, ignore errors for now
        let _ = state.api_client.delete_workflow(id).await;
    }

    // Return empty to remove the element from DOM
    Ok(Html(String::new()))
}

/// Workflow DAG preview SVG (HTMX)
pub async fn workflow_preview(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    // Build a representative mock DAG for preview using the SVG module
    let nodes = vec![
        crate::svg::SvgNode {
            id: "start_1".into(),
            label: "Start".into(),
            node_type: "start".into(),
            x: 0.0,
            y: 0.0,
        },
        crate::svg::SvgNode {
            id: "llm_1".into(),
            label: "LLM Process".into(),
            node_type: "llm".into(),
            x: 0.0,
            y: 0.0,
        },
        crate::svg::SvgNode {
            id: "code_1".into(),
            label: "Transform".into(),
            node_type: "code".into(),
            x: 0.0,
            y: 0.0,
        },
        crate::svg::SvgNode {
            id: "end_1".into(),
            label: "End".into(),
            node_type: "end".into(),
            x: 0.0,
            y: 0.0,
        },
    ];
    let edges = vec![
        crate::svg::SvgEdge {
            from_id: "start_1".into(),
            to_id: "llm_1".into(),
        },
        crate::svg::SvgEdge {
            from_id: "llm_1".into(),
            to_id: "code_1".into(),
        },
        crate::svg::SvgEdge {
            from_id: "code_1".into(),
            to_id: "end_1".into(),
        },
    ];
    let svg_content = crate::svg::workflow_to_svg(&nodes, &edges, 400, 120);

    let template = WorkflowPreview {
        workflow_id: id,
        svg_content,
    };

    Ok(Html(template.render()?))
}

/// Execution list partial (HTMX)
pub async fn execution_list_partial(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExecutionListQueryParams>,
) -> Result<Html<String>, UiError> {
    let page = query.page.unwrap_or(1);
    let api_query = ExecutionListQuery {
        workflow_id: query.workflow_id,
        status: query.status.clone(),
        page: Some(page),
        per_page: Some(20),
    };

    let response = if state.is_mock_data_enabled().await {
        mock::mock_execution_list(api_query)
    } else {
        state
            .api_client
            .list_executions(api_query.clone())
            .await
            .unwrap_or_else(|_| mock::mock_execution_list(api_query))
    };

    let executions: Vec<ExecutionRow> = response
        .items
        .into_iter()
        .map(|execution| ExecutionRow {
            id: execution.id,
            short_id: execution.id.to_string()[..8].to_string(),
            workflow_name: execution.workflow_name,
            status: execution.status.clone(),
            status_class: crate::api::execution_status_class(&execution.status).to_string(),
            started_at: execution.started_at,
            duration: execution.duration.unwrap_or("-".to_string()),
        })
        .collect();

    let template = ExecutionListPartial {
        executions,
        has_more: response.has_more,
        next_page: page + 1,
        status: query.status.unwrap_or_default(),
        workflow_id: query
            .workflow_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
    };

    Ok(Html(template.render()?))
}

/// Execution rows partial for infinite scroll
pub async fn execution_rows(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExecutionListQueryParams>,
) -> Result<Html<String>, UiError> {
    let page = query.page.unwrap_or(1);
    let api_query = ExecutionListQuery {
        workflow_id: query.workflow_id,
        status: query.status.clone(),
        page: Some(page),
        per_page: Some(20),
    };

    let response = if state.is_mock_data_enabled().await {
        mock::mock_execution_list(api_query)
    } else {
        state
            .api_client
            .list_executions(api_query.clone())
            .await
            .unwrap_or_else(|_| mock::mock_execution_list(api_query))
    };

    let executions: Vec<ExecutionRow> = response
        .items
        .into_iter()
        .map(|execution| ExecutionRow {
            id: execution.id,
            short_id: execution.id.to_string()[..8].to_string(),
            workflow_name: execution.workflow_name,
            status: execution.status.clone(),
            status_class: crate::api::execution_status_class(&execution.status).to_string(),
            started_at: execution.started_at,
            duration: execution.duration.unwrap_or("-".to_string()),
        })
        .collect();

    let template = ExecutionRowsPartial {
        executions,
        has_more: response.has_more,
        next_page: page + 1,
        status: query.status.unwrap_or_default(),
        workflow_id: query
            .workflow_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
    };

    Ok(Html(template.render()?))
}

#[derive(Debug, Deserialize)]
pub struct ExecutionListQueryParams {
    pub workflow_id: Option<Uuid>,
    pub status: Option<String>,
    pub page: Option<u32>,
}

/// Execution status partial (HTMX polling)
pub async fn execution_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    let data = if state.is_mock_data_enabled().await {
        mock::mock_execution_detail(id)
    } else {
        match state.api_client.get_execution_summary(id).await {
            Ok(summary) => mock::MockExecutionDetail {
                id: summary.id,
                workflow_id: summary.workflow_id,
                workflow_name: summary.workflow_name,
                status: summary.status,
                progress: summary.progress,
                started_at: summary.started_at,
                completed_at: summary.completed_at,
                duration: summary.duration,
                current_node: None,
                node_results: vec![],
                variables: serde_json::Value::Null,
            },
            Err(_) => mock::mock_execution_detail(id),
        }
    };

    let template = ExecutionStatus {
        execution_id: id,
        status: data.status,
        progress: data.progress,
        current_node: data.current_node,
        elapsed_time: data.duration.unwrap_or("--".to_string()),
    };

    Ok(Html(template.render()?))
}

/// Execution logs partial (HTMX)
pub async fn execution_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    let logs = if state.is_mock_data_enabled().await {
        vec![
            (
                "10:30:00".to_string(),
                "info".to_string(),
                "Workflow started".to_string(),
            ),
            (
                "10:30:01".to_string(),
                "info".to_string(),
                "Node 'start' completed".to_string(),
            ),
            (
                "10:30:02".to_string(),
                "info".to_string(),
                "Node 'llm_node_1' started".to_string(),
            ),
            (
                "10:30:15".to_string(),
                "debug".to_string(),
                "LLM response received (456 tokens)".to_string(),
            ),
            (
                "10:30:15".to_string(),
                "info".to_string(),
                "Node 'llm_node_1' completed".to_string(),
            ),
        ]
    } else {
        match state.api_client.get_execution_logs(id, Some(50)).await {
            Ok(entries) => entries
                .into_iter()
                .map(|e| (e.timestamp, e.level, e.message))
                .collect(),
            Err(_) => vec![
                (
                    "10:30:00".to_string(),
                    "info".to_string(),
                    "Workflow started".to_string(),
                ),
                (
                    "10:30:01".to_string(),
                    "info".to_string(),
                    "Node 'start' completed".to_string(),
                ),
            ],
        }
    };

    let template = ExecutionLogs { logs };

    Ok(Html(template.render()?))
}

/// Node configuration form (HTMX)
pub async fn node_form(
    State(_state): State<Arc<AppState>>,
    Path(node_type): Path<String>,
) -> Result<Html<String>, UiError> {
    let template = NodeFormPartial { node_type };

    Ok(Html(template.render()?))
}

/// Node validation request
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NodeValidateRequest {
    pub node_type: String,
    pub config: String,
}

/// Validate node configuration (HTMX)
pub async fn node_validate(
    State(_state): State<Arc<AppState>>,
    Form(form): Form<NodeValidateRequest>,
) -> Result<Html<String>, UiError> {
    let errors = validation::validate_node_config(&form.node_type, &form.config);

    if errors.is_empty() {
        Ok(Html(
            r#"<span class="text-green-500 flex items-center gap-1">
            <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20" aria-hidden="true">
              <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/>
            </svg>
            Valid configuration
        </span>"#
            .to_string(),
        ))
    } else {
        let error_items: String = errors
            .iter()
            .map(|e| {
                format!(
                    r#"<li class="text-red-500 text-sm">{}</li>"#,
                    validation::html_escape(e)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Html(format!(
            r#"<ul class="space-y-1 list-disc list-inside" role="alert" aria-label="Validation errors">{}</ul>"#,
            error_items
        )))
    }
}

/// Template gallery list partial (HTMX)
pub async fn template_list_partial(
    State(_state): State<Arc<AppState>>,
) -> Result<Html<String>, UiError> {
    let templates = mock::mock_templates();
    Ok(Html(TemplateListPartial { templates }.render()?))
}

/// Template detail partial for a single template (HTMX)
pub async fn template_detail_partial(
    State(_state): State<Arc<AppState>>,
    Path(template_id): Path<String>,
) -> Result<Html<String>, UiError> {
    let templates = mock::mock_templates();
    let template = templates
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| UiError::NotFound(format!("Template '{}' not found", template_id)))?;
    Ok(Html(TemplateDetailPartial { template }.render()?))
}

/// Toast notification (HTMX OOB)
pub async fn toast(Query(params): Query<ToastParams>) -> Result<Html<String>, UiError> {
    let template = ToastPartial {
        message: params
            .message
            .unwrap_or_else(|| "Action completed".to_string()),
        toast_type: params.toast_type.unwrap_or_else(|| "success".to_string()),
    };

    Ok(Html(template.render()?))
}

#[derive(Debug, Deserialize)]
pub struct ToastParams {
    pub message: Option<String>,
    #[serde(rename = "type")]
    pub toast_type: Option<String>,
}

/// Execution graph with node statuses (HTMX)
pub async fn execution_graph(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, UiError> {
    let execution_data = if state.is_mock_data_enabled().await {
        mock::mock_execution_detail(id)
    } else {
        match state.api_client.get_execution_summary(id).await {
            Ok(summary) => mock::MockExecutionDetail {
                id: summary.id,
                workflow_id: summary.workflow_id,
                workflow_name: summary.workflow_name,
                status: summary.status,
                progress: summary.progress,
                started_at: summary.started_at,
                completed_at: summary.completed_at,
                duration: summary.duration,
                current_node: None,
                node_results: vec![],
                variables: serde_json::Value::Null,
            },
            Err(_) => mock::mock_execution_detail(id),
        }
    };

    // Generate node statuses based on execution progress
    let node_statuses = generate_mock_node_statuses(&execution_data);

    // Return a div that initializes the graph viewer
    let html = format!(
        r#"<div id="execution-graph-viewer" class="w-full h-full min-h-[300px]"
             data-execution-id="{}"
             data-node-statuses='{}'>
        </div>
        <script>
            (function() {{
                const container = document.getElementById('execution-graph-viewer');
                if (!container || container.dataset.initialized) return;

                // Check if DAGEditor is available
                if (typeof DAGEditor === 'undefined') {{
                    container.innerHTML = '<div class="flex items-center justify-center h-full text-gray-500">Loading graph viewer...</div>';
                    return;
                }}

                container.dataset.initialized = 'true';

                // Initialize a read-only DAG viewer
                const viewer = new DAGEditor('execution-graph-viewer', {{
                    snapToGrid: false,
                    onNodeSelect: () => {{}},
                    onNodeDeselect: () => {{}},
                    onChange: () => {{}},
                }});

                // Load workflow data (mock for now)
                const mockData = {{
                    nodes: [
                        {{ id: 'start_1', type: 'start', name: 'Start', x: 50, y: 120, config: {{}} }},
                        {{ id: 'llm_1', type: 'llm', name: 'Process Input', x: 250, y: 60, config: {{}} }},
                        {{ id: 'llm_2', type: 'llm', name: 'Generate Response', x: 250, y: 180, config: {{}} }},
                        {{ id: 'code_1', type: 'code', name: 'Format Output', x: 450, y: 120, config: {{}} }},
                        {{ id: 'end_1', type: 'end', name: 'End', x: 650, y: 120, config: {{}} }}
                    ],
                    edges: [
                        {{ from: 'start_1', to: 'llm_1' }},
                        {{ from: 'start_1', to: 'llm_2' }},
                        {{ from: 'llm_1', to: 'code_1' }},
                        {{ from: 'llm_2', to: 'code_1' }},
                        {{ from: 'code_1', to: 'end_1' }}
                    ]
                }};

                viewer.setData(mockData);
                viewer.fitToContent();

                // Apply node statuses
                try {{
                    const statuses = JSON.parse(container.dataset.nodeStatuses || '{{}}');
                    if (Object.keys(statuses).length > 0) {{
                        viewer.setNodeStatuses(statuses);
                    }}
                }} catch (e) {{
                    console.error('Failed to parse node statuses:', e);
                }}
            }})();
        </script>"#,
        id,
        serde_json::to_string(&node_statuses).unwrap_or_else(|_| "{}".to_string())
    );

    Ok(Html(html))
}

/// Generate mock node statuses based on execution progress
fn generate_mock_node_statuses(
    execution: &mock::MockExecutionDetail,
) -> std::collections::HashMap<String, String> {
    let mut statuses = std::collections::HashMap::new();

    let progress = execution.progress;
    let exec_status = &execution.status;

    // Mock node IDs (should match the mock workflow)
    let node_ids = ["start_1", "llm_1", "llm_2", "code_1", "end_1"];

    for (i, node_id) in node_ids.iter().enumerate() {
        let threshold = (i as u32 + 1) * 20; // 20, 40, 60, 80, 100

        let status = if exec_status == "failed" && progress < threshold {
            if i == (progress as usize / 20) {
                "failed"
            } else if progress >= threshold - 20 {
                "completed"
            } else {
                "pending"
            }
        } else if progress >= threshold {
            "completed"
        } else if progress >= threshold - 20 {
            "running"
        } else {
            "pending"
        };

        statuses.insert(node_id.to_string(), status.to_string());
    }

    statuses
}

#[cfg(test)]
mod node_validate_tests {
    use crate::validation::{html_escape, validate_node_config};

    #[test]
    fn test_validate_llm_node_missing_required_fields() {
        let errors = validate_node_config("LLM", "{}");
        assert!(
            errors.iter().any(|e| e.contains("Model")),
            "Should require model"
        );
        assert!(
            errors.iter().any(|e| e.contains("Prompt")),
            "Should require prompt_template"
        );
    }

    #[test]
    fn test_validate_llm_node_valid() {
        let config = r#"{"model": "gpt-4", "prompt_template": "Hello {{input}}"}"#;
        let errors = validate_node_config("LLM", config);
        assert!(
            errors.is_empty(),
            "Valid LLM config should have no errors: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_llm_node_bad_temperature() {
        let config = r#"{"model": "gpt-4", "prompt_template": "Hi", "temperature": 3.5}"#;
        let errors = validate_node_config("LLM", config);
        assert!(errors.iter().any(|e| e.contains("Temperature")));
    }

    #[test]
    fn test_validate_loop_node_zero_iterations() {
        let config = r#"{"max_iterations": 0}"#;
        let errors = validate_node_config("Loop", config);
        assert!(errors.iter().any(|e| e.contains("greater than 0")));
    }

    #[test]
    fn test_validate_start_node_no_required_fields() {
        let errors = validate_node_config("Start", "{}");
        assert!(
            errors.is_empty(),
            "Start nodes should need no required fields"
        );
    }

    #[test]
    fn test_validate_invalid_json() {
        let errors = validate_node_config("LLM", "not json at all {{{");
        assert!(!errors.is_empty(), "Invalid JSON should produce an error");
    }

    #[test]
    fn test_html_escape_xss() {
        // Verify single-quote escaping is present (&#x27; form).
        let result = html_escape("<script>alert('xss')</script>");
        assert_eq!(
            result,
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }
}

#[cfg(test)]
mod mock_templates_tests {
    use crate::mock;

    #[test]
    fn test_mock_templates_returns_at_least_three() {
        let templates = mock::mock_templates();
        assert!(
            templates.len() >= 3,
            "Expected at least 3 templates, got {}",
            templates.len()
        );
    }

    #[test]
    fn test_mock_templates_all_have_non_empty_id_name_description() {
        for tmpl in mock::mock_templates() {
            assert!(!tmpl.id.is_empty(), "Template id must not be empty");
            assert!(!tmpl.name.is_empty(), "Template name must not be empty");
            assert!(
                !tmpl.description.is_empty(),
                "Template description must not be empty"
            );
        }
    }

    #[test]
    fn test_mock_templates_ids_are_unique() {
        let templates = mock::mock_templates();
        let mut seen = std::collections::HashSet::new();
        for tmpl in &templates {
            assert!(seen.insert(&tmpl.id), "Duplicate template id: {}", tmpl.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_to_svg_via_module() {
        let nodes = vec![
            crate::svg::SvgNode {
                id: "s".into(),
                label: "Start".into(),
                node_type: "start".into(),
                x: 0.0,
                y: 0.0,
            },
            crate::svg::SvgNode {
                id: "e".into(),
                label: "End".into(),
                node_type: "end".into(),
                x: 0.0,
                y: 0.0,
            },
        ];
        let edges = vec![crate::svg::SvgEdge {
            from_id: "s".into(),
            to_id: "e".into(),
        }];
        let svg = crate::svg::workflow_to_svg(&nodes, &edges, 300, 120);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("rect"));
        assert!(svg.contains("path"));
    }

    #[test]
    fn test_generate_mock_node_statuses() {
        let execution = mock::MockExecutionDetail {
            id: uuid::Uuid::new_v4(),
            workflow_id: uuid::Uuid::new_v4(),
            workflow_name: "Test".to_string(),
            status: "running".to_string(),
            progress: 50,
            started_at: "2026-01-30".to_string(),
            completed_at: None,
            duration: None,
            current_node: Some("llm_1".to_string()),
            node_results: vec![],
            variables: serde_json::json!({}),
        };

        let statuses = generate_mock_node_statuses(&execution);
        assert!(statuses.contains_key("start_1"));
        assert!(statuses.contains_key("llm_1"));
        assert!(statuses.contains_key("end_1"));
    }
}
