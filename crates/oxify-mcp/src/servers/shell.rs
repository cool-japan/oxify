//! Shell MCP server - provides safe shell command execution

use crate::{McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;

/// Built-in MCP server for shell command execution
pub struct ShellServer {
    /// Allowed commands (whitelist for security)
    allowed_commands: Vec<String>,
    /// Working directory for commands
    working_dir: std::path::PathBuf,
    /// Environment variables to set
    env_vars: Vec<(String, String)>,
}

impl ShellServer {
    /// Create a new shell server with allowed commands
    pub fn new(allowed_commands: Vec<String>) -> Self {
        Self {
            allowed_commands,
            working_dir: std::env::current_dir().unwrap_or_else(|_| "/tmp".into()),
            env_vars: Vec::new(),
        }
    }

    /// Set the working directory
    pub fn with_working_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.working_dir = dir;
        self
    }

    /// Add environment variable
    pub fn with_env(mut self, key: String, value: String) -> Self {
        self.env_vars.push((key, value));
        self
    }

    /// Check if a command is allowed
    fn is_command_allowed(&self, command: &str) -> bool {
        // Extract the base command (first word)
        let base_cmd = command.split_whitespace().next().unwrap_or("");

        // Allow if whitelist is empty (unrestricted mode) or if command is in whitelist
        self.allowed_commands.is_empty() || self.allowed_commands.contains(&base_cmd.to_string())
    }
}

impl Default for ShellServer {
    fn default() -> Self {
        // Default with common safe commands
        Self::new(vec![
            "ls".to_string(),
            "cat".to_string(),
            "echo".to_string(),
            "pwd".to_string(),
            "date".to_string(),
            "whoami".to_string(),
            "grep".to_string(),
            "find".to_string(),
            "wc".to_string(),
            "head".to_string(),
            "tail".to_string(),
        ])
    }
}

#[async_trait]
impl McpServer for ShellServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "shell_exec" => {
                let command = arguments["command"].as_str().ok_or_else(|| {
                    crate::McpError::InvalidRequest("Missing 'command'".to_string())
                })?;

                // Security check
                if !self.is_command_allowed(command) {
                    return Err(crate::McpError::ToolExecutionError(format!(
                        "Command '{}' is not allowed. Allowed commands: {:?}",
                        command, self.allowed_commands
                    )));
                }

                // Execute command
                let output = if cfg!(target_os = "windows") {
                    Command::new("cmd")
                        .args(["/C", command])
                        .current_dir(&self.working_dir)
                        .envs(self.env_vars.iter().cloned())
                        .output()
                        .await
                } else {
                    Command::new("sh")
                        .arg("-c")
                        .arg(command)
                        .current_dir(&self.working_dir)
                        .envs(self.env_vars.iter().cloned())
                        .output()
                        .await
                }
                .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                    "exit_code": output.status.code().unwrap_or(-1),
                    "success": output.status.success(),
                }))
            }

            "shell_which" => {
                let command = arguments["command"].as_str().ok_or_else(|| {
                    crate::McpError::InvalidRequest("Missing 'command'".to_string())
                })?;

                let output = if cfg!(target_os = "windows") {
                    Command::new("where").arg(command).output().await
                } else {
                    Command::new("which").arg(command).output().await
                }
                .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "path": String::from_utf8_lossy(&output.stdout).trim(),
                    "found": output.status.success(),
                }))
            }

            _ => Err(crate::McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "shell_exec",
                "description": "Execute a shell command",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Shell command to execute"
                        }
                    },
                    "required": ["command"]
                }
            }),
            json!({
                "name": "shell_which",
                "description": "Find the path of a command",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Command name to locate"
                        }
                    },
                    "required": ["command"]
                }
            }),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_shell_exec_echo() {
        let server = ShellServer::default();

        let result = server
            .call_tool(
                "shell_exec",
                json!({
                    "command": "echo hello"
                }),
            )
            .await
            .unwrap();

        assert_eq!(result["success"], true);
        assert!(result["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_shell_exec_pwd() {
        let server = ShellServer::default();

        let result = server
            .call_tool(
                "shell_exec",
                json!({
                    "command": "pwd"
                }),
            )
            .await
            .unwrap();

        assert_eq!(result["success"], true);
        assert!(!result["stdout"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_shell_which() {
        let server = ShellServer::default();

        let result = server
            .call_tool(
                "shell_which",
                json!({
                    "command": "ls"
                }),
            )
            .await
            .unwrap();

        // On most Unix systems, 'ls' should be found
        if cfg!(not(target_os = "windows")) {
            assert_eq!(result["found"], true);
            assert!(!result["path"].as_str().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn test_shell_disallowed_command() {
        let server = ShellServer::new(vec!["echo".to_string()]);

        let result = server
            .call_tool(
                "shell_exec",
                json!({
                    "command": "rm -rf /"
                }),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_list_tools() {
        let server = ShellServer::default();

        let tools = server.list_tools().await.unwrap();

        assert_eq!(tools.len(), 2);
        assert!(tools.iter().any(|t| t["name"] == "shell_exec"));
        assert!(tools.iter().any(|t| t["name"] == "shell_which"));
    }

    #[tokio::test]
    async fn test_shell_with_working_dir() {
        let temp_dir = std::env::temp_dir();
        let server = ShellServer::default().with_working_dir(temp_dir);

        let result = server
            .call_tool(
                "shell_exec",
                json!({
                    "command": "pwd"
                }),
            )
            .await
            .unwrap();

        assert_eq!(result["success"], true);
    }

    #[tokio::test]
    async fn test_shell_with_env() {
        let server =
            ShellServer::default().with_env("TEST_VAR".to_string(), "test_value".to_string());

        let result = if cfg!(target_os = "windows") {
            server
                .call_tool(
                    "shell_exec",
                    json!({
                        "command": "echo %TEST_VAR%"
                    }),
                )
                .await
        } else {
            server
                .call_tool(
                    "shell_exec",
                    json!({
                        "command": "echo $TEST_VAR"
                    }),
                )
                .await
        };

        if let Ok(result) = result {
            assert_eq!(result["success"], true);
        }
    }
}
