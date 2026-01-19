//! Filesystem MCP server - provides file operations

use crate::{McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Built-in MCP server for filesystem operations
pub struct FilesystemServer {
    /// Root directory for operations (security boundary)
    root_dir: PathBuf,
    /// Whether to allow operations outside root_dir
    allow_absolute_paths: bool,
}

impl FilesystemServer {
    /// Create a new filesystem server with a root directory
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            allow_absolute_paths: false,
        }
    }

    /// Allow operations with absolute paths (security risk)
    pub fn allow_absolute_paths(mut self, allow: bool) -> Self {
        self.allow_absolute_paths = allow;
        self
    }

    /// Resolve a path relative to root_dir
    fn resolve_path(&self, path: &str) -> Result<PathBuf> {
        let requested_path = Path::new(path);

        if !self.allow_absolute_paths && requested_path.is_absolute() {
            return Err(crate::McpError::InvalidRequest(
                "Absolute paths not allowed".to_string(),
            ));
        }

        let full_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            self.root_dir.join(requested_path)
        };

        // Try to canonicalize the path if it exists, otherwise validate the parent directory
        let canonical = if full_path.exists() {
            full_path
                .canonicalize()
                .map_err(|e| crate::McpError::InvalidRequest(format!("Invalid path: {}", e)))?
        } else {
            // For non-existent paths, canonicalize the root dir and append the relative path
            let canonical_root = self.root_dir.canonicalize().map_err(|e| {
                crate::McpError::InvalidRequest(format!("Invalid root directory: {}", e))
            })?;

            if requested_path.is_absolute() {
                full_path
            } else {
                canonical_root.join(requested_path)
            }
        };

        // Ensure the path is within root_dir (after canonicalization of root)
        if !self.allow_absolute_paths {
            let canonical_root = self.root_dir.canonicalize().map_err(|e| {
                crate::McpError::InvalidRequest(format!("Invalid root directory: {}", e))
            })?;

            if !canonical.starts_with(&canonical_root) {
                return Err(crate::McpError::InvalidRequest(
                    "Path outside allowed directory".to_string(),
                ));
            }
        }

        Ok(canonical)
    }
}

#[async_trait]
impl McpServer for FilesystemServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "fs_read" => {
                let path = arguments["path"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'path'".to_string()))?;

                let resolved = self.resolve_path(path)?;
                let content = fs::read_to_string(&resolved)
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "content": content,
                    "path": resolved.to_string_lossy(),
                }))
            }

            "fs_write" => {
                let path = arguments["path"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'path'".to_string()))?;
                let content = arguments["content"].as_str().ok_or_else(|| {
                    crate::McpError::InvalidRequest("Missing 'content'".to_string())
                })?;

                let resolved = self.resolve_path(path)?;

                // Create parent directories if needed
                if let Some(parent) = resolved.parent() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;
                }

                fs::write(&resolved, content)
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "success": true,
                    "path": resolved.to_string_lossy(),
                    "bytes_written": content.len(),
                }))
            }

            "fs_list" => {
                let path = arguments["path"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'path'".to_string()))?;

                let resolved = self.resolve_path(path)?;
                let mut entries = Vec::new();

                let mut dir = fs::read_dir(&resolved)
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                while let Some(entry) = dir
                    .next_entry()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?
                {
                    let metadata = entry
                        .metadata()
                        .await
                        .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                    entries.push(json!({
                        "name": entry.file_name().to_string_lossy(),
                        "is_dir": metadata.is_dir(),
                        "is_file": metadata.is_file(),
                        "size": metadata.len(),
                    }));
                }

                Ok(json!({
                    "path": resolved.to_string_lossy(),
                    "entries": entries,
                }))
            }

            "fs_delete" => {
                let path = arguments["path"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'path'".to_string()))?;

                let resolved = self.resolve_path(path)?;
                let metadata = fs::metadata(&resolved)
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                if metadata.is_dir() {
                    fs::remove_dir_all(&resolved)
                        .await
                        .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;
                } else {
                    fs::remove_file(&resolved)
                        .await
                        .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;
                }

                Ok(json!({
                    "success": true,
                    "deleted": resolved.to_string_lossy(),
                }))
            }

            "fs_exists" => {
                let path = arguments["path"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'path'".to_string()))?;

                let resolved = self.resolve_path(path)?;
                let exists = resolved.exists();

                Ok(json!({
                    "exists": exists,
                    "path": resolved.to_string_lossy(),
                }))
            }

            _ => Err(crate::McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "fs_read",
                "description": "Read file contents",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to file"
                        }
                    },
                    "required": ["path"]
                }
            }),
            json!({
                "name": "fs_write",
                "description": "Write content to file",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to file"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write"
                        }
                    },
                    "required": ["path", "content"]
                }
            }),
            json!({
                "name": "fs_list",
                "description": "List directory contents",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path"
                        }
                    },
                    "required": ["path"]
                }
            }),
            json!({
                "name": "fs_delete",
                "description": "Delete file or directory",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to delete"
                        }
                    },
                    "required": ["path"]
                }
            }),
            json!({
                "name": "fs_exists",
                "description": "Check if path exists",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to check"
                        }
                    },
                    "required": ["path"]
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
    async fn test_fs_write_and_read() {
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = FilesystemServer::new(temp_dir.clone());

        // Write a file
        let write_result = server
            .call_tool(
                "fs_write",
                json!({
                    "path": "test.txt",
                    "content": "Hello, OxiFY!"
                }),
            )
            .await
            .unwrap();

        assert_eq!(write_result["success"], true);

        // Read the file
        let read_result = server
            .call_tool(
                "fs_read",
                json!({
                    "path": "test.txt"
                }),
            )
            .await
            .unwrap();

        assert_eq!(read_result["content"], "Hello, OxiFY!");

        // Cleanup
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_fs_list() {
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-list");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = FilesystemServer::new(temp_dir.clone());

        // Write some files
        server
            .call_tool(
                "fs_write",
                json!({
                    "path": "file1.txt",
                    "content": "File 1"
                }),
            )
            .await
            .unwrap();

        server
            .call_tool(
                "fs_write",
                json!({
                    "path": "file2.txt",
                    "content": "File 2"
                }),
            )
            .await
            .unwrap();

        // List directory
        let list_result = server
            .call_tool(
                "fs_list",
                json!({
                    "path": "."
                }),
            )
            .await
            .unwrap();

        let entries = list_result["entries"].as_array().unwrap();
        assert!(entries.len() >= 2);

        // Cleanup
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_fs_exists() {
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-exists");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = FilesystemServer::new(temp_dir.clone());

        // Test non-existent file
        let exists_result = server
            .call_tool(
                "fs_exists",
                json!({
                    "path": "nonexistent.txt"
                }),
            )
            .await
            .unwrap();

        assert_eq!(exists_result["exists"], false);

        // Write a file
        server
            .call_tool(
                "fs_write",
                json!({
                    "path": "exists.txt",
                    "content": "I exist!"
                }),
            )
            .await
            .unwrap();

        // Test existing file
        let exists_result = server
            .call_tool(
                "fs_exists",
                json!({
                    "path": "exists.txt"
                }),
            )
            .await
            .unwrap();

        assert_eq!(exists_result["exists"], true);

        // Cleanup
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_fs_delete() {
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-delete");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = FilesystemServer::new(temp_dir.clone());

        // Write a file
        server
            .call_tool(
                "fs_write",
                json!({
                    "path": "delete_me.txt",
                    "content": "Delete this!"
                }),
            )
            .await
            .unwrap();

        // Delete the file
        let delete_result = server
            .call_tool(
                "fs_delete",
                json!({
                    "path": "delete_me.txt"
                }),
            )
            .await
            .unwrap();

        assert_eq!(delete_result["success"], true);

        // Verify file is gone
        let exists_result = server
            .call_tool(
                "fs_exists",
                json!({
                    "path": "delete_me.txt"
                }),
            )
            .await
            .unwrap();

        assert_eq!(exists_result["exists"], false);

        // Cleanup
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_fs_list_tools() {
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-tools");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = FilesystemServer::new(temp_dir.clone());

        let tools = server.list_tools().await.unwrap();

        assert_eq!(tools.len(), 5);
        assert!(tools.iter().any(|t| t["name"] == "fs_read"));
        assert!(tools.iter().any(|t| t["name"] == "fs_write"));
        assert!(tools.iter().any(|t| t["name"] == "fs_list"));
        assert!(tools.iter().any(|t| t["name"] == "fs_delete"));
        assert!(tools.iter().any(|t| t["name"] == "fs_exists"));

        // Cleanup
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_absolute_path_security() {
        let temp_dir = std::env::temp_dir().join("oxify-mcp-test-security");
        fs::create_dir_all(&temp_dir).unwrap();

        let server = FilesystemServer::new(temp_dir.clone());

        // Try to read with absolute path (should fail)
        let result = server
            .call_tool(
                "fs_read",
                json!({
                    "path": "/etc/passwd"
                }),
            )
            .await;

        assert!(result.is_err());

        // Cleanup
        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
