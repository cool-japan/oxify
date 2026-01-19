use anyhow::Result;
use clap::Subcommand;
use oxify_model::{LlmConfig, McpConfig, ScriptConfig, SubWorkflowConfig, VectorConfig};
use serde_json::json;
use std::collections::HashMap;

#[derive(Subcommand)]
pub enum NodeCommands {
    /// List all available node types
    List,
    /// Show schema for a specific node type
    Schema {
        /// Node type (LLM, Retriever, Code, etc.)
        node_type: String,
    },
    /// Show example configuration for a node type
    Example {
        /// Node type (LLM, Retriever, Code, etc.)
        node_type: String,
        /// Output format (json or yaml)
        #[arg(short, long, default_value = "json")]
        format: String,
    },
}

pub async fn handle_nodes_command(command: NodeCommands) -> Result<()> {
    match command {
        NodeCommands::List => list_node_types().await,
        NodeCommands::Schema { node_type } => show_node_schema(&node_type).await,
        NodeCommands::Example { node_type, format } => show_node_example(&node_type, &format).await,
    }
}

async fn list_node_types() -> Result<()> {
    println!("Available Node Types");
    println!("====================\n");

    println!("Core Nodes:");
    println!("  Start            - Workflow entry point");
    println!("  End              - Workflow exit point\n");

    println!("Compute Nodes:");
    println!("  LLM              - Large Language Model inference");
    println!("  Retriever        - Vector database search");
    println!("  Code             - Execute code (Python, JavaScript, Rhai)");
    println!("  Tool             - Call external MCP tools\n");

    println!("Control Flow:");
    println!("  IfElse           - Conditional branching");
    println!("  Switch           - Multi-way branching");
    println!("  Loop             - Iteration (ForEach, While)");
    println!("  TryCatch         - Error handling");
    println!("  Parallel         - Concurrent execution\n");

    println!("Advanced:");
    println!("  SubWorkflow      - Nested workflow execution");
    println!("  Approval         - Human-in-the-loop approval");
    println!("  Form             - Collect user input\n");

    println!("Use 'oxify nodes schema <type>' to see configuration options");
    println!("Use 'oxify nodes example <type>' to see example configuration");

    Ok(())
}

async fn show_node_schema(node_type: &str) -> Result<()> {
    match node_type.to_lowercase().as_str() {
        "llm" => show_llm_schema(),
        "retriever" | "vector" => show_retriever_schema(),
        "code" => show_code_schema(),
        "tool" | "mcp" => show_tool_schema(),
        "ifelse" | "if" => show_ifelse_schema(),
        "switch" => show_switch_schema(),
        "loop" => show_loop_schema(),
        "trycatch" | "try" => show_trycatch_schema(),
        "parallel" => show_parallel_schema(),
        "subworkflow" | "sub" => show_subworkflow_schema(),
        "approval" => show_approval_schema(),
        "form" => show_form_schema(),
        "start" | "end" => {
            println!("{} node requires no configuration", node_type);
            Ok(())
        }
        _ => {
            anyhow::bail!(
                "Unknown node type: {}. Run 'oxify nodes list' to see available types.",
                node_type
            );
        }
    }
}

#[allow(dead_code)]
fn show_llm_schema() -> Result<()> {
    println!("LLM Node Configuration");
    println!("======================\n");

    println!("Required Fields:");
    println!("  provider          : string  - LLM provider (openai, anthropic, ollama, etc.)");
    println!("  model             : string  - Model identifier (gpt-4, claude-3-opus, etc.)");
    println!("  prompt_template   : string  - Prompt with variable interpolation {{var}}\n");

    println!("Optional Fields:");
    println!("  system_prompt     : string  - System/assistant instructions");
    println!("  temperature       : f64     - Sampling temperature (0.0 - 2.0)");
    println!("  max_tokens        : u32     - Maximum tokens to generate");
    println!("  tools             : array   - Function calling tools");
    println!("  images            : array   - Images for multimodal models");
    println!("  extra_params      : object  - Provider-specific parameters\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "provider": "openai",
    "model": "gpt-4",
    "system_prompt": "You are a helpful assistant.",
    "prompt_template": "Answer: {{{{query}}}}",
    "temperature": 0.7,
    "max_tokens": 500
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_retriever_schema() -> Result<()> {
    println!("Retriever Node Configuration");
    println!("=============================\n");

    println!("Required Fields:");
    println!("  db_type           : string  - Vector database type (qdrant, pgvector, etc.)");
    println!("  collection        : string  - Collection/table name");
    println!("  query             : string  - Query text with variable interpolation");
    println!("  top_k             : usize   - Number of results to retrieve\n");

    println!("Optional Fields:");
    println!("  score_threshold   : f64     - Minimum similarity score (0.0 - 1.0)\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "db_type": "qdrant",
    "collection": "knowledge_base",
    "query": "{{{{user_query}}}}",
    "top_k": 5,
    "score_threshold": 0.7
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_code_schema() -> Result<()> {
    println!("Code Node Configuration");
    println!("=======================\n");

    println!("Required Fields:");
    println!("  runtime           : string  - Script runtime (rust, wasm)");
    println!("  code              : string  - Code to execute");
    println!("  output            : string  - Output variable name\n");

    println!("Optional Fields:");
    println!("  inputs            : array   - Input variable names\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "runtime": "rust",
    "code": "let result = input_text.len();",
    "inputs": ["input_text"],
    "output": "result"
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_tool_schema() -> Result<()> {
    println!("Tool (MCP) Node Configuration");
    println!("==============================\n");

    println!("Required Fields:");
    println!("  server_id         : string  - MCP server identifier");
    println!("  tool_name         : string  - Tool/function name");
    println!("  parameters        : object  - Tool parameters\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "server_id": "filesystem",
    "tool_name": "read_file",
    "parameters": {{
      "path": "{{{{file_path}}}}"
    }}
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_ifelse_schema() -> Result<()> {
    println!("IfElse Node Configuration");
    println!("==========================\n");

    println!("Required Fields:");
    println!("  expression        : string  - Boolean condition to evaluate");
    println!("  true_branch       : uuid    - Node ID to execute if true");
    println!("  false_branch      : uuid    - Node ID to execute if false\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "expression": "result.score > 0.8",
    "true_branch": "node-id-1",
    "false_branch": "node-id-2"
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_switch_schema() -> Result<()> {
    println!("Switch Node Configuration");
    println!("==========================\n");

    println!("Required Fields:");
    println!("  switch_on         : string  - Value to match against");
    println!("  cases             : array   - List of case conditions\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "switch_on": "{{{{status}}}}",
    "cases": [
      {{"match_value": "success", "action": "process_success"}},
      {{"match_value": "error", "action": "handle_error"}}
    ]
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_loop_schema() -> Result<()> {
    println!("Loop Node Configuration");
    println!("========================\n");

    println!("Loop Types:");
    println!("  ForEach           - Iterate over a collection");
    println!("  While             - Loop while condition is true\n");

    println!("ForEach Example:");
    println!(
        r#"  "config": {{
    "loop_type": {{
      "ForEach": {{
        "collection_path": "items",
        "item_variable": "item",
        "index_variable": "index",
        "body_expression": "process {{{{item}}}}"
      }}
    }},
    "max_iterations": 100
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_trycatch_schema() -> Result<()> {
    println!("TryCatch Node Configuration");
    println!("============================\n");

    println!("Required Fields:");
    println!("  try_expression    : string  - Expression to try");
    println!("  error_variable    : string  - Variable name for error\n");

    println!("Optional Fields:");
    println!("  catch_expression  : string  - Expression to run on error");
    println!("  finally_expression: string  - Expression to always run");
    println!("  rethrow           : bool    - Re-throw error after catch\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "try_expression": "risky_operation()",
    "catch_expression": "handle_error()",
    "finally_expression": "cleanup()",
    "error_variable": "error",
    "rethrow": false
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_parallel_schema() -> Result<()> {
    println!("Parallel Node Configuration");
    println!("============================\n");

    println!("Required Fields:");
    println!("  branches          : array   - List of branch configurations\n");

    println!("Optional Fields:");
    println!("  wait_for_all      : bool    - Wait for all branches (default: true)");
    println!("  merge_strategy    : string  - How to merge results\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "branches": [
      {{"branch_id": "branch-1"}},
      {{"branch_id": "branch-2"}}
    ],
    "wait_for_all": true
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_subworkflow_schema() -> Result<()> {
    println!("SubWorkflow Node Configuration");
    println!("===============================\n");

    println!("Required Fields:");
    println!("  workflow_path     : string  - Path to sub-workflow file\n");

    println!("Optional Fields:");
    println!("  input_mappings    : object  - Map parent variables to sub-workflow");
    println!("  output_variable   : string  - Output variable name from sub-workflow");
    println!("  inherit_context   : bool    - Inherit parent context (default: false)\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "workflow_path": "./sub_workflows/process.json",
    "input_mappings": {{
      "data": "{{{{parent_data}}}}"
    }},
    "output_variable": "sub_result"
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_approval_schema() -> Result<()> {
    println!("Approval Node Configuration");
    println!("============================\n");

    println!("Required Fields:");
    println!("  message           : string  - Approval request message");
    println!("  approvers         : array   - List of approver IDs\n");

    println!("Optional Fields:");
    println!("  timeout_seconds   : u64     - Approval timeout");
    println!("  require_all       : bool    - Require all approvers (default: false)\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "message": "Please approve this action",
    "approvers": ["user-1", "user-2"],
    "timeout_seconds": 3600
  }}"#
    );

    Ok(())
}

#[allow(dead_code)]
fn show_form_schema() -> Result<()> {
    println!("Form Node Configuration");
    println!("========================\n");

    println!("Required Fields:");
    println!("  fields            : array   - Form field definitions\n");

    println!("Example:");
    println!(
        r#"  "config": {{
    "fields": [
      {{
        "name": "email",
        "type": "text",
        "label": "Email Address",
        "required": true
      }}
    ]
  }}"#
    );

    Ok(())
}

async fn show_node_example(node_type: &str, format: &str) -> Result<()> {
    let example = match node_type.to_lowercase().as_str() {
        "llm" => create_llm_example(),
        "retriever" | "vector" => create_retriever_example(),
        "code" => create_code_example(),
        "tool" | "mcp" => create_tool_example(),
        "subworkflow" | "sub" => create_subworkflow_example(),
        _ => {
            anyhow::bail!(
                "No example available for node type: {}. Try 'oxify nodes schema {}'",
                node_type,
                node_type
            );
        }
    };

    match format {
        "yaml" => {
            let yaml = serde_yaml::to_string(&example)?;
            println!("{}", yaml);
        }
        _ => {
            // Default to JSON format
            let json = serde_json::to_string_pretty(&example)?;
            println!("{}", json);
        }
    }

    Ok(())
}

fn create_llm_example() -> serde_json::Value {
    let config = LlmConfig {
        provider: "openai".to_string(),
        model: "gpt-4".to_string(),
        system_prompt: Some("You are a helpful assistant.".to_string()),
        prompt_template: "Answer the following question: {{query}}".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(500),
        tools: vec![],
        images: vec![],
        extra_params: serde_json::Value::Null,
    };

    json!({
        "id": "llm-node-1",
        "name": "Generate Answer",
        "kind": {
            "LLM": config
        }
    })
}

fn create_retriever_example() -> serde_json::Value {
    let config = VectorConfig {
        db_type: "qdrant".to_string(),
        collection: "knowledge_base".to_string(),
        query: "{{user_query}}".to_string(),
        top_k: 5,
        score_threshold: Some(0.7),
    };

    json!({
        "id": "retriever-node-1",
        "name": "Search Knowledge Base",
        "kind": {
            "Retriever": config
        }
    })
}

fn create_code_example() -> serde_json::Value {
    let config = ScriptConfig {
        runtime: "rust".to_string(),
        code: "let word_count = input_text.split_whitespace().count();".to_string(),
        inputs: vec!["input_text".to_string()],
        output: "word_count".to_string(),
    };

    json!({
        "id": "code-node-1",
        "name": "Process Data",
        "kind": {
            "Code": config
        }
    })
}

fn create_tool_example() -> serde_json::Value {
    let config = McpConfig {
        server_id: "filesystem".to_string(),
        tool_name: "read_file".to_string(),
        parameters: json!({"path": "{{file_path}}"}),
    };

    json!({
        "id": "tool-node-1",
        "name": "Read File",
        "kind": {
            "Tool": config
        }
    })
}

fn create_subworkflow_example() -> serde_json::Value {
    let mut input_mappings = HashMap::new();
    input_mappings.insert("data".to_string(), "{{parent_data}}".to_string());

    let config = SubWorkflowConfig {
        workflow_path: "./sub_workflows/process.json".to_string(),
        input_mappings,
        output_variable: Some("sub_result".to_string()),
        inherit_context: false,
    };

    json!({
        "id": "subworkflow-node-1",
        "name": "Run Sub Process",
        "kind": {
            "SubWorkflow": config
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_llm_example() {
        let example = create_llm_example();
        assert!(example.get("id").is_some());
        assert!(example.get("name").is_some());
        assert!(example.get("kind").is_some());
        assert!(example["kind"].get("LLM").is_some());
    }

    #[test]
    fn test_create_retriever_example() {
        let example = create_retriever_example();
        assert!(example.get("id").is_some());
        assert!(example["kind"].get("Retriever").is_some());
        let retriever = &example["kind"]["Retriever"];
        assert_eq!(retriever["db_type"], "qdrant");
        assert_eq!(retriever["top_k"], 5);
    }

    #[test]
    fn test_create_code_example() {
        let example = create_code_example();
        assert!(example["kind"].get("Code").is_some());
        let code = &example["kind"]["Code"];
        assert_eq!(code["runtime"], "rust");
        assert!(code["inputs"].is_array());
    }

    #[test]
    fn test_create_tool_example() {
        let example = create_tool_example();
        assert!(example["kind"].get("Tool").is_some());
        let tool = &example["kind"]["Tool"];
        assert_eq!(tool["server_id"], "filesystem");
        assert_eq!(tool["tool_name"], "read_file");
    }

    #[test]
    fn test_create_subworkflow_example() {
        let example = create_subworkflow_example();
        assert!(example["kind"].get("SubWorkflow").is_some());
        let subworkflow = &example["kind"]["SubWorkflow"];
        assert_eq!(subworkflow["workflow_path"], "./sub_workflows/process.json");
        assert_eq!(subworkflow["inherit_context"], false);
    }
}
