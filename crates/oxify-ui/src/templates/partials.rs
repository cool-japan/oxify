//! HTMX partial templates (HTML fragments)

use askama::Template;
use uuid::Uuid;

// ============================================================================
// Workflow Partials
// ============================================================================

#[derive(Template)]
#[template(path = "partials/workflow_list.html")]
pub struct WorkflowListPartial {
    pub workflows: Vec<WorkflowCard>,
    pub query: String,
    pub page: u32,
    pub has_more: bool,
}

#[derive(Template, Clone)]
#[template(path = "partials/workflow_card.html")]
pub struct WorkflowCard {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub node_count: u32,
    pub status: String,
    pub last_run: Option<String>,
}

#[derive(Template)]
#[template(path = "partials/workflow_preview.html")]
pub struct WorkflowPreview {
    pub workflow_id: Uuid,
    pub svg_content: String,
}

// ============================================================================
// Execution Partials
// ============================================================================

#[derive(Template)]
#[template(path = "partials/execution_status.html")]
pub struct ExecutionStatus {
    pub execution_id: Uuid,
    pub status: String,
    pub progress: u32,
    pub current_node: Option<String>,
    pub elapsed_time: String,
}

#[derive(Template)]
#[template(path = "partials/execution_logs.html")]
pub struct ExecutionLogs {
    pub logs: Vec<(String, String, String)>, // (timestamp, level, message)
}

#[derive(Template)]
#[template(path = "partials/execution_list.html")]
pub struct ExecutionListPartial {
    pub executions: Vec<ExecutionRow>,
    pub has_more: bool,
    pub next_page: u32,
    pub status: String,
    pub workflow_id: String,
}

#[derive(Template)]
#[template(path = "partials/execution_rows.html")]
pub struct ExecutionRowsPartial {
    pub executions: Vec<ExecutionRow>,
    pub has_more: bool,
    pub next_page: u32,
    pub status: String,
    pub workflow_id: String,
}

#[derive(Clone)]
pub struct ExecutionRow {
    pub id: Uuid,
    pub short_id: String,
    pub workflow_name: String,
    pub status: String,
    pub status_class: String,
    pub started_at: String,
    pub duration: String,
}

// ============================================================================
// Node Editor Partials
// ============================================================================

#[derive(Template)]
#[template(path = "partials/node_form.html")]
pub struct NodeFormPartial {
    pub node_type: String,
}

// ============================================================================
// UI Components
// ============================================================================

#[derive(Template)]
#[template(path = "partials/toast.html")]
pub struct ToastPartial {
    pub message: String,
    pub toast_type: String, // success, error, warning, info
}
