//! Mock data generators for development and fallback
//!
//! Provides mock data when the backend API is unavailable.

use crate::api::{
    DashboardStats, ExecutionListQuery, ExecutionSummary, PaginatedResponse, WorkflowListQuery,
    WorkflowSummary,
};
use uuid::Uuid;

/// Generate mock dashboard statistics
pub fn mock_dashboard_stats() -> DashboardStats {
    DashboardStats {
        active_workflows: 12,
        running_executions: 3,
        completed_today: 47,
        failed_today: 2,
        recent_workflows: vec![
            WorkflowSummary {
                id: Uuid::new_v4(),
                name: "RAG Pipeline".to_string(),
                description: Some("Retrieval augmented generation workflow".to_string()),
                node_count: 5,
                status: "active".to_string(),
                last_run: Some("2 hours ago".to_string()),
                created_at: "2026-01-15 09:00:00".to_string(),
                updated_at: "2026-01-17 10:00:00".to_string(),
            },
            WorkflowSummary {
                id: Uuid::new_v4(),
                name: "Data Processing".to_string(),
                description: Some("ETL workflow for data ingestion".to_string()),
                node_count: 8,
                status: "active".to_string(),
                last_run: Some("1 day ago".to_string()),
                created_at: "2026-01-10 14:30:00".to_string(),
                updated_at: "2026-01-16 08:15:00".to_string(),
            },
            WorkflowSummary {
                id: Uuid::new_v4(),
                name: "Chatbot Agent".to_string(),
                description: Some("Conversational AI agent workflow".to_string()),
                node_count: 12,
                status: "draft".to_string(),
                last_run: None,
                created_at: "2026-01-17 06:00:00".to_string(),
                updated_at: "2026-01-17 06:00:00".to_string(),
            },
        ],
        recent_executions: vec![
            ExecutionSummary {
                id: Uuid::new_v4(),
                workflow_id: Uuid::new_v4(),
                workflow_name: "RAG Pipeline".to_string(),
                status: "completed".to_string(),
                progress: 100,
                started_at: "2026-01-17 10:30:00".to_string(),
                completed_at: Some("2026-01-17 10:32:15".to_string()),
                duration: Some("2m 15s".to_string()),
            },
            ExecutionSummary {
                id: Uuid::new_v4(),
                workflow_id: Uuid::new_v4(),
                workflow_name: "Data Processing".to_string(),
                status: "running".to_string(),
                progress: 65,
                started_at: "2026-01-17 10:28:00".to_string(),
                completed_at: None,
                duration: Some("4m 30s".to_string()),
            },
            ExecutionSummary {
                id: Uuid::new_v4(),
                workflow_id: Uuid::new_v4(),
                workflow_name: "RAG Pipeline".to_string(),
                status: "failed".to_string(),
                progress: 45,
                started_at: "2026-01-17 09:15:00".to_string(),
                completed_at: Some("2026-01-17 09:17:30".to_string()),
                duration: Some("2m 30s".to_string()),
            },
        ],
    }
}

/// Generate mock workflow list
#[allow(dead_code)]
pub fn mock_workflow_list(query: WorkflowListQuery) -> PaginatedResponse<WorkflowSummary> {
    let all_workflows = vec![
        WorkflowSummary {
            id: Uuid::new_v4(),
            name: "RAG Pipeline".to_string(),
            description: Some("Retrieval augmented generation workflow".to_string()),
            node_count: 5,
            status: "active".to_string(),
            last_run: Some("2 hours ago".to_string()),
            created_at: "2026-01-15 09:00:00".to_string(),
            updated_at: "2026-01-17 10:00:00".to_string(),
        },
        WorkflowSummary {
            id: Uuid::new_v4(),
            name: "Data Processing".to_string(),
            description: Some("ETL workflow for data ingestion".to_string()),
            node_count: 8,
            status: "active".to_string(),
            last_run: Some("1 day ago".to_string()),
            created_at: "2026-01-10 14:30:00".to_string(),
            updated_at: "2026-01-16 08:15:00".to_string(),
        },
        WorkflowSummary {
            id: Uuid::new_v4(),
            name: "Chatbot Agent".to_string(),
            description: Some("Conversational AI agent workflow".to_string()),
            node_count: 12,
            status: "draft".to_string(),
            last_run: None,
            created_at: "2026-01-17 06:00:00".to_string(),
            updated_at: "2026-01-17 06:00:00".to_string(),
        },
        WorkflowSummary {
            id: Uuid::new_v4(),
            name: "Document Summarizer".to_string(),
            description: Some("Summarize long documents using LLM".to_string()),
            node_count: 4,
            status: "active".to_string(),
            last_run: Some("3 hours ago".to_string()),
            created_at: "2026-01-12 11:00:00".to_string(),
            updated_at: "2026-01-17 07:45:00".to_string(),
        },
        WorkflowSummary {
            id: Uuid::new_v4(),
            name: "Code Review Bot".to_string(),
            description: Some("Automated code review using Claude".to_string()),
            node_count: 6,
            status: "active".to_string(),
            last_run: Some("30 minutes ago".to_string()),
            created_at: "2026-01-14 16:20:00".to_string(),
            updated_at: "2026-01-17 10:15:00".to_string(),
        },
        WorkflowSummary {
            id: Uuid::new_v4(),
            name: "Email Classifier".to_string(),
            description: Some("Classify and route incoming emails".to_string()),
            node_count: 7,
            status: "archived".to_string(),
            last_run: Some("1 week ago".to_string()),
            created_at: "2026-01-01 08:00:00".to_string(),
            updated_at: "2026-01-10 12:00:00".to_string(),
        },
    ];

    // Apply search filter
    let filtered: Vec<WorkflowSummary> = if let Some(search) = &query.search {
        let search_lower = search.to_lowercase();
        all_workflows
            .into_iter()
            .filter(|w| {
                w.name.to_lowercase().contains(&search_lower)
                    || w.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&search_lower))
                        .unwrap_or(false)
            })
            .collect()
    } else {
        all_workflows
    };

    // Apply status filter
    let filtered: Vec<WorkflowSummary> = if let Some(status) = &query.status {
        filtered
            .into_iter()
            .filter(|w| &w.status == status)
            .collect()
    } else {
        filtered
    };

    let total = filtered.len() as u64;
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10);
    let start = ((page - 1) * per_page) as usize;
    let items: Vec<WorkflowSummary> = filtered
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .collect();
    let has_more = start + items.len() < total as usize;

    PaginatedResponse {
        items,
        total,
        page,
        per_page,
        has_more,
    }
}

/// Generate mock execution list
#[allow(dead_code)]
pub fn mock_execution_list(query: ExecutionListQuery) -> PaginatedResponse<ExecutionSummary> {
    let all_executions = vec![
        ExecutionSummary {
            id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            workflow_name: "RAG Pipeline".to_string(),
            status: "completed".to_string(),
            progress: 100,
            started_at: "2026-01-17 10:30:00".to_string(),
            completed_at: Some("2026-01-17 10:32:15".to_string()),
            duration: Some("2m 15s".to_string()),
        },
        ExecutionSummary {
            id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            workflow_name: "Data Processing".to_string(),
            status: "running".to_string(),
            progress: 65,
            started_at: "2026-01-17 10:28:00".to_string(),
            completed_at: None,
            duration: Some("4m 30s".to_string()),
        },
        ExecutionSummary {
            id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            workflow_name: "RAG Pipeline".to_string(),
            status: "failed".to_string(),
            progress: 45,
            started_at: "2026-01-17 09:15:00".to_string(),
            completed_at: Some("2026-01-17 09:17:30".to_string()),
            duration: Some("2m 30s".to_string()),
        },
        ExecutionSummary {
            id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            workflow_name: "Document Summarizer".to_string(),
            status: "completed".to_string(),
            progress: 100,
            started_at: "2026-01-17 08:00:00".to_string(),
            completed_at: Some("2026-01-17 08:05:30".to_string()),
            duration: Some("5m 30s".to_string()),
        },
        ExecutionSummary {
            id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            workflow_name: "Code Review Bot".to_string(),
            status: "paused".to_string(),
            progress: 30,
            started_at: "2026-01-17 07:45:00".to_string(),
            completed_at: None,
            duration: Some("15m".to_string()),
        },
        ExecutionSummary {
            id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            workflow_name: "RAG Pipeline".to_string(),
            status: "cancelled".to_string(),
            progress: 10,
            started_at: "2026-01-17 06:00:00".to_string(),
            completed_at: Some("2026-01-17 06:01:00".to_string()),
            duration: Some("1m".to_string()),
        },
    ];

    // Apply workflow_id filter
    let filtered: Vec<ExecutionSummary> = if let Some(workflow_id) = query.workflow_id {
        all_executions
            .into_iter()
            .filter(|e| e.workflow_id == workflow_id)
            .collect()
    } else {
        all_executions
    };

    // Apply status filter
    let filtered: Vec<ExecutionSummary> = if let Some(status) = &query.status {
        filtered
            .into_iter()
            .filter(|e| &e.status == status)
            .collect()
    } else {
        filtered
    };

    let total = filtered.len() as u64;
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10);
    let start = ((page - 1) * per_page) as usize;
    let items: Vec<ExecutionSummary> = filtered
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .collect();
    let has_more = start + items.len() < total as usize;

    PaginatedResponse {
        items,
        total,
        page,
        per_page,
        has_more,
    }
}

/// Generate mock workflow detail
pub fn mock_workflow_detail(id: Uuid) -> MockWorkflowDetail {
    MockWorkflowDetail {
        id,
        name: "RAG Pipeline".to_string(),
        description: Some("Retrieval augmented generation workflow for Q&A".to_string()),
        version: "1.2.0".to_string(),
        status: "active".to_string(),
        node_count: 5,
        edge_count: 4,
        tags: vec!["rag".to_string(), "qa".to_string(), "llm".to_string()],
        created_at: "2026-01-15 09:00:00".to_string(),
        updated_at: "2026-01-17 10:00:00".to_string(),
        last_executed: Some("2026-01-17 10:30:00".to_string()),
        nodes_json: serde_json::json!([
            {"id": "start", "type": "start", "x": 50, "y": 200},
            {"id": "retriever", "type": "retriever", "x": 200, "y": 200},
            {"id": "llm1", "type": "llm", "x": 350, "y": 150},
            {"id": "llm2", "type": "llm", "x": 350, "y": 250},
            {"id": "end", "type": "end", "x": 500, "y": 200}
        ]),
        edges_json: serde_json::json!([
            {"from": "start", "to": "retriever"},
            {"from": "retriever", "to": "llm1"},
            {"from": "retriever", "to": "llm2"},
            {"from": "llm1", "to": "end"},
            {"from": "llm2", "to": "end"}
        ]),
    }
}

/// Mock workflow detail data
pub struct MockWorkflowDetail {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub status: String,
    pub node_count: u32,
    pub edge_count: u32,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_executed: Option<String>,
    pub nodes_json: serde_json::Value,
    pub edges_json: serde_json::Value,
}

/// Generate mock execution detail
pub fn mock_execution_detail(id: Uuid) -> MockExecutionDetail {
    MockExecutionDetail {
        id,
        workflow_id: Uuid::new_v4(),
        workflow_name: "RAG Pipeline".to_string(),
        status: "running".to_string(),
        progress: 65,
        started_at: "2026-01-17 10:30:00".to_string(),
        completed_at: None,
        duration: Some("2m 30s".to_string()),
        current_node: Some("llm_node_1".to_string()),
        node_results: vec![
            ("start".to_string(), "completed".to_string(), 100),
            ("retriever".to_string(), "completed".to_string(), 100),
            ("llm_node_1".to_string(), "running".to_string(), 50),
            ("llm_node_2".to_string(), "pending".to_string(), 0),
            ("end".to_string(), "pending".to_string(), 0),
        ],
        variables: serde_json::json!({
            "query": "What is machine learning?",
            "retrieved_docs": ["doc1.txt", "doc2.txt", "doc3.txt"]
        }),
    }
}

/// Mock execution detail data
pub struct MockExecutionDetail {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub status: String,
    pub progress: u32,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration: Option<String>,
    pub current_node: Option<String>,
    pub node_results: Vec<(String, String, u32)>, // (node_name, status, progress)
    pub variables: serde_json::Value,
}
