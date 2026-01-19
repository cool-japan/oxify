//! PostgreSQL pgvector provider implementation
//!
//! This module provides vector similarity search using the pgvector PostgreSQL extension.
//! It requires PostgreSQL with the pgvector extension installed.

use crate::{
    BatchInsertRequest, CollectionInfo, DeleteRequest, InsertRequest, Result, SearchRequest,
    SearchResult, UpdateRequest, VectorError, VectorProvider,
};
use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::time::Duration;

/// Distance metric for vector similarity search
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceMetric {
    /// Cosine distance (1 - cosine similarity)
    /// Best for: Normalized vectors, text embeddings
    Cosine,
    /// L2 (Euclidean) distance
    /// Best for: Image embeddings, general-purpose similarity
    L2,
    /// Inner product (negative dot product)
    /// Best for: When vectors are already normalized and you want maximum performance
    InnerProduct,
}

impl DistanceMetric {
    /// Get the pgvector operator for this distance metric
    pub fn operator(&self) -> &'static str {
        match self {
            DistanceMetric::Cosine => "<=>",
            DistanceMetric::L2 => "<->",
            DistanceMetric::InnerProduct => "<#>",
        }
    }

    /// Get the index operator class for this distance metric
    pub fn index_ops(&self) -> &'static str {
        match self {
            DistanceMetric::Cosine => "vector_cosine_ops",
            DistanceMetric::L2 => "vector_l2_ops",
            DistanceMetric::InnerProduct => "vector_ip_ops",
        }
    }

    /// Convert distance to similarity score (0.0 to 1.0)
    pub fn distance_to_score(&self, distance: f32) -> f64 {
        match self {
            DistanceMetric::Cosine => 1.0 - distance as f64,
            DistanceMetric::L2 => {
                // Convert L2 distance to similarity score using exponential decay
                // Score approaches 1.0 as distance approaches 0
                (-distance as f64).exp()
            }
            DistanceMetric::InnerProduct => {
                // Inner product is already a similarity measure (higher is better)
                // Negate because pgvector returns negative values
                -distance as f64
            }
        }
    }
}

/// Builder for configuring PgVectorProvider
pub struct PgVectorProviderBuilder {
    database_url: String,
    max_connections: u32,
    acquire_timeout: Duration,
    idle_timeout: Option<Duration>,
    distance_metric: DistanceMetric,
}

impl PgVectorProviderBuilder {
    /// Create a new builder with the database URL
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            max_connections: 10,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Some(Duration::from_secs(600)),
            distance_metric: DistanceMetric::Cosine,
        }
    }

    /// Set the maximum number of connections in the pool (default: 10)
    pub fn max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Set the connection acquire timeout (default: 5 seconds)
    pub fn acquire_timeout(mut self, timeout: Duration) -> Self {
        self.acquire_timeout = timeout;
        self
    }

    /// Set the idle connection timeout (default: 600 seconds)
    pub fn idle_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Set the distance metric to use (default: Cosine)
    pub fn distance_metric(mut self, metric: DistanceMetric) -> Self {
        self.distance_metric = metric;
        self
    }

    /// Build the provider
    pub async fn build(self) -> Result<PgVectorProvider> {
        let mut pool_options = PgPoolOptions::new()
            .max_connections(self.max_connections)
            .acquire_timeout(self.acquire_timeout);

        if let Some(idle_timeout) = self.idle_timeout {
            pool_options = pool_options.idle_timeout(idle_timeout);
        }

        let pool = pool_options
            .connect(&self.database_url)
            .await
            .map_err(|e| {
                VectorError::ConnectionError(format!("Failed to connect to PostgreSQL: {}", e))
            })?;

        // Ensure pgvector extension is enabled
        sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(&pool)
            .await
            .map_err(|e| {
                VectorError::DatabaseError(format!("Failed to enable pgvector extension: {}", e))
            })?;

        Ok(PgVectorProvider {
            pool,
            distance_metric: self.distance_metric,
        })
    }
}

/// pgvector provider implementation
pub struct PgVectorProvider {
    pool: PgPool,
    distance_metric: DistanceMetric,
}

impl PgVectorProvider {
    /// Create a new pgvector provider with default settings
    ///
    /// # Arguments
    /// * `database_url` - PostgreSQL connection string (e.g., "postgres://user:pass@localhost/db")
    ///
    /// For advanced configuration, use `PgVectorProviderBuilder::new(database_url).build()`.
    pub async fn new(database_url: &str) -> Result<Self> {
        PgVectorProviderBuilder::new(database_url).build().await
    }

    /// Create a builder for configuring the provider
    ///
    /// # Example
    /// ```no_run
    /// use oxify_connect_vector::pgvector::{PgVectorProvider, DistanceMetric};
    /// use std::time::Duration;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = PgVectorProvider::builder("postgres://localhost/db")
    ///     .max_connections(20)
    ///     .acquire_timeout(Duration::from_secs(10))
    ///     .distance_metric(DistanceMetric::L2)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn builder(database_url: impl Into<String>) -> PgVectorProviderBuilder {
        PgVectorProviderBuilder::new(database_url)
    }

    /// Get the underlying connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get the distance metric being used
    pub fn distance_metric(&self) -> DistanceMetric {
        self.distance_metric
    }

    /// Convert Vec<f32> to pgvector string format: '[1,2,3]'
    fn vector_to_string(vector: &[f32]) -> String {
        format!(
            "[{}]",
            vector
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    /// Parse pgvector string '[1,2,3]' to Vec<f32>
    fn string_to_vector(s: &str) -> Vec<f32> {
        s.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .filter_map(|v| v.trim().parse::<f32>().ok())
            .collect()
    }
}

#[async_trait]
impl VectorProvider for PgVectorProvider {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let vector_str = Self::vector_to_string(&request.query);
        let distance_op = self.distance_metric.operator();

        // Build WHERE clause from filter
        let (filter_clause, filter_json) = if let Some(filter) = &request.filter {
            ("AND payload @> $3::jsonb", Some(filter.to_string()))
        } else {
            ("", None)
        };

        let query = format!(
            r#"
            SELECT id, embedding {} $1::vector AS distance, payload, embedding::text
            FROM {}
            WHERE 1=1 {}
            ORDER BY embedding {} $1::vector
            LIMIT $2
            "#,
            distance_op, request.collection, filter_clause, distance_op
        );

        let mut query_builder = sqlx::query(&query)
            .bind(&vector_str)
            .bind(request.top_k as i64);

        if let Some(filter_json) = filter_json {
            query_builder = query_builder.bind(filter_json);
        }

        let rows = query_builder
            .fetch_all(&self.pool)
            .await
            .map_err(|e| VectorError::QueryError(format!("Search failed: {}", e)))?;

        let mut results = Vec::new();

        for row in rows {
            let distance: f32 = row.try_get("distance").unwrap_or(1.0);
            let score = self.distance_metric.distance_to_score(distance);

            // Apply score threshold if specified
            if let Some(threshold) = request.score_threshold {
                if score < threshold {
                    continue;
                }
            }

            let id: String = row.try_get("id").unwrap_or_default();
            let payload: serde_json::Value = row.try_get("payload").unwrap_or_default();
            let embedding_str: Option<String> = row.try_get("embedding").ok();

            results.push(SearchResult {
                id,
                score,
                payload,
                vector: embedding_str.map(|s| Self::string_to_vector(&s)),
            });
        }

        Ok(results)
    }

    async fn insert(&self, request: InsertRequest) -> Result<()> {
        let vector_str = Self::vector_to_string(&request.vector);

        sqlx::query(&format!(
            r#"
            INSERT INTO {} (id, embedding, payload)
            VALUES ($1, $2::vector, $3::jsonb)
            ON CONFLICT (id) DO UPDATE
            SET embedding = EXCLUDED.embedding,
                payload = EXCLUDED.payload
            "#,
            request.collection
        ))
        .bind(&request.id)
        .bind(&vector_str)
        .bind(&request.payload)
        .execute(&self.pool)
        .await
        .map_err(|e| VectorError::DatabaseError(format!("Insert failed: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, request: DeleteRequest) -> Result<usize> {
        let result = sqlx::query(&format!(
            r#"
            DELETE FROM {}
            WHERE id = ANY($1)
            "#,
            request.collection
        ))
        .bind(&request.ids)
        .execute(&self.pool)
        .await
        .map_err(|e| VectorError::DatabaseError(format!("Delete failed: {}", e)))?;

        Ok(result.rows_affected() as usize)
    }

    async fn create_collection(&self, name: &str, dimension: usize) -> Result<()> {
        sqlx::query(&format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                embedding vector({}),
                payload JSONB,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#,
            name, dimension
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| VectorError::DatabaseError(format!("Create collection failed: {}", e)))?;

        // Create HNSW index for fast similarity search with appropriate distance metric
        let index_ops = self.distance_metric.index_ops();
        sqlx::query(&format!(
            r#"
            CREATE INDEX IF NOT EXISTS {}_embedding_idx
            ON {} USING hnsw (embedding {})
            "#,
            name, name, index_ops
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| VectorError::DatabaseError(format!("Create index failed: {}", e)))?;

        Ok(())
    }

    async fn collection_exists(&self, name: &str) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_name = $1
            )
            "#,
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| VectorError::QueryError(format!("Collection exists check failed: {}", e)))?;

        let exists: bool = row.try_get(0).unwrap_or(false);
        Ok(exists)
    }

    async fn batch_insert(&self, request: BatchInsertRequest) -> Result<usize> {
        if request.vectors.is_empty() {
            return Ok(0);
        }

        // Build multi-row INSERT statement for efficiency
        let mut values = Vec::new();
        let mut params: Vec<String> = Vec::new();

        for (idx, (id, vector, payload)) in request.vectors.iter().enumerate() {
            let base = idx * 3;
            values.push(format!(
                "(${}, ${}::vector, ${}::jsonb)",
                base + 1,
                base + 2,
                base + 3
            ));
            params.push(id.clone());
            params.push(Self::vector_to_string(vector));
            params.push(payload.to_string());
        }

        let query = format!(
            r#"
            INSERT INTO {} (id, embedding, payload)
            VALUES {}
            ON CONFLICT (id) DO UPDATE
            SET embedding = EXCLUDED.embedding,
                payload = EXCLUDED.payload
            "#,
            request.collection,
            values.join(", ")
        );

        let mut query_builder = sqlx::query(&query);
        for param in params {
            query_builder = query_builder.bind(param);
        }

        let result = query_builder
            .execute(&self.pool)
            .await
            .map_err(|e| VectorError::DatabaseError(format!("Batch insert failed: {}", e)))?;

        Ok(result.rows_affected() as usize)
    }

    async fn update(&self, request: UpdateRequest) -> Result<()> {
        if request.vector.is_none() && request.payload.is_none() {
            return Err(VectorError::QueryError(
                "Update requires either vector or payload".to_string(),
            ));
        }

        // Check if the vector exists first
        let exists = sqlx::query(&format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE id = $1)",
            request.collection
        ))
        .bind(&request.id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| VectorError::QueryError(format!("Failed to check if vector exists: {}", e)))?;

        let exists: bool = exists.try_get(0).unwrap_or(false);
        if !exists {
            return Err(VectorError::QueryError(format!(
                "Vector {} not found",
                request.id
            )));
        }

        // Update based on what's provided
        match (request.vector, request.payload) {
            (Some(vector), Some(payload)) => {
                // Update both
                let vector_str = Self::vector_to_string(&vector);
                sqlx::query(&format!(
                    "UPDATE {} SET embedding = $1::vector, payload = $2::jsonb WHERE id = $3",
                    request.collection
                ))
                .bind(&vector_str)
                .bind(&payload)
                .bind(&request.id)
                .execute(&self.pool)
                .await
                .map_err(|e| VectorError::DatabaseError(format!("Update failed: {}", e)))?;
            }
            (Some(vector), None) => {
                // Update only vector
                let vector_str = Self::vector_to_string(&vector);
                sqlx::query(&format!(
                    "UPDATE {} SET embedding = $1::vector WHERE id = $2",
                    request.collection
                ))
                .bind(&vector_str)
                .bind(&request.id)
                .execute(&self.pool)
                .await
                .map_err(|e| VectorError::DatabaseError(format!("Update failed: {}", e)))?;
            }
            (None, Some(payload)) => {
                // Update only payload
                sqlx::query(&format!(
                    "UPDATE {} SET payload = $1::jsonb WHERE id = $2",
                    request.collection
                ))
                .bind(&payload)
                .bind(&request.id)
                .execute(&self.pool)
                .await
                .map_err(|e| VectorError::DatabaseError(format!("Update failed: {}", e)))?;
            }
            (None, None) => {
                return Err(VectorError::QueryError(
                    "Update requires either vector or payload".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn collection_info(&self, name: &str) -> Result<CollectionInfo> {
        // Get dimension from the table schema
        let dimension_row = sqlx::query(
            r#"
            SELECT a.atttypmod
            FROM pg_class c
            JOIN pg_attribute a ON a.attrelid = c.oid
            JOIN pg_type t ON t.oid = a.atttypid
            WHERE c.relname = $1 AND a.attname = 'embedding' AND t.typname = 'vector'
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| VectorError::QueryError(format!("Failed to get dimension: {}", e)))?;

        let dimension = if let Some(row) = dimension_row {
            let typmod: i32 = row.try_get(0).unwrap_or(-1);
            if typmod > 0 {
                typmod as usize
            } else {
                0
            }
        } else {
            return Err(VectorError::QueryError(format!(
                "Collection {} not found or has no vector column",
                name
            )));
        };

        // Get vector count
        let count_row = sqlx::query(&format!("SELECT COUNT(*) FROM {}", name))
            .fetch_one(&self.pool)
            .await
            .map_err(|e| VectorError::QueryError(format!("Failed to count vectors: {}", e)))?;

        let vector_count: i64 = count_row.try_get(0).unwrap_or(0);

        Ok(CollectionInfo {
            name: name.to_string(),
            dimension,
            vector_count: vector_count as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires PostgreSQL with pgvector
    async fn test_pgvector_lifecycle() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/oxify_test".to_string());

        let provider = PgVectorProvider::new(&database_url).await.unwrap();

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
