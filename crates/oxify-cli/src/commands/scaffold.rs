//! Workflow scaffolding commands
//!
//! Generate workflow templates with interactive configuration.

use anyhow::{Context, Result};
use oxify_model::{Edge, LlmConfig, Node, NodeKind, Workflow, WorkflowMetadata};
use std::fs;
use std::io::{self, Write};

/// Scaffold a new workflow with interactive prompts
pub async fn scaffold_interactive(output: Option<String>) -> Result<()> {
    println!("OxiFY Workflow Scaffolding");
    println!("==========================\n");

    // Get workflow name
    let name = prompt("Workflow name")?;
    let description = prompt_optional("Description")?;

    // Get workflow type
    println!("\nSelect workflow type:");
    println!("1. Simple LLM call");
    println!("2. RAG (Retrieval-Augmented Generation)");
    println!("3. Agent with tools");
    println!("4. Multi-step chain");
    println!("5. Conditional workflow");
    println!("6. Loop/batch processing");

    let workflow_type = prompt("Type (1-6)")?;

    let mut workflow = WorkflowMetadata::new(name);
    if let Some(desc) = description {
        workflow.description = Some(desc);
    }

    let mut wf = Workflow {
        metadata: workflow,
        nodes: vec![],
        edges: vec![],
    };

    match workflow_type.as_str() {
        "1" => scaffold_simple_llm(&mut wf)?,
        "2" => scaffold_rag(&mut wf)?,
        "3" => scaffold_agent(&mut wf)?,
        "4" => scaffold_chain(&mut wf)?,
        "5" => scaffold_conditional(&mut wf)?,
        "6" => scaffold_loop(&mut wf)?,
        _ => anyhow::bail!("Invalid workflow type"),
    }

    // Determine output path
    let output_path = if let Some(path) = output {
        path
    } else {
        let suggested = format!("{}.json", wf.metadata.name.to_lowercase().replace(' ', "_"));
        let path = prompt_with_default("Output file", &suggested)?;
        if path.is_empty() {
            suggested
        } else {
            path
        }
    };

    // Serialize and save
    let json = serde_json::to_string_pretty(&wf)?;
    fs::write(&output_path, json)
        .with_context(|| format!("Failed to write workflow to: {}", output_path))?;

    println!("\n✓ Workflow scaffolded successfully");
    println!("  File: {}", output_path);
    println!("  Nodes: {}", wf.nodes.len());
    println!("  Edges: {}", wf.edges.len());
    println!("\nNext steps:");
    println!("  1. Edit the workflow file to customize prompts and configuration");
    println!("  2. Validate: oxify workflow validate {}", output_path);
    println!("  3. Run: oxify run {}", output_path);

    Ok(())
}

fn scaffold_simple_llm(workflow: &mut Workflow) -> Result<()> {
    let provider = prompt_with_default("LLM provider (openai/anthropic/ollama)", "openai")?;
    let model = match provider.as_str() {
        "openai" => prompt_with_default("Model", "gpt-4")?,
        "anthropic" => prompt_with_default("Model", "claude-3-5-sonnet-20241022")?,
        "ollama" => prompt_with_default("Model", "llama3.1")?,
        _ => prompt("Model")?,
    };

    let start = Node::new("Start".to_string(), NodeKind::Start);
    let llm = Node::new(
        "LLM Call".to_string(),
        NodeKind::LLM(LlmConfig {
            provider,
            model,
            system_prompt: Some("You are a helpful assistant.".to_string()),
            prompt_template: "{{user_input}}".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    );
    let end = Node::new("End".to_string(), NodeKind::End);

    workflow.edges.push(Edge::new(start.id, llm.id));
    workflow.edges.push(Edge::new(llm.id, end.id));

    workflow.nodes.push(start);
    workflow.nodes.push(llm);
    workflow.nodes.push(end);

    Ok(())
}

fn scaffold_rag(workflow: &mut Workflow) -> Result<()> {
    println!("\nRAG Workflow Configuration:");

    let provider = prompt_with_default("LLM provider (openai/anthropic/ollama)", "openai")?;
    let model = match provider.as_str() {
        "openai" => prompt_with_default("Model", "gpt-4")?,
        "anthropic" => prompt_with_default("Model", "claude-3-5-sonnet-20241022")?,
        "ollama" => prompt_with_default("Model", "llama3.1")?,
        _ => prompt("Model")?,
    };

    let vector_db = prompt_with_default("Vector DB (qdrant/pgvector)", "qdrant")?;

    let start = Node::new("Start".to_string(), NodeKind::Start);

    let retrieval = Node::new(
        "Retrieve Context".to_string(),
        NodeKind::Retriever(oxify_model::VectorConfig {
            db_type: vector_db,
            collection: "documents".to_string(),
            query: "{{user_query}}".to_string(),
            top_k: 5,
            score_threshold: Some(0.7),
        }),
    );

    let llm = Node::new(
        "Generate Answer".to_string(),
        NodeKind::LLM(LlmConfig {
            provider,
            model,
            system_prompt: Some("Answer the question using the provided context.".to_string()),
            prompt_template: "Context: {{retrieval.results}}\n\nQuestion: {{user_query}}"
                .to_string(),
            temperature: Some(0.3),
            max_tokens: Some(1500),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    );

    let end = Node::new("End".to_string(), NodeKind::End);

    workflow.edges.push(Edge::new(start.id, retrieval.id));
    workflow.edges.push(Edge::new(retrieval.id, llm.id));
    workflow.edges.push(Edge::new(llm.id, end.id));

    workflow.nodes.push(start);
    workflow.nodes.push(retrieval);
    workflow.nodes.push(llm);
    workflow.nodes.push(end);

    Ok(())
}

fn scaffold_agent(workflow: &mut Workflow) -> Result<()> {
    println!("\nAgent Workflow (with tool use):");
    let provider = prompt_with_default("LLM provider", "openai")?;
    let model = prompt_with_default("Model", "gpt-4")?;

    let start = Node::new("Start".to_string(), NodeKind::Start);

    let agent = Node::new(
        "Agent".to_string(),
        NodeKind::LLM(LlmConfig {
            provider,
            model,
            system_prompt: Some(
                "You are an AI agent with access to tools. Use them to help the user.".to_string(),
            ),
            prompt_template: "{{user_request}}".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(2000),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    );

    let tool = Node::new(
        "Tool Call".to_string(),
        NodeKind::Tool(oxify_model::McpConfig {
            server_id: "example_tool".to_string(),
            tool_name: "search".to_string(),
            parameters: serde_json::json!({}),
        }),
    );

    let end = Node::new("End".to_string(), NodeKind::End);

    workflow.edges.push(Edge::new(start.id, agent.id));
    workflow.edges.push(Edge::new(agent.id, tool.id));
    workflow.edges.push(Edge::new(tool.id, end.id));

    workflow.nodes.push(start);
    workflow.nodes.push(agent);
    workflow.nodes.push(tool);
    workflow.nodes.push(end);

    Ok(())
}

fn scaffold_chain(workflow: &mut Workflow) -> Result<()> {
    let num_steps = prompt_with_default("Number of steps", "3")?
        .parse::<usize>()
        .unwrap_or(3);

    let provider = prompt_with_default("LLM provider", "openai")?;
    let model = prompt_with_default("Model", "gpt-4")?;

    let start = Node::new("Start".to_string(), NodeKind::Start);
    workflow.nodes.push(start.clone());

    let mut prev_id = start.id;

    for i in 1..=num_steps {
        let step = Node::new(
            format!("Step {}", i),
            NodeKind::LLM(LlmConfig {
                provider: provider.clone(),
                model: model.clone(),
                system_prompt: Some(format!("Step {} of the workflow", i)),
                prompt_template: if i == 1 {
                    "{{user_input}}".to_string()
                } else {
                    format!("{{{{step_{}.result}}}}", i - 1)
                },
                temperature: Some(0.7),
                max_tokens: Some(1000),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );

        workflow.edges.push(Edge::new(prev_id, step.id));
        prev_id = step.id;
        workflow.nodes.push(step);
    }

    let end = Node::new("End".to_string(), NodeKind::End);
    workflow.edges.push(Edge::new(prev_id, end.id));
    workflow.nodes.push(end);

    Ok(())
}

fn scaffold_conditional(workflow: &mut Workflow) -> Result<()> {
    let start = Node::new("Start".to_string(), NodeKind::Start);
    let provider = prompt_with_default("LLM provider", "openai")?;
    let model = prompt_with_default("Model", "gpt-4")?;

    let analyzer = Node::new(
        "Analyze Input".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: provider.clone(),
            model: model.clone(),
            system_prompt: Some("Analyze the input and classify it.".to_string()),
            prompt_template: "{{user_input}}".to_string(),
            temperature: Some(0.3),
            max_tokens: Some(500),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    );

    let path_a = Node::new(
        "Path A".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: provider.clone(),
            model: model.clone(),
            system_prompt: Some("Handle Path A".to_string()),
            prompt_template: "{{analyzer.result}}".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    );

    let path_b = Node::new(
        "Path B".to_string(),
        NodeKind::LLM(LlmConfig {
            provider,
            model,
            system_prompt: Some("Handle Path B".to_string()),
            prompt_template: "{{analyzer.result}}".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    );

    let conditional = Node::new(
        "Route".to_string(),
        NodeKind::IfElse(oxify_model::Condition {
            expression: "analyzer.result.contains('A')".to_string(),
            true_branch: path_a.id,
            false_branch: path_b.id,
        }),
    );

    let end = Node::new("End".to_string(), NodeKind::End);

    workflow.edges.push(Edge::new(start.id, analyzer.id));
    workflow.edges.push(Edge::new(analyzer.id, conditional.id));
    workflow.edges.push(Edge::new(path_a.id, end.id));
    workflow.edges.push(Edge::new(path_b.id, end.id));

    workflow.nodes.push(start);
    workflow.nodes.push(analyzer);
    workflow.nodes.push(conditional);
    workflow.nodes.push(path_a);
    workflow.nodes.push(path_b);
    workflow.nodes.push(end);

    Ok(())
}

fn scaffold_loop(workflow: &mut Workflow) -> Result<()> {
    let start = Node::new("Start".to_string(), NodeKind::Start);

    let loop_node = Node::new(
        "Process Items".to_string(),
        NodeKind::Loop(oxify_model::LoopConfig {
            loop_type: oxify_model::LoopType::ForEach {
                collection_path: "items".to_string(),
                item_variable: "item".to_string(),
                index_variable: Some("index".to_string()),
                body_expression: "Process item: {{item}}".to_string(),
                parallel: false,
                max_concurrency: None,
            },
            max_iterations: 100,
        }),
    );

    let end = Node::new("End".to_string(), NodeKind::End);

    workflow.edges.push(Edge::new(start.id, loop_node.id));
    workflow.edges.push(Edge::new(loop_node.id, end.id));

    workflow.nodes.push(start);
    workflow.nodes.push(loop_node);
    workflow.nodes.push(end);

    Ok(())
}

fn prompt(label: &str) -> Result<String> {
    print!("{}: ", label);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_string())
}

fn prompt_optional(label: &str) -> Result<Option<String>> {
    let value = prompt(label)?;
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn prompt_with_default(label: &str, default: &str) -> Result<String> {
    print!("{} [{}]: ", label, default);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}
