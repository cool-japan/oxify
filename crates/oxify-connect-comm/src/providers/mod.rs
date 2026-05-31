#[cfg(feature = "discord")]
pub mod discord;
#[cfg(feature = "firebase")]
pub mod firebase_fcm;
pub mod mock;
#[cfg(feature = "onesignal")]
pub mod onesignal;
#[cfg(feature = "slack")]
pub mod slack;
#[cfg(feature = "smtp")]
pub mod smtp;
#[cfg(feature = "twilio")]
pub mod twilio;

pub use mock::MockMessageProvider;

use crate::{errors, types};

// ---------------------------------------------------------------------------
// MessageProvider trait
// ---------------------------------------------------------------------------

/// Core trait implemented by every communication back-end.
///
/// All methods are async so that implementations can perform I/O without
/// blocking the executor.  The trait is object-safe through `async_trait`.
#[async_trait::async_trait]
pub trait MessageProvider: Send + Sync {
    /// A human-readable name for this provider (e.g. `"slack"`, `"smtp"`).
    fn provider_name(&self) -> &str;

    /// Send `msg` and return a receipt confirming acceptance by the provider.
    async fn send_message(
        &self,
        msg: types::OutgoingMessage,
    ) -> errors::Result<types::MessageReceipt>;

    /// Return the list of channels (or mailing lists, queues, etc.) that this
    /// provider exposes.  Providers that do not have the concept of a "channel"
    /// should return `Err(CommError::Unsupported(...))`.
    async fn list_channels(&self) -> errors::Result<Vec<types::Channel>>;
}
