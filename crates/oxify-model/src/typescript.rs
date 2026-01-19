//! TypeScript type definitions generator for OxiFY Model
//!
//! This module provides TypeScript type definitions for use with JavaScript/TypeScript
//! applications. It generates `.d.ts` files that provide type safety when working with
//! OxiFY workflows in TypeScript projects.
//!
//! # Features
//!
//! - Generate TypeScript interfaces from Rust types
//! - Full type coverage for workflows, nodes, edges, and execution types
//! - Compatible with the WASM bindings
//!
//! # Usage
//!
//! ```
//! use oxify_model::typescript::generate_typescript_definitions;
//!
//! let definitions = generate_typescript_definitions();
//! assert!(definitions.contains("interface Workflow"));
//! assert!(definitions.contains("interface Node"));
//! assert!(definitions.contains("interface Edge"));
//! // In production, save to file:
//! // std::fs::write("oxify-model.d.ts", definitions).unwrap();
//! ```

/// Generate complete TypeScript type definitions for OxiFY Model
///
/// Returns a string containing all TypeScript interface and type definitions
/// that can be saved to a `.d.ts` file.
pub fn generate_typescript_definitions() -> String {
    let mut output = String::new();

    output.push_str(HEADER);
    output.push_str(WORKFLOW_TYPES);
    output.push_str(NODE_TYPES);
    output.push_str(EDGE_TYPES);
    output.push_str(EXECUTION_TYPES);
    output.push_str(CONFIG_TYPES);
    output.push_str(UTILITY_TYPES);
    output.push_str(WASM_BINDINGS);

    output
}

const HEADER: &str = r#"/**
 * OxiFY Model - TypeScript Type Definitions
 *
 * Auto-generated TypeScript definitions for OxiFY workflow models.
 * These types are compatible with the WASM bindings provided by oxify-model.
 *
 * @version 0.1.0
 * @generated
 */

"#;

const WORKFLOW_TYPES: &str = r#"// ============================================================================
// Workflow Types
// ============================================================================

/** UUID string type for unique identifiers */
export type UUID = string;

/** ISO 8601 date-time string */
export type DateTime = string;

/** Unique identifier for a workflow */
export type WorkflowId = UUID;

/** Unique identifier for a node */
export type NodeId = UUID;

/** Unique identifier for an edge */
export type EdgeId = UUID;

/** Metadata about a workflow */
export interface WorkflowMetadata {
  /** Unique workflow identifier */
  id: WorkflowId;
  /** Display name */
  name: string;
  /** Description of what this workflow does */
  description?: string;
  /** Version string (semantic versioning) */
  version: string;
  /** When the workflow was created */
  created_at: DateTime;
  /** When the workflow was last modified */
  updated_at: DateTime;
  /** Tags for categorization */
  tags: string[];
  /** Parent workflow ID (for versioning) */
  parent_id?: WorkflowId;
  /** Change description for this version */
  change_description?: string;
  /** Scheduling configuration */
  schedule?: WorkflowSchedule;
}

/** Workflow scheduling configuration */
export interface WorkflowSchedule {
  /** Cron expression for scheduling */
  cron: string;
  /** Timezone (e.g., "UTC", "America/New_York") */
  timezone?: string;
  /** Whether the schedule is enabled */
  enabled: boolean;
}

/** Version bump type */
export type VersionBump = "major" | "minor" | "patch";

/** Complete workflow definition */
export interface Workflow {
  /** Workflow metadata */
  metadata: WorkflowMetadata;
  /** Nodes in the workflow DAG */
  nodes: Node[];
  /** Edges connecting nodes */
  edges: Edge[];
}

"#;

const NODE_TYPES: &str = r#"// ============================================================================
// Node Types
// ============================================================================

/** Node in the workflow DAG */
export interface Node {
  /** Unique node identifier */
  id: NodeId;
  /** Display name of the node */
  name: string;
  /** Type and configuration of the node */
  kind: NodeKind;
  /** Position in the visual editor */
  position?: [number, number];
  /** Retry configuration */
  retry_config?: RetryConfig;
  /** Timeout configuration */
  timeout_config?: TimeoutConfig;
}

/** Retry configuration for nodes */
export interface RetryConfig {
  /** Maximum number of retry attempts */
  max_retries: number;
  /** Initial delay before first retry in milliseconds */
  initial_delay_ms: number;
  /** Backoff multiplier for exponential backoff */
  backoff_multiplier: number;
  /** Maximum delay between retries in milliseconds */
  max_delay_ms: number;
}

/** Timeout configuration for nodes */
export interface TimeoutConfig {
  /** Maximum execution time in milliseconds */
  execution_timeout_ms: number;
  /** Idle timeout in milliseconds */
  idle_timeout_ms?: number;
  /** Action to take on timeout */
  timeout_action: TimeoutAction;
}

/** Action to take when a timeout occurs */
export type TimeoutAction = "Fail" | "Skip" | "UseDefault";

/** Node kind discriminated union */
export type NodeKind =
  | { Start: {} }
  | { End: {} }
  | { LlmCall: LlmConfig }
  | { Retriever: VectorConfig }
  | { CodeExecution: ScriptConfig }
  | { IfElse: Condition }
  | { Switch: SwitchConfig }
  | { Tool: McpConfig }
  | { Loop: LoopConfig }
  | { TryCatch: TryCatchConfig }
  | { SubWorkflow: SubWorkflowConfig }
  | { Parallel: ParallelConfig }
  | { Approval: ApprovalConfig }
  | { Form: FormConfig };

/** LLM call configuration */
export interface LlmConfig {
  /** LLM provider (e.g., "openai", "anthropic", "ollama") */
  provider: string;
  /** Model name (e.g., "gpt-4", "claude-3-opus") */
  model: string;
  /** System prompt template */
  system_prompt?: string;
  /** User prompt template with {{variable}} placeholders */
  prompt_template: string;
  /** Temperature for sampling (0.0 - 2.0) */
  temperature?: number;
  /** Maximum tokens to generate */
  max_tokens?: number;
  /** Stop sequences */
  stop_sequences?: string[];
  /** Response format (json, text) */
  response_format?: string;
}

/** Vector database configuration */
export interface VectorConfig {
  /** Database type (e.g., "qdrant", "pgvector") */
  db_type: string;
  /** Collection name */
  collection: string;
  /** Query template */
  query: string;
  /** Number of results to return */
  top_k: number;
  /** Minimum similarity score threshold */
  score_threshold?: number;
  /** Embedding model to use */
  embedding_model?: string;
}

/** Script execution configuration */
export interface ScriptConfig {
  /** Runtime environment */
  runtime: "rhai" | "wasm";
  /** Script code */
  code: string;
  /** Input variable mappings */
  input_mapping?: Record<string, string>;
  /** Output variable name */
  output_variable?: string;
}

/** Conditional node configuration */
export interface Condition {
  /** Boolean expression to evaluate */
  expression: string;
  /** Node ID for true branch */
  true_branch: NodeId;
  /** Node ID for false branch */
  false_branch: NodeId;
}

/** Switch/case configuration */
export interface SwitchConfig {
  /** Expression to switch on */
  switch_on: string;
  /** Case definitions */
  cases: SwitchCase[];
  /** Default case action */
  default_case?: string;
}

/** Single switch case */
export interface SwitchCase {
  /** Value to match */
  match_value: string;
  /** Action expression or node to execute */
  action: string;
}

/** MCP tool configuration */
export interface McpConfig {
  /** MCP server ID */
  server_id: string;
  /** Tool name */
  tool_name: string;
  /** Tool arguments template */
  arguments_template?: string;
}

/** Loop configuration */
export interface LoopConfig {
  /** Type of loop */
  loop_type: LoopType;
  /** Maximum iterations (safety limit) */
  max_iterations: number;
  /** Body expression to execute */
  body: string;
}

/** Loop type variants */
export type LoopType =
  | { ForEach: ForEachLoop }
  | { While: WhileLoop }
  | { Repeat: RepeatLoop };

/** ForEach loop configuration */
export interface ForEachLoop {
  /** Path to collection to iterate over */
  collection_path: string;
  /** Variable name for current item */
  item_variable: string;
  /** Variable name for current index */
  index_variable?: string;
  /** Body expression */
  body_expression: string;
  /** Enable parallel execution of loop iterations */
  parallel: boolean;
  /** Maximum number of concurrent iterations (only used when parallel=true) */
  max_concurrency?: number;
}

/** While loop configuration */
export interface WhileLoop {
  /** Condition expression */
  condition: string;
  /** Body expression */
  body_expression: string;
}

/** Repeat loop configuration */
export interface RepeatLoop {
  /** Number of times to repeat */
  count: number;
  /** Body expression */
  body_expression: string;
}

/** Try-catch configuration */
export interface TryCatchConfig {
  /** Node ID to try */
  try_node: NodeId;
  /** Node ID for catch handler */
  catch_node?: NodeId;
  /** Node ID for finally handler */
  finally_node?: NodeId;
  /** Variable name for error */
  error_variable?: string;
  /** Whether to rethrow after catch */
  rethrow?: boolean;
}

/** Sub-workflow configuration */
export interface SubWorkflowConfig {
  /** Path to sub-workflow file */
  workflow_path: string;
  /** Input variable mappings */
  input_mappings?: Record<string, string>;
  /** Output variable mappings */
  output_mappings?: Record<string, string>;
  /** Whether to inherit parent context */
  inherit_context?: boolean;
}

/** Parallel execution configuration */
export interface ParallelConfig {
  /** Tasks to execute in parallel */
  tasks: ParallelTask[];
  /** Execution strategy */
  strategy: ParallelStrategy;
  /** Timeout in milliseconds */
  timeout_ms?: number;
  /** Maximum concurrent tasks */
  max_concurrency?: number;
}

/** Single parallel task */
export interface ParallelTask {
  /** Task identifier */
  id: string;
  /** Expression to execute */
  expression: string;
}

/** Parallel execution strategy */
export type ParallelStrategy = "WaitAll" | "Race" | "AllSettled";

/** Approval node configuration */
export interface ApprovalConfig {
  /** Message to display for approval */
  message: string;
  /** Detailed description */
  description?: string;
  /** Required approvers (user IDs or roles) */
  required_approvers?: string[];
  /** Timeout in milliseconds */
  timeout_ms?: number;
  /** Context data for approval UI */
  context_data?: Record<string, unknown>;
}

/** Form node configuration */
export interface FormConfig {
  /** Form title */
  title: string;
  /** Form description */
  description?: string;
  /** Form fields */
  fields: FormField[];
  /** Submit button label */
  submit_label?: string;
  /** Allowed submitters */
  allowed_submitters?: string[];
  /** Timeout in milliseconds */
  timeout_ms?: number;
}

/** Form field definition */
export interface FormField {
  /** Field identifier */
  id: string;
  /** Field label */
  label: string;
  /** Field type */
  field_type: FormFieldType;
  /** Whether field is required */
  required: boolean;
  /** Default value */
  default_value?: unknown;
  /** Placeholder text */
  placeholder?: string;
  /** Help text */
  help_text?: string;
  /** Validation pattern (regex) */
  validation_pattern?: string;
  /** Options for select/radio fields */
  options?: string[];
}

/** Form field types */
export type FormFieldType =
  | "Text"
  | "Number"
  | "Email"
  | "Password"
  | "TextArea"
  | "Select"
  | "MultiSelect"
  | "Radio"
  | "Checkbox"
  | "Date"
  | "DateTime";

"#;

const EDGE_TYPES: &str = r#"// ============================================================================
// Edge Types
// ============================================================================

/** Edge connecting two nodes */
export interface Edge {
  /** Unique edge identifier */
  id: EdgeId;
  /** Source node ID */
  from: NodeId;
  /** Target node ID */
  to: NodeId;
  /** Condition for conditional edges */
  condition?: string;
  /** Edge label */
  label?: string;
}

"#;

const EXECUTION_TYPES: &str = r#"// ============================================================================
// Execution Types
// ============================================================================

/** Execution state */
export type ExecutionState =
  | "Pending"
  | "Running"
  | "Completed"
  | "Failed"
  | "Cancelled"
  | "Paused";

/** Unique identifier for an execution */
export type ExecutionId = UUID;

/** Execution context variables */
export type ExecutionContext = Record<string, unknown>;

/** Node execution result */
export interface NodeExecutionResult {
  /** Node ID */
  node_id: NodeId;
  /** Execution state */
  state: ExecutionState;
  /** Output value */
  output?: unknown;
  /** Error message if failed */
  error?: string;
  /** Execution duration in milliseconds */
  duration_ms?: number;
  /** Token usage for LLM nodes */
  token_usage?: TokenUsage;
  /** Number of retry attempts */
  retry_count?: number;
}

/** Token usage for LLM calls */
export interface TokenUsage {
  /** Input tokens */
  input_tokens: number;
  /** Output tokens */
  output_tokens: number;
  /** Total tokens */
  total_tokens: number;
}

/** Complete execution result */
export interface ExecutionResult {
  /** Execution ID */
  id: ExecutionId;
  /** Workflow ID */
  workflow_id: WorkflowId;
  /** Overall state */
  state: ExecutionState;
  /** Started at timestamp */
  started_at: DateTime;
  /** Completed at timestamp */
  completed_at?: DateTime;
  /** Node results */
  node_results: Record<string, NodeExecutionResult>;
  /** Final context */
  context: ExecutionContext;
  /** Total duration in milliseconds */
  duration_ms?: number;
  /** Total cost in USD */
  cost_usd?: number;
}

"#;

const CONFIG_TYPES: &str = r#"// ============================================================================
// Configuration Types
// ============================================================================

/** Cost estimate for a workflow */
export interface CostEstimate {
  /** Total estimated cost in USD */
  total_cost_usd: number;
  /** Cost breakdown by node */
  node_costs: NodeCost[];
  /** Cost breakdown by category */
  category_costs: CategoryCosts;
  /** Token estimates */
  token_estimates: TokenEstimates;
}

/** Cost for a single node */
export interface NodeCost {
  /** Node ID */
  node_id: NodeId;
  /** Node name */
  node_name: string;
  /** Estimated cost in USD */
  cost_usd: number;
  /** Cost components */
  components: CostComponent[];
}

/** Cost component */
export interface CostComponent {
  /** Component name */
  name: string;
  /** Cost in USD */
  cost_usd: number;
  /** Quantity */
  quantity?: number;
  /** Unit */
  unit?: string;
}

/** Costs by category */
export interface CategoryCosts {
  /** LLM costs */
  llm: number;
  /** Vector search costs */
  vector: number;
  /** Tool/API costs */
  tool: number;
  /** Other costs */
  other: number;
}

/** Token estimates */
export interface TokenEstimates {
  /** Estimated input tokens */
  input_tokens: number;
  /** Estimated output tokens */
  output_tokens: number;
  /** Total estimated tokens */
  total_tokens: number;
}

/** Time estimate for a workflow */
export interface TimeEstimate {
  /** Total estimated time in milliseconds */
  total_ms: number;
  /** Time per node */
  node_times: NodeTime[];
  /** Critical path */
  critical_path: NodeId[];
  /** Estimated parallel speedup factor */
  parallel_speedup: number;
}

/** Time estimate for a single node */
export interface NodeTime {
  /** Node ID */
  node_id: NodeId;
  /** Node name */
  node_name: string;
  /** Estimated time in milliseconds */
  estimated_ms: number;
  /** Minimum time in milliseconds */
  min_ms?: number;
  /** Maximum time in milliseconds */
  max_ms?: number;
}

"#;

const UTILITY_TYPES: &str = r#"// ============================================================================
// Utility Types
// ============================================================================

/** Validation error */
export interface ValidationError {
  /** Error code */
  code: string;
  /** Error message */
  message: string;
  /** Related node ID */
  node_id?: NodeId;
  /** Related field */
  field?: string;
}

/** Validation report */
export interface ValidationReport {
  /** Whether validation passed */
  is_valid: boolean;
  /** Validation errors */
  errors: ValidationError[];
  /** Validation warnings */
  warnings: ValidationError[];
  /** Validation statistics */
  stats: ValidationStats;
}

/** Validation statistics */
export interface ValidationStats {
  /** Number of nodes */
  node_count: number;
  /** Number of edges */
  edge_count: number;
  /** Workflow depth */
  max_depth: number;
  /** Has cycles */
  has_cycles: boolean;
}

"#;

const WASM_BINDINGS: &str = r#"// ============================================================================
// WASM Bindings
// ============================================================================

/**
 * WASM-based workflow manipulation.
 * Use with oxify-model compiled with `wasm-pack build --features wasm`
 */
export interface WasmWorkflow {
  /** Get workflow as JSON string */
  to_json(): string;
  /** Get workflow as YAML string */
  to_yaml(): string;
  /** Get workflow ID */
  id(): string;
  /** Get workflow name */
  name(): string;
  /** Get workflow version */
  version(): string;
  /** Get node count */
  node_count(): number;
  /** Get edge count */
  edge_count(): number;
  /** Validate workflow */
  validate(): string;
}

/**
 * Fluent builder for creating workflows.
 */
export interface WasmWorkflowBuilder {
  /** Set workflow description */
  description(desc: string): WasmWorkflowBuilder;
  /** Set workflow version */
  version(version: string): WasmWorkflowBuilder;
  /** Add a tag */
  tag(tag: string): WasmWorkflowBuilder;
  /** Add start node */
  start_node(name: string): WasmWorkflowBuilder;
  /** Add end node */
  end_node(name: string): WasmWorkflowBuilder;
  /** Add LLM node */
  llm_node(name: string, provider: string, model: string, prompt: string): WasmWorkflowBuilder;
  /** Add code node */
  code_node(name: string, code: string): WasmWorkflowBuilder;
  /** Add retriever node */
  retriever_node(name: string, db_type: string, collection: string, query: string, top_k: number): WasmWorkflowBuilder;
  /** Add if-else node */
  if_else_node(name: string, expression: string, true_branch: string, false_branch: string): WasmWorkflowBuilder;
  /** Add switch node */
  switch_node(name: string, expression: string): WasmWorkflowBuilder;
  /** Add tool node */
  tool_node(name: string, server_id: string, tool_name: string): WasmWorkflowBuilder;
  /** Add foreach loop node */
  foreach_node(name: string, collection_path: string, item_var: string, body: string): WasmWorkflowBuilder;
  /** Add while loop node */
  while_node(name: string, condition: string, body: string, max_iterations: number): WasmWorkflowBuilder;
  /** Add repeat loop node */
  repeat_node(name: string, count: number, body: string): WasmWorkflowBuilder;
  /** Connect two nodes */
  connect(from_name: string, to_name: string): WasmWorkflowBuilder;
  /** Build the workflow */
  build(): WasmWorkflow;
}

/**
 * Utility functions for working with workflows.
 */
export interface WasmWorkflowUtils {
  /** Generate a new UUID */
  generate_uuid(): string;
  /** Convert JSON to YAML */
  json_to_yaml(json: string): string;
  /** Convert YAML to JSON */
  yaml_to_json(yaml: string): string;
}

/** Create a new workflow from JSON */
export function workflow_from_json(json: string): WasmWorkflow;

/** Create a new workflow from YAML */
export function workflow_from_yaml(yaml: string): WasmWorkflow;

/** Create a new workflow builder */
export function new_workflow_builder(name: string): WasmWorkflowBuilder;

/** Get workflow utilities */
export function workflow_utils(): WasmWorkflowUtils;
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_typescript_definitions() {
        let definitions = generate_typescript_definitions();
        assert!(!definitions.is_empty());
        assert!(definitions.contains("export interface Workflow"));
        assert!(definitions.contains("export interface Node"));
        assert!(definitions.contains("export interface Edge"));
        assert!(definitions.contains("export type NodeKind"));
        assert!(definitions.contains("export interface LlmConfig"));
        assert!(definitions.contains("export interface VectorConfig"));
    }

    #[test]
    fn test_definitions_contain_all_node_types() {
        let definitions = generate_typescript_definitions();
        assert!(definitions.contains("LlmCall"));
        assert!(definitions.contains("Retriever"));
        assert!(definitions.contains("CodeExecution"));
        assert!(definitions.contains("IfElse"));
        assert!(definitions.contains("Switch"));
        assert!(definitions.contains("Loop"));
        assert!(definitions.contains("TryCatch"));
        assert!(definitions.contains("SubWorkflow"));
        assert!(definitions.contains("Parallel"));
        assert!(definitions.contains("Approval"));
        assert!(definitions.contains("Form"));
    }

    #[test]
    fn test_definitions_contain_execution_types() {
        let definitions = generate_typescript_definitions();
        assert!(definitions.contains("ExecutionState"));
        assert!(definitions.contains("ExecutionResult"));
        assert!(definitions.contains("NodeExecutionResult"));
        assert!(definitions.contains("TokenUsage"));
    }

    #[test]
    fn test_definitions_contain_wasm_bindings() {
        let definitions = generate_typescript_definitions();
        assert!(definitions.contains("WasmWorkflow"));
        assert!(definitions.contains("WasmWorkflowBuilder"));
        assert!(definitions.contains("WasmWorkflowUtils"));
        assert!(definitions.contains("workflow_from_json"));
        assert!(definitions.contains("new_workflow_builder"));
    }

    #[test]
    fn test_definitions_contain_config_types() {
        let definitions = generate_typescript_definitions();
        assert!(definitions.contains("CostEstimate"));
        assert!(definitions.contains("TimeEstimate"));
        assert!(definitions.contains("ValidationReport"));
    }

    #[test]
    fn test_definitions_valid_typescript_syntax() {
        let definitions = generate_typescript_definitions();
        // Check for proper TypeScript syntax patterns
        assert!(definitions.contains("export interface"));
        assert!(definitions.contains("export type"));
        assert!(definitions.contains(": string"));
        assert!(definitions.contains(": number"));
        assert!(definitions.contains("?: ")); // Optional fields
        assert!(definitions.contains("Record<")); // Generic types
    }
}
