//! Workflow visualization and export utilities
//!
//! This module provides tools for visualizing workflows in various formats:
//! - DOT/Graphviz format for graph visualization
//! - ASCII art for terminal display
//! - Mermaid diagram format for documentation

use oxify_model::{Node, NodeId, NodeKind, Workflow};
use std::collections::HashMap;

/// Visualization format options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizationFormat {
    /// GraphViz DOT format
    Dot,

    /// Mermaid diagram format
    Mermaid,

    /// ASCII art for terminal
    Ascii,
}

/// Workflow visualizer
pub struct WorkflowVisualizer {
    /// Show node details in output
    pub show_details: bool,

    /// Include execution metadata
    pub include_metadata: bool,

    /// Color scheme for different node types
    pub use_colors: bool,
}

impl Default for WorkflowVisualizer {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowVisualizer {
    /// Create a new visualizer with default settings
    pub fn new() -> Self {
        Self {
            show_details: true,
            include_metadata: false,
            use_colors: true,
        }
    }

    /// Export workflow to DOT format (GraphViz)
    pub fn to_dot(&self, workflow: &Workflow) -> String {
        let mut output = String::new();

        // Header
        output.push_str("digraph workflow {\n");
        output.push_str("  rankdir=TB;\n");
        output.push_str("  node [shape=box, style=rounded];\n");
        output.push_str("  graph [splines=ortho];\n\n");

        // Add workflow metadata as label
        if self.include_metadata {
            output.push_str(&format!(
                "  label=\"{}\\nNodes: {}\\nEdges: {}\";\n",
                workflow.metadata.name,
                workflow.nodes.len(),
                workflow.edges.len()
            ));
            output.push_str("  labelloc=t;\n\n");
        }

        // Add nodes
        for node in &workflow.nodes {
            let (shape, color, style) = self.get_node_style(&node.kind);
            let label = self.get_node_label(node);

            output.push_str(&format!(
                "  \"{}\" [label=\"{}\", shape={}, fillcolor=\"{}\", style=\"{}\"];\n",
                node.id, label, shape, color, style
            ));
        }

        output.push('\n');

        // Add edges
        for edge in &workflow.edges {
            let edge_label = if let Some(condition) = &edge.condition {
                format!(" [label=\"{}\"]", condition)
            } else {
                String::new()
            };

            output.push_str(&format!(
                "  \"{}\" -> \"{}\"{};\n",
                edge.from, edge.to, edge_label
            ));
        }

        output.push_str("}\n");
        output
    }

    /// Export workflow to Mermaid format
    pub fn to_mermaid(&self, workflow: &Workflow) -> String {
        let mut output = String::new();

        // Header
        output.push_str("flowchart TD\n");

        // Add nodes with sanitized IDs (Mermaid doesn't like UUIDs with dashes)
        for node in &workflow.nodes {
            let node_id = self.sanitize_id(&node.id.to_string());
            let label = self.get_node_label(node);
            let (prefix, suffix) = self.get_mermaid_node_style(&node.kind);

            output.push_str(&format!("  {}{}\"{}\"{}", node_id, prefix, label, suffix));

            // Add node type class
            let node_class = self.get_node_class(&node.kind);
            output.push_str(&format!(":::{}\n", node_class));
        }

        output.push('\n');

        // Add edges
        for edge in &workflow.edges {
            let source_id = self.sanitize_id(&edge.from.to_string());
            let target_id = self.sanitize_id(&edge.to.to_string());

            if let Some(condition) = &edge.condition {
                output.push_str(&format!(
                    "  {} -->|\"{}\"| {}\n",
                    source_id, condition, target_id
                ));
            } else {
                output.push_str(&format!("  {} --> {}\n", source_id, target_id));
            }
        }

        // Add style classes
        output.push_str("\n  classDef start fill:#90EE90,stroke:#006400;\n");
        output.push_str("  classDef end fill:#FFB6C1,stroke:#8B0000;\n");
        output.push_str("  classDef llm fill:#87CEEB,stroke:#00008B;\n");
        output.push_str("  classDef retriever fill:#DDA0DD,stroke:#4B0082;\n");
        output.push_str("  classDef code fill:#F0E68C,stroke:#8B8000;\n");
        output.push_str("  classDef tool fill:#FFA07A,stroke:#8B0000;\n");
        output.push_str("  classDef control fill:#FFE4B5,stroke:#8B4500;\n");
        output.push_str("  classDef custom fill:#8B5CF6,stroke:#6D28D9,stroke-width:2px;\n");

        output
    }

    /// Export workflow to ASCII art
    pub fn to_ascii(&self, workflow: &Workflow) -> String {
        let mut output = String::new();

        output.push_str(&format!("Workflow: {}\n", workflow.metadata.name));
        output.push_str(&format!(
            "Nodes: {}, Edges: {}\n\n",
            workflow.nodes.len(),
            workflow.edges.len()
        ));

        // Build adjacency list
        let mut adj: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for edge in &workflow.edges {
            adj.entry(edge.from).or_default().push(edge.to);
        }

        // Find start node
        let start_node = workflow
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Start));

        if let Some(start) = start_node {
            self.print_node_tree(
                &mut output,
                start,
                &workflow.nodes,
                &adj,
                0,
                &mut Vec::new(),
            );
        } else {
            output.push_str("No start node found\n");
        }

        output
    }

    /// Get node style for DOT format
    fn get_node_style(&self, kind: &NodeKind) -> (&str, &str, &str) {
        match kind {
            NodeKind::Start => ("ellipse", "#90EE90", "filled"),
            NodeKind::End => ("ellipse", "#FFB6C1", "filled"),
            NodeKind::LLM(_) => ("box", "#87CEEB", "filled,rounded"),
            NodeKind::Retriever(_) => ("box", "#DDA0DD", "filled,rounded"),
            NodeKind::Code(_) => ("box", "#F0E68C", "filled,rounded"),
            NodeKind::Tool(_) => ("box", "#FFA07A", "filled,rounded"),
            NodeKind::IfElse(_) | NodeKind::Switch(_) => ("diamond", "#FFE4B5", "filled"),
            NodeKind::Loop(_) => ("box", "#FFD700", "filled,rounded"),
            NodeKind::TryCatch(_) => ("box", "#FF6347", "filled,rounded"),
            NodeKind::SubWorkflow(_) => ("box", "#9370DB", "filled,rounded,bold"),
            NodeKind::Parallel(_) => ("parallelogram", "#20B2AA", "filled"),
            NodeKind::Approval(_) => ("hexagon", "#FF69B4", "filled"),
            NodeKind::Form(_) => ("note", "#FFC0CB", "filled"),
            NodeKind::Vision(_) => ("box", "#98FB98", "filled,rounded"),
            NodeKind::Custom(_) => ("box", "#8B5CF6", "filled,rounded"),
        }
    }

    /// Get Mermaid node style
    fn get_mermaid_node_style(&self, kind: &NodeKind) -> (&str, &str) {
        match kind {
            NodeKind::Start => ("([", "])"),
            NodeKind::End => ("([", "])"),
            NodeKind::IfElse(_) | NodeKind::Switch(_) => ("{", "}"),
            NodeKind::Parallel(_) => ("[/", "/]"),
            NodeKind::LLM(_)
            | NodeKind::Retriever(_)
            | NodeKind::Code(_)
            | NodeKind::Tool(_)
            | NodeKind::Loop(_)
            | NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_)
            | NodeKind::Custom(_) => ("[", "]"),
        }
    }

    /// Get node class for Mermaid
    fn get_node_class(&self, kind: &NodeKind) -> &str {
        match kind {
            NodeKind::Start => "start",
            NodeKind::End => "end",
            NodeKind::LLM(_) => "llm",
            NodeKind::Retriever(_) => "retriever",
            NodeKind::Code(_) => "code",
            NodeKind::Tool(_) => "tool",
            NodeKind::IfElse(_) | NodeKind::Switch(_) | NodeKind::Loop(_) => "control",
            NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Parallel(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_) => "control",
            NodeKind::Custom(_) => "custom",
        }
    }

    /// Get node label
    fn get_node_label(&self, node: &Node) -> String {
        if self.show_details {
            let type_name = self.get_node_type_name(&node.kind);
            format!("{}\\n({})", node.name, type_name)
        } else {
            node.name.clone()
        }
    }

    /// Get human-readable node type name
    fn get_node_type_name(&self, kind: &NodeKind) -> String {
        match kind {
            NodeKind::Start => "Start".to_string(),
            NodeKind::End => "End".to_string(),
            NodeKind::LLM(cfg) => format!("LLM: {}", cfg.model),
            NodeKind::Retriever(cfg) => format!("Retriever: {}", cfg.db_type),
            NodeKind::Code(_) => "Code".to_string(),
            NodeKind::Tool(cfg) => format!("Tool: {}", cfg.server_id),
            NodeKind::IfElse(_) => "If-Else".to_string(),
            NodeKind::Switch(_) => "Switch".to_string(),
            NodeKind::Loop(_) => "Loop".to_string(),
            NodeKind::TryCatch(_) => "Try-Catch".to_string(),
            NodeKind::SubWorkflow(_) => "SubWorkflow".to_string(),
            NodeKind::Parallel(_) => "Parallel".to_string(),
            NodeKind::Approval(_) => "Approval".to_string(),
            NodeKind::Form(_) => "Form".to_string(),
            NodeKind::Vision(cfg) => format!("Vision: {}", cfg.provider),
            NodeKind::Custom(cfg) => format!("Plugin: {}", cfg.plugin_id),
        }
    }

    /// Sanitize ID for Mermaid (replace dashes with underscores)
    fn sanitize_id(&self, id: &str) -> String {
        id.replace('-', "_")
    }

    /// Print node tree recursively (for ASCII output)
    fn print_node_tree(
        &self,
        output: &mut String,
        node: &Node,
        all_nodes: &[Node],
        adj: &HashMap<NodeId, Vec<NodeId>>,
        depth: usize,
        visited: &mut Vec<NodeId>,
    ) {
        // Avoid infinite loops
        if visited.contains(&node.id) {
            return;
        }
        visited.push(node.id);

        // Print indentation
        for _ in 0..depth {
            output.push_str("  ");
        }

        // Print node
        let type_name = self.get_node_type_name(&node.kind);
        output.push_str(&format!("└─ {} ({})\n", node.name, type_name));

        // Print children
        if let Some(children) = adj.get(&node.id) {
            for child_id in children {
                if let Some(child_node) = all_nodes.iter().find(|n| n.id == *child_id) {
                    self.print_node_tree(output, child_node, all_nodes, adj, depth + 1, visited);
                }
            }
        }
    }
}

/// Export workflow to DOT format
pub fn export_to_dot(workflow: &Workflow) -> String {
    WorkflowVisualizer::new().to_dot(workflow)
}

/// Export workflow to Mermaid format
pub fn export_to_mermaid(workflow: &Workflow) -> String {
    WorkflowVisualizer::new().to_mermaid(workflow)
}

/// Export workflow to ASCII art
pub fn export_to_ascii(workflow: &Workflow) -> String {
    WorkflowVisualizer::new().to_ascii(workflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, LlmConfig};

    fn create_test_workflow() -> Workflow {
        let mut workflow = Workflow::new("test_workflow".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let start_id = start.id;
        workflow.add_node(start);

        let llm = Node::new(
            "LLM".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "Test".to_string(),
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );
        let llm_id = llm.id;
        workflow.add_node(llm);

        let end = Node::new("End".to_string(), NodeKind::End);
        let end_id = end.id;
        workflow.add_node(end);

        workflow.add_edge(Edge::new(start_id, llm_id));
        workflow.add_edge(Edge::new(llm_id, end_id));

        workflow
    }

    #[test]
    fn test_dot_export() {
        let workflow = create_test_workflow();
        let visualizer = WorkflowVisualizer::new();
        let dot = visualizer.to_dot(&workflow);

        assert!(dot.contains("digraph workflow"));
        assert!(dot.contains("Start"));
        assert!(dot.contains("LLM"));
        assert!(dot.contains("End"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_mermaid_export() {
        let workflow = create_test_workflow();
        let visualizer = WorkflowVisualizer::new();
        let mermaid = visualizer.to_mermaid(&workflow);

        assert!(mermaid.contains("flowchart TD"));
        assert!(mermaid.contains("Start"));
        assert!(mermaid.contains("LLM"));
        assert!(mermaid.contains("End"));
        assert!(mermaid.contains("-->"));
    }

    #[test]
    fn test_ascii_export() {
        let workflow = create_test_workflow();
        let visualizer = WorkflowVisualizer::new();
        let ascii = visualizer.to_ascii(&workflow);

        assert!(ascii.contains("Workflow: test_workflow"));
        assert!(ascii.contains("Start"));
        assert!(ascii.contains("LLM"));
        assert!(ascii.contains("End"));
    }

    #[test]
    fn test_visualizer_options() {
        let workflow = create_test_workflow();

        let mut visualizer = WorkflowVisualizer::new();
        visualizer.show_details = false;
        let dot = visualizer.to_dot(&workflow);

        // Should have node names but not type details
        assert!(dot.contains("Start"));
        assert!(!dot.contains("\\n"));
    }

    #[test]
    fn test_export_functions() {
        let workflow = create_test_workflow();

        let dot = export_to_dot(&workflow);
        assert!(dot.contains("digraph"));

        let mermaid = export_to_mermaid(&workflow);
        assert!(mermaid.contains("flowchart"));

        let ascii = export_to_ascii(&workflow);
        assert!(ascii.contains("Workflow:"));
    }
}
