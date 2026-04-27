//! # oxify-connect-comm
//!
//! Communication provider connectors for OxiFY.  Ships three back-ends:
//!
//! * **Slack** (feature `slack`, enabled by default) — Slack Web API over HTTPS
//!   using a Bot Token.
//! * **SMTP** (feature `smtp`) — Email delivery via `lettre` / Tokio async.
//! * **Mock** (always compiled) — In-memory recorder for unit tests.
//!
//! All back-ends implement the [`providers::MessageProvider`] trait so they can
//! be swapped transparently.
//!
//! ## Quick-start
//!
//! ```rust,no_run
//! use oxify_connect_comm::{MockMessageProvider, providers::MessageProvider};
//! use oxify_connect_comm::types::{OutgoingMessage, Recipient};
//!
//! #[tokio::main]
//! async fn main() {
//!     let provider = MockMessageProvider::new();
//!     provider
//!         .send_message(OutgoingMessage::simple(
//!             Recipient::Channel("#general".to_owned()),
//!             "Hello, world!",
//!         ))
//!         .await
//!         .unwrap();
//! }
//! ```

pub mod errors;
pub mod providers;
pub mod types;

// Convenience re-exports at crate root.
pub use providers::MessageProvider;
pub use providers::MockMessageProvider;

#[cfg(feature = "slack")]
pub use providers::slack::{SlackConfig, SlackProvider};

#[cfg(feature = "smtp")]
pub use providers::smtp::{SmtpConfig, SmtpProvider};
