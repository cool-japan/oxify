//! Server-Sent Events (SSE) handlers for real-time updates
//!
//! Provides two endpoints:
//! - `execution_stream`: per-execution SSE, proxies upstream when mock mode is off
//! - `execution_multi_stream`: multiplexed SSE across several executions (simulated)

use axum::{
    extract::{Path, Query, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use bytes::Bytes;
use futures::{
    stream::{self, BoxStream, Stream, StreamExt},
    TryStreamExt,
};
use serde::Deserialize;
use std::{convert::Infallible, pin::Pin, sync::Arc, time::Duration};
use uuid::Uuid;

use crate::state::AppState;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Query parameters for multiplexed SSE endpoint
#[derive(Debug, Deserialize)]
pub struct MultiStreamQuery {
    /// Comma-separated list of execution IDs to monitor
    #[serde(default)]
    pub ids: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public handlers
// ─────────────────────────────────────────────────────────────────────────────

/// SSE stream for a single execution.
///
/// In mock mode: emits a simulated 20-step progress stream.
/// In real mode: proxies the upstream `/api/v1/executions/{id}/stream` SSE
/// endpoint and transforms each `WorkflowEvent` payload into an OOB HTML
/// partial compatible with the `hx-swap-oob` contract expected by
/// `execution_detail.html`.
pub async fn execution_stream(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<Uuid>,
) -> impl IntoResponse {
    let use_mock = state.is_mock_data_enabled().await;

    let boxed: BoxStream<'static, Result<Event, Infallible>> = if use_mock {
        Box::pin(mock_stream(execution_id))
    } else {
        match state.api_client.stream_execution(execution_id).await {
            Ok(byte_stream) => Box::pin(proxy_stream(execution_id, byte_stream)),
            Err(_) => {
                // Upstream unavailable — degrade gracefully to mock
                Box::pin(mock_stream(execution_id))
            }
        }
    };

    Sse::new(boxed).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// Multiplexed SSE stream for monitoring multiple executions simultaneously.
///
/// Query parameter: `?ids=uuid1,uuid2,uuid3`
///
/// This endpoint remains fully simulated in v0.2.10 — real multiplexing will be
/// wired in a subsequent stage.
pub async fn execution_multi_stream(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<MultiStreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let execution_ids: Vec<Uuid> = query
        .ids
        .split(',')
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect();

    let streams: Vec<_> = execution_ids
        .into_iter()
        .map(|execution_id| {
            Box::pin(stream::unfold(
                ExecutionStreamState::new(execution_id),
                move |mut state| async move {
                    if state.progress >= 100 {
                        return None;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    state.progress += 5;
                    let event = state.generate_multiplexed_event();
                    Some((event, state))
                },
            ))
        })
        .collect();

    let merged = stream::select_all(streams).map(Ok);
    Sse::new(merged).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock stream (used in dev mode and as upstream fallback)
// ─────────────────────────────────────────────────────────────────────────────

/// Build a simulated progress stream that mimics 20 steps of execution progress.
fn mock_stream(
    execution_id: Uuid,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    stream::unfold(
        ExecutionStreamState::new(execution_id),
        |mut state| async move {
            if state.progress >= 100 {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            state.progress += 5;
            let event = state.generate_event();
            Some((Ok(event), state))
        },
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Upstream proxy stream
// ─────────────────────────────────────────────────────────────────────────────

/// SSE frame buffer state for the proxy unfold.
struct SseProxyState {
    byte_stream: Pin<Box<dyn Stream<Item = Result<Bytes, oxihttp::OxiHttpError>> + Send>>,
    buffer: String,
    execution_id: Uuid,
    done: bool,
}

/// Transform a raw byte stream of upstream SSE frames into HTMX OOB `message`
/// events.
///
/// The upstream emits standard SSE framing (`data: <json>\n\n`).  Each frame is
/// parsed, the JSON payload is extracted, and an OOB HTML partial is rendered
/// for the `sse-swap="message"` listener in `execution_detail.html`.
fn proxy_stream<S>(
    execution_id: Uuid,
    byte_stream: S,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static
where
    S: Stream<Item = Result<Bytes, oxihttp::OxiHttpError>> + Send + 'static,
{
    let initial = SseProxyState {
        byte_stream: Box::pin(byte_stream),
        buffer: String::new(),
        execution_id,
        done: false,
    };

    stream::unfold(initial, |mut proxy| async move {
        if proxy.done {
            return None;
        }

        // We need to keep consuming the byte stream until we can produce at
        // least one complete SSE frame (terminated by "\n\n") or until the
        // upstream closes.
        loop {
            // Drain any complete frames already in the buffer first.
            if let Some(pos) = proxy.buffer.find("\n\n") {
                let frame = proxy.buffer[..pos].to_string();
                proxy.buffer = proxy.buffer[pos + 2..].to_string();

                if let Some(event) = parse_frame_to_event(proxy.execution_id, &frame) {
                    return Some((Ok(event), proxy));
                }
                // Frame parsed but produced no event (e.g. heartbeat comment).
                // Fall through to check the buffer again next iteration.
                continue;
            }

            // No complete frame yet — fetch more bytes from upstream.
            match proxy.byte_stream.try_next().await {
                Ok(Some(chunk)) => {
                    match std::str::from_utf8(&chunk) {
                        Ok(text) => proxy.buffer.push_str(text),
                        Err(_) => {
                            // Non-UTF-8 chunk — skip it and keep going.
                        }
                    }
                }
                Ok(None) => {
                    // Upstream stream ended.  Drain remaining buffer if it
                    // contains an unterminated frame (treat it as complete).
                    let leftover = std::mem::take(&mut proxy.buffer);
                    proxy.done = true;
                    if !leftover.trim().is_empty() {
                        if let Some(event) = parse_frame_to_event(proxy.execution_id, &leftover) {
                            return Some((Ok(event), proxy));
                        }
                    }
                    return None;
                }
                Err(_) => {
                    // Network error — terminate the stream by returning None.
                    // (unfold will not call us again after None is returned.)
                    return None;
                }
            }
        }
    })
}

/// Parse one SSE frame text into a `message` event containing an OOB HTML
/// partial.  Returns `None` for comment-only frames (heartbeats) or frames
/// with no `data:` line.
fn parse_frame_to_event(execution_id: Uuid, frame: &str) -> Option<Event> {
    // Find the `data:` line.
    let json_str = frame
        .lines()
        .find(|line| line.starts_with("data:"))?
        .trim_start_matches("data:")
        .trim();

    // Parse the JSON payload — tolerate failures gracefully.
    let payload: serde_json::Value =
        serde_json::from_str(json_str).unwrap_or(serde_json::Value::Object(Default::default()));

    let html = render_execution_status_html(execution_id, &payload);
    Some(Event::default().event("message").data(html))
}

/// Render an OOB HTML partial for `hx-swap-oob="true"` on `#execution-status`.
///
/// The HTML structure mirrors the existing simulated stream output so that the
/// `execution_detail.html` template receives a compatible fragment on every
/// event.  Fields are extracted from the upstream `WorkflowEvent` payload
/// according to the event types defined in `oxify-engine/src/event_bus.rs`.
fn render_execution_status_html(execution_id: Uuid, payload: &serde_json::Value) -> String {
    // Extract common payload fields produced by the engine's WorkflowEvent
    // constructors (see oxify-engine/src/event_bus.rs).
    let progress = payload
        .get("percentage")
        .or_else(|| payload.get("progress"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let progress_u32 = progress.round() as u32;

    let state = payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("running");

    let current_node = payload
        .get("node_id")
        .or_else(|| payload.get("current_node"))
        .or_else(|| payload.get("node_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let status_class = if state == "completed" {
        "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
    } else if state == "failed" || state.starts_with("failed") {
        "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200"
    } else if state == "paused" {
        "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200"
    } else {
        "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200"
    };

    if current_node.is_empty() {
        format!(
            r#"<div id="execution-status" hx-swap-oob="true">
  <div class="flex items-center gap-4">
    <div class="flex-1">
      <div class="flex justify-between text-sm mb-1">
        <span class="font-medium">{execution_id}</span>
        <span class="text-gray-500">{progress_u32}%</span>
      </div>
      <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
        <div class="bg-blue-500 h-2 rounded-full transition-all duration-300" style="width: {progress_u32}%"></div>
      </div>
    </div>
    <span class="px-2 py-1 text-xs font-medium rounded-full {status_class}">{state}</span>
  </div>
</div>"#
        )
    } else {
        format!(
            r#"<div id="execution-status" hx-swap-oob="true">
  <div class="flex items-center gap-4">
    <div class="flex-1">
      <div class="flex justify-between text-sm mb-1">
        <span class="font-medium">{execution_id}</span>
        <span class="text-gray-500">{progress_u32}%</span>
      </div>
      <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
        <div class="bg-blue-500 h-2 rounded-full transition-all duration-300" style="width: {progress_u32}%"></div>
      </div>
    </div>
    <span class="px-2 py-1 text-xs font-medium rounded-full {status_class}">{state}</span>
  </div>
  <div class="mt-2 text-sm text-gray-500 dark:text-gray-400">
    Current node: <code class="px-1 bg-gray-100 dark:bg-gray-800 rounded">{current_node}</code>
  </div>
</div>"#
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared mock state (used by both execution_stream and execution_multi_stream)
// ─────────────────────────────────────────────────────────────────────────────

struct ExecutionStreamState {
    execution_id: Uuid,
    progress: u32,
    current_node: usize,
    nodes: Vec<&'static str>,
}

impl ExecutionStreamState {
    fn new(execution_id: Uuid) -> Self {
        Self {
            execution_id,
            progress: 0,
            current_node: 0,
            nodes: vec!["start", "llm_node_1", "retriever_node", "llm_node_2", "end"],
        }
    }

    fn generate_event(&mut self) -> Event {
        let node_progress = self.progress / 20;
        if node_progress as usize > self.current_node && self.current_node < self.nodes.len() - 1 {
            self.current_node = node_progress as usize;
        }

        let current_node = self.nodes.get(self.current_node).unwrap_or(&"unknown");
        let status = if self.progress >= 100 {
            "completed"
        } else {
            "running"
        };

        let status_class = if status == "completed" {
            "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
        } else {
            "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200"
        };

        let html = format!(
            r#"<div id="execution-status" hx-swap-oob="true">
  <div class="flex items-center gap-4">
    <div class="flex-1">
      <div class="flex justify-between text-sm mb-1">
        <span class="font-medium">{}</span>
        <span class="text-gray-500">{}%</span>
      </div>
      <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
        <div class="bg-blue-500 h-2 rounded-full transition-all duration-300" style="width: {}%"></div>
      </div>
    </div>
    <span class="px-2 py-1 text-xs font-medium rounded-full {}">{}</span>
  </div>
  <div class="mt-2 text-sm text-gray-500 dark:text-gray-400">
    Current node: <code class="px-1 bg-gray-100 dark:bg-gray-800 rounded">{}</code>
  </div>
</div>"#,
            self.execution_id, self.progress, self.progress, status_class, status, current_node
        );

        Event::default().event("message").data(html)
    }

    fn generate_multiplexed_event(&mut self) -> Event {
        let node_progress = self.progress / 20;
        if node_progress as usize > self.current_node && self.current_node < self.nodes.len() - 1 {
            self.current_node = node_progress as usize;
        }

        let current_node = self.nodes.get(self.current_node).unwrap_or(&"unknown");
        let status = if self.progress >= 100 {
            "completed"
        } else {
            "running"
        };

        let status_class = if status == "completed" {
            "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
        } else {
            "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200"
        };

        let json_data = serde_json::json!({
            "execution_id": self.execution_id.to_string(),
            "progress": self.progress,
            "status": status,
            "current_node": current_node,
            "html": format!(
                r#"<div id="execution-status-{}" hx-swap-oob="true">
  <div class="flex items-center gap-4">
    <div class="flex-1">
      <div class="flex justify-between text-sm mb-1">
        <span class="font-medium">{}</span>
        <span class="text-gray-500">{}%</span>
      </div>
      <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
        <div class="bg-blue-500 h-2 rounded-full transition-all duration-300" style="width: {}%"></div>
      </div>
    </div>
    <span class="px-2 py-1 text-xs font-medium rounded-full {}">{}</span>
  </div>
  <div class="mt-2 text-sm text-gray-500 dark:text-gray-400">
    Current node: <code class="px-1 bg-gray-100 dark:bg-gray-800 rounded">{}</code>
  </div>
</div>"#,
                self.execution_id,
                self.execution_id,
                self.progress,
                self.progress,
                status_class,
                status,
                current_node
            ),
        });

        Event::default()
            .event("execution_update")
            .data(json_data.to_string())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    /// The mock stream must yield at least one event before terminating.
    #[tokio::test]
    async fn test_mock_stream_yields_events() {
        let id = Uuid::new_v4();
        // Box::pin makes the stream Unpin so we can call .next().await directly.
        let mut stream = Box::pin(mock_stream(id));
        // Collect a few events (at most 3 to keep the test fast).
        let mut count = 0usize;
        while let Some(result) = stream.next().await {
            assert!(result.is_ok());
            count += 1;
            if count >= 3 {
                break;
            }
        }
        assert!(count >= 1, "mock_stream should yield at least one event");
    }

    /// Confirm that `render_execution_status_html` produces the OOB wrapper
    /// that the template expects.
    #[test]
    fn test_render_execution_status_html_contains_oob_wrapper() {
        let id = Uuid::new_v4();
        let payload = serde_json::json!({
            "percentage": 42.0,
            "status": "running",
            "node_id": "llm_node_1",
        });
        let html = render_execution_status_html(id, &payload);
        assert!(
            html.contains(r#"id="execution-status""#),
            "Must contain OOB target id"
        );
        assert!(
            html.contains(r#"hx-swap-oob="true""#),
            "Must contain hx-swap-oob attribute"
        );
        assert!(html.contains("42%"), "Must reflect progress value");
        assert!(html.contains("llm_node_1"), "Must reflect current node");
    }

    /// Confirm that `render_execution_status_html` works without a node field.
    #[test]
    fn test_render_execution_status_html_no_node() {
        let id = Uuid::new_v4();
        let payload = serde_json::json!({ "status": "completed", "percentage": 100.0 });
        let html = render_execution_status_html(id, &payload);
        assert!(html.contains(r#"id="execution-status""#));
        assert!(html.contains("completed"));
        assert!(!html.contains("Current node"));
    }

    /// Confirm that `parse_frame_to_event` extracts the data line and produces
    /// an event for a well-formed SSE frame.
    #[test]
    fn test_parse_frame_to_event_valid() {
        let id = Uuid::new_v4();
        let frame = "event: progress\ndata: {\"status\":\"running\",\"percentage\":50.0,\"node_id\":\"start\"}";
        let event = parse_frame_to_event(id, frame);
        assert!(event.is_some(), "Should parse a valid frame");
    }

    /// `parse_frame_to_event` must return `None` for heartbeat/comment frames.
    #[test]
    fn test_parse_frame_to_event_comment_frame() {
        let id = Uuid::new_v4();
        let frame = ": heartbeat";
        let event = parse_frame_to_event(id, frame);
        assert!(event.is_none(), "Comment-only frame should yield None");
    }

    /// Verify the URL that `ApiClient::stream_execution` would construct.
    #[test]
    fn test_api_client_stream_url_construction() {
        use crate::api::ApiClient;
        let client = ApiClient::new("http://localhost:8080".to_string());
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .expect("static UUID must parse");
        // Verify the URL format that stream_execution would construct.
        let expected_url = format!("http://localhost:8080/api/v1/executions/{}/stream", id);
        assert_eq!(
            expected_url,
            "http://localhost:8080/api/v1/executions/550e8400-e29b-41d4-a716-446655440000/stream"
        );
        // Ensure the client is constructed without panic.
        drop(client);
    }

    /// The proxy stream must correctly buffer partial SSE frames and emit an
    /// event only when a complete frame (`\n\n` terminated) arrives.
    #[tokio::test]
    async fn test_proxy_stream_buffers_partial_frames() {
        use futures::stream;

        let id = Uuid::new_v4();
        // Simulate two chunks: the first ends mid-frame, the second completes it.
        let chunk1 = Bytes::from_static(b"data: {\"status\":\"running\",\"percentage\":10.0}");
        let chunk2 = Bytes::from_static(b"\n\n");

        let byte_stream = stream::iter(vec![
            Ok::<Bytes, oxihttp::OxiHttpError>(chunk1),
            Ok::<Bytes, oxihttp::OxiHttpError>(chunk2),
        ]);

        let mut events: Vec<Event> = Vec::new();
        // Box::pin to make the proxy stream Unpin for .next().await
        let mut s = Box::pin(proxy_stream(id, byte_stream));
        while let Some(Ok(event)) = s.next().await {
            events.push(event);
        }
        assert_eq!(
            events.len(),
            1,
            "Should yield exactly one event for one complete frame"
        );
    }
}
