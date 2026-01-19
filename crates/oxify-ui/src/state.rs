//! Application state for the UI server

use crate::api::ApiClient;
use oxify_authn::JwtManager;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared application state
pub struct AppState {
    /// API base URL for backend calls
    pub api_base_url: String,
    /// API client for backend communication
    pub api_client: ApiClient,
    /// Whether to use mock data (when API is unavailable)
    pub use_mock_data: Arc<RwLock<bool>>,
    /// Current user session (simplified for now)
    pub sessions: Arc<RwLock<Sessions>>,
    /// JWT manager for authentication
    pub jwt_manager: Arc<JwtManager>,
}

impl AppState {
    /// Create new application state
    pub fn new(api_base_url: String, use_mock_data: bool) -> anyhow::Result<Self> {
        let api_client = ApiClient::new(api_base_url.clone());
        let jwt_config = crate::auth::create_jwt_config();
        let jwt_manager = JwtManager::new(&jwt_config)
            .map_err(|e| anyhow::anyhow!("Failed to create JWT manager: {}", e))?;

        Ok(Self {
            api_base_url,
            api_client,
            use_mock_data: Arc::new(RwLock::new(use_mock_data)),
            sessions: Arc::new(RwLock::new(Sessions::default())),
            jwt_manager: Arc::new(jwt_manager),
        })
    }

    /// Enable mock data mode
    pub async fn enable_mock_data(&self) {
        let mut mock = self.use_mock_data.write().await;
        *mock = true;
    }

    /// Disable mock data mode (use real API)
    pub async fn disable_mock_data(&self) {
        let mut mock = self.use_mock_data.write().await;
        *mock = false;
    }

    /// Check if mock data mode is enabled
    pub async fn is_mock_data_enabled(&self) -> bool {
        *self.use_mock_data.read().await
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("api_base_url", &self.api_base_url)
            .finish()
    }
}

/// Session storage (in-memory for now)
#[derive(Debug, Default)]
pub struct Sessions {
    // Simplified session storage
    // In production, use Redis or similar
    pub active_count: usize,
}
