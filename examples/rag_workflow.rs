//! RAG (Retrieval Augmented Generation) workflow example
//!
//! This example demonstrates:
//! - Vector database retrieval
//! - LLM integration with retrieved context
//! - Multi-node workflows
//!
//! Run this example:
//! ```bash
//! cargo run --example rag_workflow
//! ```

use oxify_engine::Engine;
use oxify_model::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== OxiFY RAG Workflow Example ===\n");

    // Create RAG workflow
    let workflow = create_rag_workflow();

    // Validate
    println!("Validating workflow...");
    workflow.validate()?;
    println!("✓ Workflow is valid\n");

    // Display structure
    print_workflow_info(&workflow);

    // Execute
    println!("\nExecuting workflow...");
    let engine = Engine::new();
    let mut context = engine.execute(&workflow).await?;

    // Add input variable
    context.variables.insert(
        "query".to_string(),
        serde_json::json!("How do I use async/await in Rust?"),
    );

    println!("✓ Execution completed");
    println!("State: {:?}", context.state);
    println!("Nodes executed: {}", context.node_results.len());

    Ok(())
}

fn create_rag_workflow() -> Workflow {
    let mut workflow = Workflow::new("RAG Pipeline".to_string());
    workflow.metadata.description = Some(
        "Retrieval-Augmented Generation workflow for answering questions with context".to_string(),
    );
    workflow.metadata.tags = vec!["rag".to_string(), "qa".to_string(), "search".to_string()];

    // Nodes
    let start = Node::new("Start".to_string(), NodeKind::Start)
        .with_position(100.0, 200.0);
    let start_id = start.id;

    // Vector search node
    let retriever = Node::new(
        "Search Documents".to_string(),
        NodeKind::Retriever(VectorConfig {
            db_type: "qdrant".to_string(),
            collection: "rust_docs".to_string(),
            query: "{{query}}".to_string(),
            top_k: 5,
            score_threshold: Some(0.7),
        }),
    ).with_position(100.0, 300.0);
    let retriever_id = retriever.id;

    // LLM synthesis node
    let llm = Node::new(
        "Generate Answer".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some(
                "You are a Rust programming expert. Answer the question using ONLY the provided context. \
                 If the context doesn't contain the answer, say so.".to_string(),
            ),
            prompt_template: "Context:\n{{retriever.results}}\n\nQuestion: {{query}}\n\nAnswer:".to_string(),
            temperature: Some(0.3),
            max_tokens: Some(1000),
            extra_params: serde_json::Value::Null,
        }),
    ).with_position(100.0, 400.0);
    let llm_id = llm.id;

    let end = Node::new("End".to_string(), NodeKind::End)
        .with_position(100.0, 500.0);
    let end_id = end.id;

    // Add all nodes
    workflow.add_node(start);
    workflow.add_node(retriever);
    workflow.add_node(llm);
    workflow.add_node(end);

    // Connect nodes
    workflow.add_edge(Edge::new(start_id, retriever_id).with_label("query".to_string()));
    workflow.add_edge(Edge::new(retriever_id, llm_id).with_label("context".to_string()));
    workflow.add_edge(Edge::new(llm_id, end_id).with_label("answer".to_string()));

    workflow
}

fn print_workflow_info(workflow: &Workflow) {
    println!("Workflow: {}", workflow.metadata.name);
    println!("Description: {}", workflow.metadata.description.as_deref().unwrap_or("N/A"));
    println!("\nExecution Flow:");
    println!("  1. Start");
    println!("  2. Search Documents (Vector DB)");
    println!("  3. Generate Answer (LLM)");
    println!("  4. End");
    println!("\nTotal nodes: {}", workflow.nodes.len());
    println!("Total edges: {}", workflow.edges.len());
}
