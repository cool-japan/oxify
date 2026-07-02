//! Web MCP server - provides HTTP and web scraping operations

use crate::{McpServer, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

/// Built-in MCP server for web operations
pub struct WebServer {
    client: oxihttp::HttpsClient,
    /// Maximum response size in bytes (default: 10MB)
    max_response_size: usize,
}

impl WebServer {
    /// Create a new web server
    pub fn new() -> Self {
        Self {
            client: oxihttp::Client::builder()
                .user_agent("OxiFY-MCP/0.1.0")
                .connect_timeout(std::time::Duration::from_secs(30))
                .read_timeout(std::time::Duration::from_secs(30))
                .with_tls()
                .build_https()
                .expect("oxihttp::Client::builder() with default settings should not fail"),
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
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?
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
                    .body_text()
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
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?
                    .header("Content-Type", content_type)
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?
                    .body(body.to_string())
                    .send()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                let status = response.status().as_u16();
                let response_body = response
                    .body_text()
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
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?
                    .send()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                let html = response
                    .body_text()
                    .await
                    .map_err(|e| crate::McpError::ToolExecutionError(e.to_string()))?;

                // Basic HTML to text conversion.
                let text = if let Some(css_selector) = selector {
                    // Extract the text content of every element matching the caller-supplied
                    // CSS selector. A malformed selector is a caller error (InvalidRequest,
                    // never a panic); a selector that matches nothing yields an empty string.
                    extract_selected_text(&html, css_selector)?
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
                #[cfg(feature = "headless-browser")]
                {
                    use base64::Engine as _;

                    let url = arguments["url"].as_str().ok_or_else(|| {
                        crate::McpError::InvalidRequest("Missing 'url'".to_string())
                    })?;

                    let png = capture_screenshot(url).await?;
                    let screenshot_base64 = base64::engine::general_purpose::STANDARD.encode(&png);

                    Ok(json!({
                        "url": url,
                        "screenshot_base64": screenshot_base64,
                        "format": "png",
                    }))
                }

                #[cfg(not(feature = "headless-browser"))]
                {
                    // Off by default: real capture requires a local headless browser. Rebuild
                    // oxify-mcp with `--features headless-browser` to enable the CDP path.
                    Err(crate::McpError::ToolExecutionError(
                        "Screenshot not yet implemented. Requires headless browser.".to_string(),
                    ))
                }
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
                "description": "Take a screenshot of a web page (PNG, base64-encoded). Requires oxify-mcp to be built with the `headless-browser` feature and a local Chrome/Chromium binary at runtime.",
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

/// Extract the text content of every element in `html` matching the given CSS `selector`.
///
/// The text of each matched element is the space-joined, trimmed concatenation of its
/// descendant text nodes; matched elements are joined by newlines, in document order.
///
/// # Errors
///
/// Returns [`crate::McpError::InvalidRequest`] if `selector` is not a valid CSS selector.
/// This never panics on malformed input. A syntactically valid selector that matches no
/// elements is not an error — it yields an empty string.
fn extract_selected_text(html: &str, selector: &str) -> Result<String> {
    let document = scraper::Html::parse_document(html);
    let parsed = scraper::Selector::parse(selector).map_err(|e| {
        crate::McpError::InvalidRequest(format!("Invalid CSS selector '{selector}': {e:?}"))
    })?;

    let text = document
        .select(&parsed)
        .map(|element| {
            element
                .text()
                .map(str::trim)
                .filter(|fragment| !fragment.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(text)
}

/// Launch a headless Chrome/Chromium instance, navigate to `url`, and capture a PNG
/// screenshot of the loaded page, returning the raw PNG bytes.
///
/// The external browser process and its event handler are always torn down before this
/// function returns, on both the success and error paths, so no Chrome process is leaked.
///
/// # Errors
///
/// Every failure mode — a browser that cannot be configured or launched (e.g. no
/// Chrome/Chromium binary is installed), a navigation that fails, or a screenshot that
/// cannot be captured — is surfaced as [`crate::McpError::ToolExecutionError`] with an
/// actionable message. This function never panics.
#[cfg(feature = "headless-browser")]
async fn capture_screenshot(url: &str) -> Result<Vec<u8>> {
    use chromiumoxide::browser::{Browser, BrowserConfig};
    use futures::StreamExt as _;

    let config = BrowserConfig::builder().build().map_err(|e| {
        crate::McpError::ToolExecutionError(format!("Failed to build browser config: {e}"))
    })?;

    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|e| {
        crate::McpError::ToolExecutionError(format!(
            "Failed to launch headless browser (is Chrome/Chromium installed?): {e}"
        ))
    })?;

    // Drive the CDP event handler in the background for the lifetime of this call; it must
    // be polled continuously for commands (navigation, screenshot, close) to make progress.
    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    // Capture the screenshot, then unconditionally tear the browser down so a failure part
    // way through never leaks the external Chrome process.
    let outcome = capture_page_png(&browser, url).await;

    // Best-effort clean shutdown: we already hold the PNG bytes on success, and closing
    // here also collects the child process and silences chromiumoxide's drop-time
    // "browser was not closed manually" warning. Shutdown errors must not mask `outcome`.
    let _ = browser.close().await;
    let _ = browser.wait().await;
    handler_task.abort();

    outcome
}

/// Navigate a launched [`chromiumoxide::Browser`] to `url`, wait for the navigation to
/// settle, and capture a PNG screenshot of the page.
#[cfg(feature = "headless-browser")]
async fn capture_page_png(browser: &chromiumoxide::Browser, url: &str) -> Result<Vec<u8>> {
    use chromiumoxide::page::ScreenshotParams;

    let page = browser.new_page(url).await.map_err(|e| {
        crate::McpError::ToolExecutionError(format!("Failed to navigate to {url}: {e}"))
    })?;

    page.wait_for_navigation().await.map_err(|e| {
        crate::McpError::ToolExecutionError(format!("Navigation to {url} did not complete: {e}"))
    })?;

    // `ScreenshotParams::builder().build()` defaults to the PNG capture format.
    page.screenshot(ScreenshotParams::builder().build())
        .await
        .map_err(|e| crate::McpError::ToolExecutionError(format!("Screenshot capture failed: {e}")))
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

    // Without the `headless-browser` feature, `web_screenshot` is an unimplemented off-path
    // returning a clear error. With the feature, that arm instead drives a real browser, so
    // this "not implemented" expectation only holds in the default (feature-off) build.
    #[cfg(not(feature = "headless-browser"))]
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

    #[test]
    fn test_extract_selected_text_matches_known_structure() {
        let html = r#"
            <html><body>
                <div class="post"><h2>First</h2><p>Hello <b>world</b></p></div>
                <div class="post"><h2>Second</h2><p>Goodbye</p></div>
                <div class="sidebar"><p>ignore me</p></div>
            </body></html>
        "#;

        // Two `<p>` elements live inside `div.post`; the sidebar paragraph is excluded.
        // Each element's descendant text nodes are trimmed and space-joined; elements are
        // newline-joined in document order.
        let text = extract_selected_text(html, "div.post p")
            .expect("a valid selector should parse without error");
        assert_eq!(text, "Hello world\nGoodbye");
    }

    #[test]
    fn test_extract_selected_text_zero_matches_is_empty_not_error() {
        let html = "<html><body><p>content</p></body></html>";

        // A syntactically valid selector that matches nothing must yield an empty string,
        // never an error.
        let text = extract_selected_text(html, "table.does-not-exist")
            .expect("a valid selector matching zero elements must not error");
        assert!(
            text.is_empty(),
            "expected an empty string for zero matches, got {text:?}"
        );
    }

    #[test]
    fn test_extract_selected_text_invalid_selector_is_invalid_request() {
        let html = "<html><body><p>content</p></body></html>";

        // A malformed selector must be reported as InvalidRequest, not panic.
        let result = extract_selected_text(html, ">>> not a valid selector <<<");
        match result {
            Err(crate::McpError::InvalidRequest(msg)) => {
                assert!(
                    msg.contains("Invalid CSS selector"),
                    "error message should identify the bad selector, got: {msg}"
                );
            }
            other => {
                panic!("expected McpError::InvalidRequest for a malformed selector, got {other:?}")
            }
        }
    }

    // Real headless-browser screenshot capture. Compiled only with the `headless-browser`
    // feature, and ignored by default because it needs a local Chrome/Chromium binary and
    // network access. Run explicitly on a suitable machine with:
    //   cargo nextest run -p oxify-mcp --features headless-browser --run-ignored all
    #[cfg(feature = "headless-browser")]
    #[tokio::test]
    #[ignore = "requires a local Chrome/Chromium binary"]
    async fn test_web_screenshot_captures_png() {
        use base64::Engine as _;

        let server = WebServer::new();

        let result = server
            .call_tool("web_screenshot", json!({ "url": "https://example.com" }))
            .await
            .expect("screenshot capture should succeed with a local Chrome/Chromium");

        assert_eq!(result["url"], "https://example.com");
        assert_eq!(result["format"], "png");

        let encoded = result["screenshot_base64"]
            .as_str()
            .expect("screenshot_base64 must be a string");
        assert!(!encoded.is_empty(), "screenshot data must not be empty");

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("screenshot_base64 must be valid base64");
        assert!(
            bytes.starts_with(b"\x89PNG"),
            "decoded screenshot bytes should carry the PNG magic header"
        );
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
