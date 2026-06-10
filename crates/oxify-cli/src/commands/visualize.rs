use anyhow::{Context, Result};
use oxify_model::{Node, NodeId, NodeKind, Workflow};
use std::fs;
use std::path::Path;

pub async fn visualize_workflow(file: &str, format: &str, output: Option<String>) -> Result<()> {
    let path = Path::new(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read workflow file: {}", file))?;

    // Parse workflow
    let workflow: Workflow = match path.extension().and_then(|s| s.to_str()) {
        Some("json") => {
            serde_json::from_str(&content).with_context(|| "Failed to parse workflow JSON")?
        }
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow YAML")?
        }
        _ => serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content))
            .with_context(|| "Failed to parse workflow")?,
    };

    let visualization = match format {
        "dot" => generate_dot(&workflow),
        "ascii" => generate_ascii(&workflow),
        _ => anyhow::bail!("Unsupported format: {}. Use 'dot' or 'ascii'.", format),
    };

    if let Some(output_path) = output {
        fs::write(&output_path, &visualization)
            .with_context(|| format!("Failed to write output to: {}", output_path))?;
        println!("✓ Visualization saved to: {}", output_path);
    } else {
        println!("{}", visualization);
    }

    Ok(())
}

fn generate_dot(workflow: &Workflow) -> String {
    let mut dot = String::new();
    dot.push_str("digraph workflow {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  node [shape=box, style=rounded];\n\n");

    // Add nodes
    for node in &workflow.nodes {
        let (shape, color) = match &node.kind {
            NodeKind::Start => ("circle", "lightgreen"),
            NodeKind::End => ("circle", "lightcoral"),
            NodeKind::LLM(_) => ("box", "lightblue"),
            NodeKind::Retriever(_) => ("box", "lightyellow"),
            NodeKind::Code(_) => ("box", "lightgray"),
            NodeKind::IfElse(_) => ("diamond", "lightpink"),
            NodeKind::Tool(_) => ("box", "lightcyan"),
            NodeKind::Loop(_) => ("box", "lavender"),
            NodeKind::TryCatch(_) => ("box", "lightsalmon"),
            NodeKind::SubWorkflow(_) => ("box", "lightsteelblue"),
            NodeKind::Switch(_) => ("diamond", "plum"),
            NodeKind::Parallel(_) => ("box", "wheat"),
            NodeKind::Approval(_) => ("box", "mistyrose"),
            NodeKind::Form(_) => ("box", "honeydew"),
            NodeKind::Vision(_) => ("box", "lightseagreen"),
            NodeKind::Custom(_) => ("box", "mediumpurple"),
        };

        let label = node.name.replace("\"", "\\\"");
        dot.push_str(&format!(
            "  \"{}\" [label=\"{}\", shape={}, fillcolor={}, style=filled];\n",
            node.id, label, shape, color
        ));
    }

    dot.push('\n');

    // Add edges
    for edge in &workflow.edges {
        dot.push_str(&format!("  \"{}\" -> \"{}\";\n", edge.from, edge.to));
    }

    dot.push_str("}\n");
    dot
}

fn generate_ascii(workflow: &Workflow) -> String {
    let mut output = String::new();
    output.push_str(&format!("Workflow: {}\n", workflow.metadata.name));
    output.push_str(&format!(
        "Nodes: {}, Edges: {}\n\n",
        workflow.nodes.len(),
        workflow.edges.len()
    ));

    // Find start node
    let start_nodes: Vec<&Node> = workflow
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Start))
        .collect();

    if start_nodes.is_empty() {
        output.push_str("Warning: No start node found\n");
        return output;
    }

    // Simple tree representation
    let mut visited = std::collections::HashSet::new();
    for start in start_nodes {
        render_node_tree(workflow, start, &mut visited, 0, &mut output);
    }

    output
}

fn render_node_tree(
    workflow: &Workflow,
    node: &Node,
    visited: &mut std::collections::HashSet<NodeId>,
    depth: usize,
    output: &mut String,
) {
    if visited.contains(&node.id) {
        output.push_str(&format!(
            "{}↻ {} (already visited)\n",
            "  ".repeat(depth),
            node.name
        ));
        return;
    }

    visited.insert(node.id);

    let node_type = match &node.kind {
        NodeKind::Start => "[START]",
        NodeKind::End => "[END]",
        NodeKind::LLM(_) => "[LLM]",
        NodeKind::Retriever(_) => "[RETRIEVER]",
        NodeKind::Code(_) => "[CODE]",
        NodeKind::IfElse(_) => "[IF-ELSE]",
        NodeKind::Tool(_) => "[TOOL]",
        NodeKind::Loop(_) => "[LOOP]",
        NodeKind::TryCatch(_) => "[TRY-CATCH]",
        NodeKind::SubWorkflow(_) => "[SUB-WORKFLOW]",
        NodeKind::Switch(_) => "[SWITCH]",
        NodeKind::Parallel(_) => "[PARALLEL]",
        NodeKind::Approval(_) => "[APPROVAL]",
        NodeKind::Form(_) => "[FORM]",
        NodeKind::Vision(_) => "[VISION]",
        NodeKind::Custom(_) => "[CUSTOM]",
    };

    output.push_str(&format!(
        "{}→ {} {}\n",
        "  ".repeat(depth),
        node_type,
        node.name
    ));

    // Find outgoing edges
    let outgoing = workflow.get_outgoing_edges(&node.id);
    for edge in outgoing {
        if let Some(next_node) = workflow.get_node(&edge.to) {
            render_node_tree(workflow, next_node, visited, depth + 1, output);
        }
    }
}
