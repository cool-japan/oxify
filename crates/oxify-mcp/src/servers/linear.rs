//! Linear MCP server - provides Linear project management operations via GraphQL

use crate::{McpError, McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

/// Configuration for the Linear MCP server
#[derive(Debug)]
pub struct LinearConfig {
    /// Linear API key used for authentication
    pub api_key: String,
    /// Base URL for the Linear GraphQL endpoint
    pub base_url: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// User-Agent header value sent to Linear
    pub user_agent: String,
}

impl LinearConfig {
    /// Construct configuration from environment variables.
    ///
    /// Required env vars:
    /// * `LINEAR_API_KEY` — Linear personal API key
    ///
    /// Optional env vars:
    /// * `LINEAR_BASE_URL` — GraphQL endpoint (default: `https://api.linear.app/graphql`)
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("LINEAR_API_KEY")
            .map_err(|_| McpError::InvalidRequest("LINEAR_API_KEY not set".to_string()))?;
        let base_url = std::env::var("LINEAR_BASE_URL")
            .unwrap_or_else(|_| "https://api.linear.app/graphql".to_string());
        Ok(Self {
            api_key,
            base_url,
            timeout_secs: 30,
            user_agent: "oxify-mcp/0.2".to_string(),
        })
    }
}

impl Default for LinearConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.linear.app/graphql".to_string(),
            timeout_secs: 30,
            user_agent: "oxify-mcp/0.2".to_string(),
        }
    }
}

/// MCP server backed by the Linear GraphQL API
pub struct LinearServer {
    client: oxihttp::HttpsClient,
    cfg: LinearConfig,
}

impl LinearServer {
    /// Create a new Linear server using the supplied configuration.
    pub fn new(cfg: LinearConfig) -> Result<Self> {
        let mut headers = oxihttp::HeaderMap::new();
        if !cfg.api_key.is_empty() {
            headers.insert(
                oxihttp::HeaderName::from_static("authorization"),
                oxihttp::HeaderValue::from_str(&cfg.api_key)
                    .map_err(|e| McpError::InvalidRequest(e.to_string()))?,
            );
        }
        let client = oxihttp::Client::builder()
            .user_agent(&cfg.user_agent)
            .connect_timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .read_timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .default_headers(headers)
            .with_tls()
            .build_https()
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;
        Ok(Self { client, cfg })
    }

    /// Execute a GraphQL query/mutation against the Linear API.
    ///
    /// Linear returns HTTP 200 even on logical errors; those are surfaced via the
    /// `errors` field in the response body and must be checked explicitly.
    async fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({ "query": query, "variables": variables });
        let response = self
            .client
            .post(&self.cfg.base_url)
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .json(&body)
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response
                .body_text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(McpError::ToolExecutionError(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        let result: Value = response
            .body_json()
            .await
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

        // GraphQL can return HTTP 200 with logical errors — surface them explicitly
        if let Some(errors) = result.get("errors") {
            if let Some(error_array) = errors.as_array() {
                if !error_array.is_empty() {
                    let msg = error_array
                        .iter()
                        .filter_map(|e| e["message"].as_str())
                        .collect::<Vec<_>>()
                        .join("; ");
                    return Err(McpError::ToolExecutionError(format!(
                        "Linear GraphQL error: {}",
                        msg
                    )));
                }
            }
        }

        Ok(result["data"].clone())
    }
}

#[async_trait]
impl McpServer for LinearServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            // ── 1. list_teams ─────────────────────────────────────────────────
            "list_teams" => {
                let q = "query { teams { nodes { id name key description } } }";
                let data = self.graphql(q, json!({})).await?;
                Ok(json!({ "teams": data["teams"]["nodes"] }))
            }

            // ── 2. list_issues ────────────────────────────────────────────────
            "list_issues" => {
                let limit = arguments["limit"].as_u64().unwrap_or(20);
                let q = "query ListIssues($first: Int!) { issues(first: $first) { nodes { id title description state { name type } assignee { id name } priority createdAt updatedAt } } }";
                let vars = json!({ "first": limit });
                let data = self.graphql(q, vars).await?;
                Ok(json!({ "issues": data["issues"]["nodes"] }))
            }

            // ── 3. get_issue ──────────────────────────────────────────────────
            "get_issue" => {
                let id = arguments["id"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'id'".to_string()))?;
                let q = "query GetIssue($id: String!) { issue(id: $id) { id title description state { name } assignee { id name } priority createdAt updatedAt comments { nodes { id body } } } }";
                let vars = json!({ "id": id });
                let data = self.graphql(q, vars).await?;
                Ok(json!({ "issue": data["issue"] }))
            }

            // ── 4. create_issue ───────────────────────────────────────────────
            "create_issue" => {
                let team_id = arguments["team_id"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'team_id'".to_string()))?;
                let title = arguments["title"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'title'".to_string()))?;

                let mut vars = json!({
                    "teamId": team_id,
                    "title": title,
                });
                if let Some(description) = arguments["description"].as_str() {
                    vars["description"] = json!(description);
                }
                if let Some(priority) = arguments["priority"].as_i64() {
                    vars["priority"] = json!(priority);
                }

                let q = "mutation CreateIssue($teamId: String!, $title: String!, $description: String, $priority: Int) { issueCreate(input: { teamId: $teamId, title: $title, description: $description, priority: $priority }) { success issue { id title } } }";
                let data = self.graphql(q, vars).await?;
                Ok(json!({ "result": data["issueCreate"] }))
            }

            // ── 5. update_issue ───────────────────────────────────────────────
            "update_issue" => {
                let id = arguments["id"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'id'".to_string()))?;

                let mut vars = json!({ "id": id });
                if let Some(title) = arguments["title"].as_str() {
                    vars["title"] = json!(title);
                }
                if let Some(state_id) = arguments["state_id"].as_str() {
                    vars["stateId"] = json!(state_id);
                }
                if let Some(assignee_id) = arguments["assignee_id"].as_str() {
                    vars["assigneeId"] = json!(assignee_id);
                }
                if let Some(priority) = arguments["priority"].as_i64() {
                    vars["priority"] = json!(priority);
                }

                let q = "mutation UpdateIssue($id: String!, $title: String, $stateId: String, $assigneeId: String, $priority: Int) { issueUpdate(id: $id, input: { title: $title, stateId: $stateId, assigneeId: $assigneeId, priority: $priority }) { success issue { id title } } }";
                let data = self.graphql(q, vars).await?;
                Ok(json!({ "result": data["issueUpdate"] }))
            }

            // ── 6. list_projects ──────────────────────────────────────────────
            "list_projects" => {
                let q = "query { projects { nodes { id name description state } } }";
                let data = self.graphql(q, json!({})).await?;
                Ok(json!({ "projects": data["projects"]["nodes"] }))
            }

            // ── 7. create_comment ─────────────────────────────────────────────
            "create_comment" => {
                let issue_id = arguments["issue_id"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'issue_id'".to_string()))?;
                let body = arguments["body"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'body'".to_string()))?;

                let vars = json!({ "issueId": issue_id, "body": body });
                let q = "mutation CreateComment($issueId: String!, $body: String!) { commentCreate(input: { issueId: $issueId, body: $body }) { success comment { id body } } }";
                let data = self.graphql(q, vars).await?;
                Ok(json!({ "result": data["commentCreate"] }))
            }

            // ── 8. search_issues ──────────────────────────────────────────────
            "search_issues" => {
                let query = arguments["query"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'query'".to_string()))?;
                let limit = arguments["limit"].as_u64().unwrap_or(20);

                let vars = json!({ "query": query, "first": limit });
                let q = "query SearchIssues($query: String!, $first: Int!) { issueSearch(query: $query, first: $first) { nodes { id title state { name } } } }";
                let data = self.graphql(q, vars).await?;
                Ok(json!({ "issues": data["issueSearch"]["nodes"] }))
            }

            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "list_teams",
                "description": "List all teams in the Linear workspace",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            json!({
                "name": "list_issues",
                "description": "List issues with optional filters",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "team_id": {
                            "type": "string",
                            "description": "Optional team ID to filter issues by"
                        },
                        "state": {
                            "type": "string",
                            "enum": ["triage", "todo", "in_progress", "done", "cancelled"],
                            "description": "Optional issue state filter"
                        },
                        "assignee_id": {
                            "type": "string",
                            "description": "Optional assignee user ID to filter issues by"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of issues to return (default: 20)"
                        }
                    }
                }
            }),
            json!({
                "name": "get_issue",
                "description": "Get a specific issue by ID",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The unique identifier of the issue"
                        }
                    },
                    "required": ["id"]
                }
            }),
            json!({
                "name": "create_issue",
                "description": "Create a new issue",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "team_id": {
                            "type": "string",
                            "description": "The ID of the team this issue belongs to"
                        },
                        "title": {
                            "type": "string",
                            "description": "Issue title"
                        },
                        "description": {
                            "type": "string",
                            "description": "Issue description (markdown supported)"
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 4,
                            "description": "Priority level: 0=no priority, 1=urgent, 2=high, 3=medium, 4=low"
                        }
                    },
                    "required": ["team_id", "title"]
                }
            }),
            json!({
                "name": "update_issue",
                "description": "Update an existing issue",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The unique identifier of the issue to update"
                        },
                        "title": {
                            "type": "string",
                            "description": "New title for the issue"
                        },
                        "state_id": {
                            "type": "string",
                            "description": "New workflow state ID"
                        },
                        "assignee_id": {
                            "type": "string",
                            "description": "New assignee user ID"
                        },
                        "priority": {
                            "type": "integer",
                            "description": "New priority level (0-4)"
                        }
                    },
                    "required": ["id"]
                }
            }),
            json!({
                "name": "list_projects",
                "description": "List all projects in the workspace",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            json!({
                "name": "create_comment",
                "description": "Add a comment to an issue",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "issue_id": {
                            "type": "string",
                            "description": "The ID of the issue to comment on"
                        },
                        "body": {
                            "type": "string",
                            "description": "Comment body text (markdown supported)"
                        }
                    },
                    "required": ["issue_id", "body"]
                }
            }),
            json!({
                "name": "search_issues",
                "description": "Search for issues by text query",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Full-text search query"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return (default: 20)"
                        }
                    },
                    "required": ["query"]
                }
            }),
        ])
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_has_empty_key() {
        let cfg = LinearConfig::default();
        assert!(cfg.api_key.is_empty());
        assert_eq!(cfg.base_url, "https://api.linear.app/graphql");
        assert_eq!(cfg.timeout_secs, 30);
    }

    #[test]
    fn test_config_from_env_missing_key_errors() {
        std::env::remove_var("LINEAR_API_KEY");
        let result = LinearConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("LINEAR_API_KEY"));
    }

    #[tokio::test]
    async fn test_list_tools_returns_eight() {
        let server = LinearServer::new(LinearConfig::default()).unwrap();
        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 8);
    }

    #[tokio::test]
    async fn test_list_tools_all_names_unique() {
        let server = LinearServer::new(LinearConfig::default()).unwrap();
        let tools = server.list_tools().await.unwrap();
        let names: std::collections::HashSet<&str> =
            tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names.len(), 8, "all tool names must be unique");
    }

    #[tokio::test]
    async fn test_call_unknown_tool_returns_not_found() {
        let server = LinearServer::new(LinearConfig::default()).unwrap();
        let result = server.call_tool("nonexistent_tool", json!({})).await;
        assert!(result.is_err());
        matches!(result.unwrap_err(), McpError::ToolNotFound(_));
    }

    #[tokio::test]
    async fn test_get_issue_missing_id_returns_invalid_request() {
        let cfg = LinearConfig {
            api_key: "fake_key".to_string(),
            ..Default::default()
        };
        let server = LinearServer::new(cfg).unwrap();
        let result = server.call_tool("get_issue", json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::InvalidRequest(_) => {}
            other => panic!("Expected InvalidRequest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_create_issue_missing_title_returns_invalid_request() {
        let cfg = LinearConfig {
            api_key: "fake_key".to_string(),
            ..Default::default()
        };
        let server = LinearServer::new(cfg).unwrap();
        let result = server
            .call_tool("create_issue", json!({"team_id": "TEAM-1"}))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::InvalidRequest(_) => {}
            other => panic!("Expected InvalidRequest, got {:?}", other),
        }
    }

    #[test]
    fn test_all_tools_have_description() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tools = rt.block_on(async {
            let server = LinearServer::new(LinearConfig::default()).unwrap();
            server.list_tools().await.unwrap()
        });
        for tool in &tools {
            assert!(
                tool["description"]
                    .as_str()
                    .map(|d| !d.is_empty())
                    .unwrap_or(false),
                "tool {} has no description",
                tool["name"].as_str().unwrap_or("?")
            );
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_list_teams_live() {
        let cfg = LinearConfig::from_env().expect("LINEAR_API_KEY must be set");
        let server = LinearServer::new(cfg).unwrap();
        let result = server.call_tool("list_teams", json!({})).await;
        assert!(result.is_ok(), "live test failed: {:?}", result);
    }
}
