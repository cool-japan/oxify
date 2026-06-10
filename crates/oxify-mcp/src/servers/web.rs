//! Web MCP server - provides HTTP and web scraping operations

use crate::{McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

/// Built-in MCP server for web operations
pub struct WebServer {
    client: reqwest::Client,
    /// Maximum response size in bytes (default: 10MB)
    max_response_size: usize,
}

impl WebServer {
    /// Create a new web server
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("OxiFY-MCP/0.1.0")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest::Client::builder() with default settings should not fail"),
            max_response_size: 10 * 1024 * 1024, // 10MB
        }
    }

    /// Set maximum response size
    pub fn with_max_response_size(mut self, size: usize) -> Self {
        self.max_response_size = size;
        self
    }
}

impl Default for WebServer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl McpServer for WebServer {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "http_get" => {
                let url = arguments["url"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'url'".to_string()))?;

                let response = self
                    .client
                    .get(url)
                    .send()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                let status = response.status().as_u16();
                let headers: Vec<(String, String)> = response
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();

                let body = response
                    .text()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                // Truncate if too large
                let body = if body.len() > self.max_response_size {
                    format!("{}...[truncated]", &body[..self.max_response_size])
                } else {
                    body
                };

                Ok(json!({
                    "status": status,
                    "headers": headers,
                    "body": body,
                }))
            }

            "http_post" => {
                let url = arguments["url"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'url'".to_string()))?;
                let body = arguments["body"].as_str().unwrap_or("");
                let content_type = arguments["content_type"]
                    .as_str()
                    .unwrap_or("application/json");

                let response = self
                    .client
                    .post(url)
                    .header("Content-Type", content_type)
                    .body(body.to_string())
                    .send()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                let status = response.status().as_u16();
                let response_body = response
                    .text()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                Ok(json!({
                    "status": status,
                    "body": response_body,
                }))
            }

            "web_scrape" => {
                let url = arguments["url"]
                    .as_str()
                    .ok_or_else(|| crate::McpError::InvalidRequest("Missing 'url'".to_string()))?;
                let selector = arguments.get("selector").and_then(|v| v.as_str());

                let response = self
                    .client
                    .get(url)
                    .send()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                let html = response
                    .text()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                // Basic HTML to text conversion (simple implementation)
                // In production, use a proper HTML parser like scraper or html2text
                let text = if let Some(_css_selector) = selector {
                    // TODO: Implement CSS selector parsing with scraper crate
                    html
                } else {
                    // Simple HTML tag removal
                    html.replace("<script", "\n<script")
                        .replace("<style", "\n<style")
                        .lines()
                        .filter(|line| !line.trim_start().starts_with("<script"))
                        .filter(|line| !line.trim_start().starts_with("<style"))
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                Ok(json!({
                    "url": url,
                    "text": text,
                    "length": text.len(),
                }))
            }

            "web_screenshot" => {
                // TODO: Implement headless browser screenshot
                // Requires puppeteer/playwright integration
                Err(crate::McpError::ToolExecutionError(
                    "Screenshot not yet implemented. Requires headless browser.".to_string(),
                ))
            }

            _ => Err(crate::McpError::ToolNotFound(name.to_string())),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Value>> {
        Ok(vec![
            json!({
                "name": "http_get",
                "description": "Perform HTTP GET request",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "URL to fetch"
                        }
                    },
                    "required": ["url"]
                }
            }),
            json!({
                "name": "http_post",
                "description": "Perform HTTP POST request",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "URL to post to"
                        },
                        "body": {
                            "type": "string",
                            "description": "Request body"
                        },
                        "content_type": {
                            "type": "string",
                            "description": "Content-Type header",
                            "default": "application/json"
                        }
                    },
                    "required": ["url"]
                }
            }),
            json!({
                "name": "web_scrape",
                "description": "Scrape web page content",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "URL to scrape"
                        },
                        "selector": {
                            "type": "string",
                            "description": "CSS selector (optional)"
                        }
                    },
                    "required": ["url"]
                }
            }),
            json!({
                "name": "web_screenshot",
                "description": "Take screenshot of web page (not yet implemented)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "URL to screenshot"
                        }
                    },
                    "required": ["url"]
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
    async fn test_web_server_creation() {
        let server = WebServer::new();
        let tools = server.list_tools().await.unwrap();
        assert_eq!(tools.len(), 4);
    }

    #[tokio::test]
    async fn test_web_server_with_max_response_size() {
        let server = WebServer::new().with_max_response_size(1024);
        assert_eq!(server.max_response_size, 1024);
    }

    #[tokio::test]
    async fn test_web_list_tools() {
        let server = WebServer::new();
        let tools = server.list_tools().await.unwrap();

        assert!(tools.iter().any(|t| t["name"] == "http_get"));
        assert!(tools.iter().any(|t| t["name"] == "http_post"));
        assert!(tools.iter().any(|t| t["name"] == "web_scrape"));
        assert!(tools.iter().any(|t| t["name"] == "web_screenshot"));
    }

    #[tokio::test]
    async fn test_web_screenshot_not_implemented() {
        let server = WebServer::new();

        let result = server
            .call_tool(
                "web_screenshot",
                json!({
                    "url": "https://example.com"
                }),
            )
            .await;

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("not yet implemented"));
        }
    }

    #[tokio::test]
    async fn test_web_invalid_tool() {
        let server = WebServer::new();

        let result = server.call_tool("nonexistent_tool", json!({})).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_web_http_get_missing_url() {
        let server = WebServer::new();

        let result = server.call_tool("http_get", json!({})).await;

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("url"));
        }
    }

    #[tokio::test]
    async fn test_web_http_post_missing_url() {
        let server = WebServer::new();

        let result = server
            .call_tool(
                "http_post",
                json!({
                    "body": "test"
                }),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_web_scrape_missing_url() {
        let server = WebServer::new();

        let result = server.call_tool("web_scrape", json!({})).await;

        assert!(result.is_err());
    }

    // Note: The following tests require a real HTTP server
    // They are commented out but show how to test with real requests

    /*
    #[tokio::test]
    async fn test_http_get_real() {
        let server = WebServer::new();

        let result = server
            .call_tool(
                "http_get",
                json!({
                    "url": "https://httpbin.org/get"
                }),
            )
            .await
            .unwrap();

        assert_eq!(result["status"], 200);
        assert!(result["body"].as_str().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn test_http_post_real() {
        let server = WebServer::new();

        let result = server
            .call_tool(
                "http_post",
                json!({
                    "url": "https://httpbin.org/post",
                    "body": "{\"test\": \"data\"}",
                    "content_type": "application/json"
                }),
            )
            .await
            .unwrap();

        assert_eq!(result["status"], 200);
    }

    #[tokio::test]
    async fn test_web_scrape_real() {
        let server = WebServer::new();

        let result = server
            .call_tool(
                "web_scrape",
                json!({
                    "url": "https://example.com"
                }),
            )
            .await
            .unwrap();

        assert!(result["text"].as_str().unwrap().contains("Example Domain"));
    }
    */
}
