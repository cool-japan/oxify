# oxify-model

**The Brain** - Domain Models for OxiFY LLM Workflow Orchestration

## Overview

`oxify-model` provides the core data structures for defining and managing LLM workflows as directed acyclic graphs (DAGs). These models are the foundation of OxiFY's **Type-Safe Workflow Engine**, leveraging Rust's type system to guarantee workflow correctness at compile time.

**Status**: ✅ Production Ready with Advanced Optimization Features
**Part of**: OxiFY Enterprise Architecture (Codename: Absolute Zero)

## Features

- 🎯 **Type-Safe Workflow Definition**: DAG-based workflows with compile-time guarantees
- 💰 **Cost Estimation**: Predict LLM execution costs before running
- ⏱️ **Time Prediction**: Estimate workflow execution time with confidence scores
- 🔍 **Workflow Optimization**: Automatic detection of optimization opportunities
- ⚡ **Batch Analysis**: Identify parallelization opportunities for faster execution
- 🧠 **Variable Optimization**: Minimize memory usage and eliminate unnecessary copies
- 📊 **Comprehensive Analytics**: Track execution stats, performance metrics, and trends
- 🔐 **Enterprise Features**: Versioning, checkpointing, secrets management, webhooks

## Key Types

### Workflow

Represents a complete workflow definition:

```rust
pub struct Workflow {
    pub metadata: WorkflowMetadata,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
```

### Node Types

```rust
pub enum NodeKind {
    Start,                      // Entry point
    End,                        // Exit point
    LLM(LlmConfig),            // LLM invocation (OpenAI, Anthropic, Ollama)
    Retriever(VectorConfig),   // Vector DB query (Qdrant, pgvector)
    Code(ScriptConfig),        // Code execution (Rust, WASM)
    IfElse(Condition),         // Conditional branching
    Switch(SwitchConfig),      // Multi-way branching
    Tool(McpConfig),           // MCP tool invocation / HTTP calls
    Loop(LoopConfig),          // ForEach, While, Repeat loops
    TryCatch(TryCatchConfig),  // Error handling
    SubWorkflow(SubWorkflowConfig), // Nested workflows
    Parallel(ParallelConfig),  // Parallel execution
    Approval(ApprovalConfig),  // Human-in-the-loop gates
    Form(FormConfig),          // User input forms
}
```

### Execution Context

Tracks workflow execution state:

```rust
pub struct ExecutionContext {
    pub execution_id: ExecutionId,
    pub workflow_id: WorkflowId,
    pub started_at: DateTime<Utc>,
    pub state: ExecutionState,
    pub node_results: HashMap<NodeId, NodeExecutionResult>,
    pub variables: HashMap<String, serde_json::Value>,
}
```

## Usage

### Creating a Workflow

```rust
use oxify_model::*;

let mut workflow = Workflow::new("My Workflow".to_string());

// Add start node
let start = Node::new("Start".to_string(), NodeKind::Start);
workflow.add_node(start.clone());

// Add LLM node
let llm = Node::new("LLM".to_string(), NodeKind::LLM(LlmConfig {
    provider: "openai".to_string(),
    model: "gpt-4".to_string(),
    system_prompt: Some("You are a helpful assistant.".to_string()),
    prompt_template: "{{user_input}}".to_string(),
    temperature: Some(0.7),
    max_tokens: Some(1000),
    extra_params: serde_json::Value::Null,
}));
workflow.add_node(llm.clone());

// Add end node
let end = Node::new("End".to_string(), NodeKind::End);
workflow.add_node(end.clone());

// Connect nodes
workflow.add_edge(Edge::new(start.id, llm.id));
workflow.add_edge(Edge::new(llm.id, end.id));

// Validate workflow
workflow.validate()?;
```

### Serialization

All types are serializable to/from JSON:

```rust
// To JSON
let json = serde_json::to_string_pretty(&workflow)?;

// From JSON
let workflow: Workflow = serde_json::from_str(&json)?;
```

## Node Configuration Details

### LlmConfig

```rust
pub struct LlmConfig {
    pub provider: String,        // "openai", "anthropic", "local"
    pub model: String,           // "gpt-4", "claude-3-opus"
    pub system_prompt: Option<String>,
    pub prompt_template: String, // Supports {{variable}} syntax
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub extra_params: serde_json::Value,
}
```

### VectorConfig

```rust
pub struct VectorConfig {
    pub db_type: String,         // "qdrant", "pgvector"
    pub collection: String,
    pub query: String,
    pub top_k: usize,
    pub score_threshold: Option<f64>,
}
```

### ScriptConfig

```rust
pub struct ScriptConfig {
    pub runtime: String,         // "rust", "wasm"
    pub code: String,
    pub inputs: Vec<String>,
    pub output: String,
}
```

## Validation

Workflows can be validated for:
- At least one Start node exists
- All edge references point to valid nodes
- No cycles in the graph (DAG property)

```rust
if let Err(e) = workflow.validate() {
    println!("Workflow validation failed: {}", e);
}
```

## Optimization Features

### Cost Estimation

Predict workflow execution costs before running:

```rust
use oxify_model::{CostEstimator, WorkflowBuilder};

let workflow = WorkflowBuilder::new("RAG Pipeline")
    .start("Start")
    .llm("Generate", llm_config)
    .retriever("Search", vector_config)
    .end("End")
    .build();

let estimate = CostEstimator::estimate(&workflow);
println!("{}", estimate.format_summary());
// Output:
// Total Cost: $0.0125
// LLM: $0.0120 | Vector: $0.0005
// Tokens: 1250 input, 1000 output (2250 total)
```

### Time Prediction

Estimate execution time with confidence scores:

```rust
use oxify_model::TimePredictor;

let predictor = TimePredictor::new();
let estimate = predictor.predict(&workflow);
println!("{}", estimate.format_summary());
// Output:
// Estimated Time: 2s - 30s (avg: 8s)
// Critical Path: Start → Generate → Search → End
// Confidence: 75%
```

### Workflow Optimization

Automatically detect optimization opportunities:

```rust
use oxify_model::WorkflowOptimizer;

let report = WorkflowOptimizer::analyze(&workflow);
println!("{}", report.format_summary());
// Output:
// Optimization Score: 65%
// Potential Savings: $0.0083 | 2500ms
// Opportunities: 2 parallelization, 1 redundant nodes

for suggestion in report.high_priority_suggestions() {
    println!("  [{:?}] {}", suggestion.severity, suggestion.description);
}
```

### Batch Analysis

Identify parallelization opportunities:

```rust
use oxify_model::BatchAnalyzer;

let plan = BatchAnalyzer::analyze(&workflow);
println!("{}", plan.format_summary());
// Output:
// Batch Execution Plan:
// Total Nodes: 10 | Batches: 5 | Max Parallelism: 4
// Speedup Factor: 2.5x | Efficiency: 60%
```

### Variable Optimization

Minimize memory usage and eliminate unnecessary copies:

```rust
use oxify_model::VariableOptimizer;

let analysis = VariableOptimizer::analyze(&workflow);
println!("{}", analysis.format_summary());
// Output:
// Variable Optimization Analysis:
// Total Variable Flows: 8 | Tracked Variables: 5
// Optimization Opportunities: 3 | Unnecessary Copies: 1
// Estimated Memory Savings: 12 KB
```

## Performance

All optimization features are benchmarked for performance:

- **Cost Estimation**: ~4.7μs for 10 nodes, ~63μs for 100 nodes
- **Time Prediction**: ~4.3μs for 10 nodes, ~52μs for 100 nodes
- **Optimization Analysis**: ~20μs for 10 nodes, ~192μs for 100 nodes

## Testing

- **177 tests** covering all modules
- Property-based tests with `proptest`
- Comprehensive integration tests
- Zero warnings with `cargo clippy`

## See Also

- `oxify-engine`: DAG execution engine
- `oxify-api`: REST API for workflow management
- `oxify-cli`: Local workflow runner and development tool
