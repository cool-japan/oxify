//! Conditional Workflow Example
//!
//! Demonstrates conditional branching in workflows based on data conditions.
//! This example shows a workflow that routes execution based on user score:
//! - If score >= 80: Route to "pass" path
//! - If score < 80: Route to "fail" path

use oxify_engine::Engine;
use oxify_model::{Condition, Edge, ExecutionState, Node, NodeKind, Workflow};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🔀 OxiFY Conditional Workflow Example\n");
    println!("This example demonstrates conditional branching based on a score variable.\n");

    // Create workflow
    let mut workflow = Workflow::new("Score Evaluation Workflow".to_string());

    // Start node
    let start = Node::new("Start".to_string(), NodeKind::Start);
    let start_id = start.id;
    workflow.add_node(start);

    // Create placeholder nodes for "pass" and "fail" branches
    let pass_node = Node::new(
        "Pass Handler".to_string(),
        NodeKind::Code(oxify_model::ScriptConfig {
            runtime: "rust".to_string(),
            code: "println!(\"Congratulations! You passed.\");".to_string(),
            inputs: vec![],
            output: "pass_message".to_string(),
        }),
    );
    let pass_id = pass_node.id;
    workflow.add_node(pass_node);

    let fail_node = Node::new(
        "Fail Handler".to_string(),
        NodeKind::Code(oxify_model::ScriptConfig {
            runtime: "rust".to_string(),
            code: "println!(\"Sorry, you did not pass.\");".to_string(),
            inputs: vec![],
            output: "fail_message".to_string(),
        }),
    );
    let fail_id = fail_node.id;
    workflow.add_node(fail_node);

    // Conditional node
    let conditional = Node::new(
        "Score Check".to_string(),
        NodeKind::IfElse(Condition {
            expression: "score >= 80".to_string(),
            true_branch: pass_id,
            false_branch: fail_id,
        }),
    );
    let conditional_id = conditional.id;
    workflow.add_node(conditional);

    // End nodes for each branch
    let end_pass = Node::new("End (Pass)".to_string(), NodeKind::End);
    let end_pass_id = end_pass.id;
    workflow.add_node(end_pass);

    let end_fail = Node::new("End (Fail)".to_string(), NodeKind::End);
    let end_fail_id = end_fail.id;
    workflow.add_node(end_fail);

    // Build the DAG
    workflow.add_edge(Edge::new(start_id, conditional_id));
    workflow.add_edge(Edge::new(conditional_id, pass_id)); // Won't be used if condition is false
    workflow.add_edge(Edge::new(conditional_id, fail_id)); // Won't be used if condition is true
    workflow.add_edge(Edge::new(pass_id, end_pass_id));
    workflow.add_edge(Edge::new(fail_id, end_fail_id));

    // Test with passing score
    println!("📊 Test Case 1: Score = 85 (Should Pass)\n");
    let engine = Engine::new();
    let mut ctx = oxify_model::ExecutionContext::new(workflow.metadata.id);
    ctx.set_variable("score".to_string(), serde_json::json!(85));

    match engine.execute(&workflow).await {
        Ok(result) => {
            println!("✅ Workflow executed successfully!");
            println!("   Status: {:?}", result.state);

            if let Some(cond_result) = result.get_node_result(&conditional_id) {
                if let Some(output) = &cond_result.output {
                    println!("   Condition Result: {}", serde_json::to_string_pretty(output)?);
                }
            }

            assert_eq!(result.state, ExecutionState::Completed);
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
            return Err(e.into());
        }
    }

    println!("\n" + &"=".repeat(60) + "\n");

    // Test with failing score
    println!("📊 Test Case 2: Score = 65 (Should Fail)\n");
    let engine = Engine::new();
    let mut ctx = oxify_model::ExecutionContext::new(workflow.metadata.id);
    ctx.set_variable("score".to_string(), serde_json::json!(65));

    match engine.execute(&workflow).await {
        Ok(result) => {
            println!("✅ Workflow executed successfully!");
            println!("   Status: {:?}", result.state);

            if let Some(cond_result) = result.get_node_result(&conditional_id) {
                if let Some(output) = &cond_result.output {
                    println!("   Condition Result: {}", serde_json::to_string_pretty(output)?);
                }
            }

            assert_eq!(result.state, ExecutionState::Completed);
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
            return Err(e.into());
        }
    }

    println!("\n" + &"=".repeat(60) + "\n");

    // Test with complex condition
    println!("📊 Test Case 3: Complex Condition (score >= 80 && passed == true)\n");

    let mut workflow2 = Workflow::new("Complex Condition Workflow".to_string());

    let start2 = Node::new("Start".to_string(), NodeKind::Start);
    let start2_id = start2.id;
    workflow2.add_node(start2);

    let pass2 = Node::new("Success".to_string(), NodeKind::End);
    let pass2_id = pass2.id;
    workflow2.add_node(pass2);

    let fail2 = Node::new("Failure".to_string(), NodeKind::End);
    let fail2_id = fail2.id;
    workflow2.add_node(fail2);

    let conditional2 = Node::new(
        "Complex Check".to_string(),
        NodeKind::IfElse(Condition {
            expression: "score >= 80 && passed == true".to_string(),
            true_branch: pass2_id,
            false_branch: fail2_id,
        }),
    );
    let conditional2_id = conditional2.id;
    workflow2.add_node(conditional2);

    workflow2.add_edge(Edge::new(start2_id, conditional2_id));
    workflow2.add_edge(Edge::new(conditional2_id, pass2_id));
    workflow2.add_edge(Edge::new(conditional2_id, fail2_id));

    let engine = Engine::new();
    let mut ctx = oxify_model::ExecutionContext::new(workflow2.metadata.id);
    ctx.set_variable("score".to_string(), serde_json::json!(85));
    ctx.set_variable("passed".to_string(), serde_json::json!(true));

    match engine.execute(&workflow2).await {
        Ok(result) => {
            println!("✅ Workflow executed successfully!");
            println!("   Status: {:?}", result.state);

            if let Some(cond_result) = result.get_node_result(&conditional2_id) {
                if let Some(output) = &cond_result.output {
                    println!("   Condition Result: {}", serde_json::to_string_pretty(output)?);
                }
            }

            assert_eq!(result.state, ExecutionState::Completed);
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
            return Err(e.into());
        }
    }

    println!("\n🎉 All conditional workflow tests completed!");

    Ok(())
}
