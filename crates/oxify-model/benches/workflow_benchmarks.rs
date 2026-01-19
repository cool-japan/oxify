use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_model::*;
use serde_json::Value;
use std::hint::black_box;

fn create_linear_workflow(size: usize) -> Workflow {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let start = Node::new("Start".to_string(), NodeKind::Start);
    nodes.push(start.clone());

    let mut prev_id = start.id;

    for i in 0..size {
        let llm = Node::new(
            format!("LLM{}", i),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: format!("Process step {}", i),
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: vec![],
                images: vec![],
                extra_params: Value::Null,
            }),
        );
        edges.push(Edge::new(prev_id, llm.id));
        prev_id = llm.id;
        nodes.push(llm);
    }

    let end = Node::new("End".to_string(), NodeKind::End);
    edges.push(Edge::new(prev_id, end.id));
    nodes.push(end);

    Workflow {
        metadata: WorkflowMetadata::new(format!("benchmark-workflow-{}", size)),
        nodes,
        edges,
    }
}

fn create_branching_workflow(depth: usize, branching_factor: usize) -> Workflow {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let start = Node::new("Start".to_string(), NodeKind::Start);
    nodes.push(start.clone());

    let mut current_level = vec![start.id];
    let mut next_level = Vec::new();

    for level in 0..depth {
        for (i, &parent_id) in current_level.iter().enumerate() {
            for j in 0..branching_factor {
                let llm = Node::new(
                    format!("LLM_L{}_{}", level, i * branching_factor + j),
                    NodeKind::LLM(LlmConfig {
                        provider: "openai".to_string(),
                        model: "gpt-4".to_string(),
                        system_prompt: None,
                        prompt_template: format!("Level {} branch {}", level, j),
                        temperature: Some(0.7),
                        max_tokens: Some(100),
                        tools: vec![],
                        images: vec![],
                        extra_params: Value::Null,
                    }),
                );
                edges.push(Edge::new(parent_id, llm.id));
                next_level.push(llm.id);
                nodes.push(llm);
            }
        }
        current_level = next_level.clone();
        next_level.clear();
    }

    let end = Node::new("End".to_string(), NodeKind::End);
    for &node_id in &current_level {
        edges.push(Edge::new(node_id, end.id));
    }
    nodes.push(end);

    Workflow {
        metadata: WorkflowMetadata::new(format!(
            "branching-workflow-d{}-b{}",
            depth, branching_factor
        )),
        nodes,
        edges,
    }
}

fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    for size in [10, 50, 100, 200].iter() {
        let workflow = create_linear_workflow(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("json", size), size, |b, _| {
            b.iter(|| {
                let json = serde_json::to_string(black_box(&workflow)).unwrap();
                black_box(json);
            });
        });

        group.bench_with_input(BenchmarkId::new("yaml", size), size, |b, _| {
            b.iter(|| {
                let yaml = serde_yaml::to_string(black_box(&workflow)).unwrap();
                black_box(yaml);
            });
        });
    }

    group.finish();
}

fn bench_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialization");

    for size in [10, 50, 100, 200].iter() {
        let workflow = create_linear_workflow(*size);
        let json_str = serde_json::to_string(&workflow).unwrap();
        let yaml_str = serde_yaml::to_string(&workflow).unwrap();

        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("json", size), size, |b, _| {
            b.iter(|| {
                let wf: Workflow = serde_json::from_str(black_box(&json_str)).unwrap();
                black_box(wf);
            });
        });

        group.bench_with_input(BenchmarkId::new("yaml", size), size, |b, _| {
            b.iter(|| {
                let wf: Workflow = serde_yaml::from_str(black_box(&yaml_str)).unwrap();
                black_box(wf);
            });
        });
    }

    group.finish();
}

fn bench_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation");

    // Linear workflows
    for size in [10, 50, 100, 200].iter() {
        let workflow = create_linear_workflow(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("linear", size), size, |b, _| {
            b.iter(|| {
                let result = WorkflowValidator::validate(black_box(&workflow));
                let _ = black_box(result);
            });
        });
    }

    // Branching workflows
    for depth in [3, 4, 5].iter() {
        let workflow = create_branching_workflow(*depth, 2);
        let total_nodes = workflow.nodes.len();
        group.throughput(Throughput::Elements(total_nodes as u64));

        group.bench_with_input(
            BenchmarkId::new("branching", format!("d{}_b2", depth)),
            depth,
            |b, _| {
                b.iter(|| {
                    let result = WorkflowValidator::validate(black_box(&workflow));
                    let _ = black_box(result);
                });
            },
        );
    }

    group.finish();
}

fn bench_node_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("node_creation");

    group.bench_function("start_node", |b| {
        b.iter(|| {
            let node = Node::new(black_box("Start".to_string()), NodeKind::Start);
            black_box(node);
        });
    });

    group.bench_function("llm_node", |b| {
        b.iter(|| {
            let node = Node::new(
                black_box("LLM".to_string()),
                NodeKind::LLM(LlmConfig {
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    system_prompt: None,
                    prompt_template: "test".to_string(),
                    temperature: Some(0.7),
                    max_tokens: Some(100),
                    tools: vec![],
                    images: vec![],
                    extra_params: Value::Null,
                }),
            );
            black_box(node);
        });
    });

    group.bench_function("conditional_node", |b| {
        let true_branch = NodeId::new_v4();
        let false_branch = NodeId::new_v4();
        b.iter(|| {
            let node = Node::new(
                black_box("IfElse".to_string()),
                NodeKind::IfElse(Condition {
                    expression: "x > 0".to_string(),
                    true_branch,
                    false_branch,
                }),
            );
            black_box(node);
        });
    });

    group.finish();
}

fn bench_workflow_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_construction");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("linear", size), size, |b, &s| {
            b.iter(|| {
                let workflow = create_linear_workflow(s);
                black_box(workflow);
            });
        });
    }

    group.finish();
}

fn bench_template_instantiation(c: &mut Criterion) {
    use std::collections::HashMap;

    let mut group = c.benchmark_group("template_instantiation");

    // Create a simple workflow JSON template
    let workflow_json = r#"{
        "metadata": {
            "id": "00000000-0000-0000-0000-000000000000",
            "name": "{{workflow_name}}",
            "description": null,
            "version": "1.0.0",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "tags": [],
            "parent_id": null,
            "change_description": null,
            "schedule": null
        },
        "nodes": [
            {"id": "00000000-0000-0000-0000-000000000001", "name": "Start", "kind": "Start"},
            {"id": "00000000-0000-0000-0000-000000000002", "name": "End", "kind": "End"}
        ],
        "edges": [
            {"from": "00000000-0000-0000-0000-000000000001", "to": "00000000-0000-0000-0000-000000000002"}
        ]
    }"#;

    // Create a template with multiple parameters
    let template = WorkflowTemplate {
        id: TemplateId::new_v4(),
        name: "test-template".to_string(),
        description: Some("Test template".to_string()),
        category: Some("testing".to_string()),
        tags: vec!["benchmark".to_string()],
        version: "1.0.0".to_string(),
        parameters: vec![TemplateParameter {
            name: "workflow_name".to_string(),
            label: "Workflow Name".to_string(),
            description: Some("Workflow name".to_string()),
            param_type: ParameterType::String,
            default_value: None,
            required: true,
            validation: None,
            allowed_values: vec![],
            group: None,
            order: 0,
        }],
        workflow_json: workflow_json.to_string(),
        author: Some("benchmark".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        usage_count: 0,
        is_public: true,
        owner_id: None,
    };

    group.bench_function("instantiate_template", |b| {
        let mut params = HashMap::new();
        params.insert("workflow_name".to_string(), Value::from("instantiated"));

        b.iter(|| {
            let result = template.instantiate(black_box(&params));
            let _ = black_box(result);
        });
    });

    group.finish();
}

fn bench_event_timeline(c: &mut Criterion) {
    use std::collections::HashMap;

    let mut group = c.benchmark_group("event_timeline");

    let execution_id = ExecutionId::new_v4();
    let workflow_id = WorkflowId::new_v4();
    let node_id = NodeId::new_v4();

    group.bench_function("create_event", |b| {
        b.iter(|| {
            let event = ExecutionEvent::node_started(
                black_box(execution_id),
                black_box(workflow_id),
                black_box(node_id),
                NodeKind::Start,
                HashMap::new(),
            );
            black_box(event);
        });
    });

    // Create a timeline with many events
    let mut timeline = EventTimeline::new();
    for _ in 0..100 {
        timeline.push(ExecutionEvent::node_started(
            execution_id,
            workflow_id,
            NodeId::new_v4(),
            NodeKind::Start,
            HashMap::new(),
        ));
    }

    group.bench_function("filter_by_type", |b| {
        b.iter(|| {
            let events = timeline.filter_by_type(black_box(EventType::NodeStarted));
            black_box(events);
        });
    });

    group.bench_function("filter_by_node", |b| {
        b.iter(|| {
            let events = timeline.filter_by_node(black_box(node_id));
            black_box(events);
        });
    });

    group.finish();
}

fn bench_cost_estimation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cost_estimation");

    // Linear workflows
    for size in [10, 50, 100].iter() {
        let workflow = create_linear_workflow(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("estimate", size), size, |b, _| {
            b.iter(|| {
                let estimate = CostEstimator::estimate(black_box(&workflow));
                black_box(estimate);
            });
        });
    }

    group.finish();
}

fn bench_time_prediction(c: &mut Criterion) {
    let mut group = c.benchmark_group("time_prediction");

    // Linear workflows
    for size in [10, 50, 100].iter() {
        let workflow = create_linear_workflow(*size);
        let predictor = TimePredictor::new();
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("predict", size), size, |b, _| {
            b.iter(|| {
                let estimate = predictor.predict(black_box(&workflow));
                black_box(estimate);
            });
        });
    }

    group.finish();
}

fn bench_optimization_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimization_analysis");

    // Linear workflows
    for size in [10, 50, 100].iter() {
        let workflow = create_linear_workflow(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("analyze", size), size, |b, _| {
            b.iter(|| {
                let report = WorkflowOptimizer::analyze(black_box(&workflow));
                black_box(report);
            });
        });
    }

    group.finish();
}

fn bench_batch_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_analysis");

    // Linear workflows
    for size in [10, 50, 100].iter() {
        let workflow = create_linear_workflow(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("analyze", size), size, |b, _| {
            b.iter(|| {
                let plan = BatchAnalyzer::analyze(black_box(&workflow));
                black_box(plan);
            });
        });
    }

    group.finish();
}

fn bench_variable_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_optimization");

    // Linear workflows
    for size in [10, 50, 100].iter() {
        let workflow = create_linear_workflow(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("analyze", size), size, |b, _| {
            b.iter(|| {
                let analysis = VariableOptimizer::analyze(black_box(&workflow));
                black_box(analysis);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_serialization,
    bench_deserialization,
    bench_validation,
    bench_node_creation,
    bench_workflow_construction,
    bench_template_instantiation,
    bench_event_timeline,
    bench_cost_estimation,
    bench_time_prediction,
    bench_optimization_analysis,
    bench_batch_analysis,
    bench_variable_optimization
);
criterion_main!(benches);
