/// Integration test helpers for oxify-connect-vector
///
/// These helpers provide utilities for running integration tests against real databases.
/// Tests using these helpers are ignored by default and must be run explicitly.
///
/// # Environment Variables
///
/// - `QDRANT_URL`: Qdrant server URL (default: http://localhost:6333)
/// - `POSTGRES_URL`: PostgreSQL connection URL (default: postgres://test:test@localhost/vectordb)
/// - `CHROMADB_URL`: ChromaDB server URL (default: http://localhost:8000)
/// - `MILVUS_URL`: Milvus server URL (default: http://localhost:19530)
///
/// # Running Integration Tests
///
/// ```bash
/// # Start test databases
/// docker-compose up -d
///
/// # Wait for databases to be ready
/// sleep 10
///
/// # Run integration tests
/// cargo test --test integration -- --ignored --test-threads=1
///
/// # Cleanup
/// docker-compose down -v
/// ```
use std::env;

/// Get Qdrant URL from environment or use default
pub fn qdrant_url() -> String {
    env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".to_string())
}

/// Get PostgreSQL URL from environment or use default
pub fn postgres_url() -> String {
    env::var("POSTGRES_URL")
        .unwrap_or_else(|_| "postgres://test:test@localhost/vectordb".to_string())
}

/// Get ChromaDB URL from environment or use default
pub fn chromadb_url() -> String {
    env::var("CHROMADB_URL").unwrap_or_else(|_| "http://localhost:8000".to_string())
}

/// Get Milvus URL from environment or use default
pub fn milvus_url() -> String {
    env::var("MILVUS_URL").unwrap_or_else(|_| "http://localhost:19530".to_string())
}

/// Check if integration tests should run
#[allow(dead_code)]
pub fn should_run_integration_tests() -> bool {
    env::var("RUN_INTEGRATION_TESTS").is_ok()
}

/// Generate a unique collection name for tests
pub fn unique_collection_name(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{}_{}", prefix, timestamp)
}

/// Test vector generation helper
pub fn generate_test_vectors(count: usize, dimension: usize) -> Vec<Vec<f32>> {
    (0..count)
        .map(|i| {
            let mut vec = vec![0.0; dimension];
            // Create a simple pattern for testing
            vec[i % dimension] = 1.0;
            if dimension > 1 {
                vec[(i + 1) % dimension] = 0.5;
            }
            vec
        })
        .collect()
}

/// Normalize a vector to unit length
pub fn normalize(vec: &[f32]) -> Vec<f32> {
    let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude == 0.0 {
        return vec.to_vec();
    }
    vec.iter().map(|x| x / magnitude).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_collection_name() {
        let name1 = unique_collection_name("test");
        // Small sleep to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(2));
        let name2 = unique_collection_name("test");
        assert_ne!(name1, name2);
        assert!(name1.starts_with("test_"));
    }

    #[test]
    fn test_generate_test_vectors() {
        let vectors = generate_test_vectors(5, 3);
        assert_eq!(vectors.len(), 5);
        assert_eq!(vectors[0].len(), 3);

        // Check pattern
        assert_eq!(vectors[0][0], 1.0);
        assert_eq!(vectors[0][1], 0.5);
        assert_eq!(vectors[0][2], 0.0);
    }

    #[test]
    fn test_normalize() {
        let vec = vec![3.0, 4.0];
        let normalized = normalize(&vec);
        assert!((normalized[0] - 0.6).abs() < 1e-6);
        assert!((normalized[1] - 0.8).abs() < 1e-6);

        // Verify unit length
        let magnitude: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_default_urls() {
        // These should not panic
        let _ = qdrant_url();
        let _ = postgres_url();
        let _ = chromadb_url();
        let _ = milvus_url();
    }
}
