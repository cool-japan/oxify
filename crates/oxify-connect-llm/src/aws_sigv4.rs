use crate::{LlmError, Result};
use oxicrypto_hash::Sha256;
use oxicrypto_mac::hmac_sha256_to_vec;
use std::collections::HashMap;

#[derive(Debug)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

impl AwsCredentials {
    pub fn new(access_key_id: String, secret_access_key: String) -> Self {
        Self {
            access_key_id,
            secret_access_key,
            session_token: None,
        }
    }

    pub fn with_session_token(mut self, token: String) -> Self {
        self.session_token = Some(token);
        self
    }

    pub fn from_env() -> Result<Self> {
        let access_key_id = std::env::var("AWS_ACCESS_KEY_ID")
            .map_err(|_| LlmError::ConfigError("AWS_ACCESS_KEY_ID not set".to_string()))?;
        let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .map_err(|_| LlmError::ConfigError("AWS_SECRET_ACCESS_KEY not set".to_string()))?;
        let session_token = std::env::var("AWS_SESSION_TOKEN").ok();
        Ok(Self {
            access_key_id,
            secret_access_key,
            session_token,
        })
    }

    pub(crate) fn with_session_token_opt(mut self, token: Option<String>) -> Self {
        self.session_token = token;
        self
    }
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256.hash_fixed(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    hmac_sha256_to_vec(key, data).expect("HMAC-SHA256 accepts any key length")
}

fn derive_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_secret = format!("AWS4{}", secret);
    let k_date = hmac_sha256(k_secret.as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn extract_path_and_query(url: &str) -> (&str, &str) {
    let without_scheme = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else {
        url
    };

    let path_start = without_scheme
        .find('/')
        .map(|i| i + url.len() - without_scheme.len());

    match path_start {
        None => ("/", ""),
        Some(idx) => {
            let path_and_query = &url[idx..];
            match path_and_query.find('?') {
                None => (path_and_query, ""),
                Some(q) => (&path_and_query[..q], &path_and_query[q + 1..]),
            }
        }
    }
}

fn extract_host(url: &str) -> &str {
    let without_scheme = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else {
        url
    };

    let end = without_scheme.find('/').unwrap_or(without_scheme.len());
    &without_scheme[..end]
}

pub fn sign_request(
    method: &str,
    url: &str,
    region: &str,
    service: &str,
    body: &[u8],
    credentials: &AwsCredentials,
    datetime: &str,
) -> HashMap<String, String> {
    let date = &datetime[..8];
    let host = extract_host(url);
    let (uri_path, canonical_query) = extract_path_and_query(url);

    let content_sha256 = sha256_hex(body);

    let mut signed_header_names: Vec<&str> = vec!["content-type", "host", "x-amz-date"];
    if credentials.session_token.is_some() {
        signed_header_names.push("x-amz-security-token");
    }
    signed_header_names.sort_unstable();

    let content_type = "application/json";
    let x_amz_date = datetime;

    let mut header_map: Vec<(&str, String)> = vec![
        ("content-type", content_type.to_string()),
        ("host", host.to_string()),
        ("x-amz-date", x_amz_date.to_string()),
    ];
    if let Some(token) = &credentials.session_token {
        header_map.push(("x-amz-security-token", token.clone()));
    }
    header_map.sort_unstable_by_key(|(k, _)| *k);

    let canonical_headers: String = header_map
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
        .collect();

    let signed_headers = signed_header_names.join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.to_uppercase(),
        uri_path,
        canonical_query,
        canonical_headers,
        signed_headers,
        content_sha256
    );

    let credential_scope = format!("{}/{}/{}/aws4_request", date, region, service);
    let canonical_request_hash = sha256_hex(canonical_request.as_bytes());

    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        datetime, credential_scope, canonical_request_hash
    );

    let signing_key = derive_signing_key(&credentials.secret_access_key, date, region, service);
    let signature_bytes = hmac_sha256(&signing_key, string_to_sign.as_bytes());
    let signature = hex::encode(&signature_bytes);

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        credentials.access_key_id, credential_scope, signed_headers, signature
    );

    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), authorization);
    headers.insert("x-amz-date".to_string(), datetime.to_string());
    if let Some(token) = &credentials.session_token {
        headers.insert("x-amz-security-token".to_string(), token.clone());
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_creds() -> AwsCredentials {
        AwsCredentials::new(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        )
    }

    #[test]
    fn test_sign_request_produces_authorization_header() {
        let creds = make_creds();
        let headers = sign_request(
            "POST",
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/test-model/invoke",
            "us-east-1",
            "bedrock",
            b"{\"test\":\"body\"}",
            &creds,
            "20240101T120000Z",
        );
        let auth = headers.get("Authorization").unwrap();
        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/"));
        assert!(auth.contains("SignedHeaders="));
        assert!(auth.contains("Signature="));
        assert!(auth.contains("20240101/us-east-1/bedrock/aws4_request"));
    }

    #[test]
    fn test_signing_key_derivation() {
        let key = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        );
        assert_eq!(key.len(), 32);
        let hex_key = hex::encode(&key);
        assert_eq!(
            hex_key,
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    #[test]
    fn test_canonical_request_format() {
        let creds = make_creds();
        let url = "https://example.amazonaws.com/test/path?foo=bar";
        let headers = sign_request(
            "GET",
            url,
            "us-east-1",
            "execute-api",
            b"",
            &creds,
            "20240115T093000Z",
        );
        let auth = headers.get("Authorization").unwrap();
        assert!(auth.contains("content-type;host;x-amz-date"));
    }

    #[test]
    fn test_credential_scope() {
        let creds = make_creds();
        let headers = sign_request(
            "POST",
            "https://sagemaker.us-west-2.amazonaws.com/endpoints/my-endpoint/invocations",
            "us-west-2",
            "sagemaker",
            b"{}",
            &creds,
            "20240301T080000Z",
        );
        let auth = headers.get("Authorization").unwrap();
        assert!(auth.contains("20240301/us-west-2/sagemaker/aws4_request"));
    }

    #[test]
    fn test_credentials_from_env() {
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "env_access_key");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "env_secret_key");
            std::env::remove_var("AWS_SESSION_TOKEN");
        }
        let creds = AwsCredentials::from_env().unwrap();
        assert_eq!(creds.access_key_id, "env_access_key");
        assert_eq!(creds.secret_access_key, "env_secret_key");
        assert!(creds.session_token.is_none());
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        }
    }

    #[test]
    fn test_credentials_from_env_missing_key() {
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        }
        let result = AwsCredentials::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::ConfigError(_)));
    }

    #[test]
    fn test_with_session_token() {
        let creds = AwsCredentials::new("key".to_string(), "secret".to_string())
            .with_session_token("my-session-token".to_string());
        assert_eq!(creds.session_token, Some("my-session-token".to_string()));
    }

    #[test]
    fn test_sign_request_includes_security_token_header() {
        let creds = AwsCredentials::new("AKID".to_string(), "SECRET".to_string())
            .with_session_token("SESSION123".to_string());
        let headers = sign_request(
            "POST",
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/m/invoke",
            "us-east-1",
            "bedrock",
            b"{}",
            &creds,
            "20240201T100000Z",
        );
        assert!(headers.contains_key("x-amz-security-token"));
        assert_eq!(headers["x-amz-security-token"], "SESSION123");
        let auth = headers.get("Authorization").unwrap();
        assert!(auth.contains("x-amz-security-token"));
    }
}
