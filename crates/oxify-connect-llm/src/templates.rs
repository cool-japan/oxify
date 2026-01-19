//! Prompt template engine for LLM requests
//!
//! This module provides a simple yet powerful template engine for creating
//! reusable prompt templates with variable substitution and conditional sections.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("Missing variable: {0}")]
    MissingVariable(String),

    #[error("Template parsing error: {0}")]
    ParseError(String),

    #[error("Invalid template syntax: {0}")]
    SyntaxError(String),
}

/// A prompt template with variable substitution support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Template string with {{variable}} placeholders
    template: String,
    /// Optional list of required variables
    #[serde(default)]
    required_vars: Vec<String>,
    /// Optional description of the template
    #[serde(default)]
    description: Option<String>,
}

impl PromptTemplate {
    /// Create a new prompt template
    pub fn new(template: String) -> Self {
        Self {
            template,
            required_vars: Vec::new(),
            description: None,
        }
    }

    /// Set required variables
    pub fn with_required_vars(mut self, vars: Vec<String>) -> Self {
        self.required_vars = vars;
        self
    }

    /// Set description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Render the template with the given variables
    pub fn render(
        &self,
        variables: &HashMap<String, String>,
    ) -> std::result::Result<String, TemplateError> {
        // Check for required variables
        for var in &self.required_vars {
            if !variables.contains_key(var) {
                return Err(TemplateError::MissingVariable(var.clone()));
            }
        }

        let mut result = self.template.clone();

        // Replace {{variable}} with values
        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }

        // Check for any remaining unreplaced variables (strict mode)
        if result.contains("{{") && result.contains("}}") {
            let start = result.find("{{").unwrap();
            let end = result[start..].find("}}").unwrap() + start + 2;
            let var_name = &result[start + 2..end - 2];
            return Err(TemplateError::MissingVariable(var_name.to_string()));
        }

        Ok(result)
    }

    /// Render the template, allowing missing variables (they will be kept as-is)
    pub fn render_partial(&self, variables: &HashMap<String, String>) -> String {
        let mut result = self.template.clone();

        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }

        result
    }

    /// Extract all variable names from the template
    pub fn extract_variables(&self) -> Vec<String> {
        let mut variables = Vec::new();
        let mut chars = self.template.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' {
                if let Some(&'{') = chars.peek() {
                    chars.next(); // consume second '{'
                    let mut var_name = String::new();

                    // Read until we find '}}'
                    while let Some(ch) = chars.next() {
                        if ch == '}' {
                            if let Some(&'}') = chars.peek() {
                                chars.next(); // consume second '}'
                                variables.push(var_name);
                                break;
                            }
                        }
                        var_name.push(ch);
                    }
                }
            }
        }

        variables
    }
}

/// Template library for common prompt patterns
pub struct TemplateLibrary {
    templates: HashMap<String, PromptTemplate>,
}

impl Default for TemplateLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateLibrary {
    /// Create a new template library with common templates
    pub fn new() -> Self {
        let mut library = Self {
            templates: HashMap::new(),
        };

        // Add common templates
        library.add_common_templates();
        library
    }

    /// Add a template to the library
    pub fn add(&mut self, name: String, template: PromptTemplate) {
        self.templates.insert(name, template);
    }

    /// Get a template by name
    pub fn get(&self, name: &str) -> Option<&PromptTemplate> {
        self.templates.get(name)
    }

    /// Remove a template by name
    pub fn remove(&mut self, name: &str) -> Option<PromptTemplate> {
        self.templates.remove(name)
    }

    /// List all template names
    pub fn list(&self) -> Vec<String> {
        self.templates.keys().cloned().collect()
    }

    /// Add common templates to the library
    fn add_common_templates(&mut self) {
        // Code review template
        self.add(
            "code_review".to_string(),
            PromptTemplate::new(
                "Review the following {{language}} code and provide feedback on:\n\
                 1. Code quality and best practices\n\
                 2. Potential bugs or issues\n\
                 3. Performance improvements\n\
                 4. Security concerns\n\n\
                 Code:\n```{{language}}\n{{code}}\n```"
                    .to_string(),
            )
            .with_required_vars(vec!["language".to_string(), "code".to_string()])
            .with_description("Code review template for analyzing code quality".to_string()),
        );

        // Summarization template
        self.add(
            "summarize".to_string(),
            PromptTemplate::new(
                "Summarize the following text in {{style}} style:\n\n{{text}}".to_string(),
            )
            .with_required_vars(vec!["text".to_string()])
            .with_description("Text summarization template".to_string()),
        );

        // Question answering template
        self.add(
            "qa".to_string(),
            PromptTemplate::new(
                "Context:\n{{context}}\n\nQuestion: {{question}}\n\nAnswer:".to_string(),
            )
            .with_required_vars(vec!["context".to_string(), "question".to_string()])
            .with_description("Question answering with context".to_string()),
        );

        // Translation template
        self.add(
            "translate".to_string(),
            PromptTemplate::new(
                "Translate the following text from {{source_lang}} to {{target_lang}}:\n\n{{text}}"
                    .to_string(),
            )
            .with_required_vars(vec![
                "source_lang".to_string(),
                "target_lang".to_string(),
                "text".to_string(),
            ])
            .with_description("Language translation template".to_string()),
        );

        // Text classification template
        self.add(
            "classify".to_string(),
            PromptTemplate::new(
                "Classify the following text into one of these categories: {{categories}}\n\n\
                 Text: {{text}}\n\n\
                 Category:"
                    .to_string(),
            )
            .with_required_vars(vec!["categories".to_string(), "text".to_string()])
            .with_description("Text classification template".to_string()),
        );

        // Data extraction template
        self.add(
            "extract".to_string(),
            PromptTemplate::new(
                "Extract the following information from the text:\n{{fields}}\n\n\
                 Text: {{text}}\n\n\
                 Extracted information (as JSON):"
                    .to_string(),
            )
            .with_required_vars(vec!["fields".to_string(), "text".to_string()])
            .with_description("Structured data extraction template".to_string()),
        );

        // Chain of thought template
        self.add(
            "chain_of_thought".to_string(),
            PromptTemplate::new(
                "{{task}}\n\n\
                 Let's approach this step-by-step:\n\
                 1. First, let's understand what we know\n\
                 2. Then, let's identify what we need to find\n\
                 3. Finally, let's solve the problem\n\n\
                 Input: {{input}}"
                    .to_string(),
            )
            .with_required_vars(vec!["task".to_string(), "input".to_string()])
            .with_description("Chain of thought reasoning template".to_string()),
        );

        // Few-shot learning template
        self.add(
            "few_shot".to_string(),
            PromptTemplate::new(
                "{{task_description}}\n\n\
                 Examples:\n{{examples}}\n\n\
                 Now, apply the same pattern:\n{{input}}"
                    .to_string(),
            )
            .with_required_vars(vec![
                "task_description".to_string(),
                "examples".to_string(),
                "input".to_string(),
            ])
            .with_description("Few-shot learning template".to_string()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_render() {
        let template =
            PromptTemplate::new("Hello {{name}}, you are {{age}} years old.".to_string());

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        vars.insert("age".to_string(), "30".to_string());

        let result = template.render(&vars).unwrap();
        assert_eq!(result, "Hello Alice, you are 30 years old.");
    }

    #[test]
    fn test_template_missing_variable() {
        let template = PromptTemplate::new("Hello {{name}}".to_string())
            .with_required_vars(vec!["name".to_string()]);

        let vars = HashMap::new();
        let result = template.render(&vars);
        assert!(result.is_err());
        assert!(matches!(result, Err(TemplateError::MissingVariable(_))));
    }

    #[test]
    fn test_template_partial_render() {
        let template = PromptTemplate::new("Hello {{name}}, {{greeting}}".to_string());

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Bob".to_string());

        let result = template.render_partial(&vars);
        assert_eq!(result, "Hello Bob, {{greeting}}");
    }

    #[test]
    fn test_extract_variables() {
        let template = PromptTemplate::new("{{var1}} and {{var2}} and {{var3}}".to_string());
        let vars = template.extract_variables();
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"var1".to_string()));
        assert!(vars.contains(&"var2".to_string()));
        assert!(vars.contains(&"var3".to_string()));
    }

    #[test]
    fn test_template_library() {
        let library = TemplateLibrary::new();

        // Test that common templates are loaded
        assert!(library.get("code_review").is_some());
        assert!(library.get("summarize").is_some());
        assert!(library.get("qa").is_some());
        assert!(library.get("translate").is_some());

        let code_review = library.get("code_review").unwrap();
        let vars_needed = code_review.extract_variables();
        assert!(vars_needed.contains(&"language".to_string()));
        assert!(vars_needed.contains(&"code".to_string()));
    }

    #[test]
    fn test_code_review_template() {
        let library = TemplateLibrary::new();
        let template = library.get("code_review").unwrap();

        let mut vars = HashMap::new();
        vars.insert("language".to_string(), "Rust".to_string());
        vars.insert(
            "code".to_string(),
            "fn main() { println!(\"Hello\"); }".to_string(),
        );

        let result = template.render(&vars).unwrap();
        assert!(result.contains("Rust"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_qa_template() {
        let library = TemplateLibrary::new();
        let template = library.get("qa").unwrap();

        let mut vars = HashMap::new();
        vars.insert("context".to_string(), "The sky is blue.".to_string());
        vars.insert("question".to_string(), "What color is the sky?".to_string());

        let result = template.render(&vars).unwrap();
        assert!(result.contains("Context"));
        assert!(result.contains("The sky is blue"));
        assert!(result.contains("What color is the sky?"));
    }

    #[test]
    fn test_custom_template_addition() {
        let mut library = TemplateLibrary::new();

        let custom = PromptTemplate::new("Custom: {{value}}".to_string());
        library.add("custom".to_string(), custom);

        assert!(library.get("custom").is_some());

        let template = library.get("custom").unwrap();
        let mut vars = HashMap::new();
        vars.insert("value".to_string(), "test".to_string());

        let result = template.render(&vars).unwrap();
        assert_eq!(result, "Custom: test");
    }
}
