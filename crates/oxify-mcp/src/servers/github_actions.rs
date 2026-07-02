//! GitHub Actions MCP server — provides CI/CD operations via the GitHub REST API
//!
//! This server uses raw `oxihttp` calls (no octocrab) following the same
//! pattern as [`super::gitlab`].

use crate::{McpError, McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

// ── Configuration ─────────────────────────────────────────────────────────────

/// Configuration for the GitHub Actions MCP server.
#[derive(Debug)]
pub struct GitHubActionsConfig {
    /// Personal access token or GitHub App token with `actions` scope.
    pub token: String,
    /// Default repository owner (user or org) applied when tool calls omit `owner`.
    pub default_owner: Option<String>,
    /// Default repository name applied when tool calls omit `repo`.
    pub default_repo: Option<String>,
    /// Base URL for the GitHub REST API.  Override in tests to point at a mock
    /// server (default: `https://api.github.com`).
    pub base_url: String,
}

impl GitHubActionsConfig {
    /// Construct configuration from environment variables.
    ///
    /// Required env vars:
    /// * `GITHUB_TOKEN` — personal access token or GitHub App token
    ///
    /// Optional env vars:
    /// * `GITHUB_DEFAULT_OWNER` — default owner applied when tool calls omit `owner`
    /// * `GITHUB_DEFAULT_REPO`  — default repo applied when tool calls omit `repo`
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN")
            .map_err(|_| McpError::InvalidRequest("GITHUB_TOKEN not set".to_string()))?;
        Ok(Self {
            token,
            default_owner: std::env::var("GITHUB_DEFAULT_OWNER").ok(),
            default_repo: std::env::var("GITHUB_DEFAULT_REPO").ok(),
            base_url: "https://api.github.com".to_string(),
        })
    }
}

impl Default for GitHubActionsConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            default_owner: None,
            default_repo: None,
            base_url: "https://api.github.com".to_string(),
        }
    }
}

// ── Server ────────────────────────────────────────────────────────────────────

/// MCP server that exposes GitHub Actions CI/CD operations.
///
/// All HTTP calls use raw `oxihttp` — no octocrab dependency.
pub struct GitHubActionsServer {
    cfg: GitHubActionsConfig,
    http: oxihttp::HttpsClient,
}

impl GitHubActionsServer {
    /// Create a new server using the supplied configuration.
    ///
    /// The underlying `oxihttp::HttpsClient` is pre-loaded with the three headers
    /// that the GitHub REST API v2022-11-28 requires on every request:
    /// `Accept`, `X-GitHub-Api-Version`, and `User-Agent`.
    pub fn new(cfg: GitHubActionsConfig) -> Result<Self> {
        let mut headers = oxihttp::HeaderMap::new();
        headers.insert(
            oxihttp::HeaderName::from_static("accept"),
            oxihttp::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            oxihttp::HeaderValue::from_static("2022-11-28"),
        );
        headers.insert(
            oxihttp::HeaderName::from_static("user-agent"),
            oxihttp::HeaderValue::from_static("oxify-mcp/0.2"),
        );

        let http = oxihttp::Client::builder()
            .default_headers(headers)
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(30))
            .with_tls()
            .build_https()
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

        Ok(Self { cfg, http })
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Return the `Authorization: Bearer <token>` header value.
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.cfg.token)
    }

    /// Resolve `owner` and `repo` from explicit tool arguments, falling back
    /// to the configured defaults when either is absent.
    fn resolve_owner_repo<'a>(&'a self, args: &'a Value) -> Result<(&'a str, &'a str)> {
        let owner = args["owner"]
            .as_str()
            .or(self.cfg.default_owner.as_deref())
            .ok_or_else(|| {
                McpError::InvalidRequest(
                    "Missing 'owner' argument and GITHUB_DEFAULT_OWNER is not configured"
                        .to_string(),
                )
            })?;

        let repo = args["repo"]
            .as_str()
            .or(self.cfg.default_repo.as_deref())
            .ok_or_else(|| {
                McpError::InvalidRequest(
                    "Missing 'repo' argument and GITHUB_DEFAULT_REPO is not configured".to_string(),
                )
            })?;

        Ok((owner, repo))
    }

    /// Map a non-success HTTP status to the appropriate [`McpError`] variant.
    ///
    /// `url` is passed in explicitly because `oxihttp::Response` (unlike
    /// `reqwest::Response`) does not retain the request URL it was produced from.
    async fn map_error_response(
        &self,
        url: &str,
        status: oxihttp::StatusCode,
        response: oxihttp::Response,
    ) -> McpError {
        let body = response
            .body_text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_string());

        match status.as_u16() {
            401 => McpError::InvalidRequest("GitHub authentication failed".to_string()),
            403 => McpError::InvalidRequest("GitHub authorization denied".to_string()),
            404 => McpError::ToolExecutionError(format!("Not found: {}", url)),
            422 => McpError::InvalidRequest(format!("GitHub validation error: {}", body)),
            _ => McpError::ToolExecutionError(format!("GitHub API error {}: {}", status, body)),
        }
    }

    /// Perform an authenticated GET request and parse the JSON response.
    async fn api_get(&self, url: &str) -> Result<Value> {
        let response = self
            .http
            .get(url)
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .header("Authorization", &self.auth_header())
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(self.map_error_response(url, status, response).await);
        }

        response
            .body_json::<Value>()
            .await
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))
    }

    /// Perform an authenticated POST request with a JSON body.
    ///
    /// Returns `(status_code, response_body_text)`.  The caller decides whether
    /// the status represents success, because several GitHub endpoints return
    /// 201 or 204 rather than 200.
    async fn api_post_raw(&self, url: &str, body: &Value) -> Result<(u16, String)> {
        let response = self
            .http
            .post(url)
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .header("Authorization", &self.auth_header())
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .json(body)
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(self.map_error_response(url, status, response).await);
        }

        let text = response.body_text().await.unwrap_or_else(|_| String::new());
        Ok((status.as_u16(), text))
    }
}

// ── McpServer impl ────────────────────────────────────────────────────────────

#[async_trait]
impl McpServer for GitHubActionsServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            // ── 1. list_workflows ─────────────────────────────────────────────
            "list_workflows" => {
                let (owner, repo) = self.resolve_owner_repo(&arguments)?;
                let url = format!(
                    "{}/repos/{}/{}/actions/workflows",
                    self.cfg.base_url, owner, repo
                );
                let data = self.api_get(&url).await?;
                let total = data["total_count"].as_u64().unwrap_or(0);
                let workflows = data["workflows"].as_array().cloned().unwrap_or_default();

                // Return a curated view: id, name, state, path, created_at, updated_at
                let curated: Vec<Value> = workflows
                    .into_iter()
                    .map(|w| {
                        json!({
                            "id":         w["id"],
                            "name":       w["name"],
                            "state":      w["state"],
                            "path":       w["path"],
                            "created_at": w["created_at"],
                            "updated_at": w["updated_at"],
                        })
                    })
                    .collect();

                Ok(json!({ "total_count": total, "workflows": curated }))
            }

            // ── 2. get_workflow ───────────────────────────────────────────────
            "get_workflow" => {
                let (owner, repo) = self.resolve_owner_repo(&arguments)?;
                let workflow_id = arguments["workflow_id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| arguments["workflow_id"].as_u64().map(|n| n.to_string()))
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'workflow_id'".to_string()))?;

                let url = format!(
                    "{}/repos/{}/{}/actions/workflows/{}",
                    self.cfg.base_url, owner, repo, workflow_id
                );
                self.api_get(&url).await
            }

            // ── 3. list_runs ──────────────────────────────────────────────────
            "list_runs" => {
                let (owner, repo) = self.resolve_owner_repo(&arguments)?;
                let mut query_params: Vec<String> = Vec::new();

                if let Some(branch) = arguments["branch"].as_str() {
                    query_params.push(format!("branch={}", branch));
                }
                if let Some(status) = arguments["status"].as_str() {
                    query_params.push(format!("status={}", status));
                }
                let per_page = arguments["per_page"]
                    .as_u64()
                    .map(|n| n.min(100))
                    .unwrap_or(30);
                query_params.push(format!("per_page={}", per_page));

                let qs = if query_params.is_empty() {
                    String::new()
                } else {
                    format!("?{}", query_params.join("&"))
                };

                let url = format!(
                    "{}/repos/{}/{}/actions/runs{}",
                    self.cfg.base_url, owner, repo, qs
                );
                let data = self.api_get(&url).await?;
                let total = data["total_count"].as_u64().unwrap_or(0);
                let runs = data["workflow_runs"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                Ok(json!({ "total_count": total, "workflow_runs": runs }))
            }

            // ── 4. get_run ────────────────────────────────────────────────────
            "get_run" => {
                let (owner, repo) = self.resolve_owner_repo(&arguments)?;
                let run_id = arguments["run_id"].as_u64().ok_or_else(|| {
                    McpError::InvalidRequest("Missing or invalid 'run_id'".to_string())
                })?;

                let url = format!(
                    "{}/repos/{}/{}/actions/runs/{}",
                    self.cfg.base_url, owner, repo, run_id
                );
                self.api_get(&url).await
            }

            // ── 5. trigger_workflow ───────────────────────────────────────────
            "trigger_workflow" => {
                let (owner, repo) = self.resolve_owner_repo(&arguments)?;
                let workflow_id = arguments["workflow_id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| arguments["workflow_id"].as_u64().map(|n| n.to_string()))
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'workflow_id'".to_string()))?;
                let ref_val = arguments["ref"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'ref'".to_string()))?;
                let inputs = arguments
                    .get("inputs")
                    .cloned()
                    .unwrap_or_else(|| json!({}));

                let url = format!(
                    "{}/repos/{}/{}/actions/workflows/{}/dispatches",
                    self.cfg.base_url, owner, repo, workflow_id
                );
                let body = json!({ "ref": ref_val, "inputs": inputs });
                // GitHub returns 204 No Content on success
                self.api_post_raw(&url, &body).await?;

                Ok(json!({
                    "triggered": true,
                    "workflow_id": workflow_id,
                    "ref": ref_val,
                }))
            }

            // ── 6. cancel_run ─────────────────────────────────────────────────
            "cancel_run" => {
                let (owner, repo) = self.resolve_owner_repo(&arguments)?;
                let run_id = arguments["run_id"].as_u64().ok_or_else(|| {
                    McpError::InvalidRequest("Missing or invalid 'run_id'".to_string())
                })?;

                let url = format!(
                    "{}/repos/{}/{}/actions/runs/{}/cancel",
                    self.cfg.base_url, owner, repo, run_id
                );
                // GitHub returns 202 Accepted with empty body
                self.api_post_raw(&url, &json!({})).await?;

                Ok(json!({ "cancelled": true, "run_id": run_id }))
            }

            // ── 7. rerun_failed_jobs ──────────────────────────────────────────
            "rerun_failed_jobs" => {
                let (owner, repo) = self.resolve_owner_repo(&arguments)?;
                let run_id = arguments["run_id"].as_u64().ok_or_else(|| {
                    McpError::InvalidRequest("Missing or invalid 'run_id'".to_string())
                })?;
                let enable_debug_logging =
                    arguments["enable_debug_logging"].as_bool().unwrap_or(false);

                let url = format!(
                    "{}/repos/{}/{}/actions/runs/{}/rerun-failed-jobs",
                    self.cfg.base_url, owner, repo, run_id
                );
                let body = json!({ "enable_debug_logging": enable_debug_logging });
                // GitHub returns 201 Created
                self.api_post_raw(&url, &body).await?;

                Ok(json!({ "rerun_triggered": true, "run_id": run_id }))
            }

            // ── 8. list_artifacts ─────────────────────────────────────────────
            "list_artifacts" => {
                let (owner, repo) = self.resolve_owner_repo(&arguments)?;
                let run_id = arguments["run_id"].as_u64().ok_or_else(|| {
                    McpError::InvalidRequest("Missing or invalid 'run_id'".to_string())
                })?;

                let url = format!(
                    "{}/repos/{}/{}/actions/runs/{}/artifacts",
                    self.cfg.base_url, owner, repo, run_id
                );
                let data = self.api_get(&url).await?;
                let total = data["total_count"].as_u64().unwrap_or(0);
                let artifacts = data["artifacts"].as_array().cloned().unwrap_or_default();

                let curated: Vec<Value> = artifacts
                    .into_iter()
                    .map(|a| {
                        json!({
                            "id":                    a["id"],
                            "name":                  a["name"],
                            "size_in_bytes":         a["size_in_bytes"],
                            "created_at":            a["created_at"],
                            "expires_at":            a["expires_at"],
                            "archive_download_url":  a["archive_download_url"],
                        })
                    })
                    .collect();

                Ok(json!({ "total_count": total, "artifacts": curated }))
            }

            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            // ── 1 ─────────────────────────────────────────────────────────────
            json!({
                "name": "list_workflows",
                "description": "List GitHub Actions workflows defined in a repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner (user or org). Uses GITHUB_DEFAULT_OWNER if omitted."
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name. Uses GITHUB_DEFAULT_REPO if omitted."
                        }
                    }
                }
            }),
            // ── 2 ─────────────────────────────────────────────────────────────
            json!({
                "name": "get_workflow",
                "description": "Fetch details for a single GitHub Actions workflow by ID or filename",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner. Uses GITHUB_DEFAULT_OWNER if omitted."
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name. Uses GITHUB_DEFAULT_REPO if omitted."
                        },
                        "workflow_id": {
                            "type": ["string", "integer"],
                            "description": "Workflow ID (numeric) or workflow filename (e.g. ci.yml)"
                        }
                    },
                    "required": ["workflow_id"]
                }
            }),
            // ── 3 ─────────────────────────────────────────────────────────────
            json!({
                "name": "list_runs",
                "description": "List workflow runs for a repository, optionally filtered by branch or status",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner. Uses GITHUB_DEFAULT_OWNER if omitted."
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name. Uses GITHUB_DEFAULT_REPO if omitted."
                        },
                        "branch": {
                            "type": "string",
                            "description": "Filter runs to a specific branch name"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["queued", "in_progress", "completed", "waiting"],
                            "description": "Filter runs by status"
                        },
                        "per_page": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 100,
                            "description": "Number of runs to return (default: 30, max: 100)"
                        }
                    }
                }
            }),
            // ── 4 ─────────────────────────────────────────────────────────────
            json!({
                "name": "get_run",
                "description": "Fetch details for a single workflow run",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner. Uses GITHUB_DEFAULT_OWNER if omitted."
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name. Uses GITHUB_DEFAULT_REPO if omitted."
                        },
                        "run_id": {
                            "type": "integer",
                            "description": "Unique workflow run identifier"
                        }
                    },
                    "required": ["run_id"]
                }
            }),
            // ── 5 ─────────────────────────────────────────────────────────────
            json!({
                "name": "trigger_workflow",
                "description": "Manually dispatch a GitHub Actions workflow via workflow_dispatch event",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner. Uses GITHUB_DEFAULT_OWNER if omitted."
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name. Uses GITHUB_DEFAULT_REPO if omitted."
                        },
                        "workflow_id": {
                            "type": ["string", "integer"],
                            "description": "Workflow ID or filename (e.g. ci.yml)"
                        },
                        "ref": {
                            "type": "string",
                            "description": "The branch, tag, or SHA to run the workflow on"
                        },
                        "inputs": {
                            "type": "object",
                            "description": "Key-value pairs of workflow_dispatch inputs"
                        }
                    },
                    "required": ["workflow_id", "ref"]
                }
            }),
            // ── 6 ─────────────────────────────────────────────────────────────
            json!({
                "name": "cancel_run",
                "description": "Cancel an in-progress or queued workflow run",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner. Uses GITHUB_DEFAULT_OWNER if omitted."
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name. Uses GITHUB_DEFAULT_REPO if omitted."
                        },
                        "run_id": {
                            "type": "integer",
                            "description": "Unique workflow run identifier"
                        }
                    },
                    "required": ["run_id"]
                }
            }),
            // ── 7 ─────────────────────────────────────────────────────────────
            json!({
                "name": "rerun_failed_jobs",
                "description": "Re-run only the failed jobs from a completed workflow run",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner. Uses GITHUB_DEFAULT_OWNER if omitted."
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name. Uses GITHUB_DEFAULT_REPO if omitted."
                        },
                        "run_id": {
                            "type": "integer",
                            "description": "Unique workflow run identifier"
                        },
                        "enable_debug_logging": {
                            "type": "boolean",
                            "description": "Whether to enable debug logging for the re-run (default: false)"
                        }
                    },
                    "required": ["run_id"]
                }
            }),
            // ── 8 ─────────────────────────────────────────────────────────────
            json!({
                "name": "list_artifacts",
                "description": "List artifacts produced by a workflow run",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner. Uses GITHUB_DEFAULT_OWNER if omitted."
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name. Uses GITHUB_DEFAULT_REPO if omitted."
                        },
                        "run_id": {
                            "type": "integer",
                            "description": "Unique workflow run identifier"
                        }
                    },
                    "required": ["run_id"]
                }
            }),
        ])
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Build a [`GitHubActionsServer`] whose base URL points at `mock_uri`.
    fn server_with_mock(mock_uri: &str) -> GitHubActionsServer {
        let cfg = GitHubActionsConfig {
            token: "ghs_test_token".to_string(),
            default_owner: Some("cool-japan".to_string()),
            default_repo: Some("oxify".to_string()),
            base_url: mock_uri.to_string(),
        };
        GitHubActionsServer::new(cfg).expect("server construction")
    }

    // ── 1. list_tools ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_tools_returns_eight() {
        let server = GitHubActionsServer::new(GitHubActionsConfig::default()).unwrap();
        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 8, "expected exactly 8 tools");
    }

    // ── 2. list_workflows_success ─────────────────────────────────────────────

    #[tokio::test]
    async fn list_workflows_success() {
        let mock = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/cool-japan/oxify/actions/workflows"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 1,
                "workflows": [
                    {
                        "id": 1,
                        "name": "CI",
                        "state": "active",
                        "path": ".github/workflows/ci.yml",
                        "created_at": "2024-01-01T00:00:00Z",
                        "updated_at": "2024-06-01T00:00:00Z"
                    }
                ]
            })))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool("list_workflows", json!({}))
            .await
            .expect("list_workflows");

        assert_eq!(result["total_count"], 1);
        let wf = &result["workflows"][0];
        assert_eq!(wf["name"], "CI");
        assert_eq!(wf["state"], "active");
    }

    // ── 3. list_runs_success ──────────────────────────────────────────────────

    #[tokio::test]
    async fn list_runs_success() {
        let mock = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/cool-japan/oxify/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 2,
                "workflow_runs": [
                    { "id": 10, "status": "completed", "conclusion": "success" },
                    { "id": 11, "status": "in_progress", "conclusion": null }
                ]
            })))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool("list_runs", json!({}))
            .await
            .expect("list_runs");

        assert_eq!(result["total_count"], 2);
        assert_eq!(result["workflow_runs"].as_array().unwrap().len(), 2);
    }

    // ── 4. get_run_success ────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_run_success() {
        let mock = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/cool-japan/oxify/actions/runs/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 42,
                "status": "completed",
                "conclusion": "failure",
                "head_branch": "main"
            })))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool("get_run", json!({ "run_id": 42 }))
            .await
            .expect("get_run");

        assert_eq!(result["id"], 42);
        assert_eq!(result["conclusion"], "failure");
    }

    // ── 5. trigger_workflow_success ───────────────────────────────────────────

    #[tokio::test]
    async fn trigger_workflow_success() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path(
                "/repos/cool-japan/oxify/actions/workflows/ci.yml/dispatches",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool(
                "trigger_workflow",
                json!({
                    "workflow_id": "ci.yml",
                    "ref": "main"
                }),
            )
            .await
            .expect("trigger_workflow");

        assert_eq!(result["triggered"], true);
        assert_eq!(result["workflow_id"], "ci.yml");
        assert_eq!(result["ref"], "main");
    }

    // ── 6. cancel_run_success ─────────────────────────────────────────────────

    #[tokio::test]
    async fn cancel_run_success() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/cool-japan/oxify/actions/runs/99/cancel"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool("cancel_run", json!({ "run_id": 99 }))
            .await
            .expect("cancel_run");

        assert_eq!(result["cancelled"], true);
        assert_eq!(result["run_id"], 99);
    }

    // ── 7. rerun_failed_jobs_success ──────────────────────────────────────────

    #[tokio::test]
    async fn rerun_failed_jobs_success() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path(
                "/repos/cool-japan/oxify/actions/runs/77/rerun-failed-jobs",
            ))
            .and(body_json(json!({ "enable_debug_logging": false })))
            .respond_with(ResponseTemplate::new(201))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool("rerun_failed_jobs", json!({ "run_id": 77 }))
            .await
            .expect("rerun_failed_jobs");

        assert_eq!(result["rerun_triggered"], true);
        assert_eq!(result["run_id"], 77);
    }

    // ── 8. list_artifacts_success ─────────────────────────────────────────────

    #[tokio::test]
    async fn list_artifacts_success() {
        let mock = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/cool-japan/oxify/actions/runs/55/artifacts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 1,
                "artifacts": [
                    {
                        "id": 1001,
                        "name": "build-output",
                        "size_in_bytes": 204800,
                        "created_at": "2024-06-01T10:00:00Z",
                        "expires_at": "2024-07-01T10:00:00Z",
                        "archive_download_url": "https://api.github.com/repos/cool-japan/oxify/actions/artifacts/1001/zip"
                    }
                ]
            })))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool("list_artifacts", json!({ "run_id": 55 }))
            .await
            .expect("list_artifacts");

        assert_eq!(result["total_count"], 1);
        let art = &result["artifacts"][0];
        assert_eq!(art["name"], "build-output");
        assert_eq!(art["size_in_bytes"], 204800);
    }

    // ── 9. auth_header_sent ───────────────────────────────────────────────────

    #[tokio::test]
    async fn auth_header_sent() {
        let mock = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/cool-japan/oxify/actions/workflows"))
            .and(header("Authorization", "Bearer ghs_test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 0,
                "workflows": []
            })))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server.call_tool("list_workflows", json!({})).await;
        // The mock only matches if the Authorization header was correctly sent;
        // without the header wiremock returns 404.
        assert!(
            result.is_ok(),
            "Authorization header was not sent correctly"
        );
    }

    // ── 10. from_env_missing_token ────────────────────────────────────────────

    #[test]
    fn from_env_missing_token() {
        // Temporarily remove the token from the environment
        let prev = std::env::var("GITHUB_TOKEN").ok();
        std::env::remove_var("GITHUB_TOKEN");

        let result = GitHubActionsConfig::from_env();
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::InvalidRequest(msg) => {
                assert!(msg.contains("GITHUB_TOKEN"), "unexpected message: {msg}");
            }
            other => panic!("Expected InvalidRequest, got {:?}", other),
        }

        // Restore
        if let Some(val) = prev {
            std::env::set_var("GITHUB_TOKEN", val);
        }
    }

    // ── 11. unknown_tool ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn unknown_tool() {
        let server = GitHubActionsServer::new(GitHubActionsConfig::default()).unwrap();
        let result = server.call_tool("does_not_exist", json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::ToolNotFound(name) => {
                assert_eq!(name, "does_not_exist");
            }
            other => panic!("Expected ToolNotFound, got {:?}", other),
        }
    }

    // ── 12. list_runs_with_filters ────────────────────────────────────────────

    #[tokio::test]
    async fn list_runs_with_branch_and_status_filters() {
        let mock = MockServer::start().await;

        // The path is fixed; query params are not matched by path() but that's
        // fine — we just verify the call succeeds and the response is parsed.
        Mock::given(method("GET"))
            .and(path("/repos/cool-japan/oxify/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 1,
                "workflow_runs": [{ "id": 5, "status": "completed" }]
            })))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool(
                "list_runs",
                json!({ "branch": "main", "status": "completed", "per_page": 10 }),
            )
            .await
            .expect("list_runs with filters");

        assert_eq!(result["total_count"], 1);
    }

    // ── 13. rerun_with_debug_logging ──────────────────────────────────────────

    #[tokio::test]
    async fn rerun_with_debug_logging_enabled() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path(
                "/repos/cool-japan/oxify/actions/runs/88/rerun-failed-jobs",
            ))
            .and(body_json(json!({ "enable_debug_logging": true })))
            .respond_with(ResponseTemplate::new(201))
            .mount(&mock)
            .await;

        let server = server_with_mock(&mock.uri());
        let result = server
            .call_tool(
                "rerun_failed_jobs",
                json!({ "run_id": 88, "enable_debug_logging": true }),
            )
            .await
            .expect("rerun with debug logging");

        assert_eq!(result["rerun_triggered"], true);
        assert_eq!(result["run_id"], 88);
    }
}
