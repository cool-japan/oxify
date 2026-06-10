use anyhow::{Context, Result};
use clap::Subcommand;
use oxify_model::{NodeKind, Workflow};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Subcommand)]
pub enum StatsCommands {
    /// Show workflow statistics
    Workflow {
        /// Workflow file path
        file: String,
        /// Show detailed statistics
        #[arg(short, long)]
        detailed: bool,
    },
    /// Compare statistics of multiple workflows
    Compare {
        /// Workflow files to compare
        files: Vec<String>,
    },
}

pub async fn handle_stats_command(command: StatsCommands) -> Result<()> {
    match command {
        StatsCommands::Workflow { file, detailed } => show_workflow_stats(&file, detailed).await,
        StatsCommands::Compare { files } => compare_workflows(files).await,
    }
}

async fn show_workflow_stats(file: &str, detailed: bool) -> Result<()> {
    let workflow = load_workflow(file)?;
    let stats = calculate_workflow_stats(&workflow);

    println!("Workflow Statistics");
    println!("===================\n");

    println!("📊 Overview:");
    println!("  Name:        {}", workflow.metadata.name);
    println!("  Version:     {}", workflow.metadata.version);
    println!("  Nodes:       {}", stats.total_nodes);
    println!("  Edges:       {}", stats.total_edges);
    println!("  Complexity:  {}", stats.complexity_score);
    println!();

    println!("🔢 Node Type Distribution:");
    let mut sorted_types: Vec<_> = stats.node_types.iter().collect();
    sorted_types.sort_by(|a, b| b.1.cmp(a.1));
    for (node_type, count) in sorted_types {
        let percentage = (*count as f64 / stats.total_nodes as f64) * 100.0;
        println!("  {:15} : {:3} ({:5.1}%)", node_type, count, percentage);
    }
    println!();

    println!("📈 Graph Metrics:");
    println!("  Start Nodes:      {}", stats.start_nodes);
    println!("  End Nodes:        {}", stats.end_nodes);
    println!("  Max Depth:        {}", stats.max_depth);
    println!("  Avg Branching:    {:.2}", stats.avg_branching_factor);
    println!(
        "  Cyclic:           {}",
        if stats.has_cycles { "Yes" } else { "No" }
    );
    println!();

    if detailed {
        println!("🔍 Detailed Analysis:\n");

        if !stats.llm_models.is_empty() {
            println!("LLM Models Used:");
            for (provider, models) in &stats.llm_models {
                println!("  {} ({}):", provider, models.len());
                for model in models {
                    println!("    - {}", model);
                }
            }
            println!();
        }

        if !stats.vector_dbs.is_empty() {
            println!("Vector Databases:");
            for (db_type, collections) in &stats.vector_dbs {
                println!("  {} ({} collections)", db_type, collections.len());
            }
            println!();
        }

        if stats.has_control_flow {
            println!("Control Flow:");
            if stats.has_conditionals {
                println!("  ✓ Conditionals (IfElse/Switch)");
            }
            if stats.has_loops {
                println!("  ✓ Loops");
            }
            if stats.has_error_handling {
                println!("  ✓ Error Handling (TryCatch)");
            }
            if stats.has_parallel {
                println!("  ✓ Parallel Execution");
            }
            println!();
        }

        println!("💡 Recommendations:");
        for recommendation in &stats.recommendations {
            println!("  • {}", recommendation);
        }
        println!();
    }

    if !stats.warnings.is_empty() {
        println!("⚠️  Warnings:");
        for warning in &stats.warnings {
            println!("  • {}", warning);
        }
        println!();
    }

    Ok(())
}

async fn compare_workflows(files: Vec<String>) -> Result<()> {
    if files.len() < 2 {
        anyhow::bail!("At least 2 workflows are required for comparison");
    }

    println!("Workflow Comparison");
    println!("===================\n");

    let mut all_stats = Vec::new();
    for file in &files {
        let workflow = load_workflow(file)?;
        let stats = calculate_workflow_stats(&workflow);
        all_stats.push((workflow.metadata.name.clone(), stats));
    }

    // Print comparison table
    println!(
        "{:20} {:>8} {:>8} {:>12} {:>8}",
        "Name", "Nodes", "Edges", "Complexity", "Depth"
    );
    println!("{:-<60}", "");

    for (name, stats) in &all_stats {
        println!(
            "{:20} {:>8} {:>8} {:>12} {:>8}",
            truncate(name, 20),
            stats.total_nodes,
            stats.total_edges,
            stats.complexity_score,
            stats.max_depth
        );
    }

    println!();

    // Find most/least complex
    let (most_complex_name, most_complex) = all_stats
        .iter()
        .max_by_key(|(_, s)| s.complexity_score)
        .expect("invariant: all_stats non-empty");
    let (least_complex_name, least_complex) = all_stats
        .iter()
        .min_by_key(|(_, s)| s.complexity_score)
        .expect("invariant: all_stats non-empty");

    println!("📊 Summary:");
    println!(
        "  Most Complex:  {} (score: {})",
        most_complex_name, most_complex.complexity_score
    );
    println!(
        "  Least Complex: {} (score: {})",
        least_complex_name, least_complex.complexity_score
    );
    println!();

    Ok(())
}

#[allow(dead_code)]
struct WorkflowStats {
    total_nodes: usize,
    total_edges: usize,
    start_nodes: usize,
    end_nodes: usize,
    max_depth: usize,
    avg_branching_factor: f64,
    has_cycles: bool,
    complexity_score: usize,
    node_types: HashMap<String, usize>,
    llm_models: HashMap<String, Vec<String>>,
    vector_dbs: HashMap<String, Vec<String>>,
    has_control_flow: bool,
    has_conditionals: bool,
    has_loops: bool,
    has_error_handling: bool,
    has_parallel: bool,
    warnings: Vec<String>,
    recommendations: Vec<String>,
}

fn calculate_workflow_stats(workflow: &Workflow) -> WorkflowStats {
    let mut node_types = HashMap::new();
    let mut llm_models: HashMap<String, Vec<String>> = HashMap::new();
    let mut vector_dbs: HashMap<String, Vec<String>> = HashMap::new();
    let mut start_nodes = 0;
    let mut end_nodes = 0;
    let mut has_conditionals = false;
    let mut has_loops = false;
    let mut has_error_handling = false;
    let mut has_parallel = false;
    let mut warnings = Vec::new();
    let mut recommendations = Vec::new();

    for node in &workflow.nodes {
        let type_name = match &node.kind {
            NodeKind::Start => {
                start_nodes += 1;
                "Start"
            }
            NodeKind::End => {
                end_nodes += 1;
                "End"
            }
            NodeKind::LLM(config) => {
                llm_models
                    .entry(config.provider.clone())
                    .or_default()
                    .push(config.model.clone());
                "LLM"
            }
            NodeKind::Retriever(config) => {
                vector_dbs
                    .entry(config.db_type.clone())
                    .or_default()
                    .push(config.collection.clone());
                "Retriever"
            }
            NodeKind::Code(_) => "Code",
            NodeKind::IfElse(_) => {
                has_conditionals = true;
                "IfElse"
            }
            NodeKind::Tool(_) => "Tool",
            NodeKind::Loop(_) => {
                has_loops = true;
                "Loop"
            }
            NodeKind::TryCatch(_) => {
                has_error_handling = true;
                "TryCatch"
            }
            NodeKind::SubWorkflow(_) => "SubWorkflow",
            NodeKind::Switch(_) => {
                has_conditionals = true;
                "Switch"
            }
            NodeKind::Parallel(_) => {
                has_parallel = true;
                "Parallel"
            }
            NodeKind::Approval(_) => "Approval",
            NodeKind::Form(_) => "Form",
            NodeKind::Vision(_) => "Vision",
            NodeKind::Custom(_) => "Custom",
        };
        *node_types.entry(type_name.to_string()).or_insert(0) += 1;
    }

    // Deduplicate LLM models
    for models in llm_models.values_mut() {
        models.sort();
        models.dedup();
    }

    // Calculate branching factor
    let mut out_degrees = HashMap::new();
    for edge in &workflow.edges {
        *out_degrees.entry(edge.from).or_insert(0) += 1;
    }

    let avg_branching_factor = if !out_degrees.is_empty() {
        out_degrees.values().sum::<usize>() as f64 / out_degrees.len() as f64
    } else {
        0.0
    };

    // Calculate max depth (simplified - actual depth calculation would need topological sort)
    let max_depth = estimate_max_depth(workflow);

    // Detect cycles (simplified)
    let has_cycles = detect_cycles_simple(workflow);

    // Calculate complexity score
    let complexity_score = calculate_complexity(workflow, &node_types);

    // Generate warnings
    if start_nodes == 0 {
        warnings.push("No start node found".to_string());
    } else if start_nodes > 1 {
        warnings.push(format!("Multiple start nodes found ({})", start_nodes));
    }

    if end_nodes == 0 {
        warnings.push("No end node found".to_string());
    }

    if workflow.nodes.len() > 50 {
        warnings.push("Large workflow (>50 nodes) - consider splitting".to_string());
    }

    if max_depth > 10 {
        warnings.push(format!(
            "Deep workflow (depth {}) - may be hard to debug",
            max_depth
        ));
    }

    // Generate recommendations
    if !has_error_handling && workflow.nodes.len() > 5 {
        recommendations.push("Consider adding error handling (TryCatch nodes)".to_string());
    }

    if avg_branching_factor > 3.0 {
        recommendations
            .push("High branching factor - consider simplifying control flow".to_string());
    }

    let llm_count = node_types.get("LLM").unwrap_or(&0);
    if *llm_count > 5 {
        recommendations.push("Many LLM nodes - consider batching or caching".to_string());
    }

    WorkflowStats {
        total_nodes: workflow.nodes.len(),
        total_edges: workflow.edges.len(),
        start_nodes,
        end_nodes,
        max_depth,
        avg_branching_factor,
        has_cycles,
        complexity_score,
        node_types,
        llm_models,
        vector_dbs,
        has_control_flow: has_conditionals || has_loops || has_error_handling || has_parallel,
        has_conditionals,
        has_loops,
        has_error_handling,
        has_parallel,
        warnings,
        recommendations,
    }
}

fn estimate_max_depth(workflow: &Workflow) -> usize {
    // Simple estimation: longest path from any start node
    let mut adjacency: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
    for edge in &workflow.edges {
        adjacency.entry(edge.from).or_default().push(edge.to);
    }

    let mut max_depth = 0;
    for node in &workflow.nodes {
        if matches!(node.kind, NodeKind::Start) {
            let depth = dfs_depth(&adjacency, node.id, &mut HashMap::new());
            max_depth = max_depth.max(depth);
        }
    }

    max_depth
}

fn dfs_depth(
    adjacency: &HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
    current: uuid::Uuid,
    visited: &mut HashMap<uuid::Uuid, usize>,
) -> usize {
    if let Some(&depth) = visited.get(&current) {
        return depth;
    }

    let mut max_child_depth = 0;
    if let Some(neighbors) = adjacency.get(&current) {
        for &neighbor in neighbors {
            let child_depth = dfs_depth(adjacency, neighbor, visited);
            max_child_depth = max_child_depth.max(child_depth);
        }
    }

    let depth = 1 + max_child_depth;
    visited.insert(current, depth);
    depth
}

fn detect_cycles_simple(workflow: &Workflow) -> bool {
    // Simple cycle detection - check if any node has a path back to itself
    let mut adjacency: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
    for edge in &workflow.edges {
        adjacency.entry(edge.from).or_default().push(edge.to);
    }

    for node in &workflow.nodes {
        let mut visited = std::collections::HashSet::new();
        let mut rec_stack = std::collections::HashSet::new();
        if has_cycle_from(&adjacency, node.id, &mut visited, &mut rec_stack) {
            return true;
        }
    }

    false
}

fn has_cycle_from(
    adjacency: &HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
    current: uuid::Uuid,
    visited: &mut std::collections::HashSet<uuid::Uuid>,
    rec_stack: &mut std::collections::HashSet<uuid::Uuid>,
) -> bool {
    if rec_stack.contains(&current) {
        return true;
    }

    if visited.contains(&current) {
        return false;
    }

    visited.insert(current);
    rec_stack.insert(current);

    if let Some(neighbors) = adjacency.get(&current) {
        for &neighbor in neighbors {
            if has_cycle_from(adjacency, neighbor, visited, rec_stack) {
                return true;
            }
        }
    }

    rec_stack.remove(&current);
    false
}

fn calculate_complexity(workflow: &Workflow, node_types: &HashMap<String, usize>) -> usize {
    // Complexity score based on various factors
    let mut score = workflow.nodes.len(); // Base: number of nodes

    // Add complexity for control flow
    score += node_types.get("IfElse").unwrap_or(&0) * 2;
    score += node_types.get("Switch").unwrap_or(&0) * 3;
    score += node_types.get("Loop").unwrap_or(&0) * 4;
    score += node_types.get("TryCatch").unwrap_or(&0) * 2;
    score += node_types.get("Parallel").unwrap_or(&0) * 3;

    // Add for edges (connectivity)
    score += workflow.edges.len() / 2;

    score
}

fn load_workflow(file: &str) -> Result<Workflow> {
    let path = Path::new(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", file))?;

    let workflow: Workflow = if path.extension().and_then(|s| s.to_str()) == Some("yaml")
        || path.extension().and_then(|s| s.to_str()) == Some("yml")
    {
        serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
    } else {
        serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
    };

    Ok(workflow)
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
