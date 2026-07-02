//! Jira MCP server - provides Jira API operations via REST v3

use crate::{McpError, McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

/// Configuration for the Jira MCP server
pub struct JiraConfig {
    /// Atlassian Cloud site name (e.g. `mycompany` for `mycompany.atlassian.net`)
    pub site: String,
    /// Atlassian account e-mail used for basic auth
    pub email: String,
    /// Atlassian API token used for basic auth
    pub api_token: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// User-Agent header value sent to Jira
    pub user_agent: String,
}

impl JiraConfig {
    /// Construct configuration from environment variables.
    ///
    /// Required env vars:
    /// * `JIRA_SITE`      — Atlassian Cloud site name (subdomain only, not full hostname)
    /// * `JIRA_EMAIL`     — Atlassian account e-mail
    /// * `JIRA_API_TOKEN` — Atlassian API token
    pub fn from_env() -> Result<Self> {
        let site = std::env::var("JIRA_SITE")
            .map_err(|_| McpError::InvalidRequest("JIRA_SITE not set".to_string()))?;
        let email = std::env::var("JIRA_EMAIL")
            .map_err(|_| McpError::InvalidRequest("JIRA_EMAIL not set".to_string()))?;
        let api_token = std::env::var("JIRA_API_TOKEN")
            .map_err(|_| McpError::InvalidRequest("JIRA_API_TOKEN not set".to_string()))?;
        Ok(Self {
            site,
            email,
            api_token,
            timeout_secs: 30,
            user_agent: "oxify-mcp/0.2".to_string(),
        })
    }
}

impl Default for JiraConfig {
    fn default() -> Self {
        Self {
            site: String::new(),
            email: String::new(),
            api_token: String::new(),
            timeout_secs: 30,
            user_agent: "oxify-mcp/0.2".to_string(),
        }
    }
}

/// MCP server backed by the Jira REST API v3
pub struct JiraServer {
    client: oxihttp::HttpsClient,
    cfg: JiraConfig,
}

impl JiraServer {
    /// Create a new Jira server using the supplied configuration.
    pub fn new(cfg: JiraConfig) -> Result<Self> {
        let client = oxihttp::Client::builder()
            .user_agent(&cfg.user_agent)
            .connect_timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .read_timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .with_tls()
            .build_https()
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;
        Ok(Self { client, cfg })
    }

    /// Compute the Jira REST API v3 base URL from the configured site name.
    fn base_url(&self) -> String {
        format!("https://{}.atlassian.net/rest/api/3", self.cfg.site)
    }

    /// Perform an authenticated GET request and parse the JSON response.
    async fn authed_get(&self, url: &str) -> Result<Value> {
        let response = self
            .client
            .get(url)
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .basic_auth(&self.cfg.email, Some(&self.cfg.api_token))
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

    /// Perform an authenticated POST request and parse the JSON response.
    async fn authed_post(&self, url: &str, body: Value) -> Result<Value> {
        let response = self
            .client
            .post(url)
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
            .basic_auth(&self.cfg.email, Some(&self.cfg.api_token))
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

    /// Wrap a plain-text string in Atlassian Document Format (ADF).
    fn to_adf(text: &str) -> Value {
        json!({
            "version": 1,
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }
            ]
        })
    }
}

#[async_trait]
impl McpServer for JiraServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let base = self.base_url();
        match name {
            // ── 1. list_projects ─────────────────────────────────────────────
            "list_projects" => {
                let url = format!("{}/project/search?maxResults=50", base);
                let result = self.authed_get(&url).await?;
                Ok(json!({ "projects": result }))
            }

            // ── 2. get_project ───────────────────────────────────────────────
            "get_project" => {
                let key = arguments["key"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'key'".to_string()))?;
                let url = format!("{}/project/{}", base, key);
                let result = self.authed_get(&url).await?;
                Ok(result)
            }

            // ── 3. search_issues ─────────────────────────────────────────────
            "search_issues" => {
                let jql = arguments["jql"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'jql'".to_string()))?;
                let max_results = arguments["max_results"].as_u64().unwrap_or(20);

                let url = format!("{}/search", base);
                let payload = json!({
                    "jql": jql,
                    "maxResults": max_results,
                    "fields": [
                        "summary", "status", "assignee", "reporter",
                        "priority", "created", "updated"
                    ]
                });
                let result = self.authed_post(&url, payload).await?;
                Ok(result)
            }

            // ── 4. get_issue ─────────────────────────────────────────────────
            "get_issue" => {
                let key = arguments["key"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'key'".to_string()))?;
                let url = format!("{}/issue/{}", base, key);
                let result = self.authed_get(&url).await?;
                Ok(result)
            }

            // ── 5. create_issue ──────────────────────────────────────────────
            "create_issue" => {
                let project = arguments["project"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'project'".to_string()))?;
                let title = arguments["title"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'title'".to_string()))?;
                let issue_type = arguments["issue_type"].as_str().unwrap_or("Task");

                let mut fields = json!({
                    "project": { "key": project },
                    "summary": title,
                    "issuetype": { "name": issue_type }
                });

                if let Some(desc) = arguments["description"].as_str() {
                    fields["description"] = Self::to_adf(desc);
                }

                let url = format!("{}/issue", base);
                let result = self.authed_post(&url, json!({ "fields": fields })).await?;
                Ok(result)
            }

            // ── 6. add_comment ───────────────────────────────────────────────
            "add_comment" => {
                let key = arguments["key"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'key'".to_string()))?;
                let body_text = arguments["body"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'body'".to_string()))?;

                let url = format!("{}/issue/{}/comment", base, key);
                let payload = json!({ "body": Self::to_adf(body_text) });
                let result = self.authed_post(&url, payload).await?;
                Ok(result)
            }

            // ── 7. list_transitions ──────────────────────────────────────────
            "list_transitions" => {
                let key = arguments["key"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'key'".to_string()))?;
                let url = format!("{}/issue/{}/transitions", base, key);
                let result = self.authed_get(&url).await?;
                Ok(result)
            }

            // ── 8. transition_issue ──────────────────────────────────────────
            "transition_issue" => {
                let key = arguments["key"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'key'".to_string()))?;
                let transition_id = arguments["transition_id"].as_str().ok_or_else(|| {
                    McpError::InvalidRequest("Missing 'transition_id'".to_string())
                })?;

                let url = format!("{}/issue/{}/transitions", base, key);
                let payload = json!({ "transition": { "id": transition_id } });
                // Jira returns 204 No Content on success; handle the empty body case
                let response = self
                    .client
                    .post(&url)
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
                    .basic_auth(&self.cfg.email, Some(&self.cfg.api_token))
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?
                    .json(&payload)
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

                Ok(json!({ "success": true, "issue": key, "transition_id": transition_id }))
            }

            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "list_projects",
                "description": "List Jira projects on the configured Atlassian site",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            json!({
                "name": "get_project",
                "description": "Fetch details for a single Jira project",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Jira project key (e.g. PROJ)"
                        }
                    },
                    "required": ["key"]
                }
            }),
            json!({
                "name": "search_issues",
                "description": "Search for Jira issues using JQL",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "jql": {
                            "type": "string",
                            "description": "JQL query string (e.g. \"project = MY AND status = Open\")"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of results to return (default: 20)"
                        }
                    },
                    "required": ["jql"]
                }
            }),
            json!({
                "name": "get_issue",
                "description": "Fetch details for a single Jira issue",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Issue key (e.g. PROJ-123)"
                        }
                    },
                    "required": ["key"]
                }
            }),
            json!({
                "name": "create_issue",
                "description": "Create a new issue in a Jira project",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Jira project key (e.g. PROJ)"
                        },
                        "title": {
                            "type": "string",
                            "description": "Issue summary / title"
                        },
                        "description": {
                            "type": "string",
                            "description": "Issue description (plain text, converted to ADF)"
                        },
                        "issue_type": {
                            "type": "string",
                            "description": "Issue type name (default: Task)"
                        }
                    },
                    "required": ["project", "title"]
                }
            }),
            json!({
                "name": "add_comment",
                "description": "Add a comment to a Jira issue",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Issue key (e.g. PROJ-123)"
                        },
                        "body": {
                            "type": "string",
                            "description": "Comment text (plain text, converted to ADF)"
                        }
                    },
                    "required": ["key", "body"]
                }
            }),
            json!({
                "name": "list_transitions",
                "description": "List available status transitions for a Jira issue",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Issue key (e.g. PROJ-123)"
                        }
                    },
                    "required": ["key"]
                }
            }),
            json!({
                "name": "transition_issue",
                "description": "Transition a Jira issue to a new status",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Issue key (e.g. PROJ-123)"
                        },
                        "transition_id": {
                            "type": "string",
                            "description": "Transition ID obtained from list_transitions"
                        }
                    },
                    "required": ["key", "transition_id"]
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
    fn test_config_from_env_missing_env_errors() {
        std::env::remove_var("JIRA_SITE");
        let result = JiraConfig::from_env();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_tools_returns_eight() {
        let cfg = JiraConfig::default();
        let server = JiraServer::new(cfg).unwrap();
        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 8);
    }

    #[tokio::test]
    async fn test_call_tool_unknown_returns_error() {
        let cfg = JiraConfig::default();
        let server = JiraServer::new(cfg).unwrap();
        let result = server.call_tool("nonexistent", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_issue_missing_key() {
        let cfg = JiraConfig::default();
        let server = JiraServer::new(cfg).unwrap();
        let result = server.call_tool("get_issue", serde_json::json!({})).await;
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
            let server = JiraServer::new(JiraConfig::default()).unwrap();
            server.list_tools().await.unwrap()
        });
        let names: std::collections::HashSet<&str> =
            tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names.len(), 8, "all tool names must be unique");
    }

    #[test]
    fn test_config_default_fields() {
        let cfg = JiraConfig::default();
        assert!(cfg.site.is_empty());
        assert!(cfg.email.is_empty());
        assert!(cfg.api_token.is_empty());
        assert_eq!(cfg.timeout_secs, 30);
        assert_eq!(cfg.user_agent, "oxify-mcp/0.2");
    }

    /// Live integration test — skipped in CI; run manually with valid Jira credentials set.
    #[ignore]
    #[tokio::test]
    async fn test_search_issues_live() {
        let cfg = JiraConfig::from_env()
            .expect("JIRA_SITE, JIRA_EMAIL, JIRA_API_TOKEN must be set for live test");
        let server = JiraServer::new(cfg).unwrap();
        let result = server
            .call_tool(
                "search_issues",
                serde_json::json!({ "jql": "ORDER BY created DESC", "max_results": 5 }),
            )
            .await;
        assert!(result.is_ok(), "live search_issues failed: {:?}", result);
    }
}
