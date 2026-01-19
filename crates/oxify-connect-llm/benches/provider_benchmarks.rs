//! Benchmark suite for LLM provider performance comparison

use criterion::{criterion_group, criterion_main, Criterion};
use oxify_connect_llm::{
    CachedProvider, LlmProvider, LlmRequest, MetricsProvider, ObservableProvider, OpenAIProvider,
    RetryProvider, TimeoutProvider, TrackedProvider,
};
use std::hint::black_box;
use std::time::Duration;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

/// Benchmark basic provider creation
fn bench_provider_creation(c: &mut Criterion) {
    c.bench_function("openai_provider_creation", |b| {
        b.iter(|| {
            black_box(OpenAIProvider::new(
                "test_key".to_string(),
                "gpt-3.5-turbo".to_string(),
            ))
        });
    });
}

/// Benchmark middleware wrapping
fn bench_middleware_wrapping(c: &mut Criterion) {
    let mut group = c.benchmark_group("middleware_wrapping");

    group.bench_function("retry_wrapper", |b| {
        b.iter(|| {
            let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string());
            black_box(RetryProvider::new(provider))
        });
    });

    group.bench_function("timeout_wrapper", |b| {
        b.iter(|| {
            let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string());
            black_box(TimeoutProvider::new(provider))
        });
    });

    group.bench_function("cache_wrapper", |b| {
        b.iter(|| {
            let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string());
            black_box(CachedProvider::new(
                Box::new(provider),
                "gpt-3.5-turbo".to_string(),
            ))
        });
    });

    group.bench_function("tracked_wrapper", |b| {
        b.iter(|| {
            let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string());
            black_box(TrackedProvider::new(provider))
        });
    });

    group.bench_function("observable_wrapper", |b| {
        b.iter(|| {
            let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string());
            black_box(ObservableProvider::new(
                Box::new(provider),
                "openai".to_string(),
            ))
        });
    });

    group.bench_function("metrics_wrapper", |b| {
        b.iter(|| {
            let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string());
            black_box(MetricsProvider::new(Box::new(provider)))
        });
    });

    group.finish();
}

/// Benchmark template rendering
fn bench_template_rendering(c: &mut Criterion) {
    use oxify_connect_llm::{PromptTemplate, TemplateLibrary};
    use std::collections::HashMap;

    let mut group = c.benchmark_group("template_rendering");

    // Simple template
    let simple_template = PromptTemplate::new("Hello {{name}}!".to_string());
    let mut simple_vars = HashMap::new();
    simple_vars.insert("name".to_string(), "World".to_string());

    group.bench_function("simple_template", |b| {
        b.iter(|| black_box(simple_template.render(&simple_vars).unwrap()));
    });

    // Complex template
    let library = TemplateLibrary::new();
    let code_review = library.get("code_review").unwrap();
    let mut code_vars = HashMap::new();
    code_vars.insert("language".to_string(), "Rust".to_string());
    code_vars.insert(
        "code".to_string(),
        "fn main() { println!(\"Hello\"); }".to_string(),
    );

    group.bench_function("code_review_template", |b| {
        b.iter(|| black_box(code_review.render(&code_vars).unwrap()));
    });

    // Variable extraction
    let extract_template =
        PromptTemplate::new("{{var1}} {{var2}} {{var3}} {{var4}} {{var5}}".to_string());

    group.bench_function("variable_extraction", |b| {
        b.iter(|| black_box(extract_template.extract_variables()));
    });

    group.finish();
}

/// Benchmark provider selection
fn bench_provider_selection(c: &mut Criterion) {
    use oxify_connect_llm::{ProviderMetadata, ProviderSelector, SelectionCriteria};
    use std::sync::Arc;

    let mut group = c.benchmark_group("provider_selection");

    let mut selector = ProviderSelector::new();
    let provider1 = Arc::new(OpenAIProvider::new("key1".to_string(), "gpt-4".to_string()));
    let provider2 = Arc::new(OpenAIProvider::new(
        "key2".to_string(),
        "gpt-3.5-turbo".to_string(),
    ));

    selector.register(ProviderMetadata::openai_gpt4(), provider1);
    selector.register(ProviderMetadata::openai_gpt35_turbo(), provider2);

    let cost_criteria = SelectionCriteria {
        optimize_cost: true,
        ..Default::default()
    };

    group.bench_function("select_by_cost", |b| {
        b.iter(|| black_box(selector.select(&cost_criteria)));
    });

    let speed_criteria = SelectionCriteria {
        optimize_speed: true,
        ..Default::default()
    };

    group.bench_function("select_by_speed", |b| {
        b.iter(|| black_box(selector.select(&speed_criteria)));
    });

    group.finish();
}

/// Benchmark cache operations
fn bench_cache_operations(c: &mut Criterion) {
    use oxify_connect_llm::LlmCache;

    let mut group = c.benchmark_group("cache_operations");

    let cache = LlmCache::new();

    group.bench_function("cache_stats", |b| {
        b.iter(|| black_box(cache.stats()));
    });

    group.bench_function("cache_creation", |b| {
        b.iter(|| black_box(LlmCache::new()));
    });

    group.finish();
}

/// Benchmark async operations with mock server
fn bench_async_operations(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("async_operations");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("mock_completion", |b| {
        b.to_async(&runtime).iter(|| async {
            let mock_server = MockServer::start().await;

            let response_body = serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Test response"
                    }
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 5,
                    "total_tokens": 15
                },
                "model": "gpt-3.5-turbo"
            });

            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
                .mount(&mock_server)
                .await;

            let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string())
                .with_base_url(mock_server.uri());

            let request = LlmRequest {
                prompt: "Hello".to_string(),
                system_prompt: None,
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
            };

            black_box(provider.complete(request).await.unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_provider_creation,
    bench_middleware_wrapping,
    bench_template_rendering,
    bench_provider_selection,
    bench_cache_operations,
    bench_async_operations,
);
criterion_main!(benches);
