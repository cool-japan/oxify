//! Integration tests with mock HTTP servers

use oxify_connect_llm::{
    EmbeddingProvider, EmbeddingRequest, LlmProvider, LlmRequest, OpenAIProvider,
};
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn test_openai_complete_success() {
    // Start mock server
    let mock_server = MockServer::start().await;

    // Mock successful response
    let response_body = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "gpt-3.5-turbo",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you today?"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 9,
            "total_tokens": 19
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer test_key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(&mock_server)
        .await;

    // Create provider with mock server URL
    let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string())
        .with_base_url(mock_server.uri());

    // Test request
    let request = LlmRequest {
        prompt: "Hello!".to_string(),
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        tools: Vec::new(),
        images: Vec::new(),
    };

    let response = provider.complete(request).await.unwrap();

    assert_eq!(response.content, "Hello! How can I help you today?");
    assert_eq!(response.model, "gpt-3.5-turbo");
    assert!(response.usage.is_some());
    let usage = response.usage.unwrap();
    assert_eq!(usage.prompt_tokens, 10);
    assert_eq!(usage.completion_tokens, 9);
    assert_eq!(usage.total_tokens, 19);
}

#[tokio::test]
async fn test_openai_complete_rate_limit() {
    let mock_server = MockServer::start().await;

    // Mock rate limit response
    let response_body = serde_json::json!({
        "error": {
            "message": "Rate limit exceeded",
            "type": "rate_limit_error",
            "param": null,
            "code": "rate_limit_exceeded"
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_json(response_body))
        .mount(&mock_server)
        .await;

    let provider = OpenAIProvider::new("test_key".to_string(), "gpt-3.5-turbo".to_string())
        .with_base_url(mock_server.uri());

    let request = LlmRequest {
        prompt: "Hello!".to_string(),
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        tools: Vec::new(),
        images: Vec::new(),
    };

    let result = provider.complete(request).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        oxify_connect_llm::LlmError::RateLimited(_)
    ));
}

#[tokio::test]
async fn test_openai_complete_api_error() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "error": {
            "message": "Invalid API key",
            "type": "invalid_request_error",
            "param": null,
            "code": "invalid_api_key"
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(response_body))
        .mount(&mock_server)
        .await;

    let provider = OpenAIProvider::new("invalid_key".to_string(), "gpt-3.5-turbo".to_string())
        .with_base_url(mock_server.uri());

    let request = LlmRequest {
        prompt: "Hello!".to_string(),
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        tools: Vec::new(),
        images: Vec::new(),
    };

    let result = provider.complete(request).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        oxify_connect_llm::LlmError::ApiError(_)
    ));
}

#[tokio::test]
async fn test_openai_embeddings_success() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "object": "list",
        "data": [
            {
                "object": "embedding",
                "embedding": [0.1, 0.2, 0.3],
                "index": 0
            },
            {
                "object": "embedding",
                "embedding": [0.4, 0.5, 0.6],
                "index": 1
            }
        ],
        "model": "text-embedding-ada-002",
        "usage": {
            "prompt_tokens": 8,
            "total_tokens": 8
        }
    });

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .and(header("authorization", "Bearer test_key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(&mock_server)
        .await;

    let provider =
        OpenAIProvider::new("test_key".to_string(), "text-embedding-ada-002".to_string())
            .with_base_url(mock_server.uri());

    let request = EmbeddingRequest {
        texts: vec!["Hello".to_string(), "World".to_string()],
        model: None,
    };

    let response = provider.embed(request).await.unwrap();

    assert_eq!(response.embeddings.len(), 2);
    assert_eq!(response.embeddings[0], vec![0.1, 0.2, 0.3]);
    assert_eq!(response.embeddings[1], vec![0.4, 0.5, 0.6]);
    assert_eq!(response.model, "text-embedding-ada-002");
    assert!(response.usage.is_some());
}

#[tokio::test]
async fn test_openai_function_calling() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"location\": \"San Francisco\", \"unit\": \"celsius\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 50,
            "completion_tokens": 20,
            "total_tokens": 70
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(&mock_server)
        .await;

    let provider = OpenAIProvider::new("test_key".to_string(), "gpt-4".to_string())
        .with_base_url(mock_server.uri());

    let request = LlmRequest {
        prompt: "What's the weather in San Francisco?".to_string(),
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        tools: vec![oxify_connect_llm::Tool {
            name: "get_weather".to_string(),
            description: "Get the current weather".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"},
                    "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
                },
                "required": ["location"]
            }),
        }],
        images: Vec::new(),
    };

    let response = provider.complete(request).await.unwrap();

    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].name, "get_weather");
    assert_eq!(response.tool_calls[0].id, "call_123");
}

#[tokio::test]
async fn test_openai_vision_support() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "gpt-4-vision-preview",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "This is a beautiful sunset over the ocean."
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 10,
            "total_tokens": 110
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(&mock_server)
        .await;

    let provider = OpenAIProvider::new("test_key".to_string(), "gpt-4-vision-preview".to_string())
        .with_base_url(mock_server.uri());

    let request = LlmRequest {
        prompt: "What do you see in this image?".to_string(),
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        tools: Vec::new(),
        images: vec![oxify_connect_llm::ImageInput {
            data: "https://example.com/sunset.jpg".to_string(),
            source_type: oxify_connect_llm::ImageSourceType::Url,
            media_type: None,
        }],
    };

    let response = provider.complete(request).await.unwrap();

    assert_eq!(
        response.content,
        "This is a beautiful sunset over the ocean."
    );
    assert_eq!(response.model, "gpt-4-vision-preview");
}
