//! Server-Sent Events for real-time execution updates
//!
//! Enhanced SSE implementation with:
//! - Event filtering (subscribe to specific event types)
//! - Reconnection handling with last-event-id
//! - Heartbeat events to keep connection alive
//! - Proper event typing
//! - Real-time events sourced from the engine EventBus (no polling)

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream};
use oxify_engine::execution_events;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tracing::info;
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

/// Map an engine event type string to an SSE event type.
///
/// Returns `None` for event types that should be silently skipped
/// (e.g. `node.started`, `level.*`, `checkpoint.*`).
fn map_event_type(event_type: &str) -> Option<SseEventType> {
    if event_type == execution_events::WORKFLOW_STARTED
        || event_type == execution_events::WORKFLOW_COMPLETED
        || event_type == execution_events::WORKFLOW_PAUSED
        || event_type == execution_events::WORKFLOW_RESUMED
    {
        Some(SseEventType::StateChange)
    } else if event_type == execution_events::WORKFLOW_FAILED
        || event_type == execution_events::NODE_FAILED
    {
        Some(SseEventType::Error)
    } else if event_type == execution_events::NODE_COMPLETED {
        Some(SseEventType::NodeComplete)
    } else if event_type == execution_events::PROGRESS_UPDATE {
        Some(SseEventType::Progress)
    } else {
        // node.started, level.*, checkpoint.*, variable.updated, etc. → skip
        None
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

/// Internal state threaded through the `stream::unfold` combinator
struct SseStreamState {
    rx: tokio::sync::broadcast::Receiver<oxify_engine::WorkflowEvent>,
    exec_id: Uuid,
    query: SseQuery,
    last_event_id: u64,
    completed: bool,
}

/// Stream execution updates via SSE
///
/// Enhanced with event filtering, reconnection support, and heartbeats.
/// Events are pushed in real-time from the engine's EventBus — no polling.
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

    // Subscribe to the event bus before checking the initial state so we
    // cannot miss events that fire between the store read and subscription.
    let rx = state.event_bus.subscribe();

    // Seed: check whether the execution is already in a terminal state.
    // If so we mark completed immediately and the stream will drain any
    // buffered events then close.
    let initial_completed = state
        .execution_store
        .get(&id)
        .await
        .ok()
        .flatten()
        .map(|ctx| {
            matches!(
                ctx.state,
                oxify_model::ExecutionState::Completed
                    | oxify_model::ExecutionState::Failed(_)
                    | oxify_model::ExecutionState::Cancelled
            )
        })
        .unwrap_or(false);

    let sse_state = SseStreamState {
        rx,
        exec_id: id,
        query,
        last_event_id,
        completed: initial_completed,
    };

    let stream = stream::unfold(sse_state, |mut s| async move {
        if s.completed {
            return None;
        }

        let heartbeat_interval = Duration::from_secs(s.query.heartbeat_interval);

        loop {
            let heartbeat_enabled =
                s.query.heartbeat && s.query.is_event_enabled(SseEventType::Heartbeat);

            tokio::select! {
                recv_result = s.rx.recv() => {
                    match recv_result {
                        Ok(event) => {
                            // Only forward events for our execution
                            if event.execution_id != Some(s.exec_id) {
                                continue;
                            }

                            let Some(sse_type) = map_event_type(&event.event_type) else {
                                continue;
                            };

                            if !s.query.is_event_enabled(sse_type) {
                                continue;
                            }

                            s.last_event_id += 1;

                            // Detect terminal events so we close the stream
                            // after flushing this last event.
                            if event.event_type == execution_events::WORKFLOW_COMPLETED
                                || event.event_type == execution_events::WORKFLOW_FAILED
                            {
                                s.completed = true;
                            }

                            let sse_event = Event::default()
                                .event(sse_type.as_str())
                                .id(s.last_event_id.to_string())
                                .json_data(&event.payload)
                                .unwrap_or_else(|_| {
                                    Event::default()
                                        .event(SseEventType::Error.as_str())
                                        .data("serialization_error")
                                });

                            return Some((Ok(sse_event), s));
                        }
                        Err(RecvError::Lagged(_)) => {
                            // Slow consumer dropped some events — keep going
                            continue;
                        }
                        Err(RecvError::Closed) => {
                            // Bus shut down
                            return None;
                        }
                    }
                }

                _ = tokio::time::sleep(heartbeat_interval), if heartbeat_enabled => {
                    s.last_event_id += 1;
                    return Some((
                        Ok(Event::default()
                            .event(SseEventType::Heartbeat.as_str())
                            .id(s.last_event_id.to_string())
                            .data("ping")),
                        s,
                    ));
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
