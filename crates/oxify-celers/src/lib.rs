//! Bridge OxiFY workflow execution to the CeleRS distributed task queue.
//!
//! # Architecture
//!
//! This crate wires OxiFY's workflow engine into CeleRS's distributed task queue
//! infrastructure. It provides three main components:
//!
//! - **[`task`]**: Defines [`OxifyWorkflowTask`](task::OxifyWorkflowTask), the CeleRS
//!   [`Task`](celers_core::Task) implementation that executes a serialised
//!   [`Workflow`](oxify_model::Workflow) via the OxiFY engine.
//!
//! - **[`client`]**: [`OxifyCelersClient`](client::OxifyCelersClient) submits workflows
//!   to the broker and polls the result backend until completion.
//!
//! - **[`worker`]**: [`run_worker`](worker::run_worker) bootstraps a dequeue loop that
//!   (unlike the stock `celers::Worker`) **stores results** so clients can poll them.
//!
//! # Feature flags
//!
//! | Flag | Enables |
//! |------|---------|
//! | `redis` | `RedisBroker`, `RedisResultBackend`, all task/client code |
//! | `worker` | `run_worker` / `run_loop` (implies `redis`) |
//! | `test-utils` | `celers::dev_utils::MockBroker` for hermetic tests |

pub use error::CelersBridgeError;
pub type Result<T> = std::result::Result<T, CelersBridgeError>;

mod error;

#[cfg(feature = "redis")]
pub mod client;
#[cfg(feature = "redis")]
pub mod task;
#[cfg(feature = "worker")]
pub mod worker;

/// The canonical CeleRS task name — must match on both submit and worker sides.
pub const OXIFY_WORKFLOW_TASK_NAME: &str = "oxify.workflow.execute";
