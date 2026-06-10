use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Unique identifier for a node
pub type NodeId = Uuid;

/// Node in the workflow DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Node {
    /// Unique node identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: NodeId,

    /// Display name of the node
    pub name: String,

    /// Type and configuration of the node
    pub kind: NodeKind,

    /// Position in the visual editor (optional)
    pub position: Option<(f64, f64)>,

    /// Retry configuration for this node (optional)
    #[serde(default)]
    pub retry_config: Option<RetryConfig>,

    /// Timeout configuration for this node (optional)
    #[serde(default)]
    pub timeout_config: Option<TimeoutConfig>,
}

/// Retry configuration for nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    pub max_retries: u32,

    /// Initial delay before first retry in milliseconds (default: 1000ms)
    pub initial_delay_ms: u64,

    /// Backoff multiplier for exponential backoff (default: 2.0)
    pub backoff_multiplier: f64,

    /// Maximum delay between retries in milliseconds (default: 30000ms = 30s)
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }
}

/// Timeout configuration for nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TimeoutConfig {
    /// Maximum execution time in milliseconds
    pub execution_timeout_ms: u64,

    /// Idle timeout - max time with no progress in milliseconds (optional)
    #[serde(default)]
    pub idle_timeout_ms: Option<u64>,

    /// Action to take on timeout
    #[serde(default)]
    pub timeout_action: TimeoutAction,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            execution_timeout_ms: 60000, // 1 minute default
            idle_timeout_ms: None,
            timeout_action: TimeoutAction::Fail,
        }
    }
}

/// Action to take when a timeout occurs
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum TimeoutAction {
    /// Fail the node execution (default)
    #[default]
    Fail,

    /// Skip this node and continue with default/empty output
    Skip,

    /// Use a default value for the output
    UseDefault(String),
}

impl Node {
    pub fn new(name: String, kind: NodeKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            kind,
            position: None,
            retry_config: None,
            timeout_config: None,
        }
    }

    pub fn with_retry(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = Some(retry_config);
        self
    }

    pub fn with_timeout(mut self, timeout_config: TimeoutConfig) -> Self {
        self.timeout_config = Some(timeout_config);
        self
    }

    pub fn with_position(mut self, x: f64, y: f64) -> Self {
        self.position = Some((x, y));
        self
    }
}

/// Types of nodes available in the workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(tag = "type", content = "config")]
pub enum NodeKind {
    /// Start node - entry point of the workflow
    Start,

    /// End node - exit point of the workflow
    End,

    /// LLM invocation node
    LLM(LlmConfig),

    /// Vector database retrieval node
    Retriever(VectorConfig),

    /// Code execution node (Rust script or Wasm)
    Code(ScriptConfig),

    /// Conditional branching node
    IfElse(Condition),

    /// MCP (Model Context Protocol) tool invocation
    Tool(McpConfig),

    /// Loop/iteration node
    Loop(LoopConfig),

    /// Error handling node (try-catch-finally)
    TryCatch(TryCatchConfig),

    /// Sub-workflow execution node
    SubWorkflow(SubWorkflowConfig),

    /// Switch/Case multi-branch routing
    Switch(SwitchConfig),

    /// Parallel execution node (fan-out/fan-in)
    Parallel(ParallelConfig),

    /// Human approval node (wait for approval before continuing)
    Approval(ApprovalConfig),

    /// Human form input node (wait for form submission)
    Form(FormConfig),

    /// Vision/OCR node for image text extraction
    Vision(VisionConfig),

    /// Custom plugin node — dispatches to a registered NodePlugin
    Custom(CustomConfig),
}

/// Configuration for custom plugin nodes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CustomConfig {
    /// Plugin identifier — must match a registered NodePlugin::name()
    pub plugin_id: String,

    /// Optional minimum version constraint (semver)
    pub plugin_version: Option<String>,

    /// Plugin-specific configuration passed to execute()
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Configuration for LLM nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct LlmConfig {
    /// Provider (e.g., "openai", "anthropic", "local")
    pub provider: String,

    /// Model identifier (e.g., "gpt-4", "claude-3-opus")
    pub model: String,

    /// System prompt template
    pub system_prompt: Option<String>,

    /// User prompt template (can reference previous node outputs)
    pub prompt_template: String,

    /// Temperature for sampling (0.0 - 2.0)
    pub temperature: Option<f64>,

    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,

    /// Tools/functions available for the LLM to call
    #[serde(default)]
    pub tools: Vec<serde_json::Value>,

    /// Images for vision models (multimodal input)
    #[serde(default)]
    pub images: Vec<serde_json::Value>,

    /// Additional provider-specific parameters
    #[serde(default)]
    pub extra_params: serde_json::Value,
}

/// Configuration for vector database retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct VectorConfig {
    /// Vector database type (e.g., "qdrant", "pgvector")
    pub db_type: String,

    /// Collection/table name
    pub collection: String,

    /// Query text or reference to previous node output
    pub query: String,

    /// Number of results to retrieve
    pub top_k: usize,

    /// Minimum similarity score threshold
    pub score_threshold: Option<f64>,
}

/// Configuration for code execution nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ScriptConfig {
    /// Script language/runtime ("rust", "wasm")
    pub runtime: String,

    /// The script code to execute
    pub code: String,

    /// Input variable bindings
    #[serde(default)]
    pub inputs: Vec<String>,

    /// Expected output variable name
    pub output: String,
}

/// Configuration for conditional nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Condition {
    /// JavaScript-like expression to evaluate
    pub expression: String,

    /// Node to execute if condition is true
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub true_branch: NodeId,

    /// Node to execute if condition is false
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub false_branch: NodeId,
}

/// Configuration for MCP tool invocation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct McpConfig {
    /// MCP server identifier
    pub server_id: String,

    /// Tool name to invoke
    pub tool_name: String,

    /// Tool parameters (can reference previous outputs)
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Configuration for loop nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct LoopConfig {
    /// Type of loop to execute
    pub loop_type: LoopType,

    /// Maximum iterations allowed (safety limit)
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_max_iterations() -> usize {
    1000
}

/// Types of loops supported
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(tag = "variant")]
pub enum LoopType {
    /// Iterate over a collection (like map/forEach)
    ForEach {
        /// Path to array variable in context (e.g., "items", "results")
        collection_path: String,

        /// Variable name to bind each item (e.g., "item")
        item_variable: String,

        /// Optional index variable (e.g., "index")
        #[serde(default)]
        index_variable: Option<String>,

        /// Expression or template to execute for each item
        /// Can reference {{item}}, {{index}}, and other context variables
        body_expression: String,

        /// Enable parallel execution of loop iterations
        #[serde(default)]
        parallel: bool,

        /// Maximum number of concurrent iterations (only used when parallel=true)
        /// If None, uses default concurrency limit
        #[serde(default)]
        max_concurrency: Option<usize>,
    },

    /// Iterate while condition is true
    While {
        /// Condition expression (evaluated each iteration)
        condition: String,

        /// Expression to execute each iteration
        body_expression: String,

        /// Optional counter variable name
        #[serde(default)]
        counter_variable: Option<String>,
    },

    /// Repeat N times
    Repeat {
        /// Number of iterations (can be template like "{{count}}")
        count: String,

        /// Expression to execute each iteration
        body_expression: String,

        /// Variable name for iteration index (0-based)
        #[serde(default)]
        index_variable: Option<String>,
    },
}

/// Configuration for try-catch error handling nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TryCatchConfig {
    /// Expression or template to try executing
    pub try_expression: String,

    /// Optional expression to execute if try fails
    /// Can access {{error}} variable containing the error message
    #[serde(default)]
    pub catch_expression: Option<String>,

    /// Optional expression to always execute (after try or catch)
    #[serde(default)]
    pub finally_expression: Option<String>,

    /// Whether to re-throw the error after catch (default: false)
    /// If true, the node will still fail even after executing catch
    #[serde(default)]
    pub rethrow: bool,

    /// Variable name to store the error in catch block (default: "error")
    #[serde(default = "default_error_variable")]
    pub error_variable: String,
}

fn default_error_variable() -> String {
    "error".to_string()
}

/// Configuration for sub-workflow execution nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SubWorkflowConfig {
    /// Path to JSON file containing the workflow to execute
    pub workflow_path: String,

    /// Input mappings: map parent context variables to sub-workflow variables
    /// Format: {"sub_var_name": "{{parent_var_name}}"}
    #[serde(default)]
    pub input_mappings: std::collections::HashMap<String, String>,

    /// Output variable name to extract from sub-workflow results
    /// If not specified, all sub-workflow results are returned
    #[serde(default)]
    pub output_variable: Option<String>,

    /// Whether to inherit parent context variables (default: false)
    #[serde(default)]
    pub inherit_context: bool,
}

/// Configuration for switch/case multi-branch routing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SwitchConfig {
    /// Expression to evaluate for routing (e.g., "{{status}}", "{{node_x.result}}")
    pub switch_on: String,

    /// List of cases to match against
    pub cases: Vec<SwitchCase>,

    /// Default case if no matches (optional)
    /// If None and no match, the node fails
    #[serde(default)]
    pub default_case: Option<String>,
}

/// A single case in a switch statement
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SwitchCase {
    /// Value to match (e.g., "success", "error", "pending")
    /// Supports exact match or regex if prefixed with "regex:"
    pub match_value: String,

    /// Expression or action to execute if matched
    /// Can be a simple value or template expression
    pub action: String,
}

/// Configuration for parallel execution node
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ParallelConfig {
    /// Parallel execution strategy
    pub strategy: ParallelStrategy,

    /// List of expressions/tasks to execute in parallel
    /// Each can reference context variables
    pub tasks: Vec<ParallelTask>,

    /// Maximum number of concurrent tasks (default: no limit)
    #[serde(default)]
    pub max_concurrency: Option<usize>,

    /// Timeout for all tasks in milliseconds (default: no timeout)
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Strategy for parallel execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum ParallelStrategy {
    /// Wait for all tasks to complete (fan-out/fan-in)
    /// Fails if any task fails
    WaitAll,

    /// Wait for first task to complete successfully
    /// Ignore other tasks once one succeeds
    Race,

    /// Wait for all tasks, but don't fail if some fail
    /// Collect both successes and failures
    AllSettled,
}

/// A task to execute in parallel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ParallelTask {
    /// Task identifier (used for result mapping)
    pub id: String,

    /// Expression or template to execute
    pub expression: String,

    /// Optional description of what this task does
    #[serde(default)]
    pub description: Option<String>,
}

/// Configuration for approval nodes (human-in-the-loop)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ApprovalConfig {
    /// Message to show to the approver
    pub message: String,

    /// Description of what is being approved
    pub description: Option<String>,

    /// Required approvers (user IDs or roles)
    #[serde(default)]
    pub approvers: Vec<String>,

    /// Timeout in seconds (if no approval, workflow fails)
    pub timeout_seconds: Option<u64>,

    /// Data to show in approval UI (e.g., context, results)
    #[serde(default)]
    pub context_data: serde_json::Value,
}

/// Configuration for form input nodes (human-in-the-loop)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FormConfig {
    /// Form title
    pub title: String,

    /// Form description/instructions
    pub description: Option<String>,

    /// Form fields to collect
    pub fields: Vec<FormField>,

    /// Timeout in seconds (if no submission, workflow fails)
    pub timeout_seconds: Option<u64>,

    /// Who can submit this form (user IDs or roles)
    #[serde(default)]
    pub allowed_submitters: Vec<String>,
}

/// A field in a form
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FormField {
    /// Field identifier (used in result mapping)
    pub id: String,

    /// Field label shown to user
    pub label: String,

    /// Field type (text, number, email, select, etc.)
    pub field_type: FormFieldType,

    /// Whether this field is required
    #[serde(default)]
    pub required: bool,

    /// Default value
    pub default_value: Option<serde_json::Value>,

    /// Validation rules (regex, min, max, etc.)
    #[serde(default)]
    pub validation: Option<serde_json::Value>,

    /// For select/radio fields, the available options
    #[serde(default)]
    pub options: Vec<String>,
}

/// Types of form fields
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum FormFieldType {
    Text,
    Number,
    Email,
    Password,
    TextArea,
    Select,
    MultiSelect,
    Radio,
    Checkbox,
    Date,
    DateTime,
}

/// Configuration for Vision/OCR nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct VisionConfig {
    /// OCR provider (e.g., "mock", "tesseract", "surya", "paddle")
    pub provider: String,

    /// Path to model files (for ONNX-based providers)
    #[serde(default)]
    pub model_path: Option<String>,

    /// Output format preference ("text", "markdown", "json")
    #[serde(default = "default_output_format")]
    pub output_format: String,

    /// Whether to use GPU acceleration
    #[serde(default)]
    pub use_gpu: bool,

    /// Target language(s) for OCR (e.g., "eng", "jpn", "eng+jpn")
    #[serde(default)]
    pub language: Option<String>,

    /// Image input reference (variable name containing image data or path)
    pub image_input: String,

    /// Additional provider-specific options
    #[serde(default)]
    pub options: serde_json::Value,
}

fn default_output_format() -> String {
    "markdown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = Node::new(
            "Test LLM".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "Hello {{input}}".to_string(),
                temperature: Some(0.7),
                max_tokens: Some(1000),
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        )
        .with_position(100.0, 200.0);

        assert_eq!(node.name, "Test LLM");
        assert_eq!(node.position, Some((100.0, 200.0)));
    }

    #[test]
    fn test_switch_node() {
        let switch_config = SwitchConfig {
            switch_on: "{{status}}".to_string(),
            cases: vec![
                SwitchCase {
                    match_value: "success".to_string(),
                    action: "process_success".to_string(),
                },
                SwitchCase {
                    match_value: "error".to_string(),
                    action: "handle_error".to_string(),
                },
            ],
            default_case: Some("unknown_status".to_string()),
        };

        let node = Node::new(
            "Status Router".to_string(),
            NodeKind::Switch(switch_config.clone()),
        );

        assert_eq!(node.name, "Status Router");
        if let NodeKind::Switch(config) = &node.kind {
            assert_eq!(config.switch_on, "{{status}}");
            assert_eq!(config.cases.len(), 2);
            assert_eq!(config.default_case, Some("unknown_status".to_string()));
        } else {
            panic!("Expected Switch node");
        }
    }

    #[test]
    fn test_parallel_node() {
        let parallel_config = ParallelConfig {
            strategy: ParallelStrategy::WaitAll,
            tasks: vec![
                ParallelTask {
                    id: "task1".to_string(),
                    expression: "{{query1}}".to_string(),
                    description: Some("First query".to_string()),
                },
                ParallelTask {
                    id: "task2".to_string(),
                    expression: "{{query2}}".to_string(),
                    description: Some("Second query".to_string()),
                },
            ],
            max_concurrency: Some(2),
            timeout_ms: Some(30000),
        };

        let node = Node::new(
            "Parallel Execution".to_string(),
            NodeKind::Parallel(parallel_config.clone()),
        );

        assert_eq!(node.name, "Parallel Execution");
        if let NodeKind::Parallel(config) = &node.kind {
            assert_eq!(config.strategy, ParallelStrategy::WaitAll);
            assert_eq!(config.tasks.len(), 2);
            assert_eq!(config.max_concurrency, Some(2));
            assert_eq!(config.timeout_ms, Some(30000));
        } else {
            panic!("Expected Parallel node");
        }
    }

    #[test]
    fn test_parallel_strategy_race() {
        let parallel_config = ParallelConfig {
            strategy: ParallelStrategy::Race,
            tasks: vec![
                ParallelTask {
                    id: "fast".to_string(),
                    expression: "{{fast_api}}".to_string(),
                    description: None,
                },
                ParallelTask {
                    id: "slow".to_string(),
                    expression: "{{slow_api}}".to_string(),
                    description: None,
                },
            ],
            max_concurrency: None,
            timeout_ms: None,
        };

        let node = Node::new(
            "Race Condition".to_string(),
            NodeKind::Parallel(parallel_config),
        );

        if let NodeKind::Parallel(config) = &node.kind {
            assert_eq!(config.strategy, ParallelStrategy::Race);
        } else {
            panic!("Expected Parallel node");
        }
    }

    #[test]
    fn test_approval_node() {
        let approval_config = ApprovalConfig {
            message: "Please approve this action".to_string(),
            description: Some("This will deploy to production".to_string()),
            approvers: vec!["admin".to_string(), "manager".to_string()],
            timeout_seconds: Some(3600),
            context_data: serde_json::json!({
                "deployment": "production",
                "version": "1.0.0"
            }),
        };

        let node = Node::new(
            "Production Approval".to_string(),
            NodeKind::Approval(approval_config.clone()),
        );

        assert_eq!(node.name, "Production Approval");
        if let NodeKind::Approval(config) = &node.kind {
            assert_eq!(config.message, "Please approve this action");
            assert_eq!(config.approvers.len(), 2);
            assert_eq!(config.timeout_seconds, Some(3600));
        } else {
            panic!("Expected Approval node");
        }
    }

    #[test]
    fn test_form_node() {
        let form_config = FormConfig {
            title: "User Information".to_string(),
            description: Some("Please provide your details".to_string()),
            fields: vec![
                FormField {
                    id: "name".to_string(),
                    label: "Full Name".to_string(),
                    field_type: FormFieldType::Text,
                    required: true,
                    default_value: None,
                    validation: None,
                    options: vec![],
                },
                FormField {
                    id: "email".to_string(),
                    label: "Email Address".to_string(),
                    field_type: FormFieldType::Email,
                    required: true,
                    default_value: None,
                    validation: None,
                    options: vec![],
                },
                FormField {
                    id: "age".to_string(),
                    label: "Age".to_string(),
                    field_type: FormFieldType::Number,
                    required: false,
                    default_value: None,
                    validation: Some(serde_json::json!({"min": 18, "max": 100})),
                    options: vec![],
                },
            ],
            timeout_seconds: Some(600),
            allowed_submitters: vec!["user1".to_string()],
        };

        let node = Node::new("User Form".to_string(), NodeKind::Form(form_config.clone()));

        assert_eq!(node.name, "User Form");
        if let NodeKind::Form(config) = &node.kind {
            assert_eq!(config.title, "User Information");
            assert_eq!(config.fields.len(), 3);
            assert_eq!(config.fields[0].field_type, FormFieldType::Text);
            assert_eq!(config.fields[1].field_type, FormFieldType::Email);
            assert_eq!(config.fields[2].field_type, FormFieldType::Number);
            assert!(config.fields[0].required);
            assert!(!config.fields[2].required);
        } else {
            panic!("Expected Form node");
        }
    }

    #[test]
    fn test_form_field_types() {
        let field_types = vec![
            FormFieldType::Text,
            FormFieldType::Number,
            FormFieldType::Email,
            FormFieldType::Password,
            FormFieldType::TextArea,
            FormFieldType::Select,
            FormFieldType::MultiSelect,
            FormFieldType::Radio,
            FormFieldType::Checkbox,
            FormFieldType::Date,
            FormFieldType::DateTime,
        ];

        // Test that all field types can be created and compared
        for field_type in field_types {
            let _field = FormField {
                id: "test".to_string(),
                label: "Test".to_string(),
                field_type: field_type.clone(),
                required: false,
                default_value: None,
                validation: None,
                options: vec![],
            };
        }
    }

    #[test]
    fn test_node_with_retry_and_timeout() {
        let node = Node::new("Resilient Node".to_string(), NodeKind::Start)
            .with_retry(RetryConfig {
                max_retries: 5,
                initial_delay_ms: 500,
                backoff_multiplier: 3.0,
                max_delay_ms: 60000,
            })
            .with_timeout(TimeoutConfig {
                execution_timeout_ms: 10000,
                idle_timeout_ms: Some(5000),
                timeout_action: TimeoutAction::Skip,
            });

        assert!(node.retry_config.is_some());
        assert!(node.timeout_config.is_some());

        if let Some(retry) = &node.retry_config {
            assert_eq!(retry.max_retries, 5);
            assert_eq!(retry.backoff_multiplier, 3.0);
        }

        if let Some(timeout) = &node.timeout_config {
            assert_eq!(timeout.execution_timeout_ms, 10000);
            assert_eq!(timeout.timeout_action, TimeoutAction::Skip);
        }
    }

    #[test]
    fn test_foreach_parallel_execution() {
        let loop_config = LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: "items".to_string(),
                item_variable: "item".to_string(),
                index_variable: Some("idx".to_string()),
                body_expression: "process({{item}})".to_string(),
                parallel: true,
                max_concurrency: Some(10),
            },
            max_iterations: 1000,
        };

        let node = Node::new("Parallel Loop".to_string(), NodeKind::Loop(loop_config));

        if let NodeKind::Loop(config) = &node.kind {
            if let LoopType::ForEach {
                parallel,
                max_concurrency,
                collection_path,
                item_variable,
                ..
            } = &config.loop_type
            {
                assert!(parallel);
                assert_eq!(*max_concurrency, Some(10));
                assert_eq!(collection_path, "items");
                assert_eq!(item_variable, "item");
            } else {
                panic!("Expected ForEach loop");
            }
        } else {
            panic!("Expected Loop node");
        }
    }

    #[test]
    fn test_foreach_sequential_execution() {
        let loop_config = LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: "items".to_string(),
                item_variable: "item".to_string(),
                index_variable: None,
                body_expression: "process({{item}})".to_string(),
                parallel: false,
                max_concurrency: None,
            },
            max_iterations: 1000,
        };

        let node = Node::new("Sequential Loop".to_string(), NodeKind::Loop(loop_config));

        if let NodeKind::Loop(config) = &node.kind {
            if let LoopType::ForEach {
                parallel,
                max_concurrency,
                ..
            } = &config.loop_type
            {
                assert!(!parallel);
                assert_eq!(*max_concurrency, None);
            } else {
                panic!("Expected ForEach loop");
            }
        } else {
            panic!("Expected Loop node");
        }
    }

    #[test]
    fn test_foreach_serialization_with_parallel() {
        let loop_config = LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: "data".to_string(),
                item_variable: "x".to_string(),
                index_variable: Some("i".to_string()),
                body_expression: "{{x}} * 2".to_string(),
                parallel: true,
                max_concurrency: Some(5),
            },
            max_iterations: 100,
        };

        // Test JSON serialization
        let json = serde_json::to_string(&loop_config).unwrap();
        let deserialized: LoopConfig = serde_json::from_str(&json).unwrap();

        if let LoopType::ForEach {
            parallel,
            max_concurrency,
            ..
        } = deserialized.loop_type
        {
            assert!(parallel);
            assert_eq!(max_concurrency, Some(5));
        } else {
            panic!("Expected ForEach loop");
        }
    }
}
