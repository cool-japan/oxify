//! OxiFY Web UI Server
//!
//! A server-side rendered web interface using Axum + Askama + HTMX.

use oxify_ui::{create_router, AppState, UiConfig};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "oxify_ui=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = UiConfig::from_env();

    tracing::info!(
        "Starting OxiFY UI server on {}:{} (mock mode: {})",
        config.host,
        config.port,
        config.use_mock_data
    );

    // Create application state
    let state = Arc::new(AppState::new(
        config.api_base_url.clone(),
        config.use_mock_data,
    )?);

    // Create router
    let app = create_router(state);

    // Add hot reload in development
    #[cfg(feature = "hot-reload")]
    let app = {
        tracing::info!("Hot reload enabled");
        app.layer(tower_livereload::LiveReloadLayer::new())
    };

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;

    tracing::info!("Listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
