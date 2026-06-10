//! Workflow Visualization Export
//!
//! This module provides functionality to export workflows to various
//! diagram formats for visualization and documentation purposes.
//!
//! Supported formats:
//! - **Mermaid**: Popular markdown-based diagramming (flowchart syntax)
//! - **Graphviz DOT**: Industry standard graph visualization language
//! - **PlantUML**: UML and diagram generation tool
//!
//! # Example
//!
//! ```rust
//! use oxify_model::{Workflow, WorkflowBuilder, LlmConfig, visualization::WorkflowVisualizer};
//!
//! let llm_config = LlmConfig {
//!     provider: "openai".to_string(),
//!     model: "gpt-4".to_string(),
//!     system_prompt: None,
//!     prompt_template: "{{input}}".to_string(),
//!     temperature: Some(0.7),
//!     max_tokens: Some(100),
//!     tools: vec![],
//!     images: vec![],
//!     extra_params: serde_json::json!({}),
//! };
//!
//! let workflow = WorkflowBuilder::new("example")
//!     .description("Example workflow")
//!     .start("Start")
//!     .llm("Generate text", llm_config)
//!     .end("End")
//!     .build();
//!
//! let visualizer = WorkflowVisualizer::new(&workflow);
//! let mermaid = visualizer.to_mermaid();
//! println!("{}", mermaid);
//! ```

use crate::{Edge, Node, NodeKind, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Visualization format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualizationFormat {
    /// Mermaid flowchart format
    Mermaid,
    /// Graphviz DOT format
    Graphviz,
    /// PlantUML activity diagram format
    PlantUML,
}

/// Visual styling options for workflow diagrams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationStyle {
    /// Show node IDs in addition to names
    pub show_node_ids: bool,

    /// Show edge labels (condition expressions)
    pub show_edge_labels: bool,

    /// Use colors to differentiate node types
    pub use_colors: bool,

    /// Include node descriptions as tooltips/notes
    pub include_descriptions: bool,

    /// Diagram orientation (TB, LR, BT, RL)
    pub orientation: DiagramOrientation,

    /// Group nodes by type
    pub group_by_type: bool,
}

impl Default for VisualizationStyle {
    fn default() -> Self {
        Self {
            show_node_ids: false,
            show_edge_labels: true,
            use_colors: true,
            include_descriptions: false,
            orientation: DiagramOrientation::TopBottom,
            group_by_type: false,
        }
    }
}

/// Diagram layout orientation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagramOrientation {
    /// Top to bottom
    TopBottom,
    /// Left to right
    LeftRight,
    /// Bottom to top
    BottomTop,
    /// Right to left
    RightLeft,
}

impl DiagramOrientation {
    /// Convert to Mermaid orientation code
    fn to_mermaid(self) -> &'static str {
        match self {
            DiagramOrientation::TopBottom => "TB",
            DiagramOrientation::LeftRight => "LR",
            DiagramOrientation::BottomTop => "BT",
            DiagramOrientation::RightLeft => "RL",
        }
    }

    /// Convert to Graphviz rankdir
    fn to_graphviz(self) -> &'static str {
        match self {
            DiagramOrientation::TopBottom => "TB",
            DiagramOrientation::LeftRight => "LR",
            DiagramOrientation::BottomTop => "BT",
            DiagramOrientation::RightLeft => "RL",
        }
    }
}

/// Workflow visualizer for generating diagrams
pub struct WorkflowVisualizer<'a> {
    workflow: &'a Workflow,
    style: VisualizationStyle,
}

impl<'a> WorkflowVisualizer<'a> {
    /// Create a new visualizer for a workflow
    pub fn new(workflow: &'a Workflow) -> Self {
        Self {
            workflow,
            style: VisualizationStyle::default(),
        }
    }

    /// Create a visualizer with custom styling
    pub fn with_style(workflow: &'a Workflow, style: VisualizationStyle) -> Self {
        Self { workflow, style }
    }

    /// Export to Mermaid flowchart format
    pub fn to_mermaid(&self) -> String {
        let mut output = String::new();

        // Header
        output.push_str(&format!(
            "flowchart {}\n",
            self.style.orientation.to_mermaid()
        ));

        // Add title if present
        if let Some(desc) = &self.workflow.metadata.description {
            output.push_str("    %%{ init: {'theme':'base', 'themeVariables': { 'primaryColor':'#ff9900'}}}%%\n");
            output.push_str(&format!("    %% {}\n", desc));
        }

        // Add nodes
        for node in &self.workflow.nodes {
            let node_def = self.mermaid_node_definition(node);
            output.push_str(&format!("    {}\n", node_def));
        }

        output.push('\n');

        // Add edges
        for edge in &self.workflow.edges {
            let edge_def = self.mermaid_edge_definition(edge);
            output.push_str(&format!("    {}\n", edge_def));
        }

        // Add styling if enabled
        if self.style.use_colors {
            output.push('\n');
            output.push_str(&self.mermaid_styling());
        }

        output
    }

    /// Generate Mermaid node definition
    fn mermaid_node_definition(&self, node: &Node) -> String {
        let node_id = self.sanitize_id(&node.id.to_string());
        let label = self.node_label(node);

        // Choose shape based on node type
        let (open, close) = match node.kind {
            NodeKind::Start => ("[", "]"),
            NodeKind::End => ("[", "]"),
            NodeKind::IfElse(_) => ("{", "}"),
            NodeKind::Switch(_) => ("{", "}"),
            NodeKind::Parallel(_) => ("[[", "]]"),
            NodeKind::Loop(_) => ("{{", "}}"),
            NodeKind::Custom(_) => ("(", ")"),
            NodeKind::LLM(_)
            | NodeKind::Retriever(_)
            | NodeKind::Code(_)
            | NodeKind::Tool(_)
            | NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_) => ("(", ")"),
        };

        format!("{}{}\"{}\"{}", node_id, open, label, close)
    }

    /// Generate Mermaid edge definition
    fn mermaid_edge_definition(&self, edge: &Edge) -> String {
        let from_id = self.sanitize_id(&edge.from.to_string());
        let to_id = self.sanitize_id(&edge.to.to_string());

        if self.style.show_edge_labels {
            if let Some(label) = &edge.label {
                return format!("{} -->|\"{}\"| {}", from_id, label, to_id);
            }
        }

        format!("{} --> {}", from_id, to_id)
    }

    /// Generate Mermaid styling classes
    fn mermaid_styling(&self) -> String {
        let mut styling = String::new();

        // Define style classes for different node types
        styling.push_str("    classDef startEnd fill:#90EE90,stroke:#228B22,stroke-width:2px\n");
        styling.push_str("    classDef llm fill:#87CEEB,stroke:#4682B4,stroke-width:2px\n");
        styling.push_str("    classDef code fill:#FFB6C1,stroke:#C71585,stroke-width:2px\n");
        styling.push_str("    classDef decision fill:#FFD700,stroke:#FF8C00,stroke-width:2px\n");
        styling.push_str("    classDef loop fill:#DDA0DD,stroke:#8B008B,stroke-width:2px\n");
        styling.push_str("    classDef parallel fill:#F0E68C,stroke:#BDB76B,stroke-width:2px\n");

        // Apply classes to nodes
        styling.push_str("    classDef custom fill:#8B5CF6,stroke:#6D28D9,stroke-width:2px\n");
        for node in &self.workflow.nodes {
            let node_id = self.sanitize_id(&node.id.to_string());
            let class_name = match node.kind {
                NodeKind::Start | NodeKind::End => "startEnd",
                NodeKind::LLM(_) => "llm",
                NodeKind::Code(_) => "code",
                NodeKind::IfElse(_) | NodeKind::Switch(_) => "decision",
                NodeKind::Loop(_) => "loop",
                NodeKind::Parallel(_) => "parallel",
                NodeKind::Custom(_) => "custom",
                NodeKind::Retriever(_)
                | NodeKind::Tool(_)
                | NodeKind::TryCatch(_)
                | NodeKind::SubWorkflow(_)
                | NodeKind::Approval(_)
                | NodeKind::Form(_)
                | NodeKind::Vision(_) => continue,
            };
            styling.push_str(&format!("    class {} {}\n", node_id, class_name));
        }

        styling
    }

    /// Export to Graphviz DOT format
    pub fn to_graphviz(&self) -> String {
        let mut output = String::new();

        // Header
        output.push_str("digraph workflow {\n");
        output.push_str(&format!(
            "    rankdir={};\n",
            self.style.orientation.to_graphviz()
        ));
        output.push_str("    node [shape=box, style=\"rounded,filled\"];\n");
        output.push_str("    edge [fontsize=10];\n\n");

        // Add workflow metadata as graph label
        if let Some(desc) = &self.workflow.metadata.description {
            output.push_str("    labelloc=\"t\";\n");
            output.push_str(&format!(
                "    label=\"{}\";\n\n",
                self.escape_graphviz(desc)
            ));
        }

        // Add nodes
        for node in &self.workflow.nodes {
            let node_def = self.graphviz_node_definition(node);
            output.push_str(&format!("    {};\n", node_def));
        }

        output.push('\n');

        // Add edges
        for edge in &self.workflow.edges {
            let edge_def = self.graphviz_edge_definition(edge);
            output.push_str(&format!("    {};\n", edge_def));
        }

        output.push_str("}\n");
        output
    }

    /// Generate Graphviz node definition
    fn graphviz_node_definition(&self, node: &Node) -> String {
        let node_id = self.sanitize_id(&node.id.to_string());
        let label = self.escape_graphviz(&self.node_label(node));

        let (shape, color) = match node.kind {
            NodeKind::Start => ("ellipse", "#90EE90"),
            NodeKind::End => ("ellipse", "#FFB6C1"),
            NodeKind::LLM(_) => ("box", "#87CEEB"),
            NodeKind::Code(_) => ("box", "#FFB6C1"),
            NodeKind::IfElse(_) | NodeKind::Switch(_) => ("diamond", "#FFD700"),
            NodeKind::Loop(_) => ("hexagon", "#DDA0DD"),
            NodeKind::Parallel(_) => ("parallelogram", "#F0E68C"),
            NodeKind::Custom(_) => ("box", "#8B5CF6"),
            NodeKind::Retriever(_)
            | NodeKind::Tool(_)
            | NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_) => ("box", "#E0E0E0"),
        };

        if self.style.use_colors {
            format!(
                "{} [label=\"{}\", shape={}, fillcolor=\"{}\"]",
                node_id, label, shape, color
            )
        } else {
            format!("{} [label=\"{}\", shape={}]", node_id, label, shape)
        }
    }

    /// Generate Graphviz edge definition
    fn graphviz_edge_definition(&self, edge: &Edge) -> String {
        let from_id = self.sanitize_id(&edge.from.to_string());
        let to_id = self.sanitize_id(&edge.to.to_string());

        if self.style.show_edge_labels {
            if let Some(label) = &edge.label {
                let escaped_label = self.escape_graphviz(label);
                return format!("{} -> {} [label=\"{}\"]", from_id, to_id, escaped_label);
            }
        }

        format!("{} -> {}", from_id, to_id)
    }

    /// Export to PlantUML activity diagram format
    pub fn to_plantuml(&self) -> String {
        let mut output = String::new();

        // Header
        output.push_str("@startuml\n");

        if let Some(desc) = &self.workflow.metadata.description {
            output.push_str(&format!("title {}\n", desc));
        }

        output.push_str("start\n\n");

        // Build execution order using topological sort
        let execution_order = self.topological_sort();

        // Track visited nodes to handle branching
        let mut visited = HashSet::new();

        for node_id in execution_order {
            if visited.contains(&node_id) {
                continue;
            }
            visited.insert(node_id);

            if let Some(node) = self.workflow.nodes.iter().find(|n| n.id == node_id) {
                let node_def = self.plantuml_node_definition(node);
                output.push_str(&format!("{}\n", node_def));
            }
        }

        output.push_str("\nstop\n");
        output.push_str("@enduml\n");
        output
    }

    /// Generate PlantUML node definition
    fn plantuml_node_definition(&self, node: &Node) -> String {
        let label = self.node_label(node);

        match node.kind {
            NodeKind::Start => "start".to_string(),
            NodeKind::End => "stop".to_string(),
            NodeKind::IfElse(_) => format!("if ({}) then (yes)\n  :proceed;\nelse (no)\n  :alternative;\nendif", label),
            NodeKind::Switch(_) => format!("switch ({})\ncase (option 1)\n  :handle option 1;\ncase (option 2)\n  :handle option 2;\nendswitch", label),
            NodeKind::Loop(_) => format!("while ({})\n  :process;\nendwhile", label),
            NodeKind::Custom(ref cfg) => format!(":Custom Plugin: {}\\n[plugin: {}];", label, cfg.plugin_id),
            NodeKind::LLM(_)
            | NodeKind::Retriever(_)
            | NodeKind::Code(_)
            | NodeKind::Tool(_)
            | NodeKind::TryCatch(_)
            | NodeKind::SubWorkflow(_)
            | NodeKind::Parallel(_)
            | NodeKind::Approval(_)
            | NodeKind::Form(_)
            | NodeKind::Vision(_) => format!(":{};", label),
        }
    }

    /// Perform topological sort on workflow nodes
    fn topological_sort(&self) -> Vec<uuid::Uuid> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_mark = HashSet::new();

        // Build adjacency list
        let mut adj: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
        for edge in &self.workflow.edges {
            adj.entry(edge.from).or_default().push(edge.to);
        }

        // Find start nodes
        let start_nodes: Vec<_> = self
            .workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Start))
            .map(|n| n.id)
            .collect();

        fn visit(
            node: uuid::Uuid,
            adj: &HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
            visited: &mut HashSet<uuid::Uuid>,
            temp_mark: &mut HashSet<uuid::Uuid>,
            result: &mut Vec<uuid::Uuid>,
        ) {
            if visited.contains(&node) {
                return;
            }

            if temp_mark.contains(&node) {
                // Cycle detected, skip
                return;
            }

            temp_mark.insert(node);

            if let Some(neighbors) = adj.get(&node) {
                for &neighbor in neighbors {
                    visit(neighbor, adj, visited, temp_mark, result);
                }
            }

            temp_mark.remove(&node);
            visited.insert(node);
            result.push(node);
        }

        for start in start_nodes {
            visit(start, &adj, &mut visited, &mut temp_mark, &mut result);
        }

        result.reverse();
        result
    }

    /// Generate node label with optional ID
    fn node_label(&self, node: &Node) -> String {
        if self.style.show_node_ids {
            format!("{}\n({})", node.name, &node.id.to_string()[..8])
        } else {
            node.name.clone()
        }
    }

    /// Sanitize ID for use in diagram formats
    fn sanitize_id(&self, id: &str) -> String {
        id.replace('-', "_").chars().take(8).collect::<String>()
    }

    /// Escape special characters for Graphviz
    fn escape_graphviz(&self, s: &str) -> String {
        s.replace('"', "\\\"").replace('\n', "\\n")
    }

    /// Export to specified format
    pub fn export(&self, format: VisualizationFormat) -> String {
        match format {
            VisualizationFormat::Mermaid => self.to_mermaid(),
            VisualizationFormat::Graphviz => self.to_graphviz(),
            VisualizationFormat::PlantUML => self.to_plantuml(),
        }
    }
}

/// Helper function to generate Mermaid diagram from workflow
pub fn workflow_to_mermaid(workflow: &Workflow) -> String {
    WorkflowVisualizer::new(workflow).to_mermaid()
}

/// Helper function to generate Graphviz DOT from workflow
pub fn workflow_to_graphviz(workflow: &Workflow) -> String {
    WorkflowVisualizer::new(workflow).to_graphviz()
}

/// Helper function to generate PlantUML from workflow
pub fn workflow_to_plantuml(workflow: &Workflow) -> String {
    WorkflowVisualizer::new(workflow).to_plantuml()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmConfig, ScriptConfig, WorkflowBuilder};

    fn create_llm_config() -> LlmConfig {
        LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "test".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::json!({}),
        }
    }

    fn create_script_config() -> ScriptConfig {
        ScriptConfig {
            runtime: "rust".to_string(),
            code: "fn main() {}".to_string(),
            inputs: vec![],
            output: "result".to_string(),
        }
    }

    #[test]
    fn test_mermaid_export() {
        let workflow = WorkflowBuilder::new("test")
            .description("Test workflow")
            .start("Start")
            .llm("Generate", create_llm_config())
            .end("End")
            .build();

        let mermaid = workflow_to_mermaid(&workflow);
        assert!(mermaid.contains("flowchart TB"));
        assert!(mermaid.contains("Generate"));
    }

    #[test]
    fn test_graphviz_export() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm("Process", create_llm_config())
            .end("End")
            .build();

        let dot = workflow_to_graphviz(&workflow);
        assert!(dot.contains("digraph workflow"));
        assert!(dot.contains("Process"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_plantuml_export() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm("Action", create_llm_config())
            .end("End")
            .build();

        let plantuml = workflow_to_plantuml(&workflow);
        assert!(plantuml.contains("@startuml"));
        assert!(plantuml.contains("@enduml"));
        assert!(plantuml.contains("Action"));
    }

    #[test]
    fn test_visualization_with_custom_style() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm("Task", create_llm_config())
            .end("End")
            .build();

        let style = VisualizationStyle {
            show_node_ids: true,
            show_edge_labels: true,
            use_colors: false,
            include_descriptions: false,
            orientation: DiagramOrientation::LeftRight,
            group_by_type: false,
        };

        let visualizer = WorkflowVisualizer::with_style(&workflow, style);
        let mermaid = visualizer.to_mermaid();
        assert!(mermaid.contains("flowchart LR"));
    }

    #[test]
    fn test_mermaid_with_colors() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm("LLM", create_llm_config())
            .end("End")
            .build();

        let visualizer = WorkflowVisualizer::new(&workflow);
        let mermaid = visualizer.to_mermaid();
        assert!(mermaid.contains("classDef"));
        assert!(mermaid.contains("class"));
    }

    #[test]
    fn test_export_all_formats() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm("Process", create_llm_config())
            .end("End")
            .build();

        let visualizer = WorkflowVisualizer::new(&workflow);

        let mermaid = visualizer.export(VisualizationFormat::Mermaid);
        assert!(mermaid.contains("flowchart"));

        let graphviz = visualizer.export(VisualizationFormat::Graphviz);
        assert!(graphviz.contains("digraph"));

        let plantuml = visualizer.export(VisualizationFormat::PlantUML);
        assert!(plantuml.contains("@startuml"));
    }

    #[test]
    fn test_diagram_orientations() {
        assert_eq!(DiagramOrientation::TopBottom.to_mermaid(), "TB");
        assert_eq!(DiagramOrientation::LeftRight.to_mermaid(), "LR");
        assert_eq!(DiagramOrientation::BottomTop.to_mermaid(), "BT");
        assert_eq!(DiagramOrientation::RightLeft.to_mermaid(), "RL");
    }

    #[test]
    fn test_node_shapes_in_mermaid() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm("LLM", create_llm_config())
            .end("End")
            .build();

        let mermaid = workflow_to_mermaid(&workflow);
        // Start/End nodes use brackets []
        assert!(mermaid.contains('[') && mermaid.contains(']'));
    }

    #[test]
    fn test_edge_labels() {
        let mut workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm("Process", create_llm_config())
            .end("End")
            .build();

        // Add edge label
        if let Some(edge) = workflow.edges.get_mut(0) {
            edge.label = Some("success".to_string());
        }

        let mermaid = workflow_to_mermaid(&workflow);
        assert!(mermaid.contains("success"));
    }

    #[test]
    fn test_graphviz_colors() {
        let workflow = WorkflowBuilder::new("test")
            .start("Start")
            .llm("LLM", create_llm_config())
            .code("Code", create_script_config())
            .end("End")
            .build();

        let dot = workflow_to_graphviz(&workflow);
        assert!(dot.contains("fillcolor"));
        assert!(dot.contains("#87CEEB")); // LLM color
    }
}
