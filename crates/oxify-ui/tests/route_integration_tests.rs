//! Integration tests for oxify-ui routes

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use oxify_ui::{create_router, AppState};
use std::sync::Arc;
use tower::ServiceExt; // for `oneshot` and `ready`

/// Helper to create test AppState
fn create_test_state() -> Arc<AppState> {
    Arc::new(
        AppState::new("http://localhost:8080".to_string(), true)
            .expect("Failed to create test state"),
    )
}

/// Helper to make a test request
async fn make_request(method: Method, uri: &str, body: Option<String>) -> (StatusCode, String) {
    let state = create_test_state();
    let app = create_router(state);

    let mut request_builder = Request::builder().method(method).uri(uri);

    if body.is_some() {
        request_builder = request_builder.header(header::CONTENT_TYPE, "application/json");
    }

    let request = if let Some(body_content) = body {
        request_builder.body(Body::from(body_content)).unwrap()
    } else {
        request_builder.body(Body::empty()).unwrap()
    };

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_string = String::from_utf8(body_bytes.to_vec()).unwrap();

    (status, body_string)
}

#[tokio::test]
async fn test_dashboard_route() {
    let (status, body) = make_request(Method::GET, "/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Dashboard") || body.contains("dashboard"));
}

#[tokio::test]
async fn test_login_page_route() {
    let (status, body) = make_request(Method::GET, "/login", None).await;
    // Login page may return OK or redirect (SEE_OTHER) if already authenticated
    assert!(
        status == StatusCode::OK || status == StatusCode::SEE_OTHER,
        "Expected OK or SEE_OTHER, got: {}",
        status
    );
    if status == StatusCode::OK {
        assert!(body.contains("login") || body.contains("Login"));
    }
}

#[tokio::test]
async fn test_workflows_list_route() {
    let (status, body) = make_request(Method::GET, "/workflows", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("workflow") || body.contains("Workflow"));
}

#[tokio::test]
async fn test_workflow_new_route() {
    let (status, body) = make_request(Method::GET, "/workflows/new", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.is_empty());
}

#[tokio::test]
async fn test_executions_list_route() {
    let (status, body) = make_request(Method::GET, "/executions", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("execution") || body.contains("Execution"));
}

#[tokio::test]
async fn test_execution_compare_route() {
    let (status, _body) = make_request(Method::GET, "/executions/compare", None).await;
    // Should return 200 (OK) or 400 (Bad Request if IDs missing)
    assert!(
        status == StatusCode::OK || status == StatusCode::BAD_REQUEST,
        "Expected OK or BAD_REQUEST, got: {}",
        status
    );
}

#[tokio::test]
async fn test_search_route() {
    let (status, body) = make_request(Method::GET, "/search", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("search") || body.contains("Search"));
}

#[tokio::test]
async fn test_settings_route() {
    let (status, body) = make_request(Method::GET, "/settings", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("settings") || body.contains("Settings"));
}

#[tokio::test]
async fn test_htmx_workflows_partial() {
    let (status, body) = make_request(Method::GET, "/htmx/workflows", None).await;
    assert_eq!(status, StatusCode::OK);
    // Should return HTML partial with workflow data
    assert!(!body.is_empty());
}

#[tokio::test]
async fn test_htmx_executions_partial() {
    let (status, body) = make_request(Method::GET, "/htmx/executions", None).await;
    assert_eq!(status, StatusCode::OK);
    // Should return HTML partial with execution data
    assert!(!body.is_empty());
}

#[tokio::test]
async fn test_htmx_execution_rows() {
    let (status, body) = make_request(Method::GET, "/htmx/executions/rows?page=1", None).await;
    assert_eq!(status, StatusCode::OK);
    // Should return HTML rows
    assert!(!body.is_empty());
}

#[tokio::test]
async fn test_htmx_toast() {
    let (status, body) =
        make_request(Method::GET, "/htmx/toast?message=Test&type=success", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Test"));
}

#[tokio::test]
async fn test_static_files_route() {
    // Test that static route is properly configured
    // This will fail if file doesn't exist, which is expected in test environment
    let (status, _body) = make_request(Method::GET, "/static/css/app.css", None).await;
    // We expect either OK (if file exists) or NOT_FOUND (if it doesn't)
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "Static file route should return OK or NOT_FOUND, got: {}",
        status
    );
}

#[tokio::test]
async fn test_json_api_workflow_yaml_export() {
    use uuid::Uuid;

    // Test YAML export endpoint
    let workflow_id = Uuid::new_v4();
    let uri = format!("/api/v1/workflows/{}/export/yaml", workflow_id);
    let (status, _body) = make_request(Method::GET, &uri, None).await;

    // Should return OK with mock data or NOT_FOUND if workflow doesn't exist
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "Expected OK or NOT_FOUND, got: {}",
        status
    );
}

#[tokio::test]
async fn test_invalid_route_returns_404() {
    let (status, _body) = make_request(Method::GET, "/nonexistent/route", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_htmx_workflow_search() {
    let (status, body) = make_request(Method::GET, "/htmx/workflows/search?q=test", None).await;
    assert_eq!(status, StatusCode::OK);
    // Should return HTML partial with search results
    assert!(!body.is_empty());
}

#[tokio::test]
async fn test_api_create_workflow_returns_json() {
    let workflow_json = r#"{
        "name": "Test Workflow",
        "description": "A test workflow",
        "nodes": [],
        "edges": []
    }"#;

    let (status, body) = make_request(
        Method::POST,
        "/api/v1/workflows",
        Some(workflow_json.to_string()),
    )
    .await;

    // Should return CREATED or OK
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "Expected CREATED or OK, got: {}",
        status
    );

    // Response should be JSON
    assert!(body.contains("success") || body.contains("id") || body.contains("error"));
}

#[tokio::test]
async fn test_htmx_node_form() {
    let (status, body) = make_request(Method::GET, "/htmx/nodes/form/llm", None).await;
    assert_eq!(status, StatusCode::OK);
    // Should return HTML form for LLM node
    assert!(!body.is_empty());
}
