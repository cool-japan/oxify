//! Mock vector provider for testing

use crate::{
    BatchInsertRequest, CollectionInfo, DeleteRequest, InsertRequest, SearchRequest, SearchResult,
    UpdateRequest, VectorError, VectorProvider,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock vector provider that stores vectors in memory
/// Useful for testing without needing a real database
#[derive(Clone)]
pub struct MockVectorProvider {
    collections: Arc<Mutex<HashMap<String, MockCollection>>>,
    /// If set, causes all operations to fail with this error
    pub fail_with: Arc<Mutex<Option<VectorError>>>,
}

#[derive(Clone)]
struct MockCollection {
    dimension: usize,
    vectors: HashMap<String, (Vec<f32>, serde_json::Value)>,
}

impl Default for MockVectorProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockVectorProvider {
    pub fn new() -> Self {
        Self {
            collections: Arc::new(Mutex::new(HashMap::new())),
            fail_with: Arc::new(Mutex::new(None)),
        }
    }

    /// Set an error that will be returned for all operations
    pub fn set_error(&self, error: VectorError) {
        *self.fail_with.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
    }

    /// Clear the error state
    pub fn clear_error(&self) {
        *self.fail_with.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Get the number of vectors in a collection
    pub fn count(&self, collection: &str) -> usize {
        let collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());
        collections
            .get(collection)
            .map(|c| c.vectors.len())
            .unwrap_or(0)
    }

    /// Compute cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            0.0
        } else {
            (dot_product / (magnitude_a * magnitude_b)) as f64
        }
    }
}

#[async_trait]
impl VectorProvider for MockVectorProvider {
    async fn search(&self, request: SearchRequest) -> crate::Result<Vec<SearchResult>> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());
        let collection = collections.get(&request.collection).ok_or_else(|| {
            VectorError::QueryError(format!("Collection {} not found", request.collection))
        })?;

        // Compute similarities for all vectors
        let mut results: Vec<SearchResult> = collection
            .vectors
            .iter()
            .map(|(id, (vector, payload))| {
                let score = Self::cosine_similarity(&request.query, vector);
                SearchResult {
                    id: id.clone(),
                    score,
                    payload: payload.clone(),
                    vector: Some(vector.clone()),
                }
            })
            .collect();

        // Filter by score threshold
        if let Some(threshold) = request.score_threshold {
            results.retain(|r| r.score >= threshold);
        }

        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Take top_k
        results.truncate(request.top_k);

        Ok(results)
    }

    async fn insert(&self, request: InsertRequest) -> crate::Result<()> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let mut collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());
        let collection = collections.get_mut(&request.collection).ok_or_else(|| {
            VectorError::QueryError(format!("Collection {} not found", request.collection))
        })?;

        // Check dimension
        if request.vector.len() != collection.dimension {
            return Err(VectorError::QueryError(format!(
                "Vector dimension {} does not match collection dimension {}",
                request.vector.len(),
                collection.dimension
            )));
        }

        collection
            .vectors
            .insert(request.id, (request.vector, request.payload));

        Ok(())
    }

    async fn delete(&self, request: DeleteRequest) -> crate::Result<usize> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let mut collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());
        let collection = collections.get_mut(&request.collection).ok_or_else(|| {
            VectorError::QueryError(format!("Collection {} not found", request.collection))
        })?;

        let mut deleted = 0;
        for id in request.ids {
            if collection.vectors.remove(&id).is_some() {
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    async fn create_collection(&self, name: &str, dimension: usize) -> crate::Result<()> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let mut collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());

        if collections.contains_key(name) {
            return Err(VectorError::ConfigError(format!(
                "Collection {} already exists",
                name
            )));
        }

        collections.insert(
            name.to_string(),
            MockCollection {
                dimension,
                vectors: HashMap::new(),
            },
        );

        Ok(())
    }

    async fn collection_exists(&self, name: &str) -> crate::Result<bool> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());
        Ok(collections.contains_key(name))
    }

    async fn batch_insert(&self, request: BatchInsertRequest) -> crate::Result<usize> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let mut collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());
        let collection = collections.get_mut(&request.collection).ok_or_else(|| {
            VectorError::QueryError(format!("Collection {} not found", request.collection))
        })?;

        let mut count = 0;
        for (id, vector, payload) in request.vectors {
            // Check dimension
            if vector.len() != collection.dimension {
                return Err(VectorError::QueryError(format!(
                    "Vector dimension {} does not match collection dimension {}",
                    vector.len(),
                    collection.dimension
                )));
            }

            collection.vectors.insert(id, (vector, payload));
            count += 1;
        }

        Ok(count)
    }

    async fn update(&self, request: UpdateRequest) -> crate::Result<()> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let mut collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());
        let collection = collections.get_mut(&request.collection).ok_or_else(|| {
            VectorError::QueryError(format!("Collection {} not found", request.collection))
        })?;

        let entry = collection
            .vectors
            .get_mut(&request.id)
            .ok_or_else(|| VectorError::QueryError(format!("Vector {} not found", request.id)))?;

        if let Some(vector) = request.vector {
            if vector.len() != collection.dimension {
                return Err(VectorError::QueryError(format!(
                    "Vector dimension {} does not match collection dimension {}",
                    vector.len(),
                    collection.dimension
                )));
            }
            entry.0 = vector;
        }

        if let Some(payload) = request.payload {
            entry.1 = payload;
        }

        Ok(())
    }

    async fn collection_info(&self, name: &str) -> crate::Result<CollectionInfo> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let collections = self.collections.lock().unwrap_or_else(|e| e.into_inner());
        let collection = collections
            .get(name)
            .ok_or_else(|| VectorError::QueryError(format!("Collection {} not found", name)))?;

        Ok(CollectionInfo {
            name: name.to_string(),
            dimension: collection.dimension,
            vector_count: collection.vectors.len(),
        })
    }

    async fn batch_update(&self, requests: Vec<UpdateRequest>) -> crate::Result<usize> {
        // Check for forced error
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Err(VectorError::DatabaseError(err.to_string()));
        }

        let mut count = 0;
        for request in requests {
            self.update(request).await?;
            count += 1;
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_mock_lifecycle() {
        let provider = MockVectorProvider::new();

        // Create collection
        provider.create_collection("test", 3).await.unwrap();
        assert!(provider.collection_exists("test").await.unwrap());

        // Insert vectors
        provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: vec![1.0, 0.0, 0.0],
                payload: json!({"text": "first"}),
            })
            .await
            .unwrap();

        provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "2".to_string(),
                vector: vec![0.9, 0.1, 0.0],
                payload: json!({"text": "second"}),
            })
            .await
            .unwrap();

        assert_eq!(provider.count("test"), 2);

        // Search for similar vectors
        let results = provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![1.0, 0.0, 0.0],
                top_k: 2,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "1"); // Most similar
        assert!(results[0].score > results[1].score);

        // Delete
        let deleted = provider
            .delete(DeleteRequest {
                collection: "test".to_string(),
                ids: vec!["1".to_string()],
            })
            .await
            .unwrap();

        assert_eq!(deleted, 1);
        assert_eq!(provider.count("test"), 1);
    }

    #[tokio::test]
    async fn test_mock_errors() {
        let provider = MockVectorProvider::new();

        // Test collection not found
        let result = provider
            .search(SearchRequest {
                collection: "nonexistent".to_string(),
                query: vec![1.0],
                top_k: 1,
                score_threshold: None,
                filter: None,
            })
            .await;
        assert!(result.is_err());

        // Test forced error
        provider.set_error(VectorError::ConnectionError("Test error".to_string()));

        let result = provider.create_collection("test", 3).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Test error"));

        // Clear error and retry
        provider.clear_error();
        provider.create_collection("test", 3).await.unwrap();
        assert!(provider.collection_exists("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_dimension_mismatch() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 3).await.unwrap();

        // Try to insert vector with wrong dimension
        let result = provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: vec![1.0, 2.0], // Wrong dimension (2 instead of 3)
                payload: json!({}),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("dimension"));
    }

    #[tokio::test]
    async fn test_score_threshold() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 2).await.unwrap();

        provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: vec![1.0, 0.0],
                payload: json!({}),
            })
            .await
            .unwrap();

        provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "2".to_string(),
                vector: vec![0.5, 0.5],
                payload: json!({}),
            })
            .await
            .unwrap();

        // Search with high threshold
        let results = provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![1.0, 0.0],
                top_k: 10,
                score_threshold: Some(0.9),
                filter: None,
            })
            .await
            .unwrap();

        // Only the first vector should match
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[tokio::test]
    async fn test_batch_insert() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 3).await.unwrap();

        // Batch insert multiple vectors
        let vectors = vec![
            ("1".to_string(), vec![1.0, 0.0, 0.0], json!({"label": "a"})),
            ("2".to_string(), vec![0.0, 1.0, 0.0], json!({"label": "b"})),
            ("3".to_string(), vec![0.0, 0.0, 1.0], json!({"label": "c"})),
        ];

        let count = provider
            .batch_insert(BatchInsertRequest {
                collection: "test".to_string(),
                vectors,
            })
            .await
            .unwrap();

        assert_eq!(count, 3);
        assert_eq!(provider.count("test"), 3);

        // Verify vectors were inserted correctly
        let results = provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![1.0, 0.0, 0.0],
                top_k: 1,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results[0].id, "1");
        assert_eq!(results[0].payload["label"], "a");
    }

    #[tokio::test]
    async fn test_update_vector() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 3).await.unwrap();

        // Insert initial vector
        provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: vec![1.0, 0.0, 0.0],
                payload: json!({"label": "original"}),
            })
            .await
            .unwrap();

        // Update vector only
        provider
            .update(UpdateRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: Some(vec![0.0, 1.0, 0.0]),
                payload: None,
            })
            .await
            .unwrap();

        // Verify vector was updated
        let results = provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![0.0, 1.0, 0.0],
                top_k: 1,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results[0].id, "1");
        assert_eq!(results[0].payload["label"], "original"); // Payload unchanged
    }

    #[tokio::test]
    async fn test_update_payload() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 3).await.unwrap();

        // Insert initial vector
        provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: vec![1.0, 0.0, 0.0],
                payload: json!({"label": "original"}),
            })
            .await
            .unwrap();

        // Update payload only
        provider
            .update(UpdateRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: None,
                payload: Some(json!({"label": "updated"})),
            })
            .await
            .unwrap();

        // Verify payload was updated
        let results = provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![1.0, 0.0, 0.0],
                top_k: 1,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results[0].id, "1");
        assert_eq!(results[0].payload["label"], "updated");
    }

    #[tokio::test]
    async fn test_collection_info() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 128).await.unwrap();

        // Insert some vectors
        for i in 0..5 {
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("vec_{}", i),
                    vector: vec![i as f32; 128],
                    payload: json!({}),
                })
                .await
                .unwrap();
        }

        // Get collection info
        let info = provider.collection_info("test").await.unwrap();

        assert_eq!(info.name, "test");
        assert_eq!(info.dimension, 128);
        assert_eq!(info.vector_count, 5);
    }

    #[tokio::test]
    async fn test_update_nonexistent() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 3).await.unwrap();

        // Try to update non-existent vector
        let result = provider
            .update(UpdateRequest {
                collection: "test".to_string(),
                id: "nonexistent".to_string(),
                vector: Some(vec![1.0, 0.0, 0.0]),
                payload: None,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
