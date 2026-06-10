//! Append-only event store for [`WorkflowEvent`] persistence and replay.
//!
//! Events are stored in JSONL format (one JSON-serialised [`StoredEvent`] per
//! line).  The [`attach_to_bus`] function mirrors the NATS bridge pattern: it
//! subscribes to the broadcast channel of an [`EventBus`], appends every
//! received event to the backing store in a background task, and returns an
//! [`EventRecorder`] handle whose [`EventRecorder::shutdown`] method aborts
//! the task.  On startup, call [`EventStore::replay`] to republish all
//! persisted events to a fresh [`EventBus`] so downstream subscribers can
//! reconstruct state.

use crate::event_bus::{EventBus, WorkflowEvent};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

// ─── Error type ─────────────────────────────────────────────────────────────

/// Errors that can be produced by the event store.
#[derive(Debug, Error)]
pub enum EventStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialisation error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Broadcast channel lagged by {0} events")]
    Lagged(u64),
}

/// Convenience `Result` alias for event store operations.
pub type Result<T> = std::result::Result<T, EventStoreError>;

// ─── StoredEvent ────────────────────────────────────────────────────────────

/// A [`WorkflowEvent`] wrapped with a monotonically-increasing sequence number
/// and a wall-clock timestamp for durable storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    /// Monotonically-increasing sequence number assigned on `append`.
    pub seq: u64,

    /// Wall-clock time at which the event was persisted.
    pub recorded_at: DateTime<Utc>,

    /// The original workflow event.
    pub event: WorkflowEvent,
}

// ─── EventStore trait ────────────────────────────────────────────────────────

/// Trait for append-only event persistence.
///
/// Implementors must be `Send + Sync` so they can be wrapped in `Arc` and
/// shared across async tasks.
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Append `event` to the store and return the newly assigned sequence number.
    async fn append(&self, event: &WorkflowEvent) -> Result<u64>;

    /// Load all stored events in ascending sequence order.
    async fn load(&self) -> Result<Vec<StoredEvent>>;

    /// Load all events whose sequence number is **strictly greater than** `seq`.
    async fn load_since(&self, seq: u64) -> Result<Vec<StoredEvent>>;

    /// Re-publish every stored event to `bus` in order.
    ///
    /// Returns the number of events that were replayed.
    async fn replay(&self, bus: &Arc<EventBus>) -> Result<usize>;
}

// ─── InMemoryEventStore ──────────────────────────────────────────────────────

/// An in-process, non-durable event store backed by a `Vec` protected by a
/// `tokio::sync::RwLock`.
pub struct InMemoryEventStore {
    events: RwLock<Vec<StoredEvent>>,
    next_seq: AtomicU64,
}

impl InMemoryEventStore {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self {
            events: RwLock::new(Vec::new()),
            next_seq: AtomicU64::new(1),
        }
    }
}

impl Default for InMemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn append(&self, event: &WorkflowEvent) -> Result<u64> {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let stored = StoredEvent {
            seq,
            recorded_at: Utc::now(),
            event: event.clone(),
        };
        self.events.write().await.push(stored);
        Ok(seq)
    }

    async fn load(&self) -> Result<Vec<StoredEvent>> {
        Ok(self.events.read().await.clone())
    }

    async fn load_since(&self, seq: u64) -> Result<Vec<StoredEvent>> {
        Ok(self
            .events
            .read()
            .await
            .iter()
            .filter(|e| e.seq > seq)
            .cloned()
            .collect())
    }

    async fn replay(&self, bus: &Arc<EventBus>) -> Result<usize> {
        let events = self.load().await?;
        let count = events.len();
        for stored in events {
            let _ = bus.publish(stored.event).await;
        }
        Ok(count)
    }
}

// ─── FileEventStore ──────────────────────────────────────────────────────────

/// A file-backed event store that persists events as JSONL (one
/// [`StoredEvent`] JSON per line).
///
/// Appends are atomic at the line level.  On `load`, any partial trailing
/// line (e.g. from an interrupted write) is silently tolerated.
pub struct FileEventStore {
    path: PathBuf,
    next_seq: AtomicU64,
    write_lock: tokio::sync::Mutex<()>,
}

impl FileEventStore {
    /// Open (or create) a JSONL event log at `path`.
    ///
    /// The highest existing sequence number is scanned from the file at
    /// construction time so that subsequent appends continue monotonically.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let next_seq = Self::scan_next_seq(&path)?;
        Ok(Self {
            path,
            next_seq: AtomicU64::new(next_seq),
            write_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Scan the file to find the highest sequence number + 1 (or 1 if empty).
    fn scan_next_seq(path: &PathBuf) -> Result<u64> {
        if !path.exists() {
            return Ok(1);
        }
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut max_seq: u64 = 0;
        for line in reader.lines() {
            let line = match line {
                Ok(l) if !l.trim().is_empty() => l,
                _ => continue,
            };
            if let Ok(stored) = serde_json::from_str::<StoredEvent>(&line) {
                if stored.seq > max_seq {
                    max_seq = stored.seq;
                }
            }
        }
        Ok(max_seq + 1)
    }

    /// Parse all complete lines from the file, tolerating a partial trailing line.
    fn read_all_events(path: &PathBuf) -> Result<Vec<StoredEvent>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = match line {
                Ok(l) if !l.trim().is_empty() => l,
                _ => continue,
            };
            match serde_json::from_str::<StoredEvent>(&line) {
                Ok(stored) => events.push(stored),
                Err(_) => {
                    // Tolerate partial / corrupt lines (e.g. interrupted write)
                    tracing::warn!(
                        "event_store: skipping malformed line while reading {:?}",
                        path
                    );
                }
            }
        }
        Ok(events)
    }
}

#[async_trait]
impl EventStore for FileEventStore {
    async fn append(&self, event: &WorkflowEvent) -> Result<u64> {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let stored = StoredEvent {
            seq,
            recorded_at: Utc::now(),
            event: event.clone(),
        };
        let line = serde_json::to_string(&stored)?;

        // Serialise file writes under a mutex to prevent interleaving.
        let _guard = self.write_lock.lock().await;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", line)?;
        Ok(seq)
    }

    async fn load(&self) -> Result<Vec<StoredEvent>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || Self::read_all_events(&path))
            .await
            .map_err(|e| EventStoreError::Io(std::io::Error::other(e.to_string())))?
    }

    async fn load_since(&self, seq: u64) -> Result<Vec<StoredEvent>> {
        Ok(self
            .load()
            .await?
            .into_iter()
            .filter(|e| e.seq > seq)
            .collect())
    }

    async fn replay(&self, bus: &Arc<EventBus>) -> Result<usize> {
        let events = self.load().await?;
        let count = events.len();
        for stored in events {
            let _ = bus.publish(stored.event).await;
        }
        Ok(count)
    }
}

// ─── EventRecorder ───────────────────────────────────────────────────────────

/// Handle to the background task that records events from an [`EventBus`] into
/// an [`EventStore`].
///
/// Dropping or calling [`EventRecorder::shutdown`] aborts the task.
pub struct EventRecorder {
    handle: JoinHandle<()>,
}

impl EventRecorder {
    /// Abort the background recording task.
    pub fn shutdown(self) {
        self.handle.abort();
    }
}

// ─── attach_to_bus ───────────────────────────────────────────────────────────

/// Subscribe `store` to `bus` and return an [`EventRecorder`] that keeps the
/// background task alive.
///
/// The implementation mirrors the NATS bridge: it subscribes to the broadcast
/// channel, loops over received events, appends each to `store`, and warns on
/// lag without exiting.
pub fn attach_to_bus(bus: Arc<EventBus>, store: Arc<dyn EventStore>) -> EventRecorder {
    let mut rx = bus.subscribe();

    let handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let _ = store.append(&event).await;
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("event_store: broadcast channel lagged by {} events", n);
                    // Continue — do not exit on lag.
                }
                Err(RecvError::Closed) => {
                    tracing::info!("event_store: event bus closed; recorder task exiting");
                    break;
                }
            }
        }
    });

    EventRecorder { handle }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;
    use crate::event_bus::WorkflowEvent;
    use std::sync::Arc;
    use tokio::time::Duration;
    use uuid::Uuid;

    /// Helper: build a dummy [`WorkflowEvent`] for testing.
    fn make_event(event_type: &str) -> WorkflowEvent {
        WorkflowEvent::new(
            event_type.to_string(),
            Uuid::new_v4(),
            serde_json::json!({"test": true}),
        )
    }

    // ── 1. InMemoryEventStore: append + load ─────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_store_append_and_load() {
        let store = InMemoryEventStore::new();

        for i in 0..5_u64 {
            let seq = store.append(&make_event("test.event")).await.unwrap();
            assert_eq!(
                seq,
                i + 1,
                "sequence numbers must start at 1 and be monotonic"
            );
        }

        let events = store.load().await.unwrap();
        assert_eq!(events.len(), 5);

        // Verify strict monotonic ordering
        for (idx, ev) in events.iter().enumerate() {
            assert_eq!(ev.seq, (idx + 1) as u64);
        }
    }

    // ── 2. InMemoryEventStore: load_since ────────────────────────────────────

    #[tokio::test]
    async fn test_in_memory_store_load_since() {
        let store = InMemoryEventStore::new();

        for _ in 0..10 {
            store.append(&make_event("test.event")).await.unwrap();
        }

        let after_5 = store.load_since(5).await.unwrap();
        assert_eq!(after_5.len(), 5, "load_since(5) should return seqs 6-10");
        assert_eq!(after_5.first().unwrap().seq, 6);
        assert_eq!(after_5.last().unwrap().seq, 10);
    }

    // ── 3. FileEventStore: round-trip ────────────────────────────────────────

    #[tokio::test]
    async fn test_file_store_round_trip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let tmp_path =
            std::env::temp_dir().join(format!("oxify_event_store_roundtrip_{}.jsonl", suffix));

        let n = 7_usize;
        {
            let store = FileEventStore::open(&tmp_path).unwrap();
            for _ in 0..n {
                store.append(&make_event("file.event")).await.unwrap();
            }
        } // store dropped — file handle closed

        // Re-open and verify all events persisted
        {
            let store = FileEventStore::open(&tmp_path).unwrap();
            let events = store.load().await.unwrap();
            assert_eq!(events.len(), n, "all {} events should survive re-open", n);
            for (idx, ev) in events.iter().enumerate() {
                assert_eq!(
                    ev.seq,
                    (idx + 1) as u64,
                    "sequence must be monotonic after reload"
                );
            }
        }

        let _ = std::fs::remove_file(&tmp_path);
    }

    // ── 4. FileEventStore: replay ────────────────────────────────────────────

    #[tokio::test]
    async fn test_file_store_replay() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let tmp_path =
            std::env::temp_dir().join(format!("oxify_event_store_replay_{}.jsonl", suffix));

        let n = 3_usize;

        // Write events to file
        {
            let store = FileEventStore::open(&tmp_path).unwrap();
            for _ in 0..n {
                store.append(&make_event("replay.event")).await.unwrap();
            }
        }

        // Replay to a fresh bus and receive via subscriber
        let bus = Arc::new(EventBus::new(64));
        let mut rx = bus.subscribe();

        let store = FileEventStore::open(&tmp_path).unwrap();
        let replayed = store.replay(&bus).await.unwrap();
        assert_eq!(
            replayed, n,
            "replay should return the count of events replayed"
        );

        // Collect replayed events with timeout
        let mut received = Vec::new();
        for _ in 0..n {
            let res = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
            match res {
                Ok(Ok(ev)) => received.push(ev),
                Ok(Err(e)) => panic!("recv error during replay: {}", e),
                Err(_) => panic!("timeout waiting for replayed event"),
            }
        }
        assert_eq!(received.len(), n);

        let _ = std::fs::remove_file(&tmp_path);
    }

    // ── 5. attach_to_bus ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_attach_to_bus() {
        let bus = Arc::new(EventBus::new(128));
        let store = Arc::new(InMemoryEventStore::new());

        let _recorder = attach_to_bus(bus.clone(), store.clone());

        // Publish 5 events
        for _ in 0..5 {
            bus.publish(make_event("attach.event")).await.unwrap();
        }

        // Poll until all 5 arrive (up to 50 × 10 ms = 500 ms)
        let mut count = 0_usize;
        for _ in 0..50 {
            count = store.load().await.unwrap().len();
            if count >= 5 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(
            count, 5,
            "all 5 published events must be recorded in the store"
        );
    }

    // ── 6. check_triggers fires a secondary event ─────────────────────────────

    #[tokio::test]
    async fn test_check_triggers_fires_event() {
        use crate::event_bus::WorkflowTrigger;
        use oxify_model::{Node, NodeKind, Workflow, WorkflowMetadata};

        let bus = Arc::new(EventBus::new(128));
        let mut rx = bus.subscribe();

        // Build a minimal workflow for the trigger to reference
        let trigger_workflow = Workflow {
            metadata: WorkflowMetadata::new("trigger-workflow".to_string()),
            nodes: vec![
                Node::new("Start".to_string(), NodeKind::Start),
                Node::new("End".to_string(), NodeKind::End),
            ],
            edges: vec![],
        };

        // Register a trigger that fires on "user.signup"
        let trigger = WorkflowTrigger::new("user.signup".to_string(), trigger_workflow);
        let trigger_id = trigger.id;
        bus.register_trigger(trigger).await;

        // Publish the matching event — this causes `check_triggers` to fire and
        // publish a secondary "workflow.trigger.fired" event.
        bus.publish(WorkflowEvent::new(
            "user.signup".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        ))
        .await
        .unwrap();

        // Poll subscriber for the "workflow.trigger.fired" secondary event
        let mut found: Option<WorkflowEvent> = None;
        for _ in 0..50 {
            while let Ok(ev) = rx.try_recv() {
                if ev.event_type == "workflow.trigger.fired" {
                    found = Some(ev);
                    break;
                }
            }
            if found.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let fired = found.expect("workflow.trigger.fired event should have been published");
        let payload_trigger_id = fired
            .payload
            .get("trigger_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        assert_eq!(
            payload_trigger_id,
            trigger_id.to_string().as_str(),
            "fired event payload must contain the matching trigger_id"
        );
    }
}
