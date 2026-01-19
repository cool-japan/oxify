//! Data migration utilities for vector databases
//!
//! This module provides tools to migrate, backup, and restore vector data
//! between different providers or for disaster recovery.

use crate::{BatchInsertRequest, Result, SearchRequest, VectorProvider};
use serde::{Deserialize, Serialize};

/// A snapshot of vector data from a collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSnapshot {
    /// Collection name
    pub collection: String,
    /// Vector dimension
    pub dimension: usize,
    /// Vector data (id, vector, payload)
    pub vectors: Vec<VectorRecord>,
}

/// A single vector record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: String,
    pub vector: Vec<f32>,
    pub payload: serde_json::Value,
}

impl VectorSnapshot {
    /// Create a new empty snapshot
    pub fn new(collection: String, dimension: usize) -> Self {
        Self {
            collection,
            dimension,
            vectors: Vec::new(),
        }
    }

    /// Add a vector record to the snapshot
    pub fn add(&mut self, record: VectorRecord) {
        self.vectors.push(record);
    }

    /// Get the number of vectors in the snapshot
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if the snapshot is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| crate::VectorError::QueryError(format!("Serialization failed: {}", e)))
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| crate::VectorError::QueryError(format!("Deserialization failed: {}", e)))
    }

    /// Save to file
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        let json = self.to_json()?;
        std::fs::write(path, json)
            .map_err(|e| crate::VectorError::QueryError(format!("Failed to write file: {}", e)))
    }

    /// Load from file
    pub fn load_from_file(path: &std::path::Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| crate::VectorError::QueryError(format!("Failed to read file: {}", e)))?;
        Self::from_json(&json)
    }
}

/// Migration progress callback
pub type ProgressCallback = Box<dyn Fn(MigrationProgress) + Send + Sync>;

/// Migration progress information
#[derive(Debug, Clone)]
pub struct MigrationProgress {
    pub total_vectors: usize,
    pub migrated_vectors: usize,
    pub current_batch: usize,
    pub total_batches: usize,
}

impl MigrationProgress {
    /// Calculate progress percentage (0.0 to 1.0)
    pub fn percentage(&self) -> f64 {
        if self.total_vectors == 0 {
            0.0
        } else {
            (self.migrated_vectors as f64) / (self.total_vectors as f64)
        }
    }
}

/// Options for migration operations
pub struct MigrationOptions {
    /// Batch size for export/import operations
    pub batch_size: usize,
    /// Whether to create the target collection if it doesn't exist
    pub create_collection: bool,
    /// Maximum number of vectors to export (None = all)
    pub max_vectors: Option<usize>,
    /// Progress callback
    pub progress_callback: Option<ProgressCallback>,
}

impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            batch_size: 100,
            create_collection: true,
            max_vectors: None,
            progress_callback: None,
        }
    }
}

/// Export data from a collection
///
/// This performs a full scan of the collection by searching with a zero vector
/// and retrieving all results in batches.
pub async fn export_collection<P: VectorProvider>(
    provider: &P,
    collection: &str,
    dimension: usize,
    options: &MigrationOptions,
) -> Result<VectorSnapshot> {
    let mut snapshot = VectorSnapshot::new(collection.to_string(), dimension);
    let mut total_exported = 0;

    tracing::info!(
        collection = collection,
        dimension = dimension,
        "Starting collection export"
    );

    // Search with zero vector to get all vectors
    // Note: This is a simple implementation. Production systems might need
    // pagination APIs or specialized export endpoints.
    let query = vec![0.0; dimension];
    let limit = options.max_vectors.unwrap_or(10000);

    let results = provider
        .search(SearchRequest {
            collection: collection.to_string(),
            query,
            top_k: limit,
            score_threshold: None,
            filter: None,
        })
        .await?;

    for result in results {
        if let Some(vector) = result.vector {
            snapshot.add(VectorRecord {
                id: result.id,
                vector,
                payload: result.payload,
            });
            total_exported += 1;

            if let Some(max) = options.max_vectors {
                if total_exported >= max {
                    break;
                }
            }
        }
    }

    tracing::info!(
        collection = collection,
        total_exported = total_exported,
        "Collection export completed"
    );

    Ok(snapshot)
}

/// Import data to a collection
pub async fn import_snapshot<P: VectorProvider>(
    provider: &P,
    snapshot: &VectorSnapshot,
    options: &MigrationOptions,
) -> Result<usize> {
    tracing::info!(
        collection = snapshot.collection,
        total_vectors = snapshot.vectors.len(),
        "Starting snapshot import"
    );

    // Create collection if needed
    if options.create_collection {
        let exists = provider.collection_exists(&snapshot.collection).await?;
        if !exists {
            provider
                .create_collection(&snapshot.collection, snapshot.dimension)
                .await?;
            tracing::info!(
                collection = snapshot.collection,
                dimension = snapshot.dimension,
                "Created target collection"
            );
        }
    }

    // Import in batches
    let total_vectors = snapshot.vectors.len();
    let mut imported = 0;
    let chunks: Vec<_> = snapshot.vectors.chunks(options.batch_size).collect();
    let total_batches = chunks.len();

    for (batch_idx, chunk) in chunks.iter().enumerate() {
        let batch = chunk
            .iter()
            .map(|r| (r.id.clone(), r.vector.clone(), r.payload.clone()))
            .collect();

        provider
            .batch_insert(BatchInsertRequest {
                collection: snapshot.collection.clone(),
                vectors: batch,
            })
            .await?;

        imported += chunk.len();

        // Report progress
        if let Some(callback) = &options.progress_callback {
            callback(MigrationProgress {
                total_vectors,
                migrated_vectors: imported,
                current_batch: batch_idx + 1,
                total_batches,
            });
        }

        tracing::debug!(
            batch = batch_idx + 1,
            total_batches = total_batches,
            imported = imported,
            "Batch imported"
        );
    }

    tracing::info!(
        collection = snapshot.collection,
        total_imported = imported,
        "Snapshot import completed"
    );

    Ok(imported)
}

/// Migrate data from one provider to another
pub async fn migrate_collection<S: VectorProvider, T: VectorProvider>(
    source: &S,
    target: &T,
    collection: &str,
    dimension: usize,
    options: &MigrationOptions,
) -> Result<usize> {
    tracing::info!(
        collection = collection,
        dimension = dimension,
        "Starting collection migration"
    );

    // Export from source
    let snapshot = export_collection(source, collection, dimension, options).await?;

    tracing::info!(
        collection = collection,
        vectors = snapshot.len(),
        "Exported from source"
    );

    // Import to target
    let imported = import_snapshot(target, &snapshot, options).await?;

    tracing::info!(
        collection = collection,
        imported = imported,
        "Migration completed"
    );

    Ok(imported)
}

/// Verify migration by comparing vector counts and sampling data
pub async fn verify_migration<S: VectorProvider, T: VectorProvider>(
    source: &S,
    target: &T,
    collection: &str,
    dimension: usize,
    sample_size: usize,
) -> Result<MigrationVerification> {
    tracing::info!(
        collection = collection,
        sample_size = sample_size,
        "Starting migration verification"
    );

    // Get collection info from both providers
    let source_info = source.collection_info(collection).await.ok();
    let target_info = target.collection_info(collection).await.ok();

    let source_count = source_info.as_ref().map(|i| i.vector_count).unwrap_or(0);
    let target_count = target_info.as_ref().map(|i| i.vector_count).unwrap_or(0);

    let count_matches = source_count == target_count;

    // Sample some vectors and verify they exist in target
    let query = vec![0.0; dimension];
    let source_results = source
        .search(SearchRequest {
            collection: collection.to_string(),
            query: query.clone(),
            top_k: sample_size,
            score_threshold: None,
            filter: None,
        })
        .await?;

    let mut sample_matches = 0;
    let total_samples = source_results.len();

    for result in &source_results {
        if let Some(vector) = &result.vector {
            // Search for this vector in target
            let target_results = target
                .search(SearchRequest {
                    collection: collection.to_string(),
                    query: vector.clone(),
                    top_k: 1,
                    score_threshold: Some(0.99), // High similarity threshold
                    filter: None,
                })
                .await?;

            if !target_results.is_empty() && target_results[0].id == result.id {
                sample_matches += 1;
            }
        }
    }

    let verification = MigrationVerification {
        source_count,
        target_count,
        count_matches,
        sample_size: total_samples,
        sample_matches,
        sample_match_rate: if total_samples > 0 {
            (sample_matches as f64) / (total_samples as f64)
        } else {
            0.0
        },
    };

    tracing::info!(
        verification = ?verification,
        "Migration verification completed"
    );

    Ok(verification)
}

/// Migration verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationVerification {
    pub source_count: usize,
    pub target_count: usize,
    pub count_matches: bool,
    pub sample_size: usize,
    pub sample_matches: usize,
    pub sample_match_rate: f64,
}

impl MigrationVerification {
    /// Check if migration appears successful
    pub fn is_successful(&self) -> bool {
        self.count_matches && self.sample_match_rate > 0.95
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockVectorProvider;
    use crate::InsertRequest;

    #[test]
    fn test_snapshot_serialization() {
        let mut snapshot = VectorSnapshot::new("test".to_string(), 3);
        snapshot.add(VectorRecord {
            id: "1".to_string(),
            vector: vec![1.0, 2.0, 3.0],
            payload: serde_json::json!({"key": "value"}),
        });

        let json = snapshot.to_json().unwrap();
        let deserialized = VectorSnapshot::from_json(&json).unwrap();

        assert_eq!(deserialized.collection, "test");
        assert_eq!(deserialized.dimension, 3);
        assert_eq!(deserialized.vectors.len(), 1);
        assert_eq!(deserialized.vectors[0].id, "1");
    }

    #[tokio::test]
    async fn test_export_collection() {
        let provider = MockVectorProvider::new();
        provider.create_collection("test", 128).await.unwrap();

        // Insert some test data
        for i in 0..5 {
            provider
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("{}", i),
                    vector: vec![i as f32; 128],
                    payload: serde_json::json!({"index": i}),
                })
                .await
                .unwrap();
        }

        let options = MigrationOptions::default();
        let snapshot = export_collection(&provider, "test", 128, &options)
            .await
            .unwrap();

        assert_eq!(snapshot.collection, "test");
        assert_eq!(snapshot.dimension, 128);
        assert_eq!(snapshot.vectors.len(), 5);
    }

    #[tokio::test]
    async fn test_import_snapshot() {
        let provider = MockVectorProvider::new();

        let mut snapshot = VectorSnapshot::new("imported".to_string(), 128);
        for i in 0..3 {
            snapshot.add(VectorRecord {
                id: format!("{}", i),
                vector: vec![i as f32; 128],
                payload: serde_json::json!({"index": i}),
            });
        }

        let options = MigrationOptions::default();
        let imported = import_snapshot(&provider, &snapshot, &options)
            .await
            .unwrap();

        assert_eq!(imported, 3);

        // Verify data was imported
        let results = provider
            .search(SearchRequest {
                collection: "imported".to_string(),
                query: vec![0.0; 128],
                top_k: 10,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_migrate_collection() {
        let source = MockVectorProvider::new();
        let target = MockVectorProvider::new();

        // Populate source
        source.create_collection("data", 64).await.unwrap();
        for i in 0..10 {
            source
                .insert(InsertRequest {
                    collection: "data".to_string(),
                    id: format!("{}", i),
                    vector: vec![i as f32; 64],
                    payload: serde_json::json!({"value": i}),
                })
                .await
                .unwrap();
        }

        // Migrate
        let options = MigrationOptions::default();
        let migrated = migrate_collection(&source, &target, "data", 64, &options)
            .await
            .unwrap();

        assert_eq!(migrated, 10);

        // Verify target has data
        let results = target
            .search(SearchRequest {
                collection: "data".to_string(),
                query: vec![0.0; 64],
                top_k: 20,
                score_threshold: None,
                filter: None,
            })
            .await
            .unwrap();

        assert_eq!(results.len(), 10);
    }

    #[tokio::test]
    async fn test_migration_progress() {
        let source = MockVectorProvider::new();
        let target = MockVectorProvider::new();

        source.create_collection("test", 32).await.unwrap();
        for i in 0..25 {
            source
                .insert(InsertRequest {
                    collection: "test".to_string(),
                    id: format!("{}", i),
                    vector: vec![i as f32; 32],
                    payload: serde_json::json!({}),
                })
                .await
                .unwrap();
        }

        let progress_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let progress_called_clone = progress_called.clone();

        let options = MigrationOptions {
            batch_size: 10,
            create_collection: true,
            max_vectors: None,
            progress_callback: Some(Box::new(move |progress| {
                progress_called_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                assert!(progress.total_vectors > 0);
                assert!(progress.percentage() >= 0.0 && progress.percentage() <= 1.0);
            })),
        };

        migrate_collection(&source, &target, "test", 32, &options)
            .await
            .unwrap();

        assert!(progress_called.load(std::sync::atomic::Ordering::Relaxed));
    }
}
