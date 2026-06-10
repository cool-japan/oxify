//! Node configuration validation logic for the HTMX node_validate endpoint.
//!
//! This module is deliberately separated from `handlers/htmx.rs` so that:
//!   1. The validation functions remain unit-testable without any Axum dependencies.
//!   2. The handler file does not grow beyond the 2000-line policy threshold.

/// Escape special HTML characters to prevent XSS when interpolating user-supplied
/// strings into HTML responses.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Validate a node's JSON configuration string against the rules for its `node_type`.
///
/// Returns a (possibly empty) list of human-readable error messages.
/// An empty return value means the configuration is valid.
pub fn validate_node_config(node_type: &str, config_json: &str) -> Vec<String> {
    let config: serde_json::Value = match serde_json::from_str(config_json) {
        Ok(v) => v,
        Err(_) => {
            if config_json.trim().is_empty() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                return vec!["Configuration must be valid JSON".to_string()];
            }
        }
    };

    let mut errors = Vec::new();

    match node_type {
        "llm" | "LLM" => {
            require_field(
                &config,
                "model",
                "Model is required for LLM nodes",
                &mut errors,
            );
            require_field(
                &config,
                "prompt_template",
                "Prompt template is required for LLM nodes",
                &mut errors,
            );
            if let Some(temp) = config.get("temperature").and_then(|v| v.as_f64()) {
                if !(0.0..=2.0).contains(&temp) {
                    errors.push("Temperature must be between 0.0 and 2.0".to_string());
                }
            }
        }
        "retriever" | "Retriever" => {
            require_field(
                &config,
                "collection",
                "Collection name is required for Retriever nodes",
                &mut errors,
            );
        }
        "code" | "Code" => {
            require_field(
                &config,
                "script",
                "Script is required for Code nodes",
                &mut errors,
            );
        }
        "http" | "rest" | "Tool" => {
            require_field(
                &config,
                "url",
                "URL is required for HTTP/Tool nodes",
                &mut errors,
            );
        }
        "webhook" => {
            // Webhook nodes have no required fields; the webhook ID is generated server-side.
        }
        "ifelse" | "IfElse" => {
            require_field(
                &config,
                "condition",
                "Condition expression is required for IfElse nodes",
                &mut errors,
            );
        }
        "loop" | "Loop" => {
            require_field(
                &config,
                "max_iterations",
                "Max iterations is required for Loop nodes",
                &mut errors,
            );
            if let Some(max) = config.get("max_iterations").and_then(|v| v.as_u64()) {
                if max == 0 {
                    errors.push("Max iterations must be greater than 0".to_string());
                }
                if max > 10_000 {
                    errors.push("Max iterations must not exceed 10,000".to_string());
                }
            }
        }
        "subworkflow" | "SubWorkflow" => {
            require_field(
                &config,
                "workflow_id",
                "Workflow ID is required for SubWorkflow nodes",
                &mut errors,
            );
        }
        "vision" | "Vision" => {
            require_field(
                &config,
                "provider",
                "Provider is required for Vision nodes",
                &mut errors,
            );
        }
        // Types with no universally-required config fields:
        "start" | "Start" | "end" | "End" | "approval" | "Approval" | "form" | "Form"
        | "switch" | "Switch" | "parallel" | "Parallel" | "trycatch" | "TryCatch" => {}
        // Unknown / custom / plugin nodes — warn in logs but do not fail validation.
        _ => {}
    }

    errors
}

/// Check that `field` is present in `config`, is non-null, and (for strings) is non-empty.
/// Pushes `message` onto `errors` if the check fails.
pub fn require_field(
    config: &serde_json::Value,
    field: &str,
    message: &str,
    errors: &mut Vec<String>,
) {
    match config.get(field) {
        None => errors.push(message.to_string()),
        Some(v) if v.is_null() => errors.push(message.to_string()),
        Some(serde_json::Value::String(s)) if s.trim().is_empty() => {
            errors.push(message.to_string())
        }
        _ => {}
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- html_escape ---

    #[test]
    fn test_html_escape_ampersand() {
        assert_eq!(html_escape("a & b"), "a &amp; b");
    }

    #[test]
    fn test_html_escape_angle_brackets() {
        assert_eq!(html_escape("<b>"), "&lt;b&gt;");
    }

    #[test]
    fn test_html_escape_quotes() {
        assert_eq!(html_escape(r#"say "hi""#), "say &quot;hi&quot;");
    }

    #[test]
    fn test_html_escape_single_quote_xss() {
        // Single-quote escaping is required to prevent attribute-injection XSS.
        let result = html_escape("O'Reilly");
        assert_eq!(result, "O&#x27;Reilly");
    }

    #[test]
    fn test_html_escape_script_tag() {
        let input = "<script>alert('xss')</script>";
        let output = html_escape(input);
        assert_eq!(
            output,
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }

    #[test]
    fn test_html_escape_clean_string() {
        assert_eq!(html_escape("hello world"), "hello world");
    }

    // --- validate_node_config: LLM ---

    #[test]
    fn test_validate_llm_node_missing_required_fields() {
        let errors = validate_node_config("LLM", "{}");
        assert!(
            errors.iter().any(|e| e.contains("Model")),
            "Should require model: {:?}",
            errors
        );
        assert!(
            errors.iter().any(|e| e.contains("Prompt")),
            "Should require prompt_template: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_llm_node_valid() {
        let config = r#"{"model": "gpt-4", "prompt_template": "Hello {{input}}"}"#;
        let errors = validate_node_config("LLM", config);
        assert!(
            errors.is_empty(),
            "Valid LLM config should have no errors: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_llm_node_lowercase_alias() {
        let config = r#"{"model": "gpt-4o", "prompt_template": "Summarize: {{text}}"}"#;
        let errors = validate_node_config("llm", config);
        assert!(
            errors.is_empty(),
            "Lowercase 'llm' should also validate: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_llm_node_bad_temperature_high() {
        let config = r#"{"model": "gpt-4", "prompt_template": "Hi", "temperature": 3.5}"#;
        let errors = validate_node_config("LLM", config);
        assert!(
            errors.iter().any(|e| e.contains("Temperature")),
            "Should reject temperature > 2.0: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_llm_node_bad_temperature_negative() {
        let config = r#"{"model": "gpt-4", "prompt_template": "Hi", "temperature": -0.5}"#;
        let errors = validate_node_config("LLM", config);
        assert!(
            errors.iter().any(|e| e.contains("Temperature")),
            "Should reject negative temperature: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_llm_node_temperature_at_boundary() {
        let config = r#"{"model": "gpt-4", "prompt_template": "Hi", "temperature": 2.0}"#;
        let errors = validate_node_config("LLM", config);
        assert!(
            errors.is_empty(),
            "Temperature exactly 2.0 should be valid: {:?}",
            errors
        );
    }

    // --- validate_node_config: Loop ---

    #[test]
    fn test_validate_loop_node_zero_iterations() {
        let config = r#"{"max_iterations": 0}"#;
        let errors = validate_node_config("Loop", config);
        assert!(
            errors.iter().any(|e| e.contains("greater than 0")),
            "Should reject max_iterations=0: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_loop_node_too_many_iterations() {
        let config = r#"{"max_iterations": 99999}"#;
        let errors = validate_node_config("Loop", config);
        assert!(
            errors.iter().any(|e| e.contains("10,000")),
            "Should reject max_iterations > 10000: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_loop_node_valid() {
        let config = r#"{"max_iterations": 100}"#;
        let errors = validate_node_config("Loop", config);
        assert!(
            errors.is_empty(),
            "Loop with valid iterations: {:?}",
            errors
        );
    }

    // --- validate_node_config: no-required-fields types ---

    #[test]
    fn test_validate_start_node_no_required_fields() {
        let errors = validate_node_config("Start", "{}");
        assert!(
            errors.is_empty(),
            "Start nodes should need no required fields: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_end_node_no_required_fields() {
        let errors = validate_node_config("End", "{}");
        assert!(errors.is_empty(), "End nodes: {:?}", errors);
    }

    #[test]
    fn test_validate_parallel_node_no_required_fields() {
        let errors = validate_node_config("Parallel", "{}");
        assert!(errors.is_empty(), "Parallel nodes: {:?}", errors);
    }

    #[test]
    fn test_validate_webhook_node_no_required_fields() {
        let errors = validate_node_config("webhook", "{}");
        assert!(errors.is_empty(), "Webhook nodes: {:?}", errors);
    }

    // --- validate_node_config: other types ---

    #[test]
    fn test_validate_retriever_missing_collection() {
        let errors = validate_node_config("Retriever", "{}");
        assert!(
            errors.iter().any(|e| e.contains("Collection")),
            "Retriever requires collection: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_code_missing_script() {
        let errors = validate_node_config("Code", "{}");
        assert!(
            errors.iter().any(|e| e.contains("Script")),
            "Code requires script: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_http_missing_url() {
        let errors = validate_node_config("http", "{}");
        assert!(
            errors.iter().any(|e| e.contains("URL")),
            "http requires url: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_ifelse_missing_condition() {
        let errors = validate_node_config("IfElse", "{}");
        assert!(
            errors.iter().any(|e| e.contains("Condition")),
            "IfElse requires condition: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_subworkflow_missing_workflow_id() {
        let errors = validate_node_config("SubWorkflow", "{}");
        assert!(
            errors.iter().any(|e| e.contains("Workflow ID")),
            "SubWorkflow requires workflow_id: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_vision_missing_provider() {
        let errors = validate_node_config("Vision", "{}");
        assert!(
            errors.iter().any(|e| e.contains("Provider")),
            "Vision requires provider: {:?}",
            errors
        );
    }

    // --- validate_node_config: unknown type ---

    #[test]
    fn test_validate_unknown_node_type_no_errors() {
        // Unknown / custom nodes should not produce errors — they may have their own schema.
        let errors = validate_node_config("my_custom_plugin_v2", "{}");
        assert!(
            errors.is_empty(),
            "Unknown node types should not error: {:?}",
            errors
        );
    }

    // --- validate_node_config: malformed JSON ---

    #[test]
    fn test_validate_invalid_json() {
        let errors = validate_node_config("LLM", "not json at all {{{");
        assert!(
            !errors.is_empty(),
            "Invalid JSON should produce an error: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_empty_string_treated_as_empty_object() {
        // An empty config string is equivalent to `{}` — no parse error.
        let errors = validate_node_config("Start", "");
        assert!(
            errors.is_empty(),
            "Empty string for Start node should be fine: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_empty_string_llm_still_requires_fields() {
        // Empty string is treated as `{}`, so required fields must still be present.
        let errors = validate_node_config("LLM", "");
        assert!(
            !errors.is_empty(),
            "LLM with empty config should have errors: {:?}",
            errors
        );
    }

    // --- require_field ---

    #[test]
    fn test_require_field_present_and_non_empty() {
        let v = serde_json::json!({"key": "value"});
        let mut errs = Vec::new();
        require_field(&v, "key", "Key is required", &mut errs);
        assert!(errs.is_empty());
    }

    #[test]
    fn test_require_field_null_value() {
        let v = serde_json::json!({"key": null});
        let mut errs = Vec::new();
        require_field(&v, "key", "Key is required", &mut errs);
        assert_eq!(errs, vec!["Key is required"]);
    }

    #[test]
    fn test_require_field_empty_string() {
        let v = serde_json::json!({"key": "   "});
        let mut errs = Vec::new();
        require_field(&v, "key", "Key is required", &mut errs);
        assert_eq!(errs, vec!["Key is required"]);
    }

    #[test]
    fn test_require_field_missing() {
        let v = serde_json::json!({});
        let mut errs = Vec::new();
        require_field(&v, "key", "Key is required", &mut errs);
        assert_eq!(errs, vec!["Key is required"]);
    }

    #[test]
    fn test_require_field_numeric_value_passes() {
        let v = serde_json::json!({"count": 42});
        let mut errs = Vec::new();
        require_field(&v, "count", "Count is required", &mut errs);
        assert!(errs.is_empty(), "Numeric value should pass require_field");
    }
}
