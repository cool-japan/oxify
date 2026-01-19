//! Complete OxiFY Server Example
//!
//! This example demonstrates all ported components working together:
//! - oxify-server: HTTP server with middleware
//! - oxify-authn: JWT authentication
//! - oxify-vector: Vector search for RAG
//!
//! Run with: cargo run --bin server

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use oxify_authn::{JwtConfig, JwtManager, Permission, User};
use oxify_server::{ServerConfig, ServerRuntime};
use oxify_vector::{DistanceMetric, SearchConfig, VectorSearchIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, Level};

/// Application state shared across handlers
#[derive(Clone)]
struct AppState {
    jwt_manager: Arc<JwtManager>,
    vector_index: Arc<RwLock<VectorSearchIndex>>,
}

/// Login request
#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    #[allow(dead_code)]
    password: String,
}

/// Login response
#[derive(Debug, Serialize)]
struct LoginResponse {
    token: String,
    user: User,
}

/// Vector search request
#[derive(Debug, Deserialize)]
struct SearchRequest {
    query: Vec<f32>,
    k: usize,
}

/// Vector search response
#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Debug, Serialize)]
struct SearchResult {
    entity_id: String,
    score: f32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🚀 Starting OxiFY Complete Server Example");
    info!("");

    // Initialize JWT manager
    let jwt_config = JwtConfig::development();
    let jwt_manager = Arc::new(JwtManager::new(&jwt_config)?);
    info!("✓ JWT manager initialized");

    // Initialize vector search index with example documents
    let mut embeddings = HashMap::new();
    embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3, 0.4]);
    embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4, 0.5]);
    embeddings.insert("doc3".to_string(), vec![0.3, 0.4, 0.5, 0.6]);
    embeddings.insert("doc4".to_string(), vec![0.4, 0.5, 0.6, 0.7]);
    embeddings.insert("doc5".to_string(), vec![0.5, 0.6, 0.7, 0.8]);

    let mut search_config = SearchConfig::default();
    search_config.metric = DistanceMetric::Cosine;

    let mut vector_index = VectorSearchIndex::new(search_config);
    vector_index.build(&embeddings)?;
    let vector_index = Arc::new(RwLock::new(vector_index));
    info!("✓ Vector search index initialized with {} documents", embeddings.len());

    // Create application state
    let app_state = AppState {
        jwt_manager,
        vector_index,
    };

    // Build application routes
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/login", post(login_handler))
        .route("/search", post(search_handler))
        .with_state(app_state);

    // Create and run server
    let config = ServerConfig::development();
    let server = ServerRuntime::new(config).with_router(app);

    info!("");
    info!("🌟 Server ready at http://127.0.0.1:3000");
    info!("");
    info!("Try these endpoints:");
    info!("  POST /login  - Authenticate and get JWT token");
    info!("  POST /search - Search vectors with cosine similarity");
    info!("");
    info!("Example curl commands:");
    info!(r#"  curl -X POST http://localhost:3000/login -H "Content-Type: application/json" -d '{{"username":"alice","password":"secret"}}"#);
    info!(r#"  curl -X POST http://localhost:3000/search -H "Content-Type: application/json" -d '{{"query":[0.15,0.25,0.35,0.45],"k":3}}"#);
    info!("");

    server.run().await?;

    Ok(())
}

/// Root handler
async fn root_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "OxiFY Complete Server",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "running",
        "components": {
            "authentication": "JWT (oxify-authn)",
            "authorization": "ReBAC (oxify-authz)",
            "vector_search": "In-memory (oxify-vector)",
            "server": "Axum (oxify-server)"
        },
        "endpoints": {
            "/": "This information",
            "/login": "POST - Authenticate and get JWT token",
            "/search": "POST - Vector similarity search"
        }
    }))
}

/// Login handler - Issues JWT tokens
async fn login_handler(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    info!("Login request for user: {}", req.username);

    // In a real app, verify password here
    // For demo, accept any password and create a user

    // Create user with permissions
    let user = User {
        username: req.username.clone(),
        roles: vec!["user".to_string()],
        email: Some(format!("{}@example.com", req.username)),
        full_name: Some(req.username.clone()),
        last_login: Some(chrono::Utc::now()),
        permissions: vec![Permission::Read, Permission::Write],
    };

    // Generate JWT token
    let token = state
        .jwt_manager
        .generate_token(&user)
        .map_err(|e| {
            tracing::error!("Failed to generate token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!("✓ Generated token for user: {}", user.username);

    Ok(Json(LoginResponse { token, user }))
}

/// Vector search handler - Demonstrates vector similarity search for RAG
async fn search_handler(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, StatusCode> {
    info!("Search request for top-{} results", req.k);

    // Validate query dimensions
    if req.query.len() != 4 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Perform vector similarity search
    let index = state.vector_index.read().await;
    let results = index
        .search(&req.query, req.k)
        .map_err(|e| {
            tracing::error!("Search failed: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    let search_results: Vec<SearchResult> = results
        .into_iter()
        .map(|r| SearchResult {
            entity_id: r.entity_id,
            score: r.score,
        })
        .collect();

    info!("✓ Found {} results", search_results.len());

    Ok(Json(SearchResponse {
        results: search_results,
    }))
}
