//! Hybrid Search Example
//!
//! This example demonstrates:
//! - Combining semantic vector search with keyword (BM25) search
//! - Using Reciprocal Rank Fusion to merge results
//! - Configuring search weights
//!
//! Run with: cargo run --example hybrid_search

use oxify_connect_vector::{
    Bm25Document, HybridSearchEngine, HybridSearchParams, InsertRequest, MockVectorProvider,
    SearchRequest, VectorProvider,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Hybrid Search Example\n");

    // 1. Create a mock vector provider (for demonstration)
    println!("📡 Setting up vector store...");
    let vector_provider = MockVectorProvider::new();
    let collection = "articles";

    // Create collection
    vector_provider.create_collection(collection, 128).await?;
    println!("✅ Vector store ready!\n");

    // 2. Prepare sample articles
    println!("📥 Adding sample articles...");

    let articles = vec![
        (
            "art_001",
            "Introduction to Rust Programming",
            "Rust is a systems programming language that runs blazingly fast...",
            vec![0.1; 128],
        ),
        (
            "art_002",
            "Python for Data Science",
            "Python has become the dominant language for data science and machine learning...",
            vec![0.2; 128],
        ),
        (
            "art_003",
            "Rust vs C++: Performance Comparison",
            "Comparing Rust and C++ for high-performance applications...",
            vec![0.15; 128],
        ),
        (
            "art_004",
            "Machine Learning with Python",
            "Learn how to build machine learning models using Python and scikit-learn...",
            vec![0.25; 128],
        ),
    ];

    // Insert into vector store
    for (id, title, content, vector) in &articles {
        vector_provider
            .insert(InsertRequest {
                collection: collection.to_string(),
                id: id.to_string(),
                vector: vector.clone(),
                payload: json!({
                    "title": title,
                    "content": content
                }),
            })
            .await?;
    }

    // 3. Create BM25 index for keyword search
    let bm25_docs: Vec<Bm25Document> = articles
        .iter()
        .map(|(id, title, content, _)| Bm25Document {
            id: id.to_string(),
            text: format!("{} {}", title, content),
            metadata: json!({
                "title": title,
                "content": content
            }),
        })
        .collect();

    println!("✅ Indexed {} articles\n", articles.len());

    // 4. Create hybrid search engine
    println!("🔧 Creating hybrid search engine...");
    let hybrid_engine = HybridSearchEngine::new(
        vector_provider,
        bm25_docs,
        HybridSearchParams {
            semantic_weight: 0.7, // 70% semantic, 30% keyword
            keyword_weight: 0.3,
            rrf_k: 60.0,
        },
    );
    println!("✅ Hybrid engine ready!\n");

    // 5. Perform hybrid search
    println!("🔎 Searching for: 'Rust programming performance'\n");

    let query_vector = vec![0.12; 128]; // Semantic query embedding
    let query_text = "Rust programming performance";

    let search_request = SearchRequest {
        collection: collection.to_string(),
        query: query_vector,
        top_k: 3,
        score_threshold: None,
        filter: None,
    };

    let results = hybrid_engine.search(search_request, query_text, 3).await?;

    println!("✅ Top {} results:\n", results.len());

    for (i, result) in results.iter().enumerate() {
        println!("  {}. {} (score: {:.4})", i + 1, result.id, result.score);
        println!("     Title: {}", result.payload["title"]);
        println!(
            "     Snippet: {}...\n",
            result.payload["content"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>()
        );
    }

    println!("💡 Note: Hybrid search combines:");
    println!("   • Semantic similarity (70%) - understands meaning");
    println!("   • Keyword matching (30%) - finds exact terms");
    println!("   • Result: Better relevance than either alone!\n");

    println!("✨ Example complete!");

    Ok(())
}
