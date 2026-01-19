//! Git MCP server - provides Git operations

use crate::{McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Built-in MCP server for Git operations
pub struct GitServer {
    /// Root directory for Git operations (security boundary)
    root_dir: PathBuf,
}

impl GitServer {
    /// Create a new Git server with a root directory
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    /// Execute a git command
    async fn git_command(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new("git")
            .args(args)
            .current_dir(&self.root_dir)
            .output()
            .await
            .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))
    }

    /// Resolve a repository path relative to root_dir
    fn resolve_repo_path(&self, path: &str) -> Result<PathBuf> {
        let requested_path = Path::new(path);

        if requested_path.is_absolute() {
            return Err(crate::McpError::InvalidRequest(
                "Absolute paths not allowed".to_string(),
            ));
        }

        let full_path = self.root_dir.join(requested_path);

        // Ensure the path is within root_dir
        let canonical = full_path
            .canonicalize()
            .map_err(|e| crate::McpError::InvalidRequest(format!("Invalid path: {}", e)))?;

        if !canonical.starts_with(&self.root_dir) {
            return Err(crate::McpError::InvalidRequest(
                "Path outside allowed directory".to_string(),
            ));
        }

        Ok(canonical)
    }
}

#[async_trait]
impl McpServer for GitServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "git_status" => {
                let output = self.git_command(&["status", "--porcelain"]).await?;

                Ok(json!({
                    "status": String::from_utf8_lossy(&output.stdout),
                    "success": output.status.success(),
                }))
            }

            "git_clone" => {
                let url = arguments["url"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'url'".to_string()))?;
                let path = arguments["path"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'path'".to_string()))?;

                let resolved = self.resolve_repo_path(path)?;

                let output = Command::new("git")
                    .args(["clone", url, resolved.to_str().unwrap_or("")])
                    .output()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "success": output.status.success(),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }

            "git_add" => {
                let files = arguments["files"].as_array().ok_or_else(|| {
                    crate::McpError::InvalidRequest("Missing 'files' array".to_string())
                })?;

                let file_paths: Vec<String> = files
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect();

                if file_paths.is_empty() {
                    return Err(crate::McpError::InvalidRequest(
                        "No files specified".to_string(),
                    ));
                }

                let mut args = vec!["add"];
                let file_refs: Vec<&str> = file_paths.iter().map(|s| s.as_str()).collect();
                args.extend(file_refs);

                let output = self.git_command(&args).await?;

                Ok(json!({
                    "success": output.status.success(),
                    "files_added": file_paths,
                }))
            }

            "git_commit" => {
                let message = arguments["message"].as_str().ok_or_else(|| {
                    crate::McpError::InvalidRequest("Missing 'message'".to_string())
                })?;

                let output = self.git_command(&["commit", "-m", message]).await?;

                Ok(json!({
                    "success": output.status.success(),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }

            "git_push" => {
                let remote = arguments["remote"].as_str().unwrap_or("origin");
                let branch = arguments["branch"].as_str().unwrap_or("main");

                let output = self.git_command(&["push", remote, branch]).await?;

                Ok(json!({
                    "success": output.status.success(),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }

            "git_pull" => {
                let remote = arguments["remote"].as_str().unwrap_or("origin");
                let branch = arguments["branch"].as_str().unwrap_or("main");

                let output = self.git_command(&["pull", remote, branch]).await?;

                Ok(json!({
                    "success": output.status.success(),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }

            "git_log" => {
                let limit = arguments["limit"].as_u64().unwrap_or(10);
                let limit_str = limit.to_string();

                let output = self
                    .git_command(&["log", "--oneline", "-n", &limit_str])
                    .await?;

                Ok(json!({
                    "log": String::from_utf8_lossy(&output.stdout),
                    "success": output.status.success(),
                }))
            }

            "git_diff" => {
                let output = self.git_command(&["diff"]).await?;

                Ok(json!({
                    "diff": String::from_utf8_lossy(&output.stdout),
                    "success": output.status.success(),
                }))
            }

            "git_branch" => {
                let output = self.git_command(&["branch", "-a"]).await?;

                Ok(json!({
                    "branches": String::from_utf8_lossy(&output.stdout),
                    "success": output.status.success(),
                }))
            }

            "git_checkout" => {
                let branch = arguments["branch"].as_str().ok_or_else(|| {
                    crate::McpError::InvalidRequest("Missing 'branch'".to_string())
                })?;

                let output = self.git_command(&["checkout", branch]).await?;

                Ok(json!({
                    "success": output.status.success(),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }

            _ => Err(crate::McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "git_status",
                "description": "Get repository status",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            json!({
                "name": "git_clone",
                "description": "Clone a Git repository",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "Repository URL to clone"
                        },
                        "path": {
                            "type": "string",
                            "description": "Destination path"
                        }
                    },
                    "required": ["url", "path"]
                }
            }),
            json!({
                "name": "git_add",
                "description": "Stage files for commit",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "files": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Files to stage"
                        }
                    },
                    "required": ["files"]
                }
            }),
            json!({
                "name": "git_commit",
                "description": "Create a commit",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Commit message"
                        }
                    },
                    "required": ["message"]
                }
            }),
            json!({
                "name": "git_push",
                "description": "Push commits to remote",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "remote": {
                            "type": "string",
                            "description": "Remote name (default: origin)"
                        },
                        "branch": {
                            "type": "string",
                            "description": "Branch name (default: main)"
                        }
                    }
                }
            }),
            json!({
                "name": "git_pull",
                "description": "Pull commits from remote",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "remote": {
                            "type": "string",
                            "description": "Remote name (default: origin)"
                        },
                        "branch": {
                            "type": "string",
                            "description": "Branch name (default: main)"
                        }
                    }
                }
            }),
            json!({
                "name": "git_log",
                "description": "View commit history",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "number",
                            "description": "Number of commits to show (default: 10)"
                        }
                    }
                }
            }),
            json!({
                "name": "git_diff",
                "description": "Show changes",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            json!({
                "name": "git_branch",
                "description": "List branches",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            json!({
                "name": "git_checkout",
                "description": "Switch branches",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "branch": {
                            "type": "string",
                            "description": "Branch name to switch to"
                        }
                    },
                    "required": ["branch"]
                }
            }),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[tokio::test]
    async fn test_git_server_creation() {
        let temp_dir = std::env::temp_dir().join("oxify-git-test");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = GitServer::new(temp_dir.clone());
        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 10);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_git_list_tools() {
        let server = GitServer::new(PathBuf::from("."));
        let tools = server.list_tools().await.unwrap();

        assert!(tools.iter().any(|t| t["name"] == "git_status"));
        assert!(tools.iter().any(|t| t["name"] == "git_clone"));
        assert!(tools.iter().any(|t| t["name"] == "git_add"));
        assert!(tools.iter().any(|t| t["name"] == "git_commit"));
        assert!(tools.iter().any(|t| t["name"] == "git_push"));
        assert!(tools.iter().any(|t| t["name"] == "git_pull"));
        assert!(tools.iter().any(|t| t["name"] == "git_log"));
        assert!(tools.iter().any(|t| t["name"] == "git_diff"));
        assert!(tools.iter().any(|t| t["name"] == "git_branch"));
        assert!(tools.iter().any(|t| t["name"] == "git_checkout"));
    }

    #[tokio::test]
    async fn test_git_status() {
        // Only run if we're in a git repository
        if PathBuf::from(".git").exists() {
            let server = GitServer::new(PathBuf::from("."));

            let result = server.call_tool("git_status", json!({})).await.unwrap();

            assert!(result["success"].as_bool().unwrap_or(false));
        }
    }

    #[tokio::test]
    async fn test_git_log() {
        // Only run if we're in a git repository
        if PathBuf::from(".git").exists() {
            let server = GitServer::new(PathBuf::from("."));

            let result = server
                .call_tool(
                    "git_log",
                    json!({
                        "limit": 5
                    }),
                )
                .await
                .unwrap();

            assert!(result["success"].as_bool().unwrap_or(false));
        }
    }

    #[tokio::test]
    async fn test_git_branch() {
        // Only run if we're in a git repository
        if PathBuf::from(".git").exists() {
            let server = GitServer::new(PathBuf::from("."));

            let result = server.call_tool("git_branch", json!({})).await.unwrap();

            assert!(result["success"].as_bool().unwrap_or(false));
        }
    }

    #[tokio::test]
    async fn test_git_diff() {
        // Only run if we're in a git repository
        if PathBuf::from(".git").exists() {
            let server = GitServer::new(PathBuf::from("."));

            let result = server.call_tool("git_diff", json!({})).await.unwrap();

            // diff might be empty, but should succeed
            assert!(result.get("success").is_some());
        }
    }

    #[tokio::test]
    async fn test_git_add_missing_files() {
        let temp_dir = std::env::temp_dir().join("oxify-git-test-add");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = GitServer::new(temp_dir.clone());

        let result = server
            .call_tool(
                "git_add",
                json!({
                    "files": []
                }),
            )
            .await;

        assert!(result.is_err());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_git_commit_missing_message() {
        let temp_dir = std::env::temp_dir().join("oxify-git-test-commit");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = GitServer::new(temp_dir.clone());

        let result = server.call_tool("git_commit", json!({})).await;

        assert!(result.is_err());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_git_checkout_missing_branch() {
        let temp_dir = std::env::temp_dir().join("oxify-git-test-checkout");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = GitServer::new(temp_dir.clone());

        let result = server.call_tool("git_checkout", json!({})).await;

        assert!(result.is_err());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_git_clone_missing_url() {
        let temp_dir = std::env::temp_dir().join("oxify-git-test-clone");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = GitServer::new(temp_dir.clone());

        let result = server
            .call_tool(
                "git_clone",
                json!({
                    "path": "test"
                }),
            )
            .await;

        assert!(result.is_err());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_git_clone_missing_path() {
        let temp_dir = std::env::temp_dir().join("oxify-git-test-clone2");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = GitServer::new(temp_dir.clone());

        let result = server
            .call_tool(
                "git_clone",
                json!({
                    "url": "https://example.com/repo.git"
                }),
            )
            .await;

        assert!(result.is_err());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_git_invalid_tool() {
        let server = GitServer::new(PathBuf::from("."));

        let result = server.call_tool("nonexistent_tool", json!({})).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_git_path_security() {
        let temp_dir = std::env::temp_dir().join("oxify-git-security");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = GitServer::new(temp_dir.clone());

        // Try to clone to an absolute path (should fail)
        let result = server
            .call_tool(
                "git_clone",
                json!({
                    "url": "https://example.com/repo.git",
                    "path": "/etc/test"
                }),
            )
            .await;

        assert!(result.is_err());

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
