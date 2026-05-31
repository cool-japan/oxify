//! AWS Signature Version 4 signing for oxify-connect-vision.
//!
//! This is an independent implementation of AWS SigV4 for the vision crate.
//! It does not depend on or share code with the llm crate's signing logic —
//! that separation is intentional so the vision crate has no cross-crate deps
//! on internal LLM modules.
//!
//! ## Algorithm summary
//!
//! 1. `content_sha256 = hex(sha256(body))`
//! 2. `canonical_request = METHOD\npath\ncanonical_query\ncanonical_headers\nsigned_headers\ncontent_sha256`
//! 3. `string_to_sign = "AWS4-HMAC-SHA256\n{datetime}\n{scope}\n{hex(sha256(canonical_request))}"`
//! 4. `signing_key = HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), service), "aws4_request")`
//! 5. `signature = hex(HMAC(signing_key, string_to_sign))`
//! 6. `Authorization = "AWS4-HMAC-SHA256 Credential={ak}/{scope}, SignedHeaders={headers}, Signature={signature}"`

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// AwsCredentials
// ---------------------------------------------------------------------------

/// AWS credentials for SigV4 request signing.
#[derive(Debug, Clone)]
pub struct AwsCredentials {
    /// AWS access key ID.
    pub access_key_id: String,
    /// AWS secret access key.
    pub secret_access_key: String,
    /// Optional session token (for temporary credentials / IAM roles).
    pub session_token: Option<String>,
}

impl AwsCredentials {
    /// Create credentials with a permanent access key and secret.
    ///
    /// # Arguments
    /// * `access_key_id`     — AWS access key ID (e.g. `"AKIAIOSFODNN7EXAMPLE"`)
    /// * `secret_access_key` — AWS secret access key
    pub fn new(access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
        }
    }

    /// Attach an optional session token for temporary credentials.
    pub fn with_session_token(mut self, token: impl Into<String>) -> Self {
        self.session_token = Some(token.into());
        self
    }

    /// Load credentials from environment variables.
    ///
    /// Reads:
    /// * `AWS_ACCESS_KEY_ID`     (required)
    /// * `AWS_SECRET_ACCESS_KEY` (required)
    /// * `AWS_SESSION_TOKEN`     (optional)
    ///
    /// Returns `Err(String)` describing which variable is missing.
    pub fn from_env() -> Result<Self, String> {
        let access_key_id = std::env::var("AWS_ACCESS_KEY_ID")
            .map_err(|_| "AWS_ACCESS_KEY_ID environment variable is not set".to_string())?;
        let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .map_err(|_| "AWS_SECRET_ACCESS_KEY environment variable is not set".to_string())?;
        let session_token = std::env::var("AWS_SESSION_TOKEN").ok();

        Ok(Self {
            access_key_id,
            secret_access_key,
            session_token,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Compute `hex(sha256(data))`.
fn hex_sha256(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    hex::encode(digest)
}

/// Compute `HMAC-SHA256(key_bytes, data)` and return the raw bytes.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Derive the SigV4 signing key from credentials and scope components.
///
/// `signing_key = HMAC(HMAC(HMAC(HMAC("AWS4" + secret, date), region), service), "aws4_request")`
pub fn derive_signing_key(
    secret_access_key: &str,
    date: &str, // "YYYYMMDD"
    region: &str,
    service: &str,
) -> Vec<u8> {
    let seed = format!("AWS4{}", secret_access_key);
    let k_date = hmac_sha256(seed.as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

/// Build the canonical request string.
///
/// The canonical form is:
/// ```text
/// METHOD\npath\ncanonical_query\ncanonical_headers\nsigned_headers\ncontent_sha256
/// ```
///
/// Headers in `extra_headers` must already be lower-cased; they will be sorted
/// alphabetically and the `host` header is always prepended.
fn build_canonical_request(
    method: &str,
    path: &str,
    host: &str,
    datetime: &str,
    body_sha256: &str,
    session_token: Option<&str>,
) -> (String, String) {
    // Assemble the headers we will sign.
    // Fixed set: host, x-amz-date, optionally x-amz-security-token.
    let mut headers: Vec<(String, String)> = vec![
        ("host".to_string(), host.to_string()),
        ("x-amz-date".to_string(), datetime.to_string()),
    ];
    if let Some(token) = session_token {
        headers.push(("x-amz-security-token".to_string(), token.to_string()));
    }
    // Sort alphabetically by header name for canonical form.
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = headers
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
        .collect();

    let signed_headers: String = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_request = format!(
        "{}\n{}\n\n{}{}\n{}",
        method, path, canonical_headers, signed_headers, body_sha256
    );

    (canonical_request, signed_headers)
}

// ---------------------------------------------------------------------------
// Public API: sign_request
// ---------------------------------------------------------------------------

/// Sign an HTTP request using AWS Signature Version 4.
///
/// Returns a map of headers to add to the request:
/// * `"authorization"` — the full `Authorization` header value
/// * `"x-amz-date"`    — the datetime used for signing (same as `datetime`)
/// * `"x-amz-security-token"` — only present when `credentials.session_token` is `Some`
///
/// # Arguments
/// * `method`      — HTTP verb in **upper case** (e.g. `"POST"`)
/// * `host`        — hostname without scheme (e.g. `"textract.us-east-1.amazonaws.com"`)
/// * `path`        — URL path component (e.g. `"/"`)
/// * `region`      — AWS region identifier (e.g. `"us-east-1"`)
/// * `service`     — AWS service identifier (e.g. `"textract"`)
/// * `body`        — raw request body bytes
/// * `credentials` — AWS credentials to sign with
/// * `datetime`    — ISO-8601 compact datetime `"YYYYMMDDTHHmmssZ"` (UTC)
#[allow(clippy::too_many_arguments)]
pub fn sign_request(
    method: &str,
    host: &str,
    path: &str,
    region: &str,
    service: &str,
    body: &[u8],
    credentials: &AwsCredentials,
    datetime: &str,
) -> HashMap<String, String> {
    // Extract the date portion: first 8 chars of the datetime string.
    let date = &datetime[..8];

    // Step 1: hash the body.
    let body_sha256 = hex_sha256(body);

    // Step 2: build the canonical request.
    let (canonical_request, signed_headers) = build_canonical_request(
        method,
        path,
        host,
        datetime,
        &body_sha256,
        credentials.session_token.as_deref(),
    );

    // Step 3: construct the string-to-sign.
    let credential_scope = format!("{}/{}/{}/aws4_request", date, region, service);
    let hashed_canonical = hex_sha256(canonical_request.as_bytes());
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        datetime, credential_scope, hashed_canonical
    );

    // Step 4: derive the signing key and compute the signature.
    let signing_key = derive_signing_key(&credentials.secret_access_key, date, region, service);
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    // Step 5: build the Authorization header.
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        credentials.access_key_id, credential_scope, signed_headers, signature
    );

    // Assemble the result map.
    let mut result = HashMap::new();
    result.insert("authorization".to_string(), authorization);
    result.insert("x-amz-date".to_string(), datetime.to_string());
    if let Some(token) = &credentials.session_token {
        result.insert("x-amz-security-token".to_string(), token.clone());
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // AwsCredentials
    // -----------------------------------------------------------------------

    #[test]
    fn test_credentials_new() {
        let creds = AwsCredentials::new("AKID", "SECRET");
        assert_eq!(creds.access_key_id, "AKID");
        assert_eq!(creds.secret_access_key, "SECRET");
        assert!(creds.session_token.is_none());
    }

    #[test]
    fn test_credentials_with_session_token() {
        let creds = AwsCredentials::new("AKID", "SECRET").with_session_token("TOKEN");
        assert_eq!(creds.session_token.as_deref(), Some("TOKEN"));
    }

    #[test]
    fn test_credentials_from_env_success() {
        // Temporarily set env vars for this test.
        std::env::set_var("AWS_ACCESS_KEY_ID", "TEST_AKID");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "TEST_SECRET");
        std::env::remove_var("AWS_SESSION_TOKEN");

        let result = AwsCredentials::from_env();

        // Clean up before asserting so we don't leave env vars behind.
        std::env::remove_var("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");

        let creds = result.expect("from_env should succeed when env vars are set");
        assert_eq!(creds.access_key_id, "TEST_AKID");
        assert_eq!(creds.secret_access_key, "TEST_SECRET");
        assert!(creds.session_token.is_none());
    }

    #[test]
    fn test_credentials_from_env_with_session_token() {
        std::env::set_var("AWS_ACCESS_KEY_ID", "TEST_AKID2");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "TEST_SECRET2");
        std::env::set_var("AWS_SESSION_TOKEN", "TEST_TOKEN2");

        let result = AwsCredentials::from_env();

        std::env::remove_var("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        std::env::remove_var("AWS_SESSION_TOKEN");

        let creds = result.expect("from_env with session token should succeed");
        assert_eq!(creds.session_token.as_deref(), Some("TEST_TOKEN2"));
    }

    #[test]
    fn test_credentials_from_env_missing_access_key() {
        std::env::remove_var("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");

        let result = AwsCredentials::from_env();
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("AWS_ACCESS_KEY_ID"), "got: {}", msg);
    }

    #[test]
    fn test_credentials_from_env_missing_secret_key() {
        std::env::set_var("AWS_ACCESS_KEY_ID", "SOME_AKID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");

        let result = AwsCredentials::from_env();

        std::env::remove_var("AWS_ACCESS_KEY_ID");

        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("AWS_SECRET_ACCESS_KEY"), "got: {}", msg);
    }

    // -----------------------------------------------------------------------
    // sign_request
    // -----------------------------------------------------------------------

    #[test]
    fn test_sign_request_produces_authorization_header() {
        let creds = AwsCredentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        let headers = sign_request(
            "POST",
            "textract.us-east-1.amazonaws.com",
            "/",
            "us-east-1",
            "textract",
            b"{}",
            &creds,
            "20240101T120000Z",
        );

        assert!(
            headers.contains_key("authorization"),
            "missing authorization header"
        );
        assert!(
            headers.contains_key("x-amz-date"),
            "missing x-amz-date header"
        );
        assert!(
            !headers.contains_key("x-amz-security-token"),
            "unexpected security-token"
        );

        let auth = &headers["authorization"];
        assert!(
            auth.starts_with("AWS4-HMAC-SHA256 "),
            "wrong prefix: {}",
            auth
        );
        assert!(
            auth.contains("Credential=AKIAIOSFODNN7EXAMPLE/"),
            "missing credential: {}",
            auth
        );
        assert!(
            auth.contains("SignedHeaders="),
            "missing signed-headers: {}",
            auth
        );
        assert!(auth.contains("Signature="), "missing signature: {}", auth);
    }

    #[test]
    fn test_sign_request_session_token_included() {
        let creds = AwsCredentials::new("AKID", "SECRET").with_session_token("MYTOKEN");
        let headers = sign_request(
            "POST",
            "textract.us-east-1.amazonaws.com",
            "/",
            "us-east-1",
            "textract",
            b"body",
            &creds,
            "20240101T120000Z",
        );

        assert!(
            headers.contains_key("x-amz-security-token"),
            "session token must appear"
        );
        assert_eq!(headers["x-amz-security-token"], "MYTOKEN");
    }

    #[test]
    fn test_canonical_request_signed_headers_sorted() {
        // When session_token is present the signed_headers should include it
        // and they must remain sorted alphabetically.
        let creds = AwsCredentials::new("AKID", "SECRET").with_session_token("TOKEN");
        let headers = sign_request(
            "POST",
            "textract.us-east-1.amazonaws.com",
            "/",
            "us-east-1",
            "textract",
            b"body",
            &creds,
            "20240115T080000Z",
        );

        let auth = &headers["authorization"];
        // Extract SignedHeaders value.
        let sh_start = auth.find("SignedHeaders=").unwrap() + "SignedHeaders=".len();
        let sh_end = auth[sh_start..].find(',').unwrap() + sh_start;
        let signed_headers = &auth[sh_start..sh_end];

        // Must be alphabetically sorted.
        let parts: Vec<&str> = signed_headers.split(';').collect();
        let mut sorted = parts.clone();
        sorted.sort();
        assert_eq!(
            parts, sorted,
            "signed headers not sorted: {}",
            signed_headers
        );
    }

    #[test]
    fn test_signing_key_deterministic() {
        // Same inputs must always produce the same key.
        let key1 = derive_signing_key("SECRET", "20240101", "us-east-1", "textract");
        let key2 = derive_signing_key("SECRET", "20240101", "us-east-1", "textract");
        assert_eq!(key1, key2, "signing key is not deterministic");
    }

    #[test]
    fn test_signing_key_differs_by_region() {
        let key_east = derive_signing_key("SECRET", "20240101", "us-east-1", "textract");
        let key_west = derive_signing_key("SECRET", "20240101", "us-west-2", "textract");
        assert_ne!(
            key_east, key_west,
            "different regions must yield different keys"
        );
    }

    #[test]
    fn test_credential_scope_in_authorization() {
        let creds = AwsCredentials::new("AKID", "SECRET");
        let headers = sign_request(
            "POST",
            "textract.us-east-1.amazonaws.com",
            "/",
            "us-east-1",
            "textract",
            b"",
            &creds,
            "20240601T000000Z",
        );
        let auth = &headers["authorization"];
        // Scope must be "YYYYMMDD/region/service/aws4_request".
        assert!(
            auth.contains("20240601/us-east-1/textract/aws4_request"),
            "wrong scope in: {}",
            auth
        );
    }
}
