//! Health check functionality for vector database providers
//!
//! This module provides health check capabilities to monitor the connectivity
//! and operational status of vector database providers.

use crate::{Result, VectorProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Health status for a vector database provider
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    /// Provider is healthy and operational
    Healthy,
    /// Provider is degraded (slow responses, partial failures)
    Degraded,
    /// Provider is unhealthy (connection failures, errors)
    Unhealthy,
}

/// Detailed health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Overall health status
    pub status: HealthStatus,
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Error message if unhealthy
    pub message: Option<String>,
    /// Timestamp of the health check
    pub timestamp: std::time::SystemTime,
}

/// Trait for health checkable providers
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Perform a health check
    ///
    /// This should be a lightweight operation that verifies connectivity
    /// and basic functionality without heavy computation.
    async fn health_check(&self) -> HealthCheckResult;
}

/// Default health check implementation for VectorProvider
///
/// Attempts to check if a test collection exists, which verifies
/// connectivity without performing expensive operations.
pub async fn default_health_check<P: VectorProvider>(provider: &P) -> HealthCheckResult {
    let start = Instant::now();
    let test_collection = "__health_check__";

    match provider.collection_exists(test_collection).await {
        Ok(_) => HealthCheckResult {
            status: HealthStatus::Healthy,
            response_time_ms: start.elapsed().as_millis() as u64,
            message: None,
            timestamp: std::time::SystemTime::now(),
        },
        Err(e) => {
            let response_time_ms = start.elapsed().as_millis() as u64;
            let status = if response_time_ms > 5000 {
                HealthStatus::Degraded
            } else {
                HealthStatus::Unhealthy
            };

            HealthCheckResult {
                status,
                response_time_ms,
                message: Some(format!("Health check failed: {}", e)),
                timestamp: std::time::SystemTime::now(),
            }
        }
    }
}

/// Health monitor that periodically checks provider health
pub struct HealthMonitor<P>
where
    P: VectorProvider,
{
    provider: P,
    interval: Duration,
    last_result: Option<HealthCheckResult>,
}

impl<P> HealthMonitor<P>
where
    P: VectorProvider,
{
    /// Create a new health monitor
    pub fn new(provider: P, interval: Duration) -> Self {
        Self {
            provider,
            interval,
            last_result: None,
        }
    }

    /// Get the check interval
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Get the last health check result
    pub fn last_result(&self) -> Option<&HealthCheckResult> {
        self.last_result.as_ref()
    }

    /// Perform a health check now
    pub async fn check_now(&mut self) -> &HealthCheckResult {
        let result = default_health_check(&self.provider).await;
        self.last_result = Some(result);
        self.last_result.as_ref().unwrap()
    }

    /// Get reference to the provider
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Consume the monitor and return the provider
    pub fn into_provider(self) -> P {
        self.provider
    }
}

/// Health check middleware that wraps a VectorProvider
pub struct HealthCheckProvider<P>
where
    P: VectorProvider,
{
    provider: P,
    last_health_check: std::sync::Arc<tokio::sync::RwLock<Option<HealthCheckResult>>>,
}

impl<P> HealthCheckProvider<P>
where
    P: VectorProvider,
{
    /// Create a new health check provider
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            last_health_check: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Get the last health check result
    pub async fn last_health_check(&self) -> Option<HealthCheckResult> {
        self.last_health_check.read().await.clone()
    }

    /// Get reference to the underlying provider
    pub fn inner(&self) -> &P {
        &self.provider
    }

    /// Consume this wrapper and return the underlying provider
    pub fn into_inner(self) -> P {
        self.provider
    }
}

#[async_trait]
impl<P> HealthCheck for HealthCheckProvider<P>
where
    P: VectorProvider,
{
    async fn health_check(&self) -> HealthCheckResult {
        let result = default_health_check(&self.provider).await;
        *self.last_health_check.write().await = Some(result.clone());
        result
    }
}

#[async_trait]
impl<P> VectorProvider for HealthCheckProvider<P>
where
    P: VectorProvider,
{
    async fn search(&self, request: crate::SearchRequest) -> Result<Vec<crate::SearchResult>> {
        self.provider.search(request).await
    }

    async fn insert(&self, request: crate::InsertRequest) -> Result<()> {
        self.provider.insert(request).await
    }

    async fn delete(&self, request: crate::DeleteRequest) -> Result<usize> {
        self.provider.delete(request).await
    }

    async fn create_collection(&self, name: &str, dimension: usize) -> Result<()> {
        self.provider.create_collection(name, dimension).await
    }

    async fn collection_exists(&self, name: &str) -> Result<bool> {
        self.provider.collection_exists(name).await
    }

    async fn batch_insert(&self, request: crate::BatchInsertRequest) -> Result<usize> {
        self.provider.batch_insert(request).await
    }

    async fn update(&self, request: crate::UpdateRequest) -> Result<()> {
        self.provider.update(request).await
    }

    async fn collection_info(&self, name: &str) -> Result<crate::CollectionInfo> {
        self.provider.collection_info(name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockVectorProvider;

    #[tokio::test]
    async fn test_default_health_check() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 128).await.unwrap();

        let result = default_health_check(&provider).await;
        assert_eq!(result.status, HealthStatus::Healthy);
        assert!(result.message.is_none());
    }

    #[tokio::test]
    async fn test_health_monitor() {
        let provider = MockVectorProvider::new();
        let mut monitor = HealthMonitor::new(provider, Duration::from_secs(60));

        assert!(monitor.last_result().is_none());

        let result = monitor.check_now().await;
        assert_eq!(result.status, HealthStatus::Healthy);
        assert!(monitor.last_result().is_some());
    }

    #[tokio::test]
    async fn test_health_check_provider() {
        let provider = MockVectorProvider::new();
        let health_provider = HealthCheckProvider::new(provider);

        assert!(health_provider.last_health_check().await.is_none());

        let result = health_provider.health_check().await;
        assert_eq!(result.status, HealthStatus::Healthy);

        let last = health_provider.last_health_check().await;
        assert!(last.is_some());
        assert_eq!(last.unwrap().status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_health_check_integration() {
        use crate::{InsertRequest, SearchRequest};

        let provider = MockVectorProvider::new();
        let health_provider = HealthCheckProvider::new(provider);

        // Perform health check
        let result = health_provider.health_check().await;
        assert_eq!(result.status, HealthStatus::Healthy);

        // Use provider normally
        health_provider
            .create_collection("test", 128)
            .await
            .unwrap();

        health_provider
            .insert(InsertRequest {
                collection: "test".to_string(),
                id: "1".to_string(),
                vector: vec![0.1; 128],
                payload: serde_json::json!({"key": "value"}),
            })
            .await
            .unwrap();

        let results = health_provider
            .search(SearchRequest {
                collection: "test".to_string(),
                query: vec![0.1; 128],
                top_k: 5,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }
}
