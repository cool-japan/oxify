//! Basic vector search example
//!
//! This example demonstrates:
//! - Connecting to a vector database (Qdrant)
//! - Creating a collection
//! - Inserting vectors with metadata
//! - Performing similarity search
//!
//! Run with: cargo run --example basic_search

use oxify_connect_vector::{InsertRequest, QdrantProvider, SearchRequest, VectorProvider};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Basic Vector Search Example\n");

    // 1. Connect to Qdrant
    println!("📡 Connecting to Qdrant...");
    let provider = QdrantProvider::new("http://localhost:6334").await?;
    println!("✅ Connected!\n");

    // 2. Create a collection
    let collection = "demo_products";
    println!("📦 Creating collection '{}'...", collection);

    if !provider.collection_exists(collection).await? {
        provider.create_collection(collection, 384).await?;
        println!("✅ Collection created!\n");
    } else {
        println!("ℹ️  Collection already exists\n");
    }

    // 3. Insert some sample product vectors
    println!("📥 Inserting sample products...");

    let products = vec![
        (
            "prod_001",
            vec![0.1; 384], // Simplified embedding
            json!({
                "name": "Wireless Mouse",
                "category": "electronics",
                "price": 29.99
            }),
        ),
        (
            "prod_002",
            vec![0.2; 384],
            json!({
                "name": "USB Keyboard",
                "category": "electronics",
                "price": 49.99
            }),
        ),
        (
            "prod_003",
            vec![0.3; 384],
            json!({
                "name": "Coffee Mug",
                "category": "kitchen",
                "price": 12.99
            }),
        ),
    ];

    for (id, vector, payload) in products {
        provider
            .insert(InsertRequest {
                collection: collection.to_string(),
                id: id.to_string(),
                vector,
                payload,
            })
            .await?;
    }

    println!("✅ Inserted {} products\n", 3);

    // 4. Perform a similarity search
    println!("🔎 Searching for similar products...");

    let query_vector = vec![0.15; 384]; // Query embedding (looking for electronics)

    let results = provider
        .search(SearchRequest {
            collection: collection.to_string(),
            query: query_vector,
            top_k: 2,
            score_threshold: Some(0.5),
            filter: None,
        })
        .await?;

    println!("✅ Found {} results:\n", results.len());

    for (i, result) in results.iter().enumerate() {
        println!("  {}. {} (score: {:.4})", i + 1, result.id, result.score);
        println!("     Name: {}", result.payload["name"]);
        println!("     Category: {}", result.payload["category"]);
        println!("     Price: ${}\n", result.payload["price"]);
    }

    println!("✨ Example complete!");

    Ok(())
}
