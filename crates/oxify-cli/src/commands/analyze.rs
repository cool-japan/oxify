//! Workflow analysis command
//!
//! Analyze workflows for optimization opportunities

use anyhow::{Context, Result};
use oxify_engine::BatchAnalyzer;
use oxify_model::{Node, Workflow};
use std::fs;
use std::path::Path;

/// Analyze workflow for batching opportunities
pub async fn handle_analyze_batching_command(
    workflow_file: String,
    min_batch_size: Option<usize>,
    max_batch_size: Option<usize>,
) -> Result<()> {
    // Load workflow
    let path = Path::new(&workflow_file);
    if !path.exists() {
        anyhow::bail!("Workflow file not found: {}", workflow_file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

    let workflow: Workflow = match path.extension().and_then(|s| s.to_str()) {
        Some("json") => {
            serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
        }
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
        }
        _ => serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content))
            .with_context(|| "Failed to parse workflow (tried both JSON and YAML)")?,
    };

    // Validate workflow
    workflow
        .validate()
        .map_err(|error| anyhow::anyhow!("Workflow validation failed: {}", error))?;

    println!("Batching Analysis for: {}", workflow.metadata.name);
    println!("=====================================\n");

    // Create analyzer
    let analyzer = if let (Some(min), Some(max)) = (min_batch_size, max_batch_size) {
        BatchAnalyzer::with_limits(min, max)
    } else {
        BatchAnalyzer::new()
    };

    // Analyze nodes
    let node_refs: Vec<&Node> = workflow.nodes.iter().collect();
    let plan = analyzer.analyze(&node_refs);
    let stats = oxify_engine::BatchStats::from_plan(&plan, &analyzer);

    // Display overall statistics
    println!("Overall Statistics:");
    println!("  Total nodes:          {}", stats.total_nodes);
    println!("  Batchable nodes:      {}", stats.batched_nodes);
    println!("  Individual nodes:     {}", plan.individual_nodes.len());
    println!("  Number of batches:    {}", stats.batch_count);
    println!("  Average batch size:   {:.1}", stats.average_batch_size);
    println!("  Batching efficiency:  {:.1}%", stats.efficiency() * 100.0);
    println!(
        "  Estimated time saved: {:.1}%\n",
        stats.estimated_time_savings * 100.0
    );

    // Display batches
    if !plan.batches.is_empty() {
        println!("Detected Batches:");
        println!("-----------------");

        for (idx, batch) in plan.batches.iter().enumerate() {
            println!("\nBatch #{} ({} nodes)", idx + 1, batch.size());
            println!("  Type: {:?}", batch.group);
            println!("  Speedup factor: {:.2}x", batch.speedup_factor);

            // Show nodes in batch
            println!("  Nodes:");
            for node_id in &batch.nodes {
                if let Some(node) = workflow.nodes.iter().find(|n| n.id == *node_id) {
                    println!("    - {} ({})", node.name, node_id);
                }
            }
        }
        println!();
    }

    // Display individual nodes
    if !plan.individual_nodes.is_empty() {
        println!("Individual Execution (No Batching):");
        println!("-----------------------------------");
        for node_id in &plan.individual_nodes {
            if let Some(node) = workflow.nodes.iter().find(|n| n.id == *node_id) {
                let reason = match &node.kind {
                    oxify_model::NodeKind::Start => "Start node",
                    oxify_model::NodeKind::End => "End node",
                    oxify_model::NodeKind::Loop(_) => "Loop node",
                    oxify_model::NodeKind::TryCatch(_) => "Error handler",
                    oxify_model::NodeKind::SubWorkflow(_) => "Sub-workflow",
                    _ => "Insufficient similar nodes",
                };
                println!("  - {} ({}) - {}", node.name, node_id, reason);
            }
        }
        println!();
    }

    // Recommendations
    println!("Recommendations:");
    println!("----------------");

    if stats.efficiency() < 0.3 {
        println!("⚠️  Low batching efficiency detected");
        println!("   - Consider grouping similar operations together");
        println!("   - Use the same LLM provider across multiple nodes");
        println!("   - Batch vector database searches when possible");
    } else if stats.efficiency() > 0.7 {
        println!("✓ Excellent batching potential!");
        println!("  Your workflow is well-structured for batch execution");
    } else {
        println!("ℹ️  Moderate batching opportunities");
        println!("   Some optimization is possible");
    }

    if stats.estimated_time_savings > 0.2 {
        println!(
            "\n💡 Estimated {:.0}% time reduction with batch execution",
            stats.estimated_time_savings * 100.0
        );
    }

    // Specific recommendations based on node types
    let llm_count = workflow
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, oxify_model::NodeKind::LLM(_)))
        .count();

    let vector_count = workflow
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, oxify_model::NodeKind::Retriever(_)))
        .count();

    if llm_count >= 3 {
        println!("\n📝 LLM Optimization:");
        println!("   - {} LLM nodes detected", llm_count);
        println!("   - Consider using the same provider for better batching");
        println!("   - Batch similar prompts together when possible");
    }

    if vector_count >= 2 {
        println!("\n🔍 Vector Search Optimization:");
        println!("   - {} vector search nodes detected", vector_count);
        println!("   - Vector searches batch very efficiently");
        println!("   - Consider using consistent database types");
    }

    Ok(())
}

/// Analyze workflow structure and complexity
pub async fn handle_analyze_structure_command(workflow_file: String) -> Result<()> {
    // Load workflow
    let path = Path::new(&workflow_file);
    if !path.exists() {
        anyhow::bail!("Workflow file not found: {}", workflow_file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

    let workflow: Workflow = match path.extension().and_then(|s| s.to_str()) {
        Some("json") => {
            serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
        }
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
        }
        _ => serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content))
            .with_context(|| "Failed to parse workflow (tried both JSON and YAML)")?,
    };

    println!("Structure Analysis for: {}", workflow.metadata.name);
    println!("======================================\n");

    // Basic metrics
    println!("Basic Metrics:");
    println!("  Nodes:       {}", workflow.nodes.len());
    println!("  Edges:       {}", workflow.edges.len());
    println!(
        "  Avg degree:  {:.1}",
        workflow.edges.len() as f64 / workflow.nodes.len() as f64
    );
    println!();

    // Node type distribution
    use std::collections::HashMap;
    let mut node_types: HashMap<&str, usize> = HashMap::new();

    for node in &workflow.nodes {
        let type_name = match &node.kind {
            oxify_model::NodeKind::Start => "Start",
            oxify_model::NodeKind::End => "End",
            oxify_model::NodeKind::LLM(_) => "LLM",
            oxify_model::NodeKind::Retriever(_) => "Retriever",
            oxify_model::NodeKind::Code(_) => "Code",
            oxify_model::NodeKind::IfElse(_) => "IfElse",
            oxify_model::NodeKind::Tool(_) => "Tool",
            oxify_model::NodeKind::Loop(_) => "Loop",
            oxify_model::NodeKind::TryCatch(_) => "TryCatch",
            oxify_model::NodeKind::SubWorkflow(_) => "SubWorkflow",
            oxify_model::NodeKind::Switch(_) => "Switch",
            oxify_model::NodeKind::Parallel(_) => "Parallel",
            oxify_model::NodeKind::Approval(_) => "Approval",
            oxify_model::NodeKind::Form(_) => "Form",
            oxify_model::NodeKind::Vision(_) => "Vision",
        };
        *node_types.entry(type_name).or_insert(0) += 1;
    }

    println!("Node Type Distribution:");
    let mut type_vec: Vec<_> = node_types.iter().collect();
    type_vec.sort_by(|a, b| b.1.cmp(a.1));

    for (node_type, count) in type_vec {
        let percentage = (*count as f64 / workflow.nodes.len() as f64) * 100.0;
        println!("  {:<15} {:>3} ({:>5.1}%)", node_type, count, percentage);
    }
    println!();

    // Complexity indicators
    let has_loops = workflow
        .nodes
        .iter()
        .any(|n| matches!(n.kind, oxify_model::NodeKind::Loop(_)));
    let has_conditionals = workflow.nodes.iter().any(|n| {
        matches!(
            n.kind,
            oxify_model::NodeKind::IfElse(_) | oxify_model::NodeKind::Switch(_)
        )
    });
    let has_error_handling = workflow
        .nodes
        .iter()
        .any(|n| matches!(n.kind, oxify_model::NodeKind::TryCatch(_)));
    let has_subworkflows = workflow
        .nodes
        .iter()
        .any(|n| matches!(n.kind, oxify_model::NodeKind::SubWorkflow(_)));
    let has_approvals = workflow
        .nodes
        .iter()
        .any(|n| matches!(n.kind, oxify_model::NodeKind::Approval(_)));

    println!("Complexity Features:");
    println!(
        "  Loops:          {}",
        if has_loops { "Yes ⚠️" } else { "No" }
    );
    println!(
        "  Conditionals:   {}",
        if has_conditionals { "Yes ⚠️" } else { "No" }
    );
    println!(
        "  Error handling: {}",
        if has_error_handling { "Yes ✓" } else { "No" }
    );
    println!(
        "  Sub-workflows:  {}",
        if has_subworkflows { "Yes ⚠️" } else { "No" }
    );
    println!(
        "  Approval gates: {}",
        if has_approvals { "Yes" } else { "No" }
    );
    println!();

    // Calculate complexity score
    let mut complexity_score = workflow.nodes.len();
    if has_loops {
        complexity_score += 10;
    }
    if has_conditionals {
        complexity_score += 5;
    }
    if has_subworkflows {
        complexity_score += 8;
    }

    println!("Complexity Assessment:");
    let complexity_level = if complexity_score < 10 {
        "Simple ✓"
    } else if complexity_score < 30 {
        "Moderate"
    } else if complexity_score < 60 {
        "Complex ⚠️"
    } else {
        "Very Complex ⚠️⚠️"
    };

    println!("  Score: {}", complexity_score);
    println!("  Level: {}", complexity_level);
    println!();

    // Recommendations
    println!("Recommendations:");
    println!("----------------");

    if complexity_score > 60 {
        println!("⚠️  High complexity detected");
        println!("   - Consider breaking down into smaller sub-workflows");
        println!("   - Add comprehensive error handling");
        println!("   - Document complex logic thoroughly");
    }

    if !has_error_handling && complexity_score > 20 {
        println!("⚠️  No error handling detected in complex workflow");
        println!("   - Add TryCatch nodes around critical operations");
        println!("   - Implement retry logic for unstable operations");
    }

    if has_loops && !has_error_handling {
        println!("⚠️  Loops without error handling");
        println!("   - Wrap loop bodies in TryCatch blocks");
        println!("   - Set appropriate max_iterations limits");
    }

    Ok(())
}

/// Analyze workflow for optimization opportunities
pub async fn handle_analyze_optimization_command(
    workflow_file: String,
    strict_mode: bool,
) -> Result<()> {
    // Load workflow
    let path = Path::new(&workflow_file);
    if !path.exists() {
        anyhow::bail!("Workflow file not found: {}", workflow_file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

    let workflow: Workflow = match path.extension().and_then(|s| s.to_str()) {
        Some("json") => {
            serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
        }
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
        }
        _ => serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content))
            .with_context(|| "Failed to parse workflow (tried both JSON and YAML)")?,
    };

    println!("Optimization Analysis for: {}", workflow.metadata.name);
    println!("=========================================\n");

    // Create optimizer
    let optimizer = if strict_mode {
        oxify_engine::WorkflowOptimizer::with_strict_mode()
    } else {
        oxify_engine::WorkflowOptimizer::new()
    };

    // Analyze workflow
    let optimizations = optimizer.optimize(&workflow);

    if optimizations.is_empty() {
        println!("✓ No optimization opportunities found!");
        println!("  Your workflow is already well-optimized.\n");
        return Ok(());
    }

    println!(
        "Found {} optimization opportunities:\n",
        optimizations.len()
    );

    // Group by priority
    let critical: Vec<_> = optimizations
        .iter()
        .filter(|o| matches!(o.priority, oxify_engine::Priority::Critical))
        .collect();
    let high: Vec<_> = optimizations
        .iter()
        .filter(|o| matches!(o.priority, oxify_engine::Priority::High))
        .collect();
    let medium: Vec<_> = optimizations
        .iter()
        .filter(|o| matches!(o.priority, oxify_engine::Priority::Medium))
        .collect();
    let low: Vec<_> = optimizations
        .iter()
        .filter(|o| matches!(o.priority, oxify_engine::Priority::Low))
        .collect();

    // Display by priority
    if !critical.is_empty() {
        println!("🔴 CRITICAL PRIORITY ({}):", critical.len());
        println!("─────────────────────────");
        for opt in critical {
            print_optimization(opt);
        }
        println!();
    }

    if !high.is_empty() {
        println!("🟠 HIGH PRIORITY ({}):", high.len());
        println!("──────────────────");
        for opt in high {
            print_optimization(opt);
        }
        println!();
    }

    if !medium.is_empty() {
        println!("🟡 MEDIUM PRIORITY ({}):", medium.len());
        println!("────────────────────");
        for opt in medium {
            print_optimization(opt);
        }
        println!();
    }

    if !low.is_empty() {
        println!("🟢 LOW PRIORITY ({}):", low.len());
        println!("─────────────────");
        for opt in low {
            print_optimization(opt);
        }
        println!();
    }

    // Summary
    println!("Summary");
    println!("═══════");
    println!("Total optimizations: {}", optimizations.len());

    let total_time_savings: f32 = optimizations
        .iter()
        .filter_map(|o| o.impact.time_savings)
        .sum();
    let total_cost_reduction: f32 = optimizations
        .iter()
        .filter_map(|o| o.impact.cost_reduction)
        .sum();

    if total_time_savings > 0.0 {
        println!("Potential time savings: {:.0}%", total_time_savings);
    }
    if total_cost_reduction > 0.0 {
        println!("Potential cost reduction: {:.0}%", total_cost_reduction);
    }

    Ok(())
}

fn print_optimization(opt: &oxify_engine::Optimization) {
    println!("\n  {} - {:?}", opt.title, opt.category);
    println!("  {}", opt.description);

    if !opt.affected_nodes.is_empty() {
        println!("  Affected nodes: {}", opt.affected_nodes.len());
    }

    if let Some(time) = opt.impact.time_savings {
        println!("  ⏱️  Time savings: {:.0}%", time);
    }
    if let Some(cost) = opt.impact.cost_reduction {
        println!("  💰 Cost reduction: {:.0}%", cost);
    }
    if let Some(rel) = &opt.impact.reliability_improvement {
        println!("  🛡️  Reliability: {}", rel);
    }

    println!("  ➡️  Action: {}", opt.action);
}
