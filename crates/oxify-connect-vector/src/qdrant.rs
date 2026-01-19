//! Qdrant vector database provider implementation

use crate::{
    BatchInsertRequest, CollectionInfo, DeleteRequest, InsertRequest, Result, SearchRequest,
    SearchResult, UpdateRequest, VectorError, VectorProvider,
};
use async_trait::async_trait;
use qdrant_client::{
    config::QdrantConfig,
    qdrant::{
        vectors_config::Config, with_payload_selector::SelectorOptions, Condition,
        CreateCollection, DeletePoints, Distance, Filter, PointId, PointStruct, PointsIdsList,
        PointsSelector, SearchPoints, UpsertPoints, VectorParams, VectorsConfig,
        WithPayloadSelector,
    },
    Qdrant,
};

/// Qdrant provider implementation
pub struct QdrantProvider {
    client: Qdrant,
}

impl QdrantProvider {
    /// Create a new Qdrant provider
    ///
    /// # Arguments
    /// * `url` - Qdrant server URL (e.g., "http://localhost:6334")
    pub async fn new(url: &str) -> Result<Self> {
        let client = Qdrant::from_url(url).build().map_err(|e| {
            VectorError::ConnectionError(format!("Failed to connect to Qdrant: {}", e))
        })?;

        Ok(Self { client })
    }

    /// Create with API key authentication
    pub async fn with_api_key(url: &str, api_key: &str) -> Result<Self> {
        let mut config = QdrantConfig::from_url(url);
        config.set_api_key(api_key);

        let client = Qdrant::new(config).map_err(|e| {
            VectorError::ConnectionError(format!("Failed to connect to Qdrant: {}", e))
        })?;

        Ok(Self { client })
    }

    /// Convert JSON filter to Qdrant filter
    fn convert_filter(&self, filter_json: &serde_json::Value) -> Option<Filter> {
        // Simple implementation: convert JSON object to key-value matches
        if let Some(obj) = filter_json.as_object() {
            let mut conditions = Vec::new();

            for (key, value) in obj {
                if let Some(str_val) = value.as_str() {
                    conditions.push(Condition::matches(key.clone(), str_val.to_string()));
                }
                // Add more type conversions as needed
            }

            if !conditions.is_empty() {
                return Some(Filter::must(conditions));
            }
        }

        None
    }
}

#[async_trait]
impl VectorProvider for QdrantProvider {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let filter = request.filter.as_ref().and_then(|f| self.convert_filter(f));

        let search_points = SearchPoints {
            collection_name: request.collection,
            vector: request.query,
            limit: request.top_k as u64,
            with_payload: Some(WithPayloadSelector {
                selector_options: Some(SelectorOptions::Enable(true)),
            }),
            score_threshold: request.score_threshold.map(|t| t as f32),
            filter,
            ..Default::default()
        };

        let search_result = self
            .client
            .search_points(search_points)
            .await
            .map_err(|e| VectorError::QueryError(format!("Search failed: {}", e)))?;

        let results = search_result
            .result
            .into_iter()
            .map(|point| {
                let id = match point.id {
                    Some(PointId {
                        point_id_options: Some(point_id),
                    }) => match point_id {
                        qdrant_client::qdrant::point_id::PointIdOptions::Num(n) => n.to_string(),
                        qdrant_client::qdrant::point_id::PointIdOptions::Uuid(u) => u,
                    },
                    _ => "unknown".to_string(),
                };

                SearchResult {
                    id,
                    score: point.score as f64,
                    payload: serde_json::to_value(&point.payload).unwrap_or_default(),
                    vector: point.vectors.and_then(|v| {
                        v.vectors_options.and_then(|vo| match vo {
                            qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(vec) => {
                                #[allow(deprecated)]
                                let data = vec.data;
                                Some(data)
                            }
                            _ => None,
                        })
                    }),
                }
            })
            .collect();

        Ok(results)
    }

    async fn insert(&self, request: InsertRequest) -> Result<()> {
        // Convert JSON payload to Qdrant payload
        let payload: std::collections::HashMap<String, qdrant_client::qdrant::Value> = request
            .payload
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| {
                        let qdrant_value = if let Some(s) = v.as_str() {
                            Some(qdrant_client::qdrant::Value {
                                kind: Some(qdrant_client::qdrant::value::Kind::StringValue(
                                    s.to_string(),
                                )),
                            })
                        } else if let Some(n) = v.as_f64() {
                            Some(qdrant_client::qdrant::Value {
                                kind: Some(qdrant_client::qdrant::value::Kind::DoubleValue(n)),
                            })
                        } else {
                            v.as_bool().map(|b| qdrant_client::qdrant::Value {
                                kind: Some(qdrant_client::qdrant::value::Kind::BoolValue(b)),
                            })
                        };
                        qdrant_value.map(|val| (k.clone(), val))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let point = PointStruct::new(request.id.clone(), request.vector, payload);

        let upsert_request = UpsertPoints {
            collection_name: request.collection.clone(),
            points: vec![point],
            ..Default::default()
        };

        self.client
            .upsert_points(upsert_request)
            .await
            .map_err(|e| VectorError::DatabaseError(format!("Insert failed: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, request: DeleteRequest) -> Result<usize> {
        let point_ids: Vec<PointId> = request.ids.into_iter().map(|id| id.into()).collect();

        let delete_request = DeletePoints {
            collection_name: request.collection.clone(),
            points: Some(PointsSelector {
                points_selector_one_of: Some(
                    qdrant_client::qdrant::points_selector::PointsSelectorOneOf::Points(
                        PointsIdsList { ids: point_ids },
                    ),
                ),
            }),
            ..Default::default()
        };

        let result = self
            .client
            .delete_points(delete_request)
            .await
            .map_err(|e| VectorError::DatabaseError(format!("Delete failed: {}", e)))?;

        Ok(result.result.map(|r| r.status as usize).unwrap_or(0))
    }

    async fn create_collection(&self, name: &str, dimension: usize) -> Result<()> {
        self.client
            .create_collection(CreateCollection {
                collection_name: name.to_string(),
                vectors_config: Some(VectorsConfig {
                    config: Some(Config::Params(VectorParams {
                        size: dimension as u64,
                        distance: Distance::Cosine.into(),
                        ..Default::default()
                    })),
                }),
                ..Default::default()
            })
            .await
            .map_err(|e| VectorError::DatabaseError(format!("Create collection failed: {}", e)))?;

        Ok(())
    }

    async fn collection_exists(&self, name: &str) -> Result<bool> {
        self.client
            .collection_exists(name)
            .await
            .map_err(|e| VectorError::QueryError(format!("Collection exists check failed: {}", e)))
    }

    async fn batch_insert(&self, request: BatchInsertRequest) -> Result<usize> {
        // Qdrant supports batch upsert natively
        let points: Vec<PointStruct> = request
            .vectors
            .iter()
            .map(|(id, vector, payload)| {
                let payload_map: std::collections::HashMap<String, qdrant_client::qdrant::Value> =
                    payload
                        .as_object()
                        .map(|obj| {
                            obj.iter()
                                .filter_map(|(k, v)| {
                                    let qdrant_value = if let Some(s) = v.as_str() {
                                        Some(qdrant_client::qdrant::Value {
                                            kind: Some(
                                                qdrant_client::qdrant::value::Kind::StringValue(
                                                    s.to_string(),
                                                ),
                                            ),
                                        })
                                    } else if let Some(n) = v.as_f64() {
                                        Some(qdrant_client::qdrant::Value {
                                            kind: Some(
                                                qdrant_client::qdrant::value::Kind::DoubleValue(n),
                                            ),
                                        })
                                    } else {
                                        v.as_bool().map(|b| qdrant_client::qdrant::Value {
                                            kind: Some(
                                                qdrant_client::qdrant::value::Kind::BoolValue(b),
                                            ),
                                        })
                                    };
                                    qdrant_value.map(|val| (k.clone(), val))
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                PointStruct::new(id.clone(), vector.clone(), payload_map)
            })
            .collect();

        let count = points.len();

        let upsert_request = UpsertPoints {
            collection_name: request.collection.clone(),
            points,
            ..Default::default()
        };

        self.client
            .upsert_points(upsert_request)
            .await
            .map_err(|e| VectorError::DatabaseError(format!("Batch insert failed: {}", e)))?;

        Ok(count)
    }

    async fn update(&self, request: UpdateRequest) -> Result<()> {
        // Qdrant doesn't have a separate update API, but upsert works for updates
        // We need to fetch the existing point first if we're doing a partial update
        if request.vector.is_none() || request.payload.is_none() {
            // Partial update - need to fetch existing data
            let search_result = self
                .client
                .search_points(SearchPoints {
                    collection_name: request.collection.clone(),
                    vector: vec![0.0], // Dummy vector, we just want to retrieve by ID
                    limit: 1,
                    filter: Some(Filter::must([Condition::matches(
                        "id".to_string(),
                        request.id.clone(),
                    )])),
                    with_payload: Some(WithPayloadSelector {
                        selector_options: Some(SelectorOptions::Enable(true)),
                    }),
                    ..Default::default()
                })
                .await
                .map_err(|e| {
                    VectorError::QueryError(format!("Failed to fetch existing point: {}", e))
                })?;

            if search_result.result.is_empty() {
                return Err(VectorError::QueryError(format!(
                    "Vector {} not found",
                    request.id
                )));
            }

            let existing = &search_result.result[0];

            // Merge with existing data
            let final_vector = request.vector.unwrap_or_else(|| {
                existing
                    .vectors
                    .as_ref()
                    .and_then(|v| {
                        v.vectors_options.as_ref().and_then(|vo| match vo {
                            qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(vec) => {
                                #[allow(deprecated)]
                                let data = vec.data.clone();
                                Some(data)
                            }
                            _ => None,
                        })
                    })
                    .unwrap_or_default()
            });

            let final_payload = request
                .payload
                .unwrap_or_else(|| serde_json::to_value(&existing.payload).unwrap_or_default());

            // Now do the full upsert
            self.insert(InsertRequest {
                collection: request.collection,
                id: request.id,
                vector: final_vector,
                payload: final_payload,
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
        let info = self.client.collection_info(name).await.map_err(|e| {
            VectorError::QueryError(format!("Failed to get collection info: {}", e))
        })?;

        let result = info
            .result
            .ok_or_else(|| VectorError::QueryError(format!("Collection {} not found", name)))?;

        let config = result.config.ok_or_else(|| {
            VectorError::QueryError("Collection config not available".to_string())
        })?;

        let vectors_config = config
            .params
            .ok_or_else(|| VectorError::QueryError("Vector params not available".to_string()))?;

        let dimension = match vectors_config.vectors_config {
            Some(VectorsConfig {
                config: Some(Config::Params(params)),
            }) => params.size as usize,
            _ => {
                return Err(VectorError::QueryError(
                    "Invalid vectors config".to_string(),
                ))
            }
        };

        // Qdrant doesn't expose vector count directly in the API
        // We'd need to use the scroll API to count vectors, which is expensive
        // For now, return 0 as a placeholder
        let vector_count = 0;

        Ok(CollectionInfo {
            name: name.to_string(),
            dimension,
            vector_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Qdrant running
    async fn test_qdrant_lifecycle() {
        let provider = QdrantProvider::new("http://localhost:6334").await.unwrap();

        let collection = "test_collection";
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

        provider.delete(delete_req).await.unwrap();
    }
}
