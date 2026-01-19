//! Code Execution Workflow Example
//!
//! This example demonstrates code execution nodes in OxiFY workflows using
//! the Rhai scripting language. Rhai is a safe, embedded scripting language
//! designed for Rust with built-in security features like operation limits.
//!
//! Features demonstrated:
//! - Basic arithmetic operations
//! - String manipulation
//! - Array and map operations
//! - Data transformation pipelines
//! - Variable passing between nodes
//! - Resource limits and sandboxing
//!
//! Run this example:
//! ```bash
//! cargo run --example code_execution_workflow
//! ```

use oxify_engine::Engine;
use oxify_model::{Edge, ExecutionContext, ExecutionState, Node, NodeKind, ScriptConfig, Workflow};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== OxiFY Code Execution Workflow Example ===\n");
    println!("This example demonstrates Rhai script execution in workflows.\n");

    // Example 1: Basic Math Operations
    println!("--- Example 1: Basic Math Operations ---");
    run_math_example().await?;

    println!("\n" + &"=".repeat(60) + "\n");

    // Example 2: String Manipulation
    println!("--- Example 2: String Manipulation ---");
    run_string_example().await?;

    println!("\n" + &"=".repeat(60) + "\n");

    // Example 3: Array/Data Transformation
    println!("--- Example 3: Array/Data Transformation ---");
    run_array_example().await?;

    println!("\n" + &"=".repeat(60) + "\n");

    // Example 4: Multi-Stage Data Pipeline
    println!("--- Example 4: Multi-Stage Data Pipeline ---");
    run_pipeline_example().await?;

    println!("\n=== All code execution examples completed! ===");

    Ok(())
}

/// Example 1: Basic math operations with Rhai
async fn run_math_example() -> anyhow::Result<()> {
    let mut workflow = Workflow::new("Math Operations".to_string());
    workflow.metadata.description = Some("Demonstrates basic Rhai math operations".to_string());

    // Start node
    let start = Node::new("Start".to_string(), NodeKind::Start);
    let start_id = start.id;
    workflow.add_node(start);

    // Code node for math operations
    let math_code = Node::new(
        "Calculate".to_string(),
        NodeKind::Code(ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#"
// Rhai supports all standard math operations
let sum = x + y;
let diff = x - y;
let product = x * y;
let quotient = x / y;
let modulo = x % y;
let power = x * x;  // Rhai doesn't have ** operator, use multiplication

// Return a map with all results
#{
    "sum": sum,
    "difference": diff,
    "product": product,
    "quotient": quotient,
    "modulo": modulo,
    "x_squared": power,
    "input_x": x,
    "input_y": y
}
            "#
            .to_string(),
            inputs: vec!["x".to_string(), "y".to_string()],
            output: "math_result".to_string(),
        }),
    );
    let math_id = math_code.id;
    workflow.add_node(math_code);

    // End node
    let end = Node::new("End".to_string(), NodeKind::End);
    let end_id = end.id;
    workflow.add_node(end);

    // Connect nodes
    workflow.add_edge(Edge::new(start_id, math_id));
    workflow.add_edge(Edge::new(math_id, end_id));

    // Execute with input variables
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new(workflow.metadata.id);
    ctx.set_variable("x".to_string(), serde_json::json!(42));
    ctx.set_variable("y".to_string(), serde_json::json!(7));

    let result = engine.execute(&workflow).await?;

    println!("  Input: x=42, y=7");
    if let Some(node_result) = result.get_node_result(&math_id) {
        if let Some(output) = &node_result.output {
            println!("  Result: {}", serde_json::to_string_pretty(output)?);
        }
    }
    assert_eq!(result.state, ExecutionState::Completed);
    println!("  Status: Completed");

    Ok(())
}

/// Example 2: String manipulation with Rhai
async fn run_string_example() -> anyhow::Result<()> {
    let mut workflow = Workflow::new("String Manipulation".to_string());
    workflow.metadata.description = Some("Demonstrates Rhai string operations".to_string());

    // Start node
    let start = Node::new("Start".to_string(), NodeKind::Start);
    let start_id = start.id;
    workflow.add_node(start);

    // Code node for string operations
    let string_code = Node::new(
        "Process Text".to_string(),
        NodeKind::Code(ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#"
// String concatenation
let greeting = "Hello, " + name + "!";

// String methods
let upper = name.to_upper();
let lower = name.to_lower();
let length = name.len();
let reversed = name.chars().rev().collect::<String>();

// Template-style formatting
let formatted = `User "${name}" has ${length} characters`;

// String contains check
let has_e = name.contains("e");

#{
    "greeting": greeting,
    "uppercase": upper,
    "lowercase": lower,
    "length": length,
    "reversed": reversed,
    "formatted": formatted,
    "contains_e": has_e
}
            "#
            .to_string(),
            inputs: vec!["name".to_string()],
            output: "string_result".to_string(),
        }),
    );
    let string_id = string_code.id;
    workflow.add_node(string_code);

    // End node
    let end = Node::new("End".to_string(), NodeKind::End);
    let end_id = end.id;
    workflow.add_node(end);

    // Connect nodes
    workflow.add_edge(Edge::new(start_id, string_id));
    workflow.add_edge(Edge::new(string_id, end_id));

    // Execute
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new(workflow.metadata.id);
    ctx.set_variable("name".to_string(), serde_json::json!("Alice"));

    let result = engine.execute(&workflow).await?;

    println!("  Input: name=\"Alice\"");
    if let Some(node_result) = result.get_node_result(&string_id) {
        if let Some(output) = &node_result.output {
            println!("  Result: {}", serde_json::to_string_pretty(output)?);
        }
    }
    assert_eq!(result.state, ExecutionState::Completed);
    println!("  Status: Completed");

    Ok(())
}

/// Example 3: Array and data transformation
async fn run_array_example() -> anyhow::Result<()> {
    let mut workflow = Workflow::new("Array Transformation".to_string());
    workflow.metadata.description =
        Some("Demonstrates Rhai array and map operations".to_string());

    // Start node
    let start = Node::new("Start".to_string(), NodeKind::Start);
    let start_id = start.id;
    workflow.add_node(start);

    // Code node for array operations
    let array_code = Node::new(
        "Transform Data".to_string(),
        NodeKind::Code(ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#"
// Calculate statistics from numbers array
let count = numbers.len();
let mut sum = 0;
let mut max = numbers[0];
let mut min = numbers[0];

for n in numbers {
    sum += n;
    if n > max { max = n; }
    if n < min { min = n; }
}

let average = sum / count;

// Filter numbers greater than threshold
let mut filtered = [];
for n in numbers {
    if n > threshold {
        filtered.push(n);
    }
}

// Double each number
let mut doubled = [];
for n in numbers {
    doubled.push(n * 2);
}

#{
    "count": count,
    "sum": sum,
    "average": average,
    "max": max,
    "min": min,
    "filtered_above_threshold": filtered,
    "doubled": doubled,
    "threshold_used": threshold
}
            "#
            .to_string(),
            inputs: vec!["numbers".to_string(), "threshold".to_string()],
            output: "array_result".to_string(),
        }),
    );
    let array_id = array_code.id;
    workflow.add_node(array_code);

    // End node
    let end = Node::new("End".to_string(), NodeKind::End);
    let end_id = end.id;
    workflow.add_node(end);

    // Connect nodes
    workflow.add_edge(Edge::new(start_id, array_id));
    workflow.add_edge(Edge::new(array_id, end_id));

    // Execute
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new(workflow.metadata.id);
    ctx.set_variable(
        "numbers".to_string(),
        serde_json::json!([10, 25, 7, 42, 15, 3, 89, 33]),
    );
    ctx.set_variable("threshold".to_string(), serde_json::json!(20));

    let result = engine.execute(&workflow).await?;

    println!("  Input: numbers=[10, 25, 7, 42, 15, 3, 89, 33], threshold=20");
    if let Some(node_result) = result.get_node_result(&array_id) {
        if let Some(output) = &node_result.output {
            println!("  Result: {}", serde_json::to_string_pretty(output)?);
        }
    }
    assert_eq!(result.state, ExecutionState::Completed);
    println!("  Status: Completed");

    Ok(())
}

/// Example 4: Multi-stage data processing pipeline
async fn run_pipeline_example() -> anyhow::Result<()> {
    let mut workflow = Workflow::new("Data Pipeline".to_string());
    workflow.metadata.description =
        Some("Demonstrates multi-stage code execution pipeline".to_string());

    // Start node
    let start = Node::new("Start".to_string(), NodeKind::Start);
    let start_id = start.id;
    workflow.add_node(start);

    // Stage 1: Parse and validate input data
    let parse_code = Node::new(
        "Parse Input".to_string(),
        NodeKind::Code(ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#"
// Parse raw data and extract fields
let parsed = [];
for item in raw_data {
    let record = #{
        "id": item.id,
        "value": item.value,
        "category": item.category,
        "is_valid": item.value > 0
    };
    parsed.push(record);
}
parsed
            "#
            .to_string(),
            inputs: vec!["raw_data".to_string()],
            output: "parsed_data".to_string(),
        }),
    );
    let parse_id = parse_code.id;
    workflow.add_node(parse_code);

    // Stage 2: Filter and aggregate
    let aggregate_code = Node::new(
        "Aggregate Data".to_string(),
        NodeKind::Code(ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#"
// Aggregate by category
let mut category_totals = #{};
let mut valid_count = 0;
let mut invalid_count = 0;

for record in parsed_data {
    if record.is_valid {
        valid_count += 1;
        let cat = record.category;
        if category_totals.contains(cat) {
            category_totals[cat] += record.value;
        } else {
            category_totals[cat] = record.value;
        }
    } else {
        invalid_count += 1;
    }
}

#{
    "category_totals": category_totals,
    "valid_records": valid_count,
    "invalid_records": invalid_count,
    "total_records": valid_count + invalid_count
}
            "#
            .to_string(),
            inputs: vec!["parsed_data".to_string()],
            output: "aggregated_data".to_string(),
        }),
    );
    let aggregate_id = aggregate_code.id;
    workflow.add_node(aggregate_code);

    // Stage 3: Generate summary report
    let report_code = Node::new(
        "Generate Report".to_string(),
        NodeKind::Code(ScriptConfig {
            runtime: "rhai".to_string(),
            code: r#"
// Generate final report
let totals = aggregated_data.category_totals;
let mut report_lines = [];
report_lines.push("=== Data Processing Report ===");
report_lines.push(`Total Records: ${aggregated_data.total_records}`);
report_lines.push(`Valid: ${aggregated_data.valid_records}, Invalid: ${aggregated_data.invalid_records}`);
report_lines.push("");
report_lines.push("Category Totals:");

// Calculate grand total
let grand_total = 0;
for cat in totals.keys() {
    let val = totals[cat];
    grand_total += val;
    report_lines.push(`  - ${cat}: ${val}`);
}
report_lines.push("");
report_lines.push(`Grand Total: ${grand_total}`);

#{
    "report": report_lines,
    "grand_total": grand_total,
    "summary": aggregated_data
}
            "#
            .to_string(),
            inputs: vec!["aggregated_data".to_string()],
            output: "final_report".to_string(),
        }),
    );
    let report_id = report_code.id;
    workflow.add_node(report_code);

    // End node
    let end = Node::new("End".to_string(), NodeKind::End);
    let end_id = end.id;
    workflow.add_node(end);

    // Connect nodes in pipeline
    workflow.add_edge(Edge::new(start_id, parse_id));
    workflow.add_edge(Edge::new(parse_id, aggregate_id));
    workflow.add_edge(Edge::new(aggregate_id, report_id));
    workflow.add_edge(Edge::new(report_id, end_id));

    // Execute with sample data
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new(workflow.metadata.id);
    ctx.set_variable(
        "raw_data".to_string(),
        serde_json::json!([
            {"id": 1, "value": 100, "category": "electronics"},
            {"id": 2, "value": 50, "category": "books"},
            {"id": 3, "value": -10, "category": "electronics"},  // invalid (negative)
            {"id": 4, "value": 200, "category": "electronics"},
            {"id": 5, "value": 75, "category": "books"},
            {"id": 6, "value": 150, "category": "clothing"},
            {"id": 7, "value": 0, "category": "books"},  // invalid (zero)
            {"id": 8, "value": 300, "category": "electronics"}
        ]),
    );

    let result = engine.execute(&workflow).await?;

    println!("  Pipeline: Parse -> Aggregate -> Report");
    println!("  Input: 8 records across 3 categories");
    println!();

    // Show each stage's output
    if let Some(node_result) = result.get_node_result(&parse_id) {
        if let Some(output) = &node_result.output {
            let parsed: Vec<serde_json::Value> = serde_json::from_value(output.clone())?;
            println!("  Stage 1 (Parse): {} records parsed", parsed.len());
        }
    }

    if let Some(node_result) = result.get_node_result(&aggregate_id) {
        if let Some(output) = &node_result.output {
            println!("  Stage 2 (Aggregate): {}", serde_json::to_string(output)?);
        }
    }

    if let Some(node_result) = result.get_node_result(&report_id) {
        if let Some(output) = &node_result.output {
            println!("\n  Stage 3 (Final Report):");
            if let Some(report) = output.get("report") {
                if let Some(lines) = report.as_array() {
                    for line in lines {
                        if let Some(s) = line.as_str() {
                            println!("    {}", s);
                        }
                    }
                }
            }
        }
    }

    assert_eq!(result.state, ExecutionState::Completed);
    println!("\n  Status: Completed");

    Ok(())
}
