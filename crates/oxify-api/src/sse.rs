//! Server-Sent Events for real-time execution updates
//!
//! Enhanced SSE implementation with:
//! - Event filtering (subscribe to specific event types)
//! - Reconnection handling with last-event-id
//! - Heartbeat events to keep connection alive
//! - Proper event typing

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream};
use oxify_model::ExecutionState;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{error, info};
use uuid::Uuid;

use crate::handlers::AppState;

/// SSE event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SseEventType {
    /// Execution state changed
    StateChange,
    /// Node completed execution
    NodeComplete,
    /// Execution progress update
    Progress,
    /// Error occurred
    Error,
    /// Heartbeat to keep connection alive
    Heartbeat,
}

impl SseEventType {
    fn as_str(&self) -> &'static str {
        match self {
            SseEventType::StateChange => "state_change",
            SseEventType::NodeComplete => "node_complete",
            SseEventType::Progress => "progress",
            SseEventType::Error => "error",
            SseEventType::Heartbeat => "heartbeat",
        }
    }
}

/// SSE stream query parameters
#[derive(Debug, Deserialize)]
pub struct SseQuery {
    /// Filter events by type (comma-separated)
    #[serde(default)]
    pub events: Option<String>,
    /// Enable heartbeat events (default: true)
    #[serde(default = "default_heartbeat")]
    pub heartbeat: bool,
    /// Heartbeat interval in seconds (default: 15)
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
}

fn default_heartbeat() -> bool {
    true
}

fn default_heartbeat_interval() -> u64 {
    15
}

impl SseQuery {
    /// Check if a specific event type is enabled
    fn is_event_enabled(&self, event_type: SseEventType) -> bool {
        if let Some(ref events_str) = self.events {
            let enabled_events: Vec<&str> = events_str.split(',').map(|s| s.trim()).collect();
            enabled_events.contains(&event_type.as_str())
        } else {
            // If no filter specified, all events are enabled
            true
        }
    }
}

/// SSE stream state
struct SseStreamState {
    state: Arc<AppState>,
    exec_id: Uuid,
    query: SseQuery,
    last_event_id: u64,
    completed: bool,
    last_heartbeat: time::Instant,
    last_state: Option<ExecutionState>,
}

/// Stream execution updates via SSE
///
/// Enhanced with event filtering, reconnection support, and heartbeats.
///
/// Query parameters:
/// - `events`: Filter events by type (comma-separated: state_change,node_complete,progress,error)
/// - `heartbeat`: Enable heartbeat events (default: true)
/// - `heartbeat_interval`: Heartbeat interval in seconds (default: 15)
///
/// Headers:
/// - `Last-Event-ID`: Resume from this event ID on reconnection
#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/stream",
    params(
        ("id" = String, Path, description = "Execution ID"),
        ("events" = Option<String>, Query, description = "Filter events (comma-separated)"),
        ("heartbeat" = Option<bool>, Query, description = "Enable heartbeat (default: true)"),
        ("heartbeat_interval" = Option<u64>, Query, description = "Heartbeat interval in seconds (default: 15)")
    ),
    responses(
        (status = 200, description = "Execution update stream")
    )
)]
pub async fn stream_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<SseQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Extract last event ID from headers for reconnection support
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    info!(
        "Streaming execution: {} (last_event_id: {}, events: {:?})",
        id, last_event_id, query.events
    );

    let stream_state = SseStreamState {
        state: state.clone(),
        exec_id: id,
        query,
        last_event_id,
        completed: false,
        last_heartbeat: time::Instant::now(),
        last_state: None,
    };

    let stream = stream::unfold(stream_state, |mut stream_state| async move {
        // Poll every 500ms
        time::sleep(Duration::from_millis(500)).await;

        if stream_state.completed {
            return None;
        }

        // Check if heartbeat is needed
        let now = time::Instant::now();
        let heartbeat_interval = Duration::from_secs(stream_state.query.heartbeat_interval);

        if stream_state.query.heartbeat
            && stream_state.query.is_event_enabled(SseEventType::Heartbeat)
            && now.duration_since(stream_state.last_heartbeat) >= heartbeat_interval
        {
            stream_state.last_heartbeat = now;
            stream_state.last_event_id += 1;

            let event = Event::default()
                .event(SseEventType::Heartbeat.as_str())
                .id(stream_state.last_event_id.to_string())
                .data("ping");

            return Some((Ok(event), stream_state));
        }

        // Fetch execution state
        match stream_state
            .state
            .execution_store
            .get(&stream_state.exec_id)
            .await
        {
            Ok(Some(ctx)) => {
                let current_state = ctx.state.clone();
                let state_changed = stream_state.last_state.as_ref() != Some(&current_state);

                // Only send event if state changed or it's a progress update
                let should_send =
                    state_changed || stream_state.query.is_event_enabled(SseEventType::Progress);

                if !should_send {
                    return Some((Ok(Event::default().comment("no update")), stream_state));
                }

                stream_state.last_event_id += 1;

                // Determine event type
                let event_type = if state_changed {
                    SseEventType::StateChange
                } else {
                    SseEventType::Progress
                };

                // Check if event is enabled
                if !stream_state.query.is_event_enabled(event_type) {
                    return Some((Ok(Event::default().comment("filtered")), stream_state));
                }

                let event_data = serde_json::json!({
                    "execution_id": stream_state.exec_id,
                    "workflow_id": ctx.workflow_id,
                    "state": current_state,
                    "node_results_count": ctx.node_results.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });

                let event = Event::default()
                    .event(event_type.as_str())
                    .id(stream_state.last_event_id.to_string())
                    .json_data(event_data)
                    .unwrap_or_else(|e| {
                        error!("Failed to serialize event data: {}", e);
                        Event::default()
                            .event(SseEventType::Error.as_str())
                            .data("serialization_error")
                    });

                // Update last state
                stream_state.last_state = Some(current_state.clone());

                // Check if execution is completed
                if matches!(
                    current_state,
                    ExecutionState::Completed
                        | ExecutionState::Failed(_)
                        | ExecutionState::Cancelled
                ) {
                    stream_state.completed = true;
                }

                Some((Ok(event), stream_state))
            }
            Ok(None) => {
                // Execution not found, send error and stop
                stream_state.last_event_id += 1;

                if stream_state.query.is_event_enabled(SseEventType::Error) {
                    let event = Event::default()
                        .event(SseEventType::Error.as_str())
                        .id(stream_state.last_event_id.to_string())
                        .data(format!("Execution {} not found", stream_state.exec_id));
                    stream_state.completed = true;
                    Some((Ok(event), stream_state))
                } else {
                    None
                }
            }
            Err(e) => {
                // Storage error, send error and stop
                error!("Failed to get execution {}: {}", stream_state.exec_id, e);
                stream_state.last_event_id += 1;

                if stream_state.query.is_event_enabled(SseEventType::Error) {
                    let event = Event::default()
                        .event(SseEventType::Error.as_str())
                        .id(stream_state.last_event_id.to_string())
                        .data(format!("Error fetching execution: {}", e));
                    stream_state.completed = true;
                    Some((Ok(event), stream_state))
                } else {
                    None
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
