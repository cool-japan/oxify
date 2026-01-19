//! Simple OxiFY workflow example
//!
//! This example demonstrates:
//! - Creating a simple workflow with LLM nodes
//! - Executing the workflow using the engine
//! - Inspecting execution results
//!
//! Run this example:
//! ```bash
//! cargo run --example simple_workflow
//! ```

use oxify_engine::Engine;
use oxify_model::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== OxiFY Simple Workflow Example ===\n");

    // Create a simple workflow
    let mut workflow = create_simple_chatbot();

    // Validate workflow
    println!("Validating workflow...");
    workflow.validate()?;
    println!("✓ Workflow is valid\n");

    // Display workflow structure
    print_workflow_structure(&workflow);

    // Execute workflow
    println!("\nExecuting workflow...");
    let engine = Engine::new();
    let mut context = engine.execute(&workflow).await?;

    // Set some initial variables
    context.variables.insert(
        "user_input".to_string(),
        serde_json::json!("What is Rust programming language?"),
    );

    println!("✓ Workflow execution completed\n");

    // Display results
    print_execution_results(&context);

    // Export workflow to JSON
    let workflow_json = serde_json::to_string_pretty(&workflow)?;
    println!("\n=== Workflow JSON ===");
    println!("{}", workflow_json);

    Ok(())
}

fn create_simple_chatbot() -> Workflow {
    let mut workflow = Workflow::new("Simple Chatbot".to_string());
    workflow.metadata.description = Some("A simple chatbot that answers questions using GPT-4".to_string());
    workflow.metadata.tags = vec!["chatbot".to_string(), "llm".to_string()];

    // Create nodes
    let start = Node::new("Start".to_string(), NodeKind::Start);
    let start_id = start.id;

    let llm = Node::new(
        "GPT-4 Chat".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some(
                "You are a helpful AI assistant that provides clear and concise answers.".to_string(),
            ),
            prompt_template: "{{user_input}}".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(500),
            extra_params: serde_json::Value::Null,
        }),
    ).with_position(200.0, 100.0);
    let llm_id = llm.id;

    let end = Node::new("End".to_string(), NodeKind::End);
    let end_id = end.id;

    // Add nodes to workflow
    workflow.add_node(start);
    workflow.add_node(llm);
    workflow.add_node(end);

    // Connect nodes
    workflow.add_edge(Edge::new(start_id, llm_id).with_label("input".to_string()));
    workflow.add_edge(Edge::new(llm_id, end_id).with_label("response".to_string()));

    workflow
}

fn print_workflow_structure(workflow: &Workflow) {
    println!("=== Workflow Structure ===");
    println!("Name: {}", workflow.metadata.name);
    if let Some(desc) = &workflow.metadata.description {
        println!("Description: {}", desc);
    }
    println!("Version: {}", workflow.metadata.version);
    println!("Tags: {:?}", workflow.metadata.tags);
    println!("\nNodes ({}):", workflow.nodes.len());
    for node in &workflow.nodes {
        println!("  - {} ({}): {:?}", node.name, node.id, match &node.kind {
            NodeKind::Start => "Start".to_string(),
            NodeKind::End => "End".to_string(),
            NodeKind::LLM(config) => format!("LLM ({}/{})", config.provider, config.model),
            NodeKind::Retriever(config) => format!("Retriever ({})", config.db_type),
            NodeKind::Code(config) => format!("Code ({})", config.runtime),
            NodeKind::IfElse(_) => "IfElse".to_string(),
            NodeKind::Tool(config) => format!("Tool ({})", config.tool_name),
        });
    }
    println!("\nEdges ({}):", workflow.edges.len());
    for edge in &workflow.edges {
        let label = edge.label.as_deref().unwrap_or("");
        println!("  - {} → {} ({})", edge.from, edge.to, label);
    }
}

fn print_execution_results(context: &ExecutionContext) {
    println!("=== Execution Results ===");
    println!("Execution ID: {}", context.execution_id);
    println!("Workflow ID: {}", context.workflow_id);
    println!("State: {:?}", context.state);
    println!("Started at: {}", context.started_at);
    println!("\nNode Results ({}):", context.node_results.len());
    for (node_id, result) in &context.node_results {
        println!("\nNode: {}", node_id);
        println!("  Started: {}", result.started_at);
        if let Some(completed) = result.completed_at {
            println!("  Completed: {}", completed);
            let duration = completed - result.started_at;
            println!("  Duration: {}ms", duration.num_milliseconds());
        }
        println!("  Result: {:?}", result.result);
    }
}
