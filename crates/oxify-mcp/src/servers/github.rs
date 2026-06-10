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
                            "head_ref": pr.head.as_deref().map(|h| h.ref_field.as_str()),
                            "base_ref": pr.base.as_deref().map(|b| b.ref_field.as_str()),
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

            // ── 9. get_issue ─────────────────────────────────────────────────
            "get_issue" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let issue_number = arguments["issue_number"].as_u64().ok_or_else(|| {
                    McpError::InvalidRequest("Missing 'issue_number'".to_string())
                })?;

                let issue = self
                    .client
                    .issues(owner, repo)
                    .get(issue_number)
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let labels: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();
                Ok(json!({
                    "number": issue.number,
                    "title": issue.title,
                    "state": format!("{:?}", issue.state).to_lowercase(),
                    "body": issue.body,
                    "html_url": issue.html_url.to_string(),
                    "user": issue.user.login,
                    "labels": labels,
                    "created_at": issue.created_at.to_rfc3339(),
                    "updated_at": issue.updated_at.to_rfc3339(),
                    "closed_at": issue.closed_at.map(|t| t.to_rfc3339()),
                }))
            }

            // ── 10. comment_on_issue ─────────────────────────────────────────
            "comment_on_issue" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let issue_number = arguments["issue_number"].as_u64().ok_or_else(|| {
                    McpError::InvalidRequest("Missing 'issue_number'".to_string())
                })?;
                let body = arguments["body"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'body'".to_string()))?;

                let comment = self
                    .client
                    .issues(owner, repo)
                    .create_comment(issue_number, body)
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "id": comment.id,
                    "html_url": comment.html_url.to_string(),
                    "body": comment.body,
                    "user": comment.user.login,
                    "created_at": comment.created_at.to_rfc3339(),
                }))
            }

            // ── 11. close_issue ──────────────────────────────────────────────
            "close_issue" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let issue_number = arguments["issue_number"].as_u64().ok_or_else(|| {
                    McpError::InvalidRequest("Missing 'issue_number'".to_string())
                })?;
                let state_str = arguments["state"].as_str().unwrap_or("closed");

                let issue_state = parse_issue_state(state_str)?;

                let issue = self
                    .client
                    .issues(owner, repo)
                    .update(issue_number)
                    .state(issue_state)
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let labels: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();
                Ok(json!({
                    "number": issue.number,
                    "title": issue.title,
                    "state": format!("{:?}", issue.state).to_lowercase(),
                    "html_url": issue.html_url.to_string(),
                    "labels": labels,
                }))
            }

            // ── 12. get_pr ───────────────────────────────────────────────────
            "get_pr" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let pr_number = arguments["pr_number"]
                    .as_u64()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'pr_number'".to_string()))?;

                let pr = self
                    .client
                    .pulls(owner, repo)
                    .get(pr_number)
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "number": pr.number,
                    "title": pr.title,
                    "state": pr.state.as_ref().map(|s| format!("{:?}", s).to_lowercase()),
                    "body": pr.body,
                    "head_ref": pr.head.as_deref().map(|h| h.ref_field.as_str()),
                    "base_ref": pr.base.as_deref().map(|b| b.ref_field.as_str()),
                    "html_url": pr.html_url.as_ref().map(|u| u.to_string()),
                    "merged": pr.merged,
                    "merged_at": pr.merged_at.map(|t| t.to_rfc3339()),
                    "user": pr.user.as_ref().map(|u| u.login.as_str()),
                }))
            }

            // ── 13. merge_pr ─────────────────────────────────────────────────
            "merge_pr" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let pr_number = arguments["pr_number"]
                    .as_u64()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'pr_number'".to_string()))?;
                let merge_method_str = arguments["merge_method"].as_str().unwrap_or("merge");
                let commit_title_opt: Option<String> =
                    arguments["commit_title"].as_str().map(|s| s.to_string());

                let merge_method = parse_merge_method(merge_method_str)?;

                let pulls_handler = self.client.pulls(owner, repo);
                let merge_builder = pulls_handler.merge(pr_number).method(merge_method);
                let merge_builder = match commit_title_opt {
                    Some(t) => merge_builder.title(t),
                    None => merge_builder,
                };

                let merge_result = merge_builder
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "merged": merge_result.merged,
                    "sha": merge_result.sha,
                    "message": merge_result.message,
                }))
            }

            // ── 14. list_commits ─────────────────────────────────────────────
            "list_commits" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let branch_opt: Option<String> =
                    arguments["branch"].as_str().map(|s| s.to_string());
                let per_page_opt: Option<u8> =
                    arguments["per_page"].as_u64().map(|v| v.min(100) as u8);

                let repo_handler = self.client.repos(owner, repo);
                let commits_builder = repo_handler.list_commits();
                let commits_builder = match branch_opt {
                    Some(branch) => commits_builder.branch(branch),
                    None => commits_builder,
                };
                let commits_builder = match per_page_opt {
                    Some(pp) => commits_builder.per_page(pp),
                    None => commits_builder,
                };

                let page = commits_builder
                    .send()
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let commits: Vec<Value> = page
                    .items
                    .into_iter()
                    .map(|c| {
                        json!({
                            "sha": c.sha,
                            "message": c.commit.message,
                            "author": c.commit.author.as_ref().map(|a| a.name.as_str()),
                            "url": c.html_url,
                        })
                    })
                    .collect();

                Ok(json!({ "commits": commits }))
            }

            // ── 15. get_commit ───────────────────────────────────────────────
            "get_commit" => {
                let owner = self.cfg.resolve_owner(&arguments)?;
                let repo = arguments["repo"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'repo'".to_string()))?;
                let sha = arguments["sha"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("Missing 'sha'".to_string()))?;

                let commit = self
                    .client
                    .commits(owner, repo)
                    .get(sha)
                    .await
                    .map_err(|e| McpError::ToolExecutionError(e.to_string()))?;

                let files: Vec<Value> = commit
                    .files
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(|f| {
                        json!({
                            "filename": f.filename,
                            "status": format!("{:?}", f.status).to_lowercase(),
                            "additions": f.additions,
                            "deletions": f.deletions,
                            "changes": f.changes,
                        })
                    })
                    .collect();

                Ok(json!({
                    "sha": commit.sha,
                    "message": commit.commit.message,
                    "author": commit.commit.author.as_ref().map(|a| a.name.as_str()),
                    "url": commit.html_url,
                    "stats": commit.stats.as_ref().map(|s| json!({
                        "additions": s.additions,
                        "deletions": s.deletions,
                        "total": s.total,
                    })),
                    "files": files,
                }))
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
            json!({
                "name": "get_issue",
                "description": "Fetch a single issue by number from a GitHub repository",
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
                        "issue_number": {
                            "type": "integer",
                            "description": "The issue number"
                        }
                    },
                    "required": ["repo", "issue_number"]
                }
            }),
            json!({
                "name": "comment_on_issue",
                "description": "Post a comment on a GitHub issue",
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
                        "issue_number": {
                            "type": "integer",
                            "description": "The issue number to comment on"
                        },
                        "body": {
                            "type": "string",
                            "description": "The comment text"
                        }
                    },
                    "required": ["repo", "issue_number", "body"]
                }
            }),
            json!({
                "name": "close_issue",
                "description": "Close or reopen a GitHub issue",
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
                        "issue_number": {
                            "type": "integer",
                            "description": "The issue number"
                        },
                        "state": {
                            "type": "string",
                            "enum": ["closed", "open"],
                            "description": "Target state: 'closed' to close, 'open' to reopen (default: closed)"
                        }
                    },
                    "required": ["repo", "issue_number"]
                }
            }),
            json!({
                "name": "get_pr",
                "description": "Fetch a single pull request by number from a GitHub repository",
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
                        "pr_number": {
                            "type": "integer",
                            "description": "The pull request number"
                        }
                    },
                    "required": ["repo", "pr_number"]
                }
            }),
            json!({
                "name": "merge_pr",
                "description": "Merge a pull request in a GitHub repository",
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
                        "pr_number": {
                            "type": "integer",
                            "description": "The pull request number to merge"
                        },
                        "merge_method": {
                            "type": "string",
                            "enum": ["merge", "squash", "rebase"],
                            "description": "Merge strategy (default: merge)"
                        },
                        "commit_title": {
                            "type": "string",
                            "description": "Title for the automatic commit message (optional)"
                        }
                    },
                    "required": ["repo", "pr_number"]
                }
            }),
            json!({
                "name": "list_commits",
                "description": "List commits in a GitHub repository, optionally filtered by branch",
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
                        "branch": {
                            "type": "string",
                            "description": "Branch or SHA to start listing commits from (defaults to default branch)"
                        },
                        "per_page": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 100,
                            "description": "Number of commits to return per page (default: 30, max: 100)"
                        }
                    },
                    "required": ["repo"]
                }
            }),
            json!({
                "name": "get_commit",
                "description": "Fetch a single commit by SHA with full diff stats from a GitHub repository",
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
                        "sha": {
                            "type": "string",
                            "description": "The commit SHA"
                        }
                    },
                    "required": ["repo", "sha"]
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

/// Map a state string to `octocrab::models::IssueState`.
fn parse_issue_state(state: &str) -> Result<octocrab::models::IssueState> {
    match state {
        "open" => Ok(octocrab::models::IssueState::Open),
        "closed" => Ok(octocrab::models::IssueState::Closed),
        other => Err(McpError::InvalidRequest(format!(
            "Invalid issue state '{}': expected one of open, closed",
            other
        ))),
    }
}

/// Map a merge method string to `octocrab::params::pulls::MergeMethod`.
fn parse_merge_method(method: &str) -> Result<octocrab::params::pulls::MergeMethod> {
    match method {
        "merge" => Ok(octocrab::params::pulls::MergeMethod::Merge),
        "squash" => Ok(octocrab::params::pulls::MergeMethod::Squash),
        "rebase" => Ok(octocrab::params::pulls::MergeMethod::Rebase),
        other => Err(McpError::InvalidRequest(format!(
            "Invalid merge method '{}': expected one of merge, squash, rebase",
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
    async fn test_list_tools_returns_fifteen() {
        install_crypto_provider();
        let cfg = GitHubConfig::default();
        let server = GitHubServer::new(cfg).unwrap();
        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 15);
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
        assert_eq!(names.len(), 15, "all tool names must be unique");
    }

    #[test]
    fn test_config_default_has_empty_token() {
        let cfg = GitHubConfig::default();
        assert!(cfg.token.is_empty());
        assert!(cfg.default_owner.is_none());
        assert_eq!(cfg.timeout_secs, 30);
    }

    // ── New tool input-validation tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_get_issue_missing_repo_errors() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            default_owner: Some("octocat".to_string()),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool("get_issue", serde_json::json!({ "issue_number": 1 }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_issue_missing_number_errors() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            default_owner: Some("octocat".to_string()),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool("get_issue", serde_json::json!({ "repo": "hello-world" }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_comment_on_issue_missing_body_errors() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            default_owner: Some("octocat".to_string()),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool(
                "comment_on_issue",
                serde_json::json!({ "repo": "hello-world", "issue_number": 1 }),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_close_issue_invalid_state_errors() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            default_owner: Some("octocat".to_string()),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool(
                "close_issue",
                serde_json::json!({
                    "repo": "hello-world",
                    "issue_number": 1,
                    "state": "all"
                }),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_pr_missing_number_errors() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            default_owner: Some("octocat".to_string()),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool("get_pr", serde_json::json!({ "repo": "hello-world" }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_merge_pr_invalid_method_errors() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            default_owner: Some("octocat".to_string()),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool(
                "merge_pr",
                serde_json::json!({
                    "repo": "hello-world",
                    "pr_number": 1,
                    "merge_method": "invalid"
                }),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_commits_missing_repo_errors() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            default_owner: Some("octocat".to_string()),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool("list_commits", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_commit_missing_sha_errors() {
        install_crypto_provider();
        let cfg = GitHubConfig {
            token: "fake".to_string(),
            default_owner: Some("octocat".to_string()),
            ..Default::default()
        };
        let server = GitHubServer::new(cfg).unwrap();
        let result = server
            .call_tool("get_commit", serde_json::json!({ "repo": "hello-world" }))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_issue_state_valid() {
        assert!(parse_issue_state("open").is_ok());
        assert!(parse_issue_state("closed").is_ok());
    }

    #[test]
    fn test_parse_issue_state_invalid() {
        assert!(parse_issue_state("all").is_err());
        assert!(parse_issue_state("unknown").is_err());
        assert!(parse_issue_state("").is_err());
    }

    #[test]
    fn test_parse_merge_method_valid() {
        assert!(parse_merge_method("merge").is_ok());
        assert!(parse_merge_method("squash").is_ok());
        assert!(parse_merge_method("rebase").is_ok());
    }

    #[test]
    fn test_parse_merge_method_invalid() {
        assert!(parse_merge_method("fast-forward").is_err());
        assert!(parse_merge_method("").is_err());
    }
}
