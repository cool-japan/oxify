//! Askama templates for the UI

use askama::Template;
use uuid::Uuid;

pub mod partials;

/// Base layout data shared by all pages
pub trait PageData {
    fn title(&self) -> &str;
    fn active_nav(&self) -> &str {
        ""
    }
}

// ============================================================================
// Dashboard
// ============================================================================

#[derive(Template)]
#[template(path = "pages/dashboard.html")]
pub struct DashboardTemplate {
    pub title: String,
    pub active_workflows: u32,
    pub running_executions: u32,
    pub completed_today: u32,
    pub failed_today: u32,
}

// ============================================================================
// Workflows
// ============================================================================

#[derive(Template)]
#[template(path = "pages/workflow_list.html")]
pub struct WorkflowListTemplate {
    pub title: String,
    pub workflows: Vec<WorkflowSummary>,
}

#[derive(Clone)]
pub struct WorkflowSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub node_count: u32,
    pub status: String,
}

#[derive(Template)]
#[template(path = "pages/workflow_new.html")]
pub struct WorkflowNewTemplate {
    pub title: String,
}

#[derive(Template)]
#[template(path = "pages/workflow_detail.html")]
pub struct WorkflowDetailTemplate {
    pub title: String,
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub workflow_description: Option<String>,
    pub node_count: u32,
    pub edge_count: u32,
    pub last_executed: Option<String>,
}

#[derive(Template)]
#[template(path = "pages/workflow_edit.html")]
pub struct WorkflowEditTemplate {
    pub title: String,
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub workflow_json: String,
}

// ============================================================================
// Executions
// ============================================================================

#[derive(Template)]
#[template(path = "pages/execution_list.html")]
pub struct ExecutionListTemplate {
    pub title: String,
    pub executions: Vec<ExecutionSummary>,
}

#[derive(Clone)]
pub struct ExecutionSummary {
    pub id: Uuid,
    pub workflow_name: String,
    pub status: String,
    pub started_at: String,
    pub duration: Option<String>,
}

#[derive(Template)]
#[template(path = "pages/execution_detail.html")]
pub struct ExecutionDetailTemplate {
    pub title: String,
    pub execution_id: Uuid,
    pub workflow_name: String,
    pub status: String,
    pub progress: u32,
    pub started_at: String,
}

#[derive(Template)]
#[template(path = "pages/execution_compare.html")]
pub struct ExecutionCompareTemplate {
    pub title: String,
    pub execution_ids: Vec<Uuid>,
    pub execution_count: usize,
    pub executions: Vec<ExecutionCompareData>,
    pub node_comparison: Vec<NodeComparisonRow>,
}

#[derive(Clone)]
pub struct ExecutionCompareData {
    pub id: Uuid,
    pub workflow_name: String,
    pub status: String,
    pub progress: u32,
    pub duration: String,
    pub started_at: String,
    pub completed_at: String,
}

#[derive(Clone)]
pub struct NodeComparisonRow {
    pub node_name: String,
    pub results: Vec<Option<NodeResultData>>,
}

#[derive(Clone)]
pub struct NodeResultData {
    pub status: String,
    pub duration: String,
}

// ============================================================================
// Templates Gallery
// ============================================================================

#[derive(Template)]
#[template(path = "pages/templates_page.html")]
pub struct TemplatesPageTemplate {
    pub title: String,
    pub templates: Vec<crate::mock::MockTemplateInfo>,
}

// ============================================================================
// Settings
// ============================================================================

#[derive(Template)]
#[template(path = "pages/settings.html")]
pub struct SettingsTemplate {
    pub title: String,
    pub dark_mode: bool,
    pub api_url: String,
}

// ============================================================================
// Error Pages
// ============================================================================

#[derive(Template)]
#[template(path = "pages/error_404.html")]
pub struct NotFoundPage;

#[derive(Template)]
#[template(path = "pages/error_500.html")]
pub struct InternalErrorPage {
    pub error_id: String,
    pub request_path: String,
}
