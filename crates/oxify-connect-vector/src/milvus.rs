//! Milvus vector database provider
//!
//! Milvus is a cloud-native vector database designed for scalable similarity search.
//! This implementation uses the REST API (available in Milvus 2.x).

use crate::{
    BatchInsertRequest, CollectionInfo, DeleteRequest, InsertRequest, Result, SearchRequest,
    SearchResult, UpdateRequest, VectorError, VectorProvider,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Build an authenticated request for the given HTTP method and URL.
///
/// This replaces the removed reqwest `Client::request(method, url)` generic
/// dispatch by matching over [`oxihttp::Method`] and calling the corresponding
/// verb method on the HTTPS client, then layering the optional `Authorization`
/// bearer header and the mandatory `Content-Type: application/json` header.
///
/// It is a `macro_rules!` rather than a plain method because the concrete
/// HTTPS request-builder type (`RequestBuilder<OxiHttpsConnector<HttpConnector>>`)
/// is not nameable through the public `oxihttp` facade, and every
/// `RequestBuilder<C>` method is bound by the un-re-exported hyper `Connect`
/// trait, so no generic helper signature can be written. Expanding inline lets
/// the connector type be inferred at each call site. The macro yields
/// `Result<oxihttp::RequestBuilder<_>, oxihttp::OxiHttpError>`, so every call
/// site propagates the error with `?` (via `map_err`).
macro_rules! build_request {
    ($self:expr, $method:expr, $url:expr) => {{
        let dispatched = match $method.as_str() {
            "GET" => $self.client.get($url),
            "POST" => $self.client.post($url),
            "PUT" => $self.client.put($url),
            "DELETE" => $self.client.delete($url),
            "PATCH" => $self.client.patch($url),
            "HEAD" => $self.client.head($url),
            other => Err(oxihttp::OxiHttpError::MethodNotAllowed {
                method: other.to_string(),
                path: ($url).to_string(),
            }),
        };
        dispatched.and_then(|request| {
            let request = if let Some(ref token) = $self.token {
                request.header("Authorization", &format!("Bearer {}", token))?
            } else {
                request
            };
            request.header("Content-Type", "application/json")
        })
    }};
}

/// Milvus vector database provider
pub struct MilvusProvider {
    client: oxihttp::HttpsClient,
    base_url: String,
    token: Option<String>,
}

#[derive(Serialize)]
struct MilvusCreateCollectionRequest {
    #[serde(rename = "collectionName")]
    collection_name: String,
    dimension: usize,
    #[serde(rename = "metricType")]
    metric_type: String,
}

#[derive(Serialize)]
struct MilvusInsertRequest {
    #[serde(rename = "collectionName")]
    collection_name: String,
    data: Vec<MilvusInsertData>,
}

#[derive(Serialize)]
struct MilvusInsertData {
    id: String,
    vector: Vec<f32>,
    #[serde(flatten)]
    metadata: serde_json::Value,
}

#[derive(Serialize)]
struct MilvusSearchRequest {
    #[serde(rename = "collectionName")]
    collection_name: String,
    data: Vec<Vec<f32>>,
    limit: usize,
    #[serde(rename = "outputFields")]
    output_fields: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<String>,
}

#[derive(Serialize)]
struct MilvusDeleteRequest {
    #[serde(rename = "collectionName")]
    collection_name: String,
    filter: String,
}

#[derive(Deserialize)]
struct MilvusResponse<T> {
    code: i32,
    #[serde(default)]
    message: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct MilvusSearchData {
    id: serde_json::Value,
    distance: f64,
    #[serde(flatten)]
    fields: serde_json::Value,
}

#[derive(Deserialize)]
struct MilvusCollectionInfo {
    #[serde(rename = "collectionName")]
    #[allow(dead_code)]
    collection_name: String,
}

#[derive(Deserialize)]
struct MilvusCollectionStats {
    #[serde(rename = "collectionName")]
    #[allow(dead_code)]
    collection_name: String,
    #[serde(default)]
    dimension: Option<usize>,
    #[serde(rename = "rowCount", default)]
    row_count: Option<usize>,
}

#[derive(Deserialize)]
struct MilvusGetResponse {
    #[allow(dead_code)]
    id: serde_json::Value,
    #[serde(default)]
    vector: Option<Vec<f32>>,
    #[serde(flatten)]
    fields: serde_json::Value,
}

impl MilvusProvider {
    /// Create a new Milvus provider
    ///
    /// # Arguments
    /// * `base_url` - Milvus server URL (e.g., "http://localhost:19530")
    /// * `token` - Optional authentication token
    pub fn new(base_url: String, token: Option<String>) -> Self {
        Self {
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for Milvus"),
            base_url,
            token,
        }
    }

    /// Convert JSON filter to Milvus filter expression
    fn convert_filter(filter: &serde_json::Value) -> Option<String> {
        if let Some(obj) = filter.as_object() {
            let conditions: Vec<String> = obj
                .iter()
                .filter_map(|(key, value)| {
                    if let Some(str_val) = value.as_str() {
                        Some(format!("{} == \"{}\"", key, str_val))
                    } else if let Some(num_val) = value.as_f64() {
                        Some(format!("{} == {}", key, num_val))
                    } else {
                        value
                            .as_bool()
                            .map(|bool_val| format!("{} == {}", key, bool_val))
                    }
                })
                .collect();

            if !conditions.is_empty() {
                return Some(conditions.join(" and "));
            }
        }
        None
    }
}

#[async_trait]
impl VectorProvider for MilvusProvider {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let filter = request.filter.as_ref().and_then(Self::convert_filter);

        let search_request = MilvusSearchRequest {
            collection_name: request.collection.clone(),
            data: vec![request.query],
            limit: request.top_k,
            output_fields: vec!["*".to_string()],
            filter,
        };

        let url = format!("{}/v2/vectordb/entities/search", self.base_url);

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&search_request))
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

        let milvus_response: MilvusResponse<Vec<MilvusSearchData>> =
            serde_json::from_str(&body).map_err(|e| VectorError::QueryError(e.to_string()))?;

        if milvus_response.code != 0 {
            return Err(VectorError::QueryError(format!(
                "Milvus error {}: {}",
                milvus_response.code, milvus_response.message
            )));
        }

        let data = milvus_response.data.unwrap_or_default();

        let results: Vec<SearchResult> = data
            .into_iter()
            .filter_map(|item| {
                // Milvus uses distance (lower is better for L2), convert to similarity
                let score = 1.0 / (1.0 + item.distance);

                if let Some(threshold) = request.score_threshold {
                    if score < threshold {
                        return None;
                    }
                }

                let id = match &item.id {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => return None,
                };

                Some(SearchResult {
                    id,
                    score,
                    payload: item.fields,
                    vector: None,
                })
            })
            .collect();

        Ok(results)
    }

    async fn insert(&self, request: InsertRequest) -> Result<()> {
        let insert_data = MilvusInsertData {
            id: request.id,
            vector: request.vector,
            metadata: request.payload,
        };

        let insert_request = MilvusInsertRequest {
            collection_name: request.collection,
            data: vec![insert_data],
        };

        let url = format!("{}/v2/vectordb/entities/insert", self.base_url);

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&insert_request))
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
            return Err(VectorError::DatabaseError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let milvus_response: MilvusResponse<serde_json::Value> =
            serde_json::from_str(&body).map_err(|e| VectorError::DatabaseError(e.to_string()))?;

        if milvus_response.code != 0 {
            return Err(VectorError::DatabaseError(format!(
                "Milvus error {}: {}",
                milvus_response.code, milvus_response.message
            )));
        }

        Ok(())
    }

    async fn delete(&self, request: DeleteRequest) -> Result<usize> {
        // Build filter expression for IDs
        let id_conditions: Vec<String> = request
            .ids
            .iter()
            .map(|id| format!("id == \"{}\"", id))
            .collect();

        let filter = id_conditions.join(" or ");

        let delete_request = MilvusDeleteRequest {
            collection_name: request.collection,
            filter,
        };

        let url = format!("{}/v2/vectordb/entities/delete", self.base_url);

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&delete_request))
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
            return Err(VectorError::DatabaseError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let milvus_response: MilvusResponse<serde_json::Value> =
            serde_json::from_str(&body).map_err(|e| VectorError::DatabaseError(e.to_string()))?;

        if milvus_response.code != 0 {
            return Err(VectorError::DatabaseError(format!(
                "Milvus error {}: {}",
                milvus_response.code, milvus_response.message
            )));
        }

        Ok(request.ids.len())
    }

    async fn create_collection(&self, name: &str, dimension: usize) -> Result<()> {
        let create_request = MilvusCreateCollectionRequest {
            collection_name: name.to_string(),
            dimension,
            metric_type: "COSINE".to_string(),
        };

        let url = format!("{}/v2/vectordb/collections/create", self.base_url);

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&create_request))
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
            return Err(VectorError::DatabaseError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let milvus_response: MilvusResponse<serde_json::Value> =
            serde_json::from_str(&body).map_err(|e| VectorError::DatabaseError(e.to_string()))?;

        if milvus_response.code != 0 {
            return Err(VectorError::DatabaseError(format!(
                "Milvus error {}: {}",
                milvus_response.code, milvus_response.message
            )));
        }

        Ok(())
    }

    async fn collection_exists(&self, name: &str) -> Result<bool> {
        let url = format!(
            "{}/v2/vectordb/collections/describe?collectionName={}",
            self.base_url, name
        );

        let response = build_request!(self, oxihttp::Method::GET, &url)
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(false);
        }

        let body = response
            .body_text()
            .await
            .map_err(|e| VectorError::QueryError(e.to_string()))?;

        let milvus_response: MilvusResponse<MilvusCollectionInfo> =
            serde_json::from_str(&body).map_err(|e| VectorError::QueryError(e.to_string()))?;

        Ok(milvus_response.code == 0 && milvus_response.data.is_some())
    }

    async fn batch_insert(&self, request: BatchInsertRequest) -> Result<usize> {
        if request.vectors.is_empty() {
            return Ok(0);
        }

        // Milvus supports batch inserts natively
        let data: Vec<MilvusInsertData> = request
            .vectors
            .into_iter()
            .map(|(id, vector, metadata)| MilvusInsertData {
                id,
                vector,
                metadata,
            })
            .collect();

        let count = data.len();

        let insert_request = MilvusInsertRequest {
            collection_name: request.collection,
            data,
        };

        let url = format!("{}/v2/vectordb/entities/insert", self.base_url);

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&insert_request))
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
            return Err(VectorError::DatabaseError(format!(
                "Batch insert failed: HTTP {}: {}",
                status, body
            )));
        }

        let milvus_response: MilvusResponse<serde_json::Value> =
            serde_json::from_str(&body).map_err(|e| VectorError::DatabaseError(e.to_string()))?;

        if milvus_response.code != 0 {
            return Err(VectorError::DatabaseError(format!(
                "Milvus error {}: {}",
                milvus_response.code, milvus_response.message
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

        // Milvus doesn't have a native update operation
        // We need to delete and re-insert
        // For partial updates, fetch existing data first
        if request.vector.is_none() || request.payload.is_none() {
            // Query for existing vector
            let filter = format!("id == \"{}\"", request.id);

            let get_request = serde_json::json!({
                "collectionName": request.collection,
                "filter": filter,
                "outputFields": ["*"],
                "limit": 1
            });

            let url = format!("{}/v2/vectordb/entities/query", self.base_url);

            let get_response = build_request!(self, oxihttp::Method::POST, &url)
                .and_then(|request| request.json(&get_request))
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

            let get_body = get_response
                .body_text()
                .await
                .map_err(|e| VectorError::QueryError(e.to_string()))?;

            let get_result: MilvusResponse<Vec<MilvusGetResponse>> =
                serde_json::from_str(&get_body)
                    .map_err(|e| VectorError::QueryError(e.to_string()))?;

            if get_result.code != 0 {
                return Err(VectorError::QueryError(format!(
                    "Failed to fetch existing vector: {}",
                    get_result.message
                )));
            }

            let existing = get_result.data.and_then(|mut d| d.pop()).ok_or_else(|| {
                VectorError::QueryError(format!("Vector {} not found", request.id))
            })?;

            let final_vector = request
                .vector
                .unwrap_or_else(|| existing.vector.unwrap_or_default());

            let final_metadata = request.payload.unwrap_or(existing.fields);

            // Delete old version
            self.delete(DeleteRequest {
                collection: request.collection.clone(),
                ids: vec![request.id.clone()],
            })
            .await?;

            // Insert updated version
            self.insert(InsertRequest {
                collection: request.collection,
                id: request.id,
                vector: final_vector,
                payload: final_metadata,
            })
            .await
        } else if let (Some(vector), Some(payload)) = (request.vector, request.payload) {
            // Full update - delete and re-insert
            self.delete(DeleteRequest {
                collection: request.collection.clone(),
                ids: vec![request.id.clone()],
            })
            .await?;

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
        let url = format!(
            "{}/v2/vectordb/collections/describe?collectionName={}",
            self.base_url, name
        );

        let response = build_request!(self, oxihttp::Method::GET, &url)
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
                "Collection {} not found",
                name
            )));
        }

        let milvus_response: MilvusResponse<MilvusCollectionStats> =
            serde_json::from_str(&body).map_err(|e| VectorError::QueryError(e.to_string()))?;

        if milvus_response.code != 0 {
            return Err(VectorError::QueryError(format!(
                "Milvus error {}: {}",
                milvus_response.code, milvus_response.message
            )));
        }

        let stats = milvus_response
            .data
            .ok_or_else(|| VectorError::QueryError(format!("Collection {} not found", name)))?;

        Ok(CollectionInfo {
            name: name.to_string(),
            dimension: stats.dimension.unwrap_or(0),
            vector_count: stats.row_count.unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_filter() {
        let filter = serde_json::json!({
            "category": "test",
            "count": 5
        });

        let result = MilvusProvider::convert_filter(&filter);
        assert!(result.is_some());
        let expr = result.unwrap();
        assert!(expr.contains("category == \"test\""));
        assert!(expr.contains("count == 5"));
    }

    #[tokio::test]
    #[ignore] // Requires Milvus running
    async fn test_milvus_lifecycle() {
        let provider = MilvusProvider::new("http://localhost:19530".to_string(), None);

        let collection = "test_vectors";
        let dimension = 128;

        // Create collection
        if !provider.collection_exists(collection).await.unwrap() {
            provider
                .create_collection(collection, dimension)
                .await
                .unwrap();
        }

        // Insert a vector
        let insert_req = InsertRequest {
            collection: collection.to_string(),
            id: "test_1".to_string(),
            vector: vec![0.1; dimension],
            payload: serde_json::json!({
                "text": "Hello, world!",
                "category": "greeting"
            }),
        };

        provider.insert(insert_req).await.unwrap();

        // Search
        let search_req = SearchRequest {
            collection: collection.to_string(),
            query: vec![0.1; dimension],
            top_k: 5,
            score_threshold: None,
            filter: None,
        };

        let results = provider.search(search_req).await.unwrap();
        assert!(!results.is_empty());

        // Delete
        let delete_req = DeleteRequest {
            collection: collection.to_string(),
            ids: vec!["test_1".to_string()],
        };

        let deleted = provider.delete(delete_req).await.unwrap();
        assert_eq!(deleted, 1);
    }
}
