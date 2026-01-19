//! Response post-processing utilities for LLM outputs
//!
//! This module provides helper functions for common response transformations,
//! including code extraction, JSON parsing, markdown formatting, and content
//! filtering. These utilities make it easier to work with structured LLM outputs.
//!
//! # Examples
//!
//! ```rust
//! use oxify_connect_llm::ResponseUtils;
//!
//! let response = "Here's a Python example:\n\
//!     ```python\n\
//!     def hello():\n\
//!         print(\"Hello, world!\")\n\
//!     ```\n";
//!
//! let code_blocks = ResponseUtils::extract_code_blocks(response);
//! assert_eq!(code_blocks.len(), 1);
//! assert_eq!(code_blocks[0].language, Some("python".to_string()));
//! ```

use serde_json::Value;

/// Code block extracted from LLM response
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    /// Programming language (if specified)
    pub language: Option<String>,
    /// Code content
    pub code: String,
}

/// Response post-processing utilities
pub struct ResponseUtils;

impl ResponseUtils {
    /// Extract code blocks from markdown-formatted response
    ///
    /// Recognizes both ``` and ` code fence formats.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "```rust\nfn main() {}\n```";
    /// let blocks = ResponseUtils::extract_code_blocks(response);
    /// assert_eq!(blocks.len(), 1);
    /// assert_eq!(blocks[0].language, Some("rust".to_string()));
    /// ```
    pub fn extract_code_blocks(response: &str) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        let mut in_code_block = false;
        let mut current_language = None;
        let mut current_code = String::new();

        for line in response.lines() {
            if line.starts_with("```") {
                if in_code_block {
                    // End of code block
                    blocks.push(CodeBlock {
                        language: current_language.take(),
                        code: current_code.trim().to_string(),
                    });
                    current_code.clear();
                    in_code_block = false;
                } else {
                    // Start of code block
                    let lang = line.trim_start_matches('`').trim();
                    current_language = if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    };
                    in_code_block = true;
                }
            } else if in_code_block {
                current_code.push_str(line);
                current_code.push('\n');
            }
        }

        // Handle unclosed code block
        if in_code_block && !current_code.is_empty() {
            blocks.push(CodeBlock {
                language: current_language,
                code: current_code.trim().to_string(),
            });
        }

        blocks
    }

    /// Extract first code block of a specific language
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "```python\nprint('hello')\n```\n```rust\nprintln!(\"hi\")\n```";
    /// let python_code = ResponseUtils::extract_code_by_language(response, "python");
    /// assert_eq!(python_code, Some("print('hello')".to_string()));
    /// ```
    pub fn extract_code_by_language(response: &str, language: &str) -> Option<String> {
        Self::extract_code_blocks(response)
            .into_iter()
            .find(|block| {
                block
                    .language
                    .as_ref()
                    .map(|lang| lang.eq_ignore_ascii_case(language))
                    .unwrap_or(false)
            })
            .map(|block| block.code)
    }

    /// Extract all code regardless of language
    ///
    /// Returns concatenated code from all blocks.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "```\ncode1\n```\n```\ncode2\n```";
    /// let code = ResponseUtils::extract_all_code(response);
    /// assert!(code.contains("code1"));
    /// assert!(code.contains("code2"));
    /// ```
    pub fn extract_all_code(response: &str) -> String {
        Self::extract_code_blocks(response)
            .into_iter()
            .map(|block| block.code)
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Try to parse response as JSON
    ///
    /// Attempts to extract JSON from the response, handling cases where
    /// the LLM wraps JSON in markdown code blocks.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = r#"```json
    /// {"name": "Alice", "age": 30}
    /// ```"#;
    /// let json = ResponseUtils::parse_json(response);
    /// assert!(json.is_ok());
    /// ```
    pub fn parse_json(response: &str) -> Result<Value, serde_json::Error> {
        // Try parsing as-is first
        if let Ok(value) = serde_json::from_str(response.trim()) {
            return Ok(value);
        }

        // Try extracting from JSON code block
        if let Some(json_code) = Self::extract_code_by_language(response, "json") {
            return serde_json::from_str(&json_code);
        }

        // Try first code block (might be unlabeled JSON)
        if let Some(first_block) = Self::extract_code_blocks(response).first() {
            if let Ok(value) = serde_json::from_str(&first_block.code) {
                return Ok(value);
            }
        }

        // Last resort: try parsing the whole response
        serde_json::from_str(response.trim())
    }

    /// Remove markdown formatting from response
    ///
    /// Strips common markdown elements like headers, bold, italic, etc.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "# Title\n**Bold** and *italic*";
    /// let plain = ResponseUtils::strip_markdown(response);
    /// assert!(!plain.contains('*'));
    /// assert!(!plain.contains('#'));
    /// ```
    pub fn strip_markdown(response: &str) -> String {
        let mut result = response.to_string();

        // Remove headers
        result = result
            .lines()
            .map(|line| line.trim_start_matches('#').trim())
            .collect::<Vec<_>>()
            .join("\n");

        // Remove bold and italic
        result = result.replace("**", "");
        result = result.replace("__", "");
        result = result.replace('*', "");
        result = result.replace('_', "");

        // Remove inline code
        result = result.replace('`', "");

        result.trim().to_string()
    }

    /// Extract numbered list items from response
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "1. First\n2. Second\n3. Third";
    /// let items = ResponseUtils::extract_numbered_list(response);
    /// assert_eq!(items, vec!["First", "Second", "Third"]);
    /// ```
    pub fn extract_numbered_list(response: &str) -> Vec<String> {
        response
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                // Match patterns like "1. ", "2) ", etc.
                if let Some(pos) = trimmed.find(['.', ')']) {
                    let prefix = &trimmed[..pos];
                    if prefix.chars().all(|c| c.is_ascii_digit()) {
                        let content = trimmed[pos + 1..].trim();
                        if !content.is_empty() {
                            return Some(content.to_string());
                        }
                    }
                }
                None
            })
            .collect()
    }

    /// Extract bullet list items from response
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "- First\n* Second\n- Third";
    /// let items = ResponseUtils::extract_bullet_list(response);
    /// assert_eq!(items.len(), 3);
    /// ```
    pub fn extract_bullet_list(response: &str) -> Vec<String> {
        response
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with('-') || trimmed.starts_with('*') {
                    let content = trimmed[1..].trim();
                    if !content.is_empty() {
                        return Some(content.to_string());
                    }
                }
                None
            })
            .collect()
    }

    /// Truncate response to a maximum length
    ///
    /// Tries to truncate at sentence boundary if possible.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "First sentence. Second sentence. Third sentence.";
    /// let truncated = ResponseUtils::truncate(response, 20);
    /// assert!(truncated.len() <= 23); // 20 + "..."
    /// ```
    pub fn truncate(response: &str, max_length: usize) -> String {
        if response.len() <= max_length {
            return response.to_string();
        }

        // Try to find sentence boundary
        if let Some(pos) = response[..max_length].rfind(['.', '!', '?']) {
            return format!("{}...", &response[..=pos]);
        }

        // Fall back to word boundary
        if let Some(pos) = response[..max_length].rfind(' ') {
            return format!("{}...", &response[..pos]);
        }

        // Last resort: hard truncate
        format!("{}...", &response[..max_length])
    }

    /// Extract URLs from response
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "Check out https://example.com and http://test.org";
    /// let urls = ResponseUtils::extract_urls(response);
    /// assert_eq!(urls.len(), 2);
    /// ```
    pub fn extract_urls(response: &str) -> Vec<String> {
        let mut urls = Vec::new();
        for word in response.split_whitespace() {
            if word.starts_with("http://") || word.starts_with("https://") {
                // Clean up trailing punctuation
                let cleaned = word.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '/');
                urls.push(cleaned.to_string());
            }
        }
        urls
    }

    /// Count sentences in response
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "First sentence. Second sentence! Third sentence?";
    /// assert_eq!(ResponseUtils::count_sentences(response), 3);
    /// ```
    pub fn count_sentences(response: &str) -> usize {
        response
            .chars()
            .filter(|c| *c == '.' || *c == '!' || *c == '?')
            .count()
    }

    /// Count words in response
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "This is a test response";
    /// assert_eq!(ResponseUtils::count_words(response), 5);
    /// ```
    pub fn count_words(response: &str) -> usize {
        response.split_whitespace().count()
    }

    /// Remove extra whitespace from response
    ///
    /// # Examples
    ///
    /// ```
    /// use oxify_connect_llm::ResponseUtils;
    ///
    /// let response = "Too   many    spaces";
    /// let normalized = ResponseUtils::normalize_whitespace(response);
    /// assert_eq!(normalized, "Too many spaces");
    /// ```
    pub fn normalize_whitespace(response: &str) -> String {
        response.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_blocks() {
        let response = r#"
Here's some code:
```rust
fn main() {
    println!("Hello");
}
```

And Python:
```python
print("World")
```
"#;

        let blocks = ResponseUtils::extract_code_blocks(response);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].language, Some("rust".to_string()));
        assert_eq!(blocks[1].language, Some("python".to_string()));
        assert!(blocks[0].code.contains("fn main"));
        assert!(blocks[1].code.contains("print"));
    }

    #[test]
    fn test_extract_code_by_language() {
        let response = "```rust\nlet x = 5;\n```\n```python\ny = 10\n```";
        let rust_code = ResponseUtils::extract_code_by_language(response, "rust");
        assert_eq!(rust_code, Some("let x = 5;".to_string()));

        let python_code = ResponseUtils::extract_code_by_language(response, "python");
        assert_eq!(python_code, Some("y = 10".to_string()));

        let js_code = ResponseUtils::extract_code_by_language(response, "javascript");
        assert_eq!(js_code, None);
    }

    #[test]
    fn test_parse_json() {
        let response = r#"```json
{
  "name": "Alice",
  "age": 30
}
```"#;

        let json = ResponseUtils::parse_json(response).unwrap();
        assert_eq!(json["name"], "Alice");
        assert_eq!(json["age"], 30);
    }

    #[test]
    fn test_parse_json_direct() {
        let response = r#"{"name": "Bob", "age": 25}"#;
        let json = ResponseUtils::parse_json(response).unwrap();
        assert_eq!(json["name"], "Bob");
    }

    #[test]
    fn test_strip_markdown() {
        let response = "# Title\n**Bold** and *italic* text";
        let plain = ResponseUtils::strip_markdown(response);
        assert_eq!(plain, "Title\nBold and italic text");
    }

    #[test]
    fn test_extract_numbered_list() {
        let response = "1. First\n2. Second\n3. Third";
        let items = ResponseUtils::extract_numbered_list(response);
        assert_eq!(items, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn test_extract_bullet_list() {
        let response = "- Apple\n* Banana\n- Cherry";
        let items = ResponseUtils::extract_bullet_list(response);
        assert_eq!(items, vec!["Apple", "Banana", "Cherry"]);
    }

    #[test]
    fn test_truncate() {
        let response = "This is a long sentence. This is another sentence.";
        let truncated = ResponseUtils::truncate(response, 25);
        assert!(truncated.len() <= 28); // 25 + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_extract_urls() {
        let response = "Visit https://example.com and http://test.org for more info.";
        let urls = ResponseUtils::extract_urls(response);
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://example.com".to_string()));
        assert!(urls.contains(&"http://test.org".to_string()));
    }

    #[test]
    fn test_count_sentences() {
        let response = "First. Second! Third?";
        assert_eq!(ResponseUtils::count_sentences(response), 3);
    }

    #[test]
    fn test_count_words() {
        let response = "This is a test";
        assert_eq!(ResponseUtils::count_words(response), 4);
    }

    #[test]
    fn test_normalize_whitespace() {
        let response = "Too   many    spaces";
        let normalized = ResponseUtils::normalize_whitespace(response);
        assert_eq!(normalized, "Too many spaces");
    }

    #[test]
    fn test_extract_all_code() {
        let response = "```\ncode1\n```\nSome text\n```\ncode2\n```";
        let code = ResponseUtils::extract_all_code(response);
        assert!(code.contains("code1"));
        assert!(code.contains("code2"));
    }
}
