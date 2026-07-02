use anyhow::{Context, Result};
use clap::Subcommand;
use std::fs;
use std::path::Path;

#[derive(Subcommand)]
pub enum WorkflowCommands {
    Validate {
        file: String,
        #[arg(long)]
        strict: bool,
    },
    Create {
        file: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    Import {
        /// Source file or URL to import from
        source: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
        /// Force overwrite if output file exists
        #[arg(short, long)]
        force: bool,
        /// Validate workflow after import
        #[arg(short, long)]
        validate: bool,
    },
    Export {
        file: String,
        #[arg(short, long)]
        output: String,
        #[arg(short, long, value_parser = ["json", "yaml"])]
        format: Option<String>,
    },
    Package {
        /// Workflow file to package
        file: String,
        /// Output package file (.tar.gz or .zip)
        #[arg(short, long)]
        output: String,
        /// Include sub-workflows and dependencies
        #[arg(short, long)]
        include_deps: bool,
    },
    Version {
        file: String,
        #[arg(short, long)]
        output: String,
        #[arg(short, long, value_parser = ["major", "minor", "patch"])]
        bump: String,
        #[arg(short, long)]
        message: String,
    },
    Info {
        file: String,
    },
    Diff {
        file1: String,
        file2: String,
        #[arg(long)]
        verbose: bool,
    },
}

pub async fn handle_workflow_command(command: WorkflowCommands) -> Result<()> {
    match command {
        WorkflowCommands::Validate { file, strict } => validate_workflow(&file, strict).await,
        WorkflowCommands::Create {
            file,
            name,
            description,
        } => create_workflow(&file, name, description).await,
        WorkflowCommands::Import {
            source,
            output,
            force,
            validate,
        } => import_workflow(&source, output, force, validate).await,
        WorkflowCommands::Export {
            file,
            output,
            format,
        } => export_workflow(&file, &output, format).await,
        WorkflowCommands::Package {
            file,
            output,
            include_deps,
        } => package_workflow(&file, &output, include_deps).await,
        WorkflowCommands::Version {
            file,
            output,
            bump,
            message,
        } => create_workflow_version(&file, &output, &bump, &message).await,
        WorkflowCommands::Info { file } => show_workflow_info(&file).await,
        WorkflowCommands::Diff {
            file1,
            file2,
            verbose,
        } => diff_workflows(&file1, &file2, verbose).await,
    }
}

async fn validate_workflow(file: &str, strict: bool) -> Result<()> {
    use oxify_model::validation::WorkflowValidator;

    let path = Path::new(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", file))?;

    // Detect format based on file extension
    let workflow: oxify_model::Workflow = if path.extension().and_then(|s| s.to_str())
        == Some("yaml")
        || path.extension().and_then(|s| s.to_str()) == Some("yml")
    {
        serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
    } else {
        serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
    };

    // Use comprehensive validator
    let validation_result = WorkflowValidator::validate(&workflow);

    match validation_result {
        Ok(report) => {
            println!("✓ Workflow validation PASSED");
            println!("\n📊 Workflow Information:");
            println!("  Name:        {}", workflow.metadata.name);
            println!("  Version:     {}", workflow.metadata.version);
            if let Some(desc) = &workflow.metadata.description {
                println!("  Description: {}", desc);
            }

            println!("\n📈 Statistics:");
            println!("  Total Nodes:  {}", report.stats.total_nodes);
            println!("  Total Edges:  {}", report.stats.total_edges);
            println!("  Start Nodes:  {}", report.stats.start_nodes);
            println!("  End Nodes:    {}", report.stats.end_nodes);
            println!("  Max Depth:    {}", report.stats.max_depth);

            println!("\n🔢 Node Types:");
            let mut sorted_types: Vec<_> = report.stats.node_type_counts.iter().collect();
            sorted_types.sort_by(|a, b| b.1.cmp(a.1));
            for (node_type, count) in sorted_types {
                println!("  {:12} : {}", node_type, count);
            }

            // Show warnings
            if !report.warnings.is_empty() {
                println!("\n⚠️  Warnings ({}):", report.warnings.len());
                for (i, warning) in report.warnings.iter().enumerate() {
                    println!("  {}. {}", i + 1, warning);
                }

                if strict {
                    eprintln!("\n✗ Strict mode enabled: Warnings treated as errors");
                    std::process::exit(1);
                } else {
                    println!("\n💡 Tip: Use --strict to treat warnings as errors");
                }
            } else {
                println!("\n✅ No warnings found");
            }

            Ok(())
        }
        Err(error) => {
            eprintln!("✗ Workflow validation FAILED:");
            eprintln!("\n❌ Error: {}", error);
            eprintln!("\n💡 Fix the error above and try again");
            std::process::exit(1);
        }
    }
}

async fn create_workflow(
    file: &str,
    _name: Option<String>,
    _description: Option<String>,
) -> Result<()> {
    let path = Path::new(file);
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", file))?;

    // Parse workflow based on file extension
    let workflow: oxify_model::Workflow = match path.extension().and_then(|s| s.to_str()) {
        Some("json") => {
            serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
        }
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
        }
        _ => anyhow::bail!("Unsupported file format. Use .json, .yaml, or .yml extension."),
    };

    workflow
        .validate()
        .map_err(|error| anyhow::anyhow!("Validation failed: {}", error))?;

    println!("✓ Workflow created successfully");
    println!("  ID: {}", workflow.metadata.id);
    println!("  Nodes: {}", workflow.nodes.len());
    println!("  Edges: {}", workflow.edges.len());

    Ok(())
}

async fn export_workflow(file: &str, output: &str, format: Option<String>) -> Result<()> {
    let path = Path::new(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", file))?;

    // Parse workflow from source file
    let workflow: oxify_model::Workflow = if path.extension().and_then(|s| s.to_str())
        == Some("yaml")
        || path.extension().and_then(|s| s.to_str()) == Some("yml")
    {
        serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
    } else {
        serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
    };

    // Determine output format
    let output_path = Path::new(output);
    let output_format = format
        .as_deref()
        .or_else(|| {
            output_path
                .extension()
                .and_then(|ext| ext.to_str())
                .and_then(|ext| match ext {
                    "yaml" | "yml" => Some("yaml"),
                    "json" => Some("json"),
                    _ => None,
                })
        })
        .unwrap_or("json");

    // Export to specified format
    let output_content = match output_format {
        "yaml" => {
            serde_yaml::to_string(&workflow).with_context(|| "Failed to serialize to YAML")?
        }
        "json" => serde_json::to_string_pretty(&workflow)
            .with_context(|| "Failed to serialize to JSON")?,
        _ => anyhow::bail!(
            "Unsupported format: {}. Use 'json' or 'yaml'.",
            output_format
        ),
    };

    fs::write(output, &output_content)
        .with_context(|| format!("Failed to write output to: {}", output))?;

    println!("✓ Exported workflow to {} ({})", output, output_format);
    println!("  Nodes: {}", workflow.nodes.len());
    println!("  Edges: {}", workflow.edges.len());

    Ok(())
}

async fn create_workflow_version(
    file: &str,
    output: &str,
    bump: &str,
    message: &str,
) -> Result<()> {
    use oxify_model::VersionBump;

    let path = Path::new(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", file))?;

    // Parse workflow from source file
    let workflow: oxify_model::Workflow = if path.extension().and_then(|s| s.to_str())
        == Some("yaml")
        || path.extension().and_then(|s| s.to_str()) == Some("yml")
    {
        serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
    } else {
        serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
    };

    // Determine version bump type
    let version_bump = match bump {
        "major" => VersionBump::Major,
        "minor" => VersionBump::Minor,
        "patch" => VersionBump::Patch,
        _ => anyhow::bail!(
            "Invalid bump type: {}. Use 'major', 'minor', or 'patch'.",
            bump
        ),
    };

    // Create new version
    let new_workflow = workflow.create_new_version(message.to_string(), version_bump);

    // Determine output format
    let output_path = Path::new(output);
    let output_format = output_path
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| match ext {
            "yaml" | "yml" => Some("yaml"),
            "json" => Some("json"),
            _ => None,
        })
        .unwrap_or("json");

    // Export to specified format
    let output_content = match output_format {
        "yaml" => {
            serde_yaml::to_string(&new_workflow).with_context(|| "Failed to serialize to YAML")?
        }
        "json" => serde_json::to_string_pretty(&new_workflow)
            .with_context(|| "Failed to serialize to JSON")?,
        _ => anyhow::bail!("Unsupported format"),
    };

    fs::write(output, &output_content)
        .with_context(|| format!("Failed to write output to: {}", output))?;

    println!("✓ Created new workflow version");
    println!("  Old version: {}", workflow.metadata.version);
    println!("  New version: {}", new_workflow.metadata.version);
    println!("  Parent ID: {}", workflow.metadata.id);
    println!("  New ID: {}", new_workflow.metadata.id);
    println!("  Change: {}", message);
    println!("  Output: {}", output);

    Ok(())
}

async fn show_workflow_info(file: &str) -> Result<()> {
    let path = Path::new(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", file))?;

    // Parse workflow
    let workflow: oxify_model::Workflow = if path.extension().and_then(|s| s.to_str())
        == Some("yaml")
        || path.extension().and_then(|s| s.to_str()) == Some("yml")
    {
        serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
    } else {
        serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
    };

    // Display comprehensive workflow information
    println!("Workflow Information");
    println!("====================");
    println!();
    println!("Metadata:");
    println!("  Name: {}", workflow.metadata.name);
    println!("  ID: {}", workflow.metadata.id);
    println!("  Version: {}", workflow.metadata.version);
    println!(
        "  Created: {}",
        workflow.metadata.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!(
        "  Updated: {}",
        workflow.metadata.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    if let Some(desc) = &workflow.metadata.description {
        println!("  Description: {}", desc);
    }

    if !workflow.metadata.tags.is_empty() {
        println!("  Tags: {}", workflow.metadata.tags.join(", "));
    }

    if let Some(parent_id) = workflow.metadata.parent_id {
        println!("  Parent ID: {}", parent_id);
    }

    if let Some(change) = &workflow.metadata.change_description {
        println!("  Change Description: {}", change);
    }

    if let Some(schedule) = &workflow.metadata.schedule {
        println!();
        println!("Schedule:");
        println!("  Cron: {}", schedule.cron);
        println!("  Timezone: {}", schedule.timezone);
        println!("  Enabled: {}", schedule.enabled);
        if let Some(max_runs) = schedule.max_concurrent_runs {
            println!("  Max Concurrent Runs: {}", max_runs);
        }
    }

    println!();
    println!("Structure:");
    println!("  Nodes: {}", workflow.nodes.len());
    println!("  Edges: {}", workflow.edges.len());

    // Count node types
    let mut node_types = std::collections::HashMap::new();
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
            oxify_model::NodeKind::Custom(_) => "Custom",
        };
        *node_types.entry(type_name).or_insert(0) += 1;
    }

    println!();
    println!("Node Types:");
    for (type_name, count) in node_types {
        println!("  {}: {}", type_name, count);
    }

    // Validate and report
    println!();
    match workflow.validate() {
        Ok(_) => println!("✓ Workflow is valid"),
        Err(e) => println!("✗ Workflow has validation errors: {}", e),
    }

    Ok(())
}

async fn diff_workflows(file1: &str, file2: &str, verbose: bool) -> Result<()> {
    // Load both workflows
    let workflow1 = load_workflow(file1)?;
    let workflow2 = load_workflow(file2)?;

    println!("Comparing Workflows");
    println!("===================");
    println!();
    println!("File 1: {}", file1);
    println!("  Name: {}", workflow1.metadata.name);
    println!("  Version: {}", workflow1.metadata.version);
    println!("  ID: {}", workflow1.metadata.id);
    println!();
    println!("File 2: {}", file2);
    println!("  Name: {}", workflow2.metadata.name);
    println!("  Version: {}", workflow2.metadata.version);
    println!("  ID: {}", workflow2.metadata.id);
    println!();

    // Check if they're related by version
    if workflow2.metadata.parent_id == Some(workflow1.metadata.id) {
        println!("✓ File 2 is a direct version of File 1");
        if let Some(change_desc) = &workflow2.metadata.change_description {
            println!("  Change: {}", change_desc);
        }
        println!();
    } else if workflow1.metadata.parent_id == Some(workflow2.metadata.id) {
        println!("✓ File 1 is a direct version of File 2");
        if let Some(change_desc) = &workflow1.metadata.change_description {
            println!("  Change: {}", change_desc);
        }
        println!();
    }

    // Compare metadata
    println!("Metadata Changes:");
    if workflow1.metadata.name != workflow2.metadata.name {
        println!(
            "  Name: \"{}\" → \"{}\"",
            workflow1.metadata.name, workflow2.metadata.name
        );
    }
    if workflow1.metadata.description != workflow2.metadata.description {
        println!("  Description changed");
    }
    if workflow1.metadata.tags != workflow2.metadata.tags {
        println!("  Tags changed");
    }

    // Compare structure
    println!();
    println!("Structure Changes:");
    let nodes_diff = workflow2.nodes.len() as i32 - workflow1.nodes.len() as i32;
    let edges_diff = workflow2.edges.len() as i32 - workflow1.edges.len() as i32;

    if nodes_diff != 0 {
        println!(
            "  Nodes: {} → {} ({:+})",
            workflow1.nodes.len(),
            workflow2.nodes.len(),
            nodes_diff
        );
    } else {
        println!("  Nodes: {} (no change)", workflow1.nodes.len());
    }

    if edges_diff != 0 {
        println!(
            "  Edges: {} → {} ({:+})",
            workflow1.edges.len(),
            workflow2.edges.len(),
            edges_diff
        );
    } else {
        println!("  Edges: {} (no change)", workflow1.edges.len());
    }

    if verbose {
        // Detailed node comparison
        println!();
        println!("Detailed Node Changes:");

        // Find added nodes
        let nodes1_ids: std::collections::HashSet<_> =
            workflow1.nodes.iter().map(|n| n.id).collect();
        let nodes2_ids: std::collections::HashSet<_> =
            workflow2.nodes.iter().map(|n| n.id).collect();

        let added: Vec<_> = workflow2
            .nodes
            .iter()
            .filter(|n| !nodes1_ids.contains(&n.id))
            .collect();

        let removed: Vec<_> = workflow1
            .nodes
            .iter()
            .filter(|n| !nodes2_ids.contains(&n.id))
            .collect();

        if !added.is_empty() {
            println!();
            println!("  Added Nodes ({}):", added.len());
            for node in added {
                let node_type = get_node_type_name(&node.kind);
                println!("    + {} [{}]", node.name, node_type);
            }
        }

        if !removed.is_empty() {
            println!();
            println!("  Removed Nodes ({}):", removed.len());
            for node in removed {
                let node_type = get_node_type_name(&node.kind);
                println!("    - {} [{}]", node.name, node_type);
            }
        }

        // Check for modified nodes (same ID but different content)
        let common_ids: Vec<_> = nodes1_ids.intersection(&nodes2_ids).collect();
        let mut modified = Vec::new();

        for id in common_ids {
            let node1 = workflow1
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .expect("invariant: id from intersection of both node sets");
            let node2 = workflow2
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .expect("invariant: id from intersection of both node sets");

            if node1.name != node2.name
                || format!("{:?}", node1.kind) != format!("{:?}", node2.kind)
            {
                modified.push((node1, node2));
            }
        }

        if !modified.is_empty() {
            println!();
            println!("  Modified Nodes ({}):", modified.len());
            for (node1, node2) in modified {
                if node1.name != node2.name {
                    println!("    ~ Name: \"{}\" → \"{}\"", node1.name, node2.name);
                }
                let type1 = get_node_type_name(&node1.kind);
                let type2 = get_node_type_name(&node2.kind);
                if type1 != type2 {
                    println!("    ~ Type: {} → {}", type1, type2);
                }
            }
        }
    }

    // Summary
    println!();
    if nodes_diff == 0 && edges_diff == 0 {
        println!("✓ Workflows have identical structure");
    } else {
        println!("✗ Workflows differ in structure");
    }

    Ok(())
}

fn load_workflow(file: &str) -> Result<oxify_model::Workflow> {
    let path = Path::new(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", file))?;

    let workflow: oxify_model::Workflow = if path.extension().and_then(|s| s.to_str())
        == Some("yaml")
        || path.extension().and_then(|s| s.to_str()) == Some("yml")
    {
        serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
    } else {
        serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
    };

    Ok(workflow)
}

fn get_node_type_name(kind: &oxify_model::NodeKind) -> &str {
    match kind {
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
        oxify_model::NodeKind::Custom(_) => "Custom",
    }
}

/// Import workflow from file or URL
async fn import_workflow(
    source: &str,
    output: Option<String>,
    force: bool,
    validate: bool,
) -> Result<()> {
    println!("Importing workflow from: {}", source);

    // Determine if source is a URL or file path
    let content = if source.starts_with("http://") || source.starts_with("https://") {
        println!("Downloading from URL...");

        // Use oxihttp to download from URL. A dedicated HTTPS-capable client
        // is built (rather than the `oxihttp::get` one-shot helper) because
        // that helper only builds a plain, non-TLS client and this import
        // path explicitly accepts `https://` sources as well.
        let client = oxihttp::Client::builder()
            .with_tls()
            .build_https()
            .context("Failed to build HTTP client")?;
        let response = client
            .get(source)?
            .send()
            .await
            .with_context(|| format!("Failed to download from URL: {}", source))?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP error {}: {}", response.status(), source);
        }

        response
            .body_text()
            .await
            .with_context(|| "Failed to read response body")?
    } else {
        // Read from local file
        let path = Path::new(source);
        if !path.exists() {
            anyhow::bail!("File not found: {}", source);
        }

        fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", source))?
    };

    // Parse workflow (try JSON first, then YAML)
    let workflow: oxify_model::Workflow = serde_json::from_str(&content)
        .or_else(|_| serde_yaml::from_str(&content))
        .with_context(|| "Failed to parse workflow (tried both JSON and YAML)")?;

    // Validate if requested
    if validate {
        println!("Validating workflow...");
        workflow
            .validate()
            .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;
        println!("✓ Workflow is valid");
    }

    // Determine output path
    let output_path = match output {
        Some(path) => path,
        None => {
            // Generate output filename from workflow name
            let sanitized_name = workflow
                .metadata
                .name
                .to_lowercase()
                .replace(' ', "_")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>();
            format!("{}.json", sanitized_name)
        }
    };

    let output_path_obj = Path::new(&output_path);

    // Check if output file exists
    if output_path_obj.exists() && !force {
        anyhow::bail!(
            "Output file already exists: {}. Use --force to overwrite.",
            output_path
        );
    }

    // Determine output format from extension
    let output_format = output_path_obj
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("json");

    // Serialize to output format
    let output_content = match output_format {
        "yaml" | "yml" => {
            serde_yaml::to_string(&workflow).with_context(|| "Failed to serialize to YAML")?
        }
        _ => serde_json::to_string_pretty(&workflow)
            .with_context(|| "Failed to serialize to JSON")?,
    };

    // Write to output file
    fs::write(&output_path, &output_content)
        .with_context(|| format!("Failed to write to file: {}", output_path))?;

    println!("✓ Workflow imported successfully");
    println!("  Name: {}", workflow.metadata.name);
    println!("  ID: {}", workflow.metadata.id);
    println!("  Version: {}", workflow.metadata.version);
    println!("  Nodes: {}", workflow.nodes.len());
    println!("  Edges: {}", workflow.edges.len());
    println!("  Output: {}", output_path);

    Ok(())
}

/// Package workflow with dependencies
async fn package_workflow(file: &str, output: &str, include_deps: bool) -> Result<()> {
    use std::collections::HashSet;

    println!("Packaging workflow: {}", file);

    // Load main workflow
    let workflow = load_workflow(file)?;

    // Collect all workflows to package
    let workflows = [workflow.clone()];
    let mut processed_ids = HashSet::new();
    processed_ids.insert(workflow.metadata.id);

    if include_deps {
        println!("Scanning for sub-workflow dependencies...");

        // Find all sub-workflow nodes
        let mut to_process = vec![workflow.clone()];
        let mut dep_count = 0;

        while let Some(current) = to_process.pop() {
            for node in &current.nodes {
                if let oxify_model::NodeKind::SubWorkflow(sub) = &node.kind {
                    // Use workflow_path as identifier
                    if !processed_ids.contains(&node.id) {
                        // In a real implementation, we would load the sub-workflow from storage
                        // For now, we'll just track that it should be included
                        println!("  Found dependency: {}", sub.workflow_path);
                        processed_ids.insert(node.id);
                        dep_count += 1;
                        // to_process.push(sub_workflow);
                    }
                }
            }
        }

        if dep_count > 0 {
            println!("  Total dependencies found: {}", dep_count);
            println!("  Note: Sub-workflow loading not yet implemented");
        } else {
            println!("  No sub-workflow dependencies found");
        }
    }

    // Create package manifest
    #[derive(serde::Serialize)]
    struct PackageManifest {
        name: String,
        version: String,
        main_workflow_id: uuid::Uuid,
        workflow_count: usize,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let manifest = PackageManifest {
        name: workflow.metadata.name.clone(),
        version: workflow.metadata.version.clone(),
        main_workflow_id: workflow.metadata.id,
        workflow_count: workflows.len(),
        created_at: chrono::Utc::now(),
    };

    // Create temporary directory for package contents
    let temp_dir = std::env::temp_dir().join(format!("oxify_package_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("Failed to create temp directory: {:?}", temp_dir))?;

    // Write manifest
    let manifest_path = temp_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, manifest_json)?;

    // Write workflows
    let workflows_dir = temp_dir.join("workflows");
    fs::create_dir_all(&workflows_dir)?;

    for (idx, wf) in workflows.iter().enumerate() {
        let wf_path = workflows_dir.join(format!("{}.json", idx));
        let wf_json = serde_json::to_string_pretty(wf)?;
        fs::write(&wf_path, wf_json)?;
    }

    // Determine package format from output extension
    let output_path = Path::new(output);
    let extension = output_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("tar.gz");

    // Create archive
    match extension {
        "tar" | "gz" | "tgz" => {
            use oxiarc_archive::tar::TarWriter;
            use oxiarc_deflate::streaming::GzipStreamEncoder;

            let tar_gz = fs::File::create(output)
                .with_context(|| format!("Failed to create output file: {}", output))?;
            // Level 6 matches flate2's previous default compression level.
            let enc = GzipStreamEncoder::new(tar_gz, 6);
            let mut tar = TarWriter::new(enc);

            // Recursively add the package contents under `temp_dir`, mirroring the
            // behaviour of the previous `append_dir_all(".", &temp_dir)` call.
            add_dir_to_tar(&mut tar, &temp_dir, "")
                .with_context(|| "Failed to create tar archive")?;

            // Finalize the tar (writes trailing blocks) and recover the gzip
            // encoder, then write the gzip trailer (CRC-32 + ISIZE).
            let enc = tar
                .into_inner()
                .with_context(|| "Failed to finalize tar archive")?;
            enc.finish()
                .with_context(|| "Failed to finalize gzip stream")?;
        }
        "zip" => {
            use oxiarc_archive::ZipWriter;

            let zip_file = fs::File::create(output)
                .with_context(|| format!("Failed to create output file: {}", output))?;
            let mut zip = ZipWriter::new(zip_file);

            // Add manifest
            let manifest_data =
                fs::read(&manifest_path).with_context(|| "Failed to read manifest file")?;
            zip.add_file("manifest.json", &manifest_data)?;

            // Add workflows
            for entry in fs::read_dir(&workflows_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let file_name = path.file_name().ok_or_else(|| {
                        anyhow::anyhow!("Workflow entry has no file name component: {:?}", path)
                    })?;
                    let name = format!("workflows/{}", file_name.to_string_lossy());
                    let data = fs::read(&path)
                        .with_context(|| format!("Failed to read workflow file: {:?}", path))?;
                    zip.add_file(&name, &data)?;
                }
            }

            zip.finish()?;
        }
        _ => anyhow::bail!(
            "Unsupported archive format: {}. Use .tar.gz, .tgz, or .zip",
            extension
        ),
    }

    // Clean up temp directory
    fs::remove_dir_all(&temp_dir)?;

    println!("✓ Package created successfully");
    println!("  Name: {}", manifest.name);
    println!("  Version: {}", manifest.version);
    println!("  Workflows: {}", manifest.workflow_count);
    println!("  Output: {}", output);
    println!("  Format: {}", extension);

    Ok(())
}

/// Recursively append the contents of `dir` to a [`oxiarc_archive::tar::TarWriter`].
///
/// `prefix` is the slash-separated path of `dir` within the archive (empty for
/// the archive root). Directory entries are emitted before their contents so
/// the resulting TAR mirrors the layout produced by the previous
/// `tar::Builder::append_dir_all(".", dir)` call. Entry names always use `/`
/// separators regardless of the host platform.
fn add_dir_to_tar<W: std::io::Write>(
    tar: &mut oxiarc_archive::tar::TarWriter<W>,
    dir: &Path,
    prefix: &str,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {:?}", dir))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to enumerate directory: {:?}", dir))?;
    // Deterministic ordering so archives are reproducible across runs/platforms.
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        let entry_name = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", prefix, name)
        };

        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to read file type: {:?}", path))?;

        if file_type.is_dir() {
            tar.add_directory(&entry_name)
                .with_context(|| format!("Failed to add directory to archive: {}", entry_name))?;
            add_dir_to_tar(tar, &path, &entry_name)?;
        } else if file_type.is_file() {
            let data =
                fs::read(&path).with_context(|| format!("Failed to read file: {:?}", path))?;
            tar.add_file(&entry_name, &data)
                .with_context(|| format!("Failed to add file to archive: {}", entry_name))?;
        }
        // Symlinks and other special entries are intentionally skipped; the
        // package layout only contains regular files and directories.
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::add_dir_to_tar;
    use oxiarc_archive::tar::TarStreamReader;
    use oxiarc_deflate::streaming::{GzipStreamDecoder, GzipStreamEncoder};
    use std::fs;
    use std::io::Read;
    use std::path::Path;

    /// Exercises the same gzip+tar plumbing used by `package_workflow` for the
    /// `tar`/`gz`/`tgz` formats, and verifies the output is a valid, readable
    /// gzip stream (gzip magic + tar round-trip).
    #[test]
    fn test_gzip_tar_archive_roundtrip() {
        let base = std::env::temp_dir().join(format!("oxify_cli_gztest_{}", uuid::Uuid::new_v4()));
        let src_dir = base.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");

        let payload = b"{\"hello\":\"gzip\"}\n";
        fs::write(src_dir.join("manifest.json"), payload).expect("write payload");

        let archive_path = base.join("package.tar.gz");

        // --- Mirror the production archive creation path ---
        {
            use oxiarc_archive::tar::TarWriter;
            let tar_gz = fs::File::create(&archive_path).expect("create archive");
            let enc = GzipStreamEncoder::new(tar_gz, 6);
            let mut tar = TarWriter::new(enc);
            add_dir_to_tar(&mut tar, &src_dir, "").expect("append dir");
            let enc = tar.into_inner().expect("finalize tar");
            enc.finish().expect("finalize gzip");
        }

        // --- The file must be a valid gzip stream (RFC 1952 magic) ---
        let raw = fs::read(&archive_path).expect("read archive");
        assert!(
            raw.starts_with(&[0x1f, 0x8b]),
            "archive must start with gzip magic bytes"
        );

        // --- And it must decompress + untar back to the original payload ---
        // `GzipStreamDecoder` is `Read`-only (not `Seek`), so use the streaming
        // TAR reader, which only requires `Read`.
        let file = fs::File::open(&archive_path).expect("open archive");
        let mut stream = TarStreamReader::new(GzipStreamDecoder::new(file));
        let mut found = None;
        while let Some(mut entry) = stream.next_entry().expect("read entry") {
            let basename = Path::new(&entry.header.name)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
            if basename.as_deref() == Some("manifest.json") {
                let mut contents = Vec::new();
                entry.read_to_end(&mut contents).expect("read entry");
                found = Some(contents);
            }
        }

        let _ = fs::remove_dir_all(&base);
        assert_eq!(found.as_deref(), Some(&payload[..]));
    }
}
