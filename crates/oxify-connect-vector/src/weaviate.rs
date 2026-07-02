//! Weaviate vector database provider
//!
//! Weaviate is an open-source vector database that stores both objects and vectors.
//! This implementation uses the REST API for all operations.

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
            let request = if let Some(ref api_key) = $self.api_key {
                request.header("Authorization", &format!("Bearer {}", api_key))?
            } else {
                request
            };
            request.header("Content-Type", "application/json")
        })
    }};
}

/// Weaviate vector database provider
pub struct WeaviateProvider {
    client: oxihttp::HttpsClient,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Serialize)]
struct WeaviateObject {
    class: String,
    id: String,
    properties: serde_json::Value,
    vector: Vec<f32>,
}

#[derive(Serialize)]
struct WeaviateGraphQLQuery {
    query: String,
}

#[derive(Deserialize)]
struct WeaviateGraphQLResponse {
    data: Option<WeaviateGraphQLData>,
    errors: Option<Vec<WeaviateError>>,
}

#[derive(Deserialize)]
struct WeaviateGraphQLData {
    #[serde(rename = "Get")]
    get: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct WeaviateError {
    message: String,
}

#[derive(Deserialize)]
struct WeaviateSearchResult {
    #[serde(rename = "_additional")]
    additional: WeaviateAdditional,
    #[serde(flatten)]
    properties: serde_json::Value,
}

#[derive(Deserialize)]
struct WeaviateAdditional {
    id: String,
    #[serde(default)]
    distance: Option<f64>,
    #[serde(default)]
    certainty: Option<f64>,
    #[serde(default)]
    vector: Option<Vec<f32>>,
}

#[derive(Serialize)]
struct WeaviateClassConfig {
    class: String,
    #[serde(rename = "vectorizer")]
    vectorizer: String,
    #[serde(rename = "vectorIndexConfig")]
    vector_index_config: WeaviateVectorIndexConfig,
}

#[derive(Serialize)]
struct WeaviateVectorIndexConfig {
    distance: String,
}

#[derive(Serialize)]
struct WeaviateBatchRequest {
    objects: Vec<WeaviateObject>,
}

#[derive(Deserialize)]
struct WeaviateSchemaResponse {
    #[allow(dead_code)]
    class: String,
    #[serde(rename = "vectorIndexConfig")]
    #[allow(dead_code)]
    vector_index_config: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct WeaviateObjectResponse {
    #[allow(dead_code)]
    id: String,
    vector: Option<Vec<f32>>,
    properties: Option<serde_json::Value>,
}

impl WeaviateProvider {
    /// Create a new Weaviate provider
    ///
    /// # Arguments
    /// * `base_url` - Weaviate server URL (e.g., "http://localhost:8080")
    /// * `api_key` - Optional API key for authentication
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            client: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for Weaviate"),
            base_url,
            api_key,
        }
    }

    /// Convert collection name to Weaviate class name (capitalized)
    fn to_class_name(name: &str) -> String {
        let mut chars: Vec<char> = name.chars().collect();
        if let Some(first) = chars.first_mut() {
            *first = first.to_ascii_uppercase();
        }
        chars.into_iter().collect()
    }

    /// Build GraphQL query for vector search
    fn build_search_query(
        &self,
        class_name: &str,
        vector: &[f32],
        top_k: usize,
        filter: Option<&serde_json::Value>,
    ) -> String {
        let vector_str = format!(
            "[{}]",
            vector
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let where_clause = if let Some(filter) = filter {
            self.build_where_clause(filter)
        } else {
            String::new()
        };

        format!(
            r#"{{
                Get {{
                    {class_name}(
                        nearVector: {{ vector: {vector_str} }}
                        limit: {top_k}
                        {where_clause}
                    ) {{
                        _additional {{
                            id
                            distance
                            certainty
                            vector
                        }}
                    }}
                }}
            }}"#,
            class_name = class_name,
            vector_str = vector_str,
            top_k = top_k,
            where_clause = where_clause
        )
    }

    /// Build WHERE clause from JSON filter
    fn build_where_clause(&self, filter: &serde_json::Value) -> String {
        if let Some(obj) = filter.as_object() {
            let conditions: Vec<String> = obj
                .iter()
                .filter_map(|(key, value)| {
                    if let Some(str_val) = value.as_str() {
                        Some(format!(
                            r#"{{ path: ["{}"], operator: Equal, valueText: "{}" }}"#,
                            key, str_val
                        ))
                    } else if let Some(num_val) = value.as_f64() {
                        Some(format!(
                            r#"{{ path: ["{}"], operator: Equal, valueNumber: {} }}"#,
                            key, num_val
                        ))
                    } else {
                        value.as_bool().map(|bool_val| {
                            format!(
                                r#"{{ path: ["{}"], operator: Equal, valueBoolean: {} }}"#,
                                key, bool_val
                            )
                        })
                    }
                })
                .collect();

            if !conditions.is_empty() {
                if conditions.len() == 1 {
                    return format!("where: {}", conditions[0]);
                } else {
                    return format!(
                        "where: {{ operator: And, operands: [{}] }}",
                        conditions.join(", ")
                    );
                }
            }
        }
        String::new()
    }
}

#[async_trait]
impl VectorProvider for WeaviateProvider {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let class_name = Self::to_class_name(&request.collection);
        let query = self.build_search_query(
            &class_name,
            &request.query,
            request.top_k,
            request.filter.as_ref(),
        );

        let url = format!("{}/v1/graphql", self.base_url);
        let graphql_query = WeaviateGraphQLQuery { query };

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&graphql_query))
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

        let graphql_response: WeaviateGraphQLResponse =
            serde_json::from_str(&body).map_err(|e| VectorError::QueryError(e.to_string()))?;

        if let Some(errors) = graphql_response.errors {
            if !errors.is_empty() {
                return Err(VectorError::QueryError(
                    errors
                        .iter()
                        .map(|e| e.message.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
        }

        let data = graphql_response
            .data
            .and_then(|d| d.get)
            .ok_or_else(|| VectorError::QueryError("No data in response".to_string()))?;

        let class_results = data
            .get(&class_name)
            .and_then(|v| v.as_array())
            .ok_or_else(|| VectorError::QueryError("Invalid response format".to_string()))?;

        let results: Vec<SearchResult> = class_results
            .iter()
            .filter_map(|item| {
                let result: WeaviateSearchResult = serde_json::from_value(item.clone()).ok()?;

                // Convert distance to similarity score (Weaviate uses distance, lower is better)
                let score = if let Some(certainty) = result.additional.certainty {
                    certainty
                } else if let Some(distance) = result.additional.distance {
                    1.0 / (1.0 + distance)
                } else {
                    0.0
                };

                if let Some(threshold) = request.score_threshold {
                    if score < threshold {
                        return None;
                    }
                }

                Some(SearchResult {
                    id: result.additional.id,
                    score,
                    payload: result.properties,
                    vector: result.additional.vector,
                })
            })
            .collect();

        Ok(results)
    }

    async fn insert(&self, request: InsertRequest) -> Result<()> {
        let class_name = Self::to_class_name(&request.collection);

        let object = WeaviateObject {
            class: class_name,
            id: request.id.clone(),
            properties: request.payload,
            vector: request.vector,
        };

        let url = format!("{}/v1/objects", self.base_url);

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&object))
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
        let class_name = Self::to_class_name(&request.collection);
        let mut deleted = 0;

        for id in &request.ids {
            let url = format!("{}/v1/objects/{}/{}", self.base_url, class_name, id);

            let response = build_request!(self, oxihttp::Method::DELETE, &url)
                .map_err(|e| VectorError::ConnectionError(e.to_string()))?
                .send()
                .await
                .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

            if response.status().is_success() {
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    async fn create_collection(&self, name: &str, _dimension: usize) -> Result<()> {
        let class_name = Self::to_class_name(name);

        let class_config = WeaviateClassConfig {
            class: class_name,
            vectorizer: "none".to_string(), // We provide vectors ourselves
            vector_index_config: WeaviateVectorIndexConfig {
                distance: "cosine".to_string(),
            },
        };

        let url = format!("{}/v1/schema", self.base_url);

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&class_config))
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
        let class_name = Self::to_class_name(name);
        let url = format!("{}/v1/schema/{}", self.base_url, class_name);

        let response = build_request!(self, oxihttp::Method::GET, &url)
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

        let class_name = Self::to_class_name(&request.collection);

        // Weaviate supports batch operations
        let objects: Vec<WeaviateObject> = request
            .vectors
            .into_iter()
            .map(|(id, vector, properties)| WeaviateObject {
                class: class_name.clone(),
                id,
                properties,
                vector,
            })
            .collect();

        let count = objects.len();

        let batch_request = WeaviateBatchRequest { objects };

        let url = format!("{}/v1/batch/objects", self.base_url);

        let response = build_request!(self, oxihttp::Method::POST, &url)
            .and_then(|request| request.json(&batch_request))
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

        let class_name = Self::to_class_name(&request.collection);

        // For partial updates, fetch existing object first
        if request.vector.is_none() || request.payload.is_none() {
            let get_url = format!("{}/v1/objects/{}/{}", self.base_url, class_name, request.id);
            let get_url =
                oxify_model::http_util::append_query_params(&get_url, &[("include", "vector")]);

            let get_response = build_request!(self, oxihttp::Method::GET, &get_url)
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

            let existing: WeaviateObjectResponse = get_response
                .body_json()
                .await
                .map_err(|e| VectorError::QueryError(e.to_string()))?;

            let final_vector = request
                .vector
                .unwrap_or_else(|| existing.vector.unwrap_or_default());

            let final_properties = request
                .payload
                .unwrap_or_else(|| existing.properties.unwrap_or(serde_json::Value::Null));

            // Update with merged data
            let update_object = WeaviateObject {
                class: class_name,
                id: request.id.clone(),
                properties: final_properties,
                vector: final_vector,
            };

            let url = format!(
                "{}/v1/objects/{}/{}",
                self.base_url, update_object.class, request.id
            );

            let response = build_request!(self, oxihttp::Method::PUT, &url)
                .and_then(|request| request.json(&update_object))
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
        } else if let (Some(vector), Some(payload)) = (request.vector, request.payload) {
            // Full update
            let update_object = WeaviateObject {
                class: class_name,
                id: request.id.clone(),
                properties: payload,
                vector,
            };

            let url = format!(
                "{}/v1/objects/{}/{}",
                self.base_url, update_object.class, request.id
            );

            let response = build_request!(self, oxihttp::Method::PUT, &url)
                .and_then(|request| request.json(&update_object))
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
        } else {
            unreachable!("Both vector and payload must be Some in this branch")
        }

        Ok(())
    }

    async fn collection_info(&self, name: &str) -> Result<CollectionInfo> {
        let class_name = Self::to_class_name(name);

        // Get schema info
        let schema_url = format!("{}/v1/schema/{}", self.base_url, class_name);

        let schema_response = build_request!(self, oxihttp::Method::GET, &schema_url)
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        if !schema_response.status().is_success() {
            return Err(VectorError::QueryError(format!(
                "Collection {} not found",
                name
            )));
        }

        let _schema: WeaviateSchemaResponse = schema_response
            .body_json()
            .await
            .map_err(|e| VectorError::QueryError(e.to_string()))?;

        // Get count using GraphQL
        let count_query = format!(
            r#"{{
                Aggregate {{
                    {}(limit: 1) {{
                        meta {{
                            count
                        }}
                    }}
                }}
            }}"#,
            class_name
        );

        let graphql_url = format!("{}/v1/graphql", self.base_url);
        let graphql_query = WeaviateGraphQLQuery { query: count_query };

        let count_response = build_request!(self, oxihttp::Method::POST, &graphql_url)
            .and_then(|request| request.json(&graphql_query))
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?
            .send()
            .await
            .map_err(|e| VectorError::ConnectionError(e.to_string()))?;

        let vector_count: usize = if count_response.status().is_success() {
            let count_result: serde_json::Value = count_response
                .body_json()
                .await
                .map_err(|e| VectorError::QueryError(e.to_string()))?;

            count_result
                .pointer(&format!("/data/Aggregate/{}/0/meta/count", class_name))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize
        } else {
            0
        };

        Ok(CollectionInfo {
            name: name.to_string(),
            dimension: 0, // Weaviate doesn't expose dimension easily
            vector_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_class_name() {
        assert_eq!(WeaviateProvider::to_class_name("documents"), "Documents");
        assert_eq!(
            WeaviateProvider::to_class_name("myCollection"),
            "MyCollection"
        );
        assert_eq!(WeaviateProvider::to_class_name("Test"), "Test");
    }

    /// Regression guard for the `build_request!` verb-dispatch + auth-header
    /// logic introduced by the reqwest -> oxihttp migration.
    ///
    /// Drives a real `collection_exists` (GET) and `insert` (POST) against a
    /// mock HTTP server and asserts that both verbs are dispatched correctly
    /// and that the `Authorization: Bearer ...` and `Content-Type` headers
    /// applied by `build_request!` reach the server (the mocks only match when
    /// those headers are present).
    #[tokio::test]
    async fn test_build_request_get_and_post_against_mock() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // GET path exercised by `collection_exists` — the class name is the
        // capitalized collection name ("testvectors" -> "Testvectors").
        Mock::given(method("GET"))
            .and(path("/v1/schema/Testvectors"))
            .and(header("authorization", "Bearer test-key"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "class": "Testvectors",
                "vectorIndexConfig": {}
            })))
            .mount(&server)
            .await;

        // POST path exercised by `insert`.
        Mock::given(method("POST"))
            .and(path("/v1/objects"))
            .and(header("authorization", "Bearer test-key"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "test_1"
            })))
            .mount(&server)
            .await;

        let provider = WeaviateProvider::new(server.uri(), Some("test-key".to_string()));

        // GET dispatch through build_request!.
        let exists = provider
            .collection_exists("testvectors")
            .await
            .expect("collection_exists should succeed against the mock server");
        assert!(
            exists,
            "mocked schema GET should report the class as existing"
        );

        // POST dispatch through build_request! (with a JSON body).
        provider
            .insert(InsertRequest {
                collection: "testvectors".to_string(),
                id: "test_1".to_string(),
                vector: vec![0.1, 0.2, 0.3],
                payload: serde_json::json!({ "text": "hello" }),
            })
            .await
            .expect("insert should succeed against the mock server");
    }

    #[tokio::test]
    #[ignore] // Requires Weaviate running
    async fn test_weaviate_lifecycle() {
        let provider = WeaviateProvider::new("http://localhost:8080".to_string(), None);

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
