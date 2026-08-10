//! Web MCP server - provides HTTP and web scraping operations

use super::css_select::SelectorList;
use crate::{McpServer, Result};
use async_trait::async_trait;
use oxixml_dom::{Document, NodeId, NodeKind};
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
/// Returns [`crate::McpError::InvalidRequest`] if `selector` is not a valid CSS selector,
/// uses syntax outside the supported subset, or exceeds one of the selector engine's
/// hard size caps. This never panics on malformed input. A syntactically valid selector
/// that matches no elements is not an error — it yields an empty string.
fn extract_selected_text(html: &str, selector: &str) -> Result<String> {
    // Parse the selector before the document: a bad selector is rejected without
    // spending anything on HTML parsing.
    let selectors = SelectorList::parse(selector).map_err(|e| {
        crate::McpError::InvalidRequest(format!(
            "Invalid CSS selector '{}': {e}",
            abbreviate(selector)
        ))
    })?;

    let parsed = oxixml_html::parse_document(html);
    let document = parsed.document();

    let text = selectors
        .match_elements(document, parsed.root())
        .into_iter()
        .map(|element| element_text(document, element))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(text)
}

/// Shorten a caller-supplied selector for inclusion in an error message.
///
/// The selector arrives as an arbitrary JSON string, so echoing it back whole
/// would let a caller inflate an error message without bound.
fn abbreviate(selector: &str) -> String {
    /// Number of characters of the offending selector kept in the message.
    const MAX_ECHOED_CHARS: usize = 80;

    let mut abbreviated: String = selector.chars().take(MAX_ECHOED_CHARS).collect();
    if selector.chars().nth(MAX_ECHOED_CHARS).is_some() {
        abbreviated.push_str("...");
    }
    abbreviated
}

/// The text of one element: its descendant text nodes, each trimmed, with empty
/// fragments dropped and the rest joined by a single space.
///
/// Fragments are kept separate rather than concatenated, so `<p>a<b>b</b></p>`
/// yields `"a b"` — markup that separates two words keeps them separated.
fn element_text(document: &Document, element: NodeId) -> String {
    document
        .descendants(element)
        .filter(|node| document.kind(*node) == Some(NodeKind::Text))
        .filter_map(|node| document.character_data(node))
        .map(str::trim)
        .filter(|fragment| !fragment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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

    /// Runs `selector` against `html`, requiring it to be accepted.
    fn selected_text(html: &str, selector: &str) -> String {
        extract_selected_text(html, selector)
            .unwrap_or_else(|e| panic!("`{selector}` should be accepted, got: {e}"))
    }

    /// Runs `selector` against a trivial document, requiring it to be rejected as
    /// `InvalidRequest`, and returns the message.
    fn rejected_selector(selector: &str) -> String {
        match extract_selected_text("<p>x</p>", selector) {
            Err(crate::McpError::InvalidRequest(msg)) => msg,
            other => panic!("`{selector}` should be InvalidRequest, got {other:?}"),
        }
    }

    #[test]
    fn test_extract_selected_text_keeps_markup_separated_words_apart() {
        // Each descendant text node is a separate fragment, so markup that splits
        // two words keeps them from being run together.
        assert_eq!(selected_text("<p>a<b>b</b>c</p>", "p"), "a b c");
        assert_eq!(selected_text("<p>one <b>two</b></p>", "p"), "one two");
    }

    #[test]
    fn test_extract_selected_text_drops_whitespace_only_fragments() {
        let html = "<div>\n  <span>  kept  </span>\n  <span></span>\n</div>";
        assert_eq!(selected_text(html, "div"), "kept");
        assert_eq!(selected_text(html, "span"), "kept\n");
    }

    #[test]
    fn test_extract_selected_text_ignores_comments_and_joins_elements_by_newline() {
        let html = "<ul><li>one<!-- hidden --></li><li>two</li></ul>";
        assert_eq!(selected_text(html, "li"), "one\ntwo");
    }

    #[test]
    fn test_extract_selected_text_handles_quirky_html() {
        // Unclosed tags, mixed-case markup and unquoted attributes: the HTML5
        // parser repairs the tree, and selectors match case-insensitively.
        let html = "<DIV CLASS=post><P>first<P>second</DIV>";
        assert_eq!(selected_text(html, "div.post p"), "first\nsecond");
        assert_eq!(selected_text(html, "DIV.post > P"), "first\nsecond");
        assert_eq!(selected_text(html, "div.Post p"), "");
    }

    #[test]
    fn test_extract_selected_text_supports_the_wider_selector_subset() {
        let html = "<ul id=\"list\"><li class=\"a\">one</li><li>two</li><li>three</li></ul>";

        assert_eq!(selected_text(html, "#list li:first-child"), "one");
        assert_eq!(selected_text(html, "#list li:last-child"), "three");
        assert_eq!(selected_text(html, "li:nth-child(2)"), "two");
        assert_eq!(selected_text(html, "li:not(.a)"), "two\nthree");
        assert_eq!(selected_text(html, ".a + li"), "two");
        assert_eq!(selected_text(html, ".a ~ li"), "two\nthree");
        assert_eq!(
            selected_text(html, "[id=list] > li, [class]"),
            "one\ntwo\nthree"
        );
        assert_eq!(selected_text(html, "*[id^=li][id$=st]"), "one two three");
    }

    #[test]
    fn test_extract_selected_text_rejects_unsupported_and_malformed_selectors() {
        for selector in [
            "",
            "   ",
            ">",
            "div >",
            "div,",
            "p::before",
            "p:has(a)",
            "p:is(a)",
            "div[class",
            "div[class|=a]",
            "div[class='unterminated",
            "p:nth-child(",
            "p:not(:not(a))",
            "\u{0}",
            "🦀:🦀",
        ] {
            let msg = rejected_selector(selector);
            assert!(
                msg.contains("Invalid CSS selector"),
                "message should keep the established prefix, got: {msg}"
            );
        }
    }

    #[test]
    fn test_extract_selected_text_bounds_the_echoed_selector() {
        // A hostile caller must not be able to inflate the error message with a
        // huge selector, nor split a multi-byte character while it is truncated.
        let selector = "🦀".repeat(100 * 1024);

        let msg = rejected_selector(&selector);
        assert!(msg.contains("Invalid CSS selector"));
        assert!(
            msg.contains("..."),
            "long selectors should be elided: {msg}"
        );
        assert!(
            msg.len() < 1024,
            "error message should stay small, got {} bytes",
            msg.len()
        );
    }

    #[test]
    fn test_extract_selected_text_survives_adversarial_input() {
        // Oversized input is shed by the length cap before anything is parsed.
        let huge = format!("div{}{}", ":not(".repeat(10_000), ")".repeat(10_000));
        assert!(huge.len() > 1024);
        assert!(extract_selected_text("<p>x</p>", &huge).is_err());

        // Short enough to reach the parser, and rejected there instead: nesting
        // by the `:not()` rule, breadth by the selector-list cap.
        assert!(extract_selected_text("<p>x</p>", "div:not(:not(:not(p)))").is_err());
        let wide = ["a"; 64].join(",");
        assert!(wide.len() < 1024);
        assert!(extract_selected_text("<p>x</p>", &wide).is_err());

        // A pathological attribute value in the *document* is just data.
        let html = format!("<p data-x=\"{}\">t</p>", "y".repeat(100_000));
        assert_eq!(selected_text(&html, "p[data-x^=yyy]"), "t");
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
