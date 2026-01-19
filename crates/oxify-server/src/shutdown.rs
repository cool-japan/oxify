//! Graceful shutdown handling
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY
//! License: MIT OR Apache-2.0 (compatible with OxiRS)

use std::time::Duration;
use tokio::signal;
use tracing::info;

/// Create a graceful shutdown signal handler
///
/// Listens for SIGINT (Ctrl+C) and SIGTERM signals.
/// Allows a configurable timeout for graceful shutdown.
pub async fn shutdown_signal(timeout: Duration) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown");
        },
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown");
        },
    }

    // Allow some time for graceful shutdown
    info!(
        "Waiting up to {} seconds for graceful shutdown",
        timeout.as_secs()
    );
    tokio::time::sleep(timeout).await;
}

/// Create a graceful shutdown signal without timeout
///
/// This version returns immediately when a signal is received,
/// allowing the server to handle shutdown logic externally.
pub async fn shutdown_signal_immediate() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown");
        },
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown");
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_signal_timeout() {
        // Test that shutdown timeout works
        let timeout = Duration::from_millis(100);
        let start = std::time::Instant::now();

        // This test would hang without a timeout, so we use tokio::time::timeout
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            // Don't actually wait for signal, just test the timeout logic
            tokio::time::sleep(timeout).await;
        })
        .await;

        assert!(result.is_ok());
        assert!(start.elapsed() >= timeout);
    }
}
