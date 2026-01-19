//! API Performance Benchmarks
//!
//! Benchmarks for measuring API latency and throughput.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_model::{Workflow, WorkflowBuilder};
use serde_json::json;
use std::hint::black_box;
use uuid::Uuid;

/// Create a test workflow with N nodes
fn create_test_workflow(node_count: usize) -> Workflow {
    let mut builder = WorkflowBuilder::new(format!("Test Workflow {}", node_count))
        .description("Benchmark test workflow")
        .start("Start");

    for i in 0..node_count {
        builder = builder.llm(
            format!("Node_{}", i),
            oxify_model::LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-3.5-turbo".to_string(),
                temperature: Some(0.7),
                max_tokens: Some(100),
                system_prompt: Some("You are a helpful assistant".to_string()),
                prompt_template: "Process input".to_string(),
                extra_params: serde_json::Value::Null,
                images: Vec::new(),
                tools: Vec::new(),
            },
        );
    }

    builder.end("End").build()
}

/// Benchmark workflow serialization
fn bench_workflow_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_serialization");

    for node_count in [5, 10, 25, 50, 100].iter() {
        let workflow = create_test_workflow(*node_count);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("json_serialize", node_count),
            &workflow,
            |b, workflow| {
                b.iter(|| {
                    let json = serde_json::to_string(black_box(workflow)).unwrap();
                    black_box(json)
                })
            },
        );
    }

    group.finish();
}

/// Benchmark workflow deserialization
fn bench_workflow_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_deserialization");

    for node_count in [5, 10, 25, 50, 100].iter() {
        let workflow = create_test_workflow(*node_count);
        let json = serde_json::to_string(&workflow).unwrap();

        group.throughput(Throughput::Bytes(json.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("json_deserialize", node_count),
            &json,
            |b, json| {
                b.iter(|| {
                    let w: Workflow = serde_json::from_str(black_box(json)).unwrap();
                    black_box(w)
                })
            },
        );
    }

    group.finish();
}

/// Benchmark workflow validation
fn bench_workflow_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_validation");

    for node_count in [5, 10, 25, 50, 100].iter() {
        let workflow = create_test_workflow(*node_count);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("validate", node_count),
            &workflow,
            |b, workflow| {
                b.iter(|| {
                    let result = black_box(workflow).validate();
                    black_box(result)
                })
            },
        );
    }

    group.finish();
}

/// Benchmark UUID generation
fn bench_uuid_generation(c: &mut Criterion) {
    c.bench_function("uuid_v4_generation", |b| {
        b.iter(|| {
            let id = Uuid::new_v4();
            black_box(id)
        })
    });
}

/// Benchmark JSON request parsing
fn bench_json_request_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_request_parsing");

    // Small request
    let small_request = json!({
        "name": "Test Workflow",
        "description": "A simple test",
        "tags": ["test"]
    });

    // Medium request with variables
    let medium_request = json!({
        "workflow_id": "550e8400-e29b-41d4-a716-446655440000",
        "variables": {
            "input": "Hello world",
            "count": 10,
            "enabled": true,
            "config": {
                "temperature": 0.7,
                "max_tokens": 100
            }
        }
    });

    // Large request with nested data
    let large_items: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            json!({
                "id": Uuid::new_v4().to_string(),
                "name": format!("Item {}", i),
                "value": i * 10,
                "metadata": {
                    "created": "2026-01-01T00:00:00Z",
                    "tags": ["tag1", "tag2", "tag3"]
                }
            })
        })
        .collect();

    let large_request = json!({
        "batch_id": Uuid::new_v4().to_string(),
        "items": large_items,
        "options": {
            "parallel": true,
            "timeout_ms": 30000
        }
    });

    let small_str = serde_json::to_string(&small_request).unwrap();
    let medium_str = serde_json::to_string(&medium_request).unwrap();
    let large_str = serde_json::to_string(&large_request).unwrap();

    group.throughput(Throughput::Bytes(small_str.len() as u64));
    group.bench_with_input(BenchmarkId::new("parse", "small"), &small_str, |b, json| {
        b.iter(|| {
            let v: serde_json::Value = serde_json::from_str(black_box(json)).unwrap();
            black_box(v)
        })
    });

    group.throughput(Throughput::Bytes(medium_str.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("parse", "medium"),
        &medium_str,
        |b, json| {
            b.iter(|| {
                let v: serde_json::Value = serde_json::from_str(black_box(json)).unwrap();
                black_box(v)
            })
        },
    );

    group.throughput(Throughput::Bytes(large_str.len() as u64));
    group.bench_with_input(BenchmarkId::new("parse", "large"), &large_str, |b, json| {
        b.iter(|| {
            let v: serde_json::Value = serde_json::from_str(black_box(json)).unwrap();
            black_box(v)
        })
    });

    group.finish();
}

/// Benchmark response serialization
fn bench_response_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("response_serialization");

    // Simple response
    let simple_response = json!({
        "id": Uuid::new_v4().to_string(),
        "status": "success",
        "message": "Workflow created successfully"
    });

    // Workflow response
    let workflow = create_test_workflow(10);
    let workflow_response = json!({
        "workflow": workflow,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    });

    // Execution response with results
    let execution_response = json!({
        "id": Uuid::new_v4().to_string(),
        "workflow_id": Uuid::new_v4().to_string(),
        "state": "Completed",
        "started_at": "2026-01-01T00:00:00Z",
        "completed_at": "2026-01-01T00:01:00Z",
        "variables": {
            "input": "test",
            "output": "result"
        },
        "node_results": {
            "node_1": {"status": "success", "duration_ms": 100},
            "node_2": {"status": "success", "duration_ms": 150},
            "node_3": {"status": "success", "duration_ms": 200}
        }
    });

    group.bench_function("simple", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&simple_response)).unwrap();
            black_box(json)
        })
    });

    group.bench_function("workflow", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&workflow_response)).unwrap();
            black_box(json)
        })
    });

    group.bench_function("execution", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&execution_response)).unwrap();
            black_box(json)
        })
    });

    group.finish();
}

/// Benchmark workflow builder pattern
fn bench_workflow_builder(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_builder");

    for node_count in [5, 10, 25, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("build", node_count),
            node_count,
            |b, &count| {
                b.iter(|| {
                    let workflow = create_test_workflow(black_box(count));
                    black_box(workflow)
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_workflow_serialization,
    bench_workflow_deserialization,
    bench_workflow_validation,
    bench_uuid_generation,
    bench_json_request_parsing,
    bench_response_serialization,
    bench_workflow_builder,
);
criterion_main!(benches);
