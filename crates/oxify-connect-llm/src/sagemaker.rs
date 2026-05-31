use crate::aws_sigv4::{sign_request, AwsCredentials};
use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct SageMakerProvider {
    region: String,
    endpoint_name: String,
    model_hint: String,
    client: reqwest::Client,
    credentials: Option<AwsCredentials>,
    base_url: String,
}

#[derive(Serialize)]
struct TgiParameters {
    max_new_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    do_sample: bool,
}

#[derive(Serialize)]
struct TgiRequest {
    inputs: String,
    parameters: TgiParameters,
}

#[derive(Deserialize)]
struct TgiResponseItem {
    generated_text: String,
}

impl SageMakerProvider {
    pub fn new(region: String, endpoint_name: String) -> Self {
        let base_url = format!("https://runtime.sagemaker.{}.amazonaws.com", region);
        Self {
            region,
            endpoint_name,
            model_hint: "tgi".to_string(),
            client: reqwest::Client::new(),
            credentials: None,
            base_url,
        }
    }

    pub fn with_credentials(mut self, credentials: AwsCredentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    pub fn with_model_hint(mut self, hint: &str) -> Self {
        self.model_hint = hint.to_string();
        self
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn from_env() -> Result<Self> {
        let region = std::env::var("AWS_SAGEMAKER_REGION")
            .map_err(|_| LlmError::ConfigError("AWS_SAGEMAKER_REGION not set".to_string()))?;
        let endpoint_name = std::env::var("AWS_SAGEMAKER_ENDPOINT")
            .map_err(|_| LlmError::ConfigError("AWS_SAGEMAKER_ENDPOINT not set".to_string()))?;
        let credentials = AwsCredentials::from_env()?;
        let base_url = format!("https://runtime.sagemaker.{}.amazonaws.com", region);
        Ok(Self {
            region,
            endpoint_name,
            model_hint: "tgi".to_string(),
            client: reqwest::Client::new(),
            credentials: Some(credentials),
            base_url,
        })
    }

    fn invocation_url(&self) -> String {
        format!(
            "{}/endpoints/{}/invocations",
            self.base_url, self.endpoint_name
        )
    }

    fn build_request_body(&self, request: &LlmRequest) -> Result<Vec<u8>> {
        let tgi_req = TgiRequest {
            inputs: request.prompt.clone(),
            parameters: TgiParameters {
                max_new_tokens: request.max_tokens.unwrap_or(512),
                temperature: request.temperature,
                do_sample: request.temperature.map(|t| t > 0.0).unwrap_or(true),
            },
        };
        serde_json::to_vec(&tgi_req).map_err(|e| LlmError::SerializationError(e.to_string()))
    }

    async fn invoke(&self, request: &LlmRequest) -> Result<LlmResponse> {
        let body = self.build_request_body(request)?;
        let url = self.invocation_url();
        let datetime = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();

        let creds = match &self.credentials {
            Some(c) => AwsCredentials::new(c.access_key_id.clone(), c.secret_access_key.clone())
                .with_session_token_opt(c.session_token.clone()),
            None => AwsCredentials::from_env()?,
        };

        let sig_headers = sign_request(
            "POST",
            &url,
            &self.region,
            "sagemaker",
            &body,
            &creds,
            &datetime,
        );

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body);

        for (k, v) in &sig_headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }

        let response = req_builder.send().await?;
        let status = response.status();

        if status.as_u16() == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(std::time::Duration::from_secs);
            return Err(LlmError::RateLimited(retry_after));
        }

        let body_text = response.text().await?;

        if !status.is_success() {
            return Err(LlmError::ApiError(format!(
                "HTTP {}: {}",
                status, body_text
            )));
        }

        let items: Vec<TgiResponseItem> = serde_json::from_str(&body_text)
            .map_err(|e| LlmError::SerializationError(e.to_string()))?;

        let content = items
            .into_iter()
            .next()
            .map(|item| item.generated_text)
            .unwrap_or_default();

        Ok(LlmResponse {
            content,
            model: format!("sagemaker/{}", self.endpoint_name),
            usage: None,
            tool_calls: Vec::new(),
        })
    }
}

#[async_trait]
impl LlmProvider for SageMakerProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        self.invoke(&request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_provider_with_server(server: &MockServer) -> SageMakerProvider {
        SageMakerProvider::new("us-east-1".to_string(), "my-llm-endpoint".to_string())
            .with_credentials(AwsCredentials::new(
                "AKID".to_string(),
                "SECRET".to_string(),
            ))
            .with_base_url(server.uri())
    }

    fn make_request() -> LlmRequest {
        LlmRequest {
            prompt: "Hello, world!".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(128),
            tools: vec![],
            images: vec![],
        }
    }

    #[tokio::test]
    async fn test_sagemaker_basic_completion() {
        let server = MockServer::start().await;
        let response_body = serde_json::json!([{"generated_text": "Hi there!"}]);

        Mock::given(method("POST"))
            .and(path("/endpoints/my-llm-endpoint/invocations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&server)
            .await;

        let provider = make_provider_with_server(&server);
        let resp = provider.complete(make_request()).await.unwrap();

        assert_eq!(resp.content, "Hi there!");
        assert!(resp.model.contains("my-llm-endpoint"));
    }

    #[tokio::test]
    async fn test_sagemaker_rate_limited() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/endpoints/my-llm-endpoint/invocations"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let provider = make_provider_with_server(&server);
        let result = provider.complete(make_request()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::RateLimited(_)));
    }

    #[tokio::test]
    async fn test_sagemaker_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/endpoints/my-llm-endpoint/invocations"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let provider = make_provider_with_server(&server);
        let result = provider.complete(make_request()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::ApiError(_)));
    }

    #[test]
    fn test_sagemaker_from_env_missing() {
        unsafe {
            std::env::remove_var("AWS_SAGEMAKER_REGION");
            std::env::remove_var("AWS_SAGEMAKER_ENDPOINT");
        }
        let result = SageMakerProvider::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::ConfigError(_)));
    }

    #[test]
    fn test_sagemaker_url_construction() {
        let provider = SageMakerProvider::new("eu-west-1".to_string(), "bert-endpoint".to_string());
        let url = provider.invocation_url();
        assert!(url.contains("runtime.sagemaker.eu-west-1.amazonaws.com"));
        assert!(url.contains("bert-endpoint"));
        assert!(url.ends_with("/invocations"));
    }

    #[test]
    fn test_sagemaker_request_body_format() {
        let provider = SageMakerProvider::new("us-east-1".to_string(), "test".to_string());
        let req = LlmRequest {
            prompt: "Test prompt".to_string(),
            system_prompt: None,
            temperature: Some(0.5),
            max_tokens: Some(256),
            tools: vec![],
            images: vec![],
        };
        let body = provider.build_request_body(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["inputs"], "Test prompt");
        assert_eq!(parsed["parameters"]["max_new_tokens"], 256);
        assert_eq!(parsed["parameters"]["temperature"], 0.5);
    }

    #[test]
    fn test_sagemaker_with_credentials() {
        let creds = AwsCredentials::new("MY_KEY_ID".to_string(), "MY_SECRET".to_string());
        let provider = SageMakerProvider::new("us-west-2".to_string(), "ep".to_string())
            .with_credentials(creds);
        assert!(provider.credentials.is_some());
        let c = provider.credentials.as_ref().unwrap();
        assert_eq!(c.access_key_id, "MY_KEY_ID");
    }

    #[test]
    fn test_sagemaker_provider_name() {
        let provider = SageMakerProvider::new("us-east-1".to_string(), "my-endpoint".to_string());
        let url = provider.invocation_url();
        assert!(url.contains("my-endpoint"));
    }
}
