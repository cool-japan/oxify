//! Prompt Engineering Utilities
//!
//! This module provides utilities for constructing effective prompts using proven
//! prompt engineering techniques like few-shot learning, chain-of-thought reasoning,
//! role-based prompting, and more.
//!
//! # Examples
//!
//! ```
//! use oxify_connect_llm::{FewShotPrompt, Example, ChainOfThought};
//!
//! // Few-shot learning
//! let mut few_shot = FewShotPrompt::new("Classify the sentiment of the following text:");
//! few_shot.add_example("I love this product!", "positive");
//! few_shot.add_example("This is terrible.", "negative");
//! few_shot.set_query("This is amazing!");
//!
//! let prompt = few_shot.build();
//! assert!(prompt.contains("I love this product!"));
//!
//! // Chain-of-thought prompting
//! let cot = ChainOfThought::new("What is 15% of 80?")
//!     .with_instruction("Let's think step by step:");
//! let prompt = cot.build();
//! ```

use std::fmt;

/// Example for few-shot learning
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Example {
    /// Input for the example
    pub input: String,
    /// Expected output for the example
    pub output: String,
}

impl Example {
    /// Create a new example
    pub fn new(input: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
        }
    }
}

/// Few-shot prompt builder
///
/// Constructs prompts using few-shot learning by providing examples
/// before the actual query.
#[derive(Debug, Clone)]
pub struct FewShotPrompt {
    instruction: String,
    examples: Vec<Example>,
    query: Option<String>,
    input_prefix: String,
    output_prefix: String,
    separator: String,
}

impl FewShotPrompt {
    /// Create a new few-shot prompt with an instruction
    pub fn new(instruction: impl Into<String>) -> Self {
        Self {
            instruction: instruction.into(),
            examples: Vec::new(),
            query: None,
            input_prefix: "Input: ".to_string(),
            output_prefix: "Output: ".to_string(),
            separator: "\n\n".to_string(),
        }
    }

    /// Add an example
    pub fn add_example(
        &mut self,
        input: impl Into<String>,
        output: impl Into<String>,
    ) -> &mut Self {
        self.examples.push(Example::new(input, output));
        self
    }

    /// Add multiple examples
    pub fn add_examples(&mut self, examples: Vec<Example>) -> &mut Self {
        self.examples.extend(examples);
        self
    }

    /// Set the query (the actual input to process)
    pub fn set_query(&mut self, query: impl Into<String>) -> &mut Self {
        self.query = Some(query.into());
        self
    }

    /// Set custom input/output prefixes
    pub fn with_prefixes(
        &mut self,
        input_prefix: impl Into<String>,
        output_prefix: impl Into<String>,
    ) -> &mut Self {
        self.input_prefix = input_prefix.into();
        self.output_prefix = output_prefix.into();
        self
    }

    /// Set custom separator between examples
    pub fn with_separator(&mut self, separator: impl Into<String>) -> &mut Self {
        self.separator = separator.into();
        self
    }

    /// Build the final prompt
    pub fn build(&self) -> String {
        let mut prompt = self.instruction.clone();
        prompt.push_str(&self.separator);

        // Add examples
        for example in &self.examples {
            prompt.push_str(&self.input_prefix);
            prompt.push_str(&example.input);
            prompt.push('\n');
            prompt.push_str(&self.output_prefix);
            prompt.push_str(&example.output);
            prompt.push_str(&self.separator);
        }

        // Add query
        if let Some(query) = &self.query {
            prompt.push_str(&self.input_prefix);
            prompt.push_str(query);
            prompt.push('\n');
            prompt.push_str(&self.output_prefix);
        }

        prompt
    }
}

/// Chain-of-thought prompt builder
///
/// Encourages step-by-step reasoning by adding instructions
/// that guide the model to think through the problem.
#[derive(Debug, Clone)]
pub struct ChainOfThought {
    question: String,
    instruction: String,
    examples: Vec<(String, String)>, // (question, reasoning)
}

impl ChainOfThought {
    /// Create a new chain-of-thought prompt
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            instruction: "Let's approach this step-by-step:".to_string(),
            examples: Vec::new(),
        }
    }

    /// Set custom instruction
    pub fn with_instruction(mut self, instruction: impl Into<String>) -> Self {
        self.instruction = instruction.into();
        self
    }

    /// Add a reasoning example (question + step-by-step solution)
    pub fn add_example(
        mut self,
        question: impl Into<String>,
        reasoning: impl Into<String>,
    ) -> Self {
        self.examples.push((question.into(), reasoning.into()));
        self
    }

    /// Build the final prompt
    pub fn build(&self) -> String {
        let mut prompt = String::new();

        // Add examples first
        for (q, reasoning) in &self.examples {
            prompt.push_str("Question: ");
            prompt.push_str(q);
            prompt.push_str("\n\n");
            prompt.push_str(reasoning);
            prompt.push_str("\n\n---\n\n");
        }

        // Add the actual question
        prompt.push_str("Question: ");
        prompt.push_str(&self.question);
        prompt.push_str("\n\n");
        prompt.push_str(&self.instruction);

        prompt
    }
}

/// Role-based prompt builder
///
/// Constructs prompts with explicit role assignments (system, user, assistant)
/// for better context and behavior control.
#[derive(Debug, Clone)]
pub struct RolePrompt {
    system_message: Option<String>,
    messages: Vec<(Role, String)>,
}

/// Role in a conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// System message (sets behavior and context)
    System,
    /// User message
    User,
    /// Assistant message
    Assistant,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::System => write!(f, "System"),
            Role::User => write!(f, "User"),
            Role::Assistant => write!(f, "Assistant"),
        }
    }
}

impl RolePrompt {
    /// Create a new role-based prompt
    pub fn new() -> Self {
        Self {
            system_message: None,
            messages: Vec::new(),
        }
    }

    /// Set system message
    pub fn system(mut self, message: impl Into<String>) -> Self {
        self.system_message = Some(message.into());
        self
    }

    /// Add user message
    pub fn user(mut self, message: impl Into<String>) -> Self {
        self.messages.push((Role::User, message.into()));
        self
    }

    /// Add assistant message
    pub fn assistant(mut self, message: impl Into<String>) -> Self {
        self.messages.push((Role::Assistant, message.into()));
        self
    }

    /// Add a message with explicit role
    pub fn add_message(mut self, role: Role, message: impl Into<String>) -> Self {
        self.messages.push((role, message.into()));
        self
    }

    /// Build the final prompt
    pub fn build(&self) -> String {
        let mut prompt = String::new();

        // Add system message if present
        if let Some(system) = &self.system_message {
            prompt.push_str("System: ");
            prompt.push_str(system);
            prompt.push_str("\n\n");
        }

        // Add conversation messages
        for (role, message) in &self.messages {
            prompt.push_str(&format!("{}: ", role));
            prompt.push_str(message);
            prompt.push_str("\n\n");
        }

        prompt.trim_end().to_string()
    }

    /// Get system message and conversation separately (useful for API calls)
    pub fn split(&self) -> (Option<String>, Vec<(Role, String)>) {
        (self.system_message.clone(), self.messages.clone())
    }
}

impl Default for RolePrompt {
    fn default() -> Self {
        Self::new()
    }
}

/// Instruction-based prompt builder
///
/// Constructs clear, structured prompts with explicit instructions,
/// context, and constraints.
#[derive(Debug, Clone)]
pub struct InstructionPrompt {
    task: String,
    context: Vec<String>,
    constraints: Vec<String>,
    examples: Vec<String>,
    format: Option<String>,
}

impl InstructionPrompt {
    /// Create a new instruction prompt
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            context: Vec::new(),
            constraints: Vec::new(),
            examples: Vec::new(),
            format: None,
        }
    }

    /// Add context information
    pub fn add_context(mut self, context: impl Into<String>) -> Self {
        self.context.push(context.into());
        self
    }

    /// Add a constraint
    pub fn add_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }

    /// Add an example
    pub fn add_example(mut self, example: impl Into<String>) -> Self {
        self.examples.push(example.into());
        self
    }

    /// Specify expected output format
    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Build the final prompt
    pub fn build(&self) -> String {
        let mut prompt = String::new();

        // Task
        prompt.push_str("Task: ");
        prompt.push_str(&self.task);
        prompt.push_str("\n\n");

        // Context
        if !self.context.is_empty() {
            prompt.push_str("Context:\n");
            for (i, ctx) in self.context.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, ctx));
            }
            prompt.push('\n');
        }

        // Constraints
        if !self.constraints.is_empty() {
            prompt.push_str("Constraints:\n");
            for constraint in &self.constraints {
                prompt.push_str("- ");
                prompt.push_str(constraint);
                prompt.push('\n');
            }
            prompt.push('\n');
        }

        // Examples
        if !self.examples.is_empty() {
            prompt.push_str("Examples:\n");
            for (i, example) in self.examples.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, example));
            }
            prompt.push('\n');
        }

        // Format
        if let Some(format) = &self.format {
            prompt.push_str("Expected Format: ");
            prompt.push_str(format);
            prompt.push_str("\n\n");
        }

        prompt.trim_end().to_string()
    }
}

/// Common system prompts for different personas
pub struct SystemPrompts;

impl SystemPrompts {
    /// Expert assistant persona
    pub fn expert_assistant() -> String {
        "You are an expert assistant with deep knowledge across multiple domains. \
        Provide accurate, detailed, and well-reasoned responses. When uncertain, \
        acknowledge limitations and suggest where to find more information."
            .to_string()
    }

    /// Code assistant persona
    pub fn code_assistant() -> String {
        "You are an expert programming assistant. Provide clean, efficient, and \
        well-documented code. Explain your reasoning and suggest best practices. \
        Always consider edge cases and error handling."
            .to_string()
    }

    /// Teacher persona
    pub fn teacher() -> String {
        "You are a patient and knowledgeable teacher. Break down complex topics \
        into simple, understandable explanations. Use analogies and examples. \
        Encourage learning by asking thought-provoking questions."
            .to_string()
    }

    /// Analyst persona
    pub fn analyst() -> String {
        "You are a thorough analyst. Examine information critically, consider \
        multiple perspectives, identify patterns and trends. Support your \
        conclusions with evidence and clear reasoning."
            .to_string()
    }

    /// Creative writer persona
    pub fn creative_writer() -> String {
        "You are a creative writer with a vivid imagination. Craft engaging \
        narratives with rich descriptions and compelling characters. Pay attention \
        to tone, pacing, and emotional resonance."
            .to_string()
    }

    /// Concise responder persona
    pub fn concise() -> String {
        "You are a concise assistant. Provide brief, to-the-point responses. \
        Focus on key information without unnecessary elaboration. Use clear, \
        simple language."
            .to_string()
    }

    /// Socratic questioner persona
    pub fn socratic() -> String {
        "You are a Socratic teacher. Guide users to discover answers through \
        thoughtful questions. Encourage critical thinking and self-reflection. \
        Help users develop their own understanding."
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_few_shot_basic() {
        let mut few_shot = FewShotPrompt::new("Classify sentiment:");
        few_shot.add_example("Great!", "positive");
        few_shot.add_example("Awful.", "negative");
        few_shot.set_query("Amazing!");

        let prompt = few_shot.build();
        assert!(prompt.contains("Classify sentiment:"));
        assert!(prompt.contains("Input: Great!"));
        assert!(prompt.contains("Output: positive"));
        assert!(prompt.contains("Input: Amazing!"));
    }

    #[test]
    fn test_few_shot_custom_prefixes() {
        let mut few_shot = FewShotPrompt::new("Test:");
        few_shot
            .with_prefixes("Q: ", "A: ")
            .add_example("1+1", "2")
            .set_query("2+2");

        let prompt = few_shot.build();
        assert!(prompt.contains("Q: 1+1"));
        assert!(prompt.contains("A: 2"));
    }

    #[test]
    fn test_chain_of_thought() {
        let cot = ChainOfThought::new("What is 25% of 80?")
            .with_instruction("Let's solve this step by step:");

        let prompt = cot.build();
        assert!(prompt.contains("Question: What is 25% of 80?"));
        assert!(prompt.contains("Let's solve this step by step:"));
    }

    #[test]
    fn test_chain_of_thought_with_examples() {
        let cot = ChainOfThought::new("What is 15% of 60?").add_example(
            "What is 10% of 50?",
            "Step 1: Convert 10% to decimal: 0.10\nStep 2: Multiply: 50 × 0.10 = 5",
        );

        let prompt = cot.build();
        assert!(prompt.contains("What is 10% of 50?"));
        assert!(prompt.contains("Step 1"));
        assert!(prompt.contains("What is 15% of 60?"));
    }

    #[test]
    fn test_role_prompt() {
        let prompt = RolePrompt::new()
            .system("You are a helpful assistant.")
            .user("Hello!")
            .assistant("Hi! How can I help you?")
            .user("Tell me a joke.");

        let text = prompt.build();
        assert!(text.contains("System: You are a helpful assistant."));
        assert!(text.contains("User: Hello!"));
        assert!(text.contains("Assistant: Hi! How can I help you?"));
        assert!(text.contains("User: Tell me a joke."));
    }

    #[test]
    fn test_role_prompt_split() {
        let prompt = RolePrompt::new().system("Be helpful.").user("Hi");

        let (system, messages) = prompt.split();
        assert_eq!(system, Some("Be helpful.".to_string()));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, Role::User);
    }

    #[test]
    fn test_instruction_prompt() {
        let prompt = InstructionPrompt::new("Summarize this text")
            .add_context("The text is a news article")
            .add_constraint("Keep it under 100 words")
            .add_constraint("Focus on key facts")
            .with_format("Bullet points");

        let text = prompt.build();
        assert!(text.contains("Task: Summarize this text"));
        assert!(text.contains("Context:"));
        assert!(text.contains("news article"));
        assert!(text.contains("Constraints:"));
        assert!(text.contains("under 100 words"));
        assert!(text.contains("Expected Format: Bullet points"));
    }

    #[test]
    fn test_system_prompts() {
        assert!(!SystemPrompts::expert_assistant().is_empty());
        assert!(!SystemPrompts::code_assistant().is_empty());
        assert!(!SystemPrompts::teacher().is_empty());
        assert!(!SystemPrompts::analyst().is_empty());
        assert!(!SystemPrompts::creative_writer().is_empty());
        assert!(!SystemPrompts::concise().is_empty());
        assert!(!SystemPrompts::socratic().is_empty());
    }

    #[test]
    fn test_example_creation() {
        let example = Example::new("input", "output");
        assert_eq!(example.input, "input");
        assert_eq!(example.output, "output");
    }

    #[test]
    fn test_few_shot_add_multiple() {
        let examples = vec![Example::new("a", "1"), Example::new("b", "2")];

        let mut few_shot = FewShotPrompt::new("Test");
        few_shot.add_examples(examples);

        let prompt = few_shot.build();
        assert!(prompt.contains("Input: a"));
        assert!(prompt.contains("Input: b"));
    }
}
