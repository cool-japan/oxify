use anyhow::Result;
use oxify_model::{Edge, LlmConfig, Node, NodeKind, VectorConfig, Workflow};
use std::fs;

pub async fn init_template(template_type: &str, output: Option<String>) -> Result<()> {
    let workflow = match template_type {
        "rag" => create_rag_workflow(),
        "simple" => create_simple_workflow(),
        "chain" => create_chain_workflow(),
        "parallel" => create_parallel_workflow(),
        "conditional" => create_conditional_workflow(),
        "loop" => create_loop_workflow(),
        "error-handling" => create_error_handling_workflow(),
        "agent" => create_agent_workflow(),
        "subworkflow" => create_subworkflow_workflow(),
        _ => anyhow::bail!("Unknown template type: {}. Available: rag, simple, chain, parallel, conditional, loop, error-handling, agent, subworkflow", template_type),
    };

    if let Some(output_path) = output {
        // Determine format from extension
        let content = if output_path.ends_with(".yaml") || output_path.ends_with(".yml") {
            serde_yaml::to_string(&workflow)?
        } else {
            serde_json::to_string_pretty(&workflow)?
        };

        fs::write(&output_path, &content)?;
        println!(
            "✓ Created {} workflow template at: {}",
            template_type, output_path
        );
    } else {
        // Default to JSON for stdout
        let json = serde_json::to_string_pretty(&workflow)?;
        println!("{}", json);
    }

    Ok(())
}

fn create_simple_workflow() -> Workflow {
    let mut workflow = Workflow::new("Simple LLM Workflow".to_string());
    workflow.metadata.description = Some("A simple workflow with a single LLM call".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 100.0);

    let llm = Node::new(
        "LLM".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some("You are a helpful assistant.".to_string()),
            prompt_template: "Answer this question: {{query}}".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(500),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(300.0, 100.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(500.0, 100.0);

    let start_id = start.id;
    let llm_id = llm.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(llm);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, llm_id));
    workflow.add_edge(Edge::new(llm_id, end_id));

    workflow
}

fn create_rag_workflow() -> Workflow {
    let mut workflow = Workflow::new("RAG Workflow".to_string());
    workflow.metadata.description =
        Some("Retrieval-Augmented Generation workflow with vector search".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 200.0);

    let retriever = Node::new(
        "Search Knowledge Base".to_string(),
        NodeKind::Retriever(VectorConfig {
            db_type: "qdrant".to_string(),
            collection: "knowledge_base".to_string(),
            query: "{{query}}".to_string(),
            top_k: 5,
            score_threshold: Some(0.7),
        }),
    )
    .with_position(300.0, 200.0);

    let llm = Node::new("Generate Answer".to_string(), NodeKind::LLM(LlmConfig {
        provider: "openai".to_string(),
        model: "gpt-4".to_string(),
        system_prompt: Some("You are a helpful assistant. Use only the provided context to answer questions.".to_string()),
        prompt_template: "Based on the following context, answer the query.\n\nContext: {{retriever_results}}\n\nQuery: {{query}}\n\nAnswer:".to_string(),
        temperature: Some(0.3),
        max_tokens: Some(500),
        tools: vec![],
        images: vec![],
        extra_params: serde_json::Value::Null,
    })).with_position(500.0, 200.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(700.0, 200.0);

    let start_id = start.id;
    let retriever_id = retriever.id;
    let llm_id = llm.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(retriever);
    workflow.add_node(llm);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, retriever_id));
    workflow.add_edge(Edge::new(retriever_id, llm_id));
    workflow.add_edge(Edge::new(llm_id, end_id));

    workflow
}

fn create_chain_workflow() -> Workflow {
    let mut workflow = Workflow::new("LLM Chain Workflow".to_string());
    workflow.metadata.description = Some("Multi-step LLM processing chain".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 200.0);

    let llm1 = Node::new(
        "Analyze".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some("You are an analytical assistant.".to_string()),
            prompt_template: "Analyze the following text and extract key points: {{text}}"
                .to_string(),
            temperature: Some(0.5),
            max_tokens: Some(300),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(300.0, 200.0);

    let llm2 = Node::new(
        "Summarize".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some("You are a summarization expert.".to_string()),
            prompt_template: "Summarize these key points in 2-3 sentences: {{llm1_result}}"
                .to_string(),
            temperature: Some(0.3),
            max_tokens: Some(150),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(500.0, 200.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(700.0, 200.0);

    let start_id = start.id;
    let llm1_id = llm1.id;
    let llm2_id = llm2.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(llm1);
    workflow.add_node(llm2);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, llm1_id));
    workflow.add_edge(Edge::new(llm1_id, llm2_id));
    workflow.add_edge(Edge::new(llm2_id, end_id));

    workflow
}

fn create_parallel_workflow() -> Workflow {
    let mut workflow = Workflow::new("Parallel Execution Workflow".to_string());
    workflow.metadata.description = Some("Execute multiple LLM calls in parallel".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 300.0);

    let llm1 = Node::new(
        "Translate to Spanish".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "Translate to Spanish: {{text}}".to_string(),
            temperature: Some(0.3),
            max_tokens: Some(200),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(300.0, 200.0);

    let llm2 = Node::new(
        "Translate to French".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "Translate to French: {{text}}".to_string(),
            temperature: Some(0.3),
            max_tokens: Some(200),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(300.0, 300.0);

    let llm3 = Node::new(
        "Translate to German".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: None,
            prompt_template: "Translate to German: {{text}}".to_string(),
            temperature: Some(0.3),
            max_tokens: Some(200),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(300.0, 400.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(500.0, 300.0);

    let start_id = start.id;
    let llm1_id = llm1.id;
    let llm2_id = llm2.id;
    let llm3_id = llm3.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(llm1);
    workflow.add_node(llm2);
    workflow.add_node(llm3);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, llm1_id));
    workflow.add_edge(Edge::new(start_id, llm2_id));
    workflow.add_edge(Edge::new(start_id, llm3_id));
    workflow.add_edge(Edge::new(llm1_id, end_id));
    workflow.add_edge(Edge::new(llm2_id, end_id));
    workflow.add_edge(Edge::new(llm3_id, end_id));

    workflow
}

fn create_conditional_workflow() -> Workflow {
    use oxify_model::{SwitchCase, SwitchConfig};

    let mut workflow = Workflow::new("Conditional Routing Workflow".to_string());
    workflow.metadata.description =
        Some("Route execution based on conditions using Switch node".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 300.0);

    // Switch node that routes based on status value
    let switch = Node::new(
        "Route by Status".to_string(),
        NodeKind::Switch(SwitchConfig {
            switch_on: "{{status}}".to_string(),
            cases: vec![
                SwitchCase {
                    match_value: "success".to_string(),
                    action: "Processing successful response".to_string(),
                },
                SwitchCase {
                    match_value: "error".to_string(),
                    action: "Handling error response".to_string(),
                },
                SwitchCase {
                    match_value: "pending".to_string(),
                    action: "Waiting for completion".to_string(),
                },
            ],
            default_case: Some("Unknown status - using default handler".to_string()),
        }),
    )
    .with_position(300.0, 300.0);

    let success_llm = Node::new(
        "Process Success".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some("You process successful responses.".to_string()),
            prompt_template: "Process this successful result: {{result}}".to_string(),
            temperature: Some(0.5),
            max_tokens: Some(300),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(500.0, 200.0);

    let error_llm = Node::new(
        "Handle Error".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some("You handle error responses.".to_string()),
            prompt_template: "Handle this error: {{error}}".to_string(),
            temperature: Some(0.3),
            max_tokens: Some(200),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(500.0, 400.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(700.0, 300.0);

    let start_id = start.id;
    let switch_id = switch.id;
    let success_id = success_llm.id;
    let error_id = error_llm.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(switch);
    workflow.add_node(success_llm);
    workflow.add_node(error_llm);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, switch_id));
    workflow.add_edge(Edge::new(switch_id, success_id));
    workflow.add_edge(Edge::new(switch_id, error_id));
    workflow.add_edge(Edge::new(success_id, end_id));
    workflow.add_edge(Edge::new(error_id, end_id));

    workflow
}

fn create_loop_workflow() -> Workflow {
    use oxify_model::{LoopConfig, LoopType};

    let mut workflow = Workflow::new("Loop Iteration Workflow".to_string());
    workflow.metadata.description = Some("Iterate over a collection with ForEach".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 200.0);

    // ForEach loop that processes each item in a collection
    let foreach = Node::new(
        "Process Items".to_string(),
        NodeKind::Loop(LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: "items".to_string(),
                item_variable: "item".to_string(),
                index_variable: Some("index".to_string()),
                body_expression: "Processing item: {{item}} at index {{index}}".to_string(),
                parallel: false,
                max_concurrency: None,
            },
            max_iterations: 100,
        }),
    )
    .with_position(300.0, 200.0);

    let llm = Node::new(
        "Process Each Item".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some("You process individual items from a collection.".to_string()),
            prompt_template: "Analyze this item: {{item}}".to_string(),
            temperature: Some(0.5),
            max_tokens: Some(200),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(500.0, 200.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(700.0, 200.0);

    let start_id = start.id;
    let foreach_id = foreach.id;
    let llm_id = llm.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(foreach);
    workflow.add_node(llm);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, foreach_id));
    workflow.add_edge(Edge::new(foreach_id, llm_id));
    workflow.add_edge(Edge::new(llm_id, end_id));

    workflow
}

fn create_error_handling_workflow() -> Workflow {
    use oxify_model::TryCatchConfig;

    let mut workflow = Workflow::new("Error Handling Workflow".to_string());
    workflow.metadata.description = Some("Try-Catch-Finally error handling pattern".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 200.0);

    // Try-Catch-Finally node for robust error handling
    let try_catch = Node::new(
        "Handle Errors".to_string(),
        NodeKind::TryCatch(TryCatchConfig {
            try_expression: "{{risky_operation}}".to_string(),
            catch_expression: Some("Error occurred: {{error}}".to_string()),
            finally_expression: Some("Cleanup completed".to_string()),
            rethrow: false,
            error_variable: "error".to_string(),
        }),
    )
    .with_position(300.0, 200.0);

    let llm = Node::new(
        "Process Result".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some("You process results or handle errors gracefully.".to_string()),
            prompt_template: "The operation completed. Result or error: {{result}}".to_string(),
            temperature: Some(0.5),
            max_tokens: Some(300),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(500.0, 200.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(700.0, 200.0);

    let start_id = start.id;
    let try_catch_id = try_catch.id;
    let llm_id = llm.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(try_catch);
    workflow.add_node(llm);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, try_catch_id));
    workflow.add_edge(Edge::new(try_catch_id, llm_id));
    workflow.add_edge(Edge::new(llm_id, end_id));

    workflow
}

fn create_agent_workflow() -> Workflow {
    let mut workflow = Workflow::new("Agent Workflow".to_string());
    workflow.metadata.description = Some("AI agent with reasoning and tool use".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 200.0);

    let reasoning = Node::new("Reasoning".to_string(), NodeKind::LLM(LlmConfig {
        provider: "openai".to_string(),
        model: "gpt-4".to_string(),
        system_prompt: Some("You are a reasoning AI agent. Break down complex tasks into steps.".to_string()),
        prompt_template: "Task: {{task}}\n\nThink step-by-step about how to solve this task. What information do you need? What tools should you use?".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(500),
        tools: vec![],
        images: vec![],
        extra_params: serde_json::Value::Null,
    })).with_position(300.0, 200.0);

    let action = Node::new(
        "Action".to_string(),
        NodeKind::LLM(LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            system_prompt: Some("Execute actions based on your reasoning.".to_string()),
            prompt_template:
                "Based on your reasoning: {{reasoning_result}}\n\nNow execute the first step."
                    .to_string(),
            temperature: Some(0.5),
            max_tokens: Some(300),
            tools: vec![],
            images: vec![],
            extra_params: serde_json::Value::Null,
        }),
    )
    .with_position(500.0, 200.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(700.0, 200.0);

    let start_id = start.id;
    let reasoning_id = reasoning.id;
    let action_id = action.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(reasoning);
    workflow.add_node(action);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, reasoning_id));
    workflow.add_edge(Edge::new(reasoning_id, action_id));
    workflow.add_edge(Edge::new(action_id, end_id));

    workflow
}

fn create_subworkflow_workflow() -> Workflow {
    use oxify_model::SubWorkflowConfig;
    use std::collections::HashMap;

    let mut workflow = Workflow::new("SubWorkflow Orchestration".to_string());
    workflow.metadata.description =
        Some("Orchestrate multiple sub-workflows with data passing".to_string());

    let start = Node::new("Start".to_string(), NodeKind::Start).with_position(100.0, 300.0);

    // First sub-workflow: Data preparation
    let mut prep_mappings = HashMap::new();
    prep_mappings.insert("input_data".to_string(), "{{raw_data}}".to_string());

    let prep_subworkflow = Node::new(
        "Prepare Data".to_string(),
        NodeKind::SubWorkflow(SubWorkflowConfig {
            workflow_path: "./workflows/data_prep.json".to_string(),
            input_mappings: prep_mappings,
            output_variable: Some("prepared_data".to_string()),
            inherit_context: false,
        }),
    )
    .with_position(300.0, 300.0);

    // Second sub-workflow: Data processing
    let mut process_mappings = HashMap::new();
    process_mappings.insert("data".to_string(), "{{prepared_data}}".to_string());

    let process_subworkflow = Node::new(
        "Process Data".to_string(),
        NodeKind::SubWorkflow(SubWorkflowConfig {
            workflow_path: "./workflows/data_process.json".to_string(),
            input_mappings: process_mappings,
            output_variable: Some("processed_data".to_string()),
            inherit_context: false,
        }),
    )
    .with_position(500.0, 300.0);

    // Third sub-workflow: Generate report
    let mut report_mappings = HashMap::new();
    report_mappings.insert("data".to_string(), "{{processed_data}}".to_string());

    let report_subworkflow = Node::new(
        "Generate Report".to_string(),
        NodeKind::SubWorkflow(SubWorkflowConfig {
            workflow_path: "./workflows/report_gen.json".to_string(),
            input_mappings: report_mappings,
            output_variable: Some("report".to_string()),
            inherit_context: false,
        }),
    )
    .with_position(700.0, 300.0);

    let end = Node::new("End".to_string(), NodeKind::End).with_position(900.0, 300.0);

    let start_id = start.id;
    let prep_id = prep_subworkflow.id;
    let process_id = process_subworkflow.id;
    let report_id = report_subworkflow.id;
    let end_id = end.id;

    workflow.add_node(start);
    workflow.add_node(prep_subworkflow);
    workflow.add_node(process_subworkflow);
    workflow.add_node(report_subworkflow);
    workflow.add_node(end);

    workflow.add_edge(Edge::new(start_id, prep_id));
    workflow.add_edge(Edge::new(prep_id, process_id));
    workflow.add_edge(Edge::new(process_id, report_id));
    workflow.add_edge(Edge::new(report_id, end_id));

    workflow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_simple_workflow() {
        let workflow = create_simple_workflow();
        assert_eq!(workflow.metadata.name, "Simple LLM Workflow");
        assert_eq!(workflow.nodes.len(), 3); // start, llm, end
        assert_eq!(workflow.edges.len(), 2);
    }

    #[test]
    fn test_create_rag_workflow() {
        let workflow = create_rag_workflow();
        assert_eq!(workflow.metadata.name, "RAG Workflow");
        assert_eq!(workflow.nodes.len(), 4); // start, retriever, llm, end
        assert_eq!(workflow.edges.len(), 3);
    }

    #[test]
    fn test_create_chain_workflow() {
        let workflow = create_chain_workflow();
        assert_eq!(workflow.metadata.name, "LLM Chain Workflow");
        assert!(workflow.nodes.len() >= 4); // start, multiple llms, end
    }

    #[test]
    fn test_create_parallel_workflow() {
        let workflow = create_parallel_workflow();
        assert_eq!(workflow.metadata.name, "Parallel Execution Workflow");
        assert!(workflow.nodes.len() >= 5); // start, multiple parallel nodes, end
    }

    #[test]
    fn test_create_conditional_workflow() {
        let workflow = create_conditional_workflow();
        assert_eq!(workflow.metadata.name, "Conditional Routing Workflow");
        assert!(workflow.nodes.len() >= 3); // must have switch node
    }

    #[test]
    fn test_create_loop_workflow() {
        let workflow = create_loop_workflow();
        assert_eq!(workflow.metadata.name, "Loop Iteration Workflow");
        assert!(workflow.nodes.len() >= 3); // must have loop node
    }

    #[test]
    fn test_create_error_handling_workflow() {
        let workflow = create_error_handling_workflow();
        assert_eq!(workflow.metadata.name, "Error Handling Workflow");
        assert!(workflow.nodes.len() >= 3);
    }

    #[test]
    fn test_create_agent_workflow() {
        let workflow = create_agent_workflow();
        assert_eq!(workflow.metadata.name, "Agent Workflow");
        assert!(workflow.nodes.len() >= 4);
    }

    #[test]
    fn test_create_subworkflow_workflow() {
        let workflow = create_subworkflow_workflow();
        assert_eq!(workflow.metadata.name, "SubWorkflow Orchestration");
        assert!(workflow.nodes.len() >= 5);
    }

    #[test]
    fn test_all_workflows_validate() {
        let workflows = vec![
            create_simple_workflow(),
            create_rag_workflow(),
            create_chain_workflow(),
            create_parallel_workflow(),
            create_conditional_workflow(),
            create_loop_workflow(),
            create_error_handling_workflow(),
            create_agent_workflow(),
            create_subworkflow_workflow(),
        ];

        for workflow in workflows {
            assert!(
                workflow.validate().is_ok(),
                "Workflow {} should validate successfully",
                workflow.metadata.name
            );
        }
    }
}
