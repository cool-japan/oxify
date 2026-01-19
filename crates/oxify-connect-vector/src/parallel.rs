//! Parallel batch operations for improved throughput
//!
//! This module provides parallel implementations of batch operations that can significantly
//! improve performance when processing large numbers of vectors.
//!
//! ## Example
//!
//! ```rust,no_run
//! use oxify_connect_vector::{
//!     parallel::{parallel_batch_insert, ParallelConfig},
//!     QdrantProvider, VectorProvider, InsertRequest,
//! };
//! use serde_json::json;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = Arc::new(QdrantProvider::new("http://localhost:6334").await?);
//! provider.create_collection("docs", 384).await?;
//!
//! // Prepare 1000 vectors to insert
//! let mut requests = Vec::new();
//! for i in 0..1000 {
//!     requests.push(InsertRequest {
//!         collection: "docs".to_string(),
//!         id: format!("doc_{}", i),
//!         vector: vec![0.1; 384],
//!         payload: json!({"index": i}),
//!     });
//! }
//!
//! // Insert in parallel with 10 concurrent tasks
//! let config = ParallelConfig {
//!     max_concurrent: 10,
//!     chunk_size: 100,
//! };
//!
//! let inserted = parallel_batch_insert(provider, requests, config).await?;
//! println!("Inserted {} vectors in parallel", inserted);
//! # Ok(())
//! # }
//! ```

use crate::{
    DeleteRequest, InsertRequest, RateLimiter, Result, SearchRequest, SearchResult, UpdateRequest,
    VectorProvider,
};
use std::sync::Arc;
use tokio::task::JoinSet;

/// Configuration for parallel operations
#[derive(Debug, Clone, Copy)]
pub struct ParallelConfig {
    /// Maximum number of concurrent tasks
    pub max_concurrent: usize,
    /// Size of each chunk for parallel processing
    pub chunk_size: usize,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            chunk_size: 100,
        }
    }
}

/// Insert vectors in parallel using multiple concurrent tasks
///
/// This can significantly improve throughput when inserting large numbers of vectors.
/// The requests are split into chunks and processed concurrently.
///
/// # Arguments
///
/// * `provider` - The vector database provider (wrapped in Arc)
/// * `requests` - Vector of insert requests
/// * `config` - Parallel processing configuration
///
/// # Returns
///
/// Total number of vectors inserted
pub async fn parallel_batch_insert<P: VectorProvider + 'static>(
    provider: Arc<P>,
    requests: Vec<InsertRequest>,
    config: ParallelConfig,
) -> Result<usize> {
    if requests.is_empty() {
        return Ok(0);
    }
    let mut join_set = JoinSet::new();
    let mut total_inserted = 0;

    // Split requests into chunks
    let chunks: Vec<_> = requests.chunks(config.chunk_size).collect();

    for chunk in chunks {
        // Wait if we've reached max concurrent tasks
        while join_set.len() >= config.max_concurrent {
            if let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(count)) => total_inserted += count,
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(crate::VectorError::QueryError(format!(
                            "Task join error: {}",
                            e
                        )))
                    }
                }
            }
        }

        // Spawn a new task for this chunk
        let provider_clone = Arc::clone(&provider);
        let chunk_owned: Vec<_> = chunk.to_vec();

        join_set.spawn(async move {
            let mut count = 0;
            for request in chunk_owned {
                provider_clone.insert(request).await?;
                count += 1;
            }
            Ok::<usize, crate::VectorError>(count)
        });
    }

    // Wait for remaining tasks
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(count)) => total_inserted += count,
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(crate::VectorError::QueryError(format!(
                    "Task join error: {}",
                    e
                )))
            }
        }
    }

    Ok(total_inserted)
}

/// Execute multiple search queries in parallel
///
/// This can significantly improve throughput when executing many search queries.
/// The queries are processed concurrently up to the configured limit.
///
/// # Arguments
///
/// * `provider` - The vector database provider (wrapped in Arc)
/// * `requests` - Vector of search requests
/// * `config` - Parallel processing configuration
///
/// # Returns
///
/// Vector of search results, one for each request
pub async fn parallel_batch_search<P: VectorProvider + 'static>(
    provider: Arc<P>,
    requests: Vec<SearchRequest>,
    config: ParallelConfig,
) -> Result<Vec<Vec<SearchResult>>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let mut join_set = JoinSet::new();
    let mut results = Vec::with_capacity(requests.len());

    // Split requests into chunks
    let chunks: Vec<_> = requests.chunks(config.chunk_size).collect();

    for chunk in chunks {
        // Wait if we've reached max concurrent tasks
        while join_set.len() >= config.max_concurrent {
            if let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(chunk_results)) => results.extend(chunk_results),
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(crate::VectorError::QueryError(format!(
                            "Task join error: {}",
                            e
                        )))
                    }
                }
            }
        }

        // Spawn a new task for this chunk
        let provider_clone = Arc::clone(&provider);
        let chunk_owned: Vec<_> = chunk.to_vec();

        join_set.spawn(async move {
            let mut chunk_results = Vec::new();
            for request in chunk_owned {
                let search_results = provider_clone.search(request).await?;
                chunk_results.push(search_results);
            }
            Ok::<Vec<Vec<SearchResult>>, crate::VectorError>(chunk_results)
        });
    }

    // Wait for remaining tasks
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(chunk_results)) => results.extend(chunk_results),
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(crate::VectorError::QueryError(format!(
                    "Task join error: {}",
                    e
                )))
            }
        }
    }

    Ok(results)
}

/// Update multiple vectors in parallel
///
/// This can significantly improve throughput when updating large numbers of vectors.
/// The updates are split into chunks and processed concurrently.
///
/// # Arguments
///
/// * `provider` - The vector database provider (wrapped in Arc)
/// * `requests` - Vector of update requests
/// * `config` - Parallel processing configuration
///
/// # Returns
///
/// Total number of vectors updated
pub async fn parallel_batch_update<P: VectorProvider + 'static>(
    provider: Arc<P>,
    requests: Vec<UpdateRequest>,
    config: ParallelConfig,
) -> Result<usize> {
    if requests.is_empty() {
        return Ok(0);
    }
    let mut join_set = JoinSet::new();
    let mut total_updated = 0;

    // Split requests into chunks
    let chunks: Vec<_> = requests.chunks(config.chunk_size).collect();

    for chunk in chunks {
        // Wait if we've reached max concurrent tasks
        while join_set.len() >= config.max_concurrent {
            if let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(count)) => total_updated += count,
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(crate::VectorError::QueryError(format!(
                            "Task join error: {}",
                            e
                        )))
                    }
                }
            }
        }

        // Spawn a new task for this chunk
        let provider_clone = Arc::clone(&provider);
        let chunk_owned: Vec<_> = chunk.to_vec();

        join_set.spawn(async move {
            let mut count = 0;
            for request in chunk_owned {
                provider_clone.update(request).await?;
                count += 1;
            }
            Ok::<usize, crate::VectorError>(count)
        });
    }

    // Wait for remaining tasks
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(count)) => total_updated += count,
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(crate::VectorError::QueryError(format!(
                    "Task join error: {}",
                    e
                )))
            }
        }
    }

    Ok(total_updated)
}

/// Delete multiple vectors in parallel
///
/// This can significantly improve throughput when deleting large numbers of vectors.
/// The delete requests are split into chunks and processed concurrently.
///
/// # Arguments
///
/// * `provider` - The vector database provider (wrapped in Arc)
/// * `requests` - Vector of delete requests
/// * `config` - Parallel processing configuration
///
/// # Returns
///
/// Total number of vectors deleted
pub async fn parallel_batch_delete<P: VectorProvider + 'static>(
    provider: Arc<P>,
    requests: Vec<DeleteRequest>,
    config: ParallelConfig,
) -> Result<usize> {
    if requests.is_empty() {
        return Ok(0);
    }
    let mut join_set = JoinSet::new();
    let mut total_deleted = 0;

    // Split requests into chunks
    let chunks: Vec<_> = requests.chunks(config.chunk_size).collect();

    for chunk in chunks {
        // Wait if we've reached max concurrent tasks
        while join_set.len() >= config.max_concurrent {
            if let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(count)) => total_deleted += count,
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(crate::VectorError::QueryError(format!(
                            "Task join error: {}",
                            e
                        )))
                    }
                }
            }
        }

        // Spawn a new task for this chunk
        let provider_clone = Arc::clone(&provider);
        let chunk_owned: Vec<_> = chunk.to_vec();

        join_set.spawn(async move {
            let mut count = 0;
            for request in chunk_owned {
                let deleted = provider_clone.delete(request).await?;
                count += deleted;
            }
            Ok::<usize, crate::VectorError>(count)
        });
    }

    // Wait for remaining tasks
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(count)) => total_deleted += count,
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(crate::VectorError::QueryError(format!(
                    "Task join error: {}",
                    e
                )))
            }
        }
    }

    Ok(total_deleted)
}

/// Insert vectors in parallel with rate limiting
///
/// This prevents overwhelming the vector database by limiting the request rate.
///
/// # Arguments
///
/// * `provider` - The vector database provider (wrapped in Arc)
/// * `requests` - Vector of insert requests
/// * `config` - Parallel processing configuration
/// * `rate_limiter` - Rate limiter to control throughput
///
/// # Returns
///
/// Total number of vectors inserted
pub async fn parallel_batch_insert_with_limit<P: VectorProvider + 'static>(
    provider: Arc<P>,
    requests: Vec<InsertRequest>,
    config: ParallelConfig,
    rate_limiter: Arc<RateLimiter>,
) -> Result<usize> {
    if requests.is_empty() {
        return Ok(0);
    }
    let mut join_set = JoinSet::new();
    let mut total_inserted = 0;

    let chunks: Vec<_> = requests.chunks(config.chunk_size).collect();

    for chunk in chunks {
        while join_set.len() >= config.max_concurrent {
            if let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(count)) => total_inserted += count,
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(crate::VectorError::QueryError(format!(
                            "Task join error: {}",
                            e
                        )))
                    }
                }
            }
        }

        let provider_clone = Arc::clone(&provider);
        let rate_limiter_clone = Arc::clone(&rate_limiter);
        let chunk_owned: Vec<_> = chunk.to_vec();
        let chunk_len = chunk_owned.len();

        join_set.spawn(async move {
            // Acquire rate limit permission for the entire chunk
            rate_limiter_clone.acquire(chunk_len as u32).await;

            let mut count = 0;
            for request in chunk_owned {
                provider_clone.insert(request).await?;
                count += 1;
            }
            Ok::<usize, crate::VectorError>(count)
        });
    }

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(count)) => total_inserted += count,
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(crate::VectorError::QueryError(format!(
                    "Task join error: {}",
                    e
                )))
            }
        }
    }

    Ok(total_inserted)
}

/// Execute multiple search queries in parallel with rate limiting
///
/// # Arguments
///
/// * `provider` - The vector database provider (wrapped in Arc)
/// * `requests` - Vector of search requests
/// * `config` - Parallel processing configuration
/// * `rate_limiter` - Rate limiter to control throughput
///
/// # Returns
///
/// Vector of search results, one for each request
pub async fn parallel_batch_search_with_limit<P: VectorProvider + 'static>(
    provider: Arc<P>,
    requests: Vec<SearchRequest>,
    config: ParallelConfig,
    rate_limiter: Arc<RateLimiter>,
) -> Result<Vec<Vec<SearchResult>>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let mut join_set = JoinSet::new();
    let mut results = Vec::with_capacity(requests.len());

    let chunks: Vec<_> = requests.chunks(config.chunk_size).collect();

    for chunk in chunks {
        while join_set.len() >= config.max_concurrent {
            if let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(chunk_results)) => results.extend(chunk_results),
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(crate::VectorError::QueryError(format!(
                            "Task join error: {}",
                            e
                        )))
                    }
                }
            }
        }

        let provider_clone = Arc::clone(&provider);
        let rate_limiter_clone = Arc::clone(&rate_limiter);
        let chunk_owned: Vec<_> = chunk.to_vec();
        let chunk_len = chunk_owned.len();

        join_set.spawn(async move {
            // Acquire rate limit permission for the entire chunk
            rate_limiter_clone.acquire(chunk_len as u32).await;

            let mut chunk_results = Vec::new();
            for request in chunk_owned {
                let search_results = provider_clone.search(request).await?;
                chunk_results.push(search_results);
            }
            Ok::<Vec<Vec<SearchResult>>, crate::VectorError>(chunk_results)
        });
    }

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(chunk_results)) => results.extend(chunk_results),
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(crate::VectorError::QueryError(format!(
                    "Task join error: {}",
                    e
                )))
            }
        }
    }

    Ok(results)
}

/// Update multiple vectors in parallel with rate limiting
///
/// # Arguments
///
/// * `provider` - The vector database provider (wrapped in Arc)
/// * `requests` - Vector of update requests
/// * `config` - Parallel processing configuration
/// * `rate_limiter` - Rate limiter to control throughput
///
/// # Returns
///
/// Total number of vectors updated
pub async fn parallel_batch_update_with_limit<P: VectorProvider + 'static>(
    provider: Arc<P>,
    requests: Vec<UpdateRequest>,
    config: ParallelConfig,
    rate_limiter: Arc<RateLimiter>,
) -> Result<usize> {
    if requests.is_empty() {
        return Ok(0);
    }
    let mut join_set = JoinSet::new();
    let mut total_updated = 0;

    let chunks: Vec<_> = requests.chunks(config.chunk_size).collect();

    for chunk in chunks {
        while join_set.len() >= config.max_concurrent {
            if let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(count)) => total_updated += count,
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(crate::VectorError::QueryError(format!(
                            "Task join error: {}",
                            e
                        )))
                    }
                }
            }
        }

        let provider_clone = Arc::clone(&provider);
        let rate_limiter_clone = Arc::clone(&rate_limiter);
        let chunk_owned: Vec<_> = chunk.to_vec();
        let chunk_len = chunk_owned.len();

        join_set.spawn(async move {
            // Acquire rate limit permission for the entire chunk
            rate_limiter_clone.acquire(chunk_len as u32).await;

            let mut count = 0;
            for request in chunk_owned {
                provider_clone.update(request).await?;
                count += 1;
            }
            Ok::<usize, crate::VectorError>(count)
        });
    }

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(count)) => total_updated += count,
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(crate::VectorError::QueryError(format!(
                    "Task join error: {}",
                    e
                )))
            }
        }
    }

    Ok(total_updated)
}

/// Delete multiple vectors in parallel with rate limiting
///
/// # Arguments
///
/// * `provider` - The vector database provider (wrapped in Arc)
/// * `requests` - Vector of delete requests
/// * `config` - Parallel processing configuration
/// * `rate_limiter` - Rate limiter to control throughput
///
/// # Returns
///
/// Total number of vectors deleted
pub async fn parallel_batch_delete_with_limit<P: VectorProvider + 'static>(
    provider: Arc<P>,
    requests: Vec<DeleteRequest>,
    config: ParallelConfig,
    rate_limiter: Arc<RateLimiter>,
) -> Result<usize> {
    if requests.is_empty() {
        return Ok(0);
    }
    let mut join_set = JoinSet::new();
    let mut total_deleted = 0;

    let chunks: Vec<_> = requests.chunks(config.chunk_size).collect();

    for chunk in chunks {
        while join_set.len() >= config.max_concurrent {
            if let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(count)) => total_deleted += count,
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(crate::VectorError::QueryError(format!(
                            "Task join error: {}",
                            e
                        )))
                    }
                }
            }
        }

        let provider_clone = Arc::clone(&provider);
        let rate_limiter_clone = Arc::clone(&rate_limiter);
        let chunk_owned: Vec<_> = chunk.to_vec();
        let chunk_len = chunk_owned.len();

        join_set.spawn(async move {
            // Acquire rate limit permission for the entire chunk
            rate_limiter_clone.acquire(chunk_len as u32).await;

            let mut count = 0;
            for request in chunk_owned {
                let deleted = provider_clone.delete(request).await?;
                count += deleted;
            }
            Ok::<usize, crate::VectorError>(count)
        });
    }

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(count)) => total_deleted += count,
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(crate::VectorError::QueryError(format!(
                    "Task join error: {}",
                    e
                )))
            }
        }
    }

    Ok(total_deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockVectorProvider;
    use serde_json::json;

    #[tokio::test]
    async fn test_parallel_batch_insert() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        let mut requests = Vec::new();
        for i in 0..50 {
            requests.push(InsertRequest {
                collection: "test".to_string(),
                id: format!("id_{}", i),
                vector: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                payload: json!({"index": i}),
            });
        }

        let config = ParallelConfig {
            max_concurrent: 5,
            chunk_size: 10,
        };

        let inserted = parallel_batch_insert(provider, requests, config)
            .await
            .unwrap();

        assert_eq!(inserted, 50);
    }

    #[tokio::test]
    async fn test_parallel_batch_insert_empty() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        let requests = Vec::new();
        let config = ParallelConfig::default();

        let inserted = parallel_batch_insert(provider, requests, config)
            .await
            .unwrap();

        assert_eq!(inserted, 0);
    }

    #[tokio::test]
    async fn test_parallel_batch_search() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        // Insert some test data
        for i in 0..10 {
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("id_{}", i),
                    vector: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                    payload: json!({"index": i}),
                })
                .await
                .unwrap();
        }

        // Create search requests
        let mut requests = Vec::new();
        for i in 0..5 {
            requests.push(SearchRequest {
                collection: "test".to_string(),
                query: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                top_k: 3,
                score_threshold: None,
                filter: None,
            });
        }

        let config = ParallelConfig {
            max_concurrent: 3,
            chunk_size: 2,
        };

        let results = parallel_batch_search(provider, requests, config)
            .await
            .unwrap();

        assert_eq!(results.len(), 5);
        for result in results {
            assert!(result.len() <= 3);
        }
    }

    #[tokio::test]
    async fn test_parallel_batch_search_empty() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        let requests = Vec::new();
        let config = ParallelConfig::default();

        let results = parallel_batch_search(provider, requests, config)
            .await
            .unwrap();

        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_parallel_config_default() {
        let config = ParallelConfig::default();
        assert_eq!(config.max_concurrent, 10);
        assert_eq!(config.chunk_size, 100);
    }

    #[tokio::test]
    async fn test_parallel_batch_update() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        // Insert some test data
        for i in 0..20 {
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("id_{}", i),
                    vector: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                    payload: json!({"index": i, "status": "original"}),
                })
                .await
                .unwrap();
        }

        // Create update requests
        let mut requests = Vec::new();
        for i in 0..20 {
            requests.push(UpdateRequest {
                collection: "test".to_string(),
                id: format!("id_{}", i),
                vector: None,
                payload: Some(json!({"index": i, "status": "updated"})),
            });
        }

        let config = ParallelConfig {
            max_concurrent: 5,
            chunk_size: 5,
        };

        let updated = parallel_batch_update(provider.clone(), requests, config)
            .await
            .unwrap();

        assert_eq!(updated, 20);

        // Verify updates
        let results = provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![0.0, 1.0, 2.0],
                top_k: 1,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results[0].payload["status"], "updated");
    }

    #[tokio::test]
    async fn test_parallel_batch_update_empty() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        let requests = Vec::new();
        let config = ParallelConfig::default();

        let updated = parallel_batch_update(provider, requests, config)
            .await
            .unwrap();

        assert_eq!(updated, 0);
    }

    #[tokio::test]
    async fn test_parallel_batch_delete() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        // Insert some test data
        for i in 0..30 {
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("id_{}", i),
                    vector: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                    payload: json!({"index": i}),
                })
                .await
                .unwrap();
        }

        // Create delete requests (delete in batches of 5)
        let mut requests = Vec::new();
        for chunk_id in 0..6 {
            let ids: Vec<String> = (chunk_id * 5..(chunk_id + 1) * 5)
                .map(|i| format!("id_{}", i))
                .collect();
            requests.push(DeleteRequest {
                collection: "test".to_string(),
                ids,
            });
        }

        let config = ParallelConfig {
            max_concurrent: 3,
            chunk_size: 2,
        };

        let deleted = parallel_batch_delete(provider.clone(), requests, config)
            .await
            .unwrap();

        assert_eq!(deleted, 30);
        assert_eq!(provider.count("test"), 0);
    }

    #[tokio::test]
    async fn test_parallel_batch_delete_empty() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        let requests = Vec::new();
        let config = ParallelConfig::default();

        let deleted = parallel_batch_delete(provider, requests, config)
            .await
            .unwrap();

        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn test_parallel_batch_insert_with_limit() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        let mut requests = Vec::new();
        for i in 0..50 {
            requests.push(InsertRequest {
                collection: "test".to_string(),
                id: format!("id_{}", i),
                vector: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                payload: json!({"index": i}),
            });
        }

        let config = ParallelConfig {
            max_concurrent: 5,
            chunk_size: 10,
        };

        // Rate limit: 100 req/s
        let rate_limiter = Arc::new(RateLimiter::new(100.0));

        let inserted = parallel_batch_insert_with_limit(provider, requests, config, rate_limiter)
            .await
            .unwrap();

        assert_eq!(inserted, 50);
    }

    #[tokio::test]
    async fn test_parallel_batch_search_with_limit() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        // Insert some test data
        for i in 0..10 {
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("id_{}", i),
                    vector: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                    payload: json!({"index": i}),
                })
                .await
                .unwrap();
        }

        // Create search requests
        let mut requests = Vec::new();
        for i in 0..5 {
            requests.push(SearchRequest {
                collection: "test".to_string(),
                query: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                top_k: 3,
                score_threshold: None,
                filter: None,
            });
        }

        let config = ParallelConfig {
            max_concurrent: 3,
            chunk_size: 2,
        };

        // Rate limit: 50 req/s
        let rate_limiter = Arc::new(RateLimiter::new(50.0));

        let results = parallel_batch_search_with_limit(provider, requests, config, rate_limiter)
            .await
            .unwrap();

        assert_eq!(results.len(), 5);
        for result in results {
            assert!(result.len() <= 3);
        }
    }

    #[tokio::test]
    async fn test_parallel_batch_update_with_limit() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        // Insert some test data
        for i in 0..20 {
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("id_{}", i),
                    vector: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                    payload: json!({"index": i, "status": "original"}),
                })
                .await
                .unwrap();
        }

        // Create update requests
        let mut requests = Vec::new();
        for i in 0..20 {
            requests.push(UpdateRequest {
                collection: "test".to_string(),
                id: format!("id_{}", i),
                vector: None,
                payload: Some(json!({"index": i, "status": "updated"})),
            });
        }

        let config = ParallelConfig {
            max_concurrent: 5,
            chunk_size: 5,
        };

        // Rate limit: 100 req/s
        let rate_limiter = Arc::new(RateLimiter::new(100.0));

        let updated =
            parallel_batch_update_with_limit(provider.clone(), requests, config, rate_limiter)
                .await
                .unwrap();

        assert_eq!(updated, 20);

        // Verify updates
        let results = provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![0.0, 1.0, 2.0],
                top_k: 1,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results[0].payload["status"], "updated");
    }

    #[tokio::test]
    async fn test_parallel_batch_delete_with_limit() {
        let provider = Arc::new(MockVectorProvider::new());
        provider.create_collection("test", 3).await.unwrap();

        // Insert some test data
        for i in 0..30 {
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("id_{}", i),
                    vector: vec![i as f32, i as f32 + 1.0, i as f32 + 2.0],
                    payload: json!({"index": i}),
                })
                .await
                .unwrap();
        }

        // Create delete requests (delete in batches of 5)
        let mut requests = Vec::new();
        for chunk_id in 0..6 {
            let ids: Vec<String> = (chunk_id * 5..(chunk_id + 1) * 5)
                .map(|i| format!("id_{}", i))
                .collect();
            requests.push(DeleteRequest {
                collection: "test".to_string(),
                ids,
            });
        }

        let config = ParallelConfig {
            max_concurrent: 3,
            chunk_size: 2,
        };

        // Rate limit: 50 req/s
        let rate_limiter = Arc::new(RateLimiter::new(50.0));

        let deleted =
            parallel_batch_delete_with_limit(provider.clone(), requests, config, rate_limiter)
                .await
                .unwrap();

        assert_eq!(deleted, 30);
        assert_eq!(provider.count("test"), 0);
    }
}
