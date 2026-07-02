//! Pinecone vector database provider

use crate::{
    BatchInsertRequest, CollectionInfo, DeleteRequest, InsertRequest, Result, SearchRequest,
    SearchResult, UpdateRequest, VectorError, VectorProvider,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Pinecone vector database provider
pub struct PineconeProvider {
    client: oxihttp::HttpsClient,
    api_key: String,
    environment: String,
    index_name: String,
}

#[derive(Serialize)]
struct PineconeVector {
    id: String,
    values: Vec<f32>,
    metadata: serde_json::Value,
}

#[derive(Serialize)]
struct PineconeUpsertRequest {
    vectors: Vec<PineconeVector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
}

#[derive(Serialize)]
struct PineconeQueryRequest {
    vector: Vec<f32>,
    #[serde(rename = "topK")]
    top_k: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<serde_json::Value>,
    #[serde(rename = "includeMetadata")]
    include_metadata: bool,
    #[serde(rename = "includeValues")]
    include_values: bool,
}

#[derive(Deserialize)]
struct PineconeQueryResponse {
    matches: Vec<PineconeMatch>,
}

#[derive(Deserialize)]
struct PineconeMatch {
    id: String,
    score: f64,
    #[serde(default)]
    values: Vec<f32>,
    #[serde(default)]
    metadata: serde_json::Value,
}

#[derive(Deserialize)]
struct PineconeIndexStats {
    dimension: Option<usize>,
    #[serde(rename = "totalVectorCount")]
    total_vector_count: Option<usize>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct PineconeDescribeResponse {
    #[serde(default)]
    dimension: usize,
}

#[derive(Deserialize)]
struct PineconeFetchResponse {
    vectors: std::collections::HashMap<String, PineconeFetchedVector>,
}

#[derive(Deserialize)]
struct PineconeFetchedVector {
    #[allow(dead_code)]
    id: String,
    values: Vec<f32>,
    #[serde(default)]
    metadata: serde_json::Value,
}

impl PineconeProvider {
    /// Create a new Pinecone provider
    pub fn new(api_key: String, environment: String, index_name: String) -> Self {
        Self {
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for Pinecone"),
            api_key,
            environment,
            index_name,
        }
    }

    fn get_index_url(&self) -> String {
        format!(
            "https://{}-{}.svc.{}.pinecone.io",
            self.index_name,
            "default", // project ID would go here
            self.environment
        )
    }
}

#[async_trait]
impl VectorProvider for PineconeProvider {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let query = PineconeQueryRequest {
            vector: request.query,
            top_k: request.top_k,
            namespace: None,
            filter: request.filter,
            include_metadata: true,
            include_values: false,
        };

        let response = self
            .client
            .post(&format!("{}/query", self.get_index_url()))
            .and_then(|request| request.header("Api-Key", &self.api_key))
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

        let pinecone_response: PineconeQueryResponse =
            serde_json::from_str(&body).map_err(|e| VectorError::QueryError(e.to_string()))?;

        let results = pinecone_response
            .matches
            .into_iter()
            .filter(|m| request.score_threshold.is_none_or(|t| m.score >= t))
            .map(|m| SearchResult {
                id: m.id,
                score: m.score,
                payload: m.metadata,
                vector: if m.values.is_empty() {
                    None
                } else {
                    Some(m.values)
                },
            })
            .collect();

        Ok(results)
    }

    async fn insert(&self, request: InsertRequest) -> Result<()> {
        let vector = PineconeVector {
            id: request.id,
            values: request.vector,
            metadata: request.payload,
        };

        let upsert_request = PineconeUpsertRequest {
            vectors: vec![vector],
            namespace: None,
        };

        let response = self
            .client
            .post(&format!("{}/vectors/upsert", self.get_index_url()))
            .and_then(|request| request.header("Api-Key", &self.api_key))
            .and_then(|request| request.header("Content-Type", "application/json"))
            .and_then(|request| request.json(&upsert_request))
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
        let delete_request = serde_json::json!({
            "ids": request.ids
        });

        let response = self
            .client
            .post(&format!("{}/vectors/delete", self.get_index_url()))
            .and_then(|request| request.header("Api-Key", &self.api_key))
            .and_then(|request| request.header("Content-Type", "application/json"))
            .and_then(|request| request.json(&delete_request))
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

    async fn create_collection(&self, _name: &str, _dimension: usize) -> Result<()> {
        Err(VectorError::ConfigError(
            "Collection creation must be done via Pinecone console or API. \
             Use existing index name."
                .to_string(),
        ))
    }

    async fn collection_exists(&self, _name: &str) -> Result<bool> {
        Ok(true) // Assume index exists if provider is configured
    }

    async fn batch_insert(&self, request: BatchInsertRequest) -> Result<usize> {
        if request.vectors.is_empty() {
            return Ok(0);
        }

        // Pinecone supports batch upsert natively
        let vectors: Vec<PineconeVector> = request
            .vectors
            .into_iter()
            .map(|(id, values, metadata)| PineconeVector {
                id,
                values,
                metadata,
            })
            .collect();

        let count = vectors.len();

        let upsert_request = PineconeUpsertRequest {
            vectors,
            namespace: None,
        };

        let response = self
            .client
            .post(&format!("{}/vectors/upsert", self.get_index_url()))
            .and_then(|request| request.header("Api-Key", &self.api_key))
            .and_then(|request| request.header("Content-Type", "application/json"))
            .and_then(|request| request.json(&upsert_request))
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

        // For partial updates, we need to fetch the existing vector first
        if request.vector.is_none() || request.payload.is_none() {
            let fetch_url = oxify_model::http_util::append_query_params(
                &format!("{}/vectors/fetch", self.get_index_url()),
                &[("ids", request.id.as_str())],
            );
            let fetch_response = self
                .client
                .get(&fetch_url)
                .and_then(|request| request.header("Api-Key", &self.api_key))
                .map_err(|e| VectorError::ConnectionError(e.to_string()))?
                .send()
                .await
                .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

            if !fetch_response.status().is_success() {
                return Err(VectorError::QueryError(format!(
                    "Vector {} not found",
                    request.id
                )));
            }

            let fetch_result: PineconeFetchResponse = fetch_response
                .body_json()
                .await
                .map_err(|e| VectorError::QueryError(e.to_string()))?;

            let existing = fetch_result.vectors.get(&request.id).ok_or_else(|| {
                VectorError::QueryError(format!("Vector {} not found", request.id))
            })?;

            let final_vector = request.vector.unwrap_or_else(|| existing.values.clone());
            let final_metadata = request.payload.unwrap_or_else(|| existing.metadata.clone());

            // Upsert with merged data
            self.insert(InsertRequest {
                collection: request.collection,
                id: request.id,
                vector: final_vector,
                payload: final_metadata,
            })
            .await
        } else if let (Some(vector), Some(payload)) = (request.vector, request.payload) {
            // Full update - just upsert
            self.insert(InsertRequest {
                collection: request.collection,
                id: request.id,
                vector,
                payload,
            })
            .await
        } else {
            unreachable!("Both vector and payload must be Some in this branch")
        }
    }

    async fn collection_info(&self, name: &str) -> Result<CollectionInfo> {
        // Get index stats
        let response = self
            .client
            .get(&format!("{}/describe_index_stats", self.get_index_url()))
            .and_then(|request| request.header("Api-Key", &self.api_key))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(VectorError::QueryError(format!(
                "Failed to get index stats for {}",
                name
            )));
        }

        let stats: PineconeIndexStats = response
            .body_json()
            .await
            .map_err(|e| VectorError::QueryError(e.to_string()))?;

        Ok(CollectionInfo {
            name: name.to_string(),
            dimension: stats.dimension.unwrap_or(0),
            vector_count: stats.total_vector_count.unwrap_or(0),
        })
    }
}
