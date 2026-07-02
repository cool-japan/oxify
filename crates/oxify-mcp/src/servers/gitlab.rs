//! GitLab MCP server - provides GitLab API operations via REST v4

use crate::{McpError, McpServer, Result};
use async_trait::async_trait;
use base64::Engine as _;
use serde_json::{json, Value};

/// Configuration for the GitLab MCP server
pub struct GitLabConfig {
    /// Personal access token for the GitLab API
    pub token: String,
    /// Base URL for the GitLab API
    pub base_url: String,
    /// Default project (namespace/name) used when no `project` argument is supplied
    pub default_project: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// User-Agent header value sent to GitLab
    pub user_agent: String,
}

impl GitLabConfig {
    /// Construct configuration from environment variables.
    ///
    /// Required env vars:
    /// * `GITLAB_TOKEN` — personal access token
    ///
    /// Optional env vars:
    /// * `GITLAB_BASE_URL` — base URL (default: `https://gitlab.com/api/v4`)
    /// * `GITLAB_DEFAULT_PROJECT` — default project applied when tool calls omit `project`
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("GITLAB_TOKEN")
            .map_err(|_| McpError::InvalidRequest("GITLAB_TOKEN not set".to_string()))?;
        let base_url = std::env::var("GITLAB_BASE_URL")
            .unwrap_or_else(|_| "https://gitlab.com/api/v4".to_string());
        Ok(Self {
            token,
            base_url,
            default_project: std::env::var("GITLAB_DEFAULT_PROJECT").ok(),
            timeout_secs: 30,
            user_agent: "oxify-mcp/0.2".to_string(),
        })
    }

    /// Resolve the project from an explicit tool argument or fall back to `default_project`.
    fn resolve_project<'a>(&'a self, arguments: &'a Value) -> Result<&'a str> {
        if let Some(project) = arguments["project"].as_str() {
            return Ok(project);
        }
        self.default_project.as_deref().ok_or_else(|| {
            McpError::InvalidRequest(
                "Missing 'project' argument and GITLAB_DEFAULT_PROJECT is not configured"
                    .to_string(),
            )
        })
    }
}

impl Default for GitLabConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            base_url: "https://gitlab.com/api/v4".to_string(),
            default_project: None,
            timeout_secs: 30,
            user_agent: "oxify-mcp/0.2".to_string(),
        }
    }
}

/// MCP server backed by the GitLab REST API v4
pub struct GitLabServer {
    client: oxihttp::HttpsClient,
    cfg: GitLabConfig,
}

impl GitLabServer {
    /// Create a new GitLab server using the supplied configuration.
    pub fn new(cfg: GitLabConfig) -> Result<Self> {
        let mut headers = oxihttp::HeaderMap::new();
        if !cfg.token.is_empty() {
            headers.insert(
                "PRIVATE-TOKEN",
                oxihttp::HeaderValue::from_str(&cfg.token)
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

    /// Perform a GET request against the GitLab API and parse the JSON response.
    async fn api_get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.cfg.base_url, path);
        let response = self
            .client
            .get(&url)
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .body_text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(McpError::ToolExecutionError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        response
            .body_json::<Value>()
            .await
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))
    }

    /// Perform a POST request against the GitLab API and parse the JSON response.
    async fn api_post(&self, path: &str, body: Value) -> Result<Value> {
        let url = format!("{}{}", self.cfg.base_url, path);
        let response = self
            .client
            .post(&url)
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

        response
            .body_json::<Value>()
            .await
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))
    }
}

#[async_trait]
impl McpServer for GitLabServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            // ── 1. list_projects ─────────────────────────────────────────────
            "list_projects" => {
                let mut path = "/projects?membership=true&per_page=20".to_string();
                if let Some(search) = arguments["search"].as_str() {
                    path = format!("{}&search={}", path, search);
                }
                let result = self.api_get(&path).await?;
                Ok(json!({ "projects": result }))
            }

            // ── 2. get_project ───────────────────────────────────────────────
            "get_project" => {
                let project = arguments["project"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'project'".to_string()))?;
                let encoded = project.replace('/', "%2F");
                let result = self.api_get(&format!("/projects/{}", encoded)).await?;
                Ok(result)
            }

            // ── 3. list_issues ───────────────────────────────────────────────
            "list_issues" => {
                let project = self.cfg.resolve_project(&arguments)?;
                let encoded = project.replace('/', "%2F");
                let state = arguments["state"].as_str().unwrap_or("opened");
                let mut path = format!("/projects/{}/issues?state={}&per_page=20", encoded, state);
                if let Some(search) = arguments["search"].as_str() {
                    path = format!("{}&search={}", path, search);
                }
                let result = self.api_get(&path).await?;
                Ok(json!({ "issues": result }))
            }

            // ── 4. create_issue ──────────────────────────────────────────────
            "create_issue" => {
                let project = self.cfg.resolve_project(&arguments)?;
                let encoded = project.replace('/', "%2F");
                let title = arguments["title"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'title'".to_string()))?;

                let mut payload = json!({ "title": title });
                if let Some(desc) = arguments["body"].as_str() {
                    payload["description"] = json!(desc);
                }
                if let Some(labels) = arguments["labels"].as_str() {
                    payload["labels"] = json!(labels);
                }

                let result = self
                    .api_post(&format!("/projects/{}/issues", encoded), payload)
                    .await?;
                Ok(result)
            }

            // ── 5. list_merge_requests ───────────────────────────────────────
            "list_merge_requests" => {
                let project = self.cfg.resolve_project(&arguments)?;
                let encoded = project.replace('/', "%2F");
                let state = arguments["state"].as_str().unwrap_or("opened");
                let path = format!(
                    "/projects/{}/merge_requests?state={}&per_page=20",
                    encoded, state
                );
                let result = self.api_get(&path).await?;
                Ok(json!({ "merge_requests": result }))
            }

            // ── 6. create_merge_request ──────────────────────────────────────
            "create_merge_request" => {
                let project = self.cfg.resolve_project(&arguments)?;
                let encoded = project.replace('/', "%2F");
                let title = arguments["title"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'title'".to_string()))?;
                let source_branch = arguments["source_branch"].as_str().ok_or_else(|| {
                    McpError::InvalidRequest("Missing 'source_branch'".to_string())
                })?;
                let target_branch = arguments["target_branch"].as_str().ok_or_else(|| {
                    McpError::InvalidRequest("Missing 'target_branch'".to_string())
                })?;

                let mut payload = json!({
                    "title": title,
                    "source_branch": source_branch,
                    "target_branch": target_branch,
                });
                if let Some(desc) = arguments["body"].as_str() {
                    payload["description"] = json!(desc);
                }

                let result = self
                    .api_post(&format!("/projects/{}/merge_requests", encoded), payload)
                    .await?;
                Ok(result)
            }

            // ── 7. get_file ──────────────────────────────────────────────────
            "get_file" => {
                let project = self.cfg.resolve_project(&arguments)?;
                let proj_encoded = project.replace('/', "%2F");
                let path = arguments["path"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'path'".to_string()))?;
                let branch = arguments["branch"].as_str().unwrap_or("main");
                let file_encoded = path.replace('/', "%2F");

                let api_path = format!(
                    "/projects/{}/repository/files/{}?ref={}",
                    proj_encoded, file_encoded, branch
                );
                let result = self.api_get(&api_path).await?;

                let content_b64 = result["content"]
                    .as_str()
                    .ok_or_else(|| {
                        McpError::ToolExecutionError(
                            "No 'content' field in file response".to_string(),
                        )
                    })?
                    // GitLab includes newlines in the base64 blob — strip them
                    .replace('\n', "");

                let raw = base64::engine::general_purpose::STANDARD
                    .decode(content_b64.as_bytes())
                    .map_err(|e| {
                        McpError::ToolExecutionError(format!("Base64 decode failed: {}", e))
                    })?;
                let decoded_content = String::from_utf8(raw).map_err(|e| {
                    McpError::ToolExecutionError(format!("UTF-8 decode failed: {}", e))
                })?;

                Ok(json!({
                    "path": path,
                    "content": decoded_content,
                    "encoding": "utf-8",
                }))
            }

            // ── 8. search ────────────────────────────────────────────────────
            "search" => {
                let query = arguments["query"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'query'".to_string()))?;

                let api_path = if let Some(project) = arguments["project"].as_str() {
                    let encoded = project.replace('/', "%2F");
                    format!("/projects/{}/search?scope=blobs&search={}", encoded, query)
                } else {
                    format!("/search?scope=blobs&search={}", query)
                };

                let result = self.api_get(&api_path).await?;
                Ok(json!({ "results": result }))
            }

            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "list_projects",
                "description": "List GitLab projects the user has access to",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "search": {
                            "type": "string",
                            "description": "Optional search term"
                        }
                    }
                }
            }),
            json!({
                "name": "get_project",
                "description": "Fetch details for a single GitLab project",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Project ID or namespace/name (e.g. mygroup/myproject)"
                        }
                    },
                    "required": ["project"]
                }
            }),
            json!({
                "name": "list_issues",
                "description": "List issues in a GitLab project",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Project namespace/name or ID (uses GITLAB_DEFAULT_PROJECT if omitted)"
                        },
                        "state": {
                            "type": "string",
                            "enum": ["opened", "closed", "all"],
                            "description": "Issue state filter (default: opened)"
                        },
                        "search": {
                            "type": "string",
                            "description": "Optional search term"
                        }
                    }
                }
            }),
            json!({
                "name": "create_issue",
                "description": "Create a new issue in a GitLab project",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Project namespace/name or ID"
                        },
                        "title": {
                            "type": "string",
                            "description": "Issue title"
                        },
                        "body": {
                            "type": "string",
                            "description": "Issue description"
                        },
                        "labels": {
                            "type": "string",
                            "description": "Comma-separated label names"
                        }
                    },
                    "required": ["title"]
                }
            }),
            json!({
                "name": "list_merge_requests",
                "description": "List merge requests in a GitLab project",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Project namespace/name or ID (uses GITLAB_DEFAULT_PROJECT if omitted)"
                        },
                        "state": {
                            "type": "string",
                            "enum": ["opened", "closed", "merged", "all"],
                            "description": "Merge-request state filter (default: opened)"
                        }
                    }
                }
            }),
            json!({
                "name": "create_merge_request",
                "description": "Create a merge request in a GitLab project",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Project namespace/name or ID"
                        },
                        "title": {
                            "type": "string",
                            "description": "Merge request title"
                        },
                        "source_branch": {
                            "type": "string",
                            "description": "Source branch name"
                        },
                        "target_branch": {
                            "type": "string",
                            "description": "Target branch name"
                        },
                        "body": {
                            "type": "string",
                            "description": "Merge request description"
                        }
                    },
                    "required": ["title", "source_branch", "target_branch"]
                }
            }),
            json!({
                "name": "get_file",
                "description": "Retrieve the content of a file from a GitLab repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Project namespace/name or ID (uses GITLAB_DEFAULT_PROJECT if omitted)"
                        },
                        "path": {
                            "type": "string",
                            "description": "File path within the repository (e.g. src/main.rs)"
                        },
                        "branch": {
                            "type": "string",
                            "description": "Branch, tag or commit SHA (default: main)"
                        }
                    },
                    "required": ["path"]
                }
            }),
            json!({
                "name": "search",
                "description": "Search for code blobs in a GitLab project or across all accessible projects",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query string"
                        },
                        "project": {
                            "type": "string",
                            "description": "Restrict search to this project (namespace/name or ID)"
                        }
                    },
                    "required": ["query"]
                }
            }),
        ])
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env_missing_token_errors() {
        std::env::remove_var("GITLAB_TOKEN");
        let result = GitLabConfig::from_env();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_tools_returns_eight() {
        let cfg = GitLabConfig::default();
        let server = GitLabServer::new(cfg).unwrap();
        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 8);
    }

    #[tokio::test]
    async fn test_call_tool_unknown_returns_error() {
        let cfg = GitLabConfig::default();
        let server = GitLabServer::new(cfg).unwrap();
        let result = server.call_tool("nonexistent", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_issues_missing_project_no_default() {
        let cfg = GitLabConfig {
            token: "fake".to_string(),
            ..Default::default()
        };
        let server = GitLabServer::new(cfg).unwrap();
        let result = server.call_tool("list_issues", serde_json::json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::InvalidRequest(_) => {}
            other => panic!("Expected InvalidRequest, got {:?}", other),
        }
    }

    #[test]
    fn test_list_tools_all_names_unique() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let tools = runtime.block_on(async {
            let server = GitLabServer::new(GitLabConfig::default()).unwrap();
            server.list_tools().await.unwrap()
        });
        let names: std::collections::HashSet<&str> =
            tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names.len(), 8, "all tool names must be unique");
    }

    #[test]
    fn test_config_default_has_empty_token() {
        let cfg = GitLabConfig::default();
        assert!(cfg.token.is_empty());
        assert!(cfg.default_project.is_none());
        assert_eq!(cfg.timeout_secs, 30);
        assert_eq!(cfg.base_url, "https://gitlab.com/api/v4");
    }

    /// Live integration test — skipped in CI; run manually with a valid GITLAB_TOKEN set.
    #[ignore]
    #[tokio::test]
    async fn test_create_issue_live() {
        let cfg = GitLabConfig::from_env().expect("GITLAB_TOKEN must be set for live test");
        let server = GitLabServer::new(cfg).unwrap();
        let result = server
            .call_tool(
                "create_issue",
                serde_json::json!({
                    "project": std::env::var("GITLAB_DEFAULT_PROJECT").unwrap_or_default(),
                    "title": "oxify-mcp live test issue",
                    "body": "Created by automated live test"
                }),
            )
            .await;
        assert!(result.is_ok(), "live create_issue failed: {:?}", result);
    }
}
