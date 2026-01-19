use anyhow::{Context, Result};
use oxify_engine::Engine;
use oxify_model::{ExecutionContext, Workflow};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub async fn handle_run_command(
    workflow_file: String,
    vars: Vec<String>,
    output: Option<String>,
) -> Result<()> {
    let path = Path::new(&workflow_file);
    if !path.exists() {
        anyhow::bail!("Workflow file not found: {}", workflow_file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

    // Parse workflow based on file extension
    let workflow: Workflow = match path.extension().and_then(|s| s.to_str()) {
        Some("json") => {
            serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
        }
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
        }
        _ => {
            // Try JSON first, then YAML as fallback
            serde_json::from_str(&content)
                .or_else(|_| serde_yaml::from_str(&content))
                .with_context(|| "Failed to parse workflow (tried both JSON and YAML)")?
        }
    };

    workflow
        .validate()
        .map_err(|error| anyhow::anyhow!("Workflow validation failed: {}", error))?;

    // Parse variables from command line
    let mut initial_vars = serde_json::Map::new();
    for var in vars {
        let parts: Vec<&str> = var.splitn(2, '=').collect();
        if parts.len() == 2 {
            let key = parts[0].to_string();
            let value: Value = serde_json::from_str(parts[1])
                .unwrap_or_else(|_| Value::String(parts[1].to_string()));
            initial_vars.insert(key, value);
        }
    }

    println!("Executing workflow: {}", workflow.metadata.name);
    println!("Nodes: {}", workflow.nodes.len());
    println!("Edges: {}", workflow.edges.len());
    if !initial_vars.is_empty() {
        println!("Variables: {:?}", initial_vars);
    }

    let engine = Engine::new();

    // Create execution context with initial variables
    let mut ctx = ExecutionContext::new(workflow.metadata.id);
    for (key, value) in initial_vars {
        ctx.set_variable(key, value);
    }

    let execution_result = engine.execute(&workflow).await?;

    println!("\n✓ Workflow execution completed");
    println!("  State: {:?}", execution_result.state);

    if let Some(completed) = execution_result.completed_at {
        let duration = completed.signed_duration_since(execution_result.started_at);
        println!("  Duration: {:?}", duration);
    }

    if let Some(output_path) = output {
        let output_json = serde_json::to_string_pretty(&execution_result)?;
        fs::write(&output_path, output_json)
            .with_context(|| format!("Failed to write output to: {}", output_path))?;
        println!("  Output saved to: {}", output_path);
    } else {
        println!("\nResults:");
        for (node_id, result) in &execution_result.node_results {
            let result_str = match &result.result {
                oxify_model::ExecutionResult::Success(value) => {
                    format!(
                        "Success: {}",
                        serde_json::to_string(value).unwrap_or_default()
                    )
                }
                oxify_model::ExecutionResult::Failure(err) => {
                    format!("Failure: {}", err)
                }
                oxify_model::ExecutionResult::Pending => "Pending".to_string(),
                oxify_model::ExecutionResult::Skipped => "Skipped".to_string(),
            };
            println!("  {}: {}", node_id, result_str);
        }
    }

    Ok(())
}
