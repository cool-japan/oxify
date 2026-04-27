//! NL → Workflow Generator command
//!
//! Accepts a natural-language description, calls an LLM, produces a validated
//! workflow JSON/YAML file (with automatic retry + validation-error feedback).

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::path::PathBuf;

use oxify_connect_llm::{
    AnthropicProvider, LlmProvider, LlmRequest, OllamaProvider, OpenAIProvider,
};
use oxify_model::validation::WorkflowValidator;
use oxify_model::workflow::Workflow;

/// Arguments for the `oxify generate` sub-command.
#[derive(Parser, Debug)]
pub struct GenerateArgs {
    /// Natural language description of the workflow to create
    #[arg(short, long)]
    pub description: String,

    /// LLM provider to use (openai, anthropic, ollama)
    #[arg(short, long, default_value = "openai")]
    pub provider: String,

    /// Specific model name (e.g. gpt-4o, claude-3-5-sonnet-20241022)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Output file path (stdout if not specified)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Maximum retries on validation failure
    #[arg(long, default_value_t = 3)]
    pub max_retries: u32,

    /// API key for the LLM provider (overrides env vars)
    #[arg(long)]
    pub api_key: Option<String>,

    /// Output format: json or yaml
    #[arg(long, default_value = "json")]
    pub format: String,
}

/// Entry point for the `generate` command.
pub async fn run(args: GenerateArgs) -> Result<()> {
    let provider = build_llm_provider(&args).context("failed to initialize LLM provider")?;

    let schema_obj = oxify_model::generate_workflow_schema();
    let schema_str = oxify_model::schema_to_json(&schema_obj)
        .context("failed to serialize workflow JSON schema")?;
    let system = build_system_prompt(&schema_str);

    let mut last_error: Option<String> = None;
    let mut last_workflow: Option<Workflow> = None;

    for attempt in 0..args.max_retries {
        tracing::info!("Generate attempt {} of {}", attempt + 1, args.max_retries);

        let user_msg = match &last_error {
            Some(err) => format!(
                "Description: {}\n\n{}{}",
                args.description,
                super::generate_prompts::RETRY_PREFIX,
                err
            ),
            None => format!("Description: {}", args.description),
        };

        let request = build_llm_request(&system, &user_msg, &args);

        let response = provider
            .complete(request)
            .await
            .context("LLM call failed")?;

        let json_str = match extract_json_block(&response.content) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("attempt {}: {}", attempt + 1, e);
                last_error = Some(format!("LLM response did not contain a JSON block: {}", e));
                continue;
            }
        };

        let workflow: Workflow = match serde_json::from_str(&json_str) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("attempt {}: JSON parse error: {}", attempt + 1, e);
                last_error = Some(format!("JSON parse error: {}", e));
                continue;
            }
        };

        match WorkflowValidator::validate(&workflow) {
            Ok(_report) => {
                last_workflow = Some(workflow);
                break;
            }
            Err(validation_err) => {
                tracing::warn!(
                    "attempt {}: validation failed: {}",
                    attempt + 1,
                    validation_err
                );
                last_error = Some(validation_err.to_string());
            }
        }
    }

    match last_workflow {
        Some(wf) => write_output(&wf, &args),
        None => Err(anyhow!(
            "failed to generate a valid workflow after {} attempt(s). Last error: {}",
            args.max_retries,
            last_error.unwrap_or_else(|| "unknown".to_string())
        )),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_system_prompt(schema: &str) -> String {
    super::generate_prompts::SYSTEM_PROMPT_TEMPLATE.replace("{SCHEMA}", schema)
}

fn build_llm_request(system: &str, user_msg: &str, _args: &GenerateArgs) -> LlmRequest {
    LlmRequest {
        prompt: user_msg.to_string(),
        system_prompt: Some(system.to_string()),
        // Use a generous token budget so the LLM can emit complete JSON
        max_tokens: Some(4096),
        // Slightly lower temperature for more deterministic JSON output
        temperature: Some(0.2),
        tools: Vec::new(),
        images: Vec::new(),
    }
}

fn build_llm_provider(args: &GenerateArgs) -> Result<Box<dyn LlmProvider>> {
    match args.provider.to_lowercase().as_str() {
        "openai" => {
            let api_key = resolve_api_key(args, "OPENAI_API_KEY")?;
            let model = args.model.clone().unwrap_or_else(|| "gpt-4o".to_string());
            Ok(Box::new(OpenAIProvider::new(api_key, model)))
        }
        "anthropic" => {
            let api_key = resolve_api_key(args, "ANTHROPIC_API_KEY")?;
            let model = args
                .model
                .clone()
                .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());
            Ok(Box::new(AnthropicProvider::new(api_key, model)))
        }
        "ollama" => {
            let model = args.model.clone().unwrap_or_else(|| "llama3".to_string());
            Ok(Box::new(OllamaProvider::new(model)))
        }
        other => Err(anyhow!(
            "unsupported provider '{}'. Use one of: openai, anthropic, ollama",
            other
        )),
    }
}

/// Resolve the API key: CLI flag > environment variable.
fn resolve_api_key(args: &GenerateArgs, env_var: &str) -> Result<String> {
    if let Some(ref key) = args.api_key {
        return Ok(key.clone());
    }
    std::env::var(env_var).with_context(|| {
        format!(
            "API key not provided. Set the {} environment variable or pass --api-key",
            env_var
        )
    })
}

/// Extract a JSON object from an LLM response that may contain markdown fences.
fn extract_json_block(content: &str) -> Result<String> {
    // Priority 1: ```json ... ``` fence
    if let Some(start) = content.find("```json") {
        let after = &content[start + 7..];
        if let Some(end) = after.find("```") {
            return Ok(after[..end].trim().to_string());
        }
    }

    // Priority 2: ``` ... ``` fence where content looks like JSON
    if let Some(start) = content.find("```") {
        let after = &content[start + 3..];
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            if candidate.starts_with('{') {
                return Ok(candidate.to_string());
            }
        }
    }

    // Priority 3: entire content is already a bare JSON object
    let trimmed = content.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Ok(trimmed.to_string());
    }

    Err(anyhow!("no JSON block found in LLM response"))
}

/// Serialize and write the workflow to the requested output destination.
fn write_output(workflow: &Workflow, args: &GenerateArgs) -> Result<()> {
    let output_str = match args.format.as_str() {
        "yaml" => {
            serde_yaml::to_string(workflow).context("failed to serialize workflow to YAML")?
        }
        _ => serde_json::to_string_pretty(workflow)
            .context("failed to serialize workflow to JSON")?,
    };

    match &args.output {
        Some(path) => {
            std::fs::write(path, &output_str)
                .with_context(|| format!("failed to write to {}", path.display()))?;
            println!("Workflow written to {}", path.display());
        }
        None => {
            println!("{}", output_str);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_block_with_json_fence() {
        let content = "Here is the workflow:\n```json\n{\"id\": \"test\"}\n```\nDone.";
        let result = extract_json_block(content).unwrap();
        assert_eq!(result, "{\"id\": \"test\"}");
    }

    #[test]
    fn test_extract_json_block_with_plain_fence() {
        let content = "```\n{\"id\": \"test\"}\n```";
        let result = extract_json_block(content).unwrap();
        assert_eq!(result, "{\"id\": \"test\"}");
    }

    #[test]
    fn test_extract_json_block_bare_json() {
        let content = "{\"id\": \"test\", \"name\": \"My Workflow\"}";
        let result = extract_json_block(content).unwrap();
        assert!(result.contains("\"id\""));
    }

    #[test]
    fn test_extract_json_block_no_json_errors() {
        let content = "No JSON here, just text.";
        let result = extract_json_block(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_system_prompt_includes_schema() {
        let schema = r#"{"type": "object", "properties": {"id": {}}}"#;
        let prompt = build_system_prompt(schema);
        assert!(prompt.contains("id"));
        assert!(prompt.contains("workflow"));
    }

    #[tokio::test]
    async fn test_write_output_to_tempfile() {
        use oxify_model::{Edge, Node, NodeKind};

        let tmp = std::env::temp_dir().join("oxify_test_generate_output.json");

        let mut wf = Workflow::new("test".to_string());
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start.id;
        let end = Node::new("End".to_string(), NodeKind::End);
        let end_id = end.id;
        wf.add_node(start);
        wf.add_node(end);
        wf.add_edge(Edge::new(start_id, end_id));

        let args = GenerateArgs {
            description: "test workflow".to_string(),
            provider: "openai".to_string(),
            model: None,
            output: Some(tmp.clone()),
            max_retries: 3,
            api_key: None,
            format: "json".to_string(),
        };

        write_output(&wf, &args).unwrap();
        assert!(tmp.exists());
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("test"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_resolve_api_key_from_args() {
        let args = GenerateArgs {
            description: "d".to_string(),
            provider: "openai".to_string(),
            model: None,
            output: None,
            max_retries: 3,
            api_key: Some("sk-test-key".to_string()),
            format: "json".to_string(),
        };
        let key = resolve_api_key(&args, "OPENAI_API_KEY").unwrap();
        assert_eq!(key, "sk-test-key");
    }

    #[test]
    fn test_build_llm_request_fields() {
        let system = "system instructions".to_string();
        let user = "user message".to_string();
        let args = GenerateArgs {
            description: "d".to_string(),
            provider: "openai".to_string(),
            model: None,
            output: None,
            max_retries: 3,
            api_key: None,
            format: "json".to_string(),
        };
        let req = build_llm_request(&system, &user, &args);
        assert_eq!(req.prompt, user);
        assert_eq!(req.system_prompt.as_deref(), Some("system instructions"));
        assert_eq!(req.max_tokens, Some(4096));
    }
}
