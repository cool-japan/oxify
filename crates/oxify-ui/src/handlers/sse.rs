//! Server-Sent Events (SSE) handlers for real-time updates

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, Sse},
};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use std::{convert::Infallible, sync::Arc, time::Duration};
use uuid::Uuid;

use crate::state::AppState;

/// Query parameters for multiplexed SSE endpoint
#[derive(Debug, Deserialize)]
pub struct MultiStreamQuery {
    /// Comma-separated list of execution IDs to monitor
    #[serde(default)]
    pub ids: String,
}

/// SSE stream for execution updates
pub async fn execution_stream(
    State(_state): State<Arc<AppState>>,
    Path(execution_id): Path<Uuid>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Create a stream that simulates execution updates
    // In production, this would subscribe to actual execution events
    let stream = stream::unfold(
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
    );

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// Internal state for execution SSE stream
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
        // Update current node based on progress
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

        // Generate HTML partial for HTMX to swap
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
            self.execution_id,
            self.progress,
            self.progress,
            if status == "completed" {
                "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
            } else {
                "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200"
            },
            status,
            current_node
        );

        Event::default().event("message").data(html)
    }
}

/// Multiplexed SSE stream for monitoring multiple executions simultaneously
/// Query parameter: ?ids=uuid1,uuid2,uuid3
pub async fn execution_multi_stream(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<MultiStreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Parse execution IDs from comma-separated string
    let execution_ids: Vec<Uuid> = query
        .ids
        .split(',')
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect();

    // Create individual streams for each execution (boxed to make them Unpin)
    // If execution_ids is empty, this will create an empty vec, which select_all handles fine
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

    // Merge all streams into one (works even with empty vec)
    let merged_stream = stream::select_all(streams).map(Ok);

    Sse::new(merged_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

impl ExecutionStreamState {
    /// Generate event for multiplexed stream (includes execution_id in event type)
    fn generate_multiplexed_event(&mut self) -> Event {
        // Update current node based on progress
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

        // Generate JSON payload with execution_id for client-side demultiplexing
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
                if status == "completed" {
                    "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
                } else {
                    "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200"
                },
                status,
                current_node
            ),
        });

        Event::default()
            .event("execution_update")
            .data(json_data.to_string())
    }
}
