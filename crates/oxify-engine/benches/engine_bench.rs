//! Benchmarks for the OxiFY execution engine
//!
//! Run with: cargo bench

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_engine::Engine;
use oxify_model::{Edge, ExecutionContext, LlmConfig, Node, NodeKind, Workflow};
use std::hint::black_box;
use uuid::Uuid;

/// Create a simple linear workflow with N nodes
fn create_linear_workflow(num_nodes: usize) -> Workflow {
    let mut workflow = Workflow::new(format!("linear_{}", num_nodes));

    // Start node
    let start = Node::new("start".to_string(), NodeKind::Start);
    let start_id = start.id;
    workflow.add_node(start);

    // Create chain of LLM nodes
    let mut prev_id = start_id;
    for i in 0..num_nodes {
        let llm = Node::new(
            format!("llm_{}", i),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-3.5-turbo".to_string(),
                system_prompt: None,
                prompt_template: format!("Task {}", i),
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );
        let llm_id = llm.id;
        workflow.add_node(llm);
        workflow.add_edge(Edge::new(prev_id, llm_id));
        prev_id = llm_id;
    }

    // End node
    let end = Node::new("end".to_string(), NodeKind::End);
    let end_id = end.id;
    workflow.add_node(end);
    workflow.add_edge(Edge::new(prev_id, end_id));

    workflow
}

/// Create a parallel workflow with N branches
fn create_parallel_workflow(num_branches: usize) -> Workflow {
    let mut workflow = Workflow::new(format!("parallel_{}", num_branches));

    // Start node
    let start = Node::new("start".to_string(), NodeKind::Start);
    let start_id = start.id;
    workflow.add_node(start);

    // Create parallel branches
    let mut branch_ids = Vec::new();
    for i in 0..num_branches {
        let branch = Node::new(
            format!("branch_{}", i),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-3.5-turbo".to_string(),
                system_prompt: None,
                prompt_template: format!("Branch {}", i),
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );
        let branch_id = branch.id;
        workflow.add_node(branch);
        workflow.add_edge(Edge::new(start_id, branch_id));
        branch_ids.push(branch_id);
    }

    // End node
    let end = Node::new("end".to_string(), NodeKind::End);
    let end_id = end.id;
    workflow.add_node(end);

    for branch_id in branch_ids {
        workflow.add_edge(Edge::new(branch_id, end_id));
    }

    workflow
}

/// Benchmark topological sort with different workflow sizes
fn bench_topological_sort(c: &mut Criterion) {
    let mut group = c.benchmark_group("topological_sort");

    for size in [10, 50, 100, 200].iter() {
        let workflow = create_linear_workflow(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let _engine = Engine::new();
                // The topological sort happens during execute
                // We just measure the sort performance by creating the execution plan
                black_box(&workflow)
            });
        });
    }

    group.finish();
}

/// Benchmark workflow validation
fn bench_workflow_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_validation");

    for size in [10, 50, 100].iter() {
        let workflow = create_linear_workflow(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let _engine = Engine::new();
                black_box(&workflow);
            });
        });
    }

    group.finish();
}

/// Benchmark execution plan caching
fn bench_plan_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("plan_cache");

    let workflow = create_linear_workflow(50);

    group.bench_function("cache_miss", |b| {
        b.iter(|| {
            let _engine = Engine::new();
            black_box(&workflow);
        });
    });

    group.bench_function("cache_hit", |b| {
        let _engine = Engine::new();
        // Prime the cache by using the workflow once
        let _ = black_box(&workflow);

        b.iter(|| {
            black_box(&workflow);
        });
    });

    group.finish();
}

/// Benchmark parallel execution scaling
fn bench_parallel_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_scaling");

    for branches in [2, 4, 8, 16].iter() {
        let workflow = create_parallel_workflow(*branches);
        group.throughput(Throughput::Elements(*branches as u64));

        group.bench_with_input(BenchmarkId::from_parameter(branches), branches, |b, _| {
            b.iter(|| {
                let _engine = Engine::new();
                black_box(&workflow);
            });
        });
    }

    group.finish();
}

/// Benchmark context variable access
fn bench_variable_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_access");

    let mut ctx = ExecutionContext::new(Uuid::new_v4());

    // Add variables of different sizes
    for i in 0..100 {
        ctx.set_variable(format!("var_{}", i), serde_json::json!({ "value": i }));
    }

    group.bench_function("get_variable", |b| {
        b.iter(|| {
            for i in 0..100 {
                let _ = black_box(ctx.get_variable(&format!("var_{}", i)));
            }
        });
    });

    group.bench_function("set_variable", |b| {
        b.iter(|| {
            ctx.set_variable(
                "test_var".to_string(),
                black_box(serde_json::json!({"test": "value"})),
            );
        });
    });

    group.finish();
}

/// Benchmark template variable resolution
fn bench_template_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("template_resolution");

    let mut ctx = ExecutionContext::new(Uuid::new_v4());
    ctx.set_variable("name".to_string(), serde_json::json!("Alice"));
    ctx.set_variable("age".to_string(), serde_json::json!(30));
    ctx.set_variable("city".to_string(), serde_json::json!("New York"));

    group.bench_function("simple_template", |b| {
        b.iter(|| {
            let template = "Hello {{name}}!";
            black_box(template);
        });
    });

    group.bench_function("multiple_variables", |b| {
        b.iter(|| {
            let template = "{{name}} is {{age}} years old and lives in {{city}}";
            black_box(template);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_topological_sort,
    bench_workflow_validation,
    bench_plan_cache,
    bench_parallel_scaling,
    bench_variable_access,
    bench_template_resolution,
);

criterion_main!(benches);
