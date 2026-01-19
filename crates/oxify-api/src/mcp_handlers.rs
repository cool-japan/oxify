//! MCP management API handlers

use crate::handlers::AppState;
use crate::mcp_types::*;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

pub async fn register_mcp_server(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<RegisterMcpServerRequest>,
) -> Response {
    info!("Registering MCP server: {}", req.server_id);

    let transport_type = req.server_type.to_lowercase();
    match transport_type.as_str() {
        "stdio" | "http" => (
            StatusCode::NOT_IMPLEMENTED,
            Json(ErrorResponse {
                error: "Dynamic server registration not yet implemented in API. Use built-in servers instead.".to_string(),
                details: None,
            }),
        )
            .into_response(),
        _ => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Unknown server type: {}", req.server_type),
                details: None,
            }),
        )
            .into_response(),
    }
}

pub async fn unregister_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UnregisterMcpServerRequest>,
) -> Response {
    info!("Unregistering MCP server: {}", req.server_id);

    let registry = state.mcp_registry.write().await;
    let _ = registry.unregister(&req.server_id).await;

    (
        StatusCode::OK,
        Json(UnregisterMcpServerResponse {
            server_id: req.server_id,
            status: "unregistered".to_string(),
        }),
    )
        .into_response()
}

pub async fn list_mcp_servers(State(state): State<Arc<AppState>>) -> Response {
    info!("Listing MCP servers");

    let registry = state.mcp_registry.read().await;
    let all_metrics = registry.get_all_server_metrics().await;

    let server_infos: Vec<McpServerInfo> = all_metrics
        .into_iter()
        .map(|metrics| McpServerInfo {
            server_id: metrics.server_id,
            transport_type: "local".to_string(),
            health: format!("{:?}", metrics.health),
            weight: metrics.weight,
            tags: metrics.tags,
            metrics: ServerMetricsInfo {
                request_count: metrics.request_count,
                error_count: metrics.error_count,
                avg_response_time_ms: metrics.avg_response_time_ms as f64,
            },
        })
        .collect();

    (
        StatusCode::OK,
        Json(ListMcpServersResponse {
            servers: server_infos,
        }),
    )
        .into_response()
}

pub async fn list_mcp_tools(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ListMcpToolsRequest>,
) -> Response {
    info!("Listing MCP tools");

    let registry = state.mcp_registry.write().await;

    let tools = if let Some(server_id) = req.server_id {
        match registry.list_tools(&server_id).await {
            Ok(t) => vec![(server_id.clone(), t)],
            Err(e) => {
                error!("Failed to list tools for server {}: {}", server_id, e);
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: format!("Server not found: {}", server_id),
                        details: Some(e.to_string()),
                    }),
                )
                    .into_response();
            }
        }
    } else {
        match registry.list_all_tools().await {
            Ok(t) => t.into_iter().collect(),
            Err(e) => {
                error!("Failed to list all tools: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Failed to list tools".to_string(),
                        details: Some(e.to_string()),
                    }),
                )
                    .into_response();
            }
        }
    };

    let tool_infos: Vec<McpToolInfo> = tools
        .into_iter()
        .flat_map(|(server_id, schemas)| {
            schemas.into_iter().map(move |schema| McpToolInfo {
                server_id: server_id.clone(),
                tool_name: schema.name,
                description: schema.description,
                input_schema: schema.input_schema,
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(ListMcpToolsResponse { tools: tool_infos }),
    )
        .into_response()
}

pub async fn invoke_mcp_tool(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InvokeMcpToolRequest>,
) -> Response {
    info!(
        "Invoking MCP tool {} on server {}",
        req.tool_name, req.server_id
    );

    let start_time = std::time::Instant::now();
    let registry = state.mcp_registry.write().await;

    let result = registry
        .invoke_tool(&req.server_id, &req.tool_name, req.parameters)
        .await;

    let execution_time = start_time.elapsed().as_millis() as u64;

    match result {
        Ok(res) => (
            StatusCode::OK,
            Json(InvokeMcpToolResponse {
                execution_id: Uuid::new_v4(),
                result: res,
                execution_time_ms: execution_time,
            }),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to invoke tool: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Tool invocation failed".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response()
        }
    }
}

pub async fn get_registry_stats(State(state): State<Arc<AppState>>) -> Response {
    info!("Getting MCP registry stats");

    let registry = state.mcp_registry.read().await;
    let stats = registry.get_stats().await;
    let health_status = registry.get_server_health_status().await;

    // Count servers by health status
    let mut servers_by_health = std::collections::HashMap::new();
    for health in health_status.values() {
        let health_str = format!("{:?}", health);
        *servers_by_health.entry(health_str).or_insert(0) += 1;
    }

    (
        StatusCode::OK,
        Json(McpRegistryStatsResponse {
            total_servers: stats.server_count,
            total_tools: stats.total_tools,
            servers_by_health,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_mcp::{FilesystemServer, McpRegistry};
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_list_mcp_servers_empty() {
        let app_state = Arc::new(AppState::new());
        let state = State(app_state);

        let response = list_mcp_servers(state).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_mcp_tools_empty() {
        let app_state = Arc::new(AppState::new());
        let state = State(app_state);

        let req = ListMcpToolsRequest { server_id: None };
        let response = list_mcp_tools(state, Json(req)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_registry_stats() {
        let app_state = Arc::new(AppState::new());
        let state = State(app_state);

        let response = get_registry_stats(state).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_unregister_nonexistent_server() {
        let app_state = Arc::new(AppState::new());
        let state = State(app_state);

        let req = UnregisterMcpServerRequest {
            server_id: "nonexistent".to_string(),
        };
        let response = unregister_mcp_server(state, Json(req)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_tools_with_filesystem_server() {
        use std::path::PathBuf;

        let registry = McpRegistry::new();
        let fs_server = FilesystemServer::new(PathBuf::from("/tmp"));

        registry
            .register("fs_test".to_string(), fs_server)
            .await
            .unwrap();

        let app_state = AppState {
            mcp_registry: Arc::new(RwLock::new(registry)),
            ..AppState::new()
        };
        let state = State(Arc::new(app_state));

        let req = ListMcpToolsRequest {
            server_id: Some("fs_test".to_string()),
        };
        let response = list_mcp_tools(state, Json(req)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_invoke_tool_server_not_found() {
        let app_state = Arc::new(AppState::new());
        let state = State(app_state);

        let req = InvokeMcpToolRequest {
            server_id: "nonexistent".to_string(),
            tool_name: "test".to_string(),
            parameters: serde_json::json!({}),
        };
        let response = invoke_mcp_tool(state, Json(req)).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_register_stdio_server_not_implemented() {
        let app_state = Arc::new(AppState::new());
        let state = State(app_state);

        let req = RegisterMcpServerRequest {
            server_id: "test_server".to_string(),
            server_type: "stdio".to_string(),
            endpoint: None,
            command: Some(vec!["test".to_string()]),
            weight: None,
            tags: None,
        };
        let response = register_mcp_server(state, Json(req)).await;
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_register_http_server_not_implemented() {
        let app_state = Arc::new(AppState::new());
        let state = State(app_state);

        let req = RegisterMcpServerRequest {
            server_id: "test_server".to_string(),
            server_type: "http".to_string(),
            endpoint: Some("http://localhost:8080".to_string()),
            command: None,
            weight: None,
            tags: None,
        };
        let response = register_mcp_server(state, Json(req)).await;
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_register_unknown_server_type() {
        let app_state = Arc::new(AppState::new());
        let state = State(app_state);

        let req = RegisterMcpServerRequest {
            server_id: "test_server".to_string(),
            server_type: "unknown".to_string(),
            endpoint: None,
            command: None,
            weight: None,
            tags: None,
        };
        let response = register_mcp_server(state, Json(req)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
