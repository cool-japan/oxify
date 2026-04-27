//! GitHub MCP server - provides GitHub API operations via octocrab

use crate::{McpError, McpServer, Result};
use async_trait::async_trait;
use octocrab::Octocrab;
use serde_json::{json, Value};

/// Configuration for the GitHub MCP server
pub struct GitHubConfig {
    /// Personal access token for the GitHub API
    pub token: String,
    /// Default repository owner (user or org) used when no `owner` argument is supplied
    pub default_owner: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// User-Agent header value sent to GitHub
    pub user_agent: String,
}

impl GitHubConfig {
    /// Construct configuration from environment variables.
    ///
    /// Required env vars:
    /// * `GITHUB_TOKEN` — personal access token
    ///
    /// Optional env vars:
    /// * `GITHUB_DEFAULT_OWNER` — default owner applied when tool calls omit `owner`
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN")
            .map_err(|_| McpError::InvalidRequest("GITHUB_TOKEN not set".to_string()))?;
        Ok(Self {
            token,
            default_owner: std::env::var("GITHUB_DEFAULT_OWNER").ok(),
            timeout_secs: 30,
            user_agent: "oxify-mcp/0.2".to_string(),
        })
    }

    /// Resolve the owner from an explicit tool argument or fall back to `default_owner`.
    fn resolve_owner<'a>(&'a self, arguments: &'a Value) -> Result<&'a str> {
        if let Some(owner) = arguments["owner"].as_str() {
            return Ok(owner);
        }
        self.default_owner.as_deref().ok_or_else(|| {
            McpError::InvalidRequest(
                "Missing 'owner' argument and GITHUB_DEFAULT_OWNER is not configured".to_string(),
            )
        })
    }
}

impl Default for GitHubConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            default_owner: None,
            timeout_secs: 30,
            user_agent: "oxify-mcp/0.2".to_string(),
        }
    }
}

/// MCP server backed by the GitHub REST API
pub struct GitHubServer {
    client: Octocrab,
    cfg: GitHubConfig,
}

impl GitHubServer {
    /// Create a new GitHub server using the supplied configuration.
    pub fn new(cfg: GitHubConfig) -> Result<Self> {
        let client = Octocrab::builder()
            .personal_token(cfg.token.clone())
            .build()
            .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;
        Ok(Self { client, cfg })
    }
}

#[async_trait]
impl McpServer for GitHubServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            // ── 1. list_repos ────────────────────────────────────────────────
            "list_repos" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let page = self
                    .client
                    .users(owner)
                    .repos()
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let repos: Vec<Value> = page
                    .items
                    .into_iter()
                    .map(|r| {
                        json!({
                            "name": r.name,
                            "full_name": r.full_name,
                            "description": r.description,
                            "private": r.private,
                            "language": r.language,
                            "stargazers_count": r.stargazers_count,
                        })
                    })
                    .collect();

                Ok(json!({ "repos": repos }))
            }

            // ── 2. get_repo ──────────────────────────────────────────────────
            "get_repo" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;

                let r = self
                    .client
                    .repos(owner, repo)
                    .get()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "name": r.name,
                    "full_name": r.full_name,
                    "description": r.description,
                    "private": r.private,
                    "language": r.language,
                    "stargazers_count": r.stargazers_count,
                    "forks_count": r.forks_count,
                    "open_issues_count": r.open_issues_count,
                }))
            }

            // ── 3. list_issues ───────────────────────────────────────────────
            "list_issues" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;

                let state_str = arguments["state"].as_str().unwrap_or("open");
                let state = parse_state(state_str)?;

                let page = self
                    .client
                    .issues(owner, repo)
                    .list()
                    .state(state)
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let issues: Vec<Value> = page
                    .items
                    .into_iter()
                    .map(|i| {
                        let labels: Vec<String> = i.labels.iter().map(|l| l.name.clone()).collect();
                        json!({
                            "number": i.number,
                            "title": i.title,
                            "state": format!("{:?}", i.state).to_lowercase(),
                            "body": i.body,
                            "html_url": i.html_url.to_string(),
                            "labels": labels,
                        })
                    })
                    .collect();

                Ok(json!({ "issues": issues }))
            }

            // ── 4. create_issue ──────────────────────────────────────────────
            "create_issue" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let title = arguments["title"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'title'".to_string()))?;

                let body_opt: Option<String> = arguments["body"].as_str().map(|s| s.to_string());
                let labels_opt: Option<Vec<String>> = arguments["labels"].as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                });

                // Bind the handler to keep it alive across builder chain
                let issue_handler = self.client.issues(owner, repo);
                let create_builder = issue_handler.create(title);
                let create_builder = match body_opt {
                    Some(b) => create_builder.body(b),
                    None => create_builder,
                };
                let create_builder = match labels_opt {
                    Some(l) => create_builder.labels(l),
                    None => create_builder,
                };

                let issue = create_builder
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "number": issue.number,
                    "title": issue.title,
                    "html_url": issue.html_url.to_string(),
                }))
            }

            // ── 5. list_prs ──────────────────────────────────────────────────
            "list_prs" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;

                let state_str = arguments["state"].as_str().unwrap_or("open");
                let state = parse_state(state_str)?;

                let page = self
                    .client
                    .pulls(owner, repo)
                    .list()
                    .state(state)
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let prs: Vec<Value> = page
                    .items
                    .into_iter()
                    .map(|pr| {
                        json!({
                            "number": pr.number,
                            "title": pr.title,
                            "state": pr.state.as_ref().map(|s| format!("{:?}", s).to_lowercase()),
                            "head_ref": pr.head.ref_field,
                            "base_ref": pr.base.ref_field,
                            "html_url": pr.html_url.as_ref().map(|u| u.to_string()),
                        })
                    })
                    .collect();

                Ok(json!({ "pull_requests": prs }))
            }

            // ── 6. create_pr ─────────────────────────────────────────────────
            "create_pr" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let title = arguments["title"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'title'".to_string()))?;
                let head = arguments["head"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'head'".to_string()))?;
                let base = arguments["base"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'base'".to_string()))?;
                let body_opt: Option<String> = arguments["body"].as_str().map(|s| s.to_string());

                // Bind the handler to keep it alive across builder chain
                let pulls_handler = self.client.pulls(owner, repo);
                let create_builder = pulls_handler.create(title, head, base);
                let create_builder = match body_opt {
                    Some(b) => create_builder.body(b),
                    None => create_builder,
                };

                let pr = create_builder
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "number": pr.number,
                    "title": pr.title,
                    "html_url": pr.html_url.as_ref().map(|u| u.to_string()),
                }))
            }

            // ── 7. get_file ──────────────────────────────────────────────────
            "get_file" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let path = arguments["path"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'path'".to_string()))?;
                let branch_opt: Option<String> =
                    arguments["branch"].as_str().map(|s| s.to_string());

                // Bind the repo handler to keep it alive across builder chain
                let repo_handler = self.client.repos(owner, repo);
                let content_builder = repo_handler.get_content().path(path);
                let content_builder = match branch_opt {
                    Some(branch) => content_builder.r#ref(branch),
                    None => content_builder,
                };

                let mut content_items = content_builder
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let items = content_items.take_items();
                let item = items.into_iter().next().ok_or_else(|| {
                    McpError::ToolExecutionError("No content returned for path".to_string())
                })?;

                // Decode base64 content if encoding indicates it
                let decoded_content = if item.encoding.as_deref() == Some("base64") {
                    item.decoded_content().ok_or_else(|| {
                        McpError::ToolExecutionError(
                            "Base64 decode failed: no content field".to_string(),
                        )
                    })?
                } else {
                    item.content.clone().unwrap_or_default()
                };

                Ok(json!({
                    "path": item.path,
                    "content": decoded_content,
                    "sha": item.sha,
                    "encoding": item.encoding,
                }))
            }

            // ── 8. search_code ───────────────────────────────────────────────
            "search_code" => {
                let query_base = arguments["query"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'query'".to_string()))?;

                // Build the final query string, optionally appending a language qualifier
                let effective_query: String = match arguments["language"].as_str() {
                    Some(lang) => format!("{} language:{}", query_base, lang),
                    None => query_base.to_string(),
                };

                let page = self
                    .client
                    .search()
                    .code(&effective_query)
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let results: Vec<Value> = page
                    .items
                    .into_iter()
                    .map(|c| {
                        json!({
                            "name": c.name,
                            "path": c.path,
                            "repository": c.repository.full_name,
                            "html_url": c.html_url.to_string(),
                        })
                    })
                    .collect();

                Ok(json!({ "results": results }))
            }

            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "list_repos",
                "description": "List repositories for a GitHub user or organisation",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "GitHub username or organisation name (uses GITHUB_DEFAULT_OWNER if omitted)"
                        }
                    }
                }
            }),
            json!({
                "name": "get_repo",
                "description": "Fetch details for a single GitHub repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner"
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name"
                        }
                    },
                    "required": ["repo"]
                }
            }),
            json!({
                "name": "list_issues",
                "description": "List issues in a GitHub repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner"
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name"
                        },
                        "state": {
                            "type": "string",
                            "enum": ["open", "closed", "all"],
                            "description": "Issue state filter (default: open)"
                        }
                    },
                    "required": ["repo"]
                }
            }),
            json!({
                "name": "create_issue",
                "description": "Create a new issue in a GitHub repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner"
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name"
                        },
                        "title": {
                            "type": "string",
                            "description": "Issue title"
                        },
                        "body": {
                            "type": "string",
                            "description": "Issue body text"
                        },
                        "labels": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Labels to apply to the issue"
                        }
                    },
                    "required": ["repo", "title"]
                }
            }),
            json!({
                "name": "list_prs",
                "description": "List pull requests in a GitHub repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner"
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name"
                        },
                        "state": {
                            "type": "string",
                            "enum": ["open", "closed", "all"],
                            "description": "Pull-request state filter (default: open)"
                        }
                    },
                    "required": ["repo"]
                }
            }),
            json!({
                "name": "create_pr",
                "description": "Create a pull request in a GitHub repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner"
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name"
                        },
                        "title": {
                            "type": "string",
                            "description": "Pull-request title"
                        },
                        "head": {
                            "type": "string",
                            "description": "The name of the branch where your changes are implemented"
                        },
                        "base": {
                            "type": "string",
                            "description": "The name of the branch you want the changes pulled into"
                        },
                        "body": {
                            "type": "string",
                            "description": "Pull-request description"
                        }
                    },
                    "required": ["repo", "title", "head", "base"]
                }
            }),
            json!({
                "name": "get_file",
                "description": "Retrieve the content of a file from a GitHub repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Repository owner"
                        },
                        "repo": {
                            "type": "string",
                            "description": "Repository name"
                        },
                        "path": {
                            "type": "string",
                            "description": "File path within the repository (e.g. src/main.rs)"
                        },
                        "branch": {
                            "type": "string",
                            "description": "Branch, tag or commit SHA (defaults to the repository's default branch)"
                        }
                    },
                    "required": ["repo", "path"]
                }
            }),
            json!({
                "name": "search_code",
                "description": "Search for code across GitHub repositories",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (GitHub code search syntax)"
                        },
                        "language": {
                            "type": "string",
                            "description": "Restrict results to a specific programming language"
                        }
                    },
                    "required": ["query"]
                }
            }),
        ])
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Map a state string to `octocrab::params::State`.
fn parse_state(state: &str) -> Result<octocrab::params::State> {
    match state {
        "open" => Ok(octocrab::params::State::Open),
        "closed" => Ok(octocrab::params::State::Closed),
        "all" => Ok(octocrab::params::State::All),
        other => Err(McpError::InvalidRequest(format!(
            "Invalid state '{}': expected one of open, closed, all",
            other
        ))),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Install the ring-based rustls crypto provider once per test process.
    /// `Octocrab::builder().build()` requires a process-level provider to
    /// be set before the TLS connector is created.  Calling this helper at
    /// the start of every test that constructs a `GitHubServer` avoids the
    /// panic that rustls raises when no provider has been registered.
    fn install_crypto_provider() {
        // Silently ignore the "already installed" error so tests can run in
        // parallel without racing on the global provider slot.
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    #[test]
    fn test_config_from_env_missing_token_errors() {
        // Ensure GITHUB_TOKEN is unset so from_env() must fail
        std::env::remove_var("GITHUB_TOKEN");
        let result = GitHubConfig::from_env();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_tools_returns_eight() {
        install_crypto_provider();
        let cfg = GitHubConfig::default();
        let server = GitHubServer::new(cfg).unwrap();
        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 8);
    }

    #[tokio::test]
    async fn test_call_tool_unknown_returns_error() {
        install_crypto_provider();
        let cfg = GitHubConfig::default();
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool("nonexistent_tool", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_repos_missing_owner_no_default() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server.call_tool("list_repos", serde_json::json!({})).await;
        // Without default_owner and no owner param the server must error
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_state_valid() {
        assert!(parse_state("open").is_ok());
        assert!(parse_state("closed").is_ok());
        assert!(parse_state("all").is_ok());
    }

    #[test]
    fn test_parse_state_invalid() {
        assert!(parse_state("unknown").is_err());
        assert!(parse_state("").is_err());
    }

    #[test]
    fn test_list_tools_all_names_unique() {
        install_crypto_provider();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let tools = runtime.block_on(async {
            let server = GitHubServer::new(GitHubConfig::default()).unwrap();
            server.list_tools().await.unwrap()
        });
        let names: std::collections::HashSet<&str> =
            tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names.len(), 8, "all tool names must be unique");
    }

    #[test]
    fn test_config_default_has_empty_token() {
        let cfg = GitHubConfig::default();
        assert!(cfg.token.is_empty());
        assert!(cfg.default_owner.is_none());
        assert_eq!(cfg.timeout_secs, 30);
    }
}
