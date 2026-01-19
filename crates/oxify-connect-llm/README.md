# oxify-connect-llm

**The Ecosystem** - LLM Provider Integrations for OxiFY

## Overview

`oxify-connect-llm` provides a unified, type-safe interface for multiple LLM providers. Part of OxiFY's **Connector Strategy** for mass-producing integrations via macros and trait abstractions.

**Status**: ✅ Phase 2 Complete - OpenAI & Anthropic Production Ready
**Roadmap**: AWS Bedrock, Google Gemini, Cohere, Mistral, Ollama (Phase 3)
**Part of**: OxiFY Enterprise Architecture (Codename: Absolute Zero)

### Supported Providers (Production)
- ✅ **OpenAI**: GPT-3.5, GPT-4, GPT-4-turbo
- ✅ **Anthropic**: Claude 3 (Opus, Sonnet, Haiku)

### Planned Providers (Phase 3)
- 🚧 **AWS Bedrock**: Claude, Llama, Titan, Mistral
- 🚧 **Google Gemini**: Gemini Pro, Gemini Ultra
- 🚧 **Cohere**: Command, Command-R
- 🚧 **Mistral**: Mistral Large, Mixtral
- 🚧 **Local Models**: Ollama, llama.cpp, vLLM

## Architecture

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse>;
}
```

## Supported Providers

### OpenAI

```rust
use oxify_connect_llm::{OpenAIProvider, LlmRequest};

let provider = OpenAIProvider::new(
    api_key.to_string(),
    "gpt-4".to_string()
);

let response = provider.complete(LlmRequest {
    prompt: "What is the capital of France?".to_string(),
    system_prompt: Some("You are a geography expert.".to_string()),
    temperature: Some(0.7),
    max_tokens: Some(100),
}).await?;

println!("Response: {}", response.content);
```

### Anthropic Claude

```rust
use oxify_connect_llm::{AnthropicProvider, LlmRequest};

let provider = AnthropicProvider::new(
    api_key.to_string(),
    "claude-3-opus-20240229".to_string()
);

let response = provider.complete(LlmRequest {
    prompt: "What is the capital of France?".to_string(),
    system_prompt: Some("You are a geography expert.".to_string()),
    temperature: Some(0.7),
    max_tokens: Some(100),
}).await?;

println!("Response: {}", response.content);
```

### Local Models (Planned)

```rust
let provider = OllamaProvider::new(
    "http://localhost:11434".to_string(),
    "llama2".to_string()
);
```

## Features

### Retry Logic

Automatic retry with exponential backoff for:
- Rate limiting (429)
- Temporary failures (500, 502, 503)
- Network errors

### Streaming Support (Planned)

```rust
let mut stream = provider.complete_stream(request).await?;

while let Some(chunk) = stream.next().await {
    print!("{}", chunk.content);
}
```

### Token Counting

```rust
let usage = response.usage.unwrap();
println!("Prompt tokens: {}", usage.prompt_tokens);
println!("Completion tokens: {}", usage.completion_tokens);
println!("Total tokens: {}", usage.total_tokens);
```

### Caching (Planned)

Cache responses to reduce costs:

```rust
let provider = OpenAIProvider::new(api_key, model)
    .with_cache(RedisCache::new(redis_url));
```

## Configuration

### OpenAI

```rust
pub struct OpenAIProvider {
    api_key: String,
    model: String,
    base_url: Option<String>,  // For Azure OpenAI
    organization: Option<String>,
}
```

### Request Options

```rust
pub struct LlmRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,      // 0.0 - 2.0
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub stop: Option<Vec<String>>,
}
```

## Error Handling

```rust
pub enum LlmError {
    ApiError(String),          // Provider API error
    ConfigError(String),       // Invalid configuration
    SerializationError(String),// JSON serialization error
    NetworkError(reqwest::Error), // Network error
    RateLimited,               // Rate limit exceeded
    InvalidRequest(String),    // Invalid request parameters
}
```

## Testing

Mock provider for testing:

```rust
use oxify_connect_llm::MockProvider;

let provider = MockProvider::new()
    .with_response("Paris is the capital of France.");

let response = provider.complete(request).await?;
assert_eq!(response.content, "Paris is the capital of France.");
```

## Future Enhancements

- [ ] Function calling support
- [ ] Vision model support (GPT-4V, Claude 3)
- [ ] Embedding generation
- [ ] Fine-tuned model support
- [ ] Cost tracking per request
- [ ] Prompt template library

## See Also

- `oxify-model`: LlmConfig definition
- `oxify-engine`: Workflow execution
