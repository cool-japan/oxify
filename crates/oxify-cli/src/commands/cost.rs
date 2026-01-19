//! Cost estimation command
//!
//! Estimate the cost of running a workflow

use anyhow::{Context, Result};
use oxify_engine::CostEstimator;
use oxify_model::Workflow;
use std::fs;
use std::path::Path;

/// Estimate workflow execution cost
pub async fn handle_cost_command(
    workflow_file: String,
    avg_prompt_tokens: Option<u32>,
    avg_response_tokens: Option<u32>,
    show_breakdown: bool,
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

    // Create cost estimator
    let estimator = if let (Some(prompt), Some(response)) = (avg_prompt_tokens, avg_response_tokens)
    {
        CostEstimator::with_averages(prompt, response)
    } else {
        CostEstimator::new()
    };

    // Estimate costs
    let estimate = estimator.estimate_workflow(&workflow);

    // Display results
    println!("Cost Estimation for: {}", workflow.metadata.name);
    println!("==================================\n");

    println!("Estimated Total Cost: ${:.6}", estimate.total_cost_usd);
    println!("  Input Tokens:  {}", estimate.total_input_tokens);
    println!("  Output Tokens: {}", estimate.total_output_tokens);
    println!();

    if show_breakdown {
        // Category breakdown
        if !estimate.category_costs.is_empty() {
            println!("Cost by Category:");
            let mut categories: Vec<_> = estimate.category_costs.iter().collect();
            categories.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

            for (category, cost) in categories {
                println!("  {:<20} ${:.6}", category, cost);
            }
            println!();
        }

        // Per-node breakdown
        if !estimate.node_costs.is_empty() {
            println!("Cost by Node:");
            let mut node_costs = estimate.node_costs.clone();
            node_costs.sort_by(|a, b| {
                b.cost_usd
                    .partial_cmp(&a.cost_usd)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            for node_cost in &node_costs {
                if node_cost.cost_usd > 0.0 {
                    println!("  {} ({})", node_cost.node_name, node_cost.node_id);
                    println!("    Cost: ${:.6}", node_cost.cost_usd);

                    if node_cost.estimated_input_tokens > 0 {
                        println!(
                            "    Tokens: {} in + {} out",
                            node_cost.estimated_input_tokens, node_cost.estimated_output_tokens
                        );
                    }

                    for op in &node_cost.operations {
                        println!("    - {}: ${:.6}", op.description, op.cost_usd);
                    }
                    println!();
                }
            }
        }
    }

    // Cost estimates for different scales
    println!("Estimated Costs at Scale:");
    println!("  10 executions:    ${:.2}", estimate.total_cost_usd * 10.0);
    println!(
        "  100 executions:   ${:.2}",
        estimate.total_cost_usd * 100.0
    );
    println!(
        "  1,000 executions: ${:.2}",
        estimate.total_cost_usd * 1000.0
    );
    println!(
        "  10,000 executions:${:.2}",
        estimate.total_cost_usd * 10000.0
    );
    println!();

    // Token budgeting
    if estimate.total_input_tokens > 0 || estimate.total_output_tokens > 0 {
        let total_tokens = estimate.total_input_tokens + estimate.total_output_tokens;
        println!("Token Budgeting:");
        println!("  Total tokens per execution: {}", total_tokens);
        println!("  Monthly budget for 1,000 runs:");
        println!("    Tokens: {}", total_tokens * 1000);
        println!("    Cost: ${:.2}", estimate.total_cost_usd * 1000.0);
        println!();
    }

    // Warnings and recommendations
    if estimate.total_cost_usd > 1.0 {
        println!("⚠️  Warning: This workflow has a relatively high cost per execution");
        println!("   Consider optimizing prompts or using cheaper models for some steps");
    }

    if estimate.total_input_tokens > 10000 {
        println!("⚠️  Warning: High token usage detected");
        println!("   Consider breaking down prompts or using context windows more efficiently");
    }

    Ok(())
}

/// Compare costs of multiple workflows
pub async fn handle_cost_compare_command(workflow_files: Vec<String>) -> Result<()> {
    if workflow_files.len() < 2 {
        anyhow::bail!("Need at least 2 workflows to compare");
    }

    let estimator = CostEstimator::new();
    let mut estimates = Vec::new();

    // Load and estimate each workflow
    for workflow_file in &workflow_files {
        let path = Path::new(workflow_file);
        if !path.exists() {
            anyhow::bail!("Workflow file not found: {}", workflow_file);
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

        let workflow: Workflow =
            match path.extension().and_then(|s| s.to_str()) {
                Some("json") => serde_json::from_str(&content)
                    .with_context(|| "Failed to parse workflow JSON")?,
                Some("yaml") | Some("yml") => serde_yaml::from_str(&content)
                    .with_context(|| "Failed to parse workflow YAML")?,
                _ => serde_json::from_str(&content)
                    .or_else(|_| serde_yaml::from_str(&content))
                    .with_context(|| "Failed to parse workflow (tried both JSON and YAML)")?,
            };

        let estimate = estimator.estimate_workflow(&workflow);
        estimates.push((workflow.metadata.name, estimate));
    }

    // Display comparison
    println!("Workflow Cost Comparison");
    println!("========================\n");

    // Table header
    println!(
        "{:<30} {:>15} {:>15} {:>15}",
        "Workflow", "Cost (USD)", "Input Tokens", "Output Tokens"
    );
    println!("{}", "-".repeat(80));

    // Sort by cost
    estimates.sort_by(|a, b| {
        b.1.total_cost_usd
            .partial_cmp(&a.1.total_cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (name, estimate) in &estimates {
        println!(
            "{:<30} ${:>14.6} {:>15} {:>15}",
            name,
            estimate.total_cost_usd,
            estimate.total_input_tokens,
            estimate.total_output_tokens
        );
    }

    println!();

    // Cost difference analysis
    if estimates.len() >= 2 {
        let cheapest = &estimates.last().unwrap();
        let most_expensive = &estimates.first().unwrap();

        let difference = most_expensive.1.total_cost_usd - cheapest.1.total_cost_usd;
        let ratio = most_expensive.1.total_cost_usd / cheapest.1.total_cost_usd;

        println!("Analysis:");
        println!(
            "  Most expensive: {} (${:.6})",
            most_expensive.0, most_expensive.1.total_cost_usd
        );
        println!(
            "  Cheapest:       {} (${:.6})",
            cheapest.0, cheapest.1.total_cost_usd
        );
        println!("  Difference:     ${:.6} ({:.1}x)", difference, ratio);
        println!();

        println!(
            "  Cost difference at 1,000 runs: ${:.2}",
            difference * 1000.0
        );
    }

    Ok(())
}
