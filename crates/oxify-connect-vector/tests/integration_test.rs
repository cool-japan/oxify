/// Integration tests for oxify-connect-vector
///
/// These tests require running databases via Docker Compose.
/// All tests are ignored by default and must be run explicitly.
///
/// # Setup
///
/// ```bash
/// docker-compose up -d
/// sleep 10  # Wait for databases to be ready
/// ```
///
/// # Running Tests
///
/// ```bash
/// # Run all integration tests
/// cargo test --test integration_test -- --ignored --test-threads=1
///
/// # Run specific provider tests
/// cargo test --test integration_test test_qdrant -- --ignored
/// cargo test --test integration_test test_pgvector -- --ignored
/// cargo test --test integration_test test_chromadb -- --ignored
/// ```
///
/// # Cleanup
///
/// ```bash
/// docker-compose down -v
/// ```
mod integration_helpers;

use integration_helpers::*;
use oxify_connect_vector::*;

const TEST_DIMENSION: usize = 128;
const TEST_VECTORS_COUNT: usize = 10;

#[tokio::test]
#[ignore]
async fn test_qdrant_integration() {
    let provider = QdrantProvider::new(&qdrant_url())
        .await
        .expect("Failed to connect to Qdrant");
    let collection = unique_collection_name("qdrant_test");

    // Create collection
    provider
        .create_collection(&collection, TEST_DIMENSION)
        .await
        .expect("Failed to create collection");

    // Verify collection exists
    let exists = provider
        .collection_exists(&collection)
        .await
        .expect("Failed to check collection existence");
    assert!(exists);

    // Insert vectors
    let vectors = generate_test_vectors(TEST_VECTORS_COUNT, TEST_DIMENSION);
    for (i, vector) in vectors.iter().enumerate() {
        provider
            .insert(InsertRequest {
                collection: collection.clone(),
                id: format!("vec_{}", i),
                vector: vector.clone(),
                payload: serde_json::json!({ "index": i }),
            })
            .await
            .expect("Failed to insert vector");
    }

    // Search
    let results = provider
        .search(SearchRequest {
            collection: collection.clone(),
            query: vectors[0].clone(),
            top_k: 5,
            score_threshold: Some(0.5),
            filter: None,
        })
        .await
        .expect("Failed to search");

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "vec_0"); // Should match itself

    // Batch insert
    let batch_vectors: Vec<_> = (10..15)
        .map(|i| {
            (
                format!("batch_{}", i),
                generate_test_vectors(1, TEST_DIMENSION)[0].clone(),
                serde_json::json!({ "batch": true }),
            )
        })
        .collect();

    let count = provider
        .batch_insert(BatchInsertRequest {
            collection: collection.clone(),
            vectors: batch_vectors,
        })
        .await
        .expect("Failed to batch insert");
    assert_eq!(count, 5);

    // Update vector
    provider
        .update(UpdateRequest {
            collection: collection.clone(),
            id: "vec_0".to_string(),
            vector: Some(vectors[1].clone()),
            payload: Some(serde_json::json!({ "updated": true })),
        })
        .await
        .expect("Failed to update vector");

    // Get collection info
    let info = provider
        .collection_info(&collection)
        .await
        .expect("Failed to get collection info");
    assert_eq!(info.name, collection);
    assert_eq!(info.dimension, TEST_DIMENSION);

    // Delete vectors
    let deleted = provider
        .delete(DeleteRequest {
            collection: collection.clone(),
            ids: vec!["vec_0".to_string(), "vec_1".to_string()],
        })
        .await
        .expect("Failed to delete vectors");
    assert_eq!(deleted, 2);

    println!("✓ Qdrant integration test passed");
}

// #[tokio::test]
// #[ignore]
// async fn test_pgvector_integration() {
//     // Disabled: PgVectorProvider has been removed in SQLite migration
//     // let provider = PgVectorProvider::new(&postgres_url())
//     //     .await
//     //     .expect("Failed to connect to PostgreSQL");
//
//     // let collection = unique_collection_name("pg_test");
//
//     // // Create collection
//     // provider
//     //     .create_collection(&collection, TEST_DIMENSION)
//     //     .await
//     //     .expect("Failed to create collection");
//
//     // // Verify collection exists
//     // let exists = provider
//     //     .collection_exists(&collection)
//     //     .await
//     //     .expect("Failed to check collection existence");
//     // assert!(exists);
//
//     // // Insert vectors
//     // let vectors = generate_test_vectors(TEST_VECTORS_COUNT, TEST_DIMENSION);
//     // for (i, vector) in vectors.iter().enumerate() {
//     //     provider
//     //         .insert(InsertRequest {
//     //             collection: collection.clone(),
//     //             id: format!("vec_{}", i),
//     //             vector: vector.clone(),
//     //             payload: serde_json::json!({ "index": i }),
//     //         })
//     //         .await
//     //         .expect("Failed to insert vector");
//     // }
//
//     // // Search
//     // let results = provider
//     //     .search(SearchRequest {
//     //         collection: collection.clone(),
//     //         query: vectors[0].clone(),
//     //         top_k: 5,
//     //         score_threshold: Some(0.5),
//     //         filter: None,
//     //     })
//     //     .await
//     //     .expect("Failed to search");
//
//     // assert!(!results.is_empty());
//
//     // // Batch insert
//     // let batch_vectors: Vec<_> = (10..15)
//     //     .map(|i| {
//     //         (
//     //             format!("batch_{}", i),
//     //             generate_test_vectors(1, TEST_DIMENSION)[0].clone(),
//     //             serde_json::json!({ "batch": true }),
//     //         )
//     //     })
//     //     .collect();
//
//     // let count = provider
//     //     .batch_insert(BatchInsertRequest {
//     //         collection: collection.clone(),
//     //         vectors: batch_vectors,
//     //     })
//     //     .await
//     //     .expect("Failed to batch insert");
//     // assert_eq!(count, 5);
//
//     // // Get collection info
//     // let info = provider
//     //     .collection_info(&collection)
//     //     .await
//     //     .expect("Failed to get collection info");
//     // assert_eq!(info.name, collection);
//     // assert_eq!(info.dimension, TEST_DIMENSION);
//
//     // // Delete vectors
//     // let deleted = provider
//     //     .delete(DeleteRequest {
//     //         collection: collection.clone(),
//     //         ids: vec!["vec_0".to_string(), "vec_1".to_string()],
//     //     })
//     //     .await
//     //     .expect("Failed to delete vectors");
//     // assert_eq!(deleted, 2);
//
//     // println!("✓ pgvector integration test passed");
// }

#[tokio::test]
#[ignore]
async fn test_chromadb_integration() {
    let provider = ChromaDBProvider::new(chromadb_url());
    let collection = unique_collection_name("chroma_test");

    // Create collection
    provider
        .create_collection(&collection, TEST_DIMENSION)
        .await
        .expect("Failed to create collection");

    // Verify collection exists
    let exists = provider
        .collection_exists(&collection)
        .await
        .expect("Failed to check collection existence");
    assert!(exists);

    // Insert vectors
    let vectors = generate_test_vectors(TEST_VECTORS_COUNT, TEST_DIMENSION);
    for (i, vector) in vectors.iter().enumerate() {
        provider
            .insert(InsertRequest {
                collection: collection.clone(),
                id: format!("vec_{}", i),
                vector: vector.clone(),
                payload: serde_json::json!({ "index": i }),
            })
            .await
            .expect("Failed to insert vector");
    }

    // Search
    let results = provider
        .search(SearchRequest {
            collection: collection.clone(),
            query: vectors[0].clone(),
            top_k: 5,
            score_threshold: Some(0.5),
            filter: None,
        })
        .await
        .expect("Failed to search");

    assert!(!results.is_empty());

    // Batch insert
    let batch_vectors: Vec<_> = (10..15)
        .map(|i| {
            (
                format!("batch_{}", i),
                generate_test_vectors(1, TEST_DIMENSION)[0].clone(),
                serde_json::json!({ "batch": true }),
            )
        })
        .collect();

    let count = provider
        .batch_insert(BatchInsertRequest {
            collection: collection.clone(),
            vectors: batch_vectors,
        })
        .await
        .expect("Failed to batch insert");
    assert_eq!(count, 5);

    // Delete vectors
    let deleted = provider
        .delete(DeleteRequest {
            collection: collection.clone(),
            ids: vec!["vec_0".to_string(), "vec_1".to_string()],
        })
        .await
        .expect("Failed to delete vectors");
    assert_eq!(deleted, 2);

    println!("✓ ChromaDB integration test passed");
}

#[tokio::test]
#[ignore]
async fn test_hybrid_search_integration() {
    let mock = MockVectorProvider::new();
    let collection = "hybrid_test";

    mock.create_collection(collection, TEST_DIMENSION)
        .await
        .expect("Failed to create collection");

    // Insert test documents
    let documents = [
        "The quick brown fox jumps over the lazy dog",
        "A fast auburn fox leaps above a sleepy canine",
        "Machine learning is a subset of artificial intelligence",
        "Deep learning uses neural networks with multiple layers",
    ];

    let vectors = generate_test_vectors(documents.len(), TEST_DIMENSION);

    for (i, (doc, vector)) in documents.iter().zip(vectors.iter()).enumerate() {
        mock.insert(InsertRequest {
            collection: collection.to_string(),
            id: format!("doc_{}", i),
            vector: vector.clone(),
            payload: serde_json::json!({ "text": doc }),
        })
        .await
        .expect("Failed to insert document");
    }

    // Build BM25 documents
    let bm25_docs: Vec<_> = documents
        .iter()
        .enumerate()
        .map(|(i, text)| Bm25Document {
            id: format!("doc_{}", i),
            text: text.to_string(),
            metadata: serde_json::json!({"index": i}),
        })
        .collect();

    // Create hybrid search engine with custom params
    let params = HybridSearchParams {
        semantic_weight: 0.7,
        keyword_weight: 0.3,
        rrf_k: 60.0,
    };
    let hybrid_engine = HybridSearchEngine::new(mock, bm25_docs, params);

    // Perform hybrid search
    let results = hybrid_engine
        .search(
            SearchRequest {
                collection: collection.to_string(),
                query: vectors[0].clone(),
                top_k: 10,
                score_threshold: None,
                filter: None,
            },
            "fox dog",
            10,
        )
        .await
        .expect("Failed to perform hybrid search");

    assert!(!results.is_empty());

    println!("✓ Hybrid search integration test passed");
}

#[tokio::test]
#[ignore]
async fn test_colbert_integration() {
    let mock = MockVectorProvider::new();
    let collection = "colbert_test";

    mock.create_collection(collection, TEST_DIMENSION)
        .await
        .expect("Failed to create collection");

    let colbert = ColBERTProvider::new(mock);

    // Insert multi-vector document
    let doc_vectors = generate_test_vectors(5, TEST_DIMENSION);
    let count = colbert
        .insert_multi_vector(MultiVectorInsertRequest {
            collection: collection.to_string(),
            id: "doc1".to_string(),
            vectors: doc_vectors.clone(),
            payload: serde_json::json!({ "title": "Test Document" }),
        })
        .await
        .expect("Failed to insert multi-vector document");
    assert_eq!(count, 5);

    // Search with multiple query vectors
    let query_vectors = generate_test_vectors(3, TEST_DIMENSION);
    let results = colbert
        .search_multi_vector(collection, query_vectors, 10, None)
        .await
        .expect("Failed to search multi-vector");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");
    assert_eq!(results[0].maxsim_scores.len(), 3);

    println!("✓ ColBERT integration test passed");
}
