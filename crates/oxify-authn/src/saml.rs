//! SAML 2.0 Service Provider (SP) implementation.
//!
//! This module provides SAML 2.0 authentication support for enterprise Single Sign-On (SSO).
//! It implements the Service Provider role, allowing authentication against SAML Identity Providers.
//!
//! # Features
//! - `AuthnRequest` generation with HTTP-Redirect and HTTP-POST bindings
//! - SAML Response validation and assertion parsing
//! - XML signature validation (basic support)
//! - SP metadata generation
//! - `IdP` metadata parsing
//!
//! # Example
//! ```no_run
//! use oxify_authn::saml::{ServiceProvider, SpConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Configure Service Provider
//!     let config = SpConfig::builder()
//!         .entity_id("https://sp.example.com/metadata")
//!         .acs_url("https://sp.example.com/saml/acs")
//!         .idp_sso_url("https://idp.example.com/sso")
//!         .idp_entity_id("https://idp.example.com/metadata")
//!         .build()?;
//!
//!     let sp = ServiceProvider::new(config);
//!
//!     // Generate AuthnRequest
//!     let (authn_request, relay_state) = sp.create_authn_request()?;
//!     let redirect_url = sp.build_redirect_url(&authn_request, Some(&relay_state))?;
//!
//!     // Later, validate SAML Response
//!     let saml_response = "base64_encoded_saml_response";
//!     let assertion = sp.validate_response(saml_response).await?;
//!
//!     println!("Authenticated user: {}", assertion.subject);
//!     Ok(())
//! }
//! ```

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use flate2::{write::DeflateEncoder, Compression};
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

/// SAML-specific errors.
#[derive(Error, Debug)]
pub enum SamlError {
    #[error("Invalid SAML configuration: {0}")]
    InvalidConfig(String),

    #[error("Failed to generate AuthnRequest: {0}")]
    AuthnRequestError(String),

    #[error("Failed to parse SAML response: {0}")]
    ParseError(String),

    #[error("SAML response validation failed: {0}")]
    ValidationError(String),

    #[error("XML signature validation failed: {0}")]
    SignatureError(String),

    #[error("Assertion expired or not yet valid")]
    AssertionExpired,

    #[error("Invalid audience: expected {expected}, got {actual}")]
    InvalidAudience { expected: String, actual: String },

    #[error("Missing required attribute: {0}")]
    MissingAttribute(String),

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("URL encoding error: {0}")]
    UrlError(String),
}

/// Service Provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpConfig {
    /// SP entity ID (unique identifier)
    pub entity_id: String,

    /// Assertion Consumer Service URL (where `IdP` sends responses)
    pub acs_url: String,

    /// `IdP` SSO URL (where to send `AuthnRequests`)
    pub idp_sso_url: String,

    /// `IdP` entity ID
    pub idp_entity_id: String,

    /// `IdP` public certificate for signature validation (PEM format, optional)
    pub idp_certificate: Option<String>,

    /// SP private key for signing requests (PEM format, optional)
    pub sp_private_key: Option<String>,

    /// SP certificate for encryption (PEM format, optional)
    pub sp_certificate: Option<String>,

    /// Request signing enabled
    pub sign_requests: bool,

    /// Response signature validation required
    pub require_signed_response: bool,

    /// Assertion signature validation required
    pub require_signed_assertion: bool,

    /// Clock skew tolerance in seconds (for timestamp validation)
    pub clock_skew_seconds: i64,

    /// Attribute mapping (SAML attribute name -> local claim name)
    pub attribute_mapping: HashMap<String, String>,
}

impl SpConfig {
    /// Creates a new builder for `SpConfig`.
    #[must_use]
    pub fn builder() -> SpConfigBuilder {
        SpConfigBuilder::default()
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), SamlError> {
        if self.entity_id.is_empty() {
            return Err(SamlError::InvalidConfig("entity_id is required".into()));
        }
        if self.acs_url.is_empty() {
            return Err(SamlError::InvalidConfig("acs_url is required".into()));
        }
        if self.idp_sso_url.is_empty() {
            return Err(SamlError::InvalidConfig("idp_sso_url is required".into()));
        }
        if self.idp_entity_id.is_empty() {
            return Err(SamlError::InvalidConfig("idp_entity_id is required".into()));
        }

        // Validate URLs
        Url::parse(&self.acs_url)
            .map_err(|e| SamlError::InvalidConfig(format!("Invalid acs_url: {e}")))?;
        Url::parse(&self.idp_sso_url)
            .map_err(|e| SamlError::InvalidConfig(format!("Invalid idp_sso_url: {e}")))?;

        Ok(())
    }
}

/// Builder for `SpConfig`.
#[derive(Default)]
pub struct SpConfigBuilder {
    entity_id: Option<String>,
    acs_url: Option<String>,
    idp_sso_url: Option<String>,
    idp_entity_id: Option<String>,
    idp_certificate: Option<String>,
    sp_private_key: Option<String>,
    sp_certificate: Option<String>,
    sign_requests: bool,
    require_signed_response: bool,
    require_signed_assertion: bool,
    clock_skew_seconds: i64,
    attribute_mapping: HashMap<String, String>,
}

impl SpConfigBuilder {
    #[must_use]
    pub fn entity_id(mut self, entity_id: impl Into<String>) -> Self {
        self.entity_id = Some(entity_id.into());
        self
    }

    #[must_use]
    pub fn acs_url(mut self, acs_url: impl Into<String>) -> Self {
        self.acs_url = Some(acs_url.into());
        self
    }

    #[must_use]
    pub fn idp_sso_url(mut self, idp_sso_url: impl Into<String>) -> Self {
        self.idp_sso_url = Some(idp_sso_url.into());
        self
    }

    #[must_use]
    pub fn idp_entity_id(mut self, idp_entity_id: impl Into<String>) -> Self {
        self.idp_entity_id = Some(idp_entity_id.into());
        self
    }

    #[must_use]
    pub fn idp_certificate(mut self, cert: impl Into<String>) -> Self {
        self.idp_certificate = Some(cert.into());
        self
    }

    #[must_use]
    pub fn sp_private_key(mut self, key: impl Into<String>) -> Self {
        self.sp_private_key = Some(key.into());
        self
    }

    #[must_use]
    pub fn sp_certificate(mut self, cert: impl Into<String>) -> Self {
        self.sp_certificate = Some(cert.into());
        self
    }

    #[must_use]
    pub fn sign_requests(mut self, sign: bool) -> Self {
        self.sign_requests = sign;
        self
    }

    #[must_use]
    pub fn require_signed_response(mut self, require: bool) -> Self {
        self.require_signed_response = require;
        self
    }

    #[must_use]
    pub fn require_signed_assertion(mut self, require: bool) -> Self {
        self.require_signed_assertion = require;
        self
    }

    #[must_use]
    pub fn clock_skew_seconds(mut self, seconds: i64) -> Self {
        self.clock_skew_seconds = seconds;
        self
    }

    #[must_use]
    pub fn attribute_mapping(mut self, mapping: HashMap<String, String>) -> Self {
        self.attribute_mapping = mapping;
        self
    }

    #[must_use]
    pub fn add_attribute_mapping(
        mut self,
        saml_attr: impl Into<String>,
        local_claim: impl Into<String>,
    ) -> Self {
        self.attribute_mapping
            .insert(saml_attr.into(), local_claim.into());
        self
    }

    pub fn build(self) -> Result<SpConfig, SamlError> {
        let config = SpConfig {
            entity_id: self
                .entity_id
                .ok_or_else(|| SamlError::InvalidConfig("entity_id is required".into()))?,
            acs_url: self
                .acs_url
                .ok_or_else(|| SamlError::InvalidConfig("acs_url is required".into()))?,
            idp_sso_url: self
                .idp_sso_url
                .ok_or_else(|| SamlError::InvalidConfig("idp_sso_url is required".into()))?,
            idp_entity_id: self
                .idp_entity_id
                .ok_or_else(|| SamlError::InvalidConfig("idp_entity_id is required".into()))?,
            idp_certificate: self.idp_certificate,
            sp_private_key: self.sp_private_key,
            sp_certificate: self.sp_certificate,
            sign_requests: self.sign_requests,
            require_signed_response: self.require_signed_response,
            require_signed_assertion: self.require_signed_assertion,
            clock_skew_seconds: if self.clock_skew_seconds > 0 {
                self.clock_skew_seconds
            } else {
                300 // Default 5 minutes
            },
            attribute_mapping: self.attribute_mapping,
        };

        config.validate()?;
        Ok(config)
    }
}

/// SAML `AuthnRequest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthnRequest {
    pub id: String,
    pub issue_instant: DateTime<Utc>,
    pub destination: String,
    pub assertion_consumer_service_url: String,
    pub issuer: String,
    pub name_id_format: Option<String>,
}

impl AuthnRequest {
    /// Converts the `AuthnRequest` to XML format.
    pub fn to_xml(&self) -> Result<String, SamlError> {
        // Build AuthnRequest XML directly as string
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><samlp:AuthnRequest xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="{}" Version="2.0" IssueInstant="{}" Destination="{}" AssertionConsumerServiceURL="{}"><saml:Issuer>{}</saml:Issuer></samlp:AuthnRequest>"#,
            self.id,
            self.issue_instant.to_rfc3339(),
            self.destination,
            self.assertion_consumer_service_url,
            self.issuer
        );

        Ok(xml)
    }

    /// Encodes the `AuthnRequest` for HTTP-Redirect binding (deflate + base64).
    pub fn encode_for_redirect(&self) -> Result<String, SamlError> {
        let xml = self.to_xml()?;

        // Deflate compression
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(xml.as_bytes())
            .map_err(|e| SamlError::CompressionError(format!("Deflate failed: {e}")))?;
        let compressed = encoder
            .finish()
            .map_err(|e| SamlError::CompressionError(format!("Deflate finish failed: {e}")))?;

        // Base64 encode
        Ok(BASE64.encode(compressed))
    }

    /// Encodes the `AuthnRequest` for HTTP-POST binding (base64 only).
    pub fn encode_for_post(&self) -> Result<String, SamlError> {
        let xml = self.to_xml()?;
        Ok(BASE64.encode(xml.as_bytes()))
    }
}

/// SAML Assertion (parsed from SAML Response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assertion {
    pub id: String,
    pub issue_instant: DateTime<Utc>,
    pub issuer: String,
    pub subject: String,
    pub not_before: Option<DateTime<Utc>>,
    pub not_on_or_after: Option<DateTime<Utc>>,
    pub audience: Option<String>,
    pub attributes: HashMap<String, Vec<String>>,
    pub session_index: Option<String>,
}

impl Assertion {
    /// Validates the assertion's temporal constraints.
    pub fn validate_time(&self, clock_skew_seconds: i64) -> Result<(), SamlError> {
        let now = Utc::now();
        let clock_skew = chrono::Duration::seconds(clock_skew_seconds);

        if let Some(not_before) = self.not_before {
            if now < (not_before - clock_skew) {
                return Err(SamlError::AssertionExpired);
            }
        }

        if let Some(not_on_or_after) = self.not_on_or_after {
            if now >= (not_on_or_after + clock_skew) {
                return Err(SamlError::AssertionExpired);
            }
        }

        Ok(())
    }

    /// Validates the assertion's audience.
    pub fn validate_audience(&self, expected_audience: &str) -> Result<(), SamlError> {
        match &self.audience {
            Some(audience) if audience == expected_audience => Ok(()),
            Some(audience) => Err(SamlError::InvalidAudience {
                expected: expected_audience.to_string(),
                actual: audience.clone(),
            }),
            None => Err(SamlError::MissingAttribute("Audience".into())),
        }
    }

    /// Gets a single-valued attribute.
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .get(name)?
            .first()
            .map(std::string::String::as_str)
    }

    /// Gets a multi-valued attribute.
    pub fn get_attribute_values(&self, name: &str) -> Option<&[String]> {
        self.attributes.get(name).map(std::vec::Vec::as_slice)
    }
}

/// Service Provider for SAML 2.0 authentication.
pub struct ServiceProvider {
    config: SpConfig,
}

impl ServiceProvider {
    /// Creates a new Service Provider with the given configuration.
    #[must_use]
    pub fn new(config: SpConfig) -> Self {
        Self { config }
    }

    /// Creates a SAML `AuthnRequest`.
    pub fn create_authn_request(&self) -> Result<(AuthnRequest, String), SamlError> {
        let id = format!("_{}", Uuid::new_v4().simple());
        let relay_state = Uuid::new_v4().to_string();

        let authn_request = AuthnRequest {
            id,
            issue_instant: Utc::now(),
            destination: self.config.idp_sso_url.clone(),
            assertion_consumer_service_url: self.config.acs_url.clone(),
            issuer: self.config.entity_id.clone(),
            name_id_format: Some("urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress".into()),
        };

        Ok((authn_request, relay_state))
    }

    /// Builds a redirect URL for HTTP-Redirect binding.
    pub fn build_redirect_url(
        &self,
        authn_request: &AuthnRequest,
        relay_state: Option<&str>,
    ) -> Result<String, SamlError> {
        let encoded_request = authn_request.encode_for_redirect()?;

        let mut url = Url::parse(&self.config.idp_sso_url)
            .map_err(|e| SamlError::UrlError(format!("Invalid IdP SSO URL: {e}")))?;

        url.query_pairs_mut()
            .append_pair("SAMLRequest", &encoded_request);

        if let Some(state) = relay_state {
            url.query_pairs_mut().append_pair("RelayState", state);
        }

        Ok(url.to_string())
    }

    /// Validates a SAML Response and extracts the assertion.
    pub async fn validate_response(&self, saml_response: &str) -> Result<Assertion, SamlError> {
        // Decode base64
        let xml_bytes = BASE64
            .decode(saml_response)
            .map_err(|e| SamlError::ParseError(format!("Base64 decode failed: {e}")))?;

        let xml = String::from_utf8(xml_bytes)
            .map_err(|e| SamlError::ParseError(format!("Invalid UTF-8: {e}")))?;

        // Parse XML and extract assertion
        let assertion = Self::parse_saml_response(&xml)?;

        // Validate time constraints
        assertion.validate_time(self.config.clock_skew_seconds)?;

        // Validate audience
        assertion.validate_audience(&self.config.entity_id)?;

        // Validate XML signatures if required
        if self.config.require_signed_response || self.config.require_signed_assertion {
            self.validate_signatures(&xml)?;
        }

        Ok(assertion)
    }

    /// Validates XML signatures in the SAML response
    fn validate_signatures(&self, xml: &str) -> Result<(), SamlError> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        let mut found_signature = false;

        // Parse XML to find Signature elements
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e) | Event::Empty(e)) => match e.name().as_ref() {
                    b"ds:Signature" | b"Signature" => {
                        found_signature = true;
                        tracing::debug!("Found XML Signature element");
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(SamlError::ParseError(format!(
                        "Failed to parse XML for signature validation: {e}"
                    )))
                }
                _ => {}
            }
            buf.clear();
        }

        if !found_signature {
            if self.config.require_signed_response || self.config.require_signed_assertion {
                return Err(SamlError::SignatureError(
                    "Signature required but not found in SAML response".to_string(),
                ));
            }
            return Ok(());
        }

        // Basic signature presence validation
        // Note: Full cryptographic signature verification requires additional dependencies
        // and is left for future implementation or custom integration
        tracing::warn!(
            "XML Signature element found but cryptographic verification is not yet implemented. \
             For production use, integrate a proper XML signature verification library."
        );

        Ok(())
    }

    /// Parses a SAML Response XML and extracts the assertion.
    fn parse_saml_response(xml: &str) -> Result<Assertion, SamlError> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut assertion = Assertion {
            id: String::new(),
            issue_instant: Utc::now(),
            issuer: String::new(),
            subject: String::new(),
            not_before: None,
            not_on_or_after: None,
            audience: None,
            attributes: HashMap::new(),
            session_index: None,
        };

        let mut buf = Vec::new();
        let mut in_assertion = false;
        let mut in_subject = false;
        let mut in_conditions = false;
        let mut in_attribute = false;
        let mut current_attribute_name = String::new();
        let mut current_attribute_values = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => match e.name().as_ref() {
                    b"saml:Assertion" | b"Assertion" => {
                        in_assertion = true;
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"ID" => {
                                    assertion.id = String::from_utf8_lossy(&attr.value).to_string();
                                }
                                b"IssueInstant" => {
                                    let instant_str = String::from_utf8_lossy(&attr.value);
                                    assertion.issue_instant = DateTime::parse_from_rfc3339(
                                        &instant_str,
                                    )
                                    .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
                                }
                                _ => {}
                            }
                        }
                    }
                    b"saml:Subject" | b"Subject" => in_subject = true,
                    b"saml:NameID" | b"NameID" if in_subject => {
                        if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                            assertion.subject =
                                String::from_utf8_lossy(t.into_inner().as_ref()).to_string();
                        }
                    }
                    b"saml:Conditions" | b"Conditions" => {
                        in_conditions = true;
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"NotBefore" => {
                                    let instant_str = String::from_utf8_lossy(&attr.value);
                                    assertion.not_before =
                                        DateTime::parse_from_rfc3339(&instant_str)
                                            .map(|dt| dt.with_timezone(&Utc))
                                            .ok();
                                }
                                b"NotOnOrAfter" => {
                                    let instant_str = String::from_utf8_lossy(&attr.value);
                                    assertion.not_on_or_after =
                                        DateTime::parse_from_rfc3339(&instant_str)
                                            .map(|dt| dt.with_timezone(&Utc))
                                            .ok();
                                }
                                _ => {}
                            }
                        }
                    }
                    b"saml:Audience" | b"Audience" if in_conditions => {
                        if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                            assertion.audience =
                                Some(String::from_utf8_lossy(t.into_inner().as_ref()).to_string());
                        }
                    }
                    b"saml:Attribute" | b"Attribute" => {
                        in_attribute = true;
                        current_attribute_values.clear();
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"Name" {
                                current_attribute_name =
                                    String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    b"saml:AttributeValue" | b"AttributeValue" if in_attribute => {
                        if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                            current_attribute_values
                                .push(String::from_utf8_lossy(t.into_inner().as_ref()).to_string());
                        }
                    }
                    b"saml:Issuer" | b"Issuer" if in_assertion => {
                        if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                            assertion.issuer =
                                String::from_utf8_lossy(t.into_inner().as_ref()).to_string();
                        }
                    }
                    _ => {}
                },
                Ok(Event::End(e)) => match e.name().as_ref() {
                    b"saml:Assertion" | b"Assertion" => in_assertion = false,
                    b"saml:Subject" | b"Subject" => in_subject = false,
                    b"saml:Conditions" | b"Conditions" => in_conditions = false,
                    b"saml:Attribute" | b"Attribute" => {
                        if in_attribute && !current_attribute_name.is_empty() {
                            assertion.attributes.insert(
                                current_attribute_name.clone(),
                                current_attribute_values.clone(),
                            );
                        }
                        in_attribute = false;
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => return Err(SamlError::ParseError(format!("XML parse error: {e}"))),
                _ => {}
            }
            buf.clear();
        }

        if assertion.id.is_empty() {
            return Err(SamlError::ParseError("Missing assertion ID".into()));
        }
        if assertion.subject.is_empty() {
            return Err(SamlError::ParseError("Missing subject".into()));
        }

        Ok(assertion)
    }

    /// Generates SP metadata XML.
    pub fn generate_metadata(&self) -> Result<String, SamlError> {
        // Build metadata XML directly as string
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{}"><md:SPSSODescriptor AuthnRequestsSigned="{}" WantAssertionsSigned="{}" protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol"><md:AssertionConsumerService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST" Location="{}" index="0" isDefault="true"/></md:SPSSODescriptor></md:EntityDescriptor>"#,
            self.config.entity_id,
            self.config.sign_requests,
            self.config.require_signed_assertion,
            self.config.acs_url
        );

        Ok(xml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sp_config_builder() {
        let config = SpConfig::builder()
            .entity_id("https://sp.example.com/metadata")
            .acs_url("https://sp.example.com/saml/acs")
            .idp_sso_url("https://idp.example.com/sso")
            .idp_entity_id("https://idp.example.com/metadata")
            .clock_skew_seconds(300)
            .build()
            .unwrap();

        assert_eq!(config.entity_id, "https://sp.example.com/metadata");
        assert_eq!(config.clock_skew_seconds, 300);
    }

    #[test]
    fn test_authn_request_generation() {
        let config = SpConfig::builder()
            .entity_id("https://sp.example.com/metadata")
            .acs_url("https://sp.example.com/saml/acs")
            .idp_sso_url("https://idp.example.com/sso")
            .idp_entity_id("https://idp.example.com/metadata")
            .build()
            .unwrap();

        let sp = ServiceProvider::new(config);
        let (authn_request, relay_state) = sp.create_authn_request().unwrap();

        assert!(authn_request.id.starts_with('_'));
        assert!(!relay_state.is_empty());
        assert_eq!(authn_request.issuer, "https://sp.example.com/metadata");
    }

    #[test]
    fn test_authn_request_xml() {
        let authn_request = AuthnRequest {
            id: "_123456".to_string(),
            issue_instant: Utc::now(),
            destination: "https://idp.example.com/sso".to_string(),
            assertion_consumer_service_url: "https://sp.example.com/saml/acs".to_string(),
            issuer: "https://sp.example.com/metadata".to_string(),
            name_id_format: Some("urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress".into()),
        };

        let xml = authn_request.to_xml().unwrap();
        assert!(xml.contains("samlp:AuthnRequest"));
        assert!(xml.contains("_123456"));
        assert!(xml.contains("https://sp.example.com/metadata"));
    }

    #[test]
    fn test_redirect_url_building() {
        let config = SpConfig::builder()
            .entity_id("https://sp.example.com/metadata")
            .acs_url("https://sp.example.com/saml/acs")
            .idp_sso_url("https://idp.example.com/sso")
            .idp_entity_id("https://idp.example.com/metadata")
            .build()
            .unwrap();

        let sp = ServiceProvider::new(config);
        let (authn_request, relay_state) = sp.create_authn_request().unwrap();
        let redirect_url = sp
            .build_redirect_url(&authn_request, Some(&relay_state))
            .unwrap();

        assert!(redirect_url.starts_with("https://idp.example.com/sso?"));
        assert!(redirect_url.contains("SAMLRequest="));
        assert!(redirect_url.contains("RelayState="));
    }

    #[test]
    fn test_metadata_generation() {
        let config = SpConfig::builder()
            .entity_id("https://sp.example.com/metadata")
            .acs_url("https://sp.example.com/saml/acs")
            .idp_sso_url("https://idp.example.com/sso")
            .idp_entity_id("https://idp.example.com/metadata")
            .sign_requests(true)
            .require_signed_assertion(true)
            .build()
            .unwrap();

        let sp = ServiceProvider::new(config);
        let metadata = sp.generate_metadata().unwrap();

        assert!(metadata.contains("md:EntityDescriptor"));
        assert!(metadata.contains("https://sp.example.com/metadata"));
        assert!(metadata.contains("md:SPSSODescriptor"));
        assert!(metadata.contains("AuthnRequestsSigned=\"true\""));
    }

    #[test]
    fn test_assertion_time_validation() {
        let mut assertion = Assertion {
            id: "123".to_string(),
            issue_instant: Utc::now(),
            issuer: "https://idp.example.com".to_string(),
            subject: "user@example.com".to_string(),
            not_before: Some(Utc::now() - chrono::Duration::minutes(5)),
            not_on_or_after: Some(Utc::now() + chrono::Duration::minutes(5)),
            audience: Some("https://sp.example.com".to_string()),
            attributes: HashMap::new(),
            session_index: None,
        };

        // Should pass
        assert!(assertion.validate_time(300).is_ok());

        // Expire the assertion
        assertion.not_on_or_after = Some(Utc::now() - chrono::Duration::minutes(1));
        assert!(assertion.validate_time(0).is_err());
    }

    #[test]
    fn test_assertion_audience_validation() {
        let assertion = Assertion {
            id: "123".to_string(),
            issue_instant: Utc::now(),
            issuer: "https://idp.example.com".to_string(),
            subject: "user@example.com".to_string(),
            not_before: None,
            not_on_or_after: None,
            audience: Some("https://sp.example.com".to_string()),
            attributes: HashMap::new(),
            session_index: None,
        };

        assert!(assertion
            .validate_audience("https://sp.example.com")
            .is_ok());
        assert!(assertion
            .validate_audience("https://wrong.example.com")
            .is_err());
    }

    #[test]
    fn test_signature_validation_presence() {
        let config = SpConfig {
            entity_id: "https://sp.example.com".to_string(),
            acs_url: "https://sp.example.com/acs".to_string(),
            idp_sso_url: "https://idp.example.com/sso".to_string(),
            idp_entity_id: "https://idp.example.com".to_string(),
            idp_certificate: None,
            sp_private_key: None,
            sp_certificate: None,
            sign_requests: false,
            require_signed_response: true,
            require_signed_assertion: false,
            clock_skew_seconds: 300,
            attribute_mapping: HashMap::new(),
        };

        let sp = ServiceProvider::new(config);

        // Test XML with signature element
        let xml_with_sig = r#"
            <samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
                <ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
                    <ds:SignedInfo>
                        <ds:SignatureMethod Algorithm="http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"/>
                    </ds:SignedInfo>
                </ds:Signature>
            </samlp:Response>
        "#;

        // Should not error because signature is present
        let result = sp.validate_signatures(xml_with_sig);
        assert!(result.is_ok());

        // Test XML without signature element when required
        let xml_without_sig = r#"
            <samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
                <saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">
                </saml:Assertion>
            </samlp:Response>
        "#;

        // Should error because signature is required but not present
        let result = sp.validate_signatures(xml_without_sig);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SamlError::SignatureError(_)));
    }
}
