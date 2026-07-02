//! ChromaDB vector database provider

use crate::{
    BatchInsertRequest, CollectionInfo, DeleteRequest, InsertRequest, Result, SearchRequest,
    SearchResult, UpdateRequest, VectorError, VectorProvider,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// ChromaDB vector database provider
pub struct ChromaDBProvider {
    client: oxihttp::HttpsClient,
    base_url: String,
}

#[derive(Serialize)]
struct ChromaAddRequest {
    ids: Vec<String>,
    embeddings: Vec<Vec<f32>>,
    metadatas: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct ChromaQueryRequest {
    query_embeddings: Vec<Vec<f32>>,
    n_results: usize,
    #[serde(skip_serializing_if = "Option::is_none", rename = "where")]
    where_clause: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ChromaQueryResponse {
    ids: Vec<Vec<String>>,
    distances: Vec<Vec<f64>>,
    #[serde(default)]
    metadatas: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    embeddings: Vec<Vec<Vec<f32>>>,
}

#[derive(Serialize)]
struct ChromaCreateCollectionRequest {
    name: String,
}

#[derive(Deserialize)]
struct ChromaCollectionResponse {
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    metadata: serde_json::Value,
}

#[derive(Deserialize)]
struct ChromaGetResponse {
    ids: Vec<String>,
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
    #[serde(default)]
    metadatas: Vec<serde_json::Value>,
}

impl ChromaDBProvider {
    /// Create a new ChromaDB provider
    pub fn new(base_url: String) -> Self {
        Self {
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for ChromaDB"),
            base_url,
        }
    }
}

#[async_trait]
impl VectorProvider for ChromaDBProvider {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let query = ChromaQueryRequest {
            query_embeddings: vec![request.query],
            n_results: request.top_k,
            where_clause: request.filter,
        };

        let url = format!(
            "{}/api/v1/collections/{}/query",
            self.base_url, request.collection
        );

        let response = self
            .client
            .post(&url)
            .and_then(|request| request.header("Content-Type", "application/json"))
            .and_then(|request| request.json(&query))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        let status = response.status();
        let body = response
            .body_text()
            .await
            .map_err(|e| VectorError::QueryError(e.to_string()))?;

        if !status.is_success() {
            return Err(VectorError::QueryError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let chroma_response: ChromaQueryResponse =
            serde_json::from_str(&body).map_err(|e| VectorError::QueryError(e.to_string()))?;

        if chroma_response.ids.is_empty() {
            return Ok(Vec::new());
        }

        let results: Vec<SearchResult> = chroma_response.ids[0]
            .iter()
            .enumerate()
            .filter_map(|(idx, id)| {
                // ChromaDB uses distance (lower is better), convert to similarity score
                let distance = chroma_response.distances[0].get(idx)?;
                let score = 1.0 / (1.0 + distance);

                if let Some(threshold) = request.score_threshold {
                    if score < threshold {
                        return None;
                    }
                }

                let payload = chroma_response
                    .metadatas
                    .first()
                    .and_then(|m| m.get(idx))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                let vector = chroma_response
                    .embeddings
                    .first()
                    .and_then(|e| e.get(idx))
                    .cloned();

                Some(SearchResult {
                    id: id.clone(),
                    score,
                    payload,
                    vector,
                })
            })
            .collect();

        Ok(results)
    }

    async fn insert(&self, request: InsertRequest) -> Result<()> {
        let add_request = ChromaAddRequest {
            ids: vec![request.id],
            embeddings: vec![request.vector],
            metadatas: vec![request.payload],
        };

        let url = format!(
            "{}/api/v1/collections/{}/add",
            self.base_url, request.collection
        );

        let response = self
            .client
            .post(&url)
            .and_then(|request| request.header("Content-Type", "application/json"))
            .and_then(|request| request.json(&add_request))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.body_text().await.unwrap_or_default();
            return Err(VectorError::DatabaseError(format!(
                "Failed to insert: {}",
                body
            )));
        }

        Ok(())
    }

    async fn delete(&self, request: DeleteRequest) -> Result<usize> {
        let url = format!(
            "{}/api/v1/collections/{}/delete",
            self.base_url, request.collection
        );

        let delete_body = serde_json::json!({ "ids": request.ids });
        let response = self
            .client
            .post(&url)
            .and_then(|builder| builder.header("Content-Type", "application/json"))
            .and_then(|builder| builder.json(&delete_body))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.body_text().await.unwrap_or_default();
            return Err(VectorError::DatabaseError(format!(
                "Failed to delete: {}",
                body
            )));
        }

        Ok(request.ids.len())
    }

    async fn create_collection(&self, name: &str, _dimension: usize) -> Result<()> {
        let create_request = ChromaCreateCollectionRequest {
            name: name.to_string(),
        };

        let url = format!("{}/api/v1/collections", self.base_url);

        let response = self
            .client
            .post(&url)
            .and_then(|request| request.header("Content-Type", "application/json"))
            .and_then(|request| request.json(&create_request))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.body_text().await.unwrap_or_default();
            return Err(VectorError::DatabaseError(format!(
                "Failed to create collection: {}",
                body
            )));
        }

        Ok(())
    }

    async fn collection_exists(&self, name: &str) -> Result<bool> {
        let url = format!("{}/api/v1/collections/{}", self.base_url, name);

        let response = self
            .client
            .get(&url)
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        Ok(response.status().is_success())
    }

    async fn batch_insert(&self, request: BatchInsertRequest) -> Result<usize> {
        if request.vectors.is_empty() {
            return Ok(0);
        }

        // ChromaDB supports batch operations natively
        let mut ids = Vec::new();
        let mut embeddings = Vec::new();
        let mut metadatas = Vec::new();

        for (id, vec, payload) in request.vectors {
            ids.push(id);
            embeddings.push(vec);
            metadatas.push(payload);
        }

        let count = ids.len();

        let add_request = ChromaAddRequest {
            ids,
            embeddings,
            metadatas,
        };

        let url = format!(
            "{}/api/v1/collections/{}/add",
            self.base_url, request.collection
        );

        let response = self
            .client
            .post(&url)
            .and_then(|request| request.header("Content-Type", "application/json"))
            .and_then(|request| request.json(&add_request))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.body_text().await.unwrap_or_default();
            return Err(VectorError::DatabaseError(format!(
                "Batch insert failed: {}",
                body
            )));
        }

        Ok(count)
    }

    async fn update(&self, request: UpdateRequest) -> Result<()> {
        if request.vector.is_none() && request.payload.is_none() {
            return Err(VectorError::QueryError(
                "Update requires either vector or payload".to_string(),
            ));
        }

        // ChromaDB uses upsert semantics with update endpoint
        // We need to fetch existing data if doing partial update
        let url = format!(
            "{}/api/v1/collections/{}/get",
            self.base_url, request.collection
        );

        let get_body = serde_json::json!({ "ids": vec![&request.id] });
        let get_response = self
            .client
            .post(&url)
            .and_then(|builder| builder.header("Content-Type", "application/json"))
            .and_then(|builder| builder.json(&get_body))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !get_response.status().is_success() {
            return Err(VectorError::QueryError(format!(
                "Vector {} not found",
                request.id
            )));
        }

        let existing: ChromaGetResponse = get_response
            .body_json()
            .await
            .map_err(|e| VectorError::QueryError(e.to_string()))?;

        if existing.ids.is_empty() {
            return Err(VectorError::QueryError(format!(
                "Vector {} not found",
                request.id
            )));
        }

        // Merge with existing data
        let final_vector = request
            .vector
            .unwrap_or_else(|| existing.embeddings.first().cloned().unwrap_or_default());

        let final_payload = request.payload.unwrap_or_else(|| {
            existing
                .metadatas
                .first()
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        });

        // Use update endpoint
        let update_url = format!(
            "{}/api/v1/collections/{}/update",
            self.base_url, request.collection
        );

        let update_request = serde_json::json!({
            "ids": vec![request.id],
            "embeddings": vec![final_vector],
            "metadatas": vec![final_payload]
        });

        let response = self
            .client
            .post(&update_url)
            .and_then(|request| request.header("Content-Type", "application/json"))
            .and_then(|request| request.json(&update_request))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.body_text().await.unwrap_or_default();
            return Err(VectorError::DatabaseError(format!(
                "Update failed: {}",
                body
            )));
        }

        Ok(())
    }

    async fn collection_info(&self, name: &str) -> Result<CollectionInfo> {
        // Get collection metadata
        let url = format!("{}/api/v1/collections/{}", self.base_url, name);

        let response = self
            .client
            .get(&url)
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(VectorError::QueryError(format!(
                "Collection {} not found",
                name
            )));
        }

        let _collection: ChromaCollectionResponse = response
            .body_json()
            .await
            .map_err(|e| VectorError::QueryError(e.to_string()))?;

        // Get count by querying all vectors (ChromaDB doesn't expose count directly)
        let count_url = format!("{}/api/v1/collections/{}/count", self.base_url, name);

        let count_response = self
            .client
            .get(&count_url)
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        let vector_count: usize = if count_response.status().is_success() {
            count_response
                .body_json()
                .await
                .map_err(|e| VectorError::QueryError(e.to_string()))?
        } else {
            0
        };

        // ChromaDB doesn't enforce dimension, so we can't reliably get it
        // Return 0 as a placeholder
        Ok(CollectionInfo {
            name: name.to_string(),
            dimension: 0, // ChromaDB doesn't enforce dimensions
            vector_count,
        })
    }
}
