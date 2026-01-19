//! # OxiFY Web UI
//!
//! A server-side rendered web interface for OxiFY workflow orchestration platform.
//!
//! ## Tech Stack
//!
//! - **Axum**: High-performance HTTP server
//! - **Askama**: Type-safe, compile-time checked templates (Jinja2-style)
//! - **HTMX**: Hypermedia-driven interactions with minimal JavaScript
//! - **Tailwind CSS**: Utility-first styling
//!
//! ## Features
//!
//! - Workflow management (list, create, edit, delete)
//! - DAG visual editor
//! - Real-time execution monitoring via SSE
//! - Dark mode support
//! - Responsive design

pub mod api;
pub mod auth;
pub mod cache;
pub mod error;
pub mod handlers;
pub mod mock;
pub mod routes;
pub mod state;
pub mod templates;

pub use api::ApiClient;
pub use error::UiError;
pub use state::AppState;

use axum::Router;
use std::sync::Arc;
use tower_http::{compression::CompressionLayer, services::ServeDir, trace::TraceLayer};

/// Create the application router with all routes configured
pub fn create_router(state: Arc<AppState>) -> Router {
    let static_service = ServeDir::new("crates/oxify-ui/static")
        .precompressed_gzip()
        .precompressed_br();

    Router::new()
        .merge(routes::ui_routes())
        .merge(routes::api_routes())
        .nest_service("/static", static_service)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Configuration for the UI server
#[derive(Debug, Clone)]
pub struct UiConfig {
    /// Host to bind to
    pub host: String,
    /// Port to listen on
    pub port: u16,
    /// Enable hot reload in development
    pub hot_reload: bool,
    /// API base URL for backend
    pub api_base_url: String,
    /// Use mock data instead of real API (for development/testing)
    pub use_mock_data: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
            hot_reload: cfg!(debug_assertions),
            api_base_url: "http://localhost:8080".to_string(),
            use_mock_data: true, // Default to mock in development
        }
    }
}

impl UiConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("OXIFY_UI_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("OXIFY_UI_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            hot_reload: std::env::var("OXIFY_UI_HOT_RELOAD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(cfg!(debug_assertions)),
            api_base_url: std::env::var("OXIFY_API_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            use_mock_data: std::env::var("OXIFY_UI_USE_MOCK")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(true),
        }
    }
}
