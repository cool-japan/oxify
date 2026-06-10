//! Comprehensive workflow validation
//!
//! This module provides detailed validation for workflows to catch errors
//! before execution, including cycle detection, orphan nodes, and structural issues.

use crate::{NodeId, NodeKind, Workflow};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, ValidationError>;

#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    #[error("Workflow has no start node")]
    NoStartNode,

    #[error("Workflow has multiple start nodes: {0}")]
    MultipleStartNodes(String),

    #[error("Workflow has no end node")]
    NoEndNode,

    #[error("Workflow has multiple end nodes: {0}")]
    MultipleEndNodes(String),

    #[error("Workflow contains a cycle")]
    CycleDetected,

    #[error("Node {0} is unreachable from start")]
    UnreachableNode(NodeId),

    #[error("Node {0} cannot reach end")]
    DeadEndNode(NodeId),

    #[error("Edge references non-existent node: {0}")]
    InvalidNodeReference(NodeId),

    #[error("Conditional node {0} missing true branch")]
    MissingTrueBranch(NodeId),

    #[error("Conditional node {0} missing false branch")]
    MissingFalseBranch(NodeId),

    #[error("Conditional node {0} has invalid branch: {1}")]
    InvalidConditionalBranch(NodeId, NodeId),

    #[error("Duplicate edge from {0} to {1}")]
    DuplicateEdge(NodeId, NodeId),

    #[error("Workflow has no nodes")]
    EmptyWorkflow,

    #[error("Workflow has no edges")]
    NoEdges,

    #[error("Switch node {0} has no cases defined")]
    SwitchNodeNoCases(NodeId),

    #[error("Switch node {0} has empty switch expression")]
    SwitchNodeEmptyExpression(NodeId),

    #[error("Switch node {0} case has empty match value")]
    SwitchCaseEmptyMatch(NodeId),

    #[error("Parallel node {0} has no tasks defined")]
    ParallelNodeNoTasks(NodeId),

    #[error("Parallel node {0} task '{1}' has empty expression")]
    ParallelTaskEmptyExpression(NodeId, String),

    #[error("Parallel node {0} has duplicate task ID: {1}")]
    ParallelDuplicateTaskId(NodeId, String),

    #[error("Approval node {0} has empty message")]
    ApprovalEmptyMessage(NodeId),

    #[error("Form node {0} has no fields defined")]
    FormNoFields(NodeId),

    #[error("Form node {0} has duplicate field ID: {1}")]
    FormDuplicateFieldId(NodeId, String),

    #[error("Form node {0} field '{1}' has empty label")]
    FormFieldEmptyLabel(NodeId, String),

    #[error("Loop node {0} has empty collection path")]
    LoopEmptyCollectionPath(NodeId),

    #[error("Loop node {0} has empty body expression")]
    LoopEmptyBodyExpression(NodeId),

    #[error("TryCatch node {0} has empty try expression")]
    TryCatchEmptyTryExpression(NodeId),

    #[error("SubWorkflow node {0} has empty workflow path")]
    SubWorkflowEmptyPath(NodeId),
}

/// Comprehensive workflow validator
pub struct WorkflowValidator;

impl WorkflowValidator {
    /// Validate a workflow and return all errors
    pub fn validate(workflow: &Workflow) -> Result<ValidationReport> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Basic structure validation
        if workflow.nodes.is_empty() {
            return Err(ValidationError::EmptyWorkflow);
        }

        // Validate start/end nodes
        if let Err(e) = Self::validate_start_end_nodes(workflow) {
            errors.push(e);
        }

        // Validate node references in edges (must be valid before cycle detection)
        let edges_valid = if let Err(errs) = Self::validate_edge_references(workflow) {
            errors.extend(errs);
            false
        } else {
            true
        };

        // Validate conditional nodes
        if let Err(errs) = Self::validate_conditional_nodes(workflow) {
            errors.extend(errs);
        }

        // Validate new node types (Switch, Parallel, Approval, Form, etc.)
        if let Err(errs) = Self::validate_advanced_nodes(workflow) {
            errors.extend(errs);
        }

        // Only run cycle detection if all edges are valid
        if edges_valid {
            // Detect cycles
            if let Err(e) = Self::detect_cycles(workflow) {
                errors.push(e);
            }

            // Check for unreachable nodes
            if let Err(errs) = Self::find_unreachable_nodes(workflow) {
                warnings.extend(errs);
            }

            // Check for dead-end nodes
            if let Err(errs) = Self::find_dead_end_nodes(workflow) {
                warnings.extend(errs);
            }

            // Check for duplicate edges
            if let Err(errs) = Self::find_duplicate_edges(workflow) {
                warnings.extend(errs);
            }
        }

        if !errors.is_empty() {
            return Err(errors
                .into_iter()
                .next()
                .expect("invariant: errors non-empty from guard"));
        }

        // Calculate stats
        let stats = Self::calculate_stats(workflow);

        Ok(ValidationReport {
            valid: true,
            warnings,
            stats,
        })
    }

    fn validate_start_end_nodes(workflow: &Workflow) -> Result<()> {
        let start_nodes: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Start))
            .collect();

        let end_nodes: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::End))
            .collect();

        if start_nodes.is_empty() {
            return Err(ValidationError::NoStartNode);
        }

        if start_nodes.len() > 1 {
            let ids = start_nodes
                .iter()
                .map(|n| n.id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ValidationError::MultipleStartNodes(ids));
        }

        if end_nodes.is_empty() {
            return Err(ValidationError::NoEndNode);
        }

        if end_nodes.len() > 1 {
            let ids = end_nodes
                .iter()
                .map(|n| n.id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ValidationError::MultipleEndNodes(ids));
        }

        Ok(())
    }

    fn validate_edge_references(
        workflow: &Workflow,
    ) -> std::result::Result<(), Vec<ValidationError>> {
        let node_ids: HashSet<_> = workflow.nodes.iter().map(|n| n.id).collect();
        let mut errors = Vec::new();

        for edge in &workflow.edges {
            if !node_ids.contains(&edge.from) {
                errors.push(ValidationError::InvalidNodeReference(edge.from));
            }
            if !node_ids.contains(&edge.to) {
                errors.push(ValidationError::InvalidNodeReference(edge.to));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_conditional_nodes(
        workflow: &Workflow,
    ) -> std::result::Result<(), Vec<ValidationError>> {
        let node_ids: HashSet<_> = workflow.nodes.iter().map(|n| n.id).collect();
        let mut errors = Vec::new();

        for node in &workflow.nodes {
            if let NodeKind::IfElse(condition) = &node.kind {
                if !node_ids.contains(&condition.true_branch) {
                    errors.push(ValidationError::InvalidConditionalBranch(
                        node.id,
                        condition.true_branch,
                    ));
                }
                if !node_ids.contains(&condition.false_branch) {
                    errors.push(ValidationError::InvalidConditionalBranch(
                        node.id,
                        condition.false_branch,
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_advanced_nodes(
        workflow: &Workflow,
    ) -> std::result::Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        for node in &workflow.nodes {
            match &node.kind {
                // Validate Switch nodes
                NodeKind::Switch(config) => {
                    if config.switch_on.trim().is_empty() {
                        errors.push(ValidationError::SwitchNodeEmptyExpression(node.id));
                    }
                    if config.cases.is_empty() && config.default_case.is_none() {
                        errors.push(ValidationError::SwitchNodeNoCases(node.id));
                    }
                    for case in &config.cases {
                        if case.match_value.trim().is_empty() {
                            errors.push(ValidationError::SwitchCaseEmptyMatch(node.id));
                        }
                    }
                }

                // Validate Parallel nodes
                NodeKind::Parallel(config) => {
                    if config.tasks.is_empty() {
                        errors.push(ValidationError::ParallelNodeNoTasks(node.id));
                    }
                    let mut seen_ids = HashSet::new();
                    for task in &config.tasks {
                        if task.expression.trim().is_empty() {
                            errors.push(ValidationError::ParallelTaskEmptyExpression(
                                node.id,
                                task.id.clone(),
                            ));
                        }
                        if !seen_ids.insert(&task.id) {
                            errors.push(ValidationError::ParallelDuplicateTaskId(
                                node.id,
                                task.id.clone(),
                            ));
                        }
                    }
                }

                // Validate Approval nodes
                NodeKind::Approval(config) if config.message.trim().is_empty() => {
                    errors.push(ValidationError::ApprovalEmptyMessage(node.id));
                }
                NodeKind::Approval(_) => {}

                // Validate Form nodes
                NodeKind::Form(config) => {
                    if config.fields.is_empty() {
                        errors.push(ValidationError::FormNoFields(node.id));
                    }
                    let mut seen_ids = HashSet::new();
                    for field in &config.fields {
                        if field.label.trim().is_empty() {
                            errors.push(ValidationError::FormFieldEmptyLabel(
                                node.id,
                                field.id.clone(),
                            ));
                        }
                        if !seen_ids.insert(&field.id) {
                            errors.push(ValidationError::FormDuplicateFieldId(
                                node.id,
                                field.id.clone(),
                            ));
                        }
                    }
                }

                // Validate Loop nodes
                NodeKind::Loop(config) => match &config.loop_type {
                    crate::LoopType::ForEach {
                        collection_path,
                        body_expression,
                        ..
                    } => {
                        if collection_path.trim().is_empty() {
                            errors.push(ValidationError::LoopEmptyCollectionPath(node.id));
                        }
                        if body_expression.trim().is_empty() {
                            errors.push(ValidationError::LoopEmptyBodyExpression(node.id));
                        }
                    }
                    crate::LoopType::While {
                        body_expression, ..
                    } => {
                        if body_expression.trim().is_empty() {
                            errors.push(ValidationError::LoopEmptyBodyExpression(node.id));
                        }
                    }
                    crate::LoopType::Repeat {
                        body_expression, ..
                    } => {
                        if body_expression.trim().is_empty() {
                            errors.push(ValidationError::LoopEmptyBodyExpression(node.id));
                        }
                    }
                },

                // Validate TryCatch nodes
                NodeKind::TryCatch(config) if config.try_expression.trim().is_empty() => {
                    errors.push(ValidationError::TryCatchEmptyTryExpression(node.id));
                }
                NodeKind::TryCatch(_) => {}

                // Validate SubWorkflow nodes
                NodeKind::SubWorkflow(config) if config.workflow_path.trim().is_empty() => {
                    errors.push(ValidationError::SubWorkflowEmptyPath(node.id));
                }
                NodeKind::SubWorkflow(_) => {}

                // Other node types don't need advanced validation here
                _ => {}
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn detect_cycles(workflow: &Workflow) -> Result<()> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        let mut adj_list: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        // Initialize
        for node in &workflow.nodes {
            in_degree.insert(node.id, 0);
            adj_list.insert(node.id, Vec::new());
        }

        // Build adjacency list
        for edge in &workflow.edges {
            adj_list
                .get_mut(&edge.from)
                .expect("invariant: edge.from populated from workflow.nodes")
                .push(edge.to);
            *in_degree
                .get_mut(&edge.to)
                .expect("invariant: edge.to populated from workflow.nodes") += 1;
        }

        // Kahn's algorithm
        let mut queue: VecDeque<_> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut processed = 0;

        while let Some(node_id) = queue.pop_front() {
            processed += 1;

            if let Some(neighbors) = adj_list.get(&node_id) {
                for &neighbor in neighbors {
                    let deg = in_degree
                        .get_mut(&neighbor)
                        .expect("invariant: neighbor populated from workflow.nodes");
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        if processed != workflow.nodes.len() {
            return Err(ValidationError::CycleDetected);
        }

        Ok(())
    }

    fn find_unreachable_nodes(
        workflow: &Workflow,
    ) -> std::result::Result<(), Vec<ValidationError>> {
        let start_node = workflow
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Start));

        if start_node.is_none() {
            return Ok(()); // Already caught by start/end validation
        }

        let start_id = start_node
            .expect("invariant: is_none guard checked above")
            .id;

        // Build adjacency list
        let mut adj_list: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for node in &workflow.nodes {
            adj_list.insert(node.id, Vec::new());
        }
        for edge in &workflow.edges {
            adj_list
                .get_mut(&edge.from)
                .expect("invariant: edge.from populated from workflow.nodes")
                .push(edge.to);
        }

        // BFS from start
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start_id);
        visited.insert(start_id);

        while let Some(node_id) = queue.pop_front() {
            if let Some(neighbors) = adj_list.get(&node_id) {
                for &neighbor in neighbors {
                    if visited.insert(neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        // Find unreachable nodes
        let errors: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| !visited.contains(&n.id))
            .map(|n| ValidationError::UnreachableNode(n.id))
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn find_dead_end_nodes(workflow: &Workflow) -> std::result::Result<(), Vec<ValidationError>> {
        let end_node = workflow
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::End));

        if end_node.is_none() {
            return Ok(()); // Already caught by start/end validation
        }

        let end_id = end_node.expect("invariant: is_none guard checked above").id;

        // Build reverse adjacency list
        let mut reverse_adj: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for node in &workflow.nodes {
            reverse_adj.insert(node.id, Vec::new());
        }
        for edge in &workflow.edges {
            reverse_adj
                .get_mut(&edge.to)
                .expect("invariant: edge.to populated from workflow.nodes")
                .push(edge.from);
        }

        // BFS from end (backwards)
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(end_id);
        visited.insert(end_id);

        while let Some(node_id) = queue.pop_front() {
            if let Some(predecessors) = reverse_adj.get(&node_id) {
                for &pred in predecessors {
                    if visited.insert(pred) {
                        queue.push_back(pred);
                    }
                }
            }
        }

        // Find dead-end nodes
        let errors: Vec<_> = workflow
            .nodes
            .iter()
            .filter(|n| !visited.contains(&n.id))
            .map(|n| ValidationError::DeadEndNode(n.id))
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn find_duplicate_edges(workflow: &Workflow) -> std::result::Result<(), Vec<ValidationError>> {
        let mut seen = HashSet::new();
        let mut errors = Vec::new();

        for edge in &workflow.edges {
            let pair = (edge.from, edge.to);
            if !seen.insert(pair) {
                errors.push(ValidationError::DuplicateEdge(edge.from, edge.to));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn calculate_stats(workflow: &Workflow) -> ValidationStats {
        let total_nodes = workflow.nodes.len();
        let total_edges = workflow.edges.len();

        let start_nodes = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Start))
            .count();

        let end_nodes = workflow
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::End))
            .count();

        // Calculate max depth using BFS
        let max_depth = Self::calculate_max_depth_bfs(workflow);

        // Count node types
        let mut node_type_counts = HashMap::new();
        for node in &workflow.nodes {
            let type_name = match &node.kind {
                NodeKind::Start => "Start",
                NodeKind::End => "End",
                NodeKind::LLM(_) => "LLM",
                NodeKind::Retriever(_) => "Retriever",
                NodeKind::Code(_) => "Code",
                NodeKind::IfElse(_) => "IfElse",
                NodeKind::Tool(_) => "Tool",
                NodeKind::Loop(_) => "Loop",
                NodeKind::TryCatch(_) => "TryCatch",
                NodeKind::SubWorkflow(_) => "SubWorkflow",
                NodeKind::Switch(_) => "Switch",
                NodeKind::Parallel(_) => "Parallel",
                NodeKind::Approval(_) => "Approval",
                NodeKind::Form(_) => "Form",
                NodeKind::Vision(_) => "Vision",
            };
            *node_type_counts.entry(type_name.to_string()).or_insert(0) += 1;
        }

        ValidationStats {
            total_nodes,
            total_edges,
            start_nodes,
            end_nodes,
            max_depth,
            node_type_counts,
        }
    }

    fn calculate_max_depth_bfs(workflow: &Workflow) -> usize {
        let start_node = workflow
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Start));

        if start_node.is_none() {
            return 0;
        }

        let start_id = start_node
            .expect("invariant: is_none guard checked above")
            .id;

        // Build adjacency list
        let mut adj_list: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for node in &workflow.nodes {
            adj_list.insert(node.id, Vec::new());
        }
        for edge in &workflow.edges {
            adj_list
                .get_mut(&edge.from)
                .expect("invariant: edge.from populated from workflow.nodes")
                .push(edge.to);
        }

        // BFS to calculate depth
        let mut queue = VecDeque::new();
        let mut depths = HashMap::new();

        queue.push_back(start_id);
        depths.insert(start_id, 0);

        let mut max_depth = 0;

        while let Some(node_id) = queue.pop_front() {
            let depth = *depths
                .get(&node_id)
                .expect("invariant: node_id inserted into depths before enqueue");
            max_depth = max_depth.max(depth);

            if let Some(neighbors) = adj_list.get(&node_id) {
                for &neighbor in neighbors {
                    use std::collections::hash_map::Entry;
                    if let Entry::Vacant(e) = depths.entry(neighbor) {
                        e.insert(depth + 1);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        max_depth
    }
}

/// Validation report with detailed statistics
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub valid: bool,
    pub warnings: Vec<ValidationError>,
    pub stats: ValidationStats,
}

/// Validation statistics
#[derive(Debug, Clone)]
pub struct ValidationStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub start_nodes: usize,
    pub end_nodes: usize,
    pub max_depth: usize,
    pub node_type_counts: std::collections::HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Node, WorkflowMetadata};
    use proptest::prelude::*;

    #[test]
    fn test_valid_workflow() {
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), end.clone()],
            edges: vec![Edge::new(start.id, end.id)],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_start_node() {
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![end],
            edges: vec![],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(result, Err(ValidationError::NoStartNode)));
    }

    #[test]
    fn test_cycle_detection() {
        use crate::LlmConfig;

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let llm1 = Node::new(
            "LLM1".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test".to_string(),
                temperature: None,
                max_tokens: None,
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );
        let llm2 = Node::new(
            "LLM2".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "test".to_string(),
                temperature: None,
                max_tokens: None,
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), llm1.clone(), llm2.clone(), end.clone()],
            edges: vec![
                Edge::new(start.id, llm1.id),
                Edge::new(llm1.id, llm2.id),
                Edge::new(llm2.id, llm1.id), // Creates cycle
                Edge::new(llm2.id, end.id),
            ],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(result, Err(ValidationError::CycleDetected)));
    }

    // Property-based tests
    proptest! {
        /// Property: A valid DAG with start and end should always pass validation
        #[test]
        fn prop_valid_dag_passes_validation(node_count in 2usize..10) {
            use crate::LlmConfig;

            // Generate a simple linear workflow (guaranteed to be a DAG)
            let start = Node::new("Start".to_string(), NodeKind::Start);
            let end = Node::new("End".to_string(), NodeKind::End);

            let mut nodes = vec![start.clone()];
            let mut edges = Vec::new();

            // Create intermediate nodes
            let mut prev_id = start.id;
            for i in 0..node_count - 2 {
                let llm = Node::new(
                    format!("LLM{}", i),
                    NodeKind::LLM(LlmConfig {
                        provider: "openai".to_string(),
                        model: "gpt-4".to_string(),
                        system_prompt: None,
                        prompt_template: "test".to_string(),
                        temperature: None,
                        max_tokens: None,
                        tools: vec![],
                        images: vec![],
                        extra_params: serde_json::Value::Null,
                    }),
                );
                edges.push(Edge::new(prev_id, llm.id));
                prev_id = llm.id;
                nodes.push(llm);
            }

            nodes.push(end.clone());
            edges.push(Edge::new(prev_id, end.id));

            let workflow = Workflow {
                metadata: WorkflowMetadata::new("Test".to_string()),
                nodes,
                edges,
            };

            let result = WorkflowValidator::validate(&workflow);
            prop_assert!(result.is_ok(), "Linear DAG should be valid: {:?}", result);
        }

        /// Property: Workflow without start node always fails
        #[test]
        fn prop_no_start_fails(node_count in 1usize..5) {
            let mut nodes = Vec::new();
            for i in 0..node_count {
                nodes.push(Node::new(format!("Node{}", i), NodeKind::End));
            }

            let workflow = Workflow {
                metadata: WorkflowMetadata::new("Test".to_string()),
                nodes,
                edges: vec![],
            };

            let result = WorkflowValidator::validate(&workflow);
            prop_assert!(matches!(result, Err(ValidationError::NoStartNode)));
        }

        /// Property: Workflow without end node always fails
        #[test]
        fn prop_no_end_fails(node_count in 1usize..5) {
            let mut nodes = Vec::new();
            for i in 0..node_count {
                nodes.push(Node::new(format!("Node{}", i), NodeKind::Start));
            }

            let workflow = Workflow {
                metadata: WorkflowMetadata::new("Test".to_string()),
                nodes,
                edges: vec![],
            };

            let result = WorkflowValidator::validate(&workflow);
            prop_assert!(
                matches!(result, Err(ValidationError::NoEndNode) | Err(ValidationError::MultipleStartNodes(_)))
            );
        }

        /// Property: Edge referencing non-existent node always fails
        #[test]
        fn prop_invalid_edge_fails(_dummy in 0..10) {
            let start = Node::new("Start".to_string(), NodeKind::Start);
            let end = Node::new("End".to_string(), NodeKind::End);
            let invalid_id = uuid::Uuid::new_v4(); // Random non-existent ID

            let workflow = Workflow {
                metadata: WorkflowMetadata::new("Test".to_string()),
                nodes: vec![start.clone(), end.clone()],
                edges: vec![
                    Edge::new(start.id, invalid_id), // Invalid edge
                ],
            };

            let result = WorkflowValidator::validate(&workflow);
            prop_assert!(matches!(result, Err(ValidationError::InvalidNodeReference(_))));
        }

        /// Property: Stats are always non-negative and consistent
        #[test]
        fn prop_stats_consistency(node_count in 2usize..20) {
            use crate::LlmConfig;

            let start = Node::new("Start".to_string(), NodeKind::Start);
            let end = Node::new("End".to_string(), NodeKind::End);

            let mut nodes = vec![start.clone()];
            let mut edges = Vec::new();
            let mut prev_id = start.id;

            for i in 0..node_count - 2 {
                let llm = Node::new(
                    format!("LLM{}", i),
                    NodeKind::LLM(LlmConfig {
                        provider: "openai".to_string(),
                        model: "gpt-4".to_string(),
                        system_prompt: None,
                        prompt_template: "test".to_string(),
                        temperature: None,
                        max_tokens: None,
                        tools: vec![],
                        images: vec![],
                        extra_params: serde_json::Value::Null,
                    }),
                );
                edges.push(Edge::new(prev_id, llm.id));
                prev_id = llm.id;
                nodes.push(llm);
            }

            nodes.push(end.clone());
            edges.push(Edge::new(prev_id, end.id));

            let workflow = Workflow {
                metadata: WorkflowMetadata::new("Test".to_string()),
                nodes: nodes.clone(),
                edges: edges.clone(),
            };

            if let Ok(report) = WorkflowValidator::validate(&workflow) {
                prop_assert_eq!(report.stats.total_nodes, nodes.len());
                prop_assert_eq!(report.stats.total_edges, edges.len());
                prop_assert_eq!(report.stats.start_nodes, 1);
                prop_assert_eq!(report.stats.end_nodes, 1);
                prop_assert!(report.stats.max_depth >= node_count - 1);
            }
        }

        /// Property: Duplicate edges are detected as warnings
        #[test]
        fn prop_duplicate_edges_detected(_dummy in 0..10) {
            let start = Node::new("Start".to_string(), NodeKind::Start);
            let end = Node::new("End".to_string(), NodeKind::End);

            let workflow = Workflow {
                metadata: WorkflowMetadata::new("Test".to_string()),
                nodes: vec![start.clone(), end.clone()],
                edges: vec![
                    Edge::new(start.id, end.id),
                    Edge::new(start.id, end.id), // Duplicate
                ],
            };

            let result = WorkflowValidator::validate(&workflow);
            prop_assert!(result.is_ok(), "Duplicate edges should be a warning, not error");
            if let Ok(report) = result {
                let has_duplicate_warning = report.warnings.iter().any(|w| {
                    matches!(w, ValidationError::DuplicateEdge(_, _))
                });
                prop_assert!(has_duplicate_warning, "Should have duplicate edge warning");
            }
        }
    }

    // Validation tests for advanced node types

    #[test]
    fn test_switch_node_empty_expression() {
        use crate::{SwitchCase, SwitchConfig};

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let switch = Node::new(
            "Switch".to_string(),
            NodeKind::Switch(SwitchConfig {
                switch_on: "".to_string(), // Empty expression
                cases: vec![SwitchCase {
                    match_value: "success".to_string(),
                    action: "action1".to_string(),
                }],
                default_case: None,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), switch.clone(), end.clone()],
            edges: vec![Edge::new(start.id, switch.id), Edge::new(switch.id, end.id)],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(
            result,
            Err(ValidationError::SwitchNodeEmptyExpression(_))
        ));
    }

    #[test]
    fn test_switch_node_no_cases() {
        use crate::SwitchConfig;

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let switch = Node::new(
            "Switch".to_string(),
            NodeKind::Switch(SwitchConfig {
                switch_on: "{{status}}".to_string(),
                cases: vec![], // No cases
                default_case: None,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), switch.clone(), end.clone()],
            edges: vec![Edge::new(start.id, switch.id), Edge::new(switch.id, end.id)],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(result, Err(ValidationError::SwitchNodeNoCases(_))));
    }

    #[test]
    fn test_parallel_node_no_tasks() {
        use crate::{ParallelConfig, ParallelStrategy};

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let parallel = Node::new(
            "Parallel".to_string(),
            NodeKind::Parallel(ParallelConfig {
                strategy: ParallelStrategy::WaitAll,
                tasks: vec![], // No tasks
                max_concurrency: None,
                timeout_ms: None,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), parallel.clone(), end.clone()],
            edges: vec![
                Edge::new(start.id, parallel.id),
                Edge::new(parallel.id, end.id),
            ],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(
            result,
            Err(ValidationError::ParallelNodeNoTasks(_))
        ));
    }

    #[test]
    fn test_parallel_node_empty_expression() {
        use crate::{ParallelConfig, ParallelStrategy, ParallelTask};

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let parallel = Node::new(
            "Parallel".to_string(),
            NodeKind::Parallel(ParallelConfig {
                strategy: ParallelStrategy::WaitAll,
                tasks: vec![ParallelTask {
                    id: "task1".to_string(),
                    expression: "".to_string(), // Empty expression
                    description: None,
                }],
                max_concurrency: None,
                timeout_ms: None,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), parallel.clone(), end.clone()],
            edges: vec![
                Edge::new(start.id, parallel.id),
                Edge::new(parallel.id, end.id),
            ],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(
            result,
            Err(ValidationError::ParallelTaskEmptyExpression(_, _))
        ));
    }

    #[test]
    fn test_parallel_node_duplicate_task_id() {
        use crate::{ParallelConfig, ParallelStrategy, ParallelTask};

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let parallel = Node::new(
            "Parallel".to_string(),
            NodeKind::Parallel(ParallelConfig {
                strategy: ParallelStrategy::WaitAll,
                tasks: vec![
                    ParallelTask {
                        id: "task1".to_string(),
                        expression: "{{expr1}}".to_string(),
                        description: None,
                    },
                    ParallelTask {
                        id: "task1".to_string(), // Duplicate ID
                        expression: "{{expr2}}".to_string(),
                        description: None,
                    },
                ],
                max_concurrency: None,
                timeout_ms: None,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), parallel.clone(), end.clone()],
            edges: vec![
                Edge::new(start.id, parallel.id),
                Edge::new(parallel.id, end.id),
            ],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(
            result,
            Err(ValidationError::ParallelDuplicateTaskId(_, _))
        ));
    }

    #[test]
    fn test_approval_node_empty_message() {
        use crate::ApprovalConfig;

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let approval = Node::new(
            "Approval".to_string(),
            NodeKind::Approval(ApprovalConfig {
                message: "".to_string(), // Empty message
                description: None,
                approvers: vec![],
                timeout_seconds: None,
                context_data: serde_json::Value::Null,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), approval.clone(), end.clone()],
            edges: vec![
                Edge::new(start.id, approval.id),
                Edge::new(approval.id, end.id),
            ],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(
            result,
            Err(ValidationError::ApprovalEmptyMessage(_))
        ));
    }

    #[test]
    fn test_form_node_no_fields() {
        use crate::FormConfig;

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let form = Node::new(
            "Form".to_string(),
            NodeKind::Form(FormConfig {
                title: "Test Form".to_string(),
                description: None,
                fields: vec![], // No fields
                timeout_seconds: None,
                allowed_submitters: vec![],
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), form.clone(), end.clone()],
            edges: vec![Edge::new(start.id, form.id), Edge::new(form.id, end.id)],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(result, Err(ValidationError::FormNoFields(_))));
    }

    #[test]
    fn test_form_node_duplicate_field_id() {
        use crate::{FormConfig, FormField, FormFieldType};

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let form = Node::new(
            "Form".to_string(),
            NodeKind::Form(FormConfig {
                title: "Test Form".to_string(),
                description: None,
                fields: vec![
                    FormField {
                        id: "field1".to_string(),
                        label: "Field 1".to_string(),
                        field_type: FormFieldType::Text,
                        required: false,
                        default_value: None,
                        validation: None,
                        options: vec![],
                    },
                    FormField {
                        id: "field1".to_string(), // Duplicate ID
                        label: "Field 2".to_string(),
                        field_type: FormFieldType::Text,
                        required: false,
                        default_value: None,
                        validation: None,
                        options: vec![],
                    },
                ],
                timeout_seconds: None,
                allowed_submitters: vec![],
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), form.clone(), end.clone()],
            edges: vec![Edge::new(start.id, form.id), Edge::new(form.id, end.id)],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(matches!(
            result,
            Err(ValidationError::FormDuplicateFieldId(_, _))
        ));
    }

    #[test]
    fn test_valid_switch_node() {
        use crate::{SwitchCase, SwitchConfig};

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let switch = Node::new(
            "Switch".to_string(),
            NodeKind::Switch(SwitchConfig {
                switch_on: "{{status}}".to_string(),
                cases: vec![
                    SwitchCase {
                        match_value: "success".to_string(),
                        action: "action1".to_string(),
                    },
                    SwitchCase {
                        match_value: "error".to_string(),
                        action: "action2".to_string(),
                    },
                ],
                default_case: Some("default_action".to_string()),
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), switch.clone(), end.clone()],
            edges: vec![Edge::new(start.id, switch.id), Edge::new(switch.id, end.id)],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_parallel_node() {
        use crate::{ParallelConfig, ParallelStrategy, ParallelTask};

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let parallel = Node::new(
            "Parallel".to_string(),
            NodeKind::Parallel(ParallelConfig {
                strategy: ParallelStrategy::Race,
                tasks: vec![
                    ParallelTask {
                        id: "task1".to_string(),
                        expression: "{{query1}}".to_string(),
                        description: Some("First task".to_string()),
                    },
                    ParallelTask {
                        id: "task2".to_string(),
                        expression: "{{query2}}".to_string(),
                        description: Some("Second task".to_string()),
                    },
                ],
                max_concurrency: Some(2),
                timeout_ms: Some(10000),
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let workflow = Workflow {
            metadata: WorkflowMetadata::new("Test".to_string()),
            nodes: vec![start.clone(), parallel.clone(), end.clone()],
            edges: vec![
                Edge::new(start.id, parallel.id),
                Edge::new(parallel.id, end.id),
            ],
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(result.is_ok());
    }
}
