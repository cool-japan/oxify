//! Vector Store Management Handlers
//!
//! Provides REST API endpoints for managing vector collections and operations.

use crate::handlers::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use oxify_connect_vector::{
    DeleteRequest, InsertRequest, SearchRequest, SearchResult as VectorSearchResult, VectorProvider,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// ==================== Types ====================

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateCollectionRequest {
    pub name: String,
    pub dimension: usize,
    pub provider: String, // "qdrant", "pgvector", "chromadb", "pinecone", "weaviate", "milvus"
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CollectionInfo {
    pub name: String,
    pub dimension: usize,
    pub provider: String,
    pub vector_count: Option<usize>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InsertVectorsRequest {
    pub vectors: Vec<VectorWithId>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VectorWithId {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchVectorsRequest {
    pub query: Vec<f32>,
    pub top_k: usize,
    pub filter: Option<serde_json::Value>,
    pub score_threshold: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct CollectionMetadata {
    pub name: String,
    pub dimension: usize,
    pub provider_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ==================== Vector Store Registry ====================

/// Registry for managing multiple vector providers
pub struct VectorStoreRegistry {
    collections: Arc<RwLock<HashMap<String, CollectionMetadata>>>,
    providers: Arc<RwLock<HashMap<String, Box<dyn VectorProvider>>>>,
}

impl Default for VectorStoreRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorStoreRegistry {
    pub fn new() -> Self {
        Self {
            collections: Arc::new(RwLock::new(HashMap::new())),
            providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_collection(
        &self,
        name: String,
        dimension: usize,
        provider: Box<dyn VectorProvider>,
        provider_name: String,
    ) -> Result<CollectionMetadata, String> {
        // Create collection in provider
        provider
            .create_collection(&name, dimension)
            .await
            .map_err(|e| format!("Failed to create collection: {}", e))?;

        // Store metadata
        let metadata = CollectionMetadata {
            name: name.clone(),
            dimension,
            provider_name,
            created_at: chrono::Utc::now(),
        };

        let mut collections = self.collections.write().await;
        collections.insert(name.clone(), metadata.clone());

        let mut providers = self.providers.write().await;
        providers.insert(name, provider);

        Ok(metadata)
    }

    pub async fn get_collection(&self, name: &str) -> Option<CollectionMetadata> {
        let collections = self.collections.read().await;
        collections.get(name).cloned()
    }

    pub async fn list_collections(&self) -> Vec<CollectionMetadata> {
        let collections = self.collections.read().await;
        collections.values().cloned().collect()
    }

    pub async fn delete_collection(&self, name: &str) -> Result<(), String> {
        let provider = {
            let providers = self.providers.read().await;
            providers.get(name).is_some()
        };

        if !provider {
            return Err("Collection not found".to_string());
        }

        // Note: VectorProvider trait doesn't have delete_collection method
        // We'll just remove from registry for now
        let mut collections = self.collections.write().await;
        collections.remove(name);

        let mut providers = self.providers.write().await;
        providers.remove(name);

        Ok(())
    }

    pub async fn insert_vectors(
        &self,
        collection: &str,
        vectors: Vec<(String, Vec<f32>, Option<serde_json::Value>)>,
    ) -> Result<(), String> {
        let providers = self.providers.read().await;
        let provider = providers
            .get(collection)
            .ok_or_else(|| "Collection not found".to_string())?;

        for (id, vector, payload) in vectors {
            let request = InsertRequest {
                collection: collection.to_string(),
                id,
                vector,
                payload: payload.unwrap_or(serde_json::json!({})),
            };

            provider
                .insert(request)
                .await
                .map_err(|e| format!("Failed to insert vector: {}", e))?;
        }

        Ok(())
    }

    pub async fn search_vectors(
        &self,
        collection: &str,
        query: Vec<f32>,
        top_k: usize,
        filter: Option<serde_json::Value>,
        score_threshold: Option<f32>,
    ) -> Result<Vec<VectorSearchResult>, String> {
        let providers = self.providers.read().await;
        let provider = providers
            .get(collection)
            .ok_or_else(|| "Collection not found".to_string())?;

        let request = SearchRequest {
            collection: collection.to_string(),
            query,
            top_k,
            filter,
            score_threshold: score_threshold.map(|t| t as f64),
        };

        provider
            .search(request)
            .await
            .map_err(|e| format!("Failed to search vectors: {}", e))
    }

    pub async fn delete_vector(&self, collection: &str, id: &str) -> Result<(), String> {
        let providers = self.providers.read().await;
        let provider = providers
            .get(collection)
            .ok_or_else(|| "Collection not found".to_string())?;

        let request = DeleteRequest {
            collection: collection.to_string(),
            ids: vec![id.to_string()],
        };

        provider
            .delete(request)
            .await
            .map_err(|e| format!("Failed to delete vector: {}", e))?;

        Ok(())
    }
}

// ==================== Handlers ====================

/// Create a new vector collection
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/vectors/collections",
    request_body = CreateCollectionRequest,
    responses(
        (status = 201, description = "Collection created", body = CollectionInfo),
        (status = 400, description = "Invalid request"),
        (status = 409, description = "Collection already exists"),
    )
))]
pub async fn create_collection(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<CollectionInfo>), (StatusCode, String)> {
    info!(
        "Creating collection: {} (provider: {}, dimension: {})",
        req.name, req.provider, req.dimension
    );

    // Get or create vector registry from state
    let registry = state.vector_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Vector store not configured".to_string(),
        )
    })?;

    // Check if collection already exists
    if registry.get_collection(&req.name).await.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "Collection already exists".to_string(),
        ));
    }

    // Create provider based on type
    let provider: Box<dyn VectorProvider> = create_provider(&req.provider, &req.config)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to create provider: {}", e),
            )
        })?;

    // Create collection
    let metadata = registry
        .create_collection(
            req.name.clone(),
            req.dimension,
            provider,
            req.provider.clone(),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let info = CollectionInfo {
        name: metadata.name,
        dimension: metadata.dimension,
        provider: metadata.provider_name,
        vector_count: Some(0),
        created_at: metadata.created_at,
    };

    Ok((StatusCode::CREATED, Json(info)))
}

/// List all vector collections
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/vectors/collections",
    responses(
        (status = 200, description = "List of collections", body = Vec<CollectionInfo>),
    )
))]
pub async fn list_collections(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CollectionInfo>>, StatusCode> {
    let registry = state
        .vector_registry
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let collections = registry.list_collections().await;

    let infos = collections
        .into_iter()
        .map(|meta| CollectionInfo {
            name: meta.name,
            dimension: meta.dimension,
            provider: meta.provider_name,
            vector_count: None, // Would need to query provider for actual count
            created_at: meta.created_at,
        })
        .collect();

    Ok(Json(infos))
}

/// Get collection info
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/vectors/collections/{name}",
    params(
        ("name" = String, Path, description = "Collection name")
    ),
    responses(
        (status = 200, description = "Collection info", body = CollectionInfo),
        (status = 404, description = "Collection not found"),
    )
))]
pub async fn get_collection(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<CollectionInfo>, StatusCode> {
    let registry = state
        .vector_registry
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let metadata = registry
        .get_collection(&name)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let info = CollectionInfo {
        name: metadata.name,
        dimension: metadata.dimension,
        provider: metadata.provider_name,
        vector_count: None,
        created_at: metadata.created_at,
    };

    Ok(Json(info))
}

/// Delete a vector collection
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/v1/vectors/collections/{name}",
    params(
        ("name" = String, Path, description = "Collection name")
    ),
    responses(
        (status = 204, description = "Collection deleted"),
        (status = 404, description = "Collection not found"),
    )
))]
pub async fn delete_collection(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let registry = state
        .vector_registry
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    registry
        .delete_collection(&name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Insert vectors into a collection
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/vectors/collections/{name}/vectors",
    params(
        ("name" = String, Path, description = "Collection name")
    ),
    request_body = InsertVectorsRequest,
    responses(
        (status = 201, description = "Vectors inserted"),
        (status = 404, description = "Collection not found"),
    )
))]
pub async fn insert_vectors(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<InsertVectorsRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let registry = state.vector_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Vector store not configured".to_string(),
        )
    })?;

    let vectors: Vec<(String, Vec<f32>, Option<serde_json::Value>)> = req
        .vectors
        .into_iter()
        .map(|v| (v.id, v.vector, v.metadata))
        .collect();

    registry
        .insert_vectors(&name, vectors)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::CREATED)
}

/// Search vectors in a collection
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/vectors/collections/{name}/search",
    params(
        ("name" = String, Path, description = "Collection name")
    ),
    request_body = SearchVectorsRequest,
    responses(
        (status = 200, description = "Search results", body = Vec<SearchResult>),
        (status = 404, description = "Collection not found"),
    )
))]
pub async fn search_vectors(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<SearchVectorsRequest>,
) -> Result<Json<Vec<SearchResult>>, (StatusCode, String)> {
    let registry = state.vector_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Vector store not configured".to_string(),
        )
    })?;

    let results = registry
        .search_vectors(&name, req.query, req.top_k, req.filter, req.score_threshold)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let search_results = results
        .into_iter()
        .map(|r| SearchResult {
            id: r.id,
            score: r.score as f32,
            metadata: Some(r.payload),
        })
        .collect();

    Ok(Json(search_results))
}

/// Delete vectors from a collection
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/v1/vectors/collections/{name}/vectors/{id}",
    params(
        ("name" = String, Path, description = "Collection name"),
        ("id" = String, Path, description = "Vector ID")
    ),
    responses(
        (status = 204, description = "Vector deleted"),
        (status = 404, description = "Collection or vector not found"),
    )
))]
pub async fn delete_vector(
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let registry = state.vector_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Vector store not configured".to_string(),
        )
    })?;

    registry
        .delete_vector(&name, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(StatusCode::NO_CONTENT)
}

// ==================== Provider Factory ====================

/// Create a vector provider based on configuration
async fn create_provider(
    provider_type: &str,
    _config: &Option<serde_json::Value>,
) -> Result<Box<dyn VectorProvider>, String> {
    match provider_type {
        "qdrant" => {
            let url =
                std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".to_string());
            let api_key = std::env::var("QDRANT_API_KEY").ok();

            let provider = if let Some(key) = api_key {
                oxify_connect_vector::QdrantProvider::with_api_key(&url, &key)
                    .await
                    .map_err(|e| format!("Failed to create Qdrant provider: {}", e))?
            } else {
                oxify_connect_vector::QdrantProvider::new(&url)
                    .await
                    .map_err(|e| format!("Failed to create Qdrant provider: {}", e))?
            };

            Ok(Box::new(provider))
        }
        // pgvector disabled - requires PostgreSQL
        "pgvector" => Err("pgvector provider is disabled (requires PostgreSQL)".to_string()),
        "chromadb" => {
            let url = std::env::var("CHROMADB_URL")
                .unwrap_or_else(|_| "http://localhost:8000".to_string());

            Ok(Box::new(oxify_connect_vector::ChromaDBProvider::new(url)))
        }
        "pinecone" => {
            let api_key = std::env::var("PINECONE_API_KEY")
                .map_err(|_| "PINECONE_API_KEY environment variable not set".to_string())?;
            let environment = std::env::var("PINECONE_ENVIRONMENT")
                .map_err(|_| "PINECONE_ENVIRONMENT environment variable not set".to_string())?;
            let index_name = std::env::var("PINECONE_INDEX")
                .map_err(|_| "PINECONE_INDEX environment variable not set".to_string())?;

            Ok(Box::new(oxify_connect_vector::PineconeProvider::new(
                api_key,
                environment,
                index_name,
            )))
        }
        "weaviate" => {
            let url = std::env::var("WEAVIATE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string());
            let api_key = std::env::var("WEAVIATE_API_KEY").ok();

            Ok(Box::new(oxify_connect_vector::WeaviateProvider::new(
                url, api_key,
            )))
        }
        "milvus" => {
            let url =
                std::env::var("MILVUS_URL").unwrap_or_else(|_| "http://localhost:9091".to_string());
            let token = std::env::var("MILVUS_TOKEN").ok();

            Ok(Box::new(oxify_connect_vector::MilvusProvider::new(
                url, token,
            )))
        }
        _ => Err(format!("Unsupported provider type: {}", provider_type)),
    }
}
